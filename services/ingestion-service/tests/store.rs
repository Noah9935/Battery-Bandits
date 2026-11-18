//! Integrationstests des Datenbank-Beobachters gegen eine echte TimescaleDB (DATABASE_URL).

use std::time::Duration;

use chrono::{DateTime, Utc};
use ingestion_service::classify::{InvalidMessage, ValidReading};
use ingestion_service::observer::ReadingObserver;
use ingestion_service::store::PostgresObserver;
use shared::db::MIGRATOR;
use shared::{Device, DomainEvent, TelemetryPayload};
use sqlx::PgPool;
use sqlx::postgres::PgListener;

fn reading(node: &str, voltage_mv: i32, current_ma: i32, position: Option<(f64, f64)>) -> ValidReading {
    ValidReading {
        node_id: node.to_owned(),
        payload: TelemetryPayload {
            current_ma,
            voltage_mv,
            lat: position.map(|p| p.0),
            lon: position.map(|p| p.1),
            altitude_m: position.map(|_| 400.0),
            satellites: position.map(|_| 8),
            time: None,
            temperature_c: Some(15.0),
        },
    }
}

/// Feste Basiszeit je Testlauf, damit Zeitspannen exakt sind.
fn hours_ago(h: i64) -> DateTime<Utc> {
    static BASE: std::sync::OnceLock<DateTime<Utc>> = std::sync::OnceLock::new();
    *BASE.get_or_init(Utc::now) - chrono::Duration::hours(h)
}

fn minutes_ago(m: i64) -> DateTime<Utc> {
    hours_ago(0) - chrono::Duration::minutes(m)
}

async fn device(pool: &PgPool, id: &str) -> Device {
    sqlx::query_as("select * from devices where id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("select count(*) from {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn first_reading_creates_device_with_voltage_based_soc(pool: PgPool) {
    let observer = PostgresObserver::new(pool.clone());

    observer
        .on_valid(&reading("TN0001", 11_600, -300, None), hours_ago(1))
        .await
        .unwrap();

    let d = device(&pool, "TN0001").await;
    // 11,6 V liegen auf der gemessenen Kurve bei gut 30 Prozent. Dazu gehört eine Unsicherheit, die mit
    // abgespeichert wird, einmal am Gerät und einmal an der Messung (für das Fehlerband im Verlauf).
    let soc = d.soc_percent.unwrap();
    assert!((28.0..38.0).contains(&soc), "{soc}");
    let sigma = d.soc_sigma_percent.unwrap();
    assert!((2.0..8.0).contains(&sigma), "{sigma}");
    let stored: Option<f32> = sqlx::query_scalar("select soc_sigma_percent from readings where device_id = 'TN0001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, Some(sigma));
    assert_eq!(
        d.status,
        shared::DeviceStatus::Normal,
        "den Status setzt der Überwachungs-Service"
    );
    assert_eq!(d.voltage_mv, Some(11_600));
    assert_eq!(d.discharged_ah, 0.0);
    assert!(d.last_seen_at.is_some());
    assert_eq!(count(&pool, "readings").await, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn later_readings_integrate_the_current(pool: PgPool) {
    let observer = PostgresObserver::new(pool.clone());
    // Start bei voller Batterie, danach 8 A über eine Stunde in Viertelstunden: 8 Ah von 80 Ah sind 10 Punkte.
    // Die Spannungen folgen der Kurve (100, 97,5, 95, 92,5, 90 Prozent) abzüglich des Spannungsabfalls bei 8 A.
    for (minutes, voltage) in [(60, 12_594), (45, 12_566), (30, 12_528), (15, 12_491), (0, 12_448)] {
        observer
            .on_valid(&reading("TN0001", voltage, -8_000, None), minutes_ago(minutes))
            .await
            .unwrap();
    }

    let d = device(&pool, "TN0001").await;
    assert!((d.discharged_ah - 8.0).abs() < 1e-6, "{}", d.discharged_ah);
    let soc = d.soc_percent.unwrap();
    assert!((86.0..94.0).contains(&soc), "{soc}");
    assert_eq!(count(&pool, "readings").await, 5);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_long_pause_invents_no_discharge_and_keeps_the_state_credible(pool: PgPool) {
    let observer = PostgresObserver::new(pool.clone());
    // Zwei Messungen mit drei Stunden Pause. Die frühere Rechnung hätte über die Pause 8 A · 3 h = 24 Ah entnommen
    // und das Gerät als fast leer gemeldet. Jetzt bleibt die Entnahme bei 0, die Spannung (12,4 V) hält den Wert.
    observer
        .on_valid(&reading("TN0001", 12_400, -8_000, None), minutes_ago(240))
        .await
        .unwrap();
    observer
        .on_valid(&reading("TN0001", 12_380, -8_000, None), minutes_ago(60))
        .await
        .unwrap();

    let d = device(&pool, "TN0001").await;
    assert_eq!(d.discharged_ah, 0.0);
    assert!(d.soc_percent.unwrap() > 70.0, "{:?}", d.soc_percent);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_filter_state_survives_in_the_database_and_a_broken_state_restarts(pool: PgPool) {
    let observer = PostgresObserver::new(pool.clone());
    observer
        .on_valid(&reading("TN0001", 12_000, -300, None), minutes_ago(30))
        .await
        .unwrap();
    let state: String = sqlx::query_scalar("select estimator_state::text from devices where id = 'TN0001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(state.contains("\"cov\""), "{state}");

    // Ein unlesbarer Zustand kostet keinen Messwert: Das Filter beginnt neu.
    sqlx::query("update devices set estimator_state = '{\"kaputt\": true}'::jsonb where id = 'TN0001'")
        .execute(&pool)
        .await
        .unwrap();
    observer
        .on_valid(&reading("TN0001", 12_000, -300, None), minutes_ago(15))
        .await
        .unwrap();
    assert_eq!(count(&pool, "readings").await, 2);
    let d = device(&pool, "TN0001").await;
    assert!(d.soc_sigma_percent.is_some());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn keeps_known_position_when_gps_fix_is_lost(pool: PgPool) {
    let observer = PostgresObserver::new(pool.clone());
    observer
        .on_valid(&reading("TN0001", 12_000, -300, Some((48.8, 9.0))), hours_ago(2))
        .await
        .unwrap();
    observer
        .on_valid(&reading("TN0001", 11_990, -300, None), hours_ago(1))
        .await
        .unwrap();

    let d = device(&pool, "TN0001").await;
    assert_eq!((d.lat, d.lon), (Some(48.8), Some(9.0)));
    assert_eq!(d.satellites, None, "aktuelle Satellitenzahl wird nicht festgehalten");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn late_reading_is_stored_but_does_not_change_the_device(pool: PgPool) {
    let observer = PostgresObserver::new(pool.clone());
    observer
        .on_valid(&reading("TN0001", 12_000, -300, None), hours_ago(1))
        .await
        .unwrap();
    let before = device(&pool, "TN0001").await;

    observer
        .on_valid(&reading("TN0001", 11_000, -300, None), hours_ago(3))
        .await
        .unwrap();

    let after = device(&pool, "TN0001").await;
    assert_eq!(before.last_seen_at, after.last_seen_at);
    assert_eq!(before.voltage_mv, after.voltage_mv);
    assert_eq!(count(&pool, "readings").await, 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn publishes_reading_saved_event(pool: PgPool) {
    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen(shared::events::CHANNEL).await.unwrap();
    let observer = PostgresObserver::new(pool.clone());

    observer
        .on_valid(&reading("TN0042", 12_000, -300, None), hours_ago(1))
        .await
        .unwrap();

    let notification = tokio::time::timeout(Duration::from_secs(5), listener.recv())
        .await
        .unwrap()
        .unwrap();
    let event = DomainEvent::from_json(notification.payload()).unwrap();
    assert!(
        matches!(&event, DomainEvent::ReadingSaved { device_id, .. } if device_id == "TN0042"),
        "{event:?}"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn invalid_messages_are_logged_and_create_no_device(pool: PgPool) {
    let observer = PostgresObserver::new(pool.clone());
    let message = InvalidMessage {
        topic: "traffic-nodes/TN0001/measurements".into(),
        payload: "{kaputt".into(),
        reason: "Kein gültiges Nachrichtenformat".into(),
    };

    observer.on_invalid(&message, Utc::now()).await.unwrap();

    let (topic, payload, reason): (String, String, String) =
        sqlx::query_as("select topic, payload, reason from invalid_messages_log")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (topic.as_str(), payload.as_str()),
        ("traffic-nodes/TN0001/measurements", "{kaputt")
    );
    assert!(reason.contains("Nachrichtenformat"));
    assert_eq!(count(&pool, "devices").await, 0);
    assert_eq!(count(&pool, "readings").await, 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn stores_messages_without_temperature(pool: PgPool) {
    let observer = PostgresObserver::new(pool.clone());
    let mut r = reading("101", 12_710, -290, None);
    r.payload.temperature_c = None;

    observer.on_valid(&r, hours_ago(1)).await.unwrap();

    let stored: Option<f32> = sqlx::query_scalar("select temperature_c from readings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, None);
    assert_eq!(device(&pool, "101").await.temperature_c, None);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn new_devices_carry_the_configured_source(pool: PgPool) {
    PostgresObserver::new(pool.clone())
        .on_valid(&reading("TN0001", 12_000, -300, None), hours_ago(1))
        .await
        .unwrap();
    PostgresObserver::new(pool.clone())
        .with_source(shared::DataSource::Simulation)
        .on_valid(&reading("SIM01", 12_000, -300, None), hours_ago(1))
        .await
        .unwrap();

    assert_eq!(device(&pool, "TN0001").await.data_source, shared::DataSource::Dozent);
    assert_eq!(device(&pool, "SIM01").await.data_source, shared::DataSource::Simulation);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_source_of_an_existing_device_is_not_overwritten(pool: PgPool) {
    PostgresObserver::new(pool.clone())
        .on_valid(&reading("TN0001", 12_000, -300, None), hours_ago(2))
        .await
        .unwrap();
    PostgresObserver::new(pool.clone())
        .with_source(shared::DataSource::Simulation)
        .on_valid(&reading("TN0001", 11_990, -300, None), hours_ago(1))
        .await
        .unwrap();

    assert_eq!(device(&pool, "TN0001").await.data_source, shared::DataSource::Dozent);
}

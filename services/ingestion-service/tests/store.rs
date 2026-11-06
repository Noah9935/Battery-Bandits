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
    assert!((d.soc_percent.unwrap() - 50.0).abs() < 0.01);
    assert_eq!(d.status, shared::DeviceStatus::Normal);
    assert_eq!(d.voltage_mv, Some(11_600));
    assert_eq!(d.discharged_ah, 0.0);
    assert!(d.last_seen_at.is_some());
    assert_eq!(count(&pool, "readings").await, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn later_readings_integrate_the_current(pool: PgPool) {
    let observer = PostgresObserver::new(pool.clone());
    // Start bei voller Batterie, danach 8 A über eine Stunde: 8 Ah von 80 Ah sind 10 Punkte.
    observer
        .on_valid(&reading("TN0001", 12_700, -8_000, None), hours_ago(2))
        .await
        .unwrap();
    observer
        .on_valid(&reading("TN0001", 12_400, -8_000, None), hours_ago(1))
        .await
        .unwrap();

    let d = device(&pool, "TN0001").await;
    assert!((d.discharged_ah - 8.0).abs() < 1e-6);
    assert!((d.soc_percent.unwrap() - 90.0).abs() < 0.01);
    assert_eq!(count(&pool, "readings").await, 2);
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

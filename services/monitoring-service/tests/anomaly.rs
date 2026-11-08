//! Erkennung von Stillstand und Spannungseinbruch gegen eine echte Datenbank.

use chrono::{DateTime, Duration, Utc};
use monitoring_service::anomaly::{self, Tracker};
use shared::db::MIGRATOR;
use shared::events::CHANNEL;
use shared::{AlertKind, AnomalySettings, DomainEvent};
use sqlx::PgPool;
use sqlx::postgres::PgListener;

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-08T12:00:00Z").unwrap().to_utc()
}

async fn device(pool: &PgPool, id: &str, status: &str) {
    sqlx::query("insert into devices (id, status) values ($1, $2::device_status)")
        .bind(id)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
}

/// Ein Messwert `minutes_ago` Minuten vor `now()`.
async fn reading(pool: &PgPool, id: &str, minutes_ago: i64, voltage: i32, current: i32) {
    sqlx::query(
        "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent) \
         values ($1, $2, $3, $4, 15, 60)",
    )
    .bind(now() - Duration::minutes(minutes_ago))
    .bind(id)
    .bind(voltage)
    .bind(current)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn frozen_values_over_the_whole_window_are_stagnation(pool: PgPool) {
    device(&pool, "STUCK", "normal").await;
    device(&pool, "ALIVE", "normal").await;
    for minutes in (0..=360).step_by(30) {
        reading(&pool, "STUCK", minutes, 12_100, -500).await;
        reading(&pool, "ALIVE", minutes, 12_100 - minutes as i32, -500).await;
    }
    assert_eq!(anomaly::stagnant(&pool, now(), 6.0).await.unwrap(), ["STUCK"]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn too_few_or_too_short_series_are_not_stagnation(pool: PgPool) {
    device(&pool, "FEW", "normal").await;
    device(&pool, "SHORT", "normal").await;
    for minutes in [0, 180, 360] {
        reading(&pool, "FEW", minutes, 12_000, -400).await;
    }
    for minutes in [0, 5, 10, 15, 20, 25] {
        reading(&pool, "SHORT", minutes, 12_000, -400).await;
    }
    assert!(anomaly::stagnant(&pool, now(), 6.0).await.unwrap().is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn offline_devices_and_old_batteries_are_not_checked(pool: PgPool) {
    device(&pool, "GONE", "offline").await;
    device(&pool, "NEW", "normal").await;
    for minutes in (0..=360).step_by(30) {
        reading(&pool, "GONE", minutes, 12_000, -400).await;
        reading(&pool, "NEW", minutes, 12_000, -400).await;
    }
    // Die Batterie wurde vor zwei Stunden gewechselt: Nur die letzten zwei Stunden zählen, das reicht nicht.
    sqlx::query("update devices set battery_installed_at = $1 where id = 'NEW'")
        .bind(now() - Duration::hours(2))
        .execute(&pool)
        .await
        .unwrap();
    assert!(anomaly::stagnant(&pool, now(), 6.0).await.unwrap().is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_sudden_voltage_drop_is_found_but_a_slow_fall_is_not(pool: PgPool) {
    device(&pool, "CRASH", "normal").await;
    device(&pool, "SLOW", "normal").await;
    for (minutes, crash, slow) in [
        (50, 12_300, 12_300),
        (30, 12_250, 12_250),
        (10, 11_500, 12_200),
        (0, 11_400, 12_150),
    ] {
        reading(&pool, "CRASH", minutes, crash, -500).await;
        reading(&pool, "SLOW", minutes, slow, -500).await;
    }
    assert_eq!(anomaly::dropped(&pool, now(), 60, 600).await.unwrap(), ["CRASH"]);
    // Außerhalb des Fensters zählt der alte Höchstwert nicht.
    assert!(anomaly::dropped(&pool, now(), 5, 600).await.unwrap().is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn current_combines_both_kinds_with_the_stored_settings(pool: PgPool) {
    device(&pool, "STUCK", "normal").await;
    device(&pool, "CRASH", "normal").await;
    for minutes in (0..=360).step_by(30) {
        reading(&pool, "STUCK", minutes, 12_000, -400).await;
    }
    for (minutes, v) in [(40, 12_300), (20, 11_600), (0, 11_500)] {
        reading(&pool, "CRASH", minutes, v, -500).await;
    }
    let settings = anomaly::load_settings(&pool).await.unwrap();
    assert_eq!(settings, AnomalySettings::default());

    let found = anomaly::current(&pool, now(), &settings).await.unwrap();

    assert_eq!(
        found.into_iter().collect::<Vec<_>>(),
        [
            ("CRASH".to_owned(), AlertKind::Drop),
            ("STUCK".to_owned(), AlertKind::Stagnation)
        ]
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_anomaly_is_announced_once_and_again_after_it_went_away(pool: PgPool) {
    device(&pool, "STUCK", "normal").await;
    for minutes in (0..=360).step_by(30) {
        reading(&pool, "STUCK", minutes, 12_000, -400).await;
    }
    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen(CHANNEL).await.unwrap();
    let mut tracker = Tracker::default();

    assert_eq!(anomaly::sweep(&pool, &mut tracker, now()).await.unwrap(), 1);
    assert_eq!(
        anomaly::sweep(&pool, &mut tracker, now()).await.unwrap(),
        0,
        "nicht jede Runde neu"
    );
    let event = DomainEvent::from_json(listener.recv().await.unwrap().payload()).unwrap();
    assert_eq!(
        event,
        DomainEvent::AnomalyDetected {
            device_id: "STUCK".into(),
            kind: AlertKind::Stagnation
        }
    );

    // Ein neuer, anderer Messwert löst den Stillstand auf, später friert der Wert wieder ein.
    reading(&pool, "STUCK", -1, 11_900, -300).await;
    assert_eq!(
        anomaly::sweep(&pool, &mut tracker, now() + Duration::minutes(1))
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        anomaly::sweep(&pool, &mut tracker, now() + Duration::minutes(1))
            .await
            .unwrap(),
        0
    );
    sqlx::query("delete from readings where voltage_mv = 11900")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        anomaly::sweep(&pool, &mut tracker, now()).await.unwrap(),
        1,
        "wieder neu entstanden"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn switching_detection_off_silences_everything(pool: PgPool) {
    device(&pool, "STUCK", "normal").await;
    for minutes in (0..=360).step_by(30) {
        reading(&pool, "STUCK", minutes, 12_000, -400).await;
    }
    sqlx::query("update anomaly_settings set enabled = false")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(anomaly::sweep(&pool, &mut Tracker::default(), now()).await.unwrap(), 0);
}

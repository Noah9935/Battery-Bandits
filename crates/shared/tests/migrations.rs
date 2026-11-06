//! Integrationstest: Die Migrationen laufen auf einer leeren TimescaleDB durch.
//! Benötigt DATABASE_URL (im Dev-Container gesetzt), jeder Test bekommt eine eigene Datenbank.

use shared::db::MIGRATOR;
use sqlx::PgPool;

#[sqlx::test(migrator = "MIGRATOR")]
async fn creates_all_tables(pool: PgPool) {
    let tables: Vec<String> = sqlx::query_scalar(
        "select table_name::text from information_schema.tables where table_schema = 'public' order by 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for expected in [
        "alerts",
        "devices",
        "invalid_messages_log",
        "maintenance_tasks",
        "readings",
        "system_settings",
    ] {
        assert!(
            tables.iter().any(|t| t == expected),
            "Tabelle {expected} fehlt: {tables:?}"
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn readings_is_a_hypertable(pool: PgPool) {
    let count: i64 = sqlx::query_scalar(
        "select count(*) from timescaledb_information.hypertables where hypertable_name = 'readings'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn settings_row_matches_shared_defaults(pool: PgPool) {
    let settings: shared::Settings = sqlx::query_as(
        "select threshold_critical_pct, threshold_warning_pct, offline_timeout_seconds, \
         min_voltage_mv, max_voltage_mv, battery_capacity_ah from system_settings",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(settings, shared::Settings::default());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn settings_reject_inverted_thresholds(pool: PgPool) {
    let result = sqlx::query("update system_settings set threshold_critical_pct = 50")
        .execute(&pool)
        .await;
    assert!(result.is_err());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn device_and_reading_roundtrip_through_shared_models(pool: PgPool) {
    sqlx::query("insert into devices (id, lat, lon) values ('TN0001', 48.8, 9.0)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent) \
         values (now(), 'TN0001', 12300, -311, 16.4, 80.0)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let device: shared::Device = sqlx::query_as("select * from devices where id = 'TN0001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(device.status, shared::DeviceStatus::Normal);
    assert_eq!(device.lat, Some(48.8));

    let reading: shared::Reading = sqlx::query_as("select * from readings").fetch_one(&pool).await.unwrap();
    assert_eq!(reading.current_ma, -311);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn only_one_open_alert_per_device(pool: PgPool) {
    sqlx::query("insert into devices (id) values ('TN0001')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("insert into alerts (device_id, kind) values ('TN0001', 'critical')")
        .execute(&pool)
        .await
        .unwrap();
    let second = sqlx::query("insert into alerts (device_id, kind) values ('TN0001', 'offline')")
        .execute(&pool)
        .await;
    assert!(second.is_err());

    sqlx::query("update alerts set status = 'resolved', closed_at = now()")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("insert into alerts (device_id, kind) values ('TN0001', 'offline')")
        .execute(&pool)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn half_position_is_rejected(pool: PgPool) {
    let result = sqlx::query("insert into devices (id, lat) values ('TN0002', 48.0)")
        .execute(&pool)
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn connect_with_retry_returns_a_migrated_pool() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL ist im Dev-Container gesetzt");
    let pool = shared::db::connect_with_retry(&url).await.unwrap();
    let rows: i64 = sqlx::query_scalar("select count(*) from system_settings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

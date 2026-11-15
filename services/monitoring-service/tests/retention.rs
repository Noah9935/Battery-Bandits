//! Deckel für die Datenbank gegen eine echte TimescaleDB: Bei zu wenig Platz verschwinden die ältesten Datenstücke,
//! das jüngste bleibt.

use chrono::{DateTime, Duration, Utc};
use monitoring_service::retention::enforce_quota;
use shared::db::MIGRATOR;
use sqlx::PgPool;

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-15T12:00:00Z").unwrap().to_utc()
}

/// Ein Messwert `days_ago` Tage vor `now()`. Eine Woche Abstand ergibt je ein eigenes Datenstück.
async fn reading(pool: &PgPool, days_ago: i64) {
    sqlx::query(
        "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent) \
         values ($1, 'D1', 12000, -500, 15, 60)",
    )
    .bind(now() - Duration::days(days_ago))
    .execute(pool)
    .await
    .unwrap();
}

async fn chunks(pool: &PgPool) -> i64 {
    sqlx::query_scalar("select count(*) from timescaledb_information.chunks where hypertable_name = 'readings'")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn setup(pool: &PgPool) {
    sqlx::query("insert into devices (id, status) values ('D1', 'normal')")
        .execute(pool)
        .await
        .unwrap();
    for days in [0, 21, 42, 63] {
        reading(pool, days).await;
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn with_enough_room_nothing_is_deleted(pool: PgPool) {
    setup(&pool).await;
    let before = chunks(&pool).await;
    assert!(before >= 4, "{before}");
    assert_eq!(enforce_quota(&pool, 10 * 1024 * 1024 * 1024).await.unwrap(), 0);
    assert_eq!(chunks(&pool).await, before);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn without_room_the_oldest_pieces_go_and_the_newest_stays(pool: PgPool) {
    setup(&pool).await;
    let before = chunks(&pool).await;
    // Ein Byte Platz: Es wird gelöscht, bis nur noch das jüngste Datenstück übrig ist.
    let dropped = enforce_quota(&pool, 1).await.unwrap();
    assert_eq!(dropped as i64, before - 1);
    assert_eq!(chunks(&pool).await, 1);
    let newest: Option<DateTime<Utc>> = sqlx::query_scalar("select max(time) from readings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(newest, Some(now()));
    let left: i64 = sqlx::query_scalar("select count(*) from readings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 1);
}

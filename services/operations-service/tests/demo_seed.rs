//! Beispiel-Einsätze mit Kommentaren: nur auf erfundenen Geräten und nur einmal.

use operations_service::repo;
use shared::db::MIGRATOR;
use sqlx::PgPool;

async fn users(pool: &PgPool) {
    sqlx::query(
        "insert into users (username, display_name, password_hash) values ('noah', 'Noah', 'x'), ('fynn', 'Fynn', 'x')",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn devices(pool: &PgPool, simulated: usize) {
    sqlx::query("insert into devices (id, status, soc_percent) values ('TN0001', 'critical', 5)")
        .execute(pool)
        .await
        .unwrap();
    for i in 1..=simulated {
        sqlx::query(
            "insert into devices (id, status, soc_percent, data_source) values ($1, 'normal', 70, 'simulation')",
        )
        .bind(format!("SIM{i:02}"))
        .execute(pool)
        .await
        .unwrap();
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn demo_tasks_with_comments_appear_once_on_simulated_devices_only(pool: PgPool) {
    users(&pool).await;
    devices(&pool, 5).await;
    assert_eq!(repo::seed_demo_tasks(&pool).await.unwrap(), 3);
    assert_eq!(repo::seed_demo_tasks(&pool).await.unwrap(), 0, "nur einmal");

    let on_real: i64 = sqlx::query_scalar("select count(*) from maintenance_tasks where device_id = 'TN0001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(on_real, 0, "echte Geräte bleiben unberührt");
    let comments: i64 = sqlx::query_scalar("select count(*) from task_comments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(comments, 5);
    let inbox: i64 = sqlx::query_scalar("select count(*) from inbox_items where kind = 'comment'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(inbox >= 3, "{inbox}");
    let free: i64 = sqlx::query_scalar("select count(*) from maintenance_tasks where assignee is null")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(free, 1, "ein Einsatz ohne Zuständigkeit zeigt den Knopf zum Übernehmen");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn nothing_is_seeded_without_simulated_devices_or_accounts(pool: PgPool) {
    devices(&pool, 5).await;
    assert_eq!(repo::seed_demo_tasks(&pool).await.unwrap(), 0, "ohne Konten");
    users(&pool).await;
    sqlx::query("delete from devices where data_source = 'simulation'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(repo::seed_demo_tasks(&pool).await.unwrap(), 0, "ohne erfundene Geräte");
    let any: i64 = sqlx::query_scalar("select count(*) from maintenance_tasks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(any, 0);
}

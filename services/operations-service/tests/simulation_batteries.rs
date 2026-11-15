//! Batterien der erfundenen Geräte: feste Zuordnung aus dem Katalog, echte Geräte bleiben unberührt. Dazu die
//! einmalige Aufräum-Migration für die alten Zufallsdaten.

use operations_service::repo::{self, SIMULATION_BATTERIES};
use shared::db::MIGRATOR;
use sqlx::PgPool;

async fn device(pool: &PgPool, id: &str, source: &str) {
    sqlx::query(
        "insert into devices (id, status, soc_percent, data_source) values ($1, 'normal', 70, $2::data_source)",
    )
    .bind(id)
    .bind(source)
    .execute(pool)
    .await
    .unwrap();
}

async fn battery_of(pool: &PgPool, id: &str) -> (Option<String>, Option<i32>) {
    sqlx::query_as("select battery_model, battery_price_cents from devices where id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_catalogue_holds_every_simulation_battery_with_a_plausible_price(pool: PgPool) {
    let distinct: std::collections::BTreeSet<&str> = SIMULATION_BATTERIES.into_iter().collect();
    assert_eq!(distinct.len(), SIMULATION_BATTERIES.len());
    for name in SIMULATION_BATTERIES {
        let price: Option<Option<i32>> =
            sqlx::query_scalar("select price_cents from battery_types where name = $1 and archived_at is null")
                .bind(name)
                .fetch_optional(&pool)
                .await
                .unwrap();
        let cents = price
            .flatten()
            .unwrap_or_else(|| panic!("{name} fehlt im Katalog oder hat keinen Preis"));
        assert!((5_000..=100_000).contains(&cents), "{name}: {cents}");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn simulated_devices_get_fixed_batteries_and_real_devices_stay_untouched(pool: PgPool) {
    device(&pool, "TN0001", "dozent").await;
    for i in 1..=8 {
        device(&pool, &format!("SIM{i:02}"), "simulation").await;
    }
    sqlx::query("update devices set battery_model = 'Eigene' where id = 'SIM08'")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(repo::assign_simulated_batteries(&pool).await.unwrap(), 7);
    assert_eq!(repo::assign_simulated_batteries(&pool).await.unwrap(), 0, "nur einmal");

    let model = |id: &'static str| {
        let pool = pool.clone();
        async move { battery_of(&pool, id).await }
    };
    let (first, price) = model("SIM01").await;
    assert_eq!(first.as_deref(), Some(SIMULATION_BATTERIES[0]));
    let catalogue: Option<i32> = sqlx::query_scalar("select price_cents from battery_types where name = $1")
        .bind(SIMULATION_BATTERIES[0])
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(price, catalogue, "der Preis kommt aus dem Katalog");
    assert_eq!(model("SIM02").await.0.as_deref(), Some(SIMULATION_BATTERIES[1]));
    assert_eq!(model("SIM05").await.0.as_deref(), Some(SIMULATION_BATTERIES[4]));
    assert_eq!(
        model("SIM06").await.0.as_deref(),
        Some(SIMULATION_BATTERIES[0]),
        "danach wieder von vorn"
    );
    assert_eq!(model("TN0001").await, (None, None), "echte Geräte bleiben unberührt");
    assert_eq!(
        model("SIM08").await,
        (Some("Eigene".to_owned()), None),
        "vorhandene Angaben bleiben"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_model_taken_out_of_the_catalogue_is_skipped_until_it_is_back(pool: PgPool) {
    device(&pool, "SIM01", "simulation").await;
    device(&pool, "SIM02", "simulation").await;
    sqlx::query("update battery_types set archived_at = now() where name = $1")
        .bind(SIMULATION_BATTERIES[1])
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(repo::assign_simulated_batteries(&pool).await.unwrap(), 1);
    assert_eq!(battery_of(&pool, "SIM02").await, (None, None));

    sqlx::query("update battery_types set archived_at = null where name = $1")
        .bind(SIMULATION_BATTERIES[1])
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(repo::assign_simulated_batteries(&pool).await.unwrap(), 1);
    assert_eq!(
        battery_of(&pool, "SIM02").await.0.as_deref(),
        Some(SIMULATION_BATTERIES[1])
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_cleanup_migration_removes_old_simulation_data_and_keeps_real_devices(pool: PgPool) {
    device(&pool, "TN0001", "dozent").await;
    device(&pool, "SIM01", "simulation").await;
    for id in ["TN0001", "SIM01"] {
        sqlx::query(
            "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent) \
             values (now(), $1, 12000, -300, 15, 70)",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let task: i64 =
            sqlx::query_scalar("insert into maintenance_tasks (device_id, note) values ($1, 'x') returning id")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query("insert into task_comments (task_id, author_name, body) values ($1, 'Noah', 'x')")
            .bind(task)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("insert into user_pins (username, device_id) values ('noah', $1)")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("insert into inbox_items (username, kind, text, device_id) values ('noah', 'alert', 'x', $1)")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("insert into activity_log (actor_name, device_id, text) values ('Noah', $1, 'x')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("insert into tours (name, owner, device_ids) values ('Runde', 'noah', array['TN0001', 'SIM01'])")
        .execute(&pool)
        .await
        .unwrap();
    for topic in ["traffic-nodes/SIM01/measurements", "traffic-nodes/TN0001/measurements"] {
        sqlx::query("insert into invalid_messages_log (topic, payload, reason) values ($1, 'x', 'x')")
            .bind(topic)
            .execute(&pool)
            .await
            .unwrap();
    }

    sqlx::raw_sql(include_str!("../../../migrations/0021_clear_random_simulation.sql"))
        .execute(&pool)
        .await
        .unwrap();

    let ids: Vec<String> = sqlx::query_scalar("select id from devices order by id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(ids, ["TN0001"]);
    for table in [
        "readings",
        "maintenance_tasks",
        "user_pins",
        "inbox_items",
        "activity_log",
    ] {
        assert_eq!(
            count(&pool, &format!("select count(*) from {table}")).await,
            1,
            "{table}"
        );
    }
    assert_eq!(count(&pool, "select count(*) from task_comments").await, 1);
    let tour: Vec<String> = sqlx::query_scalar("select device_ids from tours")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tour, ["TN0001"], "erfundene Geräte verschwinden auch aus den Touren");
    assert_eq!(count(&pool, "select count(*) from invalid_messages_log").await, 1);
}

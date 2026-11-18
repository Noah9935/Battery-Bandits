//! Integrationstests der Überwachung gegen eine echte TimescaleDB (DATABASE_URL).

use std::time::Duration;

use chrono::{DateTime, Utc};
use monitoring_service::service::{evaluate_all, evaluate_device, on_battery_replaced, refresh_forecasts};
use shared::db::MIGRATOR;
use shared::{Device, DeviceStatus, DomainEvent};
use sqlx::PgPool;
use sqlx::postgres::PgListener;

async fn add_device(pool: &PgPool, id: &str, status: &str, soc: Option<f32>, seen_secs_ago: i64) {
    sqlx::query(
        "insert into devices (id, status, soc_percent, last_seen_at, soc_start_percent, discharged_ah, \
         battery_installed_at) values ($1, $2::device_status, $3, now() - make_interval(secs => $4), 100, 5, now() - interval '3 days')",
    )
    .bind(id)
    .bind(status)
    .bind(soc)
    .bind(seen_secs_ago as f64)
    .execute(pool)
    .await
    .unwrap();
}

async fn device(pool: &PgPool, id: &str) -> Device {
    sqlx::query_as("select * from devices where id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn listener(pool: &PgPool) -> PgListener {
    let mut l = PgListener::connect_with(pool).await.unwrap();
    l.listen(shared::events::CHANNEL).await.unwrap();
    l
}

async fn next_event(l: &mut PgListener) -> DomainEvent {
    let n = tokio::time::timeout(Duration::from_secs(5), l.recv())
        .await
        .expect("kein Ereignis")
        .unwrap();
    DomainEvent::from_json(n.payload()).unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn low_battery_becomes_critical_and_publishes_the_change(pool: PgPool) {
    add_device(&pool, "TN0001", "normal", Some(15.0), 10).await;
    let mut l = listener(&pool).await;

    let event = evaluate_device(&pool, "TN0001", Utc::now()).await.unwrap();

    assert_eq!(device(&pool, "TN0001").await.status, DeviceStatus::Critical);
    let expected = DomainEvent::DeviceStatusChanged {
        device_id: "TN0001".into(),
        old_status: DeviceStatus::Normal,
        new_status: DeviceStatus::Critical,
        soc_percent: Some(15.0),
    };
    assert_eq!(event, Some(expected.clone()));
    assert_eq!(next_event(&mut l).await, expected);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn unchanged_status_publishes_nothing(pool: PgPool) {
    add_device(&pool, "TN0001", "normal", Some(80.0), 10).await;
    let mut l = listener(&pool).await;

    assert_eq!(evaluate_device(&pool, "TN0001", Utc::now()).await.unwrap(), None);

    assert!(
        tokio::time::timeout(Duration::from_millis(300), l.recv())
            .await
            .is_err()
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn unknown_device_is_ignored(pool: PgPool) {
    assert_eq!(evaluate_device(&pool, "NOPE", Utc::now()).await.unwrap(), None);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn silent_devices_go_offline_and_come_back(pool: PgPool) {
    add_device(&pool, "TN0001", "normal", Some(80.0), 10).await;
    add_device(&pool, "TN0002", "normal", Some(80.0), 2 * 3600).await;

    let changed = evaluate_all(&pool, Utc::now()).await.unwrap();

    assert_eq!(changed, 1);
    assert_eq!(device(&pool, "TN0001").await.status, DeviceStatus::Normal);
    assert_eq!(device(&pool, "TN0002").await.status, DeviceStatus::Offline);

    // Wieder eine Messung: Gerät ist erreichbar und bekommt seinen Ladezustands-Status zurück.
    sqlx::query("update devices set last_seen_at = now(), soc_percent = 12 where id = 'TN0002'")
        .execute(&pool)
        .await
        .unwrap();
    evaluate_device(&pool, "TN0002", Utc::now()).await.unwrap();
    assert_eq!(device(&pool, "TN0002").await.status, DeviceStatus::Critical);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn changed_thresholds_apply_without_restart(pool: PgPool) {
    add_device(&pool, "TN0001", "normal", Some(55.0), 10).await;
    assert_eq!(evaluate_all(&pool, Utc::now()).await.unwrap(), 0);

    sqlx::query("update system_settings set threshold_warning_pct = 60")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(evaluate_all(&pool, Utc::now()).await.unwrap(), 1);
    assert_eq!(device(&pool, "TN0001").await.status, DeviceStatus::Warning);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn battery_replacement_resets_the_cycle_and_the_status(pool: PgPool) {
    add_device(&pool, "TN0001", "critical", Some(10.0), 10).await;
    let mut l = listener(&pool).await;
    let at: DateTime<Utc> = Utc::now();

    on_battery_replaced(&pool, "TN0001", at, None, None, None, Utc::now())
        .await
        .unwrap();

    let d = device(&pool, "TN0001").await;
    assert_eq!(d.status, DeviceStatus::Normal);
    assert_eq!((d.soc_percent, d.soc_start_percent, d.discharged_ah), (None, None, 0.0));
    assert!(d.battery_installed_at.unwrap() >= at - chrono::Duration::seconds(1));
    assert!(matches!(
        next_event(&mut l).await,
        DomainEvent::DeviceStatusChanged {
            new_status: DeviceStatus::Normal,
            old_status: DeviceStatus::Critical,
            ..
        }
    ));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_status_change_records_since_when_the_state_holds(pool: PgPool) {
    add_device(&pool, "TN0001", "normal", Some(15.0), 10).await;
    assert_eq!(device(&pool, "TN0001").await.status_changed_at, None);
    let now = Utc::now();

    evaluate_device(&pool, "TN0001", now).await.unwrap();
    let changed = device(&pool, "TN0001").await.status_changed_at.unwrap();
    assert!((changed - now).num_milliseconds().abs() < 5, "{changed} statt {now}");

    // Ohne Wechsel bleibt der Zeitpunkt stehen.
    evaluate_device(&pool, "TN0001", now + chrono::Duration::minutes(5))
        .await
        .unwrap();
    assert_eq!(device(&pool, "TN0001").await.status_changed_at, Some(changed));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_stated_charge_applies_at_once_and_forgets_the_old_current(pool: PgPool) {
    add_device(&pool, "TN0001", "critical", Some(10.0), 10).await;
    sqlx::query("update devices set current_ma = -800 where id = 'TN0001'")
        .execute(&pool)
        .await
        .unwrap();

    on_battery_replaced(&pool, "TN0001", Utc::now(), Some(100.0), None, None, Utc::now())
        .await
        .unwrap();

    let d = device(&pool, "TN0001").await;
    assert_eq!(d.status, DeviceStatus::Normal);
    assert_eq!(
        (d.soc_percent, d.soc_start_percent, d.discharged_ah),
        (Some(100.0), Some(100.0), 0.0)
    );
    assert_eq!(
        d.current_ma, None,
        "die Integration beginnt erst mit der nächsten Messung"
    );

    // Ein unsinniger Wert wird begrenzt.
    on_battery_replaced(&pool, "TN0001", Utc::now(), Some(250.0), None, None, Utc::now())
        .await
        .unwrap();
    assert_eq!(device(&pool, "TN0001").await.soc_percent, Some(100.0));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_finished_cycle_is_kept_for_comparison(pool: PgPool) {
    add_device(&pool, "TN0001", "critical", Some(10.0), 10).await;
    // Zwei Tage Messwerte im laufenden Zyklus: Die Spannung fällt von 12,6 auf 11,0 V.
    sqlx::query(
        "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent) \
         select now() - g * interval '1 hour', 'TN0001', 11000 + g * 100, -300, 8, 50 from generate_series(0, 16) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    // Zwei Sekunden Luft, damit die jüngste Messung sicher noch in den Zyklus fällt.
    let at = Utc::now() + chrono::Duration::seconds(2);

    on_battery_replaced(&pool, "TN0001", at, Some(100.0), None, None, at)
        .await
        .unwrap();

    let cycle: shared::BatteryCycle = sqlx::query_as("select * from battery_cycles where device_id = 'TN0001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cycle.discharged_ah, 5.0);
    assert_eq!(
        (cycle.start_voltage_mv, cycle.end_voltage_mv),
        (Some(12_600), Some(11_000))
    );
    assert_eq!(cycle.avg_temperature_c, Some(8.0));
    assert!(
        (cycle.ended_at - cycle.started_at).num_hours() >= 71,
        "der Zyklus lief drei Tage"
    );

    // Ein zweiter Wechsel direkt danach: Im neuen Zyklus gibt es keine Messwerte, also keinen Eintrag.
    on_battery_replaced(&pool, "TN0001", at + chrono::Duration::seconds(5), None, None, None, at)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("select count(*) from battery_cycles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_price_belongs_to_the_battery_and_moves_into_the_finished_cycle(pool: PgPool) {
    add_device(&pool, "TN0001", "critical", Some(10.0), 10).await;
    sqlx::query("update devices set battery_price_cents = 9000 where id = 'TN0001'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent) \
         select now() - g * interval '1 hour', 'TN0001', 11000 + g * 100, -300, 8, 50 from generate_series(0, 16) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    let at = Utc::now() + chrono::Duration::seconds(2);

    on_battery_replaced(&pool, "TN0001", at, None, Some(12_500), None, at)
        .await
        .unwrap();

    let cycle_price: Option<i32> =
        sqlx::query_scalar("select price_cents from battery_cycles where device_id = 'TN0001'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        cycle_price,
        Some(9_000),
        "der Preis der alten Batterie wandert in ihren Zyklus"
    );
    let now_price: Option<i32> = sqlx::query_scalar("select battery_price_cents from devices where id = 'TN0001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(now_price, Some(12_500), "die neue Batterie hat ihren eigenen Preis");

    // Ohne Angabe beim nächsten Wechsel bleibt der Preis leer.
    on_battery_replaced(&pool, "TN0001", at + chrono::Duration::seconds(5), None, None, None, at)
        .await
        .unwrap();
    let cleared: Option<i32> = sqlx::query_scalar("select battery_price_cents from devices where id = 'TN0001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cleared, None);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn replacing_the_battery_of_an_unknown_device_does_nothing(pool: PgPool) {
    on_battery_replaced(&pool, "GIBTESNICHT", Utc::now(), Some(100.0), None, None, Utc::now())
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("select count(*) from battery_cycles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

async fn add_series(pool: &PgPool, id: &str, points: i32) {
    // g = 0 ist die neueste Messung (56 Prozent), alle 15 Minuten 0,5 Punkte mehr, also 2 Punkte pro Stunde Entladung.
    sqlx::query(
        "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent) \
         select now() - g * interval '15 minutes', $1, 12000, -300, 15, (56 + 0.5 * g)::real from generate_series(0, $2 - 1) g",
    )
    .bind(id)
    .bind(points)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn forecast_follows_the_discharge_rate(pool: PgPool) {
    add_device(&pool, "TN0001", "normal", Some(56.0), 10).await;
    add_series(&pool, "TN0001", 25).await;

    assert_eq!(refresh_forecasts(&pool).await.unwrap(), 1);

    let d = device(&pool, "TN0001").await;
    assert!((d.discharge_rate_pct_per_hour.unwrap() - 2.0).abs() < 0.01);
    assert!((d.forecast_hours_remaining.unwrap() - 28.0).abs() < 0.1);
    // Die Spanne umschließt den Wert und wird mit der Unsicherheit des Ladezustands breiter.
    let (low, high) = (d.forecast_hours_low.unwrap(), d.forecast_hours_high.unwrap());
    assert!(low < 28.0 && 28.0 < high, "{low} bis {high}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_less_certain_charge_gives_a_wider_forecast_band(pool: PgPool) {
    add_device(&pool, "TN0001", "normal", Some(56.0), 10).await;
    add_device(&pool, "TN0002", "normal", Some(56.0), 10).await;
    add_series(&pool, "TN0001", 25).await;
    add_series(&pool, "TN0002", 25).await;
    sqlx::query("update devices set soc_sigma_percent = case id when 'TN0001' then 3 else 12 end")
        .execute(&pool)
        .await
        .unwrap();

    refresh_forecasts(&pool).await.unwrap();

    let (sure, unsure) = (device(&pool, "TN0001").await, device(&pool, "TN0002").await);
    let width = |d: &shared::Device| d.forecast_hours_high.unwrap() - d.forecast_hours_low.unwrap();
    assert!(
        width(&unsure) > width(&sure) * 1.5,
        "{} gegen {}",
        width(&unsure),
        width(&sure)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_battery_swap_with_a_stated_charge_starts_the_filter_with_that_value(pool: PgPool) {
    add_device(&pool, "TN0001", "normal", Some(30.0), 10).await;
    let at = Utc::now();

    on_battery_replaced(&pool, "TN0001", at, Some(100.0), None, None, at)
        .await
        .unwrap();

    let d = device(&pool, "TN0001").await;
    assert_eq!(d.soc_percent, Some(100.0));
    let sigma = d.soc_sigma_percent.unwrap();
    assert!((3.0..6.0).contains(&sigma), "{sigma}");
    let state: Option<String> = sqlx::query_scalar("select estimator_state::text from devices where id = 'TN0001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let state: shared::estimator::EstimatorState = serde_json::from_str(&state.unwrap()).unwrap();
    assert_eq!(
        (state.soc, state.last_at),
        (100.0, None),
        "noch keine Messung der neuen Batterie"
    );

    // Ohne Angabe beginnt das Filter bei der nächsten Messung aus der Spannung.
    on_battery_replaced(&pool, "TN0001", at + chrono::Duration::seconds(5), None, None, None, at)
        .await
        .unwrap();
    let state: Option<String> = sqlx::query_scalar("select estimator_state::text from devices where id = 'TN0001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, None);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn too_little_data_gives_no_forecast_and_clears_old_values(pool: PgPool) {
    add_device(&pool, "TN0001", "normal", Some(56.0), 10).await;
    add_device(&pool, "TN0002", "normal", Some(56.0), 10).await;
    add_series(&pool, "TN0001", 2).await;
    sqlx::query("update devices set discharge_rate_pct_per_hour = 9, forecast_hours_remaining = 9 where id = 'TN0002'")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(refresh_forecasts(&pool).await.unwrap(), 0);

    for id in ["TN0001", "TN0002"] {
        let d = device(&pool, id).await;
        assert_eq!(
            (d.discharge_rate_pct_per_hour, d.forecast_hours_remaining),
            (None, None),
            "{id}"
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn forecast_ignores_readings_before_the_last_battery_change(pool: PgPool) {
    add_device(&pool, "TN0001", "normal", Some(56.0), 10).await;
    add_series(&pool, "TN0001", 25).await;
    // Wechsel vor 10 Minuten: davor liegende Messwerte gehören zur alten Batterie.
    sqlx::query("update devices set battery_installed_at = now() - interval '10 minutes'")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(refresh_forecasts(&pool).await.unwrap(), 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn retired_devices_are_not_evaluated(pool: PgPool) {
    add_device(&pool, "TN0001", "normal", Some(5.0), 10).await;
    add_device(&pool, "TN0002", "normal", Some(5.0), 10).await;
    sqlx::query("update devices set retired_at = now(), retired_reason = 'done' where id = 'TN0001'")
        .execute(&pool)
        .await
        .unwrap();
    let changed = evaluate_all(&pool, Utc::now()).await.unwrap();
    assert_eq!(changed, 1, "nur das aktive Gerät wird kritisch");
    assert_eq!(device(&pool, "TN0001").await.status, DeviceStatus::Normal);
    assert_eq!(device(&pool, "TN0002").await.status, DeviceStatus::Critical);
    assert!(evaluate_device(&pool, "TN0001", Utc::now()).await.unwrap().is_none());
}

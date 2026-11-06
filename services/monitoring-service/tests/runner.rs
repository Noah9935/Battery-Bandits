//! Durchstich der Hauptschleife: Ereignis rein, Statusänderung raus.

use std::time::Duration;

use chrono::Utc;
use monitoring_service::runner::{RunnerConfig, run};
use shared::db::{MIGRATOR, notify_event};
use shared::{DeviceStatus, DomainEvent};
use sqlx::PgPool;
use sqlx::postgres::PgListener;

async fn status_of(pool: &PgPool, id: &str) -> DeviceStatus {
    sqlx::query_scalar("select status from devices where id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn wait_for_status(pool: &PgPool, id: &str, wanted: DeviceStatus) {
    for _ in 0..100 {
        if status_of(pool, id).await == wanted {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{id} wurde nicht {wanted:?}, ist {:?}", status_of(pool, id).await);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn reacts_to_events_and_to_silence(pool: PgPool) {
    sqlx::query(
        "insert into devices (id, soc_percent, last_seen_at) values \
         ('LIVE', 80, now()), ('QUIET', 80, now() - interval '2 hours')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let config = RunnerConfig {
        sweep_interval: Duration::from_millis(200),
        forecast_interval: Duration::from_secs(60),
    };
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn({
        let pool = pool.clone();
        async move {
            run(pool, config, async {
                let _ = stopped.await;
            })
            .await
        }
    });

    // Offline-Erkennung braucht kein Ereignis: schon der Start und der Zeitgeber finden das stumme Gerät.
    wait_for_status(&pool, "QUIET", DeviceStatus::Offline).await;
    assert_eq!(status_of(&pool, "LIVE").await, DeviceStatus::Normal);

    // Ein Messwert mit niedriger Ladung kommt als Ereignis an.
    let mut observer = PgListener::connect_with(&pool).await.unwrap();
    observer.listen(shared::events::CHANNEL).await.unwrap();
    sqlx::query("update devices set soc_percent = 12, last_seen_at = now() where id = 'LIVE'")
        .execute(&pool)
        .await
        .unwrap();
    notify_event(
        &pool,
        &DomainEvent::ReadingSaved {
            device_id: "LIVE".into(),
            time: Utc::now(),
            soc_percent: 12.0,
        },
    )
    .await
    .unwrap();
    wait_for_status(&pool, "LIVE", DeviceStatus::Critical).await;

    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn battery_replaced_event_resets_a_critical_device(pool: PgPool) {
    sqlx::query("insert into devices (id, status, soc_percent, last_seen_at) values ('TN0001', 'critical', 10, now())")
        .execute(&pool)
        .await
        .unwrap();
    let config = RunnerConfig {
        sweep_interval: Duration::from_secs(60),
        forecast_interval: Duration::from_secs(60),
    };
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn({
        let pool = pool.clone();
        async move {
            run(pool, config, async {
                let _ = stopped.await;
            })
            .await
        }
    });
    tokio::time::sleep(Duration::from_millis(300)).await; // Listener ist bereit

    notify_event(
        &pool,
        &DomainEvent::BatteryReplaced {
            device_id: "TN0001".into(),
            at: Utc::now(),
        },
    )
    .await
    .unwrap();

    wait_for_status(&pool, "TN0001", DeviceStatus::Normal).await;
    stop.send(()).unwrap();
    task.await.unwrap().unwrap();
}

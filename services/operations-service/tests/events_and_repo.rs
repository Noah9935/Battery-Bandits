//! Integrationstests: Ereignisse erzeugen und lösen Meldungen, Einsätze senden BatteryReplaced.

use std::time::Duration;

use chrono::Utc;
use operations_service::events::handle_event;
use operations_service::repo::{self, RepoError};
use shared::db::MIGRATOR;
use shared::{Alert, AlertKind, AlertStatus, DeviceStatus, DomainEvent, MaintenanceStatus, NewMaintenanceTask};
use sqlx::PgPool;
use sqlx::postgres::PgListener;

async fn add_device(pool: &PgPool, id: &str, status: &str) {
    sqlx::query("insert into devices (id, status) values ($1, $2::device_status)")
        .bind(id)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
}

fn changed(id: &str, old: DeviceStatus, new: DeviceStatus) -> DomainEvent {
    DomainEvent::DeviceStatusChanged {
        device_id: id.into(),
        old_status: old,
        new_status: new,
        soc_percent: Some(15.0),
    }
}

async fn alerts(pool: &PgPool, status: Option<AlertStatus>) -> Vec<Alert> {
    repo::list_alerts(pool, status).await.unwrap()
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

fn new_task(device: &str) -> NewMaintenanceTask {
    NewMaintenanceTask {
        device_id: device.into(),
        alert_id: None,
        note: None,
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn critical_device_gets_one_alert_and_an_event(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    let mut l = listener(&pool).await;

    handle_event(&pool, &changed("TN0001", DeviceStatus::Normal, DeviceStatus::Critical))
        .await
        .unwrap();
    handle_event(&pool, &changed("TN0001", DeviceStatus::Normal, DeviceStatus::Critical))
        .await
        .unwrap();

    let open = alerts(&pool, Some(AlertStatus::Open)).await;
    assert_eq!(open.len(), 1, "keine doppelte Meldung");
    assert_eq!(
        (open[0].device_id.as_str(), open[0].kind),
        ("TN0001", AlertKind::Critical)
    );
    assert!(
        matches!(next_event(&mut l).await, DomainEvent::AlertCreated { device_id, kind: AlertKind::Critical, .. } if device_id == "TN0001")
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(300), l.recv())
            .await
            .is_err(),
        "zweites Ereignis ohne neue Meldung"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn offline_device_gets_an_offline_alert(pool: PgPool) {
    add_device(&pool, "TN0002", "offline").await;
    handle_event(&pool, &changed("TN0002", DeviceStatus::Normal, DeviceStatus::Offline))
        .await
        .unwrap();
    assert_eq!(alerts(&pool, None).await[0].kind, AlertKind::Offline);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn recovery_resolves_the_open_alert_but_keeps_dismissed_ones(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    add_device(&pool, "TN0002", "critical").await;
    for id in ["TN0001", "TN0002"] {
        handle_event(&pool, &changed(id, DeviceStatus::Normal, DeviceStatus::Critical))
            .await
            .unwrap();
    }
    let second = alerts(&pool, None)
        .await
        .into_iter()
        .find(|a| a.device_id == "TN0002")
        .unwrap();
    repo::dismiss_alert(&pool, second.id).await.unwrap();

    for id in ["TN0001", "TN0002"] {
        handle_event(&pool, &changed(id, DeviceStatus::Critical, DeviceStatus::Normal))
            .await
            .unwrap();
    }

    let all = alerts(&pool, None).await;
    let status_of = |id: &str| all.iter().find(|a| a.device_id == id).unwrap().status;
    assert_eq!(status_of("TN0001"), AlertStatus::Resolved);
    assert_eq!(status_of("TN0002"), AlertStatus::Dismissed);
    assert!(
        all.iter()
            .find(|a| a.device_id == "TN0001")
            .unwrap()
            .closed_at
            .is_some()
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn warning_also_resolves(pool: PgPool) {
    add_device(&pool, "TN0001", "offline").await;
    handle_event(&pool, &changed("TN0001", DeviceStatus::Normal, DeviceStatus::Offline))
        .await
        .unwrap();
    handle_event(&pool, &changed("TN0001", DeviceStatus::Offline, DeviceStatus::Warning))
        .await
        .unwrap();
    assert_eq!(alerts(&pool, None).await[0].status, AlertStatus::Resolved);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn dismiss_validates_state(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    handle_event(&pool, &changed("TN0001", DeviceStatus::Normal, DeviceStatus::Critical))
        .await
        .unwrap();
    let id = alerts(&pool, None).await[0].id;

    let dismissed = repo::dismiss_alert(&pool, id).await.unwrap();
    assert_eq!(dismissed.status, AlertStatus::Dismissed);

    assert!(matches!(
        repo::dismiss_alert(&pool, id).await,
        Err(RepoError::Conflict(_))
    ));
    assert!(matches!(
        repo::dismiss_alert(&pool, 999_999).await,
        Err(RepoError::NotFound(_))
    ));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn open_alert_follows_the_current_kind_without_a_second_alert(pool: PgPool) {
    add_device(&pool, "TN0001", "offline").await;
    handle_event(&pool, &changed("TN0001", DeviceStatus::Normal, DeviceStatus::Offline))
        .await
        .unwrap();
    assert_eq!(alerts(&pool, None).await[0].kind, AlertKind::Offline);
    let mut l = listener(&pool).await;

    // Das Gerät sendet wieder und ist jetzt kritisch: Die Meldung sagt nicht mehr "keine Messwerte".
    handle_event(&pool, &changed("TN0001", DeviceStatus::Offline, DeviceStatus::Critical))
        .await
        .unwrap();

    let all = alerts(&pool, None).await;
    assert_eq!(all.len(), 1);
    assert_eq!((all[0].kind, all[0].status), (AlertKind::Critical, AlertStatus::Open));
    assert!(
        tokio::time::timeout(Duration::from_millis(300), l.recv())
            .await
            .is_err(),
        "keine neue Meldung"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_device_can_be_alerted_again_after_a_dismissed_alert(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    handle_event(&pool, &changed("TN0001", DeviceStatus::Normal, DeviceStatus::Critical))
        .await
        .unwrap();
    repo::dismiss_alert(&pool, alerts(&pool, None).await[0].id)
        .await
        .unwrap();

    handle_event(&pool, &changed("TN0001", DeviceStatus::Critical, DeviceStatus::Offline))
        .await
        .unwrap();

    assert_eq!(alerts(&pool, Some(AlertStatus::Open)).await.len(), 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn planning_rules(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;

    let task = repo::plan_task(
        &pool,
        &NewMaintenanceTask {
            note: Some("  Hinterhof  ".into()),
            ..new_task("TN0001")
        },
    )
    .await
    .unwrap();
    assert_eq!(
        (task.status, task.note.as_deref()),
        (MaintenanceStatus::Planned, Some("Hinterhof"))
    );

    assert!(
        matches!(
            repo::plan_task(&pool, &new_task("TN0001")).await,
            Err(RepoError::Conflict(_))
        ),
        "nur ein geplanter Einsatz je Gerät"
    );
    assert!(matches!(
        repo::plan_task(&pool, &new_task("GIBTS_NICHT")).await,
        Err(RepoError::NotFound(_))
    ));
    let bad_alert = NewMaintenanceTask {
        alert_id: Some(424_242),
        ..new_task("TN0001")
    };
    assert!(repo::plan_task(&pool, &bad_alert).await.is_err());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn completing_sends_battery_replaced_and_resolves_the_alert(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    handle_event(&pool, &changed("TN0001", DeviceStatus::Normal, DeviceStatus::Critical))
        .await
        .unwrap();
    let task = repo::plan_task(&pool, &new_task("TN0001")).await.unwrap();
    let mut l = listener(&pool).await;

    let done = repo::complete_task(&pool, task.id).await.unwrap();

    assert_eq!(done.status, MaintenanceStatus::Completed);
    assert!(done.completed_at.is_some());
    assert!(
        matches!(next_event(&mut l).await, DomainEvent::BatteryReplaced { device_id, .. } if device_id == "TN0001")
    );
    assert_eq!(alerts(&pool, None).await[0].status, AlertStatus::Resolved);

    assert!(matches!(
        repo::complete_task(&pool, task.id).await,
        Err(RepoError::Conflict(_))
    ));
    assert!(matches!(
        repo::complete_task(&pool, 999_999).await,
        Err(RepoError::NotFound(_))
    ));
    // Nach dem Wechsel kann wieder ein Einsatz geplant werden.
    repo::plan_task(&pool, &new_task("TN0001")).await.unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn external_battery_replaced_event_completes_planned_tasks(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    repo::plan_task(&pool, &new_task("TN0001")).await.unwrap();

    handle_event(
        &pool,
        &DomainEvent::BatteryReplaced {
            device_id: "TN0001".into(),
            at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let tasks = repo::list_tasks(&pool, Some(MaintenanceStatus::Planned)).await.unwrap();
    assert!(tasks.is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn lists_are_ordered_open_first_and_filterable(pool: PgPool) {
    for id in ["TN0001", "TN0002"] {
        add_device(&pool, id, "critical").await;
        handle_event(&pool, &changed(id, DeviceStatus::Normal, DeviceStatus::Critical))
            .await
            .unwrap();
    }
    let first = alerts(&pool, None)
        .await
        .into_iter()
        .find(|a| a.device_id == "TN0001")
        .unwrap();
    repo::dismiss_alert(&pool, first.id).await.unwrap();

    let all = alerts(&pool, None).await;
    assert_eq!(all[0].status, AlertStatus::Open, "offene Meldungen zuerst");
    assert_eq!(alerts(&pool, Some(AlertStatus::Dismissed)).await.len(), 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn reconcile_catches_up_after_downtime(pool: PgPool) {
    add_device(&pool, "MISSED", "critical").await;
    add_device(&pool, "HEALTHY", "normal").await;
    sqlx::query("insert into alerts (device_id, kind) values ('HEALTHY', 'critical')")
        .execute(&pool)
        .await
        .unwrap();
    add_device(&pool, "SILENCED", "critical").await;
    sqlx::query(
        "insert into alerts (device_id, kind, status, closed_at) values ('SILENCED', 'critical', 'dismissed', now())",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (opened, resolved) = repo::reconcile(&pool).await.unwrap();

    assert_eq!((opened, resolved), (1, 1));
    let open = alerts(&pool, Some(AlertStatus::Open)).await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].device_id, "MISSED");
}

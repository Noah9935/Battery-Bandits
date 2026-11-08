//! Integrationstests: Ereignisse erzeugen und lösen Meldungen, Einsätze senden BatteryReplaced.

use std::time::Duration;

use chrono::Utc;
use operations_service::events::handle_event;
use operations_service::repo::{self, Actor, RepoError};
use shared::db::MIGRATOR;
use shared::{
    Alert, AlertKind, AlertStatus, DeviceStatus, DomainEvent, MaintenanceStatus, NewMaintenanceTask, TaskKind,
    TaskPriority,
};
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
        ..Default::default()
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
    repo::dismiss_alert(&pool, second.id, &Actor::default()).await.unwrap();

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

    let dismissed = repo::dismiss_alert(&pool, id, &Actor::default()).await.unwrap();
    assert_eq!(dismissed.status, AlertStatus::Dismissed);

    assert!(matches!(
        repo::dismiss_alert(&pool, id, &Actor::default()).await,
        Err(RepoError::Conflict(_))
    ));
    assert!(matches!(
        repo::dismiss_alert(&pool, 999_999, &Actor::default()).await,
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
    repo::dismiss_alert(&pool, alerts(&pool, None).await[0].id, &Actor::default())
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
        &Actor::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        (task.status, task.note.as_deref()),
        (MaintenanceStatus::Planned, Some("Hinterhof"))
    );

    assert!(
        matches!(
            repo::plan_task(&pool, &new_task("TN0001"), &Actor::default()).await,
            Err(RepoError::Conflict(_))
        ),
        "nur ein geplanter Einsatz je Gerät"
    );
    assert!(matches!(
        repo::plan_task(&pool, &new_task("GIBTS_NICHT"), &Actor::default()).await,
        Err(RepoError::NotFound(_))
    ));
    let bad_alert = NewMaintenanceTask {
        alert_id: Some(424_242),
        ..new_task("TN0001")
    };
    assert!(repo::plan_task(&pool, &bad_alert, &Actor::default()).await.is_err());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn completing_sends_battery_replaced_and_resolves_the_alert(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    handle_event(&pool, &changed("TN0001", DeviceStatus::Normal, DeviceStatus::Critical))
        .await
        .unwrap();
    let task = repo::plan_task(&pool, &new_task("TN0001"), &Actor::default())
        .await
        .unwrap();
    let mut l = listener(&pool).await;

    let done = repo::complete_task(&pool, task.id, &Actor::default()).await.unwrap();

    assert_eq!(done.status, MaintenanceStatus::Completed);
    assert!(done.completed_at.is_some());
    assert!(
        matches!(next_event(&mut l).await, DomainEvent::BatteryReplaced { device_id, .. } if device_id == "TN0001")
    );
    assert_eq!(alerts(&pool, None).await[0].status, AlertStatus::Resolved);

    assert!(matches!(
        repo::complete_task(&pool, task.id, &Actor::default()).await,
        Err(RepoError::Conflict(_))
    ));
    assert!(matches!(
        repo::complete_task(&pool, 999_999, &Actor::default()).await,
        Err(RepoError::NotFound(_))
    ));
    // Nach dem Wechsel kann wieder ein Einsatz geplant werden.
    repo::plan_task(&pool, &new_task("TN0001"), &Actor::default())
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn external_battery_replaced_event_completes_planned_tasks(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    repo::plan_task(&pool, &new_task("TN0001"), &Actor::default())
        .await
        .unwrap();

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
    repo::dismiss_alert(&pool, first.id, &Actor::default()).await.unwrap();

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

#[sqlx::test(migrator = "MIGRATOR")]
async fn listener_loop_turns_a_status_change_into_an_alert(pool: PgPool) {
    // Das Gerät ist in der Datenbank "normal", der Abgleich beim Start eröffnet also nichts:
    // Die Meldung kann nur aus dem Ereignis kommen.
    add_device(&pool, "TN0001", "normal").await;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn({
        let pool = pool.clone();
        async move {
            operations_service::events::run(pool, async {
                let _ = stopped.await;
            })
            .await
        }
    });
    tokio::time::sleep(Duration::from_millis(400)).await; // Hörer ist bereit

    shared::db::notify_event(&pool, &changed("TN0001", DeviceStatus::Normal, DeviceStatus::Critical))
        .await
        .unwrap();

    let mut opened = false;
    for _ in 0..100 {
        if alerts(&pool, Some(AlertStatus::Open)).await.len() == 1 {
            opened = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(opened, "Meldung wurde nicht eröffnet");
    stop.send(()).unwrap();
    task.await.unwrap().unwrap();
}

async fn add_users(pool: &PgPool) {
    sqlx::query(
        "insert into users (username, display_name, password_hash) values ('noah', 'Noah Boufercha', 'x'), ('fynn', 'Fynn Becker', 'x')",
    )
    .execute(pool)
    .await
    .unwrap();
}

fn noah() -> Actor {
    Actor::named("noah", "Noah Boufercha")
}

fn fynn() -> Actor {
    Actor::named("fynn", "Fynn Becker")
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_ticket_carries_kind_priority_assignee_and_due_date(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    add_users(&pool).await;
    let due = chrono::NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();

    let task = repo::plan_task(
        &pool,
        &NewMaintenanceTask {
            kind: TaskKind::Inspection,
            priority: TaskPriority::High,
            assignee: Some("FYNN".into()),
            due_date: Some(due),
            ..new_task("TN0001")
        },
        &noah(),
    )
    .await
    .unwrap();

    assert_eq!((task.kind, task.priority), (TaskKind::Inspection, TaskPriority::High));
    assert_eq!(
        (task.assignee.as_deref(), task.assignee_name.as_deref()),
        (Some("fynn"), Some("Fynn Becker")),
        "Schreibweise egal, Anzeigename aus dem Konto"
    );
    assert_eq!(task.due_date, Some(due));
    assert_eq!(task.created_by_name.as_deref(), Some("Noah Boufercha"));
    assert_eq!(task.comment_count, 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_unknown_assignee_is_refused_and_nothing_is_saved(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    let result = repo::plan_task(
        &pool,
        &NewMaintenanceTask {
            assignee: Some("niemand".into()),
            ..new_task("TN0001")
        },
        &noah(),
    )
    .await;
    assert!(matches!(result, Err(RepoError::Invalid(_))), "{result:?}");
    assert!(repo::list_tasks(&pool, None).await.unwrap().is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn assignment_can_be_claimed_changed_and_removed(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    add_users(&pool).await;
    let task = repo::plan_task(&pool, &new_task("TN0001"), &noah()).await.unwrap();
    assert_eq!(task.assignee, None);

    let claimed = repo::assign_task(&pool, task.id, Some("fynn"), &fynn()).await.unwrap();
    assert_eq!(claimed.assignee_name.as_deref(), Some("Fynn Becker"));
    let handed = repo::assign_task(&pool, task.id, Some("noah"), &fynn()).await.unwrap();
    assert_eq!(handed.assignee.as_deref(), Some("noah"));
    let freed = repo::assign_task(&pool, task.id, None, &noah()).await.unwrap();
    assert_eq!((freed.assignee, freed.assignee_name), (None, None));

    assert!(matches!(
        repo::assign_task(&pool, task.id, Some("niemand"), &noah()).await,
        Err(RepoError::Invalid(_))
    ));
    assert!(matches!(
        repo::assign_task(&pool, 999_999, Some("noah"), &noah()).await,
        Err(RepoError::NotFound(_))
    ));
    repo::complete_task(&pool, task.id, &noah()).await.unwrap();
    assert!(
        matches!(
            repo::assign_task(&pool, task.id, Some("noah"), &noah()).await,
            Err(RepoError::Conflict(_))
        ),
        "erledigte Einsätze bleiben, wie sie sind"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn comments_are_kept_in_order_and_counted(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    let task = repo::plan_task(&pool, &new_task("TN0001"), &noah()).await.unwrap();

    repo::add_comment(&pool, task.id, "  Schlüssel liegt beim Hausmeister  ", &noah())
        .await
        .unwrap();
    repo::add_comment(&pool, task.id, "Danke, hole ich ab", &fynn())
        .await
        .unwrap();

    let comments = repo::list_comments(&pool, task.id).await.unwrap();
    assert_eq!(
        comments
            .iter()
            .map(|c| (c.author_name.as_str(), c.body.as_str()))
            .collect::<Vec<_>>(),
        [
            ("Noah Boufercha", "Schlüssel liegt beim Hausmeister"),
            ("Fynn Becker", "Danke, hole ich ab")
        ]
    );
    assert_eq!(repo::list_tasks(&pool, None).await.unwrap()[0].comment_count, 2);

    assert!(matches!(
        repo::add_comment(&pool, task.id, "   ", &noah()).await,
        Err(RepoError::Invalid(_))
    ));
    assert!(matches!(
        repo::add_comment(&pool, task.id, &"x".repeat(2001), &noah()).await,
        Err(RepoError::Invalid(_))
    ));
    assert!(matches!(
        repo::add_comment(&pool, 999_999, "hallo", &noah()).await,
        Err(RepoError::NotFound(_))
    ));
    assert!(matches!(
        repo::list_comments(&pool, 999_999).await,
        Err(RepoError::NotFound(_))
    ));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn only_a_battery_ticket_sends_battery_replaced(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    let task = repo::plan_task(
        &pool,
        &NewMaintenanceTask {
            kind: TaskKind::Inspection,
            ..new_task("TN0001")
        },
        &noah(),
    )
    .await
    .unwrap();
    let mut l = listener(&pool).await;

    let done = repo::complete_task(&pool, task.id, &fynn()).await.unwrap();

    assert_eq!(done.completed_by_name.as_deref(), Some("Fynn Becker"));
    assert!(
        matches!(next_event(&mut l).await, DomainEvent::TaskChanged { task_id, .. } if task_id == task.id),
        "keine Batterie getauscht, nur eine Änderung am Einsatz"
    );
    let more = tokio::time::timeout(Duration::from_millis(300), l.recv()).await;
    assert!(more.is_err(), "kein weiteres Ereignis");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_battery_event_leaves_an_inspection_ticket_alone(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    repo::plan_task(
        &pool,
        &NewMaintenanceTask {
            kind: TaskKind::Repair,
            ..new_task("TN0001")
        },
        &noah(),
    )
    .await
    .unwrap();
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
    assert_eq!(tasks.len(), 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn urgent_tickets_with_early_deadlines_come_first(pool: PgPool) {
    for id in ["A", "B", "C", "D"] {
        add_device(&pool, id, "normal").await;
    }
    let day = |d| chrono::NaiveDate::from_ymd_opt(2026, 11, d);
    let make = |device: &str, priority, due| NewMaintenanceTask {
        priority,
        due_date: due,
        ..new_task(device)
    };
    for new in [
        make("A", TaskPriority::Normal, None),
        make("B", TaskPriority::High, day(30)),
        make("C", TaskPriority::High, day(10)),
        make("D", TaskPriority::Low, day(1)),
    ] {
        repo::plan_task(&pool, &new, &noah()).await.unwrap();
    }
    let order: Vec<String> = repo::list_tasks(&pool, None)
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.device_id)
        .collect();
    assert_eq!(order, ["C", "B", "A", "D"]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn who_did_what_is_logged(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    add_device(&pool, "TN0002", "critical").await;
    add_users(&pool).await;
    handle_event(&pool, &changed("TN0001", DeviceStatus::Normal, DeviceStatus::Critical))
        .await
        .unwrap();
    let alert_id = alerts(&pool, None).await[0].id;
    repo::dismiss_alert(&pool, alert_id, &fynn()).await.unwrap();
    let task = repo::plan_task(&pool, &new_task("TN0001"), &noah()).await.unwrap();
    repo::assign_task(&pool, task.id, Some("noah"), &noah()).await.unwrap();
    repo::add_comment(&pool, task.id, "unterwegs", &noah()).await.unwrap();
    repo::complete_task(&pool, task.id, &noah()).await.unwrap();
    repo::plan_task(&pool, &new_task("TN0002"), &noah()).await.unwrap();

    let all = repo::list_activity(&pool, None, 50).await.unwrap();
    assert_eq!(all.len(), 6);
    assert_eq!(all[0].text, "Noah Boufercha hat Batteriewechsel für TN0002 geplant.");
    let texts: Vec<&str> = all.iter().map(|e| e.text.as_str()).collect();
    assert!(texts.contains(&"Fynn Becker hat die Meldung für TN0001 abgewiesen."));
    assert!(texts.contains(&"Noah Boufercha kümmert sich um TN0001."));
    assert!(texts.contains(&"Noah Boufercha hat bei TN0001 kommentiert."));
    assert!(texts.contains(&"Noah Boufercha hat Batteriewechsel bei TN0001 erledigt."));

    let only_first = repo::list_activity(&pool, Some("TN0001"), 50).await.unwrap();
    assert_eq!(only_first.len(), 5);
    assert_eq!(repo::list_activity(&pool, None, 2).await.unwrap().len(), 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn without_an_actor_the_system_is_named(pool: PgPool) {
    add_device(&pool, "TN0001", "critical").await;
    repo::plan_task(&pool, &new_task("TN0001"), &Actor::default())
        .await
        .unwrap();
    let log = repo::list_activity(&pool, None, 5).await.unwrap();
    assert_eq!(log[0].actor_name, "System");
}

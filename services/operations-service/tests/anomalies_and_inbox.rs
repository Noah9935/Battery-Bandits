//! Neue Meldungsarten (Stillstand, Spannungseinbruch, Ausfall trotz Ladung) und das Postfach.

use operations_service::events::handle_event;
use operations_service::repo::{self, Actor};
use shared::db::MIGRATOR;
use shared::{AlertKind, AlertStatus, DeviceStatus, DomainEvent, NewMaintenanceTask};
use sqlx::PgPool;

async fn device(pool: &PgPool, id: &str, status: &str) {
    sqlx::query("insert into devices (id, status) values ($1, $2::device_status)")
        .bind(id)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
}

async fn users(pool: &PgPool) {
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

fn anomaly(id: &str, kind: AlertKind) -> DomainEvent {
    DomainEvent::AnomalyDetected {
        device_id: id.into(),
        kind,
    }
}

fn status_changed(id: &str, new: DeviceStatus, soc: Option<f32>) -> DomainEvent {
    DomainEvent::DeviceStatusChanged {
        device_id: id.into(),
        old_status: DeviceStatus::Normal,
        new_status: new,
        soc_percent: soc,
    }
}

async fn open(pool: &PgPool) -> Vec<(String, AlertKind)> {
    repo::list_alerts(pool, Some(AlertStatus::Open))
        .await
        .unwrap()
        .into_iter()
        .map(|a| (a.device_id, a.kind))
        .collect()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_anomaly_opens_an_alert_once(pool: PgPool) {
    device(&pool, "TN0001", "normal").await;
    handle_event(&pool, &anomaly("TN0001", AlertKind::Stagnation))
        .await
        .unwrap();
    handle_event(&pool, &anomaly("TN0001", AlertKind::Stagnation))
        .await
        .unwrap();
    assert_eq!(open(&pool).await, [("TN0001".to_owned(), AlertKind::Stagnation)]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_anomaly_does_not_replace_an_existing_alert(pool: PgPool) {
    device(&pool, "TN0001", "critical").await;
    handle_event(&pool, &status_changed("TN0001", DeviceStatus::Critical, Some(10.0)))
        .await
        .unwrap();
    handle_event(&pool, &anomaly("TN0001", AlertKind::Drop)).await.unwrap();
    assert_eq!(open(&pool).await, [("TN0001".to_owned(), AlertKind::Critical)]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_dismissed_anomaly_stays_quiet_for_a_day(pool: PgPool) {
    device(&pool, "TN0001", "normal").await;
    handle_event(&pool, &anomaly("TN0001", AlertKind::Drop)).await.unwrap();
    let id = repo::list_alerts(&pool, None).await.unwrap()[0].id;
    repo::dismiss_alert(&pool, id, &noah()).await.unwrap();

    handle_event(&pool, &anomaly("TN0001", AlertKind::Drop)).await.unwrap();
    assert!(
        open(&pool).await.is_empty(),
        "keine Neuauflage direkt nach dem Abweisen"
    );

    // Andere Art darf sofort melden, und nach einem Tag auch dieselbe wieder.
    handle_event(&pool, &anomaly("TN0001", AlertKind::Stagnation))
        .await
        .unwrap();
    assert_eq!(open(&pool).await.len(), 1);
    sqlx::query("update alerts set closed_at = now() - interval '25 hours' where kind = 'drop'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("update alerts set status = 'dismissed', closed_at = now() where kind = 'stagnation'")
        .execute(&pool)
        .await
        .unwrap();
    handle_event(&pool, &anomaly("TN0001", AlertKind::Drop)).await.unwrap();
    assert_eq!(open(&pool).await, [("TN0001".to_owned(), AlertKind::Drop)]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_anomaly_survives_recovery_but_not_a_battery_change(pool: PgPool) {
    device(&pool, "TN0001", "normal").await;
    handle_event(&pool, &anomaly("TN0001", AlertKind::Stagnation))
        .await
        .unwrap();

    handle_event(&pool, &status_changed("TN0001", DeviceStatus::Normal, Some(90.0)))
        .await
        .unwrap();
    assert_eq!(open(&pool).await.len(), 1, "Normalstatus schließt keine Auffälligkeit");
    assert_eq!(repo::reconcile(&pool).await.unwrap().1, 0, "auch der Abgleich nicht");

    handle_event(
        &pool,
        &DomainEvent::BatteryReplaced {
            device_id: "TN0001".into(),
            at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
            soc_percent: None,
        },
    )
    .await
    .unwrap();
    assert!(open(&pool).await.is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_charged_device_that_went_silent_is_a_fault(pool: PgPool) {
    device(&pool, "FULL", "offline").await;
    device(&pool, "EMPTY", "offline").await;
    device(&pool, "UNKNOWN", "offline").await;
    handle_event(&pool, &status_changed("FULL", DeviceStatus::Offline, Some(88.0)))
        .await
        .unwrap();
    handle_event(&pool, &status_changed("EMPTY", DeviceStatus::Offline, Some(12.0)))
        .await
        .unwrap();
    handle_event(&pool, &status_changed("UNKNOWN", DeviceStatus::Offline, None))
        .await
        .unwrap();
    let mut found = open(&pool).await;
    found.sort();
    assert_eq!(
        found,
        [
            ("EMPTY".to_owned(), AlertKind::Offline),
            ("FULL".to_owned(), AlertKind::Fault),
            ("UNKNOWN".to_owned(), AlertKind::Offline)
        ]
    );

    // Kommt das Gerät zurück, schließt sich auch die Ausfall-Meldung.
    handle_event(&pool, &status_changed("FULL", DeviceStatus::Normal, Some(88.0)))
        .await
        .unwrap();
    assert!(!open(&pool).await.contains(&("FULL".to_owned(), AlertKind::Fault)));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn reconcile_uses_the_known_charge_to_pick_the_kind(pool: PgPool) {
    sqlx::query("insert into devices (id, status, soc_percent) values ('FULL', 'offline', 90)")
        .execute(&pool)
        .await
        .unwrap();
    repo::reconcile(&pool).await.unwrap();
    assert_eq!(open(&pool).await, [("FULL".to_owned(), AlertKind::Fault)]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_new_alert_lands_in_everyones_inbox(pool: PgPool) {
    users(&pool).await;
    device(&pool, "TN0001", "critical").await;
    handle_event(&pool, &status_changed("TN0001", DeviceStatus::Critical, Some(10.0)))
        .await
        .unwrap();

    for (name, actor) in [("noah", noah()), ("fynn", fynn())] {
        let inbox = repo::list_inbox(&pool, actor.username.as_deref(), 50).await.unwrap();
        assert_eq!(inbox.len(), 1, "{name}");
        assert_eq!(inbox[0].text, "TN0001: Ladestand kritisch");
        assert_eq!((inbox[0].kind.as_str(), inbox[0].read), ("alert", false));
        assert_eq!(inbox[0].device_id.as_deref(), Some("TN0001"));
    }
    assert!(repo::list_inbox(&pool, None, 50).await.unwrap().is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn assignment_and_comments_reach_the_right_people_but_never_the_author(pool: PgPool) {
    users(&pool).await;
    device(&pool, "TN0001", "normal").await;
    let task = repo::plan_task(
        &pool,
        &NewMaintenanceTask {
            device_id: "TN0001".into(),
            assignee: Some("fynn".into()),
            ..Default::default()
        },
        &noah(),
    )
    .await
    .unwrap();

    let fynns = repo::list_inbox(&pool, Some("fynn"), 50).await.unwrap();
    assert_eq!(fynns.len(), 1);
    assert_eq!(
        fynns[0].text,
        "Noah Boufercha hat dir einen Einsatz an TN0001 zugewiesen."
    );
    assert_eq!(fynns[0].task_id, Some(task.id));
    assert!(
        repo::list_inbox(&pool, Some("noah"), 50).await.unwrap().is_empty(),
        "wer zuweist, bekommt nichts"
    );

    // Fynn antwortet: Noah hat den Einsatz angelegt und erfährt es, Fynn nicht.
    repo::add_comment(&pool, task.id, "Mache ich morgen früh", &fynn())
        .await
        .unwrap();
    let noahs = repo::list_inbox(&pool, Some("noah"), 50).await.unwrap();
    assert_eq!(noahs.len(), 1);
    assert_eq!(noahs[0].kind, "comment");
    assert!(
        noahs[0]
            .text
            .contains("Fynn Becker hat bei TN0001 kommentiert: Mache ich morgen")
    );
    assert_eq!(repo::list_inbox(&pool, Some("fynn"), 50).await.unwrap().len(), 1);

    // Wer sich selbst zuweist, bekommt nichts.
    repo::assign_task(&pool, task.id, Some("noah"), &noah()).await.unwrap();
    assert_eq!(repo::list_inbox(&pool, Some("noah"), 50).await.unwrap().len(), 1);
    // Wird einer anderen Person zugewiesen, meldet sich bei ihr.
    repo::assign_task(&pool, task.id, Some("fynn"), &noah()).await.unwrap();
    assert_eq!(repo::list_inbox(&pool, Some("fynn"), 50).await.unwrap().len(), 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn read_marks_only_my_entries_and_unread_come_first(pool: PgPool) {
    users(&pool).await;
    for (i, who) in ["noah", "noah", "fynn"].into_iter().enumerate() {
        sqlx::query("insert into inbox_items (username, kind, text, at) values ($1, 'alert', $2, now() + $3 * interval '1 second')")
            .bind(who)
            .bind(format!("Eintrag {i}"))
            .bind(i as i32)
            .execute(&pool)
            .await
            .unwrap();
    }
    let inbox = repo::list_inbox(&pool, Some("noah"), 50).await.unwrap();
    assert_eq!(
        inbox.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(),
        ["Eintrag 1", "Eintrag 0"]
    );

    let first = inbox[0].id;
    assert_eq!(repo::mark_read(&pool, Some("noah"), Some(&[first])).await.unwrap(), 1);
    let inbox = repo::list_inbox(&pool, Some("noah"), 50).await.unwrap();
    assert_eq!(
        inbox.iter().map(|i| (i.text.as_str(), i.read)).collect::<Vec<_>>(),
        [("Eintrag 0", false), ("Eintrag 1", true)],
        "Ungelesenes steht oben"
    );

    // Fremde Einträge lassen sich nicht abhaken.
    let foreign = repo::list_inbox(&pool, Some("fynn"), 50).await.unwrap()[0].id;
    assert_eq!(repo::mark_read(&pool, Some("noah"), Some(&[foreign])).await.unwrap(), 0);
    assert_eq!(
        repo::mark_read(&pool, Some("noah"), None).await.unwrap(),
        1,
        "alle übrigen"
    );
    assert_eq!(repo::mark_read(&pool, Some("noah"), None).await.unwrap(), 0);
    assert_eq!(repo::mark_read(&pool, None, None).await.unwrap(), 0);
    assert!(!repo::list_inbox(&pool, Some("fynn"), 50).await.unwrap()[0].read);
}

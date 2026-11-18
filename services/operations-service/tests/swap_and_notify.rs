//! Schnellerfassung des Batteriewechsels und Weiterleitung per E-Mail, gegen eine echte Datenbank.
//! Den Mailserver spielt ein kleiner SMTP-Server im Test, der mitschreibt, was bei ihm ankommt.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use chrono::Utc;
use operations_service::events::handle_event_with;
use operations_service::http::{router, router_with};
use operations_service::notify::{Notifier, NotifyConfig};
use operations_service::{mail, repo};
use serde_json::{Value, json};
use shared::db::MIGRATOR;
use shared::notify::NotificationRule;
use shared::{Alert, AlertKind, AlertStatus, DeviceStatus, DomainEvent};
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tower::ServiceExt;

const NOAH: [(&str, &str); 2] = [("x-bb-user", "noah"), ("x-bb-name", "Noah")];

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let body = match body {
        Some(v) => {
            request = request.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let response = app.clone().oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn seed(pool: &PgPool) {
    sqlx::query(
        "insert into devices (id, name, status, soc_percent) values \
         ('TN0001', 'Kreuzung Nord', 'critical', 12), ('TN0002', null, 'normal', 90)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into users (username, display_name, password_hash) values ('noah', 'Noah', 'x'), ('fynn', 'Fynn', 'x')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("insert into alerts (device_id, kind) values ('TN0001', 'critical')")
        .execute(pool)
        .await
        .unwrap();
}

async fn open_alert(pool: &PgPool) -> Alert {
    sqlx::query_as("select id, device_id, kind, status, created_at, closed_at from alerts where status = 'open'")
        .fetch_one(pool)
        .await
        .unwrap()
}

// ---------------------------------------------------------------- Batteriewechsel

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_swap_without_a_planned_task_is_recorded_in_one_step(pool: PgPool) {
    seed(&pool).await;
    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen(shared::events::CHANNEL).await.unwrap();
    let app = router(pool.clone());

    let (status, task) = call(
        &app,
        Method::POST,
        "/maintenance/battery-swap",
        Some(json!({"device_id": "TN0001", "soc_percent": 100.0, "note": "  Pole gereinigt  "})),
        &NOAH,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        (task["status"].as_str(), task["kind"].as_str()),
        (Some("completed"), Some("battery"))
    );
    assert_eq!(task["completed_by_name"], "Noah");
    assert_eq!(task["note"], "Pole gereinigt");
    assert!(task["completed_at"].is_string());

    // Die Meldung ist gelöst, das Ereignis trägt den angegebenen Ladestand.
    let alert_status: AlertStatus = sqlx::query_scalar("select status from alerts where device_id = 'TN0001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(alert_status, AlertStatus::Resolved);
    let mut replaced = None;
    for _ in 0..3 {
        let n = tokio::time::timeout(Duration::from_secs(5), listener.recv())
            .await
            .expect("kein Ereignis")
            .unwrap();
        if let Ok(DomainEvent::BatteryReplaced {
            device_id, soc_percent, ..
        }) = DomainEvent::from_json(n.payload())
        {
            replaced = Some((device_id, soc_percent));
            break;
        }
    }
    assert_eq!(replaced, Some(("TN0001".to_owned(), Some(100.0))));

    let (_, activity) = call(&app, Method::GET, "/activity?device=TN0001", None, &[]).await;
    assert_eq!(
        activity[0]["text"],
        "Noah hat die Batterie von TN0001 gewechselt. Notiz: Pole gereinigt"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_swap_completes_the_planned_battery_task_instead_of_adding_one(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let (_, planned) = call(
        &app,
        Method::POST,
        "/maintenance",
        Some(json!({"device_id": "TN0001", "note": "Ersatzbatterie mitnehmen"})),
        &NOAH,
    )
    .await;

    let (status, done) = call(
        &app,
        Method::POST,
        "/maintenance/battery-swap",
        Some(json!({"device_id": "TN0001", "technician": "FYNN", "note": "erledigt"})),
        &NOAH,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(done["id"], planned["id"], "der geplante Einsatz wird abgeschlossen");
    assert_eq!(
        done["completed_by_name"], "Fynn",
        "gewechselt hat die ausgewählte Person"
    );
    assert_eq!(done["note"], "Ersatzbatterie mitnehmen\nerledigt");
    let count: i64 = sqlx::query_scalar("select count(*) from maintenance_tasks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let (_, activity) = call(&app, Method::GET, "/activity?device=TN0001", None, &[]).await;
    assert!(
        activity[0]["text"]
            .as_str()
            .unwrap()
            .starts_with("Fynn hat die Batterie von TN0001 gewechselt.")
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_swap_with_wrong_input_changes_nothing(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let swap = |body: Value| call(&app, Method::POST, "/maintenance/battery-swap", Some(body), &NOAH);

    let (status, body) = swap(json!({"device_id": "GIBTESNICHT"})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "Gerät nicht gefunden");
    let (status, body) = swap(json!({"device_id": "TN0001", "soc_percent": 130})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "Der Ladestand muss zwischen 0 und 100 Prozent liegen");
    let (status, _) = swap(json!({"device_id": "  "})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, body) = swap(json!({"device_id": "TN0001", "technician": "niemand"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "Die zuständige Person gibt es nicht");

    let tasks: i64 = sqlx::query_scalar("select count(*) from maintenance_tasks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tasks, 0);
    assert_eq!(open_alert(&pool).await.device_id, "TN0001", "die Meldung bleibt offen");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_swap_without_a_session_counts_as_done_by_the_system(pool: PgPool) {
    seed(&pool).await;
    let task = repo::swap_battery(
        &pool,
        &shared::BatterySwap {
            device_id: "TN0002".into(),
            ..shared::BatterySwap::default()
        },
        &repo::Actor::default(),
    )
    .await
    .unwrap();
    assert_eq!(task.completed_by_name, None);
    let text: String = sqlx::query_scalar("select text from activity_log order by id desc limit 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(text, "System hat die Batterie von TN0002 gewechselt.");
}

// ---------------------------------------------------------------- Regeln

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_rule_belongs_to_the_person_and_is_validated(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool.clone());

    let (status, rule) = call(&app, Method::GET, "/notifications/rule", None, &NOAH).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (rule["enabled"].as_bool(), rule["only_critical"].as_bool()),
        (Some(false), Some(true))
    );

    let wanted = json!({"enabled": true, "only_critical": false, "email_address": " noah@beispiel.de "});
    let (status, saved) = call(&app, Method::PUT, "/notifications/rule", Some(wanted.clone()), &NOAH).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["email_address"], "noah@beispiel.de", "ohne Leerzeichen am Rand");

    // Ändern überschreibt, statt eine zweite Regel anzulegen.
    let mut changed = wanted.clone();
    changed["only_critical"] = json!(true);
    call(&app, Method::PUT, "/notifications/rule", Some(changed), &NOAH).await;
    let (_, again) = call(&app, Method::GET, "/notifications/rule", None, &NOAH).await;
    assert_eq!(
        (again["only_critical"].as_bool(), again["email_address"].as_str()),
        (Some(true), Some("noah@beispiel.de"))
    );
    let rows: i64 = sqlx::query_scalar("select count(*) from notification_rules")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);

    // Die Regel einer anderen Person bleibt die Vorgabe.
    let fynn = [("x-bb-user", "fynn"), ("x-bb-name", "Fynn")];
    let (_, other) = call(&app, Method::GET, "/notifications/rule", None, &fynn).await;
    assert_eq!(other["enabled"], false);

    let mut broken = wanted.clone();
    broken["email_address"] = json!("kaputt");
    let (status, body) = call(&app, Method::PUT, "/notifications/rule", Some(broken), &NOAH).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "Die E-Mail-Adresse ist nicht gültig.");

    let mut missing = wanted.clone();
    missing["email_address"] = json!("");
    let (status, body) = call(&app, Method::PUT, "/notifications/rule", Some(missing), &NOAH).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "Die E-Mail-Adresse fehlt.");

    let (status, _) = call(&app, Method::GET, "/notifications/rule", None, &[]).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "ohne Anmeldung gibt es keine Regel"
    );
    let (status, _) = call(&app, Method::PUT, "/notifications/rule", Some(wanted), &[]).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = call(&app, Method::POST, "/notifications/test", None, &[]).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn without_the_mail_server_access_nothing_is_ready_and_the_test_says_so(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let (status, mail_status) = call(&app, Method::GET, "/notifications/status", None, &NOAH).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mail_status, json!({"ready": false}));

    repo::save_notification_rule(&pool, "noah", &noahs_rule())
        .await
        .unwrap();
    let (status, result) = call(&app, Method::POST, "/notifications/test", None, &NOAH).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        result,
        json!({"ok": false, "detail": "E-Mail ist auf dem Server nicht eingerichtet."})
    );
}

// ---------------------------------------------------------------- Versand

/// Eine Nachricht, die beim nachgestellten Mailserver ankam: die Empfänger und der Inhalt samt Kopfzeilen.
#[derive(Debug, Clone)]
struct Received {
    to: Vec<String>,
    data: String,
}

type Mailbox = Arc<Mutex<Vec<Received>>>;

/// Startet einen kleinen Mailserver auf einem freien Port. Er nimmt jede Nachricht an, oder lehnt sie mit 554 ab, wenn
/// `reject` gesetzt ist. Zurück kommen seine Adresse (`smtp://127.0.0.1:PORT`) und die Liste der angenommenen Mails.
async fn fake_mail_server(reject: bool) -> (String, Mailbox) {
    let mailbox: Mailbox = Arc::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("smtp://{}", listener.local_addr().unwrap());
    let seen = mailbox.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let seen = seen.clone();
            tokio::spawn(async move {
                let _ = serve_mail(stream, seen, reject).await;
            });
        }
    });
    (address, mailbox)
}

/// Das Mindeste an SMTP, damit ein Client eine Nachricht abliefern kann.
async fn serve_mail(stream: tokio::net::TcpStream, mailbox: Mailbox, reject: bool) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    write.write_all(b"220 test ESMTP\r\n").await?;
    let mut to = Vec::new();
    while let Some(line) = lines.next_line().await? {
        let command = line.to_uppercase();
        if command.starts_with("EHLO") || command.starts_with("HELO") {
            write.write_all(b"250 test\r\n").await?;
        } else if command.starts_with("MAIL FROM") {
            to.clear();
            write.write_all(b"250 ok\r\n").await?;
        } else if command.starts_with("RCPT TO") {
            let address = line.split_once(':').map_or("", |(_, rest)| rest);
            to.push(address.trim().trim_matches(['<', '>']).to_owned());
            write.write_all(b"250 ok\r\n").await?;
        } else if command == "DATA" {
            write.write_all(b"354 go\r\n").await?;
            let mut data = String::new();
            while let Some(row) = lines.next_line().await? {
                if row == "." {
                    break;
                }
                data.push_str(&row);
                data.push('\n');
            }
            if reject {
                write.write_all(b"554 abgelehnt\r\n").await?;
            } else {
                mailbox.lock().unwrap().push(Received {
                    to: std::mem::take(&mut to),
                    data,
                });
                write.write_all(b"250 angenommen\r\n").await?;
            }
        } else if command == "QUIT" {
            write.write_all(b"221 tschuess\r\n").await?;
            return Ok(());
        } else {
            write.write_all(b"250 ok\r\n").await?;
        }
    }
    Ok(())
}

/// Ein Notifier, der über den nachgestellten Mailserver sendet.
fn notifier(smtp: &str) -> Notifier {
    let values = [
        ("SMTP_URL", smtp.to_owned()),
        ("SMTP_FROM", "alarm@bandits.example.org".to_owned()),
        ("PUBLIC_URL", "https://bandits.example.org/".to_owned()),
    ];
    Notifier::new(NotifyConfig::from_lookup(|name| {
        values.iter().find(|(k, _)| *k == name).map(|(_, v)| v.clone())
    }))
}

fn noahs_rule() -> NotificationRule {
    NotificationRule {
        enabled: true,
        only_critical: true,
        email_address: "noah@beispiel.de".into(),
    }
}

async fn log_rows(pool: &PgPool) -> Vec<(String, bool, String)> {
    sqlx::query_as("select username, ok, detail from notification_log order by id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_new_alert_goes_out_by_mail_to_everyone_whose_rule_asks_for_it(pool: PgPool) {
    seed(&pool).await;
    let (smtp, mailbox) = fake_mail_server(false).await;
    let notifier = notifier(&smtp);
    assert!(notifier.status().ready);
    repo::save_notification_rule(&pool, "noah", &noahs_rule())
        .await
        .unwrap();
    // Fynns Regel ist ausgeschaltet: Er bekommt nichts.
    let fynn = NotificationRule {
        enabled: false,
        email_address: "fynn@beispiel.de".into(),
        ..noahs_rule()
    };
    repo::save_notification_rule(&pool, "fynn", &fynn).await.unwrap();

    let sent = notifier.deliver_alert(&pool, &open_alert(&pool).await).await;

    assert_eq!(sent, 1);
    let mails = mailbox.lock().unwrap().clone();
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].to, ["noah@beispiel.de"]);
    assert!(mails[0].data.contains("To: noah@beispiel.de"), "{}", mails[0].data);
    assert!(mails[0].data.contains("TN0001"), "{}", mails[0].data);
    assert_eq!(
        log_rows(&pool).await,
        [("noah".to_owned(), true, "gesendet".to_owned())]
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn hints_are_only_forwarded_when_the_rule_asks_for_everything(pool: PgPool) {
    seed(&pool).await;
    let (smtp, mailbox) = fake_mail_server(false).await;
    let notifier = notifier(&smtp);
    repo::save_notification_rule(&pool, "noah", &noahs_rule())
        .await
        .unwrap();
    let hint = Alert {
        kind: AlertKind::Stagnation,
        ..open_alert(&pool).await
    };

    assert_eq!(notifier.deliver_alert(&pool, &hint).await, 0);
    assert!(mailbox.lock().unwrap().is_empty());

    let everything = NotificationRule {
        only_critical: false,
        ..noahs_rule()
    };
    repo::save_notification_rule(&pool, "noah", &everything).await.unwrap();
    assert_eq!(notifier.deliver_alert(&pool, &hint).await, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_rule_without_a_valid_address_sends_nothing(pool: PgPool) {
    seed(&pool).await;
    let (smtp, mailbox) = fake_mail_server(false).await;
    let notifier = notifier(&smtp);
    let no_address = NotificationRule {
        email_address: String::new(),
        ..noahs_rule()
    };
    repo::save_notification_rule(&pool, "noah", &no_address).await.unwrap();

    assert_eq!(notifier.deliver_alert(&pool, &open_alert(&pool).await).await, 0);
    assert!(mailbox.lock().unwrap().is_empty());
    assert!(log_rows(&pool).await.is_empty(), "ohne Adresse wird nichts versucht");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn nothing_is_sent_or_logged_without_the_mail_server_access(pool: PgPool) {
    seed(&pool).await;
    repo::save_notification_rule(&pool, "noah", &noahs_rule())
        .await
        .unwrap();

    let sent = Notifier::disabled()
        .deliver_alert(&pool, &open_alert(&pool).await)
        .await;

    assert_eq!(sent, 0);
    assert!(log_rows(&pool).await.is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_failing_mail_server_is_logged_without_leaking_credentials(pool: PgPool) {
    seed(&pool).await;
    let (smtp, _) = fake_mail_server(true).await;
    let notifier = notifier(&smtp);
    repo::save_notification_rule(&pool, "noah", &noahs_rule())
        .await
        .unwrap();

    let sent = notifier.deliver_alert(&pool, &open_alert(&pool).await).await;

    assert_eq!(sent, 0);
    assert_eq!(
        log_rows(&pool).await,
        [(
            "noah".to_owned(),
            false,
            "Der Mailserver hat die Nachricht nicht angenommen.".to_owned()
        )]
    );

    // Ein Mailserver, der gar nicht antwortet: ebenfalls ein lesbarer Fehler, ohne Adresse und Passwort.
    let dead = Notifier::new(NotifyConfig::from_lookup(|name| match name {
        "SMTP_URL" => Some("smtp://nutzer:geheim@127.0.0.1:1".to_owned()),
        "SMTP_FROM" => Some("alarm@bandits.example.org".to_owned()),
        _ => None,
    }));
    let error = dead
        .send_mail("noah@beispiel.de", &mail::render_test(None))
        .await
        .unwrap_err();
    assert_eq!(error, "Der Mailserver hat die Nachricht nicht angenommen.");
    assert!(!error.contains("geheim") && !error.contains("127.0.0.1"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_test_mail_goes_to_the_saved_address(pool: PgPool) {
    seed(&pool).await;
    let (smtp, mailbox) = fake_mail_server(false).await;
    let app = router_with(pool.clone(), notifier(&smtp));
    repo::save_notification_rule(&pool, "noah", &noahs_rule())
        .await
        .unwrap();

    let (status, result) = call(&app, Method::POST, "/notifications/test", None, &NOAH).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(result, json!({"ok": true, "detail": "gesendet"}));
    let mails = mailbox.lock().unwrap().clone();
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].to, ["noah@beispiel.de"]);
    let (_, mail_status) = call(&app, Method::GET, "/notifications/status", None, &NOAH).await;
    assert_eq!(mail_status, json!({"ready": true}));
    assert_eq!(log_rows(&pool).await.len(), 1);

    // Ohne gespeicherte Adresse sagt der Test, was fehlt, und sendet nichts.
    let no_address = NotificationRule {
        email_address: String::new(),
        ..noahs_rule()
    };
    repo::save_notification_rule(&pool, "noah", &no_address).await.unwrap();
    let (_, result) = call(&app, Method::POST, "/notifications/test", None, &NOAH).await;
    assert_eq!(
        result,
        json!({"ok": false, "detail": "Zum Testen die E-Mail-Adresse eintragen und speichern."})
    );
    assert_eq!(mailbox.lock().unwrap().len(), 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_status_change_forwards_the_new_alert_in_the_background(pool: PgPool) {
    sqlx::query("insert into devices (id, status, soc_percent) values ('TN0009', 'critical', 8)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("insert into users (username, display_name, password_hash) values ('noah', 'Noah', 'x')")
        .execute(&pool)
        .await
        .unwrap();
    let (smtp, mailbox) = fake_mail_server(false).await;
    let notifier = notifier(&smtp);
    repo::save_notification_rule(&pool, "noah", &noahs_rule())
        .await
        .unwrap();
    let event = DomainEvent::DeviceStatusChanged {
        device_id: "TN0009".into(),
        old_status: DeviceStatus::Warning,
        new_status: DeviceStatus::Critical,
        soc_percent: Some(8.0),
    };

    handle_event_with(&pool, &notifier, &event).await.unwrap();
    // Dieselbe Meldung ein zweites Mal: Es entsteht keine neue, also geht auch nichts mehr raus.
    handle_event_with(&pool, &notifier, &event).await.unwrap();

    let started = Utc::now();
    while log_rows(&pool).await.is_empty() {
        assert!(
            (Utc::now() - started).num_seconds() < 10,
            "die Weiterleitung kam nicht an"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(mailbox.lock().unwrap().len(), 1);
    assert_eq!(log_rows(&pool).await.len(), 1);
}

// ---------------------------------------------------------------- Eintragen bei Nodes

async fn inbox_of(pool: &PgPool, username: &str) -> Vec<String> {
    sqlx::query_scalar("select text from inbox_items where username = $1 and kind = 'alert' order by id")
        .bind(username)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn raise(pool: &PgPool, device: &str) -> Alert {
    let mut tx = pool.begin().await.unwrap();
    let alert = repo::open_alert(&mut tx, device, AlertKind::Critical)
        .await
        .unwrap()
        .expect("neue Meldung");
    tx.commit().await.unwrap();
    alert
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn without_subscribers_everyone_gets_the_alert(pool: PgPool) {
    sqlx::query("insert into devices (id, status) values ('TN0005', 'critical')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into users (username, display_name, password_hash) values ('noah', 'Noah', 'x'), ('fynn', 'Fynn', 'x')",
    )
    .execute(&pool)
    .await
    .unwrap();

    raise(&pool, "TN0005").await;

    assert_eq!(inbox_of(&pool, "noah").await, ["TN0005: Ladestand kritisch"]);
    assert_eq!(inbox_of(&pool, "fynn").await, ["TN0005: Ladestand kritisch"]);
    let mut recipients = repo::alert_recipients(&pool, "TN0005").await.unwrap();
    recipients.sort();
    assert_eq!(recipients, ["fynn", "noah"]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn subscribers_of_a_node_are_the_only_ones_who_get_its_alert(pool: PgPool) {
    seed(&pool).await; // TN0001, TN0002 und die Konten noah und fynn
    sqlx::query("delete from alerts").execute(&pool).await.unwrap();
    let app = router(pool.clone());

    let (status, _) = call(&app, Method::PUT, "/subscriptions/TN0001", None, &NOAH).await;
    assert_eq!(status, StatusCode::OK);
    // Doppeltes Eintragen schadet nicht.
    assert_eq!(
        call(&app, Method::PUT, "/subscriptions/TN0001", None, &NOAH).await.0,
        StatusCode::OK
    );
    let (_, mine) = call(&app, Method::GET, "/subscriptions", None, &NOAH).await;
    assert_eq!(mine, json!(["TN0001"]));
    let fynn = [("x-bb-user", "fynn"), ("x-bb-name", "Fynn")];
    let (_, others) = call(&app, Method::GET, "/subscriptions", None, &fynn).await;
    assert_eq!(others, json!([]), "jede Person hat ihre eigene Liste");

    raise(&pool, "TN0001").await;
    assert_eq!(inbox_of(&pool, "noah").await.len(), 1);
    assert!(
        inbox_of(&pool, "fynn").await.is_empty(),
        "Fynn ist bei TN0001 nicht eingetragen"
    );

    // Ein anderer Node ohne Eingetragene geht weiter an alle.
    raise(&pool, "TN0002").await;
    assert_eq!(inbox_of(&pool, "fynn").await, ["TN0002: Ladestand kritisch"]);

    // Nach dem Austragen bekommen wieder alle die Meldung.
    sqlx::query("update alerts set status = 'resolved', closed_at = now()")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = call(&app, Method::DELETE, "/subscriptions/TN0001", None, &NOAH).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        call(&app, Method::DELETE, "/subscriptions/TN0001", None, &NOAH).await.0,
        StatusCode::OK
    );
    raise(&pool, "TN0001").await;
    assert_eq!(inbox_of(&pool, "fynn").await.len(), 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn subscribing_needs_an_existing_device_and_a_session(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let (status, body) = call(&app, Method::PUT, "/subscriptions/GIBTESNICHT", None, &NOAH).await;
    assert_eq!(
        (status, body["error"].as_str()),
        (StatusCode::NOT_FOUND, Some("Gerät nicht gefunden"))
    );
    assert_eq!(
        call(&app, Method::PUT, "/subscriptions/TN0001", None, &[]).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        call(&app, Method::DELETE, "/subscriptions/TN0001", None, &[]).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (_, none) = call(&app, Method::GET, "/subscriptions", None, &[]).await;
    assert_eq!(none, json!([]));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn forwarding_only_reaches_the_people_who_get_the_alert(pool: PgPool) {
    seed(&pool).await;
    sqlx::query("delete from alerts").execute(&pool).await.unwrap();
    let (smtp, mailbox) = fake_mail_server(false).await;
    let notifier = notifier(&smtp);
    // Beide haben eine eingeschaltete Regel, aber nur Noah ist bei TN0001 eingetragen.
    repo::save_notification_rule(&pool, "noah", &noahs_rule())
        .await
        .unwrap();
    let fynn = NotificationRule {
        email_address: "fynn@beispiel.de".into(),
        ..noahs_rule()
    };
    repo::save_notification_rule(&pool, "fynn", &fynn).await.unwrap();
    repo::subscribe(&pool, Some("noah"), "TN0001").await.unwrap();

    let alert = raise(&pool, "TN0001").await;
    let sent = notifier.deliver_alert(&pool, &alert).await;

    assert_eq!(sent, 1, "nur an Noah");
    let mails = mailbox.lock().unwrap().clone();
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].to, ["noah@beispiel.de"]);
    let users: Vec<String> = log_rows(&pool).await.into_iter().map(|r| r.0).collect();
    assert_eq!(users, ["noah"]);
}

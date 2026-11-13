//! Schnellerfassung des Batteriewechsels und Weiterleitung aufs Handy, gegen eine echte Datenbank.
//! Die Dienste (Telegram, Gateway) spielt ein kleiner Server im Test, der mitschreibt, was bei ihm ankommt.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes, to_bytes};
use axum::extract::State;
use axum::http::{HeaderMap, Method, Request, StatusCode, Uri};
use chrono::Utc;
use operations_service::events::handle_event_with;
use operations_service::http::{router, router_with};
use operations_service::notify::{Notifier, NotifyConfig};
use operations_service::repo;
use serde_json::{Value, json};
use shared::db::MIGRATOR;
use shared::notify::{Channel, NotificationRule};
use shared::{Alert, AlertKind, AlertStatus, DeviceStatus, DomainEvent};
use sqlx::PgPool;
use sqlx::postgres::PgListener;
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

    let wanted = json!({
        "enabled": true, "only_critical": false,
        "whatsapp": false, "whatsapp_number": "",
        "telegram": true, "telegram_chat_id": " 123456789 ",
        "sms": false, "sms_number": "",
        "email": true, "email_address": "noah@beispiel.de"
    });
    let (status, saved) = call(&app, Method::PUT, "/notifications/rule", Some(wanted.clone()), &NOAH).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["telegram_chat_id"], "123456789", "ohne Leerzeichen am Rand");

    // Ändern überschreibt, statt eine zweite Regel anzulegen.
    let mut changed = wanted.clone();
    changed["only_critical"] = json!(true);
    call(&app, Method::PUT, "/notifications/rule", Some(changed), &NOAH).await;
    let (_, again) = call(&app, Method::GET, "/notifications/rule", None, &NOAH).await;
    assert_eq!(
        (again["only_critical"].as_bool(), again["email"].as_bool()),
        (Some(true), Some(true))
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
    broken["sms"] = json!(true);
    let (status, body) = call(&app, Method::PUT, "/notifications/rule", Some(broken), &NOAH).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "SMS: Handynummer mit Ländervorwahl fehlt.");

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
async fn without_credentials_no_channel_is_ready_and_the_test_says_so(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let (status, channels) = call(&app, Method::GET, "/notifications/channels", None, &NOAH).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        channels,
        json!({"whatsapp": false, "telegram": false, "sms": false, "email": false})
    );

    let rule = NotificationRule {
        enabled: true,
        telegram: true,
        telegram_chat_id: "123456789".into(),
        ..NotificationRule::default()
    };
    repo::save_notification_rule(&pool, "noah", &rule).await.unwrap();
    let (status, results) = call(&app, Method::POST, "/notifications/test", None, &NOAH).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        results,
        json!([{"channel": "telegram", "ok": false, "detail": "Telegram ist auf dem Server nicht eingerichtet."}])
    );
}

// ---------------------------------------------------------------- Versand

/// Was beim nachgestellten Dienst ankam: Pfad, Authorization-Kopf und Inhalt.
type Seen = Arc<Mutex<Vec<(String, Option<String>, Value)>>>;

async fn record(State(seen): State<Seen>, uri: Uri, headers: HeaderMap, body: Bytes) -> StatusCode {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let json = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let fail = uri.path().contains("kaputt");
    seen.lock().unwrap().push((uri.path().to_owned(), auth, json));
    if fail {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::OK
    }
}

/// Startet den nachgestellten Dienst auf einem freien Port und gibt seine Adresse zurück.
async fn fake_service() -> (String, Seen) {
    let seen: Seen = Arc::default();
    let app = Router::new().fallback(record).with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (address, seen)
}

fn notifier(base: &str, sms_path: &str) -> Notifier {
    let values = [
        ("TELEGRAM_BOT_TOKEN", "42:geheim".to_owned()),
        ("TELEGRAM_API_URL", base.to_owned()),
        ("SMS_WEBHOOK_URL", format!("{base}{sms_path}")),
        ("NOTIFY_WEBHOOK_TOKEN", "gateway-geheim".to_owned()),
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
        telegram: true,
        telegram_chat_id: "123456789".into(),
        sms: true,
        sms_number: "0170 1234567".into(),
        // Eingeschaltet, aber auf dem Server nicht eingerichtet: wird still übersprungen.
        whatsapp: true,
        whatsapp_number: "+49 170 1234567".into(),
        ..NotificationRule::default()
    }
}

async fn log_rows(pool: &PgPool) -> Vec<(String, String, bool, String)> {
    sqlx::query_as("select username, channel, ok, detail from notification_log order by id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_new_alert_goes_out_over_every_ready_channel_of_the_rule(pool: PgPool) {
    seed(&pool).await;
    let (base, seen) = fake_service().await;
    let notifier = notifier(&base, "/sms");
    let status = notifier.status();
    assert!(status.telegram && status.sms && !status.whatsapp && !status.email);
    repo::save_notification_rule(&pool, "noah", &noahs_rule())
        .await
        .unwrap();
    // Fynns Regel ist ausgeschaltet: Er bekommt nichts.
    let fynn = NotificationRule {
        enabled: false,
        ..noahs_rule()
    };
    repo::save_notification_rule(&pool, "fynn", &fynn).await.unwrap();

    let sent = notifier.deliver_alert(&pool, &open_alert(&pool).await).await;

    assert_eq!(sent, 2);
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    let expected = "Battery Bandits: TN0001 (Kreuzung Nord), Ladestand kritisch. Ladestand 12 %. \
                    https://bandits.example.org/geraete/TN0001";
    let telegram = seen
        .iter()
        .find(|(path, ..)| path == "/bot42:geheim/sendMessage")
        .unwrap();
    assert_eq!(telegram.2["chat_id"], "123456789");
    assert_eq!(telegram.2["text"], expected);
    let sms = seen.iter().find(|(path, ..)| path == "/sms").unwrap();
    assert_eq!(sms.1.as_deref(), Some("Bearer gateway-geheim"));
    assert_eq!(
        sms.2,
        json!({
            "channel": "sms", "to": "491701234567",
            "subject": "Battery Bandits: TN0001, Ladestand kritisch", "text": expected
        })
    );
    assert_eq!(
        log_rows(&pool).await,
        [
            ("noah".to_owned(), "telegram".to_owned(), true, "gesendet".to_owned()),
            ("noah".to_owned(), "sms".to_owned(), true, "gesendet".to_owned()),
        ]
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn hints_are_only_forwarded_when_the_rule_asks_for_everything(pool: PgPool) {
    seed(&pool).await;
    let (base, seen) = fake_service().await;
    let notifier = notifier(&base, "/sms");
    repo::save_notification_rule(&pool, "noah", &noahs_rule())
        .await
        .unwrap();
    let hint = Alert {
        kind: AlertKind::Stagnation,
        ..open_alert(&pool).await
    };

    assert_eq!(notifier.deliver_alert(&pool, &hint).await, 0);
    assert!(seen.lock().unwrap().is_empty());

    let everything = NotificationRule {
        only_critical: false,
        ..noahs_rule()
    };
    repo::save_notification_rule(&pool, "noah", &everything).await.unwrap();
    assert_eq!(notifier.deliver_alert(&pool, &hint).await, 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_failing_service_is_logged_without_leaking_credentials(pool: PgPool) {
    seed(&pool).await;
    let (base, _) = fake_service().await;
    let notifier = notifier(&base, "/kaputt");
    repo::save_notification_rule(&pool, "noah", &noahs_rule())
        .await
        .unwrap();

    let sent = notifier.deliver_alert(&pool, &open_alert(&pool).await).await;

    assert_eq!(sent, 1, "Telegram geht durch, das Gateway lehnt ab");
    let rows = log_rows(&pool).await;
    let sms = rows.iter().find(|r| r.1 == "sms").unwrap();
    assert!(!sms.2);
    assert_eq!(sms.3, "Das Gateway hat mit Status 500 abgelehnt.");
    assert!(rows.iter().all(|r| !r.3.contains("geheim")));

    // Ein Dienst, der gar nicht antwortet: ebenfalls ein lesbarer Fehler ohne Adresse.
    let dead = notifier_at_dead_port();
    let error = dead.send(Channel::Telegram, "123456789", "s", "t").await.unwrap_err();
    assert_eq!(error, "Telegram ist nicht erreichbar.");
}

fn notifier_at_dead_port() -> Notifier {
    Notifier::new(NotifyConfig::from_lookup(|name| match name {
        "TELEGRAM_BOT_TOKEN" => Some("42:geheim".to_owned()),
        "TELEGRAM_API_URL" => Some("http://127.0.0.1:1".to_owned()),
        _ => None,
    }))
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_test_message_reports_each_channel(pool: PgPool) {
    seed(&pool).await;
    let (base, seen) = fake_service().await;
    let app = router_with(pool.clone(), notifier(&base, "/sms"));
    repo::save_notification_rule(&pool, "noah", &noahs_rule())
        .await
        .unwrap();

    let (status, results) = call(&app, Method::POST, "/notifications/test", None, &NOAH).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        results,
        json!([
            {"channel": "whatsapp", "ok": false, "detail": "WhatsApp ist auf dem Server nicht eingerichtet."},
            {"channel": "telegram", "ok": true, "detail": "gesendet"},
            {"channel": "sms", "ok": true, "detail": "gesendet"},
        ])
    );
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert_eq!(
        seen[0].2["text"],
        "Battery Bandits: Probenachricht. Die Weiterleitung funktioniert. https://bandits.example.org"
    );
    let (_, channels) = call(&app, Method::GET, "/notifications/channels", None, &NOAH).await;
    assert_eq!(channels["telegram"], true);
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
    let (base, seen) = fake_service().await;
    let notifier = notifier(&base, "/sms");
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
    while log_rows(&pool).await.len() < 2 {
        assert!(
            (Utc::now() - started).num_seconds() < 10,
            "die Weiterleitung kam nicht an"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(seen.lock().unwrap().len(), 2);
    assert_eq!(log_rows(&pool).await.len(), 2);
}

//! HTTP-Schnittstelle gegen eine echte Datenbank, ohne Netzwerk (Router wird direkt aufgerufen).

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use operations_service::http::router;
use serde_json::{Value, json};
use shared::db::MIGRATOR;
use sqlx::PgPool;
use tower::ServiceExt;

async fn call(app: &Router, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    call_as(app, method, uri, body, &[]).await
}

async fn call_as(
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
    sqlx::query("insert into devices (id, status) values ('TN0001', 'critical'), ('TN0002', 'normal')")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("insert into alerts (device_id, kind) values ('TN0001', 'critical')")
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn alerts_can_be_listed_and_dismissed(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool);

    let (status, body) = call(&app, Method::GET, "/alerts", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["device_id"], "TN0001");
    assert_eq!(body[0]["status"], "open");
    let id = body[0]["id"].as_i64().unwrap();

    let (status, body) = call(&app, Method::POST, &format!("/alerts/{id}/dismiss"), None).await;
    assert_eq!((status, body["status"].as_str()), (StatusCode::OK, Some("dismissed")));

    let (status, body) = call(&app, Method::POST, &format!("/alerts/{id}/dismiss"), None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("nicht mehr offen"));

    let (status, _) = call(&app, Method::POST, "/alerts/424242/dismiss", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, open) = call(&app, Method::GET, "/alerts?status=open", None).await;
    assert_eq!(open.as_array().unwrap().len(), 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn maintenance_plan_and_complete_flow(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool);

    let (status, task) = call(
        &app,
        Method::POST,
        "/maintenance",
        Some(json!({"device_id": "TN0001", "note": "morgen früh"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        (task["status"].as_str(), task["note"].as_str()),
        (Some("planned"), Some("morgen früh"))
    );
    let id = task["id"].as_i64().unwrap();

    let (status, _) = call(&app, Method::POST, "/maintenance", Some(json!({"device_id": "TN0001"}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = call(&app, Method::POST, "/maintenance", Some(json!({"device_id": "NOPE"}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&app, Method::POST, "/maintenance", Some(json!({"kein": "gerät"}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (_, planned) = call(&app, Method::GET, "/maintenance?status=planned", None).await;
    assert_eq!(planned.as_array().unwrap().len(), 1);

    let (status, done) = call(&app, Method::POST, &format!("/maintenance/{id}/complete"), None).await;
    assert_eq!((status, done["status"].as_str()), (StatusCode::OK, Some("completed")));
    let (status, _) = call(&app, Method::POST, &format!("/maintenance/{id}/complete"), None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Der Wechsel hat die offene Meldung gelöst.
    let (_, alerts) = call(&app, Method::GET, "/alerts", None).await;
    assert_eq!(alerts[0]["status"], "resolved");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn bad_filter_is_a_client_error_and_health_works(pool: PgPool) {
    let app = router(pool);
    let (status, _) = call(&app, Method::GET, "/alerts?status=quatsch", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(&app, Method::GET, "/healthz", None).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tickets_comments_and_activity_over_http(pool: PgPool) {
    seed(&pool).await;
    sqlx::query("insert into users (username, display_name, password_hash) values ('joerg', 'Jörg Müller', 'x')")
        .execute(&pool)
        .await
        .unwrap();
    let app = router(pool);
    // Name prozentkodiert, wie ihn der Dashboard-Service schickt.
    let me = [("x-bb-user", "joerg"), ("x-bb-name", "J%C3%B6rg%20M%C3%BCller")];

    let (status, task) = call_as(
        &app,
        Method::POST,
        "/maintenance",
        Some(json!({"device_id": "TN0001", "kind": "inspection", "priority": "high", "assignee": "joerg", "due_date": "2026-11-20"})),
        &me,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    assert_eq!(task["kind"], "inspection");
    assert_eq!(task["priority"], "high");
    assert_eq!(task["assignee_name"], "Jörg Müller");
    assert_eq!(task["due_date"], "2026-11-20");
    assert_eq!(task["created_by_name"], "Jörg Müller");
    let id = task["id"].as_i64().unwrap();

    let (status, body) = call(
        &app,
        Method::POST,
        "/maintenance",
        Some(json!({"device_id": "TN0002", "assignee": "gibtsnicht"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"].as_str().unwrap().contains("zuständige Person"));

    let (status, c) = call_as(
        &app,
        Method::POST,
        &format!("/maintenance/{id}/comments"),
        Some(json!({"body": "Bin am Montag dort"})),
        &me,
    )
    .await;
    assert_eq!(
        (status, c["author_name"].as_str()),
        (StatusCode::CREATED, Some("Jörg Müller"))
    );
    let (_, list) = call(&app, Method::GET, &format!("/maintenance/{id}/comments"), None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (status, _) = call(
        &app,
        Method::POST,
        &format!("/maintenance/{id}/comments"),
        Some(json!({"body": " "})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, freed) = call_as(
        &app,
        Method::POST,
        &format!("/maintenance/{id}/assign"),
        Some(json!({"assignee": null})),
        &me,
    )
    .await;
    assert_eq!((status, freed["assignee"].is_null()), (StatusCode::OK, true));

    let (_, log) = call(&app, Method::GET, "/activity?device=TN0001&limit=10", None).await;
    let texts: Vec<&str> = log
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts.len(), 3);
    assert!(texts.iter().all(|t| t.contains("Jörg Müller")), "{texts:?}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_inbox_belongs_to_the_person_named_in_the_header(pool: PgPool) {
    sqlx::query("insert into devices (id, status) values ('TN0001', 'normal')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into inbox_items (username, kind, text, device_id) values ('noah', 'alert', 'TN0001: Ladestand kritisch', 'TN0001'), ('fynn', 'alert', 'nur Fynn', null)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let app = router(pool);
    let noah = [("x-bb-user", "noah"), ("x-bb-name", "Noah")];

    let (_, list) = call_as(&app, Method::GET, "/inbox", None, &noah).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["read"], false);
    assert_eq!(list[0]["text"], "TN0001: Ladestand kritisch");

    let (_, anonymous) = call(&app, Method::GET, "/inbox", None).await;
    assert_eq!(anonymous.as_array().unwrap().len(), 0, "ohne Name kein Postfach");

    let (status, marked) = call_as(&app, Method::POST, "/inbox/read", Some(json!({})), &noah).await;
    assert_eq!((status, marked["marked"].as_i64()), (StatusCode::OK, Some(1)));
    let (_, list) = call_as(&app, Method::GET, "/inbox", None, &noah).await;
    assert_eq!(list[0]["read"], true);
}

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
    let mut request = Request::builder().method(method).uri(uri);
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

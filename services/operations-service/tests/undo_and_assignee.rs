//! Falscheintragungen zurücknehmen und die automatische Zuständigkeit beim Erledigen, gegen eine echte Datenbank.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use operations_service::http::router;
use serde_json::{Value, json};
use shared::db::MIGRATOR;
use sqlx::PgPool;
use tower::ServiceExt;

const NOAH: [(&str, &str); 2] = [("x-bb-user", "noah"), ("x-bb-name", "Noah")];
const FYNN: [(&str, &str); 2] = [("x-bb-user", "fynn"), ("x-bb-name", "Fynn")];

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
    sqlx::query("insert into devices (id, status, soc_percent) values ('TN0001', 'normal', 50)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into users (username, display_name, password_hash) values ('noah', 'Noah', 'x'), ('fynn', 'Fynn', 'x')",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn whoever_completes_an_unassigned_task_becomes_responsible(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let (_, task) = call(
        &app,
        Method::POST,
        "/maintenance",
        Some(json!({"device_id": "TN0001", "kind": "inspection"})),
        &NOAH,
    )
    .await;
    assert!(task["assignee"].is_null());
    let id = task["id"].as_i64().unwrap();

    let (status, done) = call(&app, Method::POST, &format!("/maintenance/{id}/complete"), None, &FYNN).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (done["assignee"].as_str(), done["assignee_name"].as_str()),
        (Some("fynn"), Some("Fynn"))
    );

    // Ein schon zugewiesener Einsatz behält seine Zuständigkeit.
    let (_, task) = call(
        &app,
        Method::POST,
        "/maintenance",
        Some(json!({"device_id": "TN0001", "kind": "inspection", "assignee": "noah"})),
        &NOAH,
    )
    .await;
    let id = task["id"].as_i64().unwrap();
    let (_, done) = call(&app, Method::POST, &format!("/maintenance/{id}/complete"), None, &FYNN).await;
    assert_eq!(done["assignee"], "noah");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_swap_made_by_mistake_can_be_taken_back(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let (status, task) = call(
        &app,
        Method::POST,
        "/maintenance/battery-swap",
        Some(json!({"device_id": "TN0001"})),
        &NOAH,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    // Ohne vorherigen Plan ist der Wechsel der Person zugeordnet, die ihn eingetragen hat.
    assert_eq!(task["assignee"], "noah");
    let id = task["id"].as_i64().unwrap();

    // Die Überwachung hat beim Wechsel den Zyklus der alten Batterie festgehalten und die neue eingetragen.
    sqlx::query(
        "insert into battery_cycles (device_id, started_at, ended_at, discharged_ah, price_cents, model) \
         values ('TN0001', now() - interval '10 days', now(), 3.2, 8000, 'Alt')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "update devices set battery_installed_at = now(), discharged_ah = 0, battery_price_cents = null, \
         battery_model = 'Neu' where id = 'TN0001'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (status, _) = call(&app, Method::POST, &format!("/maintenance/{id}/undo"), None, &NOAH).await;
    assert_eq!(status, StatusCode::OK);

    // Der Einsatz gab es vorher nicht, also ist er weg, und das Gerät hat wieder die alte Batterie.
    let tasks: i64 = sqlx::query_scalar("select count(*) from maintenance_tasks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tasks, 0);
    let (model, price, ah): (Option<String>, Option<i32>, f64) =
        sqlx::query_as("select battery_model, battery_price_cents, discharged_ah from devices where id = 'TN0001'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((model.as_deref(), price, ah), (Some("Alt"), Some(8000), 3.2));
    let cycles: i64 = sqlx::query_scalar("select count(*) from battery_cycles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cycles, 0);
    let (_, activity) = call(&app, Method::GET, "/activity?device=TN0001", None, &[]).await;
    assert_eq!(
        activity[0]["text"],
        "Noah hat das Erledigen von Batteriewechsel bei TN0001 rückgängig gemacht."
    );

    // Ein nicht erledigter Einsatz lässt sich nicht zurücknehmen.
    let (_, open) = call(
        &app,
        Method::POST,
        "/maintenance",
        Some(json!({"device_id": "TN0001", "kind": "inspection"})),
        &NOAH,
    )
    .await;
    let open_id = open["id"].as_i64().unwrap();
    let (status, _) = call(&app, Method::POST, &format!("/maintenance/{open_id}/undo"), None, &NOAH).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_planned_task_becomes_open_again_and_only_the_last_swap_can_be_taken_back(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let (_, task) = call(
        &app,
        Method::POST,
        "/maintenance",
        Some(json!({"device_id": "TN0001", "kind": "battery"})),
        &NOAH,
    )
    .await;
    let first = task["id"].as_i64().unwrap();
    // Zeit zwischen Planen und Erledigen, damit der Einsatz als vorher geplant erkannt wird.
    sqlx::query("update maintenance_tasks set created_at = now() - interval '1 hour'")
        .execute(&pool)
        .await
        .unwrap();
    call(
        &app,
        Method::POST,
        &format!("/maintenance/{first}/complete"),
        None,
        &NOAH,
    )
    .await;
    let (_, second) = call(
        &app,
        Method::POST,
        "/maintenance/battery-swap",
        Some(json!({"device_id": "TN0001"})),
        &FYNN,
    )
    .await;
    let second = second["id"].as_i64().unwrap();
    // Zwei Wechsel hintereinander: Der erste wurde vom zweiten abgelöst.
    sqlx::query(
        "update maintenance_tasks set created_at = now() - interval '2 hours', completed_at = now() - interval '1 hour' \
         where id = $1",
    )
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = call(&app, Method::POST, &format!("/maintenance/{first}/undo"), None, &NOAH).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // Den letzten kann man zurücknehmen.
    let (status, _) = call(&app, Method::POST, &format!("/maintenance/{second}/undo"), None, &FYNN).await;
    assert_eq!(status, StatusCode::OK);
    let (status, reopened) = call(&app, Method::POST, &format!("/maintenance/{first}/undo"), None, &NOAH).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reopened["status"], "planned");
    assert!(reopened["completed_at"].is_null());
}

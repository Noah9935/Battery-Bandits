//! Geräte ausmustern und wieder aufnehmen, gegen eine echte Datenbank.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use operations_service::http::router;
use serde_json::{Value, json};
use shared::db::MIGRATOR;
use sqlx::PgPool;
use tower::ServiceExt;

const NOAH: [(&str, &str); 2] = [("x-bb-user", "noah"), ("x-bb-name", "Noah")];

async fn call(app: &Router, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    for (name, value) in NOAH {
        request = request.header(name, value);
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
        "insert into devices (id, status, soc_percent) values ('TN0001', 'critical', 5), ('TN0002', 'normal', 90)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("insert into alerts (device_id, kind) values ('TN0001', 'critical')")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("insert into maintenance_tasks (device_id) values ('TN0001')")
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn retiring_a_device_closes_its_open_alerts_and_planned_tasks(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let (status, _) = call(
        &app,
        Method::POST,
        "/devices/TN0001/retire",
        Some(json!({"reason": "replaced", "note": "  durch TN0420 ersetzt "})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (retired, reason, note): (bool, Option<String>, Option<String>) =
        sqlx::query_as("select retired_at is not null, retired_reason, retired_note from devices where id = 'TN0001'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (retired, reason.as_deref(), note.as_deref()),
        (true, Some("replaced"), Some("durch TN0420 ersetzt"))
    );
    let open: i64 = sqlx::query_scalar("select count(*) from alerts where status = 'open'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(open, 0, "offene Meldungen sind gelöst");
    let planned: i64 = sqlx::query_scalar("select count(*) from maintenance_tasks where status = 'planned'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(planned, 0, "geplante Einsätze entfallen");
    let other: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("select retired_at from devices where id = 'TN0002'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(other.is_none(), "andere Geräte bleiben aktiv");
    let (_, log) = call(&app, Method::GET, "/activity", None).await;
    assert!(log.to_string().contains("ausgemustert"), "{log}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn retiring_twice_or_an_unknown_device_is_refused(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool);
    let body = || Some(json!({"reason": "done"}));
    assert_eq!(
        call(&app, Method::POST, "/devices/TN0001/retire", body()).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, Method::POST, "/devices/TN0001/retire", body()).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(&app, Method::POST, "/devices/TN9999/retire", body()).await.0,
        StatusCode::NOT_FOUND
    );
    let too_long = Some(json!({"reason": "done", "note": "x".repeat(501)}));
    assert_eq!(
        call(&app, Method::POST, "/devices/TN0002/retire", too_long).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let bad_reason = Some(json!({"reason": "kaputt"}));
    assert!(
        call(&app, Method::POST, "/devices/TN0002/retire", bad_reason)
            .await
            .0
            .is_client_error()
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_retired_device_can_be_taken_back(pool: PgPool) {
    seed(&pool).await;
    let app = router(pool.clone());
    assert_eq!(
        call(&app, Method::DELETE, "/devices/TN0001/retire", None).await.0,
        StatusCode::CONFLICT,
        "ein aktives Gerät lässt sich nicht noch einmal aufnehmen"
    );
    call(
        &app,
        Method::POST,
        "/devices/TN0001/retire",
        Some(json!({"reason": "discarded"})),
    )
    .await;
    assert_eq!(
        call(&app, Method::DELETE, "/devices/TN0001/retire", None).await.0,
        StatusCode::OK
    );
    let (retired, reason): (bool, Option<String>) =
        sqlx::query_as("select retired_at is not null, retired_reason from devices where id = 'TN0001'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((retired, reason), (false, None));
    assert_eq!(
        call(&app, Method::DELETE, "/devices/TN9999/retire", None).await.0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_price_can_be_added_afterwards_for_the_running_and_for_finished_batteries(pool: PgPool) {
    seed(&pool).await;
    sqlx::query(
        "insert into battery_cycles (device_id, started_at, ended_at, discharged_ah) \
         values ('TN0001', now() - interval '10 days', now() - interval '1 day', 12)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let cycle_id: i64 = sqlx::query_scalar("select id from battery_cycles")
        .fetch_one(&pool)
        .await
        .unwrap();
    let app = router(pool.clone());

    let running = Some(json!({"price_cents": 8950}));
    assert_eq!(
        call(&app, Method::PUT, "/devices/TN0001/battery-price", running)
            .await
            .0,
        StatusCode::OK
    );
    let price: Option<i32> = sqlx::query_scalar("select battery_price_cents from devices where id = 'TN0001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(price, Some(8950));

    let past = Some(json!({"price_cents": 10_000}));
    assert_eq!(
        call(&app, Method::PUT, &format!("/battery-cycles/{cycle_id}/price"), past)
            .await
            .0,
        StatusCode::OK
    );
    let price: Option<i32> = sqlx::query_scalar("select price_cents from battery_cycles where id = $1")
        .bind(cycle_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(price, Some(10_000));

    // Leer entfernt den Preis, Unsinn und Unbekanntes wird abgelehnt.
    assert_eq!(
        call(&app, Method::PUT, "/devices/TN0001/battery-price", Some(json!({})))
            .await
            .0,
        StatusCode::OK
    );
    let price: Option<i32> = sqlx::query_scalar("select battery_price_cents from devices where id = 'TN0001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(price, None);
    for bad in [-1, 1_000_001] {
        assert_eq!(
            call(
                &app,
                Method::PUT,
                "/devices/TN0001/battery-price",
                Some(json!({"price_cents": bad}))
            )
            .await
            .0,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{bad}"
        );
    }
    assert_eq!(
        call(
            &app,
            Method::PUT,
            "/devices/TN9999/battery-price",
            Some(json!({"price_cents": 1}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            &app,
            Method::PUT,
            "/battery-cycles/9999/price",
            Some(json!({"price_cents": 1}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (_, log) = call(&app, Method::GET, "/activity", None).await;
    assert!(log.to_string().contains("nachgetragen"), "{log}");
}

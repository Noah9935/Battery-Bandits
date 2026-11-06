//! Weiterleitung an einen (nachgebauten) Meldungs-Service und Echtzeit-Ereignisse über echte TCP-Verbindungen.

use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::Request;
use axum::http::{Method, StatusCode, Uri};
use axum::routing::any;
use dashboard_service::{AppState, api::router, sse};
use serde_json::{Value, json};
use shared::db::{MIGRATOR, notify_event};
use shared::{DeviceStatus, DomainEvent};
use sqlx::PgPool;

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

/// Nachgebauter Meldungs- und Einsatz-Service: gibt zurück, was er bekommen hat.
async fn fake_operations() -> String {
    serve(
        Router::new().fallback(any(|method: Method, uri: Uri, req: Request| async move {
            let content_type = req
                .headers()
                .get("content-type")
                .map(|v| v.to_str().unwrap().to_owned());
            let body: Bytes = axum::body::to_bytes(req.into_body(), 1 << 20).await.unwrap();
            if uri.path() == "/maintenance/99/complete" {
                return (
                    StatusCode::CONFLICT,
                    axum::Json(json!({"error": "Der Einsatz ist bereits erledigt"})),
                );
            }
            (
                StatusCode::OK,
                axum::Json(json!({
                    "method": method.as_str(),
                    "path": uri.path_and_query().unwrap().as_str(),
                    "content_type": content_type,
                    "body": String::from_utf8_lossy(&body),
                })),
            )
        })),
    )
    .await
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn commands_are_forwarded_unchanged(pool: PgPool) {
    let ops = fake_operations().await;
    let dashboard = serve(router(AppState::new(pool, format!("{ops}/")))).await;
    let client = reqwest::Client::new();

    let reply: Value = client
        .get(format!("{dashboard}/api/alerts?status=open"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        (reply["method"].as_str(), reply["path"].as_str()),
        (Some("GET"), Some("/alerts?status=open"))
    );

    let reply: Value = client
        .post(format!("{dashboard}/api/alerts/7/dismiss"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        (reply["method"].as_str(), reply["path"].as_str()),
        (Some("POST"), Some("/alerts/7/dismiss"))
    );

    let response = client
        .post(format!("{dashboard}/api/maintenance"))
        .json(&json!({"device_id": "TN0001"}))
        .send()
        .await
        .unwrap();
    let reply: Value = response.json().await.unwrap();
    assert_eq!(reply["path"], "/maintenance");
    assert_eq!(reply["content_type"], "application/json");
    assert_eq!(reply["body"], "{\"device_id\":\"TN0001\"}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn upstream_errors_pass_through_with_their_status(pool: PgPool) {
    let ops = fake_operations().await;
    let dashboard = serve(router(AppState::new(pool, ops))).await;

    let response = reqwest::Client::new()
        .post(format!("{dashboard}/api/maintenance/99/complete"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
    assert!(
        response.json::<Value>().await.unwrap()["error"]
            .as_str()
            .unwrap()
            .contains("bereits erledigt")
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn unreachable_upstream_gives_bad_gateway(pool: PgPool) {
    let dashboard = serve(router(AppState::new(pool, "http://127.0.0.1:1"))).await;

    let response = reqwest::get(format!("{dashboard}/api/alerts")).await.unwrap();

    assert_eq!(response.status(), 502);
    assert!(
        response.json::<Value>().await.unwrap()["error"]
            .as_str()
            .unwrap()
            .contains("nicht erreichbar")
    );
}

async fn next_data_line(response: &mut reqwest::Response) -> String {
    let mut buffer = String::new();
    loop {
        let chunk = tokio::time::timeout(Duration::from_secs(5), response.chunk())
            .await
            .expect("kein Ereignis innerhalb von 5 Sekunden")
            .unwrap()
            .expect("Verbindung beendet");
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        if let Some(line) = buffer.lines().find(|l| l.starts_with("data:")) {
            return line.trim_start_matches("data:").trim().to_owned();
        }
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn database_events_reach_the_browser_as_server_sent_events(pool: PgPool) {
    let state = AppState::new(pool.clone(), "http://127.0.0.1:1");
    sse::spawn_pump(pool.clone(), state.bus.clone());
    let dashboard = serve(router(state)).await;
    tokio::time::sleep(Duration::from_millis(400)).await; // Pumpe hört zu

    let mut response = reqwest::get(format!("{dashboard}/api/events")).await.unwrap();
    assert_eq!(response.headers()["content-type"], "text/event-stream");

    let event = DomainEvent::DeviceStatusChanged {
        device_id: "TN0001".into(),
        old_status: DeviceStatus::Normal,
        new_status: DeviceStatus::Critical,
        soc_percent: Some(12.0),
    };
    notify_event(&pool, &event).await.unwrap();

    let data = next_data_line(&mut response).await;
    assert_eq!(DomainEvent::from_json(&data).unwrap(), event);
}

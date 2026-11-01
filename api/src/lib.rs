//! API-Server (Dashboard-Service).
//!
//! Der Dashboard-Service ist die Lesesicht auf alle anderen Dienste (Event
//! Storming 13 und 14): Die API liest die Tabellen der anderen Dienste nur
//! lesend und schickt Änderungen als Befehle über NATS an den jeweils
//! zuständigen Dienst. Schreiben darf sie selbst nichts.

pub mod error;
pub mod handlers;
pub mod live;
pub mod queries;
pub mod state;

use axum::Router;
use axum::routing::{get, post};
use state::AppState;
use tower_http::trace::TraceLayer;

/// Tabellen der anderen Dienste, die die API liest. Sie startet erst, wenn alle da sind.
pub const REQUIRED_TABLES: &[&str] = &[
    "ingest.device",
    "ingest.reading",
    "ingest.reading_hourly",
    "ingest.rejected_message",
    "monitor.setting",
    "monitor.device_state",
    "incidents.incident",
];

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(handlers::health))
        .route("/api/devices", get(handlers::devices))
        .route("/api/devices/{id}", get(handlers::device))
        .route("/api/devices/{id}/history", get(handlers::history))
        .route("/api/settings", get(handlers::get_settings).put(handlers::put_settings))
        .route("/api/incidents", get(handlers::incidents))
        .route("/api/incidents/{id}/dismiss", post(handlers::dismiss_incident))
        .route("/api/rejected", get(handlers::rejected))
        .route("/api/events", get(handlers::events))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

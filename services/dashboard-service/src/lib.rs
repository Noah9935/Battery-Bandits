//! Dashboard-Service (Backend-for-Frontend, Lese-Modelle nach CQRS).
//!
//! * [`queries`]: Lese-Modelle (Übersicht, Geräteliste, Verlauf, Einstellungen) und Sortier-Regeln.
//! * [`csv`]: CSV-Export (rein).
//! * [`proxy`]: Weiterleitung der Kommandos an den Meldungs- und Einsatz-Service.
//! * [`sse`]: Echtzeit-Push an den Browser aus `LISTEN domain_events`.
//! * [`api`]: Router und Fehlerbehandlung.

pub mod api;
pub mod csv;
pub mod proxy;
pub mod queries;
pub mod sse;

use sqlx::PgPool;

/// Gemeinsamer Zustand aller Handler.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Basisadresse des Meldungs- und Einsatz-Service, ohne abschließenden Schrägstrich.
    pub operations_url: String,
    pub http: reqwest::Client,
    pub bus: sse::EventBus,
}

impl AppState {
    pub fn new(pool: PgPool, operations_url: impl Into<String>) -> Self {
        Self {
            pool,
            operations_url: operations_url.into().trim_end_matches('/').to_owned(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("HTTP-Client ohne TLS ist immer baubar"),
            bus: sse::new_bus(),
        }
    }
}

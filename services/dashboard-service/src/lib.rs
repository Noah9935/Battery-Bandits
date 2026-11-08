//! Dashboard-Service (Backend-for-Frontend, Lese-Modelle nach CQRS).
//!
//! * [`queries`]: Lese-Modelle (Übersicht, Geräteliste, Verlauf, Einstellungen) und Sortier-Regeln.
//! * [`csv`]: CSV-Export (rein).
//! * [`proxy`]: Weiterleitung der Kommandos an den Meldungs- und Einsatz-Service.
//! * [`sse`]: Echtzeit-Push an den Browser aus `LISTEN domain_events`.
//! * [`api`]: Router und Fehlerbehandlung.

pub mod api;
pub mod auth;
pub mod csv;
pub mod proxy;
pub mod queries;
pub mod sse;

use std::sync::Arc;

use sqlx::PgPool;

/// Gemeinsamer Zustand aller Handler.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Basisadresse des Meldungs- und Einsatz-Service, ohne abschließenden Schrägstrich.
    pub operations_url: String,
    pub http: reqwest::Client,
    pub bus: sse::EventBus,
    /// Ohne Anmeldung arbeiten alle Routen als "Gast" (nur für Entwicklung und Tests).
    pub auth_required: bool,
    /// Sitzungs-Cookie mit `Secure` (hinter HTTPS).
    pub cookie_secure: bool,
    pub limiter: Arc<auth::LoginLimiter>,
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
            auth_required: false,
            cookie_secure: false,
            limiter: Arc::new(auth::LoginLimiter::default()),
        }
    }

    pub fn with_auth(mut self, required: bool) -> Self {
        self.auth_required = required;
        self
    }

    pub fn with_cookie_secure(mut self, secure: bool) -> Self {
        self.cookie_secure = secure;
        self
    }
}

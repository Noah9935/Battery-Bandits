use kit::bus::Bus;
use shared::events::LiveEvent;
use sqlx::PgPool;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub bus: Bus,
    /// Verteilt Ereignisse an alle offenen Browser-Verbindungen
    pub live: broadcast::Sender<LiveEvent>,
}

impl AppState {
    pub fn new(pool: PgPool, bus: Bus) -> Self {
        let (live, _) = broadcast::channel(512);
        Self { pool, bus, live }
    }
}

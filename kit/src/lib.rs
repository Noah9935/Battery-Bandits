//! Gemeinsame Infrastruktur der Backend-Dienste.
//!
//! Hier liegt alles, was jeder Dienst braucht und was nichts mit Fachlogik zu
//! tun hat. Die Fachlogik steckt in `shared` (rein) und in den Diensten selbst.

pub mod bus;
pub mod db;
pub mod env;
pub mod retry;
pub mod shutdown;
pub mod time;

/// Richtet die Protokollierung ein. Die Ausführlichkeit steuert `RUST_LOG`.
pub fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

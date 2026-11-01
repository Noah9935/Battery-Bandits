use anyhow::{Context, Result};
use api::state::AppState;
use kit::bus::Bus;
use kit::env;

#[tokio::main]
async fn main() -> Result<()> {
    kit::init_tracing();
    let database_url = env::required("DATABASE_URL")?;
    let nats_url = env::or("NATS_URL", "nats://nats:4222");
    let bind = env::or("API_BIND", "0.0.0.0:8080");

    let pool = kit::db::connect_shared(&database_url).await?;
    // Die Tabellen gehören den anderen Diensten. Warten, bis deren Migrationen durch sind.
    kit::db::wait_for_tables(&pool, api::REQUIRED_TABLES).await?;
    let bus = Bus::connect(&nats_url).await?;

    let state = AppState::new(pool, bus);
    tokio::spawn({
        let state = state.clone();
        async move {
            if let Err(e) = api::live::relay(state).await {
                tracing::error!("Live-Verteilung beendet: {e:#}");
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("Port {bind} nicht verfügbar"))?;
    tracing::info!("API lauscht auf {bind}");
    axum::serve(listener, api::router(state))
        .with_graceful_shutdown(kit::shutdown::signal())
        .await?;
    Ok(())
}

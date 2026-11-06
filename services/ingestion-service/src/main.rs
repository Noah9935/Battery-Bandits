use anyhow::{Context, Result};
use ingestion_service::config::Config;
use ingestion_service::observer::Dispatcher;
use ingestion_service::{mqtt, store::PostgresObserver};
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let config = Config::from_env()?;
    let pool = shared::db::connect_with_retry(&config.database_url)
        .await
        .context("Datenbank nicht erreichbar")?;

    let mut dispatcher = Dispatcher::new();
    dispatcher.subscribe(Arc::new(PostgresObserver::new(pool)));

    tracing::info!(client_id = %config.mqtt_client_id, "Erfassungs-Service gestartet");
    mqtt::run(&config, dispatcher, async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Beende auf Anforderung");
    })
    .await;
    Ok(())
}

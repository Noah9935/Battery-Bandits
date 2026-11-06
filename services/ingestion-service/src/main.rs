use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use ingestion_service::config::Config;
use ingestion_service::observer::Dispatcher;
use ingestion_service::{mqtt, store::PostgresObserver};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let config = Config::from_env()?;
    let pool = connect_with_retry(&config.database_url).await?;

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

/// Die Datenbank kann beim Start noch hochfahren: bis zu eine Minute lang erneut versuchen.
async fn connect_with_retry(url: &str) -> Result<sqlx::PgPool> {
    let mut last = None;
    for attempt in 1..=30 {
        match shared::db::connect_and_migrate(url).await {
            Ok(pool) => return Ok(pool),
            Err(e) => {
                tracing::warn!(attempt, "Datenbank noch nicht bereit: {e}");
                last = Some(e);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
    Err(last.expect("mindestens ein Versuch")).context("Datenbank nicht erreichbar")
}

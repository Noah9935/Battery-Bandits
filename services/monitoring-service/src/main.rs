use std::time::Duration;

use anyhow::{Context, Result};
use monitoring_service::runner::{RunnerConfig, run};
use tracing_subscriber::EnvFilter;

fn seconds(key: &str, default: u64) -> Result<Duration> {
    match std::env::var(key).ok().filter(|v| !v.trim().is_empty()) {
        Some(v) => Ok(Duration::from_secs(
            v.parse().with_context(|| format!("{key} ist keine Zahl: {v}"))?,
        )),
        None => Ok(Duration::from_secs(default)),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL fehlt")?;
    let pool = shared::db::connect_with_retry(&database_url)
        .await
        .context("Datenbank nicht erreichbar")?;
    let config = RunnerConfig {
        sweep_interval: seconds("SWEEP_INTERVAL_SECS", 15)?,
        forecast_interval: seconds("FORECAST_INTERVAL_SECS", 60)?,
    };
    // Deckel für die Datenbank: Vorgabe 10 GB (DB_QUOTA_MB), alle zehn Minuten geprüft.
    let quota = shared::efficiency::quota_bytes_from(std::env::var("DB_QUOTA_MB").ok().as_deref());
    tokio::spawn(monitoring_service::retention::run(
        pool.clone(),
        quota,
        seconds("RETENTION_INTERVAL_SECS", 600)?,
    ));
    run(pool, config, async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Beende auf Anforderung");
    })
    .await
}

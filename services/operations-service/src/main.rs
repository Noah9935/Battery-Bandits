use anyhow::{Context, Result};
use operations_service::notify::{Notifier, NotifyConfig};
use operations_service::{events, http};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL fehlt")?;
    let addr = std::env::var("OPERATIONS_ADDR").unwrap_or_else(|_| "0.0.0.0:8082".to_owned());
    let pool = shared::db::connect_with_retry(&database_url)
        .await
        .context("Datenbank nicht erreichbar")?;

    // Ereignisse verarbeiten (Meldungen eröffnen und lösen) und gleichzeitig die REST-Schnittstelle bedienen.
    let notifier = Notifier::new(NotifyConfig::from_env());
    tracing::info!(dienste = ?notifier.status(), "Weiterleitung aufs Handy");
    let listener_pool = pool.clone();
    let listener_notifier = notifier.clone();
    let listener =
        tokio::spawn(async move { events::run_with(listener_pool, listener_notifier, std::future::pending()).await });

    // Erfundene Geräte (Simulator) bekommen Batteriemodell und Preis aus dem Katalog. Sie entstehen erst mit ihrer ersten
    // MQTT-Nachricht, deshalb wiederholt sich das, bis alle versorgt sind. Echte Geräte bleiben unberührt.
    let battery_pool = pool.clone();
    tokio::spawn(async move {
        loop {
            match operations_service::repo::assign_simulated_batteries(&battery_pool).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(n, "Batterien aus dem Katalog bei erfundenen Geräten eingetragen"),
                Err(e) => tracing::warn!(fehler = %e, "Batterien bei erfundenen Geräten nicht eingetragen"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });

    let tcp = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("{addr} nicht verfügbar"))?;
    tracing::info!(%addr, "Meldungs- und Einsatz-Service läuft");
    let server = axum::serve(tcp, http::router_with(pool, notifier)).with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Beende auf Anforderung");
    });

    tokio::select! {
        result = server => result.context("HTTP-Server beendet")?,
        result = listener => result.context("Ereignis-Schleife abgestürzt")??,
    }
    Ok(())
}

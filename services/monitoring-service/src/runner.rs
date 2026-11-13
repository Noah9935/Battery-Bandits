//! Hauptschleife: Ereignisse aus `LISTEN domain_events` plus zwei Zeitgeber.

use std::future::Future;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use shared::DomainEvent;
use shared::events::CHANNEL;
use sqlx::PgPool;
use sqlx::postgres::PgListener;

use crate::{anomaly, service};

#[derive(Debug, Clone, Copy)]
pub struct RunnerConfig {
    /// Abstand der Offline-Erkennung und der Gesamtbewertung (Vorgabe 15 Sekunden).
    pub sweep_interval: Duration,
    /// Abstand der Prognose-Berechnung.
    pub forecast_interval: Duration,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            sweep_interval: Duration::from_secs(15),
            forecast_interval: Duration::from_secs(60),
        }
    }
}

/// Reagiert auf ein einzelnes Ereignis. Fremde Ereignisse werden ignoriert.
pub async fn handle_event(pool: &PgPool, event: &DomainEvent) -> Result<()> {
    let now = Utc::now();
    match event {
        DomainEvent::ReadingSaved { device_id, .. } => {
            service::evaluate_device(pool, device_id, now).await?;
        }
        DomainEvent::BatteryReplaced {
            device_id,
            at,
            soc_percent,
        } => {
            service::on_battery_replaced(pool, device_id, *at, *soc_percent, now).await?;
        }
        DomainEvent::DeviceStatusChanged { .. }
        | DomainEvent::AlertCreated { .. }
        | DomainEvent::AnomalyDetected { .. }
        | DomainEvent::TaskChanged { .. } => {}
    }
    Ok(())
}

/// Läuft, bis `shutdown` fertig wird.
pub async fn run(pool: PgPool, config: RunnerConfig, shutdown: impl Future<Output = ()>) -> Result<()> {
    let mut listener = PgListener::connect_with(&pool).await?;
    listener.listen(CHANNEL).await?;

    // Beim Start nachholen, was während eines Ausfalls verpasst wurde.
    service::evaluate_all(&pool, Utc::now()).await?;
    let _ = service::refresh_forecasts(&pool).await;

    let mut tracker = anomaly::Tracker::default();
    let mut sweep = tokio::time::interval(config.sweep_interval);
    let mut forecast = tokio::time::interval(config.forecast_interval);
    tokio::pin!(shutdown);
    tracing::info!("Überwachungs-Service läuft");
    loop {
        tokio::select! {
            _ = &mut shutdown => return Ok(()),
            notification = listener.recv() => match notification {
                Ok(n) => match DomainEvent::from_json(n.payload()) {
                    Ok(event) => {
                        if let Err(e) = handle_event(&pool, &event).await {
                            tracing::error!(?event, "Ereignis nicht verarbeitet: {e:#}");
                        }
                    }
                    Err(e) => tracing::warn!("Unbekanntes Ereignis ignoriert: {e}"),
                },
                // PgListener verbindet beim nächsten Aufruf selbst neu; die Gesamtbewertung holt Verpasstes nach.
                Err(e) => {
                    tracing::warn!("LISTEN gestört: {e}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            },
            _ = sweep.tick() => {
                if let Err(e) = service::evaluate_all(&pool, Utc::now()).await {
                    tracing::warn!("Gesamtbewertung fehlgeschlagen: {e:#}");
                }
                if let Err(e) = anomaly::sweep(&pool, &mut tracker, Utc::now()).await {
                    tracing::warn!("Auffälligkeiten nicht geprüft: {e:#}");
                }
            }
            _ = forecast.tick() => {
                if let Err(e) = service::refresh_forecasts(&pool).await {
                    tracing::warn!("Prognose fehlgeschlagen: {e:#}");
                }
            }
        }
    }
}

//! Reaktion auf Ereignisse und die Hörer-Schleife.

use std::future::Future;
use std::time::Duration;

use anyhow::Result;
use shared::DomainEvent;
use shared::events::CHANNEL;
use sqlx::PgPool;
use sqlx::postgres::PgListener;

use crate::repo;
use crate::rules::{AlertAction, action_for};

pub async fn handle_event(pool: &PgPool, event: &DomainEvent) -> Result<()> {
    match event {
        DomainEvent::DeviceStatusChanged {
            device_id, new_status, ..
        } => {
            let mut tx = pool.begin().await?;
            match action_for(*new_status) {
                AlertAction::Open(kind) => {
                    repo::open_alert(&mut tx, device_id, kind).await?;
                }
                AlertAction::Resolve => {
                    repo::resolve_alerts(&mut tx, device_id).await?;
                }
            }
            tx.commit().await?;
        }
        DomainEvent::BatteryReplaced { device_id, .. } => {
            let mut tx = pool.begin().await?;
            repo::apply_battery_replaced(&mut tx, device_id).await?;
            tx.commit().await?;
        }
        DomainEvent::ReadingSaved { .. } | DomainEvent::AlertCreated { .. } => {}
    }
    Ok(())
}

/// Hört auf `domain_events`, bis `shutdown` fertig wird.
pub async fn run(pool: PgPool, shutdown: impl Future<Output = ()>) -> Result<()> {
    let mut listener = PgListener::connect_with(&pool).await?;
    listener.listen(CHANNEL).await?;
    match repo::reconcile(&pool).await {
        Ok((opened, resolved)) => tracing::info!(opened, resolved, "Meldungen abgeglichen"),
        Err(e) => tracing::warn!("Abgleich beim Start fehlgeschlagen: {e}"),
    }
    tokio::pin!(shutdown);
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
                Err(e) => {
                    tracing::warn!("LISTEN gestört: {e}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    // Nach einer Unterbrechung könnten Ereignisse fehlen: abgleichen.
                    let _ = repo::reconcile(&pool).await;
                }
            }
        }
    }
}

//! Reaktion auf Ereignisse und die Hörer-Schleife.

use std::future::Future;
use std::time::Duration;

use anyhow::Result;
use shared::events::CHANNEL;
use shared::{AlertKind, DomainEvent};
use sqlx::PgPool;
use sqlx::postgres::PgListener;

use crate::notify::Notifier;
use crate::repo;
use crate::rules::{AlertAction, action_for, offline_kind};

/// Wie [`handle_event_with`], ohne Weiterleitung aufs Handy.
pub async fn handle_event(pool: &PgPool, event: &DomainEvent) -> Result<()> {
    handle_event_with(pool, &Notifier::disabled(), event).await
}

/// Leitet eine frisch eröffnete Meldung im Hintergrund weiter, damit ein langsamer Dienst die
/// Verarbeitung der nächsten Ereignisse nicht aufhält.
fn forward(pool: &PgPool, notifier: &Notifier, alert: Option<shared::Alert>) {
    if let Some(alert) = alert {
        let (pool, notifier) = (pool.clone(), notifier.clone());
        tokio::spawn(async move {
            notifier.deliver_alert(&pool, &alert).await;
        });
    }
}

pub async fn handle_event_with(pool: &PgPool, notifier: &Notifier, event: &DomainEvent) -> Result<()> {
    match event {
        DomainEvent::DeviceStatusChanged {
            device_id,
            new_status,
            soc_percent,
            ..
        } => {
            let settings = shared::db::load_anomaly_settings(pool).await?;
            let mut tx = pool.begin().await?;
            let opened = match action_for(*new_status) {
                AlertAction::Open(kind) => {
                    let kind = if kind == AlertKind::Offline {
                        offline_kind(*soc_percent, &settings)
                    } else {
                        kind
                    };
                    repo::open_alert(&mut tx, device_id, kind).await?
                }
                AlertAction::Resolve => {
                    repo::resolve_recovered(&mut tx, device_id).await?;
                    None
                }
            };
            tx.commit().await?;
            forward(pool, notifier, opened);
        }
        DomainEvent::AnomalyDetected { device_id, kind } => {
            let mut tx = pool.begin().await?;
            let opened = repo::open_anomaly(&mut tx, device_id, *kind).await?;
            tx.commit().await?;
            forward(pool, notifier, opened);
        }
        DomainEvent::BatteryReplaced { device_id, .. } => {
            let mut tx = pool.begin().await?;
            repo::apply_battery_replaced(&mut tx, device_id).await?;
            tx.commit().await?;
        }
        DomainEvent::ReadingSaved { .. } | DomainEvent::AlertCreated { .. } | DomainEvent::TaskChanged { .. } => {}
    }
    Ok(())
}

/// Hört auf `domain_events`, bis `shutdown` fertig wird. Ohne Weiterleitung aufs Handy.
pub async fn run(pool: PgPool, shutdown: impl Future<Output = ()>) -> Result<()> {
    run_with(pool, Notifier::disabled(), shutdown).await
}

/// Wie [`run`], neue Meldungen gehen zusätzlich nach den Regeln der Personen aufs Handy.
/// Der Abgleich beim Start leitet nichts weiter: Nach einem Neustart soll niemand alte Meldungen erneut bekommen.
pub async fn run_with(pool: PgPool, notifier: Notifier, shutdown: impl Future<Output = ()>) -> Result<()> {
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
                        if let Err(e) = handle_event_with(&pool, &notifier, &event).await {
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

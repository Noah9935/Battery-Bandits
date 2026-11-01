use anyhow::Result;
use kit::bus::Bus;
use kit::time::{now_ms, ts_from_ms};
use shared::events::{
    CommandReply, DeviceCritical, DeviceRecovered, DismissIncident, IncidentDismissed, IncidentOpened,
    IncidentResolved, subject,
};
use sqlx::PgPool;

pub struct Incidents {
    pool: PgPool,
    bus: Bus,
}

impl Incidents {
    pub fn new(pool: PgPool, bus: Bus) -> Self {
        Self { pool, bus }
    }

    /// Verarbeitet ein Ereignis aus dem Stream.
    pub async fn on_event(&self, subject: &str, payload: &[u8]) -> Result<()> {
        // Eine unlesbare Nachricht wird nie lesbar. Sie wird verworfen statt endlos wiederholt.
        match subject {
            subject::DEVICE_CRITICAL => match serde_json::from_slice::<DeviceCritical>(payload) {
                Ok(event) => self.open(event).await,
                Err(e) => {
                    tracing::error!("Ungültiges Ereignis auf {subject}: {e}");
                    Ok(())
                }
            },
            subject::DEVICE_RECOVERED => match serde_json::from_slice::<DeviceRecovered>(payload) {
                Ok(event) => self.resolve(event).await,
                Err(e) => {
                    tracing::error!("Ungültiges Ereignis auf {subject}: {e}");
                    Ok(())
                }
            },
            other => {
                tracing::debug!("Ereignis {other} wird nicht verarbeitet");
                Ok(())
            }
        }
    }

    /// Befehl "Meldung eröffnen" als Folge von "Gerät kritisch".
    ///
    /// Gemeldet wird vor dem Abschluss der Transaktion, siehe `monitor::engine`.
    pub async fn open(&self, event: DeviceCritical) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let opened = sqlx::query!(
            r#"insert into incidents.incident (device_id, opened_at, charge_pct)
               values ($1, $2, $3)
               on conflict (device_id) where state = 'open' do nothing
               returning id"#,
            event.device_id,
            ts_from_ms(event.at_ms),
            event.charge_pct,
        )
        .fetch_optional(&mut *tx)
        .await?;

        // Gibt es schon eine offene Meldung, ist nichts zu tun.
        if let Some(row) = opened {
            tracing::info!(id = row.id, device_id = %event.device_id, "Meldung eröffnet");
            let announced = IncidentOpened {
                id: row.id,
                device_id: event.device_id,
                opened_at_ms: event.at_ms,
                charge_pct: event.charge_pct,
            };
            self.bus
                .publish(
                    subject::INCIDENT_OPENED,
                    Some(&format!("incident-opened:{}", row.id)),
                    &announced,
                )
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Das Gerät ist wieder normal: Die offene Meldung ist erledigt.
    pub async fn resolve(&self, event: DeviceRecovered) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let closed = sqlx::query!(
            r#"update incidents.incident
               set state = 'resolved', closed_at = $2
               where device_id = $1 and state = 'open'
               returning id"#,
            event.device_id,
            ts_from_ms(event.at_ms),
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(row) = closed {
            tracing::info!(id = row.id, device_id = %event.device_id, "Meldung erledigt");
            let announced = IncidentResolved {
                id: row.id,
                device_id: event.device_id,
            };
            self.bus
                .publish(
                    subject::INCIDENT_RESOLVED,
                    Some(&format!("incident-resolved:{}", row.id)),
                    &announced,
                )
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Befehl "Meldung abweisen" (Event Storming 9).
    pub async fn dismiss(&self, id: i64) -> Result<Option<IncidentDismissed>> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query!(
            r#"update incidents.incident
               set state = 'dismissed', closed_at = $2
               where id = $1 and state = 'open'
               returning device_id"#,
            id,
            ts_from_ms(now_ms()),
        )
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(None);
        };
        let announced = IncidentDismissed {
            id,
            device_id: row.device_id,
        };
        self.bus
            .publish(
                subject::INCIDENT_DISMISSED,
                Some(&format!("incident-dismissed:{id}")),
                &announced,
            )
            .await?;
        tx.commit().await?;
        tracing::info!(id, "Meldung abgewiesen");
        Ok(Some(announced))
    }

    /// Antwort auf den Befehl aus der API.
    pub async fn handle_dismiss(&self, cmd: DismissIncident) -> CommandReply {
        match self.dismiss(cmd.id).await {
            Ok(Some(_)) => CommandReply::ok(),
            Ok(None) => CommandReply::error("Die Meldung existiert nicht oder ist nicht mehr offen"),
            Err(e) => {
                tracing::error!("Abweisen fehlgeschlagen: {e:#}");
                CommandReply::error("Die Meldung konnte nicht abgewiesen werden")
            }
        }
    }
}

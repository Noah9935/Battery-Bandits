//! Die beiden Beobachter des Empfangsdienstes und die Protokollierung verworfener Nachrichten.

use anyhow::Result;
use async_trait::async_trait;
use kit::bus::Bus;
use kit::time::ts_from_ms;
use shared::Reading;
use shared::events::{ReadingStored, subject};
use shared::validation::Rejection;
use sqlx::PgPool;

use crate::observer::{ReadingObserver, RejectionSink};

/// Beobachter 1: speichert den Messwert (F02).
pub struct ReadingStore {
    pool: PgPool,
}

impl ReadingStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ReadingObserver for ReadingStore {
    fn name(&self) -> &'static str {
        "ReadingStore"
    }

    async fn on_reading(&self, r: &Reading) -> Result<()> {
        let ts = ts_from_ms(r.timestamp_ms);
        let mut tx = self.pool.begin().await?;

        // Das Gerät legt sich beim ersten Messwert selbst an. Ein älterer Messwert
        // (verspätet zugestellt) überschreibt den zuletzt bekannten Stand nicht.
        sqlx::query!(
            r#"insert into ingest.device (id, last_reading_at, last_charge_pct)
               values ($1, $2, $3)
               on conflict (id) do update
                 set last_reading_at = excluded.last_reading_at,
                     last_charge_pct = excluded.last_charge_pct
                 where ingest.device.last_reading_at <= excluded.last_reading_at"#,
            r.device_id,
            ts,
            r.charge_pct,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            r#"insert into ingest.reading (device_id, ts, charge_pct)
               values ($1, $2, $3)
               on conflict (device_id, ts) do nothing"#,
            r.device_id,
            ts,
            r.charge_pct,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }
}

/// Beobachter 2: meldet den gespeicherten Messwert an alle anderen Dienste.
///
/// Das ist der Übergang vom Observer-Muster im Prozess zum Observer-Muster
/// zwischen Diensten: Überwachung und Live-Anzeige sind Abonnenten dieses Ereignisses.
pub struct EventPublisher {
    bus: Bus,
}

impl EventPublisher {
    pub fn new(bus: Bus) -> Self {
        Self { bus }
    }
}

#[async_trait]
impl ReadingObserver for EventPublisher {
    fn name(&self) -> &'static str {
        "EventPublisher"
    }

    async fn on_reading(&self, r: &Reading) -> Result<()> {
        let event = ReadingStored {
            device_id: r.device_id.clone(),
            timestamp_ms: r.timestamp_ms,
            charge_pct: r.charge_pct,
        };
        // Gleiche Kennung bei erneuter Zustellung: NATS verwirft das Duplikat.
        let dedup_id = format!("reading:{}:{}", r.device_id, r.timestamp_ms);
        self.bus.publish(subject::READING_STORED, Some(&dedup_id), &event).await
    }
}

/// Schreibt verworfene Nachrichten in die Datenbank (F06).
pub struct RejectionStore {
    pool: PgPool,
}

impl RejectionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Löscht Einträge, die älter sind als `days` Tage. Gibt die Anzahl zurück.
    pub async fn purge_older_than_days(&self, days: i32) -> Result<u64> {
        let done = sqlx::query!(
            "delete from ingest.rejected_message where received_at < now() - make_interval(days => $1)",
            days
        )
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected())
    }
}

#[async_trait]
impl RejectionSink for RejectionStore {
    async fn rejected(&self, topic: &str, payload: &[u8], reason: &Rejection) -> Result<()> {
        tracing::warn!(%topic, %reason, "Nachricht verworfen");
        let text = String::from_utf8_lossy(payload);
        // Sehr große Nutzlasten werden gekürzt, damit das Protokoll klein bleibt.
        let text: String = text.chars().take(2000).collect();
        let reason = reason.to_string();
        sqlx::query!(
            "insert into ingest.rejected_message (topic, payload, reason) values ($1, $2, $3)",
            topic,
            text,
            reason,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

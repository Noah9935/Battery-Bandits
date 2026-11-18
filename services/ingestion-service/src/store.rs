//! Speichern in PostgreSQL/TimescaleDB und Ereignis `ReadingSaved` per NOTIFY.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use shared::estimator::{EstimatorState, Measurement};
use shared::events::CHANNEL;
use shared::{DataSource, DomainEvent};
use sqlx::PgPool;

use crate::classify::{InvalidMessage, ValidReading};
use crate::observer::ReadingObserver;
use crate::soc_state::{DeviceSocState, effective_time, update_soc};

pub struct PostgresObserver {
    pool: PgPool,
    source: DataSource,
}

impl PostgresObserver {
    /// Herkunft der Daten ist vom Dozenten, solange nichts anderes gesetzt wird.
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            source: DataSource::Dozent,
        }
    }

    /// Markiert neu angelegte Geräte mit der Herkunft (Simulator: erfunden).
    pub fn with_source(mut self, source: DataSource) -> Self {
        self.source = source;
        self
    }
}

#[async_trait]
impl ReadingObserver for PostgresObserver {
    /// Alles in einer Transaktion: Gerät anlegen oder sperren, Ladezustand fortschreiben,
    /// Messwert speichern, Gerät aktualisieren, Ereignis senden (NOTIFY wirkt erst beim Commit).
    async fn on_valid(&self, reading: &ValidReading, received_at: DateTime<Utc>) -> Result<()> {
        let p = &reading.payload;
        let at = effective_time(p.time, received_at);
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "insert into devices (id, battery_installed_at, data_source) values ($1, $2, $3) on conflict (id) do nothing",
        )
        .bind(&reading.node_id)
        .bind(at)
        .bind(self.source)
            .execute(&mut *tx)
            .await?;

        let capacity_ah: f32 = sqlx::query_scalar("select battery_capacity_ah from system_settings")
            .fetch_one(&mut *tx)
            .await?;

        let (last_seen_at, soc_start_percent, discharged_ah, estimator_json): (
            Option<DateTime<Utc>>,
            Option<f32>,
            f64,
            Option<String>,
        ) = sqlx::query_as(
            "select last_seen_at, soc_start_percent, discharged_ah, estimator_state::text \
             from devices where id = $1 for update",
        )
        .bind(&reading.node_id)
        .fetch_one(&mut *tx)
        .await?;

        // Ein unlesbarer Zustand (zum Beispiel nach einer Änderung des Formats) ist kein Grund, Messwerte zu verlieren:
        // Das Filter beginnt dann mit dieser Messung neu aus der Spannung.
        let estimator = estimator_json.as_deref().and_then(|json| {
            serde_json::from_str::<EstimatorState>(json)
                .inspect_err(|e| tracing::warn!(device = %reading.node_id, "Filterzustand unlesbar, Neubeginn: {e}"))
                .ok()
        });
        let prev = DeviceSocState {
            estimator,
            soc_start_percent,
            discharged_ah,
        };
        let measurement = Measurement {
            at,
            voltage_mv: p.voltage_mv,
            current_ma: p.current_ma,
            temperature_c: p.temperature_c,
        };
        let update = update_soc(&prev, &measurement, f64::from(capacity_ah));
        if matches!(
            update.verdict,
            shared::estimator::Verdict::Rejected | shared::estimator::Verdict::Restarted
        ) {
            tracing::info!(
                device = %reading.node_id,
                verdict = ?update.verdict,
                voltage_mv = p.voltage_mv,
                soc = update.estimate.soc_percent,
                "Spannung passt nicht zum erwarteten Ladezustand"
            );
        }
        // Selbstkontrolle des Filters: Weicht die Spannung dauerhaft stärker ab, als das Rauschmodell erwartet, ist die
        // angegebene Spanne schon entsprechend verbreitert. Einmal beim Überschreiten melden, damit man das Gerät prüft
        // (andere Batterie als die Standardkurve, defekter Sensor, Temperatur).
        const FIT_WARNING: f64 = 3.0;
        let was_fitting = prev.estimator.as_ref().is_none_or(|e| e.fit <= FIT_WARNING);
        if was_fitting && update.estimator.fit > FIT_WARNING {
            tracing::warn!(
                device = %reading.node_id,
                fit = update.estimator.fit,
                "Die Schätzung passt schlecht zu den Messwerten, die Fehlerspanne wird breiter angegeben"
            );
        }
        let estimator_json = serde_json::to_string(&update.estimator)?;
        let satellites = p.satellites.map(i16::from);

        sqlx::query(
            "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent, \
             soc_sigma_percent, lat, lon, altitude_m, satellites) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(at)
        .bind(&reading.node_id)
        .bind(p.voltage_mv)
        .bind(p.current_ma)
        .bind(p.temperature_c)
        .bind(update.estimate.soc_percent)
        .bind(update.estimate.sigma_percent)
        .bind(p.lat)
        .bind(p.lon)
        .bind(p.altitude_m)
        .bind(satellites)
        .execute(&mut *tx)
        .await?;

        // Eine verspätete Messung wird gespeichert, ändert aber den aktuellen Gerätestand nicht.
        let is_late = last_seen_at.is_some_and(|last| at < last);
        if !is_late {
            // Bekannte Position bleibt erhalten, wenn die Nachricht keinen GPS-Fix hat.
            sqlx::query(
                "update devices set soc_percent = $2, voltage_mv = $3, current_ma = $4, temperature_c = $5, \
                 lat = coalesce($6, lat), lon = coalesce($7, lon), altitude_m = coalesce($8, altitude_m), \
                 satellites = $9, last_seen_at = $10, soc_start_percent = $11, discharged_ah = $12, \
                 battery_installed_at = coalesce(battery_installed_at, $10), \
                 soc_sigma_percent = $13, estimator_state = $14::jsonb where id = $1",
            )
            .bind(&reading.node_id)
            .bind(update.estimate.soc_percent)
            .bind(p.voltage_mv)
            .bind(p.current_ma)
            .bind(p.temperature_c)
            .bind(p.lat)
            .bind(p.lon)
            .bind(p.altitude_m)
            .bind(satellites)
            .bind(at)
            .bind(update.soc_start_percent)
            .bind(update.discharged_ah)
            .bind(update.estimate.sigma_percent)
            .bind(&estimator_json)
            .execute(&mut *tx)
            .await?;

            let event = DomainEvent::ReadingSaved {
                device_id: reading.node_id.clone(),
                time: at,
                soc_percent: update.estimate.soc_percent,
            };
            sqlx::query("select pg_notify($1, $2)")
                .bind(CHANNEL)
                .bind(event.to_json())
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn on_invalid(&self, message: &InvalidMessage, received_at: DateTime<Utc>) -> Result<()> {
        sqlx::query("insert into invalid_messages_log (received_at, topic, payload, reason) values ($1, $2, $3, $4)")
            .bind(received_at)
            .bind(&message.topic)
            .bind(&message.payload)
            .bind(&message.reason)
            .execute(&self.pool)
            .await?;
        tracing::warn!(topic = %message.topic, reason = %message.reason, "Nachricht verworfen");
        Ok(())
    }
}

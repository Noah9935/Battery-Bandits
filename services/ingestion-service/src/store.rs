//! Speichern in PostgreSQL/TimescaleDB und Ereignis `ReadingSaved` per NOTIFY.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use shared::events::CHANNEL;
use shared::{DomainEvent, Settings};
use sqlx::PgPool;

use crate::classify::{InvalidMessage, ValidReading};
use crate::observer::ReadingObserver;
use crate::soc_state::{DeviceSocState, effective_time, update_soc};

pub struct PostgresObserver {
    pool: PgPool,
}

impl PostgresObserver {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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

        sqlx::query("insert into devices (id, battery_installed_at) values ($1, $2) on conflict (id) do nothing")
            .bind(&reading.node_id)
            .bind(at)
            .execute(&mut *tx)
            .await?;

        let settings: Settings = sqlx::query_as(
            "select threshold_critical_pct, threshold_warning_pct, offline_timeout_seconds, \
             min_voltage_mv, max_voltage_mv, battery_capacity_ah from system_settings",
        )
        .fetch_one(&mut *tx)
        .await?;

        let (last_seen_at, current_ma, soc_start_percent, discharged_ah): (
            Option<DateTime<Utc>>,
            Option<i32>,
            Option<f32>,
            f64,
        ) = sqlx::query_as(
            "select last_seen_at, current_ma, soc_start_percent, discharged_ah from devices where id = $1 for update",
        )
        .bind(&reading.node_id)
        .fetch_one(&mut *tx)
        .await?;

        let prev = DeviceSocState {
            last_seen_at,
            current_ma,
            soc_start_percent,
            discharged_ah,
        };
        let update = update_soc(&prev, p.voltage_mv, p.current_ma, at, &settings);
        let satellites = p.satellites.map(i16::from);

        sqlx::query(
            "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent, \
             lat, lon, altitude_m, satellites) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(at)
        .bind(&reading.node_id)
        .bind(p.voltage_mv)
        .bind(p.current_ma)
        .bind(p.temperature_c)
        .bind(update.soc_percent)
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
                 battery_installed_at = coalesce(battery_installed_at, $10) where id = $1",
            )
            .bind(&reading.node_id)
            .bind(update.soc_percent)
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
            .execute(&mut *tx)
            .await?;

            let event = DomainEvent::ReadingSaved {
                device_id: reading.node_id.clone(),
                time: at,
                soc_percent: update.soc_percent,
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

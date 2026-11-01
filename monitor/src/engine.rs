use anyhow::{Context, Result, bail};
use kit::bus::Bus;
use kit::time::{now_ms, ts_from_ms};
use shared::Status;
use shared::events::{
    CommandReply, DeviceCritical, DeviceRecovered, ReadingStored, SetThreshold, ThresholdChanged, subject,
};
use shared::status::{Transition, evaluate, is_valid_threshold, transition};
use sqlx::PgPool;

const THRESHOLD_KEY: &str = "critical_threshold_pct";

pub struct Monitor {
    pool: PgPool,
    bus: Bus,
}

impl Monitor {
    /// Legt den Anfangs-Schwellwert an, falls noch keiner gespeichert ist.
    /// Ein später gesetzter Wert hat Vorrang vor der Umgebungsvariable.
    pub async fn new(pool: PgPool, bus: Bus, default_threshold_pct: f64) -> Result<Self> {
        if !is_valid_threshold(default_threshold_pct) {
            bail!("CRITICAL_THRESHOLD_PCT muss zwischen 1 und 99 liegen, ist aber {default_threshold_pct}");
        }
        sqlx::query!(
            "insert into monitor.setting (key, value) values ($1, $2) on conflict (key) do nothing",
            THRESHOLD_KEY,
            default_threshold_pct
        )
        .execute(&pool)
        .await?;
        Ok(Self { pool, bus })
    }

    pub async fn threshold(&self) -> Result<f64> {
        let row = sqlx::query!("select value from monitor.setting where key = $1", THRESHOLD_KEY)
            .fetch_one(&self.pool)
            .await
            .context("Schwellwert fehlt")?;
        Ok(row.value)
    }

    /// Verarbeitet ein Ereignis aus dem Stream.
    pub async fn on_event(&self, subject: &str, payload: &[u8]) -> Result<()> {
        match subject {
            subject::READING_STORED => match serde_json::from_slice::<ReadingStored>(payload) {
                Ok(event) => self.on_reading(event).await,
                Err(e) => {
                    // Eine unlesbare Nachricht wird nie lesbar. Verwerfen statt endlos wiederholen.
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

    /// Ereignis "Ladestand bewerten": neuen Zustand bestimmen, speichern, Wechsel melden.
    ///
    /// Gemeldet wird vor dem Abschluss der Transaktion. Schlägt der Abschluss
    /// fehl, wird das Ereignis erneut zugestellt und dasselbe nochmal gemeldet;
    /// die Kennung sorgt dafür, dass NATS das Duplikat verwirft. So geht nie
    /// ein Zustandswechsel verloren.
    pub async fn on_reading(&self, event: ReadingStored) -> Result<()> {
        let ts = ts_from_ms(event.timestamp_ms);
        let mut tx = self.pool.begin().await?;

        // Der Schwellwert wird innerhalb der Transaktion gelesen und dabei gesperrt. So wartet
        // eine gleichzeitige Änderung des Schwellwerts auf diesen Messwert, und ein Messwert,
        // der den neuen Wert abwarten musste, wird nicht mit dem alten bewertet.
        let threshold = sqlx::query!(
            "select value from monitor.setting where key = $1 for share",
            THRESHOLD_KEY
        )
        .fetch_one(&mut *tx)
        .await
        .context("Schwellwert fehlt")?
        .value;
        let new_status = evaluate(event.charge_pct, threshold);

        let previous = sqlx::query!(
            "select status, reading_ts from monitor.device_state where device_id = $1 for update",
            event.device_id
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(p) = &previous
            && p.reading_ts > ts
        {
            // Verspätet zugestellter, älterer Messwert: Der Zustand bleibt.
            tx.rollback().await?;
            return Ok(());
        }

        let previous_status = previous.as_ref().and_then(|p| Status::from_db(&p.status));
        sqlx::query!(
            r#"insert into monitor.device_state (device_id, charge_pct, reading_ts, status)
               values ($1, $2, $3, $4)
               on conflict (device_id) do update
                 set charge_pct = excluded.charge_pct,
                     reading_ts = excluded.reading_ts,
                     status     = excluded.status,
                     changed_at = case when monitor.device_state.status = excluded.status
                                       then monitor.device_state.changed_at else now() end"#,
            event.device_id,
            event.charge_pct,
            ts,
            new_status.as_str(),
        )
        .execute(&mut *tx)
        .await?;

        self.announce(
            transition(previous_status, new_status),
            &event.device_id,
            event.charge_pct,
            event.timestamp_ms,
            threshold,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Befehl "Schwellwert setzen": speichern und alle Geräte neu bewerten (Event Storming 6).
    pub async fn set_threshold(&self, critical_pct: f64) -> Result<()> {
        if !is_valid_threshold(critical_pct) {
            bail!("Der Schwellwert muss zwischen 1 und 99 Prozent liegen");
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query!(
            "insert into monitor.setting (key, value) values ($1, $2)
             on conflict (key) do update set value = excluded.value",
            THRESHOLD_KEY,
            critical_pct
        )
        .execute(&mut *tx)
        .await?;

        let at_ms = now_ms();
        let rows = sqlx::query!("select device_id, charge_pct, status from monitor.device_state for update")
            .fetch_all(&mut *tx)
            .await?;
        let mut changed = 0usize;
        for row in rows {
            let previous = Status::from_db(&row.status);
            let current = evaluate(row.charge_pct, critical_pct);
            if previous == Some(current) {
                continue;
            }
            sqlx::query!(
                "update monitor.device_state set status = $2, changed_at = now() where device_id = $1",
                row.device_id,
                current.as_str()
            )
            .execute(&mut *tx)
            .await?;
            self.announce(
                transition(previous, current),
                &row.device_id,
                row.charge_pct,
                at_ms,
                critical_pct,
            )
            .await?;
            changed += 1;
        }
        self.bus
            .publish(subject::THRESHOLD_CHANGED, None, &ThresholdChanged { critical_pct })
            .await?;
        tx.commit().await?;
        tracing::info!(critical_pct, changed, "Schwellwert geändert, Geräte neu bewertet");
        Ok(())
    }

    /// Antwort auf den Befehl aus der API.
    pub async fn handle_set_threshold(&self, cmd: SetThreshold) -> CommandReply {
        match self.set_threshold(cmd.critical_pct).await {
            Ok(()) => CommandReply::ok(),
            Err(e) => {
                tracing::warn!("Schwellwert nicht gesetzt: {e:#}");
                CommandReply::error(e.to_string())
            }
        }
    }

    async fn announce(
        &self,
        t: Transition,
        device_id: &str,
        charge_pct: f64,
        at_ms: i64,
        threshold_pct: f64,
    ) -> Result<()> {
        match t {
            Transition::Unchanged => Ok(()),
            Transition::BecameCritical => {
                tracing::info!(%device_id, charge_pct, "Gerät kritisch");
                let event = DeviceCritical {
                    device_id: device_id.to_owned(),
                    charge_pct,
                    threshold_pct,
                    at_ms,
                };
                self.bus
                    .publish(
                        subject::DEVICE_CRITICAL,
                        Some(&format!("critical:{device_id}:{at_ms}")),
                        &event,
                    )
                    .await
            }
            Transition::Recovered => {
                tracing::info!(%device_id, charge_pct, "Gerät wieder normal");
                let event = DeviceRecovered {
                    device_id: device_id.to_owned(),
                    charge_pct,
                    at_ms,
                };
                self.bus
                    .publish(
                        subject::DEVICE_RECOVERED,
                        Some(&format!("recovered:{device_id}:{at_ms}")),
                        &event,
                    )
                    .await
            }
        }
    }
}

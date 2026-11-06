//! Datenbankoperationen der Überwachung. Jede Änderung eines Gerätestatus wird in
//! derselben Transaktion als `DeviceStatusChanged` auf `domain_events` gemeldet.

use anyhow::Result;
use chrono::{DateTime, Utc};
use shared::db::notify_event;
use shared::forecast::forecast_from_slope;
use shared::{DeviceStatus, DomainEvent, Settings};
use sqlx::PgPool;

use crate::rules::transition;

pub async fn load_settings<'e, E>(executor: E) -> Result<Settings>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    Ok(sqlx::query_as(
        "select threshold_critical_pct, threshold_warning_pct, offline_timeout_seconds, \
         min_voltage_mv, max_voltage_mv, battery_capacity_ah from system_settings",
    )
    .fetch_one(executor)
    .await?)
}

/// Bewertet ein Gerät neu und meldet eine Statusänderung. Gibt das gemeldete Ereignis zurück.
pub async fn evaluate_device(pool: &PgPool, device_id: &str, now: DateTime<Utc>) -> Result<Option<DomainEvent>> {
    let mut tx = pool.begin().await?;
    // Zeile sperren: Ein gleichzeitig eintreffender Messwert wartet, bis wir fertig sind.
    let row: Option<(DeviceStatus, Option<f32>, Option<DateTime<Utc>>)> =
        sqlx::query_as("select status, soc_percent, last_seen_at from devices where id = $1 for update")
            .bind(device_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((current, soc, last_seen)) = row else {
        return Ok(None);
    };
    // Schwellwerte innerhalb der Transaktion lesen, damit keine veraltete Einstellung greift.
    let settings = load_settings(&mut *tx).await?;
    let Some(new_status) = transition(current, soc, last_seen, now, &settings) else {
        return Ok(None);
    };
    sqlx::query("update devices set status = $2 where id = $1")
        .bind(device_id)
        .bind(new_status)
        .execute(&mut *tx)
        .await?;
    let event = DomainEvent::DeviceStatusChanged {
        device_id: device_id.to_owned(),
        old_status: current,
        new_status,
        soc_percent: soc,
    };
    notify_event(&mut *tx, &event).await?;
    tx.commit().await?;
    tracing::info!(
        device = device_id,
        from = current.as_str(),
        to = new_status.as_str(),
        "Status geändert"
    );
    Ok(Some(event))
}

/// Bewertet alle Geräte (Offline-Erkennung, geänderte Schwellwerte, verpasste Ereignisse).
/// Gibt die Zahl der Statuswechsel zurück.
pub async fn evaluate_all(pool: &PgPool, now: DateTime<Utc>) -> Result<usize> {
    let ids: Vec<String> = sqlx::query_scalar("select id from devices order by id")
        .fetch_all(pool)
        .await?;
    let mut changed = 0;
    for id in ids {
        match evaluate_device(pool, &id, now).await {
            Ok(Some(_)) => changed += 1,
            Ok(None) => {}
            Err(e) => tracing::warn!(device = %id, "Bewertung fehlgeschlagen: {e:#}"),
        }
    }
    Ok(changed)
}

/// Nach einem Batteriewechsel beginnt ein neuer Entladezyklus: Der Startwert wird beim
/// nächsten Messwert aus der Spannung neu geschätzt, die Prognose entfällt bis dahin.
pub async fn on_battery_replaced(pool: &PgPool, device_id: &str, at: DateTime<Utc>, now: DateTime<Utc>) -> Result<()> {
    sqlx::query(
        "update devices set soc_start_percent = null, discharged_ah = 0, soc_percent = null, \
         battery_installed_at = $2, discharge_rate_pct_per_hour = null, forecast_hours_remaining = null \
         where id = $1",
    )
    .bind(device_id)
    .bind(at)
    .execute(pool)
    .await?;
    evaluate_device(pool, device_id, now).await?;
    Ok(())
}

/// Schreibt die Restlaufzeit-Prognose für alle Geräte fort: lineare Entladerate über die
/// letzten 24 Stunden, höchstens seit dem letzten Batteriewechsel. Mindestens drei Messwerte
/// über mindestens zehn Minuten sind nötig. Gibt die Zahl der Geräte mit Prognose zurück.
pub async fn refresh_forecasts(pool: &PgPool) -> Result<usize> {
    let rows: Vec<(String, f64, f32)> = sqlx::query_as(
        "select r.device_id, \
                regr_slope(r.soc_percent::float8, extract(epoch from r.time)) * 3600 as slope_per_hour, \
                (array_agg(r.soc_percent order by r.time desc))[1] as soc \
         from readings r join devices d on d.id = r.device_id \
         where r.time >= greatest(now() - interval '24 hours', coalesce(d.battery_installed_at, '-infinity'::timestamptz)) \
         group by r.device_id \
         having count(*) >= 3 and max(r.time) - min(r.time) >= interval '10 minutes'",
    )
    .fetch_all(pool)
    .await?;

    let mut with_forecast: Vec<String> = Vec::new();
    for (device_id, slope, soc) in rows {
        let Some(forecast) = forecast_from_slope(slope, soc) else {
            continue;
        };
        sqlx::query("update devices set discharge_rate_pct_per_hour = $2, forecast_hours_remaining = $3 where id = $1")
            .bind(&device_id)
            .bind(forecast.rate_pct_per_hour)
            .bind(forecast.hours_to_empty)
            .execute(pool)
            .await?;
        with_forecast.push(device_id);
    }
    // Wer keine belastbare Prognose mehr hat, verliert den alten Wert.
    sqlx::query(
        "update devices set discharge_rate_pct_per_hour = null, forecast_hours_remaining = null \
         where id <> all($1) and (discharge_rate_pct_per_hour is not null or forecast_hours_remaining is not null)",
    )
    .bind(&with_forecast)
    .execute(pool)
    .await?;
    Ok(with_forecast.len())
}

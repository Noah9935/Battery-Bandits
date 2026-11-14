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
    let row: Option<(DeviceStatus, Option<f32>, Option<DateTime<Utc>>)> = sqlx::query_as(
        "select status, soc_percent, last_seen_at from devices where id = $1 and retired_at is null for update",
    )
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
    // Der Zeitpunkt des Wechsels wird mitgeschrieben: Daraus zeigt die Oberfläche, seit wann der Zustand besteht.
    sqlx::query("update devices set status = $2, status_changed_at = $3 where id = $1")
        .bind(device_id)
        .bind(new_status)
        .bind(now)
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
    let ids: Vec<String> = sqlx::query_scalar("select id from devices where retired_at is null order by id")
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

/// Hält fest, was die alte Batterie geleistet hat: Dauer, entnommene Ladung, Spannung am Anfang und am Ende
/// und die mittlere Temperatur. Ohne Messwerte im Zyklus entsteht kein Eintrag. Gibt zurück, ob einer entstand.
/// Die eingebaute Batterie eines Geräts.
#[derive(sqlx::FromRow)]
struct InstalledBattery {
    battery_installed_at: Option<DateTime<Utc>>,
    discharged_ah: f64,
    battery_price_cents: Option<i32>,
    battery_model: Option<String>,
}

/// Kennzahlen eines Zyklus aus seinen Messwerten.
#[derive(sqlx::FromRow)]
struct CycleStats {
    readings: i64,
    start_mv: Option<i32>,
    end_mv: Option<i32>,
    avg_temperature: Option<f32>,
    /// Steigung der Spannung über dem Strom (mV je mA, also Ohm), negativ bei Spannungsabfall unter Last.
    slope_v_per_ma: Option<f64>,
    current_spread_ma: Option<f64>,
}

async fn archive_cycle(conn: &mut sqlx::PgConnection, device_id: &str, ended_at: DateTime<Utc>) -> Result<bool> {
    let row: Option<InstalledBattery> = sqlx::query_as(
        "select battery_installed_at, discharged_ah, battery_price_cents, battery_model from devices where id = $1 for update",
    )
    .bind(device_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(InstalledBattery {
        battery_installed_at: Some(started_at),
        discharged_ah,
        battery_price_cents: price_cents,
        battery_model: model,
    }) = row
    else {
        return Ok(false);
    };
    if ended_at <= started_at {
        return Ok(false);
    }
    let stats: CycleStats = sqlx::query_as(
        "select count(*) as readings, \
         (array_agg(voltage_mv order by time asc))[1] as start_mv, \
         (array_agg(voltage_mv order by time desc))[1] as end_mv, \
         avg(temperature_c)::real as avg_temperature, \
         regr_slope(voltage_mv::float8, abs(current_ma)::float8) as slope_v_per_ma, \
         stddev_samp(abs(current_ma)::float8) as current_spread_ma \
         from readings where device_id = $1 and time >= $2 and time <= $3",
    )
    .bind(device_id)
    .bind(started_at)
    .bind(ended_at)
    .fetch_one(&mut *conn)
    .await?;
    if stats.readings == 0 {
        return Ok(false);
    }
    let resistance =
        shared::efficiency::internal_resistance_mohm(stats.slope_v_per_ma, stats.current_spread_ma, stats.readings);
    sqlx::query(
        "insert into battery_cycles \
         (device_id, started_at, ended_at, discharged_ah, start_voltage_mv, end_voltage_mv, avg_temperature_c, price_cents, \
          model, resistance_mohm) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(device_id)
    .bind(started_at)
    .bind(ended_at)
    .bind(discharged_ah)
    .bind(stats.start_mv)
    .bind(stats.end_mv)
    .bind(stats.avg_temperature)
    .bind(price_cents)
    .bind(model)
    .bind(resistance.map(|r| r as f32))
    .execute(&mut *conn)
    .await?;
    Ok(true)
}

/// Nach einem Batteriewechsel beginnt ein neuer Entladezyklus. Der alte wird vorher für den Vergleich
/// festgehalten. Ist der Ladestand der neuen Batterie bekannt (`soc_percent`), gilt er sofort. Sonst wird der
/// Startwert beim nächsten Messwert aus der Spannung geschätzt. Die Prognose entfällt bis dahin.
pub async fn on_battery_replaced(
    pool: &PgPool,
    device_id: &str,
    at: DateTime<Utc>,
    soc_percent: Option<f32>,
    price_cents: Option<i32>,
    model: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    archive_cycle(&mut tx, device_id, at).await?;
    // Der letzte Strom wird vergessen: Die Integration beginnt erst mit der ersten Messung der neuen Batterie
    // und zählt die Zeit seit der letzten Messung der alten nicht mit.
    sqlx::query(
        "update devices set soc_start_percent = $3, discharged_ah = 0, soc_percent = $3, current_ma = null, \
         battery_installed_at = $2, discharge_rate_pct_per_hour = null, forecast_hours_remaining = null, \
         battery_price_cents = $4, battery_model = $5 \
         where id = $1",
    )
    .bind(device_id)
    .bind(at)
    .bind(soc_percent.map(|soc| soc.clamp(0.0, 100.0)))
    .bind(price_cents)
    .bind(model.map(str::trim).filter(|m| !m.is_empty()))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
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

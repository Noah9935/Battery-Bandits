//! Auffälligkeiten, die der Ladezustand allein nicht verrät:
//!
//! * Stillstand: Spannung und Strom bleiben über viele Stunden auf demselben Wert, obwohl Messwerte
//!   eintreffen. Das spricht für einen hängenden Sensor.
//! * Spannungseinbruch: Die Spannung fällt innerhalb kurzer Zeit stärker, als eine normale Last erklärt.
//!
//! Die Prüfung läuft im Takt der Gesamtbewertung. Gemeldet wird nur, wenn eine Auffälligkeit neu entsteht,
//! sonst würde jede Runde dasselbe Ereignis senden. Der Operations-Service eröffnet daraus die Meldung.

use std::collections::BTreeSet;

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use shared::db::notify_event;
use shared::{AlertKind, AnomalySettings, DomainEvent};
use sqlx::PgPool;

pub async fn load_settings(pool: &PgPool) -> Result<AnomalySettings> {
    Ok(shared::db::load_anomaly_settings(pool).await?)
}

/// Geräte, deren Messwerte im Fenster lückenlos und bis auf die Stelle gleich geblieben sind.
/// Mindestens fünf Messwerte, die zusammen vier Fünftel des Fensters abdecken.
pub async fn stagnant(pool: &PgPool, now: DateTime<Utc>, hours: f32) -> Result<Vec<String>> {
    let window = Duration::seconds((f64::from(hours) * 3600.0) as i64);
    Ok(sqlx::query_scalar(
        "select r.device_id from readings r join devices d on d.id = r.device_id \
         where d.status <> 'offline' and r.time >= $1 and r.time <= $2 \
           and r.time >= coalesce(d.battery_installed_at, '-infinity'::timestamptz) \
         group by r.device_id \
         having count(*) >= 5 \
            and extract(epoch from max(r.time) - min(r.time)) >= $3 \
            and min(r.voltage_mv) = max(r.voltage_mv) and min(r.current_ma) = max(r.current_ma) \
         order by r.device_id",
    )
    .bind(now - window)
    .bind(now)
    .bind(window.num_seconds() as f64 * 0.8)
    .fetch_all(pool)
    .await?)
}

/// Geräte, deren Spannung im Fenster um mindestens `drop_mv` unter den Höchstwert gefallen ist.
pub async fn dropped(pool: &PgPool, now: DateTime<Utc>, minutes: i32, drop_mv: i32) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "select device_id from ( \
            select r.device_id, max(r.voltage_mv) as top, (array_agg(r.voltage_mv order by r.time desc))[1] as latest, \
                   count(*) as n \
            from readings r join devices d on d.id = r.device_id \
            where d.status <> 'offline' and r.time >= $1 and r.time <= $2 \
              and r.time >= coalesce(d.battery_installed_at, '-infinity'::timestamptz) \
            group by r.device_id) t \
         where n >= 3 and top - latest >= $3 order by device_id",
    )
    .bind(now - Duration::minutes(i64::from(minutes)))
    .bind(now)
    .bind(drop_mv)
    .fetch_all(pool)
    .await?)
}

/// Alle derzeit auffälligen Geräte mit der Art der Auffälligkeit.
pub async fn current(pool: &PgPool, now: DateTime<Utc>, s: &AnomalySettings) -> Result<BTreeSet<(String, AlertKind)>> {
    let mut found = BTreeSet::new();
    for device in stagnant(pool, now, s.stagnation_hours).await? {
        found.insert((device, AlertKind::Stagnation));
    }
    for device in dropped(pool, now, s.drop_window_minutes, s.drop_mv).await? {
        found.insert((device, AlertKind::Drop));
    }
    Ok(found)
}

/// Merkt sich, was schon gemeldet wurde.
#[derive(Debug, Default)]
pub struct Tracker {
    flagged: BTreeSet<(String, AlertKind)>,
}

/// Prüft alle Geräte und meldet jede neu entstandene Auffälligkeit. Gibt die Zahl der Meldungen zurück.
/// Verschwindet eine Auffälligkeit, wird sie vergessen und kann später wieder gemeldet werden.
pub async fn sweep(pool: &PgPool, tracker: &mut Tracker, now: DateTime<Utc>) -> Result<usize> {
    let settings = load_settings(pool).await?;
    let current = if settings.enabled {
        current(pool, now, &settings).await?
    } else {
        BTreeSet::new()
    };
    let mut sent = 0;
    for (device_id, kind) in current.difference(&tracker.flagged) {
        let mut conn = pool.acquire().await?;
        notify_event(
            &mut *conn,
            &DomainEvent::AnomalyDetected {
                device_id: device_id.clone(),
                kind: *kind,
            },
        )
        .await?;
        tracing::info!(device = %device_id, art = kind.as_str(), "Auffälligkeit erkannt");
        sent += 1;
    }
    tracker.flagged = current;
    Ok(sent)
}

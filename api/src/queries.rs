//! Lesende Abfragen auf die Tabellen der anderen Dienste.
//!
//! Alle Abfragen werden beim Kompilieren gegen die Datenbank geprüft (SQLx).

use anyhow::Result;
use kit::time::ts_from_ms;
use shared::Status;
use shared::api::{
    DeviceSummary, History, HistoryPoint, Incident, IncidentState, Range, RejectedMessage, Resolution, Settings,
};
use sqlx::PgPool;

const THRESHOLD_KEY: &str = "critical_threshold_pct";

fn status_of(raw: &str) -> Status {
    Status::from_db(raw).unwrap_or(Status::Ok)
}

pub async fn devices(pool: &PgPool) -> Result<Vec<DeviceSummary>> {
    let rows = sqlx::query!(
        r#"select d.id                          as "id!",
                  d.last_charge_pct             as "charge_pct!",
                  d.last_reading_at             as "last_reading_at!",
                  coalesce(s.status, 'ok')      as "status!",
                  exists (select 1 from incidents.incident i
                          where i.device_id = d.id and i.state = 'open') as "open_incident!"
           from ingest.device d
           left join monitor.device_state s on s.device_id = d.id
           order by d.id"#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| DeviceSummary {
            id: r.id,
            charge_pct: Some(r.charge_pct),
            last_reading_ms: Some(r.last_reading_at.timestamp_millis()),
            status: status_of(&r.status),
            open_incident: r.open_incident,
        })
        .collect())
}

pub async fn device(pool: &PgPool, id: &str) -> Result<Option<DeviceSummary>> {
    let row = sqlx::query!(
        r#"select d.id                          as "id!",
                  d.last_charge_pct             as "charge_pct!",
                  d.last_reading_at             as "last_reading_at!",
                  coalesce(s.status, 'ok')      as "status!",
                  exists (select 1 from incidents.incident i
                          where i.device_id = d.id and i.state = 'open') as "open_incident!"
           from ingest.device d
           left join monitor.device_state s on s.device_id = d.id
           where d.id = $1"#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| DeviceSummary {
        id: r.id,
        charge_pct: Some(r.charge_pct),
        last_reading_ms: Some(r.last_reading_at.timestamp_millis()),
        status: status_of(&r.status),
        open_incident: r.open_incident,
    }))
}

/// Verlauf eines Geräts. Kurze Zeiträume kommen aus den Rohwerten, lange aus den Stundenwerten.
pub async fn history(pool: &PgPool, device_id: &str, range: Range, now_ms: i64) -> Result<History> {
    let from_ms = now_ms - range.duration_ms();
    let (from, to) = (ts_from_ms(from_ms), ts_from_ms(now_ms));
    let resolution = range.resolution();

    let points: Vec<HistoryPoint> = match resolution {
        Resolution::Raw => sqlx::query!(
            r#"select ts as "ts!", charge_pct as "charge_pct!"
               from ingest.reading
               where device_id = $1 and ts >= $2 and ts <= $3
               order by ts"#,
            device_id,
            from,
            to
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| HistoryPoint {
            t_ms: r.ts.timestamp_millis(),
            charge_pct: r.charge_pct,
        })
        .collect(),
        Resolution::Hourly => sqlx::query!(
            r#"select bucket as "bucket!", charge_pct as "charge_pct!"
               from ingest.reading_hourly
               where device_id = $1 and bucket >= $2 and bucket <= $3
               order by bucket"#,
            device_id,
            from,
            to
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| HistoryPoint {
            t_ms: r.bucket.timestamp_millis(),
            charge_pct: r.charge_pct,
        })
        .collect(),
    };

    Ok(History {
        device_id: device_id.to_owned(),
        from_ms,
        to_ms: now_ms,
        resolution,
        points,
    })
}

pub async fn settings(pool: &PgPool) -> Result<Settings> {
    let row = sqlx::query!("select value from monitor.setting where key = $1", THRESHOLD_KEY)
        .fetch_one(pool)
        .await?;
    Ok(Settings {
        critical_threshold_pct: row.value,
    })
}

pub async fn incidents(
    pool: &PgPool,
    state: Option<IncidentState>,
    device_id: Option<&str>,
    limit: i64,
) -> Result<Vec<Incident>> {
    let state = state.map(|s| s.as_str());
    let rows = sqlx::query!(
        r#"select id as "id!", device_id as "device_id!", opened_at as "opened_at!",
                  closed_at, state as "state!", charge_pct as "charge_pct!"
           from incidents.incident
           where ($1::text is null or state = $1)
             and ($2::text is null or device_id = $2)
           order by opened_at desc
           limit $3"#,
        state,
        device_id,
        limit
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Incident {
            id: r.id,
            device_id: r.device_id,
            opened_at_ms: r.opened_at.timestamp_millis(),
            closed_at_ms: r.closed_at.map(|t| t.timestamp_millis()),
            state: IncidentState::from_db(&r.state).unwrap_or(IncidentState::Open),
            charge_pct: r.charge_pct,
        })
        .collect())
}

pub async fn rejected(pool: &PgPool, limit: i64) -> Result<Vec<RejectedMessage>> {
    let rows = sqlx::query!(
        r#"select id as "id!", received_at as "received_at!", topic as "topic!", payload as "payload!", reason as "reason!"
           from ingest.rejected_message
           order by received_at desc
           limit $1"#,
        limit
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| RejectedMessage {
            id: r.id,
            received_at_ms: r.received_at.timestamp_millis(),
            topic: r.topic,
            payload: r.payload,
            reason: r.reason,
        })
        .collect())
}

//! Lese-Modelle für das Dashboard.

use shared::api::{DeviceSort, HistoryPoint, Range, SortDir};
use shared::{DashboardSummary, Device, DeviceStatus, Forecast, Reading, Settings};
use sqlx::PgPool;

/// Höchstzahl Zeilen beim CSV-Export.
pub const EXPORT_LIMIT: i64 = 200_000;

#[derive(Debug, Clone, Default)]
pub struct DeviceFilter {
    pub status: Option<DeviceStatus>,
    /// Nur Geräte mit dieser Datenherkunft (Dozent oder Simulation).
    pub source: Option<shared::DataSource>,
    pub search: Option<String>,
    pub sort: DeviceSort,
    pub dir: SortDir,
    /// Kritische und Offline-Geräte vor allen anderen.
    pub attention_first: bool,
}

/// ORDER-BY-Klausel aus einer festen Auswahl (nie aus Benutzereingaben zusammengesetzt).
pub fn order_by_clause(sort: DeviceSort, dir: SortDir, attention_first: bool) -> String {
    let direction = match dir {
        SortDir::Asc => "asc",
        SortDir::Desc => "desc",
    };
    let key = match sort {
        DeviceSort::Id => format!("id {direction}"),
        DeviceSort::SocPercent => format!("soc_percent {direction} nulls last"),
        DeviceSort::LastSeenAt => format!("last_seen_at {direction} nulls last"),
        DeviceSort::Status => {
            format!(
                "(case status when 'critical' then 0 when 'offline' then 1 when 'warning' then 2 else 3 end) {direction}"
            )
        }
    };
    let attention = if attention_first {
        "(case when status in ('critical', 'offline') then 0 else 1 end), "
    } else {
        ""
    };
    format!("{attention}{key}, id asc")
}

/// Suchtext als LIKE-Muster: Sonderzeichen werden entschärft, leere Eingaben ergeben `None`.
pub fn search_pattern(search: Option<&str>) -> Option<String> {
    let text = search?.trim();
    if text.is_empty() {
        return None;
    }
    let escaped = text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    Some(format!("%{escaped}%"))
}

pub async fn summary(pool: &PgPool) -> Result<DashboardSummary, sqlx::Error> {
    let (total, normal, warning, critical, offline, average): (i64, i64, i64, i64, i64, Option<f64>) = sqlx::query_as(
        "select count(*), \
                count(*) filter (where status = 'normal'), \
                count(*) filter (where status = 'warning'), \
                count(*) filter (where status = 'critical'), \
                count(*) filter (where status = 'offline'), \
                avg(soc_percent)::float8 \
         from devices",
    )
    .fetch_one(pool)
    .await?;
    let open_alerts: i64 = sqlx::query_scalar("select count(*) from alerts where status = 'open'")
        .fetch_one(pool)
        .await?;
    Ok(DashboardSummary {
        total,
        normal,
        warning,
        critical,
        offline,
        open_alerts,
        average_soc_percent: average.map(|a| a as f32),
    })
}

pub async fn devices(pool: &PgPool, filter: &DeviceFilter) -> Result<Vec<Device>, sqlx::Error> {
    let sql = format!(
        "select * from devices \
         where ($1::device_status is null or status = $1) \
           and ($2::text is null or id ilike $2 or coalesce(name, '') ilike $2) \
           and ($3::data_source is null or data_source = $3) \
         order by {}",
        order_by_clause(filter.sort, filter.dir, filter.attention_first)
    );
    sqlx::query_as(&sql)
        .bind(filter.status)
        .bind(search_pattern(filter.search.as_deref()))
        .bind(filter.source)
        .fetch_all(pool)
        .await
}

pub async fn device(pool: &PgPool, id: &str) -> Result<Option<Device>, sqlx::Error> {
    sqlx::query_as("select * from devices where id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Verlauf als gemittelte Punkte. Die Prognose stammt aus der Berechnung des Monitoring-Service.
pub async fn history(
    pool: &PgPool,
    device: &Device,
    range: Range,
) -> Result<(Vec<HistoryPoint>, Option<Forecast>), sqlx::Error> {
    // Die Intervalle stammen aus einer festen Auswahl (Range), nicht aus Benutzereingaben.
    let sql = format!(
        "select time_bucket(interval '{bucket}', time) as time, \
                avg(soc_percent)::real as soc_percent, avg(voltage_mv)::real as voltage_mv, \
                avg(current_ma)::real as current_ma, avg(temperature_c)::real as temperature_c \
         from readings where device_id = $1 and time >= now() - interval '{window}' \
         group by 1 order by 1",
        bucket = range.bucket_sql(),
        window = range.window_sql()
    );
    let points = sqlx::query_as(&sql).bind(&device.id).fetch_all(pool).await?;
    let forecast = match (device.discharge_rate_pct_per_hour, device.forecast_hours_remaining) {
        (Some(rate_pct_per_hour), Some(hours_to_empty)) => Some(Forecast {
            rate_pct_per_hour,
            hours_to_empty,
        }),
        _ => None,
    };
    Ok((points, forecast))
}

/// Rohe Messwerte für den CSV-Export (ungemittelt, älteste zuerst).
pub async fn export_readings(pool: &PgPool, device_id: &str, range: Range) -> Result<Vec<Reading>, sqlx::Error> {
    let sql = format!(
        "select * from readings where device_id = $1 and time >= now() - interval '{}' order by time limit {EXPORT_LIMIT}",
        range.window_sql()
    );
    sqlx::query_as(&sql).bind(device_id).fetch_all(pool).await
}

const SETTINGS_COLUMNS: &str = "threshold_critical_pct, threshold_warning_pct, offline_timeout_seconds, min_voltage_mv, max_voltage_mv, battery_capacity_ah";

pub async fn settings(pool: &PgPool) -> Result<Settings, sqlx::Error> {
    sqlx::query_as(&format!("select {SETTINGS_COLUMNS} from system_settings"))
        .fetch_one(pool)
        .await
}

pub async fn update_settings(pool: &PgPool, s: &Settings) -> Result<Settings, sqlx::Error> {
    sqlx::query_as(&format!(
        "update system_settings set threshold_critical_pct = $1, threshold_warning_pct = $2, \
         offline_timeout_seconds = $3, min_voltage_mv = $4, max_voltage_mv = $5, battery_capacity_ah = $6 \
         where id = 1 returning {SETTINGS_COLUMNS}"
    ))
    .bind(s.threshold_critical_pct)
    .bind(s.threshold_warning_pct)
    .bind(s.offline_timeout_seconds)
    .bind(s.min_voltage_mv)
    .bind(s.max_voltage_mv)
    .bind(s.battery_capacity_ah)
    .fetch_one(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_by_uses_fixed_fragments() {
        assert_eq!(order_by_clause(DeviceSort::Id, SortDir::Asc, false), "id asc, id asc");
        assert_eq!(
            order_by_clause(DeviceSort::SocPercent, SortDir::Desc, false),
            "soc_percent desc nulls last, id asc"
        );
        assert!(
            order_by_clause(DeviceSort::LastSeenAt, SortDir::Asc, false).starts_with("last_seen_at asc nulls last")
        );
    }

    #[test]
    fn status_sort_puts_critical_first_when_ascending() {
        let clause = order_by_clause(DeviceSort::Status, SortDir::Asc, false);
        assert!(clause.contains("'critical' then 0"));
        assert!(clause.contains("else 3 end) asc"));
    }

    #[test]
    fn attention_first_prefixes_the_clause() {
        let clause = order_by_clause(DeviceSort::SocPercent, SortDir::Asc, true);
        assert!(clause.starts_with("(case when status in ('critical', 'offline') then 0 else 1 end), soc_percent"));
    }

    #[test]
    fn search_pattern_escapes_wildcards() {
        assert_eq!(search_pattern(Some(" TN00 ")).as_deref(), Some("%TN00%"));
        assert_eq!(search_pattern(Some("100%_x")).as_deref(), Some("%100\\%\\_x%"));
        assert_eq!(search_pattern(Some("   ")), None);
        assert_eq!(search_pattern(None), None);
    }
}

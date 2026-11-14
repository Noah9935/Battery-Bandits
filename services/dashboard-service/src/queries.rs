//! Lese-Modelle für das Dashboard.

use shared::api::{DeviceSort, HistoryPoint, Range, SortDir};
use shared::efficiency::{
    CycleSummary, DataStock, DeviceEfficiency, EfficiencyDetail, Evidence, FleetEfficiency, HIGH_LOAD_FACTOR,
    LoadSplit, ModelInput, ModelSummary, TemperatureBand, aggregate_models, assess, band_bounds, cycle_efficiency,
    internal_resistance_mohm,
};
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
    /// Nur ausgemusterte Geräte (sonst nur aktive).
    pub retired_only: bool,
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
         from devices where retired_at is null",
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
           and (($4 and retired_at is not null) or (not $4 and retired_at is null)) \
         order by {}",
        order_by_clause(filter.sort, filter.dir, filter.attention_first)
    );
    sqlx::query_as(&sql)
        .bind(filter.status)
        .bind(search_pattern(filter.search.as_deref()))
        .bind(filter.source)
        .bind(filter.retired_only)
        .fetch_all(pool)
        .await
}

/// Fasst Zeilen (Gerät, Ladezustand) in Reihenfolge je Gerät zusammen. Die Zeilen müssen nach Gerät sortiert sein.
pub fn group_trends(rows: Vec<(String, f32)>) -> Vec<shared::api::Trend> {
    let mut trends: Vec<shared::api::Trend> = Vec::new();
    for (device_id, soc) in rows {
        match trends.last_mut() {
            Some(last) if last.device_id == device_id => last.points.push(soc),
            _ => trends.push(shared::api::Trend {
                device_id,
                points: vec![soc],
            }),
        }
    }
    trends
}

/// Trendlinien aller Geräte: Stundenmittel der 24 Stunden vor der jeweils letzten Messung.
pub async fn trends(pool: &PgPool) -> Result<Vec<shared::api::Trend>, sqlx::Error> {
    let rows: Vec<(String, f32)> = sqlx::query_as(
        "with last as (select device_id, max(time) as t from readings group by device_id) \n         select r.device_id, avg(r.soc_percent)::real \n         from readings r join last l on l.device_id = r.device_id join devices d on d.id = r.device_id \n         where d.retired_at is null and r.time >= l.t - interval '24 hours' \n         group by r.device_id, time_bucket(interval '1 hour', r.time) \n         order by r.device_id, time_bucket(interval '1 hour', r.time)",
    )
    .fetch_all(pool)
    .await?;
    Ok(group_trends(rows))
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

pub async fn anomaly_settings(pool: &PgPool) -> Result<shared::AnomalySettings, sqlx::Error> {
    shared::db::load_anomaly_settings(pool).await
}

pub async fn update_anomaly_settings(
    pool: &PgPool,
    s: &shared::AnomalySettings,
) -> Result<shared::AnomalySettings, sqlx::Error> {
    sqlx::query_as(
        "update anomaly_settings set enabled = $1, stagnation_hours = $2, drop_mv = $3, drop_window_minutes = $4, \
         fault_min_pct = $5 where id = 1 \
         returning enabled, stagnation_hours, drop_mv, drop_window_minutes, fault_min_pct",
    )
    .bind(s.enabled)
    .bind(s.stagnation_hours)
    .bind(s.drop_mv)
    .bind(s.drop_window_minutes)
    .bind(s.fault_min_pct)
    .fetch_one(pool)
    .await
}

// ---------------------------------------------------------------- Effizienz

/// Zeile des Lastvergleichs: Median, hohe Last (Zahl, Spannung), übliche Last (Zahl, Spannung).
type LoadRow = (f64, i64, Option<f64>, i64, Option<f64>);

/// Ausgewertet wird der laufende Batteriezyklus, höchstens aber dieser Zeitraum.
const EFFICIENCY_WINDOW: &str = "30 days";
/// Beginn des Auswertungszeitraums eines Geräts als SQL-Ausdruck (Alias `d` ist das Gerät).
fn window_start_sql() -> String {
    format!(
        "greatest(now() - interval '{EFFICIENCY_WINDOW}', coalesce(d.battery_installed_at, '-infinity'::timestamptz))"
    )
}

#[derive(sqlx::FromRow)]
struct EfficiencyRow {
    device_id: String,
    name: Option<String>,
    status: DeviceStatus,
    readings: i64,
    hours: Option<f64>,
    avg_current_ma: Option<f64>,
    slope_mv_per_second: Option<f64>,
    avg_temperature_c: Option<f32>,
    /// Steigung der Spannung über dem Strom (mV je mA, also Ohm).
    slope_v_per_ma: Option<f64>,
    current_spread_ma: Option<f64>,
    price_cents: Option<i32>,
    battery_hours: Option<f64>,
}

/// Macht aus den Rohwerten der Datenbank die Auswertung eines Geräts (rein, ohne Datenbank).
fn rate(row: EfficiencyRow, settings: &Settings) -> DeviceEfficiency {
    let hours = row.hours.unwrap_or(0.0);
    // Die Steigung ist negativ, wenn die Spannung fällt. Angezeigt wird der Abfall als positive Zahl.
    let drop_per_day = row.slope_mv_per_second.map(|s| -s * 86_400.0);
    DeviceEfficiency {
        device_id: row.device_id,
        name: row.name,
        status: row.status,
        window_hours: hours,
        readings: row.readings,
        avg_temperature_c: row.avg_temperature_c,
        internal_resistance_mohm: internal_resistance_mohm(row.slope_v_per_ma, row.current_spread_ma, row.readings),
        price_cents: row.price_cents,
        battery_hours: row.battery_hours,
        // Ob die Messwerte für eine Aussage reichen, entscheidet `assess`: Wo sie nicht reichen, steht keine Zahl.
        result: assess(
            row.avg_current_ma,
            drop_per_day,
            Evidence {
                readings: Some(row.readings),
                window_hours: hours,
            },
            settings,
        ),
    }
}

async fn efficiency_rows(pool: &PgPool, only: Option<&str>) -> Result<Vec<DeviceEfficiency>, sqlx::Error> {
    let settings = settings(pool).await?;
    let sql = format!(
        "select d.id as device_id, d.name, d.status, count(r.time) as readings, \
                extract(epoch from max(r.time) - min(r.time))::float8 / 3600 as hours, \
                avg(abs(r.current_ma))::float8 as avg_current_ma, \
                regr_slope(r.voltage_mv::float8, extract(epoch from r.time)) as slope_mv_per_second, \
                avg(r.temperature_c)::real as avg_temperature_c, \
                regr_slope(r.voltage_mv::float8, abs(r.current_ma)::float8) as slope_v_per_ma, \
                stddev_samp(abs(r.current_ma)::float8) as current_spread_ma, \
                d.battery_price_cents as price_cents, \
                (extract(epoch from now() - d.battery_installed_at) / 3600)::float8 as battery_hours \
         from devices d left join readings r on r.device_id = d.id and r.time >= {start} \
         where ($1::text is null or d.id = $1) \
         group by d.id, d.name, d.status, d.battery_price_cents, d.battery_installed_at order by d.id",
        start = window_start_sql()
    );
    let rows: Vec<EfficiencyRow> = sqlx::query_as(&sql).bind(only).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|row| rate(row, &settings)).collect())
}

/// Eine Seite des Protokolls der verworfenen Nachrichten, neueste zuerst. Lange Inhalte werden gekürzt.
pub async fn invalid_messages(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<shared::api::InvalidMessagesPage, sqlx::Error> {
    let total: i64 = sqlx::query_scalar("select count(*) from invalid_messages_log")
        .fetch_one(pool)
        .await?;
    let rows: Vec<(i64, chrono::DateTime<chrono::Utc>, String, String, String)> = sqlx::query_as(
        "select id, received_at, topic, left(payload, 400), reason from invalid_messages_log \
         order by received_at desc, id desc limit $1 offset $2",
    )
    .bind(limit.clamp(1, 100))
    .bind(offset.max(0))
    .fetch_all(pool)
    .await?;
    Ok(shared::api::InvalidMessagesPage {
        total,
        items: rows
            .into_iter()
            .map(
                |(id, received_at, topic, payload, reason)| shared::api::InvalidMessage {
                    id,
                    received_at,
                    topic,
                    payload,
                    reason,
                },
            )
            .collect(),
    })
}

/// Was liegt in der Datenbank? Bei großen Beständen ist die Zahl der Messwerte die Schätzung der Datenbank.
pub async fn data_stock(pool: &PgPool) -> Result<DataStock, sqlx::Error> {
    let readings_total: i64 = sqlx::query_scalar(
        "select coalesce(nullif(approximate_row_count('readings'), 0), (select count(*) from readings))",
    )
    .fetch_one(pool)
    .await?;
    let (oldest_reading, newest_reading) = sqlx::query_as("select min(time), max(time) from readings")
        .fetch_one(pool)
        .await?;
    let invalid_messages: i64 = sqlx::query_scalar("select count(*) from invalid_messages_log")
        .fetch_one(pool)
        .await?;
    Ok(DataStock {
        readings_total,
        oldest_reading,
        newest_reading,
        invalid_messages,
    })
}

/// Effizienz aller Geräte und der Datenbestand.
pub async fn fleet_efficiency(pool: &PgPool) -> Result<FleetEfficiency, sqlx::Error> {
    let settings = settings(pool).await?;
    let devices = efficiency_rows(pool, None).await?;
    let models = model_summaries(pool, &devices, &settings).await?;
    Ok(FleetEfficiency {
        devices,
        stock: data_stock(pool).await?,
        capacity_ah: settings.battery_capacity_ah,
        models,
    })
}

#[derive(sqlx::FromRow)]
struct RunningBattery {
    id: String,
    battery_model: Option<String>,
    battery_price_cents: Option<i32>,
    battery_installed_at: Option<chrono::DateTime<chrono::Utc>>,
    discharged_ah: f64,
}

/// Alle Batterien (abgeschlossene Zyklen und die eingebauten) nach Modell zusammengefasst.
async fn model_summaries(
    pool: &PgPool,
    devices: &[DeviceEfficiency],
    settings: &Settings,
) -> Result<Vec<ModelSummary>, sqlx::Error> {
    let cycles: Vec<shared::BatteryCycle> = sqlx::query_as(
        "select id, device_id, started_at, ended_at, discharged_ah, start_voltage_mv, end_voltage_mv, avg_temperature_c, \
                price_cents, model, resistance_mohm \
         from battery_cycles order by ended_at desc, id desc limit 5000",
    )
    .fetch_all(pool)
    .await?;
    let mut inputs: Vec<ModelInput> = cycles
        .iter()
        .map(|c| {
            let hours = (c.ended_at - c.started_at).num_seconds().max(0) as f64 / 3600.0;
            ModelInput {
                model: c.model.clone(),
                hours,
                discharged_ah: c.discharged_ah,
                price_cents: c.price_cents,
                resistance_mohm: c.resistance_mohm.map(f64::from),
                score_pct: cycle_efficiency(hours, c.discharged_ah, c.start_voltage_mv, c.end_voltage_mv, settings)
                    .score_pct,
                finished: true,
            }
        })
        .collect();
    let running: Vec<RunningBattery> = sqlx::query_as(
        "select id, battery_model, battery_price_cents, battery_installed_at, discharged_ah from devices",
    )
    .fetch_all(pool)
    .await?;
    for battery in running {
        let Some(installed) = battery.battery_installed_at else {
            continue;
        };
        let current = devices.iter().find(|d| d.device_id == battery.id);
        inputs.push(ModelInput {
            model: battery.battery_model,
            hours: (chrono::Utc::now() - installed).num_seconds().max(0) as f64 / 3600.0,
            discharged_ah: battery.discharged_ah,
            price_cents: battery.battery_price_cents,
            resistance_mohm: current.and_then(|d| d.internal_resistance_mohm),
            score_pct: current.and_then(|d| d.result.score_pct),
            finished: false,
        });
    }
    Ok(aggregate_models(&inputs))
}

/// Effizienz eines Geräts im Detail: Temperaturbereiche, Lastvergleich und die Batteriezyklen.
pub async fn efficiency_detail(pool: &PgPool, device: &Device) -> Result<Option<EfficiencyDetail>, sqlx::Error> {
    let Some(current) = efficiency_rows(pool, Some(&device.id)).await?.into_iter().next() else {
        return Ok(None);
    };
    let settings = settings(pool).await?;
    let start = window_start_sql();

    let band_rows: Vec<(i32, i64, f64, f64)> = sqlx::query_as(&format!(
        "select width_bucket(r.temperature_c, array[0, 10, 20, 30]::real[]), count(*), \
                avg(abs(r.current_ma))::float8, avg(r.voltage_mv)::float8 \
         from readings r join devices d on d.id = r.device_id \
         where d.id = $1 and r.time >= {start} and r.temperature_c is not null \
         group by 1 order by 1"
    ))
    .bind(&device.id)
    .fetch_all(pool)
    .await?;
    let bands = band_rows
        .into_iter()
        .map(|(index, readings, avg_current_ma, avg_voltage_mv)| {
            let (from_c, to_c) = band_bounds(usize::try_from(index).unwrap_or(0));
            TemperatureBand {
                from_c,
                to_c,
                readings,
                avg_current_ma,
                avg_voltage_mv,
            }
        })
        .collect();

    let load: Option<LoadRow> = sqlx::query_as(&format!(
        "with w as ( \
            select abs(r.current_ma)::float8 as c, r.voltage_mv::float8 as v \
            from readings r join devices d on d.id = r.device_id where d.id = $1 and r.time >= {start}), \
         m as (select percentile_cont(0.5) within group (order by c) as median from w) \
         select m.median, \
                count(*) filter (where w.c > m.median * {HIGH_LOAD_FACTOR}), \
                avg(w.v) filter (where w.c > m.median * {HIGH_LOAD_FACTOR}), \
                count(*) filter (where w.c <= m.median * {HIGH_LOAD_FACTOR}), \
                avg(w.v) filter (where w.c <= m.median * {HIGH_LOAD_FACTOR}) \
         from w, m where m.median is not null group by m.median"
    ))
    .bind(&device.id)
    .fetch_optional(pool)
    .await?;
    let load = load.map(|(median, high_n, high_v, normal_n, normal_v)| LoadSplit {
        median_current_ma: median,
        high_readings: high_n,
        high_avg_voltage_mv: high_v,
        normal_readings: normal_n,
        normal_avg_voltage_mv: normal_v,
    });

    // Der laufende Zyklus zuerst, danach die abgeschlossenen (neueste zuerst).
    let mut cycles = Vec::new();
    let (current_price, current_model): (Option<i32>, Option<String>) =
        sqlx::query_as("select battery_price_cents, battery_model from devices where id = $1")
            .bind(&device.id)
            .fetch_one(pool)
            .await?;
    if let Some(started_at) = device.battery_installed_at {
        let hours = (chrono::Utc::now() - started_at).num_seconds().max(0) as f64 / 3600.0;
        cycles.push(CycleSummary {
            id: None,
            started_at,
            ended_at: None,
            hours,
            discharged_ah: device.discharged_ah,
            avg_temperature_c: current.avg_temperature_c,
            price_cents: current_price,
            model: current_model,
            resistance_mohm: current.internal_resistance_mohm,
            result: current.result,
        });
    }
    let finished: Vec<shared::BatteryCycle> = sqlx::query_as(
        "select id, device_id, started_at, ended_at, discharged_ah, start_voltage_mv, end_voltage_mv, avg_temperature_c, \
                price_cents, model, resistance_mohm \
         from battery_cycles where device_id = $1 order by ended_at desc, id desc limit 12",
    )
    .bind(&device.id)
    .fetch_all(pool)
    .await?;
    cycles.extend(finished.into_iter().map(|c| {
        let hours = (c.ended_at - c.started_at).num_seconds().max(0) as f64 / 3600.0;
        CycleSummary {
            id: Some(c.id),
            started_at: c.started_at,
            ended_at: Some(c.ended_at),
            hours,
            discharged_ah: c.discharged_ah,
            avg_temperature_c: c.avg_temperature_c,
            price_cents: c.price_cents,
            model: c.model.clone(),
            resistance_mohm: c.resistance_mohm.map(f64::from),
            result: cycle_efficiency(hours, c.discharged_ah, c.start_voltage_mv, c.end_voltage_mv, &settings),
        }
    }));

    Ok(Some(EfficiencyDetail {
        current,
        bands,
        load,
        cycles,
    }))
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
    fn groups_trend_rows_per_device_in_order() {
        let rows = vec![("A".to_owned(), 90.0), ("A".to_owned(), 80.0), ("B".to_owned(), 50.0)];
        let trends = group_trends(rows);
        assert_eq!(trends.len(), 2);
        assert_eq!(
            (trends[0].device_id.as_str(), trends[0].points.clone()),
            ("A", vec![90.0, 80.0])
        );
        assert_eq!(trends[1].points, vec![50.0]);
        assert!(group_trends(Vec::new()).is_empty());
    }

    fn row(readings: i64, hours: Option<f64>, current: Option<f64>, slope: Option<f64>) -> EfficiencyRow {
        EfficiencyRow {
            device_id: "TN0001".into(),
            name: None,
            status: DeviceStatus::Normal,
            readings,
            hours,
            avg_current_ma: current,
            slope_mv_per_second: slope,
            avg_temperature_c: Some(11.0),
            slope_v_per_ma: None,
            current_spread_ma: None,
            price_cents: None,
            battery_hours: None,
        }
    }

    #[test]
    fn a_falling_voltage_becomes_a_positive_drop_per_day() {
        // 528 mV pro Tag sind 528 / 86400 mV pro Sekunde, als Steigung negativ.
        let rated = rate(
            row(500, Some(48.0), Some(800.0), Some(-528.0 / 86_400.0)),
            &Settings::default(),
        );
        assert!((rated.result.voltage_drop_mv_per_day.unwrap() - 528.0).abs() < 1e-6);
        assert!((rated.result.score_pct.unwrap() - 100.0).abs() < 1e-3);
        assert_eq!(
            (rated.readings, rated.window_hours, rated.avg_temperature_c),
            (500, 48.0, Some(11.0))
        );
    }

    #[test]
    fn too_little_data_gives_no_rating() {
        use shared::efficiency::Reason;
        let few = rate(row(2, Some(48.0), Some(800.0), Some(-0.006)), &Settings::default());
        assert_eq!(
            (few.result.score_pct, few.result.reason),
            (None, Some(Reason::TooFewReadings))
        );
        let short = rate(row(80, Some(5.0), Some(800.0), Some(-0.006)), &Settings::default());
        assert_eq!(
            (short.result.score_pct, short.result.reason),
            (None, Some(Reason::TooShort))
        );
        let empty = rate(row(0, None, None, None), &Settings::default());
        assert_eq!((empty.window_hours, empty.result.avg_current_ma), (0.0, None));
    }

    #[test]
    fn an_implausible_result_is_not_shown_as_a_score() {
        use shared::efficiency::Reason;
        // So sah es beim Dozenten aus: 306 mA, 121 mV pro Tag, 126 Messwerte, zwei Tage. Das ergibt 167 Prozent.
        let rated = rate(
            row(126, Some(48.0), Some(306.0), Some(-121.0 / 86_400.0)),
            &Settings::default(),
        );
        assert_eq!(
            (rated.result.score_pct, rated.result.reason),
            (None, Some(Reason::Implausible))
        );
        assert!(
            (rated.result.voltage_drop_mv_per_day.unwrap() - 121.0).abs() < 1e-6,
            "gemessen bleibt sichtbar"
        );
    }

    #[test]
    fn the_window_never_reaches_before_the_battery_change() {
        let sql = window_start_sql();
        assert!(
            sql.contains("d.battery_installed_at") && sql.contains(EFFICIENCY_WINDOW),
            "{sql}"
        );
    }

    #[test]
    fn search_pattern_escapes_wildcards() {
        assert_eq!(search_pattern(Some(" TN00 ")).as_deref(), Some("%TN00%"));
        assert_eq!(search_pattern(Some("100%_x")).as_deref(), Some("%100\\%\\_x%"));
        assert_eq!(search_pattern(Some("   ")), None);
        assert_eq!(search_pattern(None), None);
    }
}

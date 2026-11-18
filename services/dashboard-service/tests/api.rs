//! Lese-Modelle und Einstellungen gegen eine echte TimescaleDB (Router wird direkt aufgerufen).

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use dashboard_service::{AppState, api::router};
use serde_json::{Value, json};
use shared::db::MIGRATOR;
use sqlx::PgPool;
use tower::ServiceExt;

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    text: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.text).unwrap_or(Value::Null)
    }
}

async fn call(app: &Router, method: Method, uri: &str, body: Option<Value>) -> Reply {
    let mut request = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(v) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let response = app.clone().oneshot(request.body(body).unwrap()).await.unwrap();
    let (parts, body) = response.into_parts();
    Reply {
        status: parts.status,
        headers: parts.headers,
        text: String::from_utf8_lossy(&to_bytes(body, 1 << 24).await.unwrap()).into_owned(),
    }
}

fn app(pool: PgPool) -> Router {
    router(AppState::new(pool, "http://127.0.0.1:1"))
}

async fn get(app: &Router, uri: &str) -> Reply {
    call(app, Method::GET, uri, None).await
}

async fn seed(pool: &PgPool) {
    sqlx::query(
        "insert into devices (id, name, status, soc_percent, last_seen_at, discharge_rate_pct_per_hour, forecast_hours_remaining) values \
         ('TN0001', 'Hauptbahnhof', 'normal', 90, now(), null, null), \
         ('TN0002', 'Schlossplatz', 'warning', 35, now() - interval '5 minutes', 1.5, 23), \
         ('TN0003', 'Killesberg', 'critical', 12, now() - interval '1 minute', null, null), \
         ('TN0004', null, 'offline', 60, now() - interval '3 hours', null, null), \
         ('TN0005', 'Neu', 'normal', null, null, null, null)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("insert into alerts (device_id, kind) values ('TN0003', 'critical'), ('TN0004', 'offline')")
        .execute(pool)
        .await
        .unwrap();
}

fn ids(reply: &Reply) -> Vec<String> {
    reply
        .json()
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn summary_counts_everything(pool: PgPool) {
    seed(&pool).await;
    let reply = get(&app(pool), "/api/summary").await;
    let s = reply.json();
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        (s["total"].as_i64(), s["normal"].as_i64(), s["warning"].as_i64()),
        (Some(5), Some(2), Some(1))
    );
    assert_eq!(
        (s["critical"].as_i64(), s["offline"].as_i64(), s["open_alerts"].as_i64()),
        (Some(1), Some(1), Some(2))
    );
    assert!(
        (s["average_soc_percent"].as_f64().unwrap() - 49.25).abs() < 0.01,
        "Mittel ohne unbekannte Werte"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn summary_of_an_empty_system(pool: PgPool) {
    let s = get(&app(pool), "/api/summary").await.json();
    assert_eq!(
        (s["total"].as_i64(), s["average_soc_percent"].is_null()),
        (Some(0), true)
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn devices_filter_by_status_and_search(pool: PgPool) {
    seed(&pool).await;
    let app = app(pool);
    assert_eq!(
        ids(&get(&app, "/api/devices").await),
        ["TN0001", "TN0002", "TN0003", "TN0004", "TN0005"]
    );
    assert_eq!(ids(&get(&app, "/api/devices?status=critical").await), ["TN0003"]);
    assert_eq!(
        ids(&get(&app, "/api/devices?search=schloss").await),
        ["TN0002"],
        "Suche im Namen, ohne Groß-/Kleinschreibung"
    );
    assert_eq!(
        ids(&get(&app, "/api/devices?search=TN0004").await),
        ["TN0004"],
        "Suche in der Kennung"
    );
    assert_eq!(
        ids(&get(&app, "/api/devices?status=normal&search=neu").await),
        ["TN0005"]
    );
    assert!(
        ids(&get(&app, "/api/devices?search=%25").await).is_empty(),
        "Prozentzeichen ist kein Platzhalter"
    );
    assert_eq!(
        get(&app, "/api/devices?status=quatsch").await.status,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn devices_sort_and_attention_first(pool: PgPool) {
    seed(&pool).await;
    let app = app(pool);
    assert_eq!(
        ids(&get(&app, "/api/devices?sort=soc_percent&dir=asc").await),
        ["TN0003", "TN0002", "TN0004", "TN0001", "TN0005"],
        "unbekannt zuletzt"
    );
    assert_eq!(
        ids(&get(&app, "/api/devices?sort=soc_percent&dir=desc").await),
        ["TN0001", "TN0004", "TN0002", "TN0003", "TN0005"]
    );
    assert_eq!(
        ids(&get(&app, "/api/devices?sort=status").await),
        ["TN0003", "TN0004", "TN0002", "TN0001", "TN0005"]
    );
    assert_eq!(
        ids(&get(&app, "/api/devices?sort=last_seen_at&dir=desc").await),
        ["TN0001", "TN0003", "TN0002", "TN0004", "TN0005"]
    );
    assert_eq!(
        ids(&get(&app, "/api/devices?sort=id&dir=desc").await),
        ["TN0005", "TN0004", "TN0003", "TN0002", "TN0001"]
    );
    assert_eq!(
        ids(&get(&app, "/api/devices?attention_first=true&sort=id").await),
        ["TN0003", "TN0004", "TN0001", "TN0002", "TN0005"]
    );
    assert_eq!(
        get(&app, "/api/devices?sort=drop_table").await.status,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn single_device_and_unknown_device(pool: PgPool) {
    seed(&pool).await;
    let app = app(pool);
    let reply = get(&app, "/api/devices/TN0002").await;
    assert_eq!(reply.json()["name"], "Schlossplatz");
    assert_eq!(get(&app, "/api/devices/NOPE").await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        get(&app, "/api/devices/NOPE/history").await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, "/api/devices/NOPE/export.csv").await.status,
        StatusCode::NOT_FOUND
    );
}

async fn add_readings(pool: &PgPool) {
    // 48 Stunden Verlauf alle 30 Minuten, älteste zuerst bei hohem Ladezustand.
    sqlx::query(
        "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent, lat, lon) \
         select now() - g * interval '30 minutes', 'TN0002', 12000, -300, 15, (35 + 0.75 * g)::real, 48.8, 9.0 \
         from generate_series(0, 95) g",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn history_is_bucketed_per_range_and_carries_the_forecast(pool: PgPool) {
    seed(&pool).await;
    add_readings(&pool).await;
    let app = app(pool);

    let day = get(&app, "/api/devices/TN0002/history?range=24h").await.json();
    let day_points = day["points"].as_array().unwrap();
    assert!(
        (4..=5).contains(&(day_points.len() / 10)),
        "{} Punkte",
        day_points.len()
    );
    assert!(
        (47..=48).contains(&day_points.len()),
        "5-Minuten-Fenster über 30-Minuten-Daten ergeben je Messwert einen Punkt: {}",
        day_points.len()
    );
    assert_eq!(day["range"], "24h");
    assert_eq!(day["forecast"]["hours_to_empty"], 23.0);
    assert_eq!(day["forecast"]["rate_pct_per_hour"], 1.5);
    let times: Vec<&str> = day_points.iter().map(|p| p["time"].as_str().unwrap()).collect();
    assert!(times.windows(2).all(|w| w[0] < w[1]), "aufsteigend sortiert");

    let week = get(&app, "/api/devices/TN0002/history?range=7d").await.json();
    assert!(
        week["points"].as_array().unwrap().len() > day_points.len(),
        "7 Tage enthalten mehr Daten"
    );

    let default = get(&app, "/api/devices/TN0002/history").await.json();
    assert_eq!(default["range"], "24h", "ohne Angabe gilt 24 Stunden");
    assert_eq!(
        get(&app, "/api/devices/TN0002/history?range=1y").await.status,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_whole_history_reaches_back_beyond_30_days_with_a_bounded_number_of_points(pool: PgPool) {
    seed(&pool).await;
    add_readings(&pool).await;
    // 60 Tage älter als alles andere, stündlich: rund 1400 Messwerte, die ungemittelt weit über der Punktzahl lägen.
    sqlx::query(
        "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent) \
         select now() - interval '60 days' + g * interval '1 hour', 'TN0002', 12000, -300, 15, 90 \
         from generate_series(0, 1399) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    let app = app(pool);
    let age_days = |point: &Value| {
        let time = chrono::DateTime::parse_from_rfc3339(point["time"].as_str().unwrap()).unwrap();
        (chrono::Utc::now() - time.with_timezone(&chrono::Utc)).num_hours() as f64 / 24.0
    };

    let month = get(&app, "/api/devices/TN0002/history?range=30d").await.json();
    let month_points = month["points"].as_array().unwrap();
    assert!(
        age_days(&month_points[0]) <= 30.1,
        "30 Tage reichen nicht weiter zurück"
    );

    let all = get(&app, "/api/devices/TN0002/history?range=all").await.json();
    let points = all["points"].as_array().unwrap();
    assert_eq!(all["range"], "all");
    assert!(age_days(&points[0]) > 59.0, "die ältesten Messwerte sind dabei");
    assert!(
        (100..=362).contains(&points.len()),
        "knapp 60 Tage ergeben Fenster von 4 Stunden und höchstens rund 360 Punkte: {}",
        points.len()
    );
    let times: Vec<&str> = points.iter().map(|p| p["time"].as_str().unwrap()).collect();
    assert!(times.windows(2).all(|w| w[0] < w[1]), "aufsteigend sortiert");
    assert_eq!(
        all["forecast"]["hours_to_empty"], 23.0,
        "die Prognose gilt unabhängig vom Zeitraum"
    );

    // Der Export nimmt dasselbe Fenster und benennt die Datei danach.
    let csv = get(&app, "/api/devices/TN0002/export.csv?range=all").await;
    assert_eq!(
        csv.headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"TN0002_all.csv\""
    );
    assert_eq!(csv.text.lines().count(), 1 + 96 + 1400);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_custom_range_returns_just_that_window(pool: PgPool) {
    seed(&pool).await;
    add_readings(&pool).await; // alle 30 Minuten, 48 Stunden lang bis jetzt
    let app = app(pool);
    let stamp = |hours_ago: i64| {
        (chrono::Utc::now() - chrono::Duration::hours(hours_ago)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    };

    // Ein Fenster in der Vergangenheit: nur die Messwerte darin, keine Prognose (die gilt für jetzt).
    let past = get(
        &app,
        &format!(
            "/api/devices/TN0002/history?range=custom&from={}&to={}",
            stamp(40),
            stamp(30)
        ),
    )
    .await
    .json();
    let points = past["points"].as_array().unwrap();
    assert_eq!(past["range"], "custom");
    assert!(
        (19..=21).contains(&points.len()),
        "10 Stunden alle 30 Minuten: {}",
        points.len()
    );
    assert!(
        past["forecast"].is_null(),
        "ein vergangener Zeitraum hat keine Prognose"
    );

    // Ein Fenster, das bis in die Gegenwart reicht, trägt die Prognose.
    let current = get(
        &app,
        &format!(
            "/api/devices/TN0002/history?range=custom&from={}&to={}",
            stamp(10),
            stamp(-1)
        ),
    )
    .await
    .json();
    assert_eq!(current["forecast"]["hours_to_empty"], 23.0);
    assert!(current["points"].as_array().unwrap().len() >= 19);

    // Falsche Eingaben werden erklärt statt still ignoriert. Bei festen Zeiträumen zählen from und to nicht.
    let backwards = get(
        &app,
        &format!(
            "/api/devices/TN0002/history?range=custom&from={}&to={}",
            stamp(1),
            stamp(5)
        ),
    )
    .await;
    assert_eq!(backwards.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(backwards.text.contains("vor dem Ende"));
    let open = get(&app, "/api/devices/TN0002/history?range=custom").await;
    assert_eq!(open.status, StatusCode::UNPROCESSABLE_ENTITY);
    let ignored = get(
        &app,
        &format!(
            "/api/devices/TN0002/history?range=24h&from={}&to={}",
            stamp(5),
            stamp(1)
        ),
    )
    .await;
    assert_eq!(ignored.status, StatusCode::OK);

    // Der Export nimmt dasselbe Fenster, die Datei nennt es.
    let csv = get(
        &app,
        &format!(
            "/api/devices/TN0002/export.csv?range=custom&from={}&to={}",
            stamp(40),
            stamp(30)
        ),
    )
    .await;
    assert_eq!(csv.status, StatusCode::OK);
    assert!(
        (20..=22).contains(&csv.text.lines().count()),
        "Kopfzeile plus rund 20 Messwerte"
    );
    let disposition = csv.headers[header::CONTENT_DISPOSITION].to_str().unwrap().to_owned();
    assert!(
        disposition.starts_with("attachment; filename=\"TN0002_20"),
        "{disposition}"
    );
    assert!(!disposition.contains("custom"));

    // Mitternacht in Ortszeit liegt in UTC am Vortag (hier 22 Uhr): Die Datei nennt trotzdem die gewählten Tage, 1. bis 3.
    let days = get(
        &app,
        "/api/devices/TN0002/export.csv?range=custom&from=2026-10-31T22:00:00Z&to=2026-11-03T22:00:00Z",
    )
    .await;
    assert_eq!(
        days.headers[header::CONTENT_DISPOSITION].to_str().unwrap(),
        "attachment; filename=\"TN0002_2026-11-01_2026-11-03.csv\""
    );
    // Und bei Zonen östlich und westlich davon ebenso (Mitternacht in UTC+9 und UTC-8).
    for (from, to) in [
        ("2026-10-31T15:00:00Z", "2026-11-03T15:00:00Z"),
        ("2026-11-01T08:00:00Z", "2026-11-04T08:00:00Z"),
    ] {
        let response = get(
            &app,
            &format!("/api/devices/TN0002/export.csv?range=custom&from={from}&to={to}"),
        )
        .await;
        assert_eq!(
            response.headers[header::CONTENT_DISPOSITION].to_str().unwrap(),
            "attachment; filename=\"TN0002_2026-11-01_2026-11-03.csv\""
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn history_of_a_device_without_readings_is_empty_for_the_whole_range_too(pool: PgPool) {
    seed(&pool).await;
    let body = get(&app(pool), "/api/devices/TN0001/history?range=all").await.json();
    assert_eq!(body["points"].as_array().unwrap().len(), 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn history_without_forecast_or_data(pool: PgPool) {
    seed(&pool).await;
    let body = get(&app(pool), "/api/devices/TN0001/history").await.json();
    assert!(body["forecast"].is_null());
    assert_eq!(body["points"].as_array().unwrap().len(), 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn csv_export_has_download_headers_and_all_rows(pool: PgPool) {
    seed(&pool).await;
    add_readings(&pool).await;

    let reply = get(&app(pool), "/api/devices/TN0002/export.csv?range=24h").await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.headers[header::CONTENT_TYPE], "text/csv; charset=utf-8");
    assert_eq!(
        reply.headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"TN0002_24h.csv\""
    );
    let lines: Vec<&str> = reply.text.lines().collect();
    assert_eq!(lines[0], dashboard_service::csv::HEADER);
    assert_eq!(lines.len(), 1 + 48, "24 Stunden alle 30 Minuten");
    assert!(lines[1].contains(",TN0002,12000,-300,15,"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn settings_can_be_read_and_changed(pool: PgPool) {
    let app = app(pool.clone());
    let current = get(&app, "/api/settings").await.json();
    assert_eq!(current["threshold_critical_pct"], 20.0);
    assert_eq!(current["offline_timeout_seconds"], 900);

    let mut changed = current.clone();
    changed["threshold_critical_pct"] = json!(25.0);
    changed["threshold_warning_pct"] = json!(50.0);
    changed["offline_timeout_seconds"] = json!(1800);
    let reply = call(&app, Method::PUT, "/api/settings", Some(changed.clone())).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json(), changed);
    assert_eq!(get(&app, "/api/settings").await.json(), changed, "bleibt gespeichert");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn invalid_settings_are_rejected_with_a_german_message(pool: PgPool) {
    let app = app(pool);
    let mut bad = get(&app, "/api/settings").await.json();
    bad["threshold_critical_pct"] = json!(60.0);
    let reply = call(&app, Method::PUT, "/api/settings", Some(bad)).await;
    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        reply.json()["error"]
            .as_str()
            .unwrap()
            .contains("kritische Schwellwert")
    );
    assert_eq!(
        get(&app, "/api/settings").await.json()["threshold_critical_pct"],
        20.0,
        "unverändert"
    );

    let reply = call(
        &app,
        Method::PUT,
        "/api/settings",
        Some(json!({"threshold_critical_pct": 1})),
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "unvollständiger Inhalt");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn devices_filter_by_data_source(pool: PgPool) {
    seed(&pool).await;
    sqlx::query("update devices set data_source = 'simulation' where id in ('TN0002', 'TN0005')")
        .execute(&pool)
        .await
        .unwrap();
    let app = app(pool);

    assert_eq!(
        ids(&get(&app, "/api/devices?source=simulation").await),
        ["TN0002", "TN0005"]
    );
    assert_eq!(
        ids(&get(&app, "/api/devices?source=dozent").await),
        ["TN0001", "TN0003", "TN0004"]
    );
    assert_eq!(get(&app, "/api/devices").await.json()[0]["data_source"], "dozent");
    assert_eq!(
        get(&app, "/api/devices?source=erfunden").await.status,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn trends_cover_the_last_day_before_each_devices_last_reading(pool: PgPool) {
    seed(&pool).await;
    // TN0002: 6 Stunden Verlauf, TN0004: Daten aus der fernen Vergangenheit (24 Stunden vor seiner letzten Messung zählen).
    sqlx::query(
        "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent) \
         select now() - g * interval '1 hour', 'TN0002', 12000, -300, 15, (80 - g)::real from generate_series(0, 5) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent) \
         select now() - interval '90 days' - g * interval '1 hour', 'TN0004', 12000, -300, 15, (60 + g)::real from generate_series(0, 2) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    let app = app(pool);

    let reply = get(&app, "/api/trends").await;

    assert_eq!(reply.status, StatusCode::OK);
    let trends = reply.json();
    let list = trends.as_array().unwrap();
    let by_id = |id: &str| {
        list.iter().find(|t| t["device_id"] == id).unwrap()["points"]
            .as_array()
            .unwrap()
            .len()
    };
    assert!((5..=7).contains(&by_id("TN0002")), "{} Punkte", by_id("TN0002"));
    assert_eq!(
        by_id("TN0004"),
        3,
        "alte Daten zählen relativ zur letzten Messung des Geräts"
    );
    assert!(
        list.iter().all(|t| t["device_id"] != "TN0001"),
        "Geräte ohne Messwerte fehlen"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn anomaly_settings_can_be_read_changed_and_are_validated(pool: PgPool) {
    let app = app(pool);
    let current = get(&app, "/api/anomaly-settings").await.json();
    assert_eq!(current["enabled"], true);
    assert_eq!(current["stagnation_hours"], 6.0);
    assert_eq!(current["drop_mv"], 600);
    assert_eq!(current["fault_min_pct"], 50.0);

    let mut changed = current.clone();
    changed["enabled"] = json!(false);
    changed["stagnation_hours"] = json!(12.0);
    changed["drop_mv"] = json!(800);
    let reply = call(&app, Method::PUT, "/api/anomaly-settings", Some(changed.clone())).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(get(&app, "/api/anomaly-settings").await.json(), changed);

    for (field, value, part) in [
        ("stagnation_hours", json!(0.5), "Stillstand"),
        ("drop_mv", json!(10), "Spannungseinbruch"),
        ("drop_window_minutes", json!(2), "Zeitfenster"),
        ("fault_min_pct", json!(120.0), "zwischen 0 und 100"),
    ] {
        let mut bad = changed.clone();
        bad[field] = value;
        let reply = call(&app, Method::PUT, "/api/anomaly-settings", Some(bad)).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{field}");
        assert!(reply.json()["error"].as_str().unwrap().contains(part), "{field}");
    }
    assert_eq!(get(&app, "/api/anomaly-settings").await.json(), changed, "unverändert");
}

/// Zwei Tage Messwerte im laufenden Zyklus, alle zehn Minuten: 800 mA Entladung, die Spannung fällt um 528 mV
/// pro Tag (genau nach Datenblatt), am ersten Tag ist es kalt, am zweiten mild.
async fn add_cycle(pool: &PgPool, id: &str) {
    sqlx::query(
        "update devices set battery_installed_at = now() - interval '2 days', discharged_ah = 38.4 where id = $1",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent) \
         select now() - g * interval '10 minutes', $1, (11644 + g * 528.0 / 144)::int, -800, \
                case when g >= 144 then 5 else 15 end, 50 \
         from generate_series(0, 287) g",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn fleet_efficiency_rates_every_device_and_reports_the_data_stock(pool: PgPool) {
    seed(&pool).await;
    add_cycle(&pool, "TN0001").await;
    sqlx::query("insert into invalid_messages_log (topic, payload, reason) values ('t', 'p', 'kaputt')")
        .execute(&pool)
        .await
        .unwrap();
    let app = app(pool);

    let reply = get(&app, "/api/efficiency").await;
    assert_eq!(reply.status, StatusCode::OK);
    let body = reply.json();
    assert_eq!(body["capacity_ah"], 80.0);
    assert_eq!(
        body["devices"].as_array().unwrap().len(),
        5,
        "auch Geräte ohne Messwerte stehen in der Liste"
    );
    assert_eq!(body["stock"]["readings_total"], 288);
    assert_eq!(body["stock"]["invalid_messages"], 1);
    assert!(body["stock"]["db_bytes"].as_i64().unwrap() > 0, "{}", body["stock"]);
    assert!(body["stock"]["readings_bytes"].as_i64().unwrap() > 0);
    // Ohne eingestellten Platz zählt der echte: die Datenbank plus der freie Platz auf der Festplatte.
    let quota = body["stock"]["quota_bytes"].as_i64().unwrap();
    if body["stock"]["quota_is_disk"] == true {
        assert!(quota > body["stock"]["db_bytes"].as_i64().unwrap(), "{}", body["stock"]);
    } else {
        assert_eq!(quota % (1024 * 1024), 0);
    }
    assert!(quota > 0);
    assert!(body["stock"]["oldest_reading"].is_string() && body["stock"]["newest_reading"].is_string());

    let rated = &body["devices"][0];
    assert_eq!(
        (rated["device_id"].as_str(), rated["readings"].as_i64()),
        (Some("TN0001"), Some(288))
    );
    assert!((rated["avg_current_ma"].as_f64().unwrap() - 800.0).abs() < 1e-6);
    assert!(
        (rated["voltage_drop_mv_per_day"].as_f64().unwrap() - 528.0).abs() < 2.0,
        "{rated}"
    );
    assert!((rated["score_pct"].as_f64().unwrap() - 100.0).abs() < 1.0, "{rated}");
    assert!((rated["nominal_runtime_hours"].as_f64().unwrap() - 100.0).abs() < 1e-6);
    assert!((rated["window_hours"].as_f64().unwrap() - 47.83).abs() < 0.1);
    assert!((rated["avg_temperature_c"].as_f64().unwrap() - 10.0).abs() < 0.1);

    let silent = &body["devices"][1];
    assert_eq!(silent["readings"], 0);
    assert!(silent["score_pct"].is_null() && silent["avg_current_ma"].is_null());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_efficiency_list_carries_price_and_age_of_the_running_battery(pool: PgPool) {
    seed(&pool).await;
    sqlx::query(
        "update devices set battery_price_cents = 8950, battery_installed_at = now() - interval '10 days'          where id = 'TN0001'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let app = app(pool);
    let body = get(&app, "/api/efficiency").await.json();
    let first = &body["devices"][0];
    assert_eq!(first["price_cents"], 8950);
    assert!(
        (first["battery_hours"].as_f64().unwrap() - 240.0).abs() < 1.0,
        "{first}"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_discarded_messages_are_listed_page_by_page_newest_first(pool: PgPool) {
    for i in 0..25 {
        sqlx::query(
            "insert into invalid_messages_log (received_at, topic, payload, reason)              values (now() - make_interval(mins => $1), 'tn/x', $2, 'kaputt')",
        )
        .bind(i)
        .bind(format!("{}{i}", "x".repeat(500)))
        .execute(&pool)
        .await
        .unwrap();
    }
    let app = app(pool);
    let page = get(&app, "/api/invalid-messages?limit=10&offset=0").await.json();
    assert_eq!(page["total"], 25);
    assert_eq!(page["items"].as_array().unwrap().len(), 10);
    assert_eq!(page["items"][0]["reason"], "kaputt");
    assert!(
        page["items"][0]["payload"].as_str().unwrap().len() <= 400,
        "Inhalt wird gekürzt"
    );
    let last = get(&app, "/api/invalid-messages?limit=10&offset=20").await.json();
    assert_eq!(last["items"].as_array().unwrap().len(), 5);
    let first_time = page["items"][0]["received_at"].as_str().unwrap().to_owned();
    let last_time = last["items"][4]["received_at"].as_str().unwrap().to_owned();
    assert!(first_time > last_time, "neueste zuerst");
    // Ohne Angaben gibt es die erste Seite mit höchstens 20 Einträgen.
    let default = get(&app, "/api/invalid-messages").await.json();
    assert_eq!(default["items"].as_array().unwrap().len(), 20);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_empty_system_has_an_empty_efficiency_report(pool: PgPool) {
    let app = app(pool);
    let body = get(&app, "/api/efficiency").await.json();
    assert_eq!(body["devices"], json!([]));
    assert_eq!(body["stock"]["readings_total"], 0);
    assert!(body["stock"]["oldest_reading"].is_null());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn device_efficiency_splits_by_temperature_and_load_and_lists_cycles(pool: PgPool) {
    seed(&pool).await;
    add_cycle(&pool, "TN0001").await;
    // Ein paar Messungen unter hoher Last mit eingebrochener Spannung.
    sqlx::query(
        "insert into readings (time, device_id, voltage_mv, current_ma, temperature_c, soc_percent) \
         select now() - g * interval '10 minutes' - interval '5 minutes', 'TN0001', 11000, -2400, 15, 50 \
         from generate_series(0, 11) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    // Ein früherer Zyklus genau nach Datenblatt und einer mit halber Kapazität.
    sqlx::query(
        "insert into battery_cycles (device_id, started_at, ended_at, discharged_ah, start_voltage_mv, end_voltage_mv, avg_temperature_c) values \
         ('TN0001', now() - interval '20 days', now() - interval '20 days' + interval '100 hours', 80, 12700, 10500, 12), \
         ('TN0001', now() - interval '10 days', now() - interval '10 days' + interval '50 hours', 40, 12700, 10500, -3)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let app = app(pool);

    let reply = get(&app, "/api/devices/TN0001/efficiency").await;
    assert_eq!(reply.status, StatusCode::OK);
    let body = reply.json();
    assert_eq!(body["current"]["device_id"], "TN0001");

    let bands = body["bands"].as_array().unwrap();
    assert_eq!(bands.len(), 2);
    assert_eq!(
        (bands[0]["from_c"].as_f64(), bands[0]["to_c"].as_f64()),
        (Some(0.0), Some(10.0))
    );
    assert_eq!(
        (bands[1]["from_c"].as_f64(), bands[1]["to_c"].as_f64()),
        (Some(10.0), Some(20.0))
    );
    assert_eq!(bands[0]["readings"], 144);
    assert_eq!(bands[1]["readings"], 156, "die zwölf Messungen unter Last waren mild");
    assert!(bands[0]["avg_voltage_mv"].as_f64().unwrap() > bands[1]["avg_voltage_mv"].as_f64().unwrap());

    let load = &body["load"];
    assert_eq!(load["median_current_ma"], 800.0);
    assert_eq!(
        (load["high_readings"].as_i64(), load["normal_readings"].as_i64()),
        (Some(12), Some(288))
    );
    assert_eq!(load["high_avg_voltage_mv"], 11000.0);
    assert!(load["normal_avg_voltage_mv"].as_f64().unwrap() > 11_900.0);

    let cycles = body["cycles"].as_array().unwrap();
    assert_eq!(cycles.len(), 3);
    assert!(cycles[0]["ended_at"].is_null(), "der laufende Zyklus steht vorn");
    assert_eq!(cycles[0]["discharged_ah"], 38.4);
    assert_eq!(cycles[0]["score_pct"], body["current"]["score_pct"]);
    assert!(
        (cycles[1]["score_pct"].as_f64().unwrap() - 50.0).abs() < 0.01,
        "der jüngere abgeschlossene zuerst"
    );
    assert!((cycles[2]["score_pct"].as_f64().unwrap() - 100.0).abs() < 0.01);
    assert!((cycles[2]["hours"].as_f64().unwrap() - 100.0).abs() < 1e-6);
    assert_eq!(cycles[1]["avg_temperature_c"], -3.0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn device_efficiency_without_readings_and_for_unknown_devices(pool: PgPool) {
    seed(&pool).await;
    let app = app(pool);

    let body = get(&app, "/api/devices/TN0005/efficiency").await.json();
    assert_eq!(body["current"]["readings"], 0);
    assert_eq!(body["bands"], json!([]));
    assert!(body["load"].is_null());
    assert_eq!(
        body["cycles"],
        json!([]),
        "ohne Einbaudatum gibt es keinen laufenden Zyklus"
    );

    assert_eq!(
        get(&app, "/api/devices/GIBTESNICHT/efficiency").await.status,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn retired_devices_leave_the_lists_but_stay_reachable(pool: PgPool) {
    seed(&pool).await;
    sqlx::query(
        "update devices set retired_at = now(), retired_reason = 'replaced', retired_note = 'neu' where id = 'TN0003'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let app = app(pool);
    let active = get(&app, "/api/devices").await;
    assert!(!ids(&active).contains(&"TN0003".to_owned()), "{:?}", ids(&active));
    assert_eq!(ids(&active).len(), 4);
    let retired = get(&app, "/api/devices?retired=only").await;
    assert_eq!(ids(&retired), vec!["TN0003".to_owned()]);
    assert_eq!(retired.json()[0]["retired_reason"], "replaced");
    let summary = get(&app, "/api/summary").await.json();
    assert_eq!(summary["total"], 4);
    assert_eq!(summary["critical"], 0, "das kritische Gerät ist ausgemustert");
    let single = get(&app, "/api/devices/TN0003").await;
    assert_eq!(single.status, StatusCode::OK);
    assert_eq!(single.json()["retired_note"], "neu");
}

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

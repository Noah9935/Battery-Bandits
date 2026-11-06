//! Router und Handler der Dashboard-Schnittstelle.
//!
//! GET /api/summary, /api/devices, /api/devices/{id}, /api/devices/{id}/history,
//! /api/devices/{id}/export.csv, GET und PUT /api/settings, /api/events (SSE),
//! /api/alerts/* und /api/maintenance/* (Proxy), GET /healthz

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use shared::api::{DeviceSort, HistoryResponse, Range, SortDir};
use shared::{DashboardSummary, Device, DeviceStatus, Settings};

use crate::queries::{self, DeviceFilter};
use crate::{AppState, csv, proxy, sse};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/summary", get(summary))
        .route("/api/devices", get(devices))
        .route("/api/devices/{id}", get(device))
        .route("/api/devices/{id}/history", get(history))
        .route("/api/devices/{id}/export.csv", get(export_csv))
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/events", get(sse::events))
        .route("/api/alerts", any(proxy::forward))
        .route("/api/alerts/{*rest}", any(proxy::forward))
        .route("/api/maintenance", any(proxy::forward))
        .route("/api/maintenance/{*rest}", any(proxy::forward))
        .with_state(state)
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0} nicht gefunden")]
    NotFound(&'static str),
    #[error("{0}")]
    Invalid(String),
    #[error("Datenbankfehler: {0}")]
    Db(#[from] sqlx::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self {
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
            ApiError::Db(e) => {
                tracing::error!("Datenbankfehler: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "Interner Fehler"})),
                )
                    .into_response();
            }
        };
        (status, Json(json!({"error": self.to_string()}))).into_response()
    }
}

async fn summary(State(s): State<AppState>) -> Result<Json<DashboardSummary>, ApiError> {
    Ok(Json(queries::summary(&s.pool).await?))
}

#[derive(Debug, Default, Deserialize)]
struct DevicesQuery {
    status: Option<DeviceStatus>,
    source: Option<shared::DataSource>,
    search: Option<String>,
    #[serde(default)]
    sort: DeviceSort,
    #[serde(default)]
    dir: SortDir,
    #[serde(default)]
    attention_first: bool,
}

async fn devices(State(s): State<AppState>, Query(q): Query<DevicesQuery>) -> Result<Json<Vec<Device>>, ApiError> {
    let filter = DeviceFilter {
        status: q.status,
        source: q.source,
        search: q.search,
        sort: q.sort,
        dir: q.dir,
        attention_first: q.attention_first,
    };
    Ok(Json(queries::devices(&s.pool, &filter).await?))
}

async fn device(State(s): State<AppState>, Path(id): Path<String>) -> Result<Json<Device>, ApiError> {
    queries::device(&s.pool, &id)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound("Gerät"))
}

#[derive(Debug, Deserialize)]
struct RangeQuery {
    #[serde(default = "default_range")]
    range: Range,
}

fn default_range() -> Range {
    Range::Day
}

async fn history(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<HistoryResponse>, ApiError> {
    let device = queries::device(&s.pool, &id)
        .await?
        .ok_or(ApiError::NotFound("Gerät"))?;
    let (points, forecast) = queries::history(&s.pool, &device, q.range).await?;
    Ok(Json(HistoryResponse {
        device_id: device.id,
        range: q.range,
        points,
        forecast,
    }))
}

async fn export_csv(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<RangeQuery>,
) -> Result<Response, ApiError> {
    let device = queries::device(&s.pool, &id)
        .await?
        .ok_or(ApiError::NotFound("Gerät"))?;
    let readings = queries::export_readings(&s.pool, &device.id, q.range).await?;
    let disposition = format!(
        "attachment; filename=\"{}\"",
        csv::file_name(&device.id, q.range.as_str())
    );
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        csv::readings_to_csv(&readings),
    )
        .into_response())
}

async fn get_settings(State(s): State<AppState>) -> Result<Json<Settings>, ApiError> {
    Ok(Json(queries::settings(&s.pool).await?))
}

async fn put_settings(State(s): State<AppState>, Json(new): Json<Settings>) -> Result<Json<Settings>, ApiError> {
    new.validate().map_err(|e| ApiError::Invalid(e.to_string()))?;
    Ok(Json(queries::update_settings(&s.pool, &new).await?))
}

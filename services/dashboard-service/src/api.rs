//! Router und Handler der Dashboard-Schnittstelle.
//!
//! GET /api/summary, /api/devices, /api/devices/{id}, /api/devices/{id}/history,
//! /api/devices/{id}/export.csv, GET und PUT /api/settings, /api/events (SSE),
//! /api/users, /api/alerts/*, /api/maintenance/* und /api/activity (Proxy), GET /healthz

use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use shared::api::{DeviceSort, HistoryResponse, Login, Me, Range, SortDir};
use shared::{DashboardSummary, Device, DeviceStatus, Settings};

use crate::auth::{self, CurrentUser};
use crate::queries::{self, DeviceFilter};
use crate::{AppState, csv, proxy, sse};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .route("/api/summary", get(summary))
        .route("/api/devices", get(devices))
        .route("/api/trends", get(trends))
        .route("/api/users", get(users))
        .route("/api/activity", any(proxy::forward))
        .route("/api/devices/{id}", get(device))
        .route("/api/devices/{id}/history", get(history))
        .route("/api/devices/{id}/export.csv", get(export_csv))
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/events", get(sse::events))
        .route("/api/alerts", any(proxy::forward))
        .route("/api/alerts/{*rest}", any(proxy::forward))
        .route("/api/maintenance", any(proxy::forward))
        .route("/api/maintenance/{*rest}", any(proxy::forward))
        .layer(middleware::from_fn_with_state(state.clone(), require_session))
        .with_state(state)
}

/// Ohne gültige Sitzung gibt es nur `/healthz` und die Anmeldung. Ist die Anmeldung abgeschaltet, gilt jede Anfrage als "Gast".
async fn require_session(State(s): State<AppState>, mut req: Request, next: Next) -> Response {
    if matches!(req.uri().path(), "/healthz" | "/api/auth/login" | "/api/auth/logout") {
        return next.run(req).await;
    }
    if !s.auth_required {
        req.extensions_mut().insert(CurrentUser::guest());
        return next.run(req).await;
    }
    let token = session_token(req.headers());
    let user = match token {
        Some(token) => auth::user_for_token(&s.pool, &token).await.ok().flatten(),
        None => None,
    };
    match user {
        Some(user) => {
            req.extensions_mut().insert(user);
            next.run(req).await
        }
        None => (StatusCode::UNAUTHORIZED, Json(json!({"error": "Nicht angemeldet."}))).into_response(),
    }
}

fn session_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    auth::cookie_value(cookies, auth::COOKIE_NAME).map(str::to_owned)
}

fn with_cookie(cookie: String, response: impl IntoResponse) -> Response {
    let mut response = response.into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

async fn login(State(s): State<AppState>, Json(body): Json<Login>) -> Response {
    let username = body.username.trim().to_lowercase();
    let now = std::time::Instant::now();
    if !s.limiter.allowed(&username, now) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "Zu viele Versuche. Bitte eine Minute warten."})),
        )
            .into_response();
    }
    match auth::authenticate(&s.pool, &username, &body.password).await {
        Ok(Some(user)) => {
            s.limiter.clear(&username);
            match auth::create_session(&s.pool, user.id).await {
                Ok(token) => with_cookie(
                    auth::session_cookie(&token, s.cookie_secure),
                    Json(Me {
                        username: user.username,
                        display_name: user.display_name,
                    }),
                ),
                Err(e) => ApiError::Db(e).into_response(),
            }
        }
        Ok(None) => {
            s.limiter.record_failure(&username, now);
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Benutzername oder Passwort falsch."})),
            )
                .into_response()
        }
        Err(e) => ApiError::Db(e).into_response(),
    }
}

async fn logout(State(s): State<AppState>, headers: axum::http::HeaderMap) -> Response {
    if let Some(token) = session_token(&headers) {
        let _ = auth::delete_session(&s.pool, &token).await;
    }
    with_cookie(auth::clear_cookie(s.cookie_secure), Json(json!({"ok": true})))
}

async fn me(axum::Extension(user): axum::Extension<CurrentUser>) -> Json<Me> {
    Json(Me {
        username: user.username,
        display_name: user.display_name,
    })
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

/// Alle Konten (für die Auswahl der zuständigen Person), ohne Geheimnisse.
async fn users(State(s): State<AppState>) -> Result<Json<Vec<Me>>, ApiError> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("select username, display_name from users order by lower(display_name), username")
            .fetch_all(&s.pool)
            .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(username, display_name)| Me { username, display_name })
            .collect(),
    ))
}

async fn trends(State(s): State<AppState>) -> Result<Json<Vec<shared::api::Trend>>, ApiError> {
    Ok(Json(queries::trends(&s.pool).await?))
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

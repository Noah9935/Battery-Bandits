use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;
use shared::api::{DeviceSummary, History, Incident, IncidentState, Range, RejectedMessage, Settings};
use shared::events::{DismissIncident, SetThreshold, subject};
use shared::status::is_valid_threshold;
use tokio_stream::wrappers::BroadcastStream;

use crate::error::{AppError, AppResult};
use crate::queries;
use crate::state::AppState;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

/// Für Docker-Healthchecks und die Überwachung durch den Betrieb.
pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let db = sqlx::query("select 1").execute(&state.pool).await.is_ok();
    let bus = state.bus.is_connected();
    let code = if db && bus {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        code,
        Json(json!({ "status": if db && bus { "ok" } else { "degraded" }, "database": db, "bus": bus })),
    )
}

/// F03, F13: Geräteübersicht. Filtern und Sortieren übernimmt die Web-App,
/// bei rund 500 Geräten ist die ganze Liste klein genug.
pub async fn devices(State(state): State<AppState>) -> AppResult<Json<Vec<DeviceSummary>>> {
    Ok(Json(queries::devices(&state.pool).await?))
}

pub async fn device(State(state): State<AppState>, Path(id): Path<String>) -> AppResult<Json<DeviceSummary>> {
    queries::device(&state.pool, &id)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::NotFound(format!("Gerät {id} ist unbekannt")))
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    range: Option<String>,
}

/// F04: Ladestandsverlauf über einen wählbaren Zeitraum.
pub async fn history(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> AppResult<Json<History>> {
    let raw = query.range.as_deref().unwrap_or("day");
    let range = Range::parse(raw)
        .ok_or_else(|| AppError::BadRequest(format!("Zeitraum {raw} ist ungültig (day, week, month oder year)")))?;
    if queries::device(&state.pool, &id).await?.is_none() {
        return Err(AppError::NotFound(format!("Gerät {id} ist unbekannt")));
    }
    Ok(Json(
        queries::history(&state.pool, &id, range, kit::time::now_ms()).await?,
    ))
}

pub async fn get_settings(State(state): State<AppState>) -> AppResult<Json<Settings>> {
    Ok(Json(queries::settings(&state.pool).await?))
}

/// NF07: Schwellwert ohne Codeänderung ändern. Der Überwachungsdienst führt die Änderung aus.
pub async fn put_settings(State(state): State<AppState>, Json(settings): Json<Settings>) -> AppResult<Json<Settings>> {
    if !is_valid_threshold(settings.critical_threshold_pct) {
        return Err(AppError::BadRequest(
            "Der Schwellwert muss zwischen 1 und 99 Prozent liegen".into(),
        ));
    }
    let reply = state
        .bus
        .request(
            subject::CMD_SET_THRESHOLD,
            &SetThreshold {
                critical_pct: settings.critical_threshold_pct,
            },
            COMMAND_TIMEOUT,
        )
        .await
        .map_err(|e| {
            tracing::warn!("Überwachungsdienst nicht erreichbar: {e:#}");
            AppError::Unavailable("Der Überwachungsdienst antwortet nicht".into())
        })?;
    if !reply.ok {
        return Err(AppError::BadRequest(
            reply.error.unwrap_or_else(|| "Schwellwert abgelehnt".into()),
        ));
    }
    Ok(Json(queries::settings(&state.pool).await?))
}

#[derive(Debug, Deserialize)]
pub struct IncidentsQuery {
    state: Option<String>,
    device_id: Option<String>,
    limit: Option<i64>,
}

pub async fn incidents(
    State(state): State<AppState>,
    Query(query): Query<IncidentsQuery>,
) -> AppResult<Json<Vec<Incident>>> {
    let filter = match query.state.as_deref() {
        None | Some("") | Some("all") => None,
        Some(raw) => Some(IncidentState::from_db(raw).ok_or_else(|| {
            AppError::BadRequest(format!("Status {raw} ist ungültig (open, dismissed oder resolved)"))
        })?),
    };
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);
    Ok(Json(
        queries::incidents(
            &state.pool,
            filter,
            query.device_id.as_deref().filter(|d| !d.is_empty()),
            limit,
        )
        .await?,
    ))
}

/// Ereignis "Meldung abweisen". Der Meldungsdienst führt es aus.
pub async fn dismiss_incident(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<StatusCode> {
    let reply = state
        .bus
        .request(subject::CMD_DISMISS_INCIDENT, &DismissIncident { id }, COMMAND_TIMEOUT)
        .await
        .map_err(|e| {
            tracing::warn!("Meldungsdienst nicht erreichbar: {e:#}");
            AppError::Unavailable("Der Meldungsdienst antwortet nicht".into())
        })?;
    if reply.ok {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::Conflict(
            reply.error.unwrap_or_else(|| "Meldung nicht abgewiesen".into()),
        ))
    }
}

#[derive(Debug, Deserialize)]
pub struct RejectedQuery {
    limit: Option<i64>,
}

/// F06: Protokoll der verworfenen Nachrichten.
pub async fn rejected(
    State(state): State<AppState>,
    Query(query): Query<RejectedQuery>,
) -> AppResult<Json<Vec<RejectedMessage>>> {
    let limit = query.limit.unwrap_or(100).clamp(1, 1000);
    Ok(Json(queries::rejected(&state.pool, limit).await?))
}

/// Server-Sent Events: Der Browser lädt bei jedem Ereignis die betroffenen Daten neu.
pub async fn events(State(state): State<AppState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.live.subscribe()).filter_map(|received| async move {
        // Hinkt ein Browser hinterher, gehen alte Ereignisse verloren. Er lädt ohnehin den aktuellen Stand.
        let event = received.ok()?;
        Event::default().json_data(&event).ok().map(Ok)
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping"))
}

//! REST-Schnittstelle des Meldungs- und Einsatz-Service.
//!
//! GET /alerts, POST /alerts/{id}/dismiss, GET /maintenance, POST /maintenance,
//! POST /maintenance/{id}/assign, POST /maintenance/{id}/complete, GET und POST /maintenance/{id}/comments,
//! GET /activity, GET /healthz
//!
//! Wer handelt, steht in den Kopfzeilen `x-bb-user` und `x-bb-name` (prozentkodiert). Sie setzt der Dashboard-Service
//! aus der Sitzung, dieser Dienst ist nur im internen Netz erreichbar.

use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use shared::api::decode_header;
use shared::{
    ActivityEntry, Alert, AlertStatus, AssignTask, InboxItem, MaintenanceStatus, MaintenanceTask, MarkRead,
    NewMaintenanceTask, NewTaskComment, TaskComment,
};
use sqlx::PgPool;

use crate::repo::{self, Actor, RepoError};

pub fn router(pool: PgPool) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/alerts", get(list_alerts))
        .route("/alerts/{id}/dismiss", post(dismiss_alert))
        .route("/maintenance", get(list_tasks).post(plan_task))
        .route("/maintenance/{id}/assign", post(assign_task))
        .route("/maintenance/{id}/complete", post(complete_task))
        .route("/maintenance/{id}/comments", get(list_comments).post(add_comment))
        .route("/activity", get(list_activity))
        .route("/inbox", get(list_inbox))
        .route("/inbox/read", post(mark_read))
        .with_state(pool)
}

impl IntoResponse for RepoError {
    fn into_response(self) -> Response {
        let status = match &self {
            RepoError::NotFound(_) => StatusCode::NOT_FOUND,
            RepoError::Conflict(_) => StatusCode::CONFLICT,
            RepoError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
            RepoError::Db(e) => {
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

#[derive(Deserialize)]
struct AlertFilter {
    status: Option<AlertStatus>,
}

#[derive(Deserialize)]
struct TaskFilter {
    status: Option<MaintenanceStatus>,
}

async fn list_alerts(State(pool): State<PgPool>, Query(f): Query<AlertFilter>) -> Result<Json<Vec<Alert>>, RepoError> {
    Ok(Json(repo::list_alerts(&pool, f.status).await?))
}

impl<S: Send + Sync> FromRequestParts<S> for Actor {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let get = |name: &str| {
            parts
                .headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(decode_header)
                .filter(|v| !v.trim().is_empty())
        };
        Ok(Actor {
            username: get("x-bb-user"),
            name: get("x-bb-name"),
        })
    }
}

async fn dismiss_alert(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
) -> Result<Json<Alert>, RepoError> {
    Ok(Json(repo::dismiss_alert(&pool, id, &actor).await?))
}

async fn list_tasks(
    State(pool): State<PgPool>,
    Query(f): Query<TaskFilter>,
) -> Result<Json<Vec<MaintenanceTask>>, RepoError> {
    Ok(Json(repo::list_tasks(&pool, f.status).await?))
}

async fn plan_task(
    State(pool): State<PgPool>,
    actor: Actor,
    Json(new): Json<NewMaintenanceTask>,
) -> Result<(StatusCode, Json<MaintenanceTask>), RepoError> {
    Ok((StatusCode::CREATED, Json(repo::plan_task(&pool, &new, &actor).await?)))
}

async fn assign_task(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
    Json(body): Json<AssignTask>,
) -> Result<Json<MaintenanceTask>, RepoError> {
    Ok(Json(
        repo::assign_task(&pool, id, body.assignee.as_deref(), &actor).await?,
    ))
}

async fn complete_task(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
) -> Result<Json<MaintenanceTask>, RepoError> {
    Ok(Json(repo::complete_task(&pool, id, &actor).await?))
}

async fn list_comments(State(pool): State<PgPool>, Path(id): Path<i64>) -> Result<Json<Vec<TaskComment>>, RepoError> {
    Ok(Json(repo::list_comments(&pool, id).await?))
}

async fn add_comment(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
    Json(body): Json<NewTaskComment>,
) -> Result<(StatusCode, Json<TaskComment>), RepoError> {
    Ok((
        StatusCode::CREATED,
        Json(repo::add_comment(&pool, id, &body.body, &actor).await?),
    ))
}

#[derive(Deserialize)]
struct ActivityFilter {
    device: Option<String>,
    limit: Option<i64>,
}

async fn list_activity(
    State(pool): State<PgPool>,
    Query(f): Query<ActivityFilter>,
) -> Result<Json<Vec<ActivityEntry>>, RepoError> {
    Ok(Json(
        repo::list_activity(&pool, f.device.as_deref(), f.limit.unwrap_or(50)).await?,
    ))
}

async fn list_inbox(
    State(pool): State<PgPool>,
    actor: Actor,
    Query(f): Query<ActivityFilter>,
) -> Result<Json<Vec<InboxItem>>, RepoError> {
    Ok(Json(
        repo::list_inbox(&pool, actor.username.as_deref(), f.limit.unwrap_or(50)).await?,
    ))
}

async fn mark_read(
    State(pool): State<PgPool>,
    actor: Actor,
    Json(body): Json<MarkRead>,
) -> Result<Json<serde_json::Value>, RepoError> {
    let marked = repo::mark_read(&pool, actor.username.as_deref(), body.ids.as_deref()).await?;
    Ok(Json(json!({"marked": marked})))
}

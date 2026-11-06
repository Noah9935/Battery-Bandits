//! REST-Schnittstelle des Meldungs- und Einsatz-Service.
//!
//! GET /alerts, POST /alerts/{id}/dismiss, GET /maintenance, POST /maintenance,
//! POST /maintenance/{id}/complete, GET /healthz

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use shared::{Alert, AlertStatus, MaintenanceStatus, MaintenanceTask, NewMaintenanceTask};
use sqlx::PgPool;

use crate::repo::{self, RepoError};

pub fn router(pool: PgPool) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/alerts", get(list_alerts))
        .route("/alerts/{id}/dismiss", post(dismiss_alert))
        .route("/maintenance", get(list_tasks).post(plan_task))
        .route("/maintenance/{id}/complete", post(complete_task))
        .with_state(pool)
}

impl IntoResponse for RepoError {
    fn into_response(self) -> Response {
        let status = match &self {
            RepoError::NotFound(_) => StatusCode::NOT_FOUND,
            RepoError::Conflict(_) => StatusCode::CONFLICT,
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

async fn dismiss_alert(State(pool): State<PgPool>, Path(id): Path<i64>) -> Result<Json<Alert>, RepoError> {
    Ok(Json(repo::dismiss_alert(&pool, id).await?))
}

async fn list_tasks(
    State(pool): State<PgPool>,
    Query(f): Query<TaskFilter>,
) -> Result<Json<Vec<MaintenanceTask>>, RepoError> {
    Ok(Json(repo::list_tasks(&pool, f.status).await?))
}

async fn plan_task(
    State(pool): State<PgPool>,
    Json(new): Json<NewMaintenanceTask>,
) -> Result<(StatusCode, Json<MaintenanceTask>), RepoError> {
    Ok((StatusCode::CREATED, Json(repo::plan_task(&pool, &new).await?)))
}

async fn complete_task(State(pool): State<PgPool>, Path(id): Path<i64>) -> Result<Json<MaintenanceTask>, RepoError> {
    Ok(Json(repo::complete_task(&pool, id).await?))
}

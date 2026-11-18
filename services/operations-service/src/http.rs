//! REST-Schnittstelle des Meldungs- und Einsatz-Service.
//!
//! GET /alerts, POST /alerts/{id}/dismiss, GET /maintenance, POST /maintenance,
//! POST /maintenance/{id}/assign, POST /maintenance/{id}/complete, GET und POST /maintenance/{id}/comments,
//! GET /subscriptions, PUT und DELETE /subscriptions/{device_id}, POST /maintenance/battery-swap,
//! POST und DELETE /devices/{id}/retire (Gerät ausmustern und wieder aufnehmen), GET /activity, GET und POST /inbox, GET und PUT /notifications/rule,
//! GET /notifications/status, POST /notifications/test, GET /healthz
//!
//! Wer handelt, steht in den Kopfzeilen `x-bb-user` und `x-bb-name` (prozentkodiert). Sie setzt der Dashboard-Service
//! aus der Sitzung, dieser Dienst ist nur im internen Netz erreichbar.

use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use shared::api::decode_header;
use shared::notify::{MailStatus, NotificationRule};
use shared::{
    ActivityEntry, Alert, AlertStatus, AssignTask, BatterySwap, InboxItem, MaintenanceStatus, MaintenanceTask,
    MarkRead, NewMaintenanceTask, NewTaskComment, TaskComment,
};
use sqlx::PgPool;

use crate::notify::{Delivery, Notifier};
use crate::repo::{self, Actor, RepoError};

/// Zustand der Handler: Datenbank und Versand der Weiterleitungen.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub notifier: Notifier,
}

impl axum::extract::FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}

impl axum::extract::FromRef<AppState> for Notifier {
    fn from_ref(state: &AppState) -> Self {
        state.notifier.clone()
    }
}

/// Router ohne eingerichtete Dienste für die Weiterleitung (Tests, Entwicklung).
pub fn router(pool: PgPool) -> Router {
    router_with(pool, Notifier::disabled())
}

pub fn router_with(pool: PgPool, notifier: Notifier) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/alerts", get(list_alerts))
        .route("/alerts/{id}/dismiss", post(dismiss_alert))
        .route("/maintenance", get(list_tasks).post(plan_task))
        .route("/maintenance/battery-swap", post(swap_battery))
        .route("/battery-types", get(list_battery_types).post(create_battery_type))
        .route("/battery-types/trash", get(trashed_battery_types))
        .route(
            "/battery-types/{id}",
            put(update_battery_type).delete(archive_battery_type),
        )
        .route("/battery-types/{id}/restore", post(restore_battery_type))
        .route("/battery-types/{id}/purge", delete(purge_battery_type))
        .route("/devices/{id}/retire", post(retire_device).delete(reactivate_device))
        .route("/devices/{id}/battery-price", put(set_device_battery_price))
        .route("/battery-cycles/{id}/price", put(set_cycle_price))
        .route("/maintenance/{id}/assign", post(assign_task))
        .route("/maintenance/{id}/complete", post(complete_task))
        .route("/maintenance/{id}/undo", post(undo_task))
        .route("/maintenance/{id}/comments", get(list_comments).post(add_comment))
        .route("/activity", get(list_activity))
        .route("/inbox", get(list_inbox))
        .route("/inbox/read", post(mark_read))
        .route("/pins", get(list_pins).post(add_pins).delete(clear_pins))
        .route("/pins/{device_id}", put(pin).delete(unpin))
        .route("/tours", get(list_tours).post(create_tour))
        .route("/tours/trash", get(trashed_tours))
        .route("/tours/{id}", put(update_tour).delete(delete_tour))
        .route("/tours/{id}/restore", post(restore_tour))
        .route("/tours/{id}/purge", delete(purge_tour))
        .route("/tours/{id}/shares", put(share_tour).delete(leave_tour))
        .route("/subscriptions", get(list_subscriptions))
        .route("/subscriptions/{device_id}", put(subscribe).delete(unsubscribe))
        .route("/notifications/rule", get(get_rule).put(put_rule))
        .route("/notifications/status", get(mail_status))
        .route("/notifications/test", post(test_rule))
        .with_state(AppState { pool, notifier })
}

impl IntoResponse for RepoError {
    fn into_response(self) -> Response {
        let status = match &self {
            RepoError::NotFound(_) => StatusCode::NOT_FOUND,
            RepoError::Conflict(_) => StatusCode::CONFLICT,
            RepoError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
            RepoError::Forbidden(_) => StatusCode::FORBIDDEN,
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

async fn swap_battery(
    State(pool): State<PgPool>,
    actor: Actor,
    Json(swap): Json<BatterySwap>,
) -> Result<(StatusCode, Json<MaintenanceTask>), RepoError> {
    Ok((
        StatusCode::CREATED,
        Json(repo::swap_battery(&pool, &swap, &actor).await?),
    ))
}

async fn list_battery_types(State(pool): State<PgPool>) -> Result<Json<Vec<shared::BatteryType>>, RepoError> {
    Ok(Json(repo::list_battery_types(&pool).await?))
}

async fn create_battery_type(
    State(pool): State<PgPool>,
    actor: Actor,
    Json(new): Json<shared::NewBatteryType>,
) -> Result<(StatusCode, Json<shared::BatteryType>), RepoError> {
    Ok((
        StatusCode::CREATED,
        Json(repo::create_battery_type(&pool, &new, &actor).await?),
    ))
}

async fn update_battery_type(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
    Json(new): Json<shared::NewBatteryType>,
) -> Result<Json<shared::BatteryType>, RepoError> {
    Ok(Json(repo::update_battery_type(&pool, id, &new, &actor).await?))
}

async fn archive_battery_type(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::archive_battery_type(&pool, id, &actor).await?;
    Ok(Json(json!({"archived": true})))
}

async fn trashed_battery_types(State(pool): State<PgPool>) -> Result<Json<Vec<shared::TrashedBattery>>, RepoError> {
    Ok(Json(repo::trashed_battery_types(&pool).await?))
}

async fn restore_battery_type(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::restore_battery_type(&pool, id, &actor).await?;
    Ok(Json(json!({"restored": true})))
}

async fn purge_battery_type(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::purge_battery_type(&pool, id).await?;
    Ok(Json(json!({"purged": true})))
}

async fn retire_device(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<String>,
    Json(request): Json<shared::RetireDevice>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::retire_device(&pool, &id, &request, &actor).await?;
    Ok(Json(json!({"retired": true})))
}

async fn set_device_battery_price(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<String>,
    Json(request): Json<shared::SetPrice>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::set_device_battery_price(&pool, &id, &request, &actor).await?;
    Ok(Json(json!({"price_cents": request.price_cents})))
}

async fn set_cycle_price(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
    Json(request): Json<shared::SetPrice>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::set_cycle_price(&pool, id, &request, &actor).await?;
    Ok(Json(json!({"price_cents": request.price_cents})))
}

async fn reactivate_device(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::reactivate_device(&pool, &id, &actor).await?;
    Ok(Json(json!({"retired": false})))
}

/// Die Regel gehört immer der Person aus der Sitzung. Ohne Namen (Anmeldung abgeschaltet) gibt es keine.
fn rule_owner(actor: &Actor) -> Result<&str, RepoError> {
    actor.username.as_deref().ok_or(RepoError::Invalid(
        "Weiterleitungen gibt es nur für angemeldete Personen",
    ))
}

async fn get_rule(State(pool): State<PgPool>, actor: Actor) -> Result<Json<NotificationRule>, RepoError> {
    Ok(Json(repo::notification_rule(&pool, rule_owner(&actor)?).await?))
}

async fn put_rule(
    State(pool): State<PgPool>,
    actor: Actor,
    Json(rule): Json<NotificationRule>,
) -> Result<Response, RepoError> {
    let owner = rule_owner(&actor)?;
    if let Err(message) = rule.validate() {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"error": message}))).into_response());
    }
    Ok(Json(repo::save_notification_rule(&pool, owner, &rule).await?).into_response())
}

/// Die Nodes, bei denen sich die Person eingetragen hat.
async fn list_pins(State(pool): State<PgPool>, actor: Actor) -> Result<Json<Vec<String>>, RepoError> {
    Ok(Json(repo::pins(&pool, actor.username.as_deref()).await?))
}

async fn add_pins(
    State(pool): State<PgPool>,
    actor: Actor,
    Json(ids): Json<shared::PinIds>,
) -> Result<Json<Vec<String>>, RepoError> {
    repo::add_pins(&pool, actor.username.as_deref(), &ids.ids).await?;
    Ok(Json(repo::pins(&pool, actor.username.as_deref()).await?))
}

async fn pin(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::add_pins(&pool, actor.username.as_deref(), &[device_id]).await?;
    Ok(Json(json!({"pinned": true})))
}

async fn unpin(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::remove_pin(&pool, actor.username.as_deref(), &device_id).await?;
    Ok(Json(json!({"pinned": false})))
}

async fn clear_pins(State(pool): State<PgPool>, actor: Actor) -> Result<Json<serde_json::Value>, RepoError> {
    repo::clear_pins(&pool, actor.username.as_deref()).await?;
    Ok(Json(json!({"cleared": true})))
}

async fn list_tours(State(pool): State<PgPool>, actor: Actor) -> Result<Json<Vec<shared::Tour>>, RepoError> {
    Ok(Json(repo::list_tours(&pool, actor.username.as_deref()).await?))
}

async fn create_tour(
    State(pool): State<PgPool>,
    actor: Actor,
    Json(new): Json<shared::NewTour>,
) -> Result<(StatusCode, Json<shared::Tour>), RepoError> {
    Ok((StatusCode::CREATED, Json(repo::create_tour(&pool, &actor, &new).await?)))
}

async fn update_tour(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
    Json(update): Json<shared::TourUpdate>,
) -> Result<Json<shared::Tour>, RepoError> {
    Ok(Json(repo::update_tour(&pool, &actor, id, &update).await?))
}

async fn delete_tour(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::delete_tour(&pool, actor.username.as_deref(), id).await?;
    Ok(Json(json!({"deleted": true})))
}

async fn trashed_tours(State(pool): State<PgPool>, actor: Actor) -> Result<Json<Vec<shared::TrashedTour>>, RepoError> {
    Ok(Json(repo::trashed_tours(&pool, actor.username.as_deref()).await?))
}

async fn restore_tour(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::restore_tour(&pool, actor.username.as_deref(), id).await?;
    Ok(Json(json!({"restored": true})))
}

async fn purge_tour(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::purge_tour(&pool, actor.username.as_deref(), id).await?;
    Ok(Json(json!({"purged": true})))
}

async fn share_tour(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
    Json(share): Json<shared::ShareTour>,
) -> Result<Json<shared::Tour>, RepoError> {
    Ok(Json(repo::share_tour(&pool, &actor, id, &share.usernames).await?))
}

/// Eine mit einem geteilte Tour aus der eigenen Liste nehmen.
async fn leave_tour(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::leave_tour(&pool, actor.username.as_deref(), id).await?;
    Ok(Json(json!({"left": true})))
}

async fn list_subscriptions(State(pool): State<PgPool>, actor: Actor) -> Result<Json<Vec<String>>, RepoError> {
    Ok(Json(repo::subscriptions(&pool, actor.username.as_deref()).await?))
}

async fn subscribe(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::subscribe(&pool, actor.username.as_deref(), &device_id).await?;
    Ok(Json(json!({"subscribed": true})))
}

async fn unsubscribe(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, RepoError> {
    repo::unsubscribe(&pool, actor.username.as_deref(), &device_id).await?;
    Ok(Json(json!({"subscribed": false})))
}

/// Kann der Server E-Mails senden?
async fn mail_status(State(notifier): State<Notifier>) -> Json<MailStatus> {
    Json(notifier.status())
}

/// Sendet eine Probemail an die Adresse der gespeicherten Regel.
async fn test_rule(
    State(pool): State<PgPool>,
    State(notifier): State<Notifier>,
    actor: Actor,
) -> Result<Json<Delivery>, RepoError> {
    let owner = rule_owner(&actor)?;
    let rule = repo::notification_rule(&pool, owner).await?;
    Ok(Json(notifier.send_test(&pool, owner, &rule).await))
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

async fn undo_task(
    State(pool): State<PgPool>,
    actor: Actor,
    Path(id): Path<i64>,
) -> Result<Json<MaintenanceTask>, RepoError> {
    Ok(Json(repo::undo_task(&pool, id, &actor).await?))
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

//! Aufrufe der Dashboard-Schnittstelle (`/api/...`). Fehlertexte sind deutsch und für Menschen gedacht.

use gloo_net::http::{Request, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use shared::api::{HistoryResponse, Login, Me, Range};
use shared::{
    Alert, AnomalySettings, AssignTask, DashboardSummary, Device, InboxItem, MaintenanceTask, MarkRead,
    NewMaintenanceTask, NewTaskComment, Settings, TaskComment,
};

pub type ApiResult<T> = Result<T, String>;

const UNREACHABLE: &str = "Der Server ist gerade nicht erreichbar.";

/// Fehlertext aus einer Fehlerantwort (`{"error": "..."}`) oder ein allgemeiner Hinweis.
async fn error_message(response: Response) -> String {
    let status = response.status();
    if let Ok(body) = response.json::<serde_json::Value>().await
        && let Some(text) = body.get("error").and_then(|e| e.as_str())
    {
        return text.to_owned();
    }
    match status {
        404 => "Nicht gefunden.".to_owned(),
        502..=504 => UNREACHABLE.to_owned(),
        _ => format!("Unerwartete Antwort des Servers ({status})."),
    }
}

async fn read<T: DeserializeOwned>(response: Response) -> ApiResult<T> {
    if !response.ok() {
        return Err(error_message(response).await);
    }
    response
        .json()
        .await
        .map_err(|_| "Die Antwort des Servers war nicht lesbar.".to_owned())
}

async fn get<T: DeserializeOwned>(path: &str) -> ApiResult<T> {
    read(Request::get(path).send().await.map_err(|_| UNREACHABLE.to_owned())?).await
}

async fn post<T: DeserializeOwned>(path: &str) -> ApiResult<T> {
    read(Request::post(path).send().await.map_err(|_| UNREACHABLE.to_owned())?).await
}

async fn send_json<B: Serialize, T: DeserializeOwned>(
    request: gloo_net::http::RequestBuilder,
    body: &B,
) -> ApiResult<T> {
    let request = request
        .json(body)
        .map_err(|_| "Die Eingabe konnte nicht gesendet werden.".to_owned())?;
    read(request.send().await.map_err(|_| UNREACHABLE.to_owned())?).await
}

/// Antwort des Servers ohne gültige Sitzung (siehe Dashboard-Service).
pub const UNAUTHORIZED: &str = "Nicht angemeldet.";

pub async fn me() -> ApiResult<Me> {
    get("/api/auth/me").await
}

pub async fn login(username: &str, password: &str) -> ApiResult<Me> {
    let body = Login {
        username: username.to_owned(),
        password: password.to_owned(),
    };
    send_json(Request::post("/api/auth/login"), &body).await
}

pub async fn logout() -> ApiResult<serde_json::Value> {
    post("/api/auth/logout").await
}

pub async fn summary() -> ApiResult<DashboardSummary> {
    get("/api/summary").await
}

pub async fn devices() -> ApiResult<Vec<Device>> {
    get("/api/devices").await
}

pub async fn trends() -> ApiResult<Vec<shared::api::Trend>> {
    get("/api/trends").await
}

pub async fn history(device_id: &str, range: Range) -> ApiResult<HistoryResponse> {
    get(&format!("/api/devices/{device_id}/history?range={}", range.as_str())).await
}

pub async fn open_alerts() -> ApiResult<Vec<Alert>> {
    get("/api/alerts?status=open").await
}

pub async fn dismiss_alert(id: i64) -> ApiResult<Alert> {
    post(&format!("/api/alerts/{id}/dismiss")).await
}

pub async fn tasks() -> ApiResult<Vec<MaintenanceTask>> {
    get("/api/maintenance").await
}

pub async fn plan_task(new: &NewMaintenanceTask) -> ApiResult<MaintenanceTask> {
    send_json(Request::post("/api/maintenance"), new).await
}

pub async fn inbox() -> ApiResult<Vec<InboxItem>> {
    get("/api/inbox").await
}

pub async fn mark_read(ids: Option<Vec<i64>>) -> ApiResult<serde_json::Value> {
    send_json(Request::post("/api/inbox/read"), &MarkRead { ids }).await
}

pub async fn anomaly_settings() -> ApiResult<AnomalySettings> {
    get("/api/anomaly-settings").await
}

pub async fn save_anomaly_settings(settings: &AnomalySettings) -> ApiResult<AnomalySettings> {
    send_json(Request::put("/api/anomaly-settings"), settings).await
}

pub async fn users() -> ApiResult<Vec<Me>> {
    get("/api/users").await
}

pub async fn assign_task(id: i64, assignee: Option<String>) -> ApiResult<MaintenanceTask> {
    send_json(
        Request::post(&format!("/api/maintenance/{id}/assign")),
        &AssignTask { assignee },
    )
    .await
}

pub async fn comments(id: i64) -> ApiResult<Vec<TaskComment>> {
    get(&format!("/api/maintenance/{id}/comments")).await
}

pub async fn add_comment(id: i64, body: &str) -> ApiResult<TaskComment> {
    let body = NewTaskComment { body: body.to_owned() };
    send_json(Request::post(&format!("/api/maintenance/{id}/comments")), &body).await
}

pub async fn complete_task(id: i64) -> ApiResult<MaintenanceTask> {
    post(&format!("/api/maintenance/{id}/complete")).await
}

pub async fn settings() -> ApiResult<Settings> {
    get("/api/settings").await
}

pub async fn save_settings(settings: &Settings) -> ApiResult<Settings> {
    send_json(Request::put("/api/settings"), settings).await
}

/// Adresse des CSV-Downloads.
pub fn export_url(device_id: &str, range: Range) -> String {
    format!("/api/devices/{device_id}/export.csv?range={}", range.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_url_contains_device_and_range() {
        assert_eq!(
            export_url("TN0001", Range::Week),
            "/api/devices/TN0001/export.csv?range=7d"
        );
    }
}

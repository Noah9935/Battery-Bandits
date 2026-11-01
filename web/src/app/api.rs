//! Aufrufe der REST-API. Die Typen kommen aus `shared`, sind also dieselben wie im Server.

use gloo_net::http::{Request, Response};
use serde::de::DeserializeOwned;
use shared::api::{ApiError, DeviceSummary, History, Incident, Range, RejectedMessage, Settings};

type ApiResult<T> = Result<T, String>;

async fn failure(response: Response) -> String {
    match response.json::<ApiError>().await {
        Ok(body) => body.error,
        Err(_) => format!("Serverfehler {}", response.status()),
    }
}

async fn get<T: DeserializeOwned>(url: &str) -> ApiResult<T> {
    let response = Request::get(url)
        .send()
        .await
        .map_err(|e| format!("Keine Verbindung zum Server: {e}"))?;
    if !response.ok() {
        return Err(failure(response).await);
    }
    response
        .json::<T>()
        .await
        .map_err(|e| format!("Unerwartete Antwort: {e}"))
}

pub async fn devices() -> ApiResult<Vec<DeviceSummary>> {
    get("/api/devices").await
}

pub async fn device(id: &str) -> ApiResult<DeviceSummary> {
    get(&format!("/api/devices/{id}")).await
}

pub async fn history(id: &str, range: Range) -> ApiResult<History> {
    get(&format!("/api/devices/{id}/history?range={}", range.as_str())).await
}

pub async fn settings() -> ApiResult<Settings> {
    get("/api/settings").await
}

pub async fn incidents(state: &str, device_id: Option<&str>) -> ApiResult<Vec<Incident>> {
    match device_id {
        Some(id) => get(&format!("/api/incidents?state={state}&device_id={id}")).await,
        None => get(&format!("/api/incidents?state={state}")).await,
    }
}

pub async fn rejected() -> ApiResult<Vec<RejectedMessage>> {
    get("/api/rejected?limit=100").await
}

pub async fn save_settings(settings: &Settings) -> ApiResult<Settings> {
    let response = Request::put("/api/settings")
        .json(settings)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| format!("Keine Verbindung zum Server: {e}"))?;
    if !response.ok() {
        return Err(failure(response).await);
    }
    response
        .json::<Settings>()
        .await
        .map_err(|e| format!("Unerwartete Antwort: {e}"))
}

pub async fn dismiss_incident(id: i64) -> ApiResult<()> {
    let response = Request::post(&format!("/api/incidents/{id}/dismiss"))
        .send()
        .await
        .map_err(|e| format!("Keine Verbindung zum Server: {e}"))?;
    if response.ok() {
        Ok(())
    } else {
        Err(failure(response).await)
    }
}

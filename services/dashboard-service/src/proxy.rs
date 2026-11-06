//! Weiterleitung von `/api/alerts/*` und `/api/maintenance/*` an den Meldungs- und Einsatz-Service.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderValue, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

use crate::AppState;

/// Zieladresse beim Meldungs- und Einsatz-Service: `/api` wird entfernt, die Abfrage bleibt.
pub fn upstream_url(base: &str, uri: &Uri) -> String {
    let path = uri.path().strip_prefix("/api").unwrap_or(uri.path());
    match uri.query() {
        Some(query) => format!("{base}{path}?{query}"),
        None => format!("{base}{path}"),
    }
}

pub async fn forward(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    let url = upstream_url(&state.operations_url, &uri);
    let mut request = state.http.request(method, &url).body(body);
    if let Some(content_type) = headers.get(header::CONTENT_TYPE) {
        request = request.header(header::CONTENT_TYPE, content_type);
    }
    let upstream = match request.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(%url, "Meldungs- und Einsatz-Service nicht erreichbar: {e}");
            return (
                StatusCode::BAD_GATEWAY,
                axum::Json(
                    serde_json::json!({"error": "Der Meldungs- und Einsatz-Service ist gerade nicht erreichbar"}),
                ),
            )
                .into_response();
        }
    };
    let status = upstream.status();
    let content_type = upstream
        .headers()
        .get(header::CONTENT_TYPE)
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("application/json"));
    match upstream.bytes().await {
        Ok(bytes) => (status, [(header::CONTENT_TYPE, content_type)], bytes).into_response(),
        Err(e) => {
            tracing::warn!(%url, "Antwort unvollständig: {e}");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(path: &str) -> String {
        upstream_url("http://ops:8082", &path.parse().unwrap())
    }

    #[test]
    fn strips_the_api_prefix() {
        assert_eq!(url("/api/alerts"), "http://ops:8082/alerts");
        assert_eq!(
            url("/api/maintenance/7/complete"),
            "http://ops:8082/maintenance/7/complete"
        );
    }

    #[test]
    fn keeps_the_query() {
        assert_eq!(url("/api/alerts?status=open"), "http://ops:8082/alerts?status=open");
    }
}

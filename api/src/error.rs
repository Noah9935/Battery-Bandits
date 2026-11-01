use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use shared::api::ApiError;

/// Fehler der API. Interne Details gehen ins Protokoll, nicht zum Client.
#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Conflict(String),
    /// Ein anderer Dienst antwortet nicht
    Unavailable(String),
    Internal(anyhow::Error),
}

impl AppError {
    pub fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn message(&self) -> String {
        match self {
            AppError::NotFound(m) | AppError::BadRequest(m) | AppError::Conflict(m) | AppError::Unavailable(m) => {
                m.clone()
            }
            AppError::Internal(_) => "Interner Fehler".to_owned(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let AppError::Internal(e) = &self {
            tracing::error!("Interner Fehler: {e:#}");
        }
        (self.status(), Json(ApiError { error: self.message() })).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        AppError::Internal(e.into())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_codes() {
        assert_eq!(AppError::NotFound("x".into()).status(), StatusCode::NOT_FOUND);
        assert_eq!(AppError::BadRequest("x".into()).status(), StatusCode::BAD_REQUEST);
        assert_eq!(AppError::Conflict("x".into()).status(), StatusCode::CONFLICT);
        assert_eq!(
            AppError::Unavailable("x".into()).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn internal_errors_hide_details() {
        let e = AppError::Internal(anyhow::anyhow!("Passwort in der Verbindung"));
        assert_eq!(e.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(e.message(), "Interner Fehler");
    }
}

//! Einheitlicher Fehler-Envelope (CLAUDE.md §7):
//! `{ "code": string, "message": string, "details"?: any }`.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("nicht authentifiziert")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("nicht gefunden")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("Datei wurde zwischenzeitlich geändert")]
    VersionConflict,
    #[error("interner Fehler")]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }

    fn parts(&self) -> (StatusCode, &'static str) {
        match self {
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            Self::VersionConflict => (StatusCode::CONFLICT, "version_conflict"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = self.parts();
        // Interne Fehler nicht ans Frontend durchreichen — nur loggen.
        if let Self::Internal(ref e) = self {
            tracing::error!(error = %e, "internal error");
        }
        let message = match &self {
            Self::Internal(_) => "Interner Serverfehler".to_string(),
            other => other.to_string(),
        };
        (status, Json(json!({ "code": code, "message": message }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

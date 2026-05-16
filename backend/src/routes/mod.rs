//! HTTP-Router unter `/api/v1`. Phase 2: Health + Auth (Magic-Link) +
//! Org-Invite-Code-Rotation. Workspaces/Agenten/Dateien/Chat folgen in
//! Phase 3–6.

pub mod auth;

use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;

use crate::AppState;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route(
            "/orgs/{id}/rotate-invite-code",
            post(auth::rotate_invite_code),
        )
        .merge(auth::router())
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

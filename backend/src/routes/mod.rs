//! HTTP-Router unter `/api/v1`. Phase 2: Auth. Phase 3: Workspaces.
//! Phase 4: Agenten/Settings/Keys. Phase 5: Dateien. Phase 6a: Chat
//! (Streaming) + WebSocket-Hub.

pub mod agents;
pub mod auth;
pub mod chat;
pub mod files;
pub mod secrets;
pub mod settings;
pub mod workspaces;

use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;

use crate::AppState;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/skills", get(skills))
        .route("/ws", get(crate::ws::ws_handler))
        .route(
            "/orgs/{id}/rotate-invite-code",
            post(auth::rotate_invite_code),
        )
        .merge(auth::router())
        .merge(workspaces::router())
        .merge(agents::router())
        .merge(settings::router())
        .merge(secrets::router())
        .merge(files::router())
        .merge(chat::router())
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

// Skills werden ab Phase 6 mit dem Backend gebündelt (read-only). Bis dahin
// leere Liste, damit der Agent-Editor sauber lädt statt 404.
async fn skills() -> Json<serde_json::Value> {
    Json(json!([]))
}

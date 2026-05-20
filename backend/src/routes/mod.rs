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

use axum::extract::State;
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

// Im Image eingebundene Skill-Registry, read-only (Phase 6c-2). Liest
// `SKILL.md`-Frontmatter direkt aus dem `SkillRegistry`-Cache, sortiert
// deterministisch nach `name` — gleicher JSON-Shape wie zuvor, damit das
// Frontend unverändert weiterläuft.
async fn skills(State(state): State<AppState>) -> Json<serde_json::Value> {
    let items: Vec<serde_json::Value> = state
        .skills
        .list()
        .iter()
        .map(|s| serde_json::to_value(s.as_ref()).unwrap_or(serde_json::Value::Null))
        .collect();
    Json(serde_json::Value::Array(items))
}

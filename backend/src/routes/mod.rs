//! HTTP-Router. In Phase 1 nur `/api/v1/health` — die Feature-Endpunkte
//! (auth, workspaces, agents, files, chat …) kommen in den Phasen 2–6.

use axum::{routing::get, Json, Router};
use serde_json::json;

use crate::AppState;

pub fn api_router() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

//! ProcessFox Web — Axum-Backend (Phase 1: Skeleton).
//!
//! Der Server liefert das gebaute Frontend als statische Dateien unter `/`
//! aus (SPA-Fallback auf `index.html`) und die API unter `/api/v1/`
//! (CLAUDE.md §7/§12).

pub mod auth;
pub mod config;
pub mod crypto;
pub mod db;
pub mod error;
pub mod llm;
pub mod perm;
pub mod preview;
pub mod ratelimit;
pub mod routes;
pub mod sandbox;
pub mod skills;
pub mod storage;
pub mod tools;
pub mod ws;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use axum::Router;
use sqlx::PgPool;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::ratelimit::RateLimiter;
use crate::storage::Storage;
use crate::ws::WsHub;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub storage: Storage,
    pub config: Arc<Config>,
    /// Brute-Force-Schutz für die Auth-Endpunkte.
    pub ratelimit: Arc<RateLimiter>,
    /// HTTP-Client (Magic-Link-Webhook + LLM-Provider).
    pub http: reqwest::Client,
    /// Multiplexer WebSocket-Hub (Chat-Streaming, fs-changed …).
    pub ws: WsHub,
    /// Run-IDs, deren Stream abgebrochen werden soll (Cancel).
    pub cancels: Arc<Mutex<HashSet<Uuid>>>,
    /// Aktiver Run je Agent (agent_id → run_id). Erzwingt **einen**
    /// gleichzeitigen Run pro Agent (Shared-Session, PLAN.md Lücke #3).
    pub active_runs: Arc<Mutex<std::collections::HashMap<Uuid, Uuid>>>,
    /// Offene HITL-Anfragen: hitl_id → Sender (true = approve, false =
    /// reject). Der Run-Task wartet auf die Entscheidung.
    pub pending_hitl:
        Arc<Mutex<std::collections::HashMap<Uuid, tokio::sync::oneshot::Sender<bool>>>>,
    /// Offene `ask_user`-Rückfragen: question_id → Sender (Antworttext).
    pub pending_questions:
        Arc<Mutex<std::collections::HashMap<Uuid, tokio::sync::oneshot::Sender<String>>>>,
}

/// Baut die komplette App: API-Router + statisches Frontend mit
/// SPA-Fallback. Unbekannte `/api/v1/*`-Pfade liefern JSON-404, alle
/// anderen unbekannten GETs `index.html` (Client-Side-Routing).
pub fn build_app(state: AppState) -> Router {
    let static_dir = PathBuf::from(&state.config.static_dir);
    let index_html = static_dir.join("index.html");

    let spa = ServeDir::new(&static_dir).not_found_service(ServeFile::new(index_html));

    let api = routes::api_router().fallback(|| async {
        (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "code": "not_found",
                "message": "Endpunkt nicht gefunden"
            })),
        )
    });

    Router::new()
        .nest("/api/v1", api)
        .fallback_service(spa)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

//! Multiplexer WebSocket-Hub (Phase 6a, PLAN.md Planungslücke #2).
//! Eine Verbindung pro Client: `GET /api/v1/ws?token=<access>`. Server
//! sendet `{ "channel": "...", "payload": ... }`. Broadcasts mit einer
//! `workspace_id` gehen nur an Mitglieder dieses Workspaces (CLAUDE.md §7).

use std::collections::HashSet;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::auth::decode_access_token;
use crate::AppState;

#[derive(Clone)]
pub struct WsBroadcast {
    /// `None` = an alle; `Some(w)` = nur Mitglieder von Workspace `w`.
    pub workspace_id: Option<Uuid>,
    pub channel: String,
    pub payload: Value,
}

#[derive(Clone)]
pub struct WsHub {
    tx: broadcast::Sender<WsBroadcast>,
}

impl Default for WsHub {
    fn default() -> Self {
        Self::new()
    }
}

impl WsHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self { tx }
    }

    /// Best-effort Broadcast (kein Abonnent → wird verworfen).
    pub fn publish(&self, workspace_id: Option<Uuid>, channel: impl Into<String>, payload: Value) {
        let _ = self.tx.send(WsBroadcast {
            workspace_id,
            channel: channel.into(),
            payload,
        });
    }

    fn subscribe(&self) -> broadcast::Receiver<WsBroadcast> {
        self.tx.subscribe()
    }
}

#[derive(Deserialize)]
pub struct WsQuery {
    token: String,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(q): Query<WsQuery>,
) -> Response {
    // Token im Query (Browser-WS kann keinen Auth-Header setzen, §7).
    let claims = match decode_access_token(&state.config.jwt_secret, &q.token) {
        Ok(c) => c,
        Err(_) => {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
    };
    let (user_id, org_id, is_owner) = (
        Uuid::parse_str(&claims.sub).unwrap_or_default(),
        Uuid::parse_str(&claims.org_id).unwrap_or_default(),
        claims.org_role == "owner",
    );

    // Zugreifbare Workspaces einmalig beim Connect bestimmen (kein
    // DB-Hit pro Broadcast).
    let rows: Vec<(Uuid,)> = if is_owner {
        sqlx::query_as("SELECT id FROM workspaces WHERE org_id = $1")
            .bind(org_id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default()
    } else {
        sqlx::query_as("SELECT workspace_id FROM workspace_members WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default()
    };
    let allowed: HashSet<Uuid> = rows.into_iter().map(|r| r.0).collect();

    let rx = state.ws.subscribe();
    ws.on_upgrade(move |socket| pump(socket, rx, allowed))
}

use axum::response::IntoResponse;

async fn pump(socket: WebSocket, mut rx: broadcast::Receiver<WsBroadcast>, allowed: HashSet<Uuid>) {
    let (mut sender, mut receiver) = socket.split();
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(b) => {
                    let visible = b
                        .workspace_id
                        .map(|w| allowed.contains(&w))
                        .unwrap_or(true);
                    if !visible {
                        continue;
                    }
                    let frame = json!({
                        "channel": b.channel,
                        "payload": b.payload,
                    })
                    .to_string();
                    if sender.send(Message::Text(frame.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            // Eingehende Client-Frames werden ignoriert; nur Close erkennen.
            inbound = receiver.next() => match inbound {
                None | Some(Err(_)) => break,
                Some(Ok(Message::Close(_))) => break,
                Some(Ok(_)) => {}
            },
        }
    }
}

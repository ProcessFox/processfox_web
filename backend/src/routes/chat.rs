//! Chat & Run-Streaming (Phase 6a). Antworten werden über den WS-Hub
//! gestreamt (`chat:run:<runId>`), Verlauf in `chat_messages` (Postgres).
//! Tools/HITL/Delegation folgen in Phase 6b (Skill-System).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::llm::{stream_chat, ChatMsg};
use crate::perm::{require_editor, require_member};
use crate::{crypto, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/agents/{id}/messages",
            get(list_messages).post(send_message),
        )
        .route("/runs/{id}/cancel", post(cancel_run))
        // HITL-Stubs (Tools kommen erst in Phase 6b).
        .route("/hitl/{id}/approve", post(hitl_noop))
        .route("/hitl/{id}/reject", post(hitl_noop))
        .route("/questions/{id}/respond", post(hitl_noop))
}

async fn hitl_noop() -> StatusCode {
    StatusCode::NO_CONTENT
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiMessage {
    id: String,
    role: String,
    content: String,
    created_at: String,
}

fn row_to_msg(id: Uuid, role: String, content: Value, at: OffsetDateTime) -> ApiMessage {
    ApiMessage {
        id: id.to_string(),
        role,
        content: content.as_str().unwrap_or_default().to_string(),
        created_at: at.format(&Rfc3339).unwrap_or_default(),
    }
}

async fn agent_ctx(state: &AppState, id: Uuid) -> ApiResult<(Uuid, String)> {
    let row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT workspace_id, system_prompt FROM agents WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
    row.ok_or(ApiError::NotFound)
}

async fn list_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Path(agent_id): Path<Uuid>,
) -> ApiResult<Json<Vec<ApiMessage>>> {
    let (wid, _) = agent_ctx(&state, agent_id).await?;
    require_member(&state, &user, wid).await?;
    let rows: Vec<(Uuid, String, Value, OffsetDateTime)> = sqlx::query_as(
        "SELECT id, role, content, created_at FROM chat_messages \
         WHERE agent_id = $1 ORDER BY created_at",
    )
    .bind(agent_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, r, c, at)| row_to_msg(id, r, c, at))
            .collect(),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendBody {
    provider: String,
    model_id: String,
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunStarted {
    run_id: String,
    assistant_message_id: String,
}

async fn fetch_api_key(state: &AppState, org_id: Uuid, provider: &str) -> ApiResult<String> {
    let row: Option<(Vec<u8>,)> = sqlx::query_as(
        "SELECT encrypted_key FROM api_keys \
         WHERE org_id = $1 AND provider = $2",
    )
    .bind(org_id)
    .bind(provider)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    let blob = row
        .ok_or_else(|| ApiError::BadRequest(format!("Kein API-Key für {provider} hinterlegt.")))?
        .0;
    let plain =
        crypto::decrypt(&state.config.api_key_encryption_key, &blob).map_err(ApiError::Internal)?;
    Ok(String::from_utf8_lossy(&plain).into_owned())
}

async fn send_message(
    State(state): State<AppState>,
    user: AuthUser,
    Path(agent_id): Path<Uuid>,
    Json(body): Json<SendBody>,
) -> ApiResult<Json<RunStarted>> {
    let (wid, system_prompt) = agent_ctx(&state, agent_id).await?;
    require_editor(&state, &user, wid).await?;
    if body.text.trim().is_empty() {
        return Err(ApiError::BadRequest("Leere Nachricht.".into()));
    }
    let api_key = fetch_api_key(&state, user.org_id, &body.provider).await?;

    // User-Nachricht persistieren.
    sqlx::query(
        "INSERT INTO chat_messages (agent_id, role, content) \
         VALUES ($1, 'user', $2)",
    )
    .bind(agent_id)
    .bind(Value::String(body.text.clone()))
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    // Verlauf laden (inkl. gerade eingefügter User-Nachricht).
    let hist_rows: Vec<(String, Value)> = sqlx::query_as(
        "SELECT role, content FROM chat_messages \
         WHERE agent_id = $1 ORDER BY created_at",
    )
    .bind(agent_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    let history: Vec<ChatMsg> = hist_rows
        .into_iter()
        .filter(|(r, _)| r == "user" || r == "assistant")
        .map(|(r, c)| ChatMsg {
            role: r,
            content: c.as_str().unwrap_or_default().to_string(),
        })
        .collect();

    let run_id = Uuid::new_v4();
    let assistant_id = Uuid::new_v4();
    let channel = format!("chat:run:{run_id}");

    let st = state.clone();
    let provider = body.provider.clone();
    let model = body.model_id.clone();
    tokio::spawn(async move {
        let mut acc = String::new();
        let res = stream_chat(
            &st.http,
            &provider,
            &api_key,
            &model,
            &system_prompt,
            &history,
            |chunk| {
                let cancelled = st
                    .cancels
                    .lock()
                    .map(|s| s.contains(&run_id))
                    .unwrap_or(false);
                if cancelled {
                    return false;
                }
                acc.push_str(chunk);
                st.ws.publish(
                    Some(wid),
                    channel.clone(),
                    json!({ "type": "delta", "text": chunk }),
                );
                true
            },
        )
        .await;

        let cancelled = st
            .cancels
            .lock()
            .map(|mut s| s.remove(&run_id))
            .unwrap_or(false);

        match res {
            Ok(_) => {
                // Assistenten-Nachricht persistieren.
                let saved: Result<(OffsetDateTime,), _> = sqlx::query_as(
                    "INSERT INTO chat_messages (id, agent_id, role, content) \
                     VALUES ($1, $2, 'assistant', $3) RETURNING created_at",
                )
                .bind(assistant_id)
                .bind(agent_id)
                .bind(Value::String(acc.clone()))
                .fetch_one(&st.pool)
                .await;
                let created_at = saved
                    .map(|r| r.0.format(&Rfc3339).unwrap_or_default())
                    .unwrap_or_default();
                let message = json!({
                    "id": assistant_id.to_string(),
                    "role": "assistant",
                    "content": acc,
                    "createdAt": created_at,
                });
                st.ws.publish(
                    Some(wid),
                    channel.clone(),
                    json!({
                        "type": "finish",
                        "reason": if cancelled { "cancelled" } else { "stop" },
                        "message": message,
                    }),
                );
            }
            Err(e) => {
                tracing::error!(error = %e, "chat run failed");
                st.ws.publish(
                    Some(wid),
                    channel.clone(),
                    json!({
                        "type": "error",
                        "code": "llm_error",
                        "message": e.to_string(),
                    }),
                );
            }
        }
    });

    Ok(Json(RunStarted {
        run_id: run_id.to_string(),
        assistant_message_id: assistant_id.to_string(),
    }))
}

async fn cancel_run(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(run_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    if let Ok(mut s) = state.cancels.lock() {
        s.insert(run_id);
    }
    Ok(StatusCode::NO_CONTENT)
}

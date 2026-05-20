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
use crate::llm::{self, stream_chat, ChatMsg};
use crate::perm::{require_editor, require_member};
use crate::{crypto, AppState};

/// System-Prompt für den Delegations-Worker (knappe Zell-Antwort).
const WORKER_SYS: &str = "Du bist ein Hintergrund-Worker. Antworte \
ausschließlich mit dem reinen Ergebnis für die Zelle — knapp, ohne \
Erklärungen, ohne Markdown, ohne Anführungszeichen.";

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/agents/{id}/messages",
            get(list_messages).post(send_message),
        )
        .route("/runs/{id}/cancel", post(cancel_run))
        .route("/hitl/{id}/approve", post(hitl_approve))
        .route("/hitl/{id}/reject", post(hitl_reject))
        .route("/questions/{id}/respond", post(question_respond))
}

#[derive(Deserialize)]
struct RespondBody {
    answer: String,
}

/// Beantwortet eine parkende `ask_user`-Rückfrage; den
/// `askUserResolved`-Broadcast macht der Run-Task.
async fn question_respond(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(question_id): Path<Uuid>,
    Json(body): Json<RespondBody>,
) -> StatusCode {
    let tx = state
        .pending_questions
        .lock()
        .ok()
        .and_then(|mut m| m.remove(&question_id));
    match tx {
        Some(tx) => {
            let _ = tx.send(body.answer);
            StatusCode::NO_CONTENT
        }
        None => StatusCode::NOT_FOUND,
    }
}

/// Löst die parkende HITL-Anfrage auf (true = freigegeben). Den
/// `hitlResolved`-Broadcast macht der Run-Task.
fn resolve_hitl(state: &AppState, hitl_id: Uuid, approved: bool) -> StatusCode {
    let tx = state
        .pending_hitl
        .lock()
        .ok()
        .and_then(|mut m| m.remove(&hitl_id));
    match tx {
        Some(tx) => {
            let _ = tx.send(approved);
            StatusCode::NO_CONTENT
        }
        None => StatusCode::NOT_FOUND,
    }
}

async fn hitl_approve(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(hitl_id): Path<Uuid>,
) -> StatusCode {
    resolve_hitl(&state, hitl_id, true)
}

async fn hitl_reject(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(hitl_id): Path<Uuid>,
) -> StatusCode {
    resolve_hitl(&state, hitl_id, false)
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

struct AgentCtx {
    wid: Uuid,
    system_prompt: String,
    skills: Vec<String>,
    hitl_disabled: bool,
}

async fn agent_ctx(state: &AppState, id: Uuid) -> ApiResult<AgentCtx> {
    let row: Option<(Uuid, String, Value, bool)> = sqlx::query_as(
        "SELECT workspace_id, system_prompt, skills, hitl_disabled \
         FROM agents WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    let (wid, system_prompt, skills, hitl_disabled) = row.ok_or(ApiError::NotFound)?;
    let skills = skills
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Ok(AgentCtx {
        wid,
        system_prompt,
        skills,
        hitl_disabled,
    })
}

async fn list_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Path(agent_id): Path<Uuid>,
) -> ApiResult<Json<Vec<ApiMessage>>> {
    let wid = agent_ctx(&state, agent_id).await?.wid;
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

/// Gibt den Agenten-Run-Slot frei (nur wenn er noch diesem Run gehört)
/// und räumt das Cancel-Flag ab.
fn release_run(state: &AppState, agent_id: Uuid, run_id: Uuid) {
    if let Ok(mut active) = state.active_runs.lock() {
        if active.get(&agent_id) == Some(&run_id) {
            active.remove(&agent_id);
        }
    }
    if let Ok(mut c) = state.cancels.lock() {
        c.remove(&run_id);
    }
}

fn is_cancelled(state: &AppState, run_id: Uuid) -> bool {
    state
        .cancels
        .lock()
        .map(|s| s.contains(&run_id))
        .unwrap_or(false)
}

/// Assistenten-Nachricht persistieren + `finish` broadcasten.
async fn finish_run(
    st: &AppState,
    agent_id: Uuid,
    assistant_id: Uuid,
    wid: Uuid,
    channel: &str,
    text: String,
    reason: &str,
) {
    let saved: Result<(OffsetDateTime,), _> = sqlx::query_as(
        "INSERT INTO chat_messages (id, agent_id, role, content) \
         VALUES ($1, $2, 'assistant', $3) RETURNING created_at",
    )
    .bind(assistant_id)
    .bind(agent_id)
    .bind(Value::String(text.clone()))
    .fetch_one(&st.pool)
    .await;
    let created_at = saved
        .map(|r| r.0.format(&Rfc3339).unwrap_or_default())
        .unwrap_or_default();
    st.ws.publish(
        Some(wid),
        channel.to_string(),
        json!({
            "type": "finish",
            "reason": reason,
            "message": {
                "id": assistant_id.to_string(),
                "role": "assistant",
                "content": text,
                "createdAt": created_at,
            },
        }),
    );
}

fn error_run(st: &AppState, wid: Uuid, channel: &str, msg: String) {
    tracing::error!(error = %msg, "chat run failed");
    st.ws.publish(
        Some(wid),
        channel.to_string(),
        json!({ "type": "error", "code": "llm_error", "message": msg }),
    );
}

async fn send_message(
    State(state): State<AppState>,
    user: AuthUser,
    Path(agent_id): Path<Uuid>,
    Json(body): Json<SendBody>,
) -> ApiResult<Json<RunStarted>> {
    let AgentCtx {
        wid,
        system_prompt,
        skills,
        hitl_disabled,
    } = agent_ctx(&state, agent_id).await?;
    require_editor(&state, &user, wid).await?;
    if body.text.trim().is_empty() {
        return Err(ApiError::BadRequest("Leere Nachricht.".into()));
    }
    let api_key = fetch_api_key(&state, user.org_id, &body.provider).await?;

    let run_id = Uuid::new_v4();
    let assistant_id = Uuid::new_v4();

    // Genau ein aktiver Run pro Agent (Shared-Session). Zweiter paralleler
    // Send → 409, ohne den laufenden Run zu stören.
    {
        let mut active = state
            .active_runs
            .lock()
            .map_err(|_| ApiError::Internal(anyhow::anyhow!("lock poisoned")))?;
        if active.contains_key(&agent_id) {
            return Err(ApiError::Conflict(
                "Es läuft bereits eine Antwort für diesen Agenten.".into(),
            ));
        }
        active.insert(agent_id, run_id);
    }

    // Ab hier muss bei jedem Fehlerpfad der Slot wieder frei werden.
    let setup = async {
        sqlx::query(
            "INSERT INTO chat_messages (agent_id, role, content) \
             VALUES ($1, 'user', $2)",
        )
        .bind(agent_id)
        .bind(Value::String(body.text.clone()))
        .execute(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

        let hist_rows: Vec<(String, Value)> = sqlx::query_as(
            "SELECT role, content FROM chat_messages \
             WHERE agent_id = $1 ORDER BY created_at",
        )
        .bind(agent_id)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
        Ok::<_, ApiError>(
            hist_rows
                .into_iter()
                .filter(|(r, _)| r == "user" || r == "assistant")
                .map(|(r, c)| ChatMsg {
                    role: r,
                    content: c.as_str().unwrap_or_default().to_string(),
                })
                .collect::<Vec<_>>(),
        )
    };
    let history = match setup.await {
        Ok(h) => h,
        Err(e) => {
            release_run(&state, agent_id, run_id);
            return Err(e);
        }
    };

    // Streaming an ALLE Mitglieder, die den Agenten offen haben.
    let channel = format!("chat:agent:{agent_id}");

    // Run-Start: alle Mitglieder laden den Verlauf neu → der eben
    // gesendete User-Prompt erscheint sofort bei jedem (nicht erst bei
    // finish), und der optimistische Platzhalter des Absenders wird durch
    // die persistierte Nachricht ersetzt.
    state
        .ws
        .publish(Some(wid), channel.clone(), json!({ "type": "userMessage" }));

    let st = state.clone();
    let provider = body.provider.clone();
    let model = body.model_id.clone();
    let user_id = user.user_id;
    tokio::spawn(async move {
        let tools = crate::tools::available_tools(&st.skills, &skills);

        // --- Kein Tool → reines Streaming (Phase 6a, beste UX) ----------
        if tools.is_empty() {
            let mut acc = String::new();
            let res = stream_chat(
                &st.http,
                &provider,
                &api_key,
                &model,
                &system_prompt,
                &history,
                |chunk| {
                    if is_cancelled(&st, run_id) {
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
            match res {
                Ok(_) => {
                    let reason = if is_cancelled(&st, run_id) {
                        "cancelled"
                    } else {
                        "stop"
                    };
                    finish_run(&st, agent_id, assistant_id, wid, &channel, acc, reason).await;
                }
                Err(e) => error_run(&st, wid, &channel, e.to_string()),
            }
            release_run(&st, agent_id, run_id);
            return;
        }

        // --- Tools aktiv → ReAct-Loop mit HITL (Phase 6b-1) -------------
        let mut turns: Vec<llm::Turn> = history
            .iter()
            .map(|m| {
                if m.role == "user" {
                    llm::Turn::User(m.content.clone())
                } else {
                    llm::Turn::Assistant(m.content.clone())
                }
            })
            .collect();

        const MAX_ITERS: usize = 8;
        let mut final_text: Option<String> = None;
        let mut reason = "stop";

        for _ in 0..MAX_ITERS {
            if is_cancelled(&st, run_id) {
                final_text = Some(String::new());
                reason = "cancelled";
                break;
            }
            let step = match llm::tool_step(
                &st.http,
                &provider,
                &api_key,
                &model,
                &system_prompt,
                &turns,
                &tools,
            )
            .await
            {
                Ok(s) => s,
                Err(e) => {
                    error_run(&st, wid, &channel, e.to_string());
                    release_run(&st, agent_id, run_id);
                    return;
                }
            };
            if step.calls.is_empty() {
                final_text = Some(step.text.unwrap_or_default());
                break;
            }
            turns.push(llm::Turn::ToolUse(step.calls.clone()));
            let mut results = Vec::new();
            for call in &step.calls {
                st.ws.publish(
                    Some(wid),
                    channel.clone(),
                    json!({
                        "type": "toolCallStarted",
                        "id": call.id, "name": call.name,
                        "arguments": call.input
                    }),
                );
                let outcome: ApiResult<String> = if crate::tools::is_ask_tool(&call.name) {
                    let question = call
                        .input
                        .get("question")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let qid = Uuid::new_v4();
                    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
                    if let Ok(mut m) = st.pending_questions.lock() {
                        m.insert(qid, tx);
                    }
                    st.ws.publish(
                        Some(wid),
                        channel.clone(),
                        json!({
                            "type": "askUserRequest",
                            "questionId": qid.to_string(),
                            "toolCallId": call.id,
                            "question": question
                        }),
                    );
                    let answer =
                        match tokio::time::timeout(std::time::Duration::from_secs(600), rx).await {
                            Ok(Ok(a)) => a,
                            _ => "(keine Antwort)".to_string(),
                        };
                    if let Ok(mut m) = st.pending_questions.lock() {
                        m.remove(&qid);
                    }
                    st.ws.publish(
                        Some(wid),
                        channel.clone(),
                        json!({
                            "type": "askUserResolved",
                            "questionId": qid.to_string(),
                            "answer": answer
                        }),
                    );
                    Ok(answer)
                } else if crate::tools::is_delegate_tool(&call.name) {
                    match crate::tools::delegate_plan(&st, wid, &call.input) {
                        Err(e) => Err(e),
                        Ok(plan) => {
                            let approved = if hitl_disabled {
                                true
                            } else {
                                let preview = crate::tools::delegate_preview_json(&plan, "Worker");
                                let hitl_id = Uuid::new_v4();
                                let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
                                if let Ok(mut m) = st.pending_hitl.lock() {
                                    m.insert(hitl_id, tx);
                                }
                                st.ws.publish(
                                    Some(wid),
                                    channel.clone(),
                                    json!({
                                        "type": "hitlRequest",
                                        "hitlId": hitl_id.to_string(),
                                        "toolCallId": call.id,
                                        "toolName": call.name,
                                        "preview": preview
                                    }),
                                );
                                let ok = matches!(
                                    tokio::time::timeout(std::time::Duration::from_secs(600), rx,)
                                        .await,
                                    Ok(Ok(true))
                                );
                                if let Ok(mut m) = st.pending_hitl.lock() {
                                    m.remove(&hitl_id);
                                }
                                st.ws.publish(
                                    Some(wid),
                                    channel.clone(),
                                    json!({
                                        "type": "hitlResolved",
                                        "hitlId": hitl_id.to_string(),
                                        "decision": { "kind":
                                            if ok { "approve" }
                                            else { "reject" } }
                                    }),
                                );
                                ok
                            };
                            if !approved {
                                Ok("Vom Nutzer abgelehnt.".to_string())
                            } else {
                                let total = plan.data_rows.len();
                                st.ws.publish(
                                    Some(wid),
                                    channel.clone(),
                                    json!({
                                        "type": "delegationStarted",
                                        "toolCallId": call.id,
                                        "total": total
                                    }),
                                );
                                let mut results = Vec::new();
                                let (mut ok_n, mut fail_n) = (0usize, 0usize);
                                for (idx, &row) in plan.data_rows.iter().enumerate() {
                                    if is_cancelled(&st, run_id) {
                                        break;
                                    }
                                    let prompt = crate::tools::render_prompt(&plan, row);
                                    let label = plan
                                        .grid
                                        .get(row)
                                        .and_then(|r| r.first())
                                        .filter(|s| !s.trim().is_empty())
                                        .cloned()
                                        .unwrap_or_else(|| format!("Zeile {}", row + 1));
                                    match llm::tool_step(
                                        &st.http,
                                        &provider,
                                        &api_key,
                                        &model,
                                        WORKER_SYS,
                                        &[llm::Turn::User(prompt)],
                                        &[],
                                    )
                                    .await
                                    {
                                        Ok(step) => {
                                            results.push((
                                                row,
                                                step.text.unwrap_or_default().trim().to_string(),
                                            ));
                                            ok_n += 1;
                                            st.ws.publish(
                                                Some(wid),
                                                channel.clone(),
                                                json!({
                                                    "type":
                                                        "delegationItemDone",
                                                    "toolCallId": call.id,
                                                    "index": idx,
                                                    "total": total,
                                                    "itemLabel": label
                                                }),
                                            );
                                        }
                                        Err(e) => {
                                            fail_n += 1;
                                            st.ws.publish(
                                                Some(wid),
                                                channel.clone(),
                                                json!({
                                                    "type":
                                                      "delegationItemFailed",
                                                    "toolCallId": call.id,
                                                    "index": idx,
                                                    "total": total,
                                                    "itemLabel": label,
                                                    "error": e.to_string()
                                                }),
                                            );
                                        }
                                    }
                                }
                                st.ws.publish(
                                    Some(wid),
                                    channel.clone(),
                                    json!({
                                        "type": "delegationFinished",
                                        "toolCallId": call.id,
                                        "succeeded": ok_n,
                                        "failed": fail_n
                                    }),
                                );
                                crate::tools::save_delegation(&st, wid, user_id, &plan, &results)
                                    .await
                            }
                        }
                    }
                } else if crate::tools::is_write_tool(&call.name) {
                    if hitl_disabled {
                        crate::tools::execute_write(&st, wid, user_id, &call.name, &call.input)
                            .await
                    } else {
                        match crate::tools::write_preview(&st, wid, &call.name, &call.input) {
                            Ok(preview) => {
                                let hitl_id = Uuid::new_v4();
                                let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
                                if let Ok(mut m) = st.pending_hitl.lock() {
                                    m.insert(hitl_id, tx);
                                }
                                st.ws.publish(
                                    Some(wid),
                                    channel.clone(),
                                    json!({
                                        "type": "hitlRequest",
                                        "hitlId": hitl_id.to_string(),
                                        "toolCallId": call.id,
                                        "toolName": call.name,
                                        "preview": preview
                                    }),
                                );
                                let approved = matches!(
                                    tokio::time::timeout(std::time::Duration::from_secs(600), rx,)
                                        .await,
                                    Ok(Ok(true))
                                );
                                if let Ok(mut m) = st.pending_hitl.lock() {
                                    m.remove(&hitl_id);
                                }
                                st.ws.publish(
                                    Some(wid),
                                    channel.clone(),
                                    json!({
                                        "type": "hitlResolved",
                                        "hitlId": hitl_id.to_string(),
                                        "decision": { "kind":
                                            if approved { "approve" }
                                            else { "reject" } }
                                    }),
                                );
                                if approved {
                                    crate::tools::execute_write(
                                        &st,
                                        wid,
                                        user_id,
                                        &call.name,
                                        &call.input,
                                    )
                                    .await
                                } else {
                                    Ok("Vom Nutzer abgelehnt.".to_string())
                                }
                            }
                            Err(e) => Err(e),
                        }
                    }
                } else {
                    crate::tools::execute_read_tool(&st, wid, &call.name, &call.input).await
                };
                let (content, is_error) = match outcome {
                    Ok(s) => (s, false),
                    Err(e) => (e.to_string(), true),
                };
                st.ws.publish(
                    Some(wid),
                    channel.clone(),
                    json!({
                        "type": "toolCallCompleted",
                        "id": call.id, "content": content,
                        "isError": is_error
                    }),
                );
                results.push(llm::ToolResult {
                    id: call.id.clone(),
                    content,
                });
            }
            turns.push(llm::Turn::ToolResults(results));
        }

        let text =
            final_text.unwrap_or_else(|| "(Maximale Tool-Iterationen erreicht.)".to_string());
        finish_run(&st, agent_id, assistant_id, wid, &channel, text, reason).await;
        release_run(&st, agent_id, run_id);
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

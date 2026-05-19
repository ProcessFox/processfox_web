//! Agenten (PLAN.md Phase 4). REST unter `/api/v1`. Workspace-scoped:
//! Lesen = `require_member`, Schreiben = `require_editor` (CLAUDE.md §4).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::perm::{require_editor, require_member};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/workspaces/{wid}/agents",
            get(list_agents).post(create_agent),
        )
        .route(
            "/agents/{id}",
            get(get_agent).patch(update_agent).delete(delete_agent),
        )
        .route("/agents/{id}/attachment", post(set_attachment))
}

// --- Row / DTOs -----------------------------------------------------------

#[derive(sqlx::FromRow)]
struct AgentRow {
    id: Uuid,
    workspace_id: Uuid,
    name: String,
    icon: String,
    system_prompt: String,
    provider: Option<String>,
    model_id: Option<String>,
    skills: Value,
    skill_settings: Value,
    hitl_disabled: bool,
    attachments: Value,
    delegation_profile: Option<Value>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl AgentRow {
    fn to_api(&self) -> Value {
        let model = match (&self.provider, &self.model_id) {
            (Some(p), Some(m)) => json!({ "provider": p, "id": m }),
            _ => Value::Null,
        };
        json!({
            "id": self.id.to_string(),
            "workspaceId": self.workspace_id.to_string(),
            "name": self.name,
            "icon": self.icon,
            "systemPrompt": self.system_prompt,
            "model": model,
            "skills": self.skills,
            "skillSettings": self.skill_settings,
            "hitlDisabled": self.hitl_disabled,
            "attachments": self.attachments,
            "delegationProfile":
                self.delegation_profile.clone().unwrap_or(Value::Null),
            "createdAt": self.created_at.format(&Rfc3339).unwrap_or_default(),
            "updatedAt": self.updated_at.format(&Rfc3339).unwrap_or_default(),
        })
    }
}

#[derive(Deserialize)]
struct ModelInput {
    provider: String,
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentInput {
    name: Option<String>,
    icon: Option<String>,
    system_prompt: Option<String>,
    model: Option<ModelInput>,
    skills: Option<Vec<String>>,
    hitl_disabled: Option<bool>,
    delegation_profile: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttachmentBody {
    kind: String,
    /// `null` löst die Verknüpfung.
    file_id: Option<String>,
}

const COLS: &str = "id, workspace_id, name, icon, system_prompt, provider, \
    model_id, skills, skill_settings, hitl_disabled, attachments, \
    delegation_profile, created_at, updated_at";

async fn agent_workspace(state: &AppState, id: Uuid) -> ApiResult<Uuid> {
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT workspace_id FROM agents WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(row.ok_or(ApiError::NotFound)?.0)
}

// --- Handlers -------------------------------------------------------------

async fn list_agents(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
) -> ApiResult<Json<Vec<Value>>> {
    require_member(&state, &user, wid).await?;
    let rows: Vec<AgentRow> = sqlx::query_as(&format!(
        "SELECT {COLS} FROM agents WHERE workspace_id = $1 ORDER BY created_at"
    ))
    .bind(wid)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(rows.iter().map(AgentRow::to_api).collect()))
}

async fn get_agent(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let wid = agent_workspace(&state, id).await?;
    require_member(&state, &user, wid).await?;
    let row: AgentRow = sqlx::query_as(&format!("SELECT {COLS} FROM agents WHERE id = $1"))
        .bind(id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(row.to_api()))
}

async fn create_agent(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
    Json(body): Json<AgentInput>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    require_editor(&state, &user, wid).await?;
    let name = body.name.unwrap_or_default();
    if name.trim().is_empty() {
        return Err(ApiError::BadRequest("Name erforderlich.".into()));
    }
    let (provider, model_id) = match body.model {
        Some(m) => (Some(m.provider), Some(m.id)),
        None => (None, None),
    };
    let skills = json!(body.skills.unwrap_or_default());
    let row: AgentRow = sqlx::query_as(&format!(
        "INSERT INTO agents \
         (workspace_id, name, icon, system_prompt, provider, model_id, \
          skills, hitl_disabled, delegation_profile) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING {COLS}"
    ))
    .bind(wid)
    .bind(name.trim())
    .bind(body.icon.unwrap_or_else(|| "Bot".into()))
    .bind(body.system_prompt.unwrap_or_default())
    .bind(provider)
    .bind(model_id)
    .bind(skills)
    .bind(body.hitl_disabled.unwrap_or(false))
    .bind(body.delegation_profile)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok((StatusCode::CREATED, Json(row.to_api())))
}

async fn update_agent(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AgentInput>,
) -> ApiResult<Json<Value>> {
    let wid = agent_workspace(&state, id).await?;
    require_editor(&state, &user, wid).await?;
    let name = body.name.unwrap_or_default();
    if name.trim().is_empty() {
        return Err(ApiError::BadRequest("Name erforderlich.".into()));
    }
    let (provider, model_id) = match body.model {
        Some(m) => (Some(m.provider), Some(m.id)),
        None => (None, None),
    };
    let skills = json!(body.skills.unwrap_or_default());
    let row: AgentRow = sqlx::query_as(&format!(
        "UPDATE agents SET name=$2, icon=$3, system_prompt=$4, provider=$5, \
         model_id=$6, skills=$7, hitl_disabled=$8, delegation_profile=$9, \
         updated_at=now() WHERE id=$1 RETURNING {COLS}"
    ))
    .bind(id)
    .bind(name.trim())
    .bind(body.icon.unwrap_or_else(|| "Bot".into()))
    .bind(body.system_prompt.unwrap_or_default())
    .bind(provider)
    .bind(model_id)
    .bind(skills)
    .bind(body.hitl_disabled.unwrap_or(false))
    .bind(body.delegation_profile)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(row.to_api()))
}

async fn delete_agent(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let wid = agent_workspace(&state, id).await?;
    require_editor(&state, &user, wid).await?;
    sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_attachment(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AttachmentBody>,
) -> ApiResult<Json<Value>> {
    let wid = agent_workspace(&state, id).await?;
    require_editor(&state, &user, wid).await?;
    if body.kind != "template" {
        return Err(ApiError::BadRequest("Unbekannter Attachment-Typ.".into()));
    }
    // Datei-Existenzprüfung gegen workspace_files folgt mit Phase 5.
    let attachments = json!({ "templateFileId": body.file_id });
    let row: AgentRow = sqlx::query_as(&format!(
        "UPDATE agents SET attachments=$2, updated_at=now() \
         WHERE id=$1 RETURNING {COLS}"
    ))
    .bind(id)
    .bind(attachments)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    // Live an Workspace-Mitglieder (Frontend erwartet die Agent-ID als Payload).
    state.ws.publish(
        Some(wid),
        "agent-attachments-changed",
        json!(id.to_string()),
    );
    Ok(Json(row.to_api()))
}

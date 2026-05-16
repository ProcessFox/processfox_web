//! Workspace-Berechtigungen (CLAUDE.md §4/§9). Org-`owner` hat Vollzugriff
//! in seiner Org; sonst zählt `workspace_members.role`. Fremde Org →
//! `None` (behandeln wie „nicht gefunden", kein Leak).

use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::AppState;

pub async fn effective_role(
    state: &AppState,
    user: &AuthUser,
    workspace_id: Uuid,
) -> ApiResult<Option<String>> {
    let org: Option<(Uuid,)> = sqlx::query_as("SELECT org_id FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    let Some((org_id,)) = org else {
        return Ok(None);
    };
    if org_id != user.org_id {
        return Ok(None);
    }
    if user.is_owner() {
        return Ok(Some("editor".to_string())); // Owner = Vollzugriff
    }
    let role: Option<(String,)> = sqlx::query_as(
        "SELECT role FROM workspace_members \
         WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(workspace_id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(role.map(|r| r.0))
}

pub async fn require_member(
    state: &AppState,
    user: &AuthUser,
    workspace_id: Uuid,
) -> ApiResult<String> {
    effective_role(state, user, workspace_id)
        .await?
        .ok_or(ApiError::NotFound)
}

pub async fn require_editor(
    state: &AppState,
    user: &AuthUser,
    workspace_id: Uuid,
) -> ApiResult<()> {
    match require_member(state, user, workspace_id).await?.as_str() {
        "editor" => Ok(()),
        _ => Err(ApiError::forbidden("Editor-Rolle erforderlich.")),
    }
}

pub fn require_org_owner(user: &AuthUser) -> ApiResult<()> {
    if user.is_owner() {
        Ok(())
    } else {
        Err(ApiError::forbidden("Nur der Org-Owner darf das."))
    }
}

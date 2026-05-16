//! Workspaces & Mitglieder (PLAN.md Phase 3). REST unter `/api/v1`.
//!
//! Berechtigungen (CLAUDE.md §4): Org-`owner` darf alles in seiner Org
//! (inkl. Mitglieder verwalten, Workspaces anlegen/löschen). Workspace-
//! `editor` darf inhaltlich arbeiten, `viewer` nur lesen. Jeder Handler
//! prüft serverseitig — kein Verlass auf Frontend-Filterung.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, patch};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspaces", get(list_workspaces).post(create_workspace))
        .route(
            "/workspaces/{id}",
            patch(rename_workspace).delete(delete_workspace),
        )
        .route(
            "/workspaces/{id}/members",
            get(list_members).post(add_member),
        )
        .route(
            "/workspaces/{id}/members/{user_id}",
            delete(remove_member).patch(set_member_role),
        )
}

// --- DTOs -----------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiWorkspace {
    id: String,
    org_id: String,
    name: String,
    role: String,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiMember {
    user_id: String,
    email: String,
    role: String,
}

#[derive(Deserialize)]
struct NameBody {
    name: String,
}

#[derive(Deserialize)]
struct AddMemberBody {
    email: String,
    role: String,
}

#[derive(Deserialize)]
struct RoleBody {
    role: String,
}

fn valid_role(role: &str) -> bool {
    role == "editor" || role == "viewer"
}

fn rfc3339(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

// --- Berechtigungs-Helper -------------------------------------------------

/// Effektive Rolle des Users im Workspace, oder `None` wenn kein Zugriff
/// (auch bei fremder Org → behandeln wie „nicht gefunden", kein Leak).
async fn effective_role(
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

async fn require_member(
    state: &AppState,
    user: &AuthUser,
    workspace_id: Uuid,
) -> ApiResult<String> {
    effective_role(state, user, workspace_id)
        .await?
        .ok_or(ApiError::NotFound)
}

// Wird ab Phase 4 (Agenten/Dateien-Schreibaktionen) genutzt.
#[allow(dead_code)]
async fn require_editor(state: &AppState, user: &AuthUser, workspace_id: Uuid) -> ApiResult<()> {
    match require_member(state, user, workspace_id).await?.as_str() {
        "editor" => Ok(()),
        _ => Err(ApiError::forbidden("Editor-Rolle erforderlich.")),
    }
}

fn require_org_owner(user: &AuthUser) -> ApiResult<()> {
    if user.is_owner() {
        Ok(())
    } else {
        Err(ApiError::forbidden("Nur der Org-Owner darf das."))
    }
}

// --- Workspace-Handler ----------------------------------------------------

async fn list_workspaces(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<ApiWorkspace>>> {
    let rows: Vec<(Uuid, Uuid, String, OffsetDateTime, Option<String>)> = if user.is_owner() {
        sqlx::query_as(
            "SELECT id, org_id, name, created_at, NULL::text \
                 FROM workspaces WHERE org_id = $1 ORDER BY created_at",
        )
        .bind(user.org_id)
        .fetch_all(&state.pool)
        .await
    } else {
        sqlx::query_as(
            "SELECT w.id, w.org_id, w.name, w.created_at, m.role \
                 FROM workspaces w \
                 JOIN workspace_members m \
                   ON m.workspace_id = w.id AND m.user_id = $1 \
                 WHERE w.org_id = $2 ORDER BY w.created_at",
        )
        .bind(user.user_id)
        .bind(user.org_id)
        .fetch_all(&state.pool)
        .await
    }
    .map_err(|e| ApiError::Internal(e.into()))?;

    let out = rows
        .into_iter()
        .map(|(id, org_id, name, created_at, role)| ApiWorkspace {
            id: id.to_string(),
            org_id: org_id.to_string(),
            name,
            role: role.unwrap_or_else(|| "editor".to_string()),
            created_at: rfc3339(created_at),
        })
        .collect();
    Ok(Json(out))
}

async fn create_workspace(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<NameBody>,
) -> ApiResult<(StatusCode, Json<ApiWorkspace>)> {
    require_org_owner(&user)?;
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("Name erforderlich.".into()));
    }
    let row: (Uuid, OffsetDateTime) = sqlx::query_as(
        "INSERT INTO workspaces (org_id, name) VALUES ($1, $2) \
         RETURNING id, created_at",
    )
    .bind(user.org_id)
    .bind(name)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok((
        StatusCode::CREATED,
        Json(ApiWorkspace {
            id: row.0.to_string(),
            org_id: user.org_id.to_string(),
            name: name.to_string(),
            role: "editor".to_string(),
            created_at: rfc3339(row.1),
        }),
    ))
}

async fn rename_workspace(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<NameBody>,
) -> ApiResult<StatusCode> {
    require_org_owner(&user)?;
    require_member(&state, &user, id).await?; // stellt Org-Zugehörigkeit sicher
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("Name erforderlich.".into()));
    }
    sqlx::query("UPDATE workspaces SET name = $1 WHERE id = $2")
        .bind(name)
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_workspace(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    require_org_owner(&user)?;
    require_member(&state, &user, id).await?;
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

// --- Mitglieder-Handler ---------------------------------------------------

async fn list_members(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<ApiMember>>> {
    require_member(&state, &user, id).await?;
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT u.id, u.email, m.role FROM workspace_members m \
         JOIN users u ON u.id = m.user_id \
         WHERE m.workspace_id = $1 ORDER BY u.email",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(
        rows.into_iter()
            .map(|(uid, email, role)| ApiMember {
                user_id: uid.to_string(),
                email,
                role,
            })
            .collect(),
    ))
}

async fn add_member(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AddMemberBody>,
) -> ApiResult<StatusCode> {
    require_org_owner(&user)?;
    require_member(&state, &user, id).await?;
    if !valid_role(&body.role) {
        return Err(ApiError::BadRequest("Rolle: editor|viewer.".into()));
    }
    // Nutzer muss in derselben Org registriert sein (Self-Service via Code).
    let email = body.email.trim().to_lowercase();
    let target: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE email = $1 AND org_id = $2")
            .bind(&email)
            .bind(user.org_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
    let target_id = target
        .ok_or_else(|| {
            ApiError::BadRequest(
                "Kein Org-Mitglied mit dieser E-Mail (muss sich zuerst registrieren).".into(),
            )
        })?
        .0;

    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (workspace_id, user_id) DO UPDATE SET role = $3",
    )
    .bind(id)
    .bind(target_id)
    .bind(&body.role)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_member_role(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, target_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<RoleBody>,
) -> ApiResult<StatusCode> {
    require_org_owner(&user)?;
    require_member(&state, &user, id).await?;
    if !valid_role(&body.role) {
        return Err(ApiError::BadRequest("Rolle: editor|viewer.".into()));
    }
    let res = sqlx::query(
        "UPDATE workspace_members SET role = $1 \
         WHERE workspace_id = $2 AND user_id = $3",
    )
    .bind(&body.role)
    .bind(id)
    .bind(target_id)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_member(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, target_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    require_org_owner(&user)?;
    require_member(&state, &user, id).await?;
    sqlx::query(
        "DELETE FROM workspace_members \
         WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(target_id)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::valid_role;

    #[test]
    fn role_validation() {
        assert!(valid_role("editor"));
        assert!(valid_role("viewer"));
        assert!(!valid_role("owner"));
        assert!(!valid_role(""));
        assert!(!valid_role("Editor"));
    }
}

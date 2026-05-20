//! Workspaces & Mitglieder (PLAN.md Phase 3, CLAUDE.md §4). REST unter
//! `/api/v1`.
//!
//! Berechtigungs-Matrix (ab Migration 0006):
//!   - Workspace anlegen/umbenennen/löschen ........... Admin (Org-Owner)
//!   - Mitglied einladen/entfernen .................... Admin
//!   - Workspace-Liste / Mitglieder-Liste lesen ....... jedes Workspace-Mitglied
//!
//! Die frühere zweite Rollen-Ebene `workspace_members.role` ist abgeschafft;
//! wer im Workspace ist, ist gleichberechtigter Nutzer. Jeder Handler prüft
//! serverseitig — kein Verlass auf Frontend-Filterung.

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
use crate::perm::{require_member, require_org_owner};
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
        .route("/workspaces/{id}/members/{user_id}", delete(remove_member))
}

// --- DTOs -----------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiWorkspace {
    id: String,
    org_id: String,
    name: String,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiMember {
    user_id: String,
    email: String,
}

#[derive(Deserialize)]
struct NameBody {
    name: String,
}

#[derive(Deserialize)]
struct AddMemberBody {
    email: String,
}

fn rfc3339(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

// --- Workspace-Handler ----------------------------------------------------

async fn list_workspaces(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<ApiWorkspace>>> {
    // Admin sieht alle Workspaces seiner Org; Nutzer nur die, in denen
    // sie als `workspace_members`-Eintrag stehen.
    let rows: Vec<(Uuid, Uuid, String, OffsetDateTime)> = if user.is_owner() {
        sqlx::query_as(
            "SELECT id, org_id, name, created_at \
             FROM workspaces WHERE org_id = $1 ORDER BY created_at",
        )
        .bind(user.org_id)
        .fetch_all(&state.pool)
        .await
    } else {
        sqlx::query_as(
            "SELECT w.id, w.org_id, w.name, w.created_at \
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
        .map(|(id, org_id, name, created_at)| ApiWorkspace {
            id: id.to_string(),
            org_id: org_id.to_string(),
            name,
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
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT u.id, u.email FROM workspace_members m \
         JOIN users u ON u.id = m.user_id \
         WHERE m.workspace_id = $1 ORDER BY u.email",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(
        rows.into_iter()
            .map(|(uid, email)| ApiMember {
                user_id: uid.to_string(),
                email,
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
        "INSERT INTO workspace_members (workspace_id, user_id) \
         VALUES ($1, $2) \
         ON CONFLICT (workspace_id, user_id) DO NOTHING",
    )
    .bind(id)
    .bind(target_id)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
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

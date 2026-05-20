//! Workspace-Berechtigungen (CLAUDE.md §4/§9).
//!
//! Rollen-Modell (Stand Migration 0006):
//!   - **Admin** = `users.org_role = 'owner'`. Vollzugriff in der eigenen
//!     Org. Darf Workspaces/Mitglieder/Settings verwalten, Agenten löschen.
//!   - **Nutzer** = `users.org_role = 'member'`. Wer in einem Workspace ist,
//!     darf dort uneingeschränkt arbeiten (chatten, Dateien, Agenten). Die
//!     frühere zweite Rollen-Ebene `workspace_members.role` (editor|viewer)
//!     wurde abgeschafft.
//!   - Fremde Org → behandeln wir wie *nicht gefunden* (kein Leak).

use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::AppState;

/// Stellt sicher, dass der User Zugriff auf den Workspace hat (Org-Owner
/// oder explizites `workspace_members`-Mitglied). Fremde Org oder kein
/// Mitglied → `NotFound` (kein Existenz-Leak nach §9).
pub async fn require_member(
    state: &AppState,
    user: &AuthUser,
    workspace_id: Uuid,
) -> ApiResult<()> {
    let org: Option<(Uuid,)> = sqlx::query_as("SELECT org_id FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    let Some((org_id,)) = org else {
        return Err(ApiError::NotFound);
    };
    if org_id != user.org_id {
        return Err(ApiError::NotFound);
    }
    if user.is_owner() {
        return Ok(()); // Admin hat Vollzugriff in seiner Org.
    }
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT 1 FROM workspace_members \
         WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(workspace_id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    if row.is_some() {
        Ok(())
    } else {
        Err(ApiError::NotFound)
    }
}

pub fn require_org_owner(user: &AuthUser) -> ApiResult<()> {
    if user.is_owner() {
        Ok(())
    } else {
        Err(ApiError::forbidden("Nur Admins dürfen das."))
    }
}

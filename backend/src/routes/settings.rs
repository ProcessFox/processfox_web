//! Org-Settings (PLAN.md Phase 4). Default-Provider/Modell + First-Run-Flag,
//! pro Organisation. Lesen: jedes Org-Mitglied. Schreiben: Org-Owner
//! (org-weite Konfiguration, analog zu API-Keys — CLAUDE.md §4).

use axum::extract::State;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::perm::require_org_owner;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings", get(get_settings))
        .route("/settings/provider", put(set_provider))
        .route("/settings/model", put(set_model))
        .route("/settings/first-run-done", post(set_first_run_done))
}

#[derive(Deserialize)]
struct ValueBody {
    /// String oder `null` (Default löschen).
    value: Option<String>,
}

async fn load(state: &AppState, org_id: Uuid) -> ApiResult<Value> {
    let row: Option<(Option<String>, Option<String>, bool)> = sqlx::query_as(
        "SELECT default_provider, default_model, first_run_done \
         FROM org_settings WHERE org_id = $1",
    )
    .bind(org_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    let (p, m, f) = row.unwrap_or((None, None, false));
    Ok(json!({
        "defaultProvider": p,
        "defaultModel": m,
        "firstRunDone": f,
    }))
}

/// Stellt sicher, dass eine org_settings-Zeile existiert (Seed legt sie an,
/// aber robust bleiben).
async fn ensure_row(state: &AppState, org_id: Uuid) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO org_settings (org_id) VALUES ($1) \
         ON CONFLICT (org_id) DO NOTHING",
    )
    .bind(org_id)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(())
}

async fn get_settings(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    Ok(Json(load(&state, user.org_id).await?))
}

async fn set_provider(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<ValueBody>,
) -> ApiResult<Json<Value>> {
    require_org_owner(&user)?;
    ensure_row(&state, user.org_id).await?;
    sqlx::query(
        "UPDATE org_settings SET default_provider = $2, updated_at = now() \
         WHERE org_id = $1",
    )
    .bind(user.org_id)
    .bind(body.value)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(load(&state, user.org_id).await?))
}

async fn set_model(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<ValueBody>,
) -> ApiResult<Json<Value>> {
    require_org_owner(&user)?;
    ensure_row(&state, user.org_id).await?;
    sqlx::query(
        "UPDATE org_settings SET default_model = $2, updated_at = now() \
         WHERE org_id = $1",
    )
    .bind(user.org_id)
    .bind(body.value)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(load(&state, user.org_id).await?))
}

async fn set_first_run_done(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Value>> {
    require_org_owner(&user)?;
    ensure_row(&state, user.org_id).await?;
    sqlx::query(
        "UPDATE org_settings SET first_run_done = true, updated_at = now() \
         WHERE org_id = $1",
    )
    .bind(user.org_id)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(load(&state, user.org_id).await?))
}

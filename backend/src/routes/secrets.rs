//! API-Keys pro Organisation (PLAN.md Phase 4, CLAUDE.md §9/§10).
//! AES-256-GCM-verschlüsselt in der DB, **nie** im Klartext ans Frontend.
//! Setzen/Löschen/Validieren: Org-Owner. Status (`hasKey`): jedes Mitglied
//! (das Frontend braucht es, um den Chat freizuschalten).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::crypto;
use crate::error::{ApiError, ApiResult};
use crate::perm::require_org_owner;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/secrets/{provider}",
            get(has_key).post(set_key).delete(clear_key),
        )
        .route("/secrets/{provider}/validate", post(validate_key))
}

#[derive(Deserialize)]
struct SetBody {
    value: String,
}

const PROVIDERS: [&str; 3] = ["anthropic", "openai", "openrouter"];

fn check_provider(p: &str) -> ApiResult<()> {
    if PROVIDERS.contains(&p) {
        Ok(())
    } else {
        Err(ApiError::BadRequest("Unbekannter Provider.".into()))
    }
}

async fn set_key(
    State(state): State<AppState>,
    user: AuthUser,
    Path(provider): Path<String>,
    Json(body): Json<SetBody>,
) -> ApiResult<StatusCode> {
    require_org_owner(&user)?;
    check_provider(&provider)?;
    if body.value.trim().is_empty() {
        return Err(ApiError::BadRequest("Key darf nicht leer sein.".into()));
    }
    let enc = crypto::encrypt(
        &state.config.api_key_encryption_key,
        body.value.trim().as_bytes(),
    )
    .map_err(ApiError::Internal)?;
    sqlx::query(
        "INSERT INTO api_keys (org_id, provider, encrypted_key) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (org_id, provider) \
         DO UPDATE SET encrypted_key = $3, updated_at = now()",
    )
    .bind(user.org_id)
    .bind(&provider)
    .bind(enc)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn has_key(
    State(state): State<AppState>,
    user: AuthUser,
    Path(provider): Path<String>,
) -> ApiResult<Json<Value>> {
    check_provider(&provider)?;
    let row: Option<(bool,)> =
        sqlx::query_as("SELECT true FROM api_keys WHERE org_id = $1 AND provider = $2")
            .bind(user.org_id)
            .bind(&provider)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(json!({ "hasKey": row.is_some() })))
}

async fn clear_key(
    State(state): State<AppState>,
    user: AuthUser,
    Path(provider): Path<String>,
) -> ApiResult<StatusCode> {
    require_org_owner(&user)?;
    check_provider(&provider)?;
    sqlx::query("DELETE FROM api_keys WHERE org_id = $1 AND provider = $2")
        .bind(user.org_id)
        .bind(&provider)
        .execute(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn fetch_key(
    state: &AppState,
    org_id: uuid::Uuid,
    provider: &str,
) -> ApiResult<Option<String>> {
    let row: Option<(Vec<u8>,)> = sqlx::query_as(
        "SELECT encrypted_key FROM api_keys \
         WHERE org_id = $1 AND provider = $2",
    )
    .bind(org_id)
    .bind(provider)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    match row {
        None => Ok(None),
        Some((blob,)) => {
            let plain = crypto::decrypt(&state.config.api_key_encryption_key, &blob)
                .map_err(ApiError::Internal)?;
            Ok(Some(String::from_utf8_lossy(&plain).into_owned()))
        }
    }
}

/// Leichter Live-Check gegen die Provider-API (Modelle-Endpunkt).
async fn validate_key(
    State(state): State<AppState>,
    user: AuthUser,
    Path(provider): Path<String>,
) -> ApiResult<Json<Value>> {
    require_org_owner(&user)?;
    check_provider(&provider)?;
    let Some(key) = fetch_key(&state, user.org_id, &provider).await? else {
        return Ok(Json(
            json!({ "ok": false, "error": "Kein Key hinterlegt." }),
        ));
    };

    let req = match provider.as_str() {
        "anthropic" => state
            .http
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", &key)
            .header("anthropic-version", "2023-06-01"),
        "openai" => state
            .http
            .get("https://api.openai.com/v1/models")
            .bearer_auth(&key),
        _ => state
            .http
            .get("https://openrouter.ai/api/v1/models")
            .bearer_auth(&key),
    };

    match req.send().await {
        Ok(resp) if resp.status().is_success() => Ok(Json(json!({ "ok": true }))),
        Ok(resp) => Ok(Json(json!({
            "ok": false,
            "error": format!("Provider-Antwort: HTTP {}", resp.status().as_u16())
        }))),
        Err(e) => Ok(Json(json!({
            "ok": false,
            "error": format!("Verbindung fehlgeschlagen: {e}")
        }))),
    }
}

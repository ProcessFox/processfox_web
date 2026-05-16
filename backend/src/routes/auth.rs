//! Auth-Endpunkte (PLAN.md Phase 2, **passwordless Magic-Link**).
//! REST unter `/api/v1/auth/...`.
//!
//! Ablauf: E-Mail (+ bei Registrierung Org-Invite-Code) → Backend erzeugt
//! ein einmaliges Magic-Link-Token (15 min, nur als Hash in DB) → POST an
//! den n8n-Webhook (versendet die Mail) → Klick auf
//! `/auth/callback?token=…` → SPA ruft `POST /auth/verify` → Session.
//! Keine Org-Erstellung über die App (erste Org via Seed-SQL, DEPLOY.md).

use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::auth::{
    encode_access_token, generate_opaque_token, hash_token, AuthUser, ACCESS_TOKEN_TTL_SECS,
    MAGIC_LINK_TTL_SECS, REFRESH_COOKIE, REFRESH_TOKEN_TTL_SECS,
};
use crate::error::{ApiError, ApiResult};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/request-login", post(request_login))
        .route("/auth/request-register", post(request_register))
        .route("/auth/verify", post(verify))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
}

// --- DTOs -----------------------------------------------------------------

#[derive(Deserialize)]
struct RequestLoginBody {
    email: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestRegisterBody {
    email: String,
    invite_code: String,
}

#[derive(Deserialize)]
struct VerifyBody {
    token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiUser {
    id: String,
    email: String,
    org_id: String,
    org_role: String,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthSession {
    access_token: String,
    expires_in: i64,
    user: ApiUser,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    email: String,
    org_id: Uuid,
    org_role: String,
    created_at: OffsetDateTime,
}

impl UserRow {
    fn to_api(&self) -> ApiUser {
        ApiUser {
            id: self.id.to_string(),
            email: self.email.clone(),
            org_id: self.org_id.to_string(),
            org_role: self.org_role.clone(),
            created_at: self.created_at.format(&Rfc3339).unwrap_or_default(),
        }
    }
}

const USER_COLS: &str = "id, email, org_id, org_role, created_at";

// --- Helpers --------------------------------------------------------------

fn client_ip(addr: &SocketAddr) -> String {
    addr.ip().to_string()
}

fn refresh_cookie(value: &str, max_age_secs: i64) -> Cookie<'static> {
    Cookie::build((REFRESH_COOKIE, value.to_string()))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .path("/api/v1/auth")
        .max_age(time::Duration::seconds(max_age_secs))
        .build()
}

/// Generische OK-Antwort — verrät nicht, ob die E-Mail existiert.
fn generic_ok() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "message": "Wenn die Adresse gültig ist, wurde ein Link versendet."
    }))
}

async fn fetch_user_by_email(state: &AppState, email: &str) -> ApiResult<Option<UserRow>> {
    sqlx::query_as::<_, UserRow>(&format!("SELECT {USER_COLS} FROM users WHERE email = $1"))
        .bind(email)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))
}

/// Magic-Link-Token anlegen und an den n8n-Webhook pushen. Fehler werden
/// geloggt, aber nicht an den Client geleakt (kein Enumeration-Signal).
async fn create_and_send_magic_link(
    state: &AppState,
    email: &str,
    purpose: &str,
    org_id: Option<Uuid>,
) {
    let (raw, hash) = generate_opaque_token();
    let expires = OffsetDateTime::now_utc() + time::Duration::seconds(MAGIC_LINK_TTL_SECS);
    let insert = sqlx::query(
        "INSERT INTO login_tokens (email, purpose, org_id, token_hash, expires_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(email)
    .bind(purpose)
    .bind(org_id)
    .bind(&hash)
    .bind(expires)
    .execute(&state.pool)
    .await;
    if let Err(e) = insert {
        tracing::error!(error = %e, "login_token insert failed");
        return;
    }

    let link = format!(
        "{}/auth/callback?token={}",
        state.config.public_base_url, raw
    );
    let mut req = state
        .http
        .post(&state.config.magic_link_webhook_url)
        .json(&serde_json::json!({
            "email": email,
            "magicLink": link,
            "purpose": purpose,
        }));
    if let Some(secret) = &state.config.magic_link_webhook_secret {
        req = req.header("X-Webhook-Secret", secret);
    }
    match req.send().await {
        Ok(resp) if resp.status().is_success() => {}
        Ok(resp) => {
            tracing::error!(status = %resp.status(), "magic-link webhook non-2xx")
        }
        Err(e) => tracing::error!(error = %e, "magic-link webhook failed"),
    }
}

// --- Handlers -------------------------------------------------------------

async fn request_login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<RequestLoginBody>,
) -> ApiResult<Json<serde_json::Value>> {
    if !state.ratelimit.check(&format!("rl:{}", client_ip(&addr))) {
        return Err(ApiError::Conflict(
            "Zu viele Versuche — bitte später erneut.".into(),
        ));
    }
    let email = body.email.trim().to_lowercase();
    if !email.contains('@') {
        return Err(ApiError::BadRequest("Ungültige E-Mail.".into()));
    }
    // Nur senden, wenn der Account existiert — aber generisch antworten.
    if fetch_user_by_email(&state, &email).await?.is_some() {
        create_and_send_magic_link(&state, &email, "login", None).await;
    }
    Ok(generic_ok())
}

async fn request_register(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<RequestRegisterBody>,
) -> ApiResult<Json<serde_json::Value>> {
    if !state.ratelimit.check(&format!("rr:{}", client_ip(&addr))) {
        return Err(ApiError::Conflict(
            "Zu viele Versuche — bitte später erneut.".into(),
        ));
    }
    let email = body.email.trim().to_lowercase();
    if !email.contains('@') {
        return Err(ApiError::BadRequest("Ungültige E-Mail.".into()));
    }
    let code = body.invite_code.trim().to_uppercase();

    let org: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM organizations WHERE invite_code = $1")
            .bind(&code)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
    // Falscher Code darf scheitern (kein User-Enumeration-Risiko).
    let org_id = org
        .ok_or_else(|| ApiError::BadRequest("Ungültiger Einladungscode.".into()))?
        .0;

    // E-Mail schon vorhanden → still einen Login-Link schicken statt Register.
    if fetch_user_by_email(&state, &email).await?.is_some() {
        create_and_send_magic_link(&state, &email, "login", None).await;
    } else {
        create_and_send_magic_link(&state, &email, "register", Some(org_id)).await;
    }
    Ok(generic_ok())
}

async fn verify(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<VerifyBody>,
) -> ApiResult<(CookieJar, Json<AuthSession>)> {
    let hash = hash_token(body.token.trim());

    // Atomar konsumieren (Single-Use, race-frei).
    let row: Option<(String, String, Option<Uuid>)> = sqlx::query_as(
        "UPDATE login_tokens SET consumed_at = now() \
         WHERE token_hash = $1 AND consumed_at IS NULL AND expires_at > now() \
         RETURNING email, purpose, org_id",
    )
    .bind(&hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    let (email, purpose, org_id) = row.ok_or(ApiError::Unauthorized)?;

    let user = if purpose == "register" {
        let org_id = org_id.ok_or(ApiError::Unauthorized)?;
        // Falls inzwischen doch angelegt (Doppelklick): bestehenden nehmen.
        if let Some(u) = fetch_user_by_email(&state, &email).await? {
            u
        } else {
            sqlx::query_as::<_, UserRow>(&format!(
                "INSERT INTO users (email, org_id, org_role) \
                 VALUES ($1, $2, 'member') RETURNING {USER_COLS}"
            ))
            .bind(&email)
            .bind(org_id)
            .fetch_one(&state.pool)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?
        }
    } else {
        fetch_user_by_email(&state, &email)
            .await?
            .ok_or(ApiError::Unauthorized)?
    };

    let access = encode_access_token(
        &state.config.jwt_secret,
        user.id,
        user.org_id,
        &user.org_role,
    )
    .map_err(ApiError::Internal)?;

    let (raw, rhash) = generate_opaque_token();
    let expires = OffsetDateTime::now_utc() + time::Duration::seconds(REFRESH_TOKEN_TTL_SECS);
    sqlx::query(
        "INSERT INTO refresh_tokens (user_id, token_hash, expires_at) \
         VALUES ($1, $2, $3)",
    )
    .bind(user.id)
    .bind(&rhash)
    .bind(expires)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    let jar = jar.add(refresh_cookie(&raw, REFRESH_TOKEN_TTL_SECS));
    Ok((
        jar,
        Json(AuthSession {
            access_token: access,
            expires_in: ACCESS_TOKEN_TTL_SECS,
            user: user.to_api(),
        }),
    ))
}

async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
) -> ApiResult<(CookieJar, Json<AuthSession>)> {
    let raw = jar
        .get(REFRESH_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or(ApiError::Unauthorized)?;
    let hash = hash_token(&raw);

    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, user_id FROM refresh_tokens \
         WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > now()",
    )
    .bind(&hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    let (token_id, user_id) = row.ok_or(ApiError::Unauthorized)?;

    // Rotation: altes Token sofort widerrufen.
    sqlx::query("UPDATE refresh_tokens SET revoked_at = now() WHERE id = $1")
        .bind(token_id)
        .execute(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let user =
        sqlx::query_as::<_, UserRow>(&format!("SELECT {USER_COLS} FROM users WHERE id = $1"))
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?
            .ok_or(ApiError::Unauthorized)?;

    let access = encode_access_token(
        &state.config.jwt_secret,
        user.id,
        user.org_id,
        &user.org_role,
    )
    .map_err(ApiError::Internal)?;

    let (raw, rhash) = generate_opaque_token();
    let expires = OffsetDateTime::now_utc() + time::Duration::seconds(REFRESH_TOKEN_TTL_SECS);
    sqlx::query(
        "INSERT INTO refresh_tokens (user_id, token_hash, expires_at) \
         VALUES ($1, $2, $3)",
    )
    .bind(user.id)
    .bind(&rhash)
    .bind(expires)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    let jar = jar.add(refresh_cookie(&raw, REFRESH_TOKEN_TTL_SECS));
    Ok((
        jar,
        Json(AuthSession {
            access_token: access,
            expires_in: ACCESS_TOKEN_TTL_SECS,
            user: user.to_api(),
        }),
    ))
}

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> ApiResult<(CookieJar, StatusCode)> {
    if let Some(c) = jar.get(REFRESH_COOKIE) {
        let hash = hash_token(c.value());
        sqlx::query(
            "UPDATE refresh_tokens SET revoked_at = now() \
             WHERE token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(&hash)
        .execute(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    }
    let jar = jar.add(refresh_cookie("", 0));
    Ok((jar, StatusCode::NO_CONTENT))
}

/// Owner-only: neuen Invite-Code für die eigene Org erzeugen.
pub async fn rotate_invite_code(
    State(state): State<AppState>,
    user: AuthUser,
    axum::extract::Path(org_id): axum::extract::Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    if !user.is_owner() || user.org_id != org_id {
        return Err(ApiError::forbidden("Nur der Owner darf den Code ändern."));
    }
    let code = generate_invite_code();
    sqlx::query("UPDATE organizations SET invite_code = $1 WHERE id = $2")
        .bind(&code)
        .bind(org_id)
        .execute(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "inviteCode": code })))
}

/// 6 Zeichen, ohne mehrdeutige (0/O, 1/I/L). Siehe PLAN.md „Registrierung".
fn generate_invite_code() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..6)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

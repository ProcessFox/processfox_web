//! `AuthUser`-Extractor: liest das Bearer-Access-Token, validiert es und
//! stellt die Identität des Aufrufers bereit (CLAUDE.md §9, Pflicht-Check 1).

use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use uuid::Uuid;

use super::decode_access_token;
use crate::error::ApiError;
use crate::AppState;

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub org_id: Uuid,
    pub org_role: String,
}

impl AuthUser {
    pub fn is_owner(&self) -> bool {
        self.org_role == "owner"
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(ApiError::Unauthorized)?;
        let token = header
            .strip_prefix("Bearer ")
            .ok_or(ApiError::Unauthorized)?;

        let claims = decode_access_token(&state.config.jwt_secret, token)
            .map_err(|_| ApiError::Unauthorized)?;

        Ok(AuthUser {
            user_id: Uuid::parse_str(&claims.sub).map_err(|_| ApiError::Unauthorized)?,
            org_id: Uuid::parse_str(&claims.org_id).map_err(|_| ApiError::Unauthorized)?,
            org_role: claims.org_role,
        })
    }
}

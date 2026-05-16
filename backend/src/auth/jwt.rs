//! Access-Token: JWT HS256, signiert mit `JWT_SECRET`.

use anyhow::{Context, Result};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::ACCESS_TOKEN_TTL_SECS;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// User-ID (UUID als String).
    pub sub: String,
    pub org_id: String,
    /// `owner` | `member`.
    pub org_role: String,
    pub iat: i64,
    pub exp: i64,
}

pub fn encode_access_token(
    secret: &str,
    user_id: Uuid,
    org_id: Uuid,
    org_role: &str,
) -> Result<String> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        org_id: org_id.to_string(),
        org_role: org_role.to_string(),
        iat: now,
        exp: now + ACCESS_TOKEN_TTL_SECS,
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .context("JWT-Signierung fehlgeschlagen")
}

/// Validiert Signatur + Ablauf und gibt die Claims zurück.
pub fn decode_access_token(secret: &str, token: &str) -> Result<Claims> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .context("Token ungültig oder abgelaufen")?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_identity() {
        let uid = Uuid::new_v4();
        let oid = Uuid::new_v4();
        let tok = encode_access_token("supersecret-key", uid, oid, "owner").expect("encode");
        let claims = decode_access_token("supersecret-key", &tok).expect("decode");
        assert_eq!(claims.sub, uid.to_string());
        assert_eq!(claims.org_id, oid.to_string());
        assert_eq!(claims.org_role, "owner");
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let tok =
            encode_access_token("secret-a", Uuid::new_v4(), Uuid::new_v4(), "member").unwrap();
        assert!(decode_access_token("secret-b", &tok).is_err());
    }
}

//! Auth-Kern (Phase 2, **passwordless**): Magic-Link-Login, Access-Token
//! (JWT HS256, 15 min) und Refresh-Token (zufällig, serverseitig nur als
//! SHA-256-Hash gespeichert, 7 Tage, widerrufbar). Kein Passwort.
//! Siehe CLAUDE.md §4/§9, PLAN.md Phase 2.

mod extractor;
mod jwt;
mod token;

pub use extractor::AuthUser;
pub use jwt::{decode_access_token, encode_access_token, Claims};
pub use token::{generate_opaque_token, hash_token};

/// Lebensdauer des Access-Tokens (Sekunden).
pub const ACCESS_TOKEN_TTL_SECS: i64 = 15 * 60;
/// Lebensdauer des Refresh-Tokens (Sekunden).
pub const REFRESH_TOKEN_TTL_SECS: i64 = 7 * 24 * 60 * 60;
/// Lebensdauer eines Magic-Link-Tokens (Sekunden).
pub const MAGIC_LINK_TTL_SECS: i64 = 15 * 60;
/// Name des httpOnly-Refresh-Cookies.
pub const REFRESH_COOKIE: &str = "pfx_refresh";

//! Laufzeit-Konfiguration aus Umgebungsvariablen (CLAUDE.md §12).
//!
//! Fail-fast: fehlt eine Pflicht-Variable oder ist ein Secret offensichtlich
//! zu schwach (JWT-Secret < 32 Bytes, Encryption-Key kein 32-Byte-Hex), bricht
//! der Start mit einer klaren Meldung ab — kein stiller Default (PLAN.md
//! Planungslücke #8).

use anyhow::{bail, Context, Result};

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub s3: S3Config,
    pub jwt_secret: String,
    /// 32 rohe Bytes, aus einem 64-stelligen Hex-String dekodiert.
    pub api_key_encryption_key: [u8; 32],
    pub port: u16,
    /// Verzeichnis mit dem gebauten Frontend (Vite `dist/`).
    pub static_dir: String,
    /// Basis-URL der App (für Magic-Links), z. B. `https://chat.processfox.ai`.
    pub public_base_url: String,
    /// n8n-Webhook, an den Magic-Links zum Mailversand gepusht werden.
    pub magic_link_webhook_url: String,
    /// Optionales Shared-Secret (Header `X-Webhook-Secret`) für den Webhook.
    pub magic_link_webhook_secret: Option<String>,
}

#[derive(Clone, Debug)]
pub struct S3Config {
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    /// Region — MinIO ist es egal, AWS braucht sie. Default `us-east-1`.
    pub region: String,
}

fn required(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("Pflicht-Env-Var fehlt: {key}"))
}

fn optional(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn decode_hex_32(s: &str) -> Result<[u8; 32]> {
    let s = s.trim();
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("API_KEY_ENCRYPTION_KEY muss ein 64-stelliger Hex-String (32 Bytes) sein");
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = (chunk[0] as char).to_digit(16).unwrap() as u8;
        let lo = (chunk[1] as char).to_digit(16).unwrap() as u8;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let jwt_secret = required("JWT_SECRET")?;
        if jwt_secret.len() < 32 {
            bail!("JWT_SECRET muss mindestens 32 Zeichen lang sein");
        }

        let api_key_encryption_key = decode_hex_32(&required("API_KEY_ENCRYPTION_KEY")?)?;

        let port: u16 = optional("PORT", "3000")
            .parse()
            .context("PORT ist keine gültige Portnummer")?;

        Ok(Self {
            database_url: required("DATABASE_URL")?,
            s3: S3Config {
                endpoint: required("S3_ENDPOINT")?,
                bucket: required("S3_BUCKET")?,
                access_key: required("S3_ACCESS_KEY")?,
                secret_key: required("S3_SECRET_KEY")?,
                region: optional("S3_REGION", "us-east-1"),
            },
            jwt_secret,
            api_key_encryption_key,
            port,
            static_dir: optional("STATIC_DIR", "/app/static"),
            public_base_url: required("PUBLIC_BASE_URL")?
                .trim_end_matches('/')
                .to_string(),
            magic_link_webhook_url: required("MAGIC_LINK_WEBHOOK_URL")?,
            magic_link_webhook_secret: std::env::var("MAGIC_LINK_WEBHOOK_SECRET")
                .ok()
                .filter(|s| !s.is_empty()),
        })
    }
}

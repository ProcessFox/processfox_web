//! Opake Tokens (Refresh- **und** Magic-Link-Token): 32 zufällige Bytes,
//! base64url-kodiert. Der Klartext geht nur an den Client (Cookie bzw.
//! Magic-Link-URL); serverseitig wird ausschließlich der SHA-256-Hash
//! gespeichert (PLAN.md Planungslücke #1).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Liefert `(klartext, sha256_hex)`.
pub fn generate_opaque_token() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let raw = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_token(&raw);
    (raw, hash)
}

pub fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    let digest = hasher.finalize();
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic_and_64_hex() {
        let h = hash_token("hello");
        assert_eq!(h, hash_token("hello"));
        assert_eq!(h.len(), 64);
        assert!(h.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn generated_token_matches_its_hash_and_is_unique() {
        let (raw1, hash1) = generate_opaque_token();
        let (raw2, _) = generate_opaque_token();
        assert_eq!(hash1, hash_token(&raw1));
        assert_ne!(raw1, raw2, "Tokens müssen zufällig/eindeutig sein");
    }
}

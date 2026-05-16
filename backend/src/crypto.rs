//! AES-256-GCM für API-Key-Verschlüsselung (CLAUDE.md §9). Speicherformat:
//! `nonce(12) || ciphertext+tag`. Schlüssel = `API_KEY_ENCRYPTION_KEY`
//! (32 Byte). Klartext-Keys verlassen nie die DB / nie das Frontend.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{anyhow, Result};
use rand::RngCore;

pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .map_err(|e| anyhow!("AES-Encrypt fehlgeschlagen: {e}"))?;
    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn decrypt(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 12 {
        return Err(anyhow!("Chiffrat zu kurz"));
    }
    let (nonce, ct) = data.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    cipher
        .decrypt(Nonce::from_slice(nonce), ct)
        .map_err(|e| anyhow!("AES-Decrypt fehlgeschlagen: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_tamper_detection() {
        let key = [7u8; 32];
        let enc = encrypt(&key, b"sk-secret-123").unwrap();
        assert_ne!(&enc[12..], b"sk-secret-123"); // wirklich verschlüsselt
        assert_eq!(decrypt(&key, &enc).unwrap(), b"sk-secret-123");

        let mut bad = enc.clone();
        *bad.last_mut().unwrap() ^= 0xff;
        assert!(decrypt(&key, &bad).is_err()); // GCM-Tag schlägt an

        let other = [9u8; 32];
        assert!(decrypt(&other, &enc).is_err()); // falscher Schlüssel
    }
}

//! The sync file on Google Drive is encrypted before it leaves the device (AES-256-GCM,
//! a random 96-bit nonce per upload). Even someone with access to the Google account sees
//! only noise. The key is on the devices only: in the credential store, and it reaches the
//! phone as a QR code shown for a moment on the PC screen - never over the internet.

use std::io::{Read, Write};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};

use crate::error::{AppError, AppResult};

const MAGIC: &[u8; 4] = b"TOF1";
const NONCE_LEN: usize = 12;
/// A decompressed sync file larger than this is refused (protects against a "zip bomb").
const MAX_PLAIN: u64 = 200 * 1024 * 1024;

pub fn new_key() -> AppResult<[u8; 32]> {
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).map_err(|e| AppError::Secret(e.to_string()))?;
    Ok(key)
}

pub fn key_to_hex(key: &[u8; 32]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn key_from_hex(text: &str) -> AppResult<[u8; 32]> {
    let text = text.trim();
    if text.len() != 64 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::Invalid("the sync key has a wrong format".into()));
    }
    let mut key = [0u8; 32];
    for (i, chunk) in text.as_bytes().chunks(2).enumerate() {
        key[i] = u8::from_str_radix(std::str::from_utf8(chunk).unwrap_or("00"), 16).unwrap_or(0);
    }
    Ok(key)
}

/// Compressed (gzip), then encrypted: `TOF1` + nonce + ciphertext with tag.
pub fn seal(key: &[u8; 32], plain: &[u8]) -> AppResult<Vec<u8>> {
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(plain)?;
    let packed = gz.finish()?;
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(|e| AppError::Secret(e.to_string()))?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| AppError::Secret(e.to_string()))?;
    let sealed = cipher.encrypt(&Nonce::from(nonce), packed.as_ref()).map_err(|_| AppError::Secret("encryption failed".into()))?;
    let mut out = Vec::with_capacity(4 + NONCE_LEN + sealed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Fails with `Invalid` for a wrong key or a damaged / tampered file (GCM checks every byte).
pub fn open(key: &[u8; 32], data: &[u8]) -> AppResult<Vec<u8>> {
    if data.len() < 4 + NONCE_LEN + 16 || &data[..4] != MAGIC {
        return Err(AppError::Invalid("the sync file is not a Tablica Ofert file".into()));
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| AppError::Secret(e.to_string()))?;
    let packed = cipher
        .decrypt(
            &Nonce::try_from(&data[4..4 + NONCE_LEN]).map_err(|_| AppError::Invalid("damaged sync file".into()))?,
            &data[4 + NONCE_LEN..],
        )
        .map_err(|_| AppError::Invalid("the sync file cannot be read with this key - connect the devices again".into()))?;
    let mut plain = Vec::new();
    flate2::read::GzDecoder::new(packed.as_slice()).take(MAX_PLAIN + 1).read_to_end(&mut plain)?;
    if plain.len() as u64 > MAX_PLAIN {
        return Err(AppError::Invalid("the sync file is too big".into()));
    }
    Ok(plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_wrong_key_and_tampering() {
        let key = new_key().unwrap();
        let text = "{\"offers\": [\"Frontend Developer\"]}".repeat(50);
        let sealed = seal(&key, text.as_bytes()).unwrap();
        assert!(!sealed.windows(8).any(|w| w == b"Frontend"));
        assert_eq!(open(&key, &sealed).unwrap(), text.as_bytes());
        // Two uploads of the same data look different (fresh nonce).
        assert_ne!(seal(&key, text.as_bytes()).unwrap(), sealed);

        let other = new_key().unwrap();
        assert!(open(&other, &sealed).is_err());
        let mut broken = sealed.clone();
        let last = broken.len() - 1;
        broken[last] ^= 1;
        assert!(open(&key, &broken).is_err());
        assert!(open(&key, b"TOF1short").is_err());
    }

    #[test]
    fn key_text() {
        let key = new_key().unwrap();
        assert_eq!(key_from_hex(&key_to_hex(&key)).unwrap(), key);
        assert!(key_from_hex("zz").is_err());
    }
}

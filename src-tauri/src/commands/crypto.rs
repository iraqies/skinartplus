use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};

const PREFIX: &str = "enc:";

fn machine_id() -> String {
    machine_uid::get().unwrap_or_else(|_| "unknown-machine".to_string())
}

fn derive_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    let hash = Sha256::digest(machine_id().as_bytes());
    key.copy_from_slice(&hash);
    key
}

pub fn encrypt_tokens(plaintext: &str) -> Result<String, String> {
    let cipher = Aes256Gcm::new_from_slice(&derive_key()).map_err(|e| e.to_string())?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut out = nonce_bytes.to_vec();
    out.extend_from_slice(&ct);
    Ok(format!("{}{}", PREFIX, B64.encode(&out)))
}

pub fn decrypt_tokens(encoded: &str) -> Result<String, String> {
    let body = encoded.strip_prefix(PREFIX).ok_or("not encrypted")?;
    let raw = B64.decode(body).map_err(|e| e.to_string())?;
    if raw.len() < 12 {
        return Err("bad ciphertext".into());
    }
    let (nonce_bytes, ct) = raw.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(&derive_key()).map_err(|e| e.to_string())?;
    let pt = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ct)
        .map_err(|_| "decrypt failed (key mismatch?)".to_string())?;
    String::from_utf8(pt).map_err(|e| e.to_string())
}

pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(PREFIX)
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let enc = encrypt_tokens("hello-token-123").unwrap();
        let dec = decrypt_tokens(&enc).unwrap();
        assert_eq!(dec, "hello-token-123");
        println!("machine={} enc={} dec={}", machine_id(), enc, dec);
    }
}

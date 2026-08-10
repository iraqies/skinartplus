// Minecraft login crypto: shared secret, Mojang server-id hash, RSA PKCS#1v15.

use rand::RngCore;
use rsa::pkcs8::DecodePublicKey;
use rsa::{Pkcs1v15Encrypt, RsaPublicKey};
use sha1::{Digest, Sha1};

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Minecraft's "Mojang digest": interpret the sha1 digest as a signed
/// big-endian integer and render it as (optionally negative) hex.
/// Mirror of Cryptomanager.encrypt: new BigInteger(digest).toString(16).
pub fn mojang_digest(bytes: &[u8; 20]) -> String {
    let negative = bytes[0] & 0x80 != 0;
    let mut v = bytes.to_vec();
    if negative {
        for b in v.iter_mut() {
            *b = !*b;
        }
        let mut carry = 1u16;
        for b in v.iter_mut().rev() {
            let sum = (*b as u16) + carry;
            *b = (sum & 0xFF) as u8;
            carry = sum >> 8;
            if carry == 0 {
                break;
            }
        }
    }
    let mut start = 0;
    while start < v.len() - 1 && v[start] == 0 {
        start += 1;
    }
    let mut s = hex(&v[start..]);
    if negative {
        s = format!("-{}", s);
    }
    s
}

pub fn compute_server_id(
    server_id: &str,
    shared_secret: &[u8; 16],
    public_key: &[u8],
) -> String {
    let mut h = Sha1::new();
    h.update(server_id.as_bytes());
    h.update(&shared_secret[..]);
    h.update(public_key);
    let out = h.finalize();
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&out);
    mojang_digest(&arr)
}

pub struct EncryptedLogin {
    pub encrypted_secret: Vec<u8>,
    pub encrypted_verify_token: Vec<u8>,
}

pub fn encrypt_login(
    public_key_der: &[u8],
    shared_secret: &[u8; 16],
    verify_token: &[u8],
) -> Result<EncryptedLogin, String> {
    let pub_key = RsaPublicKey::from_public_key_der(public_key_der).map_err(|e| e.to_string())?;
    let mut rng = rand::thread_rng();
    let encrypted_secret = pub_key
        .encrypt(&mut rng, Pkcs1v15Encrypt, shared_secret)
        .map_err(|e| e.to_string())?;
    let encrypted_verify_token = pub_key
        .encrypt(&mut rng, Pkcs1v15Encrypt, verify_token)
        .map_err(|e| e.to_string())?;
    Ok(EncryptedLogin {
        encrypted_secret,
        encrypted_verify_token,
    })
}

pub fn generate_shared_secret() -> [u8; 16] {
    let mut key = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

/// Strip dashes from a UUID string (the form expected by Mojang's
/// session server `selectedProfile` field).
pub fn uuid_without_dashes(uuid: &str) -> String {
    uuid.chars().filter(|c| *c != '-').collect()
}

/// Convert a UUID string (with or without dashes) into its raw 16 bytes,
/// as written in the 1.19.1+ login start packet.
pub fn uuid_bytes(uuid: &str) -> [u8; 16] {
    let cleaned: String = uuid.chars().filter(|c| *c != '-').collect();
    let mut out = [0u8; 16];
    if cleaned.len() == 32 {
        for i in 0..16 {
            out[i] = u8::from_str_radix(&cleaned[i * 2..i * 2 + 2], 16).unwrap_or(0);
        }
    }
    out
}

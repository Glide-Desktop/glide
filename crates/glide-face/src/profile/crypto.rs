use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;

use super::ProfileError;

pub const SALT_SIZE: usize = 16;
pub const NONCE_SIZE: usize = 12;
pub const MASTER_KEY_SIZE: usize = 32;
const TAG_SIZE: usize = 16;
const STRETCH_ROUNDS: usize = 50_000;

type HmacSha256 = Hmac<Sha256>;

fn hmac(key: &[u8]) -> HmacSha256 {
    <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts any key length")
}

#[must_use]
pub fn machine_id() -> Vec<u8> {
    for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
        if let Ok(data) = std::fs::read_to_string(path) {
            let t = data.trim();
            if !t.is_empty() {
                return t.as_bytes().to_vec();
            }
        }
    }
    if let Ok(host) = nix::unistd::gethostname() {
        let host = host.to_string_lossy();
        if !host.is_empty() {
            return format!("glide-face-{host}").into_bytes();
        }
    }
    b"glide-face-biometric-host".to_vec()
}

#[must_use]
pub fn generate_master_key() -> Vec<u8> {
    let mut key = vec![0u8; MASTER_KEY_SIZE];
    rand::rngs::OsRng.fill_bytes(&mut key);
    key
}

pub fn host_bound_secret(raw_key: &[u8]) -> Result<Vec<u8>, ProfileError> {
    host_bound_secret_with(raw_key, &machine_id())
}

pub fn host_bound_secret_with(raw_key: &[u8], machine_id: &[u8]) -> Result<Vec<u8>, ProfileError> {
    if raw_key.len() < MASTER_KEY_SIZE {
        return Err(ProfileError::Corrupt("master key is truncated or corrupted".into()));
    }
    let mut mac = hmac(&raw_key[..MASTER_KEY_SIZE]);
    mac.update(machine_id);
    Ok(mac.finalize().into_bytes().to_vec())
}

#[must_use]
pub fn build_aad(uid: u32, username: &str) -> Vec<u8> {
    format!("glide-face:profile:v3:uid={uid}:user={username}").into_bytes()
}

#[must_use]
pub fn derive_key(secret: &[u8], salt: &[u8]) -> [u8; 32] {
    let mut mac = hmac(secret);
    mac.update(salt);
    let mut key: [u8; 32] = mac.finalize().into_bytes().into();
    for _ in 0..STRETCH_ROUNDS {
        let mut mac = hmac(secret);
        mac.update(&key);
        mac.update(salt);
        key = mac.finalize().into_bytes().into();
    }
    key
}

pub fn encrypt_with(
    secret: &[u8], plaintext: &[u8], aad: &[u8], salt: &[u8; SALT_SIZE], nonce: &[u8; NONCE_SIZE],
) -> Result<Vec<u8>, ProfileError> {
    let cipher = Aes256Gcm::new_from_slice(&derive_key(secret, salt))
        .map_err(|e| ProfileError::Crypto(format!("cipher init failed: {e}")))?;
    let ct = cipher
        .encrypt(Nonce::from_slice(nonce), Payload { msg: plaintext, aad })
        .map_err(|_| ProfileError::Crypto("encryption failed".into()))?;
    let mut out = Vec::with_capacity(SALT_SIZE + NONCE_SIZE + ct.len());
    out.extend_from_slice(salt);
    out.extend_from_slice(nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn encrypt(secret: &[u8], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, ProfileError> {
    let mut salt = [0u8; SALT_SIZE];
    let mut nonce = [0u8; NONCE_SIZE];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    encrypt_with(secret, plaintext, aad, &salt, &nonce)
}

pub fn decrypt(secret: &[u8], data: &[u8], aad: &[u8]) -> Result<Vec<u8>, ProfileError> {
    if data.len() < SALT_SIZE + NONCE_SIZE + TAG_SIZE {
        return Err(ProfileError::Corrupt("invalid or corrupted encrypted data".into()));
    }
    let (salt, rest) = data.split_at(SALT_SIZE);
    let (nonce, ct) = rest.split_at(NONCE_SIZE);
    let cipher = Aes256Gcm::new_from_slice(&derive_key(secret, salt))
        .map_err(|e| ProfileError::Crypto(format!("cipher init failed: {e}")))?;
    cipher
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad })
        .map_err(|_| ProfileError::Crypto("decryption failed (integrity or AAD check failed)".into()))
}

#[must_use]
pub fn template_hash(secret: &[u8], embedding: &[f32]) -> String {
    let mut mac = hmac(secret);
    for v in embedding {
        mac.update(&v.to_bits().to_le_bytes());
    }
    hex::encode(mac.finalize().into_bytes())[..16].to_string()
}

use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use rsa::{
    Oaep, RsaPrivateKey, RsaPublicKey, pkcs8::DecodePrivateKey, pkcs8::EncodePrivateKey, pkcs8::EncodePublicKey,
};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

type HmacSha256 = Hmac<Sha256>;
type Encryptor = cbc::Encryptor<aes::Aes256>;
type Decryptor = cbc::Decryptor<aes::Aes256>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    Malformed,
    Mac,
    Decrypt,
    Kdf,
    Rsa,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Malformed => "a malformed encrypted value",
            Self::Mac => "an encrypted value failed its integrity check",
            Self::Decrypt => "an encrypted value could not be decrypted",
            Self::Kdf => "the key derivation parameters are not usable",
            Self::Rsa => "an RSA key could not be used",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Kdf {
    Pbkdf2 { iterations: u32 },
    Argon2id { iterations: u32, memory_mib: u32, parallelism: u32 },
}

impl Kdf {
    pub fn from_prelogin(
        kind: u8, iterations: u32, memory: Option<u32>, parallelism: Option<u32>,
    ) -> Result<Self, CryptoError> {
        match kind {
            0 if iterations >= 5_000 => Ok(Self::Pbkdf2 { iterations }),
            1 if iterations >= 1 => Ok(Self::Argon2id {
                iterations,
                memory_mib: memory.filter(|memory| (15..=1024).contains(memory)).ok_or(CryptoError::Kdf)?,
                parallelism: parallelism.filter(|lanes| (1..=16).contains(lanes)).ok_or(CryptoError::Kdf)?,
            }),
            _ => Err(CryptoError::Kdf),
        }
    }
}

#[must_use]
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

pub fn master_key(password: &[u8], email: &str, kdf: Kdf) -> Result<Zeroizing<[u8; 32]>, CryptoError> {
    derive_key(password, normalize_email(email).as_bytes(), kdf)
}

pub fn derive_key(password: &[u8], salt: &[u8], kdf: Kdf) -> Result<Zeroizing<[u8; 32]>, CryptoError> {
    let mut key = Zeroizing::new([0u8; 32]);
    match kdf {
        Kdf::Pbkdf2 { iterations } => {
            pbkdf2::pbkdf2_hmac::<Sha256>(password, salt, iterations, key.as_mut());
        }
        Kdf::Argon2id { iterations, memory_mib, parallelism } => {
            let salt = Sha256::digest(salt);
            let params = argon2::Params::new(memory_mib * 1024, iterations, parallelism, Some(32))
                .map_err(|_| CryptoError::Kdf)?;
            argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
                .hash_password_into(password, &salt, key.as_mut())
                .map_err(|_| CryptoError::Kdf)?;
        }
    }
    Ok(key)
}

#[must_use]
pub fn master_password_hash(master_key: &[u8; 32], password: &[u8]) -> String {
    let mut hash = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<Sha256>(master_key, password, 1, hash.as_mut());
    STANDARD.encode(hash.as_ref())
}

#[derive(Clone)]
pub struct SymmetricKey {
    enc: Zeroizing<[u8; 32]>,
    mac: Option<Zeroizing<[u8; 32]>>,
}

impl std::fmt::Debug for SymmetricKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SymmetricKey(..)")
    }
}

impl SymmetricKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        match bytes.len() {
            64 => {
                let mut enc = Zeroizing::new([0u8; 32]);
                let mut mac = Zeroizing::new([0u8; 32]);
                enc.copy_from_slice(&bytes[..32]);
                mac.copy_from_slice(&bytes[32..]);
                Ok(Self { enc, mac: Some(mac) })
            }
            32 => {
                let mut enc = Zeroizing::new([0u8; 32]);
                enc.copy_from_slice(bytes);
                Ok(Self { enc, mac: None })
            }
            _ => Err(CryptoError::Malformed),
        }
    }

    pub fn generate() -> Result<Self, CryptoError> {
        let mut bytes = Zeroizing::new([0u8; 64]);
        getrandom::fill(bytes.as_mut()).map_err(|_| CryptoError::Decrypt)?;
        Self::from_bytes(bytes.as_ref())
    }

    #[must_use]
    pub fn stretch(master_key: &[u8; 32]) -> Self {
        let hkdf = hkdf::Hkdf::<Sha256>::from_prk(master_key).expect("a 32-byte PRK is long enough");
        let mut enc = Zeroizing::new([0u8; 32]);
        let mut mac = Zeroizing::new([0u8; 32]);
        hkdf.expand(b"enc", enc.as_mut()).expect("32 bytes is a valid HKDF length");
        hkdf.expand(b"mac", mac.as_mut()).expect("32 bytes is a valid HKDF length");
        Self { enc, mac: Some(mac) }
    }

    #[must_use]
    pub fn send_key(secret: &[u8]) -> Self {
        let hkdf = hkdf::Hkdf::<Sha256>::new(Some(b"bitwarden-send"), secret);
        let mut bytes = Zeroizing::new([0u8; 64]);
        hkdf.expand(b"send", bytes.as_mut()).expect("64 bytes is a valid HKDF length");
        Self::from_bytes(bytes.as_ref()).expect("64 bytes")
    }

    #[must_use]
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut bytes = Zeroizing::new(self.enc.to_vec());
        if let Some(mac) = &self.mac {
            bytes.extend_from_slice(mac.as_ref());
        }
        bytes
    }

    pub fn encrypt(&self, plain: &[u8]) -> Result<String, CryptoError> {
        let mac_key = self.mac.as_ref().ok_or(CryptoError::Malformed)?;
        let mut iv = [0u8; 16];
        getrandom::fill(&mut iv).map_err(|_| CryptoError::Decrypt)?;
        let data = Encryptor::new(self.enc.as_ref().into(), &iv.into()).encrypt_padded_vec_mut::<Pkcs7>(plain);
        let mut mac = HmacSha256::new_from_slice(mac_key.as_ref()).map_err(|_| CryptoError::Malformed)?;
        mac.update(&iv);
        mac.update(&data);
        let tag = mac.finalize().into_bytes();
        Ok(format!("2.{}|{}|{}", STANDARD.encode(iv), STANDARD.encode(data), STANDARD.encode(tag)))
    }

    pub fn encrypt_text(&self, plain: &str) -> Result<String, CryptoError> {
        self.encrypt(plain.as_bytes())
    }

    pub fn decrypt(&self, value: &str) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        let (kind, rest) = value.split_once('.').ok_or(CryptoError::Malformed)?;
        let parts: Vec<&str> = rest.split('|').collect();
        let decode = |part: &str| STANDARD.decode(part).map_err(|_| CryptoError::Malformed);
        let (iv, data) = match (kind, parts.as_slice()) {
            ("2", [iv, data, tag]) => {
                let (iv, data, tag) = (decode(iv)?, decode(data)?, decode(tag)?);
                let mac_key = self.mac.as_ref().ok_or(CryptoError::Mac)?;
                let mut mac = HmacSha256::new_from_slice(mac_key.as_ref()).map_err(|_| CryptoError::Mac)?;
                mac.update(&iv);
                mac.update(&data);
                mac.verify_slice(&tag).map_err(|_| CryptoError::Mac)?;
                (iv, data)
            }
            ("0", [iv, data]) if self.mac.is_none() => (decode(iv)?, decode(data)?),
            _ => return Err(CryptoError::Malformed),
        };
        let iv: [u8; 16] = iv.try_into().map_err(|_| CryptoError::Malformed)?;
        Decryptor::new(self.enc.as_ref().into(), &iv.into())
            .decrypt_padded_vec_mut::<Pkcs7>(&data)
            .map(Zeroizing::new)
            .map_err(|_| CryptoError::Decrypt)
    }

    pub fn decrypt_text(&self, value: &str) -> Result<String, CryptoError> {
        let bytes = self.decrypt(value)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| CryptoError::Decrypt)
    }

    pub fn decrypt_key(&self, value: &str) -> Result<Self, CryptoError> {
        Self::from_bytes(&self.decrypt(value)?)
    }
}

pub fn decrypt_optional(key: &SymmetricKey, value: Option<&str>) -> Result<String, CryptoError> {
    match value {
        Some(value) if !value.is_empty() => key.decrypt_text(value),
        _ => Ok(String::new()),
    }
}

pub struct PrivateKey(RsaPrivateKey);

impl std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrivateKey(..)")
    }
}

impl PrivateKey {
    pub fn from_encrypted(user_key: &SymmetricKey, value: &str) -> Result<Self, CryptoError> {
        let der = user_key.decrypt(value)?;
        RsaPrivateKey::from_pkcs8_der(&der).map(Self).map_err(|_| CryptoError::Rsa)
    }

    pub fn decrypt_key(&self, value: &str) -> Result<SymmetricKey, CryptoError> {
        let (kind, rest) = value.split_once('.').ok_or(CryptoError::Malformed)?;
        let data = STANDARD.decode(rest.split('|').next().unwrap_or_default()).map_err(|_| CryptoError::Malformed)?;
        let plain = match kind {
            "4" | "6" => self.0.decrypt(Oaep::new::<sha1::Sha1>(), &data),
            "3" | "5" => self.0.decrypt(Oaep::new::<Sha256>(), &data),
            _ => return Err(CryptoError::Malformed),
        }
        .map_err(|_| CryptoError::Rsa)?;
        SymmetricKey::from_bytes(&Zeroizing::new(plain))
    }
}

pub struct KeyPair {
    pub public: String,
    pub private_encrypted: String,
}

pub fn generate_key_pair(user_key: &SymmetricKey) -> Result<KeyPair, CryptoError> {
    let mut rng = rsa::rand_core::OsRng;
    let private = RsaPrivateKey::new(&mut rng, 2048).map_err(|_| CryptoError::Rsa)?;
    let public = RsaPublicKey::from(&private).to_public_key_der().map_err(|_| CryptoError::Rsa)?;
    let der = private.to_pkcs8_der().map_err(|_| CryptoError::Rsa)?;
    Ok(KeyPair { public: STANDARD.encode(public.as_bytes()), private_encrypted: user_key.encrypt(der.as_bytes())? })
}

#[must_use]
pub fn send_password_hash(password: &str, send_secret: &[u8]) -> String {
    let mut hash = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), send_secret, 100_000, hash.as_mut());
    STANDARD.encode(hash.as_ref())
}

#[must_use]
pub fn url_b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn random_bytes<const N: usize>() -> Result<[u8; N], CryptoError> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|_| CryptoError::Decrypt)?;
    Ok(bytes)
}

pub fn uuid_v4() -> Result<String, CryptoError> {
    let mut bytes = random_bytes::<16>()?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = data_encoding::HEXLOWER.encode(&bytes);
    Ok(format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_round_trips_and_any_tampering_is_refused() {
        let key = SymmetricKey::generate().expect("key");
        let sealed = key.encrypt_text("hunter2").expect("encrypt");
        assert!(sealed.starts_with("2."));
        assert_eq!(key.decrypt_text(&sealed).expect("decrypt"), "hunter2");
        let mut parts: Vec<String> = sealed[2..].split('|').map(str::to_string).collect();
        let mut data = STANDARD.decode(&parts[1]).expect("b64");
        data[0] ^= 1;
        parts[1] = STANDARD.encode(data);
        assert_eq!(key.decrypt(&format!("2.{}", parts.join("|"))), Err(CryptoError::Mac));
        let other = SymmetricKey::generate().expect("key");
        assert_eq!(other.decrypt(&sealed), Err(CryptoError::Mac));
    }

    #[test]
    fn the_master_key_follows_the_kdf_and_the_email_case() {
        let pbkdf2 = Kdf::Pbkdf2 { iterations: 5_000 };
        let lower = master_key(b"password", "user@example.com", pbkdf2).expect("key");
        let upper = master_key(b"password", " User@Example.com ", pbkdf2).expect("key");
        assert_eq!(*lower, *upper);
        let argon = Kdf::Argon2id { iterations: 1, memory_mib: 16, parallelism: 1 };
        assert_ne!(*master_key(b"password", "user@example.com", argon).expect("key"), *lower);
        assert_eq!(master_password_hash(&lower, b"password"), master_password_hash(&upper, b"password"));
    }

    #[test]
    fn kdf_parameters_out_of_range_are_refused() {
        assert!(Kdf::from_prelogin(0, 600_000, None, None).is_ok());
        assert!(Kdf::from_prelogin(0, 10, None, None).is_err());
        assert!(Kdf::from_prelogin(1, 3, Some(64), Some(4)).is_ok());
        assert!(Kdf::from_prelogin(1, 3, None, Some(4)).is_err());
        assert!(Kdf::from_prelogin(7, 3, None, None).is_err());
    }

    #[test]
    fn a_user_key_wrapped_by_the_stretched_master_key_unwraps() {
        let master = master_key(b"password", "user@example.com", Kdf::Pbkdf2 { iterations: 5_000 }).expect("key");
        let stretched = SymmetricKey::stretch(&master);
        let user = SymmetricKey::generate().expect("key");
        let wrapped = stretched.encrypt(&user.to_bytes()).expect("wrap");
        assert_eq!(*stretched.decrypt_key(&wrapped).expect("unwrap").to_bytes(), *user.to_bytes());
    }

    #[test]
    fn organisation_keys_unwrap_with_the_private_key() {
        let user = SymmetricKey::generate().expect("key");
        let pair = generate_key_pair(&user).expect("pair");
        let private = PrivateKey::from_encrypted(&user, &pair.private_encrypted).expect("private");
        let public = rsa::pkcs8::DecodePublicKey::from_public_key_der(&STANDARD.decode(&pair.public).expect("b64"));
        let public: RsaPublicKey = public.expect("public");
        let org = SymmetricKey::generate().expect("key");
        let sealed =
            public.encrypt(&mut rsa::rand_core::OsRng, Oaep::new::<sha1::Sha1>(), &org.to_bytes()).expect("encrypt");
        let wrapped = format!("4.{}", STANDARD.encode(sealed));
        assert_eq!(*private.decrypt_key(&wrapped).expect("unwrap").to_bytes(), *org.to_bytes());
    }

    #[test]
    fn send_keys_and_uuids_have_the_expected_shape() {
        let send = SymmetricKey::send_key(&[1u8; 16]);
        assert_eq!(send.to_bytes().len(), 64);
        let id = uuid_v4().expect("uuid");
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert_eq!(send_password_hash("pw", &[1u8; 16]).len(), 44);
    }
}

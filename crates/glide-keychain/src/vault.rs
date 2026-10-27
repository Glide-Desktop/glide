use std::collections::BTreeMap;
use std::fmt;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use glide_ipc::keychain::{AppLink, Site};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const MAGIC: &[u8; 4] = b"GLKC";
const VERSION: u8 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const HEADER_LEN: usize = 4 + 1 + 4 + 4 + 4 + SALT_LEN + NONCE_LEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Params {
    pub memory_kib: u32,
    pub passes: u32,
    pub lanes: u32,
}

impl Params {
    pub const DEFAULT: Self = Self { memory_kib: 64 * 1024, passes: 3, lanes: 1 };

    const fn plausible(self) -> bool {
        self.memory_kib <= 1024 * 1024 && matches!(self.passes, 1..=16) && matches!(self.lanes, 1..=16)
    }
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Secret(Vec<u8>);

impl Secret {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret({} bytes)", self.0.len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
    pub collection: String,
    pub label: String,
    pub attributes: BTreeMap<String, String>,
    pub secret: Secret,
    pub content_type: String,
    pub created: u64,
    pub modified: u64,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub sites: Vec<Site>,
    #[serde(default)]
    pub apps: Vec<AppLink>,
    #[serde(default)]
    pub fields: Vec<StoredField>,
    #[serde(default)]
    pub totp: Secret,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub deleted: Option<u64>,
    #[serde(default)]
    pub history: Vec<PastSecret>,
    #[serde(default)]
    pub card: Option<StoredCard>,
}

impl Item {
    #[must_use]
    pub fn new(collection: &str, label: &str, now: u64) -> Self {
        Self {
            id: 0,
            collection: collection.into(),
            label: label.into(),
            attributes: BTreeMap::new(),
            secret: Secret::default(),
            content_type: "text/plain".into(),
            created: now,
            modified: now,
            notes: String::new(),
            sites: Vec::new(),
            apps: Vec::new(),
            fields: Vec::new(),
            totp: Secret::default(),
            folder: String::new(),
            favorite: false,
            deleted: None,
            history: Vec::new(),
            card: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredField {
    pub name: String,
    pub value: Secret,
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PastSecret {
    pub secret: Secret,
    pub until: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCard {
    pub holder: String,
    pub brand: String,
    pub number: Secret,
    pub code: Secret,
    pub expiry: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contents {
    pub items: Vec<Item>,
    pub next_id: u64,
    #[serde(default)]
    pub folders: Vec<String>,
}

impl Contents {
    pub fn add(&mut self, mut item: Item) -> u64 {
        self.next_id += 1;
        item.id = self.next_id;
        self.items.push(item);
        self.next_id
    }

    #[must_use]
    pub fn get(&self, id: u64) -> Option<&Item> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Item> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    pub fn remove(&mut self, id: u64) -> Option<Item> {
        let index = self.items.iter().position(|item| item.id == id)?;
        Some(self.items.remove(index))
    }

    pub fn search<'a>(&'a self, wanted: &'a BTreeMap<String, String>) -> impl Iterator<Item = &'a Item> + 'a {
        self.items.iter().filter(move |item| wanted.iter().all(|(key, value)| item.attributes.get(key) == Some(value)))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    WrongPassword,
    Corrupt,
    UnknownVersion(u8),
    Crypto,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongPassword => f.write_str("the password is wrong or the keychain was changed"),
            Self::Corrupt => f.write_str("the keychain file is damaged"),
            Self::UnknownVersion(version) => write!(f, "the keychain file has unknown version {version}"),
            Self::Crypto => f.write_str("the keychain could not be encrypted"),
        }
    }
}

impl std::error::Error for Error {}

pub struct Key {
    bytes: Zeroizing<[u8; 32]>,
    salt: [u8; SALT_LEN],
    params: Params,
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Key").field("params", &self.params).finish_non_exhaustive()
    }
}

impl Key {
    pub fn derive(password: &[u8], params: Params) -> Result<Self, Error> {
        let mut salt = [0; SALT_LEN];
        getrandom::fill(&mut salt).map_err(|_| Error::Crypto)?;
        Self::derive_with(password, salt, params)
    }

    fn derive_with(password: &[u8], salt: [u8; SALT_LEN], params: Params) -> Result<Self, Error> {
        let argon_params = argon2::Params::new(params.memory_kib, params.passes, params.lanes, Some(32))
            .map_err(|_| Error::Corrupt)?;
        let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, argon_params);
        let mut bytes = Zeroizing::new([0; 32]);
        argon.hash_password_into(password, &salt, bytes.as_mut()).map_err(|_| Error::Crypto)?;
        Ok(Self { bytes, salt, params })
    }

    fn header(&self, nonce: &[u8; NONCE_LEN]) -> Vec<u8> {
        let mut header = Vec::with_capacity(HEADER_LEN);
        header.extend_from_slice(MAGIC);
        header.push(VERSION);
        header.extend_from_slice(&self.params.memory_kib.to_le_bytes());
        header.extend_from_slice(&self.params.passes.to_le_bytes());
        header.extend_from_slice(&self.params.lanes.to_le_bytes());
        header.extend_from_slice(&self.salt);
        header.extend_from_slice(nonce);
        header
    }
}

pub fn seal(contents: &Contents, key: &Key) -> Result<Vec<u8>, Error> {
    let mut nonce = [0; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(|_| Error::Crypto)?;
    let header = key.header(&nonce);
    let plain = Zeroizing::new(serde_json::to_vec(contents).map_err(|_| Error::Crypto)?);
    let cipher = XChaCha20Poly1305::new((&*key.bytes).into());
    let sealed =
        cipher.encrypt(&XNonce::from(nonce), Payload { msg: &plain, aad: &header }).map_err(|_| Error::Crypto)?;
    let mut file = header;
    file.extend_from_slice(&sealed);
    Ok(file)
}

pub fn open(file: &[u8], password: &[u8]) -> Result<(Contents, Key), Error> {
    if file.len() < HEADER_LEN || &file[..4] != MAGIC {
        return Err(Error::Corrupt);
    }
    if file[4] != VERSION {
        return Err(Error::UnknownVersion(file[4]));
    }
    let word = |at: usize| u32::from_le_bytes([file[at], file[at + 1], file[at + 2], file[at + 3]]);
    let params = Params { memory_kib: word(5), passes: word(9), lanes: word(13) };
    if !params.plausible() {
        return Err(Error::Corrupt);
    }
    let mut salt = [0; SALT_LEN];
    salt.copy_from_slice(&file[17..17 + SALT_LEN]);
    let nonce = XNonce::try_from(&file[17 + SALT_LEN..HEADER_LEN]).map_err(|_| Error::Corrupt)?;
    let key = Key::derive_with(password, salt, params)?;
    let cipher = XChaCha20Poly1305::new((&*key.bytes).into());
    let plain = Zeroizing::new(
        cipher
            .decrypt(&nonce, Payload { msg: &file[HEADER_LEN..], aad: &file[..HEADER_LEN] })
            .map_err(|_| Error::WrongPassword)?,
    );
    let contents = serde_json::from_slice(&plain).map_err(|_| Error::Corrupt)?;
    Ok((contents, key))
}

pub(crate) use glide_config::write_private as write_atomically;

#[cfg(test)]
mod tests {
    use super::*;

    const FAST: Params = Params { memory_kib: 64, passes: 1, lanes: 1 };

    fn item(label: &str, secret: &str, attributes: &[(&str, &str)]) -> Item {
        let mut item = Item::new("login", label, 1);
        item.attributes = attributes.iter().map(|(key, value)| ((*key).to_string(), (*value).to_string())).collect();
        item.secret = Secret::new(secret.as_bytes().to_vec());
        item
    }

    #[test]
    fn a_file_written_before_the_new_fields_still_opens() {
        let old = r#"{"items":[{"id":1,"collection":"login","label":"Telegram","attributes":{"username":"me"},"secret":[104,105],"content_type":"text/plain","created":1,"modified":2}],"next_id":1}"#;
        let contents: Contents = serde_json::from_str(old).unwrap();
        assert_eq!(contents.items[0].label, "Telegram");
        assert!(contents.items[0].sites.is_empty() && contents.items[0].deleted.is_none());
        assert!(contents.folders.is_empty());
    }

    fn sample() -> Contents {
        let mut contents = Contents::default();
        contents.add(item("Telegram", "hunter2", &[("glide:app", "org.telegram.desktop")]));
        contents.add(item("example.com", "correct horse", &[("glide:domain", "example.com"), ("username", "me")]));
        contents
    }

    #[test]
    fn a_sealed_keychain_opens_with_its_password() {
        let key = Key::derive(b"open sesame", FAST).unwrap();
        let file = seal(&sample(), &key).unwrap();
        let (contents, _) = open(&file, b"open sesame").unwrap();
        assert_eq!(contents, sample());
    }

    #[test]
    fn a_wrong_password_is_refused() {
        let file = seal(&sample(), &Key::derive(b"open sesame", FAST).unwrap()).unwrap();
        assert_eq!(open(&file, b"open sesame!").unwrap_err(), Error::WrongPassword);
    }

    #[test]
    fn any_changed_byte_is_detected() {
        let file = seal(&sample(), &Key::derive(b"pw", FAST).unwrap()).unwrap();
        for at in [5, 20, HEADER_LEN - 1, HEADER_LEN, file.len() - 1] {
            let mut damaged = file.clone();
            damaged[at] ^= 1;
            assert!(open(&damaged, b"pw").is_err(), "byte {at} was changed without notice");
        }
    }

    #[test]
    fn the_plaintext_never_appears_in_the_file() {
        let file = seal(&sample(), &Key::derive(b"pw", FAST).unwrap()).unwrap();
        assert!(!file.windows(7).any(|window| window == b"hunter2"));
        assert!(!file.windows(8).any(|window| window == b"Telegram"), "labels are encrypted too");
    }

    #[test]
    fn a_header_asking_for_absurd_key_stretching_is_refused_before_any_work() {
        let mut file = seal(&sample(), &Key::derive(b"pw", FAST).unwrap()).unwrap();
        file[5..9].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(open(&file, b"pw").unwrap_err(), Error::Corrupt, "4 TiB of memory");
        let mut file = seal(&sample(), &Key::derive(b"pw", FAST).unwrap()).unwrap();
        file[9..13].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(open(&file, b"pw").unwrap_err(), Error::Corrupt, "four billion passes");
    }

    #[test]
    fn sealing_twice_uses_a_fresh_nonce() {
        let key = Key::derive(b"pw", FAST).unwrap();
        assert_ne!(seal(&sample(), &key).unwrap(), seal(&sample(), &key).unwrap());
    }

    #[test]
    fn debug_output_never_shows_secret_bytes() {
        let key = Key::derive(b"pw", FAST).unwrap();
        let shown = format!("{:?} {:?}", sample(), key);
        assert!(!shown.contains("hunter2") && !shown.contains("104, 117, 110"), "{shown}");
        assert!(shown.contains("Secret(7 bytes)"));
    }

    #[test]
    fn search_matches_every_requested_attribute() {
        let contents = sample();
        let wanted: BTreeMap<String, String> = [("glide:domain".into(), "example.com".into())].into();
        assert_eq!(contents.search(&wanted).map(|item| item.label.as_str()).collect::<Vec<_>>(), ["example.com"]);
        let none: BTreeMap<String, String> =
            [("glide:domain".into(), "example.com".into()), ("username".into(), "you".into())].into();
        assert_eq!(contents.search(&none).count(), 0);
    }

    #[test]
    fn ids_stay_unique_after_removal() {
        let mut contents = sample();
        let removed = contents.remove(2).unwrap();
        assert_eq!(removed.label, "example.com");
        let id = contents.add(item("again", "x", &[]));
        assert_ne!(id, 2);
        assert!(contents.get(id).is_some());
    }

    #[test]
    fn a_truncated_or_foreign_file_is_corrupt() {
        assert_eq!(open(b"GLKC", b"pw").unwrap_err(), Error::Corrupt);
        assert_eq!(open(&[0; 80], b"pw").unwrap_err(), Error::Corrupt);
        let mut future = seal(&sample(), &Key::derive(b"pw", FAST).unwrap()).unwrap();
        future[4] = 9;
        assert_eq!(open(&future, b"pw").unwrap_err(), Error::UnknownVersion(9));
    }

    #[test]
    fn the_file_is_written_private() {
        let dir = std::env::temp_dir().join(format!("glide-keychain-test-{}", std::process::id()));
        let path = dir.join("keychain");
        write_atomically(&path, b"bytes").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions();
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777, 0o600);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

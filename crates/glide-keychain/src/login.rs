use std::path::{Path, PathBuf};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use glide_login_key::{LEN, WrappingKey};
use zeroize::Zeroizing;

use crate::local::SERVICE_COLLECTION;
use crate::provider::Fault;
use crate::vault::{Contents, Item, write_atomically};

const KEY_MAGIC: &[u8; 4] = b"GLLK";
const FILE_MAGIC: &[u8; 4] = b"GLLC";
const VERSION: u8 = 1;
const PLAIN: u8 = 0;
const WRAPPED: u8 = 1;
const NONCE_LEN: usize = 24;
const KEY_HEADER: usize = 6;
const FILE_HEADER: usize = 5 + NONCE_LEN;

type Key = Zeroizing<[u8; LEN]>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guard {
    Absent,
    File,
    Login,
}

enum Stored {
    Plain(Key),
    Wrapped(Vec<u8>),
}

struct Open {
    key: Option<Key>,
    contents: Contents,
}

pub struct LoginKeyring {
    file: PathBuf,
    key_file: PathBuf,
    previous_file: PathBuf,
    open: Option<Open>,
    fresh: Option<WrappingKey>,
}

impl std::fmt::Debug for LoginKeyring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginKeyring")
            .field("file", &self.file)
            .field("unlocked", &self.open.is_some())
            .finish_non_exhaustive()
    }
}

fn cipher(key: &[u8; LEN]) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(key.into())
}

fn nonce() -> Result<[u8; NONCE_LEN], Fault> {
    let mut nonce = [0; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(|_| Fault::Failed("no randomness for the login keyring".into()))?;
    Ok(nonce)
}

fn crypto() -> Fault {
    Fault::Failed("the login keyring could not be encrypted".into())
}

fn plain_key_file(key: &Key) -> Zeroizing<Vec<u8>> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(KEY_HEADER + LEN));
    bytes.extend_from_slice(KEY_MAGIC);
    bytes.extend_from_slice(&[VERSION, PLAIN]);
    bytes.extend_from_slice(key.as_ref());
    bytes
}

fn wrapped_key_file(key: &Key, wrapping: &WrappingKey) -> Result<Vec<u8>, Fault> {
    let nonce = nonce()?;
    let mut bytes = Vec::with_capacity(KEY_HEADER + NONCE_LEN + LEN + 16);
    bytes.extend_from_slice(KEY_MAGIC);
    bytes.extend_from_slice(&[VERSION, WRAPPED]);
    let sealed = cipher(wrapping)
        .encrypt(&XNonce::from(nonce), Payload { msg: key.as_ref(), aad: &bytes[..KEY_HEADER] })
        .map_err(|_| crypto())?;
    bytes.extend_from_slice(&nonce);
    bytes.extend_from_slice(&sealed);
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Option<Stored> {
    if bytes.len() < KEY_HEADER || &bytes[..4] != KEY_MAGIC || bytes[4] != VERSION {
        return None;
    }
    match bytes[5] {
        PLAIN => glide_login_key::key_from(&bytes[KEY_HEADER..]).map(Stored::Plain),
        WRAPPED if bytes.len() > KEY_HEADER + NONCE_LEN => Some(Stored::Wrapped(bytes.to_vec())),
        _ => None,
    }
}

fn unwrap(bytes: &[u8], wrapping: &WrappingKey) -> Option<Key> {
    let nonce = XNonce::try_from(&bytes[KEY_HEADER..KEY_HEADER + NONCE_LEN]).ok()?;
    let plain = Zeroizing::new(
        cipher(wrapping)
            .decrypt(&nonce, Payload { msg: &bytes[KEY_HEADER + NONCE_LEN..], aad: &bytes[..KEY_HEADER] })
            .ok()?,
    );
    glide_login_key::key_from(&plain)
}

fn seal(contents: &Contents, key: &Key) -> Result<Vec<u8>, Fault> {
    let nonce = nonce()?;
    let mut file = Vec::with_capacity(FILE_HEADER);
    file.extend_from_slice(FILE_MAGIC);
    file.push(VERSION);
    file.extend_from_slice(&nonce);
    let plain = Zeroizing::new(serde_json::to_vec(contents).map_err(|_| crypto())?);
    let sealed =
        cipher(key).encrypt(&XNonce::from(nonce), Payload { msg: &plain, aad: &file }).map_err(|_| crypto())?;
    file.extend_from_slice(&sealed);
    Ok(file)
}

fn open_file(file: &[u8], key: &Key) -> Result<Contents, Fault> {
    let damaged = || Fault::Failed("the login keyring file is damaged".into());
    if file.len() < FILE_HEADER || &file[..4] != FILE_MAGIC || file[4] != VERSION {
        return Err(damaged());
    }
    let nonce = XNonce::try_from(&file[5..FILE_HEADER]).map_err(|_| damaged())?;
    let plain = Zeroizing::new(
        cipher(key)
            .decrypt(&nonce, Payload { msg: &file[FILE_HEADER..], aad: &file[..FILE_HEADER] })
            .map_err(|_| damaged())?,
    );
    serde_json::from_slice(&plain).map_err(|_| damaged())
}

fn random_key() -> Result<Key, Fault> {
    let mut key = Zeroizing::new([0; LEN]);
    getrandom::fill(key.as_mut()).map_err(|_| Fault::Failed("no randomness for the login keyring".into()))?;
    Ok(key)
}

#[must_use]
pub fn current_user() -> Option<(String, u32)> {
    // SAFETY: getuid has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    // SAFETY: passwd is a C struct of integers and pointers, for which all zero bytes are a valid value.
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buffer = vec![0 as libc::c_char; 16 * 1024];
    let mut found = std::ptr::null_mut();
    // SAFETY: entry, buffer and found outlive the call, and buffer.len() is the length handed over.
    let status = unsafe { libc::getpwuid_r(uid, &raw mut entry, buffer.as_mut_ptr(), buffer.len(), &raw mut found) };
    if status != 0 || found.is_null() || entry.pw_name.is_null() {
        return None;
    }
    // SAFETY: on success pw_name points at a NUL-terminated string inside buffer, which is still alive.
    let name = unsafe { std::ffi::CStr::from_ptr(entry.pw_name) };
    Some((name.to_str().ok()?.to_string(), uid))
}

impl LoginKeyring {
    #[must_use]
    pub fn load(dir: &Path) -> Self {
        let mut keyring = Self {
            file: dir.join("login-keyring"),
            key_file: dir.join("login-keyring.key"),
            previous_file: dir.join("login-keyring.key.previous"),
            open: None,
            fresh: None,
        };
        match keyring.stored() {
            None if !keyring.file.exists() => keyring.open = Some(Open { key: None, contents: Contents::default() }),
            None => eprintln!("glide-keychain: the login keyring has lost its key file and stays locked"),
            Some(Stored::Plain(key)) => {
                if let Err(fault) = keyring.adopt(key) {
                    eprintln!("glide-keychain: the login keyring did not open ({fault})");
                }
            }
            Some(Stored::Wrapped(_)) => {}
        }
        keyring
    }

    fn stored(&self) -> Option<Stored> {
        let bytes = Zeroizing::new(std::fs::read(&self.key_file).ok()?);
        decode(&bytes)
    }

    fn adopt(&mut self, key: Key) -> Result<(), Fault> {
        let contents = match std::fs::read(&self.file) {
            Ok(file) => open_file(&file, &key)?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Contents::default(),
            Err(err) => return Err(Fault::Failed(err.to_string())),
        };
        self.open = Some(Open { key: Some(key), contents });
        Ok(())
    }

    fn write_key(&self, bytes: &[u8]) -> Result<(), Fault> {
        write_atomically(&self.key_file, bytes).map_err(|err| Fault::Failed(err.to_string()))
    }

    #[must_use]
    pub fn unlocked(&self) -> bool {
        self.open.is_some()
    }

    #[must_use]
    pub fn guard(&self) -> Guard {
        match self.stored() {
            None => Guard::Absent,
            Some(Stored::Plain(_)) => Guard::File,
            Some(Stored::Wrapped(_)) => Guard::Login,
        }
    }

    pub fn items(&self) -> Result<&[Item], Fault> {
        Ok(&self.open.as_ref().ok_or(Fault::Locked)?.contents.items)
    }

    pub fn edit(&mut self, edit: &mut dyn FnMut(&mut Vec<Item>, &mut u64)) -> Result<(), Fault> {
        let open = self.open.as_mut().ok_or(Fault::Locked)?;
        edit(&mut open.contents.items, &mut open.contents.next_id);
        for item in &mut open.contents.items {
            item.collection = SERVICE_COLLECTION.to_string();
        }
        if open.key.is_none() {
            let key = random_key()?;
            let bytes = match self.fresh.take() {
                Some(wrapping) => Zeroizing::new(wrapped_key_file(&key, &wrapping)?),
                None => plain_key_file(&key),
            };
            write_atomically(&self.key_file, &bytes).map_err(|err| Fault::Failed(err.to_string()))?;
            open.key = Some(key);
        }
        let open = self.open.as_ref().ok_or(Fault::Locked)?;
        let key = open.key.as_ref().ok_or(Fault::Locked)?;
        let sealed = seal(&open.contents, key)?;
        write_atomically(&self.file, &sealed).map_err(|err| Fault::Failed(err.to_string()))
    }

    pub fn offer(&mut self, wrapping: &WrappingKey, verified: bool) -> Result<(), Fault> {
        let stored = self.stored();
        match self.open.as_ref().map(|open| open.key.as_ref()) {
            Some(None) => {
                if verified {
                    self.fresh = Some(wrapping.clone());
                }
                Ok(())
            }
            Some(Some(key)) => {
                let current = matches!(&stored, Some(Stored::Wrapped(bytes)) if unwrap(bytes, wrapping).is_some());
                if verified && !current {
                    self.write_key(&wrapped_key_file(key, wrapping)?)?;
                }
                if verified {
                    self.forget_previous();
                }
                Ok(())
            }
            None => {
                let Some(Stored::Wrapped(bytes)) = stored else {
                    return Err(Fault::Failed("the login keyring cannot be unlocked".into()));
                };
                let key = if let Some(key) = unwrap(&bytes, wrapping) {
                    key
                } else {
                    let Some((key, previous)) = self.previous(wrapping).filter(|_| verified) else {
                        if verified {
                            self.fresh = Some(wrapping.clone());
                        }
                        return Err(Fault::WrongPassword);
                    };
                    self.write_key(&previous)?;
                    key
                };
                self.forget_previous();
                self.adopt(key)?;
                if let Some(fresh) = self.fresh.take().filter(|_| !verified)
                    && let Some(key) = self.open.as_ref().and_then(|open| open.key.as_ref())
                {
                    self.write_key(&wrapped_key_file(key, &fresh)?)?;
                }
                Ok(())
            }
        }
    }

    pub fn rekey(&mut self, old: &WrappingKey, new: &WrappingKey) -> Result<(), Fault> {
        if self.open.is_none() {
            self.offer(old, false)?;
        }
        match self.open.as_ref().map(|open| open.key.as_ref()) {
            Some(Some(key)) => {
                let rewrapped = wrapped_key_file(key, new)?;
                if let Some(Stored::Wrapped(current)) = self.stored()
                    && unwrap(&current, old).is_some()
                {
                    write_atomically(&self.previous_file, &current).map_err(|err| Fault::Failed(err.to_string()))?;
                }
                self.write_key(&rewrapped)
            }
            Some(None) => {
                self.fresh = Some(new.clone());
                Ok(())
            }
            None => Err(Fault::Locked),
        }
    }

    fn previous(&self, wrapping: &WrappingKey) -> Option<(Key, Vec<u8>)> {
        let bytes = std::fs::read(&self.previous_file).ok()?;
        let Some(Stored::Wrapped(bytes)) = decode(&bytes) else { return None };
        unwrap(&bytes, wrapping).map(|key| (key, bytes))
    }

    fn forget_previous(&self) {
        let _ = std::fs::remove_file(&self.previous_file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glide-login-keyring-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn wrapping(byte: u8) -> WrappingKey {
        Zeroizing::new([byte; LEN])
    }

    fn store(keyring: &mut LoginKeyring, label: &str) {
        keyring
            .edit(&mut |items, next| {
                *next += 1;
                let mut item = Item::new(SERVICE_COLLECTION, label, 1);
                item.id = *next;
                item.secret = crate::vault::Secret::new(b"Chrome Safe Storage key".to_vec());
                items.push(item);
            })
            .unwrap();
    }

    fn labels(keyring: &LoginKeyring) -> Vec<String> {
        keyring.items().unwrap().iter().map(|item| item.label.clone()).collect()
    }

    #[test]
    fn a_new_keyring_needs_no_password_and_keeps_its_key_in_a_private_file() {
        let dir = temp("fresh");
        let mut keyring = LoginKeyring::load(&dir);
        assert!(keyring.unlocked());
        assert_eq!(keyring.guard(), Guard::Absent);
        store(&mut keyring, "Chrome Safe Storage");
        assert_eq!(keyring.guard(), Guard::File);
        let mode = std::fs::metadata(dir.join("login-keyring.key")).unwrap().permissions();
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777, 0o600);
        let again = LoginKeyring::load(&dir);
        assert!(again.unlocked(), "a key in a file opens the keyring at every start");
        assert_eq!(labels(&again), ["Chrome Safe Storage"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_secrets_never_appear_in_the_files() {
        let dir = temp("plain");
        let mut keyring = LoginKeyring::load(&dir);
        store(&mut keyring, "Chrome Safe Storage");
        let file = std::fs::read(dir.join("login-keyring")).unwrap();
        assert!(!file.windows(6).any(|window| window == b"Chrome"), "labels are encrypted too");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_login_password_replaces_the_key_file_and_then_unlocks_the_keyring() {
        let dir = temp("wrap");
        let mut keyring = LoginKeyring::load(&dir);
        store(&mut keyring, "Chrome Safe Storage");
        keyring.offer(&wrapping(7), true).unwrap();
        assert_eq!(keyring.guard(), Guard::Login);
        let mut locked = LoginKeyring::load(&dir);
        assert!(!locked.unlocked(), "a wrapped key waits for the login password");
        assert_eq!(locked.offer(&wrapping(8), false).unwrap_err(), Fault::WrongPassword);
        locked.offer(&wrapping(7), false).unwrap();
        assert_eq!(labels(&locked), ["Chrome Safe Storage"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_keyring_created_after_login_is_wrapped_from_the_start() {
        let dir = temp("early");
        let mut keyring = LoginKeyring::load(&dir);
        keyring.offer(&wrapping(3), true).unwrap();
        store(&mut keyring, "Chrome Safe Storage");
        assert_eq!(keyring.guard(), Guard::Login, "the key never touched the disk unwrapped");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_changed_login_password_rewraps_once_the_old_one_is_given() {
        let dir = temp("changed");
        let mut keyring = LoginKeyring::load(&dir);
        store(&mut keyring, "Chrome Safe Storage");
        keyring.offer(&wrapping(1), true).unwrap();
        let mut next_login = LoginKeyring::load(&dir);
        assert_eq!(next_login.offer(&wrapping(2), true).unwrap_err(), Fault::WrongPassword);
        assert_eq!(next_login.offer(&wrapping(9), false).unwrap_err(), Fault::WrongPassword);
        next_login.offer(&wrapping(1), false).unwrap();
        let mut after = LoginKeyring::load(&dir);
        after.offer(&wrapping(2), false).unwrap();
        assert_eq!(labels(&after), ["Chrome Safe Storage"], "the new login password opens it now");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_open_keyring_follows_a_password_change_reported_by_the_session() {
        let dir = temp("open-change");
        let mut keyring = LoginKeyring::load(&dir);
        store(&mut keyring, "Chrome Safe Storage");
        keyring.offer(&wrapping(1), true).unwrap();
        keyring.offer(&wrapping(2), true).unwrap();
        let mut after = LoginKeyring::load(&dir);
        assert_eq!(after.offer(&wrapping(1), false).unwrap_err(), Fault::WrongPassword);
        after.offer(&wrapping(2), false).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_rekey_moves_a_locked_keyring_to_the_new_password() {
        let dir = temp("rekey");
        let mut keyring = LoginKeyring::load(&dir);
        store(&mut keyring, "Chrome Safe Storage");
        keyring.offer(&wrapping(1), true).unwrap();
        let mut locked = LoginKeyring::load(&dir);
        locked.rekey(&wrapping(1), &wrapping(2)).unwrap();
        let mut after = LoginKeyring::load(&dir);
        after.offer(&wrapping(2), false).unwrap();
        assert_eq!(labels(&after), ["Chrome Safe Storage"]);
        assert!(!dir.join("login-keyring.key.previous").exists(), "a login with the new password settles it");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_rekey_from_a_password_change_that_failed_still_opens_with_the_old_password() {
        let dir = temp("rekey-failed");
        let mut keyring = LoginKeyring::load(&dir);
        store(&mut keyring, "Chrome Safe Storage");
        keyring.offer(&wrapping(1), true).unwrap();
        keyring.rekey(&wrapping(1), &wrapping(2)).unwrap();
        let mut typed = LoginKeyring::load(&dir);
        assert_eq!(
            typed.offer(&wrapping(1), false).unwrap_err(),
            Fault::WrongPassword,
            "a typed old password is not proof that the change failed"
        );
        let mut next_login = LoginKeyring::load(&dir);
        next_login.offer(&wrapping(1), true).unwrap();
        assert_eq!(labels(&next_login), ["Chrome Safe Storage"]);
        let mut later = LoginKeyring::load(&dir);
        assert_eq!(later.offer(&wrapping(2), false).unwrap_err(), Fault::WrongPassword, "the old wrap is back");
        later.offer(&wrapping(1), false).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_damaged_key_file_is_refused() {
        assert!(decode(b"GLLK").is_none());
        assert!(decode(b"XXXX\x01\x00aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_none());
        let mut wrapped = wrapped_key_file(&Zeroizing::new([5; LEN]), &wrapping(1)).unwrap();
        let last = wrapped.len() - 1;
        wrapped[last] ^= 1;
        assert!(unwrap(&wrapped, &wrapping(1)).is_none());
    }
}

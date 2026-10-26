use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub const LEN: usize = 32;
pub const KEY_FILE: &str = "glide-login-key";
pub const REKEY_FILE: &str = "glide-login-rekey";

const MEMORY_KIB: u32 = 64 * 1024;
const PASSES: u32 = 3;
const LANES: u32 = 1;
const SALT_LABEL: &[u8] = b"glide-login-keyring/1\0";

pub type WrappingKey = Zeroizing<[u8; LEN]>;

#[must_use]
pub fn salt(user: &str, uid: u32) -> [u8; 16] {
    let digest =
        Sha256::new().chain_update(SALT_LABEL).chain_update(user).chain_update([0]).chain_update(uid.to_le_bytes());
    let mut salt = [0; 16];
    salt.copy_from_slice(&digest.finalize()[..16]);
    salt
}

#[must_use]
pub fn wrapping_key(password: &[u8], user: &str, uid: u32) -> Option<WrappingKey> {
    let params = argon2::Params::new(MEMORY_KIB, PASSES, LANES, Some(LEN)).ok()?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = Zeroizing::new([0; LEN]);
    argon.hash_password_into(password, &salt(user, uid), key.as_mut()).ok()?;
    Some(key)
}

#[must_use]
pub fn rekey_bytes(old: &WrappingKey, new: &WrappingKey) -> Zeroizing<Vec<u8>> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(2 * LEN));
    bytes.extend_from_slice(old.as_ref());
    bytes.extend_from_slice(new.as_ref());
    bytes
}

#[must_use]
pub fn key_from(bytes: &[u8]) -> Option<WrappingKey> {
    let mut key = Zeroizing::new([0; LEN]);
    key.copy_from_slice(bytes.get(..LEN).filter(|_| bytes.len() == LEN)?);
    Some(key)
}

#[must_use]
pub fn rekey_from(bytes: &[u8]) -> Option<(WrappingKey, WrappingKey)> {
    if bytes.len() != 2 * LEN {
        return None;
    }
    Some((key_from(&bytes[..LEN])?, key_from(&bytes[LEN..])?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_login_always_gives_the_same_key() {
        let first = wrapping_key(b"hunter2", "artem", 1000).unwrap();
        assert_eq!(*first, *wrapping_key(b"hunter2", "artem", 1000).unwrap());
        assert_ne!(*first, *wrapping_key(b"hunter3", "artem", 1000).unwrap());
    }

    #[test]
    fn every_account_gets_its_own_salt() {
        assert_ne!(salt("artem", 1000), salt("artem", 1001));
        assert_ne!(salt("artem", 1000), salt("anna", 1000));
        assert_ne!(salt("ab", 1000), salt("a", 1000), "the name is separated from the uid");
    }

    #[test]
    fn a_rekey_file_carries_the_old_key_then_the_new_one() {
        let (old, new) = (Zeroizing::new([1; LEN]), Zeroizing::new([2; LEN]));
        let (read_old, read_new) = rekey_from(&rekey_bytes(&old, &new)).unwrap();
        assert_eq!((*read_old, *read_new), (*old, *new));
        assert!(rekey_from(&[0; LEN]).is_none());
        assert!(key_from(&[0; LEN + 1]).is_none());
    }
}

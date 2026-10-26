use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::vault::Item;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Index {
    salt: String,
    entries: BTreeMap<u64, Vec<String>>,
}

fn hash(salt: &str, name: &str, value: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(salt.as_bytes()).expect("HMAC takes a key of any length");
    mac.update(name.as_bytes());
    mac.update(&[0]);
    mac.update(value.as_bytes());
    data_encoding::HEXLOWER.encode(&mac.finalize().into_bytes())
}

impl Index {
    #[must_use]
    pub fn load(path: &Path) -> Self {
        std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    #[must_use]
    pub fn build(items: &[Item], previous_salt: Option<&str>) -> Self {
        let salt = previous_salt.filter(|salt| !salt.is_empty()).map_or_else(
            || {
                let mut bytes = [0u8; 16];
                let _ = getrandom::fill(&mut bytes);
                data_encoding::HEXLOWER.encode(&bytes)
            },
            str::to_string,
        );
        let entries = items
            .iter()
            .map(|item| {
                let hashes = item.attributes.iter().map(|(name, value)| hash(&salt, name, value)).collect();
                (item.id, hashes)
            })
            .collect();
        Self { salt, entries }
    }

    #[must_use]
    pub fn salt(&self) -> Option<&str> {
        (!self.salt.is_empty()).then_some(self.salt.as_str())
    }

    pub fn save(&self, path: &Path) {
        if let Ok(bytes) = serde_json::to_vec(self) {
            let _ = crate::vault::write_atomically(path, &bytes);
        }
    }

    #[must_use]
    pub fn ids(&self) -> Vec<u64> {
        self.entries.keys().copied().collect()
    }

    #[must_use]
    pub fn matches(&self, wanted: &BTreeMap<String, String>) -> Vec<u64> {
        let wanted: Vec<String> = wanted.iter().map(|(name, value)| hash(&self.salt, name, value)).collect();
        self.entries
            .iter()
            .filter(|(_, hashes)| wanted.iter().all(|want| hashes.contains(want)))
            .map(|(id, _)| *id)
            .collect()
    }
}

#[must_use]
pub fn path_beside(vault: &Path) -> PathBuf {
    vault.with_file_name("secret-service.index")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u64, pairs: &[(&str, &str)]) -> Item {
        let mut item = Item::new("secret-service", "x", 0);
        item.id = id;
        item.attributes = pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
        item
    }

    #[test]
    fn a_locked_search_finds_items_without_knowing_their_values() {
        let items = [
            item(1, &[("application", "chrome"), ("xdg:schema", "chrome_libsecret_os_crypt_password_v2")]),
            item(2, &[("service", "git")]),
        ];
        let index = Index::build(&items, None);
        let text = serde_json::to_string(&index).expect("json");
        assert!(!text.contains("chrome") && !text.contains("git"), "only salted hashes are kept");
        let wanted: BTreeMap<String, String> = [("application".to_string(), "chrome".to_string())].into();
        assert_eq!(index.matches(&wanted), [1]);
        assert_eq!(index.matches(&BTreeMap::new()), [1, 2]);
        let other: BTreeMap<String, String> = [("application".to_string(), "firefox".to_string())].into();
        assert!(index.matches(&other).is_empty());
        let rebuilt = Index::build(&items, index.salt());
        assert_eq!(rebuilt, index, "the salt is kept across rebuilds");
    }
}

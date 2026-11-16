use std::path::{Path, PathBuf};

use crate::store::{Error, Store, Values};

pub const ORIGIN: &str = "import";

#[derive(Debug, PartialEq, Eq)]
pub struct Imported {
    pub keys: usize,
    pub dropped: Vec<String>,
}

pub fn toml_text(store: &mut Store, namespace: &str, text: &str) -> Result<Imported, String> {
    let table: toml::Table = text.parse().map_err(|err: toml::de::Error| err.to_string())?;
    let value = serde_json::to_value(table).map_err(|err| err.to_string())?;
    let (mut kept, mut dropped) = (Values::new(), Vec::new());
    for (key, value) in glide_ipc::config::flatten(value) {
        if store.accepts(namespace, &key, &value) {
            kept.insert(key, value);
        } else {
            dropped.push(key);
        }
    }
    let keys = store.set_many(namespace, kept, ORIGIN).map_err(|err: Error| err.to_string())?.len();
    Ok(Imported { keys, dropped })
}

#[must_use]
pub fn migrated(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".migrated");
    path.with_file_name(name)
}

pub fn legacy_file(store: &mut Store, namespace: &str, path: &Path) {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
        Err(err) => {
            tracing::warn!("cannot read {} to import it: {err}", path.display());
            return;
        }
    };
    match toml_text(store, namespace, &text) {
        Ok(imported) => {
            tracing::info!(keys = imported.keys, dropped = ?imported.dropped, "imported {}", path.display());
            if let Err(err) = std::fs::rename(path, migrated(path)) {
                tracing::warn!("imported {} but could not move it aside: {err}", path.display());
            }
        }
        Err(err) => tracing::warn!("{} stays where it is; it could not be imported: {err}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Defaults;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn store() -> Store {
        let shell = Values::from([
            ("general.blank-after-seconds".to_owned(), json!(600)),
            ("dock.autohide".to_owned(), json!("smart")),
            ("keybinds".to_owned(), json!([])),
            ("output".to_owned(), json!([])),
        ]);
        Store::in_memory(Defaults::from_namespaces(BTreeMap::from([("shell".to_owned(), shell)]))).expect("store")
    }

    #[test]
    fn known_keys_are_imported_and_unknown_or_mistyped_ones_are_dropped() {
        let mut store = store();
        let text = "[general]\nblank-after-seconds = 120\nholograms = true\n\n[dock]\nautohide = 3\n\n\
                    [[keybinds]]\nkey = \"t\"\naction = \"terminal\"\nmods = [\"super\"]\n\n\
                    [[output]]\nname = \"eDP-1\"\nscale = 2.0\n\n[telepathy]\nenabled = true\n";
        let imported = toml_text(&mut store, "shell", text).expect("imports");
        assert_eq!(imported.keys, 3);
        assert_eq!(imported.dropped, ["dock.autohide", "general.holograms", "telepathy.enabled"]);
        let all = store.get_all("shell").expect("read");
        assert_eq!(all["general.blank-after-seconds"], json!(120));
        assert_eq!(all["dock.autohide"], json!("smart"), "a wrong kind keeps the default");
        assert_eq!(all["keybinds"], json!([{ "key": "t", "action": "terminal", "mods": ["super"] }]));
        assert_eq!(all["output"][0]["scale"], json!(2.0));
        assert_eq!(store.history("shell", 10).expect("history")[0].origin, ORIGIN);
    }

    #[test]
    fn a_broken_file_is_left_alone_and_a_good_one_is_moved_aside() {
        let dir = std::env::temp_dir().join(format!("glide-configd-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("glide.toml");
        let mut store = store();
        std::fs::write(&path, "[general\n").expect("write");
        legacy_file(&mut store, "shell", &path);
        assert!(path.exists(), "an unreadable file is not thrown away");
        std::fs::write(&path, "[general]\nblank-after-seconds = 60\n").expect("write");
        legacy_file(&mut store, "shell", &path);
        assert!(!path.exists() && migrated(&path).exists());
        assert_eq!(store.get("shell", "general.blank-after-seconds").expect("read"), json!(60));
        let _ = std::fs::remove_dir_all(dir);
    }
}

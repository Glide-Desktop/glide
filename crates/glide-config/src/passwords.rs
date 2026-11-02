use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ClickAction {
    #[default]
    Password,
    Username,
    Both,
    Code,
}

impl ClickAction {
    pub const ALL: [Self; 4] = [Self::Password, Self::Username, Self::Both, Self::Code];
}

#[allow(clippy::struct_excessive_bools)] // Each preference is its own switch in the settings sheet
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Prefs {
    pub click: ClickAction,
    pub offer: bool,
    pub type_fallback: bool,
    pub clear_after: u32,
    pub private_clipboard: bool,
    pub lock_with_screen: bool,
    pub lock_after: u32,
    pub secret_service: bool,
    pub sort_by_name: bool,
    pub leak_check: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            click: ClickAction::Password,
            offer: true,
            type_fallback: true,
            clear_after: 30,
            private_clipboard: true,
            lock_with_screen: true,
            lock_after: 15,
            secret_service: true,
            sort_by_name: false,
            leak_check: true,
        }
    }
}

pub const CLEAR_CHOICES: [u32; 5] = [0, 15, 30, 60, 90];
pub const LOCK_CHOICES: [u32; 5] = [0, 5, 15, 30, 60];

#[must_use]
pub fn path() -> Option<PathBuf> {
    crate::config_home().map(|dir| dir.join("glide").join("passwords.toml"))
}

impl Prefs {
    #[must_use]
    pub fn load() -> Self {
        path().map_or_else(Self::default, |path| Self::load_at(&path))
    }

    #[must_use]
    pub fn parse(text: &str) -> Self {
        toml::from_str(text).unwrap_or_else(|_| Self::lenient(text))
    }

    fn valid_pair(key: &str, value: &toml::Value) -> bool {
        let mut single = toml::Table::new();
        single.insert(key.to_string(), value.clone());
        toml::Value::Table(single).try_into::<Self>().is_ok()
    }

    fn lenient(text: &str) -> Self {
        let Ok(table) = text.parse::<toml::Table>() else { return Self::default() };
        let valid: toml::Table = table.into_iter().filter(|(key, value)| Self::valid_pair(key, value)).collect();
        toml::Value::Table(valid).try_into().unwrap_or_default()
    }

    fn drops_keys(text: &str) -> bool {
        let Ok(table) = text.parse::<toml::Table>() else { return true };
        let known = toml::Table::try_from(Self::default()).unwrap_or_default();
        table.iter().any(|(key, value)| !known.contains_key(key) || !Self::valid_pair(key, value))
    }

    #[must_use]
    pub fn load_at(path: &Path) -> Self {
        std::fs::read_to_string(path).map_or_else(|_| Self::default(), |text| Self::parse(&text))
    }

    pub fn save(&self) {
        if let Some(path) = path() {
            let _ = self.save_at(&path);
        }
    }

    pub fn save_at(&self, path: &Path) -> io::Result<()> {
        let text = toml::to_string(self).map_err(io::Error::other)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if let Ok(old) = std::fs::read(path)
            && std::str::from_utf8(&old).ok().is_none_or(Self::drops_keys)
        {
            let _ = std::fs::rename(path, path.with_extension("toml.broken"));
        }
        crate::write_atomic(path, text.as_bytes(), 0o644)
    }

    pub fn update(change: impl FnOnce(&mut Self)) -> io::Result<Self> {
        let path = path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no configuration folder"))?;
        Self::update_at(&path, change)
    }

    pub fn update_at(path: &Path, change: impl FnOnce(&mut Self)) -> io::Result<Self> {
        crate::file::locked(path, || {
            let mut prefs = Self::load_at(path);
            change(&mut prefs);
            prefs.save_at(path)?;
            Ok(prefs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_keys_take_their_defaults() {
        let prefs = Prefs::parse("click = \"both\"\nclear-after = 60\n");
        assert_eq!(prefs.click, ClickAction::Both);
        assert_eq!(prefs.clear_after, 60);
        assert!(prefs.offer && prefs.lock_with_screen);
    }

    #[test]
    fn a_broken_file_falls_back_to_defaults() {
        assert_eq!(Prefs::parse("click = 7"), Prefs::default());
    }

    #[test]
    fn prefs_round_trip() {
        let prefs = Prefs { click: ClickAction::Code, lock_after: 5, ..Prefs::default() };
        assert_eq!(Prefs::parse(&toml::to_string(&prefs).unwrap()), prefs);
    }

    #[test]
    fn two_writers_changing_different_keys_both_keep_their_change() {
        let dir = std::env::temp_dir().join(format!("glide-passwords-prefs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("glide").join("passwords.toml");

        let stale_app_copy = Prefs::load_at(&file);
        assert!(stale_app_copy.lock_with_screen);

        let settings = Prefs::update_at(&file, |prefs| prefs.lock_with_screen = false).unwrap();
        assert!(!settings.lock_with_screen);

        let app = Prefs::update_at(&file, |prefs| prefs.sort_by_name = !stale_app_copy.sort_by_name).unwrap();
        assert!(app.sort_by_name, "the app's own change is written");
        assert!(!app.lock_with_screen, "the value handed back already carries the other writer's change");

        let on_disk = Prefs::load_at(&file);
        assert_eq!(on_disk, app);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_whose_keys_would_be_dropped_on_save_is_kept_as_broken() {
        let dir = std::env::temp_dir().join(format!("glide-passwords-prefs-broken-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("passwords.toml");
        let original = "click = 7\nclear-after = 60\n";
        std::fs::write(&file, original).unwrap();

        let prefs = Prefs::update_at(&file, |prefs| prefs.offer = false).unwrap();
        assert_eq!((prefs.clear_after, prefs.click, prefs.offer), (60, ClickAction::Password, false));
        assert_eq!(std::fs::read_to_string(file.with_extension("toml.broken")).unwrap(), original);

        std::fs::remove_file(file.with_extension("toml.broken")).unwrap();
        Prefs::update_at(&file, |prefs| prefs.offer = true).unwrap();
        assert!(!file.with_extension("toml.broken").exists(), "a file Glide wrote itself loses nothing");

        std::fs::write(&file, "future-switch = true\n").unwrap();
        Prefs::update_at(&file, |prefs| prefs.offer = true).unwrap();
        assert!(file.with_extension("toml.broken").exists(), "a key this version does not know is kept aside");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_updates_keep_every_change() {
        let dir = std::env::temp_dir().join(format!("glide-passwords-prefs-race-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("passwords.toml");
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for _ in 0..20 {
                    Prefs::update_at(&file, |prefs| prefs.clear_after += 1).unwrap();
                }
            });
            scope.spawn(|| {
                for _ in 0..20 {
                    Prefs::update_at(&file, |prefs| prefs.lock_after += 1).unwrap();
                }
            });
        });
        let prefs = Prefs::load_at(&file);
        assert_eq!((prefs.clear_after, prefs.lock_after), (30 + 20, 15 + 20));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_update_keeps_keys_it_did_not_touch() {
        let dir = std::env::temp_dir().join(format!("glide-passwords-prefs-keep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("passwords.toml");
        std::fs::write(&file, "clear-after = 90\nclick = \"code\"\n").unwrap();

        let prefs = Prefs::update_at(&file, |prefs| prefs.offer = false).unwrap();
        assert_eq!((prefs.clear_after, prefs.click, prefs.offer), (90, ClickAction::Code, false));
        assert_eq!(Prefs::load_at(&file), prefs);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

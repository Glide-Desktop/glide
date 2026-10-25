use std::path::PathBuf;

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
    glide_config::config_home().map(|dir| dir.join("glide").join("passwords.toml"))
}

impl Prefs {
    #[must_use]
    pub fn load() -> Self {
        path().and_then(|path| std::fs::read_to_string(path).ok()).map_or_else(Self::default, |text| Self::parse(&text))
    }

    #[must_use]
    pub fn parse(text: &str) -> Self {
        toml::from_str(text).unwrap_or_default()
    }

    pub fn save(&self) {
        let (Some(path), Ok(text)) = (path(), toml::to_string(self)) else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = glide_config::write_atomic(&path, text.as_bytes(), 0o644);
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
}

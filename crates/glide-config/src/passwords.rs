use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const NAMESPACE: &str = "passwords";

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
pub fn legacy_path() -> Option<PathBuf> {
    crate::config_home().map(|dir| dir.join("glide").join("passwords.toml"))
}

impl Prefs {
    #[must_use]
    pub fn load() -> Self {
        crate::service::load(NAMESPACE)
    }

    #[must_use]
    pub fn from_values(values: &crate::service::Values) -> Self {
        crate::service::read(values).unwrap_or_default()
    }

    pub fn update(change: impl FnOnce(&mut Self)) -> io::Result<Self> {
        crate::service::update(NAMESPACE, change).map_err(io::Error::other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_installed_defaults_are_the_prefs_defaults() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/defaults/passwords.json");
        let values = serde_json::to_value(Prefs::default()).map(crate::service::flatten).expect("defaults serialize");
        let expected = serde_json::to_string_pretty(&values).expect("JSON") + "\n";
        if std::env::var_os("GLIDE_WRITE_DEFAULTS").is_some() {
            std::fs::write(&path, &expected).expect("write the defaults");
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_default(),
            expected,
            "data/defaults/passwords.json is generated from Prefs; rerun with GLIDE_WRITE_DEFAULTS=1"
        );
    }

    #[test]
    fn missing_or_unreadable_values_take_their_defaults() {
        let values = crate::service::Values::from([
            ("click".to_owned(), serde_json::json!("both")),
            ("clear-after".to_owned(), serde_json::json!(60)),
        ]);
        let prefs = Prefs::from_values(&values);
        assert_eq!((prefs.click, prefs.clear_after, prefs.offer), (ClickAction::Both, 60, true));
        let broken = crate::service::Values::from([("click".to_owned(), serde_json::json!(7))]);
        assert_eq!(Prefs::from_values(&broken), Prefs::default());
    }
}

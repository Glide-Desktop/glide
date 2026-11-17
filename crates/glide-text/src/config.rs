use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[allow(clippy::struct_excessive_bools)] // Independent text.toml switches, not a state machine
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Preferences {
    pub font_family: Option<String>,
    pub font_size: f32,
    pub monospace: bool,
    pub tab_width: usize,
    pub spaces: bool,
    pub auto_indent: bool,
    pub auto_pair: bool,
    pub sidebar: bool,
    pub status_bar: bool,
    pub status_islands: bool,
    pub auto_hide_status: bool,
    pub restore_session: bool,
    pub hidden_files: bool,
    pub trim_on_save: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            font_family: None,
            font_size: 14.0,
            monospace: true,
            tab_width: 4,
            spaces: true,
            auto_indent: true,
            auto_pair: true,
            sidebar: true,
            status_bar: true,
            status_islands: true,
            auto_hide_status: true,
            restore_session: true,
            hidden_files: false,
            trim_on_save: false,
        }
    }
}

pub const NAMESPACE: &str = "text";

impl Preferences {
    #[must_use]
    fn clamped(mut self) -> Self {
        self.font_size = self.font_size.clamp(6.0, 72.0);
        self.tab_width = self.tab_width.clamp(1, 16);
        self
    }

    #[must_use]
    pub fn parse(text: &str) -> Self {
        toml::from_str::<Self>(text).unwrap_or_default().clamped()
    }

    #[must_use]
    pub fn from_values(values: &glide_config::service::Values) -> Self {
        glide_config::service::read::<Self>(values).unwrap_or_default().clamped()
    }
}

#[must_use]
pub fn legacy_path() -> Option<PathBuf> {
    glide_config::xdg_path("glide", "text.toml")
}

pub fn import_legacy() {
    if let Some(path) = legacy_path() {
        glide_config::service::import_legacy(NAMESPACE, &path);
    }
}

#[must_use]
pub fn changes() -> iced::futures::channel::mpsc::UnboundedReceiver<glide_config::service::Values> {
    glide_app::config::watch(NAMESPACE)
}

#[must_use]
pub(crate) fn state_dir() -> Option<PathBuf> {
    Some(glide_config::state_home()?.join("glide").join("text"))
}

#[must_use]
pub fn load() -> Preferences {
    let values = glide_config::service::Client::session().and_then(|client| client.get_all(NAMESPACE));
    values.map_or_else(|_| Preferences::default(), |values| Preferences::from_values(&values))
}

pub fn save(prefs: &Preferences) -> std::io::Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    glide_config::service::update::<Preferences>(NAMESPACE, |stored| *stored = prefs.clone())
        .map(drop)
        .map_err(std::io::Error::other)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SessionTab {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk: Option<u64>,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Session {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    pub active: usize,
    pub tabs: Vec<SessionTab>,
    pub recent: Vec<PathBuf>,
}

#[must_use]
pub(crate) fn load_session() -> Session {
    state_dir()
        .and_then(|folder| std::fs::read_to_string(folder.join("session.toml")).ok())
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

pub(crate) fn save_session(session: &Session) -> std::io::Result<()> {
    let folder = state_dir().ok_or_else(|| std::io::Error::other("no state directory"))?;
    std::fs::create_dir_all(&folder)?;
    let text = toml::to_string(session).map_err(std::io::Error::other)?;
    crate::document::write(&folder.join("session.toml"), text.as_bytes()).map(|_| ())
}

#[must_use]
pub(crate) fn drafts() -> Option<PathBuf> {
    Some(state_dir()?.join("drafts"))
}

pub fn remember(recent: &mut Vec<PathBuf>, path: &Path) {
    recent.retain(|known| known != path);
    recent.insert(0, path.to_path_buf());
    recent.truncate(12);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_read_kebab_case_and_clamp_nonsense() {
        let prefs = Preferences::parse("font-size = 900\ntab-width = 0\nsidebar = false\nspaces = false\n");
        assert_eq!(prefs.font_size, 72.0);
        assert_eq!(prefs.tab_width, 1);
        assert!(!prefs.sidebar && !prefs.spaces);
        assert!(prefs.monospace, "unset keys keep their defaults");
    }

    #[test]
    fn commenting_lines_led_by_a_wide_space_does_not_split_it() {
        let text = "\u{a0}x\n y";
        let change = crate::edit::toggle_comment(text, &(0..text.len()), crate::syntax::Language::Rust)
            .expect("a comment is added");
        assert_eq!(change.text, "// \u{a0}x\n//  y");
        assert_eq!(crate::document::position("кіт€", 5).column, 4, "an offset inside a character steps back");
    }

    #[test]
    fn the_installed_defaults_are_the_preferences_defaults() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/defaults/text.json");
        let values =
            serde_json::to_value(Preferences::default()).map(glide_config::service::flatten).expect("serializes");
        let expected = serde_json::to_string_pretty(&values).expect("JSON") + "\n";
        if std::env::var_os("GLIDE_WRITE_DEFAULTS").is_some() {
            std::fs::write(&path, &expected).expect("write the defaults");
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_default(),
            expected,
            "data/defaults/text.json is generated from Preferences; rerun with GLIDE_WRITE_DEFAULTS=1"
        );
    }

    #[test]
    fn recent_files_move_to_the_front_without_duplicates() {
        let mut recent = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        remember(&mut recent, Path::new("/b"));
        assert_eq!(recent, [PathBuf::from("/b"), PathBuf::from("/a")]);
    }
}

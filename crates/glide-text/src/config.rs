use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[allow(clippy::struct_excessive_bools)] // Independent text.toml switches, not a state machine
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Preferences {
    #[serde(skip_serializing_if = "Option::is_none")]
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

impl Preferences {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut prefs: Self = toml::from_str(text).unwrap_or_else(|_| Self::lenient(text));
        prefs.font_size = prefs.font_size.clamp(6.0, 72.0);
        prefs.tab_width = prefs.tab_width.clamp(1, 16);
        prefs
    }

    fn lenient(text: &str) -> Self {
        let Ok(table) = text.parse::<toml::Table>() else { return Self::default() };
        let valid: toml::Table = table
            .into_iter()
            .filter(|(key, value)| {
                let mut single = toml::Table::new();
                single.insert(key.clone(), value.clone());
                toml::Value::Table(single).try_into::<Self>().is_ok()
            })
            .collect();
        toml::Value::Table(valid).try_into().unwrap_or_default()
    }
}

#[must_use]
pub fn path() -> Option<PathBuf> {
    glide_config::xdg_path("glide", "text.toml")
}

#[must_use]
pub(crate) fn state_dir() -> Option<PathBuf> {
    Some(glide_config::state_home()?.join("glide").join("text"))
}

#[must_use]
pub fn load() -> Preferences {
    path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map_or_else(Preferences::default, |text| Preferences::parse(&text))
}

pub fn save(prefs: &Preferences) -> std::io::Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    let path = path().ok_or_else(|| std::io::Error::other("no config directory"))?;
    save_to(&path, prefs)
}

fn save_to(path: &Path, prefs: &Preferences) -> std::io::Result<()> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let mut document: toml_edit::DocumentMut = text.parse().map_err(std::io::Error::other)?;
    let fresh: toml_edit::DocumentMut =
        toml::to_string(prefs).map_err(std::io::Error::other)?.parse().map_err(std::io::Error::other)?;
    for (key, item) in fresh.iter() {
        let unchanged = document.get(key).and_then(toml_edit::Item::as_value).map(ToString::to_string)
            == item.as_value().map(ToString::to_string);
        if !unchanged {
            document[key] = item.clone();
        }
    }
    if prefs.font_family.is_none() {
        document.remove("font-family");
    }
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    crate::document::write(path, document.to_string().as_bytes()).map(|_| ())
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
    fn one_bad_value_keeps_the_other_preferences() {
        let prefs = Preferences::parse("font-size = \"big\"\ntab-width = 2\nsidebar = false\n");
        assert_eq!(prefs.tab_width, 2);
        assert!(!prefs.sidebar);
        assert_eq!(prefs.font_size, Preferences::default().font_size);
    }

    #[test]
    fn saving_keeps_comments_and_unknown_keys() {
        let folder = std::env::temp_dir().join(format!("glide-text-prefs-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("text.toml");
        std::fs::write(&path, "# mine\nsidebar = true\nfuture-key = 3\n").unwrap();
        let prefs = Preferences { sidebar: false, ..Preferences::default() };
        save_to(&path, &prefs).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# mine") && text.contains("future-key = 3") && text.contains("sidebar = false"));
        assert_eq!(Preferences::parse(&text), prefs);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn recent_files_move_to_the_front_without_duplicates() {
        let mut recent = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        remember(&mut recent, Path::new("/b"));
        assert_eq!(recent, [PathBuf::from("/b"), PathBuf::from("/a")]);
    }
}

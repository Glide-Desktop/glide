use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use alacritty_terminal::term::Config;
use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle};
use serde::{Deserialize, Serialize};

use crate::profile::Profile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Cursor {
    #[default]
    Block,
    Beam,
    Underline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnExit {
    Close,
    #[default]
    CloseClean,
    Keep,
}

#[allow(clippy::struct_excessive_bools)] // Independent terminal.toml switches, not a state machine
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Preferences {
    pub profile: Profile,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    pub font_size: f32,
    pub line_height: f32,
    pub scrollback: usize,
    pub cursor: Cursor,
    pub cursor_blink: bool,
    pub bold_bright: bool,
    pub alt_sends_escape: bool,
    pub copy_on_select: bool,
    pub visual_bell: bool,
    pub audible_bell: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bell_sound: Option<String>,
    pub on_exit: OnExit,
    pub confirm_close: bool,
    pub mark_prompts: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            profile: Profile::Meridian,
            font_family: None,
            font_size: 13.0,
            line_height: 1.25,
            scrollback: 10_000,
            cursor: Cursor::Block,
            cursor_blink: true,
            bold_bright: true,
            alt_sends_escape: true,
            copy_on_select: true,
            visual_bell: true,
            audible_bell: true,
            bell_sound: None,
            on_exit: OnExit::CloseClean,
            confirm_close: true,
            mark_prompts: true,
            shell: None,
        }
    }
}

impl Preferences {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut prefs: Self = toml::from_str(text).unwrap_or_default();
        prefs.font_size = prefs.font_size.clamp(6.0, 48.0);
        prefs.line_height = prefs.line_height.clamp(0.9, 2.0);
        prefs.scrollback = prefs.scrollback.min(200_000);
        prefs
    }

    #[must_use]
    pub(crate) fn term_config(&self) -> Config {
        let shape = match self.cursor {
            Cursor::Block => CursorShape::Block,
            Cursor::Beam => CursorShape::Beam,
            Cursor::Underline => CursorShape::Underline,
        };
        Config {
            scrolling_history: self.scrollback,
            default_cursor_style: CursorStyle { shape, blinking: self.cursor_blink },
            ..Config::default()
        }
    }
}

static WRITES: AtomicU64 = AtomicU64::new(0);

#[must_use]
pub fn path() -> Option<PathBuf> {
    glide_config::xdg_path("glide", "terminal.toml")
}

#[must_use]
pub fn load() -> Preferences {
    path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map_or_else(Preferences::default, |text| Preferences::parse(&text))
}

pub fn save(prefs: &Preferences) -> std::io::Result<()> {
    let path = path().ok_or_else(|| std::io::Error::other("no config directory"))?;
    save_profile(&path, prefs.profile)
}

fn save_profile(path: &Path, profile: Profile) -> std::io::Result<()> {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let mut document: toml_edit::DocumentMut = text.parse().map_err(std::io::Error::other)?;
    let _: Preferences = toml::from_str(&text).map_err(std::io::Error::other)?;
    let serialized =
        toml::to_string(&Preferences { profile, ..Preferences::default() }).map_err(std::io::Error::other)?;
    let source: toml_edit::DocumentMut = serialized.parse().map_err(std::io::Error::other)?;
    document["profile"] = source["profile"].clone();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let id = WRITES.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_extension(format!("toml.{}.{id}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary)?;
    let result = (|| {
        if let Ok(metadata) = std::fs::metadata(&path) {
            file.set_permissions(metadata.permissions())?;
        }
        file.write_all(document.to_string().as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary, &path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

#[must_use]
pub fn shell_command(command: &str) -> Option<(String, Vec<String>)> {
    let mut words = shlex::split(command)?.into_iter();
    Some((words.next()?, words.collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_read_kebab_case_and_clamp_nonsense() {
        let prefs = Preferences::parse("profile = \"red-sands\"\nfont-size = 400\non-exit = \"keep\"\nbogus = 1\n");
        assert_eq!(prefs.profile, Profile::RedSands);
        assert_eq!(prefs.font_size, 48.0);
        assert_eq!(prefs.on_exit, OnExit::Keep);
        assert_eq!(prefs.cursor, Cursor::Block);
    }

    #[test]
    fn preferences_survive_a_round_trip() {
        let prefs =
            Preferences { profile: Profile::Ocean, font_family: Some("Iosevka".into()), ..Preferences::default() };
        let text = toml::to_string_pretty(&prefs).unwrap();
        assert_eq!(Preferences::parse(&text), prefs);
    }

    #[test]
    fn shell_commands_keep_quoted_arguments_and_reject_unclosed_quotes() {
        assert_eq!(
            shell_command("env FOO='a b' zsh -l"),
            Some(("env".into(), vec!["FOO=a b".into(), "zsh".into(), "-l".into()]))
        );
        assert_eq!(shell_command("zsh '"), None);
        assert_eq!(shell_command("   "), None);
    }

    #[test]
    fn saving_a_profile_preserves_comments_unknown_keys_and_rejects_invalid_preferences() {
        let folder =
            std::env::temp_dir().join(format!("glide-terminal-config-{}", WRITES.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("terminal.toml");
        let text = "# preserve me\nfont-size = 17.0\nfuture-option = true\n";
        std::fs::write(&path, text).unwrap();
        save_profile(&path, Profile::Ocean).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("# preserve me") && saved.contains("future-option = true"));
        assert_eq!(Preferences::parse(&saved).font_size, 17.0);
        assert_eq!(Preferences::parse(&saved).profile, Profile::Ocean);
        for invalid in ["font-size = [", "on-exit = 'not-an-option'"] {
            std::fs::write(&path, invalid).unwrap();
            assert!(save_profile(&path, Profile::Basic).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), invalid);
        }
        std::fs::remove_dir_all(folder).unwrap();
    }
}

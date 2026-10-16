use std::path::{Path, PathBuf};
use std::time::SystemTime;

use alacritty_terminal::term::Config;
use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle};
use glide_theme::{Accent, Appearance};
use serde::{Deserialize, Serialize};

use crate::i18n::Lang;
use crate::profile::Profile;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub appearance: Appearance,
    pub accent: Accent,
    pub language: Option<Lang>,
    pub motion_scale: f32,
    pub sounds: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Shell {
    general: General,
    topbar: Topbar,
    animations: Animations,
    notifications: Notifications,
}

#[derive(Deserialize)]
#[serde(default)]
struct Notifications {
    sounds: bool,
}

impl Default for Notifications {
    fn default() -> Self {
        Self { sounds: true }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct General {
    language: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Topbar {
    appearance: Appearance,
    accent: Accent,
}

#[derive(Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct Animations {
    enabled: bool,
    speed: f32,
    respect_system: bool,
}

impl Default for Animations {
    fn default() -> Self {
        Self { enabled: true, speed: 1.0, respect_system: true }
    }
}

impl Look {
    #[must_use]
    pub fn parse(text: &str, reduced_motion: bool) -> Self {
        let shell: Shell = toml::from_str(text).unwrap_or_default();
        let animations = &shell.animations;
        let still = !animations.enabled || (animations.respect_system && reduced_motion);
        Self {
            appearance: shell.topbar.appearance,
            accent: shell.topbar.accent,
            language: shell.general.language.as_deref().and_then(Lang::from_key),
            motion_scale: if still { 0.0 } else { animations.speed },
            sounds: shell.notifications.sounds,
        }
    }
}

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
    pub fn term_config(&self) -> Config {
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

#[must_use]
pub fn reduced_motion() -> bool {
    std::env::var("GLIDE_REDUCED_MOTION").is_ok_and(|value| value.trim() == "1")
}

#[must_use]
pub fn shell_path() -> Option<PathBuf> {
    glide_config::xdg_path("glide", "glide.toml")
}

#[must_use]
pub fn path() -> Option<PathBuf> {
    glide_config::xdg_path("glide", "terminal.toml")
}

#[must_use]
pub fn stamp(path: Option<&Path>) -> Option<SystemTime> {
    std::fs::metadata(path?).and_then(|meta| meta.modified()).ok()
}

#[must_use]
pub fn load_look() -> Look {
    let text = shell_path().and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default();
    Look::parse(&text, reduced_motion())
}

#[must_use]
pub fn load() -> Preferences {
    path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map_or_else(Preferences::default, |text| Preferences::parse(&text))
}

pub fn save(prefs: &Preferences) -> std::io::Result<()> {
    let path = path().ok_or_else(|| std::io::Error::other("no config directory"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(prefs).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
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
    fn the_shell_look_follows_glide_toml() {
        let look = Look::parse("[topbar]\nappearance = \"light\"\n[general]\nlanguage = \"en\"\n", false);
        assert_eq!(look.appearance, Appearance::Light);
        assert_eq!(look.language, Some(Lang::En));
        assert!(look.sounds);
        assert!(!Look::parse("[notifications]\nsounds = false\n", false).sounds);
        assert_eq!(Look::parse("[animations]\nenabled = false\n", false).motion_scale, 0.0);
        assert_eq!(Look::parse("", true).motion_scale, 0.0);
    }
}

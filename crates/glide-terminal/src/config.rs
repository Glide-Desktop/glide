use std::path::PathBuf;

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
    pub bell_sound: Option<String>,
    pub on_exit: OnExit,
    pub confirm_close: bool,
    pub mark_prompts: bool,
    pub login_environment: bool,
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
            login_environment: true,
            shell: None,
        }
    }
}

pub const NAMESPACE: &str = "terminal";

impl Preferences {
    #[must_use]
    fn clamped(mut self) -> Self {
        self.font_size = self.font_size.clamp(6.0, 48.0);
        self.line_height = self.line_height.clamp(0.9, 2.0);
        self.scrollback = self.scrollback.min(200_000);
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

#[must_use]
pub fn legacy_path() -> Option<PathBuf> {
    glide_config::xdg_path("glide", "terminal.toml")
}

pub fn import_legacy() {
    if let Some(path) = legacy_path() {
        glide_config::service::import_legacy(NAMESPACE, &path);
    }
}

#[must_use]
pub fn load() -> Preferences {
    let values = glide_config::service::Client::session().and_then(|client| client.get_all(NAMESPACE));
    values.map_or_else(|_| Preferences::default(), |values| Preferences::from_values(&values))
}

pub fn save(prefs: &Preferences) -> std::io::Result<()> {
    glide_config::service::update::<Preferences>(NAMESPACE, |stored| stored.profile = prefs.profile)
        .map(drop)
        .map_err(std::io::Error::other)
}

#[must_use]
pub fn changes() -> iced::futures::channel::mpsc::UnboundedReceiver<glide_config::service::Values> {
    glide_app::config::watch(NAMESPACE)
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
    fn the_installed_defaults_are_the_preferences_defaults() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/defaults/terminal.json");
        let values =
            serde_json::to_value(Preferences::default()).map(glide_config::service::flatten).expect("serializes");
        let expected = serde_json::to_string_pretty(&values).expect("JSON") + "\n";
        if std::env::var_os("GLIDE_WRITE_DEFAULTS").is_some() {
            std::fs::write(&path, &expected).expect("write the defaults");
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_default(),
            expected,
            "data/defaults/terminal.json is generated from Preferences; rerun with GLIDE_WRITE_DEFAULTS=1"
        );
    }

    #[test]
    fn stored_values_are_clamped_like_the_file_was() {
        let values = glide_config::service::Values::from([
            ("font-size".to_owned(), serde_json::json!(400.0)),
            ("shell".to_owned(), serde_json::json!("fish")),
        ]);
        let prefs = Preferences::from_values(&values);
        assert_eq!((prefs.font_size, prefs.shell.as_deref()), (48.0, Some("fish")));
    }
}

use std::path::{Path, PathBuf};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use glide_iced::Paint;
use glide_theme::{Accent, Appearance};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Lang {
    #[default]
    Uk,
    En,
}

impl Lang {
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "uk" | "ua" | "ukrainian" => Some(Self::Uk),
            "en" | "english" => Some(Self::En),
            _ => None,
        }
    }

    #[must_use]
    pub fn from_locale(locale: &str) -> Self {
        if locale.is_empty() || locale.starts_with("uk") { Self::Uk } else { Self::En }
    }

    #[must_use]
    pub fn from_env() -> Self {
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
            .unwrap_or_default();
        Self::from_locale(&locale)
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Uk => "uk",
            Self::En => "en",
        }
    }

    #[must_use]
    pub fn pick<'a>(self, uk: &'a str, en: &'a str) -> &'a str {
        match self {
            Self::Uk => uk,
            Self::En => en,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub appearance: Appearance,
    pub appearance_schedule: bool,
    pub accent: Accent,
    pub language: Option<Lang>,
    pub motion_scale: f32,
    pub font_family: Option<String>,
    pub icon_theme: Option<String>,
    pub sounds: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            appearance_schedule: false,
            accent: Accent::default(),
            language: None,
            motion_scale: 1.0,
            font_family: None,
            icon_theme: None,
            sounds: true,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct File {
    general: General,
    topbar: Topbar,
    dock: Dock,
    animations: Animations,
    notifications: Notifications,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "kebab-case")]
struct General {
    language: Option<String>,
    font_family: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "kebab-case")]
struct Topbar {
    appearance: Appearance,
    accent: Accent,
    appearance_schedule: bool,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "kebab-case")]
struct Dock {
    icon_theme: Option<String>,
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

fn named(name: Option<String>) -> Option<String> {
    name.filter(|name| !name.trim().is_empty())
}

impl Look {
    #[must_use]
    pub fn parse(text: &str, reduced_motion: bool) -> Self {
        let file: File = toml::from_str(text).unwrap_or_default();
        let animations = &file.animations;
        let still = !animations.enabled || (animations.respect_system && reduced_motion);
        Self {
            appearance: file.topbar.appearance,
            appearance_schedule: file.topbar.appearance_schedule,
            accent: file.topbar.accent,
            language: file.general.language.as_deref().and_then(Lang::from_key),
            motion_scale: if still { 0.0 } else { animations.speed.max(0.0) },
            font_family: named(file.general.font_family),
            icon_theme: named(file.dock.icon_theme),
            sounds: file.notifications.sounds,
        }
    }

    #[must_use]
    pub fn load() -> Self {
        let text = shell_path().and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default();
        Self::parse(&text, reduced_motion())
    }

    #[must_use]
    pub fn appearance_at(&self, minute: Option<u16>) -> Appearance {
        minute.filter(|_| self.appearance_schedule).map_or(self.appearance, Appearance::scheduled)
    }

    #[must_use]
    pub fn appearance_now(&self) -> Appearance {
        self.appearance_at(glide_config::local_minute())
    }

    #[must_use]
    pub fn lang(&self) -> Lang {
        self.language.unwrap_or_else(Lang::from_env)
    }

    #[must_use]
    pub fn paint(&self) -> Paint {
        Paint::new(self.appearance_now(), self.accent)
    }

    #[must_use]
    pub fn with_shell(mut self, shell: Option<&glide_app::Look>) -> Self {
        if let Some(shell) = shell {
            self.appearance = shell.appearance;
            self.appearance_schedule = false;
            self.accent = shell.accent;
            self.language = shell.language.as_deref().and_then(Lang::from_key);
            self.motion_scale = shell.animation_scale;
            self.font_family.clone_from(&shell.font_family);
            self.icon_theme.clone_from(&shell.icon_theme);
        }
        self
    }
}

#[must_use]
pub fn shell_path() -> Option<PathBuf> {
    glide_config::xdg_path("glide", "glide.toml")
}

#[must_use]
pub fn stamp(path: Option<&Path>) -> Option<SystemTime> {
    std::fs::metadata(path?).and_then(|meta| meta.modified()).ok()
}

static SYSTEM_REDUCED: AtomicBool = AtomicBool::new(false);

#[must_use]
pub fn reduced_motion() -> bool {
    SYSTEM_REDUCED.load(Ordering::Relaxed)
        || std::env::var("GLIDE_REDUCED_MOTION").is_ok_and(|value| value.trim() == "1")
}

fn system_reduced() -> bool {
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .is_ok_and(|output| output.status.success() && output.stdout.trim_ascii() == b"false")
}

pub fn watch_reduced_motion() {
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        let spawned = std::thread::Builder::new().name("glide-sdk-motion".into()).spawn(|| {
            loop {
                SYSTEM_REDUCED.store(system_reduced(), Ordering::Relaxed);
                std::thread::sleep(Duration::from_secs(2));
            }
        });
        if let Err(err) = spawned {
            tracing::debug!("cannot watch reduced motion: {err}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_is_the_default_look() {
        assert_eq!(Look::parse("", false), Look::default());
    }

    #[test]
    fn the_shell_file_sets_every_field_an_app_follows() {
        let look = Look::parse(
            "[general]\nlanguage = \"en\"\nfont-family = \"Inter\"\n[topbar]\nappearance = \"light\"\naccent = \"teal\"\n\
             [dock]\nicon-theme = \"Papirus\"\n[animations]\nspeed = 0.5\n[notifications]\nsounds = false\n",
            false,
        );
        assert_eq!(look.appearance, Appearance::Light);
        assert_eq!(look.accent, Accent::Teal);
        assert_eq!(look.language, Some(Lang::En));
        assert_eq!(look.font_family.as_deref(), Some("Inter"));
        assert_eq!(look.icon_theme.as_deref(), Some("Papirus"));
        assert!((look.motion_scale - 0.5).abs() < f32::EPSILON);
        assert!(!look.sounds);
    }

    #[test]
    fn reduced_motion_stills_animations_only_while_respected() {
        assert_eq!(Look::parse("", true).motion_scale, 0.0);
        assert_eq!(Look::parse("[animations]\nrespect-system = false\n", true).motion_scale, 1.0);
        assert_eq!(Look::parse("[animations]\nenabled = false\n", false).motion_scale, 0.0);
    }

    #[test]
    fn the_running_shell_overrides_the_file_and_its_schedule() {
        let file = Look::parse("[topbar]\nappearance-schedule = true\n", false);
        let shell = glide_app::Look {
            appearance: Appearance::Light,
            accent: Accent::Teal,
            language: Some("en".into()),
            animation_scale: 0.0,
            reduced_motion: true,
            font_family: None,
            icon_theme: Some("Papirus".into()),
            locked: false,
            active_workspace: 0,
        };
        let look = file.with_shell(Some(&shell));
        assert!(!look.appearance_schedule);
        assert_eq!(look.appearance_at(Some(23 * 60)), Appearance::Light);
        assert_eq!(look.lang(), Lang::En);
        assert_eq!(look.motion_scale, 0.0);
    }

    #[test]
    fn a_ukrainian_or_unset_locale_is_ukrainian() {
        assert_eq!(Lang::from_locale(""), Lang::Uk);
        assert_eq!(Lang::from_locale("uk_UA.UTF-8"), Lang::Uk);
        assert_eq!(Lang::from_locale("en_US.UTF-8"), Lang::En);
        assert_eq!(Lang::from_locale("de_DE.UTF-8"), Lang::En);
    }
}

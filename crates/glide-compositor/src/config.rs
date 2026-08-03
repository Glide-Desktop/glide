//! TOML Configuration for Glide.
//!
//! Config file path: `$XDG_CONFIG_HOME/glide/glide.toml` (or `~/.config/glide/glide.toml`).
//! If missing or invalid, logs a warning and falls back to defaults.

use serde::Deserialize;
use std::{fs, path::PathBuf};

use smithay::input::keyboard::{Keysym, ModifiersState, keysyms, xkb};

use crate::{GlideState, input::ResolvedKeybind};

/// Root configuration struct.
///
/// Every section rejects unknown keys. A typo in a config file is far more
/// likely than a deliberate extra key, and silently ignoring it would leave
/// the user staring at a setting that appears to do nothing.
///
/// Not `Eq`: `decorations` holds colors as `f32`, which isn't.
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub workspaces: WorkspaceConfig,
    pub keybinds: Vec<KeybindConfig>,
    pub decorations: DecorationTheme,
}

/// A window decoration's color scheme and size.
///
/// Layout is deliberately *not* configurable here, only appearance: maximize
/// sits on the left and close on the right always, so every window in a
/// session looks and behaves the same regardless of what an individual user
/// put in their config. Only colors and the titlebar's height are settable.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DecorationTheme {
    /// Titlebar height in logical pixels.
    pub height: u32,
    pub background: Color,
    pub background_focused: Color,
    pub close_button: Color,
    pub close_button_hover: Color,
    pub maximize_button: Color,
    pub maximize_button_hover: Color,
    pub chevron_button: Color,
    pub chevron_button_hover: Color,
    pub menu_background: Color,
    pub menu_item_hover: Color,
}

impl Default for DecorationTheme {
    fn default() -> Self {
        Self {
            height: 32,
            background: Color::from_hex("#26262eff").unwrap(),
            background_focused: Color::from_hex("#3b5bdbff").unwrap(),
            close_button: Color::from_hex("#e2555aff").unwrap(),
            close_button_hover: Color::from_hex("#ff7377ff").unwrap(),
            maximize_button: Color::from_hex("#4a4a56ff").unwrap(),
            maximize_button_hover: Color::from_hex("#63636fff").unwrap(),
            chevron_button: Color::from_hex("#4a4a56ff").unwrap(),
            chevron_button_hover: Color::from_hex("#63636fff").unwrap(),
            menu_background: Color::from_hex("#1c1c22ff").unwrap(),
            menu_item_hover: Color::from_hex("#3b5bdbff").unwrap(),
        }
    }
}

/// An RGBA color, written in config as `"#rrggbb"` or `"#rrggbbaa"`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color(pub [f32; 4]);

impl Color {
    /// Parses `#rrggbb` (opaque) or `#rrggbbaa`. Returns `None` for anything
    /// else — no partial parsing, a malformed color is treated the same as
    /// a missing one.
    // r/g/b/a is the universally understood name for color channels; naming
    // them out in full would hurt readability rather than help it.
    #[allow(clippy::many_single_char_names)]
    fn from_hex(s: &str) -> Option<Self> {
        let s = s.strip_prefix('#')?;
        let byte = |range: std::ops::Range<usize>| u8::from_str_radix(s.get(range)?, 16).ok();

        let (r, g, b, a) = match s.len() {
            6 => (byte(0..2)?, byte(2..4)?, byte(4..6)?, 255),
            8 => (byte(0..2)?, byte(2..4)?, byte(4..6)?, byte(6..8)?),
            _ => return None,
        };

        Some(Self([
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            f32::from(a) / 255.0,
        ]))
    }
}

impl<'de> serde::Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_hex(&s).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "invalid color {s:?}: expected \"#rrggbb\" or \"#rrggbbaa\""
            ))
        })
    }
}

/// Workspaces configuration section.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct WorkspaceConfig {
    #[serde(rename = "new-window")]
    pub new_window: NewWindowPolicyConfig,
}

/// Placement policy for new windows.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum NewWindowPolicyConfig {
    #[default]
    NewWorkspace,
    ActiveWorkspace,
}

impl From<NewWindowPolicyConfig> for glide_layout::NewWindowPolicy {
    fn from(cfg: NewWindowPolicyConfig) -> Self {
        match cfg {
            NewWindowPolicyConfig::NewWorkspace => glide_layout::NewWindowPolicy::NewWorkspace,
            NewWindowPolicyConfig::ActiveWorkspace => {
                glide_layout::NewWindowPolicy::ActiveWorkspace
            }
        }
    }
}

/// Representation of key modifiers in TOML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModifierConfig {
    Super,
    Logo,
    Shift,
    Ctrl,
    Control,
    Alt,
}

/// Representation of a single keybinding entry in TOML.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeybindConfig {
    #[serde(default)]
    pub mods: Vec<ModifierConfig>,
    pub key: String,
    pub action: KeyActionConfig,
}

/// Configurable keybinding actions in TOML.
///
/// Either a bare string (`action = "quit"`) or a single-key table
/// (`action = { exec = "alacritty" }`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum KeyActionConfig {
    Simple(String),
    Complex(ParameterizedAction),
}

/// Actions that take a parameter.
///
/// `deny_unknown_fields` matters more here than anywhere else: with every
/// field optional, `action = { exce = "foo" }` would otherwise deserialize
/// into an all-`None` value and the binding would vanish without a word.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterizedAction {
    #[serde(rename = "switch-workspace")]
    pub switch_workspace: Option<usize>,
    #[serde(rename = "move-window-to-workspace")]
    pub move_window_to_workspace: Option<usize>,
    pub exec: Option<String>,
    pub vt: Option<i32>,
}

impl ParameterizedAction {
    fn empty() -> Self {
        Self {
            switch_workspace: None,
            move_window_to_workspace: None,
            exec: None,
            vt: None,
        }
    }

    fn switch_workspace(index: usize) -> Self {
        Self {
            switch_workspace: Some(index),
            ..Self::empty()
        }
    }

    fn move_window_to_workspace(index: usize) -> Self {
        Self {
            move_window_to_workspace: Some(index),
            ..Self::empty()
        }
    }
}

impl Config {
    /// Returns default keybindings table.
    #[must_use]
    pub fn default_keybinds() -> Vec<KeybindConfig> {
        let mut keybinds = vec![
            KeybindConfig {
                mods: vec![ModifierConfig::Super],
                key: "q".to_string(),
                action: KeyActionConfig::Simple("quit".to_string()),
            },
            KeybindConfig {
                mods: vec![ModifierConfig::Super],
                key: "Tab".to_string(),
                action: KeyActionConfig::Simple("next-workspace".to_string()),
            },
            KeybindConfig {
                mods: vec![ModifierConfig::Super, ModifierConfig::Shift],
                key: "Tab".to_string(),
                action: KeyActionConfig::Simple("prev-workspace".to_string()),
            },
            KeybindConfig {
                mods: vec![ModifierConfig::Super],
                key: "f".to_string(),
                action: KeyActionConfig::Simple("maximize".to_string()),
            },
            KeybindConfig {
                mods: vec![ModifierConfig::Super, ModifierConfig::Shift],
                key: "f".to_string(),
                action: KeyActionConfig::Simple("fullscreen".to_string()),
            },
            KeybindConfig {
                mods: vec![ModifierConfig::Super],
                key: "m".to_string(),
                action: KeyActionConfig::Simple("minimize".to_string()),
            },
        ];

        for i in 1..=9 {
            keybinds.push(KeybindConfig {
                mods: vec![ModifierConfig::Super],
                key: i.to_string(),
                action: KeyActionConfig::Complex(ParameterizedAction::switch_workspace(i)),
            });

            keybinds.push(KeybindConfig {
                mods: vec![ModifierConfig::Super, ModifierConfig::Shift],
                key: i.to_string(),
                action: KeyActionConfig::Complex(ParameterizedAction::move_window_to_workspace(i)),
            });
        }

        keybinds
    }

    /// Returns the default config path: `$XDG_CONFIG_HOME/glide/glide.toml` or `~/.config/glide/glide.toml`.
    #[must_use]
    pub fn default_path() -> Option<PathBuf> {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return Some(PathBuf::from(xdg).join("glide").join("glide.toml"));
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                return Some(
                    PathBuf::from(home)
                        .join(".config")
                        .join("glide")
                        .join("glide.toml"),
                );
            }
        }
        None
    }

    /// Loads config from the default location. If the file does not exist or fails to parse,
    /// logs a warning and returns `Config::with_defaults()`.
    #[must_use]
    pub fn load_or_default() -> Self {
        let Some(path) = Self::default_path() else {
            tracing::info!("could not determine XDG_CONFIG_HOME or HOME; using default config");
            return Self::with_defaults();
        };

        Self::load_from_path(&path)
    }

    /// Returns default config populated with default keybindings.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self {
            workspaces: WorkspaceConfig::default(),
            keybinds: Self::default_keybinds(),
            decorations: DecorationTheme::default(),
        }
    }

    /// Loads config from a specific path.
    #[must_use]
    pub fn load_from_path(path: &std::path::Path) -> Self {
        if !path.exists() {
            tracing::info!(path = %path.display(), "config file not found; using default config");
            return Self::with_defaults();
        }

        match fs::read_to_string(path) {
            Ok(content) => match toml::from_str::<Config>(&content) {
                Ok(mut config) => {
                    if config.keybinds.is_empty() {
                        config.keybinds = Self::default_keybinds();
                    }
                    tracing::info!(path = %path.display(), "config loaded successfully");
                    config
                }
                Err(err) => {
                    tracing::warn!(
                        path = %path.display(),
                        ?err,
                        "failed to parse config file; using default config"
                    );
                    Self::with_defaults()
                }
            },
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    ?err,
                    "failed to read config file; using default config"
                );
                Self::with_defaults()
            }
        }
    }

    /// Resolves config keybindings into executable `ResolvedKeybind` list.
    #[must_use]
    pub fn resolve_keybinds(&self) -> Vec<ResolvedKeybind> {
        let mut resolved = Vec::new();

        for entry in &self.keybinds {
            let mut mods = ModifiersState::default();
            for m in &entry.mods {
                match m {
                    ModifierConfig::Super | ModifierConfig::Logo => mods.logo = true,
                    ModifierConfig::Shift => mods.shift = true,
                    ModifierConfig::Ctrl | ModifierConfig::Control => mods.ctrl = true,
                    ModifierConfig::Alt => mods.alt = true,
                }
            }

            let keysym = parse_keysym(&entry.key);
            let action = parse_action(&entry.action);

            if let (Some(keysym), Some(action)) = (keysym, action) {
                resolved.push(ResolvedKeybind {
                    modifiers: mods,
                    keysym,
                    action,
                });
            }
        }

        resolved
    }

    /// Applies configuration to `GlideState`.
    ///
    /// Note: a changed `[decorations]` height only takes effect for windows
    /// mapped *after* this call (e.g. via `glidectl reload`) — insets on
    /// already-open windows are set once at creation and are not currently
    /// live-updated. Colors, being read at render time, do apply instantly.
    pub fn apply_to_state(&self, state: &mut GlideState) {
        state.layout.set_policy(self.workspaces.new_window.into());
        state.keybinds = self.resolve_keybinds();
        state.decoration_theme = self.decorations.clone();
        tracing::info!(
            policy = ?self.workspaces.new_window,
            keybind_count = state.keybinds.len(),
            "applied configuration"
        );
    }
}

fn parse_keysym(s: &str) -> Option<Keysym> {
    if let Ok(num) = s.parse::<u32>() {
        match num {
            1 => return Some(Keysym::from(keysyms::KEY_1)),
            2 => return Some(Keysym::from(keysyms::KEY_2)),
            3 => return Some(Keysym::from(keysyms::KEY_3)),
            4 => return Some(Keysym::from(keysyms::KEY_4)),
            5 => return Some(Keysym::from(keysyms::KEY_5)),
            6 => return Some(Keysym::from(keysyms::KEY_6)),
            7 => return Some(Keysym::from(keysyms::KEY_7)),
            8 => return Some(Keysym::from(keysyms::KEY_8)),
            9 => return Some(Keysym::from(keysyms::KEY_9)),
            0 => return Some(Keysym::from(keysyms::KEY_0)),
            _ => {}
        }
    }

    let sym_raw = xkb::keysym_from_name(s, xkb::KEYSYM_NO_FLAGS);
    if sym_raw.raw() == keysyms::KEY_NoSymbol {
        tracing::warn!(key = s, "unrecognized key name in config");
        None
    } else {
        Some(sym_raw)
    }
}

fn parse_action(cfg: &KeyActionConfig) -> Option<crate::input::KeyAction> {
    match cfg {
        KeyActionConfig::Simple(cmd) => match cmd.to_lowercase().as_str() {
            "quit" => Some(crate::input::KeyAction::Quit),
            "next-workspace" => Some(crate::input::KeyAction::NextWorkspace),
            "prev-workspace" => Some(crate::input::KeyAction::PrevWorkspace),
            "maximize" => Some(crate::input::KeyAction::Maximize),
            "fullscreen" => Some(crate::input::KeyAction::Fullscreen),
            "minimize" => Some(crate::input::KeyAction::Minimize),
            _ => {
                tracing::warn!(action = cmd, "unknown simple key action");
                None
            }
        },
        KeyActionConfig::Complex(action) => {
            // Workspace numbers are 1-based in config (matching the key you
            // press) but 0-based internally.
            if let Some(idx) = action.switch_workspace {
                Some(crate::input::KeyAction::SwitchWorkspace(
                    idx.saturating_sub(1),
                ))
            } else if let Some(idx) = action.move_window_to_workspace {
                Some(crate::input::KeyAction::MoveWindowToWorkspace(
                    idx.saturating_sub(1),
                ))
            } else if let Some(cmd) = &action.exec {
                Some(crate::input::KeyAction::Exec(cmd.clone()))
            } else if let Some(vt) = action.vt {
                Some(crate::input::KeyAction::VtSwitch(vt))
            } else {
                tracing::warn!(
                    "keybind action table is empty; expected one of \
                     switch-workspace, move-window-to-workspace, exec or vt"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::with_defaults();
        assert_eq!(
            config.workspaces.new_window,
            NewWindowPolicyConfig::NewWorkspace
        );
        assert!(!config.keybinds.is_empty());
    }

    #[test]
    fn test_parse_valid_toml_keybinds() {
        let toml_str = r#"
[workspaces]
new-window = "active-workspace"

[[keybinds]]
mods = ["super"]
key = "Return"
action = { exec = "alacritty" }

[[keybinds]]
mods = ["super"]
key = "q"
action = "quit"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.workspaces.new_window,
            NewWindowPolicyConfig::ActiveWorkspace
        );
        assert_eq!(config.keybinds.len(), 2);
        assert_eq!(
            config.keybinds[0].action,
            KeyActionConfig::Complex(ParameterizedAction {
                exec: Some("alacritty".to_string()),
                ..ParameterizedAction::empty()
            })
        );
    }

    #[test]
    fn unknown_top_level_key_is_rejected() {
        // Silently ignoring a typo would leave the user tweaking a setting
        // that never takes effect.
        let err = toml::from_str::<Config>("[workspacs]\nnew-window = \"active-workspace\"\n")
            .expect_err("a misspelled section must not parse");

        assert!(err.to_string().contains("workspacs"), "{err}");
    }

    // These compare exact values from an 8-bit-to-f32 conversion (0/255 and
    // 255/255 both land on exact floats, no accumulated rounding), so exact
    // equality is correct here — see input.rs's clamp_to_outputs tests for
    // the same reasoning applied to clamp() output.
    #[test]
    #[allow(clippy::float_cmp)]
    fn color_parses_rrggbb_as_opaque() {
        let c = Color::from_hex("#ff0000").unwrap();
        assert_eq!(c.0, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn color_parses_rrggbbaa() {
        let c = Color::from_hex("#00ff0080").unwrap();
        assert_eq!(c.0[1], 1.0);
        assert!((c.0[3] - 0.502).abs() < 0.01);
    }

    #[test]
    fn color_rejects_malformed_hex() {
        assert!(Color::from_hex("ff0000").is_none(), "missing #");
        assert!(Color::from_hex("#ff00").is_none(), "wrong length");
        assert!(Color::from_hex("#gggggg").is_none(), "not hex digits");
    }

    #[test]
    fn decoration_theme_parses_from_toml() {
        let toml_str = "\
[decorations]
height = 40
background = \"#111111\"
background-focused = \"#222222\"
close-button = \"#333333\"
close-button-hover = \"#444444\"
maximize-button = \"#555555\"
maximize-button-hover = \"#666666\"
chevron-button = \"#777777\"
chevron-button-hover = \"#888888\"
menu-background = \"#999999\"
menu-item-hover = \"#aaaaaa\"
";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.decorations.height, 40);
        assert_eq!(
            config.decorations.background,
            Color::from_hex("#111111").unwrap()
        );
    }

    #[test]
    fn decoration_theme_rejects_invalid_color() {
        toml::from_str::<Config>("[decorations]\nbackground = \"not-a-color\"\n")
            .expect_err("a malformed color must not silently become black or default");
    }

    #[test]
    fn unknown_action_key_is_rejected() {
        let toml_str = "\
[[keybinds]]
mods = [\"super\"]
key = \"Return\"
action = { exce = \"alacritty\" }
";

        toml::from_str::<Config>(toml_str)
            .expect_err("a misspelled action key must not silently vanish");
    }

    #[test]
    fn invalid_config_falls_back_to_defaults() {
        // Dying over a config typo would strand the user on a bare tty with
        // no way to edit the file that caused it.
        let dir = std::env::temp_dir().join(format!("glide-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("glide.toml");
        std::fs::write(&path, "this is not valid toml {{{").expect("write config");

        let config = Config::load_from_path(&path);

        assert_eq!(config, Config::with_defaults());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

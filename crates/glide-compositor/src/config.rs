use serde::Deserialize;
use std::{fs, path::PathBuf};

use smithay::input::keyboard::{Keysym, ModifiersState, keysyms, xkb};

use crate::{GlideState, input::ResolvedKeybind};

pub use glide_theme::{Color, DecorationTheme, OverviewTheme, TitleAlign, WallpaperFit};

pub trait OverviewThemeExt {
    fn options(&self) -> glide_layout::OverviewOptions;
}

impl OverviewThemeExt for OverviewTheme {
    fn options(&self) -> glide_layout::OverviewOptions {
        glide_layout::OverviewOptions {
            gap: self.gap,
            label_height: if self.label { self.label_height } else { 0 },
            max_scale: self.max_scale,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: GeneralConfig,
    pub workspaces: WorkspaceConfig,
    pub keybinds: Vec<KeybindConfig>,
    pub decorations: DecorationTheme,
    pub overview: OverviewTheme,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct GeneralConfig {
    pub background: Color,
    pub autostart: Vec<String>,
    pub wallpaper: Option<PathBuf>,
    pub wallpaper_fit: WallpaperFit,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            background: Color::from_hex("#222c3cff").unwrap(),
            autostart: vec!["alacritty".to_string()],
            wallpaper: None,
            wallpaper_fit: WallpaperFit::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct WorkspaceConfig {
    #[serde(rename = "new-window")]
    pub new_window: NewWindowPolicyConfig,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeybindConfig {
    #[serde(default)]
    pub mods: Vec<ModifierConfig>,
    pub key: String,
    pub action: KeyActionConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum KeyActionConfig {
    Simple(String),
    Complex(ParameterizedAction),
}

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
            KeybindConfig {
                mods: vec![ModifierConfig::Super],
                key: "o".to_string(),
                action: KeyActionConfig::Simple("toggle-overview".to_string()),
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

    #[must_use]
    pub fn default_path() -> Option<PathBuf> {
        glide_config::xdg_path("glide", "glide.toml")
    }

    #[must_use]
    pub fn load_or_default() -> Self {
        let Some(path) = Self::default_path() else {
            tracing::info!("could not determine XDG_CONFIG_HOME or HOME; using default config");
            return Self::with_defaults();
        };

        Self::load_from_path(&path)
    }

    #[must_use]
    pub fn with_defaults() -> Self {
        Self {
            general: GeneralConfig::default(),
            workspaces: WorkspaceConfig::default(),
            keybinds: Self::default_keybinds(),
            decorations: DecorationTheme::default(),
            overview: OverviewTheme::default(),
        }
    }

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

    pub fn apply_to_state(&self, state: &mut GlideState) {
        state.layout.set_policy(self.workspaces.new_window.into());
        state.keybinds = self.resolve_keybinds();
        state.decoration_theme = self.decorations.clone();
        state.overview_theme = self.overview.clone();
        state.background_color = self.general.background;
        state.wallpaper_fit = self.general.wallpaper_fit;
        state.refresh_painter_font();

        let wallpaper_bytes = self.general.wallpaper.as_ref().and_then(|path| {
            fs::read(path)
                .inspect_err(|err| {
                    tracing::warn!(path = %path.display(), "failed to read wallpaper: {err}");
                })
                .ok()
        });
        glide_paint::init_wallpaper(wallpaper_bytes.as_deref());

        if let Ok(height) = i32::try_from(self.decorations.height) {
            let insets = glide_layout::Insets {
                top: height,
                left: 0,
                right: 0,
                bottom: 0,
            };
            let window_ids: Vec<glide_layout::WindowId> =
                state.window_map.keys().copied().collect();
            for id in window_ids {
                state.layout.set_window_insets(id, insets);
            }
            state.reconcile_space();
        }

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
            "toggle-overview" => Some(crate::input::KeyAction::ToggleOverview),
            _ => {
                tracing::warn!(action = cmd, "unknown simple key action");
                None
            }
        },
        KeyActionConfig::Complex(action) => {
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
        let err = toml::from_str::<Config>("[workspacs]\nnew-window = \"active-workspace\"\n")
            .expect_err("a misspelled section must not parse");

        assert!(err.to_string().contains("workspacs"), "{err}");
    }

    #[test]
    fn general_config_defaults_to_meridian_dark_theme_wallpaper_color() {
        assert_eq!(
            GeneralConfig::default().background,
            Color::from_hex("#222c3cff").unwrap()
        );
    }

    #[test]
    fn general_background_parses_from_toml() {
        let toml_str = "\
[general]
background = \"#101020\"
";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.general.background,
            Color::from_hex("#101020").unwrap()
        );
    }

    #[test]
    fn unknown_general_key_is_rejected() {
        toml::from_str::<Config>("[general]\nbackgroud = \"#101020\"\n")
            .expect_err("a misspelled key must not silently vanish");
    }

    #[test]
    fn autostart_defaults_to_a_single_terminal() {
        assert_eq!(GeneralConfig::default().autostart, vec!["alacritty"]);
    }

    #[test]
    fn autostart_parses_a_list_of_command_lines_from_toml() {
        let toml_str = "\
[general]
autostart = [\"foot\", \"waybar --config /tmp/bar.json\"]
";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.general.autostart,
            vec!["foot", "waybar --config /tmp/bar.json"]
        );
    }

    #[test]
    fn autostart_can_be_emptied_to_start_nothing() {
        let toml_str = "[general]\nautostart = []\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.general.autostart.is_empty());
    }

    #[test]
    fn wallpaper_defaults_to_none_with_cover_fit() {
        let general = GeneralConfig::default();
        assert_eq!(general.wallpaper, None);
        assert_eq!(general.wallpaper_fit, WallpaperFit::Cover);
    }

    #[test]
    fn wallpaper_path_and_fit_parse_from_toml() {
        let toml_str = "\
[general]
wallpaper = \"/home/user/Pictures/wall.png\"
wallpaper-fit = \"contain\"
";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.general.wallpaper,
            Some(PathBuf::from("/home/user/Pictures/wall.png"))
        );
        assert_eq!(config.general.wallpaper_fit, WallpaperFit::Contain);
    }

    #[test]
    fn wallpaper_fit_rejects_an_unknown_value() {
        let toml_str = "[general]\nwallpaper-fit = \"stretch\"\n";
        toml::from_str::<Config>(toml_str).expect_err("\"stretch\" is not a valid fit mode");
    }

    #[test]
    fn decoration_theme_parses_from_toml() {
        let toml_str = "\
[decorations]
height = 40
title-align = \"center\"
corner-radius = 12.0
background = \"#111111\"
background-focused = \"#222222\"
close-button = \"#333333\"
close-button-hover = \"#444444\"
maximize-button = \"#555555\"
maximize-button-hover = \"#666666\"
menu-background = \"#999999\"
menu-item-hover = \"#aaaaaa\"
icon = \"#bbbbbb\"
close-icon = \"#cccccc\"
border-width = 2
border = \"#dddddd\"
border-focused = \"#eeeeee\"
shadow = false
shadow-size = 24
shadow-color = \"#000000aa\"
";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.decorations.height, 40);
        assert_eq!(config.decorations.title_align, TitleAlign::Center);
        assert!((config.decorations.corner_radius - 12.0).abs() < f32::EPSILON);
        assert_eq!(
            config.decorations.background,
            Color::from_hex("#111111").unwrap()
        );
        assert_eq!(config.decorations.icon, Color::from_hex("#bbbbbb").unwrap());
        assert_eq!(
            config.decorations.close_icon,
            Color::from_hex("#cccccc").unwrap()
        );
        assert_eq!(config.decorations.border_width, 2);
        assert_eq!(
            config.decorations.border,
            Color::from_hex("#dddddd").unwrap()
        );
        assert_eq!(
            config.decorations.border_focused,
            Color::from_hex("#eeeeee").unwrap()
        );
        assert!(!config.decorations.shadow);
        assert_eq!(config.decorations.shadow_size, 24);
        assert_eq!(
            config.decorations.shadow_color,
            Color::from_hex("#000000aa").unwrap()
        );
    }

    #[test]
    fn decoration_theme_rejects_invalid_color() {
        toml::from_str::<Config>("[decorations]\nbackground = \"not-a-color\"\n")
            .expect_err("a malformed color must not silently become black or default");
    }

    #[test]
    fn overview_theme_parses_from_toml() {
        let toml_str = "\
[overview]
gap = 40
label-height = 30
max-scale = 0.9
label = false
label-color = \"#111111\"
label-background = \"#222222\"
dim = \"#33333380\"
highlight = \"#444444\"
border = 6
shadow = false
shadow-size = 12
shadow-color = \"#00000080\"
duration-ms = 150
";
        let config: Config = toml::from_str(toml_str).unwrap();
        let o = &config.overview;
        assert_eq!(o.gap, 40);
        assert_eq!(o.label_height, 30);
        assert!((o.max_scale - 0.9).abs() < f64::EPSILON);
        assert!(!o.label);
        assert_eq!(o.label_color, Color::from_hex("#111111").unwrap());
        assert_eq!(o.label_background, Color::from_hex("#222222").unwrap());
        assert_eq!(o.highlight, Color::from_hex("#444444").unwrap());
        assert_eq!(o.border, 6);
        assert!(!o.shadow);
        assert_eq!(o.shadow_size, 12);
        assert_eq!(o.shadow_color, Color::from_hex("#00000080").unwrap());
        assert_eq!(o.duration_ms, 150);
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
        let dir = std::env::temp_dir().join(format!("glide-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("glide.toml");
        std::fs::write(&path, "this is not valid toml {{{").expect("write config");

        let config = Config::load_from_path(&path);

        assert_eq!(config, Config::with_defaults());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

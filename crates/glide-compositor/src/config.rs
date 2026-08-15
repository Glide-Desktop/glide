use serde::Deserialize;
use std::{fs, path::PathBuf};

use smithay::input::keyboard::{Keysym, ModifiersState, keysyms, xkb};

use crate::{GlideState, input::ResolvedKeybind};

pub use glide_theme::{Color, DecorationTheme, DockTheme, OverviewTheme, TitleAlign, WallpaperFit};

pub trait OverviewThemeExt {
    fn options(&self) -> glide_layout::OverviewOptions;
}

impl OverviewThemeExt for OverviewTheme {
    fn options(&self) -> glide_layout::OverviewOptions {
        glide_layout::OverviewOptions {
            gap: self.gap,
            padding: self.padding,
            label_height: if self.label { self.label_height } else { 0 },
            label_offset: if self.label {
                crate::overview::label_offset(self.label_height)
            } else {
                0
            },
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
    pub dock: DockConfig,
    pub input: InputConfig,
    pub animations: AnimationConfig,
    #[serde(rename = "output")]
    pub outputs: Vec<OutputConfig>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct AnimationConfig {
    pub enabled: bool,
    pub speed: f32,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            speed: 1.0,
        }
    }
}

impl AnimationConfig {
    #[must_use]
    pub const fn motion_scale(self) -> f32 {
        if self.enabled { self.speed } else { 0.0 }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct InputConfig {
    pub xkb_layout: String,
    pub xkb_variant: String,
    pub xkb_options: Option<String>,
    pub xkb_model: String,
    pub xkb_rules: String,
    pub repeat_delay: i32,
    pub repeat_rate: i32,
    pub tap_to_click: bool,
    pub natural_scroll: bool,
    pub accel_profile: AccelProfileConfig,
    pub accel_speed: f64,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            xkb_layout: "us,ua".to_string(),
            xkb_variant: String::new(),
            xkb_options: Some("grp:win_space_toggle".to_string()),
            xkb_model: String::new(),
            xkb_rules: String::new(),
            repeat_delay: 200,
            repeat_rate: 25,
            tap_to_click: true,
            natural_scroll: false,
            accel_profile: AccelProfileConfig::default(),
            accel_speed: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AccelProfileConfig {
    #[default]
    Adaptive,
    Flat,
}

impl InputConfig {
    #[must_use]
    pub fn xkb(&self) -> smithay::input::keyboard::XkbConfig<'_> {
        smithay::input::keyboard::XkbConfig {
            rules: &self.xkb_rules,
            model: &self.xkb_model,
            layout: &self.xkb_layout,
            variant: &self.xkb_variant,
            options: self.xkb_options.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct OutputConfig {
    pub name: String,
    pub position: Option<[i32; 2]>,
    pub scale: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, rename_all = "kebab-case")]
pub struct DockConfig {
    #[serde(flatten)]
    pub theme: DockTheme,
    pub items: Vec<DockItemConfig>,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            theme: DockTheme::default(),
            items: DockItemConfig::defaults(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct DockItemConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub app_id: Option<String>,
    #[serde(default)]
    pub color_from: Option<Color>,
    #[serde(default)]
    pub color_to: Option<Color>,
    #[serde(default)]
    pub separator_before: bool,
}

impl DockItemConfig {
    fn defaults() -> Vec<Self> {
        vec![Self {
            name: "Terminal".to_string(),
            command: "alacritty".to_string(),
            icon: "terminal".to_string(),
            app_id: Some("Alacritty".to_string()),
            color_from: Color::from_hex("#4a4640"),
            color_to: Color::from_hex("#23201c"),
            separator_before: false,
        }]
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct GeneralConfig {
    pub background: Color,
    pub autostart: Vec<String>,
    pub wallpaper: Option<PathBuf>,
    pub wallpaper_fit: WallpaperFit,
    pub font_family: Option<String>,
    pub blank_after_seconds: u64,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            background: Color::from_hex("#222c3cff").unwrap(),
            autostart: vec!["alacritty".to_string()],
            wallpaper: None,
            wallpaper_fit: WallpaperFit::default(),
            font_family: None,
            blank_after_seconds: 600,
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
    #[allow(clippy::too_many_lines)]
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
            KeybindConfig {
                mods: vec![ModifierConfig::Alt],
                key: "F4".to_string(),
                action: KeyActionConfig::Simple("close-window".to_string()),
            },
            KeybindConfig {
                mods: vec![ModifierConfig::Alt],
                key: "space".to_string(),
                action: KeyActionConfig::Simple("open-window-menu".to_string()),
            },
            KeybindConfig {
                mods: vec![],
                key: "Print".to_string(),
                action: KeyActionConfig::Simple("screenshot".to_string()),
            },
            KeybindConfig {
                mods: vec![ModifierConfig::Super],
                key: "a".to_string(),
                action: KeyActionConfig::Simple("toggle-launcher".to_string()),
            },
            KeybindConfig {
                mods: vec![],
                key: "XF86AudioRaiseVolume".to_string(),
                action: KeyActionConfig::Complex(ParameterizedAction {
                    exec: Some("wpctl set-volume -l 1.5 @DEFAULT_AUDIO_SINK@ 5%+".to_string()),
                    ..ParameterizedAction::empty()
                }),
            },
            KeybindConfig {
                mods: vec![],
                key: "XF86AudioLowerVolume".to_string(),
                action: KeyActionConfig::Complex(ParameterizedAction {
                    exec: Some("wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-".to_string()),
                    ..ParameterizedAction::empty()
                }),
            },
            KeybindConfig {
                mods: vec![],
                key: "XF86AudioMute".to_string(),
                action: KeyActionConfig::Complex(ParameterizedAction {
                    exec: Some("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle".to_string()),
                    ..ParameterizedAction::empty()
                }),
            },
            KeybindConfig {
                mods: vec![],
                key: "XF86AudioMicMute".to_string(),
                action: KeyActionConfig::Complex(ParameterizedAction {
                    exec: Some("wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle".to_string()),
                    ..ParameterizedAction::empty()
                }),
            },
            KeybindConfig {
                mods: vec![],
                key: "XF86MonBrightnessUp".to_string(),
                action: KeyActionConfig::Complex(ParameterizedAction {
                    exec: Some("brightnessctl set +5%".to_string()),
                    ..ParameterizedAction::empty()
                }),
            },
            KeybindConfig {
                mods: vec![],
                key: "XF86MonBrightnessDown".to_string(),
                action: KeyActionConfig::Complex(ParameterizedAction {
                    exec: Some("brightnessctl set 5%-".to_string()),
                    ..ParameterizedAction::empty()
                }),
            },
            KeybindConfig {
                mods: vec![],
                key: "XF86AudioPlay".to_string(),
                action: KeyActionConfig::Complex(ParameterizedAction {
                    exec: Some("playerctl play-pause".to_string()),
                    ..ParameterizedAction::empty()
                }),
            },
            KeybindConfig {
                mods: vec![],
                key: "XF86AudioNext".to_string(),
                action: KeyActionConfig::Complex(ParameterizedAction {
                    exec: Some("playerctl next".to_string()),
                    ..ParameterizedAction::empty()
                }),
            },
            KeybindConfig {
                mods: vec![],
                key: "XF86AudioPrev".to_string(),
                action: KeyActionConfig::Complex(ParameterizedAction {
                    exec: Some("playerctl previous".to_string()),
                    ..ParameterizedAction::empty()
                }),
            },
            KeybindConfig {
                mods: vec![ModifierConfig::Super],
                key: "l".to_string(),
                action: KeyActionConfig::Complex(ParameterizedAction {
                    exec: Some("loginctl lock-session".to_string()),
                    ..ParameterizedAction::empty()
                }),
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
            dock: DockConfig::default(),
            input: InputConfig::default(),
            animations: AnimationConfig::default(),
            outputs: Vec::new(),
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
        glide_theme::motion::set_scale(self.animations.motion_scale());
        state.layout.set_policy(self.workspaces.new_window.into());
        state.keybinds = self.resolve_keybinds();
        state.decoration_theme = self.decorations.clone();
        state.overview_theme = self.overview.clone();
        state.dock.reconfigure(&self.dock);
        state.refresh_dock();
        state.launcher_theme = crate::launcher::LauncherTheme::from_dock(&self.dock.theme);
        state.launcher_icons = crate::icon::IconLoader::new(self.dock.theme.icon_theme.clone());
        state.background_color = self.general.background;
        state.wallpaper_fit = self.general.wallpaper_fit;
        state.font_family.clone_from(&self.general.font_family);
        state.blank_after = (self.general.blank_after_seconds > 0)
            .then(|| std::time::Duration::from_secs(self.general.blank_after_seconds));
        state.input_config.clone_from(&self.input);
        state.output_configs.clone_from(&self.outputs);
        state.apply_input_config();
        state.apply_output_configs();
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
            "close-window" => Some(crate::input::KeyAction::CloseFocused),
            "open-window-menu" => Some(crate::input::KeyAction::OpenWindowMenu),
            "toggle-overview" => Some(crate::input::KeyAction::ToggleOverview),
            "screenshot" => Some(crate::input::KeyAction::Screenshot),
            "toggle-launcher" => Some(crate::input::KeyAction::ToggleLauncher),
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

    mod animations_section {
        use super::*;

        #[test]
        fn it_defaults_to_the_spec_timing() {
            let config = Config::with_defaults();
            assert!(config.animations.enabled);
            assert!((config.animations.motion_scale() - 1.0).abs() < f32::EPSILON);
        }

        #[test]
        fn it_parses_both_keys() {
            let config: Config =
                toml::from_str("[animations]\nenabled = true\nspeed = 0.5\n").unwrap();
            assert!((config.animations.motion_scale() - 0.5).abs() < f32::EPSILON);
        }

        #[test]
        fn disabling_wins_over_any_configured_speed() {
            let config: Config =
                toml::from_str("[animations]\nenabled = false\nspeed = 3.0\n").unwrap();
            assert!(
                config.animations.motion_scale().abs() < f32::EPSILON,
                "reduced motion must be absolute, not merely faster"
            );
        }

        #[test]
        fn an_unknown_key_is_rejected_rather_than_silently_ignored() {
            let err = toml::from_str::<Config>("[animations]\nspeeed = 2.0\n")
                .expect_err("a typo in a config key must not be swallowed");
            assert!(err.to_string().contains("speeed"), "{err}");
        }
    }

    #[test]
    fn input_section_parses_and_defaults_are_sane() {
        let toml_str = r#"
[input]
xkb-layout = "us,ua"
xkb-options = "grp:alt_shift_toggle"
repeat-rate = 40
natural-scroll = true
accel-profile = "flat"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.input.xkb_layout, "us,ua");
        assert_eq!(
            config.input.xkb_options.as_deref(),
            Some("grp:alt_shift_toggle")
        );
        assert_eq!(config.input.repeat_rate, 40);
        assert_eq!(config.input.repeat_delay, 200);
        assert!(config.input.natural_scroll);
        assert!(config.input.tap_to_click);
        assert_eq!(config.input.accel_profile, AccelProfileConfig::Flat);

        let defaults = InputConfig::default();
        assert!(defaults.tap_to_click);
        assert!(!defaults.natural_scroll);
        assert_eq!(defaults.xkb_layout, "us,ua");
        assert_eq!(
            defaults.xkb_options.as_deref(),
            Some("grp:win_space_toggle")
        );
    }

    #[test]
    fn output_sections_parse() {
        let toml_str = r#"
[[output]]
name = "eDP-1"
scale = 2.0

[[output]]
name = "DP-3"
position = [2560, 0]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.outputs.len(), 2);
        assert_eq!(config.outputs[0].name, "eDP-1");
        assert_eq!(config.outputs[0].scale, Some(2.0));
        assert_eq!(config.outputs[0].position, None);
        assert_eq!(config.outputs[1].position, Some([2560, 0]));
    }

    #[test]
    fn unknown_input_key_is_rejected() {
        let err = toml::from_str::<Config>("[input]\ntap-to-clik = true\n").unwrap_err();
        assert!(err.to_string().contains("tap-to-clik"));
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
padding = 72
label-height = 30
max-scale = 0.9
label = false
label-color = \"#111111\"
label-background = \"#222222\"
label-border = \"#555555\"
close-background = \"#0a0a0a\"
close-foreground = \"#0b0b0b\"
close-border = \"#0c0c0c\"
close-hover-background = \"#0d0d0d\"
close-hover-foreground = \"#0e0e0e\"
dim = \"#33333380\"
highlight = \"#444444\"
border = 6
corner-radius = 10.0
duration-ms = 150
";
        let config: Config = toml::from_str(toml_str).unwrap();
        let o = &config.overview;
        assert_eq!(o.gap, 40);
        assert_eq!(o.padding, 72);
        assert_eq!(o.label_height, 30);
        assert!((o.max_scale - 0.9).abs() < f64::EPSILON);
        assert!(!o.label);
        assert_eq!(o.label_color, Color::from_hex("#111111").unwrap());
        assert_eq!(o.label_background, Color::from_hex("#222222").unwrap());
        assert_eq!(o.label_border, Color::from_hex("#555555").unwrap());
        assert_eq!(o.close_background, Color::from_hex("#0a0a0a").unwrap());
        assert_eq!(o.close_foreground, Color::from_hex("#0b0b0b").unwrap());
        assert_eq!(o.close_border, Color::from_hex("#0c0c0c").unwrap());
        assert_eq!(
            o.close_hover_background,
            Color::from_hex("#0d0d0d").unwrap()
        );
        assert_eq!(
            o.close_hover_foreground,
            Color::from_hex("#0e0e0e").unwrap()
        );
        assert_eq!(o.highlight, Color::from_hex("#444444").unwrap());
        assert_eq!(o.border, 6);
        assert!((o.corner_radius - 10.0).abs() < f32::EPSILON);
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

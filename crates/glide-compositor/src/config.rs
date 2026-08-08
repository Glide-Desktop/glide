use serde::Deserialize;
use std::{fs, path::PathBuf};

use smithay::input::keyboard::{Keysym, ModifiersState, keysyms, xkb};

use crate::{GlideState, input::ResolvedKeybind};

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
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            background: Color::from_hex("#295396ff").unwrap(),
            autostart: vec!["alacritty".to_string()],
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DecorationTheme {
    pub height: u32,
    pub title_align: TitleAlign,
    pub corner_radius: f32,
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
    pub icon: Color,
    pub close_icon: Color,
    pub border_width: u32,
    pub border: Color,
    pub border_focused: Color,
    pub shadow: bool,
    pub shadow_size: u32,
    pub shadow_color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TitleAlign {
    #[default]
    Left,
    Center,
}

impl Default for DecorationTheme {
    fn default() -> Self {
        Self {
            height: 32,
            title_align: TitleAlign::Left,
            corner_radius: 8.0,
            background: Color::from_hex("#1e1e26ff").unwrap(),
            background_focused: Color::from_hex("#4c6ef5ff").unwrap(),
            maximize_button: Color([0.0, 0.0, 0.0, 0.0]),
            maximize_button_hover: Color::from_hex("#5a5a72ff").unwrap(),
            chevron_button: Color([0.0, 0.0, 0.0, 0.0]),
            chevron_button_hover: Color::from_hex("#5a5a72ff").unwrap(),
            close_button: Color([0.0, 0.0, 0.0, 0.0]),
            close_button_hover: Color::from_hex("#e0455aff").unwrap(),
            menu_background: Color::from_hex("#28282fff").unwrap(),
            menu_item_hover: Color::from_hex("#4c6ef5ff").unwrap(),
            icon: Color::from_hex("#dcdcdcff").unwrap(),
            close_icon: Color::from_hex("#ffffffff").unwrap(),
            border_width: 1,
            border: Color::from_hex("#3a3a4aff").unwrap(),
            border_focused: Color::from_hex("#4c6ef5ff").unwrap(),
            shadow: true,
            shadow_size: 16,
            shadow_color: Color::from_hex("#00000066").unwrap(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct OverviewTheme {
    pub gap: i32,
    pub label_height: i32,
    pub max_scale: f64,
    pub label: bool,
    pub label_color: Color,
    pub label_background: Color,
    pub dim: Color,
    pub highlight: Color,
    pub border: i32,
    pub shadow: bool,
    pub shadow_size: u32,
    pub shadow_color: Color,
    pub duration_ms: u64,
}

impl Default for OverviewTheme {
    fn default() -> Self {
        Self {
            gap: 32,
            label_height: 24,
            max_scale: 1.0,
            label: true,
            label_color: Color::from_hex("#ffffffff").unwrap(),
            label_background: Color::from_hex("#141419e6").unwrap(),
            dim: Color::from_hex("#0b0b0fcc").unwrap(),
            highlight: Color::from_hex("#3b5bdbff").unwrap(),
            border: 4,
            shadow: true,
            shadow_size: 20,
            shadow_color: Color::from_hex("#00000066").unwrap(),
            duration_ms: 220,
        }
    }
}

impl OverviewTheme {
    #[must_use]
    pub fn options(&self) -> glide_layout::OverviewOptions {
        glide_layout::OverviewOptions {
            gap: self.gap,
            label_height: if self.label { self.label_height } else { 0 },
            max_scale: self.max_scale,
        }
    }

    #[must_use]
    pub const fn duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.duration_ms)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color(pub [f32; 4]);

impl Color {
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

impl Color {
    #[must_use]
    pub fn to_buffer_bytes(self) -> [u8; 4] {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [
            byte(self.0[2]),
            byte(self.0[1]),
            byte(self.0[0]),
            byte(self.0[3]),
        ]
    }

    #[must_use]
    pub fn darken(self, factor: f32) -> Self {
        let [r, g, b, a] = self.0;
        Self([
            (r * factor).clamp(0.0, 1.0),
            (g * factor).clamp(0.0, 1.0),
            (b * factor).clamp(0.0, 1.0),
            a,
        ])
    }

    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mut out = [0.0; 4];
        for (o, (&from, &to)) in out.iter_mut().zip(self.0.iter().zip(other.0.iter())) {
            *o = from + (to - from) * t;
        }
        Self(out)
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

        #[allow(clippy::cast_possible_truncation)]
        let font_px = (f64::from(self.decorations.height).max(14.0) * 0.45) as f32;
        crate::paint::init_or_replace(font_px.clamp(10.0, 18.0));

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
    fn to_buffer_bytes_is_b_g_r_a_not_a_r_g_b() {
        let red = Color::from_hex("#ff0000ff").unwrap().to_buffer_bytes();
        assert_eq!(red, [0, 0, 255, 255], "opaque red must store R in byte 2");

        let blue = Color::from_hex("#0000ffff").unwrap().to_buffer_bytes();
        assert_eq!(blue, [255, 0, 0, 255], "opaque blue must store B in byte 0");

        let translucent_green = Color::from_hex("#00ff0080").unwrap().to_buffer_bytes();
        assert_eq!(translucent_green[0], 0, "B");
        assert_eq!(translucent_green[1], 255, "G");
        assert_eq!(translucent_green[2], 0, "R");
        assert!(
            (i16::from(translucent_green[3]) - 0x80).abs() <= 1,
            "A byte 3, ~{:#x}, got {:#x}",
            0x80,
            translucent_green[3]
        );
    }

    #[test]
    fn general_config_defaults_to_the_previous_hardcoded_fill_color() {
        assert_eq!(
            GeneralConfig::default().background,
            Color::from_hex("#295396ff").unwrap()
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
chevron-button = \"#777777\"
chevron-button-hover = \"#888888\"
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
    fn decoration_theme_defaults_title_align_left() {
        assert_eq!(DecorationTheme::default().title_align, TitleAlign::Left);
    }

    #[test]
    fn decoration_theme_defaults_border_width_to_one() {
        assert_eq!(DecorationTheme::default().border_width, 1);
    }

    #[test]
    fn decoration_theme_defaults_shadow_enabled() {
        let d = DecorationTheme::default();
        assert!(d.shadow);
        assert!(d.shadow_size > 0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn decoration_theme_buttons_are_transparent_at_rest() {
        let d = DecorationTheme::default();
        assert_eq!(d.maximize_button.0[3], 0.0, "maximize must be transparent");
        assert_eq!(d.chevron_button.0[3], 0.0, "chevron must be transparent");
        assert_eq!(d.close_button.0[3], 0.0, "close must be transparent");
    }

    #[test]
    fn decoration_theme_close_hover_is_the_odd_one_out() {
        let d = DecorationTheme::default();
        assert_eq!(
            d.maximize_button_hover, d.chevron_button_hover,
            "maximize and chevron should read as one neutral group"
        );
        let [r, g, b, _] = d.close_button_hover.0;
        assert!(
            r > g && r > b,
            "close hover must be red-dominant, got {:?}",
            d.close_button_hover.0
        );
    }

    #[test]
    fn decoration_theme_defaults_a_positive_corner_radius() {
        assert!(DecorationTheme::default().corner_radius > 0.0);
    }

    #[test]
    fn overview_theme_defaults_shadow_enabled() {
        let o = OverviewTheme::default();
        assert!(o.shadow);
        assert!(o.shadow_size > 0);
        assert!(o.border > 0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn overview_theme_label_background_differs_from_the_scrim() {
        let o = OverviewTheme::default();
        assert_ne!(o.label_background.0, o.dim.0);
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
    fn darken_scales_rgb_and_keeps_alpha() {
        let c = Color([0.8, 0.4, 0.2, 0.5]).darken(0.5);
        assert!((c.0[0] - 0.4).abs() < 0.001);
        assert!((c.0[1] - 0.2).abs() < 0.001);
        assert!((c.0[2] - 0.1).abs() < 0.001);
        assert!((c.0[3] - 0.5).abs() < 0.001, "alpha must be untouched");
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn lerp_at_zero_and_one_returns_the_endpoints() {
        let from = Color([0.0, 0.0, 0.0, 0.0]);
        let to = Color([1.0, 0.5, 0.25, 1.0]);
        assert_eq!(from.lerp(to, 0.0).0, from.0);
        assert_eq!(from.lerp(to, 1.0).0, to.0);
    }

    #[test]
    fn lerp_at_half_averages_every_channel_including_alpha() {
        let from = Color([0.0, 0.0, 0.0, 0.0]);
        let to = Color([1.0, 0.4, 0.8, 1.0]);
        let mid = from.lerp(to, 0.5);
        assert!((mid.0[0] - 0.5).abs() < 0.001);
        assert!((mid.0[1] - 0.2).abs() < 0.001);
        assert!((mid.0[2] - 0.4).abs() < 0.001);
        assert!((mid.0[3] - 0.5).abs() < 0.001, "alpha must interpolate too");
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn lerp_clamps_an_out_of_range_t() {
        let from = Color([0.0, 0.0, 0.0, 0.0]);
        let to = Color([1.0, 1.0, 1.0, 1.0]);
        assert_eq!(from.lerp(to, -1.0).0, from.0);
        assert_eq!(from.lerp(to, 2.0).0, to.0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn darken_clamps_at_zero() {
        let c = Color([0.2, 0.0, 1.0, 1.0]).darken(-1.0);
        assert_eq!(c.0[0], 0.0);
        assert_eq!(c.0[2], 0.0);
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

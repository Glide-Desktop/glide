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
    pub general: GeneralConfig,
    pub workspaces: WorkspaceConfig,
    pub keybinds: Vec<KeybindConfig>,
    pub decorations: DecorationTheme,
    pub overview: OverviewTheme,
}

/// Settings that don't belong to any one subsystem.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct GeneralConfig {
    /// Colour the output is cleared to before anything is drawn — visible
    /// wherever no window, layer-shell surface, or overview scrim covers
    /// it. Was a compiled-in constant; still the same steel-blue by
    /// default, just configurable now.
    pub background: Color,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            background: Color::from_hex("#295396ff").unwrap(),
        }
    }
}

/// A window decoration's color scheme and size.
///
/// Button *cluster order* is deliberately not configurable — chevron,
/// maximize, close, always in that order at the frame's right edge — so
/// every window in a session looks and behaves the same regardless of what
/// an individual user put in their config. Colors, the titlebar's height,
/// and the title's alignment are settable.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DecorationTheme {
    /// Titlebar height in logical pixels.
    pub height: u32,
    /// Where the title text sits within the space left of the button
    /// cluster.
    pub title_align: TitleAlign,
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
    /// Glyph colour for the maximize/restore and chevron icons.
    pub icon: Color,
    /// Glyph colour for the close (×) icon — separately settable since it
    /// is meant to stand out a little more than the other two.
    pub close_icon: Color,
    /// Width, in logical pixels, of the outline drawn just outside the
    /// window's frame — the only focus indicator a window has beyond the
    /// titlebar's own colour, especially relevant for a future
    /// client-preferred undecorated window (see the module docs on
    /// `xdg-decoration`). `0` disables it.
    ///
    /// Capped in practice at [`crate::decoration::RESIZE_BORDER`]: the
    /// border is drawn in the same invisible margin the resize band
    /// already reserves outside the frame, so it never needs its own
    /// hit-testing or bounding-box changes — a wider value would simply
    /// draw outside the region damage tracking is told to cover.
    pub border_width: u32,
    pub border: Color,
    pub border_focused: Color,
}

/// Where the titlebar's title text sits within the space left of the
/// button cluster.
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
            // Titlebar background: charcoal when unfocused, electric
            // indigo when focused — high contrast against both light and
            // dark window content.
            background: Color::from_hex("#1e1e26ff").unwrap(),
            background_focused: Color::from_hex("#4c6ef5ff").unwrap(),
            // Maximize: subtle slate, noticeably lighter on hover.
            maximize_button: Color::from_hex("#3a3a4aff").unwrap(),
            maximize_button_hover: Color::from_hex("#5a5a72ff").unwrap(),
            // Chevron: same family as maximize so the two left buttons
            // form one visual group, distinct from the destructive close.
            chevron_button: Color::from_hex("#3a3a4aff").unwrap(),
            chevron_button_hover: Color::from_hex("#5a5a72ff").unwrap(),
            // Close: clearly destructive — warm red, brightens on hover.
            close_button: Color::from_hex("#c0404aff").unwrap(),
            close_button_hover: Color::from_hex("#e05560ff").unwrap(),
            // Dropdown: slightly lighter than the bar so it reads as a
            // separate panel; hover rows use the focused accent.
            menu_background: Color::from_hex("#28282fff").unwrap(),
            menu_item_hover: Color::from_hex("#4c6ef5ff").unwrap(),
            // Icon glyphs: an off-white that reads clearly against every
            // button background above without being as stark as pure
            // white; close gets pure white so it stands out as the odd
            // (destructive) one out.
            icon: Color::from_hex("#dcdcdcff").unwrap(),
            close_icon: Color::from_hex("#ffffffff").unwrap(),
            // A thin, mostly-unnoticed outline unfocused; the same accent
            // as the focused titlebar when focused, so the whole frame —
            // not just the bar — reads as "this one".
            border_width: 1,
            border: Color::from_hex("#3a3a4aff").unwrap(),
            border_focused: Color::from_hex("#4c6ef5ff").unwrap(),
        }
    }
}

/// Look and feel of the overview.
///
/// Sizing here is in logical pixels and applies before output scaling, the
/// same as the decoration theme.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct OverviewTheme {
    /// Space between previews, and between rows.
    pub gap: i32,
    /// Height of the strip under each preview reserved for its title.
    pub label_height: i32,
    /// Upper bound on how large a preview may be drawn relative to the
    /// window. Above 1.0 previews would be scaled *up* and look blurry.
    pub max_scale: f64,
    /// Whether to draw window titles under previews.
    pub label: bool,
    pub label_color: Color,
    /// Full-screen dim drawn behind the previews. Only the alpha channel
    /// is animated; the colour itself is used as-is.
    pub dim: Color,
    /// Highlight drawn behind the hovered/selected preview.
    pub highlight: Color,
    /// Thickness of that highlight around the preview.
    pub border: i32,
    /// Open/close animation length. `0` disables the animation entirely,
    /// which is also the escape hatch if it ever feels wrong on slow
    /// hardware.
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
            dim: Color::from_hex("#0b0b0fcc").unwrap(),
            highlight: Color::from_hex("#3b5bdbff").unwrap(),
            border: 3,
            duration_ms: 220,
        }
    }
}

impl OverviewTheme {
    /// The subset of the theme the pure layout engine needs.
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

impl Color {
    /// The colour as straight-alpha `[B, G, R, A]` bytes — the single
    /// canonical conversion from config colour to pixel bytes, used
    /// everywhere `paint.rs` and `decoration.rs` write into a
    /// [`smithay::backend::allocator::Fourcc::Argb8888`] buffer.
    ///
    /// `B, G, R, A` (byte order, low address first) is what `Argb8888`
    /// actually is in memory on a little-endian machine — *not* the
    /// `A, R, G, B` order the format's name suggests when read as a 32-bit
    /// word. Getting this backwards silently swaps red and blue for every
    /// non-grey colour (white/black/grey are symmetric under the swap,
    /// which is why a wrong version of this method can ship unnoticed
    /// against the default theme and only misrender a *custom* one).
    ///
    /// Straight, not premultiplied: callers writing into a buffer that
    /// expects premultiplied alpha (every [`crate::paint::Painter`] method
    /// does) are responsible for premultiplying at the point they draw,
    /// not here — this method only does the channel-order conversion.
    #[must_use]
    pub fn to_buffer_bytes(self) -> [u8; 4] {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [
            byte(self.0[2]), // B
            byte(self.0[1]), // G
            byte(self.0[0]), // R
            byte(self.0[3]), // A
        ]
    }

    /// Scales the RGB channels by `factor` (alpha untouched) — used to
    /// derive a "pressed" button shade from its hover colour rather than
    /// requiring a dedicated theme key per button per interaction state.
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
            general: GeneralConfig::default(),
            workspaces: WorkspaceConfig::default(),
            keybinds: Self::default_keybinds(),
            decorations: DecorationTheme::default(),
            overview: OverviewTheme::default(),
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
        state.overview_theme = self.overview.clone();
        state.background_color = self.general.background;

        // Rebuilds the shared `Painter` (font + glyph cache) at a size
        // derived from the titlebar height — the same heuristic that used
        // to live in `window.rs`, run once here instead of racing whichever
        // window happened to render first. See `paint::init_or_replace`'s
        // docs: this call is what makes a changed titlebar height actually
        // affect the font size, on both first load and `glidectl reload`.
        //
        // Titlebar heights are tens of pixels; the product is nowhere near
        // f32's precision limits.
        #[allow(clippy::cast_possible_truncation)]
        let font_px = (f64::from(self.decorations.height).max(14.0) * 0.45) as f32;
        crate::paint::init_or_replace(font_px.clamp(10.0, 18.0));

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

    // Regression test for the R/B channel swap: a symmetric colour (grey,
    // black, white) can't distinguish [B,G,R,A] from [A,R,G,B], which is
    // exactly how the bug this guards against shipped unnoticed — pure red
    // and pure blue are the minimal pair that tells the two conventions
    // apart.
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
        // #295396 was a `const FILL_COLOR` in backend/tty.rs before it
        // became configurable; the default must not silently change the
        // colour every existing session already renders.
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
    fn decoration_theme_parses_from_toml() {
        let toml_str = "\
[decorations]
height = 40
title-align = \"center\"
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
";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.decorations.height, 40);
        assert_eq!(config.decorations.title_align, TitleAlign::Center);
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
    fn darken_scales_rgb_and_keeps_alpha() {
        let c = Color([0.8, 0.4, 0.2, 0.5]).darken(0.5);
        assert!((c.0[0] - 0.4).abs() < 0.001);
        assert!((c.0[1] - 0.2).abs() < 0.001);
        assert!((c.0[2] - 0.1).abs() < 0.001);
        assert!((c.0[3] - 0.5).abs() < 0.001, "alpha must be untouched");
    }

    // `clamp(0.0, 1.0)` lands exactly on 0.0 for any negative input, no
    // accumulated rounding involved, so exact equality is correct here —
    // see `color_parses_rrggbb_as_opaque`'s comment for the same reasoning.
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

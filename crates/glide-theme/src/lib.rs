use serde::Deserialize;

pub mod component;
pub mod data;
pub mod elevation;
pub mod font;
pub mod icons;
pub mod motion;
mod palette;
pub mod plates;
pub mod shape;
pub mod spacing;
mod tokens;
mod typography;
mod wallpaper;

pub use palette::Accent;
pub use tokens::{Appearance, Tokens};
pub use typography::TextStyle;
pub use wallpaper::WallpaperFit;

pub mod export;
pub mod kvantum;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color(pub [f32; 4]);

impl Color {
    #[must_use]
    #[allow(clippy::many_single_char_names)]
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.strip_prefix('#')?;
        let byte = |range: std::ops::Range<usize>| u8::from_str_radix(s.get(range)?, 16).ok();

        let (r, g, b, a) = match s.len() {
            6 => (byte(0..2)?, byte(2..4)?, byte(4..6)?, 255),
            8 => (byte(0..2)?, byte(2..4)?, byte(4..6)?, byte(6..8)?),
            _ => return None,
        };

        Some(Self([f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, f32::from(a) / 255.0]))
    }
}

impl Color {
    #[must_use]
    pub fn to_buffer_bytes(self) -> [u8; 4] {
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [byte(self.0[2]), byte(self.0[1]), byte(self.0[0]), byte(self.0[3])]
    }

    #[must_use]
    pub fn premultiplied(self, opacity: f32) -> [f32; 4] {
        let [r, g, b, a] = self.0;
        let alpha = (a * opacity).clamp(0.0, 1.0);
        [r * alpha, g * alpha, b * alpha, alpha]
    }

    #[must_use]
    pub fn darken(self, factor: f32) -> Self {
        let [r, g, b, a] = self.0;
        Self([(r * factor).clamp(0.0, 1.0), (g * factor).clamp(0.0, 1.0), (b * factor).clamp(0.0, 1.0), a])
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

    #[must_use]
    pub fn lerp_premultiplied(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let alpha = self.0[3] + (other.0[3] - self.0[3]) * t;
        if alpha <= f32::EPSILON {
            return Self([0.0; 4]);
        }
        let mut out = [0.0, 0.0, 0.0, alpha];
        for (index, channel) in out.iter_mut().take(3).enumerate() {
            let from = self.0[index] * self.0[3];
            let to = other.0[index] * other.0[3];
            *channel = (from + (to - from) * t) / alpha;
        }
        Self(out)
    }

    #[must_use]
    pub fn state_layer(self, current: Self, opacity: f32) -> Self {
        let p = opacity.clamp(0.0, 1.0) * current.0[3];
        let base_a = self.0[3];
        let out_a = p + base_a * (1.0 - p);
        if out_a <= 0.0 {
            return Self([0.0; 4]);
        }
        let mut out = [0.0, 0.0, 0.0, out_a];
        for (channel, value) in out.iter_mut().take(3).enumerate() {
            *value = (current.0[channel] * p + self.0[channel] * base_a * (1.0 - p)) / out_a;
        }
        Self(out)
    }

    #[must_use]
    pub fn with_alpha(self, alpha: f32) -> Self {
        let [r, g, b, _] = self.0;
        Self([r, g, b, alpha.clamp(0.0, 1.0)])
    }

    #[must_use]
    pub fn from_rgb8(r: u8, g: u8, b: u8, alpha: f32) -> Self {
        Self([f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, alpha.clamp(0.0, 1.0)])
    }
}

impl<'de> serde::Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_hex(&s).ok_or_else(|| {
            serde::de::Error::custom(format!("invalid color {s:?}: expected \"#rrggbb\" or \"#rrggbbaa\""))
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TitleAlign {
    #[default]
    Left,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TitlebarAction {
    #[default]
    Maximize,
    Minimize,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ButtonLayout {
    #[default]
    MaximizeClose,
    CloseMaximize,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DecorationTheme {
    pub height: u32,
    pub title_align: TitleAlign,
    pub double_click: TitlebarAction,
    pub button_layout: ButtonLayout,
    pub corner_radius: f32,
    pub background: Color,
    pub background_focused: Color,
    pub title: Color,
    pub title_focused: Color,
    pub close_button: Color,
    pub close_button_hover: Color,
    pub maximize_button: Color,
    pub maximize_button_hover: Color,
    pub menu_background: Color,
    pub menu_border: Color,
    pub menu_item_hover: Color,
    pub menu_text: Color,
    pub menu_text_danger: Color,
    pub menu_accel_text: Color,
    pub icon: Color,
    pub close_icon: Color,
    pub border_width: u32,
    pub border: Color,
    pub border_focused: Color,
    pub shadow: bool,
    pub shadow_size: u32,
    pub shadow_size_focused: u32,
    pub shadow_color: Color,
    pub shadow_color_focused: Color,
    #[serde(skip)]
    pub appearance: Appearance,
}

macro_rules! repaint_colors {
    ($this:ident, $from:expr, $to:expr; $($field:ident),+ $(,)?) => {{
        let (from, to) = ($from, $to);
        $(
            if $this.$field == from.$field {
                $this.$field = to.$field;
            }
        )+
    }};
}

fn default_tokens() -> Tokens {
    Tokens::new(Appearance::Dark, Accent::Blue)
}

fn window_ring(t: &Tokens, level: elevation::Level) -> Color {
    elevation::layers(level, t.appearance).first().map_or(t.line, |ring| ring.color)
}

impl Default for DecorationTheme {
    fn default() -> Self {
        Self::from_tokens(&default_tokens())
    }
}

impl DecorationTheme {
    pub fn repaint(&mut self, t: &Tokens) {
        repaint_colors!(self, Self::default(), Self::from_tokens(t);
            background, background_focused, title, title_focused, close_button,
            close_button_hover, maximize_button, maximize_button_hover, menu_background,
            menu_border, menu_item_hover, menu_text, menu_text_danger, menu_accel_text, icon,
            close_icon, border, border_focused, shadow_color, shadow_color_focused);
        self.appearance = t.appearance;
    }

    #[must_use]
    pub fn from_tokens(t: &Tokens) -> Self {
        Self {
            height: component::WINDOW_HEADER as u32,
            title_align: TitleAlign::Left,
            double_click: TitlebarAction::Maximize,
            button_layout: ButtonLayout::MaximizeClose,
            corner_radius: shape::R3,
            background: t.bar_bg_idle,
            background_focused: t.bar_bg,
            title: t.fg_3,
            title_focused: t.fg_1,
            maximize_button: t.fill_1,
            maximize_button_hover: t.acc_bg,
            close_button: t.fill_1,
            close_button_hover: t.danger,
            menu_background: t.popover_bg,
            menu_border: t.line,
            menu_item_hover: t.fill_2,
            menu_text: t.fg_1,
            menu_text_danger: t.danger,
            menu_accel_text: t.fg_3,
            icon: t.fg_2,
            close_icon: t.fg_2,
            border_width: 1,
            border: window_ring(t, elevation::Level::ElWinIdle),
            border_focused: window_ring(t, elevation::Level::ElWin),
            shadow: true,
            shadow_size: 32,
            shadow_size_focused: 68,
            shadow_color: Color::from_hex("#00000066").unwrap(),
            shadow_color_focused: Color::from_hex("#00000099").unwrap(),
            appearance: t.appearance,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DockTheme {
    pub enabled: bool,
    pub icon_theme: Option<String>,
    pub bottom_margin: i32,
    pub item_size: i32,
    pub icon_size: i32,
    pub item_gap: i32,
    pub padding_x: i32,
    pub padding_y: i32,
    pub radius: f32,
    pub item_radius: f32,
    pub background: Color,
    pub border: Color,
    pub edge_highlight: Color,
    pub separator: Color,
    pub icon: Color,
    pub running_color: Color,
    pub focused_color: Color,
    pub attention_color: Color,
    pub badge_text: Color,
    pub hover_layer: Color,
    pub well_focused: Color,
    pub launcher_background: Color,
    pub launcher_background_hover: Color,
    pub launcher_glyph: Color,
    pub launcher_glyph_hover: Color,
    pub fallback_background: Color,
    pub fallback_text: Color,
    pub hover_lift: i32,
    pub hover_scale: f64,
    pub menu_background: Color,
    pub menu_border: Color,
    pub menu_text: Color,
    pub menu_text_muted: Color,
    pub menu_text_danger: Color,
    pub menu_item_hover: Color,
    #[serde(skip)]
    pub appearance: Appearance,
}

impl Default for DockTheme {
    fn default() -> Self {
        Self::from_tokens(&default_tokens())
    }
}

impl DockTheme {
    pub fn repaint(&mut self, t: &Tokens) {
        repaint_colors!(self, Self::default(), Self::from_tokens(t);
            background, border, edge_highlight, separator, icon, running_color, focused_color,
            attention_color, badge_text, hover_layer, well_focused, launcher_background,
            launcher_background_hover, launcher_glyph, launcher_glyph_hover,
            fallback_background, fallback_text, menu_background, menu_border, menu_text,
            menu_text_muted, menu_text_danger, menu_item_hover);
        self.appearance = t.appearance;
    }

    #[must_use]
    pub fn from_tokens(t: &Tokens) -> Self {
        let bezel_alpha = match t.appearance {
            Appearance::Light => 0.55,
            Appearance::Dark => 0.12,
        };
        Self {
            enabled: true,
            icon_theme: None,
            bottom_margin: spacing::SP3,
            item_size: 56,
            icon_size: 44,
            item_gap: spacing::SP1,
            padding_x: spacing::SP2,
            padding_y: spacing::SP2,
            radius: shape::R4,
            item_radius: shape::R3,
            background: t.shell_bg,
            border: t.shell_line,
            edge_highlight: Color::from_rgb8(255, 255, 255, bezel_alpha),
            separator: t.line_2,
            icon: t.fg_2,
            running_color: t.fg_1,
            focused_color: t.acc_bg,
            attention_color: t.danger,
            badge_text: t.acc_fg,
            hover_layer: t.shell_fg.with_alpha(motion::ST_HOVER),
            well_focused: t.fill_1,
            launcher_background: t.fill_1,
            launcher_background_hover: t.fill_2,
            launcher_glyph: t.fg_2,
            launcher_glyph_hover: t.fg_1,
            fallback_background: t.fill_2,
            fallback_text: t.fg_2,
            hover_lift: 4,
            hover_scale: 1.06,
            menu_background: t.popover_bg,
            menu_border: t.line,
            menu_text: t.fg_1,
            menu_text_muted: t.fg_3,
            menu_text_danger: t.danger,
            menu_item_hover: t.fill_2,
            appearance: t.appearance,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct OverviewTheme {
    pub gap: i32,
    pub padding: i32,
    pub label_height: i32,
    pub max_scale: f64,
    pub label: bool,
    pub hot_corner: bool,
    pub label_color: Color,
    pub label_background: Color,
    pub label_border: Color,
    pub close_background: Color,
    pub close_foreground: Color,
    pub close_border: Color,
    pub close_hover_background: Color,
    pub close_hover_foreground: Color,
    pub dim: Color,
    pub highlight: Color,
    pub strip_from: Color,
    pub strip_to: Color,
    pub strip_pick: Color,
    pub strip_ring: Color,
    pub strip_drop: Color,
    pub strip_add: Color,
    pub strip_add_border: Color,
    pub strip_caption: Color,
    pub strip_caption_active: Color,
    pub border: i32,
    pub corner_radius: f32,
    pub duration_ms: u64,
    #[serde(skip)]
    pub appearance: Appearance,
}

impl Default for OverviewTheme {
    fn default() -> Self {
        Self::from_tokens(&default_tokens())
    }
}

impl OverviewTheme {
    pub fn repaint(&mut self, t: &Tokens) {
        repaint_colors!(self, Self::default(), Self::from_tokens(t);
            label_color, label_background, label_border, close_background, close_foreground,
            close_border, close_hover_background, close_hover_foreground, dim, highlight,
            strip_from, strip_to, strip_pick, strip_ring, strip_drop, strip_add, strip_add_border,
            strip_caption, strip_caption_active);
        self.appearance = t.appearance;
    }

    #[must_use]
    pub fn from_tokens(t: &Tokens) -> Self {
        Self {
            gap: 30,
            padding: spacing::SP8,
            label_height: 28,
            max_scale: 0.92,
            label: true,
            hot_corner: false,
            label_color: t.fg_1,
            label_background: t.popover_bg,
            label_border: t.line,
            close_background: t.popover_bg,
            close_foreground: t.fg_1,
            close_border: t.line,
            close_hover_background: t.danger,
            close_hover_foreground: t.view_bg,
            dim: t.shell_bg,
            highlight: t.acc_bg,
            strip_from: t.popover_bg,
            strip_to: t.fill_2,
            strip_pick: t.fg_1.with_alpha(0.45),
            strip_ring: t.acc_text,
            strip_drop: t.acc_soft,
            strip_add: t.fill_2,
            strip_add_border: t.line_2,
            strip_caption: t.fg_3,
            strip_caption_active: t.fg_1,
            border: 3,
            corner_radius: shape::R3,
            duration_ms: u64::try_from(motion::T4.as_millis()).unwrap_or(u64::MAX),
            appearance: t.appearance,
        }
    }

    #[must_use]
    pub fn duration(&self) -> std::time::Duration {
        motion::scaled(self.base_duration())
    }

    #[must_use]
    pub const fn base_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.duration_ms)
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        Revision::new()
            .i32(self.gap)
            .i32(self.padding)
            .i32(self.label_height)
            .f64(self.max_scale)
            .bool(self.label)
            .color(self.label_color)
            .color(self.label_background)
            .color(self.label_border)
            .color(self.close_background)
            .color(self.close_foreground)
            .color(self.close_border)
            .color(self.close_hover_background)
            .color(self.close_hover_foreground)
            .color(self.dim)
            .color(self.highlight)
            .color(self.strip_from)
            .color(self.strip_to)
            .color(self.strip_pick)
            .color(self.strip_ring)
            .color(self.strip_drop)
            .color(self.strip_add)
            .color(self.strip_add_border)
            .color(self.strip_caption)
            .color(self.strip_caption_active)
            .i32(self.border)
            .f32(self.corner_radius)
            .bool(self.appearance == Appearance::Dark)
            .finish()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Revision(u64);

impl Default for Revision {
    fn default() -> Self {
        Self::new()
    }
}

impl Revision {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    #[must_use]
    pub const fn new() -> Self {
        Self(Self::OFFSET)
    }

    #[must_use]
    pub const fn u64(mut self, value: u64) -> Self {
        let mut i = 0;
        while i < 8 {
            let byte = (value >> (i * 8)) as u8;
            self.0 ^= byte as u64;
            self.0 = self.0.wrapping_mul(Self::PRIME);
            i += 1;
        }
        self
    }

    #[must_use]
    pub const fn i32(self, value: i32) -> Self {
        self.u64(value.cast_unsigned() as u64)
    }

    #[must_use]
    pub fn f32(self, value: f32) -> Self {
        self.u64(u64::from(value.to_bits()))
    }

    #[must_use]
    pub fn f64(self, value: f64) -> Self {
        self.u64(value.to_bits())
    }

    #[must_use]
    pub const fn bool(self, value: bool) -> Self {
        self.u64(value as u64)
    }

    #[must_use]
    pub fn color(self, value: Color) -> Self {
        value.0.into_iter().fold(self, Self::f32)
    }

    #[must_use]
    pub fn str(self, value: &str) -> Self {
        value.bytes().fold(self, |this, byte| this.u64(u64::from(byte)))
    }

    #[must_use]
    pub const fn finish(self) -> u64 {
        self.0
    }
}

impl DockTheme {
    #[must_use]
    pub fn revision(&self) -> u64 {
        Revision::new()
            .bool(self.enabled)
            .str(self.icon_theme.as_deref().unwrap_or(""))
            .i32(self.bottom_margin)
            .i32(self.item_size)
            .i32(self.icon_size)
            .i32(self.item_gap)
            .i32(self.padding_x)
            .i32(self.padding_y)
            .f32(self.radius)
            .f32(self.item_radius)
            .color(self.background)
            .color(self.border)
            .color(self.edge_highlight)
            .color(self.separator)
            .color(self.icon)
            .color(self.running_color)
            .color(self.focused_color)
            .color(self.attention_color)
            .color(self.badge_text)
            .color(self.hover_layer)
            .color(self.well_focused)
            .color(self.launcher_background)
            .color(self.launcher_background_hover)
            .color(self.launcher_glyph)
            .color(self.launcher_glyph_hover)
            .color(self.fallback_background)
            .color(self.fallback_text)
            .i32(self.hover_lift)
            .f64(self.hover_scale)
            .color(self.menu_background)
            .color(self.menu_border)
            .color(self.menu_text)
            .color(self.menu_text_muted)
            .color(self.menu_text_danger)
            .color(self.menu_item_hover)
            .bool(self.appearance == Appearance::Dark)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse_blend_and_pack_as_bgra() {
        assert_eq!(Color::from_hex("#ff0000").unwrap().0, [1.0, 0.0, 0.0, 1.0]);
        let green = Color::from_hex("#00ff0080").unwrap();
        assert_eq!(green.to_buffer_bytes(), [0, 255, 0, 128]);
        for bad in ["ff0000", "#ff00", "#gggggg"] {
            assert!(Color::from_hex(bad).is_none(), "{bad}");
        }
        let (from, to) = (Color([0.0; 4]), Color([1.0, 0.4, 0.8, 1.0]));
        assert_eq!(from.lerp(to, -1.0), from);
        assert_eq!(from.lerp(to, 2.0), to);
        assert_eq!(from.lerp(to, 0.5), Color([0.5, 0.2, 0.4, 0.5]));
        assert_eq!(Color([0.8, 0.4, 0.2, 0.5]).darken(0.5), Color([0.4, 0.2, 0.1, 0.5]));
        assert_eq!(Color([0.2, 0.0, 1.0, 1.0]).darken(-1.0).0[2], 0.0);
    }

    #[test]
    fn a_state_layer_tints_toward_the_current_colour_not_white() {
        let light = Tokens::new(Appearance::Light, Accent::Blue);
        let pressed = light.bar_bg.state_layer(light.state, motion::ST_PRESS);
        let luma = |c: Color| c.0[0] + c.0[1] + c.0[2];
        assert!(luma(pressed) < luma(light.bar_bg));
        let clear = Color([0.0; 4]).state_layer(light.state, motion::ST_HOVER);
        assert!((clear.0[3] - motion::ST_HOVER).abs() < 1e-6);
    }

    #[test]
    fn defaults_follow_the_spec_geometry() {
        let (d, o, dock) = (DecorationTheme::default(), OverviewTheme::default(), DockTheme::default());
        assert_eq!(d.corner_radius, shape::R3);
        assert_eq!(o.corner_radius, d.corner_radius);
        assert!(o.padding >= o.gap);
        assert_eq!(o.base_duration(), motion::T4);
        assert_ne!(o.label_background, o.dim);
        assert_eq!(
            o.dim,
            default_tokens().shell_bg,
            "the overview lays the launcher's glass over the blurred wallpaper"
        );
        assert_eq!((dock.item_size - dock.icon_size) % 2, 0);
        for appearance in [Appearance::Light, Appearance::Dark] {
            let d = DecorationTheme::from_tokens(&Tokens::new(appearance, Accent::Blue));
            let ring = |level| elevation::layers(level, appearance)[0].color;
            assert_eq!(d.border, ring(elevation::Level::ElWinIdle));
            assert_eq!(d.border_focused, ring(elevation::Level::ElWin));
        }
    }

    #[test]
    fn repaint_follows_the_palette_but_keeps_user_overrides() {
        let custom = Color::from_hex("#123456").unwrap();
        let mut d = DecorationTheme { title_focused: custom, ..DecorationTheme::default() };
        let light = Tokens::new(Appearance::Light, Accent::Green);
        d.repaint(&light);
        assert_eq!(d.title_focused, custom);
        assert_eq!(d.background_focused, light.bar_bg);

        let mut dock = DockTheme::default();
        dock.repaint(&light);
        assert_eq!(dock, DockTheme::from_tokens(&light));

        let mut overview = OverviewTheme::default();
        overview.repaint(&light);
        assert_eq!(overview, OverviewTheme::from_tokens(&light));
    }

    #[test]
    fn decoration_theme_reads_kebab_case_toml() {
        let theme: DecorationTheme = toml::from_str(
            "height = 40\ntitle-align = \"center\"\nmenu-text-danger = \"#b2b2b2\"\nshadow = false\nshadow-color-focused = \"#000000bb\"\n",
        )
        .unwrap();
        assert_eq!(theme.height, 40);
        assert_eq!(theme.title_align, TitleAlign::Center);
        assert_eq!(theme.menu_text_danger, Color::from_hex("#b2b2b2").unwrap());
        assert!(!theme.shadow);
        assert_eq!(theme.shadow_color_focused, Color::from_hex("#000000bb").unwrap());
        assert!(toml::from_str::<DecorationTheme>("bogus = 1").is_err());
    }

    #[test]
    fn overview_revision_covers_the_workspace_strip_colours() {
        let base = OverviewTheme::default();
        let other = Color::from_hex("#123456").unwrap();
        let variants = [
            OverviewTheme { strip_from: other, ..base },
            OverviewTheme { strip_to: other, ..base },
            OverviewTheme { strip_pick: other, ..base },
        ];
        for variant in variants {
            assert_ne!(variant.revision(), base.revision());
        }
    }
}

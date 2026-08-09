use serde::Deserialize;

pub mod elevation;
pub mod motion;
mod palette;
pub mod shape;
pub mod spacing;
mod tokens;
mod typography;

pub use palette::Accent;
pub use tokens::{Appearance, Tokens};
pub use typography::TextStyle;

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

    #[must_use]
    pub fn with_alpha(self, alpha: f32) -> Self {
        let [r, g, b, _] = self.0;
        Self([r, g, b, alpha.clamp(0.0, 1.0)])
    }

    #[must_use]
    pub fn from_rgb8(r: u8, g: u8, b: u8, alpha: f32) -> Self {
        Self([
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            alpha.clamp(0.0, 1.0),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TitleAlign {
    #[default]
    Left,
    Center,
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
    pub menu_background: Color,
    pub menu_item_hover: Color,
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
}

impl Default for DecorationTheme {
    fn default() -> Self {
        let t = Tokens::new(Appearance::Dark, Accent::Blue);
        Self {
            height: 48,
            title_align: TitleAlign::Left,
            corner_radius: shape::R3,
            background: t.bar_bg_idle,
            background_focused: t.bar_bg,
            maximize_button: t.fill_1,
            maximize_button_hover: t.acc_bg,
            close_button: t.fill_1,
            close_button_hover: t.danger,
            menu_background: t.popover_bg,
            menu_item_hover: t.fill_1,
            icon: t.fg_2,
            close_icon: t.fg_2,
            border_width: 1,
            border: t.line,
            border_focused: t.line_2,
            shadow: true,
            shadow_size: 32,
            shadow_size_focused: 68,
            shadow_color: Color::from_hex("#00000066").unwrap(),
            shadow_color_focused: Color::from_hex("#00000099").unwrap(),
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
        let t = Tokens::new(Appearance::Dark, Accent::Blue);
        Self {
            gap: 48,
            label_height: 28,
            max_scale: 1.0,
            label: true,
            label_color: t.fg_1,
            label_background: t.popover_bg,
            dim: Color::from_hex("#100f0e66").unwrap(),
            highlight: t.acc_bg,
            border: 3,
            shadow: true,
            shadow_size: 44,
            shadow_color: Color::from_hex("#0000004d").unwrap(),
            duration_ms: u64::try_from(motion::T4.as_millis()).unwrap_or(u64::MAX),
        }
    }
}

impl OverviewTheme {
    #[must_use]
    pub const fn duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.duration_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn decoration_theme_buttons_are_subtle_at_rest() {
        let d = DecorationTheme::default();
        assert!(
            (d.maximize_button.0[3] - 0.07).abs() < 0.01,
            "maximize must be subtle"
        );
        assert!(
            (d.close_button.0[3] - 0.07).abs() < 0.01,
            "close must be subtle"
        );
    }

    #[test]
    fn decoration_theme_close_hover_is_the_odd_one_out() {
        let d = DecorationTheme::default();
        let [r, g, b, _] = d.close_button_hover.0;
        assert!(
            r > g && r > b,
            "close hover must be red-dominant, got {:?}",
            d.close_button_hover.0
        );
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn decoration_theme_defaults_are_derived_from_meridian_dark_blue_tokens() {
        let d = DecorationTheme::default();
        let t = Tokens::new(Appearance::Dark, Accent::Blue);

        assert_eq!(d.corner_radius, shape::R3, "titlebar radius is --r-3");
        assert_eq!(d.background, t.bar_bg_idle);
        assert_eq!(d.background_focused, t.bar_bg);
        assert_eq!(d.maximize_button, t.fill_1);
        assert_eq!(d.maximize_button_hover, t.acc_bg);
        assert_eq!(d.close_button_hover, t.danger);
        assert_eq!(d.menu_background, t.popover_bg);
        assert_eq!(d.icon, t.fg_2);
    }

    #[test]
    fn decoration_theme_border_alpha_matches_the_line_tokens_not_a_hand_picked_value() {
        let d = DecorationTheme::default();
        let t = Tokens::new(Appearance::Dark, Accent::Blue);

        assert_eq!(d.border, t.line, "unfocused border is --line, 9% not 5%");
        assert_eq!(
            d.border_focused, t.line_2,
            "focused border is --line-2, 15% not 10%"
        );
    }

    #[test]
    fn overview_theme_defaults_are_derived_from_meridian_dark_blue_tokens() {
        let o = OverviewTheme::default();
        let t = Tokens::new(Appearance::Dark, Accent::Blue);

        assert_eq!(o.label_color, t.fg_1);
        assert_eq!(o.label_background, t.popover_bg);
        assert_eq!(o.highlight, t.acc_bg);
        assert_eq!(
            o.duration_ms,
            u64::try_from(motion::T4.as_millis()).unwrap()
        );
    }

    #[test]
    fn decoration_theme_defaults_a_positive_corner_radius() {
        assert!(DecorationTheme::default().corner_radius > 0.0);
    }

    #[test]
    fn decoration_theme_round_trips_through_toml_directly() {
        let toml_str = "\
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
shadow-size-focused = 36
shadow-color = \"#000000aa\"
shadow-color-focused = \"#000000bb\"
";
        let theme: DecorationTheme = toml::from_str(toml_str).unwrap();
        assert_eq!(theme.height, 40);
        assert_eq!(theme.title_align, TitleAlign::Center);
        assert!((theme.corner_radius - 12.0).abs() < f32::EPSILON);
        assert_eq!(theme.border_width, 2);
        assert!(!theme.shadow);
        assert_eq!(theme.shadow_size, 24);
        assert_eq!(theme.shadow_size_focused, 36);
        assert_eq!(theme.shadow_color, Color::from_hex("#000000aa").unwrap());
        assert_eq!(
            theme.shadow_color_focused,
            Color::from_hex("#000000bb").unwrap()
        );
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
    fn overview_theme_duration_converts_milliseconds() {
        let o = OverviewTheme {
            duration_ms: 150,
            ..OverviewTheme::default()
        };
        assert_eq!(o.duration(), std::time::Duration::from_millis(150));
    }
}

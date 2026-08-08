use serde::Deserialize;

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
        let theme: DecorationTheme = toml::from_str(toml_str).unwrap();
        assert_eq!(theme.height, 40);
        assert_eq!(theme.title_align, TitleAlign::Center);
        assert!((theme.corner_radius - 12.0).abs() < f32::EPSILON);
        assert_eq!(theme.border_width, 2);
        assert!(!theme.shadow);
        assert_eq!(theme.shadow_size, 24);
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

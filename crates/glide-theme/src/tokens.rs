#![allow(clippy::unreadable_literal)] // Colours are 0xRRGGBB, the same six digits as #rrggbb in the design files

use serde::Deserialize;

use crate::{Accent, Color};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Appearance {
    Light,
    #[default]
    Dark,
}

pub const DARK_FROM_MINUTE: u16 = 19 * 60;
pub const DARK_UNTIL_MINUTE: u16 = 7 * 60;

impl Appearance {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        [Self::Light, Self::Dark].into_iter().find(|appearance| appearance.key() == key)
    }

    #[must_use]
    pub const fn scheduled(minute_of_day: u16) -> Self {
        if minute_of_day >= DARK_FROM_MINUTE || minute_of_day < DARK_UNTIL_MINUTE { Self::Dark } else { Self::Light }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    pub appearance: Appearance,
    pub accent: Accent,
    pub ramp: [Color; 6],

    pub win_bg: Color,
    pub view_bg: Color,
    pub sidebar_bg: Color,
    pub bar_bg: Color,
    pub bar_bg_idle: Color,
    pub card_bg: Color,
    pub popover_bg: Color,
    pub sunken_bg: Color,
    pub inverse_bg: Color,
    pub inverse_fg: Color,

    pub fg_1: Color,
    pub fg_2: Color,
    pub fg_3: Color,
    pub fg_4: Color,

    pub line: Color,
    pub line_2: Color,
    pub fill_1: Color,
    pub fill_2: Color,
    pub fill_3: Color,
    pub state: Color,

    pub acc_bg: Color,
    pub acc_bg_hover: Color,
    pub acc_fg: Color,
    pub acc_text: Color,
    pub acc_soft: Color,
    pub acc_soft_fg: Color,
    pub acc_ring: Color,

    pub ok: Color,
    pub ok_soft: Color,
    pub warn: Color,
    pub warn_soft: Color,
    pub danger: Color,
    pub danger_soft: Color,

    pub shell_bg: Color,
    pub shell_fg: Color,
    pub shell_line: Color,

    pub island_bg: Color,
    pub island_line: Color,

    pub term_bg: Color,
    pub term_fg: Color,
    pub term_accent: Color,
    pub term_warn: Color,
    pub term_ok: Color,
}

impl Tokens {
    #[must_use]
    pub const fn segment_thumb(&self) -> Color {
        match self.appearance {
            Appearance::Light => self.card_bg,
            Appearance::Dark => self.fill_3,
        }
    }

    #[must_use]
    pub const fn switch_thumb(&self) -> Color {
        match self.appearance {
            Appearance::Light => Color([1.0, 1.0, 1.0, 1.0]),
            Appearance::Dark => Color::rgb(0xcfccc7),
        }
    }

    #[must_use]
    pub fn terminal_ansi(&self) -> [Color; 16] {
        let dark = self.appearance == Appearance::Dark;
        let mut colors = if dark {
            [
                0x131218, 0xf0796f, 0x8fe08a, 0xe6c07b, 0x8fb8ff, 0xd797e8, 0x76d6d5, 0xcfe4ca, 0x535158, 0xf4948c,
                0xa6e7a2, 0xebd097, 0xa9cbff, 0xe1aef0, 0x95dedd, 0xffffff,
            ]
        } else {
            [
                0x1e241b, 0xbf372c, 0x1c7a4c, 0x8d600b, 0x1e5bb8, 0x8a3fb8, 0x167a82, 0xd8d6cf, 0x6b6960, 0xd14f43,
                0x289460, 0xa87615, 0x2d72db, 0xa356d4, 0x21949d, 0xf5f4f0,
            ]
        }
        .map(Color::rgb);
        colors[0] = if dark { self.term_bg } else { self.term_fg };
        colors[1] = self.danger;
        colors[2] = self.term_ok;
        colors[3] = self.term_warn;
        colors[4] = self.term_accent;
        if dark {
            colors[7] = self.term_fg;
        } else {
            colors[15] = self.term_bg;
        }
        colors
    }

    #[must_use]
    pub fn new(appearance: Appearance, accent: Accent) -> Self {
        let ramp = accent.ramp();
        let white = Color([1.0, 1.0, 1.0, 1.0]);

        match appearance {
            Appearance::Light => Self {
                appearance,
                accent,
                ramp,

                win_bg: Color::rgb(0xf3f2ef),
                view_bg: Color::rgb(0xffffff),
                sidebar_bg: Color::rgb(0xeceae6),
                bar_bg: Color::rgb(0xfbfaf8),
                bar_bg_idle: Color::rgb(0xf2f0ed),
                card_bg: Color::rgb(0xffffff),
                popover_bg: Color::rgb(0xffffff),
                sunken_bg: Color::rgb(0xe8e6e2),
                inverse_bg: Color::rgb(0x26251f),
                inverse_fg: Color::rgb(0xf7f6f3),

                fg_1: Color::rgb(0x1b1a17),
                fg_2: Color::from_rgb8(27, 26, 23, 0.66),
                fg_3: Color::from_rgb8(27, 26, 23, 0.44),
                fg_4: Color::from_rgb8(27, 26, 23, 0.20),

                line: Color::from_rgb8(27, 26, 23, 0.10),
                line_2: Color::from_rgb8(27, 26, 23, 0.17),
                fill_1: Color::from_rgb8(27, 26, 23, 0.05),
                fill_2: Color::from_rgb8(27, 26, 23, 0.09),
                fill_3: Color::from_rgb8(27, 26, 23, 0.14),
                state: Color::from_rgb8(27, 26, 23, 1.0),

                acc_bg: ramp[3],
                acc_bg_hover: ramp[4],
                acc_fg: white,
                acc_text: ramp[4],
                acc_soft: ramp[0],
                acc_soft_fg: ramp[5],
                acc_ring: ramp[3].with_alpha(0.32),

                ok: Color::rgb(0x1c7a4c),
                ok_soft: Color::rgb(0xdcf0e5),
                warn: Color::rgb(0xa1690a),
                warn_soft: Color::rgb(0xf8ecd6),
                danger: Color::rgb(0xbf372c),
                danger_soft: Color::rgb(0xfae3e0),

                shell_bg: Color::from_rgb8(251, 250, 248, 0.74),
                shell_fg: Color::rgb(0x1b1a17),
                shell_line: Color::from_rgb8(27, 26, 23, 0.12),

                island_bg: Color::rgb(0xffffff),
                island_line: Color::from_rgb8(27, 26, 23, 0.07),

                term_bg: Color::rgb(0xf5f4f0),
                term_fg: Color::rgb(0x1e241b),
                term_accent: Color::rgb(0x1e5bb8),
                term_warn: Color::rgb(0x8d600b),
                term_ok: Color::rgb(0x1c7a4c),
            },
            Appearance::Dark => Self {
                appearance,
                accent,
                ramp,

                win_bg: Color::rgb(0x1c1b1e),
                view_bg: Color::rgb(0x161518),
                sidebar_bg: Color::rgb(0x232227),
                bar_bg: Color::rgb(0x29282d),
                bar_bg_idle: Color::rgb(0x222126),
                card_bg: Color::rgb(0x29282d),
                popover_bg: Color::rgb(0x2f2e34),
                sunken_bg: Color::rgb(0x121114),
                inverse_bg: Color::rgb(0xedece8),
                inverse_fg: Color::rgb(0x1b1a17),

                fg_1: Color::rgb(0xf2f0ed),
                fg_2: Color::from_rgb8(242, 240, 237, 0.68),
                fg_3: Color::from_rgb8(242, 240, 237, 0.46),
                fg_4: Color::from_rgb8(242, 240, 237, 0.22),

                line: Color::from_rgb8(255, 255, 255, 0.09),
                line_2: Color::from_rgb8(255, 255, 255, 0.15),
                fill_1: Color::from_rgb8(255, 255, 255, 0.07),
                fill_2: Color::from_rgb8(255, 255, 255, 0.12),
                fill_3: Color::from_rgb8(255, 255, 255, 0.18),
                state: Color::from_rgb8(255, 255, 255, 1.0),

                acc_bg: ramp[3],
                acc_bg_hover: ramp[2],
                acc_fg: white,
                acc_text: ramp[2],
                acc_soft: ramp[2].with_alpha(0.20),
                acc_soft_fg: ramp[1],
                acc_ring: ramp[2].with_alpha(0.40),

                ok: Color::rgb(0x62d69b),
                ok_soft: Color::from_rgb8(98, 214, 155, 0.16),
                warn: Color::rgb(0xe5b355),
                warn_soft: Color::from_rgb8(229, 179, 85, 0.16),
                danger: Color::rgb(0xf0796f),
                danger_soft: Color::from_rgb8(240, 121, 111, 0.16),

                shell_bg: Color::from_rgb8(38, 37, 42, 0.76),
                shell_fg: Color::rgb(0xf2f0ed),
                shell_line: Color::from_rgb8(255, 255, 255, 0.12),

                island_bg: Color::rgb(0x26252b),
                island_line: Color::from_rgb8(255, 255, 255, 0.07),

                term_bg: Color::rgb(0x131218),
                term_fg: Color::rgb(0xcfe4ca),
                term_accent: Color::rgb(0x8fb8ff),
                term_warn: Color::rgb(0xe6c07b),
                term_ok: Color::rgb(0x8fe08a),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_reads_from_toml_and_defaults_to_dark() {
        #[derive(Deserialize)]
        struct Wrapper {
            appearance: Appearance,
        }
        let light: Wrapper = toml::from_str("appearance = \"light\"\n").unwrap();
        assert_eq!(light.appearance, Appearance::Light);
        assert_eq!(Appearance::default(), Appearance::Dark);
    }

    #[test]
    fn soft_variants_follow_the_spec_alphas() {
        let (light, dark) = (Tokens::new(Appearance::Light, Accent::Blue), Tokens::new(Appearance::Dark, Accent::Blue));
        assert_eq!(light.acc_bg_hover, light.ramp[4]);
        assert_eq!(light.acc_soft.0[3], 1.0);
        assert_eq!(dark.acc_soft, dark.ramp[2].with_alpha(0.20));
        for (soft, base) in [(dark.ok_soft, dark.ok), (dark.warn_soft, dark.warn), (dark.danger_soft, dark.danger)] {
            assert_eq!(soft, base.with_alpha(0.16));
        }
        let luma = |c: Color| c.0[0] * 0.299 + c.0[1] * 0.587 + c.0[2] * 0.114;
        for (d, l) in [(dark.win_bg, light.win_bg), (dark.popover_bg, light.popover_bg)] {
            assert!(luma(d) < luma(l));
        }
    }
}

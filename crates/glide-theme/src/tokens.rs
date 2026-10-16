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
    pub fn new(appearance: Appearance, accent: Accent) -> Self {
        let ramp = accent.ramp();
        let white = Color([1.0, 1.0, 1.0, 1.0]);

        match appearance {
            Appearance::Light => Self {
                appearance,
                accent,
                ramp,

                win_bg: Color::from_hex("#f3f2ef").unwrap(),
                view_bg: Color::from_hex("#ffffff").unwrap(),
                sidebar_bg: Color::from_hex("#eceae6").unwrap(),
                bar_bg: Color::from_hex("#fbfaf8").unwrap(),
                bar_bg_idle: Color::from_hex("#f2f0ed").unwrap(),
                card_bg: Color::from_hex("#ffffff").unwrap(),
                popover_bg: Color::from_hex("#ffffff").unwrap(),
                sunken_bg: Color::from_hex("#e8e6e2").unwrap(),
                inverse_bg: Color::from_hex("#26251f").unwrap(),
                inverse_fg: Color::from_hex("#f7f6f3").unwrap(),

                fg_1: Color::from_hex("#1b1a17").unwrap(),
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

                ok: Color::from_hex("#1c7a4c").unwrap(),
                ok_soft: Color::from_hex("#dcf0e5").unwrap(),
                warn: Color::from_hex("#a1690a").unwrap(),
                warn_soft: Color::from_hex("#f8ecd6").unwrap(),
                danger: Color::from_hex("#bf372c").unwrap(),
                danger_soft: Color::from_hex("#fae3e0").unwrap(),

                shell_bg: Color::from_rgb8(251, 250, 248, 0.74),
                shell_fg: Color::from_hex("#1b1a17").unwrap(),
                shell_line: Color::from_rgb8(27, 26, 23, 0.12),

                island_bg: Color::from_hex("#ffffff").unwrap(),
                island_line: Color::from_rgb8(27, 26, 23, 0.07),

                term_bg: Color::from_hex("#f5f4f0").unwrap(),
                term_fg: Color::from_hex("#1e241b").unwrap(),
                term_accent: Color::from_hex("#1e5bb8").unwrap(),
                term_warn: Color::from_hex("#8d600b").unwrap(),
                term_ok: Color::from_hex("#1c7a4c").unwrap(),
            },
            Appearance::Dark => Self {
                appearance,
                accent,
                ramp,

                win_bg: Color::from_hex("#1c1b1e").unwrap(),
                view_bg: Color::from_hex("#161518").unwrap(),
                sidebar_bg: Color::from_hex("#232227").unwrap(),
                bar_bg: Color::from_hex("#29282d").unwrap(),
                bar_bg_idle: Color::from_hex("#222126").unwrap(),
                card_bg: Color::from_hex("#29282d").unwrap(),
                popover_bg: Color::from_hex("#2f2e34").unwrap(),
                sunken_bg: Color::from_hex("#121114").unwrap(),
                inverse_bg: Color::from_hex("#edece8").unwrap(),
                inverse_fg: Color::from_hex("#1b1a17").unwrap(),

                fg_1: Color::from_hex("#f2f0ed").unwrap(),
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

                ok: Color::from_hex("#62d69b").unwrap(),
                ok_soft: Color::from_rgb8(98, 214, 155, 0.16),
                warn: Color::from_hex("#e5b355").unwrap(),
                warn_soft: Color::from_rgb8(229, 179, 85, 0.16),
                danger: Color::from_hex("#f0796f").unwrap(),
                danger_soft: Color::from_rgb8(240, 121, 111, 0.16),

                shell_bg: Color::from_rgb8(38, 37, 42, 0.76),
                shell_fg: Color::from_hex("#f2f0ed").unwrap(),
                shell_line: Color::from_rgb8(255, 255, 255, 0.12),

                island_bg: Color::from_hex("#26252b").unwrap(),
                island_line: Color::from_rgb8(255, 255, 255, 0.07),

                term_bg: Color::from_hex("#131218").unwrap(),
                term_fg: Color::from_hex("#cfe4ca").unwrap(),
                term_accent: Color::from_hex("#8fb8ff").unwrap(),
                term_warn: Color::from_hex("#e6c07b").unwrap(),
                term_ok: Color::from_hex("#8fe08a").unwrap(),
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

use serde::Deserialize;

use crate::{Accent, Color};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Appearance {
    Light,
    #[default]
    Dark,
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

                shell_bg: Color::from_rgb8(251, 250, 248, 0.82),
                shell_fg: Color::from_hex("#1b1a17").unwrap(),
                shell_line: Color::from_rgb8(27, 26, 23, 0.12),

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

                shell_bg: Color::from_rgb8(38, 37, 42, 0.84),
                shell_fg: Color::from_hex("#f2f0ed").unwrap(),
                shell_line: Color::from_rgb8(255, 255, 255, 0.12),

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
    fn default_appearance_is_dark() {
        assert_eq!(Appearance::default(), Appearance::Dark);
    }

    #[test]
    fn appearance_round_trips_through_toml() {
        #[derive(Deserialize)]
        struct Wrapper {
            appearance: Appearance,
        }
        let light: Wrapper = toml::from_str("appearance = \"light\"\n").unwrap();
        assert_eq!(light.appearance, Appearance::Light);
        let dark: Wrapper = toml::from_str("appearance = \"dark\"\n").unwrap();
        assert_eq!(dark.appearance, Appearance::Dark);
    }

    #[test]
    fn acc_bg_is_ramp_three_in_both_appearances() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let t = Tokens::new(appearance, Accent::Blue);
            assert_eq!(t.acc_bg, t.ramp[3], "{appearance:?}");
        }
    }

    #[test]
    fn acc_bg_hover_differs_by_appearance() {
        let light = Tokens::new(Appearance::Light, Accent::Blue);
        assert_eq!(light.acc_bg_hover, light.ramp[4]);

        let dark = Tokens::new(Appearance::Dark, Accent::Blue);
        assert_eq!(dark.acc_bg_hover, dark.ramp[2]);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn acc_soft_is_opaque_in_light_and_translucent_in_dark() {
        let light = Tokens::new(Appearance::Light, Accent::Blue);
        assert_eq!(light.acc_soft.0[3], 1.0, "light acc-soft is a solid tint");

        let dark = Tokens::new(Appearance::Dark, Accent::Blue);
        assert!(
            (dark.acc_soft.0[3] - 0.20).abs() < 0.001,
            "dark acc-soft must be 20% of ramp[2]"
        );
        assert_eq!(
            [dark.acc_soft.0[0], dark.acc_soft.0[1], dark.acc_soft.0[2]],
            [dark.ramp[2].0[0], dark.ramp[2].0[1], dark.ramp[2].0[2]]
        );
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn semantic_soft_variants_are_rgba_sixteen_percent_in_dark() {
        let dark = Tokens::new(Appearance::Dark, Accent::Blue);
        for (soft, base) in [
            (dark.ok_soft, dark.ok),
            (dark.warn_soft, dark.warn),
            (dark.danger_soft, dark.danger),
        ] {
            assert!((soft.0[3] - 0.16).abs() < 0.001);
            assert_eq!(
                [soft.0[0], soft.0[1], soft.0[2]],
                [base.0[0], base.0[1], base.0[2]]
            );
        }
    }

    #[test]
    fn text_tiers_get_less_opaque_from_fg_1_to_fg_4() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let t = Tokens::new(appearance, Accent::Blue);
            assert!(t.fg_1.0[3] >= t.fg_2.0[3], "{appearance:?}");
            assert!(t.fg_2.0[3] > t.fg_3.0[3], "{appearance:?}");
            assert!(t.fg_3.0[3] > t.fg_4.0[3], "{appearance:?}");
        }
    }

    #[test]
    fn line_two_is_stronger_than_line_in_both_appearances() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let t = Tokens::new(appearance, Accent::Blue);
            assert!(t.line_2.0[3] > t.line.0[3], "{appearance:?}");
        }
    }

    #[test]
    fn fills_escalate_from_fill_1_to_fill_3() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let t = Tokens::new(appearance, Accent::Blue);
            assert!(t.fill_1.0[3] < t.fill_2.0[3], "{appearance:?}");
            assert!(t.fill_2.0[3] < t.fill_3.0[3], "{appearance:?}");
        }
    }

    #[test]
    fn dark_surfaces_are_darker_than_light_surfaces() {
        let luma = |c: Color| c.0[0] * 0.299 + c.0[1] * 0.587 + c.0[2] * 0.114;
        let light = Tokens::new(Appearance::Light, Accent::Blue);
        let dark = Tokens::new(Appearance::Dark, Accent::Blue);
        assert!(luma(dark.win_bg) < luma(light.win_bg));
        assert!(luma(dark.view_bg) < luma(light.view_bg));
        assert!(luma(dark.popover_bg) < luma(light.popover_bg));
    }

    #[test]
    fn switching_accent_changes_the_ramp_but_not_the_neutral_surfaces() {
        let blue = Tokens::new(Appearance::Dark, Accent::Blue);
        let red = Tokens::new(Appearance::Dark, Accent::Red);
        assert_ne!(blue.ramp, red.ramp);
        assert_eq!(blue.win_bg, red.win_bg);
        assert_eq!(blue.fg_1, red.fg_1);
    }

    #[test]
    fn every_accent_produces_a_valid_token_set_in_both_appearances() {
        for accent in Accent::ALL {
            for appearance in [Appearance::Light, Appearance::Dark] {
                let t = Tokens::new(appearance, *accent);
                assert_eq!(t.acc_bg, t.ramp[3]);
            }
        }
    }
}

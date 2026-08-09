use crate::{Appearance, Color};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowLayer {
    pub dx: f32,
    pub dy: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
}

impl ShadowLayer {
    const fn new(dx: f32, dy: f32, blur: f32, spread: f32, color: Color) -> Self {
        Self {
            dx,
            dy,
            blur,
            spread,
            color,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    El1,
    El2,
    El3,
    ElWin,
    ElWinIdle,
}

#[must_use]
pub fn layers(level: Level, appearance: Appearance) -> Vec<ShadowLayer> {
    match (level, appearance) {
        (Level::El1, Appearance::Light) => vec![ShadowLayer::new(
            0.0,
            1.0,
            2.0,
            0.0,
            Color::from_rgb8(27, 26, 23, 0.07),
        )],
        (Level::El1, Appearance::Dark) => {
            vec![ShadowLayer::new(
                0.0,
                1.0,
                2.0,
                0.0,
                Color::from_rgb8(0, 0, 0, 0.45),
            )]
        }

        (Level::El2, Appearance::Light) => vec![
            ShadowLayer::new(0.0, 2.0, 4.0, 0.0, Color::from_rgb8(27, 26, 23, 0.06)),
            ShadowLayer::new(0.0, 6.0, 16.0, 0.0, Color::from_rgb8(27, 26, 23, 0.09)),
        ],
        (Level::El2, Appearance::Dark) => vec![
            ShadowLayer::new(0.0, 2.0, 6.0, 0.0, Color::from_rgb8(0, 0, 0, 0.45)),
            ShadowLayer::new(0.0, 10.0, 24.0, 0.0, Color::from_rgb8(0, 0, 0, 0.40)),
        ],

        (Level::El3, Appearance::Light) => vec![
            ShadowLayer::new(0.0, 4.0, 8.0, 0.0, Color::from_rgb8(27, 26, 23, 0.08)),
            ShadowLayer::new(0.0, 18.0, 44.0, 0.0, Color::from_rgb8(27, 26, 23, 0.15)),
        ],
        (Level::El3, Appearance::Dark) => vec![
            ShadowLayer::new(0.0, 4.0, 10.0, 0.0, Color::from_rgb8(0, 0, 0, 0.5)),
            ShadowLayer::new(0.0, 22.0, 52.0, 0.0, Color::from_rgb8(0, 0, 0, 0.55)),
        ],

        (Level::ElWin, Appearance::Light) => vec![
            ShadowLayer::new(0.0, 0.0, 0.0, 1.0, Color::from_rgb8(27, 26, 23, 0.07)),
            ShadowLayer::new(0.0, 6.0, 14.0, 0.0, Color::from_rgb8(27, 26, 23, 0.10)),
            ShadowLayer::new(0.0, 26.0, 60.0, 0.0, Color::from_rgb8(27, 26, 23, 0.22)),
        ],
        (Level::ElWin, Appearance::Dark) => vec![
            ShadowLayer::new(0.0, 0.0, 0.0, 1.0, Color::from_rgb8(255, 255, 255, 0.10)),
            ShadowLayer::new(0.0, 8.0, 16.0, 0.0, Color::from_rgb8(0, 0, 0, 0.5)),
            ShadowLayer::new(0.0, 32.0, 68.0, 0.0, Color::from_rgb8(0, 0, 0, 0.6)),
        ],

        (Level::ElWinIdle, Appearance::Light) => vec![
            ShadowLayer::new(0.0, 0.0, 0.0, 1.0, Color::from_rgb8(27, 26, 23, 0.06)),
            ShadowLayer::new(0.0, 2.0, 6.0, 0.0, Color::from_rgb8(27, 26, 23, 0.06)),
            ShadowLayer::new(0.0, 10.0, 24.0, 0.0, Color::from_rgb8(27, 26, 23, 0.09)),
        ],
        (Level::ElWinIdle, Appearance::Dark) => vec![
            ShadowLayer::new(0.0, 0.0, 0.0, 1.0, Color::from_rgb8(255, 255, 255, 0.05)),
            ShadowLayer::new(0.0, 3.0, 8.0, 0.0, Color::from_rgb8(0, 0, 0, 0.4)),
            ShadowLayer::new(0.0, 14.0, 32.0, 0.0, Color::from_rgb8(0, 0, 0, 0.4)),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::float_cmp)]
    fn el_win_has_a_border_ring_plus_two_shadow_layers_in_both_appearances() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let l = layers(Level::ElWin, appearance);
            assert_eq!(l.len(), 3, "{appearance:?}");
            assert_eq!(l[0].spread, 1.0, "first layer is the 1px border ring");
            assert_eq!(l[0].blur, 0.0);
        }
    }

    #[test]
    fn el_win_shadows_get_deeper_further_from_the_window() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let l = layers(Level::ElWin, appearance);
            assert!(
                l[1].dy < l[2].dy,
                "{appearance:?}: dy must grow with distance"
            );
            assert!(
                l[1].blur < l[2].blur,
                "{appearance:?}: blur must grow with distance"
            );
        }
    }

    #[test]
    fn el_win_is_darker_than_el_win_idle_in_dark_appearance() {
        let active = layers(Level::ElWin, Appearance::Dark);
        let idle = layers(Level::ElWinIdle, Appearance::Dark);
        let deepest = |layers: &[ShadowLayer]| layers.last().unwrap().color.0[3];
        assert!(
            deepest(&active) > deepest(&idle),
            "an active window's shadow must read as deeper than an idle one's"
        );
    }

    #[test]
    fn el_1_through_el_3_add_more_layers_as_elevation_increases() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            assert_eq!(layers(Level::El1, appearance).len(), 1);
            assert_eq!(layers(Level::El2, appearance).len(), 2);
            assert_eq!(layers(Level::El3, appearance).len(), 2);
        }
    }
}

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
        Self { dx, dy, blur, spread, color }
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
        (Level::El1, Appearance::Light) => {
            vec![ShadowLayer::new(0.0, 1.0, 2.0, 0.0, Color::from_rgb8(27, 26, 23, 0.07))]
        }
        (Level::El1, Appearance::Dark) => {
            vec![ShadowLayer::new(0.0, 1.0, 2.0, 0.0, Color::from_rgb8(0, 0, 0, 0.45))]
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
    fn window_levels_are_a_ring_plus_deepening_shadows() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            for level in [Level::ElWin, Level::ElWinIdle] {
                let l = layers(level, appearance);
                assert_eq!((l.len(), l[0].spread, l[0].blur), (3, 1.0, 0.0));
                assert!(l[1].dy < l[2].dy && l[1].blur < l[2].blur);
            }
        }
        let deepest = |level| layers(level, Appearance::Dark)[2].color.0[3];
        assert!(deepest(Level::ElWin) > deepest(Level::ElWinIdle));
    }
}

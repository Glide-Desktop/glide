use std::time::Duration;

pub const T1: Duration = Duration::from_millis(120);
pub const T2: Duration = Duration::from_millis(200);
pub const T3: Duration = Duration::from_millis(320);
pub const T4: Duration = Duration::from_millis(420);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

pub const E_OUT: CubicBezier = CubicBezier {
    x1: 0.2,
    y1: 0.0,
    x2: 0.0,
    y2: 1.0,
};

pub const E_IO: CubicBezier = CubicBezier {
    x1: 0.4,
    y1: 0.0,
    x2: 0.2,
    y2: 1.0,
};

pub const E_OVER: CubicBezier = CubicBezier {
    x1: 0.32,
    y1: 1.28,
    x2: 0.5,
    y2: 1.0,
};

pub const ST_HOVER: f32 = 0.08;
pub const ST_PRESS: f32 = 0.13;
pub const ST_DRAG: f32 = 0.18;

#[cfg(test)]
#[allow(clippy::assertions_on_constants)]
mod tests {
    use super::*;

    #[test]
    fn durations_strictly_increase() {
        assert!(T1 < T2);
        assert!(T2 < T3);
        assert!(T3 < T4);
    }

    #[test]
    fn e_over_overshoots_past_one() {
        assert!(
            E_OVER.y1 > 1.0,
            "the overshoot curve must have a control point beyond y=1"
        );
    }

    #[test]
    fn e_out_and_e_io_stay_within_the_unit_square_on_x() {
        for curve in [E_OUT, E_IO] {
            assert!((0.0..=1.0).contains(&curve.x1));
            assert!((0.0..=1.0).contains(&curve.x2));
        }
    }

    #[test]
    fn state_layer_opacities_strictly_increase_with_intensity() {
        assert!(ST_HOVER < ST_PRESS);
        assert!(ST_PRESS < ST_DRAG);
    }
}

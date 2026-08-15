use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

pub const T1: Duration = Duration::from_millis(120);
pub const T2: Duration = Duration::from_millis(200);
pub const T3: Duration = Duration::from_millis(320);
pub const T4: Duration = Duration::from_millis(420);

static MOTION_SCALE: AtomicU32 = AtomicU32::new(1.0_f32.to_bits());

pub fn set_scale(scale: f32) {
    let clamped = if scale.is_finite() {
        scale.clamp(0.0, 10.0)
    } else {
        1.0
    };
    MOTION_SCALE.store(clamped.to_bits(), Ordering::Relaxed);
}

#[must_use]
pub fn scale() -> f32 {
    f32::from_bits(MOTION_SCALE.load(Ordering::Relaxed))
}

#[must_use]
pub fn is_disabled() -> bool {
    scale() <= 0.0
}

#[must_use]
pub fn scaled(base: Duration) -> Duration {
    let factor = scale();
    if factor <= 0.0 {
        return Duration::ZERO;
    }
    if (factor - 1.0).abs() < f32::EPSILON {
        return base;
    }
    base.mul_f32(factor)
}

#[must_use]
pub fn t1() -> Duration {
    scaled(T1)
}

#[must_use]
pub fn t2() -> Duration {
    scaled(T2)
}

#[must_use]
pub fn t3() -> Duration {
    scaled(T3)
}

#[must_use]
pub fn t4() -> Duration {
    scaled(T4)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl CubicBezier {
    const fn coefficients(p1: f32, p2: f32) -> (f32, f32, f32) {
        let c = 3.0 * p1;
        let b = 3.0 * p2 - 6.0 * p1;
        let a = 1.0 + 3.0 * p1 - 3.0 * p2;
        (a, b, c)
    }

    fn axis_at(p1: f32, p2: f32, u: f32) -> f32 {
        let (a, b, c) = Self::coefficients(p1, p2);
        ((a * u + b) * u + c) * u
    }

    fn axis_slope(p1: f32, p2: f32, u: f32) -> f32 {
        let (a, b, c) = Self::coefficients(p1, p2);
        (3.0 * a * u + 2.0 * b) * u + c
    }

    fn solve_for_x(self, x: f32) -> f32 {
        const NEWTON_ITERATIONS: usize = 8;
        const BISECTION_ITERATIONS: usize = 24;
        const EPSILON: f32 = 1e-6;

        let mut u = x;
        for _ in 0..NEWTON_ITERATIONS {
            let error = Self::axis_at(self.x1, self.x2, u) - x;
            if error.abs() < EPSILON {
                return u;
            }
            let slope = Self::axis_slope(self.x1, self.x2, u);
            if slope.abs() < EPSILON {
                break;
            }
            let next = u - error / slope;
            if !next.is_finite() {
                break;
            }
            u = next;
        }

        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        let mut u = x.clamp(0.0, 1.0);
        for _ in 0..BISECTION_ITERATIONS {
            let error = Self::axis_at(self.x1, self.x2, u) - x;
            if error.abs() < EPSILON {
                break;
            }
            if error > 0.0 {
                high = u;
            } else {
                low = u;
            }
            u = f32::midpoint(low, high);
        }
        u
    }

    #[must_use]
    pub fn ease(self, progress: f32) -> f32 {
        if progress <= 0.0 {
            return 0.0;
        }
        if progress >= 1.0 {
            return 1.0;
        }
        let u = self.solve_for_x(progress);
        Self::axis_at(self.y1, self.y2, u)
    }

    #[must_use]
    pub fn ease_f64(self, progress: f64) -> f64 {
        #[allow(clippy::cast_possible_truncation)]
        f64::from(self.ease(progress as f32))
    }
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

    mod motion_scale {
        use super::*;

        static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

        fn with_scale<F: FnOnce()>(value: f32, body: F) {
            let _lock = GUARD
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            set_scale(value);
            body();
            set_scale(1.0);
        }

        #[test]
        fn the_default_scale_leaves_tokens_at_their_spec_values() {
            with_scale(1.0, || {
                assert_eq!(t1(), T1);
                assert_eq!(t2(), T2);
                assert_eq!(t3(), T3);
                assert_eq!(t4(), T4);
            });
        }

        #[test]
        fn a_zero_scale_collapses_every_token_to_nothing() {
            with_scale(0.0, || {
                assert!(is_disabled());
                for token in [t1(), t2(), t3(), t4()] {
                    assert_eq!(token, Duration::ZERO);
                }
            });
        }

        #[test]
        fn a_fractional_scale_shortens_proportionally() {
            with_scale(0.5, || {
                assert_eq!(t2(), Duration::from_millis(100));
                assert_eq!(t4(), Duration::from_millis(210));
            });
        }

        #[test]
        fn absurd_and_non_finite_scales_are_clamped_rather_than_honoured() {
            with_scale(1e9, || assert!((scale() - 10.0).abs() < f32::EPSILON));
            with_scale(-4.0, || assert!(scale().abs() < f32::EPSILON));
            with_scale(f32::NAN, || assert!((scale() - 1.0).abs() < f32::EPSILON));
        }
    }

    mod easing {
        use super::*;

        #[test]
        fn every_curve_is_pinned_at_both_endpoints() {
            for curve in [E_OUT, E_IO, E_OVER] {
                assert!(curve.ease(0.0).abs() < 1e-4, "{curve:?} at 0");
                assert!((curve.ease(1.0) - 1.0).abs() < 1e-4, "{curve:?} at 1");
            }
        }

        #[test]
        fn out_of_range_progress_is_clamped_at_the_endpoints() {
            for curve in [E_OUT, E_IO, E_OVER] {
                assert!(curve.ease(-5.0).abs() < 1e-4);
                assert!((curve.ease(9.0) - 1.0).abs() < 1e-4);
            }
        }

        #[test]
        fn plain_curves_rise_monotonically_within_the_unit_range() {
            for curve in [E_OUT, E_IO] {
                let mut previous = 0.0;
                for step in 0..=100 {
                    #[allow(clippy::cast_precision_loss)]
                    let progress = step as f32 / 100.0;
                    let value = curve.ease(progress);
                    assert!(
                        value >= previous - 1e-4,
                        "{curve:?} dipped at {progress}: {value} < {previous}"
                    );
                    assert!(
                        (-1e-4..=1.0 + 1e-4).contains(&value),
                        "{curve:?} left the unit range at {progress}: {value}"
                    );
                    previous = value;
                }
            }
        }

        #[test]
        fn e_out_is_already_half_done_a_fifth_of_the_way_through() {
            let y = E_OUT.ease(0.2);
            assert!(
                (y - 0.5).abs() < 0.02,
                "expected ~0.5 at x=0.2, got {y} — the x-axis solve looks wrong"
            );
        }

        #[test]
        fn e_out_front_loads_more_than_e_io() {
            for step in 1..50 {
                #[allow(clippy::cast_precision_loss)]
                let progress = step as f32 / 100.0;
                assert!(
                    E_OUT.ease(progress) > E_IO.ease(progress),
                    "at {progress}: e-out {} !> e-io {}",
                    E_OUT.ease(progress),
                    E_IO.ease(progress)
                );
            }
        }

        #[test]
        fn e_over_actually_overshoots_past_one_before_settling() {
            let peak = (0..=100)
                .map(|step| {
                    #[allow(clippy::cast_precision_loss)]
                    let progress = step as f32 / 100.0;
                    E_OVER.ease(progress)
                })
                .fold(f32::MIN, f32::max);
            assert!(peak > 1.0, "overshoot curve peaked at only {peak}");
        }

        #[test]
        fn f64_and_f32_entry_points_agree() {
            for step in 0..=20 {
                #[allow(clippy::cast_precision_loss)]
                let progress = step as f32 / 20.0;
                let a = f64::from(E_OUT.ease(progress));
                let b = E_OUT.ease_f64(f64::from(progress));
                assert!((a - b).abs() < 1e-6, "at {progress}: {a} vs {b}");
            }
        }
    }
}

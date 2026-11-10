use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

pub const T1: Duration = Duration::from_millis(120);
pub const T2: Duration = Duration::from_millis(200);
pub const T3: Duration = Duration::from_millis(320);
pub const T4: Duration = Duration::from_millis(420);

static MOTION_SCALE: AtomicU32 = AtomicU32::new(1.0_f32.to_bits());

pub fn set_scale(scale: f32) {
    let clamped = if scale.is_finite() { scale.clamp(0.0, 10.0) } else { 1.0 };
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
        f64::from(self.ease(progress as f32))
    }
}

pub const E_OUT: CubicBezier = CubicBezier { x1: 0.2, y1: 0.0, x2: 0.0, y2: 1.0 };

pub const E_IO: CubicBezier = CubicBezier { x1: 0.4, y1: 0.0, x2: 0.2, y2: 1.0 };

pub const E_OVER: CubicBezier = CubicBezier { x1: 0.32, y1: 1.28, x2: 0.5, y2: 1.0 };

#[must_use]
pub fn ease_out(t: f64) -> f64 {
    E_OUT.ease_f64(t)
}

#[must_use]
pub fn ease_io(t: f64) -> f64 {
    E_IO.ease_f64(t)
}

#[must_use]
pub fn ease_over(t: f64) -> f64 {
    E_OVER.ease_f64(t)
}

#[must_use]
pub fn lerp(from: f64, to: f64, t: f64) -> f64 {
    (to - from).mul_add(t, from)
}

pub const OPACITY_LEAD: f64 = 0.4;

#[must_use]
pub fn assist(progress: f64) -> f64 {
    (progress / OPACITY_LEAD).clamp(0.0, 1.0)
}

#[must_use]
pub fn window(progress: f64, start: f64, end: f64) -> f64 {
    if end <= start {
        return if progress >= start { 1.0 } else { 0.0 };
    }
    ((progress - start) / (end - start)).clamp(0.0, 1.0)
}

pub const RELEASE_MAX_SLOPE: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Release {
    pub duration: Duration,
    pub slope: f64,
}

impl Release {
    #[must_use]
    pub fn new(distance: f64, velocity: f64, longest: Duration) -> Self {
        if is_disabled() {
            return Self { duration: Duration::ZERO, slope: 0.0 };
        }
        let distance = if distance.is_finite() { distance } else { 1.0 };
        let velocity = if velocity.is_finite() { velocity } else { 0.0 };
        let span = distance.abs();
        let paced = longest.mul_f64(span.clamp(0.25, 1.0));
        let speed = velocity.abs();
        let towards = velocity * distance > 0.0 && speed > 1e-3 && span > 1e-6;
        if !towards {
            return Self { duration: paced, slope: 0.0 };
        }
        let matched = Duration::try_from_secs_f64(RELEASE_MAX_SLOPE * span / speed).unwrap_or(paced);
        let duration = matched.clamp(t1().min(paced), paced);
        Self { duration, slope: (speed * duration.as_secs_f64() / span).min(RELEASE_MAX_SLOPE) }
    }

    #[must_use]
    pub fn at(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        let (t2, t3) = (t * t, t * t * t);
        (t3 - 2.0 * t2 + t).mul_add(self.slope, 3.0 * t2 - 2.0 * t3)
    }
}

pub const ST_HOVER: f32 = 0.08;
pub const ST_PRESS: f32 = 0.13;
pub const ST_DRAG: f32 = 0.18;

#[cfg(test)]
mod tests {
    use super::*;

    const _: () = assert!(ST_HOVER < ST_PRESS && ST_PRESS < ST_DRAG);

    #[test]
    fn opacity_is_full_for_the_first_part_of_a_motion_and_holds_until_its_last_part() {
        assert_eq!(assist(0.0), 0.0);
        assert!((assist(OPACITY_LEAD) - 1.0).abs() < 1e-12, "opaque once 40 % of the movement is done");
        assert_eq!(assist(0.7), 1.0, "an exit keeps full opacity until its last 40 %");
        assert!(assist(0.2) < 1.0 && assist(0.2) > 0.0);
    }

    #[test]
    fn a_progress_window_maps_its_span_onto_zero_to_one() {
        assert_eq!(window(0.1, 0.2, 0.6), 0.0);
        assert!((window(0.4, 0.2, 0.6) - 0.5).abs() < 1e-12);
        assert_eq!(window(0.9, 0.2, 0.6), 1.0);
        assert_eq!(window(0.5, 0.5, 0.5), 1.0, "an empty window is a step");
    }

    mod motion_scale {
        use super::*;

        static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

        fn with_scale<F: FnOnce()>(value: f32, body: F) {
            let _lock = GUARD.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            set_scale(value);
            body();
            set_scale(1.0);
        }

        #[test]
        fn the_scale_stretches_collapses_and_clamps_every_token() {
            with_scale(1.0, || {
                assert_eq!([t1(), t2(), t3(), t4()], [T1, T2, T3, T4]);
            });
            with_scale(0.0, || {
                assert!(is_disabled());
                assert_eq!([t1(), t2(), t3(), t4()], [Duration::ZERO; 4]);
            });
            with_scale(0.5, || assert_eq!(t4(), Duration::from_millis(210)));
            with_scale(1e9, || assert!((scale() - 10.0).abs() < f32::EPSILON));
            with_scale(-4.0, || assert!(scale().abs() < f32::EPSILON));
            with_scale(f32::NAN, || assert!((scale() - 1.0).abs() < f32::EPSILON));
        }

        #[test]
        fn a_release_starts_at_the_finger_speed_and_lands_at_rest() {
            with_scale(1.0, || {
                let release = Release::new(0.6, 4.0, T3);
                let dt = 1e-4;
                let start = release.at(dt) / dt * 0.6 / release.duration.as_secs_f64();
                assert!((start - 4.0).abs() < 0.05, "starts at the finger speed, got {start}");
                let end = (release.at(1.0) - release.at(1.0 - dt)) / dt;
                assert!(end.abs() < 1e-3, "lands at rest, got {end}");
                assert!((release.at(1.0) - 1.0).abs() < 1e-9);
            });
        }

        #[test]
        fn a_release_never_overshoots_or_runs_backwards() {
            with_scale(1.0, || {
                for (distance, velocity) in [(0.6, 50.0), (0.3, -8.0), (-0.5, -0.2), (0.05, 0.0)] {
                    let release = Release::new(distance, velocity, T3);
                    let mut previous = 0.0;
                    for step in 0..=100 {
                        let value = release.at(f64::from(step) / 100.0);
                        assert!(value >= previous - 1e-9 && value <= 1.0 + 1e-9);
                        previous = value;
                    }
                }
            });
        }

        #[test]
        fn a_faster_flick_releases_sooner_but_never_under_t1() {
            with_scale(1.0, || {
                let slow = Release::new(0.6, 0.0, T3);
                let fast = Release::new(0.6, 12.0, T3);
                let absurd = Release::new(0.6, 1e6, T3);
                assert!(fast.duration < slow.duration);
                assert_eq!(absurd.duration, T1);
                assert!(slow.duration <= T3);
                assert_eq!(slow.slope, 0.0);
            });
        }

        #[test]
        fn a_distance_no_duration_can_hold_releases_at_the_longest_pace_instead_of_panicking() {
            with_scale(1.0, || {
                let endless = Release::new(1e300, 5.0, T3);
                assert_eq!(endless.duration, T3);
                assert!(endless.slope.is_finite());
            });
        }

        #[test]
        fn a_disabled_scale_releases_instantly() {
            with_scale(0.0, || {
                assert_eq!(Release::new(0.6, 4.0, T3).duration, Duration::ZERO);
            });
        }
    }

    mod easing {
        use super::*;

        #[test]
        fn every_curve_is_pinned_at_both_endpoints() {
            for curve in [E_OUT, E_IO, E_OVER] {
                for (progress, expected) in [(0.0, 0.0), (1.0, 1.0), (-5.0, 0.0), (9.0, 1.0)] {
                    assert!((curve.ease(progress) - expected).abs() < 1e-4, "{curve:?}");
                }
            }
        }

        #[test]
        fn plain_curves_rise_monotonically_within_the_unit_range() {
            for curve in [E_OUT, E_IO] {
                let mut previous = 0.0;
                for step in 0..=100 {
                    let progress = step as f32 / 100.0;
                    let value = curve.ease(progress);
                    assert!(value >= previous - 1e-4, "{curve:?} dipped at {progress}: {value} < {previous}");
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
            assert!((y - 0.5).abs() < 0.02, "expected ~0.5 at x=0.2, got {y} — the x-axis solve looks wrong");
        }

        #[test]
        fn e_out_front_loads_more_than_e_io() {
            for step in 1..50 {
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
                    let progress = step as f32 / 100.0;
                    E_OVER.ease(progress)
                })
                .fold(f32::MIN, f32::max);
            assert!(peak > 1.0, "overshoot curve peaked at only {peak}");
        }
    }
}

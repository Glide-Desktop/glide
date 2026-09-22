use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Closed,
    Opening,
    Open,
    Closing,
}

#[derive(Debug, Clone, Copy)]
pub struct Reveal {
    phase: Phase,
    from: f64,
    started: Instant,
}

impl Default for Reveal {
    fn default() -> Self {
        Self::closed()
    }
}

impl Reveal {
    #[must_use]
    pub fn closed() -> Self {
        Self {
            phase: Phase::Closed,
            from: 0.0,
            started: Instant::now(),
        }
    }

    #[must_use]
    pub fn open_now() -> Self {
        Self {
            phase: Phase::Open,
            from: 1.0,
            started: Instant::now(),
        }
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        matches!(self.phase, Phase::Open | Phase::Opening)
    }

    #[must_use]
    pub fn is_visible(&self, duration: Duration) -> bool {
        self.linear(duration) > 0.0
    }

    #[must_use]
    pub fn is_animating(&self, duration: Duration) -> bool {
        match self.phase {
            Phase::Open | Phase::Closed => false,
            Phase::Opening | Phase::Closing => {
                !duration.is_zero() && self.started.elapsed() < duration
            }
        }
    }

    pub fn open(&mut self, duration: Duration) -> bool {
        if self.is_open() {
            return false;
        }
        self.from = self.linear(duration);
        self.phase = if duration.is_zero() {
            Phase::Open
        } else {
            Phase::Opening
        };
        self.started = Instant::now();
        true
    }

    pub fn close(&mut self, duration: Duration) -> bool {
        if !self.is_open() {
            return false;
        }
        self.from = self.linear(duration);
        self.phase = if duration.is_zero() {
            Phase::Closed
        } else {
            Phase::Closing
        };
        self.started = Instant::now();
        true
    }

    pub fn toggle(&mut self, duration: Duration) -> bool {
        if self.is_open() {
            self.close(duration)
        } else {
            self.open(duration)
        }
    }

    pub fn settle(&mut self, duration: Duration) {
        if !self.is_animating(duration) {
            self.phase = match self.phase {
                Phase::Opening => Phase::Open,
                Phase::Closing => Phase::Closed,
                other => other,
            };
        }
    }

    fn fraction(&self, duration: Duration) -> f64 {
        if duration.is_zero() {
            return 1.0;
        }
        (self.started.elapsed().as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn linear(&self, duration: Duration) -> f64 {
        match self.phase {
            Phase::Open => 1.0,
            Phase::Closed => 0.0,
            Phase::Opening => self.from + (1.0 - self.from) * self.fraction(duration),
            Phase::Closing => self.from * (1.0 - self.fraction(duration)),
        }
    }

    #[must_use]
    pub fn progress(&self, duration: Duration, ease: impl Fn(f64) -> f64) -> f64 {
        match self.phase {
            Phase::Open => 1.0,
            Phase::Closed => 0.0,
            Phase::Opening => self.from + (1.0 - self.from) * ease(self.fraction(duration)),
            Phase::Closing => self.from * (1.0 - ease(self.fraction(duration))),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Tween {
    from: f64,
    to: f64,
    started: Instant,
}

impl Tween {
    #[must_use]
    pub fn at(value: f64) -> Self {
        Self {
            from: value,
            to: value,
            started: Instant::now(),
        }
    }

    #[must_use]
    pub fn target(&self) -> f64 {
        self.to
    }

    pub fn retarget(&mut self, to: f64, duration: Duration, ease: impl Fn(f64) -> f64) -> bool {
        if (self.to - to).abs() < f64::EPSILON {
            return false;
        }
        self.from = self.value(duration, ease);
        self.to = to;
        self.started = Instant::now();
        true
    }

    pub fn jump(&mut self, to: f64) {
        self.from = to;
        self.to = to;
        self.started = Instant::now();
    }

    #[must_use]
    pub fn value(&self, duration: Duration, ease: impl Fn(f64) -> f64) -> f64 {
        if duration.is_zero() {
            return self.to;
        }
        let elapsed = self.started.elapsed();
        if elapsed >= duration {
            return self.to;
        }
        let t = ease(elapsed.as_secs_f64() / duration.as_secs_f64());
        self.from + (self.to - self.from) * t
    }

    #[must_use]
    pub fn is_animating(&self, duration: Duration) -> bool {
        !duration.is_zero()
            && (self.from - self.to).abs() > f64::EPSILON
            && self.started.elapsed() < duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Duration = Duration::from_millis(200);
    fn linear(t: f64) -> f64 {
        t
    }

    #[test]
    fn a_closed_reveal_is_invisible_and_idle() {
        let r = Reveal::closed();
        assert!(!r.is_open());
        assert!(!r.is_visible(D));
        assert!(!r.is_animating(D));
        assert!((r.progress(D, linear) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn opening_starts_from_zero_and_keeps_the_pump_alive() {
        let mut r = Reveal::closed();
        assert!(r.open(D));
        assert!(!r.open(D), "opening twice is a no-op");
        assert!(r.is_open());
        assert!(r.is_animating(D));
        let t = r.progress(D, linear);
        assert!((0.0..1.0).contains(&t));
    }

    #[test]
    fn closing_midway_reverses_from_the_current_progress_without_a_jump() {
        let mut r = Reveal::closed();
        r.open(D);
        std::thread::sleep(Duration::from_millis(60));
        let before = r.progress(D, linear);
        assert!(r.close(D));
        let after = r.progress(D, linear);
        assert!(
            (before - after).abs() < 0.05,
            "reversal must continue from where it was: {before} vs {after}"
        );
        assert!(!r.is_open());
        assert!(r.is_visible(D), "still fading out");
    }

    #[test]
    fn zero_duration_collapses_every_transition_to_instant() {
        let mut r = Reveal::closed();
        r.open(Duration::ZERO);
        assert!((r.progress(Duration::ZERO, linear) - 1.0).abs() < f64::EPSILON);
        assert!(!r.is_animating(Duration::ZERO));
        r.close(Duration::ZERO);
        assert!(!r.is_visible(Duration::ZERO));
    }

    #[test]
    fn settle_promotes_a_finished_transition_to_its_resting_phase() {
        let mut r = Reveal::closed();
        r.open(Duration::from_millis(5));
        std::thread::sleep(Duration::from_millis(15));
        r.settle(Duration::from_millis(5));
        assert!(r.is_open());
        assert!(!r.is_animating(Duration::from_millis(5)));
        assert!((r.linear(Duration::from_millis(5)) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_tween_retargets_from_its_current_value() {
        let mut t = Tween::at(0.0);
        assert!(t.retarget(1.0, D, linear));
        assert!(!t.retarget(1.0, D, linear), "same target is a no-op");
        std::thread::sleep(Duration::from_millis(60));
        let mid = t.value(D, linear);
        assert!(mid > 0.1 && mid < 0.9, "mid was {mid}");
        t.retarget(0.0, D, linear);
        let back = t.value(D, linear);
        assert!(
            (back - mid).abs() < 0.05,
            "no pop on reversal: {mid} vs {back}"
        );
        assert!(t.is_animating(D));
    }

    #[test]
    fn a_tween_at_rest_or_at_zero_duration_reports_its_target() {
        let mut t = Tween::at(0.3);
        assert!(!t.is_animating(D));
        t.retarget(0.8, Duration::ZERO, linear);
        assert!((t.value(Duration::ZERO, linear) - 0.8).abs() < f64::EPSILON);
        t.jump(0.1);
        assert!((t.value(D, linear) - 0.1).abs() < f64::EPSILON);
        assert!(!t.is_animating(D));
    }
}

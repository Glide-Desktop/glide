use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct HoverAnimation<T> {
    target: Option<T>,
    started: Option<Instant>,
    rise_from: f64,
    outgoing: Option<(T, Instant, f64)>,
}

impl<T> Default for HoverAnimation<T> {
    fn default() -> Self {
        Self { target: None, started: None, rise_from: 0.0, outgoing: None }
    }
}

use crate::motion::fraction;

impl<T: Copy + PartialEq> HoverAnimation<T> {
    #[must_use]
    pub fn target(&self) -> Option<T> {
        self.target
    }

    pub fn set(&mut self, target: Option<T>) -> bool {
        if self.target == target {
            return false;
        }
        self.target = target;
        self.started = target.map(|_| Instant::now());
        self.rise_from = 0.0;
        self.outgoing = None;
        true
    }

    pub fn set_with(&mut self, target: Option<T>, duration: Duration) -> bool {
        if self.target == target {
            return false;
        }
        let now = Instant::now();
        let resumed = match (target, self.outgoing) {
            (Some(next), Some((item, _, _))) if item == next => self.level(next, duration),
            _ => 0.0,
        };
        if let Some(old) = self.target {
            self.outgoing = Some((old, now, self.level(old, duration)));
        } else if target.is_some_and(|next| self.outgoing.is_some_and(|(item, _, _)| item == next)) {
            self.outgoing = None;
        }
        self.target = target;
        self.started = target.map(|_| now);
        self.rise_from = resumed;
        true
    }

    pub fn retarget_without_restarting(&mut self, target: Option<T>) {
        self.target = target;
    }

    #[must_use]
    pub fn progress(&self, duration: Duration) -> f64 {
        let Some(started) = self.started else {
            return 1.0;
        };
        let t = fraction(started, duration);
        self.rise_from + (1.0 - self.rise_from) * t
    }

    #[must_use]
    pub fn level(&self, item: T, duration: Duration) -> f64 {
        if self.target == Some(item) {
            return self.progress(duration);
        }
        match self.outgoing {
            Some((old, left, from)) if old == item => from * (1.0 - fraction(left, duration)),
            _ => 0.0,
        }
    }

    #[must_use]
    pub fn is_animating(&self, duration: Duration) -> bool {
        let rising = self.target.is_some() && self.progress(duration) < 1.0;
        let falling = self.outgoing.is_some_and(|(_, left, from)| from > 0.0 && fraction(left, duration) < 1.0);
        rising || falling
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Duration = Duration::from_millis(100);

    #[test]
    fn a_fresh_animation_has_no_target_and_is_settled() {
        let a: HoverAnimation<u8> = HoverAnimation::default();
        assert_eq!(a.target(), None);
        assert!((a.progress(D) - 1.0).abs() < f64::EPSILON);
        assert!(!a.is_animating(D));
    }

    #[test]
    fn setting_the_same_target_twice_is_a_no_op() {
        let mut a = HoverAnimation::default();
        assert!(a.set(Some(1)));
        assert!(!a.set(Some(1)), "same target again must report no change");
    }

    #[test]
    fn setting_a_new_target_restarts_the_fade_below_full_progress() {
        let mut a = HoverAnimation::default();
        a.set(Some(1));
        let t = a.progress(D);
        assert!((0.0..1.0).contains(&t), "a freshly (re)targeted animation must not already read as settled, got {t}");
        assert!(a.is_animating(D));
    }

    #[test]
    fn progress_settles_to_one_after_the_duration_elapses() {
        let mut a = HoverAnimation::default();
        a.set(Some(1));
        std::thread::sleep(Duration::from_millis(30));
        assert!((a.progress(Duration::from_millis(10)) - 1.0).abs() < f64::EPSILON);
        assert!(!a.is_animating(Duration::from_millis(10)));
    }

    const SHORT: Duration = Duration::from_millis(40);

    fn settled_on_one() -> HoverAnimation<u8> {
        let mut a = HoverAnimation::default();
        a.set_with(Some(1), SHORT);
        std::thread::sleep(Duration::from_millis(60));
        assert!((a.level(1, SHORT) - 1.0).abs() < f64::EPSILON);
        a
    }

    #[test]
    fn switching_targets_rises_the_new_one_while_the_old_one_fades() {
        let mut a = settled_on_one();
        assert!(a.set_with(Some(2), SHORT));
        assert!(a.level(2, SHORT) < 0.5, "the new target starts low");
        assert!(a.level(1, SHORT) > 0.5, "the old target fades out, it does not vanish");
        assert!(a.is_animating(SHORT));
    }

    #[test]
    fn clearing_the_target_fades_the_old_one_out() {
        let mut a = settled_on_one();
        assert!(a.set_with(None, SHORT));
        assert!(a.level(1, SHORT) > 0.5);
        assert!(a.is_animating(SHORT));
    }

    #[test]
    fn returning_to_a_fading_target_resumes_from_its_level() {
        let mut a = settled_on_one();
        a.set_with(None, D);
        let before = a.level(1, D);
        a.set_with(Some(1), D);
        assert!(a.level(1, D) >= before - 0.05, "coming back must not flash from zero: {before} -> {}", a.level(1, D));
    }

    #[test]
    fn crossing_a_gap_into_another_target_keeps_the_old_fade() {
        let mut a = settled_on_one();
        a.set_with(None, SHORT);
        a.set_with(Some(2), SHORT);
        assert!(a.level(1, SHORT) > 0.5, "the target left before the gap keeps fading out");
    }

    #[test]
    fn plain_set_keeps_the_old_snap_semantics() {
        let mut a = HoverAnimation::default();
        a.set(Some(1));
        assert!(a.set(None));
        assert!(!a.is_animating(D));
        assert!(a.level(1, D).abs() < f64::EPSILON, "plain set snaps the old target out without a fade");
    }

    #[test]
    fn zero_duration_is_immediately_settled() {
        let mut a = HoverAnimation::default();
        a.set_with(Some(1), Duration::ZERO);
        a.set_with(Some(2), Duration::ZERO);
        assert!((a.progress(Duration::ZERO) - 1.0).abs() < f64::EPSILON);
        assert!(a.level(1, Duration::ZERO).abs() < f64::EPSILON);
        assert!(!a.is_animating(Duration::ZERO));
    }

    #[test]
    fn retarget_without_restarting_changes_the_target_but_not_the_fade() {
        let mut a = HoverAnimation::default();
        a.set(Some(1));
        std::thread::sleep(Duration::from_millis(30));
        assert!((a.progress(Duration::from_millis(10)) - 1.0).abs() < f64::EPSILON);

        a.retarget_without_restarting(Some(2));
        assert_eq!(a.target(), Some(2));
        assert!(
            (a.progress(Duration::from_millis(10)) - 1.0).abs() < f64::EPSILON,
            "reassigning the target this way must not read as a fresh hover"
        );
    }

    #[test]
    fn retarget_without_restarting_to_none_clears_the_target_but_leaves_the_clock_running() {
        let mut a = HoverAnimation::default();
        a.set(Some(1));
        a.retarget_without_restarting(None);
        assert_eq!(a.target(), None);
        assert!(!a.is_animating(D), "no target means not animating, regardless of the leftover clock");
    }
}

use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct HoverAnimation<T> {
    target: Option<T>,
    started: Option<Instant>,
}

impl<T> Default for HoverAnimation<T> {
    fn default() -> Self {
        Self {
            target: None,
            started: None,
        }
    }
}

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
        if duration.is_zero() {
            return 1.0;
        }
        (started.elapsed().as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn is_animating(&self, duration: Duration) -> bool {
        self.target.is_some() && self.progress(duration) < 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_animation_has_no_target_and_is_settled() {
        let a: HoverAnimation<u8> = HoverAnimation::default();
        assert_eq!(a.target(), None);
        assert!((a.progress(Duration::from_millis(100)) - 1.0).abs() < f64::EPSILON);
        assert!(!a.is_animating(Duration::from_millis(100)));
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
        let t = a.progress(Duration::from_millis(100));
        assert!(
            (0.0..1.0).contains(&t),
            "a freshly (re)targeted animation must not already read as settled, got {t}"
        );
        assert!(a.is_animating(Duration::from_millis(100)));
    }

    #[test]
    fn progress_settles_to_one_after_the_duration_elapses() {
        let mut a = HoverAnimation::default();
        a.set(Some(1));
        std::thread::sleep(Duration::from_millis(30));
        assert!((a.progress(Duration::from_millis(10)) - 1.0).abs() < f64::EPSILON);
        assert!(!a.is_animating(Duration::from_millis(10)));
    }

    #[test]
    fn switching_targets_restarts_with_no_crossfade() {
        let mut a = HoverAnimation::default();
        a.set(Some(1));
        std::thread::sleep(Duration::from_millis(30));
        assert!((a.progress(Duration::from_millis(10)) - 1.0).abs() < f64::EPSILON);

        assert!(a.set(Some(2)));
        let t = a.progress(Duration::from_millis(100));
        assert!(
            (0.0..1.0).contains(&t),
            "a different target must restart the fade, got {t}"
        );
    }

    #[test]
    fn clearing_the_target_stops_the_animation_immediately() {
        let mut a = HoverAnimation::default();
        a.set(Some(1));
        assert!(a.is_animating(Duration::from_millis(100)));

        assert!(a.set(None));
        assert!((a.progress(Duration::from_millis(100)) - 1.0).abs() < f64::EPSILON);
        assert!(!a.is_animating(Duration::from_millis(100)));
    }

    #[test]
    fn zero_duration_is_immediately_settled() {
        let mut a = HoverAnimation::default();
        a.set(Some(1));
        assert!((a.progress(Duration::ZERO) - 1.0).abs() < f64::EPSILON);
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
        assert!(
            !a.is_animating(Duration::from_millis(100)),
            "no target means not animating, regardless of the leftover clock"
        );
    }
}

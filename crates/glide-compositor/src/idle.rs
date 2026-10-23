use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Blank,
    Lock,
    Suspend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub blank: Option<Duration>,
    pub lock: Option<Duration>,
    pub suspend: Option<Duration>,
    pub suspend_on_battery: Option<Duration>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            blank: Some(Duration::from_mins(10)),
            lock: Some(Duration::ZERO),
            suspend: None,
            suspend_on_battery: None,
        }
    }
}

impl Policy {
    const fn suspend_after(&self, on_battery: bool) -> Option<Duration> {
        if on_battery { self.suspend_on_battery } else { self.suspend }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Idle {
    pub policy: Policy,
    since: Instant,
    blanked_at: Option<Instant>,
    locked: bool,
    suspended: bool,
}

impl Idle {
    #[must_use]
    pub fn new(now: Instant) -> Self {
        Self { policy: Policy::default(), since: now, blanked_at: None, locked: false, suspended: false }
    }

    pub fn activity(&mut self, now: Instant) {
        *self = Self { policy: self.policy, ..Self::new(now) };
    }

    #[must_use]
    pub const fn is_idle(&self) -> bool {
        self.blanked_at.is_some()
    }

    pub fn poll(&mut self, now: Instant, inhibited: bool, on_battery: bool) -> [Option<Step>; 3] {
        if inhibited && self.blanked_at.is_none() {
            self.since = now;
        }
        let idle = now.saturating_duration_since(self.since);
        let passed = |after: Option<Duration>| !inhibited && after.is_some_and(|after| idle >= after);

        let blank = self.blanked_at.is_none() && passed(self.policy.blank);
        if blank {
            self.blanked_at = Some(now);
        }
        let lock = !self.locked
            && self
                .blanked_at
                .zip(self.policy.lock)
                .is_some_and(|(at, delay)| now.saturating_duration_since(at) >= delay);
        self.locked |= lock;
        let suspend = !self.suspended && passed(self.policy.suspend_after(on_battery));
        self.suspended |= suspend;

        [blank.then_some(Step::Blank), lock.then_some(Step::Lock), suspend.then_some(Step::Suspend)]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contact<S> {
    Button(u32),
    Touch(S),
}

#[derive(Debug)]
pub struct WakeGuard<S> {
    held: Vec<Contact<S>>,
}

impl<S> Default for WakeGuard<S> {
    fn default() -> Self {
        Self { held: Vec::new() }
    }
}

impl<S: PartialEq> WakeGuard<S> {
    pub fn press(&mut self, waking: bool, contact: Contact<S>) -> bool {
        if waking {
            self.held.push(contact);
        }
        waking
    }

    #[must_use]
    pub fn follows(&self, contact: &Contact<S>) -> bool {
        self.held.contains(contact)
    }

    pub fn release(&mut self, contact: &Contact<S>) -> bool {
        let before = self.held.len();
        self.held.retain(|held| held != contact);
        self.held.len() != before
    }

    pub fn cancel_touches(&mut self) {
        self.held.retain(|held| matches!(held, Contact::Button(_)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_mins(1);

    #[test]
    fn the_click_that_wakes_the_screen_is_swallowed_with_its_release() {
        let mut guard = WakeGuard::<u8>::default();
        assert!(guard.press(true, Contact::Button(0x110)));
        assert!(!guard.press(false, Contact::Button(0x111)), "a click on a lit screen goes through");
        assert!(!guard.release(&Contact::Button(0x111)));
        assert!(guard.release(&Contact::Button(0x110)));
        assert!(!guard.release(&Contact::Button(0x110)), "only the one release is swallowed");
    }

    #[test]
    fn a_waking_touch_is_swallowed_until_it_lifts_or_is_cancelled() {
        let mut guard = WakeGuard::<u8>::default();
        assert!(guard.press(true, Contact::Touch(0)));
        assert!(guard.follows(&Contact::Touch(0)));
        assert!(!guard.follows(&Contact::Touch(1)));
        assert!(guard.release(&Contact::Touch(0)));
        assert!(!guard.follows(&Contact::Touch(0)));
        guard.press(true, Contact::Touch(2));
        guard.press(true, Contact::Button(0x110));
        guard.cancel_touches();
        assert!(!guard.follows(&Contact::Touch(2)));
        assert!(guard.follows(&Contact::Button(0x110)));
    }

    fn steps(idle: &mut Idle, now: Instant, inhibited: bool, on_battery: bool) -> Vec<Step> {
        idle.poll(now, inhibited, on_battery).into_iter().flatten().collect()
    }

    fn idle_with(policy: Policy) -> (Idle, Instant) {
        let start = Instant::now();
        let mut idle = Idle::new(start);
        idle.policy = policy;
        (idle, start)
    }

    #[test]
    fn by_default_the_screen_blanks_and_locks_together_after_ten_minutes() {
        let (mut idle, start) = idle_with(Policy::default());
        assert!(steps(&mut idle, start + 9 * MINUTE, false, false).is_empty());
        assert!(!idle.is_idle());
        assert_eq!(steps(&mut idle, start + 10 * MINUTE, false, false), [Step::Blank, Step::Lock]);
        assert!(idle.is_idle());
        assert!(steps(&mut idle, start + 11 * MINUTE, false, false).is_empty(), "each step fires once per idle period");
    }

    #[test]
    fn a_lock_delay_runs_from_the_moment_the_screen_went_off() {
        let policy = Policy { lock: Some(2 * MINUTE), ..Policy::default() };
        let (mut idle, start) = idle_with(policy);
        assert_eq!(steps(&mut idle, start + 10 * MINUTE, false, false), [Step::Blank]);
        assert!(steps(&mut idle, start + 11 * MINUTE, false, false).is_empty());
        assert_eq!(steps(&mut idle, start + 12 * MINUTE, false, false), [Step::Lock]);
    }

    #[test]
    fn an_inhibitor_that_appears_after_the_blank_cannot_hold_back_the_lock() {
        let policy = Policy { lock: Some(2 * MINUTE), ..Policy::default() };
        let (mut idle, start) = idle_with(policy);
        assert_eq!(steps(&mut idle, start + 10 * MINUTE, false, false), [Step::Blank]);
        assert_eq!(steps(&mut idle, start + 12 * MINUTE, true, false), [Step::Lock]);
    }

    #[test]
    fn locking_can_be_switched_off_and_blanking_too() {
        let (mut idle, start) = idle_with(Policy { lock: None, ..Policy::default() });
        assert_eq!(steps(&mut idle, start + 60 * MINUTE, false, false), [Step::Blank]);
        let (mut idle, start) = idle_with(Policy { blank: None, ..Policy::default() });
        assert!(steps(&mut idle, start + 60 * MINUTE, false, false).is_empty(), "no blank means no idle lock either");
    }

    #[test]
    fn an_inhibitor_counts_as_activity_so_the_full_timeout_runs_after_it_ends() {
        let (mut idle, start) = idle_with(Policy::default());
        assert!(steps(&mut idle, start + 30 * MINUTE, true, false).is_empty());
        assert!(steps(&mut idle, start + 39 * MINUTE, false, false).is_empty());
        assert_eq!(steps(&mut idle, start + 40 * MINUTE, false, false), [Step::Blank, Step::Lock]);
    }

    #[test]
    fn activity_starts_a_fresh_idle_period_and_keeps_the_policy() {
        let policy = Policy { lock: None, ..Policy::default() };
        let (mut idle, start) = idle_with(policy);
        assert_eq!(steps(&mut idle, start + 10 * MINUTE, false, false), [Step::Blank]);
        idle.activity(start + 11 * MINUTE);
        assert!(!idle.is_idle());
        assert_eq!(idle.policy, policy);
        assert!(steps(&mut idle, start + 20 * MINUTE, false, false).is_empty());
        assert_eq!(steps(&mut idle, start + 21 * MINUTE, false, false), [Step::Blank]);
    }

    #[test]
    fn suspend_uses_the_battery_timeout_only_on_battery_and_respects_inhibitors() {
        let policy = Policy { suspend: Some(30 * MINUTE), suspend_on_battery: Some(5 * MINUTE), ..Policy::default() };
        let (mut idle, start) = idle_with(policy);
        assert_eq!(steps(&mut idle, start + 5 * MINUTE, false, true), [Step::Suspend], "before the blank, on battery");
        assert!(steps(&mut idle, start + 6 * MINUTE, false, true).is_empty());

        let (mut idle, start) = idle_with(policy);
        assert!(steps(&mut idle, start + 5 * MINUTE, false, false).is_empty());
        assert_eq!(steps(&mut idle, start + 10 * MINUTE, false, false), [Step::Blank, Step::Lock]);
        assert!(steps(&mut idle, start + 30 * MINUTE, true, false).is_empty(), "an inhibitor holds suspend back");
        assert_eq!(steps(&mut idle, start + 31 * MINUTE, false, false), [Step::Suspend]);
    }

    #[test]
    fn suspend_is_off_unless_configured() {
        let (mut idle, start) = idle_with(Policy::default());
        let all: Vec<Step> =
            (0..600).flat_map(|minute| steps(&mut idle, start + minute * MINUTE, false, true)).collect();
        assert_eq!(all, [Step::Blank, Step::Lock]);
    }
}

use std::time::{Duration, Instant};

const FALLBACK: Duration = Duration::from_nanos(16_666_667);

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameClock {
    refresh: Duration,
    last_vblank: Option<Instant>,
}

impl Default for FrameClock {
    fn default() -> Self {
        Self { refresh: FALLBACK, last_vblank: None }
    }
}

impl FrameClock {
    pub(crate) fn set_refresh(&mut self, refresh_mhz: i32) {
        self.refresh = u64::try_from(refresh_mhz)
            .ok()
            .filter(|mhz| *mhz > 0)
            .map_or(FALLBACK, |mhz| Duration::from_nanos(1_000_000_000_000 / mhz));
    }

    #[must_use]
    pub(crate) const fn last_vblank(&self) -> Option<Instant> {
        self.last_vblank
    }

    pub(crate) fn presented(&mut self, at: Instant) {
        self.last_vblank = Some(at);
    }

    #[must_use]
    pub(crate) fn next_vblank(&self, now: Instant) -> Instant {
        let Some(last) = self.last_vblank else {
            return now + self.refresh;
        };
        if now < last {
            return last;
        }
        let period = self.refresh.as_nanos().max(1);
        let periods = now.duration_since(last).as_nanos() / period + 1;
        let ahead = u64::try_from(periods.saturating_mul(period)).unwrap_or(u64::MAX);
        last + Duration::from_nanos(ahead)
    }

    #[must_use]
    pub(crate) fn target(&self, now: Instant) -> Instant {
        self.next_vblank(now).max(now + Duration::from_micros(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sixty() -> FrameClock {
        let mut clock = FrameClock::default();
        clock.set_refresh(60_000);
        clock
    }

    #[test]
    fn unknown_refresh_falls_back_to_60hz() {
        let mut clock = FrameClock::default();
        clock.set_refresh(0);
        assert_eq!(clock.refresh, FALLBACK);
        clock.set_refresh(-5);
        assert_eq!(clock.refresh, FALLBACK);
        clock.set_refresh(144_000);
        assert_eq!(clock.refresh, Duration::from_nanos(6_944_444));
    }

    #[test]
    fn targets_lie_on_the_vblank_grid() {
        let mut clock = sixty();
        let vblank = Instant::now();
        clock.presented(vblank);
        let period = clock.refresh;
        assert_eq!(clock.target(vblank + Duration::from_millis(1)), vblank + period);
        assert_eq!(clock.target(vblank + period / 2), vblank + period);
        assert_eq!(clock.target(vblank + period + Duration::from_millis(3)), vblank + period * 2);
    }

    #[test]
    fn after_a_no_damage_gap_the_target_skips_whole_periods() {
        let mut clock = sixty();
        let vblank = Instant::now();
        clock.presented(vblank);
        let period = clock.refresh;
        let target = clock.target(vblank + period * 7 + period / 3);
        assert_eq!(target, vblank + period * 8);
    }

    #[test]
    fn target_is_always_after_now() {
        let mut clock = sixty();
        let vblank = Instant::now();
        clock.presented(vblank);
        assert!(clock.target(vblank) > vblank, "a render started on the vblank itself is shown on the next one");
        let period = clock.refresh;
        assert_eq!(clock.target(vblank), vblank + period);
        for step in 0..50u32 {
            let now = vblank + Duration::from_micros(u64::from(step) * 997);
            assert!(clock.target(now) > now);
        }
    }

    #[test]
    fn targets_never_go_backwards() {
        let mut clock = sixty();
        let vblank = Instant::now();
        clock.presented(vblank);
        let mut last = clock.target(vblank);
        for step in 1..200u32 {
            let next = clock.target(vblank + Duration::from_micros(u64::from(step) * 431));
            assert!(next >= last);
            last = next;
        }
    }

    #[test]
    fn without_a_vblank_the_target_is_one_period_ahead() {
        let clock = sixty();
        let now = Instant::now();
        assert_eq!(clock.target(now), now + clock.refresh);
    }
}

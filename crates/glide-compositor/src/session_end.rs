use std::time::{Duration, Instant};

use crate::power::PowerAction;

pub const GRACE: Duration = Duration::from_secs(10);
pub const FORCE_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Perform,
    CloseWindows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Perform(PowerAction),
    Refused(PowerAction),
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    action: PowerAction,
    progress_at: Instant,
    windows: usize,
}

#[derive(Debug, Default)]
pub struct SessionEnd {
    pending: Option<Pending>,
    refused: Option<(PowerAction, Instant)>,
}

impl SessionEnd {
    pub fn request(&mut self, action: PowerAction, now: Instant, windows: usize) -> Request {
        let insisted = self.pending.take().is_some_and(|pending| pending.action == action);
        let refused_recently = self
            .refused
            .take()
            .is_some_and(|(refused, at)| refused == action && now.saturating_duration_since(at) <= FORCE_WINDOW);
        if windows == 0 || insisted || refused_recently {
            return Request::Perform;
        }
        self.pending = Some(Pending { action, progress_at: now, windows });
        Request::CloseWindows
    }

    #[must_use]
    pub const fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn poll(&mut self, now: Instant, windows: usize) -> Option<Outcome> {
        let pending = self.pending.as_mut()?;
        let action = pending.action;
        if windows == 0 {
            self.pending = None;
            return Some(Outcome::Perform(action));
        }
        if windows < pending.windows {
            pending.windows = windows;
            pending.progress_at = now;
        }
        if now.saturating_duration_since(pending.progress_at) < GRACE {
            return None;
        }
        self.pending = None;
        self.refused = Some((action, now));
        Some(Outcome::Refused(action))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn an_empty_session_ends_at_once() {
        let mut end = SessionEnd::default();
        assert_eq!(end.request(PowerAction::LogOut, Instant::now(), 0), Request::Perform);
        assert!(!end.is_pending());
    }

    #[test]
    fn windows_are_asked_to_close_and_the_session_ends_when_the_last_one_goes() {
        let mut end = SessionEnd::default();
        let start = Instant::now();
        assert_eq!(end.request(PowerAction::ShutDown, start, 3), Request::CloseWindows);
        assert_eq!(end.poll(start + SECOND, 3), None);
        assert_eq!(end.poll(start + 2 * SECOND, 1), None);
        assert_eq!(end.poll(start + 3 * SECOND, 0), Some(Outcome::Perform(PowerAction::ShutDown)));
        assert_eq!(end.poll(start + 4 * SECOND, 0), None, "it fires once");
    }

    #[test]
    fn an_app_that_keeps_its_window_refuses_the_session_end_after_the_grace_period() {
        let mut end = SessionEnd::default();
        let start = Instant::now();
        end.request(PowerAction::Restart, start, 2);
        assert_eq!(end.poll(start + 9 * SECOND, 2), None);
        assert_eq!(end.poll(start + 10 * SECOND, 2), Some(Outcome::Refused(PowerAction::Restart)));
        assert!(!end.is_pending());
    }

    #[test]
    fn every_window_that_closes_restarts_the_grace_period() {
        let mut end = SessionEnd::default();
        let start = Instant::now();
        end.request(PowerAction::LogOut, start, 3);
        assert_eq!(end.poll(start + 9 * SECOND, 2), None);
        assert_eq!(end.poll(start + 18 * SECOND, 2), None, "9 s since the last window closed");
        assert_eq!(end.poll(start + 19 * SECOND, 2), Some(Outcome::Refused(PowerAction::LogOut)));
    }

    #[test]
    fn asking_again_ends_the_session_anyway_but_only_for_the_same_action_and_soon_enough() {
        let mut end = SessionEnd::default();
        let start = Instant::now();
        end.request(PowerAction::LogOut, start, 1);
        assert_eq!(end.request(PowerAction::LogOut, start + SECOND, 1), Request::Perform, "insisting while waiting");

        end.request(PowerAction::LogOut, start, 1);
        end.poll(start + GRACE, 1);
        assert_eq!(end.request(PowerAction::ShutDown, start + GRACE + SECOND, 1), Request::CloseWindows);
        end.poll(start + 3 * GRACE, 1);
        assert_eq!(end.request(PowerAction::ShutDown, start + 3 * GRACE + SECOND, 1), Request::Perform);

        end.request(PowerAction::LogOut, start, 1);
        end.poll(start + GRACE, 1);
        let late = start + GRACE + FORCE_WINDOW + SECOND;
        assert_eq!(end.request(PowerAction::LogOut, late, 1), Request::CloseWindows, "the refusal has expired");
    }
}

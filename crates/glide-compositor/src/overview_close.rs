use std::time::{Duration, Instant};

use glide_layout::WindowId;

pub const GRACE: Duration = Duration::from_millis(700);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Live {
    pub id: WindowId,
    pub parent: Option<WindowId>,
    pub pid: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Request {
    window: WindowId,
    newest: WindowId,
    asked: Instant,
}

#[derive(Debug, Default)]
pub struct Watch {
    requests: Vec<Request>,
}

fn opened_by(window: &Live, asker: &Live) -> bool {
    window.parent == Some(asker.id) || (window.pid.is_some() && window.pid == asker.pid)
}

impl Watch {
    pub fn ask(&mut self, window: WindowId, newest: WindowId, now: Instant) {
        self.requests.retain(|request| request.window != window);
        self.requests.push(Request { window, newest, asked: now });
    }

    #[must_use]
    pub fn is_waiting(&self) -> bool {
        !self.requests.is_empty()
    }

    pub fn poll(&mut self, now: Instant, live: &[Live]) -> Option<WindowId> {
        self.requests.retain(|request| live.iter().any(|window| window.id == request.window));
        let refused = self.requests.iter().rev().find_map(|request| {
            let asker = live.iter().find(|window| window.id == request.window)?;
            let prompt = live
                .iter()
                .filter(|window| window.id > request.newest && opened_by(window, asker))
                .map(|window| window.id)
                .max();
            prompt.or_else(|| (now.saturating_duration_since(request.asked) >= GRACE).then_some(request.window))
        });
        if refused.is_some() {
            self.requests.clear();
        }
        refused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u64, parent: Option<u64>, pid: Option<i32>) -> Live {
        Live { id: WindowId(id), parent: parent.map(WindowId), pid }
    }

    #[test]
    fn a_window_that_closes_in_time_is_forgotten() {
        let (mut watch, now) = (Watch::default(), Instant::now());
        watch.ask(WindowId(1), WindowId(2), now);
        let live = [window(1, None, Some(10)), window(2, None, Some(20))];
        assert_eq!(watch.poll(now + GRACE / 2, &live), None);
        assert!(watch.is_waiting());
        assert_eq!(watch.poll(now + GRACE * 2, &live[1..]), None);
        assert!(!watch.is_waiting());
    }

    #[test]
    fn a_window_still_open_after_the_grace_is_the_one_to_focus() {
        let (mut watch, now) = (Watch::default(), Instant::now());
        watch.ask(WindowId(1), WindowId(1), now);
        let live = [window(1, None, Some(10))];
        assert_eq!(watch.poll(now + GRACE.mul_f64(0.99), &live), None);
        assert_eq!(watch.poll(now + GRACE, &live), Some(WindowId(1)));
        assert!(!watch.is_waiting());
        assert_eq!(watch.poll(now + GRACE * 2, &live), None);
    }

    #[test]
    fn a_dialog_the_window_opens_is_focused_at_once() {
        let (mut watch, now) = (Watch::default(), Instant::now());
        watch.ask(WindowId(3), WindowId(4), now);
        let before = [window(3, None, Some(10)), window(4, Some(3), Some(10))];
        assert_eq!(watch.poll(now, &before), None, "a dialog that was already open is no answer");
        let after = [before[0], before[1], window(5, Some(3), None)];
        assert_eq!(watch.poll(now, &after), Some(WindowId(5)));
    }

    #[test]
    fn a_new_window_of_the_same_process_counts_and_a_stranger_does_not() {
        let (mut watch, now) = (Watch::default(), Instant::now());
        watch.ask(WindowId(1), WindowId(1), now);
        let stranger = [window(1, None, Some(10)), window(2, None, Some(20)), window(3, None, None)];
        assert_eq!(watch.poll(now, &stranger), None);
        let sibling = [stranger[0], stranger[1], window(4, None, Some(10))];
        assert_eq!(watch.poll(now, &sibling), Some(WindowId(4)));
    }

    #[test]
    fn the_latest_refusal_wins_and_settles_every_request() {
        let (mut watch, now) = (Watch::default(), Instant::now());
        watch.ask(WindowId(1), WindowId(2), now);
        watch.ask(WindowId(2), WindowId(2), now + Duration::from_millis(10));
        let live = [window(1, None, Some(10)), window(2, None, Some(20))];
        assert_eq!(watch.poll(now + GRACE * 2, &live), Some(WindowId(2)));
        assert!(!watch.is_waiting());
    }
}

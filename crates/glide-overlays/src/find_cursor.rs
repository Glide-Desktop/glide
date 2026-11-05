use std::collections::VecDeque;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(1);
const LINGER: Duration = Duration::from_secs(1);
const REST: Duration = Duration::from_millis(600);
const LIMIT: Duration = Duration::from_millis(2500);
const SHAKE_RATIO: f64 = 3.0;
const MIN_SPAN: f64 = 80.0;
pub const MAGNIFY: f64 = 3.0;

#[derive(Debug, Clone, Copy)]
struct Track {
    from: f64,
    to: f64,
    started: Instant,
    duration: Duration,
    curve: fn(f64) -> f64,
}

impl Track {
    fn at(&self, now: Instant) -> f64 {
        if self.duration.is_zero() {
            return self.to;
        }
        let t = now.saturating_duration_since(self.started).as_secs_f64() / self.duration.as_secs_f64();
        if t >= 1.0 { self.to } else { glide_theme::motion::lerp(self.from, self.to, (self.curve)(t)) }
    }
}

#[derive(Debug)]
pub struct Finder {
    enabled: bool,
    position: (f64, f64),
    history: VecDeque<(Instant, (f64, f64))>,
    grow: Option<Track>,
    shook: Option<Instant>,
    release: Option<Instant>,
}

impl Default for Finder {
    fn default() -> Self {
        Self { enabled: true, position: (0.0, 0.0), history: VecDeque::new(), grow: None, shook: None, release: None }
    }
}

fn same_direction(a: f64, b: f64) -> bool {
    (a >= 0.0) == (b >= 0.0)
}

impl Finder {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.history.clear();
            self.grow = None;
            self.shook = None;
            self.release = None;
        }
    }

    pub fn interrupt(&mut self) {
        self.history.clear();
    }

    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn motion(&mut self, delta: (f64, f64), now: Instant) -> bool {
        let moved = delta.0.is_finite() && delta.1.is_finite() && (delta.0 != 0.0 || delta.1 != 0.0);
        if !self.enabled || !moved {
            return false;
        }
        self.position = (self.position.0 + delta.0, self.position.1 + delta.1);
        let turned = self.record(now);
        let held = self.release.is_some_and(|release| now < release);
        let triggered = !held && self.shaking();
        if triggered {
            let from = self.scale(now);
            self.grow = Some(Track {
                from,
                to: MAGNIFY,
                started: now,
                duration: glide_theme::motion::t3(),
                curve: glide_theme::motion::ease_over,
            });
        }
        if triggered || (held && turned && self.shaking()) {
            self.shook = Some(now);
        }
        if let Some(shook) = self.shook.filter(|_| triggered || held) {
            self.release = Some((shook + LINGER).max(now + REST).min(shook + LIMIT));
        }
        triggered
    }

    fn record(&mut self, now: Instant) -> bool {
        while self.history.front().is_some_and(|(at, _)| now.saturating_duration_since(*at) > WINDOW) {
            self.history.pop_front();
        }
        let position = self.position;
        let len = self.history.len();
        if len >= 2 {
            let (_, last) = self.history[len - 1];
            let (_, before) = self.history[len - 2];
            if same_direction(last.0 - before.0, position.0 - last.0)
                && same_direction(last.1 - before.1, position.1 - last.1)
            {
                self.history[len - 1] = (now, position);
                return false;
            }
        }
        self.history.push_back((now, position));
        true
    }

    fn shaking(&self) -> bool {
        let Some(&(_, first)) = self.history.front() else {
            return false;
        };
        let (mut left, mut right, mut top, mut bottom) = (first.0, first.0, first.1, first.1);
        let mut path = 0.0;
        let mut previous = first;
        for &(_, point) in self.history.iter().skip(1) {
            path += (point.0 - previous.0).hypot(point.1 - previous.1);
            left = left.min(point.0);
            right = right.max(point.0);
            top = top.min(point.1);
            bottom = bottom.max(point.1);
            previous = point;
        }
        let span = (right - left).hypot(bottom - top);
        span >= MIN_SPAN && path / span > SHAKE_RATIO
    }

    #[must_use]
    pub fn scale(&self, now: Instant) -> f64 {
        let Some(grow) = self.grow else {
            return 1.0;
        };
        match self.release {
            Some(release) if now >= release => Track {
                from: grow.at(release),
                to: 1.0,
                started: release,
                duration: glide_theme::motion::t3(),
                curve: glide_theme::motion::ease_out,
            }
            .at(now),
            _ => grow.at(now),
        }
    }

    #[must_use]
    pub fn is_magnified(&self, now: Instant) -> bool {
        self.scale(now) > 1.0 + 1e-3
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        let Some(release) = self.release else {
            return false;
        };
        if now >= release + glide_theme::motion::t3() {
            self.grow = None;
            self.shook = None;
            self.release = None;
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(finder: &mut Finder, start: Instant, strokes: usize, length: f64, every: Duration) -> (bool, Instant) {
        let steps = 10_u32;
        let mut now = start;
        let mut triggered = false;
        for stroke in 0..strokes {
            let sign = if stroke % 2 == 0 { 1.0 } else { -1.0 };
            for _ in 0..steps {
                now += every / steps;
                triggered |= finder.motion((sign * length / f64::from(steps), 1.0), now);
            }
        }
        (triggered, now)
    }

    #[test]
    fn shaking_back_and_forth_magnifies_the_pointer() {
        let mut finder = Finder::default();
        let start = Instant::now();
        let (triggered, now) = stroke(&mut finder, start, 6, 300.0, Duration::from_millis(120));
        assert!(triggered);
        assert!(finder.is_magnified(now + glide_theme::motion::t3()));
        let settled = finder.scale(now + glide_theme::motion::t3());
        assert!((settled - MAGNIFY).abs() < 1e-6, "{settled}");
    }

    #[test]
    fn a_long_straight_or_slow_move_does_not() {
        let mut finder = Finder::default();
        let start = Instant::now();
        let mut now = start;
        for _ in 0..200 {
            now += Duration::from_millis(4);
            assert!(!finder.motion((12.0, 3.0), now));
        }
        let (triggered, _) = stroke(&mut finder, now, 6, 300.0, Duration::from_millis(400));
        assert!(!triggered, "a slow back and forth is ordinary work");
    }

    #[test]
    fn small_jitter_is_not_a_shake() {
        let mut finder = Finder::default();
        let (triggered, _) = stroke(&mut finder, Instant::now(), 12, 20.0, Duration::from_millis(40));
        assert!(!triggered);
    }

    #[test]
    fn a_short_quick_shake_is_enough() {
        let mut finder = Finder::default();
        let (triggered, _) = stroke(&mut finder, Instant::now(), 4, 200.0, Duration::from_millis(100));
        assert!(triggered, "four quick strokes find the pointer");
    }

    #[test]
    fn it_stays_big_for_a_while_after_the_shaking_stops_then_shrinks() {
        let mut finder = Finder::default();
        let (triggered, now) = stroke(&mut finder, Instant::now(), 6, 300.0, Duration::from_millis(120));
        assert!(triggered);
        let release = now + LINGER;
        let almost = now + LINGER.saturating_sub(Duration::from_millis(150));
        assert!(finder.is_magnified(almost), "a resting pointer stays big for about a second");
        assert!(finder.tick(release));
        let gone = release + glide_theme::motion::t3();
        assert!((finder.scale(gone) - 1.0).abs() < 1e-9);
        assert!(!finder.tick(gone));
        assert!(!finder.is_magnified(gone));
    }

    #[test]
    fn moving_on_after_a_shake_keeps_it_big_until_the_pointer_rests() {
        let mut finder = Finder::default();
        let (triggered, shook) = stroke(&mut finder, Instant::now(), 6, 300.0, Duration::from_millis(120));
        assert!(triggered);
        let mut now = shook;
        while now < shook + Duration::from_millis(1800) {
            now += Duration::from_millis(8);
            finder.motion((4.0, 1.0), now);
        }
        let almost = now + REST.saturating_sub(Duration::from_millis(1));
        assert!(finder.is_magnified(almost), "still big while it is being moved");
        let rested = now + REST + glide_theme::motion::t3();
        assert!(!finder.is_magnified(rested), "and back to normal once it rests");
    }

    #[test]
    fn moving_on_forever_does_not_keep_it_big_forever() {
        let mut finder = Finder::default();
        let (triggered, shook) = stroke(&mut finder, Instant::now(), 6, 300.0, Duration::from_millis(120));
        assert!(triggered);
        let mut now = shook;
        let end = shook + LIMIT + glide_theme::motion::t3() + Duration::from_millis(100);
        while now < end {
            now += Duration::from_millis(8);
            finder.motion((4.0, 1.0), now);
        }
        assert!(!finder.is_magnified(now));
    }

    #[test]
    fn shaking_on_keeps_it_big_without_growing_again() {
        let mut finder = Finder::default();
        let start = Instant::now();
        let (triggered, now) = stroke(&mut finder, start, 6, 300.0, Duration::from_millis(120));
        assert!(triggered);
        let (again, later) = stroke(&mut finder, now, 6, 300.0, Duration::from_millis(120));
        assert!(!again, "a shake that goes on only extends the hold");
        assert!(finder.is_magnified(later + LINGER / 2));
    }

    #[test]
    fn an_interrupted_shake_starts_over() {
        let mut finder = Finder::default();
        let start = Instant::now();
        let (_, now) = stroke(&mut finder, start, 2, 300.0, Duration::from_millis(100));
        finder.interrupt();
        let (triggered, _) = stroke(&mut finder, now, 2, 300.0, Duration::from_millis(100));
        assert!(!triggered, "strokes made while a button was held do not count");
    }

    #[test]
    fn switched_off_it_never_magnifies() {
        let mut finder = Finder::default();
        finder.set_enabled(false);
        let (triggered, now) = stroke(&mut finder, Instant::now(), 8, 300.0, Duration::from_millis(100));
        assert!(!triggered);
        assert!((finder.scale(now) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_track_without_duration_lands_at_once() {
        let now = Instant::now();
        let track = Track { from: 1.0, to: MAGNIFY, started: now, duration: Duration::ZERO, curve: |t| t };
        assert!((track.at(now) - MAGNIFY).abs() < f64::EPSILON);
    }
}

use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    Vblank,
    Estimated,
    Timer,
    Idle,
}

impl Origin {
    const fn index(self) -> usize {
        match self {
            Self::Vblank => 0,
            Self::Estimated => 1,
            Self::Timer => 2,
            Self::Idle => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Spread {
    pub p50: u32,
    pub p99: u32,
    pub max: u32,
}

#[must_use]
pub(crate) fn spread(values: &mut [u32]) -> Option<Spread> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let at = |fraction: f64| {
        let last = values.len() - 1;
        let index = ((last as f64) * fraction).round() as usize;
        values[index.min(last)]
    };
    Some(Spread { p50: at(0.5), p99: at(0.99), max: values[values.len() - 1] })
}

fn micros(duration: Duration) -> u32 {
    u32::try_from(duration.as_micros()).unwrap_or(u32::MAX)
}

#[derive(Debug, Default)]
pub(crate) struct FrameStats {
    since: Option<Instant>,
    build_us: Vec<u32>,
    draw_us: Vec<u32>,
    late_us: Vec<u32>,
    origins: [u32; 4],
    no_damage: u32,
    flip_blocked: u32,
    presented: u32,
    missed: u32,
    gamma_calls: u32,
    gamma: Duration,
    last_sequence: Option<u32>,
    chained: bool,
}

impl FrameStats {
    fn open(&mut self, now: Instant) {
        self.since.get_or_insert(now);
    }

    pub(crate) fn rendered(
        &mut self, origin: Origin, late: Option<Duration>, build: Duration, draw: Duration, now: Instant,
    ) {
        self.open(now);
        self.origins[origin.index()] += 1;
        if let Some(late) = late {
            self.late_us.push(micros(late));
        }
        self.build_us.push(micros(build));
        self.draw_us.push(micros(draw));
    }

    pub(crate) fn no_damage(&mut self) {
        self.no_damage += 1;
    }

    pub(crate) fn flip_blocked(&mut self) {
        self.flip_blocked += 1;
    }

    pub(crate) fn queued(&mut self, origin: Origin) {
        self.chained = origin == Origin::Vblank;
    }

    pub(crate) fn presented(&mut self, sequence: u32) {
        self.presented += 1;
        if self.chained
            && let Some(last) = self.last_sequence
            && sequence.wrapping_sub(last) > 1
        {
            self.missed += 1;
        }
        self.last_sequence = Some(sequence);
    }

    pub(crate) fn gamma(&mut self, took: Duration, now: Instant) {
        self.open(now);
        self.gamma_calls += 1;
        self.gamma += took;
    }

    pub(crate) fn report(&mut self, output: &str, now: Instant) {
        let Some(since) = self.since else {
            return;
        };
        if now.saturating_duration_since(since) < WINDOW {
            return;
        }
        let frames = self.build_us.len();
        let build = spread(&mut self.build_us);
        let draw = spread(&mut self.draw_us);
        let late = spread(&mut self.late_us);
        tracing::info!(
            target: "glide::frame_stats",
            output,
            frames,
            presented = self.presented,
            missed = self.missed,
            no_damage = self.no_damage,
            flip_blocked = self.flip_blocked,
            vblank = self.origins[Origin::Vblank.index()],
            estimated = self.origins[Origin::Estimated.index()],
            timer = self.origins[Origin::Timer.index()],
            idle = self.origins[Origin::Idle.index()],
            build_us = ?build,
            draw_us = ?draw,
            late_us = ?late,
            gamma_calls = self.gamma_calls,
            gamma_us = micros(self.gamma),
            "frame stats"
        );
        let last_sequence = self.last_sequence;
        let chained = self.chained;
        *self = Self { last_sequence, chained, ..Self::default() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spread_of_nothing_is_none() {
        assert_eq!(spread(&mut []), None);
    }

    #[test]
    fn spread_reports_median_tail_and_maximum() {
        let mut values: Vec<u32> = (1..=100).rev().collect();
        assert_eq!(spread(&mut values), Some(Spread { p50: 51, p99: 99, max: 100 }));
    }

    #[test]
    fn a_skipped_vblank_after_a_vblank_render_counts_as_missed() {
        let mut stats = FrameStats::default();
        stats.queued(Origin::Vblank);
        stats.presented(10);
        stats.queued(Origin::Vblank);
        stats.presented(11);
        stats.queued(Origin::Vblank);
        stats.presented(13);
        assert_eq!((stats.presented, stats.missed), (3, 1));
    }

    #[test]
    fn a_gap_after_an_idle_render_is_not_missed() {
        let mut stats = FrameStats::default();
        stats.queued(Origin::Vblank);
        stats.presented(10);
        stats.queued(Origin::Idle);
        stats.presented(40);
        assert_eq!(stats.missed, 0);
    }

    #[test]
    fn a_report_starts_a_new_window_but_keeps_the_sequence() {
        let start = Instant::now();
        let mut stats = FrameStats::default();
        stats.rendered(Origin::Idle, None, Duration::ZERO, Duration::ZERO, start);
        stats.queued(Origin::Vblank);
        stats.presented(5);
        stats.report("test", start + WINDOW);
        assert!(stats.build_us.is_empty());
        assert_eq!(stats.last_sequence, Some(5));
        stats.presented(7);
        assert_eq!(stats.missed, 1);
    }

    #[test]
    fn a_report_waits_for_a_full_window() {
        let start = Instant::now();
        let mut stats = FrameStats::default();
        stats.rendered(Origin::Idle, None, Duration::ZERO, Duration::ZERO, start);
        stats.report("test", start + WINDOW / 2);
        assert_eq!(stats.build_us.len(), 1);
    }
}

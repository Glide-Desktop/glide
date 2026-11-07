use std::time::Duration;

use glide_video::export::{Segment, Transform};

pub const SHORTEST: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clip {
    pub start: Duration,
    pub end: Duration,
    pub transform: Transform,
}

impl Clip {
    #[must_use]
    pub fn length(&self) -> Duration {
        self.end.saturating_sub(self.start)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeline {
    pub clips: Vec<Clip>,
    pub selected: usize,
    source: Duration,
}

impl Timeline {
    #[must_use]
    pub fn whole(source: Duration) -> Self {
        Self {
            clips: vec![Clip { start: Duration::ZERO, end: source, transform: Transform::default() }],
            selected: 0,
            source,
        }
    }

    #[must_use]
    pub fn source(&self) -> Duration {
        self.source
    }

    #[must_use]
    pub fn total(&self) -> Duration {
        self.clips.iter().map(Clip::length).sum()
    }

    #[must_use]
    pub fn edited(&self) -> bool {
        self.clips.len() != 1
            || self.clips[0].start != Duration::ZERO
            || self.clips[0].end != self.source
            || !self.clips[0].transform.is_identity()
    }

    #[must_use]
    pub fn locate(&self, at: Duration) -> Option<(usize, Duration)> {
        let mut passed = Duration::ZERO;
        for (index, clip) in self.clips.iter().enumerate() {
            let length = clip.length();
            if at < passed + length || index + 1 == self.clips.len() {
                let inside = at.saturating_sub(passed).min(length);
                return Some((index, clip.start + inside));
            }
            passed += length;
        }
        None
    }

    #[must_use]
    pub fn offset_of(&self, index: usize) -> Duration {
        self.clips.iter().take(index).map(Clip::length).sum()
    }

    #[must_use]
    pub fn total_at(&self, index: usize, source: Duration) -> Duration {
        let Some(clip) = self.clips.get(index) else { return Duration::ZERO };
        self.offset_of(index) + source.clamp(clip.start, clip.end).saturating_sub(clip.start)
    }

    #[must_use]
    pub fn clip_of(&self, source: Duration, hint: usize) -> Option<usize> {
        let inside = |index: usize| self.clips.get(index).is_some_and(|clip| source >= clip.start && source < clip.end);
        if inside(hint) {
            return Some(hint);
        }
        (0..self.clips.len()).find(|index| inside(*index))
    }

    pub fn split(&mut self, at: Duration) -> bool {
        let Some((index, source)) = self.locate(at) else { return false };
        let clip = self.clips[index];
        if source < clip.start + SHORTEST || source + SHORTEST > clip.end {
            return false;
        }
        self.clips[index].end = source;
        self.clips.insert(index + 1, Clip { start: source, ..clip });
        self.selected = index + 1;
        true
    }

    pub fn remove(&mut self, index: usize) -> bool {
        if self.clips.len() < 2 || index >= self.clips.len() {
            return false;
        }
        self.clips.remove(index);
        self.selected = self.selected.min(self.clips.len() - 1);
        true
    }

    pub fn reorder(&mut self, from: usize, to: usize) -> bool {
        if from >= self.clips.len() || to >= self.clips.len() || from == to {
            return false;
        }
        let clip = self.clips.remove(from);
        self.clips.insert(to, clip);
        self.selected = to;
        true
    }

    pub fn trim(&mut self, start: Duration, end: Duration) {
        let (start, end) = (start.min(end), end.max(start));
        if end.saturating_sub(start) < SHORTEST {
            return;
        }
        let mut kept = Vec::new();
        let mut passed = Duration::ZERO;
        for clip in &self.clips {
            let length = clip.length();
            let (from, to) = (passed, passed + length);
            passed = to;
            let keep_from = start.max(from);
            let keep_to = end.min(to);
            if keep_to > keep_from && keep_to.saturating_sub(keep_from) >= SHORTEST.min(length) {
                kept.push(Clip {
                    start: clip.start + keep_from.saturating_sub(from),
                    end: clip.start + keep_to.saturating_sub(from),
                    transform: clip.transform,
                });
            }
        }
        if !kept.is_empty() {
            self.clips = kept;
            self.selected = self.selected.min(self.clips.len() - 1);
        }
    }

    pub fn turn(&mut self, clockwise: bool) {
        let all = self.clips.len() == 1;
        for (index, clip) in self.clips.iter_mut().enumerate() {
            if all || index == self.selected {
                clip.transform.turn =
                    if clockwise { clip.transform.turn.clockwise() } else { clip.transform.turn.counter() };
            }
        }
    }

    pub fn flip(&mut self, horizontal: bool) {
        let all = self.clips.len() == 1;
        for (index, clip) in self.clips.iter_mut().enumerate() {
            if all || index == self.selected {
                if horizontal {
                    clip.transform.flip_h = !clip.transform.flip_h;
                } else {
                    clip.transform.flip_v = !clip.transform.flip_v;
                }
            }
        }
    }

    #[must_use]
    pub fn segments(&self, uri: &str) -> Vec<Segment> {
        self.clips
            .iter()
            .map(|clip| Segment { uri: uri.to_string(), start: clip.start, end: clip.end, transform: clip.transform })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glide_video::export::Turn;

    fn secs(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    #[test]
    fn a_fresh_timeline_is_the_whole_file() {
        let timeline = Timeline::whole(secs(760));
        assert!(!timeline.edited());
        assert_eq!(timeline.total(), secs(760));
        assert_eq!(timeline.locate(secs(100)), Some((0, secs(100))));
    }

    #[test]
    fn trimming_keeps_only_the_chosen_span() {
        let mut timeline = Timeline::whole(secs(760));
        timeline.trim(secs(137), secs(426));
        assert!(timeline.edited());
        assert_eq!(timeline.clips, vec![Clip { start: secs(137), end: secs(426), transform: Transform::default() }]);
        assert_eq!(timeline.locate(Duration::ZERO), Some((0, secs(137))));
    }

    #[test]
    fn splitting_then_reordering_maps_time_through_the_clips() {
        let mut timeline = Timeline::whole(secs(100));
        assert!(timeline.split(secs(40)));
        assert_eq!(timeline.clips.len(), 2);
        assert!(timeline.reorder(1, 0));
        assert_eq!(timeline.locate(secs(10)), Some((0, secs(50))));
        assert_eq!(timeline.locate(secs(70)), Some((1, secs(10))));
        assert_eq!(timeline.total_at(1, secs(10)), secs(70));
        assert_eq!(timeline.total(), secs(100));
    }

    #[test]
    fn splits_too_close_to_an_edge_are_refused() {
        let mut timeline = Timeline::whole(secs(10));
        assert!(!timeline.split(Duration::from_millis(100)));
        assert!(!timeline.split(Duration::from_millis(9_900)));
        assert_eq!(timeline.clips.len(), 1);
    }

    #[test]
    fn trim_across_clips_cuts_each_one() {
        let mut timeline = Timeline::whole(secs(100));
        timeline.split(secs(50));
        timeline.trim(secs(30), secs(70));
        assert_eq!(timeline.clips.len(), 2);
        assert_eq!((timeline.clips[0].start, timeline.clips[0].end), (secs(30), secs(50)));
        assert_eq!((timeline.clips[1].start, timeline.clips[1].end), (secs(50), secs(70)));
    }

    #[test]
    fn turning_a_single_clip_turns_the_whole_movie_and_selected_clip_otherwise() {
        let mut timeline = Timeline::whole(secs(100));
        timeline.turn(true);
        assert_eq!(timeline.clips[0].transform.turn, Turn::Right);
        timeline.split(secs(50));
        timeline.selected = 0;
        timeline.turn(false);
        assert_eq!(timeline.clips[0].transform.turn, Turn::None);
        assert_eq!(timeline.clips[1].transform.turn, Turn::Right);
        timeline.flip(true);
        assert!(timeline.clips[0].transform.flip_h);
        assert!(!timeline.clips[1].transform.flip_h);
    }

    #[test]
    fn the_last_clip_cannot_be_removed() {
        let mut timeline = Timeline::whole(secs(10));
        assert!(!timeline.remove(0));
        timeline.split(secs(5));
        assert!(timeline.remove(0));
        assert_eq!(timeline.total(), secs(5));
    }

    #[test]
    fn source_time_finds_its_clip() {
        let mut timeline = Timeline::whole(secs(100));
        timeline.split(secs(40));
        assert_eq!(timeline.clip_of(secs(60), 0), Some(1));
        assert_eq!(timeline.clip_of(secs(10), 1), Some(0));
        timeline.trim(secs(10), secs(90));
        assert_eq!(timeline.clip_of(secs(5), 0), None);
    }
}

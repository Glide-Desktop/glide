use std::collections::VecDeque;
use std::time::Instant;

const LIMIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Accent,
    DockAutohide,
}

impl Setting {
    #[must_use]
    pub const fn key(self) -> (&'static str, &'static str) {
        match self {
            Self::Accent => ("topbar", "accent"),
            Self::DockAutohide => ("dock", "autohide"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    id: u64,
    pub setting: Setting,
    pub before: Option<String>,
    pub after: Option<String>,
    saved_at: Instant,
}

impl Change {
    #[must_use]
    pub fn age_seconds(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.saved_at).as_secs()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Undo {
    Ready(Change),
    Conflict,
    Missing,
}

#[derive(Debug, Default, Clone)]
pub struct History {
    entries: VecDeque<Change>,
    next_id: u64,
}

impl History {
    pub fn record_saved(&mut self, setting: Setting, before: Option<String>, after: Option<String>) {
        if before == after {
            return;
        }
        self.next_id = self.next_id.wrapping_add(1);
        self.entries.push_front(Change { id: self.next_id, setting, before, after, saved_at: Instant::now() });
        self.entries.truncate(LIMIT);
    }

    #[must_use]
    pub fn entries(&self) -> &VecDeque<Change> {
        &self.entries
    }

    pub fn discard_setting(&mut self, setting: Setting) {
        self.entries.retain(|change| change.setting != setting);
    }

    #[must_use]
    pub fn prepare_undo(&self, index: usize, current: Option<&str>) -> Undo {
        let Some(change) = self.entries.get(index) else { return Undo::Missing };
        if change.after.as_deref() != current
            || self.entries.iter().take(index).any(|newer| newer.setting == change.setting)
        {
            return Undo::Conflict;
        }
        Undo::Ready(change.clone())
    }

    pub fn acknowledge_undo(&mut self, change: &Change) -> bool {
        let Some(index) = self.entries.iter().position(|entry| entry.id == change.id && entry == change) else {
            return false;
        };
        if self.entries.iter().take(index).any(|newer| newer.setting == change.setting) {
            return false;
        }
        self.entries.remove(index);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_requires_the_recorded_value_and_acknowledges_only_after_success() {
        let mut history = History::default();
        history.record_saved(Setting::Accent, Some("blue".into()), Some("teal".into()));
        assert_eq!(history.prepare_undo(0, Some("purple")), Undo::Conflict);
        assert_eq!(history.entries().len(), 1);
        let Undo::Ready(change) = history.prepare_undo(0, Some("teal")) else { panic!("matching value") };
        assert_eq!(change.before.as_deref(), Some("blue"));
        assert_eq!(history.entries().len(), 1);
        assert!(history.acknowledge_undo(&change));
        assert!(!history.acknowledge_undo(&change));
        assert_eq!(history.prepare_undo(0, Some("teal")), Undo::Missing);
    }

    #[test]
    fn absence_is_distinct_from_a_stored_default() {
        let mut history = History::default();
        history.record_saved(Setting::DockAutohide, None, Some("smart".into()));
        assert_eq!(history.prepare_undo(0, None), Undo::Conflict);
        let Undo::Ready(change) = history.prepare_undo(0, Some("smart")) else { panic!("matching value") };
        assert_eq!(change.before, None);
        history.record_saved(Setting::Accent, Some("blue".into()), None);
        assert!(matches!(history.prepare_undo(0, None), Undo::Ready(_)));
    }

    #[test]
    fn undo_requires_latest_change_of_each_setting_even_when_values_repeat() {
        let mut history = History::default();
        history.record_saved(Setting::Accent, Some("blue".into()), Some("teal".into()));
        let Undo::Ready(stale) = history.prepare_undo(0, Some("teal")) else { panic!("matching value") };
        history.record_saved(Setting::Accent, Some("teal".into()), Some("blue".into()));
        history.record_saved(Setting::Accent, Some("blue".into()), Some("teal".into()));
        history.record_saved(Setting::DockAutohide, None, Some("always".into()));
        assert_eq!(history.prepare_undo(3, Some("teal")), Undo::Conflict);
        assert!(!history.acknowledge_undo(&stale));
        let Undo::Ready(latest) = history.prepare_undo(1, Some("teal")) else { panic!("latest accent") };
        assert!(history.acknowledge_undo(&latest));
        assert!(matches!(history.prepare_undo(1, Some("blue")), Undo::Ready(_)));
        assert!(matches!(history.prepare_undo(0, Some("always")), Undo::Ready(_)));
    }

    #[test]
    fn records_are_bounded_newest_first_and_noops_are_ignored() {
        let mut history = History::default();
        history.record_saved(Setting::Accent, Some("blue".into()), Some("blue".into()));
        assert!(history.entries().is_empty());
        for index in 0..70 {
            history.record_saved(Setting::Accent, Some(index.to_string()), Some((index + 1).to_string()));
        }
        assert_eq!(history.entries().len(), LIMIT);
        assert_eq!(history.entries()[0].after.as_deref(), Some("70"));
        assert_eq!(history.entries().back().unwrap().before.as_deref(), Some("6"));
    }

    #[test]
    fn age_uses_monotonic_save_time_and_never_underflows() {
        let mut history = History::default();
        let before = Instant::now();
        history.record_saved(Setting::Accent, None, Some("teal".into()));
        let change = &history.entries()[0];
        assert_eq!(change.age_seconds(before), 0);
        assert_eq!(change.age_seconds(change.saved_at + std::time::Duration::from_secs(125)), 125);
    }

    #[test]
    fn discarding_a_setting_preserves_other_history() {
        let mut history = History::default();
        history.record_saved(Setting::Accent, None, Some("blue".into()));
        history.record_saved(Setting::DockAutohide, None, Some("always".into()));
        history.discard_setting(Setting::Accent);
        assert_eq!(history.entries().len(), 1);
        assert_eq!(history.entries()[0].setting, Setting::DockAutohide);
    }
}

use std::collections::VecDeque;

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Undo {
    Ready(Change),
    Conflict,
    Missing,
}

#[derive(Debug, Default)]
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
        self.entries.push_front(Change { id: self.next_id, setting, before, after });
        self.entries.truncate(LIMIT);
    }

    #[must_use]
    pub fn entries(&self) -> &VecDeque<Change> {
        &self.entries
    }

    #[must_use]
    pub fn prepare_undo(&self, index: usize, current: Option<&str>) -> Undo {
        let Some(change) = self.entries.get(index) else { return Undo::Missing };
        if change.after.as_deref() != current {
            return Undo::Conflict;
        }
        Undo::Ready(change.clone())
    }

    pub fn acknowledge_undo(&mut self, change: &Change) -> bool {
        let Some(index) = self.entries.iter().position(|entry| entry.id == change.id && entry == change) else {
            return false;
        };
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
}

use std::ops::Range;
use std::time::{Duration, Instant};

use iced::widget::text_editor::Cursor;

const MERGE_WINDOW: Duration = Duration::from_millis(1500);
const BUDGET: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Typing,
    Deleting,
    Other,
}

#[derive(Debug, Clone)]
pub struct Change {
    pub start: usize,
    pub removed: String,
    pub inserted: String,
    pub before: Cursor,
    pub after: Cursor,
    kind: Kind,
    at: Instant,
    id: u64,
}

impl Change {
    fn size(&self) -> usize {
        self.removed.len() + self.inserted.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    pub start: usize,
    pub old_end: usize,
    pub new_end: usize,
}

#[must_use]
pub fn diff(old: &str, new: &str) -> Option<Diff> {
    if old == new {
        return None;
    }
    let mut prefix = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let limit = old.len().min(new.len()) - prefix;
    let mut suffix = old.bytes().rev().zip(new.bytes().rev()).take(limit).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    Some(Diff { start: prefix, old_end: old.len() - suffix, new_end: new.len() - suffix })
}

#[derive(Debug)]
pub struct History {
    undo: Vec<Change>,
    redo: Vec<Change>,
    next: u64,
    sealed: bool,
    floor: u64,
    budget: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::with_budget(BUDGET)
    }
}

pub struct Step<'a> {
    pub range: Range<usize>,
    pub text: &'a str,
    pub cursor: Cursor,
}

impl History {
    #[must_use]
    pub const fn with_budget(budget: usize) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), next: 0, sealed: false, floor: 0, budget }
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.undo.last().map_or(self.floor, |change| change.id)
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn seal(&mut self) {
        self.sealed = true;
    }

    pub fn clear(&mut self) {
        self.floor = self.generation();
        self.undo.clear();
        self.redo.clear();
        self.sealed = true;
    }

    pub fn record(&mut self, old: &str, new: &str, before: Cursor, after: Cursor, kind: Kind) -> Option<Diff> {
        let diff = diff(old, new)?;
        let change = Change {
            start: diff.start,
            removed: old[diff.start..diff.old_end].to_string(),
            inserted: new[diff.start..diff.new_end].to_string(),
            before,
            after,
            kind,
            at: Instant::now(),
            id: 0,
        };
        self.redo.clear();
        if !self.sealed
            && let Some(previous) = self.undo.last_mut()
            && merge(previous, &change)
        {
            return Some(diff);
        }
        self.sealed = false;
        self.next += 1;
        self.undo.push(Change { id: self.next, ..change });
        let mut total: usize = self.undo.iter().map(Change::size).sum();
        while total > self.budget && self.undo.len() > 1 {
            let evicted = self.undo.remove(0);
            total -= evicted.size();
            self.floor = evicted.id;
        }
        Some(diff)
    }

    pub fn undo(&mut self) -> Option<Step<'_>> {
        let change = self.undo.pop()?;
        self.sealed = true;
        self.redo.push(change);
        let change = self.redo.last()?;
        Some(Step {
            range: change.start..change.start + change.inserted.len(),
            text: &change.removed,
            cursor: change.before,
        })
    }

    pub fn redo(&mut self) -> Option<Step<'_>> {
        let change = self.redo.pop()?;
        self.sealed = true;
        self.undo.push(change);
        let change = self.undo.last()?;
        Some(Step {
            range: change.start..change.start + change.removed.len(),
            text: &change.inserted,
            cursor: change.after,
        })
    }
}

fn merge(previous: &mut Change, change: &Change) -> bool {
    if previous.kind != change.kind || change.at.duration_since(previous.at) > MERGE_WINDOW {
        return false;
    }
    match change.kind {
        Kind::Typing => {
            let boundary =
                change.inserted.starts_with(char::is_whitespace) && !previous.inserted.ends_with(char::is_whitespace);
            if !change.removed.is_empty()
                || !previous.removed.is_empty()
                || boundary
                || change.start != previous.start + previous.inserted.len()
            {
                return false;
            }
            previous.inserted.push_str(&change.inserted);
        }
        Kind::Deleting => {
            if !change.inserted.is_empty() || !previous.inserted.is_empty() {
                return false;
            }
            if change.start + change.removed.len() == previous.start {
                previous.removed.insert_str(0, &change.removed);
                previous.start = change.start;
            } else if change.start == previous.start {
                previous.removed.push_str(&change.removed);
            } else {
                return false;
            }
        }
        Kind::Other => return false,
    }
    previous.after = change.after;
    previous.at = change.at;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::text_editor::Position;

    fn at(column: usize) -> Cursor {
        Cursor { position: Position { line: 0, column }, selection: None }
    }

    fn apply(text: &mut String, step: &Step<'_>) {
        text.replace_range(step.range.clone(), step.text);
    }

    #[test]
    fn diffs_stay_on_character_boundaries() {
        assert_eq!(diff("абв", "абв"), None);
        assert_eq!(diff("абв", "агв"), Some(Diff { start: 2, old_end: 4, new_end: 4 }));
        assert_eq!(diff("aaa", "aa"), Some(Diff { start: 2, old_end: 3, new_end: 2 }));
        assert_eq!(diff("ї", "й"), Some(Diff { start: 0, old_end: 2, new_end: 2 }));
    }

    #[test]
    fn typing_a_word_undoes_as_one_step_and_a_space_starts_the_next() {
        let mut history = History::default();
        let mut text = String::new();
        for (index, typed) in "слово два".chars().enumerate() {
            let old = text.clone();
            text.push(typed);
            history.record(&old, &text, at(index), at(index + 1), Kind::Typing);
        }
        let step = history.undo().map(|step| (step.range, step.text.to_string())).unwrap();
        text.replace_range(step.0, &step.1);
        assert_eq!(text, "слово");
        let step = history.undo().map(|step| (step.range, step.text.to_string())).unwrap();
        text.replace_range(step.0, &step.1);
        assert_eq!(text, "");
        assert!(!history.can_undo());
        let step = history.redo().map(|step| (step.range, step.text.to_string())).unwrap();
        text.replace_range(step.0, &step.1);
        assert_eq!(text, "слово");
    }

    #[test]
    fn backspaces_merge_and_restore_in_order() {
        let mut history = History::default();
        let mut text = String::from("hello");
        for index in (2..5).rev() {
            let old = text.clone();
            text.remove(index);
            history.record(&old, &text, at(index + 1), at(index), Kind::Deleting);
        }
        assert_eq!(text, "he");
        let step = history.undo().unwrap();
        assert_eq!(step.cursor, at(5));
        let (range, restored) = (step.range.clone(), step.text.to_string());
        text.replace_range(range, &restored);
        assert_eq!(text, "hello");
    }

    #[test]
    fn the_generation_returns_to_the_saved_point_after_undo_and_redo() {
        let mut history = History::default();
        history.record("", "a", at(0), at(1), Kind::Other);
        let saved = history.generation();
        history.record("a", "ab", at(1), at(2), Kind::Other);
        assert_ne!(history.generation(), saved);
        let mut text = String::from("ab");
        let step = history.undo().unwrap();
        apply(&mut text, &step);
        assert_eq!(history.generation(), saved);
        let step = history.redo().unwrap();
        apply(&mut text, &step);
        assert_eq!(text, "ab");
        assert_ne!(history.generation(), saved);
    }

    fn grow(history: &mut History, text: &mut String, added: &str) {
        let old = text.clone();
        text.push_str(added);
        history.record(&old, text, at(old.len()), at(text.len()), Kind::Other);
    }

    fn undo_all(history: &mut History, text: &mut String) {
        while let Some(step) = history.undo() {
            apply(text, &step);
        }
    }

    #[test]
    fn undoing_everything_that_survived_the_budget_is_not_the_pristine_generation() {
        let mut history = History::with_budget(8);
        let mut text = String::new();
        for chunk in ["aaaa", "bbbb", "cccc"] {
            grow(&mut history, &mut text, chunk);
        }
        assert_eq!(history.undo.len(), 2, "the oldest change went over the budget");
        undo_all(&mut history, &mut text);
        assert_eq!(text, "aaaa", "the evicted edit is still applied");
        assert_ne!(history.generation(), 0, "an emptied stack must not read as the state before any edit");
    }

    #[test]
    fn a_save_point_inside_the_evicted_history_reads_clean_once_undone_back_to_it() {
        let mut history = History::with_budget(8);
        let mut text = String::new();
        grow(&mut history, &mut text, "aaaa");
        let saved = history.generation();
        grow(&mut history, &mut text, "bbbb");
        grow(&mut history, &mut text, "cccc");
        undo_all(&mut history, &mut text);
        assert_eq!(text, "aaaa");
        assert_eq!(history.generation(), saved);
    }

    #[test]
    fn clearing_the_history_keeps_the_generation_of_the_text_it_describes() {
        let mut history = History::default();
        let mut text = String::new();
        grow(&mut history, &mut text, "a");
        let before = history.generation();
        history.clear();
        assert!(!history.can_undo());
        assert_eq!(history.generation(), before);
    }

    #[test]
    fn a_seal_keeps_typing_after_a_save_in_its_own_step() {
        let mut history = History::default();
        history.record("", "a", at(0), at(1), Kind::Typing);
        history.seal();
        history.record("a", "ab", at(1), at(2), Kind::Typing);
        let step = history.undo().unwrap();
        assert_eq!(step.range, 1..2);
    }
}

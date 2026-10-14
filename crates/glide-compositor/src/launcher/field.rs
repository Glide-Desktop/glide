#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaretMove {
    Left,
    Right,
    WordLeft,
    WordRight,
    Home,
    End,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextField {
    text: String,
    caret: usize,
    anchor: Option<usize>,
}

impl TextField {
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn caret(&self) -> usize {
        self.caret
    }

    #[must_use]
    pub fn selection(&self) -> Option<std::ops::Range<usize>> {
        let anchor = self.anchor?;
        (anchor != self.caret).then(|| anchor.min(self.caret)..anchor.max(self.caret))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn delete_selection(&mut self) -> bool {
        let selection = self.selection();
        self.anchor = None;
        let Some(range) = selection else {
            return false;
        };
        self.caret = range.start;
        self.text.replace_range(range, "");
        true
    }

    pub fn insert(&mut self, ch: char) -> bool {
        if ch.is_control() {
            return false;
        }
        self.delete_selection();
        self.text.insert(self.caret, ch);
        self.caret += ch.len_utf8();
        true
    }

    pub fn insert_str(&mut self, text: &str) -> bool {
        let clean: String =
            text.chars().map(|ch| if ch.is_whitespace() { ' ' } else { ch }).filter(|ch| !ch.is_control()).collect();
        let clean = clean.trim_end_matches(' ');
        if clean.is_empty() {
            return false;
        }
        self.delete_selection();
        self.text.insert_str(self.caret, clean);
        self.caret += clean.len();
        true
    }

    fn previous_boundary(&self, from: usize) -> usize {
        self.text[..from].char_indices().next_back().map_or(0, |(index, _)| index)
    }

    fn next_boundary(&self, from: usize) -> usize {
        self.text[from..].chars().next().map_or(from, |ch| from + ch.len_utf8())
    }

    fn word_start_before(&self, from: usize) -> usize {
        let head = &self.text[..from];
        let trimmed = head.trim_end_matches(char::is_whitespace);
        trimmed.char_indices().rev().find(|(_, ch)| ch.is_whitespace()).map_or(0, |(index, ch)| index + ch.len_utf8())
    }

    fn word_end_after(&self, from: usize) -> usize {
        let tail = &self.text[from..];
        let skipped = tail.len() - tail.trim_start_matches(char::is_whitespace).len();
        let rest = &tail[skipped..];
        let word = rest.find(char::is_whitespace).unwrap_or(rest.len());
        from + skipped + word
    }

    pub fn backspace(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.caret == 0 {
            return false;
        }
        let start = self.previous_boundary(self.caret);
        self.text.replace_range(start..self.caret, "");
        self.caret = start;
        true
    }

    pub fn delete(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.caret >= self.text.len() {
            return false;
        }
        let end = self.next_boundary(self.caret);
        self.text.replace_range(self.caret..end, "");
        true
    }

    pub fn delete_word_back(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.caret == 0 {
            return false;
        }
        let start = self.word_start_before(self.caret);
        self.text.replace_range(start..self.caret, "");
        self.caret = start;
        true
    }

    pub fn move_caret(&mut self, movement: CaretMove) -> bool {
        let selection = self.selection();
        self.anchor = None;
        let had_selection = selection.is_some();
        let next = match (movement, selection) {
            (CaretMove::Left, Some(range)) => range.start,
            (CaretMove::Right, Some(range)) => range.end,
            (CaretMove::Left, None) => self.previous_boundary(self.caret),
            (CaretMove::Right, None) => self.next_boundary(self.caret),
            (CaretMove::WordLeft, _) => self.word_start_before(self.caret),
            (CaretMove::WordRight, _) => self.word_end_after(self.caret),
            (CaretMove::Home, _) => 0,
            (CaretMove::End, _) => self.text.len(),
        };
        let changed = next != self.caret || had_selection;
        self.caret = next;
        changed
    }

    pub fn select_all(&mut self) -> bool {
        if self.text.is_empty() || self.selection() == Some(0..self.text.len()) {
            return false;
        }
        self.anchor = Some(0);
        self.caret = self.text.len();
        true
    }

    pub fn clear(&mut self) -> bool {
        if self.text.is_empty() {
            return false;
        }
        self.text.clear();
        self.caret = 0;
        self.anchor = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(text: &str) -> TextField {
        let mut field = TextField::default();
        field.insert_str(text);
        field
    }

    #[test]
    fn typing_appends_at_the_caret() {
        let mut f = TextField::default();
        for ch in "fire".chars() {
            assert!(f.insert(ch));
        }
        assert_eq!(f.text(), "fire");
        assert_eq!(f.caret(), 4);
    }

    #[test]
    fn control_characters_are_rejected() {
        let mut f = TextField::default();
        assert!(!f.insert('\n'));
        assert!(!f.insert('\u{7f}'));
        assert!(f.is_empty());
    }

    #[test]
    fn inserting_in_the_middle_keeps_the_tail() {
        let mut f = field("frefox");
        f.move_caret(CaretMove::Home);
        f.move_caret(CaretMove::Right);
        f.insert('i');
        assert_eq!(f.text(), "firefox");
        assert_eq!(f.caret(), 2);
    }

    #[test]
    fn backspace_and_delete_work_on_either_side_of_the_caret() {
        let mut f = field("abc");
        f.move_caret(CaretMove::Left);
        assert!(f.backspace());
        assert_eq!(f.text(), "ac");
        assert!(f.delete());
        assert_eq!(f.text(), "a");
        assert!(!f.delete(), "nothing after the caret");
        assert!(f.backspace());
        assert!(!f.backspace(), "nothing before the caret");
    }

    #[test]
    fn caret_moves_step_over_whole_characters() {
        let mut f = field("їжак");
        assert_eq!(f.caret(), "їжак".len());
        f.move_caret(CaretMove::Left);
        assert_eq!(f.caret(), "їжа".len());
        f.backspace();
        assert_eq!(f.text(), "їжк");
        assert!(f.move_caret(CaretMove::End));
        assert_eq!(f.caret(), f.text().len());
    }

    #[test]
    fn home_and_end_jump_to_the_edges() {
        let mut f = field("hello world");
        assert!(f.move_caret(CaretMove::Home));
        assert_eq!(f.caret(), 0);
        assert!(!f.move_caret(CaretMove::Home), "already there");
        assert!(f.move_caret(CaretMove::End));
        assert_eq!(f.caret(), 11);
    }

    #[test]
    fn word_moves_skip_whitespace_then_a_word() {
        let mut f = field("open  the file");
        f.move_caret(CaretMove::WordLeft);
        assert_eq!(f.caret(), "open  the ".len());
        f.move_caret(CaretMove::WordLeft);
        assert_eq!(f.caret(), "open  ".len());
        f.move_caret(CaretMove::WordRight);
        assert_eq!(f.caret(), "open  the".len());
    }

    #[test]
    fn ctrl_backspace_deletes_the_previous_word_and_trailing_space() {
        let mut f = field("open the file");
        assert!(f.delete_word_back());
        assert_eq!(f.text(), "open the ");
        assert!(f.delete_word_back());
        assert_eq!(f.text(), "open ");
        assert!(f.delete_word_back());
        assert_eq!(f.text(), "");
        assert!(!f.delete_word_back());
    }

    #[test]
    fn select_all_is_replaced_by_the_next_keystroke() {
        let mut f = field("firefox");
        assert!(f.select_all());
        assert!(!f.select_all(), "already selected");
        assert_eq!(f.selection(), Some(0..7));
        f.insert('c');
        assert_eq!(f.text(), "c");
        assert_eq!(f.selection(), None);
    }

    #[test]
    fn backspace_over_a_selection_removes_only_the_selection() {
        let mut f = field("abc");
        f.select_all();
        assert!(f.backspace());
        assert!(f.is_empty());
        assert_eq!(f.caret(), 0);
    }

    #[test]
    fn an_arrow_collapses_the_selection_to_its_edge() {
        let mut f = field("abc");
        f.select_all();
        assert!(f.move_caret(CaretMove::Left));
        assert_eq!(f.caret(), 0);
        assert_eq!(f.selection(), None);
        f.select_all();
        assert!(f.move_caret(CaretMove::Right));
        assert_eq!(f.caret(), 3);
    }

    #[test]
    fn pasted_text_is_flattened_to_one_line() {
        let mut f = TextField::default();
        assert!(f.insert_str("two\nlines\t\u{7}here\n"));
        assert_eq!(f.text(), "two lines here");
        assert!(!f.insert_str("\n\n"), "nothing printable");
    }

    #[test]
    fn clearing_reports_whether_there_was_text() {
        let mut f = field("x");
        assert!(f.clear());
        assert!(!f.clear());
        assert_eq!(f.caret(), 0);
    }
}

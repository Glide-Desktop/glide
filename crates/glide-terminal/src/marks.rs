use std::time::{Duration, Instant};

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::term::cell::Hyperlink;
use alacritty_terminal::term::{Term, TermMode};

const PREFIX: &str = "glide-mark:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Line,
    Bookmark,
}

impl Mark {
    const fn uri(self) -> &'static str {
        match self {
            Self::Line => "glide-mark:line",
            Self::Bookmark => "glide-mark:bookmark",
        }
    }
}

#[must_use]
pub(crate) fn is_mark(uri: &str) -> bool {
    uri.starts_with(PREFIX)
}

#[must_use]
pub fn at<T>(term: &Term<T>, line: Line) -> Option<Mark> {
    let uri = term.grid()[line][Column(0)].hyperlink()?;
    match uri.uri() {
        uri if uri == Mark::Bookmark.uri() => Some(Mark::Bookmark),
        uri if uri == Mark::Line.uri() => Some(Mark::Line),
        _ => None,
    }
}

fn set<T>(term: &mut Term<T>, line: Line, mark: Option<Mark>) {
    let cell = &mut term.grid_mut()[line][Column(0)];
    if cell.hyperlink().is_some_and(|link| !is_mark(link.uri())) {
        return;
    }
    cell.set_hyperlink(mark.map(|mark| Hyperlink::new(Some("glide-mark"), mark.uri().to_string())));
}

fn cursor_line<T>(term: &Term<T>) -> Line {
    term.grid().cursor.point.line
}

const SETTLE: Duration = Duration::from_secs(1);
const REACH: i32 = 64;

#[derive(Debug, Clone)]
pub struct Prompt {
    prefix: String,
    line: Line,
    history: usize,
    until: Instant,
}

fn text<T>(term: &Term<T>, line: Line) -> String {
    term.bounds_to_string(Point::new(line, Column(0)), Point::new(line, term.last_column()))
}

#[must_use]
pub fn prompt<T>(term: &mut Term<T>) -> Option<Prompt> {
    if term.mode().contains(TermMode::ALT_SCREEN) {
        return None;
    }
    let cursor = term.grid().cursor.point;
    let mut prefix = text(term, cursor.line);
    prefix.truncate(prefix.char_indices().nth(cursor.column.0).map_or(prefix.len(), |(index, _)| index));
    let prompt = Prompt {
        prefix: prefix.trim_end().to_string(),
        line: cursor.line,
        history: term.history_size(),
        until: Instant::now() + SETTLE,
    };
    prompt.settle(term);
    Some(prompt)
}

impl Prompt {
    pub fn settle<T>(&self, term: &mut Term<T>) -> bool {
        let shift = term.history_size().saturating_sub(self.history) as i32;
        let expected = Line((self.line.0 - shift).min(term.bottommost_line().0));
        let top = term.topmost_line().0;
        let found = (0..REACH).map(|up| Line(expected.0 - up)).take_while(|line| line.0 >= top).find(|&line| {
            if self.prefix.is_empty() { line == expected } else { text(term, line).starts_with(&self.prefix) }
        });
        if let Some(line) = found
            && at(term, line).is_none()
        {
            set(term, line, Some(Mark::Line));
        }
        Instant::now() < self.until
    }
}

pub fn toggle<T>(term: &mut Term<T>, mark: Mark) {
    let line = cursor_line(term);
    let next = if at(term, line) == Some(mark) { None } else { Some(mark) };
    set(term, line, next);
}

pub fn jump<T: alacritty_terminal::event::EventListener>(
    term: &mut Term<T>, direction: Direction, bookmarks: bool,
) -> bool {
    let top = Line(-(term.grid().display_offset() as i32));
    let wanted = |line: Line| at(term, line).is_some_and(|mark| !bookmarks || mark == Mark::Bookmark);
    let target = match direction {
        Direction::Left => (term.topmost_line().0..top.0).rev().map(Line).find(|&line| wanted(line)),
        Direction::Right => (top.0 + 1..=term.bottommost_line().0).map(Line).find(|&line| wanted(line)),
    };
    let Some(target) = target else {
        if direction == Direction::Right {
            term.scroll_display(Scroll::Bottom);
        }
        return false;
    };
    let offset = (-target.0).max(0);
    term.scroll_display(Scroll::Delta(offset - term.grid().display_offset() as i32));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    use crate::session::Cells;

    fn feed(term: &mut Term<VoidListener>, output: &str) {
        let mut processor: Processor = Processor::new();
        processor.advance(term, output.as_bytes());
    }

    fn term(output: &str) -> Term<VoidListener> {
        let mut term = Term::new(Config::default(), &Cells { columns: 20, lines: 4 }, VoidListener);
        feed(&mut term, output);
        term
    }

    #[test]
    fn marks_ride_with_their_line_into_the_scrollback() {
        let mut term = term("$ ls\r\n");
        set(&mut term, Line(0), Some(Mark::Line));
        feed(&mut term, "a\r\nb\r\nc\r\nd\r\ne\r\n");
        assert!(term.grid().history_size() > 0);
        let found: Vec<Line> =
            (term.topmost_line().0..=term.bottommost_line().0).map(Line).filter(|&l| at(&term, l).is_some()).collect();
        assert_eq!(found.len(), 1);
        assert!(found[0].0 < 0);
        assert_eq!(term.grid()[found[0]][Column(0)].c, '$');
    }

    #[test]
    fn jumping_scrolls_the_mark_to_the_top_and_past_the_last_returns_to_the_bottom() {
        let mut term = term("$ one\r\n");
        set(&mut term, Line(0), Some(Mark::Line));
        feed(&mut term, "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n");
        assert!(jump(&mut term, Direction::Left, false));
        let offset = term.grid().display_offset() as i32;
        assert_eq!(at(&term, Line(-offset)), Some(Mark::Line));
        assert!(!jump(&mut term, Direction::Left, false));
        assert!(!jump(&mut term, Direction::Left, true));
        assert!(!jump(&mut term, Direction::Right, false));
        assert_eq!(term.grid().display_offset(), 0);
    }

    #[test]
    fn toggling_twice_clears_and_bookmarks_replace_marks() {
        let mut term = term("$ ");
        toggle(&mut term, Mark::Line);
        assert_eq!(at(&term, Line(0)), Some(Mark::Line));
        toggle(&mut term, Mark::Bookmark);
        assert_eq!(at(&term, Line(0)), Some(Mark::Bookmark));
        toggle(&mut term, Mark::Bookmark);
        assert_eq!(at(&term, Line(0)), None);
    }

    #[test]
    fn prompt_marks_skip_the_alternate_screen() {
        let mut term = term("\x1b[?1049h$ ");
        assert!(prompt(&mut term).is_none());
        assert_eq!(at(&term, cursor_line(&term)), None);
    }

    #[test]
    fn a_prompt_mark_survives_the_shell_repainting_its_line_and_output_scrolling_it_away() {
        let mut term = term("$ ls");
        let pending = prompt(&mut term).expect("outside the alternate screen");
        assert_eq!(at(&term, Line(0)), Some(Mark::Line));
        feed(&mut term, "\r\x1b[K$ ls\r\na\r\nb\r\nc\r\nd\r\n");
        assert!(pending.settle(&mut term));
        let marked: Vec<Line> =
            (term.topmost_line().0..=term.bottommost_line().0).map(Line).filter(|&l| at(&term, l).is_some()).collect();
        assert_eq!(marked.len(), 1);
        assert!(text(&term, marked[0]).starts_with("$ ls"));
    }

    #[test]
    fn real_hyperlinks_are_never_overwritten() {
        let mut term = term("\x1b]8;;https://example.com\x07x\x1b]8;;\x07");
        set(&mut term, Line(0), Some(Mark::Line));
        assert_eq!(at(&term, Line(0)), None);
    }
}

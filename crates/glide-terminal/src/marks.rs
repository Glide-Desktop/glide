use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Direction, Line};
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
pub fn is_mark(uri: &str) -> bool {
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

pub fn prompt<T>(term: &mut Term<T>) {
    if term.mode().contains(TermMode::ALT_SCREEN) {
        return;
    }
    let line = cursor_line(term);
    if at(term, line).is_none() {
        set(term, line, Some(Mark::Line));
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
        prompt(&mut term);
        assert_eq!(at(&term, cursor_line(&term)), None);
    }

    #[test]
    fn real_hyperlinks_are_never_overwritten() {
        let mut term = term("\x1b]8;;https://example.com\x07x\x1b]8;;\x07");
        set(&mut term, Line(0), Some(Mark::Line));
        assert_eq!(at(&term, Line(0)), None);
    }
}

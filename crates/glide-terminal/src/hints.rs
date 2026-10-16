use std::path::Path;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};

pub const URLS: &str = "(ipfs:|ipns:|magnet:|mailto:|gemini://|gopher://|https://|http://|news:|file:|git://|ssh:|ftp://)\
                        [^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{-}\\^⟨⟩`]+";
const PATHS: &str = "(~|\\.\\.?)?/[^\u{0000}-\u{0020}\u{007F}:'\"<>|()\\[\\]{}`]+";
const ALPHABET: &str = "asdfghjklqwertyuiopzxcvbnm";
const LIMIT: usize = ALPHABET.len() * ALPHABET.len();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Url,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub range: Match,
    pub text: String,
    pub label: String,
    pub kind: Kind,
}

#[must_use]
pub fn labels(count: usize) -> Vec<String> {
    let letters: Vec<char> = ALPHABET.chars().collect();
    if count <= letters.len() {
        return letters.iter().take(count).map(char::to_string).collect();
    }
    letters.iter().flat_map(|first| letters.iter().map(move |second| format!("{first}{second}"))).take(count).collect()
}

fn overlaps(a: &Match, b: &Match) -> bool {
    a.start() <= b.end() && b.start() <= a.end()
}

pub fn scan<T>(term: &Term<T>) -> Vec<Hint> {
    let offset = term.grid().display_offset() as i32;
    let start = Point::new(Line(-offset), Column(0));
    let end = Point::new(Line(term.screen_lines() as i32 - 1 - offset), term.last_column());
    let mut found: Vec<(Match, Kind)> = Vec::new();
    for (pattern, kind) in [(URLS, Kind::Url), (PATHS, Kind::Path)] {
        let Ok(mut regex) = RegexSearch::new(pattern) else { continue };
        for range in RegexIter::new(start, end, Direction::Right, term, &mut regex) {
            if !found.iter().any(|(other, _)| overlaps(other, &range)) {
                found.push((range, kind));
            }
        }
    }
    let standalone = |range: &Match| {
        let start = *range.start();
        start.column.0 == 0 || {
            let before = term.grid()[start.line][start.column - 1].c;
            !(before.is_alphanumeric() || "_-.~/".contains(before))
        }
    };
    found.retain(|(range, kind)| {
        *kind == Kind::Url || (standalone(range) && term.bounds_to_string(*range.start(), *range.end()).len() > 1)
    });
    found.sort_by_key(|(range, _)| *range.start());
    found.truncate(LIMIT);
    let labels = labels(found.len());
    found
        .into_iter()
        .zip(labels)
        .map(|((range, kind), label)| {
            let text = term.bounds_to_string(*range.start(), *range.end());
            let text = text.trim_end_matches(['.', ',', ';']).to_string();
            Hint { range, text, label, kind }
        })
        .collect()
}

#[must_use]
pub fn target(hint: &Hint, home: Option<&Path>, cwd: Option<&Path>) -> String {
    if hint.kind == Kind::Url {
        return hint.text.clone();
    }
    if let (Some(rest), Some(home)) = (hint.text.strip_prefix("~/"), home) {
        return home.join(rest).display().to_string();
    }
    match cwd {
        Some(cwd) if !hint.text.starts_with('/') => cwd.join(&hint.text).display().to_string(),
        _ => hint.text.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    use crate::session::Cells;

    fn term(output: &str) -> Term<VoidListener> {
        let mut term = Term::new(Config::default(), &Cells { columns: 60, lines: 4 }, VoidListener);
        let mut processor: Processor = Processor::new();
        processor.advance(&mut term, output.as_bytes());
        term
    }

    #[test]
    fn labels_are_unique_and_never_a_prefix_of_another() {
        for count in [0, 1, 26, 27, 300] {
            let labels = labels(count);
            assert_eq!(labels.len(), count);
            for (index, label) in labels.iter().enumerate() {
                for (other_index, other) in labels.iter().enumerate() {
                    assert!(index == other_index || !other.starts_with(label.as_str()), "{label} {other}");
                }
            }
        }
    }

    #[test]
    fn urls_and_paths_are_found_in_reading_order_without_doubling_a_url_as_a_path() {
        let term = term("see https://glide.dev/docs and ~/notes.txt.\r\nthen ./build/out /usr/bin a/b /\r\n");
        let hints = scan(&term);
        let texts: Vec<&str> = hints.iter().map(|hint| hint.text.as_str()).collect();
        assert_eq!(texts, ["https://glide.dev/docs", "~/notes.txt", "./build/out", "/usr/bin"]);
        assert_eq!(hints[0].kind, Kind::Url);
        assert!(hints[1..].iter().all(|hint| hint.kind == Kind::Path));
        assert_eq!(hints.iter().map(|hint| hint.label.as_str()).collect::<Vec<_>>(), ["a", "s", "d", "f"]);
    }

    #[test]
    fn paths_open_relative_to_the_shell_and_home() {
        let hint = |text: &str| Hint {
            range: Point::new(Line(0), Column(0))..=Point::new(Line(0), Column(0)),
            text: text.to_string(),
            label: String::from("a"),
            kind: Kind::Path,
        };
        let (home, cwd) = (Path::new("/home/me"), Path::new("/srv/app"));
        assert_eq!(target(&hint("~/a.txt"), Some(home), Some(cwd)), "/home/me/a.txt");
        assert_eq!(target(&hint("./build"), Some(home), Some(cwd)), "/srv/app/./build");
        assert_eq!(target(&hint("/etc/hosts"), Some(home), Some(cwd)), "/etc/hosts");
    }
}

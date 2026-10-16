use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};

const CHUNK: usize = 2000;
const LIMIT: usize = 50_000;

#[derive(Debug, Clone)]
pub struct Outcome {
    pub generation: u64,
    pub found: Option<Match>,
    pub total: usize,
    pub index: usize,
}

pub struct Job<T> {
    pub generation: u64,
    pub latest: Arc<AtomicU64>,
    pub term: Arc<FairMutex<Term<T>>>,
    pub regex: RegexSearch,
    pub target: Target,
}

#[derive(Debug, Clone)]
pub enum Target {
    Current(Match),
    Seek(Point, Direction),
}

fn chunk_end<T>(term: &Term<T>, from: Line, chunk: usize) -> Line {
    let bottom = term.bottommost_line();
    let mut end = Line((from.0 + chunk as i32 - 1).min(bottom.0));
    let last = term.last_column();
    while end < bottom && term.grid()[end][last].flags.contains(Flags::WRAPLINE) {
        end += 1;
    }
    end
}

fn scan<T>(job: &mut Job<T>, chunk: usize) -> Option<Vec<Match>> {
    let mut matches = Vec::new();
    let mut line = job.term.lock().topmost_line();
    loop {
        if job.latest.load(Ordering::Relaxed) != job.generation {
            return None;
        }
        let term = job.term.lock();
        if line > term.bottommost_line() {
            break;
        }
        line = line.max(term.topmost_line());
        let end = chunk_end(&term, line, chunk);
        let range = RegexIter::new(
            Point::new(line, Column(0)),
            Point::new(end, term.last_column()),
            Direction::Right,
            &term,
            &mut job.regex,
        );
        matches.extend(range.filter(|found| found.start().line >= line));
        line = end + 1;
        if matches.len() >= LIMIT {
            matches.truncate(LIMIT);
            break;
        }
    }
    Some(matches)
}

fn pick(matches: &[Match], target: &Target) -> Option<Match> {
    match target {
        Target::Current(current) => Some(current.clone()),
        Target::Seek(origin, Direction::Right) => {
            matches.iter().find(|found| found.start() > origin).or_else(|| matches.first()).cloned()
        }
        Target::Seek(origin, Direction::Left) => {
            matches.iter().rev().find(|found| found.start() < origin).or_else(|| matches.last()).cloned()
        }
    }
}

fn settle<T>(mut job: Job<T>, chunk: usize) -> Option<Outcome> {
    let matches = scan(&mut job, chunk)?;
    let found = pick(&matches, &job.target);
    let index = found
        .as_ref()
        .and_then(|found| matches.iter().position(|candidate| candidate.start() == found.start()))
        .map_or(0, |position| position + 1);
    let seek = matches!(job.target, Target::Seek(..));
    Some(Outcome { generation: job.generation, found: if seek { found } else { None }, total: matches.len(), index })
}

pub fn spawn<T: Send + 'static>(job: Job<T>) -> iced::futures::channel::oneshot::Receiver<Outcome> {
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    let spawned = std::thread::Builder::new().name("glide-terminal-find".into()).spawn(move || {
        if let Some(outcome) = settle(job, CHUNK) {
            let _ = sender.send(outcome);
        }
    });
    drop(spawned);
    receiver
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    use crate::session::Cells;

    fn term(output: &str) -> Arc<FairMutex<Term<VoidListener>>> {
        let mut term = Term::new(Config::default(), &Cells { columns: 10, lines: 3 }, VoidListener);
        let mut processor: Processor = Processor::new();
        processor.advance(&mut term, output.as_bytes());
        Arc::new(FairMutex::new(term))
    }

    fn job(term: &Arc<FairMutex<Term<VoidListener>>>, target: Target) -> Job<VoidListener> {
        Job {
            generation: 3,
            latest: Arc::new(AtomicU64::new(3)),
            term: Arc::clone(term),
            regex: RegexSearch::new("(?i)foo").unwrap(),
            target,
        }
    }

    #[test]
    fn counting_in_one_line_chunks_matches_a_single_pass_even_across_wrapped_lines() {
        let term = term("foo\r\nbar foo\r\nxxxxxxxfoofoo\r\nfoo\r\nnone\r\nFOO\r\n");
        let whole = settle(job(&term, Target::Seek(Point::new(Line(0), Column(0)), Direction::Right)), 10_000);
        let tiny = settle(job(&term, Target::Seek(Point::new(Line(0), Column(0)), Direction::Right)), 1);
        let (whole, tiny) = (whole.unwrap(), tiny.unwrap());
        assert_eq!(whole.total, 6);
        assert_eq!(tiny.total, whole.total);
    }

    #[test]
    fn seeking_wraps_around_and_reports_the_position_of_what_it_found() {
        let term = term("foo\r\nbar\r\nfoo\r\nbar\r\nbar\r\n");
        let bottom = term.lock().bottommost_line();
        let down = settle(job(&term, Target::Seek(Point::new(bottom, Column(0)), Direction::Right)), 2).unwrap();
        assert_eq!((down.index, down.total), (1, 2), "past the last match the search wraps to the first");
        let up = settle(job(&term, Target::Seek(Point::new(bottom, Column(0)), Direction::Left)), 2).unwrap();
        assert_eq!((up.index, up.total), (2, 2));
        assert!(up.found.is_some());
    }

    #[test]
    fn a_newer_query_cancels_the_scan() {
        let term = term("foo\r\n");
        let job = job(&term, Target::Seek(Point::new(Line(0), Column(0)), Direction::Right));
        job.latest.store(4, Ordering::Relaxed);
        assert!(settle(job, 1).is_none());
    }
}

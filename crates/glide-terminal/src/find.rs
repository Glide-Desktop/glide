use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};

const CHUNK: usize = 250;

#[derive(Debug, Clone)]
pub struct Outcome {
    pub generation: u64,
    pub found: Option<Match>,
    pub total: usize,
    pub index: usize,
    pub revision: u64,
}

pub struct Job<T> {
    pub generation: u64,
    pub latest: Arc<AtomicU64>,
    pub term: Arc<FairMutex<Term<T>>>,
    pub regex: RegexSearch,
    pub target: Target,
    pub revision: Arc<AtomicU64>,
    pub expected_revision: u64,
}

#[derive(Debug, Clone)]
pub enum Target {
    Current(Match),
    Seek(Point, Direction),
}

pub(crate) fn chunk_end<T>(term: &Term<T>, from: Line, chunk: usize) -> Line {
    let bottom = term.bottommost_line();
    let mut end = Line((from.0 + chunk as i32 - 1).min(bottom.0));
    let last = term.last_column();
    while end < bottom && term.grid()[end][last].flags.contains(Flags::WRAPLINE) {
        end += 1;
    }
    end
}

fn settle<T>(mut job: Job<T>, chunk: usize) -> Option<Outcome> {
    let mut total = 0;
    let mut first = None;
    let mut last = None;
    let mut chosen = None;
    let mut line = job.term.lock().topmost_line();
    loop {
        if job.latest.load(Ordering::Acquire) != job.generation
            || job.revision.load(Ordering::Acquire) != job.expected_revision
        {
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
        for found in range.filter(|found| found.start().line >= line) {
            total += 1;
            let candidate = (found, total);
            if first.is_none() {
                first = Some(candidate.clone());
            }
            let wanted = match &job.target {
                Target::Current(current) => candidate.0.start() == current.start(),
                Target::Seek(origin, Direction::Right) => chosen.is_none() && candidate.0.start() >= origin,
                Target::Seek(origin, Direction::Left) => candidate.0.start() <= origin,
            };
            if wanted {
                chosen = Some(candidate.clone());
            }
            last = Some(candidate);
        }
        line = end + 1;
    }
    if job.revision.load(Ordering::Acquire) != job.expected_revision {
        return None;
    }
    let chosen = chosen.or(match &job.target {
        Target::Seek(_, Direction::Right) => first,
        Target::Seek(_, Direction::Left) => last,
        Target::Current(_) => None,
    });
    let (found, index) = chosen.map_or((None, 0), |(found, index)| (Some(found), index));
    let seek = matches!(job.target, Target::Seek(..));
    Some(Outcome {
        generation: job.generation,
        found: if seek { found } else { None },
        total,
        index,
        revision: job.expected_revision,
    })
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
            revision: Arc::new(AtomicU64::new(0)),
            expected_revision: 0,
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

    #[test]
    fn a_grid_revision_change_rejects_the_entire_scan() {
        let term = term("foo\r\n");
        let job = job(&term, Target::Seek(Point::new(Line(0), Column(0)), Direction::Left));
        job.revision.fetch_add(1, Ordering::Release);
        assert!(settle(job, 1).is_none());
    }

    #[test]
    fn counting_and_navigation_do_not_truncate_after_fifty_thousand_matches() {
        let mut term = Term::new(
            Config { scrolling_history: 30_000, ..Config::default() },
            &Cells { columns: 10, lines: 3 },
            VoidListener,
        );
        let mut processor: Processor = Processor::new();
        for _ in 0..20_000 {
            processor.advance(&mut term, b"foofoofoo\r\n");
        }
        let term = Arc::new(FairMutex::new(term));
        let bottom = term.lock().bottommost_line();
        let result = settle(job(&term, Target::Seek(Point::new(bottom, Column(9)), Direction::Left)), CHUNK).unwrap();
        assert_eq!(result.total, 60_000);
        assert_eq!(result.index, 60_000);
    }
}

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use glide_files::fs::jobs::{Answer, Choice, Op, Progress, Question, Sink, delete, transfer};

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("glide-jobs-{tag}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent");
    }
    std::fs::write(path, text).expect("write");
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[derive(Default)]
struct Script {
    answers: RefCell<Vec<Answer>>,
    asked: RefCell<Vec<Question>>,
    last: RefCell<Progress>,
    cancel_on_progress: Option<&'static AtomicBool>,
}

impl Script {
    fn answering(answers: &[(Choice, bool)]) -> Self {
        Self {
            answers: RefCell::new(answers.iter().rev().map(|&(choice, for_all)| Answer { choice, for_all }).collect()),
            ..Self::default()
        }
    }
}

impl Sink for Script {
    fn progress(&self, progress: Progress) {
        let bytes = progress.bytes;
        *self.last.borrow_mut() = progress;
        if let Some(flag) = self.cancel_on_progress
            && bytes > 0
        {
            flag.store(true, Ordering::Relaxed);
        }
    }

    fn ask(&self, question: Question) -> Answer {
        self.asked.borrow_mut().push(question);
        self.answers.borrow_mut().pop().unwrap_or(Answer { choice: Choice::Cancel, for_all: false })
    }
}

fn run(op: Op, sources: &[PathBuf], target: &Path, sink: &Script) -> glide_files::fs::jobs::Report {
    transfer(op, sources, target, sink, &AtomicBool::new(false))
}

#[test]
fn copying_a_tree_keeps_contents_and_modification_times() {
    let root = scratch("tree");
    write(&root.join("src/a.txt"), "alpha");
    write(&root.join("src/deep/b.txt"), "beta");
    std::os::unix::fs::symlink("a.txt", root.join("src/link")).expect("symlink");
    std::fs::create_dir(root.join("dst")).expect("dst");
    let sink = Script::default();
    let report = run(Op::Copy, &[root.join("src")], &root.join("dst"), &sink);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read(&root.join("dst/src/deep/b.txt")), "beta");
    assert_eq!(std::fs::read_link(root.join("dst/src/link")).expect("link"), PathBuf::from("a.txt"));
    let before = std::fs::metadata(root.join("src/a.txt")).and_then(|m| m.modified()).ok();
    let after = std::fs::metadata(root.join("dst/src/a.txt")).and_then(|m| m.modified()).ok();
    assert_eq!(before, after);
    let last = sink.last.borrow().clone();
    assert_eq!(last.bytes, 9);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_fifo_is_reported_instead_of_blocking_the_copy() {
    let root = scratch("fifo");
    write(&root.join("src/plain.txt"), "ok");
    let fifo = std::ffi::CString::new(root.join("src/pipe").to_string_lossy().as_bytes()).expect("path");
    // SAFETY: `fifo` is a NUL-terminated path string.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    std::fs::create_dir(root.join("dst")).expect("dst");
    let report = run(Op::Copy, &[root.join("src")], &root.join("dst"), &Script::default());
    assert_eq!(read(&root.join("dst/src/plain.txt")), "ok");
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].path.ends_with("pipe"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn copying_a_folder_into_itself_through_a_symlink_is_refused() {
    let root = scratch("loop");
    write(&root.join("src/a.txt"), "a");
    std::os::unix::fs::symlink(root.join("src"), root.join("alias")).expect("symlink");
    let report = run(Op::Copy, &[root.join("src")], &root.join("alias"), &Script::default());
    assert_eq!(report.failures.len(), 1);
    assert!(report.done.is_empty());
    assert!(!root.join("src/src").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_link_to_a_folder_can_be_copied_into_that_folder() {
    let root = scratch("link-in");
    write(&root.join("real/a.txt"), "a");
    std::os::unix::fs::symlink(root.join("real"), root.join("link")).expect("symlink");
    let report = run(Op::Copy, &[root.join("link")], &root.join("real"), &Script::default());
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(std::fs::read_link(root.join("real/link")).expect("copied link"), root.join("real"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn copying_into_the_same_folder_duplicates_without_asking() {
    let root = scratch("dup");
    write(&root.join("note.txt"), "n");
    let sink = Script::default();
    let report = run(Op::Copy, &[root.join("note.txt")], &root, &sink);
    assert!(sink.asked.borrow().is_empty());
    assert_eq!(report.done[0].1, root.join("note (1).txt"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn conflicts_follow_the_answer_and_for_all_is_remembered() {
    let root = scratch("conflict");
    for name in ["a", "b", "c"] {
        write(&root.join(format!("src/{name}.txt")), "new");
        write(&root.join(format!("dst/{name}.txt")), "old");
    }
    let sources: Vec<PathBuf> = ["a", "b", "c"].iter().map(|name| root.join(format!("src/{name}.txt"))).collect();
    let sink = Script::answering(&[(Choice::Skip, false), (Choice::Replace, true)]);
    let report = run(Op::Copy, &sources, &root.join("dst"), &sink);
    assert_eq!(sink.asked.borrow().len(), 2, "the third conflict reuses the remembered answer");
    assert_eq!(read(&root.join("dst/a.txt")), "old");
    assert_eq!(read(&root.join("dst/b.txt")), "new");
    assert_eq!(read(&root.join("dst/c.txt")), "new");
    assert_eq!(report.done.len(), 2);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_replace_that_cannot_copy_keeps_the_file_it_would_have_replaced() {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let root = scratch("replace");
    write(&root.join("src/a.txt"), "new");
    write(&root.join("dst/a.txt"), "old");
    write(&root.join("src/b.txt"), "fresh");
    write(&root.join("dst/b.txt"), "stale");
    std::fs::set_permissions(root.join("src/a.txt"), std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let sources = [root.join("src/a.txt"), root.join("src/b.txt")];
    let sink = Script::answering(&[(Choice::Replace, true)]);
    let report = run(Op::Copy, &sources, &root.join("dst"), &sink);
    assert_eq!(report.failures.len(), 1, "the unreadable source is reported");
    assert_eq!(read(&root.join("dst/a.txt")), "old", "and the file it would have replaced is still there");
    assert_eq!(read(&root.join("dst/b.txt")), "fresh", "a readable one replaces as before");
    let names: Vec<String> = std::fs::read_dir(root.join("dst"))
        .expect("dst")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "no staging file is left behind: {names:?}");
    let _ = std::fs::set_permissions(root.join("src/a.txt"), std::fs::Permissions::from_mode(0o600));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn replacing_an_item_with_itself_or_its_own_folder_is_refused() {
    let root = scratch("self-replace");
    write(&root.join("box/box/inner.txt"), "keep");
    write(&root.join("box/other.txt"), "other");
    let sink = Script::answering(&[(Choice::Replace, false)]);
    let report = run(Op::Copy, &[root.join("box/box")], &root, &sink);
    assert_eq!(report.failures.len(), 1, "a folder can't replace the folder that holds it");
    assert_eq!(read(&root.join("box/box/inner.txt")), "keep");
    assert_eq!(read(&root.join("box/other.txt")), "other");

    write(&root.join("shelf/shelf"), "file");
    let sink = Script::answering(&[(Choice::Replace, false)]);
    let report = run(Op::Move, &[root.join("shelf/shelf")], &root, &sink);
    assert_eq!(report.failures.len(), 1, "nor can a file");
    assert_eq!(read(&root.join("shelf/shelf")), "file");

    write(&root.join("real/doc.txt"), "content");
    std::fs::create_dir(root.join("links")).expect("links");
    std::os::unix::fs::symlink(root.join("real/doc.txt"), root.join("links/doc.txt")).expect("symlink");
    for op in [Op::Copy, Op::Move] {
        let sink = Script::answering(&[(Choice::Replace, false)]);
        let report = run(op, &[root.join("links/doc.txt")], &root.join("real"), &sink);
        assert_eq!(report.failures.len(), 1, "{op:?}: a link can't replace the file it points to");
        assert_eq!(read(&root.join("real/doc.txt")), "content", "{op:?}");
    }

    std::os::unix::fs::symlink(root.join("real"), root.join("alias")).expect("alias");
    write(&root.join("real/sub/deep.txt"), "deep");
    let sink = Script::answering(&[(Choice::Replace, false)]);
    let report = run(Op::Copy, &[root.join("alias/sub")], &root.join("real"), &sink);
    assert_eq!(report.failures.len(), 1, "nor a folder reached through another path");
    assert_eq!(read(&root.join("real/sub/deep.txt")), "deep");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn keep_both_renames_and_merge_combines_folders() {
    let root = scratch("merge");
    write(&root.join("src/box/one.txt"), "1");
    write(&root.join("src/box/same.txt"), "new");
    write(&root.join("dst/box/two.txt"), "2");
    write(&root.join("dst/box/same.txt"), "old");
    let sink = Script::answering(&[(Choice::Merge, false), (Choice::KeepBoth, false)]);
    run(Op::Copy, &[root.join("src/box")], &root.join("dst"), &sink);
    assert!(sink.asked.borrow()[0].folders);
    assert_eq!(read(&root.join("dst/box/one.txt")), "1");
    assert_eq!(read(&root.join("dst/box/two.txt")), "2");
    assert_eq!(read(&root.join("dst/box/same.txt")), "old");
    assert_eq!(read(&root.join("dst/box/same (1).txt")), "new");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn cancelling_removes_the_partial_copy_and_moving_removes_the_source() {
    static CANCEL: AtomicBool = AtomicBool::new(false);
    let root = scratch("cancel");
    std::fs::create_dir_all(root.join("dst")).expect("dst");
    std::fs::write(root.join("big.bin"), vec![7_u8; 3 << 20]).expect("big");
    let sink = Script { cancel_on_progress: Some(&CANCEL), ..Script::default() };
    let report = transfer(Op::Copy, &[root.join("big.bin")], &root.join("dst"), &sink, &CANCEL);
    assert!(report.cancelled);
    assert!(!root.join("dst/big.bin").exists(), "no half-written file is left behind");

    write(&root.join("move/me.txt"), "m");
    let sink = Script::default();
    let report = run(Op::Move, &[root.join("move")], &root.join("dst"), &sink);
    assert_eq!(report.done.len(), 1);
    assert!(!root.join("move").exists());
    assert_eq!(read(&root.join("dst/move/me.txt")), "m");
    let last = sink.last.borrow().clone();
    assert_eq!((last.bytes, last.items), (last.total_bytes, last.total_items), "a renamed folder counts in full");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn deleting_reports_each_item() {
    let root = scratch("delete");
    write(&root.join("a/x.txt"), "x");
    write(&root.join("b.txt"), "b");
    let sink = Script::default();
    let report = delete(&[root.join("a"), root.join("b.txt"), root.join("missing")], &sink, &AtomicBool::new(false));
    assert_eq!(report.done.len(), 2);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(sink.last.borrow().items, 3);
    let _ = std::fs::remove_dir_all(root);
}

struct Paused {
    until: std::time::Instant,
}

impl Sink for Paused {
    fn progress(&self, _: Progress) {}

    fn ask(&self, _: Question) -> Answer {
        Answer { choice: Choice::Cancel, for_all: false }
    }

    fn paused(&self) -> bool {
        std::time::Instant::now() < self.until
    }
}

#[test]
fn a_paused_job_waits_and_then_finishes() {
    let root = scratch("pause");
    write(&root.join("a.txt"), "a");
    std::fs::create_dir(root.join("dst")).expect("dst");
    let cancel = AtomicBool::new(false);
    let started = std::time::Instant::now();
    let sink = Paused { until: started + std::time::Duration::from_millis(150) };
    let report = transfer(Op::Copy, &[root.join("a.txt")], &root.join("dst"), &sink, &cancel);
    assert!(started.elapsed() >= std::time::Duration::from_millis(150), "the copy held while paused");
    assert_eq!(report.done.len(), 1);
    assert_eq!(read(&root.join("dst/a.txt")), "a");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn progress_names_the_file_being_copied() {
    let root = scratch("current");
    write(&root.join("report.pdf"), "pdf");
    std::fs::create_dir(root.join("dst")).expect("dst");
    let sink = Script::default();
    run(Op::Copy, &[root.join("report.pdf")], &root.join("dst"), &sink);
    assert_eq!(sink.last.borrow().current.as_deref(), Some("report.pdf"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn each_applies_the_step_and_reports_destinations() {
    let root = scratch("each");
    write(&root.join("a.txt"), "a");
    let sink = Script::default();
    let paths = [root.join("a.txt"), root.join("missing")];
    let report = glide_files::fs::jobs::each(&paths, &sink, &AtomicBool::new(false), |path| {
        let dest = path.with_extension("moved");
        std::fs::rename(path, &dest).map(|()| dest)
    });
    assert_eq!(report.done, vec![(root.join("a.txt"), root.join("a.moved"))]);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(sink.last.borrow().items, 2);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_meter_reports_speed_and_time_left_only_while_running() {
    use glide_files::fs::jobs::Meter;
    let start = std::time::Instant::now();
    let mut meter = Meter::default();
    assert_eq!(meter.rate(), None);
    meter.sample(0, start);
    meter.sample(10_000_000, start + std::time::Duration::from_secs(1));
    let rate = meter.rate().expect("a rate after one window");
    assert!((rate - 10_000_000.0).abs() < 1.0);
    assert_eq!(meter.eta(20_000_000), Some(std::time::Duration::from_secs(2)));
    meter.pause(true);
    assert!(meter.is_paused());
    assert_eq!(meter.rate(), None, "a paused job shows no speed");
}

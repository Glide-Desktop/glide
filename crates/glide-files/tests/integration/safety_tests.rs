use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use glide_files::fs::jobs::{Answer, Choice, Op, Progress, Question, Sink, delete, transfer};
use glide_files::fs::listing::searchable;
use glide_files::fs::tags;
use glide_files::fs::watch::{changes, lost_a_folder};
use glide_files::fs::{Trash, create_from_template, path_from_arg, rename_item};

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("glide-safety-{tag}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn put(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent");
    }
    std::fs::write(path, text).expect("write");
}

#[derive(Default)]
struct Replacing {
    asked: RefCell<Vec<Question>>,
    mixed: Option<Choice>,
}

impl Sink for Replacing {
    fn progress(&self, _: Progress) {}

    fn ask(&self, question: Question) -> Answer {
        let both_files = question.source.is_file() && question.dest.is_file();
        self.asked.borrow_mut().push(question);
        let choice = if both_files { Choice::Replace } else { self.mixed.unwrap_or(Choice::Skip) };
        Answer { choice, for_all: true }
    }
}

#[test]
fn tags_travel_with_a_copy_and_with_a_copied_folder() {
    let root = scratch("xattr");
    put(&root.join("src/box/a.txt"), "a");
    let wanted = vec![String::from("Red"), String::from("taxes")];
    if tags::write(&root.join("src/box/a.txt"), &wanted).is_err()
        || tags::write(&root.join("src/box"), &wanted).is_err()
    {
        let _ = std::fs::remove_dir_all(root);
        return;
    }
    std::fs::create_dir(root.join("dst")).expect("dst");
    let report =
        transfer(Op::Copy, &[root.join("src/box")], &root.join("dst"), &Replacing::default(), &AtomicBool::new(false));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(tags::read(&root.join("dst/box/a.txt")), wanted);
    assert_eq!(tags::read(&root.join("dst/box")), wanted);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn replace_for_every_conflict_still_asks_before_a_file_takes_the_place_of_a_folder() {
    let root = scratch("mixed");
    put(&root.join("src/a.txt"), "new a");
    put(&root.join("src/notes"), "a file called notes");
    put(&root.join("dst/a.txt"), "old a");
    put(&root.join("dst/notes/kept.txt"), "precious");
    let sink = Replacing::default();
    let sources = [root.join("src/a.txt"), root.join("src/notes")];
    let report = transfer(Op::Copy, &sources, &root.join("dst"), &sink, &AtomicBool::new(false));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(sink.asked.borrow().len(), 2, "the remembered answer for files does not cover a folder");
    assert_eq!(std::fs::read_to_string(root.join("dst/a.txt")).expect("replaced"), "new a");
    assert_eq!(std::fs::read_to_string(root.join("dst/notes/kept.txt")).expect("kept"), "precious");

    put(&root.join("src/b.txt"), "new b");
    put(&root.join("dst/b.txt"), "old b");
    let sink = Replacing { mixed: Some(Choice::Replace), ..Replacing::default() };
    let sources = [root.join("src/notes"), root.join("src/b.txt"), root.join("src/a.txt")];
    transfer(Op::Copy, &sources, &root.join("dst"), &sink, &AtomicBool::new(false));
    assert_eq!(sink.asked.borrow().len(), 2, "an answer given for a mixed pair is not remembered for files");
    let _ = std::fs::remove_dir_all(root);
}

struct StopAtOnce<'a>(&'a AtomicBool);

impl Sink for StopAtOnce<'_> {
    fn progress(&self, progress: Progress) {
        if progress.current.is_some() {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    fn ask(&self, _: Question) -> Answer {
        Answer { choice: Choice::Cancel, for_all: false }
    }
}

#[test]
fn stopping_a_permanent_delete_leaves_the_rest_of_the_folder() {
    let root = scratch("stop-delete");
    for index in 0..40 {
        put(&root.join(format!("big/sub{}/file{index}.txt", index % 4)), "x");
    }
    let cancel = AtomicBool::new(false);
    let report = delete(&[root.join("big")], &StopAtOnce(&cancel), &cancel);
    assert!(report.cancelled);
    assert!(report.done.is_empty() && report.failures.is_empty());
    assert!(root.join("big").is_dir(), "the folder itself survives a stopped delete");

    let report = delete(&[root.join("big")], &Replacing::default(), &AtomicBool::new(false));
    assert_eq!(report.done.len(), 1);
    assert!(!root.join("big").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_read_only_tree_is_still_deleted() {
    use std::os::unix::fs::PermissionsExt;
    let root = scratch("read-only");
    put(&root.join("locked/inner/file.txt"), "x");
    for dir in ["locked/inner", "locked"] {
        std::fs::set_permissions(root.join(dir), std::fs::Permissions::from_mode(0o500)).expect("chmod");
    }
    let report = delete(&[root.join("locked")], &Replacing::default(), &AtomicBool::new(false));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(!root.join("locked").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_linked_template_gives_a_new_file_of_its_own() {
    use std::os::unix::fs::PermissionsExt;
    let root = scratch("template");
    put(&root.join("shared/letter.txt"), "Dear");
    std::fs::set_permissions(root.join("shared/letter.txt"), std::fs::Permissions::from_mode(0o444)).expect("chmod");
    std::fs::create_dir(root.join("Templates")).expect("templates");
    std::os::unix::fs::symlink("../shared/letter.txt", root.join("Templates/Letter.txt")).expect("link");
    let made = create_from_template(&root.join("Templates/Letter.txt"), &root, "Mine.txt").expect("created");
    let meta = std::fs::symlink_metadata(&made).expect("meta");
    assert!(meta.is_file(), "a regular file, not a link to the template");
    assert_ne!(meta.permissions().mode() & 0o200, 0, "the new document can be edited");
    assert_eq!(std::fs::read_to_string(&made).expect("content"), "Dear");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_rename_onto_another_name_of_the_same_file_is_refused() {
    let root = scratch("hardlink");
    put(&root.join("a.txt"), "a");
    std::fs::hard_link(root.join("a.txt"), root.join("b.txt")).expect("hard link");
    let refused = rename_item(&root.join("a.txt"), "b.txt").expect_err("the name is taken");
    assert_eq!(refused.kind(), std::io::ErrorKind::AlreadyExists);
    assert!(root.join("a.txt").exists() && root.join("b.txt").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn emptying_the_trash_clears_files_and_their_records() {
    let root = scratch("trash");
    let trash = Trash::new(root.join("Trash"));
    put(&root.join("doc.txt"), "x");
    put(&root.join("dir/inner.txt"), "y");
    let first = trash.put(&root.join("doc.txt")).expect("trashed");
    trash.put(&root.join("dir")).expect("trashed");
    put(&root.join("Trash/info/orphan.trashinfo"), "[Trash Info]\n");
    assert!(first.exists());
    trash.empty().expect("emptied");
    assert_eq!(trash.count(), 0);
    assert_eq!(std::fs::read_dir(root.join("Trash/info")).expect("info").count(), 0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn locations_are_read_from_paths_and_local_file_uris_only() {
    assert_eq!(path_from_arg("/tmp/odd://name"), Some(PathBuf::from("/tmp/odd://name")));
    assert_eq!(path_from_arg("file:///tmp/a%20b"), Some(PathBuf::from("/tmp/a b")));
    assert_eq!(path_from_arg("file:/tmp/a"), Some(PathBuf::from("/tmp/a")));
    assert_eq!(path_from_arg("file://localhost/tmp/a?x=1#frag"), Some(PathBuf::from("/tmp/a")));
    assert_eq!(path_from_arg("file://some-other-host.invalid/etc"), None);
    assert_eq!(path_from_arg("smb://server/share"), None);
}

#[test]
fn search_skips_kernel_trees_but_reaches_drives_under_run_media() {
    assert!(!searchable(Path::new("/proc")) && !searchable(Path::new("/sys")) && !searchable(Path::new("/dev")));
    assert!(!searchable(Path::new("/run/user")));
    assert!(searchable(Path::new("/run")) && searchable(Path::new("/run/media")));
    assert!(searchable(Path::new("/run/media/me/USB")) && searchable(Path::new("/home/me/run/user")));
}

fn event(mask: u32, name: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for field in [1_u32, mask, 0, name.len() as u32] {
        bytes.extend(field.to_ne_bytes());
    }
    bytes.extend(name);
    bytes
}

#[test]
fn the_watcher_notices_when_the_folder_itself_goes_away() {
    let created = event(libc::IN_CREATE, b"new.txt\0");
    let deleted = event(libc::IN_DELETE_SELF, b"");
    assert!(!lost_a_folder(&created));
    assert!(lost_a_folder(&[created.clone(), deleted].concat()));
    assert!(lost_a_folder(&event(libc::IN_IGNORED, b"")));
    assert!(!lost_a_folder(&created[..10]), "a cut-off record is ignored");
}

fn heard(stream: &mut iced::futures::channel::mpsc::UnboundedReceiver<()>, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if stream.try_recv().is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn a_folder_removed_and_made_again_keeps_being_followed() {
    let root = scratch("rewatch");
    let dir = root.join("build");
    std::fs::create_dir(&dir).expect("dir");
    let mut stream = changes(&vec![dir.clone()]);
    std::thread::sleep(Duration::from_millis(80));
    std::fs::remove_dir(&dir).expect("removed");
    assert!(heard(&mut stream, Duration::from_secs(3)), "the removal is reported");
    std::fs::create_dir(&dir).expect("made again");
    assert!(heard(&mut stream, Duration::from_secs(5)), "its return is reported");
    while stream.try_recv().is_ok() {}
    std::thread::sleep(Duration::from_millis(1500));
    while stream.try_recv().is_ok() {}
    put(&dir.join("new.txt"), "x");
    assert!(heard(&mut stream, Duration::from_secs(5)), "and so is what happens inside it afterwards");
    let _ = std::fs::remove_dir_all(root);
}

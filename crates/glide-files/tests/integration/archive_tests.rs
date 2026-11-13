use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use glide_files::fs::archive::{Format, available, compress, extract, is_archive, stem};
use glide_files::fs::jobs::{Answer, Choice, Progress, Question, Sink};

struct Quiet;

impl Sink for Quiet {
    fn progress(&self, _: Progress) {}

    fn ask(&self, _: Question) -> Answer {
        Answer { choice: Choice::Cancel, for_all: false }
    }
}

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("glide-archive-{tag}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn put(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent");
    }
    std::fs::write(path, text).expect("write");
}

fn names(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .expect("listing")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

#[test]
fn archives_are_known_by_their_suffix_whatever_its_case() {
    assert_eq!(stem("Звіти 2026.tar.gz"), Some("Звіти 2026"));
    assert_eq!(stem("PHOTOS.ZIP"), Some("PHOTOS"));
    assert_eq!(stem(".zip"), None);
    assert_eq!(stem("notes.gz"), None);
    assert!(is_archive(Path::new("/tmp/a.7z")));
    assert!(!is_archive(Path::new("/tmp/zip")));
}

#[test]
fn several_items_round_trip_through_every_format_into_a_folder_named_after_the_archive() {
    if !available() {
        return;
    }
    let cancel = AtomicBool::new(false);
    for format in Format::ALL {
        let root = scratch("round");
        put(&root.join("src/-dash.txt"), "dash");
        put(&root.join("src/@at.txt"), "at");
        put(&root.join("src/тека/вкладений файл.txt"), "вміст");
        let sources = vec![root.join("src/-dash.txt"), root.join("src/@at.txt"), root.join("src/тека")];
        let dest = root.join(format!("Архів{}", format.extension()));
        let packed = compress(&sources, &dest, format, &Quiet, &cancel);
        assert!(packed.failures.is_empty(), "{format:?}: {:?}", packed.failures);
        assert_eq!(packed.done, [(sources[0].clone(), dest.clone())]);
        assert_eq!(names(&root), [String::from("src"), format!("Архів{}", format.extension())]);

        let out = root.join("out");
        std::fs::create_dir(&out).expect("out");
        let unpacked = extract(std::slice::from_ref(&dest), &out, &Quiet, &cancel);
        assert!(unpacked.failures.is_empty(), "{format:?}: {:?}", unpacked.failures);
        assert_eq!(unpacked.done, [(dest.clone(), out.join("Архів"))]);
        assert_eq!(names(&out), ["Архів"], "nothing of the staging folder is left");
        assert_eq!(names(&out.join("Архів")), ["-dash.txt", "@at.txt", "тека"]);
        let inner = out.join("Архів/тека/вкладений файл.txt");
        assert_eq!(std::fs::read_to_string(inner).expect("inner"), "вміст");
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn an_archive_of_one_item_unpacks_to_that_item_under_a_free_name() {
    if !available() {
        return;
    }
    let cancel = AtomicBool::new(false);
    let root = scratch("single");
    put(&root.join("report.txt"), "new");
    let dest = root.join("report.zip");
    let packed = compress(&[root.join("report.txt")], &dest, Format::Zip, &Quiet, &cancel);
    assert!(packed.failures.is_empty());
    std::fs::write(root.join("report.txt"), "old").expect("edit");
    let unpacked = extract(std::slice::from_ref(&dest), &root, &Quiet, &cancel);
    assert_eq!(unpacked.done, [(dest, root.join("report (1).txt"))]);
    assert_eq!(std::fs::read_to_string(root.join("report.txt")).expect("kept"), "old");
    assert_eq!(std::fs::read_to_string(root.join("report (1).txt")).expect("new"), "new");
    assert_eq!(names(&root), ["report (1).txt", "report.txt", "report.zip"]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_broken_archive_is_reported_and_leaves_nothing_behind() {
    if !available() {
        return;
    }
    let cancel = AtomicBool::new(false);
    let root = scratch("broken");
    put(&root.join("bad.zip"), "this is not an archive");
    let report = extract(&[root.join("bad.zip")], &root, &Quiet, &cancel);
    assert!(report.done.is_empty());
    assert_eq!(report.failures.len(), 1);
    assert!(!report.failures[0].message.is_empty());
    assert_eq!(names(&root), ["bad.zip"]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_cancelled_compression_leaves_no_partial_archive() {
    if !available() {
        return;
    }
    let root = scratch("cancel");
    put(&root.join("a.txt"), "a");
    let cancel = AtomicBool::new(true);
    let report = compress(&[root.join("a.txt")], &root.join("a.zip"), Format::Zip, &Quiet, &cancel);
    assert!(report.cancelled);
    assert!(report.done.is_empty() && report.failures.is_empty());
    assert_eq!(names(&root), ["a.txt"]);
    let _ = std::fs::remove_dir_all(root);
}

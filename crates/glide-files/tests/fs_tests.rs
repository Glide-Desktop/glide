use std::io::ErrorKind;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use std::sync::atomic::AtomicBool;

use glide_files::fs::jobs::{Answer, Choice, Op, Progress, Question, Report, Sink, transfer};
use glide_files::fs::{
    FileCategory, FileEntry, LocalTime, SortBy, SortConfig, Trash, create_directory, days_from_civil, decode_path,
    encode_path, format_bytes, format_time, mounted_drives, natural_cmp, path_from_arg, permissions_string,
    query_disk_space, read_directory, rename_item, search_recursive, sort_entries, unique_path,
};
use glide_files::i18n::Lang;

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("glide-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Self(dir)
    }

    fn file(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().expect("file has a parent")).expect("mkdir");
        std::fs::write(&path, name).expect("write file");
        path
    }

    fn dir(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(&path).expect("mkdir");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn entry(name: &str, is_dir: bool, size: u64) -> FileEntry {
    FileEntry {
        name: name.to_string(),
        path: PathBuf::from("/tmp").join(name),
        is_dir,
        is_symlink: false,
        size,
        modified: Some(SystemTime::UNIX_EPOCH),
        mode: 0o644,
        category: if is_dir { FileCategory::Folder } else { FileCategory::Text },
        created: None,
        tags: Vec::new(),
    }
}

fn names(entries: &[FileEntry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

#[test]
fn categories_come_from_extension_or_exec_bit() {
    assert_eq!(FileCategory::from_extension("JPG"), FileCategory::Image);
    assert_eq!(FileCategory::from_extension("flac"), FileCategory::Audio);
    assert_eq!(FileCategory::from_extension("mkv"), FileCategory::Video);
    assert_eq!(FileCategory::from_extension("pdf"), FileCategory::Pdf);
    assert_eq!(FileCategory::from_extension("rs"), FileCategory::Code);
    assert_eq!(FileCategory::from_extension("zst"), FileCategory::Archive);
    assert_eq!(FileCategory::from_extension("so"), FileCategory::Binary);
    assert_eq!(FileCategory::from_extension("xyz"), FileCategory::Unknown);
    assert_eq!(FileCategory::classify(Path::new("/usr/bin/ls"), false, 0o755), FileCategory::Binary);
    assert_eq!(FileCategory::classify(Path::new("/src/LICENSE"), false, 0o644), FileCategory::Text);
    assert_eq!(FileCategory::classify(Path::new("/src/photos.d"), true, 0o755), FileCategory::Folder);
}

#[test]
fn byte_sizes_are_decimal_and_localised() {
    assert_eq!(format_bytes(0, Lang::En), "0 B");
    assert_eq!(format_bytes(999, Lang::Uk), "999 Б");
    assert_eq!(format_bytes(1_000, Lang::En), "1.0 kB");
    assert_eq!(format_bytes(1_500, Lang::Uk), "1,5 кБ");
    assert_eq!(format_bytes(999_950, Lang::En), "1.0 MB");
    assert_eq!(format_bytes(2_500_000_000, Lang::En), "2.5 GB");
    assert_eq!(format_bytes(7_000_000_000_000, Lang::Uk), "7,0 ТБ");
}

#[test]
fn natural_order_compares_digit_runs_numerically() {
    assert!(natural_cmp("file2", "file10").is_lt());
    assert!(natural_cmp("File10", "file9").is_gt());
    assert!(natural_cmp("img007", "img7").is_eq());
    assert!(natural_cmp("alpha", "Beta").is_lt());
    assert!(natural_cmp("a", "ab").is_lt());
}

#[test]
fn sorting_keeps_folders_first_in_both_directions() {
    let mut entries = vec![
        entry("zebra.txt", false, 500),
        entry("folder10", true, 0),
        entry("apple.txt", false, 100),
        entry("folder2", true, 0),
        entry("file10.txt", false, 100),
        entry("file9.txt", false, 100),
    ];
    sort_entries(&mut entries, SortConfig::default());
    assert_eq!(names(&entries), ["folder2", "folder10", "apple.txt", "file9.txt", "file10.txt", "zebra.txt"]);

    sort_entries(&mut entries, SortConfig { by: SortBy::Size, ascending: false });
    assert_eq!(names(&entries), ["folder2", "folder10", "zebra.txt", "apple.txt", "file9.txt", "file10.txt"]);
}

#[test]
fn directory_listing_hides_dotfiles_unless_asked() {
    let scratch = Scratch::new("listing");
    scratch.file("doc1.txt");
    scratch.file(".hidden");
    scratch.dir("sub");

    let visible = read_directory(&scratch.0, false, SortConfig::default()).expect("read");
    assert_eq!(names(&visible), ["sub", "doc1.txt"]);
    let all = read_directory(&scratch.0, true, SortConfig::default()).expect("read");
    assert_eq!(all.len(), 3);
}

#[test]
fn search_walks_breadth_first_and_honours_cancellation() {
    let scratch = Scratch::new("search");
    scratch.file("deep/deeper/report.txt");
    scratch.file("report-top.txt");
    scratch.file(".cache/report-hidden.txt");

    let found = search_recursive(&scratch.0, "REPORT", false, 10, &|| false);
    assert_eq!(names(&found), ["report-top.txt", "report.txt"]);

    let with_hidden = search_recursive(&scratch.0, "report", true, 10, &|| false);
    assert_eq!(with_hidden.len(), 3);

    let capped = search_recursive(&scratch.0, "report", true, 1, &|| false);
    assert_eq!(capped.len(), 1);

    assert!(search_recursive(&scratch.0, "report", false, 10, &|| true).is_empty());
    assert!(search_recursive(&scratch.0, "   ", false, 10, &|| false).is_empty());
}

#[test]
fn civil_days_and_relative_timestamps() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    assert_eq!(days_from_civil(1969, 12, 31), -1);

    let now = SystemTime::now();
    let today = LocalTime::of(now).expect("local time");
    let clock = format!("{:02}:{:02}", today.hour, today.minute);
    assert_eq!(format_time(now, now, Lang::En), format!("Today, {clock}"));
    assert_eq!(format_time(now - Duration::from_secs(86_400), now, Lang::Uk), format!("Учора, {clock}"));
    let old = format_time(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000), now, Lang::En);
    assert!(old.contains("Sep 2001"), "{old}");
}

#[test]
fn permissions_render_like_ls() {
    assert_eq!(permissions_string(0o100_754), "rwxr-xr-- (754)");
    assert_eq!(permissions_string(0o600), "rw------- (600)");
}

#[test]
fn uris_and_trash_paths_round_trip() {
    let path = Path::new("/tmp/My Dir/ünï%code#1");
    let encoded = encode_path(path);
    assert!(!encoded.contains(' '));
    assert_eq!(decode_path(&encoded), path);
    assert_eq!(path_from_arg("file:///tmp/My%20Dir"), Some(PathBuf::from("/tmp/My Dir")));
    assert_eq!(path_from_arg("file://localhost/etc"), Some(PathBuf::from("/etc")));
    assert_eq!(path_from_arg("https://example.com"), None);
    let here = std::env::current_dir().expect("cwd");
    assert_eq!(path_from_arg("docs"), Some(here.join("docs")), "a relative argument starts where the app did");
    assert_eq!(path_from_arg("."), Some(here.clone()));
    assert_eq!(path_from_arg("src/.."), Some(here));
    assert_eq!(path_from_arg("/tmp/../etc"), Some(PathBuf::from("/tmp/../etc")), "an absolute one is kept as given");
}

#[test]
fn unique_names_keep_extension_for_files_only() {
    let scratch = Scratch::new("unique");
    scratch.file("a.txt");
    scratch.file("a (1).txt");
    scratch.dir("my.folder");
    let taken = |path: &Path| path.exists();
    assert_eq!(unique_path(&scratch.0, "a.txt".as_ref(), true, taken), scratch.0.join("a (2).txt"));
    assert_eq!(unique_path(&scratch.0, "my.folder".as_ref(), false, taken), scratch.0.join("my.folder (1)"));
    assert_eq!(unique_path(&scratch.0, "free.txt".as_ref(), true, taken), scratch.0.join("free.txt"));
}

#[test]
fn names_are_validated_before_touching_disk() {
    let scratch = Scratch::new("names");
    let file = scratch.file("keep.txt");
    scratch.file("taken.txt");
    for bad in ["", "  ", ".", "..", "a/b"] {
        let err = rename_item(&file, bad).expect_err("invalid name must fail");
        assert_eq!(err.kind(), ErrorKind::InvalidFilename, "{bad:?}");
    }
    let err = rename_item(&file, "taken.txt").expect_err("existing name must fail");
    assert_eq!(err.kind(), ErrorKind::AlreadyExists);
    assert_eq!(rename_item(&file, "keep.txt").expect("same name"), file);
    std::fs::hard_link(&file, scratch.0.join("KEEP.txt")).expect("alias");
    assert!(
        rename_item(&file, "KEEP.txt").is_ok(),
        "a name that already is this file (case-insensitive FS) is not taken"
    );
    assert!(scratch.0.join("KEEP.txt").exists());
    let err = create_directory(&scratch.0, "taken.txt").expect_err("occupied");
    assert_eq!(err.kind(), ErrorKind::AlreadyExists);
}

struct Quiet;

impl Sink for Quiet {
    fn progress(&self, _: Progress) {}

    fn ask(&self, _: Question) -> Answer {
        Answer { choice: Choice::Cancel, for_all: false }
    }
}

fn paste(op: Op, paths: &[PathBuf], target: &Path) -> Report {
    transfer(op, paths, target, &Quiet, &AtomicBool::new(false))
}

#[test]
fn paste_refuses_to_nest_a_folder_inside_itself() {
    let scratch = Scratch::new("paste-self");
    let folder = scratch.dir("folder");
    scratch.file("folder/inner.txt");
    let report = paste(Op::Copy, std::slice::from_ref(&folder), &folder);
    assert!(report.done.is_empty());
    assert_eq!(report.failures[0].kind, ErrorKind::InvalidInput);
    assert_eq!(std::fs::read_dir(&folder).expect("read").count(), 1);
}

#[test]
fn paste_copies_beside_and_cut_in_place_is_a_no_op() {
    let scratch = Scratch::new("paste");
    let file = scratch.file("note.txt");
    let link = scratch.0.join("link");
    symlink("note.txt", &link).expect("symlink");

    let report = paste(Op::Copy, &[file.clone(), link], &scratch.0);
    assert!(report.failures.is_empty());
    let dests: Vec<PathBuf> = report.done.into_iter().map(|(_, dest)| dest).collect();
    assert_eq!(dests, [scratch.0.join("note (1).txt"), scratch.0.join("link (1)")]);
    let copied_link = std::fs::symlink_metadata(scratch.0.join("link (1)")).expect("meta");
    assert!(copied_link.file_type().is_symlink());

    let report = paste(Op::Move, std::slice::from_ref(&file), &scratch.0);
    assert!(report.done.is_empty());
    assert!(file.exists());

    let target = scratch.dir("target");
    let report = paste(Op::Move, std::slice::from_ref(&file), &target);
    assert_eq!(report.done[0].1, target.join("note.txt"));
    assert!(!file.exists());
}

#[test]
fn trashing_on_another_volume_stays_on_that_volume() {
    use std::os::unix::fs::MetadataExt;
    let (home, other) = (Path::new("/var/tmp"), Path::new("/dev/shm"));
    let dev = |path: &Path| std::fs::metadata(path).map(|meta| meta.dev()).ok();
    if dev(home).is_none() || dev(other).is_none() || dev(home) == dev(other) {
        return;
    }
    let stamp = format!("glide-trash-{}", std::process::id());
    let trash = Trash::new(home.join(&stamp).join("Trash"));
    let folder = other.join(&stamp);
    std::fs::create_dir_all(&folder).expect("folder");
    let file = folder.join("big note.txt");
    std::fs::write(&file, "x").expect("file");

    let trashed = trash.put(&file).expect("trash");
    assert_eq!(dev(&trashed), dev(other), "the file never left its volume");
    assert!(!trashed.starts_with(home));
    assert_eq!(trash.original_path(&trashed), Some(file.clone()));
    assert_eq!(trash.restore(&trashed).expect("restore"), file);
    assert!(file.exists());

    let trashed = trash.put(&file).expect("trash again");
    std::fs::remove_file(&trashed).expect("delete for good");
    trash.forget(&trashed);
    assert_eq!(trash.original_path(&trashed), None, "no orphaned trashinfo");

    let _ = std::fs::remove_dir_all(folder);
    let _ = std::fs::remove_dir_all(home.join(stamp));
}

#[test]
fn trash_on_mounted_volumes_is_found_counted_restorable_and_emptied() {
    let scratch = Scratch::new("volume-trash");
    let home = Trash::new(scratch.dir("home/Trash"));
    let usb = scratch.dir("usb");
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    let private = Trash::new(usb.join(format!(".Trash-{uid}")));
    std::fs::create_dir_all(private.files()).expect("files");
    let file = usb.join("photo.jpg");
    std::fs::write(&file, "x").expect("file");
    let trashed = private.put(&file).expect("trash on the stick");
    assert!(trashed.starts_with(usb.join(format!(".Trash-{uid}"))));

    assert!(home.contains(&usb.join(format!(".Trash-{uid}/files/Archive"))));
    assert!(home.contains(&usb.join(format!(".Trash/{uid}/files"))));
    assert!(!home.contains(&usb.join(".Trash/someone-else/files/x")), "another user's trash is not ours");
    assert!(!home.contains(&usb.join(".Trash-backup/files")), "nor a folder that only looks like a trash");
    assert!(!home.contains(&usb.join("Synced/.Trash/notes")), "nor a stray .Trash folder from another system");
    let found = home.on_volumes(&[usb, scratch.dir("empty-stick")]);
    assert_eq!(found, vec![private.clone()], "only volumes that hold a trash of ours are listed");
    assert!(home.is_top_level(&trashed), "an item in a volume trash can be restored from the Trash view");
    assert_eq!(found[0].count(), 1);
    assert_eq!(home.restore(&trashed).expect("restore"), file, "it goes back to the stick");
    let trashed = private.put(&file).expect("trash again");
    found[0].empty().expect("empty");
    assert!(!trashed.exists());
    assert_eq!(found[0].count(), 0, "emptying the Trash frees the space on the stick");
}

#[test]
fn trash_round_trip_preserves_paths_and_never_overwrites() {
    let scratch = Scratch::new("trash");
    let trash = Trash::new(scratch.0.join("Trash"));
    let original = scratch.file("My Docs/report 1.txt");

    let trashed = trash.put(&original).expect("trash");
    assert!(!original.exists());
    assert!(trash.contains(&trashed));
    assert!(trash.is_top_level(&trashed));
    assert_eq!(trash.count(), 1);
    assert_eq!(trash.original_path(&trashed), Some(original.clone()));

    std::fs::write(&original, "newer").expect("recreate original");
    let restored = trash.restore(&trashed).expect("restore");
    assert_eq!(restored, scratch.0.join("My Docs/report 1 (1).txt"));
    assert_eq!(std::fs::read_to_string(&original).expect("read"), "newer");
    assert_eq!(trash.count(), 0);

    let first = trash.put(&original).expect("trash first");
    let again = scratch.file("My Docs/report 1.txt");
    let second = trash.put(&again).expect("trash second");
    assert_ne!(first, second);
    assert_eq!(trash.count(), 2);

    std::fs::remove_file(&first).expect("erase");
    trash.forget(&first);
    assert_eq!(trash.original_path(&first), None);
    assert_eq!(trash.count(), 1);

    trash.empty().expect("empty");
    assert_eq!(trash.count(), 0);
    assert_eq!(std::fs::read_dir(scratch.0.join("Trash/info")).expect("info dir").count(), 0);
}

#[test]
fn trashing_a_symlink_moves_the_link_not_its_target() {
    let scratch = Scratch::new("trash-link");
    let trash = Trash::new(scratch.0.join("Trash"));
    let target = scratch.file("real/data.txt");
    let link = scratch.0.join("shortcut");
    symlink(&target, &link).expect("symlink");

    let trashed = trash.put(&link).expect("trash link");
    assert!(target.exists());
    assert_eq!(trash.original_path(&trashed), Some(link.clone()));
    trash.restore(&trashed).expect("restore link");
    assert!(std::fs::symlink_metadata(&link).expect("link back").file_type().is_symlink());
}

#[test]
fn restoring_a_nested_trash_item_reports_not_found() {
    let scratch = Scratch::new("trash-nested");
    let trash = Trash::new(scratch.0.join("Trash"));
    let folder = scratch.dir("box");
    scratch.file("box/inside.txt");
    let trashed = trash.put(&folder).expect("trash folder");
    let err = trash.restore(&trashed.join("inside.txt")).expect_err("nested items have no trashinfo");
    assert_eq!(err.kind(), ErrorKind::NotFound);
}

#[test]
fn a_file_outside_the_trash_is_never_restored() {
    let scratch = Scratch::new("trash-outside");
    let trash = Trash::new(scratch.0.join("Trash"));
    let old = scratch.file("old/notes.txt");
    trash.put(&old).expect("trash");
    let current = scratch.file("work/notes.txt");
    let err = trash.restore(&current).expect_err("a file that was never trashed has nothing to restore");
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert!(current.exists(), "it stays where it is");
    assert!(!old.exists());
    assert_eq!(trash.count(), 1, "and the trashed namesake keeps its record");
}

#[test]
fn disk_space_and_root_drive_are_reported() {
    let (free, total) = query_disk_space(Path::new("/")).expect("statvfs /");
    assert!(total > 0);
    assert!(free <= total);
    let drives = mounted_drives(Lang::En);
    assert_eq!(drives[0].path, Path::new("/"));
    assert_eq!(drives[0].name, "File System");
}

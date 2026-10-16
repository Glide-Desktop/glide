use std::path::{Path, PathBuf};

use glide_files::ViewMode;
use glide_files::config::{ListColumn, ViewPrefs};
use glide_files::fs::mtp::devices_in;
use glide_files::fs::tags::{self, Color};
use glide_files::fs::{create_from_template, create_link, templates_in, write_new_file};
use glide_files::widgets::content::Metrics;

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("glide-features-{tag}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn put(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent");
    }
    std::fs::write(path, text).expect("write");
}

#[test]
fn tags_parse_dedupe_and_split_colours_from_words() {
    let mut parsed = tags::parse(" Red, work ,red,, Blue ,Work");
    assert_eq!(parsed, ["Red", "work", "Blue"]);
    assert_eq!(tags::colors(&parsed), [Color::Red, Color::Blue]);
    assert_eq!(tags::words(&parsed), ["work"]);
    tags::set(&mut parsed, "red", false);
    tags::set(&mut parsed, "Green", true);
    assert_eq!(tags::render(&parsed), "work,Blue,Green");
    assert_eq!(Color::of(" purple "), Some(Color::Purple));
    assert_eq!(Color::of("magenta"), None);
}

#[test]
fn tags_round_trip_through_extended_attributes_where_supported() {
    let root = scratch("xattr");
    let file = root.join("a.txt");
    put(&file, "a");
    let wanted = vec![String::from("Orange"), String::from("taxes")];
    if tags::write(&file, &wanted).is_err() {
        let _ = std::fs::remove_dir_all(root);
        return;
    }
    assert_eq!(tags::read(&file), wanted);
    let entry = glide_files::fs::FileEntry::from_path(file.clone()).expect("entry");
    assert_eq!(entry.tags, wanted, "listings carry the tags");
    tags::write(&file, &[]).expect("clearing tags");
    assert!(tags::read(&file).is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn links_duplicates_and_pasted_files_get_unique_names() {
    let root = scratch("create");
    let source = root.join("report.pdf");
    put(&source, "pdf");
    let first = create_link(&source, &root, "Link to").expect("link");
    assert_eq!(first, root.join("Link to report.pdf"));
    assert_eq!(std::fs::read_link(&first).expect("symlink"), source);
    let second = create_link(&source, &root, "Link to").expect("second link");
    assert_eq!(second, root.join("Link to report (1).pdf"));

    let pasted = write_new_file(&root, "Pasted image.png", b"png").expect("pasted");
    let again = write_new_file(&root, "Pasted image.png", b"png2").expect("pasted again");
    assert_eq!(pasted, root.join("Pasted image.png"));
    assert_eq!(again, root.join("Pasted image (1).png"));
    assert_eq!(std::fs::read(&again).expect("bytes"), b"png2");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn templates_are_listed_nested_and_copied_under_a_new_name() {
    let root = scratch("templates");
    let templates = root.join("Templates");
    put(&templates.join("Letter.odt"), "letter");
    put(&templates.join("Office/Sheet.ods"), "sheet");
    put(&templates.join(".hidden"), "x");
    let found = templates_in(&templates);
    assert_eq!(found, [templates.join("Letter.odt"), templates.join("Office/Sheet.ods")]);

    let made = create_from_template(&found[0], &root, "Note.odt").expect("from template");
    assert_eq!(std::fs::read_to_string(&made).expect("copy"), "letter");
    assert!(create_from_template(&found[0], &root, "Note.odt").is_err(), "never overwrites");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn phones_and_cameras_are_found_in_sysfs_and_hubs_are_ignored() {
    let root = scratch("sysfs");
    let device = |name: &str, fields: &[(&str, &str)], interface: &[(&str, &str)]| {
        for (key, value) in fields {
            put(&root.join(name).join(key), value);
        }
        for (key, value) in interface {
            put(&root.join(name).join(format!("{name}:1.0")).join(key), value);
        }
    };
    let usb = [("busnum", "1"), ("devnum", "7"), ("manufacturer", "Google"), ("product", "Pixel 8"), ("serial", "ABC")];
    device("1-2", &usb, &[("interface", "MTP"), ("bInterfaceClass", "ff")]);
    device("1-3", &[("busnum", "1"), ("devnum", "9"), ("product", "EOS")], &[("bInterfaceClass", "06")]);
    device("1-4", &[("busnum", "1"), ("devnum", "2"), ("product", "Hub")], &[("bInterfaceClass", "09")]);
    let found = devices_in(&root);
    assert_eq!(found.len(), 2);
    assert_eq!((found[0].name.as_str(), found[0].camera, found[0].serial.as_str()), ("Google Pixel 8", false, "ABC"));
    assert_eq!(found[0].uri(), "mtp://[usb:001,007]/");
    assert!(found[1].camera);
    assert_eq!(found[1].uri(), "gphoto2://[usb:001,009]/");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn zoom_levels_scale_the_grid_and_list_geometry() {
    let normal = Metrics::default();
    assert_eq!((normal.tile_w, normal.tile_h, normal.icon), (104.0, 122.0, 60.0), "the default look is unchanged");
    assert_eq!(normal.plate - normal.icon, 16.0, "an 8 px margin leaves room for the hover lift");
    let big = Metrics::new(5, 3);
    let small = Metrics::new(0, 0);
    assert!(big.columns(1200.0) < normal.columns(1200.0));
    assert!(small.columns(1200.0) > normal.columns(1200.0));
    assert!(big.row > normal.row && small.row < normal.row);
    let (top, bottom) = big.span(ViewMode::List, 3, 800.0);
    assert!((bottom - top - big.row - 8.0).abs() < f32::EPSILON);
    assert_eq!(Metrics::new(99, 99), Metrics::new(5, 3), "levels clamp");
}

#[test]
fn column_preferences_keep_order_drop_duplicates_and_unknown_keys() {
    let prefs = ViewPrefs::parse("columns = tags,size,bogus,size,permissions\n");
    assert_eq!(prefs.columns, [ListColumn::Tags, ListColumn::Size, ListColumn::Permissions]);
    assert!(!prefs.queue, "queued operations stay off unless asked for");
    assert_eq!(ViewPrefs::default().columns, ListColumn::DEFAULT);
}

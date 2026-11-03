use std::path::{Path, PathBuf};

use glide_files::ViewMode;
use glide_files::config::ViewPrefs;
use glide_files::fs::mime::MimeDb;
use glide_files::fs::thumbs::{SIZE, cache_name, can_thumbnail, make_in};
use glide_files::fs::{SortBy, SortConfig};

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("glide-stage5-{tag}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn view_preferences_round_trip_and_ignore_junk() {
    let prefs = ViewPrefs {
        view: ViewMode::List,
        sort: SortConfig { by: SortBy::Modified, ascending: false },
        show_hidden: true,
        grid_zoom: 3,
        list_zoom: 0,
        columns: vec![glide_files::config::ListColumn::Tags, glide_files::config::ListColumn::Size],
        queue: true,
        inspector: false,
    };
    assert_eq!(ViewPrefs::parse(&prefs.render()), prefs);
    assert_eq!(ViewPrefs::parse("garbage\nsort = sideways\ngrid-zoom = 99\n"), ViewPrefs::default());
    let legacy = |level: u8| ViewPrefs::parse(&format!("grid-zoom = {level}\n")).grid_zoom;
    assert_eq!([0, 1, 2, 3, 4, 5].map(legacy), [0, 0, 1, 2, 3, 3], "old six-step sizes map to the nearest of four");
    assert_eq!(ViewPrefs::parse("grid-zoom = 5\ngrid-size = 0\n").grid_zoom, 0, "the new key wins");
}

#[test]
fn icons_come_from_the_mime_database_before_generic_fallbacks() {
    let dir = scratch("mime");
    std::fs::create_dir_all(dir.join("mime")).expect("mime dir");
    std::fs::write(dir.join("mime/globs2"), "50:text/x-rust:*.rs\n").expect("globs");
    std::fs::write(dir.join("mime/generic-icons"), "text/x-rust:text-x-script\n").expect("generic");
    let db = MimeDb::load(std::slice::from_ref(&dir));
    let mime = db.of(Path::new("/src/main.rs"), false);
    assert_eq!(db.icon_names(&mime), ["text-x-rust", "text-x-script"]);
    assert_eq!(db.icon_names("image/png"), ["image-png", "image-x-generic"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn images_get_a_cached_thumbnail_that_tracks_changes() {
    let dir = scratch("thumbs");
    let photo = dir.join("photo.png");
    image::RgbImage::from_pixel(640, 320, image::Rgb([200, 40, 40])).save(&photo).expect("png");
    let cache = dir.join("cache");
    assert!(can_thumbnail("image/png", &[]));
    assert!(!can_thumbnail("text/plain", &[]));
    let thumb = make_in(&cache, &photo, "image/png", &[]).expect("thumbnail");
    let (width, height) = image::image_dimensions(&thumb).expect("dimensions");
    assert_eq!((width, height), (SIZE, SIZE / 2), "aspect ratio is kept");
    assert_eq!(make_in(&cache, &photo, "image/png", &[]), Some(thumb));

    let broken = dir.join("broken.png");
    std::fs::write(&broken, b"not a png").expect("broken");
    assert_eq!(make_in(&cache, &broken, "image/png", &[]), None);
    assert_ne!(cache_name(&photo, 1, 10), cache_name(&photo, 2, 10), "a modified file gets a fresh thumbnail");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_thumbnail_cache_drops_what_is_stale_or_over_its_limit() {
    use glide_files::fs::thumbs::prune_in;
    use std::time::{Duration, SystemTime};
    let dir = scratch("prune");
    let now = SystemTime::now();
    let day = Duration::from_hours(24);
    let put = |name: &str, age: Duration| {
        let path = dir.join(name);
        std::fs::write(&path, [0_u8; 10]).expect("thumb");
        let when = now - age;
        let file = std::fs::File::options().write(true).open(&path).expect("open");
        file.set_times(std::fs::FileTimes::new().set_accessed(when).set_modified(when)).expect("times");
    };
    put("ancient.png", day * 200);
    put("old.png", day * 3);
    put("recent.png", day);
    put("fresh.png", Duration::ZERO);
    prune_in(&dir, now, day * 90, 1000);
    assert!(!dir.join("ancient.png").exists(), "unused for longer than the keep period");
    assert!(dir.join("old.png").exists());
    prune_in(&dir, now, day * 90, 25);
    assert!(!dir.join("old.png").exists(), "over the limit, the least recently used go first");
    assert!(dir.join("recent.png").exists() && dir.join("fresh.png").exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_pipe_named_like_an_image_is_not_read_for_a_thumbnail() {
    let dir = scratch("fifo-thumb");
    let pipe = dir.join("photo.png");
    let raw = std::ffi::CString::new(pipe.to_string_lossy().as_bytes()).expect("path");
    // SAFETY: `raw` is a NUL-terminated path string.
    assert_eq!(unsafe { libc::mkfifo(raw.as_ptr(), 0o600) }, 0);
    let cache = dir.join("cache");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(make_in(&cache, &pipe, "image/png", &[]));
    });
    let made = receiver.recv_timeout(std::time::Duration::from_secs(3));
    assert_eq!(made, Ok(None), "the worker would otherwise wait for a writer forever");
    let _ = std::fs::remove_dir_all(dir);
}

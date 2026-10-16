use std::path::PathBuf;
use std::time::{Duration, Instant};

use glide_files::fs::watch::changes;
use glide_files::widgets::content::visible_rows;

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("glide-files-{tag}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn next_change(dirs: &[PathBuf], act: impl FnOnce()) -> bool {
    let mut stream = changes(&dirs.to_vec());
    std::thread::sleep(Duration::from_millis(50));
    act();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if stream.try_recv().is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn the_watcher_reports_files_created_and_removed() {
    let dir = scratch("watch");
    let file = dir.join("new.txt");
    assert!(next_change(std::slice::from_ref(&dir), || std::fs::write(&file, b"x").expect("write")));
    assert!(next_change(std::slice::from_ref(&dir), || std::fs::remove_file(&file).expect("remove")));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_trash_view_hears_a_trash_on_another_volume() {
    let home = scratch("watch-home-trash");
    let usb = scratch("watch-usb-trash");
    let trashed = usb.join("photo.jpg");
    assert!(next_change(&[home.clone(), usb.clone()], || std::fs::write(&trashed, b"x").expect("write")));
    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(usb);
}

#[test]
fn only_rows_near_the_viewport_are_built() {
    assert_eq!(visible_rows(1000, 100.0, 20.0, 0.0, 500.0), 0..7);
    let middle = visible_rows(1000, 100.0, 20.0, 50_000.0, 500.0);
    assert!(middle.start <= 499 && middle.end >= 506);
    assert!(middle.len() < 12);
    assert_eq!(visible_rows(3, 100.0, 20.0, 0.0, 5000.0), 0..3);
    assert_eq!(visible_rows(0, 100.0, 20.0, 0.0, 500.0), 0..0);
}

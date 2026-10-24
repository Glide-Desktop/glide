use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub fn write_atomic(path: &Path, contents: &[u8], new_mode: u32) -> io::Result<()> {
    let target = resolve_symlinks(path);
    let mode = fs::metadata(&target).map_or(new_mode, |meta| meta.permissions().mode() & 0o7777);
    replace(&target, contents, mode)
}

pub fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    replace(&resolve_symlinks(path), contents, 0o600)
}

fn replace(target: &Path, contents: &[u8], mode: u32) -> io::Result<()> {
    let folder = target.parent().filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(folder)?;
    let temporary = temp_sibling(target);
    let written = (|| {
        let mut file = fs::OpenOptions::new().write(true).create_new(true).mode(mode).open(&temporary)?;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temporary, target)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written?;
    if let Ok(folder) = fs::File::open(folder) {
        let _ = folder.sync_all();
    }
    Ok(())
}

fn temp_sibling(target: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut name = OsString::from(".");
    name.push(target.file_name().unwrap_or_default());
    name.push(format!(".{}.{}.glide.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    target.with_file_name(name)
}

fn resolve_symlinks(target: &Path) -> PathBuf {
    let mut resolved = target.to_path_buf();
    for _ in 0..40 {
        let Ok(link) = fs::read_link(&resolved) else {
            break;
        };
        resolved = match resolved.parent() {
            Some(parent) if link.is_relative() => parent.join(link),
            _ => link,
        };
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("glide-file-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir is writable");
        dir
    }

    fn names(dir: &Path) -> Vec<OsString> {
        let mut names: Vec<_> =
            fs::read_dir(dir).expect("readable").filter_map(Result::ok).map(|e| e.file_name()).collect();
        names.sort();
        names
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).expect("exists").permissions().mode() & 0o777
    }

    #[test]
    fn rewriting_keeps_the_latest_contents_and_mode_and_leaves_no_temp_file() {
        let dir = scratch("atomic");
        let target = dir.join("nested/glide.toml");
        write_atomic(&target, b"first = 1\n", 0o644).expect("first write");
        assert_eq!(mode(&target), 0o644);
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).expect("chmod");
        write_atomic(&target, b"second = 2\n", 0o644).expect("second write");
        assert_eq!(fs::read_to_string(&target).expect("exists"), "second = 2\n");
        assert_eq!(mode(&target), 0o640);
        assert_eq!(names(&dir.join("nested")), [OsString::from("glide.toml")]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_private_file_is_owner_only_and_a_symlink_is_written_through() {
        let dir = scratch("private");
        let real = dir.join("real.toml");
        fs::write(&real, "old").expect("seed");
        fs::set_permissions(&real, fs::Permissions::from_mode(0o644)).expect("chmod");
        std::os::unix::fs::symlink("real.toml", dir.join("link.toml")).expect("symlink");
        write_private(&dir.join("link.toml"), b"secret").expect("write");
        assert_eq!(fs::read_to_string(&real).expect("exists"), "secret");
        assert_eq!(mode(&real), 0o600);
        assert!(fs::symlink_metadata(dir.join("link.toml")).expect("link").file_type().is_symlink());
        let _ = fs::remove_dir_all(&dir);
    }
}

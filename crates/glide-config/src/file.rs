use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const BORROWED_NAME_BYTES: usize = 100;
const DROPPED_ATTRIBUTES: &[&[u8]] = &[b"security.capability"];

pub fn write_atomic(path: &Path, contents: &[u8], new_mode: u32) -> io::Result<()> {
    let target = resolve_symlinks(path)?;
    let mode = fs::metadata(&target).map_or(new_mode, |meta| meta.permissions().mode() & 0o7777);
    replace(&target, contents, mode, true)
}

pub fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    replace(&resolve_symlinks(path)?, contents, 0o600, false)
}

fn replace(target: &Path, contents: &[u8], mode: u32, keep_attributes: bool) -> io::Result<()> {
    let folder = target.parent().filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(folder)?;
    let temporary = temp_sibling(target);
    let mut file = fs::OpenOptions::new().write(true).create_new(true).mode(mode).open(&temporary)?;
    let written = (|| {
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.write_all(contents)?;
        if keep_attributes {
            copy_attributes(target, file.as_raw_fd());
        }
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
    name.push(OsStr::from_bytes(borrowed_name(target.file_name().unwrap_or_default().as_bytes())));
    name.push(format!(".{}.{}.glide.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    target.with_file_name(name)
}

fn borrowed_name(name: &[u8]) -> &[u8] {
    let mut end = name.len().min(BORROWED_NAME_BYTES);
    while end > 0 && end < name.len() && name[end] & 0xC0 == 0x80 {
        end -= 1;
    }
    &name[..end]
}

fn copy_attributes(from: &Path, to: RawFd) {
    let Ok(source) = CString::new(from.as_os_str().as_bytes()) else {
        return;
    };
    for name in attribute_names(&source) {
        if DROPPED_ATTRIBUTES.contains(&name.to_bytes()) {
            continue;
        }
        if let Some(value) = attribute_value(&source, &name) {
            set_attribute(to, &name, &value);
        }
    }
}

fn attribute_names(path: &CStr) -> Vec<CString> {
    // SAFETY: `path` is a NUL-terminated string and a null buffer with size 0 only asks for the needed length.
    let needed = unsafe { libc::listxattr(path.as_ptr(), std::ptr::null_mut(), 0) };
    let Ok(needed) = usize::try_from(needed) else {
        return Vec::new();
    };
    if needed == 0 {
        return Vec::new();
    }
    let mut buffer = vec![0u8; needed];
    // SAFETY: `buffer` is writable for `buffer.len()` bytes and `path` is NUL-terminated.
    let filled = unsafe { libc::listxattr(path.as_ptr(), buffer.as_mut_ptr().cast(), buffer.len()) };
    let Ok(filled) = usize::try_from(filled) else {
        return Vec::new();
    };
    buffer.truncate(filled);
    buffer.split(|byte| *byte == 0).filter(|name| !name.is_empty()).filter_map(|name| CString::new(name).ok()).collect()
}

fn attribute_value(path: &CStr, name: &CStr) -> Option<Vec<u8>> {
    // SAFETY: `path` and `name` are NUL-terminated and a null buffer with size 0 only asks for the needed length.
    let needed = unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0) };
    let mut value = vec![0u8; usize::try_from(needed).ok()?];
    if value.is_empty() {
        return Some(value);
    }
    // SAFETY: `value` is writable for `value.len()` bytes; `path` and `name` are NUL-terminated.
    let filled = unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), value.as_mut_ptr().cast(), value.len()) };
    value.truncate(usize::try_from(filled).ok()?);
    Some(value)
}

fn set_attribute(fd: RawFd, name: &CStr, value: &[u8]) -> bool {
    // SAFETY: `name` is NUL-terminated and `value` is readable for `value.len()` bytes; a bad `fd` only fails the call.
    unsafe { libc::fsetxattr(fd, name.as_ptr(), value.as_ptr().cast(), value.len(), 0) == 0 }
}

pub(crate) fn locked<T>(path: &Path, work: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    let folder = path.parent().filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(folder)?;
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".lock");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(folder.join(name))?;
    loop {
        // SAFETY: flock only takes an advisory lock on the live descriptor, released when `lock` is dropped.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } == 0 {
            break;
        }
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::Interrupted {
            return Err(err);
        }
    }
    let result = work();
    drop(lock);
    result
}

fn resolve_symlinks(target: &Path) -> io::Result<PathBuf> {
    let mut resolved = target.to_path_buf();
    for depth in 0..=40 {
        let link = match fs::read_link(&resolved) {
            Ok(link) => link,
            Err(err) if matches!(err.kind(), io::ErrorKind::NotFound | io::ErrorKind::InvalidInput) => {
                return Ok(resolved);
            }
            Err(err) => return Err(err),
        };
        if depth == 40 {
            return Err(io::Error::from_raw_os_error(libc::ELOOP));
        }
        resolved = match resolved.parent() {
            Some(parent) if link.is_relative() => parent.join(link),
            _ => link,
        };
    }
    unreachable!()
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

    fn long_name() -> String {
        let name = format!("a{}.txt", "\u{436}".repeat(117));
        assert_eq!(name.len(), 239, "a Cyrillic name just under NAME_MAX");
        name
    }

    fn cpath(path: &Path) -> CString {
        CString::new(path.as_os_str().as_bytes()).expect("no NUL in a path")
    }

    fn tag_supported(path: &Path, name: &CStr, value: &[u8]) -> bool {
        let file = fs::OpenOptions::new().write(true).open(path).expect("seed file opens");
        set_attribute(file.as_raw_fd(), name, value)
    }

    #[test]
    fn the_temporary_name_of_a_long_target_stays_within_name_max_on_a_char_boundary() {
        let dir = Path::new("/some/folder");
        for name in [long_name(), "b".repeat(255), format!("\u{436}{}", "\u{1f600}".repeat(60))] {
            let temporary = temp_sibling(&dir.join(&name));
            let file_name = temporary.file_name().expect("has a name");
            assert!(file_name.len() <= 255, "{} bytes for a {}-byte target", file_name.len(), name.len());
            assert!(file_name.to_str().is_some(), "the borrowed part is cut on a character boundary");
            assert_eq!(temporary.parent(), Some(dir));
        }
        let short = temp_sibling(&dir.join("glide.toml"));
        assert!(short.file_name().and_then(OsStr::to_str).is_some_and(|name| name.starts_with(".glide.toml.")));
    }

    #[test]
    fn a_target_with_a_name_near_the_limit_can_still_be_saved() {
        let dir = scratch("longname");
        let target = dir.join(long_name());
        write_atomic(&target, b"first", 0o644).expect("first write");
        write_atomic(&target, b"second", 0o644).expect("rewrite");
        assert_eq!(fs::read_to_string(&target).expect("exists"), "second");
        assert_eq!(names(&dir), [OsString::from(long_name())]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rewriting_keeps_extended_attributes_such_as_the_tags_glide_files_stores() {
        let dir = scratch("xattr");
        let target = dir.join("notes.txt");
        fs::write(&target, "old").expect("seed");
        let name = CString::new("user.xdg.tags").expect("name");
        if !tag_supported(&target, &name, b"red,work") {
            let _ = fs::remove_dir_all(&dir);
            return;
        }
        write_atomic(&target, b"new", 0o644).expect("write");
        assert_eq!(fs::read_to_string(&target).expect("exists"), "new");
        assert_eq!(attribute_value(&cpath(&target), &name), Some(b"red,work".to_vec()));
        assert_eq!(names(&dir), [OsString::from("notes.txt")]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_private_rewrite_starts_without_the_old_files_attributes() {
        let dir = scratch("xattr-private");
        let target = dir.join("secret.toml");
        fs::write(&target, "old").expect("seed");
        let name = CString::new("user.test").expect("name");
        if !tag_supported(&target, &name, b"1") {
            let _ = fs::remove_dir_all(&dir);
            return;
        }
        write_private(&target, b"new").expect("write");
        assert_eq!(attribute_value(&cpath(&target), &name), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_target_that_does_not_exist_yet_has_no_attributes_to_copy() {
        assert!(attribute_names(&cpath(Path::new("/nonexistent/glide-file-test"))).is_empty());
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

    #[test]
    fn cyclic_symlinks_are_refused_without_replacing_either_link() {
        let dir = scratch("cycle");
        let first = dir.join("first");
        let second = dir.join("second");
        std::os::unix::fs::symlink("second", &first).unwrap();
        std::os::unix::fs::symlink("first", &second).unwrap();
        for result in [write_atomic(&first, b"new", 0o644), write_private(&first, b"secret")] {
            assert_eq!(result.unwrap_err().raw_os_error(), Some(libc::ELOOP));
        }
        assert_eq!(fs::read_link(&first).unwrap(), Path::new("second"));
        assert_eq!(fs::read_link(&second).unwrap(), Path::new("first"));
        assert_eq!(names(&dir), [OsString::from("first"), OsString::from("second")]);
        fs::remove_dir_all(dir).unwrap();
    }
}

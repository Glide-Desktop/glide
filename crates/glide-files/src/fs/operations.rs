use std::ffi::{OsStr, OsString};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardAction {
    Copy,
    Cut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileClipboard {
    pub action: ClipboardAction,
    pub paths: Vec<PathBuf>,
}

pub fn validate_name(name: &str) -> io::Result<&str> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']) {
        return Err(io::Error::new(ErrorKind::InvalidFilename, "invalid name"));
    }
    Ok(name)
}

#[must_use]
pub fn occupied(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let id = |path: &Path| std::fs::symlink_metadata(path).map(|meta| (meta.dev(), meta.ino())).ok();
    id(a).is_some_and(|first| Some(first) == id(b))
}

pub fn create_directory(parent: &Path, name: &str) -> io::Result<PathBuf> {
    let target = parent.join(validate_name(name)?);
    std::fs::create_dir(&target)?;
    Ok(target)
}

pub fn create_empty_file(parent: &Path, name: &str) -> io::Result<PathBuf> {
    let target = parent.join(validate_name(name)?);
    std::fs::OpenOptions::new().write(true).create_new(true).open(&target)?;
    Ok(target)
}

pub fn rename_item(target: &Path, new_name: &str) -> io::Result<PathBuf> {
    let name = validate_name(new_name)?;
    let parent = target.parent().ok_or_else(|| io::Error::new(ErrorKind::InvalidFilename, "no parent directory"))?;
    let renamed = parent.join(name);
    if renamed == target {
        return Ok(renamed);
    }
    if occupied(&renamed) && !same_file(target, &renamed) {
        return Err(io::Error::new(ErrorKind::AlreadyExists, "name taken"));
    }
    std::fs::rename(target, &renamed)?;
    Ok(renamed)
}

pub fn create_link(target: &Path, dir: &Path, prefix: &str) -> io::Result<PathBuf> {
    let name = target.file_name().ok_or_else(|| io::Error::from(ErrorKind::InvalidFilename))?;
    let label = format!("{prefix} {}", name.to_string_lossy());
    let dest = unique_path(dir, OsStr::new(&label), !target.is_dir(), occupied);
    std::os::unix::fs::symlink(target, &dest)?;
    Ok(dest)
}

pub fn create_from_template(template: &Path, dir: &Path, name: &str) -> io::Result<PathBuf> {
    let dest = dir.join(validate_name(name)?);
    if occupied(&dest) {
        return Err(io::Error::new(ErrorKind::AlreadyExists, "name taken"));
    }
    super::jobs::copy_exact(template, &dest)?;
    Ok(dest)
}

pub fn write_new_file(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    use std::io::Write;
    let dest = unique_path(dir, OsStr::new(name), true, occupied);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&dest)?;
    file.write_all(bytes)?;
    Ok(dest)
}

pub fn move_path(source: &Path, dest: &Path) -> io::Result<()> {
    match std::fs::rename(source, dest) {
        Err(err) if err.kind() == ErrorKind::CrossesDevices => {
            super::jobs::copy_exact(source, dest)?;
            remove_recursive(source)
        }
        result => result,
    }
}

pub fn remove_recursive(path: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(path)?.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }
}

#[must_use]
pub fn unique_path(dir: &Path, name: &OsStr, keep_extension: bool, taken: impl Fn(&Path) -> bool) -> PathBuf {
    let candidate = dir.join(name);
    if !taken(&candidate) {
        return candidate;
    }
    let original = Path::new(name);
    let (stem, extension) = match (original.file_stem(), original.extension()) {
        (Some(stem), Some(ext)) if keep_extension => (stem, Some(ext)),
        _ => (name, None),
    };
    let mut counter = 1_u32;
    loop {
        let mut file = OsString::from(stem);
        file.push(format!(" ({counter})"));
        if let Some(ext) = extension {
            file.push(".");
            file.push(ext);
        }
        let candidate = dir.join(file);
        if !taken(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

pub fn open_with_default(path: &Path) -> io::Result<()> {
    let status =
        Command::new("xdg-open").arg(path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status()?;
    if status.success() { Ok(()) } else { Err(io::Error::other(format!("xdg-open exited with {status}"))) }
}

pub fn open_in_terminal(dir: &Path) -> io::Result<()> {
    let terminal = glide_config::terminal::current();
    let mut words = terminal.split_whitespace();
    let program = words.next().ok_or_else(|| io::Error::new(ErrorKind::NotFound, "no terminal emulator"))?;
    let mut child = Command::new(program).args(words).current_dir(dir).stdin(Stdio::null()).spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[must_use]
pub fn query_disk_space(path: &Path) -> Option<(u64, u64)> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `c_path` is NUL-terminated and `stat` is writable storage for one `statvfs`.
    if unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: `statvfs` returned 0, so it initialised `stat`.
    let stat = unsafe { stat.assume_init() };
    Some((stat.f_bavail * stat.f_frsize, stat.f_blocks * stat.f_frsize))
}

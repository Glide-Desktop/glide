use std::ffi::{OsStr, OsString};
use std::io::{self, BufRead, BufReader, ErrorKind};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use super::jobs::{Failure, Progress, Report, Sink, tree_size};
use super::operations::{occupied, remove_recursive, unique_path};

pub const TOOL: &str = "bsdtar";
const POLL: Duration = Duration::from_millis(60);
const SUFFIXES: [&str; 28] = [
    ".tar.gz",
    ".tar.xz",
    ".tar.bz2",
    ".tar.zst",
    ".tar.lz",
    ".tar.lzma",
    ".tar.lz4",
    ".tar.Z",
    ".tgz",
    ".tbz2",
    ".tbz",
    ".txz",
    ".tzst",
    ".tar",
    ".zip",
    ".jar",
    ".7z",
    ".rar",
    ".iso",
    ".cab",
    ".cpio",
    ".rpm",
    ".deb",
    ".lzh",
    ".lha",
    ".xar",
    ".apk",
    ".cbz",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Zip,
    TarGz,
    TarXz,
    TarZst,
    SevenZip,
}

impl Format {
    pub const ALL: [Self; 5] = [Self::Zip, Self::TarGz, Self::TarXz, Self::TarZst, Self::SevenZip];

    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Zip => ".zip",
            Self::TarGz => ".tar.gz",
            Self::TarXz => ".tar.xz",
            Self::TarZst => ".tar.zst",
            Self::SevenZip => ".7z",
        }
    }

    const fn flags(self) -> &'static [&'static str] {
        match self {
            Self::Zip => &["--format", "zip"],
            Self::TarGz => &["--format", "pax", "-z"],
            Self::TarXz => &["--format", "pax", "-J"],
            Self::TarZst => &["--format", "pax", "--zstd"],
            Self::SevenZip => &["--format", "7zip"],
        }
    }
}

#[must_use]
pub fn stem(name: &str) -> Option<&str> {
    let lower = name.to_lowercase();
    SUFFIXES
        .iter()
        .find(|suffix| lower.len() > suffix.len() && lower.ends_with(&suffix.to_lowercase()))
        .and_then(|suffix| name.get(..name.len() - suffix.len()))
}

#[must_use]
pub fn is_archive(path: &Path) -> bool {
    path.file_name().is_some_and(|name| stem(&name.to_string_lossy()).is_some())
}

#[must_use]
pub fn available() -> bool {
    static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| {
        std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(TOOL).is_file()))
    })
}

fn tool() -> Command {
    let mut command = Command::new(TOOL);
    command.env("LC_ALL", "C.UTF-8").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    command
}

fn signal(child: &Child, signal: libc::c_int) {
    if let Ok(pid) = libc::pid_t::try_from(child.id()) {
        // SAFETY: `pid` names a child this process spawned and still owns; no memory is passed.
        unsafe {
            libc::kill(pid, signal);
        }
    }
}

struct Run<'a> {
    sink: &'a dyn Sink,
    cancel: &'a AtomicBool,
    progress: Progress,
}

impl Run<'_> {
    fn drive(&mut self, mut command: Command, mark: char) -> io::Result<()> {
        let mut child = command.spawn()?;
        let (lines, inbox) = mpsc::channel::<String>();
        let reader = child.stderr.take().map(|stderr| {
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).split(b'\n').map_while(Result::ok) {
                    if lines.send(String::from_utf8_lossy(&line).into_owned()).is_err() {
                        break;
                    }
                }
            })
        });
        let mut held = false;
        let mut complaint = None;
        let status = loop {
            if self.cancel.load(Ordering::Relaxed) {
                signal(&child, libc::SIGCONT);
                let _ = child.kill();
                let _ = child.wait();
                break Err(io::Error::new(ErrorKind::Interrupted, "cancelled"));
            }
            let paused = self.sink.paused();
            if paused != held {
                held = paused;
                signal(&child, if paused { libc::SIGSTOP } else { libc::SIGCONT });
            }
            let mut moved = false;
            while let Ok(line) = inbox.try_recv() {
                match line.strip_prefix(mark).and_then(|rest| rest.strip_prefix(' ')) {
                    Some(name) => {
                        self.progress.items += 1;
                        self.progress.current =
                            Path::new(name.trim_end_matches('/')).file_name().map(|n| n.to_string_lossy().into_owned());
                        moved = true;
                    }
                    None if !line.trim().is_empty() => complaint = Some(line),
                    None => {}
                }
            }
            if moved {
                self.sink.progress(self.progress.clone());
            }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => std::thread::sleep(POLL),
                Err(err) => break Err(err),
            }
        };
        if let Some(reader) = reader {
            let _ = reader.join();
        }
        for line in inbox.try_iter() {
            if !line.starts_with(mark) && !line.trim().is_empty() {
                complaint = Some(line);
            }
        }
        match status? {
            status if status.success() => Ok(()),
            _ => {
                let text = complaint.map_or_else(|| String::from("archive tool failed"), |line| clean(&line));
                Err(io::Error::other(text))
            }
        }
    }
}

fn clean(line: &str) -> String {
    line.trim().trim_start_matches("bsdtar: ").to_string()
}

fn count(archive: &Path, cancel: &AtomicBool) -> usize {
    let Ok(mut child) = Command::new(TOOL)
        .arg("-tf")
        .arg(archive)
        .env("LC_ALL", "C.UTF-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return 0;
    };
    let mut found = 0;
    if let Some(stdout) = child.stdout.take() {
        for _ in BufReader::new(stdout).split(b'\n').map_while(Result::ok) {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                break;
            }
            found += 1;
        }
    }
    let _ = child.wait();
    found
}

fn hidden_beside(dir: &Path, name: &OsStr, suffix: &str) -> PathBuf {
    let mut hidden = OsString::from(".");
    hidden.push(name);
    hidden.push(suffix);
    unique_path(dir, &hidden, false, occupied)
}

fn settle(staging: &Path, into: &Path, folder: &OsStr) -> io::Result<PathBuf> {
    let children: Vec<PathBuf> = std::fs::read_dir(staging)?.flatten().map(|child| child.path()).collect();
    if let [only] = children.as_slice()
        && let Some(name) = only.file_name()
    {
        let is_dir = std::fs::symlink_metadata(only).is_ok_and(|meta| meta.is_dir());
        let dest = unique_path(into, name, !is_dir, occupied);
        std::fs::rename(only, &dest)?;
        std::fs::remove_dir(staging)?;
        return Ok(dest);
    }
    let dest = unique_path(into, folder, false, occupied);
    std::fs::rename(staging, &dest)?;
    Ok(dest)
}

fn unpack(archive: &Path, into: &Path, run: &mut Run<'_>) -> io::Result<PathBuf> {
    let name = archive.file_name().ok_or_else(|| io::Error::from(ErrorKind::InvalidFilename))?;
    let shown = name.to_string_lossy();
    let folder = OsString::from(stem(&shown).unwrap_or(&shown));
    let staging = hidden_beside(into, name, ".extracting");
    std::fs::DirBuilder::new().mode(0o700).create(&staging)?;
    let mut command = tool();
    command.arg("-xvf").arg(archive).arg("-C").arg(&staging);
    let unpacked = run.drive(command, 'x').and_then(|()| {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755))?;
        settle(&staging, into, &folder)
    });
    if unpacked.is_err() && occupied(&staging) {
        let _ = remove_recursive(&staging);
    }
    unpacked
}

pub fn extract(archives: &[PathBuf], into: &Path, sink: &dyn Sink, cancel: &AtomicBool) -> Report {
    let total: usize = archives.iter().map(|archive| count(archive, cancel)).sum();
    let mut run = Run { sink, cancel, progress: Progress { total_items: total, ..Progress::default() } };
    let mut report = Report::default();
    sink.progress(run.progress.clone());
    for archive in archives {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match unpack(archive, into, &mut run) {
            Ok(dest) => report.done.push((archive.clone(), dest)),
            Err(err) if err.kind() == ErrorKind::Interrupted => break,
            Err(err) => report.failures.push(Failure::new(archive, &err)),
        }
    }
    report.cancelled = cancel.load(Ordering::Relaxed);
    report
}

fn member(source: &Path) -> Option<(&Path, OsString)> {
    use std::os::unix::ffi::OsStrExt;
    let (dir, name) = (source.parent()?, source.file_name()?);
    let guarded = matches!(name.as_bytes().first(), Some(b'-' | b'@'));
    let mut shown = OsString::from(if guarded { "./" } else { "" });
    shown.push(name);
    Some((dir, shown))
}

pub fn compress(sources: &[PathBuf], dest: &Path, format: Format, sink: &dyn Sink, cancel: &AtomicBool) -> Report {
    let mut report = Report::default();
    let total: usize = sources.iter().map(|source| tree_size(source).1 + 1).sum();
    let mut run = Run { sink, cancel, progress: Progress { total_items: total, ..Progress::default() } };
    sink.progress(run.progress.clone());
    let (Some(dir), Some(name), Some(first)) = (dest.parent(), dest.file_name(), sources.first()) else {
        return report;
    };
    let staging = hidden_beside(dir, name, ".part");
    let mut command = tool();
    command.arg("-cvf").arg(&staging).args(format.flags());
    for (parent, shown) in sources.iter().filter_map(|source| member(source)) {
        command.arg("-C").arg(parent).arg(shown);
    }
    let packed = run.drive(command, 'a').and_then(|()| {
        let placed = unique_path(dir, name, true, occupied);
        std::fs::rename(&staging, &placed).map(|()| placed)
    });
    match packed {
        Ok(placed) => report.done.push((first.clone(), placed)),
        Err(err) => {
            let _ = std::fs::remove_file(&staging);
            if err.kind() != ErrorKind::Interrupted {
                report.failures.push(Failure::new(dest, &err));
            }
        }
    }
    report.cancelled = cancel.load(Ordering::Relaxed);
    report
}

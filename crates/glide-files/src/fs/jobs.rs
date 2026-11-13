use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::operations::{occupied, remove_recursive, unique_path};

const CHUNK: usize = 1 << 20;
const REPORT_EVERY: Duration = Duration::from_millis(80);
pub const PER_DEVICE: usize = 2;

#[must_use]
pub fn admits(running: &[&[u64]], wanted: &[u64], one_at_a_time: bool) -> bool {
    if one_at_a_time {
        return running.is_empty();
    }
    wanted.iter().all(|device| running.iter().filter(|devices| devices.contains(device)).count() < PER_DEVICE)
}

#[must_use]
pub fn devices(paths: &[PathBuf], target: Option<&Path>) -> Vec<u64> {
    let table = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    let points: Vec<PathBuf> =
        table.lines().filter_map(|line| line.split_whitespace().nth(1)).map(mount_point).collect();
    on_disks(&points, paths, target)
}

#[must_use]
pub fn on_disks(points: &[PathBuf], paths: &[PathBuf], target: Option<&Path>) -> Vec<u64> {
    let mut found: Vec<u64> = paths
        .iter()
        .map(|path| path.parent().unwrap_or(path))
        .chain(target)
        .map(|path| {
            use std::hash::{Hash, Hasher};
            let point = points
                .iter()
                .filter(|point| path.starts_with(point))
                .max_by_key(|point| point.as_os_str().len())
                .map_or(Path::new(""), PathBuf::as_path);
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            point.hash(&mut hasher);
            hasher.finish()
        })
        .collect();
    found.sort_unstable();
    found.dedup();
    found
}

pub(crate) fn mount_point(field: &str) -> PathBuf {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let octal = bytes.get(index + 1..index + 4).and_then(|digits| {
            let text = std::str::from_utf8(digits).ok()?;
            u8::from_str_radix(text, 8).ok()
        });
        match (bytes[index], octal) {
            (b'\\', Some(byte)) => {
                out.push(byte);
                index += 4;
            }
            (byte, _) => {
                out.push(byte);
                index += 1;
            }
        }
    }
    PathBuf::from(<std::ffi::OsString as std::os::unix::ffi::OsStringExt>::from_vec(out))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Copy,
    Move,
    Delete,
    Trash,
    Extract,
    Compress(super::archive::Format),
}

impl Op {
    #[must_use]
    pub const fn places(self) -> bool {
        matches!(self, Self::Copy | Self::Move | Self::Extract | Self::Compress(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Replace,
    Merge,
    Skip,
    KeepBoth,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer {
    pub choice: Choice,
    pub for_all: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub folders: bool,
    pub incoming: Option<Stamp>,
    pub existing: Option<Stamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
}

impl Stamp {
    #[must_use]
    pub fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::symlink_metadata(path).ok()?;
        Some(Self { size: meta.len(), modified: meta.modified().ok() })
    }
}

impl Question {
    #[must_use]
    pub fn new(source: &Path, dest: &Path, folders: bool) -> Self {
        Self {
            source: source.to_path_buf(),
            dest: dest.to_path_buf(),
            folders,
            incoming: Stamp::of(source),
            existing: Stamp::of(dest),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Progress {
    pub bytes: u64,
    pub total_bytes: u64,
    pub items: usize,
    pub total_items: usize,
    pub current: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Meter {
    last: Option<(Instant, u64)>,
    rate: f64,
    paused: bool,
}

impl Meter {
    const WINDOW: Duration = Duration::from_millis(400);

    pub fn sample(&mut self, bytes: u64, now: Instant) {
        if self.paused {
            return;
        }
        let Some((at, before)) = self.last else {
            self.last = Some((now, bytes));
            return;
        };
        let elapsed = now.saturating_duration_since(at);
        if elapsed < Self::WINDOW {
            return;
        }
        let current = bytes.saturating_sub(before) as f64 / elapsed.as_secs_f64();
        self.rate = if self.rate > 0.0 { self.rate * 0.6 + current * 0.4 } else { current };
        self.last = Some((now, bytes));
    }

    pub fn pause(&mut self, paused: bool) {
        self.paused = paused;
        self.last = None;
    }

    #[must_use]
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    #[must_use]
    pub fn rate(&self) -> Option<f64> {
        (!self.paused && self.rate >= 1.0).then_some(self.rate)
    }

    #[must_use]
    pub fn eta(&self, remaining: u64) -> Option<Duration> {
        self.rate().map(|rate| Duration::from_secs_f64(remaining as f64 / rate))
    }
}

impl Progress {
    #[must_use]
    pub fn fraction(&self) -> f32 {
        if self.total_bytes > 0 {
            (self.bytes as f64 / self.total_bytes as f64).clamp(0.0, 1.0) as f32
        } else if self.total_items > 0 {
            (self.items as f64 / self.total_items as f64).clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub path: PathBuf,
    pub kind: ErrorKind,
    pub message: String,
}

impl Failure {
    pub(crate) fn new(path: &Path, err: &io::Error) -> Self {
        Self { path: path.to_path_buf(), kind: err.kind(), message: err.to_string() }
    }

    #[must_use]
    pub fn error(&self) -> io::Error {
        io::Error::new(self.kind, self.message.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    pub done: Vec<(PathBuf, PathBuf)>,
    pub failures: Vec<Failure>,
    pub cancelled: bool,
}

pub trait Sink {
    fn progress(&self, progress: Progress);
    fn ask(&self, question: Question) -> Answer;
    fn paused(&self) -> bool {
        false
    }
}

fn hold(sink: &dyn Sink, cancel: &AtomicBool) {
    while sink.paused() && !cancel.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(60));
    }
}

fn name_of(path: &Path) -> Option<String> {
    path.file_name().map(|name| name.to_string_lossy().into_owned())
}

struct Worker<'a> {
    op: Op,
    sink: &'a dyn Sink,
    cancel: &'a AtomicBool,
    progress: Progress,
    reported: Instant,
    remember_files: Option<Choice>,
    remember_folders: Option<Choice>,
    failures: Vec<Failure>,
    known: HashMap<PathBuf, Size>,
    placed: Vec<(PathBuf, PathBuf)>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Size {
    bytes: u64,
    items: usize,
}

fn measure(path: &Path, cancel: &AtomicBool) -> Size {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return Size::default();
    };
    let mut size = Size { bytes: if meta.is_file() { meta.len() } else { 0 }, items: 1 };
    if meta.is_dir()
        && let Some(_inside) = Inside::enter(&meta)
        && let Ok(children) = std::fs::read_dir(path)
    {
        for child in children.flatten() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let inner = measure(&child.path(), cancel);
            size.bytes += inner.bytes;
            size.items += inner.items;
        }
    }
    size
}

thread_local! {
    static ANCESTORS: std::cell::RefCell<Vec<(u64, u64)>> = const { std::cell::RefCell::new(Vec::new()) };
}

struct Inside;

impl Inside {
    fn enter(meta: &std::fs::Metadata) -> Option<Self> {
        use std::os::unix::fs::MetadataExt;
        let key = (meta.dev(), meta.ino());
        ANCESTORS.with_borrow_mut(|ancestors| {
            if ancestors.contains(&key) {
                return None;
            }
            ancestors.push(key);
            Some(Self)
        })
    }
}

impl Drop for Inside {
    fn drop(&mut self) {
        ANCESTORS.with_borrow_mut(|ancestors| {
            ancestors.pop();
        });
    }
}

#[must_use]
pub(crate) fn tree_size(path: &Path) -> (u64, usize) {
    let size = measure(path, &AtomicBool::new(false));
    (size.bytes, size.items.saturating_sub(1))
}

fn is_special(meta: &std::fs::Metadata) -> bool {
    let kind = meta.file_type();
    !(kind.is_file() || kind.is_dir() || kind.is_symlink())
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn inside(target: &Path, source: &Path) -> bool {
    canonical(target).starts_with(canonical(source))
}

fn removing_takes(dest: &Path, source: &Path) -> bool {
    let dest = match (dest.parent(), dest.file_name()) {
        (Some(parent), Some(name)) => canonical(parent).join(name),
        _ => dest.to_path_buf(),
    };
    canonical(source).starts_with(dest)
}

fn staging_for(dest: &Path) -> PathBuf {
    let (Some(dir), Some(name)) = (dest.parent(), dest.file_name()) else {
        return dest.to_path_buf();
    };
    let mut hidden = std::ffi::OsString::from(".");
    hidden.push(name);
    hidden.push(".part");
    unique_path(dir, &hidden, false, occupied)
}

fn unique_beside(dest: &Path, is_dir: bool) -> PathBuf {
    let (Some(dir), Some(name)) = (dest.parent(), dest.file_name()) else {
        return dest.to_path_buf();
    };
    unique_path(dir, name, !is_dir, occupied)
}

impl Worker<'_> {
    fn cancelled(&self) -> bool {
        hold(self.sink, self.cancel);
        self.cancel.load(Ordering::Relaxed)
    }

    fn advance(&mut self, bytes: u64, items: usize) {
        self.progress.bytes += bytes;
        self.progress.items += items;
        if self.reported.elapsed() >= REPORT_EVERY {
            self.reported = Instant::now();
            self.sink.progress(self.progress.clone());
        }
    }

    fn fail(&mut self, path: &Path, err: &io::Error) {
        self.failures.push(Failure::new(path, err));
    }

    fn resolve(&mut self, source: &Path, dest: &Path, folders: bool) -> Choice {
        let remembered = if folders { self.remember_folders } else { self.remember_files };
        if let Some(choice) = remembered {
            return choice;
        }
        let answer = self.sink.ask(Question::new(source, dest, folders));
        if answer.for_all {
            if folders {
                self.remember_folders = Some(answer.choice);
            } else {
                self.remember_files = Some(answer.choice);
            }
        }
        if answer.choice == Choice::Cancel {
            self.cancel.store(true, Ordering::Relaxed);
        }
        answer.choice
    }

    fn transfer(&mut self, source: &Path, dest: PathBuf) -> Option<PathBuf> {
        if self.cancelled() {
            return None;
        }
        let meta = match std::fs::symlink_metadata(source) {
            Ok(meta) => meta,
            Err(err) => {
                self.fail(source, &err);
                return None;
            }
        };
        let mut dest = dest;
        let mut swap = false;
        let mut aside = None;
        if let Ok(existing) = std::fs::symlink_metadata(&dest) {
            let folders = meta.is_dir() && existing.is_dir();
            match self.resolve(source, &dest, folders) {
                Choice::Cancel => return None,
                Choice::Skip => {
                    let size = measure(source, self.cancel);
                    self.advance(size.bytes, size.items);
                    return None;
                }
                Choice::KeepBoth => dest = unique_beside(&dest, meta.is_dir()),
                Choice::Merge if folders => return self.merge(source, &dest),
                Choice::Replace | Choice::Merge if removing_takes(&dest, source) => {
                    self.fail(source, &io::Error::new(ErrorKind::InvalidInput, "item over itself"));
                    return None;
                }
                Choice::Replace | Choice::Merge if !meta.is_dir() && !existing.is_dir() => swap = true,
                Choice::Replace | Choice::Merge => {
                    let backup = staging_for(&dest);
                    if let Err(err) = std::fs::rename(&dest, &backup) {
                        self.fail(&dest, &err);
                        return None;
                    }
                    aside = Some(backup);
                }
            }
        }
        let strict = swap || aside.is_some() || self.op == Op::Move;
        let placed = self.place(source, dest.clone(), &meta, swap, strict);
        if let Some(backup) = aside {
            if placed.is_some() {
                if let Err(err) = remove_recursive(&backup) {
                    self.fail(&backup, &err);
                }
            } else if !occupied(&dest)
                && let Err(err) = std::fs::rename(&backup, &dest)
            {
                self.fail(&dest, &err);
            }
        }
        placed
    }

    fn place(
        &mut self, source: &Path, dest: PathBuf, meta: &std::fs::Metadata, swap: bool, strict: bool,
    ) -> Option<PathBuf> {
        if self.op == Op::Move {
            match std::fs::rename(source, &dest) {
                Ok(()) => {
                    let size = match self.known.remove(source) {
                        Some(size) => size,
                        None if meta.is_dir() => measure(&dest, self.cancel),
                        None => Size { bytes: meta.len(), items: 1 },
                    };
                    self.advance(size.bytes, size.items);
                    return Some(dest);
                }
                Err(err) if err.kind() == ErrorKind::CrossesDevices => {}
                Err(err) => {
                    self.fail(source, &err);
                    return None;
                }
            }
        }
        let staging = if swap { staging_for(&dest) } else { dest.clone() };
        let failures = self.failures.len();
        let mut created = false;
        let copied = self.copy_tree(source, &staging, &mut created);
        let clean = copied && self.failures.len() == failures;
        if !copied || self.cancelled() || (strict && !clean) {
            if created {
                let _ = remove_recursive(&staging);
            }
            return None;
        }
        if !created {
            return None;
        }
        if swap && let Err(err) = std::fs::rename(&staging, &dest) {
            let _ = remove_recursive(&staging);
            self.fail(&dest, &err);
            return None;
        }
        if self.op == Op::Move
            && clean
            && let Err(err) = remove_recursive(source)
        {
            self.fail(source, &err);
        }
        Some(dest)
    }

    fn merge(&mut self, source: &Path, dest: &Path) -> Option<PathBuf> {
        let children = match std::fs::read_dir(source) {
            Ok(children) => children,
            Err(err) => {
                self.fail(source, &err);
                return None;
            }
        };
        self.advance(0, 1);
        for child in children {
            let child = match child {
                Ok(child) => child,
                Err(err) => {
                    self.fail(source, &err);
                    break;
                }
            };
            let path = child.path();
            if let Some(placed) = self.transfer(&path, dest.join(child.file_name())) {
                self.placed.push((path, placed));
            }
            if self.cancelled() {
                break;
            }
        }
        if self.op == Op::Move && !self.cancelled() {
            let _ = std::fs::remove_dir(source);
        }
        None
    }

    fn copy_tree(&mut self, source: &Path, dest: &Path, created: &mut bool) -> bool {
        *created = false;
        if self.cancelled() {
            return false;
        }
        let meta = match std::fs::symlink_metadata(source) {
            Ok(meta) => meta,
            Err(err) => {
                self.fail(source, &err);
                return true;
            }
        };
        if meta.file_type().is_symlink() {
            let linked = std::fs::read_link(source).and_then(|target| std::os::unix::fs::symlink(target, dest));
            *created = linked.is_ok();
            if let Err(err) = linked {
                self.fail(source, &err);
            }
            self.advance(0, 1);
            return true;
        }
        if is_special(&meta) {
            let err = io::Error::new(ErrorKind::Unsupported, "special file");
            self.fail(source, &err);
            self.advance(0, 1);
            return true;
        }
        if meta.is_dir() {
            return self.copy_dir(source, dest, &meta, created);
        }
        match self.copy_file(source, dest, &meta, created) {
            Ok(()) => true,
            Err(err) => {
                if *created {
                    let _ = std::fs::remove_file(dest);
                    *created = false;
                }
                if self.cancelled() {
                    return false;
                }
                self.fail(source, &err);
                true
            }
        }
    }

    fn copy_dir(&mut self, source: &Path, dest: &Path, meta: &std::fs::Metadata, created: &mut bool) -> bool {
        let Some(_inside) = Inside::enter(meta) else {
            self.fail(source, &io::Error::new(ErrorKind::InvalidInput, "the folder contains itself"));
            return true;
        };
        if let Err(err) = std::fs::DirBuilder::new().mode(0o700).create(dest) {
            self.fail(dest, &err);
            return true;
        }
        *created = true;
        self.advance(0, 1);
        match std::fs::read_dir(source) {
            Ok(children) => {
                for child in children {
                    let child = match child {
                        Ok(child) => child,
                        Err(err) => {
                            self.fail(source, &err);
                            break;
                        }
                    };
                    let mut child_created = false;
                    if !self.copy_tree(&child.path(), &dest.join(child.file_name()), &mut child_created) {
                        return false;
                    }
                }
            }
            Err(err) => self.fail(source, &err),
        }
        let _ = std::fs::set_permissions(dest, meta.permissions());
        if let (Ok(modified), Ok(file)) = (meta.modified(), File::open(dest)) {
            let _ = file.set_modified(modified);
        }
        true
    }

    fn copy_file(
        &mut self, source: &Path, dest: &Path, meta: &std::fs::Metadata, created: &mut bool,
    ) -> io::Result<()> {
        self.progress.current = name_of(source);
        let mut input = File::open(source)?;
        let mut output = OpenOptions::new().write(true).create_new(true).mode(0o600).open(dest)?;
        *created = true;
        let mut buffer = vec![0_u8; CHUNK];
        loop {
            if self.cancelled() {
                return Err(io::Error::new(ErrorKind::Interrupted, "cancelled"));
            }
            let read = input.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read])?;
            self.advance(read as u64, 0);
        }
        if self.op == Op::Move {
            output.sync_all()?;
        }
        output.set_permissions(meta.permissions())?;
        if let Ok(modified) = meta.modified() {
            let _ = output.set_modified(modified);
        }
        self.advance(0, 1);
        Ok(())
    }
}

pub fn transfer(op: Op, sources: &[PathBuf], target: &Path, sink: &dyn Sink, cancel: &AtomicBool) -> Report {
    let known: HashMap<PathBuf, Size> =
        sources.iter().map(|source| (source.clone(), measure(source, cancel))).collect();
    let mut worker = Worker {
        op,
        sink,
        cancel,
        progress: Progress {
            total_bytes: known.values().map(|size| size.bytes).sum(),
            total_items: known.values().map(|size| size.items).sum(),
            ..Progress::default()
        },
        reported: Instant::now().checked_sub(REPORT_EVERY).unwrap_or_else(Instant::now),
        remember_files: None,
        remember_folders: None,
        failures: Vec::new(),
        known,
        placed: Vec::new(),
    };
    sink.progress(worker.progress.clone());
    for source in sources {
        if worker.cancelled() {
            break;
        }
        let Some(name) = source.file_name() else {
            continue;
        };
        let is_dir = std::fs::symlink_metadata(source).is_ok_and(|meta| meta.is_dir());
        if is_dir && inside(target, source) {
            let err = io::Error::new(ErrorKind::InvalidInput, "folder into itself");
            worker.fail(source, &err);
            continue;
        }
        let dest = target.join(name);
        if op == Op::Move && source.parent() == Some(target) {
            continue;
        }
        let dest = if dest == *source { unique_beside(&dest, is_dir) } else { dest };
        if let Some(final_dest) = worker.transfer(source, dest) {
            worker.placed.push((source.clone(), final_dest));
        }
    }
    sink.progress(worker.progress.clone());
    let cancelled = worker.cancelled();
    Report { done: worker.placed, failures: worker.failures, cancelled }
}

struct Silent;

impl Sink for Silent {
    fn progress(&self, _: Progress) {}

    fn ask(&self, _: Question) -> Answer {
        Answer { choice: Choice::Cancel, for_all: false }
    }
}

pub fn copy_exact(source: &Path, dest: &Path) -> io::Result<()> {
    let cancel = AtomicBool::new(false);
    let mut worker = Worker {
        op: Op::Copy,
        sink: &Silent,
        cancel: &cancel,
        progress: Progress::default(),
        reported: Instant::now(),
        remember_files: None,
        remember_folders: None,
        failures: Vec::new(),
        known: HashMap::new(),
        placed: Vec::new(),
    };
    let mut created = false;
    worker.copy_tree(source, dest, &mut created);
    match worker.failures.first() {
        None => Ok(()),
        Some(failure) => {
            if created {
                let _ = remove_recursive(dest);
            }
            Err(failure.error())
        }
    }
}

pub fn delete(paths: &[PathBuf], sink: &dyn Sink, cancel: &AtomicBool) -> Report {
    each(paths, sink, cancel, |path| remove_recursive(path).map(|()| path.to_path_buf()))
}

pub fn each(
    paths: &[PathBuf], sink: &dyn Sink, cancel: &AtomicBool, mut apply: impl FnMut(&Path) -> io::Result<PathBuf>,
) -> Report {
    let mut progress = Progress { total_items: paths.len(), ..Progress::default() };
    let mut report = Report::default();
    sink.progress(progress.clone());
    for path in paths {
        hold(sink, cancel);
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        progress.current = name_of(path);
        sink.progress(progress.clone());
        match apply(path) {
            Ok(dest) => report.done.push((path.clone(), dest)),
            Err(err) => report.failures.push(Failure::new(path, &err)),
        }
        progress.items += 1;
        sink.progress(progress.clone());
    }
    report
}

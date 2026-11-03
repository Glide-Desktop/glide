use std::{
    fs::File,
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use tracing_subscriber::{EnvFilter, Layer, filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt};

const SMITHAY_FILTER: &str = "smithay=info,\
     smithay::wayland::selection=debug,\
     smithay::backend::egl=warn,\
     smithay::backend::renderer::gles=warn,\
     smithay::backend::drm=info,\
     smithay::backend::session::libseat=info";

const FILTER_VARIABLE: &str = "GLIDE_LOG";
pub const MAX_LOG_BYTES: u64 = 64 * 1024 * 1024;
const KEPT_LOGS: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Session,
    Greeter,
}

impl Mode {
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Session => "glide.log",
            Self::Greeter => "glide-greeter.log",
        }
    }

    #[must_use]
    pub fn from_args(mut args: impl Iterator<Item = String>) -> Self {
        if args.any(|arg| arg == "--greeter") { Self::Greeter } else { Self::Session }
    }
}

fn default_filter() -> String {
    let glide = if cfg!(debug_assertions) { "debug" } else { "info" };
    format!("glide={glide},{SMITHAY_FILTER}")
}

fn filter_spec() -> (String, &'static str) {
    for variable in [FILTER_VARIABLE, "RUST_LOG"] {
        if let Ok(spec) = std::env::var(variable)
            && !spec.trim().is_empty()
        {
            return (spec, variable);
        }
    }
    (default_filter(), "default")
}

pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_owned());
        let location = info
            .location()
            .map_or_else(|| "<unknown location>".to_owned(), |l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let thread = std::thread::current();
        let thread = thread.name().unwrap_or("<unnamed>");
        let backtrace = std::backtrace::Backtrace::force_capture();
        tracing::error!("panic in thread '{thread}' at {location}: {message}\n{backtrace}");
        previous(info);
    }));
}

#[derive(Debug)]
struct Capped {
    file: File,
    written: u64,
    limit: u64,
    stopped: bool,
}

impl Capped {
    const fn new(file: File, limit: u64) -> Self {
        Self { file, written: 0, limit, stopped: false }
    }

    fn write_line(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.stopped {
            return Ok(buf.len());
        }
        let len = u64::try_from(buf.len()).unwrap_or(u64::MAX);
        if self.written.saturating_add(len) > self.limit {
            self.stopped = true;
            let notice =
                format!("log reached {} MiB; further lines are dropped until the next start\n", self.limit >> 20);
            self.file.write_all(notice.as_bytes())?;
            return Ok(buf.len());
        }
        self.written += len;
        self.file.write_all(buf)?;
        Ok(buf.len())
    }
}

#[derive(Debug, Clone)]
struct CappedWriter(Arc<Mutex<Capped>>);

impl Write for CappedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).write_line(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).file.flush()
    }
}

#[must_use]
pub fn init(mode: Mode) -> Option<PathBuf> {
    let (spec, source) = filter_spec();
    let file_filter = EnvFilter::try_new(&spec).unwrap_or_else(|_| EnvFilter::new(default_filter()));
    let stderr =
        tracing_subscriber::fmt::layer().with_ansi(false).with_writer(std::io::stderr).with_filter(LevelFilter::WARN);
    let Some((path, file)) = open_log_file(mode.file_name()) else {
        let everything = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_thread_names(true)
            .with_writer(std::io::stderr)
            .with_filter(file_filter);
        tracing_subscriber::registry().with(everything).init();
        log_header(mode, None, &spec, source);
        return None;
    };
    let writer = CappedWriter(Arc::new(Mutex::new(Capped::new(file, MAX_LOG_BYTES))));
    let to_file = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_thread_names(true)
        .with_writer(move || writer.clone())
        .with_filter(file_filter);
    tracing_subscriber::registry().with(to_file).with(stderr).init();
    log_header(mode, Some(&path), &spec, source);
    Some(path)
}

fn kernel() -> String {
    // SAFETY: `uname` only writes into the zeroed struct it is given.
    let mut name: libc::utsname = unsafe { std::mem::zeroed() };
    // SAFETY: `name` is a valid, writable `utsname`.
    if unsafe { libc::uname(&raw mut name) } != 0 {
        return "unknown".into();
    }
    let field = |chars: &[libc::c_char]| {
        let bytes: Vec<u8> = chars.iter().take_while(|c| **c != 0).map(|c| c.to_ne_bytes()[0]).collect();
        String::from_utf8_lossy(&bytes).into_owned()
    };
    format!("{} {} {}", field(&name.sysname), field(&name.release), field(&name.machine))
}

fn log_header(mode: Mode, path: Option<&Path>, filter: &str, source: &str) {
    let env = |name: &str| std::env::var(name).unwrap_or_default();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        ?mode,
        pid = std::process::id(),
        kernel = kernel(),
        log = ?path,
        filter,
        filter_from = source,
        session_type = env("XDG_SESSION_TYPE"),
        desktop = env("XDG_CURRENT_DESKTOP"),
        vt = env("XDG_VTNR"),
        seat = env("XDG_SEAT"),
        "starting glide"
    );
}

fn rotate(dir: &Path, name: &str, kept: u32) {
    for index in (1..kept).rev() {
        let _ = std::fs::rename(dir.join(format!("{name}.{index}")), dir.join(format!("{name}.{}", index + 1)));
    }
    let _ = std::fs::rename(dir.join(name), dir.join(format!("{name}.1")));
}

fn open_log_file(name: &str) -> Option<(PathBuf, File)> {
    let dir = log_dirs().into_iter().find(|dir| usable_dir(dir))?;
    rotate(&dir, name, KEPT_LOGS);
    let path = dir.join(name);
    let file = File::create(&path).ok()?;
    Some((path, file))
}

fn usable_dir(dir: &Path) -> bool {
    let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir);
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.is_dir() && meta.uid() == uid) && tempfile_probe(dir)
}

fn tempfile_probe(dir: &Path) -> bool {
    let probe = dir.join(format!(".write-probe-{}", std::process::id()));
    let writable = File::create(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    writable
}

fn log_dirs() -> Vec<PathBuf> {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    glide_config::state_home()
        .into_iter()
        .chain(glide_config::runtime_dir())
        .map(|base| base.join("glide"))
        .chain(std::iter::once(std::env::temp_dir().join(format!("glide-{uid}"))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_five_logs_survive_repeated_restarts() {
        let dir = std::env::temp_dir().join(format!("glide-log-rotate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let name = Mode::Session.file_name();
        for run in 1..=8 {
            rotate(&dir, name, KEPT_LOGS);
            std::fs::write(dir.join(name), format!("run {run}")).expect("log is writable");
        }
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
        assert_eq!(read(name).as_deref(), Some("run 8"));
        assert_eq!(read("glide.log.1").as_deref(), Some("run 7"), "the crashed run is one restart back");
        assert_eq!(read("glide.log.5").as_deref(), Some("run 3"));
        assert_eq!(read("glide.log.6"), None, "older runs are dropped");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_greeter_keeps_a_log_of_its_own() {
        assert_eq!(Mode::from_args(["--greeter".to_string()].into_iter()), Mode::Greeter);
        assert_eq!(Mode::from_args(std::iter::empty()), Mode::Session);
        assert_ne!(Mode::Greeter.file_name(), Mode::Session.file_name());
    }

    #[test]
    fn a_runaway_log_stops_at_its_cap_with_one_notice() {
        let path = std::env::temp_dir().join(format!("glide-log-cap-{}", std::process::id()));
        let file = File::create(&path).expect("scratch file");
        let mut capped = Capped::new(file, 10);
        assert_eq!(capped.write_line(b"12345\n").expect("write"), 6);
        assert_eq!(capped.write_line(b"67890\n").expect("write"), 6, "reported as written so tracing carries on");
        capped.write_line(b"more\n").expect("write");
        let text = std::fs::read_to_string(&path).expect("read back");
        assert!(text.starts_with("12345\n"));
        assert_eq!(text.matches("further lines are dropped").count(), 1);
        assert!(!text.contains("67890"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_fallback_directory_is_per_user_not_per_process() {
        let dirs = log_dirs();
        let last = dirs.last().expect("there is always a fallback");
        assert!(last.starts_with(std::env::temp_dir()));
        assert!(!last.to_string_lossy().contains(&std::process::id().to_string()));
    }
}

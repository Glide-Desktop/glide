//! Logging setup.
//!
//! Glide runs on a bare tty with the console in graphics mode, so stderr is
//! not readable while it runs — a log *file* is the only way to find out
//! what happened. Everything here exists to make that file useful:
//!
//! - written to a predictable path, rotated so the previous run survives
//! - no ANSI escapes (they only corrupt a file, and split field names in a
//!   way that breaks grepping)
//! - a default filter that keeps our own events verbose while muting the
//!   handful of upstream targets that would otherwise drown them out

use std::{fs::File, path::PathBuf};

use tracing_subscriber::EnvFilter;

/// Default filter, used when `RUST_LOG` is unset.
///
/// The muted targets are not arbitrary: `egl::display` and `renderer::gles`
/// each dump their full extension list as a *single* multi-kilobyte INFO
/// line, and libseat logs every device open/close during startup and around
/// every VT switch. Together they used to account for the bulk of a log by
/// volume while saying nothing about what Glide itself was doing.
const DEFAULT_FILTER: &str = "glide=debug,\
     smithay=info,\
     smithay::backend::egl=warn,\
     smithay::backend::renderer::gles=warn,\
     smithay::backend::session::libseat=info";

/// Initializes tracing. Returns the path being logged to, if a file could be
/// opened; otherwise logging falls back to stderr and `None` is returned.
pub fn init() -> Option<PathBuf> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    if let Some((path, file)) = open_log_file() {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(file)
            .with_ansi(false)
            .init();
        Some(path)
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_ansi(false)
            .init();
        None
    }
}

/// Opens `$XDG_STATE_HOME/glide/glide.log` (falling back to
/// `$HOME/.local/state`, then `/tmp`), first rotating any existing log to
/// `glide.log.1` so the previous run is still available for comparison —
/// which matters when a fix has to be judged against how the last run
/// behaved.
fn open_log_file() -> Option<(PathBuf, File)> {
    let dir = log_dir();
    std::fs::create_dir_all(&dir).ok()?;

    let path = dir.join("glide.log");
    if path.exists() {
        let _ = std::fs::rename(&path, dir.join("glide.log.1"));
    }

    let file = File::create(&path).ok()?;
    Some((path, file))
}

/// Directory to log into: `$XDG_STATE_HOME/glide`, falling back to
/// `$HOME/.local/state/glide`, then `/tmp/glide`. Always returns a path;
/// `open_log_file` is what may fail (e.g. the directory can't be created).
fn log_dir() -> PathBuf {
    if let Some(state) = std::env::var_os("XDG_STATE_HOME").filter(|s| !s.is_empty()) {
        return PathBuf::from(state).join("glide");
    }
    if let Some(home) = std::env::var_os("HOME").filter(|s| !s.is_empty()) {
        return PathBuf::from(home).join(".local/state/glide");
    }
    PathBuf::from("/tmp/glide")
}

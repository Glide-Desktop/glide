use std::{fs::File, path::PathBuf};

use tracing_subscriber::EnvFilter;

const DEFAULT_FILTER: &str = "glide=debug,\
     smithay=info,\
     smithay::backend::egl=warn,\
     smithay::backend::renderer::gles=warn,\
     smithay::backend::session::libseat=info";

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

fn log_dir() -> PathBuf {
    if let Some(state) = std::env::var_os("XDG_STATE_HOME").filter(|s| !s.is_empty()) {
        return PathBuf::from(state).join("glide");
    }
    if let Some(home) = std::env::var_os("HOME").filter(|s| !s.is_empty()) {
        return PathBuf::from(home).join(".local/state/glide");
    }
    PathBuf::from("/tmp/glide")
}

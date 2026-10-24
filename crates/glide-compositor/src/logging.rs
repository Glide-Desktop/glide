use std::{
    fs::File,
    path::{Path, PathBuf},
};

use tracing_subscriber::EnvFilter;

const SMITHAY_FILTER: &str = "smithay=info,\
     smithay::backend::egl=warn,\
     smithay::backend::renderer::gles=warn,\
     smithay::backend::session::libseat=info";

fn default_filter() -> String {
    let glide = if cfg!(debug_assertions) { "debug" } else { "info" };
    format!("glide={glide},{SMITHAY_FILTER}")
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

pub fn init() -> Option<PathBuf> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter()));

    let builder = tracing_subscriber::fmt().with_env_filter(filter).with_ansi(false);
    let Some((path, file)) = open_log_file() else {
        builder.init();
        return None;
    };
    builder.with_writer(file).init();
    Some(path)
}

const LOG: &str = "glide.log";
const KEPT_LOGS: u32 = 5;

fn rotate(dir: &Path, kept: u32) {
    for index in (1..kept).rev() {
        let _ = std::fs::rename(dir.join(format!("{LOG}.{index}")), dir.join(format!("{LOG}.{}", index + 1)));
    }
    let _ = std::fs::rename(dir.join(LOG), dir.join(format!("{LOG}.1")));
}

fn open_log_file() -> Option<(PathBuf, File)> {
    let dir = log_dir();
    std::fs::create_dir_all(&dir).ok()?;
    rotate(&dir, KEPT_LOGS);

    let path = dir.join(LOG);
    let file = File::create(&path).ok()?;
    Some((path, file))
}

fn log_dir() -> PathBuf {
    glide_config::state_home()
        .or_else(glide_config::runtime_dir)
        .map_or_else(|| std::env::temp_dir().join(format!("glide-{}", std::process::id())), |base| base.join("glide"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_five_logs_survive_repeated_restarts() {
        let dir = std::env::temp_dir().join(format!("glide-log-rotate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        for run in 1..=8 {
            rotate(&dir, KEPT_LOGS);
            std::fs::write(dir.join(LOG), format!("run {run}")).expect("log is writable");
        }
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
        assert_eq!(read(LOG).as_deref(), Some("run 8"));
        assert_eq!(read("glide.log.1").as_deref(), Some("run 7"), "the crashed run is one restart back");
        assert_eq!(read("glide.log.5").as_deref(), Some("run 3"));
        assert_eq!(read("glide.log.6"), None, "older runs are dropped");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

use std::collections::HashMap;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use zbus::zvariant::Value;

const APP_URI: &str = "application://glide-files.desktop";
const OBJECT: &str = "/org/glide/Files/LauncherEntry";
const INTERFACE: &str = "com.canonical.Unity.LauncherEntry";
const FAREWELL: Duration = Duration::from_millis(300);

enum Update {
    Progress(Option<u16>),
    Flush(Sender<()>),
}

struct Reporter {
    updates: Sender<Update>,
    last: Mutex<u32>,
}

static REPORTER: OnceLock<Reporter> = OnceLock::new();

fn reporter() -> &'static Reporter {
    REPORTER.get_or_init(|| {
        let (updates, inbox) = channel::<Update>();
        let _ = std::thread::Builder::new().name(String::from("glide-files-launcher")).spawn(move || {
            let Ok(connection) = zbus::blocking::Connection::session() else {
                return;
            };
            while let Ok(first) = inbox.recv() {
                let mut pending = vec![first];
                pending.extend(inbox.try_iter());
                let latest = pending.iter().rev().find_map(|update| match update {
                    Update::Progress(progress) => Some(*progress),
                    Update::Flush(_) => None,
                });
                if let Some(progress) = latest {
                    let mut props: HashMap<&str, Value<'_>> = HashMap::new();
                    props.insert("progress-visible", Value::from(progress.is_some()));
                    props.insert("progress", Value::from(f64::from(progress.unwrap_or(0)) / 1000.0));
                    let _ = connection.emit_signal(None::<&str>, OBJECT, INTERFACE, "Update", &(APP_URI, props));
                }
                for update in pending {
                    if let Update::Flush(done) = update {
                        let _ = done.send(());
                    }
                }
            }
        });
        Reporter { updates, last: Mutex::new(u32::MAX) }
    })
}

#[must_use]
pub fn permille(progress: Option<f64>) -> Option<u16> {
    progress.map(|fraction| (fraction.clamp(0.0, 1.0) * 1000.0).round() as u16)
}

pub fn report(progress: Option<f64>) {
    let value = permille(progress);
    let reporter = reporter();
    let mut last = reporter.last.lock().unwrap_or_else(PoisonError::into_inner);
    let code = value.map_or(u32::from(u16::MAX), u32::from);
    if *last != code {
        *last = code;
        let _ = reporter.updates.send(Update::Progress(value));
    }
}

pub fn finish() {
    let Some(reporter) = REPORTER.get() else {
        return;
    };
    let shown = *reporter.last.lock().unwrap_or_else(PoisonError::into_inner) < u32::from(u16::MAX);
    if !shown {
        return;
    }
    let (done, flushed) = channel();
    if reporter.updates.send(Update::Progress(None)).is_ok() && reporter.updates.send(Update::Flush(done)).is_ok() {
        let _ = flushed.recv_timeout(FAREWELL);
    }
}

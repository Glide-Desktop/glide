use std::collections::HashMap;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock, PoisonError};

use zbus::zvariant::Value;

const APP_URI: &str = "application://glide-files.desktop";
const OBJECT: &str = "/org/glide/Files/LauncherEntry";
const INTERFACE: &str = "com.canonical.Unity.LauncherEntry";

struct Reporter {
    updates: Sender<Option<u16>>,
    last: Mutex<u32>,
}

fn reporter() -> &'static Reporter {
    static REPORTER: OnceLock<Reporter> = OnceLock::new();
    REPORTER.get_or_init(|| {
        let (updates, inbox) = channel::<Option<u16>>();
        let _ = std::thread::Builder::new().name(String::from("glide-files-launcher")).spawn(move || {
            let Ok(connection) = zbus::blocking::Connection::session() else {
                return;
            };
            while let Ok(mut update) = inbox.recv() {
                while let Ok(newer) = inbox.try_recv() {
                    update = newer;
                }
                let mut props: HashMap<&str, Value<'_>> = HashMap::new();
                props.insert("progress-visible", Value::from(update.is_some()));
                props.insert("progress", Value::from(f64::from(update.unwrap_or(0)) / 1000.0));
                let _ = connection.emit_signal(None::<&str>, OBJECT, INTERFACE, "Update", &(APP_URI, props));
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
        let _ = reporter.updates.send(value);
    }
}

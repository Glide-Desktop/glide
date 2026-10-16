use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Point};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use iced::futures::channel::oneshot;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saved {
    Written(PathBuf),
    Cancelled,
    Failed(String),
}

#[must_use]
pub fn text<T>(term: &Term<T>) -> String {
    let start = Point::new(term.topmost_line(), Column(0));
    let end = Point::new(term.bottommost_line(), term.last_column());
    let mut text = term.bounds_to_string(start, end).trim_end().to_string();
    text.push('\n');
    text
}

#[must_use]
pub fn unused(folder: &Path, name: &str) -> PathBuf {
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, ""));
    let dot = if extension.is_empty() { "" } else { "." };
    std::iter::once(folder.join(name))
        .chain((2..10_000).map(|number| folder.join(format!("{stem} {number}{dot}{extension}"))))
        .find(|path| !path.exists())
        .unwrap_or_else(|| folder.join(name))
}

fn choose(title: &str, name: &str, folder: Option<&Path>) -> zbus::Result<Option<PathBuf>> {
    let connection = zbus::blocking::Connection::session()?;
    let sender =
        connection.unique_name().map(|name| name.trim_start_matches(':').replace('.', "_")).unwrap_or_default();
    let nanos = std::time::SystemTime::UNIX_EPOCH.elapsed().map_or(0, |elapsed| elapsed.subsec_nanos());
    let token = format!("glide_terminal_{}_{nanos}", std::process::id());
    let request = format!("{PORTAL_PATH}/request/{sender}/{token}");
    let response = zbus::blocking::Proxy::new(&connection, PORTAL, request.as_str(), "org.freedesktop.portal.Request")?;
    let mut signals = response.receive_signal("Response")?;
    let chooser = zbus::blocking::Proxy::new(&connection, PORTAL, PORTAL_PATH, "org.freedesktop.portal.FileChooser")?;
    let folder = folder.map(|folder| {
        let mut bytes = folder.as_os_str().as_encoded_bytes().to_vec();
        bytes.push(0);
        bytes
    });
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("current_name", Value::from(name));
    options.insert("modal", Value::from(true));
    if let Some(folder) = folder.as_deref() {
        options.insert("current_folder", Value::from(folder));
    }
    let _: OwnedObjectPath = chooser.call("SaveFile", &("", title, options))?;
    let Some(message) = signals.next() else { return Ok(None) };
    let (code, results): (u32, HashMap<String, OwnedValue>) = message.body().deserialize()?;
    if code != 0 {
        return Ok(None);
    }
    let uris = results.get("uris").and_then(|uris| <Vec<String>>::try_from(uris.try_clone().ok()?).ok());
    Ok(uris.unwrap_or_default().first().map(|uri| crate::app::uri_path(uri)))
}

pub struct Request<T> {
    pub term: Arc<FairMutex<Term<T>>>,
    pub title: String,
    pub name: String,
    pub folder: Option<PathBuf>,
}

pub fn spawn<T: Send + 'static>(request: Request<T>) -> oneshot::Receiver<Saved> {
    let (sender, receiver) = oneshot::channel();
    let spawned = std::thread::Builder::new().name("glide-terminal-export".into()).spawn(move || {
        let content = text(&request.term.lock());
        let destination = match choose(&request.title, &request.name, request.folder.as_deref()) {
            Ok(Some(path)) => Some(path),
            Ok(None) => None,
            Err(_) => glide_config::home().map(|home| unused(&home, &request.name)),
        };
        let saved = destination.map_or(Saved::Cancelled, |path| match std::fs::write(&path, content) {
            Ok(()) => Saved::Written(path),
            Err(error) => Saved::Failed(error.to_string()),
        });
        let _ = sender.send(saved);
    });
    drop(spawned);
    receiver
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    use crate::session::Cells;

    #[test]
    fn the_export_holds_the_scrollback_and_the_screen_without_trailing_blank_lines() {
        let mut term = Term::new(Config::default(), &Cells { columns: 20, lines: 3 }, VoidListener);
        let mut processor: Processor = Processor::new();
        processor.advance(&mut term, b"one\r\ntwo\r\nthree\r\nfour\r\n");
        assert_eq!(text(&term), "one\ntwo\nthree\nfour\n");
    }

    #[test]
    fn a_taken_name_gets_a_number_before_its_extension() {
        let folder = std::env::temp_dir().join(format!("glide-terminal-export-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let name = "Terminal Saved Output.txt";
        assert_eq!(unused(&folder, name), folder.join(name));
        std::fs::write(folder.join(name), "").unwrap();
        assert_eq!(unused(&folder, name), folder.join("Terminal Saved Output 2.txt"));
        assert_eq!(unused(&folder, "notes"), folder.join("notes"));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}

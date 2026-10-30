use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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

fn write_private(path: &Path, content: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    file.write_all(content).and_then(|()| file.sync_all())
}

fn write_unused(folder: &Path, name: &str, content: &[u8]) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::OpenOptionsExt;
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, ""));
    let dot = if extension.is_empty() { "" } else { "." };
    for number in 1..10_000 {
        let path = folder.join(if number == 1 { name.to_string() } else { format!("{stem} {number}{dot}{extension}") });
        let mut file = match std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        if let Err(error) = file.write_all(content).and_then(|()| file.sync_all()) {
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        return Ok(path);
    }
    Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "no unused export name"))
}

fn snapshot<T>(term: &Arc<FairMutex<Term<T>>>, revision: &AtomicU64) -> std::io::Result<String> {
    for _ in 0..3 {
        let expected = revision.load(Ordering::Acquire);
        let mut line = term.lock().topmost_line();
        let mut output = String::new();
        let mut complete = true;
        loop {
            let term = term.lock();
            if revision.load(Ordering::Acquire) != expected {
                complete = false;
                break;
            }
            if line > term.bottommost_line() {
                break;
            }
            let end = crate::find::chunk_end(&term, line, 256);
            output.push_str(&term.bounds_to_string(Point::new(line, Column(0)), Point::new(end, term.last_column())));
            output.push('\n');
            line = end + 1;
        }
        if complete && revision.load(Ordering::Acquire) == expected {
            output.truncate(output.trim_end().len());
            output.push('\n');
            return Ok(output);
        }
    }
    Err(std::io::Error::other("terminal output changed during export; pause output and retry"))
}

const CHOOSER_WAIT: std::time::Duration = std::time::Duration::from_mins(30);

fn choose(title: &str, name: &str, folder: Option<&Path>) -> zbus::Result<Option<PathBuf>> {
    let connection = zbus::blocking::Connection::session()?;
    let sender =
        connection.unique_name().map(|name| name.trim_start_matches(':').replace('.', "_")).unwrap_or_default();
    let nanos = std::time::SystemTime::UNIX_EPOCH.elapsed().map_or(0, |elapsed| elapsed.subsec_nanos());
    let token = format!("glide_terminal_{}_{nanos}", std::process::id());
    let request = format!("{PORTAL_PATH}/request/{sender}/{token}");
    let response = zbus::blocking::Proxy::new(
        &connection,
        PORTAL,
        zbus::zvariant::OwnedObjectPath::try_from(request)?,
        "org.freedesktop.portal.Request",
    )?;
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
    let (sender, receiver) = std::sync::mpsc::channel();
    let waiting = std::thread::Builder::new().name("glide-terminal-portal".into()).spawn(move || {
        let _ = sender.send(signals.next());
    });
    drop(waiting);
    let Ok(Some(message)) = receiver.recv_timeout(CHOOSER_WAIT) else { return Ok(None) };
    let (code, results): (u32, HashMap<String, OwnedValue>) = message.body().deserialize()?;
    if code != 0 {
        return Ok(None);
    }
    let uris = results.get("uris").and_then(|uris| <Vec<String>>::try_from(uris.try_clone().ok()?).ok());
    let uri = uris.unwrap_or_default().into_iter().next();
    match uri {
        Some(uri) => crate::app::local_uri_path(&uri)
            .map(Some)
            .ok_or_else(|| zbus::Error::Failure("non-local export destination".into())),
        None => Ok(None),
    }
}

pub struct Request<T> {
    pub term: Arc<FairMutex<Term<T>>>,
    pub title: String,
    pub name: String,
    pub folder: Option<PathBuf>,
    pub revision: Arc<AtomicU64>,
}

pub fn spawn<T: Send + 'static>(request: Request<T>) -> oneshot::Receiver<Saved> {
    let (sender, receiver) = oneshot::channel();
    let spawned = std::thread::Builder::new().name("glide-terminal-export".into()).spawn(move || {
        let saved = match snapshot(&request.term, &request.revision) {
            Ok(content) => match choose(&request.title, &request.name, request.folder.as_deref()) {
                Ok(Some(path)) => match write_private(&path, content.as_bytes()) {
                    Ok(()) => Saved::Written(path),
                    Err(error) => Saved::Failed(error.to_string()),
                },
                Ok(None) => Saved::Cancelled,
                Err(_) => match glide_config::home()
                    .ok_or_else(|| std::io::Error::other("no home directory"))
                    .and_then(|home| write_unused(&home, &request.name, content.as_bytes()))
                {
                    Ok(path) => Saved::Written(path),
                    Err(error) => Saved::Failed(error.to_string()),
                },
            },
            Err(error) => Saved::Failed(error.to_string()),
        };
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

    #[test]
    fn simultaneous_fallback_exports_never_overwrite_each_other() {
        let folder = std::env::temp_dir().join(format!("glide-terminal-export-race-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let threads: Vec<_> = (0..8)
            .map(|index| {
                let folder = folder.clone();
                std::thread::spawn(move || write_unused(&folder, "output.txt", index.to_string().as_bytes()).unwrap())
            })
            .collect();
        let mut paths: Vec<_> = threads.into_iter().map(|thread| thread.join().unwrap()).collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), 8);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn chunked_exports_match_a_single_pass_including_wrapped_lines() {
        let mut term = Term::new(Config::default(), &Cells { columns: 20, lines: 3 }, VoidListener);
        let mut processor: Processor = Processor::new();
        for _ in 0..300 {
            processor.advance(&mut term, b"a wrapped line that crosses the cell boundary\r\n");
        }
        let expected = text(&term);
        let term = Arc::new(FairMutex::new(term));
        assert_eq!(snapshot(&term, &AtomicU64::new(0)).unwrap(), expected);
    }
}

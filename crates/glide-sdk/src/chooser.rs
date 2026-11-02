use std::collections::HashMap;
use std::path::{Path, PathBuf};

use glide_app::chooser::{Mode, Request, Response};
use iced::futures::channel::oneshot;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Open,
    Folder,
    Save,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub purpose: Purpose,
    pub title: String,
    pub accept: String,
    pub name: Option<String>,
    pub folder: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chosen {
    Paths(Vec<PathBuf>),
    Cancelled,
    Failed(String),
}

pub use glide_config::uri::local_path as uri_path;

fn nul_terminated(path: &Path) -> Vec<u8> {
    let mut bytes = path.as_os_str().as_encoded_bytes().to_vec();
    bytes.push(0);
    bytes
}

fn portal(ask: &Ask) -> zbus::Result<Chosen> {
    let connection = zbus::blocking::Connection::session()?;
    let sender =
        connection.unique_name().map(|name| name.trim_start_matches(':').replace('.', "_")).unwrap_or_default();
    let nanos = std::time::SystemTime::UNIX_EPOCH.elapsed().map_or(0, |elapsed| elapsed.subsec_nanos());
    let token = format!("glide_{}_{nanos}", std::process::id());
    let request = format!("{PORTAL_PATH}/request/{sender}/{token}");
    let response = zbus::blocking::Proxy::new(&connection, PORTAL, request.as_str(), "org.freedesktop.portal.Request")?;
    let mut signals = response.receive_signal("Response")?;
    let chooser = zbus::blocking::Proxy::new(&connection, PORTAL, PORTAL_PATH, "org.freedesktop.portal.FileChooser")?;
    let folder = ask.folder.as_deref().map(nul_terminated);
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("modal", Value::from(true));
    options.insert("accept_label", Value::from(ask.accept.as_str()));
    if let Some(folder) = folder.as_deref() {
        options.insert("current_folder", Value::from(folder));
    }
    let method = match ask.purpose {
        Purpose::Open => {
            options.insert("multiple", Value::from(true));
            "OpenFile"
        }
        Purpose::Folder => {
            options.insert("directory", Value::from(true));
            "OpenFile"
        }
        Purpose::Save => {
            if let Some(name) = &ask.name {
                options.insert("current_name", Value::from(name.as_str()));
            }
            "SaveFile"
        }
    };
    let _: OwnedObjectPath = chooser.call(method, &("", ask.title.as_str(), options))?;
    let Some(message) = signals.next() else { return Ok(Chosen::Cancelled) };
    let (code, results): (u32, HashMap<String, OwnedValue>) = message.body().deserialize()?;
    if code != 0 {
        return Ok(Chosen::Cancelled);
    }
    let uris = results.get("uris").and_then(|uris| <Vec<String>>::try_from(uris.try_clone().ok()?).ok());
    let paths: Vec<PathBuf> = uris.unwrap_or_default().iter().filter_map(|uri| uri_path(uri)).collect();
    Ok(if paths.is_empty() { Chosen::Cancelled } else { Chosen::Paths(paths) })
}

fn files_app(ask: &Ask) -> Chosen {
    let request = Request {
        mode: if ask.purpose == Purpose::Save { Mode::Save } else { Mode::Open },
        title: ask.title.clone(),
        accept_label: Some(ask.accept.clone()),
        multiple: ask.purpose == Purpose::Open,
        directory: ask.purpose == Purpose::Folder,
        current_name: ask.name.clone(),
        current_folder: ask.folder.clone(),
        ..Request::default()
    };
    let Some(json) = request.to_json() else {
        return Chosen::Failed("the folder name is not valid UTF-8".into());
    };
    let output = std::process::Command::new("glide-files")
        .arg(glide_app::chooser::FLAG)
        .env(glide_app::chooser::REQUEST_ENV, json)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let paths: Vec<PathBuf> = Response::parse(&String::from_utf8_lossy(&output.stdout))
                .map(|response| response.uris.iter().filter_map(|uri| uri_path(uri)).collect())
                .unwrap_or_default();
            if paths.is_empty() { Chosen::Cancelled } else { Chosen::Paths(paths) }
        }
        Ok(_) => Chosen::Cancelled,
        Err(error) => Chosen::Failed(error.to_string()),
    }
}

pub fn spawn(ask: Ask) -> oneshot::Receiver<Chosen> {
    let (sender, receiver) = oneshot::channel();
    let spawned = std::thread::Builder::new().name("glide-chooser".into()).spawn(move || {
        let chosen = portal(&ask).unwrap_or_else(|_| files_app(&ask));
        let _ = sender.send(chosen);
    });
    drop(spawned);
    receiver
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uris_decode_percent_escapes_and_refuse_remote_hosts() {
        assert_eq!(
            uri_path("file:///home/u/%D0%BD%D0%BE%D1%82%D0%B0%D1%82%D0%BA%D0%B8.md"),
            Some(PathBuf::from("/home/u/нотатки.md"))
        );
        assert_eq!(uri_path("file://localhost/tmp/a%20b"), Some(PathBuf::from("/tmp/a b")));
        assert_eq!(uri_path("file://server/share"), None);
        assert_eq!(uri_path("https://example.com"), None);
        assert_eq!(uri_path("file:///a%00b"), None);
    }
}

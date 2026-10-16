use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

use iced::futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use zbus::blocking::Connection;
use zbus::fdo::{RequestNameFlags, RequestNameReply};

use crate::fs::path_from_arg;

const OWN_NAME: &str = "org.glide.Files";
const FILE_MANAGER: &str = "org.freedesktop.FileManager1";
const OBJECT: &str = "/org/freedesktop/FileManager1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Open(PathBuf),
    Reveal(PathBuf),
    Properties(PathBuf),
}

struct FileManager {
    requests: UnboundedSender<Request>,
}

impl FileManager {
    fn forward(&self, uris: Vec<String>, startup_id: String, make: fn(PathBuf) -> Request) {
        drop(startup_id);
        for path in uris.into_iter().filter_map(|uri| path_from_arg(&uri)) {
            let _ = self.requests.unbounded_send(make(path));
        }
    }
}

#[zbus::interface(name = "org.freedesktop.FileManager1")]
impl FileManager {
    fn show_folders(&self, uris: Vec<String>, startup_id: String) {
        self.forward(uris, startup_id, Request::Open);
    }

    fn show_items(&self, uris: Vec<String>, startup_id: String) {
        self.forward(uris, startup_id, Request::Reveal);
    }

    fn show_item_properties(&self, uris: Vec<String>, startup_id: String) {
        self.forward(uris, startup_id, Request::Properties);
    }
}

struct Service {
    _connection: Connection,
    requests: Mutex<Option<UnboundedReceiver<Request>>>,
}

static SERVICE: OnceLock<Service> = OnceLock::new();

pub enum Claim {
    First,
    Forwarded,
    Unavailable,
}

fn forward(connection: &Connection, target: Option<&Path>) -> bool {
    let home = crate::fs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let target = target.unwrap_or(&home);
    let method = if target.is_dir() { "ShowFolders" } else { "ShowItems" };
    let uris = vec![crate::fs::file_uri(target)];
    connection.call_method(Some(OWN_NAME), OBJECT, Some(FILE_MANAGER), method, &(uris, "")).is_ok()
}

#[must_use]
pub fn claim(target: Option<&Path>) -> Claim {
    let (sender, receiver) = unbounded();
    let built = zbus::blocking::connection::Builder::session()
        .and_then(|builder| builder.serve_at(OBJECT, FileManager { requests: sender }))
        .and_then(zbus::blocking::connection::Builder::build);
    let Ok(connection) = built else {
        return Claim::Unavailable;
    };
    let flags = RequestNameFlags::DoNotQueue.into();
    match connection.request_name_with_flags(OWN_NAME, flags) {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {}
        Ok(_) => {
            return if forward(&connection, target) { Claim::Forwarded } else { Claim::Unavailable };
        }
        Err(_) => return Claim::Unavailable,
    }
    let _ = connection.request_name_with_flags(FILE_MANAGER, flags);
    let _ = SERVICE.set(Service { _connection: connection, requests: Mutex::new(Some(receiver)) });
    Claim::First
}

#[must_use]
pub fn requests() -> UnboundedReceiver<Request> {
    SERVICE
        .get()
        .and_then(|service| service.requests.lock().unwrap_or_else(PoisonError::into_inner).take())
        .unwrap_or_else(|| unbounded().1)
}

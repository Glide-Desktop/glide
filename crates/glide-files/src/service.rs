use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

use iced::futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use zbus::blocking::Connection;
use zbus::fdo::{RequestNameFlags, RequestNameReply};

use crate::fs::path_from_arg;

const OWN_NAME: &str = "org.glide.Files";
const FILE_MANAGER: &str = "org.freedesktop.FileManager1";
const OBJECT: &str = "/org/freedesktop/FileManager1";
const SEARCH_NAME: &str = "org.glide.Files.Search";
const SEARCH_PATH: &str = "/org/glide/Files/SearchProvider";
const INDEX_DEPTH: usize = 4;
const INDEX_LIMIT: usize = 5000;
const INDEX_AGE: std::time::Duration = std::time::Duration::from_mins(1);
const SEARCH_LIMIT: usize = 20;

struct Folders {
    requests: UnboundedSender<Request>,
    cache: Mutex<Option<(std::time::Instant, Vec<PathBuf>)>>,
}

const QUICK_DIRS: [&str; 5] =
    ["XDG_DOCUMENTS_DIR", "XDG_DOWNLOAD_DIR", "XDG_PICTURES_DIR", "XDG_MUSIC_DIR", "XDG_VIDEOS_DIR"];

#[must_use]
pub fn user_dirs(text: &str, home: &Path) -> Vec<PathBuf> {
    QUICK_DIRS
        .iter()
        .filter_map(|key| {
            let line = text.lines().map(str::trim).find(|line| line.starts_with(&format!("{key}=")))?;
            let value = line.split_once('=')?.1.trim().trim_matches('"');
            let path = match value.strip_prefix("$HOME") {
                Some(rest) => home.join(rest.trim_start_matches('/')),
                None => PathBuf::from(value),
            };
            (path != home).then_some(path)
        })
        .collect()
}

fn quick_folders() -> Vec<(String, PathBuf)> {
    let lang = crate::i18n::system_lang();
    let Some(home) = crate::fs::home_dir() else {
        return Vec::new();
    };
    let text = glide_config::xdg_path("", "user-dirs.dirs")
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    let mut folders = vec![(crate::i18n::Key::Home.text(lang).to_string(), home.clone())];
    folders.extend(user_dirs(&text, &home).into_iter().filter(|path| path.is_dir()).map(|path| {
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        (name, path)
    }));
    folders
}

fn quicklist(requests: UnboundedSender<Request>) -> glide_app::quicklist::QuickMenu {
    let folders = quick_folders();
    let labels = folders.iter().map(|(label, _)| label.clone()).collect();
    glide_app::quicklist::QuickMenu::new(labels, move |index| {
        if let Some((_, path)) = folders.get(index) {
            let _ = requests.unbounded_send(Request::Open(path.clone()));
        }
    })
}

#[must_use]
pub(crate) fn index_folders(root: &Path, depth: usize, limit: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut level = vec![root.to_path_buf()];
    for _ in 0..depth {
        let mut next = Vec::new();
        for dir in &level {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let hidden = entry.file_name().to_string_lossy().starts_with('.');
                if hidden || !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    continue;
                }
                found.push(entry.path());
                next.push(entry.path());
                if found.len() >= limit {
                    return found;
                }
            }
        }
        level = next;
    }
    found
}

impl Folders {
    fn folders(&self) -> Vec<PathBuf> {
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((at, folders)) = cache.as_ref()
            && at.elapsed() < INDEX_AGE
        {
            return folders.clone();
        }
        let home = crate::fs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let folders = index_folders(&home, INDEX_DEPTH, INDEX_LIMIT);
        *cache = Some((std::time::Instant::now(), folders.clone()));
        folders
    }
}

impl glide_app::search_provider::Source for Folders {
    fn search(&self, terms: &[String]) -> Vec<glide_app::search_provider::Item> {
        let home = crate::fs::home_dir();
        let mut hits: Vec<PathBuf> = self
            .folders()
            .into_iter()
            .filter(|path| {
                path.file_name().is_some_and(|name| glide_app::search_provider::matches(&name.to_string_lossy(), terms))
            })
            .collect();
        hits.sort_by_key(|path| path.components().count());
        hits.into_iter()
            .take(SEARCH_LIMIT)
            .map(|path| {
                let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
                let description = match home.as_deref().and_then(|home| parent.strip_prefix(home).ok()) {
                    Some(rest) if rest.as_os_str().is_empty() => String::from("~"),
                    Some(rest) => format!("~/{}", rest.display()),
                    None => parent.display().to_string(),
                };
                glide_app::search_provider::Item {
                    id: path.display().to_string(),
                    name: path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
                    description,
                }
            })
            .collect()
    }

    fn activate(&self, id: &str) {
        let _ = self.requests.unbounded_send(Request::Open(PathBuf::from(id)));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Open(PathBuf),
    Reveal(PathBuf),
    Properties(PathBuf),
    Activate(String),
}

struct FileManager {
    requests: UnboundedSender<Request>,
}

impl FileManager {
    fn forward(&self, uris: Vec<String>, startup_id: String, make: fn(PathBuf) -> Request) {
        for path in uris.into_iter().filter_map(|uri| path_from_arg(&uri)) {
            let _ = self.requests.unbounded_send(make(path));
        }
        if !startup_id.is_empty() {
            let _ = self.requests.unbounded_send(Request::Activate(startup_id));
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
    let folders = Folders { requests: sender.clone(), cache: Mutex::default() };
    let menu = quicklist(sender.clone());
    let built = zbus::blocking::connection::Builder::session()
        .and_then(|builder| builder.serve_at(OBJECT, FileManager { requests: sender }))
        .and_then(|builder| builder.serve_at(SEARCH_PATH, glide_app::search_provider::Provider::new(folders)))
        .and_then(|builder| builder.serve_at(glide_app::quicklist::PATH, menu))
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
    let _ = connection.request_name_with_flags(SEARCH_NAME, flags);
    glide_app::quicklist::keep_announced(&connection, "glide-files");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_dirs_expand_home_and_skip_home_itself() {
        let text = "XDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_DOWNLOAD_DIR=\"$HOME/Завантаження\"\nXDG_MUSIC_DIR=\"$HOME/\"\nXDG_VIDEOS_DIR=\"/media/films\"\n";
        let home = Path::new("/home/me");
        assert_eq!(user_dirs(text, home), [home.join("Завантаження"), PathBuf::from("/media/films")]);
    }

    #[test]
    fn the_folder_index_skips_hidden_folders_and_files_and_stops_at_its_depth() {
        let root = std::env::temp_dir().join(format!("glide-files-index-{}", std::process::id()));
        for dir in ["a/b/c", ".hidden/x"] {
            std::fs::create_dir_all(root.join(dir)).expect("dirs");
        }
        std::fs::write(root.join("a/file.txt"), "x").expect("file");
        let found = index_folders(&root, 2, 100);
        assert!(found.contains(&root.join("a")) && found.contains(&root.join("a/b")));
        assert!(!found.contains(&root.join("a/b/c")), "deeper than the limit");
        assert!(found.iter().all(|path| !path.to_string_lossy().contains(".hidden")));
        assert!(!found.contains(&root.join("a/file.txt")));
        assert_eq!(index_folders(&root, 4, 1).len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }
}

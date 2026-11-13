use std::path::{Path, PathBuf};

use super::trash::Trash;
use super::uri::path_from_arg;
use crate::i18n::{Key, Lang};
use crate::icons::Icon;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub name: String,
    pub path: PathBuf,
    pub icon: Icon,
}

impl Place {
    fn new(name: impl Into<String>, path: PathBuf, icon: Icon) -> Self {
        Self { name: name.into(), path, icon }
    }
}

const USER_DIRS: [(Key, &str, &str, Icon); 6] = [
    (Key::Desktop, "XDG_DESKTOP_DIR", "Desktop", Icon::Desktop),
    (Key::Documents, "XDG_DOCUMENTS_DIR", "Documents", Icon::Documents),
    (Key::Downloads, "XDG_DOWNLOAD_DIR", "Downloads", Icon::Downloads),
    (Key::Pictures, "XDG_PICTURES_DIR", "Pictures", Icon::Pictures),
    (Key::Music, "XDG_MUSIC_DIR", "Music", Icon::Music),
    (Key::Videos, "XDG_VIDEOS_DIR", "Videos", Icon::Videos),
];

const REMOVABLE_ROOTS: [&str; 3] = ["/run/media/", "/media/", "/mnt/"];

pub use glide_config::{config_home, data_home, home as home_dir};

#[must_use]
pub fn standard_places(lang: Lang, trash: &Trash) -> Vec<Place> {
    let mut places = Vec::new();
    if let Some(home) = home_dir() {
        let user_dirs = read_user_dirs(&home);
        for (key, var, fallback, icon) in USER_DIRS {
            let path = user_dirs
                .iter()
                .find(|(name, _)| name == var)
                .map_or_else(|| home.join(fallback), |(_, path)| path.clone());
            if path != home && path.is_dir() {
                places.push(Place::new(key.text(lang), path, icon));
            }
        }
        places.insert(0, Place::new(Key::Home.text(lang), home, Icon::Home));
    }
    places.push(Place::new(Key::Trash.text(lang), trash.files(), Icon::Trash));
    places
}

fn read_user_dirs(home: &Path) -> Vec<(String, PathBuf)> {
    let Some(content) = config_home().and_then(|config| std::fs::read_to_string(config.join("user-dirs.dirs")).ok())
    else {
        return Vec::new();
    };
    content
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| {
            let (key, value) = line.trim().split_once('=')?;
            let value = value.trim().trim_matches('"');
            let path = value
                .strip_prefix("$HOME")
                .map_or_else(|| PathBuf::from(value), |rest| home.join(rest.trim_start_matches('/')));
            Some((key.trim().to_string(), path))
        })
        .collect()
}

#[must_use]
pub fn templates_dir() -> Option<PathBuf> {
    let home = home_dir()?;
    let dir = read_user_dirs(&home)
        .into_iter()
        .find(|(name, _)| name == "XDG_TEMPLATES_DIR")
        .map_or_else(|| home.join("Templates"), |(_, path)| path);
    (dir != home).then_some(dir)
}

#[must_use]
pub fn templates_in(dir: &Path) -> Vec<PathBuf> {
    let visible = |path: &Path| !path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.'));
    let mut found: Vec<PathBuf> = Vec::new();
    let mut pending = vec![(dir.to_path_buf(), 0)];
    while let Some((folder, depth)) = pending.pop() {
        let Ok(children) = std::fs::read_dir(&folder) else {
            continue;
        };
        for path in children.flatten().map(|child| child.path()).filter(|path| visible(path)) {
            if path.is_dir() {
                if depth < 2 {
                    pending.push((path, depth + 1));
                }
            } else {
                found.push(path);
            }
        }
    }
    found.sort_by(|a, b| super::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    found
}

#[must_use]
pub fn mounted_drives(lang: Lang) -> Vec<Place> {
    let mut drives = vec![Place::new(Key::Root.text(lang), PathBuf::from("/"), Icon::Drive)];
    let Ok(mounts) = std::fs::read_to_string("/proc/self/mounts") else {
        return drives;
    };
    for point in mounts.lines().filter_map(|line| line.split(' ').nth(1)) {
        let point = unescape_mount(point);
        if !REMOVABLE_ROOTS.iter().any(|root| point.starts_with(root)) {
            continue;
        }
        let path = PathBuf::from(point);
        if drives.iter().any(|drive| drive.path == path) {
            continue;
        }
        let name = path.file_name().map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy()).into_owned();
        drives.push(Place::new(name, path, Icon::Drive));
    }
    drives
}

fn unescape_mount(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut rest = field;
    while let Some(slash) = rest.find('\\') {
        out.push_str(&rest[..slash]);
        let code = rest.get(slash + 1..slash + 4).and_then(|octal| u8::from_str_radix(octal, 8).ok());
        if let Some(byte) = code {
            out.push(char::from(byte));
            rest = &rest[slash + 4..];
        } else {
            out.push('\\');
            rest = &rest[slash + 1..];
        }
    }
    out.push_str(rest);
    out
}

#[must_use]
pub fn bookmarks_file() -> Option<PathBuf> {
    config_home().map(|config| config.join("gtk-3.0/bookmarks"))
}

#[must_use]
pub fn load_bookmarks() -> Vec<Place> {
    let Some(content) = bookmarks_file().and_then(|file| std::fs::read_to_string(file).ok()) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| {
            let (uri, label) =
                line.trim().split_once(' ').map_or((line.trim(), None), |(uri, label)| (uri, Some(label)));
            let path = uri.starts_with("file://").then(|| path_from_arg(uri))??;
            let name = label.map_or_else(
                || path.file_name().map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy()).into_owned(),
                str::to_string,
            );
            Some(Place::new(name, path, Icon::Folder))
        })
        .collect()
}

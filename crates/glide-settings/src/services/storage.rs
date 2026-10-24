use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::apps;
use super::run;

const PSEUDO_FS: &[&str] = &[
    "tmpfs",
    "devtmpfs",
    "squashfs",
    "overlay",
    "efivarfs",
    "ramfs",
    "proc",
    "sysfs",
    "devpts",
    "cgroup2",
    "fuse.portal",
    "fuse.gvfsd-fuse",
    "autofs",
    "nsfs",
];

const HIDDEN_MOUNTS: &[&str] = &["/run", "/sys", "/proc", "/dev", "/var/lib/docker", "/snap"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub source: String,
    pub fstype: String,
    pub total: u64,
    pub used: u64,
    pub avail: u64,
    pub mount: String,
}

impl Volume {
    #[must_use]
    pub fn fraction(&self) -> f32 {
        if self.total == 0 { 0.0 } else { (self.used as f64 / self.total as f64) as f32 }
    }
}

#[must_use]
pub fn parse_volumes(text: &str) -> Vec<Volume> {
    let mut volumes: Vec<Volume> = text
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let source = fields.next()?.to_string();
            let fstype = fields.next()?.to_string();
            let total: u64 = fields.next()?.parse().ok()?;
            let used: u64 = fields.next()?.parse().ok()?;
            let avail: u64 = fields.next()?.parse().ok()?;
            let mount = fields.collect::<Vec<_>>().join(" ");
            Some(Volume { source, fstype, total, used, avail, mount })
        })
        .filter(|volume| {
            volume.total > 0
                && !PSEUDO_FS.contains(&volume.fstype.as_str())
                && !HIDDEN_MOUNTS.iter().any(|prefix| volume.mount.starts_with(prefix))
        })
        .collect();
    volumes.sort_by(|a, b| {
        (a.mount != "/")
            .cmp(&(b.mount != "/"))
            .then_with(|| a.mount.len().cmp(&b.mount.len()))
            .then_with(|| a.mount.cmp(&b.mount))
    });
    let mut seen = std::collections::HashSet::new();
    volumes.retain(|volume| seen.insert(volume.source.clone()));
    volumes
}

#[must_use]
pub fn volumes() -> Vec<Volume> {
    run::output("df", &["-B1", "--output=source,fstype,size,used,avail,target"])
        .map(|text| parse_volumes(&text))
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Documents,
    Downloads,
    Pictures,
    Music,
    Videos,
    Desktop,
    Cache,
    Trash,
    Other,
}

impl Category {
    pub const ALL: &'static [Self] = &[
        Self::Documents,
        Self::Downloads,
        Self::Pictures,
        Self::Music,
        Self::Videos,
        Self::Desktop,
        Self::Cache,
        Self::Trash,
        Self::Other,
    ];

    const fn user_dir_key(self) -> Option<&'static str> {
        match self {
            Self::Documents => Some("XDG_DOCUMENTS_DIR"),
            Self::Downloads => Some("XDG_DOWNLOAD_DIR"),
            Self::Pictures => Some("XDG_PICTURES_DIR"),
            Self::Music => Some("XDG_MUSIC_DIR"),
            Self::Videos => Some("XDG_VIDEOS_DIR"),
            Self::Desktop => Some("XDG_DESKTOP_DIR"),
            _ => None,
        }
    }

    const fn fallback(self) -> &'static str {
        match self {
            Self::Documents => "Documents",
            Self::Downloads => "Downloads",
            Self::Pictures => "Pictures",
            Self::Music => "Music",
            Self::Videos => "Videos",
            Self::Desktop => "Desktop",
            Self::Cache => ".cache",
            Self::Trash => ".local/share/Trash",
            Self::Other => "",
        }
    }
}

#[must_use]
pub fn parse_user_dirs(text: &str, home: &Path) -> HashMap<String, PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let value = value.trim().trim_matches('"');
            let path = if let Some(rest) = value.strip_prefix("$HOME") {
                home.join(rest.trim_start_matches('/'))
            } else if value.starts_with('/') {
                PathBuf::from(value)
            } else {
                return None;
            };
            Some((key.trim().to_string(), path))
        })
        .collect()
}

pub use glide_config::home as home_dir;

#[must_use]
pub fn cache_dir(home: &Path) -> PathBuf {
    glide_config::cache_home().unwrap_or_else(|| home.join(".cache"))
}

#[must_use]
pub(crate) fn trash_dir(home: &Path) -> PathBuf {
    glide_config::data_home().unwrap_or_else(|| home.join(".local/share")).join("Trash")
}

#[must_use]
pub fn category_roots<S: std::hash::BuildHasher>(
    home: &Path, user_dirs: &HashMap<String, PathBuf, S>,
) -> Vec<(Category, PathBuf)> {
    let mut roots = Vec::new();
    for category in Category::ALL {
        let path = match category {
            Category::Cache => cache_dir(home),
            Category::Trash => trash_dir(home),
            Category::Other => continue,
            _ => category
                .user_dir_key()
                .and_then(|key| user_dirs.get(key).cloned())
                .unwrap_or_else(|| home.join(category.fallback())),
        };
        if path != home && path.starts_with(home) && !roots.iter().any(|(_, p)| *p == path) {
            roots.push((*category, path));
        }
    }
    roots
}

#[must_use]
pub fn roots() -> (PathBuf, Vec<(Category, PathBuf)>) {
    let home = home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let text = apps::config_home()
        .map(|dir| dir.join("user-dirs.dirs"))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    let roots = category_roots(&home, &parse_user_dirs(&text, &home));
    (home, roots)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HomeUsage {
    pub home: PathBuf,
    pub total: u64,
    pub sizes: Vec<(Category, u64)>,
    pub paths: Vec<(Category, PathBuf)>,
    pub same_disk_as_root: bool,
}

impl HomeUsage {
    #[must_use]
    pub fn size(&self, category: Category) -> u64 {
        self.sizes.iter().find(|(kind, _)| *kind == category).map_or(0, |(_, size)| *size)
    }

    #[must_use]
    pub fn path(&self, category: Category) -> Option<&PathBuf> {
        self.paths.iter().find(|(kind, _)| *kind == category).map(|(_, path)| path)
    }
}

fn disk_bytes(meta: &std::fs::Metadata) -> u64 {
    meta.blocks().saturating_mul(512)
}

#[must_use]
pub fn measure_tree(home: &Path, roots: &[(Category, PathBuf)]) -> HomeUsage {
    let lookup: HashMap<&Path, Category> = roots.iter().map(|(category, path)| (path.as_path(), *category)).collect();
    let mut sizes: HashMap<Category, u64> = HashMap::new();
    let device = std::fs::symlink_metadata(home).map(|meta| meta.dev()).ok();
    let mut stack: Vec<(PathBuf, Category)> = vec![(home.to_path_buf(), Category::Other)];
    let mut total: u64 = 0;
    while let Some((dir, category)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            let bytes = disk_bytes(&meta);
            let kind = lookup.get(path.as_path()).copied().unwrap_or(category);
            total = total.saturating_add(bytes);
            *sizes.entry(kind).or_default() += bytes;
            if meta.is_dir() && device.is_none_or(|dev| meta.dev() == dev) {
                stack.push((path, kind));
            }
        }
    }
    let root_device = std::fs::metadata("/").map(|meta| meta.dev()).ok();
    HomeUsage {
        home: home.to_path_buf(),
        total,
        sizes: Category::ALL.iter().map(|category| (*category, sizes.get(category).copied().unwrap_or(0))).collect(),
        paths: roots.to_vec(),
        same_disk_as_root: device.is_some() && device == root_device,
    }
}

#[must_use]
pub(crate) fn measure_home() -> HomeUsage {
    let (home, roots) = roots();
    measure_tree(&home, &roots)
}

fn remove_children(dir: &Path) -> (u64, usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, 0);
    };
    let mut freed = 0;
    let mut failed = 0;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        let (size, removed) = if meta.is_dir() {
            let size = measure_tree(&path, &[]).total + disk_bytes(&meta);
            (size, std::fs::remove_dir_all(&path))
        } else {
            (disk_bytes(&meta), std::fs::remove_file(&path))
        };
        match removed {
            Ok(()) => freed += size,
            Err(_) => failed += 1,
        }
    }
    (freed, failed)
}

pub(crate) fn clear_cache() -> Result<u64, String> {
    let home = home_dir().ok_or_else(|| String::from("HOME is not set"))?;
    let cache = cache_dir(&home);
    if cache == home || !cache.starts_with(&home) {
        return Err(format!("{} is outside the home folder", cache.display()));
    }
    let (freed, _) = remove_children(&cache);
    Ok(freed)
}

pub(crate) fn empty_trash() -> Result<u64, String> {
    let home = home_dir().ok_or_else(|| String::from("HOME is not set"))?;
    let trash = trash_dir(&home);
    let before = measure_tree(&trash, &[]).total;
    if run::available("gio") && run::status("gio", &["trash", "--empty"]).is_ok() {
        let after = measure_tree(&trash, &[]).total;
        return Ok(before.saturating_sub(after));
    }
    let (files, _) = remove_children(&trash.join("files"));
    let (info, _) = remove_children(&trash.join("info"));
    let (expunged, _) = remove_children(&trash.join("expunged"));
    Ok(files + info + expunged)
}

pub fn open(path: &Path) -> Result<(), String> {
    apps::open_path(path).map(|_| ())
}

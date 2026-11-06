use std::collections::BTreeMap;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::doc::{self, Engine, Format};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Layout {
    Single,
    #[default]
    Continuous,
    Spread,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum Zoom {
    #[default]
    FitWidth,
    FitPage,
    Percent(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Paper {
    #[default]
    White,
    Sepia,
    Night,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Tab {
    #[default]
    Thumbs,
    Outline,
    Notes,
    Marks,
}

#[allow(clippy::struct_excessive_bools)] // Independent per-file view switches the reading settings popover toggles one by one
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Memory {
    pub page: usize,
    pub pages: usize,
    pub zoom: Zoom,
    pub layout: Layout,
    pub paper: Paper,
    pub keep_images: bool,
    pub cover_alone: bool,
    pub crop: bool,
    pub rtl: bool,
    pub tab: Tab,
    pub side: bool,
    pub bookmarks: Vec<(String, usize)>,
    pub opened: u64,
    pub favourite: bool,
    pub notes: usize,
    pub unsigned: bool,
    pub filled: (usize, usize),
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            page: 0,
            pages: 0,
            zoom: Zoom::FitWidth,
            layout: Layout::Continuous,
            paper: Paper::White,
            keep_images: true,
            cover_alone: true,
            crop: false,
            rtl: false,
            tab: Tab::Thumbs,
            side: true,
            bookmarks: Vec::new(),
            opened: 0,
            favourite: false,
            notes: 0,
            unsigned: false,
            filled: (0, 0),
        }
    }
}

impl Memory {
    #[must_use]
    pub fn progress(&self) -> f32 {
        if self.pages == 0 { 0.0 } else { (self.page + 1) as f32 / self.pages as f32 }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub folders: Vec<PathBuf>,
}

fn state_file() -> Option<PathBuf> {
    Some(glide_config::state_home()?.join("glide/folio/files.toml"))
}

fn prefs_file() -> Option<PathBuf> {
    Some(glide_config::config_home()?.join("glide/folio.toml"))
}

fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension(format!("{}.part", std::process::id()));
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, path)
}

#[must_use]
pub fn load_memories() -> BTreeMap<String, Memory> {
    state_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| toml::from_str::<BTreeMap<String, Memory>>(&text).ok())
        .unwrap_or_default()
}

pub fn save_memories(memories: &BTreeMap<String, Memory>) {
    if let (Some(path), Ok(text)) = (state_file(), toml::to_string(memories)) {
        let _ = write_atomic(&path, &text);
    }
}

static SAVES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn save_memories_later(memories: BTreeMap<String, Memory>) -> std::thread::JoinHandle<()> {
    let ticket = SAVES.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        let Ok(_guard) = WRITER.lock() else {
            return;
        };
        if SAVES.load(std::sync::atomic::Ordering::SeqCst) == ticket {
            save_memories(&memories);
        }
    })
}

#[must_use]
pub fn load_prefs() -> Prefs {
    prefs_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_prefs(prefs: &Prefs) {
    if let (Some(path), Ok(text)) = (prefs_file(), toml::to_string(prefs)) {
        let _ = write_atomic(&path, &text);
    }
}

#[must_use]
pub fn key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!(" {name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

#[must_use]
pub fn recent_from(xbel: &str) -> Vec<(PathBuf, u64)> {
    let mut out: Vec<(PathBuf, u64)> = Vec::new();
    for tag in xbel.split("<bookmark ").skip(1) {
        let head = format!(" {}", tag.split('>').next().unwrap_or(""));
        let Some(href) = attribute(&head, "href").map(doc::epub::entities) else {
            continue;
        };
        let Some(path) = glide_config::uri::local_path(&href) else {
            continue;
        };
        if Format::of(&path).is_none() || out.iter().any(|(known, _)| *known == path) {
            continue;
        }
        let visited = attribute(&head, "visited").or_else(|| attribute(&head, "modified")).map_or(0, stamp);
        out.push((path, visited));
    }
    out
}

fn stamp(iso: &str) -> u64 {
    let part = |range: std::ops::Range<usize>| iso.get(range).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
    let (year, month, day) = (part(0..4), part(5..7), part(8..10));
    let seconds = part(11..13) * 3600 + part(14..16) * 60 + part(17..19);
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + seconds).unwrap_or(0)
}

#[must_use]
pub fn recent() -> Vec<(PathBuf, u64)> {
    glide_config::data_home()
        .and_then(|dir| std::fs::read_to_string(dir.join("recently-used.xbel")).ok())
        .map(|text| recent_from(&text))
        .unwrap_or_default()
        .into_iter()
        .filter(|(path, _)| path.is_file())
        .collect()
}

#[must_use]
pub fn scan_folder(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0u8)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let hidden = path.file_name().is_some_and(|n| n.as_bytes().first() == Some(&b'.'));
            if hidden {
                continue;
            }
            if path.is_dir() {
                if depth < 2 {
                    stack.push((path, depth + 1));
                }
            } else if Format::of(&path).is_some() {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub path: PathBuf,
    pub format: Format,
    pub opened: u64,
    pub modified: u64,
    pub bytes: u64,
    pub folder: Option<PathBuf>,
}

#[must_use]
pub fn entries(prefs: &Prefs, memories: &BTreeMap<String, Memory>) -> Vec<Entry> {
    let mut seen: BTreeMap<PathBuf, Entry> = BTreeMap::new();
    let mut add = |path: PathBuf, opened: u64, folder: Option<PathBuf>| {
        let Some(format) = Format::of(&path) else {
            return;
        };
        let Ok(meta) = std::fs::metadata(&path) else {
            return;
        };
        let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        let entry = seen.entry(path.clone()).or_insert(Entry {
            path,
            format,
            opened: 0,
            modified,
            bytes: meta.len(),
            folder: None,
        });
        entry.opened = entry.opened.max(opened);
        if entry.folder.is_none() {
            entry.folder = folder;
        }
    };
    for (path, visited) in recent() {
        add(path, visited, None);
    }
    for (key, memory) in memories {
        add(PathBuf::from(key), memory.opened, None);
    }
    for folder in &prefs.folders {
        for path in scan_folder(folder) {
            add(path, 0, Some(folder.clone()));
        }
    }
    let mut out: Vec<Entry> = seen.into_values().collect();
    out.sort_by(|a, b| {
        b.opened.max(b.modified / 2).cmp(&a.opened.max(a.modified / 2)).then_with(|| a.path.cmp(&b.path))
    });
    out
}

fn fnv(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

pub const COVER_SIDE: u32 = 320;

#[must_use]
pub fn cover_path(path: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    let key =
        fnv(path.as_os_str().as_bytes().iter().copied().chain(modified.to_le_bytes()).chain(meta.len().to_le_bytes()));
    Some(glide_config::cache_home()?.join("glide/thumbnails").join(format!("{key:016x}-{COVER_SIDE}.png")))
}

#[must_use]
pub fn cover(path: &Path) -> Option<PathBuf> {
    let output = cover_path(path)?;
    if output.is_file() {
        return Some(output);
    }
    let failed = output.with_extension("fail");
    if failed.exists() {
        return None;
    }
    let made = std::panic::catch_unwind(|| {
        let mut engine: Box<dyn Engine> = doc::open(path, None).ok()?;
        let size = *engine.opened().pages.first()?;
        let scale = COVER_SIDE as f32 / size.w.max(size.h);
        engine.render(0, scale)
    })
    .ok()
    .flatten();
    let Some(image) = made else {
        if let Some(dir) = failed.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&failed, b"");
        return None;
    };
    std::fs::create_dir_all(output.parent()?).ok()?;
    let partial = output.with_extension(format!("{}.part.png", std::process::id()));
    image.save_with_format(&partial, image::ImageFormat::Png).ok()?;
    std::fs::rename(&partial, &output).ok()?;
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_files_come_from_the_xbel_in_order_without_duplicates() {
        let xbel = r#"<?xml version="1.0"?><xbel>
            <bookmark href="file:///home/u/%D0%9F%D0%BE%D1%81%D1%96%D0%B1%D0%BD%D0%B8%D0%BA.pdf" added="2026-11-01T10:00:00Z" visited="2026-11-06T10:42:00Z"></bookmark>
            <bookmark href="file:///home/u/photo.png" visited="2026-11-05T10:00:00Z"></bookmark>
            <bookmark href="file:///home/u/book.epub" modified="2026-11-05T09:00:00Z"></bookmark>
            <bookmark href="file:///home/u/%D0%9F%D0%BE%D1%81%D1%96%D0%B1%D0%BD%D0%B8%D0%BA.pdf" visited="2026-10-01T10:00:00Z"></bookmark>
        </xbel>"#;
        let found = recent_from(xbel);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, PathBuf::from("/home/u/Посібник.pdf"));
        assert_eq!(found[1].0, PathBuf::from("/home/u/book.epub"));
        assert_eq!(found[0].1, 1_793_961_720);
    }

    #[test]
    fn memories_survive_a_round_trip() {
        let mut memories = BTreeMap::new();
        let memory = Memory {
            page: 6,
            pages: 42,
            paper: Paper::Night,
            bookmarks: vec![("Перемикання".into(), 6)],
            ..Memory::default()
        };
        memories.insert("/home/u/a.pdf".to_string(), memory.clone());
        let text = toml::to_string(&memories).unwrap();
        let back: BTreeMap<String, Memory> = toml::from_str(&text).unwrap();
        assert_eq!(back.get("/home/u/a.pdf"), Some(&memory));
        assert!((memory.progress() - 7.0 / 42.0).abs() < 1e-6);
    }
}

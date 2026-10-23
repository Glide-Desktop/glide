use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use glide_sdk::theme::Accent;
pub use glide_sdk::widget::Art;

use crate::fs::{FileCategory, FileEntry};
use crate::icons::Icon;

const SIZES: [&str; 7] = ["64x64", "scalable", "48x48", "96x96", "128x128", "256x256", "512x512"];
const CONTEXTS: [&str; 6] = ["places", "mimetypes", "apps", "devices", "status", "legacy"];
const DEFAULT_THEME: &str = "Papirus";

const BY_EXTENSION: [(&str, &str); 38] = [
    ("rs", "text-x-rust"),
    ("py", "text-x-python"),
    ("js", "application-javascript"),
    ("ts", "text-x-typescript"),
    ("json", "application-json"),
    ("html", "text-html"),
    ("css", "text-css"),
    ("md", "text-markdown"),
    ("sh", "application-x-shellscript"),
    ("bash", "application-x-shellscript"),
    ("zsh", "application-x-shellscript"),
    ("c", "text-x-csrc"),
    ("h", "text-x-chdr"),
    ("cpp", "text-x-c++src"),
    ("go", "text-x-go"),
    ("java", "text-x-java"),
    ("yaml", "application-x-yaml"),
    ("yml", "application-x-yaml"),
    ("xml", "text-xml"),
    ("csv", "text-csv"),
    ("log", "text-x-log"),
    ("svg", "image-svg+xml"),
    ("zip", "application-zip"),
    ("gz", "application-gzip"),
    ("tar", "application-x-tar"),
    ("7z", "application-x-7z-compressed"),
    ("rar", "application-x-rar"),
    ("iso", "application-x-cd-image"),
    ("deb", "application-vnd.debian.binary-package"),
    ("docx", "x-office-document"),
    ("odt", "x-office-document"),
    ("xlsx", "x-office-spreadsheet"),
    ("ods", "x-office-spreadsheet"),
    ("pptx", "x-office-presentation"),
    ("odp", "x-office-presentation"),
    ("epub", "application-epub+zip"),
    ("ttf", "font-x-generic"),
    ("desktop", "application-x-desktop"),
];

const TARGET: u32 = 256;

type Directories = Vec<(PathBuf, u32)>;

pub struct IconTheme {
    themes: Vec<Directories>,
    folder: &'static str,
    cache: RefCell<HashMap<String, Option<Art>>>,
}

fn icon_bases() -> Vec<PathBuf> {
    glide_config::data_dirs().into_iter().map(|dir| dir.join("icons")).collect()
}

fn inherits(theme: &Path) -> Vec<String> {
    std::fs::read_to_string(theme.join("index.theme"))
        .ok()
        .and_then(|index| {
            index.lines().find_map(|line| {
                line.strip_prefix("Inherits=").map(|list| {
                    list.split(',').map(|name| name.trim().to_string()).filter(|name| !name.is_empty()).collect()
                })
            })
        })
        .unwrap_or_default()
}

fn directories(theme: &Path) -> Directories {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current = String::new();
    let index = std::fs::read_to_string(theme.join("index.theme")).unwrap_or_default();
    for line in index.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        if let Some(name) = line.strip_prefix('[').and_then(|line| line.strip_suffix(']')) {
            current = name.to_string();
        } else if let Some((key, value)) = line.split_once('=') {
            let section = sections.entry(current.clone()).or_default();
            section.entry(key.trim().to_string()).or_insert_with(|| value.trim().to_string());
        }
    }
    let listed: Vec<String> = sections
        .get("Icon Theme")
        .map(|main| {
            ["Directories", "ScaledDirectories"]
                .iter()
                .filter_map(|key| main.get(*key))
                .flat_map(|list| list.split(','))
                .map(str::trim)
                .filter(|dir| !dir.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if listed.is_empty() {
        return SIZES
            .iter()
            .flat_map(|size| {
                let px = size.split('x').next().and_then(|side| side.parse::<u32>().ok()).unwrap_or(0);
                CONTEXTS.iter().map(move |context| (theme.join(size).join(context), px))
            })
            .collect();
    }
    listed
        .into_iter()
        .map(|dir| {
            let section = sections.get(&dir);
            let number = |key: &str, default: u32| {
                section.and_then(|values| values.get(key)).and_then(|value| value.parse().ok()).unwrap_or(default)
            };
            let px = number("Size", 0).saturating_mul(number("Scale", 1));
            (theme.join(&dir), px)
        })
        .collect()
}

fn find_file(dirs: &[(PathBuf, u32)], name: &str) -> Option<PathBuf> {
    let mut best: Option<((bool, u32), PathBuf)> = None;
    for (dir, px) in dirs {
        let vector = dir.join(format!("{name}.svg"));
        if vector.is_file() {
            return Some(vector);
        }
        let rank = if *px >= TARGET { (false, *px) } else { (true, u32::MAX - *px) };
        if best.as_ref().is_none_or(|(known, _)| rank < *known) {
            let raster = dir.join(format!("{name}.png"));
            if raster.is_file() {
                best = Some((rank, raster));
            }
        }
    }
    best.map(|(_, path)| path)
}

const fn folder_colour(accent: Accent) -> &'static str {
    match accent {
        Accent::Blue => "blue",
        Accent::Teal => "teal",
        Accent::Green => "green",
        Accent::Amber => "yellow",
        Accent::Orange => "orange",
        Accent::Red => "red",
        Accent::Purple => "violet",
        Accent::Graphite => "grey",
    }
}

impl IconTheme {
    #[must_use]
    pub fn new(name: Option<&str>, accent: Accent) -> Self {
        let bases = icon_bases();
        let mut names = vec![name.unwrap_or(DEFAULT_THEME).to_string()];
        let mut index = 0;
        while index < names.len() && names.len() < 8 {
            let parents: Vec<String> = bases
                .iter()
                .map(|base| base.join(&names[index]))
                .filter(|dir| dir.is_dir())
                .flat_map(|dir| inherits(&dir))
                .collect();
            for parent in parents {
                if !names.contains(&parent) {
                    names.push(parent);
                }
            }
            index += 1;
        }
        if !names.iter().any(|name| name == "hicolor") {
            names.push(String::from("hicolor"));
        }
        let themes = names
            .iter()
            .flat_map(|name| bases.iter().map(move |base| base.join(name)))
            .filter(|dir| dir.is_dir())
            .map(|dir| directories(&dir))
            .collect();
        Self { themes, folder: folder_colour(accent), cache: RefCell::default() }
    }

    #[must_use]
    pub fn lookup(&self, names: &[String]) -> Option<Art> {
        names.iter().find_map(|name| self.find(name))
    }

    fn find(&self, name: &str) -> Option<Art> {
        if let Some(hit) = self.cache.borrow().get(name) {
            return hit.clone();
        }
        let found = self.search(name);
        self.cache.borrow_mut().insert(name.to_string(), found.clone());
        found
    }

    fn search(&self, name: &str) -> Option<Art> {
        let path = Path::new(name);
        if path.is_absolute() && path.is_file() {
            return Some(Art::from_path(path));
        }
        let path = self.themes.iter().find_map(|dirs| find_file(dirs, name))?;
        Some(Art::from_path(&path))
    }

    #[must_use]
    pub fn names(&self, entry: &FileEntry, place: Option<Icon>, by_mime: &[String]) -> Vec<String> {
        let colour = self.folder;
        if entry.is_dir {
            let special = match place {
                Some(Icon::Home) => Some("home"),
                Some(Icon::Desktop) => Some("desktop"),
                Some(Icon::Documents) => Some("documents"),
                Some(Icon::Downloads) => Some("download"),
                Some(Icon::Pictures) => Some("pictures"),
                Some(Icon::Music) => Some("music"),
                Some(Icon::Videos) => Some("videos"),
                _ => None,
            };
            let mut names = Vec::new();
            if let Some(kind) = special {
                names.push(format!("folder-{colour}-{kind}"));
                names.push(format!("folder-{kind}"));
            }
            names.push(format!("folder-{colour}"));
            names.push(String::from("folder"));
            return names;
        }
        let extension = entry.path.extension().map(|ext| ext.to_string_lossy().to_lowercase()).unwrap_or_default();
        let mut names: Vec<String> = by_mime.to_vec();
        names.extend(BY_EXTENSION.iter().filter(|(ext, _)| *ext == extension).map(|(_, name)| (*name).to_string()));
        let fallback: &[&str] = match entry.category {
            FileCategory::Folder => &["folder"],
            FileCategory::Document => &["x-office-document"],
            FileCategory::Text => &["text-x-generic"],
            FileCategory::Code => &["text-x-script", "text-x-generic"],
            FileCategory::Image => &["image-x-generic"],
            FileCategory::Audio => &["audio-x-generic"],
            FileCategory::Video => &["video-x-generic"],
            FileCategory::Archive => &["package-x-generic", "application-x-archive"],
            FileCategory::Pdf => &["application-pdf"],
            FileCategory::Binary => &["application-x-executable"],
            FileCategory::Unknown => &["unknown", "application-octet-stream", "text-x-generic"],
        };
        names.extend(fallback.iter().map(|name| (*name).to_string()));
        names
    }
}

#[cfg(test)]
mod tests {
    use super::{CONTEXTS, SIZES, directories, find_file};

    fn scratch(tag: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("clock").as_nanos();
        let dir = std::env::temp_dir().join(format!("glide-icons-{tag}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch theme");
        dir
    }

    #[test]
    fn icon_lookup_follows_index_theme_and_prefers_sharp_art() {
        let theme = scratch("indexed");
        for dir in ["16x16/apps", "256x256/apps", "apps/48", "apps/48@2x"] {
            std::fs::create_dir_all(theme.join(dir)).expect("theme directory");
        }
        std::fs::write(
            theme.join("index.theme"),
            "[Icon Theme]\nName=Test\nDirectories=16x16/apps,256x256/apps,apps/48\nScaledDirectories=apps/48@2x\n\n\
             [16x16/apps]\nSize=16\n\n[256x256/apps]\nSize=256\n\n[apps/48]\nSize=48\n\n[apps/48@2x]\nSize=48\nScale=2\n",
        )
        .expect("index");
        let dirs = directories(&theme);
        assert_eq!(dirs.len(), 4);
        assert!(dirs.contains(&(theme.join("apps/48@2x"), 96)), "scaled directories count physical pixels");
        for dir in ["16x16/apps", "256x256/apps"] {
            std::fs::write(theme.join(dir).join("viewer.png"), b"png").expect("raster");
        }
        let large = theme.join("256x256/apps/viewer.png");
        assert_eq!(find_file(&dirs, "viewer"), Some(large), "large art is not upscaled");
        std::fs::write(theme.join("apps/48/breeze.png"), b"png").expect("nested layout");
        assert_eq!(find_file(&dirs, "breeze"), Some(theme.join("apps/48/breeze.png")), "declared layouts are searched");
        std::fs::write(theme.join("16x16/apps/viewer.svg"), b"<svg/>").expect("vector");
        assert_eq!(find_file(&dirs, "viewer"), Some(theme.join("16x16/apps/viewer.svg")), "vector art wins");
        assert_eq!(find_file(&dirs, "missing"), None);
        let _ = std::fs::remove_dir_all(theme);
    }

    #[test]
    fn themes_without_an_index_use_the_common_layout() {
        let theme = scratch("plain");
        let dirs = directories(&theme);
        assert_eq!(dirs.len(), SIZES.len() * CONTEXTS.len());
        assert!(dirs.contains(&(theme.join("128x128/places"), 128)));
        assert!(dirs.contains(&(theme.join("scalable/apps"), 0)));
        let _ = std::fs::remove_dir_all(theme);
    }
}

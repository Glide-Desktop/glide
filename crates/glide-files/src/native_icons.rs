use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use glide_theme::Accent;
use iced::widget::{image, svg};

use crate::fs::{FileCategory, FileEntry};
use crate::icons::Icon;

#[derive(Debug, Clone)]
pub enum Art {
    Vector(svg::Handle),
    Raster(image::Handle),
}

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

pub struct IconTheme {
    themes: Vec<PathBuf>,
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
        for theme in &self.themes {
            for size in SIZES {
                for context in CONTEXTS {
                    let dir = theme.join(size).join(context);
                    let vector = dir.join(format!("{name}.svg"));
                    if vector.is_file() {
                        return Some(Art::Vector(svg::Handle::from_path(vector)));
                    }
                    let raster = dir.join(format!("{name}.png"));
                    if raster.is_file() {
                        return Some(Art::Raster(image::Handle::from_path(raster)));
                    }
                }
            }
        }
        None
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

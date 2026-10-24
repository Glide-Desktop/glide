use std::path::{Path, PathBuf};

use super::gsettings::{self, Snapshot};

pub const INTERFACE: &str = "org.gnome.desktop.interface";
pub(crate) const DEFAULT_ICON_THEME: &str = "Papirus";

pub(crate) const PREVIEW_ICONS: &[&[&str]] = &[
    &["folder", "inode-directory"],
    &["user-home", "folder-home"],
    &["utilities-terminal", "org.gnome.Terminal", "terminal", "Alacritty"],
    &["web-browser", "firefox", "applications-internet"],
    &["preferences-system", "org.gnome.Settings", "systemsettings"],
    &["text-x-generic", "text-plain"],
    &["image-x-generic", "image"],
    &["user-trash", "user-trash-full"],
];

const SIZES: &[&str] = &["scalable", "256x256", "128x128", "96x96", "64x64", "48x48", "256", "128", "96", "64", "48"];
const CONTEXTS: &[&str] = &["places", "apps", "mimetypes", "categories", "devices"];

#[must_use]
pub(crate) fn parse_inherits(index: &str) -> Vec<String> {
    index
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Inherits="))
        .flat_map(|list| list.split(','))
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

fn theme_chain(name: &str, roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut chain: Vec<PathBuf> = Vec::new();
    let mut queue = vec![name.to_string()];
    let mut seen: Vec<String> = Vec::new();
    while let Some(theme) = queue.pop() {
        if seen.contains(&theme) || seen.len() > 6 {
            continue;
        }
        seen.push(theme.clone());
        for root in roots {
            let dir = root.join(&theme);
            if !dir.is_dir() {
                continue;
            }
            if let Ok(index) = std::fs::read_to_string(dir.join("index.theme")) {
                for parent in parse_inherits(&index).into_iter().rev() {
                    queue.insert(0, parent);
                }
            }
            chain.push(dir);
        }
    }
    for root in roots {
        let dir = root.join("hicolor");
        if dir.is_dir() && !chain.contains(&dir) {
            chain.push(dir);
        }
    }
    chain
}

fn find_in(dir: &Path, name: &str) -> Option<PathBuf> {
    for size in SIZES {
        for context in CONTEXTS {
            for layout in [dir.join(size).join(context), dir.join(context).join(size)] {
                for ext in ["svg", "png"] {
                    let path = layout.join(format!("{name}.{ext}"));
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

#[must_use]
pub(crate) fn icon_preview(theme: Option<&str>) -> Vec<PathBuf> {
    let roots = super::apps::theme_dirs();
    let mut chain = theme_chain(theme.unwrap_or(DEFAULT_ICON_THEME), &roots);
    for fallback in ["Adwaita", "breeze"] {
        chain.extend(theme_chain(fallback, &roots));
    }
    PREVIEW_ICONS
        .iter()
        .filter_map(|names| chain.iter().find_map(|dir| names.iter().find_map(|name| find_in(dir, name))))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Smoothing {
    Grayscale,
    Subpixel,
    None,
}

impl Smoothing {
    pub const ALL: &'static [Self] = &[Self::Grayscale, Self::Subpixel, Self::None];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Grayscale => "grayscale",
            Self::Subpixel => "rgba",
            Self::None => "none",
        }
    }

    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|mode| mode.key() == key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hinting {
    None,
    Slight,
    Medium,
    Full,
}

impl Hinting {
    pub const ALL: &'static [Self] = &[Self::None, Self::Slight, Self::Medium, Self::Full];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Slight => "slight",
            Self::Medium => "medium",
            Self::Full => "full",
        }
    }

    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|mode| mode.key() == key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontRendering {
    pub smoothing: Smoothing,
    pub hinting: Hinting,
}

#[must_use]
pub(crate) fn font_rendering_from(snapshot: &Snapshot) -> Option<FontRendering> {
    let smoothing = snapshot.string(INTERFACE, "font-antialiasing").and_then(|key| Smoothing::from_key(&key))?;
    let hinting = snapshot.string(INTERFACE, "font-hinting").and_then(|key| Hinting::from_key(&key))?;
    Some(FontRendering { smoothing, hinting })
}

#[must_use]
pub(crate) fn font_rendering() -> Option<FontRendering> {
    font_rendering_from(&Snapshot::read(&[INTERFACE]))
}

pub(crate) fn set_smoothing(mode: Smoothing) -> Result<String, String> {
    gsettings::set_string(INTERFACE, "font-antialiasing", mode.key())
}

pub(crate) fn set_hinting(mode: Hinting) -> Result<String, String> {
    gsettings::set_string(INTERFACE, "font-hinting", mode.key())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherited_themes_are_read_in_order() {
        let index = "[Icon Theme]\nName=Papirus-Dark\nInherits=Papirus, breeze-dark,hicolor\n";
        assert_eq!(parse_inherits(index), ["Papirus", "breeze-dark", "hicolor"]);
        assert!(parse_inherits("[Icon Theme]\nName=Lonely\n").is_empty());
    }

    #[test]
    fn font_rendering_is_read_from_gsettings_output() {
        let snapshot = Snapshot::parse(
            "org.gnome.desktop.interface font-antialiasing 'rgba'\norg.gnome.desktop.interface font-hinting 'slight'\n",
        );
        assert_eq!(
            font_rendering_from(&snapshot),
            Some(FontRendering { smoothing: Smoothing::Subpixel, hinting: Hinting::Slight })
        );
        assert_eq!(font_rendering_from(&Snapshot::default()), None);
    }

    #[test]
    fn a_theme_directory_is_searched_by_size_and_context() {
        let dir = std::env::temp_dir().join(format!("glide-icons-{}", std::process::id()));
        let places = dir.join("48x48/places");
        std::fs::create_dir_all(&places).expect("writable");
        std::fs::write(places.join("folder.svg"), "<svg/>").expect("writable");
        assert_eq!(find_in(&dir, "folder"), Some(places.join("folder.svg")));
        assert_eq!(find_in(&dir, "user-trash"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

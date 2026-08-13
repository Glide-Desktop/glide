//! Finding an app's icon on disk.
//!
//! Two lookups chained together, both defined by freedesktop specs:
//!
//! 1. `app_id` → a `.desktop` entry → its `Icon=` key.
//! 2. That icon name → an actual file, by walking the configured icon
//!    theme, the themes it inherits, and finally `hicolor` and the legacy
//!    pixmap directory.
//!
//! Everything here touches the filesystem, so results are cached: a dock
//! tile is rasterised once and then reused until the dock is rebuilt.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Theme searched first. Its `index.theme` names what it inherits, and
/// `hicolor` is always the last resort per the icon theme spec.
const DEFAULT_THEME: &str = "Papirus";

/// Fallback chain used when a theme's `index.theme` cannot be read.
const FALLBACK_THEMES: [&str; 1] = ["hicolor"];

/// Resolves and rasterises app icons, remembering what it has already
/// looked up — including the misses, which are the expensive case.
#[derive(Debug, Default)]
pub struct IconLoader {
    theme: Option<String>,
    /// `app_id` (lowercased) → rendered BGRA buffer at the cached size.
    rendered: HashMap<(String, i32), Option<Vec<u8>>>,
}

impl IconLoader {
    #[must_use]
    pub fn new(theme: Option<String>) -> Self {
        Self {
            theme,
            rendered: HashMap::new(),
        }
    }

    fn theme_name(&self) -> &str {
        self.theme.as_deref().unwrap_or(DEFAULT_THEME)
    }

    /// A `side × side` premultiplied BGRA image for `app_id`, or `None`
    /// when nothing suitable was found.
    pub fn load(&mut self, app_id: &str, side: i32) -> Option<&[u8]> {
        let key = (app_id.to_lowercase(), side);
        if !self.rendered.contains_key(&key) {
            let theme = self.theme_name().to_string();
            let rendered = resolve(app_id, &theme).and_then(|path| render(&path, side));
            if rendered.is_none() {
                tracing::debug!(app_id, "no icon found; falling back to a built-in glyph");
            }
            self.rendered.insert(key.clone(), rendered);
        }
        self.rendered.get(&key)?.as_deref()
    }
}

/// Directories that hold icon themes, most specific first.
fn icon_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(&home).join(".icons"));
        roots.push(PathBuf::from(&home).join(".local/share/icons"));
    }
    for dir in data_dirs() {
        roots.push(dir.join("icons"));
    }
    roots.push(PathBuf::from("/usr/share/pixmaps"));
    roots
}

/// `$XDG_DATA_HOME` plus `$XDG_DATA_DIRS`, with the spec's defaults.
fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(home));
    } else if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share"));
    }

    let list = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    dirs.extend(list.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    dirs
}

/// Finds the `Icon=` value for an app id by locating its desktop entry.
fn desktop_icon_name(app_id: &str) -> Option<String> {
    // Clients report ids with varying case and, for some toolkits, without
    // the reverse-DNS prefix the file uses. Try the obvious spellings
    // before falling back to scanning.
    let candidates = [
        app_id.to_string(),
        format!("{app_id}.desktop"),
        format!("{}.desktop", app_id.to_lowercase()),
    ];

    for dir in data_dirs() {
        let apps = dir.join("applications");
        for candidate in &candidates {
            let name = if candidate.ends_with(".desktop") {
                candidate.clone()
            } else {
                format!("{candidate}.desktop")
            };
            let path = apps.join(&name);
            if let Some(icon) = read_icon_key(&path) {
                return Some(icon);
            }
        }

        // Last resort: a case-insensitive scan of the directory. Desktop
        // file names do not have to match the app id at all.
        let Ok(entries) = std::fs::read_dir(&apps) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "desktop") {
                continue;
            }
            let matches = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem.eq_ignore_ascii_case(app_id));
            if matches {
                if let Some(icon) = read_icon_key(&path) {
                    return Some(icon);
                }
            }
        }
    }
    None
}

/// Reads the `Icon=` key out of the `[Desktop Entry]` group.
fn read_icon_key(path: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some(value) = line.strip_prefix("Icon=") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// The themes to search, in order: the configured one, whatever it
/// inherits, then the spec-mandated `hicolor`.
fn theme_chain(theme: &str) -> Vec<String> {
    let mut chain = vec![theme.to_string()];

    for root in icon_roots() {
        let index = root.join(theme).join("index.theme");
        let Ok(contents) = std::fs::read_to_string(&index) else {
            continue;
        };
        for line in contents.lines() {
            if let Some(value) = line.trim().strip_prefix("Inherits=") {
                chain.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToString::to_string),
                );
                break;
            }
        }
        break;
    }

    for fallback in FALLBACK_THEMES {
        if !chain.iter().any(|name| name == fallback) {
            chain.push(fallback.to_string());
        }
    }
    chain
}

/// Locates an icon file for `app_id`.
fn resolve(app_id: &str, theme: &str) -> Option<PathBuf> {
    // The `Icon=` key is authoritative; the app id itself is a common
    // enough icon name to be worth trying too, and some apps ship an icon
    // without a matching desktop entry.
    let mut names = Vec::new();
    if let Some(from_desktop) = desktop_icon_name(app_id) {
        names.push(from_desktop);
    }
    names.push(app_id.to_string());
    names.push(app_id.to_lowercase());

    for name in &names {
        // An absolute path is allowed in `Icon=`.
        let direct = Path::new(name);
        if direct.is_absolute() && direct.is_file() {
            return Some(direct.to_path_buf());
        }
        if let Some(found) = find_in_themes(name, theme) {
            return Some(found);
        }
    }
    None
}

fn find_in_themes(name: &str, theme: &str) -> Option<PathBuf> {
    let roots = icon_roots();
    for theme_name in theme_chain(theme) {
        for root in &roots {
            let theme_dir = root.join(&theme_name);
            if !theme_dir.is_dir() {
                continue;
            }
            if let Some(found) = find_in_theme_dir(&theme_dir, name) {
                return Some(found);
            }
        }
    }

    // Legacy flat directory, and any root that stores icons directly.
    for root in &roots {
        for extension in ["svg", "png"] {
            let path = root.join(format!("{name}.{extension}"));
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

/// Searches one theme directory, preferring the largest available size so
/// the tile is never upscaled from a tiny raster.
fn find_in_theme_dir(theme_dir: &Path, name: &str) -> Option<PathBuf> {
    let Ok(entries) = std::fs::read_dir(theme_dir) else {
        return None;
    };

    let mut best: Option<(u32, PathBuf)> = None;
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        // Directory names are `48x48`, `64x64@2x`, `scalable`, `symbolic`.
        let label = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        // Scalable art wins outright: it renders crisply at any size.
        let rank = if label.starts_with("scalable") {
            u32::MAX
        } else {
            label
                .split(['x', '@'])
                .next()
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(0)
        };

        for category in ["apps", "applications"] {
            for extension in ["svg", "png"] {
                let candidate = dir.join(category).join(format!("{name}.{extension}"));
                if candidate.is_file() && best.as_ref().is_none_or(|(size, _)| rank > *size) {
                    best = Some((rank, candidate));
                }
            }
        }
    }
    best.map(|(_, path)| path)
}

/// Loads and rasterises an icon file to `side × side`.
fn render(path: &Path, side: i32) -> Option<Vec<u8>> {
    let data = std::fs::read(path).ok()?;
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("svg") => glide_paint::render_svg(&data, side),
        Some("png") => glide_paint::render_png(&data, side),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_theme_chain_always_ends_at_hicolor() {
        let chain = theme_chain("Papirus");
        assert_eq!(chain.first().map(String::as_str), Some("Papirus"));
        assert!(
            chain.iter().any(|name| name == "hicolor"),
            "the spec makes hicolor the final fallback: {chain:?}"
        );
    }

    #[test]
    fn an_unknown_theme_still_falls_back() {
        let chain = theme_chain("DefinitelyNotInstalled");
        assert_eq!(chain.len(), 2, "{chain:?}");
        assert!(chain.iter().any(|name| name == "hicolor"));
    }

    #[test]
    fn data_dirs_have_the_spec_defaults() {
        let dirs = data_dirs();
        assert!(
            dirs.iter().any(|dir| dir.ends_with("share")),
            "expected a system share directory in {dirs:?}"
        );
    }

    mod desktop_entries {
        use super::*;

        fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
            std::fs::create_dir_all(dir).expect("create temp dir");
            let path = dir.join(name);
            std::fs::write(&path, body).expect("write desktop entry");
            path
        }

        fn temp(name: &str) -> PathBuf {
            let dir =
                std::env::temp_dir().join(format!("glide-icon-test-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            dir
        }

        #[test]
        fn the_icon_key_is_read_from_the_desktop_entry_group() {
            let dir = temp("icon-key");
            let path = write(
                &dir,
                "app.desktop",
                "[Desktop Entry]\nName=App\nIcon=my-icon\nExec=app\n",
            );
            assert_eq!(read_icon_key(&path).as_deref(), Some("my-icon"));
        }

        /// Other groups carry their own `Icon=` keys for actions; only the
        /// main entry's icon describes the app.
        #[test]
        fn an_icon_in_another_group_is_ignored() {
            let dir = temp("other-group");
            let path = write(
                &dir,
                "app.desktop",
                "[Desktop Action New]\nIcon=wrong\n\n[Desktop Entry]\nIcon=right\n",
            );
            assert_eq!(read_icon_key(&path).as_deref(), Some("right"));
        }

        #[test]
        fn an_entry_without_an_icon_yields_nothing() {
            let dir = temp("no-icon");
            let path = write(&dir, "app.desktop", "[Desktop Entry]\nName=App\n");
            assert_eq!(read_icon_key(&path), None);
        }

        #[test]
        fn an_empty_icon_value_is_not_accepted() {
            let dir = temp("empty-icon");
            let path = write(&dir, "app.desktop", "[Desktop Entry]\nIcon=\n");
            assert_eq!(read_icon_key(&path), None);
        }

        #[test]
        fn a_missing_file_is_not_an_error() {
            assert_eq!(read_icon_key(Path::new("/nonexistent/app.desktop")), None);
        }
    }

    /// Exercised against whatever is really installed. Skips rather than
    /// fails when a machine has no icon theme, so the suite stays useful
    /// in a bare container.
    mod against_the_real_system {
        use super::*;

        #[test]
        fn a_known_app_resolves_to_a_readable_icon_file() {
            let Some(path) = resolve("Alacritty", DEFAULT_THEME) else {
                eprintln!("no icon for Alacritty on this machine; skipping");
                return;
            };
            assert!(path.is_file(), "{path:?} should exist");
            let extension = path.extension().and_then(|e| e.to_str());
            assert!(
                matches!(extension, Some("svg" | "png")),
                "unexpected icon format: {path:?}"
            );
        }

        #[test]
        fn a_resolved_icon_rasterises_to_the_requested_square() {
            let Some(path) = resolve("Alacritty", DEFAULT_THEME) else {
                eprintln!("no icon for Alacritty on this machine; skipping");
                return;
            };
            let Some(pixels) = render(&path, 42) else {
                panic!("{path:?} resolved but would not render");
            };
            assert_eq!(pixels.len(), 42 * 42 * 4);
            assert!(
                pixels.chunks_exact(4).any(|px| px[3] > 0),
                "the rendered icon is entirely transparent"
            );
        }

        #[test]
        fn an_unknown_app_resolves_to_nothing() {
            assert_eq!(
                resolve("definitely.not.an.installed.app.xyz", DEFAULT_THEME),
                None
            );
        }
    }
}

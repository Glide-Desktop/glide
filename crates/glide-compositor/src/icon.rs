use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

const DEFAULT_THEME: &str = "Papirus";

const FALLBACK_THEMES: [&str; 1] = ["hicolor"];

#[derive(Debug, Default)]
pub struct IconLoader {
    theme: Option<String>,
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

    pub fn load_named(&mut self, icon: &str, side: i32) -> Option<&[u8]> {
        let key = (format!("icon:{}", icon.to_lowercase()), side);
        if !self.rendered.contains_key(&key) {
            let theme = self.theme_name().to_string();
            let rendered = find_in_themes(icon, &theme).and_then(|path| render(&path, side));
            self.rendered.insert(key.clone(), rendered);
        }
        self.rendered.get(&key)?.as_deref()
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub command: String,
    pub icon: String,
    pub category: AppCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCategory {
    Utilities,
    Office,
    Internet,
    Media,
    System,
    Other,
}

impl AppCategory {
    fn from_desktop(categories: &str) -> Self {
        for entry in categories.split(';') {
            match entry.trim() {
                "Network" | "WebBrowser" | "Email" => return Self::Internet,
                "Office" | "TextEditor" | "Spreadsheet" | "Presentation" => return Self::Office,
                "AudioVideo" | "Audio" | "Video" | "Graphics" | "Player" => return Self::Media,
                "System" | "Settings" | "Security" => return Self::System,
                "Utility" | "Accessories" | "TerminalEmulator" => return Self::Utilities,
                _ => {}
            }
        }
        Self::Other
    }
}

#[must_use]
pub fn list_applications() -> Vec<DesktopApp> {
    let mut seen: HashMap<String, Option<DesktopApp>> = HashMap::new();

    for dir in data_dirs() {
        let Ok(entries) = std::fs::read_dir(dir.join("applications")) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "desktop") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if seen.contains_key(id) {
                continue;
            }
            seen.insert(id.to_string(), read_desktop_app(&path));
        }
    }

    let mut apps: Vec<DesktopApp> = seen.into_values().flatten().collect();
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

#[must_use]
pub fn find_application(app_id: &str) -> Option<DesktopApp> {
    let app_id = app_id.trim();
    if app_id.is_empty() {
        return None;
    }
    let wanted = app_id.to_lowercase();
    for dir in data_dirs() {
        let apps = dir.join("applications");
        let direct = apps.join(format!("{app_id}.desktop"));
        if let Some(app) = read_desktop_app(&direct) {
            return Some(app);
        }
        let Ok(entries) = std::fs::read_dir(&apps) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let stem_matches = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem.to_lowercase() == wanted);
            if stem_matches && let Some(app) = read_desktop_app(&path) {
                return Some(app);
            }
        }
    }
    list_applications().into_iter().find(|app| {
        app.name.eq_ignore_ascii_case(app_id)
            || app.icon.eq_ignore_ascii_case(app_id)
            || app
                .command
                .split_whitespace()
                .next()
                .and_then(|bin| bin.rsplit('/').next())
                .is_some_and(|bin| bin.eq_ignore_ascii_case(app_id))
    })
}

fn read_desktop_app(path: &Path) -> Option<DesktopApp> {
    let contents = std::fs::read_to_string(path).ok()?;

    let mut in_entry = false;
    let (mut name, mut exec, mut icon, mut categories) = (
        None::<String>,
        None::<String>,
        None::<String>,
        String::new(),
    );
    let (mut no_display, mut hidden, mut is_app) = (false, false, true);

    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            if in_entry {
                break;
            }
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => name = Some(value.trim().to_string()),
            "Exec" => exec = Some(value.trim().to_string()),
            "Icon" => icon = Some(value.trim().to_string()),
            "Categories" => categories = value.trim().to_string(),
            "NoDisplay" => no_display = value.trim().eq_ignore_ascii_case("true"),
            "Hidden" => hidden = value.trim().eq_ignore_ascii_case("true"),
            "Type" => is_app = value.trim() == "Application",
            _ => {}
        }
    }

    if no_display || hidden || !is_app {
        return None;
    }

    let id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_string();
    Some(DesktopApp {
        id,
        name: name?,
        command: strip_field_codes(&exec?),
        icon: icon.unwrap_or_default(),
        category: AppCategory::from_desktop(&categories),
    })
}

fn strip_field_codes(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    let mut chars = exec.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        if chars.next() == Some('%') {
            out.push('%');
        }
    }
    out.trim().to_string()
}

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

fn desktop_icon_name(app_id: &str) -> Option<String> {
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
            if matches && let Some(icon) = read_icon_key(&path) {
                return Some(icon);
            }
        }
    }
    None
}

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

fn resolve(app_id: &str, theme: &str) -> Option<PathBuf> {
    let mut names = Vec::new();
    if let Some(from_desktop) = desktop_icon_name(app_id) {
        names.push(from_desktop);
    }
    names.push(app_id.to_string());
    names.push(app_id.to_lowercase());

    for name in &names {
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
        let label = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
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

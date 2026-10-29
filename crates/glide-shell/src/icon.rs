use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use glide_theme::Color;

const DEFAULT_THEME: &str = glide_theme::icons::FALLBACK;

const FALLBACK_THEME: &str = "hicolor";

const MISSING_THEME_FALLBACK: &str = "Adwaita";

const NEGATIVE_TTL: Duration = Duration::from_secs(30);

const EXTENSIONS: [&str; 2] = ["svg", "png"];

#[derive(Debug, Default)]
pub struct IconLoader {
    theme: Option<String>,
    index: ThemeIndex,
    rendered: HashMap<(String, i32), Option<Vec<u8>>>,
    symbolic_rendered: HashMap<(String, i32, [u8; 4]), Option<Vec<u8>>>,
    refreshed: Option<Instant>,
    generation: u64,
}

impl IconLoader {
    #[must_use]
    pub fn new(theme: Option<String>) -> Self {
        Self { theme, ..Self::default() }
    }

    #[cfg(test)]
    fn with_roots(theme: &str, roots: Vec<PathBuf>) -> Self {
        Self { theme: Some(theme.to_string()), index: ThemeIndex::with_roots(roots), ..Self::default() }
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.wrapping_add(ALIAS_REVISION.load(std::sync::atomic::Ordering::Relaxed))
    }

    pub fn expire_missing(&mut self) -> bool {
        self.expire_missing_at(Instant::now())
    }

    fn expire_missing_at(&mut self, now: Instant) -> bool {
        let since = *self.refreshed.get_or_insert(now);
        if now.saturating_duration_since(since) < NEGATIVE_TTL {
            return false;
        }
        self.refreshed = Some(now);
        let missing =
            self.rendered.values().any(Option::is_none) || self.symbolic_rendered.values().any(Option::is_none);
        if !missing {
            return false;
        }
        self.rendered.retain(|_, pixels| pixels.is_some());
        self.symbolic_rendered.retain(|_, pixels| pixels.is_some());
        self.index.forget_listings();
        self.generation = self.generation.wrapping_add(1);
        true
    }

    fn theme_name(&self) -> String {
        self.theme.as_deref().unwrap_or(DEFAULT_THEME).to_string()
    }

    pub fn load_named(&mut self, icon: &str, side: i32) -> Option<&[u8]> {
        self.expire_missing();
        let theme = self.theme_name();
        let index = &mut self.index;
        self.rendered
            .entry((format!("icon:{icon}"), side))
            .or_insert_with(|| index.resolve_named(icon, &theme, side).and_then(|path| render(&path, side, None)))
            .as_deref()
    }

    pub fn load_symbolic(&mut self, icon: &str, side: i32, tint: Color) -> Option<&[u8]> {
        self.expire_missing();
        let theme = self.theme_name();
        let index = &mut self.index;
        self.symbolic_rendered
            .entry((icon.to_string(), side, tint.to_buffer_bytes()))
            .or_insert_with(|| {
                index.resolve_symbolic(icon, &theme, side).and_then(|path| render(&path, side, Some(tint)))
            })
            .as_deref()
    }

    pub fn load(&mut self, app_id: &str, side: i32) -> Option<&[u8]> {
        self.expire_missing();
        let theme = self.theme_name();
        let index = &mut self.index;
        let key = (app_id.to_string(), side);
        let found = self
            .rendered
            .entry(key.clone())
            .or_insert_with(|| {
                let rendered = index.resolve_app(app_id, &theme, side).and_then(|path| render(&path, side, None));
                if rendered.is_none() {
                    tracing::debug!(app_id, "no icon found; falling back to a built-in glyph");
                }
                rendered
            })
            .is_some();
        if !found && let Some(icon) = alias_for(app_id) {
            return self.load_named(&icon, side);
        }
        self.rendered.get(&key).and_then(|pixels| pixels.as_deref())
    }
}

static ALIASES: std::sync::Mutex<Option<HashMap<String, String>>> = std::sync::Mutex::new(None);
static ALIAS_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn set_alias(key: &str, icon: Option<&str>) {
    let mut aliases = ALIASES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let map = aliases.get_or_insert_with(HashMap::new);
    let changed = match icon.filter(|icon| !icon.trim().is_empty() && !icon.contains('/')) {
        Some(icon) => map.insert(key.to_string(), icon.to_string()).as_deref() != Some(icon),
        None => map.remove(key).is_some(),
    };
    if changed {
        ALIAS_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

fn alias_for(key: &str) -> Option<String> {
    ALIASES.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref()?.get(key).cloned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ThemeDir {
    subdir: String,
    min: u32,
    max: u32,
}

impl ThemeDir {
    fn distance(&self, px: u32) -> u32 {
        if px < self.min {
            self.min - px
        } else if px > self.max {
            (px - self.max).saturating_mul(2)
        } else {
            0
        }
    }
}

#[derive(Debug, Default, Clone)]
struct Theme {
    bases: Vec<PathBuf>,
    dirs: Vec<ThemeDir>,
    parents: Vec<String>,
}

#[derive(Debug, Default)]
struct ThemeIndex {
    roots: Option<Vec<PathBuf>>,
    themes: HashMap<String, Option<Theme>>,
    listings: HashMap<PathBuf, HashSet<String>>,
}

impl ThemeIndex {
    #[cfg(test)]
    fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self { roots: Some(roots), ..Self::default() }
    }

    fn roots(&mut self) -> Vec<PathBuf> {
        self.roots.get_or_insert_with(icon_roots).clone()
    }

    fn theme(&mut self, name: &str) -> Option<Theme> {
        if !self.themes.contains_key(name) {
            let roots = self.roots();
            let loaded = load_theme(name, &roots);
            self.themes.insert(name.to_string(), loaded);
        }
        self.themes.get(name)?.clone()
    }

    fn chain(&mut self, theme: &str) -> Vec<String> {
        let mut heads = vec![theme.to_string()];
        if self.theme(theme).is_none() {
            heads.push(MISSING_THEME_FALLBACK.to_string());
        }
        let mut chain = Vec::new();
        for head in heads {
            let mut pending = vec![head];
            while let Some(name) = pending.pop() {
                if name == FALLBACK_THEME || chain.contains(&name) {
                    continue;
                }
                let parents = self.theme(&name).map(|t| t.parents).unwrap_or_default();
                chain.push(name);
                pending.extend(parents.into_iter().rev());
            }
        }
        chain.push(FALLBACK_THEME.to_string());
        chain
    }

    fn forget_listings(&mut self) {
        self.listings.clear();
        self.themes.retain(|_, theme| theme.is_some());
    }

    fn contains(&mut self, dir: &Path, file: &str) -> bool {
        self.listings
            .entry(dir.to_path_buf())
            .or_insert_with(|| {
                std::fs::read_dir(dir)
                    .map(|entries| entries.flatten().filter_map(|entry| entry.file_name().into_string().ok()).collect())
                    .unwrap_or_default()
            })
            .contains(file)
    }

    fn file_in(&mut self, dir: &Path, name: &str) -> Option<PathBuf> {
        EXTENSIONS
            .iter()
            .map(|extension| format!("{name}.{extension}"))
            .find(|file| self.contains(dir, file))
            .map(|file| dir.join(file))
    }

    fn lookup_in_theme(&mut self, theme_name: &str, name: &str, px: u32) -> Option<PathBuf> {
        let theme = self.theme(theme_name)?;
        let mut dirs: Vec<&ThemeDir> = theme.dirs.iter().collect();
        dirs.sort_by_key(|dir| (dir.distance(px), std::cmp::Reverse(dir.max)));
        dirs.iter()
            .flat_map(|dir| theme.bases.iter().map(|base| base.join(&dir.subdir)))
            .find_map(|path| self.file_in(&path, name))
    }

    fn find(&mut self, names: &[String], theme: &str, side: i32) -> Option<PathBuf> {
        let px = u32::try_from(side.max(1)).unwrap_or(1);
        let chain = self.chain(theme);
        let roots = self.roots();
        names.iter().find_map(|name| {
            direct_path(name)
                .or_else(|| chain.iter().find_map(|theme_name| self.lookup_in_theme(theme_name, name, px)))
                .or_else(|| roots.iter().find_map(|root| self.file_in(root, name)))
        })
    }

    fn resolve_named(&mut self, icon: &str, theme: &str, side: i32) -> Option<PathBuf> {
        self.find(&named_candidates(icon), theme, side)
    }

    fn resolve_symbolic(&mut self, icon: &str, theme: &str, side: i32) -> Option<PathBuf> {
        self.find(&symbolic_candidates(icon), theme, side)
    }

    fn resolve_app(&mut self, app_id: &str, theme: &str, side: i32) -> Option<PathBuf> {
        let mut primaries = Vec::new();
        if let Some(from_desktop) = desktop_icon_name(app_id) {
            primaries.push(normalize_icon_name(&from_desktop));
        }
        primaries.push(normalize_icon_name(app_id));
        primaries.push(normalize_icon_name(&app_id.to_lowercase()));
        self.find(&with_variants(&primaries), theme, side)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub command: String,
    pub exec: String,
    pub icon: String,
    pub category: AppCategory,
    pub generic_name: String,
    pub comment: String,
    pub keywords: Vec<String>,
    pub actions: Vec<DesktopAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DesktopAction {
    pub id: String,
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppCategory {
    Utilities,
    Office,
    Internet,
    Media,
    System,
    #[default]
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
            seen.entry(id.to_string()).or_insert_with(|| read_desktop_app(&path));
        }
    }

    let mut apps: Vec<DesktopApp> = seen.into_values().flatten().collect();
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

#[must_use]
pub fn find_application(app_id: &str) -> Option<DesktopApp> {
    let app_id = app_id.trim();
    if !is_plain_app_id(app_id) {
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
            let stem_matches =
                path.file_stem().and_then(|stem| stem.to_str()).is_some_and(|stem| stem.to_lowercase() == wanted);
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
    let id = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default();
    parse_desktop_entry(id, &contents, ui_language_code())
}

fn ui_language_code() -> &'static str {
    crate::i18n::language().key()
}

fn locale_rank(locale: Option<&str>, lang: &str) -> u8 {
    let Some(locale) = locale else {
        return 2;
    };
    let base = locale.split(['_', '@', '.']).next().unwrap_or_default().to_ascii_lowercase();
    if locale.eq_ignore_ascii_case(lang) {
        4
    } else if base != lang {
        0
    } else if lang == "en" {
        1
    } else {
        3
    }
}

#[derive(Debug, Default)]
struct DesktopGroup {
    plain: HashMap<String, String>,
    localized: HashMap<String, (u8, String)>,
}

impl DesktopGroup {
    fn insert(&mut self, key: &str, value: &str, lang: &str) {
        let (field, locale) = match key.split_once('[') {
            Some((field, rest)) => (field.trim(), rest.strip_suffix(']')),
            None => (key.trim(), None),
        };
        if locale.is_none() {
            self.plain.entry(field.to_string()).or_insert_with(|| value.to_string());
        }
        let rank = locale_rank(locale, lang);
        if rank == 0 {
            return;
        }
        let slot = self.localized.entry(field.to_string()).or_insert((0, String::new()));
        if rank > slot.0 {
            *slot = (rank, value.to_string());
        }
    }

    fn plain(&self, key: &str) -> Option<&str> {
        self.plain.get(key).map(String::as_str)
    }

    fn localized(&self, key: &str) -> Option<&str> {
        self.localized.get(key).map(|(_, value)| value.as_str()).or_else(|| self.plain(key))
    }

    fn flag(&self, key: &str) -> bool {
        self.plain(key).is_some_and(|value| value.eq_ignore_ascii_case("true"))
    }
}

fn unescape_desktop_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('s' | 'n' | 't' | 'r') => out.push(' '),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out.trim().to_string()
}

fn desktop_list(value: Option<&str>) -> Vec<String> {
    value.unwrap_or_default().split(';').map(unescape_desktop_value).filter(|item| !item.is_empty()).collect()
}

fn parse_desktop_groups(contents: &str, lang: &str) -> HashMap<String, DesktopGroup> {
    let mut groups: HashMap<String, DesktopGroup> = HashMap::new();
    let mut current: Option<String> = None;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(group) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = (!groups.contains_key(group)).then(|| group.to_string());
            if let Some(name) = current.as_ref() {
                groups.insert(name.clone(), DesktopGroup::default());
            }
            continue;
        }
        let (Some(group), Some((key, value))) = (current.as_ref(), line.split_once('=')) else {
            continue;
        };
        if let Some(entry) = groups.get_mut(group) {
            entry.insert(key.trim(), value.trim(), lang);
        }
    }
    groups
}

fn parse_desktop_entry(id: &str, contents: &str, lang: &str) -> Option<DesktopApp> {
    parse_desktop_entry_for(id, contents, lang, &glide_config::terminal::current)
}

fn parse_desktop_entry_for(id: &str, contents: &str, lang: &str, terminal: &dyn Fn() -> String) -> Option<DesktopApp> {
    let groups = parse_desktop_groups(contents, lang);
    let entry = groups.get("Desktop Entry")?;

    let is_app = entry.plain("Type").is_none_or(|kind| kind == "Application");
    if entry.flag("NoDisplay") || entry.flag("Hidden") || !is_app {
        return None;
    }

    let terminal = entry.flag("Terminal").then(terminal);
    let runnable = |exec: &str| {
        let command = strip_field_codes(exec);
        match terminal.as_deref() {
            Some(terminal) if !command.is_empty() => glide_config::terminal::wrap(terminal, &command),
            _ => command,
        }
    };

    let name = unescape_desktop_value(entry.localized("Name")?);
    let command = runnable(entry.plain("Exec")?);
    let exec = if terminal.is_some() { String::new() } else { entry.plain("Exec").unwrap_or_default().to_string() };

    let mut keywords = desktop_list(entry.localized("Keywords"));
    for extra in desktop_list(entry.plain("Keywords")) {
        if !keywords.contains(&extra) {
            keywords.push(extra);
        }
    }
    if let Some(plain_name) = entry.plain("Name").map(unescape_desktop_value)
        && plain_name != name
        && !keywords.contains(&plain_name)
    {
        keywords.push(plain_name);
    }

    let actions = desktop_list(entry.plain("Actions"))
        .into_iter()
        .filter_map(|action_id| {
            let group = groups.get(&format!("Desktop Action {action_id}"))?;
            let name = unescape_desktop_value(group.localized("Name")?);
            let command = runnable(group.plain("Exec")?);
            (!name.is_empty() && !command.is_empty()).then_some(DesktopAction { id: action_id, name, command })
        })
        .collect();

    Some(DesktopApp {
        id: id.to_string(),
        name,
        command,
        exec,
        icon: entry.plain("Icon").unwrap_or_default().to_string(),
        category: AppCategory::from_desktop(entry.plain("Categories").unwrap_or_default()),
        generic_name: entry.localized("GenericName").map(unescape_desktop_value).unwrap_or_default(),
        comment: entry.localized("Comment").map(unescape_desktop_value).unwrap_or_default(),
        keywords,
        actions,
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

#[must_use]
pub fn icon_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(&home).join(".icons"));
    }
    for dir in data_dirs() {
        roots.push(dir.join("icons"));
    }
    roots.push(PathBuf::from("/usr/share/pixmaps"));
    let mut seen = HashSet::new();
    roots.retain(|root| seen.insert(root.clone()));
    roots
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = glide_config::data_home().into_iter().collect();
    if let Ok(list) = std::env::var("XDG_DATA_DIRS") {
        dirs.extend(list.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    } else {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        dirs.extend(default_system_data_dirs(home.as_deref()).into_iter().filter(|dir| dir.is_dir()));
    }
    dirs
}

fn default_system_data_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = home.map(|home| home.join(".local/share/flatpak/exports/share")).into_iter().collect();
    dirs.extend(
        ["/var/lib/flatpak/exports/share", "/usr/local/share", "/usr/share", "/var/lib/snapd/desktop"]
            .into_iter()
            .map(PathBuf::from),
    );
    dirs
}

fn is_plain_app_id(app_id: &str) -> bool {
    !app_id.is_empty() && !app_id.starts_with('.') && !app_id.contains(['/', '\0'])
}

fn desktop_icon_name(app_id: &str) -> Option<String> {
    if !is_plain_app_id(app_id) {
        return None;
    }
    let exact = if app_id.ends_with(".desktop") { app_id.to_string() } else { format!("{app_id}.desktop") };
    let candidates = [exact, format!("{app_id}.desktop"), format!("{}.desktop", app_id.to_lowercase())];

    for dir in data_dirs() {
        let apps = dir.join("applications");
        if let Some(icon) = candidates.iter().find_map(|name| read_icon_key(&apps.join(name))) {
            return Some(icon);
        }

        let Ok(entries) = std::fs::read_dir(&apps) else {
            continue;
        };
        let mut by_wm_class = None;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "desktop") {
                continue;
            }
            let matches =
                path.file_stem().and_then(|stem| stem.to_str()).is_some_and(|stem| stem.eq_ignore_ascii_case(app_id));
            if matches && let Some(icon) = read_icon_key(&path) {
                return Some(icon);
            }
            if by_wm_class.is_none()
                && read_desktop_key(&path, "StartupWMClass").is_some_and(|class| class.eq_ignore_ascii_case(app_id))
            {
                by_wm_class = read_icon_key(&path);
            }
        }
        if by_wm_class.is_some() {
            return by_wm_class;
        }
    }
    None
}

fn read_icon_key(path: &Path) -> Option<String> {
    read_desktop_key(path, "Icon")
}

fn read_desktop_key(path: &Path, key: &str) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    glide_config::desktop::entry(&contents)
        .find(|&(name, value)| name == key && !value.is_empty())
        .map(|(_, value)| value.to_string())
}

fn load_theme(name: &str, roots: &[PathBuf]) -> Option<Theme> {
    let bases: Vec<PathBuf> = roots.iter().map(|root| root.join(name)).filter(|dir| dir.is_dir()).collect();
    let contents = bases.iter().find_map(|base| std::fs::read_to_string(base.join("index.theme")).ok())?;
    let (dirs, parents) = parse_index_theme(&contents);
    Some(Theme { bases, dirs, parents })
}

fn parse_index_theme(contents: &str) -> (Vec<ThemeDir>, Vec<String>) {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current = String::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = section.to_string();
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            sections
                .entry(current.clone())
                .or_default()
                .entry(key.trim().to_string())
                .or_insert_with(|| value.trim().to_string());
        }
    }

    let list = |value: Option<&String>| -> Vec<String> {
        value
            .map(|v| v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(ToString::to_string).collect())
            .unwrap_or_default()
    };

    let header = sections.get("Icon Theme").cloned().unwrap_or_default();
    let parents = list(header.get("Inherits"));
    let mut names = list(header.get("Directories"));
    names.extend(list(header.get("ScaledDirectories")));

    let mut seen = HashSet::new();
    let dirs = names
        .into_iter()
        .filter(|name| seen.insert(name.clone()))
        .filter_map(|subdir| {
            let keys = sections.get(&subdir)?;
            let number = |key: &str| keys.get(key).and_then(|v| v.parse::<u32>().ok());
            let size = number("Size")?;
            let scale = number("Scale").unwrap_or(1).max(1);
            let (min, max) = match keys.get("Type").map(String::as_str) {
                Some("Fixed") => (size, size),
                Some("Scalable") => (number("MinSize").unwrap_or(size), number("MaxSize").unwrap_or(size)),
                _ => {
                    let threshold = number("Threshold").unwrap_or(2);
                    (size.saturating_sub(threshold), size + threshold)
                }
            };
            Some(ThemeDir { subdir, min: min.saturating_mul(scale), max: max.saturating_mul(scale) })
        })
        .collect();
    (dirs, parents)
}

fn direct_path(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    (path.is_absolute() && path.is_file()).then(|| path.to_path_buf())
}

fn normalize_icon_name(icon: &str) -> String {
    let icon = icon.trim();
    let icon = icon.strip_prefix("file://").unwrap_or(icon);
    if Path::new(icon).is_absolute() {
        return icon.to_string();
    }
    for extension in [".svg", ".png", ".xpm"] {
        if let Some(stem) = icon.strip_suffix(extension) {
            return stem.to_string();
        }
    }
    icon.to_string()
}

fn with_variants(primaries: &[String]) -> Vec<String> {
    let mut names = primaries.to_vec();
    for primary in primaries.iter().filter(|p| !Path::new(p).is_absolute()) {
        for suffix in ["-symbolic", "-panel"] {
            names.push(primary.strip_suffix(suffix).map_or_else(|| format!("{primary}{suffix}"), str::to_string));
        }
    }
    let mut seen = HashSet::new();
    names.retain(|name| !name.is_empty() && seen.insert(name.clone()));
    names
}

fn named_candidates(icon: &str) -> Vec<String> {
    with_variants(&[normalize_icon_name(icon)])
}

fn symbolic_candidates(icon: &str) -> Vec<String> {
    let icon = normalize_icon_name(icon);
    let base = icon.strip_suffix("-symbolic").unwrap_or(&icon).to_string();
    vec![format!("{base}-symbolic"), base]
}

const MAX_ICON_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[must_use]
pub fn render(path: &Path, side: i32, tint: Option<Color>) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_ICON_FILE_BYTES {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    match (path.extension().and_then(|ext| ext.to_str()), tint) {
        (Some("svg"), None) => glide_paint::render_svg(&data, side),
        (Some("png"), None) => glide_paint::render_png(&data, side),
        (Some("svg"), Some(tint)) => glide_paint::render_svg_tinted(&data, side, tint),
        (Some("png"), Some(tint)) => glide_paint::render_png_tinted(&data, side, tint),
        _ => None,
    }
}

pub const LAUNCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[must_use]
pub fn fallback_initial(name: &str) -> String {
    name.chars().find(|c| c.is_alphanumeric()).map_or_else(|| "?".to_string(), |c| c.to_uppercase().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_theme_chain_always_ends_at_hicolor() {
        let chain = ThemeIndex::default().chain("Papirus");
        assert_eq!(chain.first().map(String::as_str), Some("Papirus"));
        assert_eq!(
            chain.last().map(String::as_str),
            Some("hicolor"),
            "the spec makes hicolor the final fallback: {chain:?}"
        );
    }

    #[test]
    fn an_unknown_theme_falls_back_through_adwaita_to_hicolor() {
        let chain = ThemeIndex::with_roots(Vec::new()).chain("DefinitelyNotInstalled");
        assert_eq!(chain, ["DefinitelyNotInstalled", "Adwaita", "hicolor"]);
    }

    #[test]
    fn default_data_dirs_include_flatpak_and_snap_exports() {
        let dirs = default_system_data_dirs(Some(Path::new("/home/u")));
        for expected in [
            "/home/u/.local/share/flatpak/exports/share",
            "/var/lib/flatpak/exports/share",
            "/usr/share",
            "/var/lib/snapd/desktop",
        ] {
            assert!(dirs.contains(&PathBuf::from(expected)), "{expected}");
        }
    }

    #[test]
    fn a_missing_icon_is_retried_after_the_negative_ttl() {
        let root = std::env::temp_dir().join(format!("glide-icon-ttl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let mut loader = IconLoader::with_roots("None", vec![root.clone()]);
        let start = Instant::now();
        assert!(!loader.expire_missing_at(start));
        assert!(loader.load_named("late", 16).is_none());
        assert!(loader.index.resolve_named("late", "None", 16).is_none());
        std::fs::write(root.join("late.png"), b"x").expect("write");
        assert!(
            loader.index.resolve_named("late", "None", 16).is_none(),
            "the directory listing is cached until expiry"
        );
        let generation = loader.generation();
        assert!(!loader.expire_missing_at(start + NEGATIVE_TTL / 2));
        assert!(loader.expire_missing_at(start + NEGATIVE_TTL));
        assert_ne!(loader.generation(), generation);
        assert!(!loader.rendered.values().any(Option::is_none));
        assert_eq!(loader.index.resolve_named("late", "None", 16), Some(root.join("late.png")));
        assert!(!loader.expire_missing_at(start + NEGATIVE_TTL * 3));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn file_uris_and_extensions_are_normalised() {
        assert_eq!(normalize_icon_name("file:///opt/a/b.png"), "/opt/a/b.png");
        assert_eq!(normalize_icon_name("steam.png"), "steam");
        assert_eq!(normalize_icon_name(" firefox "), "firefox");
    }

    #[test]
    fn symbolic_lookups_prefer_the_symbolic_file() {
        assert_eq!(symbolic_candidates("audio-volume-high"), ["audio-volume-high-symbolic", "audio-volume-high"]);
        assert_eq!(
            symbolic_candidates("audio-volume-high-symbolic"),
            ["audio-volume-high-symbolic", "audio-volume-high"]
        );
    }

    #[test]
    fn app_lookups_try_every_real_name_before_any_variant() {
        let names = with_variants(&["a".into(), "b".into()]);
        let first_variant = names.iter().position(|n| n.contains('-')).unwrap_or(0);
        assert_eq!(&names[..first_variant], ["a", "b"]);
    }

    #[test]
    fn threshold_is_the_default_directory_type() {
        let (dirs, parents) = parse_index_theme(
            "[Icon Theme]\nInherits=a, b\nDirectories=48x48/apps,scalable/apps,16x16@2/apps\n\
             [48x48/apps]\nSize=48\n\
             [scalable/apps]\nSize=128\nMinSize=8\nMaxSize=512\nType=Scalable\n\
             [16x16@2/apps]\nSize=16\nScale=2\nType=Fixed\n",
        );
        assert_eq!(parents, ["a", "b"]);
        let ranges: Vec<(u32, u32)> = dirs.iter().map(|d| (d.min, d.max)).collect();
        assert_eq!(ranges, [(46, 50), (8, 512), (32, 32)]);
    }

    mod theme_tree {
        use super::*;

        fn tree(name: &str) -> PathBuf {
            let root = std::env::temp_dir().join(format!("glide-theme-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            root
        }

        fn touch(path: PathBuf) {
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(path, b"x").expect("write");
        }

        fn theme(root: &Path) {
            let dir = root.join("T");
            std::fs::create_dir_all(&dir).expect("mkdir");
            std::fs::write(
                dir.join("index.theme"),
                "[Icon Theme]\nInherits=P\nDirectories=16x16/apps,48x48/apps,128x128/apps,16x16/symbolic/status,48x48/status\n\
                 [16x16/apps]\nSize=16\nType=Fixed\n\
                 [48x48/apps]\nSize=48\nType=Fixed\n\
                 [128x128/apps]\nSize=128\nType=Fixed\n\
                 [16x16/symbolic/status]\nSize=16\nType=Fixed\n\
                 [48x48/status]\nSize=48\nType=Fixed\n",
            )
            .expect("index");
            for size in ["16x16", "48x48", "128x128"] {
                touch(dir.join(size).join("apps/app.png"));
            }
            touch(dir.join("48x48/status/volume.svg"));
            touch(dir.join("16x16/symbolic/status/volume-symbolic.svg"));
            touch(dir.join("48x48/apps/only-panel-panel.svg"));

            let parent = root.join("P");
            std::fs::create_dir_all(&parent).expect("mkdir");
            std::fs::write(
                parent.join("index.theme"),
                "[Icon Theme]\nDirectories=32x32/apps\n[32x32/apps]\nSize=32\nType=Fixed\n",
            )
            .expect("index");
            touch(parent.join("32x32/apps/only-panel.png"));
        }

        #[test]
        fn lookups_pick_the_closest_size_the_symbolic_file_and_real_inherited_icons() {
            let root = tree("resolve");
            theme(&root);
            let mut index = ThemeIndex::with_roots(vec![root.clone()]);
            for (side, dir) in [(16, "16x16"), (40, "48x48"), (96, "128x128")] {
                let want = root.join(format!("T/{dir}/apps/app.png"));
                assert_eq!(index.resolve_named("app", "T", side), Some(want), "{side}");
            }
            assert_eq!(
                index.resolve_symbolic("volume", "T", 16),
                Some(root.join("T/16x16/symbolic/status/volume-symbolic.svg"))
            );
            assert_eq!(
                index.resolve_named("only-panel", "T", 48),
                Some(root.join("P/32x32/apps/only-panel.png")),
                "an inherited real icon beats a panel variant"
            );
        }

        #[test]
        fn a_loader_serves_the_requested_square() {
            let root = tree("loader");
            let dir = root.join("T/scalable/apps");
            std::fs::create_dir_all(&dir).expect("mkdir");
            std::fs::write(
                root.join("T/index.theme"),
                "[Icon Theme]\nDirectories=scalable/apps\n[scalable/apps]\nSize=48\nMinSize=8\nMaxSize=512\nType=Scalable\n",
            )
            .expect("index");
            std::fs::write(
                dir.join("dot.svg"),
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"#,
            )
            .expect("svg");
            std::fs::write(
                dir.join("blank.svg"),
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"></svg>"#,
            )
            .expect("svg");
            let mut loader = IconLoader::with_roots("T", vec![root]);
            assert_eq!(loader.load_named("dot", 32).map(<[u8]>::len), Some(32 * 32 * 4));
            assert_eq!(
                loader.load_named("blank", 32),
                None,
                "an empty raster must fall through to the built-in fallback"
            );
        }
    }

    #[test]
    fn data_dirs_have_the_spec_defaults() {
        let dirs = data_dirs();
        assert!(dirs.iter().any(|dir| dir.ends_with("share")), "expected a system share directory in {dirs:?}");
    }

    #[test]
    fn the_icon_key_comes_only_from_a_non_empty_desktop_entry_group() {
        let dir = std::env::temp_dir().join(format!("glide-icon-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        for (body, icon) in [
            ("[Desktop Entry]\nName=App\nIcon=my-icon\nExec=app\n", Some("my-icon")),
            ("[Desktop Action New]\nIcon=wrong\n\n[Desktop Entry]\nIcon=right\n", Some("right")),
            ("[Desktop Entry]\nName=App\n", None),
            ("[Desktop Entry]\nIcon=\n", None),
        ] {
            let path = dir.join("app.desktop");
            std::fs::write(&path, body).expect("write desktop entry");
            assert_eq!(read_icon_key(&path).as_deref(), icon, "{body}");
        }
        assert_eq!(read_icon_key(Path::new("/nonexistent/app.desktop")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    mod desktop_parsing {
        use super::*;

        const FIREFOX: &str = "\
[Desktop Entry]
Type=Application
Name=Firefox
Name[uk]=Firefox Браузер
Name[en_GB]=Firefox (GB)
GenericName=Web Browser
GenericName[uk]=Веббраузер
GenericName[uk_UA]=Не це
Comment=Browse the World Wide Web
Comment[uk]=Перегляд Інтернету
Keywords=Internet;WWW;Browser;
Keywords[uk]=Інтернет;Браузер;
Exec=firefox %u
Icon=firefox
Categories=Network;WebBrowser;
Actions=new-window;new-private-window;missing;

# a comment line
[Desktop Action new-window]
Name=New Window
Name[uk]=Нове вікно
Exec=firefox --new-window %u

[Desktop Action new-private-window]
Name=New Private Window
Name[uk_UA]=Нове приватне вікно
Exec=firefox --private-window %u
";

        fn parse(lang: &str) -> DesktopApp {
            parse_desktop_entry("firefox", FIREFOX, lang).expect("a valid application entry")
        }

        #[test]
        fn english_prefers_the_unlocalised_values_and_lists_known_actions_in_order() {
            let app = parse("en");
            assert_eq!((app.id.as_str(), app.icon.as_str()), ("firefox", "firefox"));
            assert_eq!(app.category, AppCategory::Internet);
            assert_eq!(app.name, "Firefox", "en_GB must not win over plain Name");
            assert_eq!(app.command, "firefox");
            assert_eq!(app.generic_name, "Web Browser");
            assert_eq!(app.comment, "Browse the World Wide Web");
            assert_eq!(app.keywords, ["Internet", "WWW", "Browser"]);
            let names: Vec<&str> = app.actions.iter().map(|a| a.name.as_str()).collect();
            assert_eq!(names, ["New Window", "New Private Window"]);
            assert_eq!(app.actions[0].id, "new-window");
            assert_eq!(app.actions[1].command, "firefox --private-window");
        }

        #[test]
        fn ukrainian_takes_the_exact_locale_first_and_keeps_english_keywords() {
            let app = parse("uk");
            assert_eq!(app.name, "Firefox Браузер");
            assert_eq!(app.generic_name, "Веббраузер", "[uk] outranks [uk_UA]");
            assert_eq!(app.comment, "Перегляд Інтернету");
            for wanted in ["Інтернет", "Браузер", "Internet", "WWW", "Firefox"] {
                assert!(app.keywords.iter().any(|keyword| keyword == wanted), "{wanted}");
            }
            let names: Vec<&str> = app.actions.iter().map(|a| a.name.as_str()).collect();
            assert_eq!(names, ["Нове вікно", "Нове приватне вікно"]);
        }

        #[test]
        fn terminal_apps_and_their_actions_run_inside_the_default_terminal() {
            let body = "[Desktop Entry]\nName=htop\nExec=htop %U\nTerminal=true\nActions=tree;\n\n\
                        [Desktop Action tree]\nName=Tree\nExec=htop --tree\n";
            let foot = || String::from("foot");
            let app = parse_desktop_entry_for("htop", body, "en", &foot).expect("entry");
            assert_eq!(app.command, "foot htop");
            assert_eq!(app.actions[0].command, "foot htop --tree");
            let gnome = || String::from("gnome-terminal");
            let app = parse_desktop_entry_for("htop", body, "en", &gnome).expect("entry");
            assert_eq!(app.command, "gnome-terminal -- htop");
            let graphical = body.replace("Terminal=true", "Terminal=false");
            let never = || -> String { panic!("no terminal is looked up for a graphical app") };
            let app = parse_desktop_entry_for("htop", &graphical, "en", &never).expect("entry");
            assert_eq!(app.command, "htop");
        }

        #[test]
        fn hidden_and_non_application_entries_are_dropped() {
            for body in [
                "[Desktop Entry]\nName=A\nExec=a\nNoDisplay=true\n",
                "[Desktop Entry]\nName=A\nExec=a\nHidden=true\n",
                "[Desktop Entry]\nType=Link\nName=A\nURL=https://example.com\n",
                "[Desktop Entry]\nExec=a\n",
            ] {
                assert_eq!(parse_desktop_entry("a", body, "en"), None, "{body}");
            }
        }

        #[test]
        fn keys_from_other_groups_never_leak_into_the_entry() {
            let body = "[Desktop Action x]\nName=Wrong\nExec=wrong\n[Desktop Entry]\nName=Right\nExec=right\n";
            let app = parse_desktop_entry("a", body, "en").expect("entry");
            assert_eq!(app.name, "Right");
            assert_eq!(app.command, "right");
            assert!(app.actions.is_empty(), "no Actions= key lists it");
        }

        #[test]
        fn escapes_in_string_values_are_decoded() {
            let body = "[Desktop Entry]\nName=A\\sB\nComment=one\\ntwo\nExec=a\n";
            let app = parse_desktop_entry("a", body, "en").expect("entry");
            assert_eq!(app.name, "A B");
            assert_eq!(app.comment, "one two");
        }
    }

    mod against_the_real_system {
        use super::*;

        fn resolve(app_id: &str, theme: &str) -> Option<PathBuf> {
            ThemeIndex::default().resolve_app(app_id, theme, 48)
        }

        #[test]
        fn a_known_app_resolves_to_an_icon_that_rasterises_and_an_unknown_one_to_nothing() {
            assert_eq!(resolve("definitely.not.an.installed.app.xyz", DEFAULT_THEME), None);
            let Some(path) = resolve("Alacritty", DEFAULT_THEME) else {
                return;
            };
            assert!(path.is_file(), "{path:?} should exist");
            let extension = path.extension().and_then(|e| e.to_str());
            assert!(matches!(extension, Some("svg" | "png")), "{path:?}");
            let pixels = render(&path, 42, None).expect("a resolved icon renders");
            assert_eq!(pixels.len(), 42 * 42 * 4);
            assert!(pixels.chunks_exact(4).any(|px| px[3] > 0));
        }

        #[test]
        fn papirus_status_glyphs_come_from_symbolic_files() {
            let mut index = ThemeIndex::default();
            for icon in ["audio-volume-high", "network-wireless-signal-good"] {
                let Some(path) = index.resolve_symbolic(icon, "Papirus", 16) else {
                    eprintln!("icon {icon} not found on this system; skipping");
                    continue;
                };
                assert!(
                    path.to_string_lossy().contains("-symbolic."),
                    "{icon} resolved to a full-colour file: {path:?}"
                );
            }
        }

        #[test]
        fn papirus_symbolic_and_panel_icons_resolve_and_render() {
            let mut loader = IconLoader::new(Some("Papirus".to_string()));
            let test_icons = [
                "view-app-grid",
                "network-wireless-signal-good",
                "network-wireless-signal-none",
                "audio-volume-high",
                "audio-volume-muted",
                "battery-080",
                "caffeine-cup-full",
                "caffeine-cup-empty",
                "notifications-disabled",
                "bluetooth-active",
                "night-light",
                "accessories-screenshot",
                "display-brightness",
                "emblem-system",
                "system-lock-screen",
                "system-shutdown",
                "media-playback-start",
                "media-playback-pause",
                "media-skip-backward",
                "media-skip-forward",
            ];
            for icon in test_icons {
                let Some(pixels) = loader.load_symbolic(icon, 24, Color([1.0, 0.5, 0.25, 1.0])) else {
                    eprintln!("icon {icon} not found on this system; skipping");
                    continue;
                };
                assert_eq!(pixels.len(), 24 * 24 * 4);
                assert!(pixels.chunks_exact(4).any(|px| px[3] > 0));
            }
        }
    }
}

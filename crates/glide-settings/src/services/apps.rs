use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use super::run;
use super::storage::home_dir as home;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub mime_types: Vec<String>,
    pub terminal: bool,
    pub no_display: bool,
    pub icon: String,
    pub comment: String,
}

impl std::fmt::Display for DesktopApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Web,
    Mail,
    Calendar,
    Files,
    Text,
    Images,
    Music,
    Video,
    Pdf,
}

impl Role {
    pub const ALL: &'static [Self] = &[
        Self::Web,
        Self::Mail,
        Self::Calendar,
        Self::Files,
        Self::Text,
        Self::Images,
        Self::Music,
        Self::Video,
        Self::Pdf,
    ];

    #[must_use]
    pub const fn mime_types(self) -> &'static [&'static str] {
        match self {
            Self::Web => &["x-scheme-handler/http", "x-scheme-handler/https", "text/html"],
            Self::Mail => &["x-scheme-handler/mailto"],
            Self::Calendar => &["text/calendar"],
            Self::Files => &["inode/directory"],
            Self::Text => &["text/plain"],
            Self::Images => &["image/png", "image/jpeg", "image/webp", "image/gif"],
            Self::Music => &["audio/mpeg", "audio/flac", "audio/ogg", "audio/x-wav"],
            Self::Video => &["video/mp4", "video/x-matroska", "video/webm"],
            Self::Pdf => &["application/pdf"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Documents,
    Images,
    Audio,
    Video,
    Archives,
    Code,
    Links,
}

impl Kind {
    pub const ALL: &'static [Self] =
        &[Self::Documents, Self::Images, Self::Audio, Self::Video, Self::Archives, Self::Code, Self::Links];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileType {
    pub mime: &'static str,
    pub extension: &'static str,
    pub kind: Kind,
}

const fn file_type(mime: &'static str, extension: &'static str, kind: Kind) -> FileType {
    FileType { mime, extension, kind }
}

pub(crate) const FILE_TYPES: &[FileType] = &[
    file_type("text/plain", ".txt", Kind::Documents),
    file_type("text/markdown", ".md", Kind::Documents),
    file_type("application/pdf", ".pdf", Kind::Documents),
    file_type("application/epub+zip", ".epub", Kind::Documents),
    file_type("text/csv", ".csv", Kind::Documents),
    file_type("application/vnd.oasis.opendocument.text", ".odt", Kind::Documents),
    file_type("application/vnd.openxmlformats-officedocument.wordprocessingml.document", ".docx", Kind::Documents),
    file_type("application/vnd.oasis.opendocument.spreadsheet", ".ods", Kind::Documents),
    file_type("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", ".xlsx", Kind::Documents),
    file_type("application/vnd.oasis.opendocument.presentation", ".odp", Kind::Documents),
    file_type("application/vnd.openxmlformats-officedocument.presentationml.presentation", ".pptx", Kind::Documents),
    file_type("text/calendar", ".ics", Kind::Documents),
    file_type("text/vcard", ".vcf", Kind::Documents),
    file_type("image/png", ".png", Kind::Images),
    file_type("image/jpeg", ".jpg", Kind::Images),
    file_type("image/gif", ".gif", Kind::Images),
    file_type("image/webp", ".webp", Kind::Images),
    file_type("image/svg+xml", ".svg", Kind::Images),
    file_type("image/heif", ".heic", Kind::Images),
    file_type("image/tiff", ".tiff", Kind::Images),
    file_type("image/bmp", ".bmp", Kind::Images),
    file_type("audio/mpeg", ".mp3", Kind::Audio),
    file_type("audio/flac", ".flac", Kind::Audio),
    file_type("audio/ogg", ".ogg", Kind::Audio),
    file_type("audio/x-wav", ".wav", Kind::Audio),
    file_type("audio/mp4", ".m4a", Kind::Audio),
    file_type("audio/x-opus+ogg", ".opus", Kind::Audio),
    file_type("video/mp4", ".mp4", Kind::Video),
    file_type("video/x-matroska", ".mkv", Kind::Video),
    file_type("video/webm", ".webm", Kind::Video),
    file_type("video/quicktime", ".mov", Kind::Video),
    file_type("video/x-msvideo", ".avi", Kind::Video),
    file_type("application/zip", ".zip", Kind::Archives),
    file_type("application/x-7z-compressed", ".7z", Kind::Archives),
    file_type("application/x-compressed-tar", ".tar.gz", Kind::Archives),
    file_type("application/x-tar", ".tar", Kind::Archives),
    file_type("application/vnd.rar", ".rar", Kind::Archives),
    file_type("application/x-iso9660-image", ".iso", Kind::Archives),
    file_type("application/json", ".json", Kind::Code),
    file_type("application/xml", ".xml", Kind::Code),
    file_type("text/x-python", ".py", Kind::Code),
    file_type("text/rust", ".rs", Kind::Code),
    file_type("application/x-shellscript", ".sh", Kind::Code),
    file_type("text/x-csrc", ".c", Kind::Code),
    file_type("text/html", ".html", Kind::Code),
    file_type("text/css", ".css", Kind::Code),
    file_type("x-scheme-handler/http", "http://", Kind::Links),
    file_type("x-scheme-handler/https", "https://", Kind::Links),
    file_type("x-scheme-handler/mailto", "mailto:", Kind::Links),
    file_type("x-scheme-handler/magnet", "magnet:", Kind::Links),
    file_type("application/x-bittorrent", ".torrent", Kind::Links),
    file_type("inode/directory", "/", Kind::Links),
];

#[must_use]
pub fn managed_mime_types() -> Vec<&'static str> {
    let mut all: Vec<&'static str> = Role::ALL
        .iter()
        .flat_map(|role| role.mime_types().iter().copied())
        .chain(FILE_TYPES.iter().map(|file| file.mime))
        .collect();
    all.sort_unstable();
    all.dedup();
    all
}

#[must_use]
pub fn parse_desktop(id: &str, text: &str, lang: &str) -> Option<DesktopApp> {
    let mut in_entry = false;
    let mut name = None;
    let mut local_name = None;
    let mut comment = None;
    let mut local_comment = None;
    let mut exec = None;
    let mut icon = String::new();
    let mut mime_types = Vec::new();
    let mut terminal = false;
    let mut hidden = false;
    let mut no_display = false;
    let mut is_app = false;
    let local_key = format!("Name[{lang}]");
    let local_comment_key = format!("Comment[{lang}]");
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "Name" => name = Some(value.to_string()),
            "Comment" => comment = Some(value.to_string()),
            "Exec" => exec = Some(value.to_string()),
            "Icon" => icon = value.to_string(),
            "Type" => is_app = value == "Application",
            "Terminal" => terminal = value == "true",
            "NoDisplay" => no_display = value == "true",
            "Hidden" => hidden = value == "true",
            "MimeType" => {
                mime_types = value.split(';').filter(|mime| !mime.is_empty()).map(str::to_string).collect();
            }
            _ if key == local_key => local_name = Some(value.to_string()),
            _ if key == local_comment_key => local_comment = Some(value.to_string()),
            _ => {}
        }
    }
    if !is_app || hidden {
        return None;
    }
    Some(DesktopApp {
        id: id.to_string(),
        name: local_name.or(name)?,
        exec: exec?,
        mime_types,
        terminal,
        no_display,
        icon,
        comment: local_comment.or(comment).unwrap_or_default(),
    })
}

#[must_use]
pub fn visible(apps: &[DesktopApp]) -> Vec<DesktopApp> {
    apps.iter().filter(|app| !app.no_display).cloned().collect()
}

#[must_use]
pub fn resolve(apps: &[DesktopApp], id: &str) -> DesktopApp {
    apps.iter().find(|app| app.id == id).cloned().unwrap_or_else(|| DesktopApp {
        id: id.to_string(),
        name: id.trim_end_matches(".desktop").to_string(),
        no_display: true,
        ..DesktopApp::default()
    })
}

#[must_use]
pub fn mime_options(apps: &[DesktopApp], mimes: &[&str], current: Option<&str>) -> Vec<DesktopApp> {
    let mut options: Vec<DesktopApp> =
        apps.iter().filter(|app| !app.no_display && handles(app, mimes)).cloned().collect();
    if let Some(id) = current
        && !options.iter().any(|app| app.id == id)
    {
        options.insert(0, resolve(apps, id));
    }
    options
}

#[must_use]
pub fn strip_field_codes(exec: &str) -> String {
    exec.split_whitespace().filter(|word| !(word.len() == 2 && word.starts_with('%'))).collect::<Vec<_>>().join(" ")
}

pub use glide_config::{config_home, data_dirs, system_config_dirs as config_dirs};

#[must_use]
pub fn installed(lang: &str) -> Vec<DesktopApp> {
    let mut apps: BTreeMap<String, DesktopApp> = BTreeMap::new();
    let mut seen = std::collections::HashSet::new();
    for dir in data_dirs() {
        let Ok(entries) = std::fs::read_dir(dir.join("applications")) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            if !seen.insert(id.clone()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(app) = parse_desktop(&id, &text, lang) {
                apps.insert(id, app);
            }
        }
    }
    let mut list: Vec<DesktopApp> = apps.into_values().collect();
    list.sort_by_key(|app| app.name.to_lowercase());
    list
}

fn handles(app: &DesktopApp, mimes: &[&str]) -> bool {
    mimes.iter().any(|mime| app.mime_types.iter().any(|own| own == mime))
}

#[must_use]
pub fn handlers(apps: &[DesktopApp], role: Role) -> Vec<&DesktopApp> {
    apps.iter().filter(|app| handles(app, role.mime_types())).collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MimeList {
    pub defaults: Vec<(String, Vec<String>)>,
    pub added: Vec<(String, Vec<String>)>,
    pub removed: Vec<(String, Vec<String>)>,
}

pub(crate) fn split_ids(value: &str) -> Vec<String> {
    value.split(';').map(str::trim).filter(|id| !id.is_empty()).map(str::to_string).collect()
}

#[must_use]
pub fn parse_mimeapps(text: &str) -> MimeList {
    let mut list = MimeList::default();
    let mut section = "";
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = match line {
                "[Default Applications]" => "default",
                "[Added Associations]" => "added",
                "[Removed Associations]" => "removed",
                _ => "",
            };
            continue;
        }
        let Some((mime, ids)) = line.split_once('=') else {
            continue;
        };
        let entry = (mime.trim().to_string(), split_ids(ids));
        match section {
            "default" => list.defaults.push(entry),
            "added" => list.added.push(entry),
            "removed" => list.removed.push(entry),
            _ => {}
        }
    }
    list
}

pub(crate) fn desktop_names() -> Vec<String> {
    std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_lowercase)
        .collect()
}

#[must_use]
pub(crate) fn mimeapps_paths() -> Vec<PathBuf> {
    let desktops = desktop_names();
    let mut bases: Vec<PathBuf> = Vec::new();
    bases.extend(config_home());
    bases.extend(config_dirs());
    bases.extend(data_dirs().into_iter().map(|dir| dir.join("applications")));
    let mut paths = Vec::new();
    for base in bases {
        for desktop in &desktops {
            paths.push(base.join(format!("{desktop}-mimeapps.list")));
        }
        paths.push(base.join("mimeapps.list"));
    }
    paths
}

#[must_use]
pub(crate) fn load_mimeapps() -> Vec<MimeList> {
    mimeapps_paths()
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| parse_mimeapps(&text))
        .collect()
}

#[must_use]
pub fn resolve_default(lists: &[MimeList], apps: &[DesktopApp], mime: &str) -> Option<String> {
    let pick = |entries: &[(String, Vec<String>)], removed: &[&str]| {
        let (_, ids) = entries.iter().find(|(key, _)| key == mime)?;
        ids.iter().find(|id| apps.iter().any(|app| app.id == **id) && !removed.contains(&id.as_str())).cloned()
    };
    let mut removed: Vec<&str> = Vec::new();
    for list in lists {
        if let Some(id) = pick(&list.defaults, &removed) {
            return Some(id);
        }
        for (key, ids) in &list.removed {
            if key == mime {
                removed.extend(ids.iter().map(String::as_str));
            }
        }
    }
    lists.iter().find_map(|list| pick(&list.added, &removed)).or_else(|| {
        apps.iter()
            .find(|app| {
                !app.no_display && !removed.contains(&app.id.as_str()) && app.mime_types.iter().any(|own| own == mime)
            })
            .map(|app| app.id.clone())
    })
}

#[must_use]
pub fn explicit_default(lists: &[MimeList], mime: &str) -> bool {
    lists.iter().any(|list| list.defaults.iter().any(|(key, ids)| key == mime && !ids.is_empty()))
}

#[must_use]
pub fn default_for(role: Role) -> Option<String> {
    let mime = role.mime_types().first()?;
    run::output("xdg-mime", &["query", "default", mime]).map(|text| text.trim().to_string()).filter(|id| !id.is_empty())
}

pub fn set_default(role: Role, app_id: &str) -> Result<String, String> {
    set_default_mimes(role.mime_types(), app_id)
}

pub(crate) fn set_default_mimes(mimes: &[&str], app_id: &str) -> Result<String, String> {
    let mut args = vec!["default", app_id];
    args.extend(mimes);
    run::status("xdg-mime", &args)
}

#[must_use]
pub fn strip_defaults(text: &str, mimes: &[&str]) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut in_defaults = false;
    let mut removed = 0;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_defaults = trimmed == "[Default Applications]";
        } else if in_defaults
            && let Some((key, _)) = trimmed.split_once('=')
            && mimes.contains(&key.trim())
        {
            removed += 1;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    (out, removed)
}

pub(crate) fn reset_defaults(mimes: &[&str]) -> Result<usize, String> {
    let Some(dir) = config_home() else {
        return Ok(0);
    };
    let mut names = vec![String::from("mimeapps.list")];
    names.extend(desktop_names().into_iter().map(|desktop| format!("{desktop}-mimeapps.list")));
    let mut total = 0;
    for name in names {
        let path = dir.join(name);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let (next, removed) = strip_defaults(&text, mimes);
        if removed > 0 {
            write_file(&path, &next).map_err(|err| format!("{}: {err}", path.display()))?;
            total += removed;
        }
    }
    Ok(total)
}

pub(crate) fn write_file(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    super::config::write_atomic(path, contents.as_bytes())
}

#[must_use]
pub fn parse_mime_comment(xml: &str, lang: &str) -> Option<String> {
    let mut generic = None;
    let mut local = None;
    let local_tag = format!("<comment xml:lang=\"{lang}\">");
    for line in xml.lines() {
        let line = line.trim();
        let (rest, is_local) = if let Some(rest) = line.strip_prefix(&local_tag) {
            (rest, true)
        } else if let Some(rest) = line.strip_prefix("<comment>") {
            (rest, false)
        } else {
            continue;
        };
        let Some(end) = rest.find("</comment>") else {
            continue;
        };
        let value = unescape_xml(&rest[..end]);
        if is_local {
            local.get_or_insert(value);
        } else {
            generic.get_or_insert(value);
        }
    }
    local.or(generic).filter(|text| !text.is_empty())
}

fn unescape_xml(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

#[must_use]
pub(crate) fn mime_description(mime: &str, lang: &str) -> Option<String> {
    data_dirs().iter().find_map(|dir| {
        let path = dir.join("mime").join(format!("{mime}.xml"));
        std::fs::read_to_string(path).ok().and_then(|xml| parse_mime_comment(&xml, lang))
    })
}

#[must_use]
pub(crate) fn mime_descriptions(lang: &str) -> HashMap<&'static str, String> {
    FILE_TYPES.iter().filter_map(|file| Some((file.mime, mime_description(file.mime, lang)?))).collect()
}

const ICON_SUBDIRS: &[&str] = &[
    "scalable/apps",
    "48x48/apps",
    "64x64/apps",
    "128x128/apps",
    "256x256/apps",
    "32x32/apps",
    "apps/scalable",
    "apps/48",
    "512x512/apps",
];

#[must_use]
pub(crate) fn icon_candidates(base: &Path, theme: &str, name: &str) -> Vec<PathBuf> {
    ICON_SUBDIRS
        .iter()
        .flat_map(|sub| ["svg", "png"].iter().map(move |ext| base.join(theme).join(sub).join(format!("{name}.{ext}"))))
        .collect()
}

#[must_use]
pub(crate) fn icon_path(name: &str, theme: Option<&str>) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let direct = Path::new(name);
    if direct.is_absolute() {
        return direct.is_file().then(|| direct.to_path_buf());
    }
    let mut themes: Vec<&str> = Vec::new();
    themes.extend(theme);
    themes.extend(["Papirus", "Adwaita", "hicolor"]);
    themes.dedup();
    let bases = theme_dirs();
    for theme in themes {
        for base in &bases {
            if !base.join(theme).is_dir() {
                continue;
            }
            if let Some(found) = icon_candidates(base, theme, name).into_iter().find(|path| path.is_file()) {
                return Some(found);
            }
        }
    }
    ["svg", "png", "xpm"]
        .iter()
        .map(|ext| PathBuf::from(format!("/usr/share/pixmaps/{name}.{ext}")))
        .find(|path| path.is_file() && path.extension().is_some_and(|ext| ext != "xpm"))
}

#[must_use]
pub(crate) fn icon_paths<'a>(
    names: impl IntoIterator<Item = &'a str>, theme: Option<&str>,
) -> HashMap<String, PathBuf> {
    let mut found = HashMap::new();
    for name in names {
        if found.contains_key(name) {
            continue;
        }
        if let Some(path) = icon_path(name, theme) {
            found.insert(name.to_string(), path);
        }
    }
    found
}

pub(crate) fn theme_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = data_dirs().into_iter().map(|dir| dir.join("icons")).collect();
    if let Some(home) = home() {
        dirs.insert(0, home.join(".icons"));
    }
    dirs
}

#[must_use]
pub(crate) fn is_icon_theme(dir: &Path) -> bool {
    let Ok(index) = std::fs::read_to_string(dir.join("index.theme")) else {
        return false;
    };
    index.contains("Directories=") && !index.lines().any(|line| line.trim() == "Hidden=true")
}

#[must_use]
pub(crate) fn icon_themes() -> Vec<String> {
    let mut names: Vec<String> = theme_dirs()
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter(|entry| is_icon_theme(&entry.path()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "hicolor" && name != "default")
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    names
}

#[must_use]
pub fn cursor_themes_in(dirs: &[PathBuf]) -> Vec<String> {
    let mut names: Vec<String> = dirs
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter(|entry| entry.path().join("cursors").is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "default")
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    names
}

#[must_use]
pub(crate) fn cursor_themes() -> Vec<String> {
    cursor_themes_in(&theme_dirs())
}

#[must_use]
pub fn parse_font_families(text: &str) -> Vec<String> {
    let mut families: Vec<String> = text
        .lines()
        .filter_map(|line| line.split(',').next())
        .map(|family| family.trim().replace('\\', ""))
        .filter(|family| !family.is_empty() && !family.starts_with('.'))
        .collect();
    families.sort_by_key(|family| family.to_lowercase());
    families.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    families
}

#[must_use]
pub(crate) fn font_families() -> Vec<String> {
    run::output("fc-list", &[":", "family"]).map(|text| parse_font_families(&text)).unwrap_or_default()
}

pub fn open_path(path: &Path) -> Result<String, String> {
    let status = std::process::Command::new("xdg-open")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|err| format!("xdg-open: {err}"))?;
    if status.success() { Ok(String::new()) } else { Err(format!("xdg-open: {status}")) }
}

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::gsettings::Snapshot;
use super::run;

pub const PRIVACY: &str = "org.gnome.desktop.privacy";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Usage {
    pub microphone: Vec<String>,
    pub camera: Vec<String>,
    pub screen: Vec<String>,
}

impl Usage {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.microphone.is_empty() && self.camera.is_empty() && self.screen.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Footprint {
    pub items: u64,
    pub bytes: u64,
}

#[allow(clippy::struct_excessive_bools)] // Independent facts: loaded, schema present, a gsettings flag, PipeWire running
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrivacyState {
    pub loaded: bool,
    pub gsettings: bool,
    pub remember_recent: bool,
    pub recent_max_age: i64,
    pub recent: Footprint,
    pub trash: Footprint,
    pub thumbnails: Footprint,
    pub usage: Usage,
    pub pipewire: bool,
}

pub(crate) use glide_config::data_home;

#[must_use]
pub fn recent_path() -> Option<PathBuf> {
    data_home().map(|dir| dir.join("recently-used.xbel"))
}

#[must_use]
pub fn trash_path() -> Option<PathBuf> {
    data_home().map(|dir| dir.join("Trash"))
}

#[must_use]
pub fn thumbnails_path() -> Option<PathBuf> {
    glide_config::cache_home().map(|dir| dir.join("thumbnails"))
}

const THUMBNAIL_SIZES: [&str; 5] = ["normal", "large", "x-large", "xx-large", "fail"];

#[must_use]
pub fn count_bookmarks(xbel: &str) -> u64 {
    xbel.matches("<bookmark ").count() as u64
}

const WALK_LIMIT: u64 = 200_000;

fn walk(path: &Path, footprint: &mut Footprint, depth: u32) {
    if depth > 32 || footprint.items > WALK_LIMIT {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.path().symlink_metadata() else {
            continue;
        };
        footprint.items += 1;
        footprint.bytes += meta.len();
        if meta.is_dir() {
            walk(&entry.path(), footprint, depth + 1);
        }
    }
}

#[must_use]
pub fn measure(path: &Path) -> Footprint {
    let mut footprint = Footprint::default();
    walk(path, &mut footprint, 0);
    footprint
}

fn trash_footprint() -> Footprint {
    let Some(trash) = trash_path() else {
        return Footprint::default();
    };
    let files = trash.join("files");
    let items = std::fs::read_dir(&files).map_or(0, |entries| entries.flatten().count() as u64);
    Footprint { items, bytes: measure(&files).bytes }
}

fn thumbnail_footprint() -> Footprint {
    let Some(dir) = thumbnails_path() else {
        return Footprint::default();
    };
    let mut footprint = Footprint::default();
    for size in THUMBNAIL_SIZES {
        if let Ok(entries) = std::fs::read_dir(dir.join(size)) {
            for entry in entries.flatten() {
                footprint.items += 1;
                footprint.bytes += entry.metadata().map_or(0, |meta| meta.len());
            }
        }
    }
    footprint
}

fn recent_footprint() -> Footprint {
    let Some(path) = recent_path() else {
        return Footprint::default();
    };
    let bytes = std::fs::metadata(&path).map_or(0, |meta| meta.len());
    let items = std::fs::read_to_string(&path).map_or(0, |text| count_bookmarks(&text));
    Footprint { items, bytes }
}

fn app_name(props: &Value) -> String {
    ["application.name", "application.process.binary", "node.name"]
        .into_iter()
        .find_map(|key| props.get(key).and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

fn flag(props: &Value, key: &str) -> bool {
    match props.get(key) {
        Some(Value::Bool(on)) => *on,
        Some(Value::String(text)) => text == "true",
        _ => false,
    }
}

fn is_camera(props: &Value) -> bool {
    let name = props.get("node.name").and_then(Value::as_str).unwrap_or_default();
    props.get("device.api").is_some()
        || props.get("api.v4l2.path").is_some()
        || props.get("api.libcamera.path").is_some()
        || name.starts_with("v4l2_")
        || name.starts_with("libcamera_")
}

fn tidy(mut names: Vec<String>) -> Vec<String> {
    names.retain(|name| !name.is_empty());
    names.sort();
    names.dedup();
    names
}

#[must_use]
pub fn parse_pw_dump(text: &str) -> Usage {
    let Ok(Value::Array(objects)) = serde_json::from_str::<Value>(text) else {
        return Usage::default();
    };
    let null = Value::Null;
    let mut nodes = std::collections::HashMap::new();
    let mut links = Vec::new();
    for object in &objects {
        let kind = object.get("type").and_then(Value::as_str).unwrap_or_default();
        let id = object.get("id").and_then(Value::as_u64);
        let info = object.get("info").unwrap_or(&null);
        match (kind, id) {
            ("PipeWire:Interface:Node", Some(id)) => {
                let props = info.get("props").unwrap_or(&null);
                let class = props.get("media.class").and_then(Value::as_str).unwrap_or_default();
                let running = info.get("state").and_then(Value::as_str) == Some("running");
                nodes.insert(id, (class, running, props));
            }
            ("PipeWire:Interface:Link", _) => {
                let output = info.get("output-node-id").and_then(Value::as_u64);
                let input = info.get("input-node-id").and_then(Value::as_u64);
                if let (Some(output), Some(input)) = (output, input) {
                    links.push((output, input));
                }
            }
            _ => {}
        }
    }
    let mut usage = Usage::default();
    for (class, running, props) in nodes.values() {
        if *class == "Stream/Input/Audio"
            && *running
            && !flag(props, "stream.capture.sink")
            && !flag(props, "stream.monitor")
        {
            usage.microphone.push(app_name(props));
        }
    }
    for (id, (class, _, props)) in &nodes {
        if *class != "Video/Source" {
            continue;
        }
        let consumers = links
            .iter()
            .filter(|(output, _)| output == id)
            .filter_map(|(_, input)| nodes.get(input))
            .filter(|(class, running, _)| class.starts_with("Stream/Input/Video") && *running)
            .map(|(_, _, props)| app_name(props));
        if is_camera(props) {
            usage.camera.extend(consumers);
        } else {
            usage.screen.extend(consumers);
        }
    }
    Usage { microphone: tidy(usage.microphone), camera: tidy(usage.camera), screen: tidy(usage.screen) }
}

fn camera_openers() -> Vec<String> {
    let Ok(processes) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for process in processes.flatten() {
        if !process.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let path = process.path();
        let Ok(fds) = std::fs::read_dir(path.join("fd")) else {
            continue;
        };
        let opened = fds.flatten().any(|fd| {
            std::fs::read_link(fd.path()).is_ok_and(|target| target.to_string_lossy().starts_with("/dev/video"))
        });
        if opened {
            let comm = std::fs::read_to_string(path.join("comm")).unwrap_or_default();
            let comm = comm.trim();
            if !matches!(comm, "pipewire" | "wireplumber") {
                names.push(comm.to_string());
            }
        }
    }
    names
}

#[must_use]
pub fn read_usage() -> (bool, Usage) {
    let dump = run::output("pw-dump", &[]);
    let pipewire = dump.is_some();
    let mut usage = dump.map(|text| parse_pw_dump(&text)).unwrap_or_default();
    usage.camera.extend(camera_openers());
    usage.camera = tidy(std::mem::take(&mut usage.camera));
    (pipewire, usage)
}

#[must_use]
pub fn read() -> PrivacyState {
    let snapshot = Snapshot::read(&[PRIVACY]);
    let (pipewire, usage) = read_usage();
    PrivacyState {
        loaded: true,
        gsettings: snapshot.has_schema(PRIVACY),
        remember_recent: snapshot.bool(PRIVACY, "remember-recent-files").unwrap_or(true),
        recent_max_age: snapshot.int(PRIVACY, "recent-files-max-age").unwrap_or(-1),
        recent: recent_footprint(),
        trash: trash_footprint(),
        thumbnails: thumbnail_footprint(),
        usage,
        pipewire,
    }
}

fn clear_dir(dir: &Path) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let meta = path.symlink_metadata()?;
        if meta.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

pub fn empty_trash() -> Result<String, String> {
    let trash = trash_path().ok_or_else(|| "HOME is not set".to_string())?;
    for sub in ["files", "info", "expunged"] {
        clear_dir(&trash.join(sub)).map_err(|err| format!("{}: {err}", trash.join(sub).display()))?;
    }
    Ok(String::new())
}

pub fn clear_recent() -> Result<String, String> {
    let path = recent_path().ok_or_else(|| "HOME is not set".to_string())?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(String::new()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(format!("{}: {err}", path.display())),
    }
}

pub fn clear_thumbnails() -> Result<String, String> {
    let dir = thumbnails_path().ok_or_else(|| "HOME is not set".to_string())?;
    for size in THUMBNAIL_SIZES {
        clear_dir(&dir.join(size)).map_err(|err| format!("{}: {err}", dir.join(size).display()))?;
    }
    Ok(String::new())
}

pub fn lock_screen() -> Result<String, String> {
    run::status("loginctl", &["lock-session"])
}

#[must_use]
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

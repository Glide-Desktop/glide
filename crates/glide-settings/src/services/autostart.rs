use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use glide_config::autostart::Provided;
use glide_config::autostart::{is_phone_daemon, provided_by_shell};

use super::apps;

pub(crate) const OVERRIDE_KEY: &str = "X-Glide-Override";

#[allow(clippy::struct_excessive_bools)] // Independent facts read from a .desktop file, not a state machine
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub comment: String,
    pub icon: String,
    pub exec: String,
    pub user_path: Option<PathBuf>,
    pub system_path: Option<PathBuf>,
    pub enabled: bool,
    pub desktop_ok: bool,
    pub conditional: bool,
    pub runnable: bool,
    pub replaced: bool,
    pub only_in: Vec<String>,
}

impl Entry {
    #[must_use]
    pub const fn removable(&self) -> bool {
        self.user_path.is_some() && self.system_path.is_none()
    }

    #[must_use]
    pub const fn starts(&self) -> bool {
        self.enabled && self.desktop_ok && !self.conditional && self.runnable && !self.replaced
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fields {
    pub values: HashMap<String, String>,
    pub is_app: bool,
}

#[must_use]
pub fn parse_fields(text: &str) -> Fields {
    let mut fields = Fields::default();
    for (key, value) in glide_config::desktop::entry(text) {
        fields.values.entry(key.to_string()).or_insert_with(|| value.to_string());
    }
    fields.is_app = fields.values.get("Type").is_some_and(|kind| kind == "Application");
    fields
}

fn list(value: Option<&String>) -> Vec<String> {
    value.map(|value| apps::split_ids(value)).unwrap_or_default()
}

fn overridden(fields: &Fields) -> bool {
    fields.values.get(OVERRIDE_KEY).is_some_and(|flag| flag == "true")
}

#[must_use]
pub fn build_entry(
    id: &str, fields: &Fields, lang: &str, desktops: &[String], exists: &dyn Fn(&str) -> bool, provided: Provided,
) -> Entry {
    let get = |key: &str| fields.values.get(key).cloned().unwrap_or_default();
    let localized = |key: &str| fields.values.get(&format!("{key}[{lang}]")).cloned().unwrap_or_else(|| get(key));
    let only_in = list(fields.values.get("OnlyShowIn"));
    let not_in = list(fields.values.get("NotShowIn"));
    let lower = |items: &[String]| -> Vec<String> { items.iter().map(|s| s.to_lowercase()).collect() };
    let only_lower = lower(&only_in);
    let not_lower = lower(&not_in);
    let desktop_ok = (only_in.is_empty() || desktops.iter().any(|d| only_lower.contains(d)))
        && !desktops.iter().any(|d| not_lower.contains(d));
    let try_exec = get("TryExec");
    let exec = get("Exec");
    let program =
        if try_exec.is_empty() { exec.split_whitespace().next().unwrap_or_default().to_string() } else { try_exec };
    let name = localized("Name");
    Entry {
        id: id.to_string(),
        name: if name.is_empty() { id.trim_end_matches(".desktop").to_string() } else { name },
        comment: localized("Comment"),
        icon: get("Icon"),
        exec: apps::strip_field_codes(&exec),
        user_path: None,
        system_path: None,
        enabled: get("Hidden") != "true" && get("X-GNOME-Autostart-enabled") != "false",
        desktop_ok,
        conditional: !get("AutostartCondition").is_empty(),
        runnable: fields.is_app && !exec.is_empty() && (program.is_empty() || exists(&program)),
        replaced: provided_by_shell(&exec, provided),
        only_in,
    }
}

#[must_use]
pub fn user_dir() -> Option<PathBuf> {
    apps::config_home().map(|dir| dir.join("autostart"))
}

#[must_use]
pub(crate) fn system_dirs() -> Vec<PathBuf> {
    apps::config_dirs().into_iter().map(|dir| dir.join("autostart")).collect()
}

#[must_use]
pub fn desktops() -> Vec<String> {
    let names = apps::desktop_names();
    if names.is_empty() { vec![String::from("glide")] } else { names }
}

use glide_config::on_path;

fn desktop_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "desktop"))
        .collect();
    paths.sort();
    paths
}

fn file_id(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

#[must_use]
pub fn scan_dirs(
    user: Option<&Path>, system: &[PathBuf], lang: &str, desktops: &[String], exists: &dyn Fn(&str) -> bool,
    provided: Provided,
) -> Vec<Entry> {
    let mut system_paths: HashMap<String, PathBuf> = HashMap::new();
    for dir in system {
        for path in desktop_files(dir) {
            system_paths.entry(file_id(&path)).or_insert(path);
        }
    }
    let mut entries: Vec<Entry> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let user_paths = user.map(desktop_files).unwrap_or_default();
    for path in user_paths.iter().chain(system_paths.values()) {
        let id = file_id(path);
        if !seen.insert(id.clone()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let fields = parse_fields(&text);
        let mut entry = build_entry(&id, &fields, lang, desktops, exists, provided);
        let from_user = user.is_some_and(|dir| path.starts_with(dir));
        if from_user {
            entry.user_path = Some(path.clone());
        }
        entry.system_path = system_paths.get(&id).cloned();
        if from_user
            && overridden(&fields)
            && let Some(system) = &entry.system_path
            && let Ok(original) = std::fs::read_to_string(system)
        {
            let original = build_entry(&id, &parse_fields(&original), lang, desktops, exists, provided);
            entry.name = original.name;
            entry.comment = original.comment;
            entry.icon = original.icon;
        }
        if !fields.is_app && !entry.enabled {
            entry.runnable = false;
        }
        entries.push(entry);
    }
    let phone_runs = entries.iter().any(|entry| entry.starts() && is_phone_daemon(&entry.exec));
    if provided.phone && !phone_runs {
        let provided = Provided { phone: false, ..provided };
        for entry in &mut entries {
            entry.replaced = provided_by_shell(&entry.exec, provided);
        }
    }
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    entries
}

#[must_use]
pub fn scan(lang: &str, provided: Provided) -> Vec<Entry> {
    scan_dirs(user_dir().as_deref(), &system_dirs(), lang, &desktops(), &on_path, provided)
}

#[must_use]
pub fn set_key(text: &str, key: &str, value: Option<&str>) -> String {
    let mut out = String::with_capacity(text.len() + 32);
    let mut in_entry = false;
    let mut written = false;
    let mut saw_entry = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if in_entry && !written {
                if let Some(value) = value {
                    push_pair(&mut out, key, value);
                }
                written = true;
            }
            in_entry = trimmed == "[Desktop Entry]";
            saw_entry |= in_entry;
        } else if in_entry
            && let Some((name, _)) = trimmed.split_once('=')
            && name.trim() == key
        {
            if !written && let Some(value) = value {
                push_pair(&mut out, key, value);
            }
            written = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !written && let Some(value) = value {
        if !saw_entry {
            out.push_str("[Desktop Entry]\n");
        }
        push_pair(&mut out, key, value);
    }
    out
}

fn push_pair(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push('=');
    out.push_str(value);
    out.push('\n');
}

#[must_use]
pub fn enabled_text(text: &str, enabled: bool) -> String {
    let text = set_key(text, "X-GNOME-Autostart-enabled", None);
    set_key(&text, "Hidden", (!enabled).then_some("true"))
}

pub(crate) fn set_enabled(entry: &Entry, enabled: bool) -> Result<(), String> {
    let describe = |path: &Path, err: std::io::Error| format!("{}: {err}", path.display());
    if let Some(user) = &entry.user_path {
        let text = std::fs::read_to_string(user).map_err(|err| describe(user, err))?;
        if enabled && overridden(&parse_fields(&text)) && entry.system_path.is_some() {
            return std::fs::remove_file(user).map_err(|err| describe(user, err));
        }
        return apps::write_file(user, &enabled_text(&text, enabled)).map_err(|err| describe(user, err));
    }
    let Some(system) = &entry.system_path else {
        return Ok(());
    };
    if enabled {
        return Ok(());
    }
    let dir = user_dir().ok_or_else(|| String::from("HOME is not set"))?;
    let text = std::fs::read_to_string(system).map_err(|err| describe(system, err))?;
    let text = set_key(&enabled_text(&text, false), OVERRIDE_KEY, Some("true"));
    let target = dir.join(&entry.id);
    apps::write_file(&target, &text).map_err(|err| describe(&target, err))
}

pub fn remove(entry: &Entry) -> Result<(), String> {
    match &entry.user_path {
        Some(path) if entry.system_path.is_none() => {
            std::fs::remove_file(path).map_err(|err| format!("{}: {err}", path.display()))
        }
        _ => Ok(()),
    }
}

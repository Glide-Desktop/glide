use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

use super::mime::{MimeDb, glob_match};
use super::uri::file_uri;
use crate::i18n::{Key, Lang};

const SERVICE_MENU_DIRS: [&str; 3] = ["kio/servicemenus", "kservices5/ServiceMenus", "kservices6/ServiceMenus"];
const FILE_MANAGER_ACTIONS: &str = "file-manager/actions";
const TAP_DIRS: [&str; 4] = [
    "/usr/lib/xfce4/thunar-archive-plugin",
    "/usr/libexec/thunar-archive-plugin",
    "/usr/lib64/xfce4/thunar-archive-plugin",
    "/usr/lib/x86_64-linux-gnu/xfce4/thunar-archive-plugin",
];
const PLURAL: &str = "FUDBWXM";
const SINGULAR: &str = "fudbwxm";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Roots {
    pub data: Vec<PathBuf>,
    pub taps: Vec<PathBuf>,
}

impl Roots {
    #[must_use]
    pub fn system() -> Self {
        Self { data: glide_config::data_dirs(), taps: TAP_DIRS.iter().map(PathBuf::from).collect() }
    }

    #[must_use]
    pub fn stamp(&self) -> Vec<Option<SystemTime>> {
        let watched = SERVICE_MENU_DIRS.into_iter().chain([FILE_MANAGER_ACTIONS, "applications", "mime"]);
        self.data
            .iter()
            .flat_map(|dir| watched.clone().map(move |sub| dir.join(sub)))
            .chain(self.taps.iter().cloned())
            .map(|dir| std::fs::metadata(dir).and_then(|meta| meta.modified()).ok())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Run {
    Exec { line: String, desktop: PathBuf, codes: Codes },
    Tap { script: PathBuf, verb: &'static str },
    SetDefault { mime: String, app: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Codes {
    Entry { name: String, icon: Option<String> },
    Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub exec: String,
    pub desktop: PathBuf,
    pub listed: bool,
    mimes: Vec<String>,
}

impl DesktopApp {
    #[must_use]
    pub fn run(&self) -> Run {
        Run::Exec {
            line: self.exec.clone(),
            desktop: self.desktop.clone(),
            codes: Codes::Entry { name: self.name.clone(), icon: self.icon.clone() },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Associations {
    defaults: HashMap<String, Vec<String>>,
    added: HashMap<String, Vec<String>>,
    removed: HashMap<String, Vec<String>>,
}

impl Associations {
    fn merge_file(&mut self, text: &str) {
        let groups = parse_ini(text);
        for (section, map) in [
            ("Default Applications", &mut self.defaults),
            ("Added Associations", &mut self.added),
            ("Removed Associations", &mut self.removed),
        ] {
            let Some(group) = groups.get(section) else {
                continue;
            };
            for mime in group.keys() {
                map.entry(mime.clone()).or_default().extend(list(group, mime));
            }
        }
    }
}

fn mimeapps_files(roots: &Roots) -> Vec<PathBuf> {
    glide_config::config_dirs()
        .into_iter()
        .map(|dir| dir.join("mimeapps.list"))
        .chain(roots.data.iter().map(|dir| dir.join("applications/mimeapps.list")))
        .collect()
}

#[must_use]
pub fn exec_line(exec: &str, in_terminal: bool, terminal: &dyn Fn() -> String) -> String {
    if in_terminal { glide_config::terminal::wrap(&terminal(), exec) } else { exec.to_string() }
}

fn desktop_apps(roots: &Roots, lang: Lang) -> Vec<DesktopApp> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for base in roots.data.iter().map(|dir| dir.join("applications")) {
        for file in desktop_files(&base, "desktop", 4) {
            let Some(id) = file.strip_prefix(&base).ok().map(|relative| relative.to_string_lossy().replace('/', "-"))
            else {
                continue;
            };
            if !seen.insert(id.clone()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            let groups = parse_ini(&text);
            let Some(entry) = groups.get("Desktop Entry") else {
                continue;
            };
            let application = entry.get("Type").is_none_or(|kind| kind == "Application");
            let mimes = list(entry, "MimeType");
            let (Some(exec), Some(name)) = (entry.get("Exec"), localized(entry, "Name", lang)) else {
                continue;
            };
            if application && usable(entry) && !mimes.is_empty() {
                let exec = exec_line(exec, entry.get("Terminal").is_some_and(|value| value == "true"), &|| {
                    glide_config::terminal::current()
                });
                let listed = flag(entry, "NoDisplay") != Some(true);
                let icon = entry.get("Icon").filter(|icon| !icon.trim().is_empty()).cloned();
                apps.push(DesktopApp { id, name, icon, exec, desktop: file, listed, mimes });
            }
        }
    }
    apps
}

pub fn set_default_app(mime: &str, app: &str) -> io::Result<()> {
    let file = glide_config::config_home()
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "no config directory"))?
        .join("mimeapps.list");
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let updated = with_default(&text, mime, app);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(file, updated)
}

#[must_use]
pub fn with_default(text: &str, mime: &str, app: &str) -> String {
    const SECTION: &str = "[Default Applications]";
    let line = format!("{mime}={app};");
    let mut out: Vec<String> = Vec::new();
    let mut inside = false;
    let mut written = false;
    for raw in text.lines() {
        let trimmed = raw.trim();
        if trimmed.starts_with('[') {
            if inside && !written {
                out.push(line.clone());
                written = true;
            }
            inside = trimmed == SECTION;
        } else if inside && trimmed.split_once('=').is_some_and(|(key, _)| key.trim() == mime) {
            if !written {
                out.push(line.clone());
                written = true;
            }
            continue;
        }
        out.push(raw.to_string());
    }
    if !written {
        if !inside {
            out.push(SECTION.to_string());
        }
        out.push(line);
    }
    out.join("\n") + "\n"
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Filter {
    mimes: Vec<String>,
    names: Vec<String>,
    match_case: bool,
    min: usize,
    max: usize,
    only: Vec<usize>,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            mimes: vec!["*".to_string()],
            names: Vec::new(),
            match_case: true,
            min: 1,
            max: usize::MAX,
            only: Vec::new(),
        }
    }
}

fn split_negated(patterns: &[String]) -> (Vec<&str>, Vec<&str>) {
    let (deny, allow): (Vec<&str>, Vec<&str>) =
        patterns.iter().map(|pattern| pattern.trim()).partition(|pattern| pattern.starts_with('!'));
    (allow, deny.into_iter().map(|pattern| &pattern[1..]).collect())
}

impl Filter {
    fn accepts(&self, db: &MimeDb, targets: &[Target]) -> bool {
        let count = targets.len();
        let counted =
            if self.only.is_empty() { (self.min..=self.max).contains(&count) } else { self.only.contains(&count) };
        counted && targets.iter().all(|target| self.fits(db, target))
    }

    fn fits(&self, db: &MimeDb, target: &Target) -> bool {
        let (allow, deny) = split_negated(&self.mimes);
        let typed = allow.iter().any(|pattern| db.is_a(&target.mime, pattern))
            && !deny.iter().any(|pattern| db.is_a(&target.mime, pattern));
        if !typed || self.names.is_empty() {
            return typed;
        }
        let fold = |text: &str| {
            if self.match_case { text.to_string() } else { text.to_lowercase() }
        };
        let name = fold(&target.name());
        let (allow, deny) = split_negated(&self.names);
        allow.iter().any(|pattern| glob_match(&fold(pattern), &name))
            && !deny.iter().any(|pattern| glob_match(&fold(pattern), &name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Profile {
    filter: Filter,
    run: Run,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub label: String,
    pub group: Option<String>,
    profiles: Vec<Profile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub label: String,
    pub group: Option<String>,
    pub run: Run,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub path: PathBuf,
    pub mime: String,
}

impl Target {
    fn name(&self) -> String {
        self.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Extensions {
    pub mime: MimeDb,
    pub actions: Vec<Action>,
    pub apps: Vec<DesktopApp>,
    pub associations: Associations,
    pub thumbnailers: std::sync::Arc<Vec<super::thumbs::Thumbnailer>>,
}

impl Extensions {
    #[must_use]
    pub fn load(roots: &Roots, lang: Lang) -> Self {
        let mut actions = archivers(roots, lang);
        let mut seen = HashSet::new();
        for dir in &roots.data {
            for sub in SERVICE_MENU_DIRS {
                for file in desktop_files(&dir.join(sub), "desktop", 3) {
                    if seen.insert(file.file_name().map(ToOwned::to_owned)) {
                        actions.extend(service_menu(&file, lang));
                    }
                }
            }
            for file in desktop_files(&dir.join(FILE_MANAGER_ACTIONS), "desktop", 1) {
                if seen.insert(file.file_name().map(ToOwned::to_owned)) {
                    actions.extend(file_manager_action(&file, lang));
                }
            }
        }
        let mut associations = Associations::default();
        for file in mimeapps_files(roots) {
            if let Ok(text) = std::fs::read_to_string(file) {
                associations.merge_file(&text);
            }
        }
        Self {
            mime: MimeDb::load(&roots.data),
            actions,
            apps: desktop_apps(roots, lang),
            thumbnailers: std::sync::Arc::new(super::thumbs::load_thumbnailers(&roots.data)),
            associations,
        }
    }

    #[must_use]
    pub fn apps_for(&self, targets: &[Target]) -> Vec<&DesktopApp> {
        let Some(first) = targets.first() else {
            return Vec::new();
        };
        let lineage = self.mime.lineage(&first.mime);
        let removed: HashSet<&String> =
            lineage.iter().filter_map(|mime| self.associations.removed.get(mime)).flatten().collect();
        let by_id = |id: &String| self.apps.iter().find(|app| &app.id == id);
        let mut ranked: Vec<&DesktopApp> = Vec::new();
        for mime in &lineage {
            let listed = [&self.associations.defaults, &self.associations.added]
                .into_iter()
                .filter_map(|map| map.get(mime))
                .flatten()
                .filter_map(by_id);
            let declared =
                self.apps.iter().filter(|app| app.mimes.iter().any(|declared| self.mime.canonical(declared) == mime));
            for app in listed.chain(declared) {
                if !removed.contains(&app.id) && !ranked.iter().any(|known| known.id == app.id) {
                    ranked.push(app);
                }
            }
        }
        ranked.retain(|app| {
            targets.iter().all(|target| app.mimes.iter().any(|declared| self.mime.is_a(&target.mime, declared)))
        });
        ranked
    }

    #[must_use]
    pub fn listed_apps_for(&self, targets: &[Target]) -> Vec<&DesktopApp> {
        let mut seen = HashSet::new();
        self.apps_for(targets).into_iter().filter(|app| app.listed && seen.insert(app.exec.as_str())).collect()
    }

    #[must_use]
    pub fn default_app(&self, mime: &str) -> Option<&str> {
        self.mime.lineage(mime).iter().find_map(|mime| {
            self.associations
                .defaults
                .get(mime)
                .and_then(|ids| ids.iter().find(|id| self.apps.iter().any(|app| &app.id == *id)))
                .map(String::as_str)
        })
    }

    #[must_use]
    pub fn target(&self, path: &Path, is_dir: bool) -> Target {
        Target { path: path.to_path_buf(), mime: self.mime.of(path, is_dir) }
    }

    #[must_use]
    pub fn offers(&self, targets: &[Target]) -> Vec<Offer> {
        if targets.is_empty() {
            return Vec::new();
        }
        let mut offers: Vec<Offer> = self
            .actions
            .iter()
            .filter_map(|action| {
                let profile = action.profiles.iter().find(|profile| profile.filter.accepts(&self.mime, targets))?;
                Some(Offer { label: action.label.clone(), group: action.group.clone(), run: profile.run.clone() })
            })
            .collect();
        let mut groups: Vec<Option<String>> = Vec::new();
        for offer in &offers {
            if !groups.contains(&offer.group) {
                groups.push(offer.group.clone());
            }
        }
        offers.sort_by_key(|offer| groups.iter().position(|group| *group == offer.group));
        offers
    }
}

fn desktop_files(dir: &Path, extension: &str, depth: u32) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = Vec::new();
    for path in read.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            if depth > 1 {
                files.extend(desktop_files(&path, extension, depth - 1));
            }
        } else if path.extension().is_some_and(|ext| ext == extension) {
            files.push(path);
        }
    }
    files.sort();
    files
}

type Group = HashMap<String, String>;

fn parse_ini(text: &str) -> HashMap<String, Group> {
    let mut groups: HashMap<String, Group> = HashMap::new();
    let mut current = None;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            groups.entry(name.to_string()).or_default();
            current = Some(name.to_string());
        } else if let (Some(group), Some((key, value))) = (&current, line.split_once('='))
            && let Some(group) = groups.get_mut(group)
        {
            group.entry(key.trim().to_string()).or_insert_with(|| unescape(value.trim()));
        }
    }
    groups
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn localized(group: &Group, key: &str, lang: Lang) -> Option<String> {
    let code = match lang {
        Lang::Uk => "uk",
        Lang::En => "en",
    };
    let regional = format!("{key}[{code}_");
    group
        .get(&format!("{key}[{code}]"))
        .or_else(|| group.iter().find(|(name, _)| name.starts_with(&regional)).map(|(_, value)| value))
        .or_else(|| group.get(key))
        .filter(|value| !value.is_empty())
        .cloned()
}

fn list(group: &Group, key: &str) -> Vec<String> {
    group
        .get(key)
        .map(|value| {
            value.split([';', ',']).map(str::trim).filter(|item| !item.is_empty()).map(str::to_string).collect()
        })
        .unwrap_or_default()
}

fn flag(group: &Group, key: &str) -> Option<bool> {
    group.get(key).map(|value| value.trim() == "true")
}

fn number(group: &Group, key: &str) -> Option<usize> {
    group.get(key).and_then(|value| value.trim().parse().ok())
}

fn installed(group: &Group) -> bool {
    let Some(program) = group.get("TryExec").map(|value| value.trim()) else {
        return true;
    };
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

fn usable(group: &Group) -> bool {
    flag(group, "Hidden") != Some(true) && installed(group)
}

fn serves_files(group: &Group, keys: &[&str]) -> bool {
    let schemes: Vec<String> = keys.iter().flat_map(|key| list(group, key)).collect();
    schemes.is_empty() || schemes.iter().any(|scheme| scheme == "file")
}

fn service_menu(file: &Path, lang: Lang) -> Vec<Action> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    let groups = parse_ini(&text);
    let Some(entry) = groups.get("Desktop Entry") else {
        return Vec::new();
    };
    if !usable(entry) || !serves_files(entry, &["X-KDE-Protocols", "X-KDE-Protocol"]) {
        return Vec::new();
    }
    let mut mimes = list(entry, "MimeType");
    mimes.extend(
        ["ServiceTypes", "X-KDE-ServiceTypes"]
            .iter()
            .flat_map(|key| list(entry, key))
            .filter(|kind| kind.contains('/') && !kind.ends_with("/Plugin")),
    );
    if mimes.is_empty() {
        return Vec::new();
    }
    mimes.extend(
        ["ExcludeServiceTypes", "X-KDE-ExcludeServiceTypes"]
            .iter()
            .flat_map(|key| list(entry, key))
            .map(|kind| format!("!{kind}")),
    );
    let filter = Filter {
        mimes,
        min: number(entry, "X-KDE-MinNumberOfUrls").unwrap_or(1),
        max: number(entry, "X-KDE-MaxNumberOfUrls").unwrap_or(usize::MAX),
        only: list(entry, "X-KDE-RequiredNumberOfUrls").iter().filter_map(|count| count.parse().ok()).collect(),
        ..Filter::default()
    };
    let group = localized(entry, "X-KDE-Submenu", lang);
    list(entry, "Actions")
        .iter()
        .filter_map(|id| {
            let section = groups.get(&format!("Desktop Action {id}"))?;
            if !usable(section) {
                return None;
            }
            let label = localized(section, "Name", lang)?;
            let icon =
                section.get("Icon").or_else(|| entry.get("Icon")).filter(|icon| !icon.trim().is_empty()).cloned();
            let codes = Codes::Entry { name: label.clone(), icon };
            Some(Action {
                label,
                group: group.clone(),
                profiles: vec![Profile {
                    filter: filter.clone(),
                    run: Run::Exec { line: section.get("Exec")?.clone(), desktop: file.to_path_buf(), codes },
                }],
            })
        })
        .collect()
}

fn selection_count(spec: &str, filter: &mut Filter) {
    let spec = spec.trim();
    let Some(count) = spec.get(1..).and_then(|n| n.trim().parse::<usize>().ok()) else {
        return;
    };
    match spec.chars().next() {
        Some('<') => filter.max = count.saturating_sub(1),
        Some('>') => filter.min = count + 1,
        Some('=') => (filter.min, filter.max) = (count, count),
        _ => {}
    }
}

fn file_manager_action(file: &Path, lang: Lang) -> Option<Action> {
    let text = std::fs::read_to_string(file).ok()?;
    let groups = parse_ini(&text);
    let entry = groups.get("Desktop Entry")?;
    let kind = entry.get("Type").map_or("Action", String::as_str);
    if kind != "Action"
        || !usable(entry)
        || flag(entry, "Enabled") == Some(false)
        || flag(entry, "TargetContext") == Some(false)
    {
        return None;
    }
    let profiles: Vec<Profile> = list(entry, "Profiles")
        .iter()
        .filter_map(|id| {
            let section = groups.get(&format!("X-Action-Profile {id}"))?;
            if !serves_files(section, &["Schemes"]) {
                return None;
            }
            let mut filter = Filter {
                match_case: flag(section, "Matchcase") != Some(false),
                names: list(section, "Basenames"),
                ..Filter::default()
            };
            let mimes = list(section, "MimeTypes");
            if !mimes.is_empty() {
                filter.mimes = mimes;
            }
            if let Some(spec) = section.get("SelectionCount") {
                selection_count(spec, &mut filter);
            }
            let line = section.get("Exec")?.clone();
            Some(Profile { filter, run: Run::Exec { line, desktop: file.to_path_buf(), codes: Codes::Action } })
        })
        .collect();
    if profiles.is_empty() {
        return None;
    }
    Some(Action { label: localized(entry, "Name", lang)?, group: None, profiles })
}

struct Archiver {
    name: String,
    script: PathBuf,
    mimes: Vec<String>,
}

fn archiver(roots: &Roots, script: PathBuf, lang: Lang) -> Option<Archiver> {
    let id = script.file_stem()?.to_string_lossy().into_owned();
    let text = roots
        .data
        .iter()
        .find_map(|dir| std::fs::read_to_string(dir.join(format!("applications/{id}.desktop"))).ok())?;
    let groups = parse_ini(&text);
    let entry = groups.get("Desktop Entry")?;
    if !usable(entry) {
        return None;
    }
    Some(Archiver { name: localized(entry, "Name", lang).unwrap_or(id), mimes: list(entry, "MimeType"), script })
}

fn archivers(roots: &Roots, lang: Lang) -> Vec<Action> {
    let mut found: Vec<Archiver> = Vec::new();
    for script in roots.taps.iter().flat_map(|dir| desktop_files(dir, "tap", 1)) {
        let known = found.iter().any(|archiver| archiver.script.file_name() == script.file_name());
        if !known && let Some(archiver) = archiver(roots, script, lang) {
            found.push(archiver);
        }
    }
    let several = found.len() > 1;
    found
        .into_iter()
        .flat_map(|archiver| {
            let group = several.then(|| archiver.name.clone());
            let extract = Filter { mimes: archiver.mimes.clone(), ..Filter::default() };
            let verbs = [
                (Key::Compress, "create", Filter::default()),
                (Key::ExtractHere, "extract-here", extract.clone()),
                (Key::ExtractTo, "extract-to", extract),
            ];
            verbs.into_iter().filter(|(_, _, filter)| !filter.mimes.is_empty()).map(move |(key, verb, filter)| Action {
                label: key.text(lang).to_string(),
                group: group.clone(),
                profiles: vec![Profile { filter, run: Run::Tap { script: archiver.script.clone(), verb } }],
            })
        })
        .collect()
}

impl Run {
    pub fn launch(&self, targets: &[Target], cwd: &Path) -> io::Result<()> {
        if let Self::SetDefault { mime, app } = self {
            return set_default_app(mime, app);
        }
        for argv in self.commands(targets, cwd)? {
            spawn(&argv, cwd)?;
        }
        Ok(())
    }

    pub fn commands(&self, targets: &[Target], cwd: &Path) -> io::Result<Vec<Vec<OsString>>> {
        match self {
            Self::SetDefault { .. } => Ok(Vec::new()),
            Self::Tap { script, verb } => {
                let mut argv: Vec<OsString> = vec!["sh".into(), script.into(), (*verb).into(), cwd.into()];
                argv.extend(targets.iter().map(|target| target.path.clone().into()));
                Ok(vec![argv])
            }
            Self::Exec { line, desktop, codes } => {
                let args = split_exec(line)
                    .filter(|args| !args.is_empty())
                    .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "malformed Exec"))?;
                let found: String = args.iter().flat_map(|arg| field_codes(arg)).collect();
                let (plural, singular) = match codes {
                    Codes::Entry { .. } => ("FU", "fu"),
                    Codes::Action => (PLURAL, SINGULAR),
                };
                let items: Vec<&Target> = targets.iter().collect();
                let runs: Vec<&[&Target]> =
                    if !found.contains(|c| plural.contains(c)) && found.contains(|c| singular.contains(c)) {
                        items.chunks(1).collect()
                    } else {
                        vec![&items[..]]
                    };
                Ok(runs.into_iter().map(|run| expand(&args, run, desktop, codes)).collect())
            }
        }
    }
}

fn spawn(command: &[OsString], cwd: &Path) -> io::Result<()> {
    let (program, args) =
        command.split_first().ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "empty command"))?;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

fn split_exec(line: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => current.push(chars.next()?),
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return None;
    }
    if started {
        args.push(current);
    }
    Some(args)
}

fn field_codes(arg: &str) -> Vec<char> {
    let mut codes = Vec::new();
    let mut chars = arg.chars();
    while let Some(c) = chars.next() {
        if c == '%'
            && let Some(code) = chars.next()
        {
            codes.push(code);
        }
    }
    codes
}

fn field(code: char, items: &[&Target], desktop: &Path, codes: &Codes) -> Vec<OsString> {
    let each = |value: &dyn Fn(&Target) -> OsString| -> Vec<OsString> {
        let values = items.iter().map(|target| value(target));
        if code.is_ascii_lowercase() { values.take(1).collect() } else { values.collect() }
    };
    if let Codes::Entry { name, icon } = codes {
        return match code {
            'f' | 'F' => each(&|target| target.path.clone().into()),
            'u' | 'U' => each(&|target| file_uri(&target.path).into()),
            'c' => vec![name.into()],
            'i' => icon.iter().flat_map(|icon| [OsString::from("--icon"), icon.into()]).collect(),
            'k' => vec![desktop.into()],
            _ => Vec::new(),
        };
    }
    match code.to_ascii_lowercase() {
        'f' => each(&|target| target.path.clone().into()),
        'u' => each(&|target| file_uri(&target.path).into()),
        'd' => each(&|target| target.path.parent().map(|parent| parent.as_os_str().to_owned()).unwrap_or_default()),
        'b' => each(&|target| target.path.file_name().map(ToOwned::to_owned).unwrap_or_default()),
        'w' => each(&|target| target.path.file_stem().map(ToOwned::to_owned).unwrap_or_default()),
        'x' => each(&|target| {
            target
                .path
                .extension()
                .map(|ext| {
                    let mut dotted = OsString::from(".");
                    dotted.push(ext);
                    dotted
                })
                .unwrap_or_default()
        }),
        'm' => each(&|target| target.mime.clone().into()),
        'c' => vec![items.len().to_string().into()],
        'k' => vec![desktop.into()],
        _ => Vec::new(),
    }
}

fn expand(words: &[String], items: &[&Target], desktop: &Path, codes: &Codes) -> Vec<OsString> {
    let mut argv = Vec::new();
    for arg in words {
        let mut chars = arg.chars();
        if let (Some('%'), Some(code), None) = (chars.next(), chars.next(), chars.next())
            && code != '%'
        {
            argv.extend(field(code, items, desktop, codes));
            continue;
        }
        let mut out = OsString::new();
        let mut chars = arg.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c.encode_utf8(&mut [0; 4]));
                continue;
            }
            match chars.next() {
                Some('%') => out.push("%"),
                Some(code) => {
                    let values = field(code, items, desktop, codes);
                    for (index, value) in values.iter().enumerate() {
                        if index > 0 {
                            out.push(" ");
                        }
                        out.push(value);
                    }
                }
                None => {}
            }
        }
        argv.push(out);
    }
    argv
}

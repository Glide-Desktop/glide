use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, mpsc},
};

use crate::icon::DesktopApp;

#[derive(Debug, Default)]
pub struct AppIndex {
    apps: Vec<DesktopApp>,
    by_id: HashMap<String, usize>,
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn exec_binary(command: &str) -> Option<&str> {
    let mut words = command.split_whitespace();
    let mut first = words.next()?;
    if basename(first) == "env" {
        first = words.find(|word| !word.contains('=') && !word.starts_with('-'))?;
    }
    Some(basename(first.trim_matches('"')))
}

impl AppIndex {
    #[must_use]
    pub fn new(apps: Vec<DesktopApp>) -> Self {
        let by_id = apps.iter().enumerate().map(|(index, app)| (app.id.to_lowercase(), index)).collect();
        Self { apps, by_id }
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&DesktopApp> {
        let key = key.trim();
        if key.is_empty() {
            return None;
        }
        let wanted = key.to_lowercase();
        if let Some(app) = self.by_id(&wanted) {
            return Some(app);
        }
        let short = wanted.rsplit('.').next().unwrap_or(&wanted);
        self.apps.iter().find(|app| {
            app.name.eq_ignore_ascii_case(key)
                || app.icon.eq_ignore_ascii_case(key)
                || app.id.to_lowercase().rsplit('.').next() == Some(short)
                || exec_binary(&app.command).is_some_and(|bin| bin.eq_ignore_ascii_case(key))
        })
    }

    #[must_use]
    pub fn name(&self, key: &str) -> Option<&str> {
        self.get(key).map(|app| app.name.as_str()).filter(|name| !name.trim().is_empty())
    }

    #[must_use]
    pub fn match_process(&self, exe: Option<&str>, argv0: Option<&str>) -> Option<&DesktopApp> {
        let names: Vec<&str> =
            [exe, argv0].into_iter().flatten().map(basename).filter(|name| !name.is_empty()).collect();
        if names.is_empty() {
            return None;
        }
        self.apps
            .iter()
            .find(|app| {
                exec_binary(&app.command).is_some_and(|bin| names.iter().any(|name| name.eq_ignore_ascii_case(bin)))
            })
            .or_else(|| names.iter().find_map(|name| self.by_id(&name.to_lowercase())))
    }

    fn by_id(&self, id: &str) -> Option<&DesktopApp> {
        self.by_id.get(id).and_then(|index| self.apps.get(*index))
    }
}

#[derive(Debug)]
pub enum Reply {
    Index(Arc<AppIndex>),
    Process { pid: i32, app: Option<String> },
}

fn read_process(pid: i32) -> (Option<String>, Option<String>) {
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok().and_then(|path| path.to_str().map(str::to_string));
    let argv0 = std::fs::read(format!("/proc/{pid}/cmdline"))
        .ok()
        .and_then(|bytes| {
            let first = bytes.split(|b| *b == 0).next()?.to_vec();
            String::from_utf8(first).ok()
        })
        .filter(|arg| !arg.is_empty());
    (exe, argv0)
}

pub fn start(sink: impl Fn(Reply) + Send + 'static) -> Option<mpsc::Sender<i32>> {
    let (tx, rx) = mpsc::channel::<i32>();
    let spawned = std::thread::Builder::new().name("glide-app-index".into()).spawn(move || {
        let index = Arc::new(AppIndex::new(crate::icon::list_applications()));
        sink(Reply::Index(index.clone()));
        while let Ok(pid) = rx.recv() {
            let (exe, argv0) = read_process(pid);
            let app = index.match_process(exe.as_deref(), argv0.as_deref()).map(|app| app.id.clone());
            sink(Reply::Process { pid, app });
        }
    });
    spawned.inspect_err(|err| tracing::warn!(?err, "failed to start the desktop entry index")).ok().map(|_| tx)
}

#[derive(Debug, Default)]
pub struct AppDirectory {
    index: Option<Arc<AppIndex>>,
    processes: HashMap<i32, Option<String>>,
    requested: HashSet<i32>,
    worker: Option<mpsc::Sender<i32>>,
}

impl AppDirectory {
    pub fn attach(&mut self, worker: mpsc::Sender<i32>) {
        self.worker = Some(worker);
    }

    #[must_use]
    pub fn index(&self) -> Option<&AppIndex> {
        self.index.as_deref()
    }

    pub fn apply(&mut self, reply: Reply) {
        match reply {
            Reply::Index(index) => self.index = Some(index),
            Reply::Process { pid, app } => {
                self.processes.insert(pid, app);
            }
        }
    }

    #[must_use]
    pub fn process_app(&self, pid: i32) -> Option<&str> {
        self.processes.get(&pid).and_then(|app| app.as_deref())
    }

    pub fn resolve(&mut self, pid: i32) {
        if pid <= 0 || !self.requested.insert(pid) {
            return;
        }
        if let Some(worker) = self.worker.as_ref()
            && worker.send(pid).is_err()
        {
            self.worker = None;
        }
    }

    pub fn forget_processes_except(&mut self, alive: &HashSet<i32>) {
        self.processes.retain(|pid, _| alive.contains(pid));
        self.requested.retain(|pid| alive.contains(pid));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, command: &str) -> DesktopApp {
        DesktopApp {
            id: id.to_string(),
            name: name.to_string(),
            command: command.to_string(),
            icon: id.to_string(),
            ..DesktopApp::default()
        }
    }

    fn index() -> AppIndex {
        AppIndex::new(vec![
            app("org.mozilla.firefox", "Firefox", "/usr/lib/firefox/firefox %u"),
            app("org.gnome.Nautilus", "Files", "nautilus --new-window"),
            app("steam", "Steam", "env GDK_SCALE=1 /usr/bin/steam-runtime %U"),
        ])
    }

    #[test]
    fn a_process_resolves_through_the_exec_basename() {
        let index = index();
        let found = index.match_process(Some("/usr/lib/firefox/firefox"), None);
        assert_eq!(found.map(|app| app.id.as_str()), Some("org.mozilla.firefox"));
        let found = index.match_process(None, Some("nautilus"));
        assert_eq!(found.map(|app| app.id.as_str()), Some("org.gnome.Nautilus"));
    }

    #[test]
    fn an_env_prefix_is_skipped_when_reading_the_exec_line() {
        let index = index();
        let found = index.match_process(Some("/usr/bin/steam-runtime"), None);
        assert_eq!(found.map(|app| app.id.as_str()), Some("steam"));
    }

    #[test]
    fn an_unknown_process_resolves_to_nothing() {
        assert!(index().match_process(Some("/usr/bin/sh"), Some("sh")).is_none());
        assert!(index().match_process(None, None).is_none());
    }

    #[test]
    fn names_are_looked_up_by_id_ignoring_case_and_reverse_dns_prefix() {
        let index = index();
        assert_eq!(index.name("ORG.GNOME.NAUTILUS"), Some("Files"));
        assert_eq!(index.name("firefox"), Some("Firefox"));
        assert_eq!(index.name(""), None);
        assert_eq!(index.name("org.example.Absent"), None);
    }

    #[test]
    fn a_process_is_requested_from_the_worker_only_once() {
        let (tx, rx) = mpsc::channel();
        let mut directory = AppDirectory::default();
        directory.attach(tx);
        directory.resolve(42);
        directory.resolve(42);
        directory.resolve(0);
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec![42]);
        directory.apply(Reply::Process { pid: 42, app: Some("steam".to_string()) });
        assert_eq!(directory.process_app(42), Some("steam"));
        directory.forget_processes_except(&HashSet::new());
        assert_eq!(directory.process_app(42), None);
    }
}

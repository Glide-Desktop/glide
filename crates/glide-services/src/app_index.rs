use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
};

use crate::icon::DesktopApp;

const SHARED_DIRS: [&str; 9] = [
    "/opt",
    "/usr/lib",
    "/usr/lib64",
    "/usr/libexec",
    "/usr/local/lib",
    "/usr/share",
    "/usr/lib/x86_64-linux-gnu",
    "/usr/lib/aarch64-linux-gnu",
    "/",
];

const INTERPRETERS: [&str; 30] = [
    "sh",
    "bash",
    "dash",
    "zsh",
    "fish",
    "ksh",
    "mksh",
    "csh",
    "tcsh",
    "env",
    "python",
    "pypy",
    "perl",
    "ruby",
    "node",
    "nodejs",
    "deno",
    "bun",
    "java",
    "gjs",
    "lua",
    "luajit",
    "php",
    "tclsh",
    "wish",
    "wine",
    "mono",
    "dotnet",
    "electron",
    "osascript",
];

#[derive(Debug, Default)]
pub struct AppIndex {
    apps: Vec<DesktopApp>,
    by_id: HashMap<String, usize>,
    binaries: Vec<Option<String>>,
    targets: Vec<Option<PathBuf>>,
    shared: HashSet<PathBuf>,
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn is_interpreter(name: &str) -> bool {
    let name = basename(name).to_ascii_lowercase();
    let stem = name.trim_end_matches(|ch: char| ch.is_ascii_digit() || ch == '.' || ch == '-');
    INTERPRETERS.contains(&stem)
}

fn exec_words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote = None;
    let mut chars = command.chars();
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some(open), ch) if ch == open => quote = None,
            (Some('"'), '\\') => current.extend(chars.next()),
            (Some(_), ch) => current.push(ch),
            (None, '"' | '\'') => {
                quote = Some(ch);
                started = true;
            }
            (None, ch) if ch.is_whitespace() => {
                if std::mem::take(&mut started) {
                    words.push(std::mem::take(&mut current));
                }
            }
            (None, ch) => {
                current.push(ch);
                started = true;
            }
        }
    }
    if started {
        words.push(current);
    }
    words
}

fn exec_program(command: &str) -> Option<String> {
    let mut words = exec_words(command).into_iter();
    let mut first = words.next()?;
    if basename(&first) == "env" {
        first = loop {
            let word = words.next()?;
            match word.as_str() {
                "-u" | "--unset" | "-C" | "--chdir" => {
                    words.next();
                }
                "-S" | "--split-string" => return words.next().and_then(|line| exec_program(&line)),
                _ if word.starts_with('-') || word.contains('=') => {}
                _ => break word,
            }
        };
    }
    Some(first)
}

fn exec_binary(command: &str) -> Option<String> {
    exec_program(command).map(|program| basename(&program).to_string()).filter(|name| !is_interpreter(name))
}

fn exec_target(command: &str, search: &[PathBuf]) -> Option<PathBuf> {
    let program = exec_program(command).filter(|program| !is_interpreter(program))?;
    let found = if program.contains('/') {
        Some(PathBuf::from(program))
    } else {
        search.iter().map(|dir| dir.join(&program)).find(|path| path.is_file())
    }?;
    std::fs::canonicalize(found).ok().filter(|target| !target.to_str().is_some_and(is_interpreter))
}

fn extra_words(command: &str) -> usize {
    command.split_whitespace().filter(|word| !word.starts_with('%')).count()
}

impl AppIndex {
    #[must_use]
    pub fn new(apps: Vec<DesktopApp>) -> Self {
        let by_id = apps.iter().enumerate().map(|(index, app)| (app.id.to_lowercase(), index)).collect();
        let binaries = apps.iter().map(|app| exec_binary(&app.command)).collect();
        let targets = vec![None; apps.len()];
        Self { apps, by_id, binaries, targets, shared: HashSet::new() }
    }

    #[must_use]
    pub fn located(apps: Vec<DesktopApp>, search: &OsStr) -> Self {
        let search: Vec<PathBuf> = std::env::split_paths(search).collect();
        let shared: HashSet<PathBuf> = search
            .iter()
            .filter_map(|dir| std::fs::canonicalize(dir).ok())
            .chain(SHARED_DIRS.iter().map(PathBuf::from))
            .collect();
        let targets = apps.iter().map(|app| exec_target(&app.command, &search)).collect();
        Self { targets, shared, ..Self::new(apps) }
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
        self.apps.iter().zip(&self.binaries).find_map(|(app, binary)| {
            let found = app.name.eq_ignore_ascii_case(key)
                || app.icon.eq_ignore_ascii_case(key)
                || app.id.to_lowercase().rsplit('.').next() == Some(short)
                || binary.as_deref().is_some_and(|bin| bin.eq_ignore_ascii_case(key));
            found.then_some(app)
        })
    }

    #[must_use]
    pub fn name(&self, key: &str) -> Option<&str> {
        self.get(key).map(|app| app.name.as_str()).filter(|name| !name.trim().is_empty())
    }

    #[must_use]
    pub fn match_process(&self, exe: Option<&str>, argv0: Option<&str>) -> Option<&DesktopApp> {
        let names: Vec<&str> = [exe, argv0]
            .into_iter()
            .flatten()
            .map(basename)
            .filter(|name| !name.is_empty() && !is_interpreter(name))
            .collect();
        if names.is_empty() {
            return None;
        }
        self.by_binary(&names, exe)
            .or_else(|| self.plainest(|target| Some(target) == exe.map(Path::new)))
            .or_else(|| names.iter().find_map(|name| self.by_id(&name.to_lowercase())))
            .or_else(|| self.beside(Path::new(exe?)))
    }

    fn by_binary(&self, names: &[&str], exe: Option<&str>) -> Option<&DesktopApp> {
        let matched: Vec<usize> = (0..self.apps.len())
            .filter(|&index| {
                self.binaries[index]
                    .as_deref()
                    .is_some_and(|bin| names.iter().any(|name| name.eq_ignore_ascii_case(bin)))
            })
            .collect();
        let launches = |index: usize| match &self.targets[index] {
            Some(target) => target.to_string_lossy().into_owned(),
            None => exec_program(&self.apps[index].command).unwrap_or_default(),
        };
        let distinct: HashSet<String> = matched.iter().map(|&index| launches(index)).collect();
        let chosen: Vec<usize> = match distinct.len() {
            0 => return None,
            1 => matched,
            _ => {
                let exe = exe.map(Path::new)?;
                let exact: Vec<usize> =
                    matched.into_iter().filter(|&index| self.targets[index].as_deref() == Some(exe)).collect();
                if exact.is_empty() {
                    return None;
                }
                exact
            }
        };
        chosen
            .into_iter()
            .map(|index| &self.apps[index])
            .min_by_key(|app| (extra_words(&app.command), app.command.len()))
    }

    fn beside(&self, exe: &Path) -> Option<&DesktopApp> {
        let dir = exe.parent()?;
        if self.shared.contains(dir) {
            return None;
        }
        let mut targets = self.targets.iter().flatten().filter(|target| target.parent() == Some(dir));
        let first = targets.next()?;
        if targets.any(|target| target != first) {
            return None;
        }
        self.plainest(|target| target == first)
    }

    fn plainest(&self, wanted: impl Fn(&Path) -> bool) -> Option<&DesktopApp> {
        self.apps
            .iter()
            .zip(&self.targets)
            .filter(|(_, target)| target.as_deref().is_some_and(&wanted))
            .min_by_key(|(app, _)| (extra_words(&app.command), app.command.len()))
            .map(|(app, _)| app)
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

#[derive(Debug)]
pub enum Job {
    Pid(i32),
    Rebuild(Vec<DesktopApp>),
}

pub fn start(sink: impl Fn(Reply) + Send + 'static) -> Option<mpsc::Sender<Job>> {
    let (tx, rx) = mpsc::channel::<Job>();
    let spawned = std::thread::Builder::new().name("glide-app-index".into()).spawn(move || {
        let search = std::env::var_os("PATH").unwrap_or_default();
        let mut index = Arc::new(AppIndex::located(crate::icon::list_applications(), &search));
        sink(Reply::Index(index.clone()));
        while let Ok(job) = rx.recv() {
            match job {
                Job::Pid(pid) => {
                    let (exe, argv0) = read_process(pid);
                    let app = index.match_process(exe.as_deref(), argv0.as_deref()).map(|app| app.id.clone());
                    sink(Reply::Process { pid, app });
                }
                Job::Rebuild(apps) => {
                    let search = std::env::var_os("PATH").unwrap_or_default();
                    index = Arc::new(AppIndex::located(apps, &search));
                    sink(Reply::Index(index.clone()));
                }
            }
        }
    });
    spawned.inspect_err(|err| tracing::warn!(?err, "failed to start the desktop entry index")).ok().map(|_| tx)
}

#[derive(Debug, Default)]
pub struct AppDirectory {
    index: Option<Arc<AppIndex>>,
    processes: HashMap<i32, Option<String>>,
    requested: HashSet<i32>,
    worker: Option<mpsc::Sender<Job>>,
}

impl AppDirectory {
    pub fn attach(&mut self, worker: mpsc::Sender<Job>) {
        self.worker = Some(worker);
    }

    #[must_use]
    pub fn index(&self) -> Option<&AppIndex> {
        self.index.as_deref()
    }

    pub fn apply(&mut self, reply: Reply) {
        match reply {
            Reply::Index(index) => {
                self.index = Some(index);
                self.processes.clear();
                self.requested.clear();
            }
            Reply::Process { pid, app } => {
                self.processes.insert(pid, app);
            }
        }
    }

    pub fn rebuild(&mut self, apps: Vec<DesktopApp>) {
        if self.index.as_ref().is_some_and(|index| index.apps == apps) {
            return;
        }
        if let Some(worker) = self.worker.as_ref()
            && worker.send(Job::Rebuild(apps)).is_err()
        {
            self.worker = None;
        }
    }

    #[must_use]
    pub fn process_app(&self, pid: i32) -> Option<&str> {
        self.processes.get(&pid).and_then(|app| app.as_deref())
    }

    pub fn resolve(&mut self, pid: i32) {
        if pid <= 0 || self.requested.contains(&pid) {
            return;
        }
        if let Some(worker) = self.worker.as_ref() {
            if worker.send(Job::Pid(pid)).is_err() {
                self.worker = None;
            } else {
                self.requested.insert(pid);
            }
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

    #[test]
    fn a_program_in_either_kind_of_quotes_resolves_to_its_bare_path() {
        assert_eq!(exec_program("'/opt/app/run' --flag").as_deref(), Some("/opt/app/run"));
        assert_eq!(exec_program("\"/opt/app/run\" --flag").as_deref(), Some("/opt/app/run"));
        assert_eq!(exec_binary("env FOO=1 '/opt/app/run'").as_deref(), Some("run"));
    }

    #[test]
    fn env_options_and_quoted_paths_with_spaces_are_read_whole() {
        assert_eq!(exec_program("\"/opt/My App/run\" %U").as_deref(), Some("/opt/My App/run"));
        assert_eq!(exec_program("env -u GTK_IM_MODULE /usr/bin/app %F").as_deref(), Some("/usr/bin/app"));
        assert_eq!(exec_program("env --chdir /tmp A=1 app").as_deref(), Some("app"));
        assert_eq!(exec_program("env -S \"B=2 tool --x\"").as_deref(), Some("tool"));
        assert_eq!(exec_program("env -u ONLY"), None);
    }

    #[test]
    fn interpreters_never_name_an_app() {
        for program in ["sh", "/bin/bash", "python3", "/usr/bin/python3.12", "perl", "node", "java", "env", "wine64"] {
            assert!(is_interpreter(program), "{program}");
        }
        for program in ["firefox", "shotwell", "nautilus", "javaws-launcher", "steam-runtime"] {
            assert!(!is_interpreter(program), "{program}");
        }
        let index = AppIndex::new(vec![
            app("org.example.Tool", "Example Tool", "python3 /opt/tool/main.py"),
            app("org.example.Script", "Script Runner", "env PYTHONPATH=/opt sh -c 'exec tool'"),
            app("org.example.Jar", "Jar App", "java -jar /opt/app.jar"),
        ]);
        assert!(index.match_process(Some("/usr/bin/python3.12"), Some("python3")).is_none());
        assert!(index.match_process(Some("/usr/bin/bash"), Some("sh")).is_none());
        assert!(index.match_process(Some("/usr/lib/jvm/java-21/bin/java"), None).is_none());
        assert_eq!(index.get("python3"), None, "a lookup by binary skips interpreters too");
    }

    #[test]
    fn a_basename_shared_by_different_programs_names_none_of_them() {
        let index = AppIndex::new(vec![
            app("org.example.Stable", "Editor", "/usr/bin/editor %F"),
            app("org.example.Nightly", "Editor Nightly", "/opt/nightly/editor %F"),
            app("org.example.Plain", "Viewer", "viewer"),
            app("org.example.Window", "Viewer (new window)", "viewer --new-window"),
        ]);
        assert!(index.match_process(Some("/usr/local/bin/editor"), None).is_none());
        let found = index.match_process(Some("/usr/bin/viewer"), None);
        assert_eq!(found.map(|app| app.name.as_str()), Some("Viewer"), "one program, the plainest entry");
    }

    #[test]
    fn the_exact_target_decides_between_programs_that_share_a_basename() {
        let root = std::env::temp_dir().join(format!("glide-app-index-twins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a")).expect("a");
        std::fs::create_dir_all(root.join("b")).expect("b");
        std::fs::write(root.join("a/editor"), b"").expect("a editor");
        std::fs::write(root.join("b/editor"), b"").expect("b editor");
        let root = std::fs::canonicalize(&root).expect("canonical root");
        let (a, b) = (root.join("a/editor"), root.join("b/editor"));
        let index = AppIndex::located(
            vec![
                app("stable", "Editor", &format!("{} %F", a.display())),
                app("nightly", "Editor Nightly", &format!("{} %F", b.display())),
            ],
            OsStr::new(""),
        );
        assert_eq!(index.match_process(b.to_str(), None).map(|app| app.id.as_str()), Some("nightly"));
        assert_eq!(index.match_process(None, Some("editor")), None, "argv0 alone cannot tell them apart");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_rebuild_is_sent_only_for_a_changed_list_and_a_new_index_forgets_resolved_processes() {
        let (tx, rx) = mpsc::channel();
        let mut directory = AppDirectory::default();
        directory.attach(tx);
        let apps = vec![app("a", "A", "a")];
        directory.apply(Reply::Index(Arc::new(AppIndex::new(apps.clone()))));
        directory.rebuild(apps.clone());
        assert!(rx.try_recv().is_err(), "the same list is not rebuilt");
        directory.resolve(7);
        directory.apply(Reply::Process { pid: 7, app: Some("a".into()) });
        let more = vec![app("a", "A", "a"), app("b", "B", "b")];
        directory.rebuild(more.clone());
        let jobs: Vec<Job> = rx.try_iter().collect();
        assert!(matches!(jobs.as_slice(), [Job::Pid(7), Job::Rebuild(list)] if *list == more), "{jobs:?}");
        directory.apply(Reply::Index(Arc::new(AppIndex::new(more))));
        assert_eq!(directory.process_app(7), None);
        directory.resolve(7);
        assert!(matches!(rx.try_recv(), Ok(Job::Pid(7))), "the process is resolved again against the new index");
    }

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
    fn a_helper_binary_beside_the_exec_target_resolves_to_the_plainest_entry_of_its_folder() {
        let root = std::env::temp_dir().join(format!("glide-app-index-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let program = root.join("lib/libreoffice/program");
        std::fs::create_dir_all(&program).expect("program dir");
        std::fs::create_dir_all(root.join("bin")).expect("bin dir");
        std::fs::write(program.join("soffice"), b"#!/bin/sh\n").expect("launcher script");
        std::fs::write(program.join("soffice.bin"), b"").expect("real binary");
        std::os::unix::fs::symlink("../lib/libreoffice/program/soffice", root.join("bin/libreoffice")).expect("link");
        std::fs::write(root.join("bin/sh"), b"").expect("shell");

        let index = AppIndex::located(
            vec![
                app("libreoffice-calc", "LibreOffice Calc", "libreoffice --calc %U"),
                app("libreoffice-startcenter", "LibreOffice", "libreoffice %U"),
                app("libreoffice-writer", "LibreOffice Writer", "libreoffice --writer %U"),
                app("shell", "Shell", "sh"),
            ],
            root.join("bin").as_os_str(),
        );
        let exe = std::fs::canonicalize(program.join("soffice.bin")).expect("canonical exe");
        let found = index.match_process(exe.to_str(), None);
        assert_eq!(found.map(|app| app.name.as_str()), Some("LibreOffice"));
        let stranger = std::fs::canonicalize(root.join("bin")).expect("bin").join("python3");
        assert!(index.match_process(stranger.to_str(), None).is_none(), "a folder on PATH names no app");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_toolbox_folder_resolves_only_exact_targets_because_its_programs_are_different_apps() {
        let root = std::env::temp_dir().join(format!("glide-app-index-qt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let tools = root.join("lib/qt6/bin");
        std::fs::create_dir_all(&tools).expect("tools dir");
        std::fs::create_dir_all(root.join("bin")).expect("bin dir");
        for (tool, link) in [("assistant", "assistant6"), ("designer", "designer6"), ("lupdate", "")] {
            std::fs::write(tools.join(tool), b"").expect("tool");
            if !link.is_empty() {
                std::os::unix::fs::symlink(format!("../lib/qt6/bin/{tool}"), root.join("bin").join(link))
                    .expect("link");
            }
        }

        let index = AppIndex::located(
            vec![
                app("assistant", "Qt Assistant", "assistant6"),
                app("designer", "Qt Widgets Designer", "designer6 %F"),
            ],
            root.join("bin").as_os_str(),
        );
        let tools = std::fs::canonicalize(&tools).expect("canonical tools");
        let named = |tool: &str| index.match_process(tools.join(tool).to_str(), None).map(|app| app.name.clone());
        assert_eq!(named("designer").as_deref(), Some("Qt Widgets Designer"));
        assert_eq!(named("assistant").as_deref(), Some("Qt Assistant"));
        assert_eq!(named("lupdate"), None, "a folder of distinct programs lends none of their names");
        let _ = std::fs::remove_dir_all(root);
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
        assert!(matches!(rx.try_iter().collect::<Vec<_>>().as_slice(), [Job::Pid(42)]));
        directory.apply(Reply::Process { pid: 42, app: Some("steam".to_string()) });
        assert_eq!(directory.process_app(42), Some("steam"));
        directory.forget_processes_except(&HashSet::new());
        assert_eq!(directory.process_app(42), None);
    }

    #[test]
    fn requests_made_before_a_worker_exists_can_be_retried_after_attach() {
        let mut directory = AppDirectory::default();
        directory.resolve(42);
        let (tx, rx) = mpsc::channel();
        directory.attach(tx);
        directory.resolve(42);
        assert!(matches!(rx.try_recv(), Ok(Job::Pid(42))));
        drop(rx);
        directory.resolve(43);
        let (tx, rx) = mpsc::channel();
        directory.attach(tx);
        directory.resolve(43);
        assert!(matches!(rx.try_recv(), Ok(Job::Pid(43))));
    }
}

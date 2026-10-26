use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use alacritty_terminal::event::{Event, EventListener, Notify, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Msg, Notifier};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty::{self, Options, Shell};
use glide_sdk::widget::cells::Grid;
use iced::futures::channel::mpsc::{self, UnboundedSender};
use iced::futures::{SinkExt, Stream, StreamExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(pub u64);

#[derive(Debug, Clone)]
pub struct Bus(pub(crate) UnboundedSender<(PaneId, Event)>);

#[derive(Clone)]
pub struct Listener {
    pane: PaneId,
    bus: Bus,
    revision: Arc<AtomicU64>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        if matches!(event, Event::Wakeup) {
            self.revision.fetch_add(1, Ordering::Release);
        }
        let _ = self.bus.0.unbounded_send((self.pane, event));
    }
}

#[derive(Debug, Clone)]
pub enum Feed {
    Connected(Bus),
    Events(Vec<(PaneId, Event)>),
}

pub fn feed() -> impl Stream<Item = Feed> {
    iced::stream::channel(16, async |mut output| {
        let (sender, mut receiver) = mpsc::unbounded();
        if output.send(Feed::Connected(Bus(sender))).await.is_err() {
            return;
        }
        while let Some(first) = receiver.next().await {
            let mut batch = vec![first];
            while let Ok(next) = receiver.try_recv() {
                batch.push(next);
            }
            if output.send(Feed::Events(batch)).await.is_err() {
                return;
            }
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cells {
    pub columns: usize,
    pub lines: usize,
}

impl From<Grid> for Cells {
    fn from(Grid { columns, lines }: Grid) -> Self {
        Self { columns, lines }
    }
}

impl From<Cells> for Grid {
    fn from(Cells { columns, lines }: Cells) -> Self {
        Self { columns, lines }
    }
}

impl Dimensions for Cells {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Context {
    #[default]
    Local,
    Elevated,
    Remote {
        login: Option<String>,
        host: String,
    },
}

impl Context {
    #[must_use]
    pub fn root(&self) -> bool {
        match self {
            Self::Local => false,
            Self::Elevated => true,
            Self::Remote { login, .. } => login.as_deref() == Some("root"),
        }
    }

    #[must_use]
    pub fn address(&self) -> Option<String> {
        let Self::Remote { login, host } = self else { return None };
        Some(login.as_ref().map_or_else(|| host.clone(), |login| format!("{login}@{host}")))
    }

    #[must_use]
    pub fn short_address(&self) -> Option<String> {
        let Self::Remote { login, host } = self else { return None };
        let numeric = host.contains(':') || host.chars().all(|c| c.is_ascii_digit() || c == '.');
        let host = if numeric { host.as_str() } else { host.split('.').next().unwrap_or(host) };
        Some(login.as_ref().map_or_else(|| host.to_string(), |login| format!("{login}@{host}")))
    }
}

const ELEVATORS: [&str; 6] = ["sudo", "doas", "su", "run0", "pkexec", "sudo-rs"];
const REMOTES: [&str; 3] = ["ssh", "mosh", "autossh"];
const SSH_VALUED: &str = "bceilmopBDEFIJLOPQRSwW";

#[must_use]
fn remote(args: &[&str]) -> Option<Context> {
    let mut login = None;
    let mut args = args.iter();
    let destination = loop {
        let arg = *args.next()?;
        if arg == "--" {
            break *args.next()?;
        }
        if arg.starts_with("--") {
            continue;
        }
        let Some(flags) = arg.strip_prefix('-').filter(|flags| !flags.is_empty()) else { break arg };
        if let Some((at, flag)) = flags.char_indices().find(|(_, flag)| SSH_VALUED.contains(*flag)) {
            let rest = &flags[at + flag.len_utf8()..];
            let value = if rest.is_empty() { args.next().copied() } else { Some(rest) };
            if flag == 'l' {
                login = value;
            }
        }
    };
    let destination = destination.strip_prefix("ssh://").map_or(destination, |rest| {
        rest.rsplit_once(':').filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit())).map_or(rest, |(at, _)| at)
    });
    let (user, host) = destination.rsplit_once('@').map_or((None, destination), |(user, host)| (Some(user), host));
    (!host.is_empty()).then(|| Context::Remote {
        login: user.or(login).filter(|login| !login.is_empty()).map(str::to_string),
        host: host.to_string(),
    })
}

#[must_use]
fn effective_uid(status: &str) -> Option<u32> {
    status.lines().find_map(|line| line.strip_prefix("Uid:")?.split_whitespace().nth(1)?.parse().ok())
}

fn context(pid: i32, process: &str) -> Context {
    let name = process.trim_start_matches('-');
    if REMOTES.contains(&name)
        && let Ok(cmdline) = std::fs::read(format!("/proc/{pid}/cmdline"))
    {
        let cmdline = String::from_utf8_lossy(&cmdline);
        let args: Vec<&str> = cmdline.split('\0').skip(1).filter(|arg| !arg.is_empty()).collect();
        if let Some(remote) = remote(&args) {
            return remote;
        }
    }
    let uid = std::fs::read_to_string(format!("/proc/{pid}/status")).ok().and_then(|status| effective_uid(&status));
    if uid == Some(0) || ELEVATORS.contains(&name) { Context::Elevated } else { Context::Local }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Launch {
    pub command: Option<(String, Vec<String>)>,
    pub directory: Option<PathBuf>,
}

pub struct Pane {
    pub id: PaneId,
    pub term: Arc<FairMutex<Term<Listener>>>,
    notifier: Notifier,
    master: File,
    shell: i32,
    shell_name: Option<String>,
    pub revision: Arc<AtomicU64>,
    pub cells: Cells,
    pub cell_size: iced::Size,
    pub title: Option<String>,
    pub process: String,
    pub directory: Option<PathBuf>,
    pub busy: bool,
    pub context: Context,
    pub exit: Option<ExitStatus>,
}

const DEFAULT_CELLS: Cells = Cells { columns: 80, lines: 24 };

fn window_size(cells: Cells, cell: iced::Size) -> WindowSize {
    let px = |value: f32| value.round().clamp(1.0, f32::from(u16::MAX)) as u16;
    WindowSize {
        num_lines: cells.lines.min(usize::from(u16::MAX)) as u16,
        num_cols: cells.columns.min(usize::from(u16::MAX)) as u16,
        cell_width: px(cell.width),
        cell_height: px(cell.height),
    }
}

impl Pane {
    pub fn spawn(id: PaneId, bus: &Bus, launch: &Launch, config: Config) -> io::Result<Self> {
        let env = HashMap::from([
            ("TERM".to_string(), "xterm-256color".to_string()),
            ("COLORTERM".to_string(), "truecolor".to_string()),
            ("TERM_PROGRAM".to_string(), "glide-terminal".to_string()),
            ("TERM_PROGRAM_VERSION".to_string(), env!("CARGO_PKG_VERSION").to_string()),
        ]);
        let options = Options {
            shell: launch.command.clone().map(|(program, args)| Shell::new(program, args)),
            working_directory: launch
                .directory
                .clone()
                .filter(|directory| directory.is_dir())
                .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from).filter(|home| home.is_dir())),
            drain_on_exit: true,
            env,
        };
        let cell_size = iced::Size::new(8.0, 16.0);
        let pty = tty::new(&options, window_size(DEFAULT_CELLS, cell_size), id.0)?;
        let master = pty.file().try_clone()?;
        let shell = i32::try_from(pty.child().id()).unwrap_or(-1);
        let revision = Arc::new(AtomicU64::new(0));
        let listener = Listener { pane: id, bus: bus.clone(), revision: Arc::clone(&revision) };
        let term = Arc::new(FairMutex::new(Term::new(config, &DEFAULT_CELLS, listener.clone())));
        let event_loop = EventLoop::new(Arc::clone(&term), listener, pty, true, false)?;
        let notifier = Notifier(event_loop.channel());
        drop(event_loop.spawn());
        let mut pane = Self {
            id,
            term,
            notifier,
            master,
            shell,
            shell_name: shell_name(launch),
            revision,
            cells: DEFAULT_CELLS,
            cell_size,
            title: None,
            process: String::new(),
            directory: launch.directory.clone(),
            busy: false,
            context: Context::Local,
            exit: None,
        };
        pane.refresh();
        Ok(pane)
    }

    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        if self.exit.is_none() {
            self.notifier.notify(bytes);
        }
    }

    pub fn resize(&mut self, cells: Cells, cell_size: iced::Size) {
        if cells == self.cells && cell_size == self.cell_size {
            return;
        }
        self.cells = cells;
        self.cell_size = cell_size;
        let mut term = self.term.lock();
        term.resize(cells);
        self.revision.fetch_add(1, Ordering::Release);
        drop(term);
        self.notifier.on_resize(window_size(cells, cell_size));
    }

    #[must_use]
    pub fn window_size(&self) -> WindowSize {
        window_size(self.cells, self.cell_size)
    }

    fn foreground(&self) -> Option<i32> {
        rustix::termios::tcgetpgrp(&self.master).ok().map(|pid| pid.as_raw_nonzero().get())
    }

    pub fn refresh(&mut self) {
        if self.exit.is_some() {
            self.busy = false;
            self.context = Context::Local;
            return;
        }
        let pid = self.foreground().unwrap_or(self.shell);
        if let Ok(name) = std::fs::read_to_string(format!("/proc/{pid}/comm")) {
            self.process = name.trim().to_string();
        }
        if let Ok(directory) = std::fs::read_link(format!("/proc/{}/cwd", self.shell)) {
            self.directory = Some(directory);
        }
        self.busy = running_program(pid, self.shell, self.shell_name.as_deref(), &self.process);
        self.context = context(pid, &self.process);
    }
}

fn program_name(program: &str) -> Option<String> {
    Some(std::path::Path::new(program).file_name()?.to_str()?.to_string())
}

fn passwd_shell() -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata("/proc/self").ok()?.uid().to_string();
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|line| {
        let fields: Vec<&str> = line.split(':').collect();
        (fields.len() >= 7 && fields[2] == uid).then(|| fields[6].to_string())
    })
}

fn shell_name(launch: &Launch) -> Option<String> {
    let login_shell = std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .or_else(passwd_shell)
        .and_then(|shell| program_name(&shell));
    let Some((program, _)) = launch.command.as_ref() else {
        return login_shell;
    };
    let name = program_name(program)?;
    let known = matches!(
        name.as_str(),
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "dash"
            | "ksh"
            | "mksh"
            | "nu"
            | "tcsh"
            | "csh"
            | "elvish"
            | "xonsh"
            | "pwsh"
            | "ion"
            | "osh"
            | "ysh"
            | "yash"
            | "rc"
            | "es"
            | "oksh"
            | "loksh"
            | "murex"
    );
    (known || login_shell.as_deref() == Some(name.as_str())).then_some(name)
}

fn running_program(pid: i32, child: i32, shell: Option<&str>, process: &str) -> bool {
    pid != child || shell.is_none_or(|shell| process.trim_start_matches('-') != shell)
}

impl Drop for Pane {
    fn drop(&mut self) {
        let _ = self.notifier.0.send(Msg::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::index::{Column, Line, Point};
    use std::time::{Duration, Instant};

    #[test]
    fn explicit_programs_and_exec_replacements_are_busy_but_an_idle_shell_is_not() {
        assert!(running_program(12, 12, None, "vim"));
        assert!(running_program(12, 12, Some("bash"), "vim"));
        assert!(running_program(13, 12, Some("bash"), "sleep"));
        assert!(!running_program(12, 12, Some("bash"), "bash"));
        assert!(!running_program(12, 12, Some("bash"), "-bash"));
    }

    fn host(args: &[&str]) -> Option<(Option<String>, String)> {
        match remote(args)? {
            Context::Remote { login, host } => Some((login, host)),
            _ => None,
        }
    }

    #[test]
    fn the_ssh_destination_is_the_first_argument_no_option_takes() {
        let pair = |login: Option<&str>, host: &str| Some((login.map(str::to_string), host.to_string()));
        assert_eq!(host(&["artem@prod-01.glide.lan"]), pair(Some("artem"), "prod-01.glide.lan"));
        assert_eq!(host(&["-p", "2222", "-i", "~/.ssh/id", "db"]), pair(None, "db"));
        assert_eq!(host(&["-vp2222", "-A", "db", "uptime"]), pair(None, "db"));
        assert_eq!(host(&["-l", "root", "db"]), pair(Some("root"), "db"));
        assert_eq!(host(&["-oProxyJump=bastion", "me@db"]), pair(Some("me"), "db"));
        assert_eq!(host(&["ssh://ops@10.0.0.7:2200"]), pair(Some("ops"), "10.0.0.7"));
        assert_eq!(host(&["--", "-weird"]), pair(None, "-weird"));
        assert_eq!(host(&["--ssh=ssh -p 22", "me@box"]), pair(Some("me"), "box"));
        assert_eq!(host(&["-V"]), None);
        assert_eq!(host(&["-p"]), None);
        assert_eq!(host(&["user@"]), None);
    }

    #[test]
    fn remote_addresses_shorten_names_but_keep_numeric_hosts_whole() {
        let remote = |login: Option<&str>, host: &str| Context::Remote {
            login: login.map(str::to_string),
            host: host.to_string(),
        };
        assert_eq!(remote(Some("artem"), "prod-01.glide.lan").short_address().as_deref(), Some("artem@prod-01"));
        assert_eq!(remote(None, "10.0.0.7").short_address().as_deref(), Some("10.0.0.7"));
        assert_eq!(remote(None, "fe80::1").short_address().as_deref(), Some("fe80::1"));
        assert_eq!(remote(Some("artem"), "prod-01.glide.lan").address().as_deref(), Some("artem@prod-01.glide.lan"));
        assert!(remote(Some("root"), "db").root());
        assert!(!remote(Some("artem"), "db").root());
        assert!(Context::Elevated.root());
        assert_eq!(Context::Local.address(), None);
    }

    #[test]
    fn the_effective_uid_is_the_second_field_of_the_uid_line() {
        let status = "Name:\tsudo\nUmask:\t0022\nUid:\t1000\t0\t0\t0\nGid:\t1000\t1000\t1000\t1000\n";
        assert_eq!(effective_uid(status), Some(0));
        assert_eq!(effective_uid("Uid:\t1000\t1000\t1000\t1000\n"), Some(1000));
        assert_eq!(effective_uid("Name:\tx\n"), None);
    }

    #[test]
    fn a_program_named_like_ssh_or_sudo_marks_its_pane() {
        let dir = std::env::temp_dir().join(format!("glide-terminal-context-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let seen = |name: &str, args: &[&str]| {
            let link = dir.join(name);
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink("/bin/sh", &link).expect("link");
            let mut child = std::process::Command::new(&link).args(args).spawn().expect("spawn");
            let pid = i32::try_from(child.id()).expect("pid");
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut seen = None;
            while seen.is_none() && Instant::now() < deadline {
                let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
                let started = std::fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|args| !args.is_empty());
                if comm.trim() == name && started {
                    seen = Some(context(pid, name));
                } else {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            let _ = child.kill();
            let _ = child.wait();
            seen.expect("the program started under its own name")
        };
        let ssh = seen("ssh", &["-c", "sleep 5; :", "artem@prod-01"]);
        assert_eq!(ssh.address().as_deref(), Some("artem@prod-01"));
        assert_eq!(seen("sudo", &["-c", "sleep 5; :"]), Context::Elevated);
        assert_eq!(seen("plain", &["-c", "sleep 5; :"]), Context::Local);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_command_runs_in_its_directory_and_its_output_reaches_the_grid() {
        let (sender, mut receiver) = mpsc::unbounded();
        let launch = Launch {
            command: Some(("sh".into(), vec!["-c".into(), "printf 'glide\\n'; pwd".into()])),
            directory: Some(PathBuf::from("/tmp")),
        };
        let pane = Pane::spawn(PaneId(7), &Bus(sender), &launch, Config::default()).expect("sh starts");
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut exit = None;
        while exit.is_none() && Instant::now() < deadline {
            match receiver.try_recv() {
                Ok((id, Event::ChildExit(status))) => exit = Some((id, status)),
                Ok(_) => {}
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        let (id, status) = exit.expect("the child exits");
        assert_eq!(id, PaneId(7));
        assert!(status.success());
        let text = pane.term.lock().bounds_to_string(Point::new(Line(0), Column(0)), Point::new(Line(1), Column(79)));
        assert!(text.starts_with("glide\n/tmp"), "{text:?}");
    }
}

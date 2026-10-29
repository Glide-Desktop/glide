use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use alacritty_terminal::event::Event as TermEvent;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::term::{ClipboardType, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use glide_sdk::Paint;
use glide_sdk::anim::{Lingering, Reveal};
use glide_sdk::look::{self as shell_look, Look as ShellLook};
use glide_sdk::theme::motion;
use glide_sdk::window::{Action as WindowAction, Window};
use iced::keyboard::key::Named;
use iced::keyboard::{self, Key, Modifiers};
use iced::widget::{operation, pane_grid};
use iced::{Point, Size, Subscription, Task, clipboard, window};

use crate::config::{self, OnExit, Preferences};
use crate::dnd;
use crate::export;
use crate::find;
use crate::hints;
use crate::i18n::{Key as Text, Lang};
use crate::input;
use crate::marks::{self, Mark};
use crate::profile::Profile;
use crate::session::{self, Bus, Context, Feed, Launch, Pane, PaneId};
use crate::sound;
use crate::terminal::{Action, Flash, Fonts, Look};

pub const FIND_ID: &str = "find";
static SEARCH_ID: AtomicU64 = AtomicU64::new(1);
const BLINK: Duration = Duration::from_millis(530);
const POLL: Duration = Duration::from_secs(1);
const SERVICE_WAIT: Duration = Duration::from_secs(3);
const BELL_GAP: Duration = Duration::from_millis(250);
const NOTICE: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    NewWindow,
    NewTab,
    ManPage,
    SplitDown,
    SplitRight,
    ClosePane,
    CloseTab(usize),
    CloseWindow,
    NextTab,
    PreviousTab,
    SelectTab(usize),
    MoveTab(isize),
    FocusPane(pane_grid::Direction),
    Copy,
    Paste,
    SelectAll,
    Find,
    FindUp,
    FindDown,
    UseSelectionForFind,
    Hints,
    Export,
    ClearScrollback,
    Reset,
    Mark(Mark),
    Jump(Direction, bool),
    Bigger,
    Smaller,
    ActualSize,
    FullScreen,
    ScrollPage(bool),
    ScrollEdge(bool),
    Options,
    Profiles,
    SetProfile(Profile),
    MakeDefault,
    Shortcuts,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuKind {
    Options,
    Window,
    Context(Option<String>),
    Profiles,
}

#[derive(Debug, Clone)]
pub struct Menu {
    pub kind: MenuKind,
    pub at: Point,
    pub opened: Instant,
    pub selected: Option<usize>,
    pub reveal: Reveal,
}

impl Menu {
    fn new(kind: MenuKind, at: Point, selected: Option<usize>) -> Self {
        let mut reveal = Reveal::closed().with_ease(motion::ease_out);
        reveal.open(motion::t2());
        Self { kind, at, opened: Instant::now(), selected, reveal }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Pane(PaneId),
    Tab(TabId),
    Window,
}

#[derive(Debug, Clone)]
pub struct Confirm {
    pub scope: Scope,
    pub processes: Vec<String>,
    pub opened: Instant,
    pub accept_focused: bool,
    pub reveal: Reveal,
}

pub struct HintMode {
    pub pane: PaneId,
    pub items: Vec<hints::Hint>,
    pub typed: String,
}

#[derive(Default)]
pub struct Find {
    pub query: String,
    pub case: bool,
    pub pattern: String,
    regex: Option<RegexSearch>,
    current: Option<Match>,
    pub total: usize,
    pub index: usize,
    pub counting: bool,
    generation: u64,
    latest: Arc<AtomicU64>,
    pane: Option<PaneId>,
    revision: u64,
}

impl Drop for Find {
    fn drop(&mut self) {
        self.latest.store(0, Ordering::Release);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabId(pub(crate) u64);

fn select(term: &mut alacritty_terminal::term::Term<session::Listener>, found: &Match) {
    let mut selection = Selection::new(SelectionType::Simple, *found.start(), Side::Left);
    selection.update(*found.end(), Side::Right);
    term.selection = Some(selection);
    term.scroll_to_point(*found.start());
}

pub struct Tab {
    pub id: TabId,
    pub panes: pane_grid::State<Pane>,
    pub focus: pane_grid::Pane,
    pub profile: Profile,
    pub bell: bool,
    pub activity: bool,
    pub hint: String,
}

impl Tab {
    #[must_use]
    pub fn focused(&self) -> Option<&Pane> {
        self.panes.get(self.focus)
    }

    #[must_use]
    pub fn context(&self) -> &Context {
        let focused = self.focused().map(|pane| &pane.context).filter(|context| **context != Context::Local);
        focused
            .or_else(|| self.panes.iter().map(|(_, pane)| &pane.context).find(|context| context.root()))
            .or_else(|| self.panes.iter().map(|(_, pane)| &pane.context).find(|context| **context != Context::Local))
            .unwrap_or(&Context::Local)
    }

    fn describe(&mut self, lang: Lang) {
        let process = self.focused().map(|pane| pane.process.clone()).unwrap_or_default();
        self.hint = match self.context() {
            Context::Local => String::new(),
            Context::Elevated => format!("{} · {process}", Text::Elevated.text(lang)),
            remote @ Context::Remote { .. } => format!("SSH · {}", remote.address().unwrap_or_default()),
        };
    }

    fn handle(&self, id: PaneId) -> Option<pane_grid::Pane> {
        self.panes.iter().find(|(_, pane)| pane.id == id).map(|(handle, _)| *handle)
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Remote(crate::service::Request),
    Feed(Feed),
    Terminal(Action),
    Key(keyboard::Event, bool),
    Command(Command),
    PaneClicked(pane_grid::Pane),
    SplitResized(pane_grid::ResizeEvent),
    TabHovered(Option<usize>),
    TabPressed(usize),
    TabMoved(usize, usize),
    FindInput(String),
    FindSubmit,
    FindCase,
    FindClose,
    Found(Option<find::Outcome>),
    Exported(Option<export::Saved>),
    Handles(Option<dnd::Handles>),
    Dropped(dnd::Dropped),
    Pasted(Option<String>),
    Open(String),
    CopyText(String),
    Dismiss,
    Confirm(bool),
    CloseRequested,
    Tick,
    Blink,
    Frame,
    Window(WindowAction),
    HeaderMenu,
    MenuHovered(usize),
}

pub struct App {
    pub prefs: Preferences,
    pub paint: Paint,
    pub lang: Lang,
    sounds: bool,
    rung: Option<Instant>,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub find: Option<Find>,
    pub(crate) find_hold: Lingering<crate::chrome::FindView>,
    pub hint_mode: Option<HintMode>,
    pub notice: Option<(String, Instant)>,
    pub drop_at: Option<(f32, f32)>,
    handles: Option<dnd::Handles>,
    pub menu: Option<Menu>,
    pub confirm: Option<Confirm>,
    pub closing_confirm: Option<Confirm>,
    pub hovered_tab: Option<usize>,
    pub window: Window,
    pub error: Option<String>,
    zoom: f32,
    font: iced::Font,
    modifiers: Modifiers,
    reported_focus: Option<PaneId>,
    shell_look: ShellLook,
    typed: Instant,
    flashes: HashMap<PaneId, (Instant, Flash)>,
    prompts: HashMap<PaneId, marks::Prompt>,
    launch: Launch,
    bus: Option<Bus>,
    next_id: u64,
    stamp: Option<SystemTime>,
    awaiting: Option<Instant>,
    pending: Vec<crate::service::OpenTab>,
    shortcuts_asked: bool,
}

#[must_use]
pub fn uri_path(arg: &str) -> PathBuf {
    local_uri_path(arg).unwrap_or_else(|| PathBuf::from(arg))
}

#[must_use]
pub(crate) fn local_uri_path(arg: &str) -> Option<PathBuf> {
    if arg.starts_with("file://") { glide_config::uri::local_path(arg) } else { Some(PathBuf::from(arg)) }
}

#[must_use]
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Launch {
    let mut launch = Launch::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if let Some(directory) = arg.strip_prefix("--working-directory=") {
            launch.directory = Some(uri_path(directory));
        } else if matches!(arg.as_str(), "--working-directory" | "-d" | "--cwd") {
            launch.directory = args.next().map(|directory| uri_path(&directory));
        } else if matches!(arg.as_str(), "-e" | "-x" | "--command" | "--") {
            let mut rest: Vec<String> = args.by_ref().collect();
            if rest.len() == 1 && rest[0].contains(char::is_whitespace) {
                rest = vec!["sh".into(), "-c".into(), rest.remove(0)];
            }
            if !rest.is_empty() {
                let program = rest.remove(0);
                launch.command = Some((program, rest));
            }
        } else if !arg.starts_with('-') {
            launch.directory = Some(uri_path(&arg));
        }
    }
    launch
}

#[must_use]
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn shortcut(key: &Key, physical: keyboard::key::Physical, modifiers: Modifiers) -> Option<Command> {
    let (ctrl, shift, alt) = (modifiers.control(), modifiers.shift(), modifiers.alt());
    if modifiers.logo() {
        return None;
    }
    if let Key::Named(named) = key {
        return Some(match (ctrl, shift, alt, named) {
            (true, false, false, Named::PageDown | Named::Tab) => Command::NextTab,
            (true, false, false, Named::PageUp) | (true, true, false, Named::Tab) => Command::PreviousTab,
            (true, true, false, Named::PageDown) => Command::MoveTab(1),
            (true, true, false, Named::PageUp) => Command::MoveTab(-1),
            (true, true, false, Named::ArrowUp) => Command::Jump(Direction::Left, false),
            (true, true, false, Named::ArrowDown) => Command::Jump(Direction::Right, false),
            (true, true, true, Named::ArrowUp) => Command::Jump(Direction::Left, true),
            (true, true, true, Named::ArrowDown) => Command::Jump(Direction::Right, true),
            (true, false, true, Named::ArrowUp) => Command::FocusPane(pane_grid::Direction::Up),
            (true, false, true, Named::ArrowDown) => Command::FocusPane(pane_grid::Direction::Down),
            (true, false, true, Named::ArrowLeft) => Command::FocusPane(pane_grid::Direction::Left),
            (true, false, true, Named::ArrowRight) => Command::FocusPane(pane_grid::Direction::Right),
            (false, true, false, Named::PageUp) => Command::ScrollPage(true),
            (false, true, false, Named::PageDown) => Command::ScrollPage(false),
            (false, true, false, Named::Home) => Command::ScrollEdge(true),
            (false, true, false, Named::End) => Command::ScrollEdge(false),
            (false, true, false, Named::Insert) => Command::Paste,
            (false, false, false, Named::F3) => Command::FindUp,
            (false, true, false, Named::F3) => Command::FindDown,
            (false, false, false, Named::F11) => Command::FullScreen,
            _ => return None,
        });
    }
    let letter = input::character(key, physical)?;
    Some(match (ctrl, shift, alt, letter) {
        (false, false, true, '1'..='9') => Command::SelectTab(letter as usize - '1' as usize),
        (true, _, false, '=' | '+') => Command::Bigger,
        (true, _, false, '-' | '_') => Command::Smaller,
        (true, false, false, '0') => Command::ActualSize,
        (true, true, false, 'n') => Command::NewWindow,
        (true, true, false, 't') => Command::NewTab,
        (true, true, false, 'w') => Command::ClosePane,
        (true, true, false, 'q') => Command::CloseWindow,
        (true, true, false, 'd') => Command::SplitDown,
        (true, true, false, 'r') => Command::SplitRight,
        (true, true, false, 'c') => Command::Copy,
        (true, true, false, 'v') => Command::Paste,
        (true, true, false, 'a') => Command::SelectAll,
        (true, true, false, 'f') => Command::Find,
        (true, true, false, 'g') => Command::FindUp,
        (true, true, false, 'e') => Command::UseSelectionForFind,
        (true, true, false, 'o') => Command::Hints,
        (true, true, false, 's') => Command::Export,
        (true, true, false, 'k') => Command::ClearScrollback,
        (true, true, false, 'u') => Command::Mark(Mark::Line),
        (true, true, false, 'm') => Command::Mark(Mark::Bookmark),
        (true, true, false, 'i') => Command::Profiles,
        (true, true, false, '/' | '?') => Command::Shortcuts,
        _ => return None,
    })
}

#[must_use]
pub fn shortcut_label(command: Command) -> Option<&'static str> {
    Some(match command {
        Command::NewWindow => "Ctrl+Shift+N",
        Command::NewTab => "Ctrl+Shift+T",
        Command::SplitDown => "Ctrl+Shift+D",
        Command::SplitRight => "Ctrl+Shift+R",
        Command::ClosePane => "Ctrl+Shift+W",
        Command::Copy => "Ctrl+Shift+C",
        Command::Paste => "Ctrl+Shift+V",
        Command::SelectAll => "Ctrl+Shift+A",
        Command::Find => "Ctrl+Shift+F",
        Command::ClearScrollback => "Ctrl+Shift+K",
        Command::Hints => "Ctrl+Shift+O",
        Command::Export => "Ctrl+Shift+S",
        Command::Mark(Mark::Line) => "Ctrl+Shift+U",
        Command::Mark(Mark::Bookmark) => "Ctrl+Shift+M",
        Command::Jump(Direction::Left, false) => "Ctrl+Shift+↑",
        Command::Jump(Direction::Right, false) => "Ctrl+Shift+↓",
        Command::Bigger => "Ctrl++",
        Command::Smaller => "Ctrl+−",
        Command::ActualSize => "Ctrl+0",
        Command::FullScreen => "F11",
        Command::Profiles => "Ctrl+Shift+I",
        Command::Shortcuts => "Ctrl+Shift+?",
        _ => return None,
    })
}

fn home() -> Option<PathBuf> {
    glide_config::home()
}

#[must_use]
pub(crate) fn directory_label(directory: Option<&Path>) -> String {
    let Some(directory) = directory else { return String::new() };
    if home().as_deref() == Some(directory) {
        return "~".to_string();
    }
    directory.file_name().map_or_else(|| directory.display().to_string(), |name| name.to_string_lossy().into_owned())
}

fn stamp() -> Option<SystemTime> {
    shell_look::stamp(config::path().as_deref())
}

fn family(prefs: &Preferences) -> iced::Font {
    prefs.font_family.as_deref().and_then(glide_sdk::theme::installed_monospace).unwrap_or(crate::MONO)
}

fn reveal_or_open(path: PathBuf) {
    let _ = std::thread::Builder::new().name("glide-terminal-reveal".into()).spawn(move || {
        let token = glide_sdk::system::launch_token().unwrap_or_default();
        if path.exists() && glide_app::integration::show_in_files(&path, &token).is_ok() {
            return;
        }
        if hints::openable_path(&path) {
            open_detached("xdg-open", &[&path.to_string_lossy()]);
        }
    });
}

fn open_detached(program: &str, args: &[&str]) {
    let program = program.to_string();
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let _ = std::thread::Builder::new().name("glide-terminal-open".into()).spawn(move || {
        let env = glide_sdk::system::launch_env();
        if let Ok(mut child) = std::process::Command::new(program).args(args).envs(env).spawn() {
            let _ = child.wait();
        }
    });
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let (mut app, task) = Self::with_launch(parse_args(std::env::args().skip(1)));
        if std::env::args().any(|arg| arg == "--service") {
            app.awaiting = Some(Instant::now());
        }
        (app, task)
    }

    pub(crate) fn with_launch(mut launch: Launch) -> (Self, Task<Message>) {
        let look = ShellLook::load();
        motion::set_scale(look.motion_scale);
        let prefs = config::load();
        if launch.command.is_none()
            && let Some(shell) = prefs.shell.clone()
        {
            launch.command = Some(config::shell_command(&shell).unwrap_or((shell, Vec::new())));
        }
        let app = Self {
            paint: look.paint(),
            lang: look.lang(),
            sounds: look.sounds,
            rung: None,
            font: family(&prefs),
            modifiers: Modifiers::empty(),
            prefs,
            tabs: Vec::new(),
            active: 0,
            find: None,
            find_hold: Lingering::default(),
            hint_mode: None,
            notice: None,
            handles: None,
            drop_at: None,
            menu: None,
            confirm: None,
            closing_confirm: None,
            hovered_tab: None,
            window: Window { size: Size::new(960.0, 600.0), ..Window::default() },
            error: None,
            zoom: 0.0,
            reported_focus: None,
            shell_look: look,
            typed: Instant::now(),
            flashes: HashMap::new(),
            prompts: HashMap::new(),
            launch,
            bus: None,
            next_id: 1,
            stamp: stamp(),
            awaiting: None,
            pending: Vec::new(),
            shortcuts_asked: false,
        };
        let handles = glide_sdk::window::handles().map(Message::Handles);
        (app, handles)
    }

    #[must_use]
    pub fn tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    fn tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    #[must_use]
    pub fn focused(&self) -> Option<&Pane> {
        self.tab()?.focused()
    }

    fn locate(&self, id: PaneId) -> Option<(usize, pane_grid::Pane)> {
        self.tabs.iter().enumerate().find_map(|(index, tab)| tab.handle(id).map(|handle| (index, handle)))
    }

    fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        let (index, handle) = self.locate(id)?;
        self.tabs[index].panes.get_mut(handle)
    }

    #[must_use]
    pub fn look(&self, profile: Profile) -> Look {
        Look {
            palette: profile.palette(&self.paint.tokens),
            fonts: self.fonts(),
            bold_bright: self.prefs.bold_bright,
            accent: self.paint.acc_bg(),
            bookmark: self.paint.warn(),
        }
    }

    #[must_use]
    pub fn fonts(&self) -> Fonts {
        Fonts {
            regular: self.font,
            size: (self.prefs.font_size + self.zoom).clamp(6.0, 48.0),
            line_height: self.prefs.line_height,
        }
    }

    #[must_use]
    pub const fn window_focused(&self) -> bool {
        self.window.focused
    }

    #[must_use]
    pub fn edge_to_edge(&self) -> bool {
        self.window.edge_to_edge()
    }

    #[must_use]
    pub(crate) fn pane_focused(&self, id: PaneId) -> bool {
        self.reported_focus == Some(id)
    }

    #[must_use]
    pub fn view(&self) -> iced::Element<'_, Message> {
        crate::chrome::view(self)
    }

    #[must_use]
    pub(crate) fn man_topic(&self) -> Option<String> {
        let selected = self.focused()?.term.lock().selection_to_string()?;
        let topic = selected.trim();
        let word = |c: char| c.is_alphanumeric() || "-_.:+".contains(c);
        (!topic.is_empty() && topic.len() <= 64 && topic.chars().all(word)).then(|| topic.to_string())
    }

    #[must_use]
    pub(crate) fn blink_visible(&self) -> bool {
        !self.window.focused
            || motion::is_disabled()
            || (self.typed.elapsed().as_millis() / BLINK.as_millis()).is_multiple_of(2)
    }

    #[must_use]
    pub fn flash(&self, id: PaneId) -> Option<(Flash, f32)> {
        let duration = motion::t3().max(Duration::from_millis(1));
        let (at, kind) = self.flashes.get(&id)?;
        let left = 1.0 - (at.elapsed().as_secs_f32() / duration.as_secs_f32()).min(1.0);
        (left > 0.0).then_some((*kind, left))
    }

    #[must_use]
    pub fn reveal(opened: Instant) -> f32 {
        let duration = motion::t2();
        if duration.is_zero() {
            return 1.0;
        }
        motion::E_OUT.ease((opened.elapsed().as_secs_f32() / duration.as_secs_f32()).min(1.0))
    }

    fn animating(&self) -> bool {
        let opening = |opened: Instant| opened.elapsed() < motion::t2();
        self.flashes.values().any(|(at, _)| at.elapsed() < motion::t3())
            || self.notice.as_ref().is_some_and(|(_, at)| opening(*at))
            || self.menu.as_ref().is_some_and(|menu| menu.reveal.is_animating(motion::t2()))
            || self.confirm.as_ref().is_some_and(|confirm| opening(confirm.opened))
            || self.closing_confirm.as_ref().is_some_and(|confirm| confirm.reveal.is_animating(motion::t2()))
    }

    #[must_use]
    pub fn title(&self) -> String {
        let Some(pane) = self.focused() else { return Text::AppName.text(self.lang).to_string() };
        let label = pane.title.clone().filter(|title| !title.trim().is_empty()).unwrap_or_else(|| {
            let directory = directory_label(pane.directory.as_deref());
            if pane.process.is_empty() { directory } else { format!("{directory} — {}", pane.process) }
        });
        format!("{label} — {}×{}", pane.cells.columns, pane.cells.lines)
    }

    #[must_use]
    pub fn tab_title(&self, tab: &Tab) -> String {
        let Some(pane) = tab.focused() else { return String::new() };
        if let Some(title) = pane.title.as_deref().filter(|title| !title.trim().is_empty()) {
            return title.to_string();
        }
        if let Some(address) = pane.context.short_address() {
            return address;
        }
        if pane.busy { pane.process.clone() } else { directory_label(pane.directory.as_deref()) }
    }

    #[must_use]
    pub fn iced_theme(&self) -> iced::Theme {
        self.paint.iced_theme()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            Subscription::run(session::feed).map(Message::Feed),
            iced::time::every(POLL).map(|_| Message::Tick),
            Subscription::run(crate::service::requests).map(Message::Remote),
        ];
        if self.reported_focus.is_some()
            && !motion::is_disabled()
            && self.focused().is_some_and(|pane| {
                let term = pane.term.lock();
                term.cursor_style().blinking && term.mode().contains(TermMode::SHOW_CURSOR)
            })
        {
            subscriptions.push(iced::time::every(BLINK).map(|_| Message::Blink));
        }
        if let Some(handles) = self.handles {
            subscriptions.push(Subscription::run_with(handles, dnd::events).map(Message::Dropped));
        }
        if self.animating() {
            subscriptions.push(window::frames().map(|_| Message::Frame));
        }
        Subscription::batch(subscriptions)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.update_message(message);
        self.sync_focus();
        task
    }

    fn update_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Feed(Feed::Connected(bus)) => {
                self.bus = Some(bus);
                for request in std::mem::take(&mut self.pending) {
                    self.open_remote(&request);
                }
                if self.tabs.is_empty() && self.awaiting.is_none() {
                    let launch = self.launch.clone();
                    self.open_tab(&launch);
                }
                Task::none()
            }
            Message::Feed(Feed::Events(events)) => {
                let tasks: Vec<_> = events.into_iter().map(|(id, event)| self.on_event(id, event)).collect();
                self.settle_prompts();
                Task::batch(tasks)
            }
            Message::Terminal(action) => self.on_action(action),
            Message::Key(event, captured) => self.on_key(&event, captured),
            Message::Command(command) => self.command(command),
            Message::Handles(_)
            | Message::Dropped(_)
            | Message::Exported(_)
            | Message::PaneClicked(_)
            | Message::SplitResized(_)
            | Message::TabPressed(_)
            | Message::TabMoved(..)
            | Message::TabHovered(_) => {
                self.on_layout(message);
                Task::none()
            }
            Message::FindInput(_)
            | Message::FindSubmit
            | Message::FindCase
            | Message::FindClose
            | Message::Found(_) => self.on_find(message),
            Message::CloseRequested
            | Message::Tick
            | Message::Blink
            | Message::Frame
            | Message::Window(_)
            | Message::Remote(_) => self.on_window(&message),
            Message::Pasted(text) => {
                if let Some(text) = text {
                    self.paste(&text);
                }
                Task::none()
            }
            Message::Open(uri) => {
                self.dismiss_menu();
                match local_uri_path(&uri) {
                    Some(path) => reveal_or_open(path),
                    None if hints::openable(&uri) => open_detached("xdg-open", &[&uri]),
                    None => {}
                }
                Task::none()
            }
            Message::CopyText(text) => {
                self.dismiss_menu();
                clipboard::write(text)
            }
            Message::Dismiss => {
                self.dismiss_menu();
                Task::none()
            }
            Message::MenuHovered(index) => {
                if let Some(menu) = &mut self.menu {
                    menu.selected = Some(index);
                }
                Task::none()
            }
            Message::HeaderMenu => {
                self.toggle_menu(MenuKind::Window, Point::new(8.0, crate::chrome::HEADER));
                Task::none()
            }
            Message::Confirm(accepted) => {
                let Some(mut confirm) = self.confirm.take() else { return Task::none() };
                confirm.reveal.close(motion::t2());
                confirm.reveal = confirm.reveal.with_ease(motion::ease_io);
                let scope = confirm.scope;
                self.closing_confirm = Some(confirm);
                if accepted { self.close(scope) } else { Task::none() }
            }
        }
    }

    fn on_layout(&mut self, message: Message) {
        match message {
            Message::Handles(handles) => self.handles = handles,
            Message::Dropped(dropped) => self.dropped(dropped),
            Message::Exported(saved) => {
                let lang = self.lang;
                self.notice = match saved {
                    Some(export::Saved::Written(path)) => {
                        Some(format!("{} {}", Text::SavedTo.text(lang), path.display()))
                    }
                    Some(export::Saved::Failed(error)) => Some(format!("{}: {error}", Text::SaveFailed.text(lang))),
                    Some(export::Saved::Cancelled) | None => None,
                }
                .map(|text| (text, Instant::now()));
            }
            Message::PaneClicked(handle) => {
                if let Some(tab) = self.tab_mut() {
                    tab.focus = handle;
                }
            }
            Message::SplitResized(event) => {
                if let Some(tab) = self.tab_mut() {
                    tab.panes.resize(event.split, event.ratio);
                }
            }
            Message::TabPressed(index) => self.select(index),
            Message::TabMoved(from, to) => self.move_tab(from, to),
            Message::TabHovered(index) => self.hovered_tab = index,
            _ => {}
        }
    }

    fn on_find(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::FindInput(query) => {
                if let Some(find) = &mut self.find {
                    find.query = query;
                }
                return self.rebuild_find();
            }
            Message::FindSubmit => {
                return self.command(if self.modifiers.shift() { Command::FindDown } else { Command::FindUp });
            }
            Message::FindCase => {
                if let Some(find) = &mut self.find {
                    find.case = !find.case;
                }
                return self.rebuild_find();
            }
            Message::Found(Some(outcome)) => self.found(outcome),
            Message::Found(None) => {}
            _ => self.find = None,
        }
        Task::none()
    }

    fn on_window(&mut self, message: &Message) -> Task<Message> {
        match *message {
            Message::Remote(ref request) => self.on_remote(request),
            Message::CloseRequested => self.request_close(Scope::Window),
            Message::Tick => {
                for tab in &mut self.tabs {
                    for (_, pane) in tab.panes.iter_mut() {
                        pane.refresh();
                    }
                    tab.describe(self.lang);
                }
                self.settle_prompts();
                self.publish_tabs();
                if self.awaiting.is_some_and(|since| since.elapsed() >= SERVICE_WAIT) && self.bus.is_some() {
                    self.awaiting = None;
                    if self.tabs.is_empty() {
                        let launch = self.launch.clone();
                        self.open_tab(&launch);
                    }
                }
                self.notice.take_if(|(_, at)| at.elapsed() >= NOTICE);
                self.reload();
                if self.find.as_ref().is_some_and(|find| {
                    self.focused().is_some_and(|pane| find.revision != pane.revision.load(Ordering::Acquire))
                }) {
                    return self.rebuild_find();
                }
                Task::none()
            }
            Message::Blink | Message::Frame => {
                self.flashes.retain(|_, (at, _)| at.elapsed() < motion::t3());
                if let Some(menu) = &mut self.menu {
                    menu.reveal.settle(motion::t2());
                }
                self.menu.take_if(|menu| !menu.reveal.is_open() && !menu.reveal.is_visible(motion::t2()));
                if let Some(confirm) = &mut self.closing_confirm {
                    confirm.reveal.settle(motion::t2());
                }
                self.closing_confirm.take_if(|confirm| !confirm.reveal.is_visible(motion::t2()));
                Task::none()
            }
            Message::Window(WindowAction::ToggleMaximize | WindowAction::Minimize) => {
                self.dismiss_menu();
                Task::none()
            }
            _ => Task::none(),
        }
    }

    fn reload(&mut self) {
        let stamp = stamp();
        if stamp == self.stamp {
            return;
        }
        self.stamp = stamp;
        let prefs = config::load();
        if prefs.profile != self.prefs.profile {
            let previous = self.prefs.profile;
            for tab in self.tabs.iter_mut().filter(|tab| tab.profile == previous) {
                tab.profile = prefs.profile;
            }
        }
        if prefs.font_family != self.prefs.font_family {
            self.font = family(&prefs);
        }
        self.prefs = prefs;
        let options = self.prefs.term_config();
        for tab in &self.tabs {
            for (_, pane) in tab.panes.iter() {
                pane.term.lock().set_options(options.clone());
            }
        }
    }

    pub fn follow_look(&mut self, look: &ShellLook, paint: Paint) {
        self.paint = paint;
        self.lang = look.lang();
        self.sounds = look.sounds;
        self.shell_look = look.clone();
    }

    pub fn follow_window(&mut self, window: Window) {
        if window.focused != self.window.focused {
            self.typed = Instant::now();
        }
        self.window = window;
        self.sync_focus();
    }

    fn sync_focus(&mut self) {
        let focused = self.focused().map(|pane| pane.id);
        if self.find.as_ref().is_some_and(|find| find.pane.is_some() && find.pane != focused) {
            self.find = None;
        }
        if self.hint_mode.as_ref().is_some_and(|mode| Some(mode.pane) != focused) {
            self.hint_mode = None;
        }
        let next = focused.filter(|_| self.window.focused && self.confirm.is_none() && self.find.is_none());
        if self.reported_focus == next {
            return;
        }
        for (id, active) in [(self.reported_focus, false), (next, true)] {
            if let Some((index, handle)) = id.and_then(|id| self.locate(id))
                && let Some(pane) = self.tabs[index].panes.get(handle)
            {
                let mut term = pane.term.lock();
                term.is_focused = active;
                if term.mode().contains(TermMode::FOCUS_IN_OUT) {
                    pane.write(if active { &b"\x1b[I"[..] } else { &b"\x1b[O"[..] });
                }
            }
        }
        self.reported_focus = next;
        self.typed = Instant::now();
    }

    fn spawn(&mut self, launch: &Launch) -> Option<Pane> {
        let bus = self.bus.as_ref()?;
        let id = PaneId(self.next_id);
        self.next_id += 1;
        match Pane::spawn(id, bus, launch, self.prefs.term_config()) {
            Ok(pane) => {
                self.error = None;
                Some(pane)
            }
            Err(error) => {
                self.error = Some(error.to_string());
                None
            }
        }
    }

    fn here(&self) -> Launch {
        Launch {
            command: self.prefs.shell.clone().map(|shell| config::shell_command(&shell).unwrap_or((shell, Vec::new()))),
            directory: self.focused().and_then(|pane| pane.directory.clone()).or_else(home),
        }
    }

    fn on_remote(&mut self, request: &crate::service::Request) -> Task<Message> {
        match request {
            crate::service::Request::OpenTab(request) => {
                if self.bus.is_none() {
                    self.pending.push(request.clone());
                } else {
                    self.open_remote(request);
                }
                Task::none()
            }
            crate::service::Request::NewTab => {
                if self.bus.is_some() {
                    let launch = self.here();
                    self.open_tab(&launch);
                }
                Task::done(Message::Window(WindowAction::Activate))
            }
            crate::service::Request::FocusTab(id) => {
                let Some(index) = self.tabs.iter().position(|tab| tab.id.0 == *id) else {
                    return Task::none();
                };
                self.active = index;
                Task::done(Message::Window(WindowAction::Activate))
            }
        }
    }

    fn publish_tabs(&self) {
        let home = home();
        let tabs = self
            .tabs
            .iter()
            .filter_map(|tab| {
                let pane = tab.panes.get(tab.focus)?;
                let directory = pane.directory.as_ref().map_or_else(String::new, |dir| {
                    match home.as_deref().and_then(|home| dir.strip_prefix(home).ok()) {
                        Some(rest) if rest.as_os_str().is_empty() => String::from("~"),
                        Some(rest) => format!("~/{}", rest.display()),
                        None => dir.display().to_string(),
                    }
                });
                let title =
                    pane.title.clone().filter(|title| !title.is_empty()).unwrap_or_else(|| pane.process.clone());
                Some(crate::service::TabEntry { id: tab.id.0, title, directory })
            })
            .collect();
        crate::service::publish_tabs(tabs);
    }

    fn open_remote(&mut self, request: &crate::service::OpenTab) {
        self.awaiting = None;
        let here = self.here();
        let launch = Launch {
            command: request.command.clone().or(here.command),
            directory: request.directory.clone().or(here.directory),
        };
        self.open_tab(&launch);
        if let (Some(handles), Some(token)) = (self.handles, request.token.clone()) {
            let _ = std::thread::Builder::new().name("glide-terminal-activate".into()).spawn(move || {
                let _ = glide_app::activation::activate(handles.display, handles.surface, &token);
            });
        }
    }

    fn open_tab(&mut self, launch: &Launch) {
        let Some(pane) = self.spawn(launch) else { return };
        let (panes, focus) = pane_grid::State::new(pane);
        let profile = self.tab().map_or(self.prefs.profile, |tab| tab.profile);
        let index = if self.tabs.is_empty() { 0 } else { self.active + 1 };
        let tab =
            Tab { id: TabId(self.next_id), panes, focus, profile, bell: false, activity: false, hint: String::new() };
        self.tabs.insert(index, tab);
        self.next_id += 1;
        self.active = index;
    }

    fn split(&mut self, axis: pane_grid::Axis) {
        let launch = self.here();
        let Some(pane) = self.spawn(&launch) else { return };
        if let Some(tab) = self.tab_mut()
            && let Some((handle, _)) = tab.panes.split(axis, tab.focus, pane)
        {
            tab.focus = handle;
        }
    }

    fn select(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.hint_mode = None;
            self.active = index;
            self.tabs[index].bell = false;
            self.tabs[index].activity = false;
            self.find = None;
        }
    }

    fn request_close(&mut self, scope: Scope) -> Task<Message> {
        let mut busy = Vec::new();
        for tab in &mut self.tabs {
            for (_, pane) in tab.panes.iter_mut() {
                let inside = match scope {
                    Scope::Pane(id) => pane.id == id,
                    Scope::Tab(id) => tab.id == id,
                    Scope::Window => true,
                };
                if inside {
                    pane.refresh();
                    if pane.busy && !busy.contains(&pane.process) {
                        busy.push(pane.process.clone());
                    }
                }
            }
        }
        if self.prefs.confirm_close && !busy.is_empty() {
            self.dismiss_menu();
            let mut reveal = Reveal::closed().with_ease(motion::ease_out);
            reveal.open(motion::t2());
            self.closing_confirm = None;
            self.confirm =
                Some(Confirm { scope, processes: busy, opened: Instant::now(), accept_focused: false, reveal });
            return Task::none();
        }
        self.close(scope)
    }

    fn close(&mut self, scope: Scope) -> Task<Message> {
        match scope {
            Scope::Window => return iced::exit(),
            Scope::Tab(id) => {
                let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else { return Task::none() };
                self.tabs.remove(index);
                if index < self.active || self.active >= self.tabs.len() {
                    self.active = self.active.saturating_sub(1);
                }
            }
            Scope::Pane(id) => {
                let Some((index, handle)) = self.locate(id) else { return Task::none() };
                let tab = &mut self.tabs[index];
                if tab.panes.len() > 1 {
                    if let Some((_, sibling)) = tab.panes.close(handle)
                        && tab.focus == handle
                    {
                        tab.focus = sibling;
                    }
                } else {
                    let id = tab.id;
                    return self.close(Scope::Tab(id));
                }
            }
        }
        self.flashes.retain(|id, _| self.tabs.iter().any(|tab| tab.handle(*id).is_some()));
        if self.tabs.is_empty() { iced::exit() } else { Task::none() }
    }

    fn on_event(&mut self, id: PaneId, event: TermEvent) -> Task<Message> {
        let active = self.active;
        let Some((index, _)) = self.locate(id) else { return Task::none() };
        let profile = self.tabs[index].profile;
        let palette = profile.palette(&self.paint.tokens);
        let Some(pane) = self.pane_mut(id) else { return Task::none() };
        match event {
            TermEvent::Wakeup => {
                if index != active {
                    self.tabs[index].activity = true;
                }
                if self.hint_mode.as_ref().is_some_and(|mode| mode.pane == id) {
                    self.hint_mode = None;
                }
            }
            TermEvent::Title(title) => pane.title = Some(title),
            TermEvent::ResetTitle => pane.title = None,
            TermEvent::ClipboardStore(ClipboardType::Clipboard, text) => return clipboard::write(text),
            TermEvent::ClipboardStore(ClipboardType::Selection, text) => return clipboard::write_primary(text),
            TermEvent::ColorRequest(index, format) => {
                let rgb = palette.query(pane.term.lock().colors(), index);
                pane.write(format(rgb).into_bytes());
            }
            TermEvent::PtyWrite(text) => pane.write(text.into_bytes()),
            TermEvent::TextAreaSizeRequest(format) => pane.write(format(pane.window_size()).into_bytes()),
            TermEvent::Bell => return self.bell(id, index),
            TermEvent::ChildExit(status) => return self.exited(id, status),
            TermEvent::ClipboardLoad(..)
            | TermEvent::Exit
            | TermEvent::CursorBlinkingChange
            | TermEvent::MouseCursorDirty => {}
        }
        Task::none()
    }

    fn bell(&mut self, id: PaneId, index: usize) -> Task<Message> {
        if self.prefs.visual_bell {
            self.flashes.insert(id, (Instant::now(), Flash::Bell));
        }
        if index != self.active {
            self.tabs[index].bell = true;
        }
        let quiet = self.rung.is_some_and(|at| at.elapsed() < BELL_GAP);
        if self.prefs.audible_bell
            && self.sounds
            && !quiet
            && let Some(file) = sound::bell_file(self.prefs.bell_sound.as_deref())
        {
            self.rung = Some(Instant::now());
            sound::play(file);
        }
        Task::done(Message::Window(WindowAction::Attention))
    }

    fn exited(&mut self, id: PaneId, status: std::process::ExitStatus) -> Task<Message> {
        let close = match self.prefs.on_exit {
            OnExit::Close => true,
            OnExit::CloseClean => status.success(),
            OnExit::Keep => false,
        };
        let lang = self.lang;
        let Some(pane) = self.pane_mut(id) else { return Task::none() };
        pane.exit = Some(status);
        pane.busy = false;
        if close {
            return self.close(Scope::Pane(id));
        }
        let note = match (status.code(), std::os::unix::process::ExitStatusExt::signal(&status)) {
            (Some(code), _) if code != 0 => format!("{} {code}]", Text::CompletedWithCode.text(lang)),
            (None, Some(signal)) => format!("{} {signal}]", Text::CompletedWithSignal.text(lang)),
            _ => Text::Completed.text(lang).to_string(),
        };
        let mut processor: Processor = Processor::new();
        processor.advance(&mut *pane.term.lock(), format!("\r\n\x1b[2m{note}\x1b[0m").as_bytes());
        Task::none()
    }

    fn on_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Resize(id, cells, cell) => {
                if let Some(pane) = self.pane_mut(id) {
                    pane.resize(cells, cell);
                }
                self.hint_mode = None;
            }
            Action::Focus(id) => {
                self.dismiss_menu();
                self.hint_mode = None;
                if let Some((index, handle)) = self.locate(id) {
                    self.tabs[index].focus = handle;
                }
            }
            Action::Selected(id) => {
                let text = self.pane_mut(id).and_then(|pane| pane.term.lock().selection_to_string());
                if let Some(text) = text.filter(|text| !text.is_empty() && self.prefs.copy_on_select) {
                    return clipboard::write_primary(text);
                }
            }
            Action::PastePrimary(id) => {
                if let Some((index, handle)) = self.locate(id) {
                    self.tabs[index].focus = handle;
                }
                return clipboard::read_primary().map(Message::Pasted);
            }
            Action::Open(uri) => return self.update(Message::Open(uri)),
            Action::Context(id, at, link) => {
                if let Some((index, handle)) = self.locate(id) {
                    self.tabs[index].focus = handle;
                }
                self.menu = Some(Menu::new(MenuKind::Context(link), at, None));
            }
        }
        Task::none()
    }

    fn on_key(&mut self, event: &keyboard::Event, captured: bool) -> Task<Message> {
        let keyboard::Event::KeyPressed { key, modified_key, physical_key, modifiers, text, .. } = event else {
            if let keyboard::Event::ModifiersChanged(modifiers) = event {
                self.modifiers = *modifiers;
            }
            return Task::none();
        };
        self.modifiers = *modifiers;
        let escape = matches!(key, Key::Named(Named::Escape));
        if let Some(confirm) = &self.confirm {
            let accepted = confirm.accept_focused;
            return match key {
                Key::Named(Named::Escape) => self.update(Message::Confirm(false)),
                Key::Named(Named::Enter | Named::Space) => self.update(Message::Confirm(accepted)),
                Key::Named(Named::Tab | Named::ArrowLeft | Named::ArrowRight) => {
                    if let Some(confirm) = &mut self.confirm {
                        confirm.accept_focused = !accepted;
                    }
                    Task::none()
                }
                _ => Task::none(),
            };
        }
        if escape && self.menu_open() {
            self.dismiss_menu();
            return Task::none();
        }
        if captured {
            if escape && self.find.is_some() {
                self.find = None;
            }
            return Task::none();
        }
        if self.hint_mode.is_some() {
            return self.hint_key(key, *physical_key, modifiers.shift());
        }
        if self.menu_open()
            && let Some(task) = self.menu_key(key, *modifiers)
        {
            return task;
        }
        if let Some(command) = shortcut(key, *physical_key, *modifiers).filter(|command| {
            self.find.is_some()
                || !matches!(key, Key::Named(Named::F3))
                || !matches!(command, Command::FindUp | Command::FindDown)
        }) {
            return self.command(command);
        }
        if self.menu_open() {
            self.dismiss_menu();
            return Task::none();
        }
        let mark_prompts = self.prefs.mark_prompts;
        let alt_is_meta = self.prefs.alt_sends_escape;
        self.typed = Instant::now();
        let Some(pane) = self.focused() else { return Task::none() };
        let id = pane.id;
        let enter = matches!(key, Key::Named(Named::Enter));
        if pane.exit.is_some() {
            return if enter { self.close(Scope::Pane(id)) } else { Task::none() };
        }
        let mut term = pane.term.lock();
        let mode = *term.mode();
        let encoded_key = if modifiers.control() { key } else { modified_key };
        if let Some(bytes) = input::encode(encoded_key, *physical_key, *modifiers, text.as_deref(), mode, alt_is_meta) {
            let prompt = (mark_prompts && enter).then(|| marks::prompt(&mut term)).flatten();
            term.scroll_display(Scroll::Bottom);
            term.selection = None;
            drop(term);
            pane.write(bytes);
            if let Some(prompt) = prompt {
                self.prompts.insert(id, prompt);
            }
        }
        Task::none()
    }

    fn hint_key(&mut self, key: &Key, physical: keyboard::key::Physical, copy: bool) -> Task<Message> {
        let Some(mode) = &mut self.hint_mode else { return Task::none() };
        match key {
            Key::Named(Named::Escape) => self.hint_mode = None,
            Key::Named(Named::Backspace) => {
                mode.typed.pop();
            }
            _ => {
                let Some(letter) = input::character(key, physical).filter(char::is_ascii_lowercase) else {
                    return Task::none();
                };
                mode.typed.push(letter);
                let mut matching = mode.items.iter().filter(|hint| hint.label.starts_with(&mode.typed));
                match (matching.next().cloned(), matching.next()) {
                    (None, _) => mode.typed.clear(),
                    (Some(hint), None) if hint.label == mode.typed => {
                        let pane = mode.pane;
                        self.hint_mode = None;
                        return self.use_hint(pane, &hint, copy);
                    }
                    _ => {}
                }
            }
        }
        Task::none()
    }

    fn menu_key(&mut self, key: &Key, modifiers: Modifiers) -> Option<Task<Message>> {
        let menu = self.menu.as_ref()?;
        let actions = crate::chrome::menu_actions(self, &menu.kind);
        if actions.is_empty() {
            return None;
        }
        let columns = if menu.kind == MenuKind::Profiles { 3 } else { 1 };
        let next = match key {
            Key::Named(Named::ArrowDown) => Some(columns),
            Key::Named(Named::ArrowUp) => Some(-columns),
            Key::Named(Named::ArrowRight) if columns > 1 => Some(1),
            Key::Named(Named::ArrowLeft) if columns > 1 => Some(-1),
            Key::Named(Named::Tab) => Some(if modifiers.shift() { -1 } else { 1 }),
            Key::Named(Named::Home) => {
                self.menu.as_mut()?.selected = Some(0);
                return Some(operation::snap_to(
                    glide_sdk::menu::SCROLL,
                    iced::widget::scrollable::RelativeOffset { x: 0.0, y: 0.0 },
                ));
            }
            Key::Named(Named::End) => {
                self.menu.as_mut()?.selected = Some(actions.len() - 1);
                return Some(operation::snap_to(
                    glide_sdk::menu::SCROLL,
                    iced::widget::scrollable::RelativeOffset { x: 0.0, y: 1.0 },
                ));
            }
            Key::Named(Named::Enter | Named::Space) => {
                let selected = menu.selected.unwrap_or(0).min(actions.len() - 1);
                return Some(match actions[selected].clone() {
                    window @ Message::Window(_) => {
                        self.dismiss_menu();
                        Task::done(window)
                    }
                    action => self.update(action),
                });
            }
            _ => None,
        };
        if let Some(step) = next {
            let selected = menu.selected.map_or(if step < 0 { actions.len() - 1 } else { 0 }, |selected| {
                (selected as isize + step).rem_euclid(actions.len() as isize) as usize
            });
            self.menu.as_mut()?.selected = Some(selected);
            return Some(operation::snap_to(
                glide_sdk::menu::SCROLL,
                iced::widget::scrollable::RelativeOffset {
                    x: 0.0,
                    y: selected as f32 / actions.len().saturating_sub(1).max(1) as f32,
                },
            ));
        }
        None
    }

    fn use_hint(&self, pane: PaneId, hint: &hints::Hint, copy: bool) -> Task<Message> {
        if copy {
            return clipboard::write(hint.text.clone());
        }
        let cwd = self.locate(pane).and_then(|(index, handle)| self.tabs[index].panes.get(handle)?.directory.clone());
        let target = hints::target(hint, home().as_deref(), cwd.as_deref());
        if target.starts_with('/') {
            reveal_or_open(PathBuf::from(target));
        } else if let Some(path) = local_uri_path(&target) {
            reveal_or_open(path);
        } else if hints::openable(&target) {
            open_detached("xdg-open", &[&target]);
        }
        Task::none()
    }

    fn move_tab(&mut self, from: usize, to: usize) {
        let to = to.min(self.tabs.len().saturating_sub(1));
        if from != to && from < self.tabs.len() {
            let tab = self.tabs.remove(from);
            self.tabs.insert(to, tab);
            self.active = to;
        }
    }

    fn settle_prompts(&mut self) {
        let mut prompts = std::mem::take(&mut self.prompts);
        prompts.retain(|id, prompt| {
            self.locate(*id)
                .and_then(|(index, handle)| self.tabs[index].panes.get(handle))
                .is_some_and(|pane| prompt.settle(&mut pane.term.lock()))
        });
        self.prompts = prompts;
    }

    fn dropped(&mut self, dropped: dnd::Dropped) {
        let (text, at) = match dropped {
            dnd::Dropped::Hover(at) => {
                self.drop_at = at;
                return;
            }
            dnd::Dropped::Paths(paths, at) => {
                let mut bytes = Vec::new();
                for path in &paths {
                    bytes.extend(dnd::quote_bytes(path));
                    bytes.push(b' ');
                }
                self.drop_at = None;
                if self.focus_drop(at) {
                    self.paste_bytes(&bytes);
                }
                return;
            }
            dnd::Dropped::Text(text, at) => (text, at),
        };
        self.drop_at = None;
        if self.focus_drop(at) {
            self.paste(&text);
        }
    }

    fn focus_drop(&mut self, at: (f32, f32)) -> bool {
        if let Some(handle) = crate::chrome::pane_at(self, at)
            && let Some(tab) = self.tab_mut()
        {
            tab.focus = handle;
            return true;
        }
        false
    }

    fn paste(&mut self, text: &str) {
        self.paste_bytes(text.as_bytes());
    }

    fn paste_bytes(&mut self, text: &[u8]) {
        let Some(pane) = self.focused() else { return };
        let mut term = pane.term.lock();
        let bracketed = term.mode().contains(TermMode::BRACKETED_PASTE);
        term.scroll_display(Scroll::Bottom);
        drop(term);
        let text = dnd::without_controls(text);
        let mut bytes = Vec::new();
        if bracketed {
            bytes.extend_from_slice(b"\x1b[200~");
        }
        for (index, &byte) in text.iter().enumerate() {
            if bracketed {
                bytes.push(byte);
            } else if byte == b'\n' {
                if index == 0 || text[index - 1] != b'\r' {
                    bytes.push(b'\r');
                }
            } else {
                bytes.push(byte);
            }
        }
        if bracketed {
            bytes.extend_from_slice(b"\x1b[201~");
        }
        pane.write(bytes);
    }

    fn rebuild_find(&mut self) -> Task<Message> {
        if let Some(pane) = self.focused() {
            pane.term.lock().selection = None;
        }
        let Some(find) = &mut self.find else { return Task::none() };
        let flag = if find.case { "(?-i)" } else { "(?i)" };
        find.pattern = if find.query.is_empty() { String::new() } else { format!("{flag}{}", escape(&find.query)) };
        find.regex = (!find.query.is_empty()).then(|| RegexSearch::new(&find.pattern).ok()).flatten();
        find.current = None;
        find.total = 0;
        find.index = 0;
        find.counting = false;
        find.generation = SEARCH_ID.fetch_add(1, Ordering::Relaxed);
        find.latest.store(find.generation, std::sync::atomic::Ordering::Relaxed);
        self.find_step(Direction::Left)
    }

    fn find_step(&mut self, direction: Direction) -> Task<Message> {
        let Some(pane) = self.tabs.get(self.active).and_then(Tab::focused) else { return Task::none() };
        let Some(find) = &mut self.find else { return Task::none() };
        let Some(regex) = &mut find.regex else { return Task::none() };
        if find.pane != Some(pane.id) {
            return Task::none();
        }
        let revision = pane.revision.load(Ordering::Acquire);
        if find.revision != revision {
            find.current = None;
        }
        find.revision = revision;
        let mut term = pane.term.lock();
        let origin = match (&find.current, direction) {
            (Some(found), Direction::Right) => found.end().add(&*term, Boundary::None, 1),
            (Some(found), Direction::Left) => found.start().sub(&*term, Boundary::None, 1),
            (None, _) => GridPoint::new(term.bottommost_line(), term.last_column()),
        };
        let near = term.screen_lines() * 2;
        let ahead = |found: &Match| match direction {
            Direction::Right => *found.start() >= origin,
            Direction::Left => *found.start() <= origin,
        };
        let target = match term.search_next(regex, origin, direction, Side::Left, Some(near)).filter(ahead) {
            Some(found) => {
                select(&mut term, &found);
                find.current = Some(found.clone());
                find::Target::Current(found)
            }
            None => find::Target::Seek(origin, direction),
        };
        drop(term);
        find.generation = SEARCH_ID.fetch_add(1, Ordering::Relaxed);
        find.latest.store(find.generation, std::sync::atomic::Ordering::Relaxed);
        find.counting = true;
        let job = find::Job {
            generation: find.generation,
            latest: Arc::clone(&find.latest),
            term: Arc::clone(&pane.term),
            regex: regex.clone(),
            target,
            revision: Arc::clone(&pane.revision),
            expected_revision: revision,
        };
        Task::perform(find::spawn(job), |outcome| Message::Found(outcome.ok()))
    }

    fn found(&mut self, outcome: find::Outcome) {
        let Some(pane) = self.tabs.get(self.active).and_then(Tab::focused) else { return };
        let Some(find) = self.find.as_mut().filter(|find| {
            find.generation == outcome.generation
                && find.pane == Some(pane.id)
                && outcome.revision == pane.revision.load(Ordering::Acquire)
        }) else {
            return;
        };
        find.counting = false;
        find.total = outcome.total;
        find.index = outcome.index;
        if let Some(found) = outcome.found {
            select(&mut pane.term.lock(), &found);
            find.current = Some(found);
        } else if outcome.total == 0 {
            find.current = None;
        }
    }

    fn command(&mut self, command: Command) -> Task<Message> {
        if !matches!(command, Command::Profiles | Command::Options | Command::SetProfile(_)) {
            self.dismiss_menu();
        }
        match command {
            Command::NewWindow => {
                let directory = self.here().directory;
                if let Ok(exe) = std::env::current_exe() {
                    let mut args = Vec::new();
                    if let Some(directory) = directory {
                        args.push(format!("--working-directory={}", directory.display()));
                    }
                    let args: Vec<&str> = args.iter().map(String::as_str).collect();
                    open_detached(&exe.to_string_lossy(), &args);
                }
            }
            Command::NewTab => {
                let launch = self.here();
                self.open_tab(&launch);
            }
            Command::ManPage => {
                if let Some(topic) = self.man_topic() {
                    let launch =
                        Launch { command: Some(("man".into(), vec![topic])), directory: self.here().directory };
                    self.open_tab(&launch);
                    if let Some(tab) = self.tab_mut() {
                        tab.profile = Profile::ManPage;
                    }
                }
            }
            Command::SplitDown => self.split(pane_grid::Axis::Horizontal),
            Command::SplitRight => self.split(pane_grid::Axis::Vertical),
            Command::ClosePane => {
                if let Some(pane) = self.focused() {
                    return self.request_close(Scope::Pane(pane.id));
                }
            }
            Command::CloseTab(index) => {
                if let Some(tab) = self.tabs.get(index) {
                    return self.request_close(Scope::Tab(tab.id));
                }
            }
            Command::CloseWindow => return self.request_close(Scope::Window),
            Command::NextTab if !self.tabs.is_empty() => self.select((self.active + 1) % self.tabs.len()),
            Command::PreviousTab if !self.tabs.is_empty() => {
                self.select((self.active + self.tabs.len() - 1) % self.tabs.len());
            }
            Command::SelectTab(index) => self.select(index),
            Command::MoveTab(step) => self.move_tab(self.active, self.active.saturating_add_signed(step)),
            Command::FocusPane(direction) => {
                if let Some(tab) = self.tab_mut()
                    && let Some(next) = tab.panes.adjacent(tab.focus, direction)
                {
                    tab.focus = next;
                }
            }
            Command::Copy
            | Command::Paste
            | Command::SelectAll
            | Command::Find
            | Command::FindUp
            | Command::FindDown
            | Command::UseSelectionForFind
            | Command::ClearScrollback
            | Command::Reset
            | Command::Mark(_)
            | Command::Jump(..)
            | Command::Hints
            | Command::Export => return self.edit(command),
            _ => return self.adjust(command),
        }
        Task::none()
    }

    fn edit(&mut self, command: Command) -> Task<Message> {
        match command {
            Command::Copy => {
                if let Some(text) = self.focused().and_then(|pane| pane.term.lock().selection_to_string()) {
                    return clipboard::write(text);
                }
            }
            Command::Paste => return clipboard::read().map(Message::Pasted),
            Command::SelectAll => {
                if let Some(pane) = self.focused() {
                    let mut term = pane.term.lock();
                    let mut selection = Selection::new(
                        SelectionType::Simple,
                        GridPoint::new(term.topmost_line(), Column(0)),
                        Side::Left,
                    );
                    selection.update(GridPoint::new(term.bottommost_line(), term.last_column()), Side::Right);
                    term.selection = Some(selection);
                }
            }
            Command::Find => {
                if self.find.is_none() {
                    let mut find = Find::default();
                    find.pane = self.focused().map(|pane| pane.id);
                    self.find = Some(find);
                }
                return Task::batch([operation::focus(FIND_ID), operation::select_all(FIND_ID)]);
            }
            Command::FindUp | Command::FindDown if self.find.is_some() => {
                return self.find_step(if command == Command::FindUp { Direction::Left } else { Direction::Right });
            }
            Command::FindUp | Command::FindDown => return self.command(Command::Find),
            Command::UseSelectionForFind => {
                let selected = self.focused().and_then(|pane| pane.term.lock().selection_to_string());
                if let Some(query) = selected.filter(|text| !text.trim().is_empty() && !text.contains('\n')) {
                    let focus = self.command(Command::Find);
                    if let Some(find) = &mut self.find {
                        find.query = query;
                    }
                    return Task::batch([focus, self.rebuild_find()]);
                }
            }
            Command::ClearScrollback => {
                if let Some(pane) = self.focused() {
                    let mut term = pane.term.lock();
                    let line = term.grid().cursor.point.line.0.max(0) as usize;
                    if !term.mode().contains(TermMode::ALT_SCREEN) && line > 0 {
                        let region = Line(0)..Line(term.screen_lines() as i32);
                        term.grid_mut().scroll_up(&region, line);
                        term.grid_mut().cursor.point.line = Line(0);
                    }
                    term.grid_mut().clear_history();
                    term.selection = None;
                }
            }
            Command::Reset => {
                if let Some(pane) = self.focused() {
                    let mut processor: Processor = Processor::new();
                    processor.advance(&mut *pane.term.lock(), b"\x1bc");
                    pane.write(&b"\x0c"[..]);
                }
            }
            Command::Export => {
                if let Some(pane) = self.focused() {
                    let request = export::Request {
                        term: Arc::clone(&pane.term),
                        title: Text::ExportText.text(self.lang).trim_end_matches('…').to_string(),
                        name: Text::ExportName.text(self.lang).to_string(),
                        folder: pane.directory.clone().or_else(home),
                        revision: Arc::clone(&pane.revision),
                    };
                    return Task::perform(export::spawn(request), |saved| Message::Exported(saved.ok()));
                }
            }
            Command::Hints => {
                if let Some(pane) = self.focused() {
                    let items = hints::scan(&pane.term.lock());
                    let id = pane.id;
                    self.hint_mode = (!items.is_empty()).then(|| HintMode { pane: id, items, typed: String::new() });
                }
            }
            Command::Mark(mark) => {
                if let Some(pane) = self.focused() {
                    marks::toggle(&mut pane.term.lock(), mark);
                }
            }
            Command::Jump(direction, bookmarks) => {
                if let Some(pane) = self.focused() {
                    let id = pane.id;
                    if marks::jump(&mut pane.term.lock(), direction, bookmarks) {
                        self.flashes.insert(id, (Instant::now(), Flash::Mark));
                    }
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn adjust(&mut self, command: Command) -> Task<Message> {
        match command {
            Command::Bigger => self.zoom = (self.zoom + 1.0).min(48.0 - self.prefs.font_size),
            Command::Smaller => self.zoom = (self.zoom - 1.0).max(6.0 - self.prefs.font_size),
            Command::ActualSize => self.zoom = 0.0,
            Command::FullScreen => return Task::done(Message::Window(WindowAction::ToggleFullscreen)),
            Command::ScrollPage(up) | Command::ScrollEdge(up) => {
                let scroll = match (command, up) {
                    (Command::ScrollPage(_), true) => Scroll::PageUp,
                    (Command::ScrollPage(_), false) => Scroll::PageDown,
                    (_, true) => Scroll::Top,
                    (_, false) => Scroll::Bottom,
                };
                if let Some(pane) = self.focused() {
                    pane.term.lock().scroll_display(scroll);
                }
            }
            Command::Options => self.toggle_menu(MenuKind::Options, Point::new(self.window.size.width - 44.0, 40.0)),
            Command::Profiles => self.toggle_menu(MenuKind::Profiles, Point::new(self.window.size.width - 44.0, 40.0)),
            Command::SetProfile(profile) => {
                if let Some(tab) = self.tab_mut() {
                    tab.profile = profile;
                }
            }
            Command::MakeDefault => {
                if let Some(profile) = self.tab().map(|tab| tab.profile) {
                    let mut prefs = self.prefs.clone();
                    prefs.profile = profile;
                    match config::save(&prefs) {
                        Ok(()) => {
                            self.prefs = prefs;
                            self.stamp = stamp();
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
                self.dismiss_menu();
            }
            Command::Shortcuts => self.shortcuts_asked = self.confirm.is_none() && self.closing_confirm.is_none(),
            _ => {}
        }
        Task::none()
    }

    fn toggle_menu(&mut self, kind: MenuKind, at: Point) {
        if self.menu.as_ref().is_some_and(|menu| menu.kind == kind) {
            if self.menu_open() {
                self.dismiss_menu();
            } else if let Some(menu) = &mut self.menu {
                menu.reveal.open(motion::t2());
                menu.reveal = menu.reveal.with_ease(motion::ease_out);
            }
        } else {
            let at = self.menu.as_ref().filter(|menu| menu.kind == MenuKind::Options).map_or(at, |menu| menu.at);
            self.menu = Some(Menu::new(kind, at, Some(0)));
        }
    }

    fn menu_open(&self) -> bool {
        self.menu.as_ref().is_some_and(|menu| menu.reveal.is_open())
    }

    fn dismiss_menu(&mut self) {
        if let Some(menu) = &mut self.menu {
            menu.reveal.close(motion::t2());
            menu.reveal = menu.reveal.with_ease(motion::ease_io);
        }
    }

    fn offer_shortcuts(&mut self, shell: &mut glide_sdk::Shell<Message>) {
        if std::mem::take(&mut self.shortcuts_asked) {
            shell.dialog(
                glide_sdk::Dialog::new(Text::Shortcuts.text(self.lang), Text::ShortcutsBody.text(self.lang))
                    .cancel(Text::Done.text(self.lang)),
            );
        }
    }
}

impl glide_sdk::App for App {
    type Message = Message;

    fn new(_: &mut glide_sdk::Shell<Message>) -> (Self, Task<Message>) {
        Self::new()
    }

    fn title(&self, _: &glide_sdk::Shell<Message>) -> String {
        Self::title(self)
    }

    fn update(&mut self, message: Message, shell: &mut glide_sdk::Shell<Message>) -> Task<Message> {
        if let Message::Window(action) = message {
            shell.window_action(action);
        }
        let task = Self::update(self, message);
        self.offer_shortcuts(shell);
        task
    }

    fn view<'a>(&'a self, _: &'a glide_sdk::Shell<Message>) -> iced::Element<'a, Message> {
        Self::view(self)
    }

    fn subscription(&self) -> Subscription<Message> {
        Self::subscription(self)
    }

    fn close_requested(&mut self, _: &mut glide_sdk::Shell<Message>) -> Task<Message> {
        Self::update(self, Message::CloseRequested)
    }

    fn window_changed(&mut self, shell: &mut glide_sdk::Shell<Message>) -> Task<Message> {
        self.follow_window(*shell.window());
        Task::none()
    }

    fn key(&mut self, event: &keyboard::Event, captured: bool, shell: &mut glide_sdk::Shell<Message>) -> Task<Message> {
        let task = Self::update(self, Message::Key(event.clone(), captured));
        self.offer_shortcuts(shell);
        task
    }

    fn paint(&self, _: &glide_sdk::Shell<Message>) -> Paint {
        self.paint
    }

    fn look_changed(&mut self, shell: &mut glide_sdk::Shell<Message>) -> Task<Message> {
        self.follow_look(shell.look(), *shell.paint());
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::key::{Code, NativeCode, Physical};

    fn unidentified() -> Physical {
        Physical::Unidentified(NativeCode::Unidentified)
    }

    #[test]
    fn plain_control_letters_always_reach_the_shell() {
        for letter in 'a'..='z' {
            let key = Key::Character(letter.to_string().into());
            assert_eq!(shortcut(&key, unidentified(), Modifiers::CTRL), None, "Ctrl+{letter}");
            assert_eq!(shortcut(&key, unidentified(), Modifiers::ALT), None, "Alt+{letter}");
        }
    }

    #[test]
    fn super_combinations_belong_to_the_compositor() {
        let key = Key::Character("t".into());
        assert_eq!(shortcut(&key, unidentified(), Modifiers::LOGO | Modifiers::CTRL | Modifiers::SHIFT), None);
        assert_eq!(shortcut(&key, unidentified(), Modifiers::CTRL | Modifiers::SHIFT), Some(Command::NewTab));
    }

    #[test]
    fn shortcuts_follow_the_physical_key_on_cyrillic_layouts() {
        let key = Key::Character("Е".into());
        let command = shortcut(&key, Physical::Code(Code::KeyT), Modifiers::CTRL | Modifiers::SHIFT);
        assert_eq!(command, Some(Command::NewTab));
    }

    #[test]
    fn arguments_accept_commands_directories_and_uris() {
        let launch = parse_args(["-e", "htop", "-d", "5"].map(String::from));
        assert_eq!(launch.command, Some(("htop".into(), vec!["-d".into(), "5".into()])));
        let launch = parse_args(["-e", "ls -la"].map(String::from));
        assert_eq!(launch.command, Some(("sh".into(), vec!["-c".into(), "ls -la".into()])));
        let launch = parse_args(["--working-directory=/tmp"].map(String::from));
        assert_eq!(launch.directory, Some(PathBuf::from("/tmp")));
        let launch = parse_args(["file:///home/me/My%20Files"].map(String::from));
        assert_eq!(launch.directory, Some(PathBuf::from("/home/me/My Files")));
    }

    #[test]
    fn find_queries_are_literal() {
        assert_eq!(escape("a.b*(c)"), "a\\.b\\*\\(c\\)");
        assert!(RegexSearch::new(&format!("(?i){}", escape("[x]+?"))).is_ok());
    }

    fn press(key: Key, text: Option<&str>) -> Message {
        press_with(key, text, Modifiers::empty())
    }

    fn press_with(key: Key, text: Option<&str>, modifiers: Modifiers) -> Message {
        Message::Key(
            keyboard::Event::KeyPressed {
                modified_key: key.clone(),
                key,
                physical_key: unidentified(),
                location: keyboard::Location::Standard,
                modifiers,
                text: text.map(Into::into),
                repeat: false,
            },
            false,
        )
    }

    #[test]
    fn typed_keys_reach_the_program_and_return_marks_the_prompt_line() {
        let (sender, mut receiver) = iced::futures::channel::mpsc::unbounded();
        let (mut app, _) = App::with_launch(Launch { command: Some(("cat".into(), Vec::new())), directory: None });
        app.prefs = Preferences::default();
        let _ = app.update(Message::Feed(Feed::Connected(Bus(sender))));
        for (key, text) in [("h", Some("h")), ("i", None)] {
            let _ = app.update(press(Key::Character(key.into()), text));
        }
        let _ = app.update(press(Key::Named(Named::Enter), None));
        let deadline = Instant::now() + Duration::from_secs(10);
        let screen = |app: &App| {
            let term = app.focused().expect("a pane").term.lock();
            term.bounds_to_string(GridPoint::new(Line(0), Column(0)), GridPoint::new(Line(1), term.last_column()))
        };
        let mut pump = |app: &mut App| {
            let mut events = Vec::new();
            while let Ok(event) = receiver.try_recv() {
                events.push(event);
            }
            let _ = app.update(Message::Feed(Feed::Events(events)));
        };
        while !screen(&app).starts_with("hi\nhi") && Instant::now() < deadline {
            pump(&mut app);
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(50));
        pump(&mut app);
        assert!(screen(&app).starts_with("hi\nhi"), "{:?}", screen(&app));
        let term = app.focused().expect("a pane").term.lock();
        assert_eq!(marks::at(&term, Line(0)), Some(Mark::Line));
        assert_eq!(marks::at(&term, Line(1)), None);
    }

    #[test]
    fn hint_mode_narrows_by_typed_labels_resets_on_a_miss_and_ends_on_a_pick() {
        let (sender, mut receiver) = iced::futures::channel::mpsc::unbounded();
        let script = "printf 'https://a.example ~/one\\n'; exec cat";
        let launch = Launch { command: Some(("sh".into(), vec!["-c".into(), script.into()])), directory: None };
        let (mut app, _) = App::with_launch(launch);
        let _ = app.update(Message::Feed(Feed::Connected(Bus(sender))));
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let mut events = Vec::new();
            while let Ok(event) = receiver.try_recv() {
                events.push(event);
            }
            let _ = app.update(Message::Feed(Feed::Events(events)));
            let _ = app.update(Message::Command(Command::Hints));
            if app.hint_mode.as_ref().is_some_and(|mode| mode.items.len() == 2) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let labels = |app: &App| app.hint_mode.as_ref().map(|mode| mode.typed.clone());
        assert_eq!(app.hint_mode.as_ref().map(|mode| mode.items.len()), Some(2));
        let _ = app.update(press(Key::Character("x".into()), Some("x")));
        assert_eq!(labels(&app), Some(String::new()), "a letter no label starts with clears the input");
        let _ = app.update(press(Key::Named(Named::Escape), None));
        assert!(app.hint_mode.is_none());
        let _ = app.update(Message::Command(Command::Hints));
        let _ = app.update(press_with(Key::Character("S".into()), Some("S"), Modifiers::SHIFT));
        assert!(app.hint_mode.is_none(), "a full label picks its hint, Shift copies instead of opening");
    }

    #[test]
    fn a_tab_the_strip_reorders_moves_with_its_panes_and_stays_selected() {
        let (sender, _receiver) = iced::futures::channel::mpsc::unbounded();
        let (mut app, _) = App::with_launch(Launch { command: Some(("cat".into(), Vec::new())), directory: None });
        let _ = app.update(Message::Feed(Feed::Connected(Bus(sender))));
        let _ = app.update(Message::Command(Command::NewTab));
        let _ = app.update(Message::Command(Command::NewTab));
        let ids = |app: &App| app.tabs.iter().map(|tab| tab.focused().expect("a pane").id).collect::<Vec<_>>();
        let before = ids(&app);
        let _ = app.update(Message::TabPressed(0));
        let _ = app.update(Message::TabMoved(0, 1));
        let _ = app.update(Message::TabMoved(1, 2));
        assert_eq!(ids(&app), vec![before[1], before[2], before[0]]);
        assert_eq!(app.active, 2);
        let _ = app.update(Message::TabMoved(2, 9));
        assert_eq!(ids(&app), vec![before[1], before[2], before[0]], "a slot past the end is the last one");
        let _ = app.update(Message::Command(Command::MoveTab(-1)));
        assert_eq!(ids(&app), vec![before[1], before[0], before[2]]);
        assert_eq!(app.active, 1);
    }

    fn review_app() -> App {
        let (sender, _receiver) = iced::futures::channel::mpsc::unbounded();
        let (mut app, _) = App::with_launch(Launch { command: Some(("cat".into(), Vec::new())), directory: None });
        app.prefs = Preferences { shell: Some("cat".into()), ..Preferences::default() };
        let _ = app.update(Message::Feed(Feed::Connected(Bus(sender))));
        app
    }

    #[test]
    fn confirmation_follows_its_tab_when_an_earlier_tab_exits() {
        let mut app = review_app();
        let _ = app.update(Message::Command(Command::NewTab));
        let _ = app.update(Message::Command(Command::NewTab));
        let ids: Vec<_> = app.tabs.iter().map(|tab| tab.id).collect();
        let _ = app.request_close(Scope::Tab(ids[1]));
        assert!(app.confirm.is_some());
        let _ = app.close(Scope::Tab(ids[0]));
        let _ = app.update(Message::Confirm(true));
        assert_eq!(app.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(), [ids[2]]);
    }

    #[test]
    fn explicitly_launched_programs_ask_before_closing_and_enter_defaults_to_cancel() {
        let mut app = review_app();
        let _ = app.update(Message::CloseRequested);
        assert!(app.confirm.is_some());
        let _ = app.update(press(Key::Named(Named::Enter), None));
        assert!(app.confirm.is_none());
        assert_eq!(app.tabs.len(), 1);
    }

    #[test]
    fn reopening_find_rejects_an_old_sessions_outcome() {
        let mut app = review_app();
        let _ = app.update(Message::Command(Command::Find));
        let _ = app.update(Message::FindInput("foo".into()));
        let previous = app.find.as_ref().unwrap().generation;
        let _ = app.update(Message::FindClose);
        let _ = app.update(Message::Command(Command::Find));
        let _ = app.update(Message::FindInput("bar".into()));
        assert_ne!(app.find.as_ref().unwrap().generation, previous);
        let revision = app.focused().unwrap().revision.load(Ordering::Acquire);
        let _ = app.update(Message::Found(Some(find::Outcome {
            generation: previous,
            found: None,
            total: 999,
            index: 999,
            revision,
        })));
        assert_eq!(app.find.as_ref().unwrap().total, 0);
    }

    #[test]
    fn switching_panes_cancels_find_and_transfers_terminal_focus() {
        let mut app = review_app();
        let first = app.focused().unwrap().id;
        let _ = app.update(Message::Command(Command::SplitRight));
        let second = app.focused().unwrap().id;
        assert_ne!(first, second);
        assert!(!app.pane_mut(first).unwrap().term.lock().is_focused);
        assert!(app.focused().unwrap().term.lock().is_focused);
        let _ = app.update(Message::Command(Command::Find));
        let _ = app.update(Message::Command(Command::FocusPane(pane_grid::Direction::Left)));
        assert!(app.find.is_none());
        assert!(app.focused().unwrap().term.lock().is_focused);
        assert!(!app.pane_mut(second).unwrap().term.lock().is_focused);
    }

    #[test]
    fn f3_without_find_reaches_the_program_and_tab_changes_end_hints() {
        let mut app = review_app();
        let _ = app.update(press(Key::Named(Named::F3), None));
        assert!(app.find.is_none());
        app.hint_mode = Some(HintMode { pane: app.focused().unwrap().id, items: Vec::new(), typed: String::new() });
        let _ = app.update(Message::Command(Command::NewTab));
        assert!(app.hint_mode.is_none());
    }

    #[test]
    fn local_file_uris_preserve_bytes_and_reject_remote_hosts_and_nuls() {
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(local_uri_path("file:///tmp/a%ff").unwrap().as_os_str().as_bytes(), b"/tmp/a\xff");
        assert_eq!(local_uri_path("file://localhost/tmp/a"), Some(PathBuf::from("/tmp/a")));
        assert!(local_uri_path("file://remote/tmp/a").is_none());
        assert!(local_uri_path("file:///tmp/a%00").is_none());
    }

    #[test]
    fn a_new_look_from_the_host_rethemes_the_app_without_touching_its_preferences() {
        let mut app = review_app();
        let stamp = app.stamp;
        let look = ShellLook::parse("[general]\nlanguage = \"en\"\n[topbar]\nappearance = \"light\"\n", false);
        app.follow_look(&look, look.paint());
        assert_eq!(app.paint.tokens.appearance, glide_sdk::theme::Appearance::Light);
        assert_eq!(app.lang, Lang::En);
        assert_eq!(app.stamp, stamp);
    }

    #[test]
    fn menu_arrows_select_actions_and_escape_starts_a_reversible_close() {
        let mut app = review_app();
        let _ = app.update(Message::Command(Command::Options));
        let _ = app.update(press(Key::Named(Named::ArrowDown), None));
        assert_eq!(app.menu.as_ref().unwrap().selected, Some(1));
        let _ = app.update(press(Key::Named(Named::End), None));
        assert_eq!(
            app.menu.as_ref().unwrap().selected,
            Some(crate::chrome::menu_actions(&app, &MenuKind::Options).len() - 1)
        );
        let _ = app.update(press(Key::Named(Named::Escape), None));
        assert!(!app.menu_open());
        let _ = app.update(Message::Command(Command::Options));
        assert!(app.menu_open());
    }
}

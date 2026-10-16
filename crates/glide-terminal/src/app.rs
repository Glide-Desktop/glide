use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant, SystemTime};

use alacritty_terminal::event::Event as TermEvent;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::term::{ClipboardType, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use glide_theme::motion;
use iced::keyboard::key::Named;
use iced::keyboard::{self, Key, Modifiers};
use iced::widget::{operation, pane_grid};
use iced::{Point, Size, Subscription, Task, clipboard, window};

use crate::config::{self, OnExit, Preferences};
use crate::export;
use crate::find;
use crate::hints;
use crate::i18n::{Key as Text, Lang};
use crate::input;
use crate::marks::{self, Mark};
use crate::profile::Profile;
use crate::session::{self, Bus, Feed, Launch, Pane, PaneId};
use crate::sound;
use crate::style::Paint;
use crate::terminal::{Action, Flash, Fonts, Look};

pub const FIND_ID: &str = "find";
const BLINK: Duration = Duration::from_millis(530);
const POLL: Duration = Duration::from_secs(1);
const BELL_GAP: Duration = Duration::from_millis(250);
const DRAG_SLOP: f32 = 6.0;
const NOTICE: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy)]
struct TabDrag {
    index: usize,
    start: Option<f32>,
    moved: bool,
}

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
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuKind {
    Options,
    Context(Option<String>),
    Profiles,
}

#[derive(Debug, Clone)]
pub struct Menu {
    pub kind: MenuKind,
    pub at: Point,
    pub opened: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Pane(PaneId),
    Tab(usize),
    Window,
}

#[derive(Debug, Clone)]
pub struct Confirm {
    pub scope: Scope,
    pub processes: Vec<String>,
    pub opened: Instant,
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
}

fn select(term: &mut alacritty_terminal::term::Term<session::Listener>, found: &Match) {
    let mut selection = Selection::new(SelectionType::Simple, *found.start(), Side::Left);
    selection.update(*found.end(), Side::Right);
    term.selection = Some(selection);
    term.scroll_to_point(*found.start());
}

pub struct Tab {
    pub panes: pane_grid::State<Pane>,
    pub focus: pane_grid::Pane,
    pub profile: Profile,
    pub bell: bool,
    pub activity: bool,
}

impl Tab {
    #[must_use]
    pub fn focused(&self) -> Option<&Pane> {
        self.panes.get(self.focus)
    }

    fn handle(&self, id: PaneId) -> Option<pane_grid::Pane> {
        self.panes.iter().find(|(_, pane)| pane.id == id).map(|(handle, _)| *handle)
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Feed(Feed),
    Terminal(Action),
    Key(keyboard::Event, bool),
    Command(Command),
    PaneClicked(pane_grid::Pane),
    SplitResized(pane_grid::ResizeEvent),
    TabHovered(Option<usize>),
    TabPressed(usize),
    TabMoved(f32),
    TabReleased,
    FindInput(String),
    FindSubmit,
    FindCase,
    FindClose,
    Found(Option<find::Outcome>),
    Exported(Option<export::Saved>),
    Pasted(Option<String>),
    Open(String),
    CopyText(String),
    Dismiss,
    Confirm(bool),
    Focused(bool),
    Resized(Size),
    CloseRequested,
    Tick,
    Blink,
    Frame,
    WindowDrag,
    WindowMaximize,
    WindowResize(window::Direction),
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
    pub hint_mode: Option<HintMode>,
    pub notice: Option<(String, Instant)>,
    pub menu: Option<Menu>,
    pub confirm: Option<Confirm>,
    pub hovered_tab: Option<usize>,
    tab_drag: Option<TabDrag>,
    pub window: Size,
    pub error: Option<String>,
    zoom: f32,
    font: iced::Font,
    modifiers: Modifiers,
    focused_window: bool,
    typed: Instant,
    flashes: HashMap<PaneId, (Instant, Flash)>,
    prompts: HashMap<PaneId, marks::Prompt>,
    launch: Launch,
    bus: Option<Bus>,
    next_id: u64,
    stamps: (Option<SystemTime>, Option<SystemTime>),
}

#[must_use]
pub fn uri_path(arg: &str) -> PathBuf {
    let Some(rest) = arg.strip_prefix("file://") else { return PathBuf::from(arg) };
    let path = rest.find('/').map_or(rest, |slash| &rest[slash..]);
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let decoded = (bytes[index] == b'%')
            .then(|| bytes.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok());
        if let Some(byte) = decoded {
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    PathBuf::from(String::from_utf8_lossy(&out).into_owned())
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
        _ => return None,
    })
}

fn home() -> Option<PathBuf> {
    glide_config::home()
}

#[must_use]
pub fn directory_label(directory: Option<&Path>) -> String {
    let Some(directory) = directory else { return String::new() };
    if home().as_deref() == Some(directory) {
        return "~".to_string();
    }
    directory.file_name().map_or_else(|| directory.display().to_string(), |name| name.to_string_lossy().into_owned())
}

fn family(prefs: &Preferences) -> iced::Font {
    match prefs.font_family.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => iced::Font::with_name(Box::leak(name.to_string().into_boxed_str())),
        _ => crate::MONO,
    }
}

fn open_detached(program: &str, args: &[&str]) {
    if let Ok(mut child) = std::process::Command::new(program).args(args).spawn() {
        std::thread::spawn(move || child.wait());
    }
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        Self::with_launch(parse_args(std::env::args().skip(1)))
    }

    pub fn with_launch(mut launch: Launch) -> (Self, Task<Message>) {
        let look = config::load_look();
        motion::set_scale(look.motion_scale);
        let prefs = config::load();
        if launch.command.is_none()
            && let Some(shell) = prefs.shell.clone()
        {
            launch.command = Some((shell, Vec::new()));
        }
        let app = Self {
            paint: Paint::new(look.appearance, look.accent),
            lang: look.language.unwrap_or_else(Lang::from_env),
            sounds: look.sounds,
            rung: None,
            font: family(&prefs),
            modifiers: Modifiers::empty(),
            prefs,
            tabs: Vec::new(),
            active: 0,
            find: None,
            hint_mode: None,
            notice: None,
            menu: None,
            confirm: None,
            hovered_tab: None,
            tab_drag: None,
            window: Size::new(960.0, 600.0),
            error: None,
            zoom: 0.0,
            focused_window: true,
            typed: Instant::now(),
            flashes: HashMap::new(),
            prompts: HashMap::new(),
            launch,
            bus: None,
            next_id: 1,
            stamps: (config::stamp(config::shell_path().as_deref()), config::stamp(config::path().as_deref())),
        };
        (app, Task::none())
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
        self.focused_window
    }

    #[must_use]
    pub fn view(&self) -> iced::Element<'_, Message> {
        crate::chrome::view(self)
    }

    #[must_use]
    pub fn man_topic(&self) -> Option<String> {
        let selected = self.focused()?.term.lock().selection_to_string()?;
        let topic = selected.trim();
        let word = |c: char| c.is_alphanumeric() || "-_.:+".contains(c);
        (!topic.is_empty() && topic.len() <= 64 && topic.chars().all(word)).then(|| topic.to_string())
    }

    #[must_use]
    pub fn blink_visible(&self) -> bool {
        !self.focused_window
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
            || self.menu.as_ref().is_some_and(|menu| opening(menu.opened))
            || self.confirm.as_ref().is_some_and(|confirm| opening(confirm.opened))
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
        if pane.busy { pane.process.clone() } else { directory_label(pane.directory.as_deref()) }
    }

    #[must_use]
    pub fn iced_theme(&self) -> iced::Theme {
        self.paint.iced_theme()
    }

    #[must_use]
    pub fn style(&self, _theme: &iced::Theme) -> iced::theme::Style {
        iced::theme::Style { background_color: iced::Color::TRANSPARENT, text_color: self.paint.fg_1() }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            Subscription::run(session::feed).map(Message::Feed),
            iced::event::listen_with(|event, status, _| match event {
                iced::Event::Keyboard(event) => Some(Message::Key(event, status == iced::event::Status::Captured)),
                iced::Event::Window(window::Event::Focused) => Some(Message::Focused(true)),
                iced::Event::Window(window::Event::Unfocused) => Some(Message::Focused(false)),
                iced::Event::Window(window::Event::Resized(size) | window::Event::Opened { size, .. }) => {
                    Some(Message::Resized(size))
                }
                _ => None,
            }),
            window::close_requests().map(|_| Message::CloseRequested),
            iced::time::every(POLL).map(|_| Message::Tick),
        ];
        if self.focused_window && self.prefs.cursor_blink && !motion::is_disabled() {
            subscriptions.push(iced::time::every(BLINK).map(|_| Message::Blink));
        }
        if self.tab_drag.is_some() {
            subscriptions.push(iced::event::listen_with(|event, _, _| {
                let released = iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left));
                (event == released).then_some(Message::TabReleased)
            }));
        }
        if self.animating() {
            subscriptions.push(window::frames().map(|_| Message::Frame));
        }
        Subscription::batch(subscriptions)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Feed(Feed::Connected(bus)) => {
                self.bus = Some(bus);
                if self.tabs.is_empty() {
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
                Task::none()
            }
            Message::Key(event, captured) => self.on_key(&event, captured),
            Message::Command(command) => self.command(command),
            Message::PaneClicked(handle) => {
                if let Some(tab) = self.tab_mut() {
                    tab.focus = handle;
                }
                Task::none()
            }
            Message::SplitResized(event) => {
                if let Some(tab) = self.tab_mut() {
                    tab.panes.resize(event.split, event.ratio);
                }
                Task::none()
            }
            Message::TabPressed(index) => {
                self.select(index);
                self.tab_drag = Some(TabDrag { index, start: None, moved: false });
                Task::none()
            }
            Message::TabMoved(x) => {
                self.drag_tab(x);
                Task::none()
            }
            Message::TabReleased => {
                self.tab_drag = None;
                Task::none()
            }
            Message::TabHovered(index) => {
                self.hovered_tab = index;
                Task::none()
            }
            Message::FindInput(_)
            | Message::FindSubmit
            | Message::FindCase
            | Message::FindClose
            | Message::Found(_) => self.on_find(message),
            Message::Focused(_)
            | Message::Resized(_)
            | Message::CloseRequested
            | Message::Tick
            | Message::Blink
            | Message::Frame
            | Message::WindowDrag
            | Message::WindowMaximize
            | Message::WindowResize(_) => self.on_window(&message),
            Message::Pasted(text) => {
                if let Some(text) = text {
                    self.paste(&text);
                }
                Task::none()
            }
            Message::Open(uri) => {
                self.menu = None;
                open_detached("xdg-open", &[&uri]);
                Task::none()
            }
            Message::CopyText(text) => {
                self.menu = None;
                clipboard::write(text)
            }
            Message::Dismiss => {
                self.menu = None;
                Task::none()
            }
            Message::Confirm(accepted) => {
                let confirm = self.confirm.take();
                match confirm {
                    Some(confirm) if accepted => self.close(confirm.scope),
                    _ => Task::none(),
                }
            }
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
            Message::Focused(focused) => {
                self.focused_window = focused;
                self.typed = Instant::now();
                if let Some(pane) = self.focused() {
                    let mut term = pane.term.lock();
                    term.is_focused = focused;
                    if term.mode().contains(TermMode::FOCUS_IN_OUT) {
                        pane.write(if focused { &b"\x1b[I"[..] } else { &b"\x1b[O"[..] });
                    }
                }
                Task::none()
            }
            Message::Resized(size) => {
                self.window = size;
                Task::none()
            }
            Message::CloseRequested => self.request_close(Scope::Window),
            Message::Tick => {
                for tab in &mut self.tabs {
                    for (_, pane) in tab.panes.iter_mut() {
                        pane.refresh();
                    }
                }
                self.settle_prompts();
                self.notice.take_if(|(_, at)| at.elapsed() >= NOTICE);
                self.reload();
                Task::none()
            }
            Message::Blink | Message::Frame => {
                self.flashes.retain(|_, (at, _)| at.elapsed() < motion::t3());
                Task::none()
            }
            Message::WindowDrag => window::latest().and_then(window::drag),
            Message::WindowMaximize => window::latest().and_then(window::toggle_maximize),
            Message::WindowResize(direction) => window::latest().and_then(move |id| window::drag_resize(id, direction)),
            _ => Task::none(),
        }
    }

    fn reload(&mut self) {
        let stamps = (config::stamp(config::shell_path().as_deref()), config::stamp(config::path().as_deref()));
        if stamps == self.stamps {
            return;
        }
        self.stamps = stamps;
        let look = config::load_look();
        motion::set_scale(look.motion_scale);
        self.paint = Paint::new(look.appearance, look.accent);
        self.lang = look.language.unwrap_or_else(Lang::from_env);
        self.sounds = look.sounds;
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
            command: self.prefs.shell.clone().map(|shell| (shell, Vec::new())),
            directory: self.focused().and_then(|pane| pane.directory.clone()).or_else(home),
        }
    }

    fn open_tab(&mut self, launch: &Launch) {
        let Some(pane) = self.spawn(launch) else { return };
        let (panes, focus) = pane_grid::State::new(pane);
        let profile = self.tab().map_or(self.prefs.profile, |tab| tab.profile);
        let index = if self.tabs.is_empty() { 0 } else { self.active + 1 };
        self.tabs.insert(index, Tab { panes, focus, profile, bell: false, activity: false });
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
            self.active = index;
            self.tabs[index].bell = false;
            self.tabs[index].activity = false;
            self.find = None;
        }
    }

    fn request_close(&mut self, scope: Scope) -> Task<Message> {
        let mut busy = Vec::new();
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            for (_, pane) in tab.panes.iter_mut() {
                let inside = match scope {
                    Scope::Pane(id) => pane.id == id,
                    Scope::Tab(tab) => index == tab,
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
            self.menu = None;
            self.confirm = Some(Confirm { scope, processes: busy, opened: Instant::now() });
            return Task::none();
        }
        self.close(scope)
    }

    fn close(&mut self, scope: Scope) -> Task<Message> {
        match scope {
            Scope::Window => return iced::exit(),
            Scope::Tab(index) if index < self.tabs.len() => {
                self.tabs.remove(index);
                if index < self.active || self.active >= self.tabs.len() {
                    self.active = self.active.saturating_sub(1);
                }
            }
            Scope::Tab(_) => {}
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
                    return self.close(Scope::Tab(index));
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
        if self.focused_window {
            Task::none()
        } else {
            window::latest()
                .and_then(|window| window::request_user_attention(window, Some(window::UserAttention::Informational)))
        }
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
        let note = match status.code() {
            Some(code) if code != 0 => format!("{} {code}]", Text::CompletedWithCode.text(lang)),
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
            }
            Action::Focus(id) => {
                self.menu = None;
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
                self.menu = Some(Menu { kind: MenuKind::Context(link), at, opened: Instant::now() });
            }
        }
        Task::none()
    }

    fn on_key(&mut self, event: &keyboard::Event, captured: bool) -> Task<Message> {
        let keyboard::Event::KeyPressed { key, physical_key, modifiers, text, .. } = event else {
            if let keyboard::Event::ModifiersChanged(modifiers) = event {
                self.modifiers = *modifiers;
            }
            return Task::none();
        };
        self.modifiers = *modifiers;
        let escape = matches!(key, Key::Named(Named::Escape));
        if let Some(confirm) = &self.confirm {
            let scope = confirm.scope;
            return match key {
                Key::Named(Named::Escape) => self.update(Message::Confirm(false)),
                Key::Named(Named::Enter) => {
                    self.confirm = None;
                    self.close(scope)
                }
                _ => Task::none(),
            };
        }
        if escape && self.menu.is_some() {
            self.menu = None;
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
        if let Some(command) = shortcut(key, *physical_key, *modifiers) {
            return self.command(command);
        }
        if self.menu.take().is_some() {
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
        if let Some(bytes) = input::encode(key, *physical_key, *modifiers, text.as_deref(), mode, alt_is_meta) {
            let prompt = (mark_prompts && enter).then(|| marks::prompt(&mut term)).flatten();
            term.scroll_display(Scroll::Bottom);
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

    fn use_hint(&self, pane: PaneId, hint: &hints::Hint, copy: bool) -> Task<Message> {
        if copy {
            return clipboard::write(hint.text.clone());
        }
        let cwd = self.locate(pane).and_then(|(index, handle)| self.tabs[index].panes.get(handle)?.directory.clone());
        open_detached("xdg-open", &[&hints::target(hint, home().as_deref(), cwd.as_deref())]);
        Task::none()
    }

    #[must_use]
    pub fn dragged_tab(&self) -> Option<usize> {
        self.tab_drag.filter(|drag| drag.moved).map(|drag| drag.index)
    }

    fn drag_tab(&mut self, x: f32) {
        let count = self.tabs.len();
        let Some(drag) = &mut self.tab_drag else { return };
        let start = *drag.start.get_or_insert(x);
        if !drag.moved && (x - start).abs() < DRAG_SLOP {
            return;
        }
        drag.moved = true;
        let target = crate::chrome::tab_slot(x, self.window.width, count);
        if target != drag.index && drag.index < count {
            let tab = self.tabs.remove(drag.index);
            self.tabs.insert(target, tab);
            drag.index = target;
            self.active = target;
            self.hovered_tab = Some(target);
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

    fn paste(&mut self, text: &str) {
        let Some(pane) = self.focused() else { return };
        let mut term = pane.term.lock();
        let bracketed = term.mode().contains(TermMode::BRACKETED_PASTE);
        term.scroll_display(Scroll::Bottom);
        drop(term);
        let bytes = if bracketed {
            format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
        } else {
            text.replace("\r\n", "\r").replace('\n', "\r")
        };
        pane.write(bytes.into_bytes());
    }

    fn rebuild_find(&mut self) -> Task<Message> {
        let Some(find) = &mut self.find else { return Task::none() };
        let flag = if find.case { "(?-i)" } else { "(?i)" };
        find.pattern = if find.query.is_empty() { String::new() } else { format!("{flag}{}", escape(&find.query)) };
        find.regex = (!find.query.is_empty()).then(|| RegexSearch::new(&find.pattern).ok()).flatten();
        find.current = None;
        find.total = 0;
        find.index = 0;
        find.counting = false;
        find.generation += 1;
        find.latest.store(find.generation, std::sync::atomic::Ordering::Relaxed);
        self.find_step(Direction::Left)
    }

    fn find_step(&mut self, direction: Direction) -> Task<Message> {
        let Some(pane) = self.tabs.get(self.active).and_then(Tab::focused) else { return Task::none() };
        let Some(find) = &mut self.find else { return Task::none() };
        let Some(regex) = &mut find.regex else { return Task::none() };
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
        find.generation += 1;
        find.latest.store(find.generation, std::sync::atomic::Ordering::Relaxed);
        find.counting = true;
        let job = find::Job {
            generation: find.generation,
            latest: Arc::clone(&find.latest),
            term: Arc::clone(&pane.term),
            regex: regex.clone(),
            target,
        };
        Task::perform(find::spawn(job), |outcome| Message::Found(outcome.ok()))
    }

    fn found(&mut self, outcome: find::Outcome) {
        let Some(pane) = self.tabs.get(self.active).and_then(Tab::focused) else { return };
        let Some(find) = self.find.as_mut().filter(|find| find.generation == outcome.generation) else { return };
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
            self.menu = None;
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
            Command::CloseTab(index) => return self.request_close(Scope::Tab(index)),
            Command::CloseWindow => return self.request_close(Scope::Window),
            Command::NextTab if !self.tabs.is_empty() => self.select((self.active + 1) % self.tabs.len()),
            Command::PreviousTab if !self.tabs.is_empty() => {
                self.select((self.active + self.tabs.len() - 1) % self.tabs.len());
            }
            Command::SelectTab(index) => self.select(index),
            Command::MoveTab(step) => {
                let target = self.active as isize + step;
                if (0..self.tabs.len() as isize).contains(&target) {
                    self.tabs.swap(self.active, target as usize);
                    self.active = target as usize;
                }
            }
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
                    self.find = Some(Find::default());
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
            Command::FullScreen => {
                return window::latest().and_then(|id| {
                    window::mode(id).then(move |mode| {
                        let next = if mode == window::Mode::Fullscreen {
                            window::Mode::Windowed
                        } else {
                            window::Mode::Fullscreen
                        };
                        window::set_mode(id, next)
                    })
                });
            }
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
            Command::Options => self.toggle_menu(MenuKind::Options, Point::new(self.window.width - 44.0, 40.0)),
            Command::Profiles => self.toggle_menu(MenuKind::Profiles, Point::new(self.window.width - 44.0, 40.0)),
            Command::SetProfile(profile) => {
                if let Some(tab) = self.tab_mut() {
                    tab.profile = profile;
                }
            }
            Command::MakeDefault => {
                if let Some(profile) = self.tab().map(|tab| tab.profile) {
                    self.prefs.profile = profile;
                    if config::save(&self.prefs).is_ok() {
                        self.stamps.1 = config::stamp(config::path().as_deref());
                    }
                }
                self.menu = None;
            }
            _ => {}
        }
        Task::none()
    }

    fn toggle_menu(&mut self, kind: MenuKind, at: Point) {
        if self.menu.as_ref().is_some_and(|menu| menu.kind == kind) {
            self.menu = None;
        } else {
            let at = self.menu.as_ref().filter(|menu| menu.kind == MenuKind::Options).map_or(at, |menu| menu.at);
            self.menu = Some(Menu { kind, at, opened: Instant::now() });
        }
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
    fn dragging_a_tab_past_the_slop_moves_it_and_keeps_it_selected() {
        let (sender, _receiver) = iced::futures::channel::mpsc::unbounded();
        let (mut app, _) = App::with_launch(Launch { command: Some(("cat".into(), Vec::new())), directory: None });
        let _ = app.update(Message::Feed(Feed::Connected(Bus(sender))));
        let _ = app.update(Message::Command(Command::NewTab));
        let _ = app.update(Message::Command(Command::NewTab));
        app.window = Size::new(616.0, 400.0);
        let ids = |app: &App| app.tabs.iter().map(|tab| tab.focused().expect("a pane").id).collect::<Vec<_>>();
        let before = ids(&app);
        let _ = app.update(Message::TabPressed(0));
        let _ = app.update(Message::TabMoved(20.0));
        let _ = app.update(Message::TabMoved(23.0));
        assert_eq!(ids(&app), before, "a wobble under the slop is still a click");
        let _ = app.update(Message::TabMoved(600.0));
        let _ = app.update(Message::TabReleased);
        assert_eq!(ids(&app), vec![before[1], before[2], before[0]]);
        assert_eq!(app.active, 2);
        assert_eq!(app.dragged_tab(), None);
    }
}

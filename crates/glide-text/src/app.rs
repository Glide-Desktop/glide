use std::collections::HashSet;
use std::ffi::OsString;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use glide_sdk::anim::{Lingering, Reveal};
use glide_sdk::theme::motion;
use iced::futures::channel::oneshot;
use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Key, Modifiers};
use iced::widget::markdown;
use iced::widget::operation;
use iced::widget::text_editor::{Action, Binding, KeyPress, Status};
use iced::{Point, Subscription, Task, clipboard, window};

use crate::config::{self, Preferences, Session, SessionTab};
use crate::document::{self, Decoded, Document, Encoding, Ending, Hazard, LoadError};
use crate::edit::{self, Case, Indent};
use crate::find::{self, Query};
use crate::finder::{self, Finder};
use crate::history::Kind;
use crate::i18n::{Key as Text, Lang};
use crate::syntax::{self, Language, Lines, Marks, Palette, Settings};
use crate::tree::{self, Entry, Tree};
use glide_app::menubar;
use glide_sdk::Paint;
use glide_sdk::chooser::{self, Ask, Chosen, Purpose};
use glide_sdk::look::{self, Look};
pub use glide_sdk::window::Frame;
use glide_sdk::window::{Action as WindowAction, Window};

pub const EDITOR: &str = "glide-text-editor";
pub const FIND_ID: &str = "glide-text-find";
pub(crate) const REPLACE_ID: &str = "glide-text-replace";
pub(crate) const GOTO_ID: &str = "glide-text-goto";
const TICK: Duration = Duration::from_secs(1);
const NOTICE: Duration = Duration::from_secs(4);
const SESSION_EVERY: Duration = Duration::from_secs(10);
const SYNC_HIGHLIGHT: usize = 96 * 1024;
const DETECT_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bar {
    File,
    Edit,
    View,
    Search,
    Format,
    Help,
}

impl Bar {
    pub const ALL: [Self; 6] = [Self::File, Self::Edit, Self::View, Self::Search, Self::Format, Self::Help];

    #[must_use]
    pub const fn key(self) -> Text {
        match self {
            Self::File => Text::File,
            Self::Edit => Text::Edit,
            Self::View => Text::View,
            Self::Search => Text::Search,
            Self::Format => Text::Format,
            Self::Help => Text::Help,
        }
    }

    fn step(self, forward: bool) -> Self {
        let index = Self::ALL.iter().position(|bar| *bar == self).unwrap_or(0);
        let count = Self::ALL.len();
        Self::ALL[if forward { (index + 1) % count } else { (index + count - 1) % count }]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    New,
    NewWindow,
    Open,
    OpenFolder,
    Recent,
    OpenRecent(usize),
    ClearRecent,
    Save,
    SaveAs,
    SaveAll,
    Revert,
    ShowInFiles,
    CopyPath,
    Close,
    CloseTab(usize),
    CloseFolder,
    CloseWindow,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    DuplicateLine,
    DeleteLine,
    MoveLine(bool),
    JoinLines,
    SortLines,
    ToggleComment,
    Indent,
    Unindent,
    Newline,
    Case(Case),
    TrimTrailing,
    InsertDate,
    ToggleCheck,
    Find,
    Replace,
    FindNext,
    FindPrevious,
    UseSelection,
    GoToLine,
    ToggleSidebar,
    ToggleStatusBar,
    ToggleIslands,
    ToggleAutoHideStatus,
    ToggleMonospace,
    TogglePreview,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    FullScreen,
    ToggleHidden,
    RefreshTree,
    Syntax,
    SetLanguage(Option<Language>),
    ToggleSpaces,
    TabWidths,
    TabWidth(usize),
    ToggleAutoIndent,
    ToggleAutoPair,
    ToggleFocus,
    QuickOpen,
    Symbols,
    SearchFolder,
    Endings,
    SetEnding(Ending),
    ToggleTrimOnSave,
    Statistics,
    Settings,
    Shortcuts,
    About,
    NextTab,
    PreviousTab,
    SelectTab(usize),
    MoveTab(isize),
    MenuBar,
    ContextMenu,
    TreeOpen,
    TreeShow,
    TreeCopyPath,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuKind {
    Bar(Bar),
    Compact,
    Context,
    Syntax,
    TabWidths,
    Endings,
    Recent,
    Tree(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Menu {
    pub kind: MenuKind,
    pub at: Point,
    pub selected: Option<usize>,
    pub reveal: Reveal,
}

impl Menu {
    fn new(kind: MenuKind, at: Point, selected: Option<usize>) -> Self {
        let mut reveal = Reveal::closed().with_ease(motion::ease_out);
        reveal.open(motion::t2());
        Self { kind, at, selected, reveal }
    }
}

pub type Item = glide_sdk::menu::Item<Command>;

#[derive(Debug, Clone)]
pub enum FinderMessage {
    Input(String),
    Pick(usize),
    Submit,
    Step(bool),
    Close,
    Filled(u64, Vec<finder::Entry>),
}

pub(crate) const FINDER_ID: &str = "glide-text-finder";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum After {
    Close(u64),
    Window,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogKind {
    Unsaved { docs: Vec<u64>, then: After },
    Revert(u64),
    Encoding { id: u64, path: PathBuf, hazard: Hazard },
    About,
    Statistics,
    Settings,
    Shortcuts,
}

#[derive(Debug, Clone)]
pub struct Dialog {
    pub kind: DialogKind,
    pub reveal: Reveal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    Case,
    Word,
    Regex,
}

#[derive(Debug, Clone, Default)]
pub struct FindBar {
    pub query: Query,
    pub replace: String,
    pub replacing: bool,
    pub found: Vec<Range<usize>>,
    pub current: Option<usize>,
    pub error: Option<String>,
    pub marks: Arc<Marks>,
    pub marked: u64,
    marked_for: Option<(u64, u64, Option<usize>)>,
    revision: u64,
    doc: u64,
}

const MARK_LIMIT: usize = 20_000;

#[derive(Debug, Clone)]
pub enum Message {
    Edit(Action),
    Command(Command),
    Escape,
    Key(keyboard::Event, bool),
    Loaded(PathBuf, Result<Decoded, LoadError>),
    Reloaded(u64, Result<Decoded, LoadError>),
    Chosen(Purpose, Chosen),
    SaveTo(u64, Chosen),
    Saved(u64, PathBuf, u64, Result<SystemTime, String>),
    Highlighted(u64, u64, Language, Arc<Lines>),
    Listed(PathBuf, Result<Vec<Entry>, String>),
    TreeClicked(PathBuf),
    TreeMenu(PathBuf),
    TabPressed(usize),
    TabHovered(Option<usize>),
    TabMoved(usize, usize),
    BarPressed(Bar),
    BarHovered(Bar),
    StatusMenu(MenuKind, f32),
    MenuHovered(usize),
    Dismiss,
    FindInput(String),
    ReplaceInput(String),
    FindSubmit,
    FindToggle(Toggle),
    FindClose,
    ToggleReplace,
    ReplaceOne,
    ReplaceAll,
    GotoInput(String),
    GotoSubmit,
    Finder(FinderMessage),
    Dialog(Answer),
    Disk(bool),
    Pasted(Option<String>),
    Link(markdown::Uri),
    Pointer(Point),
    Tick,
    Frame,
    Window(WindowAction),
    Dropped(PathBuf),
    Handles(Option<menubar::Handles>),
    MenuBar(menubar::Event),
    CloseRequested,
    Noop,
}

pub struct App {
    pub paint: Paint,
    pub lang: Lang,
    look: Look,
    pub prefs: Preferences,
    pub docs: Vec<Document>,
    pub active: usize,
    next_id: u64,
    pub tree: Option<Tree>,
    pub menu: Option<Menu>,
    pub find: Option<FindBar>,
    pub goto: Option<String>,
    pub(crate) find_hold: Lingering<crate::chrome::FindView>,
    pub goto_hold: Lingering<String>,
    pub info_hold: Lingering<bool>,
    pub dialog: Option<Dialog>,
    pub closing: Option<Dialog>,
    pub notice: Option<(String, Instant)>,
    pub window: Window,
    pub hovered_tab: Option<usize>,
    pub zoom: f32,
    pub preview: bool,
    pub focus: bool,
    pub finder: Option<Finder>,
    pub status_reveal: Reveal,
    finder_latest: Arc<std::sync::atomic::AtomicU64>,
    pending_jump: Option<(PathBuf, usize)>,
    pub markdown: Option<(u64, u64, Vec<markdown::Item>)>,
    pub table: Option<(u64, u64, Vec<Vec<String>>)>,
    in_flight: HashSet<u64>,
    pub recent: Vec<PathBuf>,
    modifiers: Modifiers,
    pub pointer: Point,
    primary: Option<std::fs::File>,
    marked: u64,
    handles: Option<menubar::Handles>,
    publisher: Option<menubar::Publisher>,
    pub shell_menu: bool,
    exported: Vec<Command>,
    session_mark: u64,
    session_at: Instant,
    queue: Vec<u64>,
    after: Option<After>,
    stamp: Option<SystemTime>,
    reverting: HashSet<u64>,
}

#[must_use]
pub fn shortcut(key: &Key, physical: Physical, modifiers: Modifiers) -> Option<Command> {
    let (ctrl, shift, alt) = (modifiers.control(), modifiers.shift(), modifiers.alt());
    if modifiers.logo() {
        return None;
    }
    if let Key::Named(named) = key.as_ref() {
        return match (named, ctrl, shift, alt) {
            (Named::F3, false, false, false) => Some(Command::FindNext),
            (Named::F3, false, true, false) => Some(Command::FindPrevious),
            (Named::Enter, true, false, false) => Some(Command::ToggleCheck),
            (Named::F9, false, false, false) => Some(Command::ToggleSidebar),
            (Named::F10, false, false, false) => Some(Command::MenuBar),
            (Named::F10, false, true, false) | (Named::ContextMenu, _, _, _) => Some(Command::ContextMenu),
            (Named::F11, false, false, false) => Some(Command::FullScreen),
            (Named::Tab | Named::PageDown, true, false, false) => Some(Command::NextTab),
            (Named::Tab, true, true, false) | (Named::PageUp, true, false, false) => Some(Command::PreviousTab),
            (Named::PageDown, true, true, false) => Some(Command::MoveTab(1)),
            (Named::PageUp, true, true, false) => Some(Command::MoveTab(-1)),
            (Named::ArrowUp, false, false, true) => Some(Command::MoveLine(true)),
            (Named::ArrowDown, false, false, true) => Some(Command::MoveLine(false)),
            _ => None,
        };
    }
    if let Physical::Code(code) = physical {
        let punctuation = match (code, ctrl, shift, alt) {
            (Code::Slash, true, false, false) => Some(Command::ToggleComment),
            (Code::Slash, true, true, false) => Some(Command::Shortcuts),
            (Code::Comma, true, false, false) => Some(Command::Settings),
            (Code::BracketRight, true, false, false) => Some(Command::Indent),
            (Code::BracketLeft, true, false, false) => Some(Command::Unindent),
            (Code::Equal | Code::NumpadAdd, true, _, false) => Some(Command::ZoomIn),
            (Code::Minus | Code::NumpadSubtract, true, false, false) => Some(Command::ZoomOut),
            (Code::Digit0 | Code::Numpad0, true, false, false) => Some(Command::ZoomReset),
            (Code::Digit1, false, false, true) => Some(Command::SelectTab(0)),
            (Code::Digit2, false, false, true) => Some(Command::SelectTab(1)),
            (Code::Digit3, false, false, true) => Some(Command::SelectTab(2)),
            (Code::Digit4, false, false, true) => Some(Command::SelectTab(3)),
            (Code::Digit5, false, false, true) => Some(Command::SelectTab(4)),
            (Code::Digit6, false, false, true) => Some(Command::SelectTab(5)),
            (Code::Digit7, false, false, true) => Some(Command::SelectTab(6)),
            (Code::Digit8, false, false, true) => Some(Command::SelectTab(7)),
            (Code::Digit9, false, false, true) => Some(Command::SelectTab(8)),
            _ => None,
        };
        if punctuation.is_some() {
            return punctuation;
        }
    }
    let latin = key.to_latin(physical)?.to_ascii_lowercase();
    let command = match (latin, ctrl, shift, alt) {
        ('n', true, false, false) => Command::New,
        ('n', true, true, false) => Command::NewWindow,
        ('o', true, false, false) => Command::Open,
        ('o', true, true, false) => Command::OpenFolder,
        ('s', true, false, false) => Command::Save,
        ('s', true, true, false) => Command::SaveAs,
        ('s', true, false, true) => Command::SaveAll,
        ('w', true, false, false) => Command::Close,
        ('w', true, true, false) | ('q', true, false, false) => Command::CloseWindow,
        ('z', true, false, false) => Command::Undo,
        ('z', true, true, false) | ('y', true, false, false) => Command::Redo,
        ('d', true, false, false) => Command::DeleteLine,
        ('d', true, true, false) => Command::DuplicateLine,
        ('j', true, false, false) => Command::JoinLines,
        ('u', true, false, false) => Command::Case(Case::Upper),
        ('u', true, true, false) => Command::Case(Case::Lower),
        ('f', true, false, false) => Command::Find,
        ('h', true, false, false) => Command::Replace,
        ('g', true, false, false) => Command::FindNext,
        ('g', true, true, false) => Command::FindPrevious,
        ('e', true, false, false) => Command::UseSelection,
        ('i' | 'l', true, false, false) => Command::GoToLine,
        ('p', true, false, true) => Command::TogglePreview,
        ('p', true, false, false) => Command::QuickOpen,
        ('r', true, false, false) => Command::Symbols,
        ('f', true, true, false) => Command::SearchFolder,
        ('d', true, false, true) => Command::ToggleFocus,
        _ => return None,
    };
    Some(command)
}

#[must_use]
pub fn shortcut_label(command: Command) -> Option<&'static str> {
    Some(match command {
        Command::New => "Ctrl+N",
        Command::NewWindow => "Ctrl+Shift+N",
        Command::Open => "Ctrl+O",
        Command::OpenFolder => "Ctrl+Shift+O",
        Command::Save => "Ctrl+S",
        Command::SaveAs => "Ctrl+Shift+S",
        Command::SaveAll => "Ctrl+Alt+S",
        Command::Close => "Ctrl+W",
        Command::CloseWindow => "Ctrl+Q",
        Command::Undo => "Ctrl+Z",
        Command::Redo => "Ctrl+Shift+Z",
        Command::Cut => "Ctrl+X",
        Command::Copy => "Ctrl+C",
        Command::Paste => "Ctrl+V",
        Command::SelectAll => "Ctrl+A",
        Command::DuplicateLine => "Ctrl+Shift+D",
        Command::DeleteLine => "Ctrl+D",
        Command::MoveLine(true) => "Alt+↑",
        Command::MoveLine(false) => "Alt+↓",
        Command::JoinLines => "Ctrl+J",
        Command::ToggleComment => "Ctrl+/",
        Command::ToggleCheck => "Ctrl+Enter",
        Command::Indent => "Ctrl+]",
        Command::Unindent => "Ctrl+[",
        Command::Case(Case::Upper) => "Ctrl+U",
        Command::Case(Case::Lower) => "Ctrl+Shift+U",
        Command::Find => "Ctrl+F",
        Command::Replace => "Ctrl+H",
        Command::FindNext => "Ctrl+G",
        Command::FindPrevious => "Ctrl+Shift+G",
        Command::UseSelection => "Ctrl+E",
        Command::GoToLine => "Ctrl+I",
        Command::ToggleSidebar => "F9",
        Command::TogglePreview => "Ctrl+Alt+P",
        Command::ToggleFocus => "Ctrl+Alt+D",
        Command::QuickOpen => "Ctrl+P",
        Command::Symbols => "Ctrl+R",
        Command::SearchFolder => "Ctrl+Shift+F",
        Command::ZoomIn => "Ctrl++",
        Command::ZoomOut => "Ctrl+−",
        Command::ZoomReset => "Ctrl+0",
        Command::FullScreen => "F11",
        Command::Shortcuts => "Ctrl+?",
        Command::Settings => "Ctrl+,",
        Command::NextTab => "Ctrl+Tab",
        Command::PreviousTab => "Ctrl+Shift+Tab",
        Command::MenuBar => "F10",
        _ => return None,
    })
}

fn pair_change(doc: &Document, action: &Action) -> Option<edit::Change> {
    use iced::widget::text_editor::Edit;
    let Action::Edit(edit) = action else { return None };
    match edit {
        Edit::Insert(c) => edit::typed(&doc.text, &doc.selected(), *c, doc.language()),
        Edit::Backspace => edit::backspace_pair(&doc.text, &doc.selected(), doc.language()),
        _ => None,
    }
}

#[must_use]
pub fn binding(press: KeyPress) -> Option<Binding<Message>> {
    if !matches!(press.status, Status::Focused { .. }) {
        return None;
    }
    if let Some(command) = shortcut(&press.key, press.physical_key, press.modifiers) {
        return Some(Binding::Custom(Message::Command(command)));
    }
    let plain = !press.modifiers.control() && !press.modifiers.alt() && !press.modifiers.logo();
    match press.key.as_ref() {
        Key::Named(Named::Tab) if plain => Some(Binding::Custom(Message::Command(if press.modifiers.shift() {
            Command::Unindent
        } else {
            Command::Indent
        }))),
        Key::Named(Named::Enter) if plain => Some(Binding::Custom(Message::Command(Command::Newline))),
        Key::Named(Named::Escape) => Some(Binding::Custom(Message::Noop)),
        _ => Binding::from_key_press(press),
    }
}

fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) -> Message + Send + 'static,
) -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    let spawned = std::thread::Builder::new().name("glide-text-io".into()).spawn(move || {
        let _ = sender.send(work());
    });
    drop(spawned);
    Task::perform(receiver, move |result| result.map_or(Message::Noop, done))
}

fn open_detached(program: &str, args: &[&str]) {
    let program = program.to_string();
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let _ = std::thread::Builder::new().name("glide-text-launch".into()).spawn(move || {
        if let Ok(mut child) = std::process::Command::new(&program).args(&args).spawn() {
            let _ = child.wait();
        }
    });
}

fn write_session(order: u64, drafts: Option<&Path>, writes: &[(PathBuf, String)], session: &Session) -> bool {
    static WRITTEN: std::sync::Mutex<u64> = std::sync::Mutex::new(0);
    let mut written = WRITTEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if order < *written {
        return true;
    }
    *written = order;
    let mut kept = true;
    if let Some(folder) = drafts {
        if !writes.is_empty() && std::fs::create_dir_all(folder).is_ok() {
            let _ = std::fs::set_permissions(
                folder,
                <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
            );
        }
        for (file, text) in writes {
            if let Err(error) = glide_config::write_private(file, text.as_bytes()) {
                eprintln!("glide-text: a draft was not saved: {error}");
                kept = false;
            }
        }
    }
    if let Err(error) = config::save_session(session) {
        eprintln!("glide-text: the session was not saved: {error}");
        return false;
    }
    if let Some(folder) = drafts
        && let Ok(entries) = std::fs::read_dir(folder)
    {
        for entry in entries.flatten() {
            if !writes.iter().any(|(file, _)| *file == entry.path()) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    kept
}

fn show_in_files(path: PathBuf) {
    let _ = std::thread::Builder::new().name("glide-text-reveal".into()).spawn(move || {
        let uri = glide_config::uri::file_uri(&path);
        let shown = zbus::blocking::Connection::session().and_then(|connection| {
            connection.call_method(
                Some("org.freedesktop.FileManager1"),
                "/org/freedesktop/FileManager1",
                Some("org.freedesktop.FileManager1"),
                "ShowItems",
                &(vec![uri.as_str()], ""),
            )
        });
        if shown.is_err()
            && let Some(folder) = path.parent()
            && let Ok(mut child) = std::process::Command::new("xdg-open").arg(folder).spawn()
        {
            let _ = child.wait();
        }
    });
}

fn claim_primary() -> Option<std::fs::File> {
    let folder = config::state_dir()?;
    std::fs::create_dir_all(&folder).ok()?;
    let file =
        std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(folder.join("instance.lock")).ok()?;
    file.try_lock().ok()?;
    Some(file)
}

#[must_use]
pub(crate) fn suggested_name(document: &Document, untitled: &str) -> String {
    if let Some(name) = document.path.as_deref().and_then(Path::file_name) {
        return name.to_string_lossy().into_owned();
    }
    let first = document.text.lines().find(|line| !line.trim().is_empty()).unwrap_or_default();
    let first = first.trim().trim_start_matches(['#', '/', '*', '-', ' ']).trim();
    let stem: String = first
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control())
        .take(40)
        .collect();
    let stem = stem.trim();
    let stem = if stem.is_empty() { untitled } else { stem };
    format!("{stem}.{}", document.language().extension())
}

#[must_use]
pub fn parse_args<I: IntoIterator<Item = OsString>>(args: I) -> (bool, Vec<PathBuf>) {
    let mut fresh = false;
    let mut paths = Vec::new();
    for arg in args {
        fresh |= arg == "--new-window";
        if arg.as_encoded_bytes().starts_with(b"--") {
            continue;
        }
        paths.push(arg.to_str().and_then(chooser::uri_path).unwrap_or_else(|| PathBuf::from(arg)));
    }
    (fresh, paths)
}

fn stamp() -> Option<SystemTime> {
    look::stamp(config::path().as_deref())
}

fn intern(family: &str) -> &'static str {
    static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut names = NAMES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(name) = names.iter().find(|name| **name == family) {
        return name;
    }
    let name: &'static str = Box::leak(family.to_string().into_boxed_str());
    names.push(name);
    name
}

fn date_time() -> String {
    glide_config::local_time().map_or_else(String::new, |time| {
        format!("{:04}-{:02}-{:02} {:02}:{:02}", time.year, time.month, time.day, time.hour, time.minute)
    })
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let (fresh, paths) = parse_args(std::env::args_os().skip(1));
        let paths: Vec<PathBuf> = paths.into_iter().map(|path| std::path::absolute(&path).unwrap_or(path)).collect();
        let primary = if fresh { None } else { claim_primary() };
        Self::launch(primary, &paths)
    }

    pub fn launch(primary: Option<std::fs::File>, paths: &[PathBuf]) -> (Self, Task<Message>) {
        let look = Look::load();
        motion::set_scale(look.motion_scale);
        let prefs = config::load();
        let session =
            if primary.is_some() && prefs.restore_session { config::load_session() } else { Session::default() };
        let mut app = Self {
            paint: look.paint(),
            lang: look.lang(),
            look,
            prefs,
            docs: Vec::new(),
            active: 0,
            next_id: 1,
            tree: None,
            menu: None,
            find: None,
            goto: None,
            find_hold: Lingering::default(),
            goto_hold: Lingering::default(),
            info_hold: Lingering::default(),
            dialog: None,
            closing: None,
            notice: None,
            window: Window::default(),
            hovered_tab: None,
            zoom: 0.0,
            preview: false,
            focus: false,
            finder: None,
            status_reveal: Reveal::closed().with_ease(motion::ease_out),
            finder_latest: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            pending_jump: None,
            markdown: None,
            table: None,
            in_flight: HashSet::new(),
            recent: session.recent.clone(),
            modifiers: Modifiers::empty(),
            pointer: Point::ORIGIN,
            primary,
            marked: 0,
            handles: None,
            publisher: None,
            shell_menu: false,
            exported: Vec::new(),
            session_mark: 0,
            session_at: Instant::now(),
            queue: Vec::new(),
            after: None,
            stamp: stamp(),
            reverting: HashSet::new(),
        };
        let mut tasks = vec![app.restore(&session)];
        for path in paths {
            tasks.push(app.open_path(path.clone()));
        }
        if app.docs.is_empty() && !paths.iter().any(|path| path.is_file() || !path.exists()) {
            app.new_document();
        }
        tasks.push(operation::focus(EDITOR));
        tasks.push(glide_sdk::window::handles().map(Message::Handles));
        (app, Task::batch(tasks))
    }

    fn restore(&mut self, session: &Session) -> Task<Message> {
        let mut tasks = Vec::new();
        if let Some(folder) = session.folder.clone().filter(|folder| folder.is_dir()) {
            tasks.push(self.open_folder(folder));
        }
        for tab in &session.tabs {
            let draft = tab.draft.as_deref().and_then(|draft| std::fs::read_to_string(draft).ok());
            let decoded = match (&draft, &tab.path) {
                (Some(text), _) => Some(Decoded {
                    text: text.clone(),
                    encoding: Encoding::Utf8,
                    bom: false,
                    ending: Ending::Lf,
                    mixed: false,
                    suspect: false,
                }),
                (None, Some(path)) => document::read(path).ok(),
                (None, None) => None,
            };
            let Some(decoded) = decoded else { continue };
            let id = self.take_id();
            let mut doc = if let Some(path) = &tab.path {
                Document::open(id, path.clone(), decoded)
            } else {
                let mut doc = Document::new(id, self.next_untitled());
                doc.set_text(decoded.text);
                doc.detected = crate::detect::detect(None, &doc.text);
                doc
            };
            if let Some(path) = tab.path.as_deref().filter(|_| draft.is_some())
                && let Ok(disk) = document::read(path)
            {
                doc.encoding = disk.encoding;
                doc.bom = disk.bom;
                doc.ending = disk.ending;
                doc.trust = document::Trust::of(disk.suspect);
            }
            if draft.is_some() {
                doc.saved = u64::MAX;
                doc.draft.clone_from(&tab.draft);
                if tab.path.is_some()
                    && let Some(stamp) = tab.disk
                {
                    doc.disk = Some(std::time::UNIX_EPOCH + std::time::Duration::from_nanos(stamp));
                }
            }
            doc.pinned = tab.language.as_deref().and_then(Language::from_key);
            let at =
                document::offset(&doc.text, iced::widget::text_editor::Position { line: tab.line, column: tab.column });
            doc.place(at..at);
            self.docs.push(doc);
        }
        self.active = session.active.min(self.docs.len().saturating_sub(1));
        Task::batch(tasks)
    }

    fn take_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn next_untitled(&self) -> usize {
        (1..=self.docs.len() + 1)
            .find(|number| !self.docs.iter().any(|doc| doc.path.is_none() && doc.untitled == *number))
            .unwrap_or(1)
    }

    fn new_document(&mut self) {
        let id = self.take_id();
        let doc = Document::new(id, self.next_untitled());
        let at = if self.docs.is_empty() { 0 } else { self.active + 1 };
        self.docs.insert(at, doc);
        self.active = at;
    }

    #[must_use]
    pub fn doc(&self) -> Option<&Document> {
        self.docs.get(self.active)
    }

    fn doc_mut(&mut self) -> Option<&mut Document> {
        self.docs.get_mut(self.active)
    }

    fn index_of(&self, id: u64) -> Option<usize> {
        self.docs.iter().position(|doc| doc.id == id)
    }

    #[must_use]
    pub fn untitled(&self) -> &'static str {
        Text::Untitled.text(self.lang)
    }

    #[must_use]
    pub(crate) fn doc_name(&self, doc: &Document) -> String {
        doc.name(self.untitled())
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
    pub fn font(&self) -> iced::Font {
        match (&self.prefs.font_family, self.prefs.monospace) {
            (Some(family), true) => iced::Font::with_name(intern(family)),
            (_, true) => crate::MONO,
            (_, false) => iced::Font::DEFAULT,
        }
    }

    #[must_use]
    pub fn font_size(&self) -> f32 {
        (self.prefs.font_size + self.zoom).clamp(6.0, 72.0)
    }

    #[must_use]
    pub fn settings(&self, doc: &Document) -> Settings {
        let (marks, marked) = match &self.find {
            Some(find) if find.doc == doc.id => (Arc::clone(&find.marks), find.marked),
            _ => (Arc::new(Marks::new()), 0),
        };
        Settings {
            lines: Arc::clone(&doc.lines),
            revision: doc.paint,
            palette: Palette::new(&self.paint.tokens, self.font()),
            marks,
            marked,
        }
    }

    fn refresh_marks(&mut self) {
        let Some(doc) = self.docs.get(self.active) else { return };
        let Some(find) = &mut self.find else { return };
        let key = Some((doc.id, doc.revision, find.current));
        if find.marked_for == key && find.marked != 0 {
            return;
        }
        find.marked_for = key;
        let shown = &find.found[..find.found.len().min(MARK_LIMIT)];
        find.marks = Arc::new(syntax::marks(&doc.text, shown, find.current));
        self.marked += 1;
        find.marked = self.marked;
    }

    #[must_use]
    pub fn indent(&self) -> Indent {
        Indent { spaces: self.prefs.spaces, width: self.prefs.tab_width }
    }

    #[must_use]
    pub fn previewing(&self) -> bool {
        self.preview && self.doc().is_some_and(Document::previewable)
    }

    #[must_use]
    pub fn title(&self) -> String {
        match self.doc() {
            Some(doc) => {
                let dirty = if doc.dirty() { "• " } else { "" };
                format!("{dirty}{} — {}", self.doc_name(doc), Text::AppName.text(self.lang))
            }
            None => Text::AppName.text(self.lang).to_string(),
        }
    }

    #[must_use]
    pub fn subtitle(&self) -> Option<String> {
        let doc = self.doc()?;
        let folder = doc.path.as_deref()?.parent()?;
        let home = glide_config::home();
        Some(match home.as_deref().and_then(|home| folder.strip_prefix(home).ok()) {
            Some(rest) if rest.as_os_str().is_empty() => String::from("~"),
            Some(rest) => format!("~/{}", rest.display()),
            None => folder.display().to_string(),
        })
    }

    #[must_use]
    pub fn iced_theme(&self) -> iced::Theme {
        self.paint.iced_theme()
    }

    #[must_use]
    pub fn view(&self) -> iced::Element<'_, Message> {
        crate::chrome::view(self)
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
        self.notice.as_ref().is_some_and(|(_, at)| at.elapsed() < motion::t2())
            || self.menu.as_ref().is_some_and(|menu| menu.reveal.is_animating(motion::t2()))
            || self.status_reveal.is_animating(motion::t2())
            || self.finder.as_ref().is_some_and(|finder| finder.reveal.is_animating(motion::t2()))
            || self.dialog.as_ref().is_some_and(|dialog| dialog.reveal.is_animating(motion::t2()))
            || self.closing.as_ref().is_some_and(|dialog| dialog.reveal.is_animating(motion::t2()))
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            iced::event::listen_with(|event, _, _| match event {
                iced::Event::Window(window::Event::FileDropped(path)) => Some(Message::Dropped(path)),
                _ => None,
            }),
            iced::time::every(TICK).map(|_| Message::Tick),
        ];
        if let Some(handles) = self.handles {
            subscriptions.push(Subscription::run_with(handles, menubar::events).map(Message::MenuBar));
        }
        if self.animating() {
            subscriptions.push(window::frames().map(|_| Message::Frame));
        }
        Subscription::batch(subscriptions)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let quiet = matches!(message, Message::Pointer(_) | Message::Frame);
        let task = self.handle(message);
        let highlight = self.schedule();
        self.refresh_find();
        self.refresh_marks();
        self.refresh_preview();
        if !quiet {
            self.publish_menu();
        }
        Task::batch([task, highlight])
    }

    fn publish_menu(&mut self) {
        let Some(publisher) = &self.publisher else { return };
        let (menu, commands) = crate::chrome::export_menu(self);
        self.exported = commands;
        publisher.set(menu);
    }

    fn on_menu_bar(&mut self, event: menubar::Event) -> Task<Message> {
        match event {
            menubar::Event::Ready(publisher) => self.publisher = Some(publisher),
            menubar::Event::Shown(shown) => {
                self.shell_menu = shown;
                if shown
                    && self.menu.as_ref().is_some_and(|menu| matches!(menu.kind, MenuKind::Bar(_) | MenuKind::Compact))
                {
                    self.dismiss_menu();
                }
            }
            menubar::Event::Activated(id) => {
                let command = usize::try_from(id).ok().and_then(|id| self.exported.get(id.checked_sub(1)?)).copied();
                if let Some(command) = command {
                    return Task::batch([self.command(command), operation::focus(EDITOR)]);
                }
            }
        }
        Task::none()
    }

    #[allow(clippy::too_many_lines)] // The message dispatcher: one short arm per Message variant
    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Edit(action) => {
                let pasted = matches!(action, Action::Edit(iced::widget::text_editor::Edit::Paste(_)));
                if self.prefs.auto_pair
                    && let Some(doc) = self.doc_mut()
                    && let Some(change) = pair_change(doc, &action)
                {
                    doc.replace(change.range, &change.text, Kind::Other);
                    doc.place(change.select);
                    return Task::none();
                }
                if let Some(doc) = self.doc_mut()
                    && doc.perform(action)
                {
                    if pasted && doc.text.len() <= DETECT_LIMIT {
                        doc.redetect();
                    }
                    self.dismiss_menu();
                }
                Task::none()
            }
            Message::Command(command) => self.command(command),
            Message::Escape => self.escape(),
            Message::Key(event, captured) => self.on_key(&event, captured),
            Message::Loaded(path, result) => self.loaded(path, result),
            Message::Reloaded(id, result) => {
                let reverting = self.reverting.remove(&id);
                if let Some(index) = self.index_of(id) {
                    match result {
                        Ok(decoded) if reverting || !self.docs[index].dirty() => self.docs[index].reload(&decoded),
                        _ => self.docs[index].changed_on_disk = true,
                    }
                }
                Task::none()
            }
            Message::Chosen(purpose, chosen) => self.chosen(purpose, chosen),
            Message::SaveTo(id, chosen) => match chosen {
                Chosen::Paths(paths) => match paths.into_iter().next() {
                    Some(path) => self.write(id, path),
                    None => self.abort_queue(),
                },
                Chosen::Cancelled => self.abort_queue(),
                Chosen::Failed(error) => {
                    self.tell(format!("{}: {error}", Text::ChooserFailed.text(self.lang)));
                    self.abort_queue()
                }
            },
            Message::Saved(id, path, generation, result) => self.saved(id, &path, generation, result),
            Message::Highlighted(id, revision, language, lines) => {
                self.in_flight.remove(&id);
                if let Some(index) = self.index_of(id) {
                    self.docs[index].install(revision, language, Arc::unwrap_or_clone(lines));
                }
                Task::none()
            }
            Message::Listed(folder, result) => {
                if let Some(tree) = &mut self.tree {
                    tree.fill(&folder, result.unwrap_or_default());
                    let pending = tree.pending().into_iter().filter(|path| *path != folder).collect::<Vec<_>>();
                    return Task::batch(pending.into_iter().map(|path| self.list(path)));
                }
                Task::none()
            }
            Message::TreeClicked(path) => {
                if path.is_dir() {
                    let wanted = self.tree.as_mut().and_then(|tree| tree.toggle(&path));
                    return wanted.map_or_else(Task::none, |folder| self.list(folder));
                }
                self.open_path(path)
            }
            Message::TreeMenu(path) => {
                self.open_menu(MenuKind::Tree(path), self.pointer, false);
                Task::none()
            }
            Message::TabPressed(index) => {
                self.select(index);
                operation::focus(EDITOR)
            }
            Message::TabHovered(index) => {
                self.hovered_tab = index;
                Task::none()
            }
            Message::TabMoved(from, to) => {
                self.move_tab(from, to);
                Task::none()
            }
            Message::BarPressed(bar) => {
                let kind = if crate::chrome::compact(self) { MenuKind::Compact } else { MenuKind::Bar(bar) };
                if self.menu.as_ref().is_some_and(|menu| menu.kind == kind && menu.reveal.is_open()) {
                    self.dismiss_menu();
                } else {
                    let at = Point::new(crate::chrome::bar_x(self, bar), crate::chrome::HEADER - 2.0);
                    self.open_menu(kind, at, false);
                }
                Task::none()
            }
            Message::BarHovered(bar) => {
                if self.menu.as_ref().is_some_and(|menu| matches!(menu.kind, MenuKind::Bar(open) if open != bar)) {
                    let at = Point::new(crate::chrome::bar_x(self, bar), crate::chrome::HEADER - 2.0);
                    self.open_menu(MenuKind::Bar(bar), at, false);
                }
                Task::none()
            }
            Message::StatusMenu(kind, x) => {
                if self.menu.as_ref().is_some_and(|menu| menu.kind == kind) {
                    self.dismiss_menu();
                } else {
                    let lift = if self.status_floating() { crate::chrome::ISLAND_LIFT } else { crate::chrome::STATUS };
                    let at = Point::new(x, self.window.size.height - lift);
                    self.open_menu(kind, at, false);
                }
                Task::none()
            }
            Message::MenuHovered(index) => {
                if let Some(menu) = &mut self.menu {
                    menu.selected = Some(index);
                }
                Task::none()
            }
            Message::Dismiss => {
                self.dismiss_menu();
                Task::none()
            }
            Message::FindInput(_)
            | Message::ReplaceInput(_)
            | Message::FindSubmit
            | Message::FindToggle(_)
            | Message::FindClose
            | Message::ToggleReplace
            | Message::ReplaceOne
            | Message::ReplaceAll => self.on_find(message),
            Message::GotoInput(text) => {
                self.goto = Some(text);
                Task::none()
            }
            Message::GotoSubmit => self.go_to(),
            Message::Finder(message) => self.on_finder(message),
            Message::Dialog(answer) => self.answer(answer),
            Message::Disk(reload) => {
                let Some(doc) = self.doc_mut() else { return Task::none() };
                doc.changed_on_disk = false;
                if reload && doc.path.is_some() {
                    let id = doc.id;
                    self.revert(id)
                } else {
                    doc.disk = doc.path.as_deref().and_then(document::modified);
                    Task::none()
                }
            }
            Message::Pasted(text) => {
                if let (Some(text), Some(doc)) = (text, self.doc_mut()) {
                    let range = doc.selected();
                    doc.replace(range, &text, Kind::Other);
                    if doc.text.len() <= DETECT_LIMIT {
                        doc.redetect();
                    }
                }
                Task::none()
            }
            Message::Link(uri) => {
                if uri.starts_with("http://") || uri.starts_with("https://") || uri.starts_with("mailto:") {
                    open_detached("xdg-open", &[&uri]);
                } else if let Some(base) =
                    self.doc().and_then(|doc| doc.path.as_deref()?.parent().map(Path::to_path_buf))
                {
                    let target = base.join(uri.split('#').next().unwrap_or_default());
                    if target.is_file() {
                        return self.open_path(target);
                    }
                }
                Task::none()
            }
            Message::Pointer(point) => {
                self.pointer = point;
                self.track_status();
                Task::none()
            }
            Message::Dropped(path) => {
                if path.is_dir() {
                    return self.open_folder(path);
                }
                self.open_path(path)
            }
            Message::Tick => self.tick(),
            Message::Frame => {
                if let Some(menu) = &mut self.menu {
                    menu.reveal.settle(motion::t2());
                }
                self.menu.take_if(|menu| !menu.reveal.is_open() && !menu.reveal.is_visible(motion::t2()));
                if let Some(dialog) = &mut self.dialog {
                    dialog.reveal.settle(motion::t2());
                }
                if let Some(dialog) = &mut self.closing {
                    dialog.reveal.settle(motion::t2());
                }
                self.closing.take_if(|dialog| !dialog.reveal.is_visible(motion::t2()));
                Task::none()
            }
            Message::Window(action) => {
                if matches!(action, WindowAction::Drag | WindowAction::ToggleMaximize) {
                    self.dismiss_menu();
                }
                Task::none()
            }
            Message::CloseRequested => self.request_window_close(),
            Message::Handles(handles) => {
                self.handles = handles;
                Task::none()
            }
            Message::MenuBar(event) => self.on_menu_bar(event),
            Message::Noop => Task::none(),
        }
    }

    fn tell(&mut self, text: String) {
        self.notice = Some((text, Instant::now()));
    }

    pub fn follow_look(&mut self, look: &Look, paint: Paint) {
        self.look = look.clone();
        self.paint = paint;
        self.lang = look.lang();
    }

    pub fn follow_window(&mut self, window: Window) -> Task<Message> {
        let regained = window.focused && !self.window.focused;
        self.window = window;
        if regained { self.refresh_tree() } else { Task::none() }
    }

    fn tick(&mut self) -> Task<Message> {
        self.notice.take_if(|(_, at)| at.elapsed() >= NOTICE);
        let stamp = stamp();
        if stamp != self.stamp {
            self.stamp = stamp;
            self.prefs = config::load();
        }
        if let Some(doc) = self.doc_mut()
            && doc.detect_pending
            && doc.text.len() <= DETECT_LIMIT
        {
            doc.redetect();
        }
        let mut tasks = Vec::new();
        for doc in &mut self.docs {
            let Some(path) = doc.path.clone() else { continue };
            let disk = document::modified(&path);
            if disk == doc.disk || doc.changed_on_disk {
                continue;
            }
            if disk.is_some() && !doc.dirty() {
                let id = doc.id;
                doc.disk = disk;
                tasks.push(blocking(move || document::read(&path), move |result| Message::Reloaded(id, result)));
            } else {
                doc.changed_on_disk = true;
            }
        }
        if self.session_at.elapsed() >= SESSION_EVERY {
            self.session_at = Instant::now();
            let mark: u64 = self.docs.iter().map(|doc| doc.revision.wrapping_mul(31).wrapping_add(doc.id)).sum::<u64>()
                + self.docs.len() as u64
                + self.active as u64;
            if mark != self.session_mark {
                self.session_mark = mark;
                self.save_session();
            }
        }
        Task::batch(tasks)
    }

    fn schedule(&mut self) -> Task<Message> {
        let Some(doc) = self.docs.get_mut(self.active) else { return Task::none() };
        if !doc.stale() || self.in_flight.contains(&doc.id) {
            return Task::none();
        }
        let language = doc.language();
        let revision = doc.revision;
        if language == Language::Plain {
            let count = doc.text.matches('\n').count() + 1;
            doc.install(revision, language, vec![Vec::new(); count]);
            return Task::none();
        }
        if doc.text.len() <= SYNC_HIGHLIGHT {
            let lines = syntax::highlight(language, &doc.text);
            doc.install(revision, language, lines);
            return Task::none();
        }
        let id = doc.id;
        let text = doc.text.clone();
        self.in_flight.insert(id);
        blocking(
            move || Arc::new(syntax::highlight(language, &text)),
            move |lines| Message::Highlighted(id, revision, language, lines),
        )
    }

    fn refresh_preview(&mut self) {
        if !self.previewing() {
            self.markdown = None;
            self.table = None;
            return;
        }
        let Some(doc) = self.docs.get(self.active) else { return };
        let key = (doc.id, doc.revision);
        if let Some(delimiter) = doc.table_delimiter() {
            self.markdown = None;
            if self.table.as_ref().is_none_or(|(id, revision, _)| (*id, *revision) != key) {
                self.table = Some((key.0, key.1, crate::table::parse(&doc.text, delimiter)));
            }
            return;
        }
        self.table = None;
        if self.markdown.as_ref().is_some_and(|(id, revision, _)| (*id, *revision) == key) {
            return;
        }
        let items = markdown::parse(&doc.text).collect();
        self.markdown = Some((key.0, key.1, items));
    }

    fn select(&mut self, index: usize) {
        if index < self.docs.len() {
            self.active = index;
            self.goto = None;
        }
    }

    fn move_tab(&mut self, from: usize, to: usize) {
        let to = to.min(self.docs.len().saturating_sub(1));
        if from != to && from < self.docs.len() {
            let doc = self.docs.remove(from);
            self.docs.insert(to, doc);
            self.active = to;
        }
    }

    #[must_use]
    pub(crate) fn sidebar_visible(&self) -> bool {
        self.prefs.sidebar && !self.focus && self.window.size.width >= crate::chrome::SIDEBAR_MIN_WINDOW
    }

    #[must_use]
    pub(crate) fn status_visible(&self) -> bool {
        self.prefs.status_bar && !self.focus && !self.prefs.status_islands
    }

    #[must_use]
    pub(crate) fn status_floating(&self) -> bool {
        self.prefs.status_bar && !self.focus && self.prefs.status_islands
    }

    fn track_status(&mut self) {
        let sidebar = if self.sidebar_visible() { crate::chrome::SIDEBAR + 1.0 } else { 0.0 };
        let size = self.window.size;
        let near = self.pointer.x >= sidebar && self.pointer.y >= size.height - crate::chrome::ISLAND_ZONE;
        let pinned = self
            .menu
            .as_ref()
            .is_some_and(|menu| matches!(menu.kind, MenuKind::Syntax | MenuKind::TabWidths | MenuKind::Endings));
        if near || pinned || !self.prefs.auto_hide_status {
            self.status_reveal.open(motion::t2());
        } else {
            self.status_reveal.close(motion::t2());
        }
    }

    fn open_menu(&mut self, kind: MenuKind, at: Point, keyboard: bool) {
        let selected = keyboard.then_some(0);
        self.menu = Some(Menu::new(kind, at, selected));
    }

    fn dismiss_menu(&mut self) {
        if let Some(menu) = &mut self.menu {
            menu.reveal = menu.reveal.with_ease(motion::ease_io);
            menu.reveal.close(motion::t2());
        }
    }

    #[must_use]
    pub fn menu_open(&self) -> bool {
        self.menu.as_ref().is_some_and(|menu| menu.reveal.is_open())
    }

    fn escape(&mut self) -> Task<Message> {
        if self.menu_open() {
            self.dismiss_menu();
            return Task::none();
        }
        if self.dialog.is_some() {
            return self.answer(Answer::Cancel);
        }
        if self.finder.take().is_some() {
            return operation::focus(EDITOR);
        }
        if self.goto.take().is_some() {
            return operation::focus(EDITOR);
        }
        if self.find.take().is_some() {
            return operation::focus(EDITOR);
        }
        if self.focus {
            self.focus = false;
        }
        Task::none()
    }

    fn on_key(&mut self, event: &keyboard::Event, captured: bool) -> Task<Message> {
        let keyboard::Event::KeyPressed { key, physical_key, modifiers, .. } = event else {
            if let keyboard::Event::ModifiersChanged(modifiers) = event {
                self.modifiers = *modifiers;
            }
            return Task::none();
        };
        if self.dialog.is_some() {
            return match key.as_ref() {
                Key::Named(Named::Escape) => self.answer(Answer::Cancel),
                Key::Named(Named::Enter) => {
                    let answer = match self.dialog.as_ref().map(|dialog| &dialog.kind) {
                        Some(DialogKind::Unsaved { .. } | DialogKind::Encoding { hazard: Hazard::Suspect, .. }) => {
                            Answer::Save
                        }
                        _ => Answer::Cancel,
                    };
                    self.answer(answer)
                }
                _ => Task::none(),
            };
        }
        if self.menu_open()
            && let Some(task) = self.menu_key(key)
        {
            return task;
        }
        if let Key::Named(Named::Escape) = key.as_ref() {
            return self.escape();
        }
        if self.finder.is_some()
            && let Key::Named(arrow @ (Named::ArrowDown | Named::ArrowUp)) = key.as_ref()
        {
            return self.on_finder(FinderMessage::Step(arrow == Named::ArrowDown));
        }
        if captured {
            return Task::none();
        }
        match shortcut(key, *physical_key, *modifiers) {
            Some(command) => self.command(command),
            None => Task::none(),
        }
    }

    fn menu_key(&mut self, key: &Key) -> Option<Task<Message>> {
        let menu = self.menu.as_ref()?;
        let kind = menu.kind.clone();
        let actions = crate::chrome::menu_actions(self, &kind);
        match key.as_ref() {
            Key::Named(Named::ArrowDown | Named::ArrowUp) => {
                let down = matches!(key.as_ref(), Key::Named(Named::ArrowDown));
                let count = actions.len();
                if let Some(menu) = &mut self.menu
                    && count > 0
                {
                    menu.selected = Some(match menu.selected {
                        None if down => 0,
                        None => count - 1,
                        Some(at) if down => (at + 1) % count,
                        Some(at) => (at + count - 1) % count,
                    });
                }
                Some(Task::none())
            }
            Key::Named(Named::ArrowLeft | Named::ArrowRight) => {
                let MenuKind::Bar(bar) = kind else { return Some(Task::none()) };
                let next = bar.step(matches!(key.as_ref(), Key::Named(Named::ArrowRight)));
                let at = Point::new(crate::chrome::bar_x(self, next), crate::chrome::HEADER - 2.0);
                self.open_menu(MenuKind::Bar(next), at, true);
                Some(Task::none())
            }
            Key::Named(Named::Enter | Named::Space) => {
                let command = menu.selected.and_then(|at| actions.get(at).copied());
                Some(command.map_or_else(Task::none, |command| self.command(command)))
            }
            Key::Named(Named::Escape) => {
                self.dismiss_menu();
                Some(operation::focus(EDITOR))
            }
            Key::Named(Named::Tab) => Some(Task::none()),
            _ => None,
        }
    }

    #[allow(clippy::too_many_lines)] // The command dispatcher: one short arm per menu or shortcut command
    fn command(&mut self, command: Command) -> Task<Message> {
        let keeps_menu = matches!(command, Command::Recent | Command::Syntax | Command::TabWidths | Command::Endings);
        if !keeps_menu {
            self.dismiss_menu();
        }
        match command {
            Command::New => {
                self.new_document();
                operation::focus(EDITOR)
            }
            Command::NewWindow => {
                if let Ok(exe) = std::env::current_exe() {
                    open_detached(&exe.to_string_lossy(), &["--new-window"]);
                }
                Task::none()
            }
            Command::Open => self.choose(Purpose::Open),
            Command::OpenFolder => self.choose(Purpose::Folder),
            Command::Recent => {
                let at = self.menu.as_ref().map_or(Point::new(8.0, crate::chrome::HEADER), |menu| menu.at);
                self.open_menu(MenuKind::Recent, at, self.menu.as_ref().is_some_and(|menu| menu.selected.is_some()));
                Task::none()
            }
            Command::OpenRecent(index) => match self.recent.get(index).cloned() {
                Some(path) if path.is_dir() => self.open_folder(path),
                Some(path) => self.open_path(path),
                None => Task::none(),
            },
            Command::ClearRecent => {
                self.recent.clear();
                self.save_session();
                Task::none()
            }
            Command::Save => self.save_active(false),
            Command::SaveAs => self.save_active(true),
            Command::SaveAll => {
                self.queue = self.docs.iter().filter(|doc| doc.dirty()).map(|doc| doc.id).collect();
                self.after = None;
                self.next_in_queue()
            }
            Command::Revert => {
                let Some(doc) = self.doc() else { return Task::none() };
                let (true, id) = (doc.path.is_some(), doc.id) else { return Task::none() };
                self.show(DialogKind::Revert(id))
            }
            Command::ShowInFiles => {
                if let Some(path) = self.doc().and_then(|doc| doc.path.clone()) {
                    show_in_files(path);
                }
                Task::none()
            }
            Command::CopyPath => self
                .doc()
                .and_then(|doc| doc.path.as_ref())
                .map_or_else(Task::none, |path| clipboard::write(path.display().to_string())),
            Command::Close => self.doc().map(|doc| doc.id).map_or_else(Task::none, |id| self.request_close(id)),
            Command::CloseTab(index) => {
                self.docs.get(index).map(|doc| doc.id).map_or_else(Task::none, |id| self.request_close(id))
            }
            Command::CloseFolder => {
                self.tree = None;
                Task::none()
            }
            Command::CloseWindow => self.request_window_close(),
            Command::Undo | Command::Redo => {
                if let Some(doc) = self.doc_mut() {
                    if command == Command::Undo {
                        doc.undo()
                    } else {
                        doc.redo()
                    };
                }
                Task::none()
            }
            Command::Cut | Command::Copy => {
                let Some(doc) = self.doc_mut() else { return Task::none() };
                let range = doc.selected();
                if range.is_empty() {
                    return Task::none();
                }
                let text = doc.text[range.clone()].to_string();
                if command == Command::Cut {
                    doc.replace(range, "", Kind::Other);
                }
                Task::batch([clipboard::write(text), operation::focus(EDITOR)])
            }
            Command::Paste => Task::batch([clipboard::read().map(Message::Pasted), operation::focus(EDITOR)]),
            Command::SelectAll => {
                if let Some(doc) = self.doc_mut() {
                    doc.perform(Action::SelectAll);
                }
                operation::focus(EDITOR)
            }
            Command::Newline => {
                let auto = self.prefs.auto_indent;
                if let Some(doc) = self.doc_mut() {
                    let range = doc.selected();
                    if doc.language() == Language::Markdown
                        && range.is_empty()
                        && let Some(change) = edit::continue_list(&doc.text, range.start)
                    {
                        doc.replace(change.range, &change.text, Kind::Other);
                        doc.place(change.select);
                        return Task::none();
                    }
                    let text = edit::newline(&doc.text, range.start, auto);
                    let at = range.start + text.len();
                    doc.replace(range, &text, Kind::Other);
                    doc.place(at..at);
                }
                Task::none()
            }
            Command::DuplicateLine
            | Command::DeleteLine
            | Command::MoveLine(_)
            | Command::JoinLines
            | Command::SortLines
            | Command::ToggleComment
            | Command::Indent
            | Command::Unindent
            | Command::Case(_)
            | Command::TrimTrailing
            | Command::ToggleCheck
            | Command::InsertDate => {
                self.transform(command);
                operation::focus(EDITOR)
            }
            Command::Find | Command::Replace => {
                let selection = self
                    .doc()
                    .map(|doc| doc.text[doc.selected()].to_string())
                    .filter(|text| !text.is_empty() && !text.contains('\n'));
                let find = self.find.get_or_insert_with(FindBar::default);
                find.replacing |= command == Command::Replace;
                if let Some(selection) = selection {
                    find.query.text = selection;
                    find.revision = u64::MAX;
                }
                self.goto = None;
                operation::focus(FIND_ID).chain(operation::select_all(FIND_ID))
            }
            Command::FindNext | Command::FindPrevious => {
                if self.find.is_none() {
                    return self.command(Command::Find);
                }
                self.step(command == Command::FindNext);
                Task::none()
            }
            Command::UseSelection => {
                let Some(text) =
                    self.doc().map(|doc| doc.text[doc.selected()].to_string()).filter(|text| !text.is_empty())
                else {
                    return Task::none();
                };
                let find = self.find.get_or_insert_with(FindBar::default);
                find.query.text = text;
                find.revision = u64::MAX;
                Task::none()
            }
            Command::GoToLine => {
                self.goto = Some(String::new());
                operation::focus(GOTO_ID)
            }
            Command::QuickOpen => self.open_finder(finder::Mode::Files),
            Command::Symbols => self.open_finder(finder::Mode::Symbols),
            Command::SearchFolder => self.open_finder(finder::Mode::Search),
            Command::ToggleSidebar => {
                self.prefs.sidebar = !self.prefs.sidebar;
                self.store_prefs();
                Task::none()
            }
            Command::ToggleStatusBar => {
                self.prefs.status_bar = !self.prefs.status_bar;
                self.store_prefs();
                Task::none()
            }
            Command::ToggleIslands => {
                self.prefs.status_islands = !self.prefs.status_islands;
                self.store_prefs();
                Task::none()
            }
            Command::ToggleAutoHideStatus => {
                self.prefs.auto_hide_status = !self.prefs.auto_hide_status;
                self.store_prefs();
                self.track_status();
                Task::none()
            }
            Command::ToggleMonospace => {
                self.prefs.monospace = !self.prefs.monospace;
                self.store_prefs();
                Task::none()
            }
            Command::TogglePreview => {
                self.preview = !self.preview;
                Task::none()
            }
            Command::ToggleFocus => {
                self.focus = !self.focus;
                operation::focus(EDITOR)
            }
            Command::ZoomIn => {
                self.zoom = (self.zoom + 1.0).min(72.0 - self.prefs.font_size);
                Task::none()
            }
            Command::ZoomOut => {
                self.zoom = (self.zoom - 1.0).max(6.0 - self.prefs.font_size);
                Task::none()
            }
            Command::ZoomReset => {
                self.zoom = 0.0;
                Task::none()
            }
            Command::FullScreen => Task::done(Message::Window(WindowAction::ToggleFullscreen)),
            Command::ToggleHidden => {
                self.prefs.hidden_files = !self.prefs.hidden_files;
                self.store_prefs();
                self.refresh_tree()
            }
            Command::RefreshTree => self.refresh_tree(),
            Command::Syntax | Command::TabWidths | Command::Endings => {
                let kind = match command {
                    Command::Syntax => MenuKind::Syntax,
                    Command::TabWidths => MenuKind::TabWidths,
                    _ => MenuKind::Endings,
                };
                let at = self.menu.as_ref().map_or(Point::new(8.0, crate::chrome::HEADER), |menu| menu.at);
                self.open_menu(kind, at, self.menu.as_ref().is_some_and(|menu| menu.selected.is_some()));
                Task::none()
            }
            Command::SetLanguage(language) => {
                if let Some(doc) = self.doc_mut() {
                    doc.pinned = language;
                    if language.is_none() {
                        doc.redetect();
                    }
                    doc.highlighted = u64::MAX;
                }
                Task::none()
            }
            Command::ToggleSpaces => {
                self.prefs.spaces = !self.prefs.spaces;
                self.store_prefs();
                Task::none()
            }
            Command::TabWidth(width) => {
                self.prefs.tab_width = width.clamp(1, 16);
                self.store_prefs();
                Task::none()
            }
            Command::ToggleAutoIndent => {
                self.prefs.auto_indent = !self.prefs.auto_indent;
                self.store_prefs();
                Task::none()
            }
            Command::ToggleAutoPair => {
                self.prefs.auto_pair = !self.prefs.auto_pair;
                self.store_prefs();
                Task::none()
            }
            Command::SetEnding(ending) => {
                if let Some(doc) = self.doc_mut()
                    && doc.ending != ending
                {
                    doc.ending = ending;
                    doc.saved = u64::MAX;
                }
                Task::none()
            }
            Command::ToggleTrimOnSave => {
                self.prefs.trim_on_save = !self.prefs.trim_on_save;
                self.store_prefs();
                Task::none()
            }
            Command::Statistics => self.show(DialogKind::Statistics),
            Command::Settings => self.show(DialogKind::Settings),
            Command::Shortcuts => self.show(DialogKind::Shortcuts),
            Command::About => self.show(DialogKind::About),
            Command::NextTab | Command::PreviousTab => {
                let count = self.docs.len();
                if count > 1 {
                    let next = if command == Command::NextTab {
                        (self.active + 1) % count
                    } else {
                        (self.active + count - 1) % count
                    };
                    self.select(next);
                }
                Task::none()
            }
            Command::SelectTab(index) => {
                self.select(index);
                Task::none()
            }
            Command::MoveTab(step) => {
                self.move_tab(self.active, self.active.saturating_add_signed(step));
                Task::none()
            }
            Command::MenuBar if self.shell_menu => Task::none(),
            Command::MenuBar => {
                let at = Point::new(crate::chrome::bar_x(self, Bar::File), crate::chrome::HEADER - 2.0);
                let kind = if crate::chrome::compact(self) { MenuKind::Compact } else { MenuKind::Bar(Bar::File) };
                self.open_menu(kind, at, true);
                Task::none()
            }
            Command::ContextMenu => {
                let at = if self.pointer == Point::ORIGIN {
                    Point::new(self.window.size.width / 2.0 - 120.0, self.window.size.height / 3.0)
                } else {
                    self.pointer
                };
                self.open_menu(MenuKind::Context, at, true);
                Task::none()
            }
            Command::TreeOpen | Command::TreeShow | Command::TreeCopyPath => {
                let Some(MenuKind::Tree(path)) = self.menu.as_ref().map(|menu| menu.kind.clone()) else {
                    return Task::none();
                };
                match command {
                    Command::TreeOpen => self.handle(Message::TreeClicked(path)),
                    Command::TreeShow => {
                        show_in_files(path);
                        Task::none()
                    }
                    _ => clipboard::write(path.display().to_string()),
                }
            }
        }
    }

    fn store_prefs(&mut self) {
        if let Err(error) = config::save(&self.prefs) {
            self.tell(format!("{}: {error}", Text::SaveFailed.text(self.lang)));
        }
    }

    fn show(&mut self, kind: DialogKind) -> Task<Message> {
        let mut reveal = Reveal::closed().with_ease(motion::ease_out);
        reveal.open(motion::t2());
        self.dialog = Some(Dialog { kind, reveal });
        Task::none()
    }

    fn hide_dialog(&mut self) {
        if let Some(mut dialog) = self.dialog.take() {
            dialog.reveal = dialog.reveal.with_ease(motion::ease_io);
            dialog.reveal.close(motion::t2());
            self.closing = Some(dialog);
        }
    }

    fn transform(&mut self, command: Command) {
        let indent = self.indent();
        let Some(doc) = self.doc_mut() else { return };
        let selection = doc.selected();
        let language = doc.language();
        let change = match command {
            Command::DuplicateLine => Some(edit::duplicate(&doc.text, &selection)),
            Command::DeleteLine => Some(edit::delete_lines(&doc.text, &selection)),
            Command::MoveLine(up) => edit::move_lines(&doc.text, &selection, up),
            Command::JoinLines => edit::join_lines(&doc.text, &selection),
            Command::SortLines => edit::sort_lines(&doc.text, &selection),
            Command::ToggleComment => edit::toggle_comment(&doc.text, &selection, language),
            Command::ToggleCheck if language == Language::Markdown => edit::toggle_check(&doc.text, &selection),
            Command::Indent => Some(edit::indent(&doc.text, &selection, indent)),
            Command::Unindent => edit::unindent(&doc.text, &selection, indent),
            Command::Case(case) => edit::change_case(&doc.text, &selection, case),
            Command::TrimTrailing => edit::trim_trailing(&doc.text).map(|text| {
                let caret = selection.start.min(text.len());
                edit::Change { range: 0..doc.text.len(), text, select: caret..caret }
            }),
            Command::InsertDate => {
                let text = date_time();
                let at = selection.start + text.len();
                Some(edit::Change { range: selection, text, select: at..at })
            }
            _ => None,
        };
        if let Some(change) = change {
            doc.replace(change.range, &change.text, Kind::Other);
            doc.place(change.select);
        }
    }

    fn on_find(&mut self, message: Message) -> Task<Message> {
        let Some(find) = &mut self.find else { return Task::none() };
        match message {
            Message::FindInput(text) => {
                find.query.text = text;
                find.revision = u64::MAX;
                self.refresh_find();
                self.step_from_selection_start();
            }
            Message::ReplaceInput(text) => find.replace = text,
            Message::FindSubmit => self.step(!self.modifiers.shift()),
            Message::FindToggle(toggle) => {
                match toggle {
                    Toggle::Case => find.query.case = !find.query.case,
                    Toggle::Word => find.query.word = !find.query.word,
                    Toggle::Regex => find.query.regex = !find.query.regex,
                }
                find.revision = u64::MAX;
            }
            Message::ToggleReplace => {
                find.replacing = !find.replacing;
                if find.replacing {
                    return operation::focus(REPLACE_ID);
                }
            }
            Message::FindClose => {
                self.find = None;
                return operation::focus(EDITOR);
            }
            Message::ReplaceOne => self.replace_one(),
            Message::ReplaceAll => self.replace_all(),
            _ => {}
        }
        Task::none()
    }

    fn refresh_find(&mut self) {
        let Some(doc) = self.docs.get(self.active) else { return };
        let Some(find) = &mut self.find else { return };
        if find.revision == doc.revision && find.doc == doc.id {
            return;
        }
        find.revision = doc.revision;
        find.doc = doc.id;
        match find.query.compile() {
            Ok(Some(pattern)) => {
                find.found = find::matches(&pattern, &doc.text);
                find.error = None;
            }
            Ok(None) => {
                find.found.clear();
                find.error = None;
            }
            Err(error) => {
                find.found.clear();
                find.error = Some(error);
            }
        }
        let selection = doc.selected();
        find.current = find.found.iter().position(|range| *range == selection);
    }

    fn step_from_selection_start(&mut self) {
        let Some(doc) = self.docs.get_mut(self.active) else { return };
        let Some(find) = &mut self.find else { return };
        let from = doc.selected().start;
        find.current = find::next(&find.found, from, true);
        if let Some(range) = find.current.and_then(|at| find.found.get(at)) {
            doc.place(range.clone());
        }
    }

    fn step(&mut self, forward: bool) {
        self.refresh_find();
        let Some(doc) = self.docs.get_mut(self.active) else { return };
        let Some(find) = &mut self.find else { return };
        let selection = doc.selected();
        let from = if forward { selection.end } else { selection.start };
        find.current = find::next(&find.found, from, forward);
        if let Some(range) = find.current.and_then(|at| find.found.get(at)) {
            doc.place(range.clone());
        }
    }

    fn replace_one(&mut self) {
        self.refresh_find();
        let Some(doc) = self.docs.get_mut(self.active) else { return };
        let Some(find) = &self.find else { return };
        let Ok(Some(pattern)) = find.query.compile() else { return };
        let selection = doc.selected();
        if find.found.contains(&selection) {
            let replacement = find::replacement(&pattern, &find.query, &doc.text, &selection, &find.replace);
            let at = selection.start + replacement.len();
            doc.replace(selection, &replacement, Kind::Other);
            doc.place(at..at);
        }
        self.step(true);
    }

    fn replace_all(&mut self) {
        let Some(doc) = self.docs.get_mut(self.active) else { return };
        let Some(find) = &self.find else { return };
        let Ok(Some(pattern)) = find.query.compile() else { return };
        let (text, count) = find::replace_all(&pattern, &find.query, &doc.text, &find.replace);
        if count == 0 {
            return;
        }
        let caret = doc.selected().start.min(text.len());
        let whole = 0..doc.text.len();
        doc.replace(whole, &text, Kind::Other);
        doc.place(caret..caret);
        let message = format!("{}: {count}", Text::Replaced.text(self.lang));
        self.tell(message);
    }

    fn finder_root(&self) -> Option<PathBuf> {
        self.tree
            .as_ref()
            .map(|tree| tree.root.clone())
            .or_else(|| self.doc()?.path.as_deref()?.parent().map(Path::to_path_buf))
    }

    fn open_finder(&mut self, mode: finder::Mode) -> Task<Message> {
        let root = self.finder_root();
        if mode == finder::Mode::Search && root.is_none() {
            self.tell(Text::NeedFolder.text(self.lang).to_string());
            return Task::none();
        }
        let generation = self.finder_latest.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        let mut opened = Finder::new(mode, root.clone(), generation);
        let task = match (mode, root) {
            (finder::Mode::Symbols, _) => {
                if let Some(doc) = self.doc() {
                    opened.fill(finder::symbols(doc.language(), &doc.text));
                } else {
                    opened.fill(Vec::new());
                }
                Task::none()
            }
            (finder::Mode::Files, Some(root)) => {
                let hidden = self.prefs.hidden_files;
                blocking(
                    move || finder::index(&root, hidden),
                    move |entries| Message::Finder(FinderMessage::Filled(generation, entries)),
                )
            }
            (finder::Mode::Files, None) => {
                let loose = self.docs.iter().filter_map(|doc| doc.path.clone()).chain(self.recent.iter().cloned());
                let mut seen = HashSet::new();
                let entries = loose
                    .filter(|path| seen.insert(path.clone()))
                    .map(|path| finder::Entry {
                        label: path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
                        detail: path.parent().map(|parent| parent.to_string_lossy().into_owned()).unwrap_or_default(),
                        target: finder::Target::File(path),
                    })
                    .collect();
                opened.fill(entries);
                Task::none()
            }
            (finder::Mode::Search, _) => {
                opened.busy = false;
                Task::none()
            }
        };
        self.finder = Some(opened);
        self.dismiss_menu();
        Task::batch([operation::focus(FINDER_ID), task])
    }

    fn on_finder(&mut self, message: FinderMessage) -> Task<Message> {
        let Some(open) = &mut self.finder else { return Task::none() };
        match message {
            FinderMessage::Input(query) => {
                open.query = query;
                if open.mode != finder::Mode::Search {
                    open.rank();
                    return Task::none();
                }
                let generation = self.finder_latest.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                open.generation = generation;
                open.entries.clear();
                open.rank();
                let (Some(root), false) = (open.root.clone(), open.query.is_empty()) else {
                    open.busy = false;
                    return Task::none();
                };
                open.busy = true;
                let query = Query { text: open.query.clone(), ..Query::default() };
                let latest = Arc::clone(&self.finder_latest);
                let hidden = self.prefs.hidden_files;
                blocking(
                    move || finder::search(&root, &query, hidden, generation, &latest),
                    move |entries| Message::Finder(FinderMessage::Filled(generation, entries)),
                )
            }
            FinderMessage::Step(forward) => {
                open.step(forward);
                Task::none()
            }
            FinderMessage::Pick(index) => {
                open.selected = index;
                self.on_finder(FinderMessage::Submit)
            }
            FinderMessage::Filled(generation, entries) => {
                if open.generation == generation {
                    open.fill(entries);
                }
                Task::none()
            }
            FinderMessage::Close => {
                self.finder = None;
                operation::focus(EDITOR)
            }
            FinderMessage::Submit => {
                let Some(target) = open.current().map(|entry| entry.target.clone()) else { return Task::none() };
                self.finder = None;
                match target {
                    finder::Target::File(path) => self.open_path(path),
                    finder::Target::Line(line) => {
                        self.place_line(line);
                        operation::focus(EDITOR)
                    }
                    finder::Target::Match(path, line) => {
                        if let Some(index) = self.docs.iter().position(|doc| doc.path.as_deref() == Some(&path)) {
                            self.select(index);
                            self.place_line(line);
                            return operation::focus(EDITOR);
                        }
                        self.pending_jump = Some((path.clone(), line));
                        self.open_path(path)
                    }
                }
            }
        }
    }

    fn place_line(&mut self, line: usize) {
        let Some(doc) = self.doc_mut() else { return };
        let line = line.min(doc.text.matches('\n').count());
        let start = document::offset(&doc.text, iced::widget::text_editor::Position { line, column: 0 });
        let end = doc.text[start..].find('\n').map_or(doc.text.len(), |at| start + at);
        doc.place(start..end);
    }

    fn go_to(&mut self) -> Task<Message> {
        let Some(input) = self.goto.take() else { return Task::none() };
        let mut parts = input.trim().split([':', ',', ' ']).filter(|part| !part.is_empty());
        let line = parts.next().and_then(|part| part.parse::<usize>().ok());
        let column = parts.next().and_then(|part| part.parse::<usize>().ok()).unwrap_or(1);
        if let (Some(line), Some(doc)) = (line, self.doc_mut()) {
            let line_index = line.saturating_sub(1).min(doc.text.matches('\n').count());
            let start =
                document::offset(&doc.text, iced::widget::text_editor::Position { line: line_index, column: 0 });
            let end = doc.text[start..].find('\n').map_or(doc.text.len(), |at| start + at);
            let at = doc.text[start..end]
                .char_indices()
                .nth(column.saturating_sub(1))
                .map_or(end, |(index, _)| start + index);
            doc.place(at..at);
        }
        operation::focus(EDITOR)
    }

    fn choose(&mut self, purpose: Purpose) -> Task<Message> {
        let lang = self.lang;
        let folder = self
            .doc()
            .and_then(|doc| doc.path.as_deref()?.parent().map(Path::to_path_buf))
            .or_else(|| self.tree.as_ref().map(|tree| tree.root.clone()))
            .or_else(glide_config::home);
        let (title, accept) = match purpose {
            Purpose::Open => (Text::Open, Text::OpenItem),
            Purpose::Folder => (Text::OpenFolder, Text::OpenItem),
            Purpose::Save => (Text::SaveAs, Text::Save),
        };
        let ask = Ask {
            purpose,
            title: title.text(lang).trim_end_matches('…').to_string(),
            accept: accept.text(lang).to_string(),
            name: None,
            folder,
        };
        Task::perform(chooser::spawn(ask), move |chosen| Message::Chosen(purpose, chosen.unwrap_or(Chosen::Cancelled)))
    }

    fn chosen(&mut self, purpose: Purpose, chosen: Chosen) -> Task<Message> {
        match chosen {
            Chosen::Paths(paths) => match purpose {
                Purpose::Folder => paths.into_iter().next().map_or_else(Task::none, |folder| self.open_folder(folder)),
                _ => Task::batch(paths.into_iter().map(|path| self.open_path(path)).collect::<Vec<_>>()),
            },
            Chosen::Cancelled => Task::none(),
            Chosen::Failed(error) => {
                self.tell(format!("{}: {error}", Text::ChooserFailed.text(self.lang)));
                Task::none()
            }
        }
    }

    pub fn open_path(&mut self, path: PathBuf) -> Task<Message> {
        if path.is_dir() {
            return self.open_folder(path);
        }
        if let Some(index) = self.docs.iter().position(|doc| doc.path.as_deref() == Some(&path)) {
            self.select(index);
            return operation::focus(EDITOR);
        }
        let read = path.clone();
        blocking(move || document::read(&read), move |result| Message::Loaded(path, result))
    }

    fn loaded(&mut self, path: PathBuf, result: Result<Decoded, LoadError>) -> Task<Message> {
        let decoded = match result {
            Ok(decoded) => decoded,
            Err(LoadError::Io(_)) if !path.exists() => Decoded {
                text: String::new(),
                encoding: Encoding::Utf8,
                bom: false,
                ending: Ending::Lf,
                mixed: false,
                suspect: false,
            },
            Err(error) => {
                let reason = match error {
                    LoadError::Binary => Text::Binary.text(self.lang).to_string(),
                    LoadError::Io(error) => error,
                };
                self.tell(format!("{} {}: {reason}", Text::OpenFailed.text(self.lang), path.display()));
                return Task::none();
            }
        };
        if let Some(index) = self.docs.iter().position(|doc| doc.path.as_deref() == Some(&path)) {
            self.select(index);
            return operation::focus(EDITOR);
        }
        config::remember(&mut self.recent, &path);
        let reveal = self.tree.as_mut().map(|tree| tree.reveal(&path)).unwrap_or_default();
        let id = self.take_id();
        let mixed = decoded.mixed;
        let doc = Document::open(id, path, decoded);
        if mixed {
            self.tell(Text::MixedEndings.text(self.lang).to_string());
        }
        let replace =
            self.doc().is_some_and(|current| current.path.is_none() && current.text.is_empty() && !current.dirty());
        if replace {
            self.docs[self.active] = doc;
        } else {
            let at = if self.docs.is_empty() { 0 } else { self.active + 1 };
            self.docs.insert(at, doc);
            self.active = at;
        }
        if let Some((jump, line)) = self.pending_jump.take()
            && self.doc().and_then(|doc| doc.path.as_ref()) == Some(&jump)
        {
            self.place_line(line);
        }
        let mut tasks: Vec<_> = reveal.into_iter().map(|folder| self.list(folder)).collect();
        tasks.push(operation::focus(EDITOR));
        Task::batch(tasks)
    }

    pub fn open_folder(&mut self, folder: PathBuf) -> Task<Message> {
        config::remember(&mut self.recent, &folder);
        self.tree = Some(Tree::new(folder.clone()));
        if !self.prefs.sidebar {
            self.prefs.sidebar = true;
            self.store_prefs();
        }
        self.list(folder)
    }

    fn list(&self, folder: PathBuf) -> Task<Message> {
        let hidden = self.prefs.hidden_files;
        let read = folder.clone();
        blocking(
            move || tree::list(&read, hidden).map_err(|error| error.to_string()),
            move |result| Message::Listed(folder, result),
        )
    }

    fn refresh_tree(&mut self) -> Task<Message> {
        let Some(tree) = &self.tree else { return Task::none() };
        let mut folders = vec![tree.root.clone()];
        folders.extend(tree.expanded());
        Task::batch(folders.into_iter().map(|folder| self.list(folder)).collect::<Vec<_>>())
    }

    fn save_active(&mut self, as_new: bool) -> Task<Message> {
        let Some(id) = self.doc().map(|doc| doc.id) else { return Task::none() };
        self.queue.clear();
        self.after = None;
        self.save(id, as_new)
    }

    fn save(&mut self, id: u64, as_new: bool) -> Task<Message> {
        let Some(index) = self.index_of(id) else { return self.abort_queue() };
        let doc = &self.docs[index];
        if let (Some(path), false) = (doc.path.clone(), as_new) {
            return self.write(id, path);
        }
        let ask = Ask {
            purpose: Purpose::Save,
            title: Text::SaveAs.text(self.lang).trim_end_matches('…').to_string(),
            accept: Text::Save.text(self.lang).to_string(),
            name: Some(suggested_name(doc, self.untitled())),
            folder: doc
                .path
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .or_else(|| self.tree.as_ref().map(|tree| tree.root.clone()))
                .or_else(glide_config::home),
        };
        Task::perform(chooser::spawn(ask), move |chosen| Message::SaveTo(id, chosen.unwrap_or(Chosen::Cancelled)))
    }

    fn write(&mut self, id: u64, path: PathBuf) -> Task<Message> {
        let trim = self.prefs.trim_on_save;
        let Some(index) = self.index_of(id) else { return Task::none() };
        if let Some(hazard) = self.docs[index].hazard() {
            return self.show(DialogKind::Encoding { id, path, hazard });
        }
        let doc = &mut self.docs[index];
        if trim && let Some(text) = edit::trim_trailing(&doc.text) {
            let caret = doc.selected().start.min(text.len());
            let whole = 0..doc.text.len();
            doc.replace(whole, &text, Kind::Other);
            doc.place(caret..caret);
        }
        let generation = doc.save_point();
        let bytes = doc.bytes();
        let target = path.clone();
        blocking(
            move || document::write(&target, &bytes).map_err(|error| error.to_string()),
            move |result| Message::Saved(id, path, generation, result),
        )
    }

    fn saved(&mut self, id: u64, path: &Path, generation: u64, result: Result<SystemTime, String>) -> Task<Message> {
        match result {
            Ok(disk) => {
                if let Some(index) = self.index_of(id) {
                    let doc = &mut self.docs[index];
                    doc.mark_saved(path.to_path_buf(), disk);
                    doc.saved = generation;
                    if let Some(draft) = doc.draft.take() {
                        let _ = std::fs::remove_file(draft);
                    }
                }
                config::remember(&mut self.recent, path);
                let tree = self.tree.as_ref().is_some_and(|tree| path.starts_with(&tree.root));
                let refresh = if tree { self.refresh_tree() } else { Task::none() };
                Task::batch([refresh, self.next_in_queue()])
            }
            Err(error) => {
                self.tell(format!("{} {}: {error}", Text::SaveFailed.text(self.lang), path.display()));
                self.abort_queue()
            }
        }
    }

    fn next_in_queue(&mut self) -> Task<Message> {
        if self.queue.is_empty() {
            return match self.after.take() {
                Some(after) => self.finish(after),
                None => Task::none(),
            };
        }
        let id = self.queue.remove(0);
        self.save(id, false)
    }

    fn abort_queue(&mut self) -> Task<Message> {
        self.queue.clear();
        self.after = None;
        Task::none()
    }

    fn revert(&mut self, id: u64) -> Task<Message> {
        let Some(path) = self.index_of(id).and_then(|index| self.docs[index].path.clone()) else {
            return Task::none();
        };
        self.reverting.insert(id);
        blocking(move || document::read(&path), move |result| Message::Reloaded(id, result))
    }

    fn request_close(&mut self, id: u64) -> Task<Message> {
        let Some(index) = self.index_of(id) else { return Task::none() };
        if self.docs[index].dirty() && (self.docs[index].path.is_some() || !self.docs[index].text.is_empty()) {
            return self.show(DialogKind::Unsaved { docs: vec![id], then: After::Close(id) });
        }
        self.finish(After::Close(id))
    }

    fn request_window_close(&mut self) -> Task<Message> {
        if self.primary.is_some()
            && self.prefs.restore_session
            && self.save_session().is_some_and(|saving| saving.join().unwrap_or(false))
        {
            return iced::exit();
        }
        let dirty: Vec<u64> = self
            .docs
            .iter()
            .filter(|doc| doc.dirty() && (doc.path.is_some() || !doc.text.is_empty()))
            .map(|doc| doc.id)
            .collect();
        if dirty.is_empty() {
            return self.finish(After::Window);
        }
        self.show(DialogKind::Unsaved { docs: dirty, then: After::Window })
    }

    fn finish(&mut self, after: After) -> Task<Message> {
        match after {
            After::Close(id) => {
                let Some(index) = self.index_of(id) else { return Task::none() };
                let doc = self.docs.remove(index);
                if let Some(draft) = doc.draft {
                    let _ = std::fs::remove_file(draft);
                }
                if self.active >= index && self.active > 0 {
                    self.active -= 1;
                }
                if self.docs.is_empty() {
                    self.new_document();
                }
                self.active = self.active.min(self.docs.len() - 1);
                operation::focus(EDITOR)
            }
            After::Window => {
                if let Some(saving) = self.save_session() {
                    let _ = saving.join();
                }
                iced::exit()
            }
        }
    }

    fn answer(&mut self, answer: Answer) -> Task<Message> {
        let Some(dialog) = self.dialog.clone() else { return Task::none() };
        self.hide_dialog();
        if let DialogKind::Revert(id) = dialog.kind {
            return if answer == Answer::Discard { self.revert(id) } else { Task::none() };
        }
        if let DialogKind::Encoding { id, path, hazard } = dialog.kind {
            return match (answer, self.index_of(id)) {
                (Answer::Save, Some(index)) => {
                    self.docs[index].accept(hazard);
                    self.write(id, path)
                }
                _ => self.abort_queue(),
            };
        }
        let DialogKind::Unsaved { docs, then } = dialog.kind else { return Task::none() };
        match answer {
            Answer::Cancel => Task::none(),
            Answer::Discard => {
                if let After::Close(id) = then
                    && let Some(index) = self.index_of(id)
                {
                    self.docs[index].saved = self.docs[index].history.generation();
                }
                if then == After::Window {
                    for doc in &mut self.docs {
                        doc.saved = doc.history.generation();
                    }
                }
                self.finish(then)
            }
            Answer::Save => {
                self.queue = docs;
                self.after = Some(then);
                self.next_in_queue()
            }
        }
    }

    fn save_session(&mut self) -> Option<std::thread::JoinHandle<bool>> {
        static ORDER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        self.primary.as_ref()?;
        let keep = self.prefs.restore_session;
        let drafts = config::drafts();
        let mut tabs = Vec::new();
        let mut writes: Vec<(PathBuf, String)> = Vec::new();
        let mut unkept = false;
        for doc in &mut self.docs {
            if !keep {
                break;
            }
            let dirty = doc.dirty() && !(doc.path.is_none() && doc.text.is_empty());
            if doc.path.is_none() && doc.text.is_empty() {
                continue;
            }
            if !dirty && doc.path.as_deref().is_some_and(|path| path.to_str().is_none()) {
                continue;
            }
            let draft = if dirty {
                drafts.as_ref().map(|folder| {
                    let file = doc
                        .draft
                        .clone()
                        .unwrap_or_else(|| folder.join(format!("{}-{}.txt", std::process::id(), doc.id)));
                    doc.draft = Some(file.clone());
                    writes.push((file.clone(), doc.text.clone()));
                    file
                })
            } else {
                None
            };
            unkept |= dirty && draft.is_none();
            let cursor = doc.cursor().position;
            let disk = doc
                .disk
                .filter(|_| draft.is_some())
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|since| u64::try_from(since.as_nanos()).ok());
            tabs.push(SessionTab {
                path: doc.path.clone(),
                draft,
                language: doc.pinned.map(|language| language.key().to_string()),
                disk,
                line: cursor.line,
                column: cursor.column,
            });
        }
        let session = Session {
            folder: self.tree.as_ref().map(|tree| tree.root.clone()).filter(|root| keep && root.to_str().is_some()),
            active: self.active.min(tabs.len().saturating_sub(1)),
            tabs,
            recent: self.recent.iter().filter(|path| path.to_str().is_some()).cloned().collect(),
        };
        let order = ORDER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        std::thread::Builder::new()
            .name("glide-text-session".into())
            .spawn(move || write_session(order, drafts.as_deref(), &writes, &session) && !unkept)
            .ok()
    }

    #[must_use]
    pub(crate) fn find_label(&self) -> String {
        let Some(find) = &self.find else { return String::new() };
        if let Some(error) = &find.error {
            return error.clone();
        }
        if find.query.text.is_empty() {
            return String::new();
        }
        if find.found.is_empty() {
            return Text::NoMatches.text(self.lang).to_string();
        }
        let total =
            if find.found.len() >= find::LIMIT { format!("{}+", find::LIMIT) } else { find.found.len().to_string() };
        match find.current {
            Some(at) => format!("{} {} {total}", at + 1, Text::Of.text(self.lang)),
            None => total,
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
        Self::update(self, message)
    }

    fn view<'a>(&'a self, _: &'a glide_sdk::Shell<Message>) -> iced::Element<'a, Message> {
        Self::view(self)
    }

    fn subscription(&self) -> Subscription<Message> {
        Self::subscription(self)
    }

    fn close_requested(&mut self, _: &mut glide_sdk::Shell<Message>) -> Task<Message> {
        self.request_window_close()
    }

    fn window_changed(&mut self, shell: &mut glide_sdk::Shell<Message>) -> Task<Message> {
        self.follow_window(*shell.window())
    }

    fn key(&mut self, event: &keyboard::Event, captured: bool, _: &mut glide_sdk::Shell<Message>) -> Task<Message> {
        Self::update(self, Message::Key(event.clone(), captured))
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
    use iced::keyboard::key::NativeCode;
    use iced::widget::text_editor::Edit;

    fn letter(c: &str, code: Code) -> (Key, Physical) {
        (Key::Character(c.into()), Physical::Code(code))
    }

    #[test]
    fn shortcuts_follow_the_physical_key_on_cyrillic_layouts() {
        let (key, physical) = letter("ы", Code::KeyS);
        assert_eq!(shortcut(&key, physical, Modifiers::CTRL), Some(Command::Save));
        let (key, physical) = letter(".", Code::Slash);
        assert_eq!(shortcut(&key, physical, Modifiers::CTRL), Some(Command::ToggleComment));
        let (key, physical) = letter("s", Code::KeyS);
        assert_eq!(shortcut(&key, physical, Modifiers::CTRL | Modifiers::SHIFT), Some(Command::SaveAs));
        assert_eq!(shortcut(&key, physical, Modifiers::empty()), None, "plain letters are typed");
        let unidentified = Physical::Unidentified(NativeCode::Unidentified);
        let enter = Key::Named(Named::Enter);
        assert_eq!(shortcut(&enter, unidentified, Modifiers::CTRL), Some(Command::ToggleCheck));
        assert_eq!(shortcut(&enter, unidentified, Modifiers::empty()), None, "plain Enter is the newline binding");
        assert_eq!(shortcut(&key, physical, Modifiers::LOGO | Modifiers::CTRL), None, "Super belongs to the shell");
        let unidentified = Physical::Unidentified(NativeCode::Unidentified);
        assert_eq!(shortcut(&Key::Named(Named::F9), unidentified, Modifiers::empty()), Some(Command::ToggleSidebar));
    }

    #[test]
    fn every_menu_shortcut_label_matches_the_key_table() {
        let cases = [
            (letter("n", Code::KeyN), Modifiers::CTRL, Command::New),
            (letter("o", Code::KeyO), Modifiers::CTRL | Modifiers::SHIFT, Command::OpenFolder),
            (letter("z", Code::KeyZ), Modifiers::CTRL | Modifiers::SHIFT, Command::Redo),
            (letter("d", Code::KeyD), Modifiers::CTRL | Modifiers::SHIFT, Command::DuplicateLine),
            (letter("i", Code::KeyI), Modifiers::CTRL, Command::GoToLine),
        ];
        for ((key, physical), modifiers, command) in cases {
            assert_eq!(shortcut(&key, physical, modifiers), Some(command));
            assert!(shortcut_label(command).is_some());
        }
    }

    #[test]
    fn untitled_documents_suggest_a_name_from_their_first_line_and_language() {
        let mut doc = Document::new(1, 1);
        doc.set_text("# Мій план: тиждень\n\n- пункт".into());
        doc.detected = Language::Markdown;
        assert_eq!(suggested_name(&doc, "Без назви"), "Мій план тиждень.md");
        let empty = Document::new(2, 1);
        assert_eq!(suggested_name(&empty, "Untitled"), "Untitled.txt");
    }

    fn app() -> App {
        let (mut app, _) = App::launch(None, &[]);
        app.prefs = Preferences::default();
        app.lang = Lang::Uk;
        app
    }

    fn typed(app: &mut App, text: &str) {
        for c in text.chars() {
            let _ = app.update(if c == '\n' {
                Message::Command(Command::Newline)
            } else {
                Message::Edit(Action::Edit(Edit::Insert(c)))
            });
        }
    }

    #[test]
    fn typing_highlights_detects_and_undoes() {
        let mut app = app();
        typed(&mut app, "# Заголовок\n\n- пункт **жирний**\n");
        let doc = app.doc().unwrap();
        assert_eq!(doc.text, "# Заголовок\n\n- пункт **жирний**\n");
        let _ = app.update(Message::Tick);
        assert_eq!(app.doc().unwrap().language(), Language::Markdown);
        let _ = app.update(Message::Noop);
        let doc = app.doc().unwrap();
        assert!(!doc.stale(), "the markdown spans are installed synchronously");
        assert!(doc.lines[0].iter().any(|(_, style)| style.tint == Some(syntax::Tint::Heading)));
        let _ = app.update(Message::Command(Command::Undo));
        assert_ne!(app.doc().unwrap().text, "# Заголовок\n\n- пункт **жирний**\n");
    }

    #[test]
    fn newline_keeps_indentation_and_tab_indents_by_the_preference() {
        let mut app = app();
        typed(&mut app, "    x\ny");
        assert_eq!(app.doc().unwrap().text, "    x\n    y");
        app.prefs.spaces = false;
        let _ = app.update(Message::Command(Command::Indent));
        assert_eq!(app.doc().unwrap().text, "    x\n    y\t");
    }

    #[test]
    fn find_and_replace_walk_the_matches() {
        let mut app = app();
        typed(&mut app, "кіт і кіт і Кіт");
        let _ = app.update(Message::Command(Command::Replace));
        let _ = app.update(Message::FindInput("кіт".into()));
        assert_eq!(app.find.as_ref().unwrap().found.len(), 3);
        assert_eq!(app.find_label(), "1 з 3");
        let _ = app.update(Message::FindSubmit);
        assert_eq!(app.find_label(), "2 з 3");
        let _ = app.update(Message::ReplaceInput("пес".into()));
        let _ = app.update(Message::ReplaceAll);
        assert_eq!(app.doc().unwrap().text, "пес і пес і пес");
        let _ = app.update(Message::Command(Command::Undo));
        assert_eq!(app.doc().unwrap().text, "кіт і кіт і Кіт");
    }

    #[test]
    fn tabs_open_switch_reorder_and_close() {
        let mut app = app();
        let _ = app.update(Message::Command(Command::New));
        let _ = app.update(Message::Command(Command::New));
        assert_eq!(app.docs.len(), 3);
        assert_eq!(app.active, 2);
        let _ = app.update(Message::Command(Command::NextTab));
        assert_eq!(app.active, 0);
        let first = app.docs[0].id;
        let _ = app.update(Message::Command(Command::MoveTab(1)));
        assert_eq!(app.docs[1].id, first);
        typed(&mut app, "draft");
        let _ = app.update(Message::Command(Command::Close));
        assert!(matches!(app.dialog.as_ref().map(|dialog| &dialog.kind), Some(DialogKind::Unsaved { .. })));
        let _ = app.update(Message::Dialog(Answer::Discard));
        assert_eq!(app.docs.len(), 2);
        assert!(app.docs.iter().all(|doc| doc.id != first));
    }

    #[test]
    fn opening_a_file_replaces_the_blank_untitled_document_and_saving_writes_it_back() {
        let folder = std::env::temp_dir().join(format!("glide-text-app-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("notes.md");
        std::fs::write(&path, "# Нотатки\r\nрядок\r\n").unwrap();
        let mut app = app();
        let decoded = document::read(&path);
        let _ = app.update(Message::Loaded(path.clone(), decoded));
        assert_eq!(app.docs.len(), 1);
        let doc = app.doc().unwrap();
        assert_eq!((doc.language(), doc.ending), (Language::Markdown, Ending::CrLf));
        let _ = app.update(Message::Edit(Action::Move(iced::widget::text_editor::Motion::DocumentEnd)));
        typed(&mut app, "ще");
        let doc = app.doc().unwrap();
        let (id, generation) = (doc.id, doc.history.generation());
        let disk = document::write(&path, &doc.bytes()).map_err(|error| error.to_string());
        let _ = app.update(Message::Saved(id, path.clone(), generation, disk));
        assert!(!app.doc().unwrap().dirty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# Нотатки\r\nрядок\r\nще");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    fn encoding_dialog(app: &App) -> Option<Hazard> {
        match app.dialog.as_ref().map(|dialog| &dialog.kind) {
            Some(DialogKind::Encoding { hazard, .. }) => Some(*hazard),
            _ => None,
        }
    }

    #[test]
    fn saving_a_latin1_file_with_new_characters_asks_before_converting_it() {
        let folder = std::env::temp_dir().join(format!("glide-text-latin1-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("old.txt");
        std::fs::write(&path, b"caf\xe9").unwrap();
        let mut app = app();
        let _ = app.update(Message::Loaded(path.clone(), document::read(&path)));
        let _ = app.update(Message::Edit(Action::Move(iced::widget::text_editor::Motion::DocumentEnd)));
        typed(&mut app, "ж");
        let _ = app.update(Message::Command(Command::Save));
        assert_eq!(encoding_dialog(&app), Some(Hazard::Convert));
        assert_eq!(app.doc().unwrap().encoding, Encoding::Latin1);
        let _ = app.update(named(Named::Enter, Code::Enter, false));
        assert_eq!(encoding_dialog(&app), None, "Enter is not consent to an irreversible conversion");
        assert_eq!(app.doc().unwrap().encoding, Encoding::Latin1);
        assert_eq!(std::fs::read(&path).unwrap(), b"caf\xe9", "nothing reached the disk");
        let _ = app.update(Message::Command(Command::Save));
        let _ = app.update(Message::Dialog(Answer::Save));
        assert_eq!(encoding_dialog(&app), None);
        assert_eq!(app.doc().unwrap().encoding, Encoding::Utf8);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_cancelled_conversion_stops_a_save_all_queue() {
        let mut app = app();
        let mut doc = Document::open(app.take_id(), PathBuf::from("/nonexistent/glide-a.txt"), {
            document::decode(b"caf\xe9").unwrap()
        });
        doc.perform(Action::Edit(Edit::Insert('ж')));
        let first = doc.id;
        app.docs = vec![doc, Document::new(app.take_id(), 1)];
        app.queue = vec![app.docs[1].id];
        app.after = Some(After::Window);
        let _ = app.save(first, false);
        assert_eq!(encoding_dialog(&app), Some(Hazard::Convert));
        let _ = app.update(Message::Dialog(Answer::Cancel));
        assert!(app.queue.is_empty() && app.after.is_none());
    }

    #[test]
    fn menus_open_from_the_bar_switch_with_arrows_and_close_with_escape() {
        let mut app = app();
        let _ = app.update(Message::BarPressed(Bar::Edit));
        assert_eq!(app.menu.as_ref().map(|menu| menu.kind.clone()), Some(MenuKind::Bar(Bar::Edit)));
        let _ = app.update(named(Named::ArrowRight, Code::ArrowRight, false));
        assert_eq!(app.menu.as_ref().map(|menu| menu.kind.clone()), Some(MenuKind::Bar(Bar::View)));
        let _ = app.update(Message::Escape);
        assert!(!app.menu_open());
    }

    fn named(key: Named, code: Code, captured: bool) -> Message {
        Message::Key(
            keyboard::Event::KeyPressed {
                key: Key::Named(key),
                modified_key: Key::Named(key),
                physical_key: Physical::Code(code),
                location: keyboard::Location::Standard,
                modifiers: Modifiers::empty(),
                text: None,
                repeat: false,
            },
            captured,
        )
    }

    #[test]
    fn the_status_islands_wait_for_the_pointer_near_the_bottom_of_the_editor_pane() {
        let mut app = app();
        app.window.size = iced::Size::new(1200.0, 700.0);
        assert!(app.status_floating() && !app.status_visible(), "islands replace the docked bar by default");
        let at = |x: f32, y: f32| Message::Pointer(Point::new(x, y));
        let _ = app.update(at(600.0, 100.0));
        assert!(!app.status_reveal.is_open());
        let _ = app.update(at(100.0, 690.0));
        assert!(!app.status_reveal.is_open(), "the sidebar column does not summon them");
        let _ = app.update(at(600.0, 690.0));
        assert!(app.status_reveal.is_open());
        let _ = app.update(at(600.0, 100.0));
        assert!(!app.status_reveal.is_open());
        let _ = app.update(Message::Command(Command::ToggleAutoHideStatus));
        assert!(app.status_reveal.is_open(), "with auto-hide off they stay");
        let _ = app.update(Message::Command(Command::ToggleFocus));
        assert!(!app.status_floating(), "distraction-free mode hides them too");
    }

    #[test]
    fn settings_open_from_the_menu_key_and_every_switch_flips_its_preference() {
        let mut app = app();
        assert_eq!(
            shortcut(&Key::Character("".into()), Physical::Code(Code::Comma), Modifiers::CTRL),
            Some(Command::Settings)
        );
        let _ = app.update(Message::Command(Command::Settings));
        assert!(matches!(app.dialog.as_ref().map(|dialog| &dialog.kind), Some(DialogKind::Settings)));
        let before = app.prefs.clone();
        for command in [Command::ToggleAutoPair, Command::ToggleTrimOnSave, Command::ToggleAutoHideStatus] {
            let _ = app.update(Message::Command(command));
        }
        assert_ne!(app.prefs, before);
        assert_eq!(app.prefs.auto_pair, !before.auto_pair);
        assert_eq!(app.prefs.trim_on_save, !before.trim_on_save);
        assert_eq!(app.prefs.auto_hide_status, !before.auto_hide_status);
    }

    #[test]
    fn a_csv_file_previews_as_a_table_and_a_text_file_has_no_preview() {
        let mut app = app();
        let doc = app.doc_mut().unwrap();
        doc.replace(0..0, "id,name\n1,\"Ada, L.\"\n", Kind::Other);
        assert!(!doc.previewable());
        doc.path = Some(PathBuf::from("/tmp/people.csv"));
        assert!(doc.previewable());
        app.preview = true;
        app.refresh_preview();
        let (_, _, rows) = app.table.as_ref().expect("a table for a csv file");
        assert_eq!(rows, &[vec!["id".to_string(), "name".to_string()], vec!["1".to_string(), "Ada, L.".to_string()]]);
        assert!(app.markdown.is_none());
        app.doc_mut().unwrap().path = Some(PathBuf::from("/tmp/notes.txt"));
        app.refresh_preview();
        assert!(app.table.is_none(), "leaving a table file drops the table");
    }

    #[test]
    fn the_symbol_finder_filters_the_document_and_enter_selects_the_line() {
        let mut app = app();
        let doc = app.doc_mut().unwrap();
        doc.pinned = Some(Language::Markdown);
        doc.replace(0..0, "# One\n\ntext\n## Two\n", Kind::Other);
        let _ = app.update(Message::Command(Command::Symbols));
        assert_eq!(app.finder.as_ref().unwrap().hits.len(), 2);
        let _ = app.update(Message::Finder(FinderMessage::Input("two".into())));
        assert_eq!(app.finder.as_ref().unwrap().hits.len(), 1);
        let _ = app.update(Message::Finder(FinderMessage::Submit));
        assert!(app.finder.is_none());
        let doc = app.doc().unwrap();
        assert_eq!(&doc.text[doc.selected()], "  ## Two".trim_start());
    }

    #[test]
    fn searching_a_folder_needs_one_and_escape_closes_the_finder_before_anything_else() {
        let mut app = app();
        app.tree = None;
        let _ = app.update(Message::Command(Command::SearchFolder));
        assert!(app.finder.is_none() && app.notice.is_some(), "no folder, no search");
        let _ = app.update(Message::Command(Command::QuickOpen));
        assert!(app.finder.is_some());
        let _ = app.update(Message::Command(Command::ToggleFocus));
        let _ = app.update(named(Named::Escape, Code::Escape, true));
        assert!(app.finder.is_none() && app.focus, "the first Escape closes only the finder");
    }

    #[test]
    fn distraction_free_mode_hides_the_sidebar_and_status_and_escape_leaves_it_last() {
        let mut app = app();
        app.window.size = iced::Size::new(1200.0, 700.0);
        app.prefs.status_islands = false;
        assert!(app.sidebar_visible() && app.status_visible());
        let _ = app.update(Message::Command(Command::ToggleFocus));
        assert!(app.focus && !app.sidebar_visible() && !app.status_visible());
        let _ = app.update(Message::Command(Command::Find));
        let _ = app.update(named(Named::Escape, Code::Escape, true));
        assert!(app.find.is_none() && app.focus, "Escape closes the find bar before it leaves the mode");
        let _ = app.update(named(Named::Escape, Code::Escape, false));
        assert!(!app.focus && app.sidebar_visible() && app.status_visible());
    }

    fn opened_file(tag: &str, text: &str) -> (App, PathBuf, PathBuf) {
        let folder = std::env::temp_dir().join(format!("glide-text-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("notes.txt");
        std::fs::write(&path, text).unwrap();
        let mut app = app();
        let _ = app.update(Message::Loaded(path.clone(), document::read(&path)));
        assert_eq!(app.docs.len(), 1);
        (app, folder, path)
    }

    fn changed_behind_our_back(app: &mut App, path: &Path, text: &str) {
        std::fs::write(path, text).unwrap();
        app.docs[0].disk = Some(SystemTime::UNIX_EPOCH);
    }

    #[test]
    fn the_reload_button_replaces_a_modified_document_with_the_file_and_the_bar_stays_gone() {
        let (mut app, folder, path) = opened_file("reload", "one\n");
        typed(&mut app, "mine");
        changed_behind_our_back(&mut app, &path, "external\n");
        let _ = app.update(Message::Tick);
        assert!(app.doc().unwrap().changed_on_disk, "a modified document is flagged, not reloaded");
        let id = app.doc().unwrap().id;
        let _ = app.update(Message::Disk(true));
        assert!(app.reverting.contains(&id), "the reload was asked for, so it may replace unsaved edits");
        let _ = app.update(Message::Reloaded(id, document::read(&path)));
        let doc = app.doc().unwrap();
        assert_eq!(doc.text, "external\n");
        assert!(!doc.dirty() && !doc.changed_on_disk);
        assert_eq!(doc.disk, document::modified(&path));
        let _ = app.update(Message::Tick);
        assert!(!app.doc().unwrap().changed_on_disk, "the bar does not come back a second later");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn an_external_change_read_while_the_user_types_is_flagged_instead_of_dropped() {
        let (mut app, folder, path) = opened_file("raced", "one\n");
        changed_behind_our_back(&mut app, &path, "external\n");
        let _ = app.update(Message::Tick);
        assert!(!app.doc().unwrap().changed_on_disk, "a clean document is reloaded quietly");
        typed(&mut app, "x");
        let id = app.doc().unwrap().id;
        let _ = app.update(Message::Reloaded(id, document::read(&path)));
        let doc = app.doc().unwrap();
        assert_eq!(doc.text, "xone\n", "the edit made meanwhile is kept");
        assert!(doc.dirty() && doc.changed_on_disk, "the next save would overwrite the external edit unnoticed");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_re_read_that_fails_leaves_the_info_bar_up() {
        let (mut app, folder, _) = opened_file("unreadable", "one\n");
        let id = app.doc().unwrap().id;
        let _ = app.update(Message::Reloaded(id, Err(LoadError::Io("gone".into()))));
        assert!(app.doc().unwrap().changed_on_disk);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_highlight_that_finishes_after_a_language_switch_does_not_replace_the_new_one() {
        let mut app = app();
        let doc = app.doc_mut().unwrap();
        doc.pinned = Some(Language::Markdown);
        doc.replace(0..0, "# One\n\ntext\n", Kind::Other);
        let (id, revision) = (doc.id, doc.revision);
        let markdown = Arc::new(syntax::highlight(Language::Markdown, &doc.text));
        assert!(markdown[0].iter().any(|(_, style)| style.tint == Some(syntax::Tint::Heading)));
        app.in_flight.insert(id);
        let _ = app.update(Message::Command(Command::SetLanguage(Some(Language::Plain))));
        assert!(app.doc().unwrap().stale(), "the switch waits for the job that is still running");
        let _ = app.update(Message::Highlighted(id, revision, Language::Markdown, markdown));
        let doc = app.doc().unwrap();
        assert!(!doc.stale(), "the finished job was refused and Plain Text was installed instead");
        assert!(doc.lines.iter().all(Vec::is_empty), "no Markdown spans survive in a Plain Text document");
    }

    fn os(bytes: &[u8]) -> OsString {
        std::os::unix::ffi::OsStringExt::from_vec(bytes.to_vec())
    }

    #[test]
    fn launch_arguments_keep_paths_that_are_not_utf8_and_split_off_flags() {
        let odd = os(b"/tmp/caf\xe9.txt");
        let (fresh, paths) = parse_args([
            OsString::from("--new-window"),
            odd.clone(),
            OsString::from("file:///tmp/a%20b.md"),
            OsString::from("--unknown"),
            os(b"--\xff"),
            OsString::from("relative.txt"),
        ]);
        assert!(fresh);
        assert_eq!(paths, [PathBuf::from(odd), PathBuf::from("/tmp/a b.md"), PathBuf::from("relative.txt")]);
        let (fresh, paths) = parse_args([OsString::from("plain.txt")]);
        assert!(!fresh);
        assert_eq!(paths, [PathBuf::from("plain.txt")]);
        assert_eq!(parse_args(Vec::new()), (false, Vec::new()));
    }

    #[test]
    fn a_file_whose_name_is_not_utf8_opens_and_shows_in_the_chrome() {
        let folder = std::env::temp_dir().join(format!("glide-text-odd-name-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(os(b"caf\xe9.txt"));
        std::fs::write(&path, "hello\n").unwrap();
        let (mut app, _) = App::launch(None, std::slice::from_ref(&path));
        app.prefs = Preferences::default();
        let _ = app.update(Message::Loaded(path.clone(), document::read(&path)));
        assert_eq!(app.doc().and_then(|doc| doc.path.clone()), Some(path));
        assert!(app.title().contains("caf"));
        assert!(app.subtitle().is_some());
        let _ = app.update(Message::Tick);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn escape_captured_by_the_find_field_still_closes_one_bar_at_a_time() {
        let mut app = app();
        let _ = app.update(Message::Command(Command::Find));
        let _ = app.update(Message::Command(Command::GoToLine));
        let _ = app.update(named(Named::Escape, Code::Escape, true));
        assert!(app.goto.is_none() && app.find.is_some(), "the first Escape closes only the newest bar");
        let _ = app.update(named(Named::Escape, Code::Escape, true));
        assert!(app.find.is_none());
    }

    #[test]
    fn keys_under_a_dialog_answer_it_without_reaching_the_document() {
        let mut app = app();
        typed(&mut app, "draft");
        let _ = app.update(Message::Command(Command::Close));
        assert!(app.dialog.is_some());
        let _ = app.update(named(Named::ArrowDown, Code::ArrowDown, false));
        let _ = app.update(named(Named::Escape, Code::Escape, true));
        assert!(app.dialog.is_none());
        assert_eq!(app.doc().unwrap().text, "draft");
        assert_eq!(app.docs.len(), 1, "cancel keeps the document");
    }

    #[test]
    fn when_the_shell_shows_the_menu_bar_the_window_drops_its_own_and_runs_clicked_commands() {
        let mut app = app();
        let _ = app.update(Message::BarPressed(Bar::File));
        assert!(app.menu_open());
        let _ = app.update(Message::MenuBar(menubar::Event::Shown(true)));
        assert!(app.shell_menu);
        assert!(!app.menu_open(), "an in-window menu bar menu closes once the shell owns the menu bar");
        let _ = app.update(Message::Command(Command::MenuBar));
        assert!(!app.menu_open(), "F10 belongs to the shell's menu bar now");
        let (_, commands) = crate::chrome::export_menu(&app);
        app.exported = commands;
        let new = app.exported.iter().position(|command| *command == Command::New).unwrap() + 1;
        let _ = app.update(Message::MenuBar(menubar::Event::Activated(u32::try_from(new).unwrap())));
        assert_eq!(app.docs.len(), 2);
        let _ = app.update(Message::MenuBar(menubar::Event::Activated(9999)));
        assert_eq!(app.docs.len(), 2, "a stale id does nothing");
        let _ = app.update(Message::MenuBar(menubar::Event::Shown(false)));
        assert!(!app.shell_menu, "the window takes its menu bar back when the shell stops showing one");
    }
}

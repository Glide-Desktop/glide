use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant, SystemTime};

use iced::futures::channel::mpsc::{UnboundedSender, unbounded};
use iced::futures::channel::oneshot;
use iced::keyboard::{self, Modifiers, key::Named};
use iced::widget::{Id, operation};
use iced::{Element, Point, Size, Subscription, Task, window};

use crate::config::{self, ListColumn, Look};
use crate::dnd;
use crate::fs::jobs::{Answer, Choice, Meter, Op, Progress, Question, Report, Sink};
use crate::fs::tags;
use crate::fs::{
    ClipboardAction, DiskError, Disks, Extensions, FileClipboard, FileEntry, MountMode, Mounted, Offer, Place, Roots,
    Run, SortBy, SortConfig, Target, Trash, Volume, bookmarks_file, create_directory, create_empty_file, file_uri,
    home_dir, load_bookmarks, mounted_drives, open_in_terminal, open_with_default, path_from_arg, query_disk_space,
    read_directory, rename_item, search_recursive, sort_entries, standard_places, templates_dir, templates_in,
    unique_path,
};
use crate::i18n::{self, Key, Lang};
use crate::icons::Icon;
use crate::motion::{Curve, Fade, Speed};
use crate::native_icons::{Art, IconTheme};
use crate::theme::ThemeState;
use crate::widgets::content::Metrics;
use crate::widgets::{self, content};

pub const SEARCH_ID: &str = "search";
pub const PATH_ID: &str = "path";
pub const MODAL_ID: &str = "modal";
pub const FILES_ID: &str = "files";
pub const RENAME_ID: &str = "rename";

const SEARCH_LIMIT: usize = 500;
const OPEN_LIMIT: usize = 20;
const OPEN_WITH_LIMIT: usize = 6;
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const NOTICE_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_secs(2);
const QUIT_GRACE: Duration = Duration::from_secs(5);
const DRAG_THRESHOLD: f32 = 6.0;
const SPRING_DELAY: Duration = Duration::from_millis(900);
const DRAG_TICK: Duration = Duration::from_millis(120);
const EDGE_SCROLL: f32 = 36.0;
const SCROLL_STEP: f32 = 32.0;
const CONTENT_TOP: f32 = widgets::HEADER_HEIGHT + 1.0;
const KEY_MENU_INSET: f32 = 48.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Grid,
    List,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Dialog {
    NewFolder,
    NewFile,
    Rename(PathBuf),
    Delete(Vec<PathBuf>),
    EmptyTrash,
    Properties(Box<Properties>),
    Repair(String),
    Conflict { job: u64, question: Question, for_all: bool },
    Unlock(String),
    NewFromTemplate(PathBuf),
    Tags(Vec<PathBuf>),
    Quit,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Modal {
    pub dialog: Dialog,
    pub input: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Properties {
    pub entry: FileEntry,
    pub created: Option<SystemTime>,
    pub target: Option<PathBuf>,
    pub contents: Option<usize>,
    pub original: Option<PathBuf>,
    pub total: Option<(u64, usize)>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuKind {
    Selection,
    Background,
    Options,
    Location(PathBuf),
    Volume(String),
    Templates,
    Tags,
    Columns,
    OpenWith,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub kind: MenuKind,
    pub at: Point,
    pub offers: Vec<Offer>,
    pub open_with: bool,
    pub highlight: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Hover {
    Entry(PathBuf),
    Place(PathBuf),
    Crumb(PathBuf),
    Column(ListColumn),
    Bookmarks,
    Toast,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Undo {
    Restore(Vec<PathBuf>),
    MoveBack(Vec<(PathBuf, PathBuf)>),
    TrashCopies(Vec<PathBuf>),
    Rename(PathBuf, PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub enum NoticeAction {
    Undo,
    Repair(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum JobEvent {
    Progress(Progress),
    Ask(Question),
    Done(Box<Report>),
}

#[derive(Debug)]
pub struct Job {
    pub id: u64,
    pub op: Op,
    pub progress: Progress,
    target: PathBuf,
    from_clipboard: bool,
    cancel: Arc<AtomicBool>,
    pub pause: Arc<AtomicBool>,
    pub meter: Meter,
    pub launch: Option<Launch>,
    answers: mpsc::Sender<Answer>,
    question: Option<Question>,
}

impl Job {
    fn stop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.question = None;
        let _ = self.answers.send(Answer { choice: Choice::Cancel, for_all: false });
    }
}

struct ChannelSink {
    events: UnboundedSender<JobEvent>,
    answers: mpsc::Receiver<Answer>,
    pause: Arc<AtomicBool>,
}

impl Sink for ChannelSink {
    fn progress(&self, progress: Progress) {
        let _ = self.events.unbounded_send(JobEvent::Progress(progress));
    }

    fn paused(&self) -> bool {
        self.pause.load(Ordering::Relaxed)
    }

    fn ask(&self, question: Question) -> Answer {
        let _ = self.events.unbounded_send(JobEvent::Ask(question));
        self.answers.recv().unwrap_or(Answer { choice: Choice::Cancel, for_all: false })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub id: u64,
    pub text: String,
    pub action: Option<NoticeAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskAction {
    Mount,
    Unmount,
    Eject,
    Repair,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiskOutcome {
    pub object: String,
    pub name: String,
    pub action: DiskAction,
    pub result: Result<Option<Mounted>, DiskError>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Device<'a> {
    Place(&'a Place),
    Volume(&'a Volume),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Search {
    pub query: String,
    pub pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Up,
    Down,
    Left,
    Right,
    First,
    Last,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outcome {
    pub text: Option<String>,
    pub select: Vec<PathBuf>,
    pub undo: Option<Undo>,
    pub clear_clipboard: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    Initial,
    Refresh,
    Push,
    Jump(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    generation: u64,
    path: PathBuf,
    arrival: Arrival,
    select: Option<PathBuf>,
    result: Result<(Vec<FileEntry>, Option<u64>), String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Navigate(PathBuf),
    Reveal(PathBuf),
    GoHome,
    Back,
    Forward,
    Up,
    Reload,
    Pressed(PathBuf),
    Secondary(PathBuf),
    MenuTarget(MenuKind),
    PressedBackground,
    ContextMenu(Point),
    Hover(Hover, bool),
    Frame(Instant),
    TerminalAt(PathBuf),
    RunAction(Box<Offer>),
    ExtensionsLoaded(Arc<Extensions>),
    PropertiesOf(PathBuf),
    CopyText(String),
    AddBookmark(PathBuf),
    RemoveBookmark(PathBuf),
    ToggleOptions,
    CloseMenu,
    KeyMenu,
    MenuHover(usize, bool),
    Menu(Box<Message>),
    OpenSelection,
    SelectAll,
    Move(Step, bool),
    SetView(ViewMode),
    SortBy(SortBy),
    SortKey(SortBy),
    ToggleReverse,
    ToggleHidden,
    ToggleSearch,
    TypeToSearch(String),
    SearchChanged(String),
    SearchResults(u64, Vec<FileEntry>),
    EditPath,
    PathChanged(String),
    PathSubmitted,
    Cut,
    Copy,
    Paste,
    NewFolder,
    NewFile,
    Rename,
    Trash,
    Delete,
    EmptyTrash,
    Restore,
    Properties,
    Terminal,
    ModalInput(String),
    ModalSubmit,
    ModalCancel,
    Done(Outcome),
    DismissNotice(u64),
    NoticeExpired(u64),
    NoticeChosen(NoticeAction),
    Undo,
    Volumes(Vec<Volume>),
    Mount(String, bool),
    Unmount(String),
    Eject(String),
    Repair(String),
    Disk(Box<DiskOutcome>),
    Modifiers(Modifiers),
    PaneResized(Size),
    Tick,
    Escape,
    Quit,
    WindowDrag,
    WindowMaximize,
    WindowResize(Edge),
    Handles(Option<dnd::Handles>),
    Dnd(dnd::Event),
    Scrolled(f32),
    Cursor(Point),
    Released,
    DragTick,
    WindowSize(Size),
    WindowFocus(bool),
    WindowMaximized(bool),
    JobPause(u64),
    Duplicate,
    CreateLink,
    SubMenu(MenuKind),
    NewFromTemplate(PathBuf),
    OpenTemplates,
    ToggleTag(tags::Color),
    EditTags,
    RenameInput(String),
    RenameSubmit,
    Zoom(i8),
    ToggleColumn(ListColumn),
    ColumnPressed(ListColumn),
    BookmarkPressed(PathBuf),
    ToggleQueue,
    PasteText(Option<String>),
    Phones(Vec<crate::fs::mtp::Phone>),
    OpenPhone(String),
    PhoneOpened(Result<PathBuf, String>),
    ClosePhone(String),
    Listed(Box<Listed>),
    DirChanged,
    Job(u64, JobEvent),
    JobCancel(u64),
    Resolve(Choice),
    ToggleForAll,
    Service(crate::service::Request),
    Thumb(ThumbKey, Option<PathBuf>),
    FolderSize(PathBuf, (u64, usize)),
    Permission(u32),
    VolumesChanged,
}

pub type ThumbKey = (PathBuf, Option<SystemTime>);

#[derive(Debug, Clone, Copy, PartialEq)]
enum DragArm {
    Pressed,
    At(Point),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

impl From<Edge> for window::Direction {
    fn from(edge: Edge) -> Self {
        match edge {
            Edge::North => Self::North,
            Edge::South => Self::South,
            Edge::East => Self::East,
            Edge::West => Self::West,
            Edge::NorthEast => Self::NorthEast,
            Edge::NorthWest => Self::NorthWest,
            Edge::SouthEast => Self::SouthEast,
            Edge::SouthWest => Self::SouthWest,
        }
    }
}

impl Message {
    #[must_use]
    pub const fn dismisses_menu(&self) -> bool {
        matches!(
            self,
            Self::Trash
                | Self::Delete
                | Self::Rename
                | Self::Cut
                | Self::Copy
                | Self::Paste
                | Self::SelectAll
                | Self::Duplicate
                | Self::Undo
                | Self::Reload
                | Self::ToggleHidden
                | Self::Properties
                | Self::NewFolder
                | Self::SetView(_)
                | Self::Zoom(_)
                | Self::ToggleSearch
                | Self::Back
                | Self::Forward
                | Self::Up
                | Self::GoHome
                | Self::TypeToSearch(_)
        )
    }

    #[must_use]
    pub fn passes_modal(&self) -> bool {
        matches!(
            self,
            Self::ModalInput(_)
                | Self::ModalSubmit
                | Self::ModalCancel
                | Self::Escape
                | Self::Done(_)
                | Self::DismissNotice(_)
                | Self::NoticeExpired(_)
                | Self::Volumes(_)
                | Self::Disk(_)
                | Self::SearchResults(..)
                | Self::ExtensionsLoaded(_)
                | Self::Modifiers(_)
                | Self::PaneResized(_)
                | Self::Tick
                | Self::Frame(_)
                | Self::Hover(..)
                | Self::Quit
                | Self::WindowDrag
                | Self::WindowMaximize
                | Self::WindowResize(_)
                | Self::Handles(_)
                | Self::Dnd(_)
                | Self::Scrolled(_)
                | Self::Cursor(_)
                | Self::Released
                | Self::DragTick
                | Self::WindowSize(_)
                | Self::WindowFocus(_)
                | Self::WindowMaximized(_)
                | Self::JobPause(_)
                | Self::Phones(_)
                | Self::PhoneOpened(_)
                | Self::Listed(_)
                | Self::DirChanged
                | Self::Job(..)
                | Self::JobCancel(_)
                | Self::Resolve(_)
                | Self::ToggleForAll
                | Self::Service(_)
                | Self::Thumb(..)
                | Self::FolderSize(..)
                | Self::Permission(_)
                | Self::VolumesChanged
        )
    }
}

pub struct Surfaces {
    hover: HashMap<Hover, (Fade, Fade)>,
    side: HashMap<PathBuf, Fade>,
    pub menu: Fade,
    pub menu_ghost: Option<Menu>,
    pub toast: Fade,
    pub toast_ghost: Option<Notice>,
    pub modal: Fade,
    pub modal_ghost: Option<Modal>,
    pub thumb: Fade,
}

impl Surfaces {
    fn new(view: ViewMode) -> Self {
        Self {
            hover: HashMap::new(),
            side: HashMap::new(),
            menu: Fade::asymmetric(false, (Speed::T2, Curve::Out), (Speed::T2, Curve::InOut)),
            menu_ghost: None,
            toast: Fade::asymmetric(false, (Speed::T4, Curve::Out), (Speed::T2, Curve::InOut)),
            toast_ghost: None,
            modal: Fade::asymmetric(false, (Speed::T3, Curve::Out), (Speed::T2, Curve::InOut)),
            modal_ghost: None,
            thumb: Fade::new(view == ViewMode::List, Speed::T2, Curve::Out),
        }
    }

    #[must_use]
    pub fn hover(&self, key: &Hover, now: Instant) -> f32 {
        self.hover.get(key).map_or(0.0, |(layer, _)| layer.progress(now))
    }

    #[must_use]
    pub fn lift(&self, key: &Hover, now: Instant) -> f32 {
        self.hover.get(key).map_or(0.0, |(_, lift)| lift.progress(now))
    }

    fn set_hover(&mut self, key: Hover, on: bool, now: Instant) {
        if on {
            for (other, (layer, lift)) in &mut self.hover {
                if other != &key {
                    layer.set(false, now);
                    lift.set(false, now);
                }
            }
        }
        let (layer, lift) = match self.hover.get_mut(&key) {
            Some(fades) => fades,
            None if on => self.hover.entry(key).or_insert_with(|| {
                (
                    Fade::new(false, Speed::T1, Curve::InOut),
                    Fade::asymmetric(false, (Speed::T2, Curve::Over), (Speed::T2, Curve::Out)),
                )
            }),
            None => return,
        };
        layer.set(on, now);
        lift.set(on, now);
    }

    fn clear_hover(&mut self, now: Instant) {
        for (layer, lift) in self.hover.values_mut() {
            layer.set(false, now);
            lift.set(false, now);
        }
    }

    #[must_use]
    pub fn indicator(&self, path: &Path, now: Instant) -> f32 {
        self.side.get(path).map_or(0.0, |fade| fade.progress(now))
    }

    fn set_active(&mut self, active: Option<&Path>, now: Instant) {
        for (path, fade) in &mut self.side {
            fade.set(Some(path.as_path()) == active, now);
        }
        if let Some(path) = active {
            self.side
                .entry(path.to_path_buf())
                .or_insert_with(|| Fade::new(false, Speed::T3, Curve::Over))
                .set(true, now);
        }
    }

    #[must_use]
    pub fn is_animating(&self, now: Instant) -> bool {
        self.hover.values().any(|(layer, lift)| layer.is_animating(now) || lift.is_animating(now))
            || self.side.values().any(|fade| fade.is_animating(now))
            || [&self.menu, &self.toast, &self.modal, &self.thumb].into_iter().any(|fade| fade.is_animating(now))
    }

    fn prune(&mut self, now: Instant) {
        self.hover.retain(|_, (layer, lift)| !layer.is_settled_off(now) || !lift.is_settled_off(now));
        self.side.retain(|_, fade| !fade.is_settled_off(now));
        if self.menu.is_settled_off(now) {
            self.menu_ghost = None;
        }
        if self.toast.is_settled_off(now) {
            self.toast_ghost = None;
        }
        if self.modal.is_settled_off(now) {
            self.modal_ghost = None;
        }
    }
}

pub struct App {
    pub theme: ThemeState,
    pub icons: IconTheme,
    pub lang: Lang,
    pub current: PathBuf,
    history: Vec<PathBuf>,
    history_index: usize,
    pub entries: Vec<FileEntry>,
    pub selection: HashSet<PathBuf>,
    anchor: Option<PathBuf>,
    cursor: Option<PathBuf>,
    last_click: Option<(PathBuf, Instant)>,
    pub clipboard: Option<FileClipboard>,
    pub view: ViewMode,
    pub sort: SortConfig,
    pub show_hidden: bool,
    pub search: Option<Search>,
    search_generation: Arc<AtomicU64>,
    pub path_edit: Option<String>,
    pub places: Vec<Place>,
    pub bookmarks: Vec<Place>,
    pub drives: Vec<Place>,
    pub volumes: Vec<Volume>,
    pub busy: HashSet<String>,
    disks: Arc<Disks>,
    disk_watch: DiskWatch,
    pub trash: Trash,
    pub trash_count: usize,
    pub extensions: Arc<Extensions>,
    roots: Roots,
    extensions_stamp: Vec<Option<SystemTime>>,
    pub free_bytes: Option<u64>,
    pub modal: Option<Modal>,
    pub menu: Option<Menu>,
    menu_target: Option<MenuKind>,
    pub notice: Option<Notice>,
    notices: u64,
    pub motion: Surfaces,
    modifiers: Modifiers,
    pub pane: Size,
    nav_generation: u64,
    navigating: bool,
    config_stamp: Option<SystemTime>,
    handles: Option<dnd::Handles>,
    dnd: Option<dnd::Link>,
    drag_arm: Option<DragArm>,
    scroll_y: f32,
    drop_target: Option<PathBuf>,
    pub jobs: Vec<Job>,
    next_job: u64,
    quit_requested: Option<Instant>,
    last_undo: Option<Undo>,
    thumbs: HashMap<ThumbKey, Option<PathBuf>>,
    thumbs_asked: HashSet<ThumbKey>,
    thumb_generation: Arc<AtomicU64>,
    band: Option<Band>,
    pub drag: Option<Drag>,
    hover_target: Option<Hover>,
    window: WindowState,
    pending_select: Option<PathBuf>,
    grid_zoom: u8,
    list_zoom: u8,
    pub columns: Vec<ListColumn>,
    pub queue: bool,
    pub renaming: Option<Renaming>,
    column_drag: Option<ListColumn>,
    bookmark_drag: Option<PathBuf>,
    menu_at: Point,
    pub templates: Vec<PathBuf>,
    pub phones: Vec<crate::fs::mtp::Phone>,
    look: Look,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Renaming {
    pub path: PathBuf,
    pub text: String,
}

pub struct Launch(Box<dyn FnOnce() -> bool + Send>);

impl std::fmt::Debug for Launch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Launch")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropAction {
    Move,
    Copy,
    Trash,
    Bookmark,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Drag {
    pub paths: Vec<PathBuf>,
    pub at: Point,
    pub action: DropAction,
    folders: bool,
    dwell: Option<(PathBuf, Instant)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Band {
    start: Option<Point>,
    end: Point,
    base: HashSet<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
struct WindowState {
    size: Size,
    focused: bool,
    maximized: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct DiskWatch {
    scanning: bool,
    live: bool,
    phones: bool,
}

fn blocking<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> impl Future<Output = Option<T>> {
    let (sender, receiver) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(job());
    });
    async move { receiver.await.ok() }
}

fn run(job: impl FnOnce() -> Outcome + Send + 'static) -> Task<Message> {
    Task::perform(blocking(job), |outcome| Message::Done(outcome.unwrap_or_default()))
}

fn minute_now() -> Option<u16> {
    crate::fs::LocalTime::of(SystemTime::now()).map(|now| u16::from(now.hour) * 60 + u16::from(now.minute))
}

fn expire(id: u64) -> Task<Message> {
    Task::perform(blocking(|| std::thread::sleep(NOTICE_TIMEOUT)), move |_| Message::NoticeExpired(id))
}

fn terminal(dir: PathBuf, lang: Lang) -> Task<Message> {
    run(move || Outcome {
        text: open_in_terminal(&dir).err().map(|_| Key::NoTerminal.text(lang).to_string()),
        ..Outcome::default()
    })
}

fn stamp(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

#[must_use]
pub fn volume_icon(volume: &Volume) -> Icon {
    if volume.removable { Icon::Usb } else { Icon::Drive }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy()).into_owned()
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let look = config::load();
        glide_theme::motion::set_scale(look.motion_scale);
        let lang = look.language.unwrap_or_else(Lang::from_env);
        let trash = Trash::user();
        let prefs = config::load_prefs();
        let requested = std::env::args().nth(1).and_then(|arg| path_from_arg(&arg));
        let fallback = home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let (current, focus) = match requested {
            Some(path) if path.is_dir() => (path, None),
            Some(path) if path.exists() => {
                (path.parent().map_or_else(|| fallback.clone(), Path::to_path_buf), Some(path))
            }
            _ => (fallback, None),
        };
        let mut app = Self {
            theme: ThemeState::new(look.appearance_at(minute_now()), look.accent),
            icons: IconTheme::new(look.icon_theme.as_deref(), look.accent),
            lang,
            current: current.clone(),
            history: vec![current],
            history_index: 0,
            entries: Vec::new(),
            selection: HashSet::new(),
            anchor: None,
            cursor: None,
            last_click: None,
            clipboard: None,
            view: prefs.view,
            sort: prefs.sort,
            show_hidden: prefs.show_hidden,
            search: None,
            search_generation: Arc::new(AtomicU64::new(0)),
            path_edit: None,
            places: standard_places(lang, &trash),
            bookmarks: Vec::new(),
            drives: mounted_drives(lang),
            volumes: Vec::new(),
            busy: HashSet::new(),
            disks: Arc::default(),
            disk_watch: DiskWatch::default(),
            trash_count: trash.count(),
            trash,
            extensions: Arc::default(),
            roots: Roots::system(),
            extensions_stamp: Vec::new(),
            free_bytes: None,
            modal: None,
            menu: None,
            menu_target: None,
            motion: Surfaces::new(prefs.view),
            notice: None,
            notices: 0,
            modifiers: Modifiers::empty(),
            pane: Size::new(760.0, 560.0),
            nav_generation: 0,
            navigating: false,
            config_stamp: config::path().as_deref().and_then(stamp),
            handles: None,
            dnd: None,
            drag_arm: None,
            scroll_y: 0.0,
            drop_target: None,
            jobs: Vec::new(),
            next_job: 0,
            quit_requested: None,
            last_undo: None,
            thumbs: HashMap::new(),
            thumbs_asked: HashSet::new(),
            thumb_generation: Arc::new(AtomicU64::new(0)),
            band: None,
            drag: None,
            hover_target: None,
            window: WindowState { size: Size::ZERO, focused: false, maximized: false },
            pending_select: None,
            grid_zoom: prefs.grid_zoom,
            list_zoom: prefs.list_zoom,
            columns: prefs.columns.clone(),
            queue: prefs.queue,
            renaming: None,
            column_drag: None,
            bookmark_drag: None,
            menu_at: Point::ORIGIN,
            templates: Vec::new(),
            phones: Vec::new(),
            look,
        };
        app.bookmarks = app.fresh_bookmarks();
        let task = app.list(app.current.clone(), Arrival::Initial, focus);
        let extensions = app.load_extensions();
        let scan = app.scan();
        let handles =
            window::latest().and_then(|id| window::run(id, |window| dnd_handles(window))).map(Message::Handles);
        let phones = app.scan_phones();
        (app, Task::batch([task, extensions, scan, handles, phones]))
    }

    #[must_use]
    pub fn title(&self) -> String {
        format!("{} — {}", self.place_title(), Key::AppName.text(self.lang))
    }

    #[must_use]
    pub fn place_title(&self) -> String {
        match &self.search {
            Some(search) if !search.query.trim().is_empty() => {
                format!("{} «{}»", Key::Search.text(self.lang), search.query.trim())
            }
            _ => self.location_name(&self.current),
        }
    }

    #[must_use]
    pub fn location_name(&self, path: &Path) -> String {
        if let Some(volume) = self.volume_at(path) {
            return volume.name(self.lang);
        }
        if self.trash.is_files_dir(path) {
            return Key::Trash.text(self.lang).to_string();
        }
        self.places
            .iter()
            .chain(&self.drives)
            .find(|place| place.path == path)
            .map_or_else(|| file_name(path), |place| place.name.clone())
    }

    fn volume_at(&self, path: &Path) -> Option<&Volume> {
        self.volumes.iter().find(|volume| volume.mount.as_deref() == Some(path))
    }

    #[must_use]
    pub fn volume(&self, object: &str) -> Option<&Volume> {
        self.volumes.iter().find(|volume| volume.object == object)
    }

    #[must_use]
    pub fn devices(&self) -> Vec<Device<'_>> {
        let (root, mounts) = self.drives.split_at(self.drives.len().min(1));
        root.iter()
            .map(Device::Place)
            .chain(self.volumes.iter().map(Device::Volume))
            .chain(mounts.iter().filter(|place| self.volume_at(&place.path).is_none()).map(Device::Place))
            .collect()
    }

    #[must_use]
    pub fn art(&self, entry: &FileEntry) -> Option<Art> {
        let place = entry
            .is_dir
            .then(|| self.places.iter().find(|place| place.path == entry.path))
            .flatten()
            .map(|place| place.icon);
        let db = &self.extensions.mime;
        let mime =
            (!entry.is_dir).then(|| db.of(&entry.path, false)).filter(|mime| mime != crate::fs::mime::OCTET_STREAM);
        let by_mime = mime.map(|mime| db.icon_names(&mime)).unwrap_or_default();
        self.icons.lookup(&self.icons.names(entry, place, &by_mime))
    }

    fn fresh_bookmarks(&self) -> Vec<Place> {
        load_bookmarks().into_iter().filter(|mark| !self.places.iter().any(|place| place.path == mark.path)).collect()
    }

    #[must_use]
    pub fn style(&self, _theme: &iced::Theme) -> iced::theme::Style {
        iced::theme::Style { background_color: iced::Color::TRANSPARENT, text_color: self.theme.fg_1() }
    }

    #[must_use]
    pub fn iced_theme(&self) -> iced::Theme {
        self.theme.iced_theme()
    }

    #[must_use]
    pub fn in_trash(&self) -> bool {
        self.trash.contains(&self.current)
    }

    #[must_use]
    pub fn is_searching(&self) -> bool {
        self.search.as_ref().is_some_and(|search| !search.query.trim().is_empty())
    }

    #[must_use]
    pub fn is_cut(&self, path: &Path) -> bool {
        self.clipboard
            .as_ref()
            .is_some_and(|clip| clip.action == ClipboardAction::Cut && clip.paths.iter().any(|cut| cut == path))
    }

    #[must_use]
    pub fn selected(&self) -> Vec<PathBuf> {
        self.entries
            .iter()
            .filter(|entry| self.selection.contains(&entry.path))
            .map(|entry| entry.path.clone())
            .collect()
    }

    pub fn selected_entries(&self) -> impl Iterator<Item = &FileEntry> {
        self.entries.iter().filter(|entry| self.selection.contains(&entry.path))
    }

    fn primary(&self) -> Option<PathBuf> {
        self.cursor
            .clone()
            .filter(|cursor| self.selection.contains(cursor))
            .or_else(|| self.selected().into_iter().next())
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions =
            vec![keyboard::listen().filter_map(handle_shortcut), iced::time::every(POLL).map(|_| Message::Tick)];
        let sweeping =
            self.jobs.iter().any(|job| job.progress.total_items == 0 && job.launch.is_none() && !job.meter.is_paused());
        if sweeping || self.motion.is_animating(Instant::now()) {
            subscriptions.push(window::frames().map(Message::Frame));
        }
        subscriptions.push(Subscription::run(crate::service::requests).map(Message::Service));
        subscriptions.push(Subscription::run(crate::fs::volumes::changes).map(|()| Message::VolumesChanged));
        subscriptions.push(
            Subscription::run_with(self.current.clone(), crate::fs::watch::changes).map(|()| Message::DirChanged),
        );
        if let Some(handles) = self.handles {
            subscriptions.push(Subscription::run_with(handles, dnd::events).map(Message::Dnd));
        }
        subscriptions.push(iced::event::listen_with(window_events));
        subscriptions.push(window::close_requests().map(|_| Message::Quit));
        if self.drag_arm.is_some() || self.band.is_some() || self.drag.is_some() {
            subscriptions.push(iced::event::listen_with(drag_motion));
        }
        if self.drag.is_some() {
            subscriptions.push(iced::time::every(DRAG_TICK).map(|_| Message::DragTick));
        }
        Subscription::batch(subscriptions)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let ends_rename = matches!(
            message,
            Message::Pressed(_)
                | Message::PressedBackground
                | Message::Navigate(_)
                | Message::GoHome
                | Message::Back
                | Message::Forward
                | Message::Up
                | Message::SetView(_)
                | Message::Move(..)
                | Message::ContextMenu(_)
                | Message::KeyMenu
                | Message::ToggleOptions
                | Message::NoticeChosen(_)
        );
        if self.renaming.is_some() && ends_rename {
            let committed = self.commit_rename();
            if self.renaming.is_some() {
                return committed;
            }
            return Task::batch([committed, self.update(message)]);
        }
        let task = self.handle(message);
        self.surface_question();
        self.sync_motion(Instant::now());
        task
    }

    fn sync_motion(&mut self, now: Instant) {
        if self.notice.is_none() && self.hover_target == Some(Hover::Toast) {
            self.hover_target = None;
        }
        let active = self.sidebar_active().map(Path::to_path_buf);
        let motion = &mut self.motion;
        motion.set_active(active.as_deref(), now);
        motion.menu.set(self.menu.is_some(), now);
        if self.menu.is_some() {
            motion.menu_ghost.clone_from(&self.menu);
        }
        motion.toast.set(self.notice.is_some(), now);
        if self.notice.is_some() {
            motion.toast_ghost.clone_from(&self.notice);
        }
        motion.modal.set(self.modal.is_some(), now);
        if self.modal.is_some() {
            motion.modal_ghost.clone_from(&self.modal);
        }
        motion.thumb.set(self.view == ViewMode::List, now);
        motion.prune(now);
    }

    #[must_use]
    pub fn hover(&self, key: &Hover) -> f32 {
        self.motion.hover(key, Instant::now())
    }

    #[must_use]
    pub fn sidebar_active(&self) -> Option<&Path> {
        if self.search.is_some() {
            return None;
        }
        let phone =
            self.phones.iter().filter_map(|phone| phone.mount.as_deref()).find(|mount| self.current.starts_with(mount));
        Some(phone.unwrap_or(&self.current))
    }

    #[must_use]
    pub fn indicator(&self, path: &Path) -> f32 {
        self.motion.indicator(path, Instant::now())
    }

    #[must_use]
    pub fn lift(&self, key: &Hover) -> f32 {
        self.motion.lift(key, Instant::now())
    }

    #[allow(clippy::too_many_lines)] // One arm per message; each arm delegates to a helper
    fn handle(&mut self, message: Message) -> Task<Message> {
        if self.modal.is_some() && !message.passes_modal() {
            return Task::none();
        }
        if message.dismisses_menu() {
            self.menu = None;
        }
        match message {
            Message::Undo
            | Message::Duplicate
            | Message::Delete
            | Message::Trash
            | Message::Properties
            | Message::NewFolder
                if self.renaming.is_some() || self.path_edit.is_some() => {}
            Message::Move(step, _) if self.menu.is_some() => return self.step_menu(step),
            Message::OpenSelection if self.menu.is_some() => return self.activate_menu(),
            Message::KeyMenu => return self.key_menu(),
            Message::Navigate(path) => return self.navigate(path),
            Message::GoHome => return home_dir().map_or_else(Task::none, |home| self.navigate(home)),
            Message::Reveal(path) => {
                let Some(parent) = path.parent().map(Path::to_path_buf) else {
                    return Task::none();
                };
                let arrival = if parent == self.current { Arrival::Jump(self.history_index) } else { Arrival::Push };
                return self.list(parent, arrival, Some(path));
            }
            Message::Back if self.history_index > 0 => {
                return self.jump(self.history_index - 1);
            }
            Message::Forward if self.history_index + 1 < self.history.len() => {
                return self.jump(self.history_index + 1);
            }
            Message::Up => {
                if let Some(parent) = self.current.parent().filter(|_| !self.trash.is_files_dir(&self.current)) {
                    let child = self.current.clone();
                    return self.navigate_select(parent.to_path_buf(), Some(child));
                }
            }
            Message::Reload | Message::DirChanged => return self.reload(),
            Message::Listed(listed) => return self.arrive(*listed),
            Message::Pressed(path) => return self.press(path),
            Message::Secondary(path) => {
                if !self.selection.contains(&path) {
                    self.select_only(path);
                }
                self.menu_target = Some(MenuKind::Selection);
            }
            Message::MenuTarget(kind) => {
                self.menu_target.get_or_insert(kind);
            }
            Message::PressedBackground => {
                self.menu = None;
                self.path_edit = None;
                if !self.modifiers.command() && !self.modifiers.shift() {
                    self.selection.clear();
                }
                self.band = Some(Band { start: None, end: Point::ORIGIN, base: self.selection.clone() });
            }
            Message::ContextMenu(at) => {
                if let Some(kind) = self.menu_target.take() {
                    if kind == MenuKind::Background {
                        self.selection.clear();
                    }
                    let (offers, open_with) = if kind == MenuKind::Selection && !self.in_trash() {
                        let targets = self.targets();
                        (self.extensions.offers(&targets), !self.open_with_offers(&targets).is_empty())
                    } else {
                        (Vec::new(), false)
                    };
                    self.menu_at = at;
                    self.menu = Some(Menu { kind, at, offers, open_with, highlight: None });
                }
            }
            Message::RunAction(offer) => return self.run_action(*offer),
            Message::ExtensionsLoaded(extensions) => {
                self.extensions = extensions;
                return self.request_thumbs();
            }
            Message::Thumb(key, thumb) => {
                self.thumbs.insert(key, thumb);
            }
            Message::FolderSize(path, total) => {
                if let Some(properties) = self.properties_mut().filter(|properties| properties.entry.path == path) {
                    properties.total = Some(total);
                }
            }
            Message::Permission(bit) => self.toggle_permission(bit),
            Message::VolumesChanged => {
                self.disk_watch.live = true;
                self.drives = mounted_drives(self.lang);
                return self.scan();
            }
            Message::Hover(key, on) => {
                if on {
                    self.hover_target = Some(key.clone());
                } else if self.hover_target.as_ref() == Some(&key) {
                    self.hover_target = None;
                }
                self.motion.set_hover(key, on, Instant::now());
            }
            Message::TerminalAt(dir) => return terminal(dir, self.lang),
            Message::PropertiesOf(path) => return self.properties_of(path),
            Message::CopyText(text) => return iced::clipboard::write(text),
            Message::AddBookmark(path) => {
                return self.edit_bookmarks(|lines| {
                    lines.push(file_uri(&path));
                });
            }
            Message::RemoveBookmark(path) => {
                return self.edit_bookmarks(|lines| {
                    lines.retain(|line| {
                        let uri = line.split(' ').next().unwrap_or_default();
                        path_from_arg(uri).as_deref() != Some(path.as_path())
                    });
                });
            }
            Message::ToggleOptions => {
                self.menu_at = widgets::menu::options_at(self.window.size);
                self.menu = match &self.menu {
                    Some(Menu { kind: MenuKind::Options, .. }) => None,
                    _ => Some(Menu {
                        kind: MenuKind::Options,
                        at: self.menu_at,
                        offers: Vec::new(),
                        open_with: false,
                        highlight: None,
                    }),
                };
            }
            Message::CloseMenu => self.menu = None,
            Message::MenuHover(index, inside) => {
                let enabled = self.menu.as_ref().is_some_and(|menu| {
                    widgets::menu::entries(self, menu)
                        .get(index)
                        .is_some_and(|entry| entry.clone().into_message().is_some())
                });
                if let Some(menu) = &mut self.menu {
                    if inside && enabled {
                        menu.highlight = Some(index);
                    } else if !inside && menu.highlight == Some(index) {
                        menu.highlight = None;
                    }
                }
            }
            Message::Menu(message) => {
                self.menu = None;
                self.path_edit = None;
                return self.handle(*message);
            }
            Message::NoticeChosen(action) => {
                self.path_edit = None;
                return self.handle(match action {
                    NoticeAction::Undo => Message::Undo,
                    NoticeAction::Repair(object) => Message::Repair(object),
                });
            }
            Message::OpenSelection => return self.open_selection(),
            Message::SelectAll => {
                self.selection = self.entries.iter().map(|e| e.path.clone()).collect();
                self.anchor = self.entries.first().map(|entry| entry.path.clone());
                self.cursor = self.entries.last().map(|entry| entry.path.clone());
            }
            Message::Move(step, extend) => return self.step(step, extend),
            Message::SetView(view) => {
                self.view = view;
                self.save_prefs();
            }
            Message::SortBy(by) => {
                self.sort = SortConfig { by, ascending: self.sort.by != by || !self.sort.ascending };
                sort_entries(&mut self.entries, self.sort);
                self.save_prefs();
            }
            Message::SortKey(by) => {
                self.sort.by = by;
                sort_entries(&mut self.entries, self.sort);
                self.save_prefs();
            }
            Message::ToggleReverse => {
                self.sort.ascending = !self.sort.ascending;
                sort_entries(&mut self.entries, self.sort);
                self.save_prefs();
            }
            Message::ToggleHidden => {
                self.show_hidden = !self.show_hidden;
                self.save_prefs();
                return self.refresh_listing();
            }
            Message::ToggleSearch => {
                return Task::batch([operation::focus(SEARCH_ID), operation::select_all(SEARCH_ID)]);
            }
            Message::TypeToSearch(text)
                if self.modal.is_none()
                    && self.path_edit.is_none()
                    && (self.search.is_some() || !text.trim().is_empty()) =>
            {
                let query = self.search.as_ref().map_or(text.clone(), |search| format!("{}{text}", search.query));
                return self.open_search(query);
            }
            Message::SearchChanged(query) => return self.set_query(query),
            Message::SearchResults(generation, mut found)
                if generation == self.search_generation.load(Ordering::Relaxed) =>
            {
                sort_entries(&mut found, self.sort);
                self.entries = found;
                self.selection.clear();
                self.forget_vanished_rename();
                if let Some(search) = &mut self.search {
                    search.pending = false;
                }
            }
            Message::EditPath => {
                self.menu = None;
                self.path_edit = Some(self.current.display().to_string());
                return Task::batch([operation::focus(PATH_ID), operation::select_all(PATH_ID)]);
            }
            Message::PathChanged(text) => self.path_edit = Some(text),
            Message::PathSubmitted => return self.submit_path(),
            Message::Cut => self.fill_clipboard(ClipboardAction::Cut),
            Message::Copy => self.fill_clipboard(ClipboardAction::Copy),
            Message::Paste => return self.paste(),
            Message::NewFolder | Message::NewFile | Message::NewFromTemplate(_) if self.in_trash() => {}
            Message::NewFolder => {
                let name = unique_path(
                    &self.current,
                    Key::NewFolder.text(self.lang).as_ref(),
                    false,
                    crate::fs::operations::occupied,
                );
                return self.prompt(Dialog::NewFolder, file_name(&name), None);
            }
            Message::NewFile => {
                let name = unique_path(
                    &self.current,
                    Key::UntitledFile.text(self.lang).as_ref(),
                    true,
                    crate::fs::operations::occupied,
                );
                return self.prompt(Dialog::NewFile, file_name(&name), None);
            }
            Message::Rename => return self.start_rename(),
            Message::RenameInput(text) => {
                if let Some(renaming) = &mut self.renaming {
                    renaming.text = text;
                }
            }
            Message::RenameSubmit => return self.commit_rename(),
            Message::Duplicate if !self.in_trash() => {
                let mut by_parent: Vec<(PathBuf, Vec<PathBuf>)> = Vec::new();
                for path in self.selected() {
                    let parent = path.parent().map_or_else(|| self.current.clone(), Path::to_path_buf);
                    match by_parent.iter_mut().find(|(dir, _)| *dir == parent) {
                        Some((_, paths)) => paths.push(path),
                        None => by_parent.push((parent, vec![path])),
                    }
                }
                let jobs: Vec<Task<Message>> =
                    by_parent.into_iter().map(|(dir, paths)| self.spawn_job(Op::Copy, paths, dir, false)).collect();
                return Task::batch(jobs);
            }
            Message::CreateLink if !self.in_trash() => {
                let (paths, dir, prefix) = (self.selected(), self.current.clone(), Key::LinkTo.text(self.lang));
                let lang = self.lang;
                return run(move || {
                    let mut outcome = Outcome::default();
                    for path in &paths {
                        match crate::fs::create_link(path, &dir, prefix) {
                            Ok(link) => outcome.select.push(link),
                            Err(err) => outcome.text = Some(i18n::io_error(&err, lang)),
                        }
                    }
                    outcome
                });
            }
            Message::SubMenu(kind) => {
                if kind == MenuKind::Templates {
                    self.templates = templates_dir().map(|dir| templates_in(&dir)).unwrap_or_default();
                }
                let offers =
                    if kind == MenuKind::OpenWith { self.open_with_offers(&self.targets()) } else { Vec::new() };
                self.menu = Some(Menu { kind, at: self.menu_at, offers, open_with: false, highlight: None });
            }
            Message::NewFromTemplate(template) => {
                let name = file_name(&template);
                let stem = Path::new(&name).file_stem().map(|stem| stem.to_string_lossy().chars().count());
                return self.prompt(Dialog::NewFromTemplate(template), name, stem);
            }
            Message::OpenTemplates => {
                if let Some(dir) = templates_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    return self.navigate(dir);
                }
            }
            Message::ToggleTag(color) => return self.toggle_tag(color),
            Message::EditTags => {
                let paths = self.selected();
                let words =
                    self.selected_entries().next().map(|entry| tags::words(&entry.tags).join(", ")).unwrap_or_default();
                if !paths.is_empty() {
                    return self.prompt(Dialog::Tags(paths), words, None);
                }
            }
            Message::Zoom(step) => self.zoom(step),
            Message::ToggleColumn(column) => {
                if self.columns.contains(&column) {
                    self.columns.retain(|shown| *shown != column);
                } else {
                    self.columns.push(column);
                }
                self.save_prefs();
            }
            Message::ColumnPressed(column) => self.column_drag = Some(column),
            Message::BookmarkPressed(path) => self.bookmark_drag = Some(path),
            Message::ToggleQueue => {
                self.queue = !self.queue;
                self.save_prefs();
                if !self.queue {
                    for job in &mut self.jobs {
                        if let Some(launch) = job.launch.take() {
                            (launch.0)();
                        }
                    }
                }
            }
            Message::Phones(phones) => {
                self.disk_watch.phones = false;
                self.phones = phones;
            }
            Message::OpenPhone(id) => {
                let Some(phone) = self.phones.iter().find(|phone| phone.id == id).cloned() else {
                    return Task::none();
                };
                if let Some(mount) = phone.mount.clone() {
                    return self.navigate(mount);
                }
                self.busy.insert(phone.id.clone());
                let lang = self.lang;
                return Task::perform(
                    blocking(move || {
                        crate::fs::mtp::open(&phone).map_err(|err| match err {
                            crate::fs::mtp::OpenError::NoTool => Key::NoMtpTool.text(lang).to_string(),
                            crate::fs::mtp::OpenError::Failed(detail) => {
                                format!("{}: {detail}. {}", Key::CantOpenPhone.text(lang), Key::PhoneHint.text(lang))
                            }
                        })
                    }),
                    |result| Message::PhoneOpened(result.unwrap_or_else(|| Err(String::new()))),
                );
            }
            Message::PhoneOpened(result) => {
                self.busy.retain(|busy| !busy.contains(':'));
                let rescan = self.scan_phones();
                return match result {
                    Ok(path) => Task::batch([rescan, self.navigate(path)]),
                    Err(text) if text.is_empty() => rescan,
                    Err(text) => Task::batch([rescan, self.notify(text, None)]),
                };
            }
            Message::ClosePhone(id) => {
                let Some(phone) = self.phones.iter().find(|phone| phone.id == id).cloned() else {
                    return Task::none();
                };
                let leave = match (&phone.mount, home_dir()) {
                    (Some(mount), Some(home)) if self.current.starts_with(mount) => self.navigate(home),
                    _ => Task::none(),
                };
                let lang = self.lang;
                let closed = run(move || Outcome {
                    text: crate::fs::mtp::close(&phone)
                        .err()
                        .map(|err| format!("{} {err}", Key::CantUnmount.text(lang))),
                    ..Outcome::default()
                });
                return Task::batch([leave, closed, self.scan_phones()]);
            }
            Message::PasteText(text) => {
                if let Some(text) = text.filter(|text| !text.is_empty()) {
                    let name = format!("{}.txt", Key::PastedText.text(self.lang));
                    return self.save_pasted(name, text.into_bytes(), self.current.clone());
                }
            }
            Message::Trash => return self.trash_selection(),
            Message::Delete => {
                let paths = self.selected();
                if !paths.is_empty() {
                    return self.prompt(Dialog::Delete(paths), String::new(), None);
                }
            }
            Message::EmptyTrash if self.trash_count > 0 => {
                return self.prompt(Dialog::EmptyTrash, String::new(), None);
            }
            Message::Restore => return self.restore(self.selected()),
            Message::Properties => return self.properties(),
            Message::Terminal => {
                let dir = self
                    .primary()
                    .filter(|path| self.selection.len() == 1 && path.is_dir())
                    .unwrap_or_else(|| self.current.clone());
                return terminal(dir, self.lang);
            }
            Message::ModalInput(text) => {
                if let Some(modal) = &mut self.modal {
                    modal.input = text;
                    modal.error = None;
                }
            }
            Message::ModalSubmit => return self.submit_modal(),
            Message::ModalCancel => self.dismiss_modal(),
            Message::Done(outcome) => return self.finish(outcome),
            Message::DismissNotice(id) => {
                if self.notice.as_ref().is_some_and(|notice| notice.id == id) {
                    self.notice = None;
                }
            }
            Message::NoticeExpired(id) if self.notice.as_ref().is_some_and(|notice| notice.id == id) => {
                if self.hover_target == Some(Hover::Toast) {
                    return expire(id);
                }
                self.notice = None;
            }
            Message::Undo => {
                if self.notice.as_ref().is_some_and(|notice| notice.action == Some(NoticeAction::Undo)) {
                    self.notice = None;
                }
                if let Some(undo) = self.last_undo.take() {
                    return self.undo(undo);
                }
            }
            Message::Job(id, event) => return self.on_job(id, event),
            Message::JobCancel(id) => {
                if let Some(index) = self.jobs.iter().position(|job| job.id == id && job.launch.is_some()) {
                    self.jobs.remove(index);
                    self.sync_dock();
                } else if let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) {
                    job.stop();
                }
            }
            Message::Resolve(choice) => self.resolve(choice),
            Message::Service(request) => return self.serve(request),
            Message::ToggleForAll => {
                if let Some(Modal { dialog: Dialog::Conflict { for_all, .. }, .. }) = &mut self.modal {
                    *for_all = !*for_all;
                }
            }
            Message::Volumes(volumes) => {
                self.disk_watch.scanning = false;
                if volumes != self.volumes {
                    self.volumes = volumes;
                    self.drives = mounted_drives(self.lang);
                }
            }
            Message::Mount(object, read_only) => return self.mount(&object, read_only),
            Message::Unmount(object) => return self.disk_job(&object, DiskAction::Unmount),
            Message::Eject(object) => return self.disk_job(&object, DiskAction::Eject),
            Message::Repair(object) => {
                self.notice = None;
                if self.volume(&object).is_some() {
                    return self.prompt(Dialog::Repair(object), String::new(), None);
                }
            }
            Message::Disk(outcome) => return self.disk_done(*outcome),
            Message::Modifiers(modifiers) => self.modifiers = modifiers,
            Message::PaneResized(size) => self.pane = size,
            Message::Tick => return self.poll(),
            Message::Escape => return self.escape(),
            Message::Quit if self.jobs.is_empty() => return iced::exit(),
            Message::Quit if self.quit_requested.is_none() => return self.prompt(Dialog::Quit, String::new(), None),
            Message::WindowDrag => {
                self.menu = None;
                return window::latest().and_then(window::drag);
            }
            Message::WindowMaximize => return window::latest().and_then(window::toggle_maximize),
            Message::WindowResize(edge) => {
                return window::latest().and_then(move |id| window::drag_resize(id, edge.into()));
            }
            Message::Handles(handles) => self.handles = handles,
            Message::Dnd(event) => return self.on_dnd(event),
            Message::Scrolled(offset) => self.scroll_y = offset,
            Message::Cursor(at) => {
                self.drag_motion(at);
                self.band_motion(at);
            }
            Message::Released => {
                self.drag_arm = None;
                self.band = None;
                if let Some(path) = self.pending_select.take().filter(|_| self.drag.is_none()) {
                    self.select_only(path);
                }
                if let Some(column) = self.column_drag.take() {
                    return self.column_released(column);
                }
                if let Some(path) = self.bookmark_drag.take() {
                    return self.bookmark_released(path);
                }
                return self.release();
            }
            Message::DragTick => return self.drag_tick(),
            Message::WindowSize(size) => {
                self.window.size = size;
                return window::latest().and_then(window::is_maximized).map(Message::WindowMaximized);
            }
            Message::WindowMaximized(maximized) => self.window.maximized = maximized,
            Message::WindowFocus(focused) => self.window.focused = focused,
            Message::JobPause(id) => {
                if let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) {
                    let paused = !job.pause.load(Ordering::Relaxed);
                    job.pause.store(paused, Ordering::Relaxed);
                    job.meter.pause(paused);
                }
            }
            Message::Back
            | Message::Forward
            | Message::EmptyTrash
            | Message::Duplicate
            | Message::CreateLink
            | Message::TypeToSearch(_)
            | Message::SearchResults(..)
            | Message::NoticeExpired(_)
            | Message::Quit
            | Message::Frame(_) => {}
        }
        Task::none()
    }

    #[must_use]
    pub fn view(&self) -> Element<'_, Message> {
        widgets::window(self)
    }

    #[must_use]
    pub const fn is_quitting(&self) -> bool {
        self.quit_requested.is_some()
    }

    #[must_use]
    pub const fn is_idle(&self) -> bool {
        !self.window.focused
    }

    #[must_use]
    pub const fn is_maximized(&self) -> bool {
        self.window.maximized
    }

    #[must_use]
    pub const fn corner(&self) -> f32 {
        if self.window.maximized { 0.0 } else { crate::theme::R3 }
    }

    #[must_use]
    pub fn can_go_back(&self) -> bool {
        self.history_index > 0
    }

    #[must_use]
    pub fn can_go_forward(&self) -> bool {
        self.history_index + 1 < self.history.len()
    }

    fn list(&mut self, path: PathBuf, arrival: Arrival, select: Option<PathBuf>) -> Task<Message> {
        if arrival != Arrival::Refresh {
            self.nav_generation += 1;
            self.navigating = true;
        }
        let generation = self.nav_generation;
        let (hidden, sort, lang) = (self.show_hidden, self.sort, self.lang);
        let dir = path.clone();
        let elsewhere: Vec<PathBuf> = if path == self.trash.files() {
            self.volume_trashes().iter().map(Trash::files).collect()
        } else {
            Vec::new()
        };
        Task::perform(
            blocking(move || {
                read_directory(&dir, hidden, sort)
                    .map(|mut entries| {
                        if !elsewhere.is_empty() {
                            for files in &elsewhere {
                                entries.extend(read_directory(files, hidden, sort).unwrap_or_default());
                            }
                            sort_entries(&mut entries, sort);
                        }
                        (entries, query_disk_space(&dir).map(|(free, _)| free))
                    })
                    .map_err(|err| i18n::io_error(&err, lang))
            }),
            move |result| {
                Message::Listed(Box::new(Listed {
                    generation,
                    path,
                    arrival,
                    select,
                    result: result.unwrap_or_else(|| Err(String::new())),
                }))
            },
        )
    }

    fn arrive(&mut self, listed: Listed) -> Task<Message> {
        if listed.generation != self.nav_generation {
            return Task::none();
        }
        let refresh = listed.arrival == Arrival::Refresh;
        if refresh && (self.navigating || listed.path != self.current || self.is_searching()) {
            return Task::none();
        }
        if !refresh {
            self.navigating = false;
        }
        let (entries, free) = match listed.result {
            Ok(found) => found,
            Err(_) if refresh => return Task::none(),
            Err(text) => {
                if let Arrival::Jump(index) = listed.arrival {
                    self.history.remove(index);
                    if index <= self.history_index {
                        self.history_index = self.history_index.saturating_sub(1);
                    }
                }
                return self.notify(text, None);
            }
        };
        if refresh {
            self.selection.retain(|selected| entries.iter().any(|entry| &entry.path == selected));
        } else {
            self.enter(listed.path.clone());
        }
        match listed.arrival {
            Arrival::Push => {
                self.history.truncate(self.history_index + 1);
                self.history.push(listed.path);
                self.history_index = self.history.len() - 1;
            }
            Arrival::Jump(index) => self.history_index = index,
            Arrival::Initial | Arrival::Refresh => {}
        }
        self.entries = entries;
        self.free_bytes = free;
        self.forget_vanished_rename();
        if !refresh {
            let current = &self.current;
            self.thumbs.retain(|(path, _), _| path.parent() == Some(current.as_path()));
            self.thumbs_asked.clear();
            self.thumb_generation.fetch_add(1, Ordering::Relaxed);
        }
        let thumbs = self.request_thumbs();
        match listed.select {
            Some(path) => {
                self.select_only(path);
                Task::batch([thumbs, self.reveal_cursor()])
            }
            None => thumbs,
        }
    }

    fn forget_vanished_rename(&mut self) {
        if self.renaming.as_ref().is_some_and(|renaming| self.index_of(&renaming.path).is_none()) {
            self.renaming = None;
        }
    }

    fn volume_trashes(&self) -> Vec<Trash> {
        let mounts: Vec<PathBuf> = self
            .drives
            .iter()
            .map(|place| place.path.clone())
            .chain(self.volumes.iter().filter_map(|volume| volume.mount.clone()))
            .collect();
        self.trash.on_volumes(&mounts)
    }

    fn count_trash(&self) -> usize {
        self.trash.count() + self.volume_trashes().iter().map(Trash::count).sum::<usize>()
    }

    fn reload(&mut self) -> Task<Message> {
        self.trash_count = self.count_trash();
        if self.is_searching() {
            return Task::none();
        }
        self.list(self.current.clone(), Arrival::Refresh, None)
    }

    fn show(&mut self, path: PathBuf) -> Task<Message> {
        if self.is_searching() {
            return self.refresh_listing();
        }
        self.list(self.current.clone(), Arrival::Refresh, Some(path))
    }

    fn refresh_listing(&mut self) -> Task<Message> {
        match self.search.as_ref().map(|search| search.query.clone()) {
            Some(query) if !query.trim().is_empty() => self.set_query(query),
            _ => self.reload(),
        }
    }

    fn navigate(&mut self, path: PathBuf) -> Task<Message> {
        self.navigate_select(path, None)
    }

    fn navigate_select(&mut self, path: PathBuf, select: Option<PathBuf>) -> Task<Message> {
        if !path.is_dir() {
            return self.open_files(vec![path]);
        }
        if path == self.current && self.search.is_none() {
            return Task::none();
        }
        self.list(path, Arrival::Push, select)
    }

    fn jump(&mut self, index: usize) -> Task<Message> {
        let target = self.history[index].clone();
        let select = Some(self.current.clone()).filter(|previous| previous.parent() == Some(target.as_path()));
        self.list(target, Arrival::Jump(index), select)
    }

    fn enter(&mut self, path: PathBuf) {
        self.selection.clear();
        self.current = path;
        self.anchor = None;
        self.cursor = None;
        self.last_click = None;
        self.search = None;
        self.search_generation.fetch_add(1, Ordering::Relaxed);
        self.path_edit = None;
        self.renaming = None;
        self.menu = None;
        if !self.in_conflict() {
            self.modal = None;
        }
        self.scroll_y = 0.0;
        self.motion.clear_hover(Instant::now());
    }

    fn select_only(&mut self, path: PathBuf) {
        self.selection.clear();
        self.selection.insert(path.clone());
        self.place_cursor(path);
    }

    fn place_cursor(&mut self, path: PathBuf) {
        self.anchor = Some(path.clone());
        self.cursor = Some(path);
    }

    fn press(&mut self, path: PathBuf) -> Task<Message> {
        self.menu = None;
        self.path_edit = None;
        let now = Instant::now();
        let double = self
            .last_click
            .as_ref()
            .is_some_and(|(last, at)| last == &path && now.saturating_duration_since(*at) < DOUBLE_CLICK);
        if double && !self.modifiers.command() && !self.modifiers.shift() {
            self.last_click = None;
            self.select_only(path.clone());
            return self.navigate(path);
        }
        self.last_click = Some((path.clone(), now));
        if !self.in_trash() {
            self.drag_arm = Some(DragArm::Pressed);
        }
        if self.modifiers.shift()
            && let Some(range) = self.range_to(&path)
        {
            if !self.modifiers.command() {
                self.selection.clear();
            }
            self.selection.extend(range);
            self.cursor = Some(path);
        } else if self.modifiers.command() {
            if !self.selection.remove(&path) {
                self.selection.insert(path.clone());
            }
            self.place_cursor(path);
        } else if self.selection.contains(&path) && self.drag_arm.is_some() {
            self.place_cursor(path.clone());
            self.pending_select = Some(path);
        } else {
            self.select_only(path);
        }
        Task::none()
    }

    fn index_of(&self, path: &Path) -> Option<usize> {
        self.entries.iter().position(|entry| entry.path == path)
    }

    fn range_to(&self, path: &Path) -> Option<Vec<PathBuf>> {
        let from = self.index_of(self.anchor.as_deref()?)?;
        let to = self.index_of(path)?;
        Some(self.entries[from.min(to)..=from.max(to)].iter().map(|entry| entry.path.clone()).collect())
    }

    fn step(&mut self, step: Step, extend: bool) -> Task<Message> {
        if self.entries.is_empty() || self.modal.is_some() {
            return Task::none();
        }
        let last = self.entries.len() - 1;
        let columns = match self.view {
            ViewMode::Grid => self.metrics().columns(self.pane.width),
            ViewMode::List => 1,
        };
        let current = self.cursor.as_deref().and_then(|cursor| self.index_of(cursor));
        let next = match (current, step) {
            (_, Step::First) | (None, Step::Down | Step::Right) => 0,
            (_, Step::Last) | (None, Step::Up | Step::Left) => last,
            (Some(index), Step::Up) => index.saturating_sub(columns),
            (Some(index), Step::Down) => (index + columns).min(last),
            (Some(index), Step::Left) if self.view == ViewMode::Grid => index.saturating_sub(1),
            (Some(index), Step::Right) if self.view == ViewMode::Grid => (index + 1).min(last),
            (Some(index), Step::Left | Step::Right) => index,
        };
        let path = self.entries[next].path.clone();
        match self.range_to(&path).filter(|_| extend) {
            Some(range) => {
                self.selection = range.into_iter().collect();
                self.cursor = Some(path);
            }
            None => self.select_only(path),
        }
        self.reveal_cursor()
    }

    fn reveal_cursor(&self) -> Task<Message> {
        let Some(index) = self.cursor.as_deref().and_then(|cursor| self.index_of(cursor)) else {
            return Task::none();
        };
        let (top, bottom) = self.metrics().span(self.view, index, self.pane.width);
        widgets::pointer::reveal(Id::new(FILES_ID), top, bottom)
    }

    fn cursor_point(&self) -> Point {
        let left = widgets::SIDEBAR_WIDTH + 1.0;
        let top = self.content_top() + if self.view == ViewMode::List { content::LIST_HEADER } else { 0.0 };
        let centre = Point::new(left + self.pane.width / 2.0, self.content_top() + self.pane.height / 2.0);
        let Some(index) = self.cursor.as_deref().and_then(|cursor| self.index_of(cursor)) else {
            return centre;
        };
        let metrics = self.metrics();
        let (x, y) = metrics.origin(self.view, index, self.pane.width);
        let (dx, dy) = match self.view {
            ViewMode::Grid => (metrics.tile_w / 2.0, metrics.tile_h),
            ViewMode::List => (KEY_MENU_INSET, metrics.row),
        };
        let y = (top + y + dy - self.scroll_y).clamp(top, self.content_top() + self.pane.height);
        Point::new(left + x + dx, y)
    }

    #[must_use]
    pub fn content_top(&self) -> f32 {
        let banner = if self.in_trash() { widgets::TRASH_BANNER + 1.0 } else { 0.0 };
        CONTENT_TOP + banner
    }

    fn key_menu(&mut self) -> Task<Message> {
        self.menu_target = Some(if self.selection.is_empty() { MenuKind::Background } else { MenuKind::Selection });
        let task = self.handle(Message::ContextMenu(self.cursor_point()));
        self.highlight_first();
        task
    }

    fn highlight_first(&mut self) {
        let Some(menu) = &self.menu else {
            return;
        };
        let first = widgets::menu::next_highlight(&widgets::menu::entries(self, menu), None, Step::First);
        if let Some(menu) = &mut self.menu {
            menu.highlight = first;
        }
    }

    fn step_menu(&mut self, step: Step) -> Task<Message> {
        let Some(menu) = &self.menu else {
            return Task::none();
        };
        let entries = widgets::menu::entries(self, menu);
        if step == Step::Right {
            let opens = menu.highlight.and_then(|index| entries.get(index)).is_some_and(widgets::menu::Entry::opens);
            return if opens { self.activate_menu() } else { Task::none() };
        }
        let next = widgets::menu::next_highlight(&entries, menu.highlight, step);
        if let Some(menu) = &mut self.menu {
            menu.highlight = next;
        }
        Task::none()
    }

    fn activate_menu(&mut self) -> Task<Message> {
        let Some(menu) = &self.menu else {
            return Task::none();
        };
        let chosen = menu
            .highlight
            .and_then(|index| widgets::menu::entries(self, menu).into_iter().nth(index))
            .and_then(widgets::menu::Entry::into_message);
        let Some(message) = chosen else {
            self.menu = None;
            return Task::none();
        };
        let task = self.handle(Message::Menu(Box::new(message)));
        self.highlight_first();
        task
    }

    fn open_selection(&mut self) -> Task<Message> {
        let selected = self.selected();
        match selected.as_slice() {
            [] => Task::none(),
            [single] if single.is_dir() => self.navigate(single.clone()),
            _ => self.open_files(selected.into_iter().filter(|p| !p.is_dir()).collect()),
        }
    }

    fn open_files(&self, paths: Vec<PathBuf>) -> Task<Message> {
        let lang = self.lang;
        let mut groups: Vec<(Option<Run>, Vec<Target>)> = Vec::new();
        for path in paths.into_iter().take(OPEN_LIMIT) {
            let target = self.extensions.target(&path, false);
            let app = self
                .extensions
                .apps_for(std::slice::from_ref(&target))
                .first()
                .map(|app| Run::Exec { line: app.exec.clone(), desktop: app.desktop.clone() });
            match groups.iter_mut().find(|(run, _)| run.is_some() && *run == app) {
                Some((_, targets)) => targets.push(target),
                None => groups.push((app, vec![target])),
            }
        }
        Task::batch(groups.into_iter().map(|(app, targets)| {
            let cwd = self.current.clone();
            run(move || {
                let opened = match app {
                    Some(run) => run.launch(&targets, &cwd),
                    None => open_with_default(&targets[0].path),
                };
                Outcome {
                    text: opened
                        .err()
                        .map(|_| format!("{} «{}»", Key::CantOpen.text(lang), file_name(&targets[0].path))),
                    ..Outcome::default()
                }
            })
        }))
    }

    fn targets(&self) -> Vec<Target> {
        self.selected_entries().map(|entry| self.extensions.target(&entry.path, entry.is_dir)).collect()
    }

    fn open_with_offers(&self, targets: &[Target]) -> Vec<Offer> {
        if targets.iter().any(|target| target.mime == crate::fs::mime::DIRECTORY) {
            return Vec::new();
        }
        let apps = self.extensions.listed_apps_for(targets);
        let lang = self.lang;
        let group = Some(Key::OpenWith.text(lang).to_string());
        let mut offers: Vec<Offer> = apps
            .iter()
            .take(OPEN_WITH_LIMIT)
            .map(|app| Offer {
                label: app.name.clone(),
                group: group.clone(),
                run: Run::Exec { line: app.exec.clone(), desktop: app.desktop.clone() },
            })
            .collect();
        if let [target] = targets {
            let current = self
                .extensions
                .default_app(&target.mime)
                .and_then(|id| self.extensions.apps.iter().find(|app| app.id == id))
                .map(|app| app.exec.as_str());
            let group = Some(Key::AlwaysOpenWith.text(lang).to_string());
            offers.extend(apps.iter().take(OPEN_WITH_LIMIT).filter(|app| Some(app.exec.as_str()) != current).map(
                |app| Offer {
                    label: app.name.clone(),
                    group: group.clone(),
                    run: Run::SetDefault { mime: target.mime.clone(), app: app.id.clone() },
                },
            ));
        }
        offers
    }

    fn run_action(&mut self, offer: Offer) -> Task<Message> {
        if let Run::SetDefault { .. } = &offer.run {
            let text = offer.run.launch(&[], &self.current).err().map(|err| i18n::io_error(&err, self.lang));
            let reload = self.load_extensions();
            return match text {
                Some(text) => Task::batch([reload, self.notify(text, None)]),
                None => reload,
            };
        }
        let targets = self.targets();
        let cwd = self.current.clone();
        let lang = self.lang;
        run(move || Outcome {
            text: offer
                .run
                .launch(&targets, &cwd)
                .err()
                .map(|_| format!("{} «{}»", Key::CantRun.text(lang), offer.label)),
            ..Outcome::default()
        })
    }

    fn load_extensions(&mut self) -> Task<Message> {
        self.extensions_stamp = self.roots.stamp();
        let roots = self.roots.clone();
        let lang = self.lang;
        Task::perform(blocking(move || Extensions::load(&roots, lang)), |loaded| {
            Message::ExtensionsLoaded(Arc::new(loaded.unwrap_or_default()))
        })
    }

    fn open_search(&mut self, query: String) -> Task<Message> {
        self.menu = None;
        self.path_edit = None;
        Task::batch([self.set_query(query), operation::focus(SEARCH_ID), operation::move_cursor_to_end(SEARCH_ID)])
    }

    fn close_search(&mut self) -> Task<Message> {
        self.search = None;
        self.search_generation.fetch_add(1, Ordering::Relaxed);
        self.reload()
    }

    fn set_query(&mut self, query: String) -> Task<Message> {
        let generation = self.search_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let trimmed = query.trim().to_string();
        if query.is_empty() {
            return self.close_search();
        }
        let search = self.search.get_or_insert_default();
        search.query = query;
        search.pending = !trimmed.is_empty();
        if trimmed.is_empty() {
            return self.reload();
        }
        let root = self.current.clone();
        let hidden = self.show_hidden;
        let latest = Arc::clone(&self.search_generation);
        Task::perform(
            blocking(move || {
                let cancelled = || latest.load(Ordering::Relaxed) != generation;
                search_recursive(&root, &trimmed, hidden, SEARCH_LIMIT, &cancelled)
            }),
            move |found| Message::SearchResults(generation, found.unwrap_or_default()),
        )
    }

    fn submit_path(&mut self) -> Task<Message> {
        let Some(text) = self.path_edit.clone() else {
            return Task::none();
        };
        let target = expand_path(&text);
        if target.is_dir() {
            self.path_edit = None;
            return self.navigate(target);
        }
        if target.exists()
            && let Some(parent) = target.parent()
        {
            self.path_edit = None;
            let parent = parent.to_path_buf();
            if parent == self.current {
                return self.list(parent, Arrival::Refresh, Some(target));
            }
            return self.navigate_select(parent, Some(target));
        }
        self.notify(Key::PathNotFound.text(self.lang).to_string(), None)
    }

    fn fill_clipboard(&mut self, action: ClipboardAction) {
        let paths = self.selected();
        if !paths.is_empty() && !self.in_trash() {
            if let Some(link) = &self.dnd {
                link.send(dnd::Command::SetSelection { paths: paths.clone(), cut: action == ClipboardAction::Cut });
            }
            self.clipboard = Some(FileClipboard { action, paths });
        }
    }

    fn paste(&mut self) -> Task<Message> {
        if self.in_trash() {
            return Task::none();
        }
        match (self.clipboard.clone(), &self.dnd) {
            (Some(clipboard), _) => self.paste_into(clipboard, self.current.clone(), true),
            (None, Some(link)) => {
                link.send(dnd::Command::Paste(self.current.clone()));
                Task::none()
            }
            (None, None) => iced::clipboard::read().map(Message::PasteText),
        }
    }

    fn serve(&mut self, request: crate::service::Request) -> Task<Message> {
        use crate::service::Request;
        let focus = window::latest().and_then(window::gain_focus);
        let task = match request {
            Request::Open(path) if path.is_dir() => self.navigate(path),
            Request::Open(path) | Request::Reveal(path) => match path.parent() {
                Some(parent) if parent == self.current => self.list(parent.to_path_buf(), Arrival::Refresh, Some(path)),
                Some(parent) => self.navigate_select(parent.to_path_buf(), Some(path)),
                None => Task::none(),
            },
            Request::Properties(path) => {
                self.modal = None;
                self.properties_of(path)
            }
        };
        Task::batch([task, focus])
    }

    fn request_thumbs(&mut self) -> Task<Message> {
        let thumbnailers = Arc::clone(&self.extensions.thumbnailers);
        let wanted: Vec<(ThumbKey, String)> = self
            .entries
            .iter()
            .filter(|entry| !entry.is_dir)
            .map(thumb_key)
            .filter(|key| !self.thumbs.contains_key(key) && !self.thumbs_asked.contains(key))
            .map(|key| {
                let mime = self.extensions.mime.of(&key.0, false);
                (key, mime)
            })
            .filter(|(_, mime)| crate::fs::thumbs::can_thumbnail(mime, &thumbnailers))
            .collect();
        if wanted.is_empty() {
            return Task::none();
        }
        let requests = wanted
            .into_iter()
            .map(|(key, mime)| {
                let path = key.0.clone();
                self.thumbs_asked.insert(key);
                (path, mime)
            })
            .collect();
        let generation = self.thumb_generation.load(Ordering::Relaxed);
        let modified: HashMap<PathBuf, Option<SystemTime>> =
            self.entries.iter().map(|entry| (entry.path.clone(), entry.modified)).collect();
        let stream = crate::fs::thumbs::generate(requests, &thumbnailers, &self.thumb_generation, generation);
        Task::run(stream, move |(path, thumb)| {
            let stamp = modified.get(&path).copied().flatten();
            Message::Thumb((path, stamp), thumb)
        })
    }

    #[must_use]
    pub fn thumbnail(&self, entry: &FileEntry) -> Option<&PathBuf> {
        self.thumbs.get(&thumb_key(entry))?.as_ref()
    }

    fn save_prefs(&self) {
        config::save_prefs(&config::ViewPrefs {
            view: self.view,
            sort: self.sort,
            show_hidden: self.show_hidden,
            grid_zoom: self.grid_zoom,
            list_zoom: self.list_zoom,
            columns: self.columns.clone(),
            queue: self.queue,
        });
    }

    #[must_use]
    pub fn metrics(&self) -> Metrics {
        Metrics::new(self.grid_zoom, self.list_zoom)
    }

    #[must_use]
    pub fn bookmark_drop_offered(&self) -> bool {
        self.drag.as_ref().is_some_and(|drag| drag.folders && drag.paths.iter().any(|path| !self.is_bookmarked(path)))
    }

    #[must_use]
    pub fn bookmark_drop_active(&self) -> bool {
        self.drag.as_ref().is_some_and(|drag| drag.action == DropAction::Bookmark)
    }

    #[must_use]
    pub fn is_bookmark_pressed(&self, path: &Path) -> bool {
        self.bookmark_drag.as_deref() == Some(path)
    }

    #[must_use]
    pub fn is_bookmark_target(&self, path: &Path) -> bool {
        self.bookmark_drag.as_deref().is_some_and(|dragged| dragged != path)
            && self.hover_target.as_ref() == Some(&Hover::Place(path.to_path_buf()))
            && self.is_bookmarked(path)
    }

    #[must_use]
    pub fn is_column_target(&self, column: ListColumn) -> bool {
        self.column_drag.is_some_and(|dragged| dragged != column) && self.hover_target == Some(Hover::Column(column))
    }

    fn start_queued(&mut self) {
        if self.jobs.iter().any(|job| job.launch.is_none()) {
            return;
        }
        while let Some(job) = self.jobs.iter_mut().find(|job| job.launch.is_some()) {
            let started = job.launch.take().is_some_and(|launch| (launch.0)());
            if started {
                return;
            }
            let id = job.id;
            self.jobs.retain(|job| job.id != id);
        }
    }

    fn zoom(&mut self, step: i8) {
        let (level, levels) = match self.view {
            ViewMode::Grid => (&mut self.grid_zoom, config::GRID_ZOOMS),
            ViewMode::List => (&mut self.list_zoom, config::LIST_ZOOMS),
        };
        *level = match step {
            0 => match self.view {
                ViewMode::Grid => config::DEFAULT_GRID_ZOOM,
                ViewMode::List => config::DEFAULT_LIST_ZOOM,
            },
            _ => level.saturating_add_signed(step).min(u8::try_from(levels - 1).unwrap_or(0)),
        };
        self.save_prefs();
    }

    fn start_rename(&mut self) -> Task<Message> {
        if self.renaming.is_some() {
            return Task::none();
        }
        let Some(target) = self.primary().filter(|_| !self.in_trash()) else {
            return Task::none();
        };
        let name = file_name(&target);
        let stem = if target.is_dir() {
            name.chars().count()
        } else {
            Path::new(&name).file_stem().map_or(0, |stem| stem.to_string_lossy().chars().count())
        };
        if self.index_of(&target).is_none() {
            return self.prompt(Dialog::Rename(target), name, Some(stem));
        }
        self.renaming = Some(Renaming { path: target, text: name });
        Task::batch([self.reveal_cursor(), operation::focus(RENAME_ID), operation::select_range(RENAME_ID, 0, stem)])
    }

    fn commit_rename(&mut self) -> Task<Message> {
        let Some(renaming) = self.renaming.take() else {
            return Task::none();
        };
        if file_name(&renaming.path) == renaming.text.trim() {
            return Task::none();
        }
        match rename_item(&renaming.path, &renaming.text) {
            Ok(path) => {
                self.last_undo = Some(Undo::Rename(path.clone(), renaming.path.clone()));
                self.show(path)
            }
            Err(err) => {
                let text = i18n::io_error(&err, self.lang);
                if self.index_of(&renaming.path).is_none() {
                    return self.notify(text, None);
                }
                self.renaming = Some(renaming);
                Task::batch([self.notify(text, None), operation::focus(RENAME_ID)])
            }
        }
    }

    fn toggle_tag(&mut self, color: tags::Color) -> Task<Message> {
        let entries: Vec<(PathBuf, Vec<String>)> =
            self.selected_entries().map(|entry| (entry.path.clone(), entry.tags.clone())).collect();
        let on = !entries.iter().all(|(_, tags)| tags::colors(tags).contains(&color));
        self.write_tags(entries.into_iter().map(|(path, mut current)| {
            tags::set(&mut current, color.tag(), on);
            (path, current)
        }))
    }

    fn write_tags(&mut self, updates: impl Iterator<Item = (PathBuf, Vec<String>)>) -> Task<Message> {
        let mut failed = None;
        for (path, tags) in updates {
            if let Err(err) = tags::write(&path, &tags) {
                failed.get_or_insert(err);
            }
        }
        let refresh = self.refresh_listing();
        match failed {
            Some(err) if err.raw_os_error() == Some(libc::ENOTSUP) => {
                Task::batch([refresh, self.notify(Key::TagsUnsupported.text(self.lang).to_string(), None)])
            }
            Some(err) => Task::batch([refresh, self.notify(i18n::io_error(&err, self.lang), None)]),
            None => refresh,
        }
    }

    fn column_released(&mut self, column: ListColumn) -> Task<Message> {
        match &self.hover_target {
            Some(Hover::Column(target)) if *target != column => {
                let target = *target;
                if let (Some(from), Some(to)) =
                    (self.columns.iter().position(|c| *c == column), self.columns.iter().position(|c| *c == target))
                {
                    let moved = self.columns.remove(from);
                    self.columns.insert(to, moved);
                    self.save_prefs();
                }
                Task::none()
            }
            Some(Hover::Column(_)) => column.sort().map_or_else(Task::none, |by| self.handle(Message::SortBy(by))),
            _ => Task::none(),
        }
    }

    fn bookmark_released(&mut self, dragged: PathBuf) -> Task<Message> {
        match self.hover_target.clone() {
            Some(Hover::Place(target)) if target != dragged && self.is_bookmarked(&target) => {
                self.edit_bookmarks(|lines| {
                    let matches = |line: &String, path: &Path| {
                        path_from_arg(line.split(' ').next().unwrap_or_default()).as_deref() == Some(path)
                    };
                    let Some(from) = lines.iter().position(|line| matches(line, &dragged)) else {
                        return;
                    };
                    let moved = lines.remove(from);
                    let to = lines.iter().position(|line| matches(line, &target)).unwrap_or(lines.len());
                    lines.insert(if to >= from { to + 1 } else { to }, moved);
                })
            }
            Some(Hover::Place(target)) if target == dragged => self.navigate(dragged),
            _ => Task::none(),
        }
    }

    fn scan_phones(&mut self) -> Task<Message> {
        if self.disk_watch.phones {
            return Task::none();
        }
        self.disk_watch.phones = true;
        Task::perform(blocking(crate::fs::mtp::scan), |phones| Message::Phones(phones.unwrap_or_default()))
    }

    fn save_pasted(&mut self, name: String, bytes: Vec<u8>, target: PathBuf) -> Task<Message> {
        let lang = self.lang;
        run(move || match crate::fs::write_new_file(&target, &name, &bytes) {
            Ok(path) => Outcome { select: vec![path], ..Outcome::default() },
            Err(err) => Outcome { text: Some(i18n::io_error(&err, lang)), ..Outcome::default() },
        })
    }

    #[must_use]
    pub const fn scroll_offset(&self) -> f32 {
        self.scroll_y
    }

    #[must_use]
    pub fn drop_target(&self) -> Option<&Path> {
        self.drop_target.as_deref()
    }

    #[must_use]
    pub fn is_drop_target(&self, path: &Path) -> bool {
        self.drop_target.as_deref() == Some(path)
    }

    fn content_point(&self, x: f32, y: f32) -> (f32, f32) {
        let mut top = self.content_top();
        if self.view == ViewMode::List {
            top += content::LIST_HEADER;
        }
        let left = widgets::SIDEBAR_WIDTH + 1.0;
        (x - left, y - top + self.scroll_y)
    }

    fn band_motion(&mut self, at: Point) {
        let Some(band) = self.band.as_mut() else {
            return;
        };
        let start = *band.start.get_or_insert(at);
        band.end = at;
        if start.distance(at) < DRAG_THRESHOLD {
            return;
        }
        let from = self.content_point(start.x, start.y);
        let to = self.content_point(at.x, at.y);
        let width = self.pane.width - if self.view == ViewMode::List { 16.0 } else { 0.0 };
        let hits = self.metrics().entries_in(self.view, width, self.entries.len(), from, to);
        let mut selection = self.band.as_ref().map(|band| band.base.clone()).unwrap_or_default();
        selection.extend(hits.into_iter().map(|index| self.entries[index].path.clone()));
        self.selection = selection;
    }

    #[must_use]
    pub fn band_rect(&self) -> Option<iced::Rectangle> {
        let band = self.band.as_ref()?;
        let start = band.start?;
        (start.distance(band.end) >= DRAG_THRESHOLD).then(|| {
            iced::Rectangle::new(
                Point::new(start.x.min(band.end.x), start.y.min(band.end.y)),
                Size::new((start.x - band.end.x).abs(), (start.y - band.end.y).abs()),
            )
        })
    }

    fn folder_at(&self, x: f64, y: f64) -> Option<PathBuf> {
        let (x, y) = self.content_point(x as f32, y as f32);
        if x < 0.0 || y < self.scroll_y || self.in_trash() {
            return None;
        }
        let index = self.metrics().entry_at(self.view, self.pane.width, self.entries.len(), x, y)?;
        self.entries.get(index).filter(|entry| entry.is_dir).map(|entry| entry.path.clone())
    }

    fn drag_motion(&mut self, at: Point) {
        if self.drag.is_some() {
            self.track_drag(at);
            return;
        }
        let origin = match self.drag_arm {
            None => return,
            Some(DragArm::Pressed) => {
                self.drag_arm = Some(DragArm::At(at));
                return;
            }
            Some(DragArm::At(origin)) => origin,
        };
        if origin.distance(at) < DRAG_THRESHOLD {
            return;
        }
        self.drag_arm = None;
        let paths = self.selected();
        if paths.is_empty() {
            return;
        }
        self.menu = None;
        let folders = paths.iter().all(|path| path.is_dir() && !self.trash.contains(path));
        self.drag = Some(Drag { paths, at, action: DropAction::Move, folders, dwell: None });
        self.track_drag(at);
    }

    fn outside_window(&self, at: Point) -> bool {
        let size = self.window.size;
        size.width > 0.0 && (at.x < 0.0 || at.y < 0.0 || at.x >= size.width || at.y >= size.height)
    }

    fn in_content(&self, at: Point) -> bool {
        let top = self.content_top();
        at.x > widgets::SIDEBAR_WIDTH && at.y >= top && at.y < top + self.pane.height
    }

    fn drop_candidate(&self, at: Point) -> Option<PathBuf> {
        if at.x <= widgets::SIDEBAR_WIDTH || !self.in_content(at) {
            return match &self.hover_target {
                Some(Hover::Place(path) | Hover::Crumb(path)) => Some(path.clone()),
                _ => None,
            };
        }
        self.folder_at(f64::from(at.x), f64::from(at.y)).or_else(|| (!self.in_trash()).then(|| self.current.clone()))
    }

    fn track_drag(&mut self, at: Point) {
        if self.outside_window(at)
            && let Some(link) = &self.dnd
            && let Some(drag) = self.drag.take()
        {
            link.send(dnd::Command::StartDrag(drag.paths));
            self.drop_target = None;
            return;
        }
        let trash = self.trash.files();
        let candidate = self.drop_candidate(at).filter(|target| *target == trash || !self.trash.contains(target));
        let bookmarking = self.hover_target == Some(Hover::Bookmarks) && self.bookmark_drop_offered();
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        drag.at = at;

        if bookmarking {
            drag.dwell = None;
            drag.action = DropAction::Bookmark;
            self.drop_target = None;
            return;
        }
        let target = candidate.filter(|target| droppable(&drag.paths, target, &trash));
        if drag.dwell.as_ref().map(|(path, _)| path) != target.as_ref() {
            drag.dwell = target.clone().map(|path| (path, Instant::now()));
        }
        drag.action = match &target {
            Some(target) if *target == trash => DropAction::Trash,
            Some(target)
                if self.modifiers.command() || (!self.modifiers.shift() && !same_device(&drag.paths[0], target)) =>
            {
                DropAction::Copy
            }
            _ => DropAction::Move,
        };
        self.drop_target = target;
    }

    fn release(&mut self) -> Task<Message> {
        let Some(drag) = self.drag.take() else {
            return Task::none();
        };
        if drag.action == DropAction::Bookmark {
            let fresh: Vec<PathBuf> = drag.paths.into_iter().filter(|path| !self.is_bookmarked(path)).collect();
            return self.edit_bookmarks(|lines| {
                lines.extend(fresh.iter().map(|path| file_uri(path)));
            });
        }
        let Some(target) = self.drop_target.take() else {
            return Task::none();
        };
        if drag.action == DropAction::Trash {
            return self.trash_paths(drag.paths);
        }
        let action = if drag.action == DropAction::Copy { ClipboardAction::Copy } else { ClipboardAction::Cut };
        self.paste_into(FileClipboard { action, paths: drag.paths }, target, false)
    }

    fn drag_tick(&mut self) -> Task<Message> {
        let Some(drag) = self.drag.as_mut() else {
            return Task::none();
        };
        let at = drag.at;
        let spring = drag
            .dwell
            .as_ref()
            .filter(|(path, since)| since.elapsed() >= SPRING_DELAY && *path != self.current)
            .map(|(path, _)| path.clone());
        if let Some(path) = spring.filter(|path| path.is_dir() && !self.trash.contains(path)) {
            drag.dwell = None;
            self.drop_target = None;
            return self.navigate(path);
        }
        let top = self.content_top();
        let bottom = top + self.pane.height;
        let step = if !self.in_content(at) {
            0.0
        } else if at.y < top + EDGE_SCROLL {
            -SCROLL_STEP
        } else if at.y > bottom - EDGE_SCROLL {
            SCROLL_STEP
        } else {
            0.0
        };
        if step == 0.0 {
            return Task::none();
        }
        operation::scroll_by(Id::new(FILES_ID), iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: step })
    }

    fn on_dnd(&mut self, event: dnd::Event) -> Task<Message> {
        match event {
            dnd::Event::Ready(link) => self.dnd = Some(link),
            dnd::Event::Hover(at) => {
                self.drop_target = at.and_then(|(x, y)| self.folder_at(x, y));
            }
            dnd::Event::Drop { paths, at, moved } => {
                self.drop_target = None;
                let target = self.folder_at(at.0, at.1).unwrap_or_else(|| self.current.clone());
                let pointless = self.in_trash()
                    || paths.iter().any(|path| path == &target)
                    || paths.iter().all(|path| path.parent() == Some(target.as_path()));
                if !pointless {
                    let action = if moved { ClipboardAction::Cut } else { ClipboardAction::Copy };
                    return self.paste_into(FileClipboard { action, paths }, target, false);
                }
            }
            dnd::Event::Pasted { paths, target, cut } => {
                let action = if cut { ClipboardAction::Cut } else { ClipboardAction::Copy };
                return self.paste_into(FileClipboard { action, paths }, target, true);
            }
            dnd::Event::PastedData { target, bytes, extension, image } => {
                let stem = if image { Key::PastedImage } else { Key::PastedText };
                return self.save_pasted(format!("{}.{extension}", stem.text(self.lang)), bytes, target);
            }
            dnd::Event::SelectionLost => self.clipboard = None,
            dnd::Event::DragEnded => self.drop_target = None,
        }
        Task::none()
    }

    fn paste_into(&mut self, clipboard: FileClipboard, target: PathBuf, from_clipboard: bool) -> Task<Message> {
        let op = match clipboard.action {
            ClipboardAction::Copy => Op::Copy,
            ClipboardAction::Cut => Op::Move,
        };
        self.spawn_job(op, clipboard.paths, target, from_clipboard)
    }

    fn spawn_job(&mut self, op: Op, sources: Vec<PathBuf>, target: PathBuf, from_clipboard: bool) -> Task<Message> {
        if self.quit_requested.is_some() {
            return Task::none();
        }
        self.next_job += 1;
        let id = self.next_job;
        let (cancel, pause) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        let (answers, inbox) = mpsc::channel();
        let (events, stream) = unbounded();
        let (flag, dir, trash) = (Arc::clone(&cancel), target.clone(), self.trash.clone());
        let held = Arc::clone(&pause);
        let start = move || {
            std::thread::Builder::new()
                .name(String::from("glide-files-job"))
                .spawn(move || {
                    let sink = ChannelSink { events: events.clone(), answers: inbox, pause: held };
                    let report = match op {
                        Op::Delete => crate::fs::jobs::delete(&sources, &sink, &flag),
                        Op::Trash => crate::fs::jobs::each(&sources, &sink, &flag, |path| trash.put(path)),
                        Op::Copy | Op::Move => crate::fs::jobs::transfer(op, &sources, &dir, &sink, &flag),
                    };
                    let _ = events.unbounded_send(JobEvent::Done(Box::new(report)));
                })
                .is_ok()
        };
        let waits = self.queue && self.jobs.iter().any(|job| job.launch.is_none());
        let launch = if waits {
            Some(Launch(Box::new(start)))
        } else if start() {
            None
        } else {
            return Task::none();
        };
        self.jobs.push(Job {
            launch,
            id,
            op,
            progress: Progress::default(),
            target,
            from_clipboard,
            cancel,
            pause,
            meter: Meter::default(),
            answers,
            question: None,
        });
        self.sync_dock();
        Task::run(stream, move |event| Message::Job(id, event))
    }

    fn on_job(&mut self, id: u64, event: JobEvent) -> Task<Message> {
        match event {
            JobEvent::Progress(progress) => {
                if let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) {
                    job.meter.sample(progress.bytes, Instant::now());
                    job.progress = progress;
                }
                self.sync_dock();
                Task::none()
            }
            JobEvent::Ask(question) => {
                if let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) {
                    job.question = Some(question);
                }
                self.surface_question();
                Task::none()
            }
            JobEvent::Done(report) => {
                let Some(index) = self.jobs.iter().position(|job| job.id == id) else {
                    return Task::none();
                };
                let job = self.jobs.remove(index);
                if self.quit_requested.is_some() {
                    return if self.jobs.is_empty() { iced::exit() } else { Task::none() };
                }
                self.start_queued();
                self.sync_dock();
                if matches!(&self.modal, Some(Modal { dialog: Dialog::Conflict { job, .. }, .. }) if *job == id) {
                    self.modal = None;
                    self.surface_question();
                }
                self.job_finished(&job, &report)
            }
        }
    }

    fn job_finished(&mut self, job: &Job, report: &Report) -> Task<Message> {
        let lang = self.lang;
        if job.op == Op::Delete {
            for (path, _) in &report.done {
                self.trash.forget(path);
            }
        }
        let count = report.done.len();
        let text = if let Some(first) = report.failures.first() {
            let name = file_name(&first.path);
            let more = report.failures.len() - 1;
            let text = format!("«{name}»: {}", i18n::io_error(&first.error(), lang));
            Some(if more > 0 { format!("{text} (+{more})") } else { text })
        } else if report.cancelled {
            Some(Key::OperationCancelled.text(lang).to_string())
        } else if count > 0 {
            let key = match job.op {
                Op::Copy => Key::Copied,
                Op::Move => Key::Moved,
                Op::Delete => Key::Deleted,
                Op::Trash => Key::MovedToTrash,
            };
            Some(format!("{}: {}", key.text(lang), i18n::items(count, lang)))
        } else {
            None
        };
        let dests: Vec<PathBuf> = report.done.iter().map(|(_, dest)| dest.clone()).collect();
        let undo = match job.op {
            _ if report.done.is_empty() => None,
            Op::Copy => Some(Undo::TrashCopies(dests.clone())),
            Op::Move => Some(Undo::MoveBack(report.done.clone())),
            Op::Trash => Some(Undo::Restore(dests.clone())),
            Op::Delete => None,
        };
        let placed = matches!(job.op, Op::Copy | Op::Move);
        let select = if job.target == self.current && placed { dests } else { Vec::new() };
        self.finish(Outcome {
            text,
            select,
            undo,
            clear_clipboard: job.from_clipboard && job.op == Op::Move && !report.cancelled,
        })
    }

    fn surface_question(&mut self) {
        if self.modal.is_some() {
            return;
        }
        let Some((job, question)) =
            self.jobs.iter().find_map(|job| job.question.clone().map(|question| (job.id, question)))
        else {
            return;
        };
        self.menu = None;
        self.modal = Some(Modal {
            dialog: Dialog::Conflict { job, question, for_all: false },
            input: String::new(),
            error: None,
        });
    }

    fn in_conflict(&self) -> bool {
        matches!(self.modal, Some(Modal { dialog: Dialog::Conflict { .. }, .. }))
    }

    fn dismiss_modal(&mut self) {
        if self.in_conflict() {
            self.resolve(Choice::Cancel);
        } else {
            self.modal = None;
            self.surface_question();
        }
    }

    fn resolve(&mut self, choice: Choice) {
        let Some(Modal { dialog: Dialog::Conflict { job, for_all, .. }, .. }) = self.modal.clone() else {
            return;
        };
        self.modal = None;
        if let Some(job) = self.jobs.iter_mut().find(|candidate| candidate.id == job) {
            job.question = None;
            let _ = job.answers.send(Answer { choice, for_all });
        }
        self.surface_question();
    }

    fn undo(&mut self, undo: Undo) -> Task<Message> {
        let lang = self.lang;
        match undo {
            Undo::Restore(paths) => self.restore(paths),
            Undo::TrashCopies(paths) => self.trash_paths(paths),
            Undo::MoveBack(pairs) => run(move || {
                let error = pairs.iter().find_map(|(original, now)| {
                    if crate::fs::operations::occupied(original) {
                        Some(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "name taken"))
                    } else {
                        crate::fs::operations::move_path(now, original).err()
                    }
                });
                Outcome { text: error.map(|err| i18n::io_error(&err, lang)), ..Outcome::default() }
            }),
            Undo::Rename(now, original) => run(move || {
                let name = file_name(&original);
                Outcome {
                    text: rename_item(&now, &name).err().map(|err| i18n::io_error(&err, lang)),
                    ..Outcome::default()
                }
            }),
        }
    }

    fn trash_selection(&mut self) -> Task<Message> {
        let paths = self.selected();
        if paths.is_empty() {
            return Task::none();
        }
        if self.in_trash() {
            return self.prompt(Dialog::Delete(paths), String::new(), None);
        }
        self.trash_paths(paths)
    }

    fn trash_paths(&mut self, paths: Vec<PathBuf>) -> Task<Message> {
        let target = self.trash.files();
        self.spawn_job(Op::Trash, paths, target, false)
    }

    fn sync_dock(&self) {
        let (done, total) = self.jobs.iter().fold((0.0, 0.0), |(done, total), job| {
            let weight = job.progress.total_bytes.max(1) as f64;
            (done + f64::from(job.progress.fraction()) * weight, total + weight)
        });
        crate::launcher::report((!self.jobs.is_empty()).then(|| if total > 0.0 { done / total } else { 0.0 }));
    }

    fn restore(&self, paths: Vec<PathBuf>) -> Task<Message> {
        if paths.is_empty() {
            return Task::none();
        }
        let trash = self.trash.clone();
        let lang = self.lang;
        run(move || {
            let mut restored = 0;
            let mut error = None;
            for path in &paths {
                match trash.restore(path) {
                    Ok(_) => restored += 1,
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                        error.get_or_insert(Key::ErrNotInTrash.text(lang).to_string());
                    }
                    Err(err) => {
                        error.get_or_insert(i18n::io_error(&err, lang));
                    }
                }
            }
            let summary =
                (restored > 0).then(|| format!("{}: {}", Key::Restored.text(lang), i18n::items(restored, lang)));
            Outcome { text: error.or(summary), ..Outcome::default() }
        })
    }

    fn properties(&mut self) -> Task<Message> {
        let target = if self.selection.is_empty() { Some(self.current.clone()) } else { self.primary() };
        match target {
            Some(path) => self.properties_of(path),
            None => Task::none(),
        }
    }

    fn edit_bookmarks(&mut self, edit: impl FnOnce(&mut Vec<String>)) -> Task<Message> {
        let Some(file) = bookmarks_file() else {
            return Task::none();
        };
        let mut lines: Vec<String> = std::fs::read_to_string(&file)
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_string)
            .collect();
        edit(&mut lines);
        let mut content = lines.join("\n");
        content.push('\n');
        let written =
            file.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&file, content));
        self.bookmarks = self.fresh_bookmarks();
        match written {
            Ok(()) => Task::none(),
            Err(err) => self.notify(i18n::io_error(&err, self.lang), None),
        }
    }

    #[must_use]
    pub fn is_bookmarked(&self, path: &Path) -> bool {
        self.bookmarks.iter().any(|mark| mark.path == path)
    }

    fn properties_of(&mut self, path: PathBuf) -> Task<Message> {
        let Some(entry) = FileEntry::from_path(path) else {
            return Task::none();
        };
        let meta = std::fs::symlink_metadata(&entry.path).ok();
        let properties = Properties {
            created: std::fs::metadata(&entry.path).ok().and_then(|meta| meta.created().ok()),
            target: meta
                .filter(|meta| meta.file_type().is_symlink())
                .and_then(|_| std::fs::read_link(&entry.path).ok()),
            contents: entry.is_dir.then(|| std::fs::read_dir(&entry.path).map(Iterator::count).ok()).flatten(),
            original: self.trash.original_path(&entry.path),
            total: None,
            error: None,
            entry,
        };
        let sizing = properties.entry.is_dir.then(|| {
            let dir = properties.entry.path.clone();
            Task::perform(
                blocking(move || {
                    let total = crate::fs::jobs::tree_size(&dir);
                    (dir, total)
                }),
                |result| {
                    let (dir, total) = result.unwrap_or_default();
                    Message::FolderSize(dir, total)
                },
            )
        });
        let shown = self.prompt(Dialog::Properties(Box::new(properties)), String::new(), None);
        Task::batch([shown, sizing.unwrap_or_else(Task::none)])
    }

    fn properties_mut(&mut self) -> Option<&mut Properties> {
        match &mut self.modal {
            Some(Modal { dialog: Dialog::Properties(properties), .. }) => Some(properties),
            _ => None,
        }
    }

    fn toggle_permission(&mut self, bit: u32) {
        use std::os::unix::fs::PermissionsExt;
        let lang = self.lang;
        let Some(properties) = self.properties_mut() else {
            return;
        };
        let mode = properties.entry.mode ^ bit;
        let path = properties.entry.path.clone();
        match std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode & 0o7777)) {
            Ok(()) => {
                properties.entry.mode = mode;
                properties.error = None;
            }
            Err(err) => properties.error = Some(i18n::io_error(&err, lang)),
        }
    }

    fn prompt(&mut self, dialog: Dialog, input: String, selected: Option<usize>) -> Task<Message> {
        self.menu = None;
        let takes_input = matches!(
            dialog,
            Dialog::NewFolder
                | Dialog::NewFile
                | Dialog::Rename(_)
                | Dialog::Unlock(_)
                | Dialog::NewFromTemplate(_)
                | Dialog::Tags(_)
        );
        self.modal = Some(Modal { dialog, input, error: None });
        if !takes_input {
            return Task::none();
        }
        let select = match selected {
            Some(end) => operation::select_range(MODAL_ID, 0, end),
            None => operation::select_all(MODAL_ID),
        };
        Task::batch([operation::focus(MODAL_ID), select])
    }

    fn submit_modal(&mut self) -> Task<Message> {
        if self.in_conflict() {
            return Task::none();
        }
        let Some(modal) = self.modal.take() else {
            return Task::none();
        };
        let created = match &modal.dialog {
            Dialog::NewFolder => create_directory(&self.current, &modal.input),
            Dialog::NewFile => create_empty_file(&self.current, &modal.input),
            Dialog::Rename(target) => rename_item(target, &modal.input),
            Dialog::NewFromTemplate(template) => crate::fs::create_from_template(template, &self.current, &modal.input),
            Dialog::Tags(paths) => {
                let words = tags::parse(&modal.input);
                let updates: Vec<(PathBuf, Vec<String>)> = paths
                    .iter()
                    .map(|path| {
                        let mut kept: Vec<String> =
                            tags::read(path).into_iter().filter(|tag| tags::Color::of(tag).is_some()).collect();
                        kept.extend(words.iter().cloned());
                        (path.clone(), kept)
                    })
                    .collect();
                return self.write_tags(updates.into_iter());
            }
            Dialog::Delete(paths) => {
                let current = self.current.clone();
                return self.spawn_job(Op::Delete, paths.clone(), current, false);
            }
            Dialog::EmptyTrash => {
                let trashes: Vec<Trash> = std::iter::once(self.trash.clone()).chain(self.volume_trashes()).collect();
                let lang = self.lang;
                return run(move || Outcome {
                    text: trashes
                        .iter()
                        .filter_map(|trash| trash.empty().err())
                        .collect::<Vec<_>>()
                        .first()
                        .map(|err| i18n::io_error(err, lang)),
                    ..Outcome::default()
                });
            }
            Dialog::Properties(_) | Dialog::Conflict { .. } => return Task::none(),
            Dialog::Quit => {
                self.quit_requested = Some(Instant::now());
                self.jobs.retain(|job| job.launch.is_none());
                self.jobs.iter_mut().for_each(Job::stop);
                self.sync_dock();
                return if self.jobs.is_empty() { iced::exit() } else { Task::none() };
            }
            Dialog::Repair(object) => return self.disk_job(object, DiskAction::Repair),
            Dialog::Unlock(object) => {
                let Some(volume) = self.volume(object).cloned() else {
                    return Task::none();
                };
                let passphrase = modal.input;
                return self.spawn_disk(volume, DiskAction::Mount, move |disks, volume| {
                    disks.unlock(volume, &passphrase).map(Some)
                });
            }
        };
        match created {
            Ok(path) => {
                if let Dialog::Rename(original) = &modal.dialog
                    && *original != path
                {
                    self.last_undo = Some(Undo::Rename(path.clone(), original.clone()));
                }
                self.trash_count = self.count_trash();
                self.show(path)
            }
            Err(err) => {
                self.modal = Some(Modal { error: Some(i18n::io_error(&err, self.lang)), ..modal });
                operation::focus(MODAL_ID)
            }
        }
    }

    fn finish(&mut self, outcome: Outcome) -> Task<Message> {
        if outcome.clear_clipboard {
            self.clipboard = None;
            if let Some(link) = &self.dnd {
                link.send(dnd::Command::ClearSelection);
            }
        }
        let task = self.refresh_listing();
        if let Some(last) = outcome.select.last().cloned() {
            self.selection = outcome.select.iter().cloned().collect();
            self.place_cursor(last);
        }
        if let Some(text) = outcome.text {
            return Task::batch([task, self.notify(text, outcome.undo)]);
        }
        if outcome.undo.is_some() {
            self.last_undo = outcome.undo;
        }
        task
    }

    fn notify(&mut self, text: String, undo: Option<Undo>) -> Task<Message> {
        let action = undo.is_some().then_some(NoticeAction::Undo);
        if undo.is_some() {
            self.last_undo = undo;
        }
        self.notify_with(text, action)
    }

    fn notify_with(&mut self, text: String, action: Option<NoticeAction>) -> Task<Message> {
        self.notices += 1;
        let id = self.notices;
        self.notice = Some(Notice { id, text, action });
        expire(id)
    }

    fn scan(&mut self) -> Task<Message> {
        if self.disk_watch.scanning {
            return Task::none();
        }
        self.disk_watch.scanning = true;
        let disks = Arc::clone(&self.disks);
        Task::perform(blocking(move || disks.volumes()), |volumes| {
            Message::Volumes(volumes.and_then(Result::ok).unwrap_or_default())
        })
    }

    fn mount(&mut self, object: &str, read_only: bool) -> Task<Message> {
        let Some(volume) = self.volume(object).cloned() else {
            return Task::none();
        };
        if volume.locked {
            return self.prompt(Dialog::Unlock(volume.object), String::new(), None);
        }
        match volume.mount.clone() {
            Some(path) if !read_only => self.navigate(path),
            Some(_) => Task::none(),
            None => self
                .spawn_disk(volume, DiskAction::Mount, move |disks, volume| disks.mount(volume, read_only).map(Some)),
        }
    }

    fn disk_job(&mut self, object: &str, action: DiskAction) -> Task<Message> {
        let Some(volume) = self.volume(object).cloned() else {
            return Task::none();
        };
        let inside = volume.mount.as_deref().is_some_and(|mount| self.current.starts_with(mount));
        let leave = match home_dir().filter(|_| inside) {
            Some(home) => self.navigate(home),
            None => Task::none(),
        };
        let job = self.spawn_disk(volume, action, move |disks, volume| match action {
            DiskAction::Mount => disks.mount(volume, false).map(Some),
            DiskAction::Unmount => disks.unmount(volume).map(|()| None),
            DiskAction::Eject => disks.eject(volume).map(|()| None),
            DiskAction::Repair => disks.repair(volume).map(Some),
        });
        Task::batch([leave, job])
    }

    fn spawn_disk(
        &mut self, volume: Volume, action: DiskAction,
        job: impl FnOnce(&Disks, &Volume) -> Result<Option<Mounted>, DiskError> + Send + 'static,
    ) -> Task<Message> {
        if !self.busy.insert(volume.object.clone()) {
            return Task::none();
        }
        let disks = Arc::clone(&self.disks);
        let object = volume.object.clone();
        let name = volume.name(self.lang);
        Task::perform(blocking(move || job(&disks, &volume)), move |result| {
            Message::Disk(Box::new(DiskOutcome {
                object,
                name,
                action,
                result: result.unwrap_or(Err(DiskError::Unavailable)),
            }))
        })
    }

    fn disk_done(&mut self, outcome: DiskOutcome) -> Task<Message> {
        self.busy.remove(&outcome.object);
        self.disk_watch.scanning = false;
        let rescan = self.scan();
        self.drives = mounted_drives(self.lang);
        let lang = self.lang;
        let name = outcome.name;
        let follow = match outcome.result {
            Err(DiskError::Cancelled) => Task::none(),
            Err(err) => {
                let failed = match outcome.action {
                    DiskAction::Mount => Key::CantMount,
                    DiskAction::Unmount => Key::CantUnmount,
                    DiskAction::Eject => Key::CantEject,
                    DiskAction::Repair => Key::CantRepair,
                };
                let text = format!("{} «{name}»: {}", failed.text(lang), i18n::disk_error(&err, lang));
                let action = (outcome.action == DiskAction::Mount && matches!(err, DiskError::Failed(_)))
                    .then(|| NoticeAction::Repair(outcome.object.clone()));
                self.notify_with(text, action)
            }
            Ok(Some(mounted)) => {
                let go = self.navigate(mounted.path);
                let status = match (outcome.action, mounted.mode) {
                    (_, MountMode::ReadOnly) => Some(Key::MountedReadOnly),
                    (_, MountMode::Fallback) => Some(Key::MountedFallback),
                    (DiskAction::Repair, MountMode::Normal) => Some(Key::Repaired),
                    _ => None,
                };
                let notice = match status {
                    Some(key) => {
                        let action = (key != Key::Repaired).then(|| NoticeAction::Repair(outcome.object.clone()));
                        self.notify_with(format!("«{name}» {}", key.text(lang)), action)
                    }
                    None => Task::none(),
                };
                Task::batch([go, notice])
            }
            Ok(None) if outcome.action == DiskAction::Eject => {
                self.notify(format!("«{name}» {}", Key::SafeToRemove.text(lang)), None)
            }
            Ok(None) => Task::none(),
        };
        Task::batch([rescan, follow])
    }

    fn poll(&mut self) -> Task<Message> {
        if self.quit_requested.is_some_and(|since| since.elapsed() >= QUIT_GRACE) {
            return iced::exit();
        }
        let drives = mounted_drives(self.lang);
        if drives != self.drives {
            self.drives = drives;
        }
        self.trash_count = self.count_trash();
        let config_stamp = config::path().as_deref().and_then(stamp);
        let lang = self.lang;
        if config_stamp != self.config_stamp {
            self.config_stamp = config_stamp;
            self.apply_look(&config::load());
        }
        let appearance = self.look.appearance_at(minute_now());
        if appearance != self.theme.tokens.appearance {
            self.theme = ThemeState::new(appearance, self.look.accent);
        }
        let extensions = if lang != self.lang || self.roots.stamp() != self.extensions_stamp {
            self.load_extensions()
        } else {
            Task::none()
        };
        let volumes = if self.disk_watch.live { Task::none() } else { self.scan() };
        let phones = self.scan_phones();
        let extensions = Task::batch([extensions, volumes, phones]);
        if !self.current.is_dir() {
            let fallback =
                self.current.ancestors().find(|dir| dir.is_dir()).map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
            return Task::batch([extensions, self.navigate(fallback)]);
        }
        extensions
    }

    fn apply_look(&mut self, look: &Look) {
        glide_theme::motion::set_scale(look.motion_scale);
        self.theme = ThemeState::new(look.appearance_at(minute_now()), look.accent);
        self.look = look.clone();
        self.icons = IconTheme::new(look.icon_theme.as_deref(), look.accent);
        let lang = look.language.unwrap_or_else(Lang::from_env);
        if lang != self.lang {
            self.lang = lang;
            self.places = standard_places(lang, &self.trash);
            self.drives = mounted_drives(lang);
        }
        self.bookmarks = self.fresh_bookmarks();
    }

    fn escape(&mut self) -> Task<Message> {
        if self.drag.take().is_some() {
            self.drop_target = None;
            return Task::none();
        }
        if self.renaming.take().is_some() {
            return Task::none();
        }
        if self.modal.is_some() {
            self.dismiss_modal();
            return Task::none();
        }
        if self.menu.take().is_some() {
            return Task::none();
        }
        if self.path_edit.take().is_some() {
            return Task::none();
        }
        if self.search.is_some() {
            return self.close_search();
        }
        self.selection.clear();
        Task::none()
    }
}

fn thumb_key(entry: &FileEntry) -> ThumbKey {
    (entry.path.clone(), entry.modified)
}

fn dnd_handles(window: &dyn iced::window::Window) -> Option<dnd::Handles> {
    use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
    let display = window.display_handle().ok()?.as_raw();
    let surface = window.window_handle().ok()?.as_raw();
    match (display, surface) {
        (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(surface)) => Some(dnd::Handles {
            display: display.display.as_ptr() as usize,
            surface: surface.surface.as_ptr() as usize,
        }),
        _ => None,
    }
}

#[must_use]
pub fn droppable(paths: &[PathBuf], target: &Path, trash: &Path) -> bool {
    if target == trash {
        return !paths.iter().any(|path| path.starts_with(trash));
    }
    !target.starts_with(trash)
        && !paths.iter().any(|path| target.starts_with(path))
        && !paths.iter().all(|path| path.parent() == Some(target))
}

fn same_device(source: &Path, target: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let device = |path: &Path| std::fs::symlink_metadata(path).map(|meta| meta.dev()).ok();
    device(source).is_some_and(|dev| Some(dev) == device(target))
}

#[must_use]
#[allow(clippy::needless_pass_by_value)] // iced::event::listen_with fixes this signature
pub fn window_events(event: iced::Event, _status: iced::event::Status, _window: window::Id) -> Option<Message> {
    match event {
        iced::Event::Window(window::Event::Opened { size, .. } | window::Event::Resized(size)) => {
            Some(Message::WindowSize(size))
        }
        iced::Event::Window(window::Event::Focused) => Some(Message::WindowFocus(true)),
        iced::Event::Window(window::Event::Unfocused) => Some(Message::WindowFocus(false)),
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => Some(Message::Released),
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Back)) => Some(Message::Back),
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Forward)) => Some(Message::Forward),
        iced::Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Named(Named::Escape), .. }) => {
            Some(Message::Escape)
        }
        _ => None,
    }
}

#[allow(clippy::needless_pass_by_value)] // iced::event::listen_with fixes this signature
fn drag_motion(event: iced::Event, _status: iced::event::Status, _window: window::Id) -> Option<Message> {
    match event {
        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => Some(Message::Cursor(position)),
        _ => None,
    }
}

fn expand_path(input: &str) -> PathBuf {
    let trimmed = input.trim();
    let home = home_dir();
    match (trimmed.strip_prefix('~'), home) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => home.join(rest.trim_start_matches('/')),
        _ => path_from_arg(trimmed).unwrap_or_else(|| PathBuf::from(trimmed)),
    }
}

#[must_use]
pub fn handle_shortcut(event: keyboard::Event) -> Option<Message> {
    use keyboard::Key;

    let (key, physical, modifiers, text) = match event {
        keyboard::Event::ModifiersChanged(modifiers) => {
            return Some(Message::Modifiers(modifiers));
        }
        keyboard::Event::KeyPressed { key, physical_key, modifiers, text, .. } => (key, physical_key, modifiers, text),
        keyboard::Event::KeyReleased { .. } => return None,
    };
    let command = modifiers.command();
    let shift = modifiers.shift();
    let alt = modifiers.alt();

    let message = match key.as_ref() {
        Key::Named(Named::ContextMenu) => Message::KeyMenu,
        Key::Named(Named::F10) if shift => Message::KeyMenu,
        Key::Named(Named::Enter) if alt => Message::Properties,
        Key::Named(Named::Enter) => Message::OpenSelection,
        Key::Named(Named::ArrowLeft) if alt => Message::Back,
        Key::Named(Named::ArrowRight) if alt => Message::Forward,
        Key::Named(Named::ArrowUp) if alt => Message::Up,
        Key::Named(Named::ArrowDown) if alt => Message::OpenSelection,
        Key::Named(Named::Backspace) => Message::Back,
        Key::Named(Named::ArrowUp) => Message::Move(Step::Up, shift),
        Key::Named(Named::ArrowDown) => Message::Move(Step::Down, shift),
        Key::Named(Named::ArrowLeft) => Message::Move(Step::Left, shift),
        Key::Named(Named::ArrowRight) => Message::Move(Step::Right, shift),
        Key::Named(Named::Home) if alt => Message::GoHome,
        Key::Named(Named::Home) => Message::Move(Step::First, shift),
        Key::Named(Named::End) => Message::Move(Step::Last, shift),
        Key::Named(Named::Delete) if shift => Message::Delete,
        Key::Named(Named::Delete) => Message::Trash,
        Key::Named(Named::F2) => Message::Rename,
        Key::Named(Named::F5) => Message::Reload,
        Key::Character(_) if command => match key.to_latin(physical)?.to_ascii_lowercase() {
            'r' => Message::Reload,
            'h' => Message::ToggleHidden,
            'l' => Message::EditPath,
            'f' => Message::ToggleSearch,
            'a' => Message::SelectAll,
            'c' => Message::Copy,
            'x' => Message::Cut,
            'v' => Message::Paste,
            'i' => Message::Properties,
            'z' => Message::Undo,
            'd' => Message::Duplicate,
            '=' | '+' => Message::Zoom(1),
            '-' => Message::Zoom(-1),
            '0' => Message::Zoom(0),
            'n' if shift => Message::NewFolder,
            '1' => Message::SetView(ViewMode::Grid),
            '2' => Message::SetView(ViewMode::List),
            'w' | 'q' => Message::Quit,
            _ => return None,
        },
        _ if !command && !alt => {
            let text = text.filter(|t| t.chars().all(|c| !c.is_control()) && !t.is_empty())?;
            Message::TypeToSearch(text.to_string())
        }
        _ => return None,
    };
    Some(message)
}

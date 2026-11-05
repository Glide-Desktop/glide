use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use glide_sdk::window::{Action, Frame, Window};
use glide_sdk::{Dialog, Item, Lang, Menu, Paint, Shell, Tone};
use iced::widget::image::Handle;
use iced::{Subscription, Task, keyboard};
use image::{Rgba32FImage, RgbaImage};

use crate::edit::{self, Edit, Filter, Output, Ratio, Slider};
use crate::formats::{self, Camera, Failure, Family, Note, Request};
use crate::i18n::{self, Key};
use crate::library::{self, Card, Filter as Chip, Photo, Smart};
use crate::prefs::{self, Album, Prefs};
use crate::thumbs::{self, Thumb};
use crate::trash::{self, Trashed};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    Library,
    Favourites,
    Albums,
    Album(usize),
    Smart(Smart),
    Folder(PathBuf),
    Card(PathBuf),
    Trash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    Years,
    Months,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backdrop {
    Checks,
    White,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Adjust,
    Filters,
    Crop,
}

pub const ZOOM_SIDES: [f32; 6] = [88.0, 112.0, 140.0, 176.0, 220.0, 280.0];
pub const SLIDE: Duration = Duration::from_secs(4);
pub const IDLE_CHROME: Duration = Duration::from_secs(2);
const THUMB_WORKERS: usize = 3;
const CARD_POLL_TICKS: u32 = 20;
const VIEW_SIDE: u32 = 4096;
const EDIT_SIDE: u32 = 1600;

#[derive(Debug, Clone)]
pub struct Loaded {
    pub path: PathBuf,
    pub handle: Handle,
    pub frames: Vec<(Handle, u32)>,
    pub size: (u32, u32),
    pub pages: u32,
    pub note: Option<Note>,
    pub hdr: Option<Arc<Rgba32FImage>>,
    pub camera: Camera,
    pub transparent: bool,
}

#[allow(clippy::struct_excessive_bools)] // Independent viewer toggles, each driven by its own message
pub struct Viewer {
    pub list: Vec<usize>,
    pub at: usize,
    pub loaded: Option<Loaded>,
    pub failure: Option<Failure>,
    pub frame: usize,
    pub playing: bool,
    pub page: u32,
    pub exposure: f32,
    pub zoom: Option<f32>,
    pub info: bool,
    pub chrome: bool,
    pub moved: Instant,
    pub pointer: iced::Point,
    pub slideshow: bool,
    pub backdrop: Backdrop,
    pub generation: u64,
}

impl Viewer {
    #[must_use]
    pub fn current(&self) -> Option<usize> {
        self.list.get(self.at).copied()
    }
}

#[derive(Debug, Clone)]
pub struct SaveForm {
    pub replace: bool,
    pub can_replace: bool,
    pub name: String,
    pub output: Output,
}

pub struct Editor {
    pub photo: usize,
    pub source: Option<Arc<RgbaImage>>,
    pub original: Option<Handle>,
    pub edit: Edit,
    pub history: Vec<Edit>,
    pub at: usize,
    pub tool: Tool,
    pub preview: Option<(Handle, (u32, u32))>,
    pub geometry: Option<(Handle, (u32, u32))>,
    pub histogram: [[u32; edit::BINS]; 3],
    pub compare: bool,
    pub generation: u64,
    pub swatches: Vec<(Filter, Handle)>,
    pub saving: Option<SaveForm>,
    pub busy: bool,
}

impl Editor {
    fn commit(&mut self) {
        if self.history.get(self.at) == Some(&self.edit) {
            return;
        }
        self.history.truncate(self.at + 1);
        self.history.push(self.edit);
        self.at = self.history.len() - 1;
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.at > 0
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.at + 1 < self.history.len()
    }
}

#[derive(Debug, Clone)]
pub enum Modal {
    NewAlbum { name: String, adding: Vec<PathBuf> },
}

#[derive(Debug, Clone)]
pub enum Message {
    Window(Action),
    Close,
    Scanned(Vec<Photo>),
    CardScanned(Vec<Photo>),
    Thumb(PathBuf, Thumb),
    Go(Place),
    Scale(Scale),
    Query(String),
    ClearQuery,
    Chip(Chip),
    Zoom(usize),
    Scrolled(iced::Rectangle, f32),
    Frame(Instant),
    StartSelect,
    EndSelect,
    SelectAll,
    SelectDay(usize),
    Tile(usize),
    TileMenu(usize),
    Open(usize),
    PeriodPressed(i32, Option<u32>),
    Favourite(Vec<PathBuf>, bool),
    AlbumMenu(Vec<PathBuf>),
    AddTo(usize, Vec<PathBuf>),
    AskNewAlbum(Vec<PathBuf>),
    AlbumName(String),
    CreateAlbum,
    CancelModal,
    DeleteAlbumAsk(usize),
    DeleteAlbum(usize),
    RemoveFromAlbum(usize, Vec<PathBuf>),
    Trash(Vec<PathBuf>),
    Trashed(Vec<Trashed>, usize),
    UndoTrash(Vec<Trashed>),
    TrashList(Vec<Trashed>),
    Restore(Trashed),
    TrashMenu(Trashed),
    Purge(Trashed),
    TrashRefresh,
    Rescan,
    AskEmptyTrash,
    EmptyTrash,
    Reveal(PathBuf),
    Rotate(Vec<PathBuf>, u8),
    Rotated(usize, usize),
    Export(Vec<PathBuf>),
    ExportTo(Vec<PathBuf>, glide_sdk::chooser::Chosen),
    Exported(usize),
    AddFolder,
    FolderChosen(glide_sdk::chooser::Chosen),
    UsePictures,
    RemoveFolder(PathBuf),
    FolderMenu(PathBuf),
    Cards(Vec<(Card, usize)>),
    CopyCard(Card),
    CardCopied(usize),
    DismissCard(PathBuf),
    Eject(PathBuf),
    SavedPrefs,
    Tick,
    Viewer(ViewerEvent),
    Decoded(u64, Result<Box<Loaded>, Failure>),
    Exposed(u64, Handle),
    Editor(EditorEvent),
    EditSource(u64, Option<Source>),
    Rendered(u64, Box<Render>),
    Saved(Result<PathBuf, String>, bool),
    Caption(String),
    Ignore,
}

pub type Source = (Arc<RgbaImage>, Vec<(Filter, Handle)>);

#[derive(Debug, Clone)]
pub struct Render {
    pub preview: (Handle, (u32, u32)),
    pub framed: Option<(Handle, (u32, u32))>,
    pub bins: [[u32; edit::BINS]; 3],
}

#[derive(Debug, Clone, Copy)]
pub enum ViewerEvent {
    Back,
    Step(isize),
    Jump(usize),
    ToggleInfo,
    ZoomIn,
    ZoomOut,
    Fit,
    Fullscreen,
    Slideshow,
    TogglePlay,
    NextFrame,
    Page(u32),
    Exposure(f32),
    Backdrop(Backdrop),
    More,
    Copy,
    Edit,
}

#[derive(Debug, Clone)]
pub enum EditorEvent {
    Tool(Tool),
    Slider(Slider, i32),
    Release,
    Reset(Slider),
    Auto,
    ResetAll,
    Filter(Filter),
    Strength(i32),
    Ratio(Ratio),
    Portrait(bool),
    Turn(i8),
    FlipH,
    FlipV,
    Tilt(f32),
    Level,
    Crop(glide_sdk::widget::CropRect),
    ResetCrop,
    Undo,
    Redo,
    Compare(bool),
    Done,
    Cancel,
    SaveReplace(bool),
    SaveName(String),
    SaveOutput(Output),
    SaveConfirm,
    SaveDiscard,
    SaveCancel,
}

#[allow(clippy::struct_excessive_bools)] // Independent pieces of window state, each driven by its own message
pub struct App {
    pub lang: Lang,
    pub paint: Paint,
    pub window: Window,
    pub prefs: Prefs,
    pub photos: Vec<Photo>,
    pub scanned: bool,
    pub thumbs: HashMap<PathBuf, Option<Handle>>,
    pub queue: VecDeque<PathBuf>,
    pub working: usize,
    pub place: Place,
    pub scale: Scale,
    pub query: String,
    pub chip: Chip,
    pub scroll: f32,
    pub viewport: f32,
    pub selecting: bool,
    pub selected: BTreeSet<PathBuf>,
    pub anchor: Option<usize>,
    pub modifiers: keyboard::Modifiers,
    pub trash: Vec<Trashed>,
    pub cards: Vec<(Card, usize)>,
    pub copying: bool,
    pub viewer: Option<Viewer>,
    pub editor: Option<Editor>,
    pub modal: Option<Modal>,
    pub generation: u64,
    pub today: library::Date,
    pub now: u64,
    pub ticks: u32,
    pub grid_bounds: Option<iced::Rectangle>,
    pub morph: Option<Morph>,
}

#[derive(Debug, Clone)]
pub struct Morph {
    pub from: iced::Rectangle,
    pub to: iced::Rectangle,
    pub started: Instant,
    pub length: Duration,
    pub opening: bool,
    pub handle: Option<Handle>,
}

impl Morph {
    #[must_use]
    pub fn progress(&self, now: Instant) -> f32 {
        if self.length.is_zero() {
            return 1.0;
        }
        (now.saturating_duration_since(self.started).as_secs_f32() / self.length.as_secs_f32()).clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn rect(&self, progress: f32) -> iced::Rectangle {
        let eased = glide_sdk::theme::motion::ease_out(f64::from(progress)) as f32;
        let mix = |a: f32, b: f32| a + (b - a) * eased;
        iced::Rectangle {
            x: mix(self.from.x, self.to.x),
            y: mix(self.from.y, self.to.y),
            width: mix(self.from.width, self.to.width),
            height: mix(self.from.height, self.to.height),
        }
    }

    #[must_use]
    pub fn backdrop(&self, progress: f32) -> f32 {
        let eased = glide_sdk::theme::motion::ease_io(f64::from(progress)) as f32;
        if self.opening { eased } else { 1.0 - eased }
    }
}

async fn work<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    let spawned = std::thread::Builder::new().name("glide-photos-work".into()).spawn(move || {
        let _ = sender.send(job());
    });
    drop(spawned);
    receiver.await.ok()
}

fn later<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static, done: impl Fn(T) -> Message + Send + 'static,
) -> Task<Message> {
    Task::perform(work(job), move |result| result.map_or(Message::Ignore, &done))
}

#[must_use]
pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

#[must_use]
pub fn writable(photo: &Photo) -> bool {
    photo.format.family == Family::Raster
        && !photo.animated()
        && !photo.paged()
        && matches!(photo.format.name, "JPEG" | "PNG" | "WebP")
}

#[must_use]
pub fn loaded_from(path: PathBuf, decoded: formats::Decoded) -> Loaded {
    let camera = formats::exif(&path).map(|exif| formats::camera(&exif)).unwrap_or_default();
    let transparent = decoded.image.pixels().step_by(97).any(|pixel| pixel.0[3] < 250);
    let rgba = |image: RgbaImage| Handle::from_rgba(image.width(), image.height(), image.into_raw());
    let frames = decoded.frames.into_iter().map(|frame| (rgba(frame.image), frame.delay_ms)).collect();
    Loaded {
        handle: rgba(decoded.image),
        frames,
        size: decoded.size,
        pages: decoded.pages,
        note: decoded.note,
        hdr: decoded.hdr.map(Arc::new),
        camera,
        transparent,
        path,
    }
}

impl App {
    #[must_use]
    pub fn new(shell: &Shell<Message>) -> Self {
        Self {
            lang: shell.lang(),
            paint: *shell.paint(),
            window: *shell.window(),
            prefs: prefs::load(),
            photos: Vec::new(),
            scanned: false,
            thumbs: HashMap::new(),
            queue: VecDeque::new(),
            working: 0,
            place: Place::Library,
            scale: Scale::All,
            query: String::new(),
            chip: Chip::Everything,
            scroll: 0.0,
            viewport: 600.0,
            selecting: false,
            selected: BTreeSet::new(),
            anchor: None,
            modifiers: keyboard::Modifiers::default(),
            trash: Vec::new(),
            cards: Vec::new(),
            copying: false,
            viewer: None,
            editor: None,
            modal: None,
            generation: 0,
            today: library::today(),
            now: unix_now(),
            ticks: 0,
            grid_bounds: None,
            morph: None,
        }
    }

    #[must_use]
    pub fn t(&self, key: Key) -> &'static str {
        key.text(self.lang)
    }

    pub fn rescan(&self) -> Task<Message> {
        let roots = self.prefs.roots();
        Task::batch([later(move || library::scan(&roots), Message::Scanned), Self::refresh_trash(), Self::poll_cards()])
    }

    fn refresh_trash() -> Task<Message> {
        later(trash::list, Message::TrashList)
    }

    fn poll_cards() -> Task<Message> {
        later(
            || {
                let pictures = library::user_dir("XDG_PICTURES_DIR").unwrap_or_default();
                library::camera_cards()
                    .into_iter()
                    .map(|card| {
                        let fresh = card_fresh(&card, &pictures).len();
                        (card, fresh)
                    })
                    .collect()
            },
            Message::Cards,
        )
    }

    fn save_prefs(&self) -> Task<Message> {
        let prefs = self.prefs.clone();
        later(
            move || {
                if let Some(path) = prefs::path()
                    && let Err(error) = prefs::save_to(&path, &prefs)
                {
                    tracing::warn!("photos: could not save preferences: {error}");
                }
            },
            |()| Message::SavedPrefs,
        )
    }

    fn pump(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        while self.working < THUMB_WORKERS
            && let Some(path) = self.queue.pop_front()
        {
            if self.thumbs.contains_key(&path) {
                continue;
            }
            let Some(format) = formats::of(&path) else {
                continue;
            };
            self.working += 1;
            let done = path.clone();
            tasks.push(later(move || thumbs::make(&path, format), move |thumb| Message::Thumb(done.clone(), thumb)));
        }
        Task::batch(tasks)
    }

    fn want_thumbs(&mut self, paths: impl IntoIterator<Item = PathBuf>) -> Task<Message> {
        for path in paths {
            if !self.thumbs.contains_key(&path) && !self.queue.contains(&path) {
                self.queue.push_back(path);
            }
        }
        self.pump()
    }

    #[must_use]
    pub fn thumb(&self, path: &Path) -> Option<Handle> {
        self.thumbs.get(path).cloned().flatten()
    }

    #[must_use]
    pub fn base(&self) -> Vec<usize> {
        let now = self.now;
        let all = 0..self.photos.len();
        match &self.place {
            Place::Library | Place::Albums | Place::Trash => all.collect(),
            Place::Favourites => all.filter(|&i| self.prefs.is_favourite(&self.photos[i].path)).collect(),
            Place::Album(index) => {
                self.prefs.albums.get(*index).map_or_else(Vec::new, |album| {
                    all.filter(|&i| album.photos.contains(&self.photos[i].path)).collect()
                })
            }
            Place::Smart(smart) => all.filter(|&i| smart.holds(&self.photos[i], now)).collect(),
            Place::Folder(root) => all.filter(|&i| self.photos[i].path.starts_with(root)).collect(),
            Place::Card(dcim) => all.filter(|&i| self.photos[i].path.starts_with(dcim)).collect(),
        }
    }

    #[must_use]
    pub fn shown(&self) -> Vec<usize> {
        let base = self.base();
        let filtered: Vec<usize> = base
            .into_iter()
            .filter(|&i| {
                let photo = &self.photos[i];
                match self.chip {
                    Chip::Everything => true,
                    Chip::Favourites => self.prefs.is_favourite(&photo.path),
                    Chip::Kind(smart) => smart.holds(photo, self.now),
                    Chip::ThisYear => photo.date.year == self.today.year,
                }
            })
            .collect();
        let captions = |path: &Path| self.prefs.captions.get(path).cloned();
        library::search(&self.photos, &filtered, &self.query, &captions)
    }

    #[must_use]
    pub fn searching(&self) -> bool {
        !self.query.trim().is_empty() || self.chip != Chip::Everything
    }

    #[must_use]
    pub fn content_width(&self) -> f32 {
        (self.window.size.width - crate::view::SIDEBAR - 1.0 - 2.0 * glide_sdk::theme::SP5).max(200.0)
    }

    #[must_use]
    pub fn grid(&self) -> (usize, f32) {
        let width = self.content_width();
        let target = ZOOM_SIDES[self.prefs.zoom.min(ZOOM_SIDES.len() - 1)];
        let columns = ((width + crate::view::GAP) / (target + crate::view::GAP)).floor().max(1.0) as usize;
        let side = ((width - crate::view::GAP * (columns as f32 - 1.0)) / columns as f32).floor();
        (columns, side)
    }

    #[must_use]
    pub fn album_name(&self, index: usize) -> &str {
        self.prefs.albums.get(index).map_or("", |album| album.name.as_str())
    }

    fn targets(&self, index: usize) -> Vec<PathBuf> {
        let path = self.photos[index].path.clone();
        if self.selecting && self.selected.contains(&path) {
            self.selected.iter().cloned().collect()
        } else {
            vec![path]
        }
    }

    fn grid_origin(&self) -> iced::Point {
        self.grid_bounds
            .map_or(iced::Point::new(crate::view::SIDEBAR + 1.0, crate::view::HEADER), |bounds| bounds.position())
    }

    #[must_use]
    pub fn tile_rect(&self, index: usize, pointer: Option<iced::Point>) -> Option<iced::Rectangle> {
        let grid = self.searching() || self.scale == Scale::All;
        if !grid || matches!(self.place, Place::Albums | Place::Trash) {
            return None;
        }
        let shown = self.shown();
        let (columns, side) = self.grid();
        let grouped = !self.searching();
        let groups = if grouped {
            library::by_day(&self.photos, &shown)
        } else {
            vec![library::Group { date: library::Date::default(), items: shown }]
        };
        let head = if grouped { crate::view::GROUP_HEAD } else { 0.0 };
        let step = side + crate::view::GAP;
        let origin = self.grid_origin();
        let mut top = 0.0;
        for group in &groups {
            let Some(position) = group.items.iter().position(|&i| i == index) else {
                top += crate::view::group_height(group.items.len(), columns, side) - (crate::view::GROUP_HEAD - head);
                continue;
            };
            let (row, column) = (position / columns, position % columns);
            let mut rect = iced::Rectangle {
                x: origin.x + glide_sdk::theme::SP5 + column as f32 * step,
                y: origin.y + top + head + row as f32 * step - self.scroll,
                width: side,
                height: side,
            };
            if let Some(point) = pointer.filter(|point| point.y < rect.y || point.y > rect.y + side) {
                rect.y = point.y - side / 2.0;
            }
            return Some(rect);
        }
        None
    }

    #[must_use]
    pub fn image_rect(&self, size: (u32, u32), zoom: Option<f32>, info: bool, chrome: bool) -> iced::Rectangle {
        let width = self.window.size.width - if info { glide_sdk::widget::INSPECTOR_WIDTH + 1.0 } else { 0.0 };
        let (top, height) = if chrome {
            (crate::view::HEADER, self.window.size.height - crate::view::HEADER - crate::view::FILMSTRIP)
        } else {
            (0.0, self.window.size.height)
        };
        let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
        let margin = 2.0 * crate::view::STAGE_MARGIN;
        let fit = ((width - margin) / w).min((height - margin) / h).min(1.0);
        let scale = zoom.unwrap_or(fit);
        let (shown_w, shown_h) = (w * scale, h * scale);
        iced::Rectangle {
            x: ((width - shown_w) / 2.0).max(0.0),
            y: top + ((height - shown_h) / 2.0).max(0.0),
            width: shown_w.min(width),
            height: shown_h.min(height),
        }
    }

    fn start_opening(&mut self, index: usize, from: Option<iced::Rectangle>) {
        let length = glide_sdk::theme::motion::t4();
        let Some(from) = from.filter(|_| !length.is_zero()) else {
            return;
        };
        let photo = &self.photos[index];
        let size = photo.dims.unwrap_or((3, 2));
        let to = self.image_rect(size, None, false, true);
        self.morph =
            Some(Morph { from, to, started: Instant::now(), length, opening: true, handle: self.thumb(&photo.path) });
    }

    fn close_viewer(&mut self) -> Task<Message> {
        let length = glide_sdk::theme::motion::t4();
        let Some(viewer) = self.viewer.as_ref() else {
            return Task::none();
        };
        let Some(index) = viewer.current() else {
            self.viewer = None;
            return Task::none();
        };
        if length.is_zero() {
            self.viewer = None;
            return Task::none();
        }
        let chrome = viewer.chrome || viewer.info;
        let (size, handle) = viewer.loaded.as_ref().map_or_else(
            || (self.photos[index].dims.unwrap_or((3, 2)), self.thumb(&self.photos[index].path)),
            |loaded| {
                let frame = loaded.frames.get(viewer.frame).map_or_else(|| loaded.handle.clone(), |(f, _)| f.clone());
                (loaded.size, Some(frame))
            },
        );
        let from = self.image_rect(size, viewer.zoom, viewer.info, chrome);
        let mut scroll = Task::none();
        let origin = self.grid_origin();
        if let Some(rect) = self.tile_rect(index, None) {
            let bottom = origin.y + self.viewport;
            if rect.y < origin.y || rect.y + rect.height > bottom {
                let target = (self.scroll + rect.y - origin.y - (self.viewport - rect.height) / 2.0).max(0.0);
                self.scroll = target;
                scroll = iced::widget::operation::scroll_to(
                    crate::view::GRID_ID,
                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: target },
                );
            }
        }
        let to = self.tile_rect(index, None).unwrap_or_else(|| {
            let shrink = 0.9;
            iced::Rectangle {
                x: from.x + from.width * (1.0 - shrink) / 2.0,
                y: from.y + from.height * (1.0 - shrink) / 2.0,
                width: from.width * shrink,
                height: from.height * shrink,
            }
        });
        self.morph = Some(Morph { from, to, started: Instant::now(), length, opening: false, handle });
        scroll
    }

    fn open(&mut self, index: usize) -> Task<Message> {
        let list = self.shown();
        let at = list.iter().position(|&i| i == index).unwrap_or(0);
        let list = if list.is_empty() { vec![index] } else { list };
        self.viewer = Some(Viewer {
            list,
            at,
            loaded: None,
            failure: None,
            frame: 0,
            playing: true,
            page: 0,
            exposure: 0.0,
            zoom: None,
            info: false,
            chrome: true,
            moved: Instant::now(),
            pointer: iced::Point::ORIGIN,
            slideshow: false,
            backdrop: Backdrop::Checks,
            generation: 0,
        });
        self.load_current()
    }

    fn load_current(&mut self) -> Task<Message> {
        self.generation += 1;
        let generation = self.generation;
        let Some(viewer) = self.viewer.as_mut() else {
            return Task::none();
        };
        viewer.generation = generation;
        viewer.failure = None;
        viewer.frame = 0;
        viewer.zoom = None;
        let Some(index) = viewer.current() else {
            return Task::none();
        };
        let photo = self.photos[index].clone();
        let page = viewer.page;
        let strip: Vec<PathBuf> = viewer
            .list
            .iter()
            .skip(viewer.at.saturating_sub(12))
            .take(25)
            .map(|&i| self.photos[i].path.clone())
            .collect();
        let decode = later(
            move || {
                let request =
                    Request { path: &photo.path, format: photo.format, max_side: Some(VIEW_SIDE), page, animate: true };
                formats::decode(&request).map(|decoded| Box::new(loaded_from(photo.path.clone(), decoded)))
            },
            move |result| Message::Decoded(generation, result),
        );
        Task::batch([decode, self.want_thumbs(strip)])
    }

    fn open_editor(&mut self) -> Task<Message> {
        let Some(index) = self.viewer.as_ref().and_then(Viewer::current) else {
            return Task::none();
        };
        self.generation += 1;
        let generation = self.generation;
        let edit = Edit::default();
        self.editor = Some(Editor {
            photo: index,
            source: None,
            original: None,
            edit,
            history: vec![edit],
            at: 0,
            tool: Tool::Adjust,
            preview: None,
            geometry: None,
            histogram: [[0; edit::BINS]; 3],
            compare: false,
            generation,
            swatches: Vec::new(),
            saving: None,
            busy: true,
        });
        let photo = self.photos[index].clone();
        let page = self.viewer.as_ref().map_or(0, |viewer| viewer.page);
        later(
            move || {
                let request = Request {
                    path: &photo.path,
                    format: photo.format,
                    max_side: Some(EDIT_SIDE),
                    page,
                    animate: false,
                };
                let decoded = formats::decode(&request).ok()?;
                let source = Arc::new(decoded.image);
                let small = image::DynamicImage::ImageRgba8((*source).clone()).thumbnail(160, 160).into_rgba8();
                let swatches = Filter::ALL
                    .iter()
                    .map(|&filter| {
                        let image = edit::tone(&small, &Edit { filter, ..Edit::default() });
                        (filter, Handle::from_rgba(image.width(), image.height(), image.into_raw()))
                    })
                    .collect();
                Some((source, swatches))
            },
            move |result| Message::EditSource(generation, result),
        )
    }

    fn render(&mut self) -> Task<Message> {
        let Some(editor) = self.editor.as_mut() else {
            return Task::none();
        };
        let Some(source) = editor.source.clone() else {
            return Task::none();
        };
        self.generation += 1;
        editor.generation = self.generation;
        let generation = self.generation;
        let edit = editor.edit;
        let cropping = editor.tool == Tool::Crop;
        later(
            move || {
                let rgba = |image: RgbaImage| {
                    let size = image.dimensions();
                    (Handle::from_rgba(size.0, size.1, image.into_raw()), size)
                };
                let result = edit::apply(&source, &edit);
                let bins = edit::histogram(&result);
                let framed = cropping.then(|| {
                    let uncut = Edit { crop: edit::Crop::default(), ..edit };
                    rgba(edit::apply(&source, &uncut))
                });
                Render { preview: rgba(result), framed, bins }
            },
            move |render| Message::Rendered(generation, Box::new(render)),
        )
    }

    fn toast_template(shell: &mut Shell<Message>, template: &str, n: usize) {
        shell.toast(template.replace("{n}", &n.to_string()));
    }

    fn tile_menu(&self, index: usize) -> Vec<Item<Message>> {
        let targets = self.targets(index);
        let photo = &self.photos[index];
        let favourite = targets.iter().all(|path| self.prefs.is_favourite(path));
        let mut items = Vec::new();
        if targets.len() > 1 {
            items.push(Item::heading(i18n::photos(self.lang, targets.len())));
        }
        items.push(Item::action(self.t(Key::Open), Message::Open(index)));
        items.push(Item::action(self.t(Key::AddToAlbum), Message::AlbumMenu(targets.clone())));
        items.push(Item::action(
            self.t(if favourite { Key::UnmarkFavourite } else { Key::MarkFavourite }),
            Message::Favourite(targets.clone(), !favourite),
        ));
        if let Place::Album(album) = self.place {
            items.push(Item::action(self.t(Key::RemoveFromAlbum), Message::RemoveFromAlbum(album, targets.clone())));
        }
        items.push(Item::separator());
        items.push(Item::action(self.t(Key::RotateLeft), Message::Rotate(targets.clone(), 3)).shortcut("Ctrl+["));
        items.push(Item::action(self.t(Key::Export), Message::Export(targets.clone())).shortcut("Ctrl+E"));
        items.push(Item::action(self.t(Key::ShowInFiles), Message::Reveal(photo.path.clone())));
        items.push(Item::separator());
        items.push(Item::action(self.t(Key::MoveToTrash), Message::Trash(targets)).danger().shortcut("Delete"));
        items
    }

    fn album_menu(&self, paths: &[PathBuf]) -> Vec<Item<Message>> {
        let mut items: Vec<Item<Message>> = self
            .prefs
            .albums
            .iter()
            .enumerate()
            .map(|(index, album)| Item::action(album.name.clone(), Message::AddTo(index, paths.to_vec())))
            .collect();
        if !items.is_empty() {
            items.push(Item::separator());
        }
        items.push(Item::action(self.t(Key::NewAlbumDots), Message::AskNewAlbum(paths.to_vec())));
        items
    }

    fn viewer_menu(&self) -> Vec<Item<Message>> {
        let Some(viewer) = &self.viewer else {
            return Vec::new();
        };
        let Some(index) = viewer.current() else {
            return Vec::new();
        };
        let path = self.photos[index].path.clone();
        let rotate = writable(&self.photos[index]);
        vec![
            Item::action(self.t(Key::RotateLeft), Message::Rotate(vec![path.clone()], 3))
                .shortcut("Ctrl+[")
                .enabled(rotate),
            Item::action(self.t(Key::RotateRight), Message::Rotate(vec![path.clone()], 1))
                .shortcut("Ctrl+]")
                .enabled(rotate),
            Item::action(
                self.t(if viewer.slideshow { Key::StopSlideshow } else { Key::Slideshow }),
                Message::Viewer(ViewerEvent::Slideshow),
            )
            .shortcut("F5"),
            Item::separator(),
            Item::action(self.t(Key::AddToAlbum), Message::AlbumMenu(vec![path.clone()])),
            Item::action(self.t(Key::ShowInFiles), Message::Reveal(path.clone())),
            Item::action(self.t(Key::Copy), Message::Viewer(ViewerEvent::Copy)).shortcut("Ctrl+C"),
            Item::separator(),
            Item::action(self.t(Key::MoveToTrash), Message::Trash(vec![path])).danger().shortcut("Delete"),
        ]
    }

    fn replace_photos(&mut self, mut photos: Vec<Photo>) {
        let viewing: Option<(Vec<PathBuf>, usize)> = self
            .viewer
            .as_ref()
            .map(|viewer| (viewer.list.iter().map(|&i| self.photos[i].path.clone()).collect(), viewer.at));
        let editing = self.editor.as_ref().map(|editor| self.photos[editor.photo].path.clone());
        if let Some((paths, _)) = &viewing {
            for path in paths {
                if !photos.iter().any(|photo| &photo.path == path)
                    && let Some(kept) = self.photos.iter().find(|photo| &photo.path == path && path.is_file())
                {
                    photos.push(kept.clone());
                }
            }
        }
        library::sort(&mut photos);
        self.photos = photos;
        let find = |photos: &[Photo], path: &PathBuf| photos.iter().position(|photo| &photo.path == path);
        if let Some((paths, at)) = viewing {
            let current = paths.get(at).cloned();
            let list: Vec<usize> = paths.iter().filter_map(|path| find(&self.photos, path)).collect();
            if list.is_empty() {
                self.viewer = None;
            } else if let Some(viewer) = self.viewer.as_mut() {
                let here = current.and_then(|path| find(&self.photos, &path));
                viewer.at = here.and_then(|index| list.iter().position(|&i| i == index)).unwrap_or(0);
                viewer.list = list;
            }
        }
        if let Some(path) = editing {
            match find(&self.photos, &path) {
                Some(index) => {
                    if let Some(editor) = self.editor.as_mut() {
                        editor.photo = index;
                    }
                }
                None => self.editor = None,
            }
        }
    }

    fn forget_paths(&mut self, gone: &[PathBuf]) {
        let listed: Option<(Vec<PathBuf>, usize)> = self
            .viewer
            .as_ref()
            .map(|viewer| (viewer.list.iter().map(|&i| self.photos[i].path.clone()).collect(), viewer.at));
        self.photos.retain(|photo| !gone.contains(&photo.path));
        for path in gone {
            self.prefs.forget(path);
            self.selected.remove(path);
            self.thumbs.remove(path);
        }
        let Some((paths, at)) = listed else {
            return;
        };
        let kept_before = paths[..at.min(paths.len())].iter().filter(|path| !gone.contains(path)).count();
        let list: Vec<usize> = paths
            .iter()
            .filter(|path| !gone.contains(path))
            .filter_map(|path| self.photos.iter().position(|photo| &photo.path == path))
            .collect();
        if list.is_empty() {
            self.viewer = None;
        } else if let Some(viewer) = self.viewer.as_mut() {
            viewer.at = kept_before.min(list.len() - 1);
            viewer.list = list;
        }
    }

    #[allow(clippy::too_many_lines)] // One arm per message; each is short
    pub fn update(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::Window(action) => shell.window_action(action),
            Message::Close => shell.exit(),
            Message::Scanned(photos) => {
                self.replace_photos(photos);
                self.scanned = true;
                self.now = unix_now();
                self.today = library::today();
                let paths: Vec<PathBuf> = self.photos.iter().map(|photo| photo.path.clone()).collect();
                return self.want_thumbs(paths);
            }
            Message::CardScanned(found) => {
                let fresh: Vec<Photo> = found
                    .into_iter()
                    .filter(|photo| !self.photos.iter().any(|known| known.path == photo.path))
                    .collect();
                let paths: Vec<PathBuf> = fresh.iter().map(|photo| photo.path.clone()).collect();
                let mut all = std::mem::take(&mut self.photos);
                all.extend(fresh);
                self.replace_photos(all);
                return self.want_thumbs(paths);
            }
            Message::Thumb(path, thumb) => {
                self.working = self.working.saturating_sub(1);
                let handle = match thumb {
                    Thumb::Ready(file) => Some(Handle::from_path(file)),
                    Thumb::Failed => None,
                };
                self.thumbs.insert(path, handle);
                return self.pump();
            }
            Message::Go(place) => {
                self.place = place;
                self.scroll = 0.0;
                self.selecting = false;
                self.selected.clear();
                if self.place == Place::Trash {
                    return Self::refresh_trash();
                }
                if let Place::Card(dcim) = &self.place {
                    let dcim = dcim.clone();
                    let offset = library::utc_offset();
                    return later(
                        move || {
                            let mut found = Vec::new();
                            card_walk(&dcim, offset, &mut found);
                            found
                        },
                        Message::CardScanned,
                    );
                }
            }
            Message::Scale(scale) => {
                self.scale = scale;
                self.scroll = 0.0;
            }
            Message::Query(query) => {
                self.query = query;
                self.scroll = 0.0;
            }
            Message::ClearQuery => {
                self.query.clear();
                self.chip = Chip::Everything;
            }
            Message::Chip(chip) => self.chip = chip,
            Message::Zoom(level) => {
                self.prefs.zoom = level.min(ZOOM_SIDES.len() - 1);
                return self.save_prefs();
            }
            Message::Scrolled(bounds, offset) => {
                self.scroll = offset;
                self.viewport = bounds.height;
                self.grid_bounds = Some(bounds);
            }
            Message::Frame(now) => {
                if let Some(morph) = &self.morph
                    && morph.progress(now) >= 1.0
                {
                    let closing = !morph.opening;
                    self.morph = None;
                    if closing {
                        self.viewer = None;
                    }
                }
            }
            Message::StartSelect => self.selecting = true,
            Message::EndSelect => {
                self.selecting = false;
                self.selected.clear();
            }
            Message::SelectAll => {
                self.selecting = true;
                self.selected = self.shown().into_iter().map(|i| self.photos[i].path.clone()).collect();
            }
            Message::SelectDay(group) => {
                let shown = self.shown();
                if let Some(day) = library::by_day(&self.photos, &shown).get(group) {
                    let paths: Vec<PathBuf> = day.items.iter().map(|&i| self.photos[i].path.clone()).collect();
                    let all = paths.iter().all(|path| self.selected.contains(path));
                    for path in paths {
                        if all {
                            self.selected.remove(&path);
                        } else {
                            self.selected.insert(path);
                        }
                    }
                    self.selecting = true;
                }
            }
            Message::Tile(index) => {
                if self.selecting || self.modifiers.control() || self.modifiers.shift() {
                    self.selecting = true;
                    let shown = self.shown();
                    let here = shown.iter().position(|&i| i == index);
                    if self.modifiers.shift()
                        && let (Some(anchor), Some(here)) =
                            (self.anchor.and_then(|a| shown.iter().position(|&i| i == a)), here)
                    {
                        let (low, high) = (anchor.min(here), anchor.max(here));
                        for &i in &shown[low..=high] {
                            self.selected.insert(self.photos[i].path.clone());
                        }
                    } else {
                        let path = self.photos[index].path.clone();
                        if !self.selected.remove(&path) {
                            self.selected.insert(path);
                        }
                        self.anchor = Some(index);
                    }
                } else {
                    let from = self.tile_rect(index, Some(shell.pointer()));
                    let task = self.open(index);
                    self.start_opening(index, from);
                    return task;
                }
            }
            Message::TileMenu(index) => shell.context_menu(self.tile_menu(index)),
            Message::Open(index) => {
                shell.close_menu();
                return self.open(index);
            }
            Message::PeriodPressed(year, month) => {
                let shown = self.shown();
                let target = shown.iter().position(|&i| {
                    let date = self.photos[i].date;
                    date.year == year && month.is_none_or(|m| date.month == m)
                });
                self.scale = if month.is_some() || self.scale == Scale::Months { Scale::All } else { Scale::Months };
                self.scroll = 0.0;
                if let (Some(position), Scale::All) = (target, self.scale) {
                    let (columns, side) = self.grid();
                    let mut top = 0.0;
                    for group in library::by_day(&self.photos, &shown) {
                        if group.items.iter().any(|&i| i == shown[position]) {
                            break;
                        }
                        top += crate::view::group_height(group.items.len(), columns, side);
                    }
                    self.scroll = top;
                    return iced::widget::operation::scroll_to(
                        crate::view::GRID_ID,
                        iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: top },
                    );
                }
            }
            Message::Favourite(paths, on) => {
                for path in &paths {
                    self.prefs.set_favourite(path, on);
                }
                shell.close_menu();
                return self.save_prefs();
            }
            Message::AlbumMenu(paths) => shell.context_menu(self.album_menu(&paths)),
            Message::AddTo(album, paths) => {
                self.prefs.add_to_album(album, &paths);
                shell.close_menu();
                shell.toast(self.t(Key::AddedToAlbum).replace("{a}", self.album_name(album)));
                self.selecting = false;
                self.selected.clear();
                return self.save_prefs();
            }
            Message::AskNewAlbum(adding) => {
                shell.close_menu();
                self.modal = Some(Modal::NewAlbum { name: String::new(), adding });
                return iced::widget::operation::focus(crate::view::ALBUM_FIELD);
            }
            Message::AlbumName(text) => {
                if let Some(Modal::NewAlbum { name, .. }) = self.modal.as_mut() {
                    *name = text;
                }
            }
            Message::CreateAlbum => {
                if let Some(Modal::NewAlbum { name, adding }) = self.modal.take() {
                    let name = name.trim().to_string();
                    if name.is_empty() {
                        self.modal = Some(Modal::NewAlbum { name, adding });
                        return Task::none();
                    }
                    self.prefs.albums.push(Album { name, photos: Vec::new() });
                    let index = self.prefs.albums.len() - 1;
                    if !adding.is_empty() {
                        self.prefs.add_to_album(index, &adding);
                        shell.toast(self.t(Key::AddedToAlbum).replace("{a}", self.album_name(index)));
                        self.selecting = false;
                        self.selected.clear();
                    }
                    return self.save_prefs();
                }
            }
            Message::CancelModal => self.modal = None,
            Message::DeleteAlbumAsk(index) => {
                shell.context_menu(vec![Item::action(self.t(Key::DeleteAlbum), Message::DeleteAlbum(index)).danger()]);
            }
            Message::DeleteAlbum(index) => {
                shell.close_menu();
                if index < self.prefs.albums.len() {
                    self.prefs.albums.remove(index);
                    if matches!(self.place, Place::Album(_)) {
                        self.place = Place::Albums;
                    }
                    return self.save_prefs();
                }
            }
            Message::RemoveFromAlbum(index, paths) => {
                shell.close_menu();
                if let Some(album) = self.prefs.albums.get_mut(index) {
                    album.photos.retain(|path| !paths.contains(path));
                    return self.save_prefs();
                }
            }
            Message::Trash(paths) => {
                shell.close_menu();
                let count = paths.len();
                return later(
                    move || paths.iter().filter_map(|path| trash::put(path).ok()).collect::<Vec<_>>(),
                    move |done| Message::Trashed(done, count),
                );
            }
            Message::Trashed(done, asked) => {
                if done.len() < asked {
                    shell.toast(self.t(Key::TrashFailed));
                }
                if !done.is_empty() {
                    let gone: Vec<PathBuf> = done.iter().map(|item| item.original.clone()).collect();
                    self.forget_paths(&gone);
                    let text = self.t(Key::Trashed).replace("{n}", &done.len().to_string());
                    shell.toast_with(text, self.t(Key::Undo), Message::UndoTrash(done));
                    self.selecting = false;
                    let load = if self.viewer.is_some() { self.load_current() } else { Task::none() };
                    return Task::batch([load, Self::refresh_trash(), self.save_prefs()]);
                }
            }
            Message::UndoTrash(items) => {
                return later(
                    move || {
                        for item in &items {
                            let _ = trash::restore(item);
                        }
                    },
                    |()| Message::Rescan,
                );
            }
            Message::Rescan => return self.rescan(),
            Message::TrashList(items) => {
                let files: Vec<PathBuf> = items.iter().map(|item| item.file.clone()).collect();
                self.trash = items;
                return self.want_thumbs(files);
            }
            Message::TrashMenu(item) => shell.context_menu(vec![
                Item::action(self.t(Key::Restore), Message::Restore(item.clone())),
                Item::separator(),
                Item::action(self.t(Key::DeleteForever), Message::Purge(item)).danger(),
            ]),
            Message::Purge(item) => {
                shell.close_menu();
                return later(move || trash::purge(&item).is_ok(), |_| Message::TrashRefresh);
            }
            Message::TrashRefresh => return Self::refresh_trash(),
            Message::Restore(item) => {
                return later(
                    move || trash::restore(&item).is_ok(),
                    |restored| {
                        if restored { Message::Rescan } else { Message::Ignore }
                    },
                );
            }
            Message::AskEmptyTrash => shell.dialog(
                Dialog::new(self.t(Key::EmptyTrashAsk), self.t(Key::EmptyTrashNote))
                    .choice(self.t(Key::DeleteForever), Tone::Danger, Message::EmptyTrash)
                    .cancel(self.t(Key::Cancel)),
            ),
            Message::EmptyTrash => {
                let items = std::mem::take(&mut self.trash);
                return later(
                    move || {
                        for item in &items {
                            let _ = trash::purge(item);
                        }
                    },
                    |()| Message::Ignore,
                )
                .chain(Self::refresh_trash());
            }
            Message::Reveal(path) => {
                shell.close_menu();
                let _ = glide_sdk::system::reveal(&path, crate::APP_ID);
            }
            Message::Rotate(paths, turns) => {
                shell.close_menu();
                let writable: Vec<PathBuf> = paths
                    .iter()
                    .filter(|path| self.photos.iter().any(|photo| &photo.path == *path && writable(photo)))
                    .cloned()
                    .collect();
                let asked = paths.len();
                return later(
                    move || {
                        let mut done = 0;
                        for path in &writable {
                            let Some(output) = Output::of(path) else { continue };
                            let Ok(image) = image::open(path) else { continue };
                            let image = formats::oriented(image, formats::orientation(path)).into_rgba8();
                            let turned = edit::geometry(&image, &Edit { turns: turns % 4, ..Edit::default() });
                            if edit::write(&turned, path, output).is_ok() {
                                done += 1;
                            }
                        }
                        done
                    },
                    move |done| Message::Rotated(done, asked),
                );
            }
            Message::Rotated(done, asked) => {
                shell.toast(self.t(if done < asked { Key::RotateFailed } else { Key::Rotated }));
                self.thumbs.clear();
                return Task::batch([self.rescan(), self.load_current()]);
            }
            Message::Export(paths) => {
                shell.close_menu();
                let ask = glide_sdk::chooser::Ask {
                    purpose: glide_sdk::chooser::Purpose::Folder,
                    title: self.t(Key::ExportTitle).into(),
                    accept: self.t(Key::ChooseFolder).into(),
                    name: None,
                    folder: None,
                };
                let receiver = glide_sdk::chooser::spawn(ask);
                return Task::perform(async move { receiver.await.ok() }, move |chosen| {
                    chosen.map_or(Message::Ignore, |chosen| Message::ExportTo(paths.clone(), chosen))
                });
            }
            Message::ExportTo(paths, glide_sdk::chooser::Chosen::Paths(folders)) => {
                if let Some(folder) = folders.into_iter().next() {
                    return later(
                        move || {
                            paths
                                .iter()
                                .filter(|path| {
                                    path.file_name()
                                        .is_some_and(|name| std::fs::copy(path, unique(&folder.join(name))).is_ok())
                                })
                                .count()
                        },
                        Message::Exported,
                    );
                }
            }
            Message::Exported(n) => Self::toast_template(shell, self.t(Key::Exported), n),
            Message::AddFolder => {
                let ask = glide_sdk::chooser::Ask {
                    purpose: glide_sdk::chooser::Purpose::Folder,
                    title: self.t(Key::AddFolder).trim_end_matches('…').into(),
                    accept: self.t(Key::ChooseFolder).into(),
                    name: None,
                    folder: glide_config::home(),
                };
                let receiver = glide_sdk::chooser::spawn(ask);
                return Task::perform(async move { receiver.await.ok() }, |chosen| {
                    chosen.map_or(Message::Ignore, Message::FolderChosen)
                });
            }
            Message::FolderChosen(glide_sdk::chooser::Chosen::Paths(folders)) => {
                if self.prefs.folders.is_empty() {
                    self.prefs.folders = self.prefs.roots();
                }
                for folder in folders {
                    if !self.prefs.folders.contains(&folder) {
                        self.prefs.folders.push(folder);
                    }
                }
                return Task::batch([self.save_prefs(), self.rescan()]);
            }
            Message::UsePictures => {
                if let Some(pictures) = library::user_dir("XDG_PICTURES_DIR") {
                    let _ = std::fs::create_dir_all(&pictures);
                    if !self.prefs.folders.contains(&pictures) {
                        self.prefs.folders.push(pictures);
                    }
                }
                return Task::batch([self.save_prefs(), self.rescan()]);
            }
            Message::RemoveFolder(folder) => {
                shell.close_menu();
                if self.prefs.folders.is_empty() {
                    self.prefs.folders = self.prefs.roots();
                }
                self.prefs.folders.retain(|known| known != &folder);
                if self.place == Place::Folder(folder) {
                    self.place = Place::Library;
                }
                return Task::batch([self.save_prefs(), self.rescan()]);
            }
            Message::FolderMenu(folder) => shell.context_menu(vec![
                Item::action(self.t(Key::ShowInFiles), Message::Reveal(folder.clone())),
                Item::action(self.t(Key::RemoveFolder), Message::RemoveFolder(folder)).danger(),
            ]),
            Message::Cards(cards) => self.cards = cards,
            Message::CopyCard(card) => {
                self.copying = true;
                shell.toast(self.t(Key::Copying));
                return later(
                    move || {
                        let pictures = library::user_dir("XDG_PICTURES_DIR").unwrap_or_default();
                        let target = pictures.join(&card.label);
                        let _ = std::fs::create_dir_all(&target);
                        card_fresh(&card, &pictures)
                            .iter()
                            .filter(|path| {
                                path.file_name().is_some_and(|name| std::fs::copy(path, target.join(name)).is_ok())
                            })
                            .count()
                    },
                    Message::CardCopied,
                );
            }
            Message::CardCopied(n) => {
                self.copying = false;
                Self::toast_template(shell, self.t(Key::Copied), n);
                return self.rescan();
            }
            Message::DismissCard(mount) => {
                self.prefs.dismissed_cards.push(mount);
                return self.save_prefs();
            }
            Message::Eject(mount) => {
                let mut command = std::process::Command::new("udisksctl");
                command.arg("unmount").arg("-b").arg(mount_device(&mount).unwrap_or_default());
                let _ = glide_sdk::system::spawn_detached(command, crate::APP_ID);
                return Self::poll_cards();
            }
            Message::SavedPrefs | Message::Ignore | Message::ExportTo(..) | Message::FolderChosen(_) => {}
            Message::Tick => return self.tick(shell),
            Message::Viewer(event) => return self.viewer_event(event, shell),
            Message::Decoded(generation, result) => {
                if let Some(viewer) = self.viewer.as_mut().filter(|viewer| viewer.generation == generation) {
                    match result {
                        Ok(loaded) => {
                            if loaded.pages > 1 {
                                viewer.page = viewer.page.min(loaded.pages - 1);
                            }
                            viewer.exposure = 0.0;
                            viewer.loaded = Some(*loaded);
                        }
                        Err(failure) => {
                            viewer.loaded = None;
                            viewer.failure = Some(failure);
                        }
                    }
                }
            }
            Message::Exposed(generation, handle) => {
                if let Some(loaded) = self
                    .viewer
                    .as_mut()
                    .filter(|viewer| viewer.generation == generation)
                    .and_then(|viewer| viewer.loaded.as_mut())
                {
                    loaded.handle = handle;
                }
            }
            Message::Editor(event) => return self.editor_event(event),
            Message::EditSource(generation, result) => {
                if let Some(editor) = self.editor.as_mut().filter(|editor| editor.generation == generation) {
                    editor.busy = false;
                    if let Some((source, swatches)) = result {
                        editor.original =
                            Some(Handle::from_rgba(source.width(), source.height(), source.as_raw().clone()));
                        editor.source = Some(source);
                        editor.swatches = swatches;
                        return self.render();
                    }
                    self.editor = None;
                    shell.toast(self.t(Key::CannotOpen));
                }
            }
            Message::Rendered(generation, render) => {
                if let Some(editor) = self.editor.as_mut().filter(|editor| editor.generation == generation) {
                    let Render { preview, framed, bins } = *render;
                    editor.preview = Some(preview);
                    if framed.is_some() {
                        editor.geometry = framed;
                    }
                    editor.histogram = bins;
                }
            }
            Message::Saved(result, replaced) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.busy = false;
                }
                match result {
                    Ok(path) => {
                        self.editor = None;
                        let key = if replaced { Key::Saved } else { Key::SavedCopy };
                        shell.toast_with(self.t(key), self.t(Key::Show), Message::Reveal(path.clone()));
                        self.thumbs.remove(&path);
                        let offset = library::utc_offset();
                        if let Some(photo) = library::read_photo(&path, offset) {
                            if let Some(existing) = self.photos.iter_mut().find(|known| known.path == path) {
                                *existing = photo;
                            } else {
                                self.photos.push(photo);
                                library::sort(&mut self.photos);
                                if let Some(position) = self.photos.iter().position(|known| known.path == path)
                                    && let Some(viewer) = self.viewer.as_mut()
                                {
                                    viewer.list = (0..self.photos.len()).collect();
                                    viewer.at = position;
                                }
                            }
                        }
                        return Task::batch([self.want_thumbs([path]), self.load_current()]);
                    }
                    Err(error) => shell.toast(self.t(Key::SaveFailed).replace("{e}", &error)),
                }
            }
            Message::Caption(text) => {
                if let Some(index) = self.viewer.as_ref().and_then(Viewer::current) {
                    let path = self.photos[index].path.clone();
                    if text.trim().is_empty() {
                        self.prefs.captions.remove(&path);
                    } else {
                        self.prefs.captions.insert(path, text);
                    }
                    return self.save_prefs();
                }
            }
        }
        Task::none()
    }

    fn tick(&mut self, shell: &Shell<Message>) -> Task<Message> {
        let now = Instant::now();
        let mut tasks = Vec::new();
        if let Some(viewer) = self.viewer.as_mut() {
            let pointer = shell.pointer();
            if pointer != viewer.pointer {
                viewer.pointer = pointer;
                viewer.moved = now;
                viewer.chrome = !viewer.slideshow;
            } else if now.duration_since(viewer.moved) >= IDLE_CHROME && self.editor.is_none() && !viewer.info {
                viewer.chrome = false;
            }
        }
        let today = library::today();
        if today != self.today {
            self.today = today;
            self.now = unix_now();
        }
        self.ticks = self.ticks.wrapping_add(1);
        if self.ticks.is_multiple_of(CARD_POLL_TICKS) && self.viewer.is_none() && self.editor.is_none() {
            tasks.push(Self::poll_cards());
        }
        Task::batch(tasks)
    }

    fn viewer_event(&mut self, event: ViewerEvent, shell: &mut Shell<Message>) -> Task<Message> {
        let Some(viewer) = self.viewer.as_mut() else {
            return Task::none();
        };
        viewer.moved = Instant::now();
        if !viewer.slideshow {
            viewer.chrome = true;
        }
        match event {
            ViewerEvent::Back => {
                if viewer.slideshow && self.window.frame == Frame::Fullscreen {
                    shell.window_action(Action::ToggleFullscreen);
                }
                if self.morph.as_ref().is_some_and(|morph| !morph.opening) {
                    return Task::none();
                }
                return self.close_viewer();
            }
            ViewerEvent::Step(delta) => {
                let len = viewer.list.len() as isize;
                let next = viewer.at as isize + delta;
                let next = if viewer.slideshow { next.rem_euclid(len.max(1)) } else { next.clamp(0, len - 1) };
                if next as usize != viewer.at {
                    viewer.at = next as usize;
                    viewer.page = 0;
                    return self.load_current();
                }
            }
            ViewerEvent::Jump(at) => {
                if at != viewer.at && at < viewer.list.len() {
                    viewer.at = at;
                    viewer.page = 0;
                    return self.load_current();
                }
            }
            ViewerEvent::ToggleInfo => viewer.info = !viewer.info,
            ViewerEvent::ZoomIn | ViewerEvent::ZoomOut => {
                let fit = self.fit_scale();
                if let Some(viewer) = self.viewer.as_mut() {
                    let now = viewer.zoom.unwrap_or(fit);
                    let next = if matches!(event, ViewerEvent::ZoomIn) { now * 1.25 } else { now / 1.25 };
                    viewer.zoom = Some(next.clamp(0.05, 8.0));
                }
            }
            ViewerEvent::Fit => viewer.zoom = None,
            ViewerEvent::Fullscreen => shell.window_action(Action::ToggleFullscreen),
            ViewerEvent::Slideshow => {
                shell.close_menu();
                viewer.slideshow = !viewer.slideshow;
                viewer.chrome = !viewer.slideshow;
                let fullscreen = self.window.frame == Frame::Fullscreen;
                if viewer.slideshow != fullscreen {
                    shell.window_action(Action::ToggleFullscreen);
                }
            }
            ViewerEvent::TogglePlay => viewer.playing = !viewer.playing,
            ViewerEvent::NextFrame => {
                if let Some(loaded) = &viewer.loaded
                    && !loaded.frames.is_empty()
                {
                    viewer.frame = (viewer.frame + 1) % loaded.frames.len();
                }
            }
            ViewerEvent::Page(page) => {
                if viewer.loaded.as_ref().is_some_and(|loaded| page < loaded.pages) && page != viewer.page {
                    viewer.page = page;
                    return self.load_current();
                }
            }
            ViewerEvent::Exposure(exposure) => {
                viewer.exposure = exposure;
                if let Some(hdr) = viewer.loaded.as_ref().and_then(|loaded| loaded.hdr.clone()) {
                    let generation = viewer.generation;
                    return later(
                        move || {
                            let mapped = formats::tone_map(&hdr, exposure);
                            Handle::from_rgba(mapped.width(), mapped.height(), mapped.into_raw())
                        },
                        move |handle| Message::Exposed(generation, handle),
                    );
                }
            }
            ViewerEvent::Backdrop(backdrop) => viewer.backdrop = backdrop,
            ViewerEvent::More => shell.context_menu(self.viewer_menu()),
            ViewerEvent::Copy => {
                shell.close_menu();
                if let Some(index) = viewer.current() {
                    return iced::clipboard::write(self.photos[index].path.to_string_lossy().into_owned());
                }
            }
            ViewerEvent::Edit => {
                if viewer.loaded.is_some() {
                    return self.open_editor();
                }
            }
        }
        Task::none()
    }

    #[must_use]
    pub fn stage(&self) -> (f32, f32) {
        let viewer = self.viewer.as_ref();
        let chrome = viewer.is_none_or(|viewer| viewer.chrome || viewer.info);
        let info = viewer.is_some_and(|viewer| viewer.info);
        let width = self.window.size.width - if info { glide_sdk::widget::INSPECTOR_WIDTH + 1.0 } else { 0.0 };
        let height = self.window.size.height - if chrome { crate::view::HEADER + crate::view::FILMSTRIP } else { 0.0 };
        (width.max(100.0), height.max(100.0))
    }

    #[must_use]
    pub fn fit_scale(&self) -> f32 {
        let Some(loaded) = self.viewer.as_ref().and_then(|viewer| viewer.loaded.as_ref()) else {
            return 1.0;
        };
        let (width, height) = self.stage();
        let (w, h) = (loaded.size.0.max(1) as f32, loaded.size.1.max(1) as f32);
        ((width - 2.0 * crate::view::STAGE_MARGIN) / w).min((height - 2.0 * crate::view::STAGE_MARGIN) / h).min(1.0)
    }

    #[allow(clippy::too_many_lines)] // One arm per editor event; each is short
    fn editor_event(&mut self, event: EditorEvent) -> Task<Message> {
        let lang = self.lang;
        let Some(editor) = self.editor.as_mut() else {
            return Task::none();
        };
        let before = editor.edit;
        let mut commit = true;
        match event {
            EditorEvent::Tool(tool) => editor.tool = tool,
            EditorEvent::Slider(slider, value) => {
                editor.edit.set(slider, value);
                commit = false;
            }
            EditorEvent::Release => {}
            EditorEvent::Reset(slider) => editor.edit.set(slider, 0),
            EditorEvent::Auto => {
                let auto = Edit::auto();
                editor.edit = Edit { filter: editor.edit.filter, strength: editor.edit.strength, ..auto };
                editor.edit.turns = before.turns;
                editor.edit.flip_h = before.flip_h;
                editor.edit.flip_v = before.flip_v;
                editor.edit.tilt = before.tilt;
                editor.edit.crop = before.crop;
                editor.edit.ratio = before.ratio;
            }
            EditorEvent::ResetAll => editor.edit = Edit::default(),
            EditorEvent::Filter(filter) => editor.edit.filter = filter,
            EditorEvent::Strength(strength) => {
                editor.edit.strength = strength.clamp(0, 100);
                commit = false;
            }
            EditorEvent::Ratio(ratio) => {
                editor.edit.ratio = ratio;
                if let Some((_, (w, h))) = editor.geometry {
                    let image_ratio = w.max(1) as f32 / h.max(1) as f32;
                    let original = editor
                        .source
                        .as_ref()
                        .map_or(image_ratio, |source| source.width().max(1) as f32 / source.height().max(1) as f32);
                    editor.edit.crop = match ratio.value(original, editor.edit.portrait) {
                        Some(value) => edit::Crop::fitted(value, image_ratio),
                        None => edit::Crop::default(),
                    };
                }
            }
            EditorEvent::Portrait(portrait) => {
                editor.edit.portrait = portrait;
                let ratio = editor.edit.ratio;
                return self.editor_event(EditorEvent::Ratio(ratio));
            }
            EditorEvent::Turn(delta) => {
                editor.edit.turns = (i16::from(editor.edit.turns) + i16::from(delta)).rem_euclid(4) as u8;
                editor.edit.crop = edit::Crop::default();
            }
            EditorEvent::FlipH => editor.edit.flip_h = !editor.edit.flip_h,
            EditorEvent::FlipV => editor.edit.flip_v = !editor.edit.flip_v,
            EditorEvent::Tilt(degrees) => {
                editor.edit.tilt = degrees.clamp(-45.0, 45.0);
                editor.edit.crop = edit::Crop::default();
                commit = false;
            }
            EditorEvent::Level => {
                editor.edit.tilt = 0.0;
                editor.edit.crop = edit::Crop::default();
            }
            EditorEvent::Crop(rect) => {
                editor.edit.crop = edit::Crop { x: rect.x, y: rect.y, w: rect.w, h: rect.h };
                editor.commit();
                return Task::none();
            }
            EditorEvent::ResetCrop => {
                editor.edit.crop = edit::Crop::default();
                editor.edit.ratio = Ratio::Free;
                editor.edit.tilt = 0.0;
            }
            EditorEvent::Undo => {
                if editor.can_undo() {
                    editor.at -= 1;
                    editor.edit = editor.history[editor.at];
                }
                commit = false;
            }
            EditorEvent::Redo => {
                if editor.can_redo() {
                    editor.at += 1;
                    editor.edit = editor.history[editor.at];
                }
                commit = false;
            }
            EditorEvent::Compare(on) => {
                editor.compare = on;
                return Task::none();
            }
            EditorEvent::Cancel | EditorEvent::SaveDiscard => {
                self.editor = None;
                return Task::none();
            }
            EditorEvent::Done => {
                if editor.edit.is_identity() {
                    self.editor = None;
                    return Task::none();
                }
                let photo = &self.photos[editor.photo];
                let stem = photo.path.file_stem().map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
                let suffix = Key::EditedSuffix.text(lang);
                editor.saving = Some(SaveForm {
                    replace: false,
                    can_replace: writable(photo),
                    name: format!("{stem} ({suffix})"),
                    output: Output::of(&photo.path).unwrap_or_default(),
                });
                return Task::none();
            }
            EditorEvent::SaveReplace(replace) => {
                if let Some(form) = editor.saving.as_mut() {
                    form.replace = replace && form.can_replace;
                }
                return Task::none();
            }
            EditorEvent::SaveName(name) => {
                if let Some(form) = editor.saving.as_mut() {
                    form.name = name;
                }
                return Task::none();
            }
            EditorEvent::SaveOutput(output) => {
                if let Some(form) = editor.saving.as_mut() {
                    form.output = output;
                }
                return Task::none();
            }
            EditorEvent::SaveCancel => {
                editor.saving = None;
                return Task::none();
            }
            EditorEvent::SaveConfirm => {
                let Some(form) = editor.saving.take() else {
                    return Task::none();
                };
                editor.busy = true;
                let photo = self.photos[editor.photo].clone();
                let edit = editor.edit;
                let page = self.viewer.as_ref().map_or(0, |viewer| viewer.page);
                let replace = form.replace;
                return later(
                    move || {
                        let request =
                            Request { path: &photo.path, format: photo.format, max_side: None, page, animate: false };
                        let decoded = formats::decode(&request).map_err(|failure| format!("{failure:?}"))?;
                        let result = edit::apply(&decoded.image, &edit);
                        let (target, output) = if replace {
                            (photo.path.clone(), Output::of(&photo.path).unwrap_or(form.output))
                        } else {
                            let name = form.name.trim();
                            let name = if name.is_empty() { "photo" } else { name };
                            (edit::copy_path(&photo.path, name, form.output), form.output)
                        };
                        edit::write(&result, &target, output).map(|()| target)
                    },
                    move |result| Message::Saved(result, replace),
                );
            }
        }
        if editor.edit != before || commit {
            if commit {
                editor.commit();
            }
            return self.render();
        }
        Task::none()
    }

    pub fn key(&mut self, event: &keyboard::Event, captured: bool, shell: &mut Shell<Message>) -> Task<Message> {
        use keyboard::key::Named;
        if let keyboard::Event::ModifiersChanged(modifiers) = event {
            self.modifiers = *modifiers;
            return Task::none();
        }
        if let keyboard::Event::KeyReleased { key: keyboard::Key::Named(Named::Space), .. } = event
            && self.editor.as_ref().is_some_and(|editor| editor.compare)
        {
            return self.update(Message::Editor(EditorEvent::Compare(false)), shell);
        }
        let keyboard::Event::KeyPressed { key, modifiers, physical_key, .. } = event else {
            return Task::none();
        };
        if !captured
            && self.modal.is_none()
            && !shell.has_dialog()
            && let Some(message) = glide_sdk::menu::shortcut(&self.menus(), key, *physical_key, *modifiers)
        {
            return self.update(message, shell);
        }
        if captured || self.modal.is_some() || shell.has_dialog() {
            if matches!(key, keyboard::Key::Named(Named::Escape)) && self.modal.is_some() {
                self.modal = None;
            }
            return Task::none();
        }
        let message = match key.as_ref() {
            keyboard::Key::Named(Named::Escape) => {
                if self.editor.as_ref().is_some_and(|editor| editor.saving.is_some()) {
                    Some(Message::Editor(EditorEvent::SaveCancel))
                } else if self.editor.is_some() {
                    Some(Message::Editor(EditorEvent::Cancel))
                } else if self.viewer.is_some() {
                    Some(Message::Viewer(ViewerEvent::Back))
                } else if self.selecting {
                    Some(Message::EndSelect)
                } else if self.searching() {
                    Some(Message::ClearQuery)
                } else {
                    None
                }
            }
            keyboard::Key::Named(Named::Space) if self.editor.is_some() => {
                Some(Message::Editor(EditorEvent::Compare(true)))
            }
            keyboard::Key::Named(Named::Space) if self.viewer.is_some() => {
                Some(Message::Viewer(ViewerEvent::TogglePlay))
            }
            keyboard::Key::Named(Named::ArrowLeft) if self.viewer.is_some() && self.editor.is_none() => {
                Some(Message::Viewer(ViewerEvent::Step(-1)))
            }
            keyboard::Key::Named(Named::ArrowRight) if self.viewer.is_some() && self.editor.is_none() => {
                Some(Message::Viewer(ViewerEvent::Step(1)))
            }
            keyboard::Key::Named(Named::PageUp) if self.viewer.is_some() => {
                self.viewer.as_ref().map(|viewer| Message::Viewer(ViewerEvent::Page(viewer.page.saturating_sub(1))))
            }
            keyboard::Key::Named(Named::PageDown) if self.viewer.is_some() => {
                self.viewer.as_ref().map(|viewer| Message::Viewer(ViewerEvent::Page(viewer.page + 1)))
            }
            keyboard::Key::Named(Named::Delete) if self.editor.is_none() => self.delete_targets().map(Message::Trash),
            keyboard::Key::Named(Named::F5) if self.viewer.is_some() => Some(Message::Viewer(ViewerEvent::Slideshow)),
            keyboard::Key::Character("i") if modifiers.control() && self.viewer.is_some() => {
                Some(Message::Viewer(ViewerEvent::ToggleInfo))
            }
            keyboard::Key::Character("e") if self.viewer.is_some() && self.editor.is_none() && !modifiers.control() => {
                Some(Message::Viewer(ViewerEvent::Edit))
            }
            keyboard::Key::Character("z") if modifiers.control() && self.editor.is_some() => {
                Some(Message::Editor(if modifiers.shift() { EditorEvent::Redo } else { EditorEvent::Undo }))
            }
            keyboard::Key::Character("a") if modifiers.control() && self.viewer.is_none() => Some(Message::SelectAll),
            keyboard::Key::Character("+" | "=") if self.viewer.is_some() => Some(Message::Viewer(ViewerEvent::ZoomIn)),
            keyboard::Key::Character("-") if self.viewer.is_some() => Some(Message::Viewer(ViewerEvent::ZoomOut)),
            keyboard::Key::Character("0") if self.viewer.is_some() => Some(Message::Viewer(ViewerEvent::Fit)),
            keyboard::Key::Character(".") if self.editor.is_none() => self.delete_targets().map(|paths| {
                let on = !paths.iter().all(|path| self.prefs.is_favourite(path));
                Message::Favourite(paths, on)
            }),
            _ => None,
        };
        message.map_or_else(Task::none, |message| self.update(message, shell))
    }

    fn delete_targets(&self) -> Option<Vec<PathBuf>> {
        if let Some(index) = self.viewer.as_ref().and_then(Viewer::current) {
            return Some(vec![self.photos[index].path.clone()]);
        }
        (self.selecting && !self.selected.is_empty()).then(|| self.selected.iter().cloned().collect())
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![iced::time::every(Duration::from_millis(500)).map(|_| Message::Tick)];
        if self.morph.is_some() {
            subs.push(iced::window::frames().map(Message::Frame));
        }
        if let Some(viewer) = &self.viewer {
            if viewer.slideshow {
                subs.push(iced::time::every(SLIDE).map(|_| Message::Viewer(ViewerEvent::Step(1))));
            }
            if viewer.playing
                && let Some(loaded) = &viewer.loaded
                && let Some((_, delay)) = loaded.frames.get(viewer.frame)
            {
                subs.push(
                    iced::time::every(Duration::from_millis(u64::from(*delay)))
                        .map(|_| Message::Viewer(ViewerEvent::NextFrame)),
                );
            }
        }
        Subscription::batch(subs)
    }

    #[must_use]
    pub fn menus(&self) -> Vec<Menu<Message>> {
        let viewing = self.viewer.is_some();
        let editing = self.editor.is_some();
        let has = |paths: &Option<Vec<PathBuf>>| paths.as_ref().is_some_and(|paths| !paths.is_empty());
        let targets = self.delete_targets();
        vec![
            Menu::new(
                self.t(Key::FileMenu),
                vec![
                    Item::action(self.t(Key::AddFolder), Message::AddFolder),
                    Item::action(self.t(Key::Export), Message::Export(targets.clone().unwrap_or_default()))
                        .shortcut("Ctrl+E")
                        .enabled(has(&targets)),
                    Item::separator(),
                    Item::action(self.t(Key::Quit), Message::Close).shortcut("Ctrl+Q"),
                ],
            ),
            Menu::new(
                self.t(Key::EditMenu),
                vec![
                    Item::action(self.t(Key::Undo), Message::Editor(EditorEvent::Undo))
                        .shortcut("Ctrl+Z")
                        .enabled(self.editor.as_ref().is_some_and(Editor::can_undo)),
                    Item::action(self.t(Key::Redo), Message::Editor(EditorEvent::Redo))
                        .shortcut("Ctrl+Shift+Z")
                        .enabled(self.editor.as_ref().is_some_and(Editor::can_redo)),
                    Item::separator(),
                    Item::action(self.t(Key::Select), Message::StartSelect).enabled(!viewing),
                    Item::action(self.t(Key::SelectAll), Message::SelectAll).shortcut("Ctrl+A").enabled(!viewing),
                    Item::separator(),
                    Item::action(self.t(Key::MoveToTrash), Message::Trash(targets.clone().unwrap_or_default()))
                        .danger()
                        .enabled(has(&targets) && !editing),
                ],
            ),
            Menu::new(
                self.t(Key::ViewMenu),
                vec![
                    Item::action(self.t(Key::Years), Message::Scale(Scale::Years)).checked(self.scale == Scale::Years),
                    Item::action(self.t(Key::Months), Message::Scale(Scale::Months))
                        .checked(self.scale == Scale::Months),
                    Item::action(self.t(Key::AllPhotos), Message::Scale(Scale::All)).checked(self.scale == Scale::All),
                    Item::separator(),
                    Item::action(self.t(Key::InfoPanel), Message::Viewer(ViewerEvent::ToggleInfo))
                        .shortcut("Ctrl+I")
                        .enabled(viewing)
                        .checked(self.viewer.as_ref().is_some_and(|viewer| viewer.info)),
                    Item::action(self.t(Key::Fullscreen), Message::Viewer(ViewerEvent::Fullscreen))
                        .shortcut("F11")
                        .enabled(viewing),
                ],
            ),
            Menu::new(
                self.t(Key::ImageMenu),
                vec![
                    Item::action(self.t(Key::Edit), Message::Viewer(ViewerEvent::Edit))
                        .shortcut("E")
                        .enabled(viewing && !editing),
                    Item::action(self.t(Key::Slideshow), Message::Viewer(ViewerEvent::Slideshow))
                        .shortcut("F5")
                        .enabled(viewing),
                    Item::separator(),
                    Item::action(self.t(Key::RotateLeft), Message::Rotate(targets.clone().unwrap_or_default(), 3))
                        .shortcut("Ctrl+[")
                        .enabled(has(&targets) && !editing),
                    Item::action(self.t(Key::RotateRight), Message::Rotate(targets.clone().unwrap_or_default(), 1))
                        .shortcut("Ctrl+]")
                        .enabled(has(&targets) && !editing),
                ],
            ),
        ]
    }

    pub fn reopened(&mut self, args: &[String]) -> Task<Message> {
        let Some(path) = args.iter().map(PathBuf::from).find(|path| path.is_file()) else {
            return Task::none();
        };
        let path = std::path::absolute(&path).unwrap_or(path);
        if let Some(index) = self.photos.iter().position(|photo| photo.path == path) {
            return self.open(index);
        }
        let Some(photo) = library::read_photo(&path, library::utc_offset()) else {
            return Task::none();
        };
        self.photos.push(photo);
        let index = self.photos.len() - 1;
        let task = self.open(index);
        if let Some(viewer) = self.viewer.as_mut() {
            viewer.list = vec![index];
            viewer.at = 0;
        }
        task
    }
}

#[must_use]
pub fn unique(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let ext = path.extension().map(|ext| format!(".{}", ext.to_string_lossy())).unwrap_or_default();
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    (2..100_000)
        .map(|n| dir.join(format!("{stem} {n}{ext}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

fn card_walk(dir: &Path, offset: i64, out: &mut Vec<Photo>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            card_walk(&path, offset, out);
        } else if let Some(photo) = library::read_photo(&path, offset) {
            out.push(photo);
        }
    }
}

#[must_use]
pub fn card_fresh(card: &Card, pictures: &Path) -> Vec<PathBuf> {
    let target = pictures.join(&card.label);
    let mut found = Vec::new();
    let mut stack = vec![card.dcim.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if formats::of(&path).is_some() && path.file_name().is_some_and(|name| !target.join(name).exists()) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn mount_device(mount: &Path) -> Option<String> {
    let mounts = std::fs::read_to_string("/proc/self/mounts").ok()?;
    mounts.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let device = parts.next()?;
        let point = parts.next()?.replace("\\040", " ");
        (Path::new(&point) == mount).then(|| device.to_string())
    })
}

impl glide_sdk::App for App {
    type Message = Message;

    fn new(shell: &mut Shell<Message>) -> (Self, Task<Message>) {
        let app = Self::new(shell);
        let task = app.rescan();
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut app = app;
        let open = app.reopened(&args);
        (app, Task::batch([task, open]))
    }

    fn title(&self, _: &Shell<Message>) -> String {
        self.t(Key::AppName).to_string()
    }

    fn update(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        Self::update(self, message, shell)
    }

    fn view<'a>(&'a self, _: &'a Shell<Message>) -> iced::Element<'a, Message> {
        crate::view::view(self)
    }

    fn subscription(&self) -> Subscription<Message> {
        Self::subscription(self)
    }

    fn menus(&self, _: &Shell<Message>) -> Vec<Menu<Message>> {
        Self::menus(self)
    }

    fn window_changed(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        self.window = *shell.window();
        Task::none()
    }

    fn key(&mut self, event: &keyboard::Event, captured: bool, shell: &mut Shell<Message>) -> Task<Message> {
        Self::key(self, event, captured, shell)
    }

    fn paint(&self, _: &Shell<Message>) -> Paint {
        self.paint
    }

    fn look_changed(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        self.lang = shell.lang();
        self.paint = *shell.paint();
        Task::none()
    }

    fn reopened(&mut self, args: Vec<String>, _: &mut Shell<Message>) -> Task<Message> {
        Self::reopened(self, &args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(name: &str) -> (PathBuf, Vec<Photo>) {
        let dir = std::env::temp_dir().join(format!("glide-photos-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Камера")).unwrap();
        for (index, file) in ["a.png", "b.png", "Камера/c.png", "Screenshot 1.png"].iter().enumerate() {
            image::RgbImage::from_pixel(64, 48, image::Rgb([index as u8 * 60, 90, 30])).save(dir.join(file)).unwrap();
        }
        std::fs::write(dir.join("notes.txt"), b"not a photo").unwrap();
        let photos = library::scan(std::slice::from_ref(&dir));
        (dir, photos)
    }

    fn harness(photos: Vec<Photo>) -> glide_sdk::Harness<App> {
        let mut app =
            glide_sdk::Harness::<App>::new(crate::APP_ID, glide_sdk::Look::default(), iced::Size::new(1280.0, 800.0));
        app.message(Message::Scanned(photos));
        app
    }

    fn finish_motion(app: &mut glide_sdk::Harness<App>) {
        app.message(Message::Frame(Instant::now() + Duration::from_secs(5)));
    }

    #[test]
    fn a_tile_morphs_into_the_viewer_and_back_into_the_same_tile() {
        let (dir, photos) = library("morph");
        let mut app = harness(photos);
        let tile = app.app().tile_rect(1, None).expect("tile in the grid");
        app.point_at(tile.center());
        app.message(Message::Tile(1));
        let morph = app.app().morph.clone().expect("opening morph");
        assert!(morph.opening);
        assert_eq!(morph.from, tile);
        assert_eq!(morph.rect(0.0), tile);
        assert_eq!(morph.rect(1.0), morph.to);
        let travel = (morph.to.center().x - tile.center().x).hypot(morph.to.center().y - tile.center().y);
        let scale = (morph.to.width / tile.width - 1.0).abs();
        assert!(travel >= 24.0 || scale >= 0.08, "scene content moves at least 24 px or scales 8 %");
        assert!(morph.backdrop(0.0) < 0.01 && (morph.backdrop(1.0) - 1.0).abs() < 1e-4);
        let _ = app.view();
        app.message(Message::Frame(morph.started));
        assert!(app.app().morph.is_some(), "a frame at the start keeps the morph");
        finish_motion(&mut app);
        assert!(app.app().morph.is_none() && app.app().viewer.is_some());
        app.message(Message::Viewer(ViewerEvent::Step(1)));
        app.message(Message::Viewer(ViewerEvent::Back));
        let back = app.app().morph.clone().expect("closing morph");
        assert!(!back.opening);
        assert_eq!(back.to, app.app().tile_rect(2, None).unwrap(), "it lands on the photo now shown");
        assert!((back.backdrop(0.0) - 1.0).abs() < 1e-4 && back.backdrop(1.0) < 0.01);
        assert!(app.app().viewer.is_some(), "the viewer stays until the morph ends");
        let _ = app.view();
        finish_motion(&mut app);
        assert!(app.app().viewer.is_none() && app.app().morph.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_scanned_folder_fills_the_library_and_every_screen_builds() {
        let (dir, photos) = library("screens");
        assert_eq!(photos.len(), 4);
        let mut app = harness(photos);
        assert_eq!(app.app().shown().len(), 4);
        let (columns, side) = app.app().grid();
        assert!(columns >= 4 && side > 80.0);
        for scale in [Scale::Years, Scale::Months, Scale::All] {
            app.message(Message::Scale(scale));
            let _ = app.view();
        }
        for place in [Place::Favourites, Place::Albums, Place::Smart(Smart::Screenshots), Place::Trash, Place::Library]
        {
            app.message(Message::Go(place));
            let _ = app.view();
        }
        app.message(Message::Go(Place::Smart(Smart::Screenshots)));
        assert_eq!(app.app().shown().len(), 1);
        app.message(Message::Go(Place::Folder(dir.join("Камера"))));
        assert_eq!(app.app().shown().len(), 1);
        app.message(Message::Go(Place::Library));
        app.message(Message::Query("zzz".into()));
        assert!(app.app().shown().is_empty());
        let _ = app.view();
        app.message(Message::ClearQuery);
        app.message(Message::Chip(Chip::Kind(Smart::Screenshots)));
        assert_eq!(app.app().shown().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tiles_open_the_viewer_and_selection_mode_picks_instead() {
        let (dir, photos) = library("viewer");
        let mut app = harness(photos);
        app.message(Message::Tile(1));
        let viewer = app.app().viewer.as_ref().expect("viewer");
        assert_eq!((viewer.at, viewer.list.len()), (1, 4));
        app.message(Message::Viewer(ViewerEvent::Step(1)));
        assert_eq!(app.app().viewer.as_ref().unwrap().at, 2);
        app.message(Message::Viewer(ViewerEvent::Step(10)));
        assert_eq!(app.app().viewer.as_ref().unwrap().at, 3);
        app.message(Message::Viewer(ViewerEvent::ToggleInfo));
        let _ = app.view();
        app.message(Message::Viewer(ViewerEvent::Back));
        finish_motion(&mut app);
        assert!(app.app().viewer.is_none());
        app.message(Message::StartSelect);
        app.message(Message::Tile(0));
        app.message(Message::Tile(2));
        assert_eq!(app.app().selected.len(), 2);
        assert!(app.app().viewer.is_none());
        let _ = app.view();
        app.message(Message::Tile(2));
        assert_eq!(app.app().selected.len(), 1);
        app.message(Message::EndSelect);
        assert!(app.app().selected.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_album_takes_the_photos_it_was_made_for() {
        let (dir, photos) = library("album");
        let first = photos[0].path.clone();
        let mut app = harness(photos);
        app.message(Message::AskNewAlbum(vec![first.clone()]));
        let _ = app.view();
        app.message(Message::CreateAlbum);
        assert!(app.app().modal.is_some());
        app.message(Message::AlbumName("Карпати".into()));
        app.message(Message::CreateAlbum);
        assert!(app.app().modal.is_none());
        let album = app.app().prefs.albums.last().expect("album");
        assert_eq!((album.name.as_str(), album.photos.as_slice()), ("Карпати", std::slice::from_ref(&first)));
        let index = app.app().prefs.albums.len() - 1;
        app.message(Message::Go(Place::Album(index)));
        assert_eq!(app.app().shown().len(), 1);
        app.message(Message::Favourite(vec![first], true));
        app.message(Message::Go(Place::Favourites));
        assert_eq!(app.app().shown().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trashed_photos_leave_the_library_and_the_viewer_moves_on() {
        let (dir, photos) = library("trash");
        let gone = photos[1].clone();
        let mut app = harness(photos);
        app.message(Message::Tile(1));
        let trashed = Trashed { file: dir.join("x"), info: dir.join("y"), original: gone.path.clone() };
        app.message(Message::Trashed(vec![trashed], 1));
        assert_eq!(app.app().photos.len(), 3);
        let viewer = app.app().viewer.as_ref().expect("viewer stays");
        assert_eq!(viewer.list.len(), 3);
        assert!(viewer.current().is_some_and(|index| app.app().photos[index].path != gone.path));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rescan_keeps_the_viewer_on_the_same_photo_and_cards_add_to_the_library() {
        let (dir, photos) = library("rescan");
        let first = photos[0].path.clone();
        let mut app = harness(photos.clone());
        app.message(Message::Tile(2));
        let current = |app: &glide_sdk::Harness<App>| {
            let viewer = app.app().viewer.as_ref().unwrap();
            app.app().photos[viewer.current().unwrap()].path.clone()
        };
        let watched = current(&app);
        let shorter: Vec<Photo> = photos.iter().filter(|photo| photo.path != first).cloned().collect();
        std::fs::remove_file(&first).unwrap();
        app.message(Message::Scanned(shorter));
        let _ = app.view();
        assert_eq!(current(&app), watched);
        assert_eq!(app.app().viewer.as_ref().unwrap().list.len(), 3);
        app.message(Message::Viewer(ViewerEvent::Back));
        finish_motion(&mut app);
        let card = dir.join("card/DCIM");
        std::fs::create_dir_all(&card).unwrap();
        image::RgbImage::new(8, 8).save(card.join("IMG_0001.png")).unwrap();
        let found = library::scan(std::slice::from_ref(&card));
        app.message(Message::CardScanned(found));
        assert_eq!(app.app().photos.len(), 4);
        app.message(Message::Go(Place::Card(card)));
        assert_eq!(app.app().shown().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writable_formats_are_the_ones_copies_and_rotation_can_replace() {
        let (dir, photos) = library("writable");
        assert!(photos.iter().all(writable));
        let svg = dir.join("v.svg");
        std::fs::write(&svg, "<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'/>").unwrap();
        let vector = library::read_photo(&svg, 0).unwrap();
        assert!(!writable(&vector));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

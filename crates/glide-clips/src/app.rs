use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use glide_sdk::glide_app::menubar::Handles;
use glide_sdk::glide_app::window_traits;
use glide_sdk::glide_theme::Accent;
use glide_sdk::widget::player::Trim;
use glide_sdk::window::Action;
use glide_sdk::{Lang, Paint};
use glide_video::export::{self, Codec, Progress, Quality};
use glide_video::frames::{self, Grabber};
use glide_video::info::Info;
use glide_video::media::{Chapter, Failure, Kind, TrackKind, Tracks};
use glide_video::player::{Cue, Event, Options, Playback, Player};
use glide_video::sink::Rect;
use glide_video::surface::Surface;
use iced::Task;
use iced::widget::image::Handle;

use crate::i18n::Key;
use crate::keys::Command;
use crate::memory::{self, Memory, Saver};
use crate::ocr;
use crate::timeline::Timeline;

pub const IDLE: Duration = Duration::from_secs(2);
pub const STRIP: usize = 12;
const PREVIEW_WIDTH: u32 = 352;
const STRIP_WIDTH: u32 = 160;
pub const BUCKETS: u32 = 400;
pub const CHAPTER_TAG: u64 = 1 << 40;

#[derive(Debug, Clone, PartialEq)]
pub struct Reopen {
    pub subtitle: Option<String>,
    pub keep_pitch: bool,
    pub resume: Option<Duration>,
    pub rate: f64,
}

impl Default for Reopen {
    fn default() -> Self {
        Self { subtitle: None, keep_pitch: true, resume: None, rate: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Aspect {
    #[default]
    File,
    Fill,
    Cinema,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Play,
    Trim { start: f32, end: f32, head: f32 },
    Clips { dragging: Option<usize> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Recent {
    pub path: PathBuf,
    pub entry: memory::Entry,
    pub cover: Option<Handle>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ocr {
    Busy,
    Lines(Vec<ocr::Line>),
    Missing,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportForm {
    pub name: String,
    pub folder: PathBuf,
    pub quality: Quality,
    pub codec: Codec,
    pub lossless: bool,
    pub hdr: bool,
}

pub enum Exporting {
    Form(ExportForm),
    Running { handle: export::Handle, progress: f64, started: Instant, hardware: bool, name: String },
    Done(PathBuf),
    Failed,
}

#[allow(clippy::struct_excessive_bools)] // Independent player switches, each driven by its own message
pub struct Media {
    pub uri: String,
    pub path: Option<PathBuf>,
    pub title: String,
    pub player: Option<Player>,
    pub info: Option<Info>,
    pub kind: Kind,
    pub playback: Playback,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub buffered: f32,
    pub rate: f64,
    pub keep_pitch: bool,
    pub volume: f64,
    pub muted: bool,
    pub repeat: bool,
    pub on_top: bool,
    pub chapters: Vec<Chapter>,
    pub tracks: Tracks,
    pub selected: Vec<String>,
    pub text_track: Option<String>,
    pub cue: Option<(Cue, Instant)>,
    pub offset_ms: i64,
    pub failure: Option<Failure>,
    pub reached: Duration,
    pub buffering: Option<u8>,
    pub live: bool,
    pub sound_only: bool,
    pub aspect: Aspect,
    pub timeline: Timeline,
    pub clip: usize,
    pub frames: HashMap<u32, Handle>,
    pub chapter_frames: HashMap<usize, Handle>,
    pub strip: Vec<Option<Handle>>,
    pub hover: Option<u32>,
    pub grabber: Option<Grabber>,
    pub grabbers: HashMap<usize, Grabber>,
    pub strip_grabbers: Vec<Grabber>,
    pub feed: u64,
    pub loaded: usize,
    pub handoff: Option<(Duration, bool)>,
    pub restore: Option<Vec<String>>,
    pub video_subtitles: bool,
    pub prefer_external: bool,
    pub sidecar_lines: Vec<glide_video::media::Line>,
    pub sidecar_on: bool,
    pub generation: u64,
    pub remembered: Option<memory::Entry>,
    pub cover: Option<Handle>,
    pub details: bool,
    pub playing_since_open: bool,
    pub suburi: Option<String>,
    pub buffer_hold: bool,
}

impl Media {
    #[must_use]
    pub fn new(uri: String, path: Option<PathBuf>, generation: u64) -> Self {
        Self {
            title: title_of(&uri, path.as_deref()),
            uri,
            path,
            player: None,
            info: None,
            kind: Kind::Video,
            playback: Playback::Stopped,
            position: Duration::ZERO,
            duration: None,
            buffered: 0.0,
            rate: 1.0,
            keep_pitch: true,
            volume: 0.7,
            muted: false,
            repeat: false,
            on_top: false,
            chapters: Vec::new(),
            tracks: Tracks::default(),
            selected: Vec::new(),
            text_track: None,
            cue: None,
            offset_ms: 0,
            failure: None,
            reached: Duration::ZERO,
            buffering: None,
            live: false,
            sound_only: false,
            aspect: Aspect::File,
            timeline: Timeline::whole(Duration::ZERO),
            clip: 0,
            frames: HashMap::new(),
            chapter_frames: HashMap::new(),
            strip: vec![None; STRIP],
            hover: None,
            grabber: None,
            grabbers: HashMap::new(),
            strip_grabbers: Vec::new(),
            feed: generation,
            loaded: crate::timeline::MAIN,
            handoff: None,
            restore: None,
            video_subtitles: false,
            prefer_external: false,
            sidecar_lines: Vec::new(),
            sidecar_on: false,
            generation,
            remembered: None,
            cover: None,
            details: false,
            playing_since_open: false,
            suburi: None,
            buffer_hold: false,
        }
    }

    #[must_use]
    pub fn playing(&self) -> bool {
        self.playback == Playback::Playing
    }

    #[must_use]
    pub fn length(&self) -> Duration {
        if self.timeline.edited() { self.timeline.total() } else { self.duration.unwrap_or_default() }
    }

    #[must_use]
    pub fn at(&self) -> Duration {
        if self.timeline.edited() { self.timeline.total_at(self.clip, self.position) } else { self.position }
    }

    #[must_use]
    pub fn fraction(&self) -> f32 {
        let length = self.length().as_secs_f64();
        if length <= 0.0 { 0.0 } else { (self.at().as_secs_f64() / length).clamp(0.0, 1.0) as f32 }
    }

    #[must_use]
    pub fn chapter(&self) -> Option<usize> {
        glide_video::media::chapter_at(&self.chapters, self.position)
    }

    #[must_use]
    pub fn video_size(&self) -> Option<(u32, u32)> {
        self.info.as_ref().and_then(|info| info.video.as_ref()).map(|video| (video.width, video.height))
    }

    #[must_use]
    pub fn showing_video(&self) -> bool {
        self.kind == Kind::Video && !self.sound_only && !matches!(self.failure, Some(Failure::Missing { .. }))
    }

    #[must_use]
    pub fn text_tracks(&self) -> Vec<&glide_video::media::Track> {
        self.tracks.of(TrackKind::Text).collect()
    }

    #[must_use]
    pub fn selected_of(&self, kind: TrackKind) -> Option<&str> {
        self.selected.iter().find(|id| self.tracks.get(id).is_some_and(|track| track.kind == kind)).map(String::as_str)
    }
}

pub struct Chrome {
    pub shown: bool,
    pub moved: Instant,
    pub pointer: iced::Point,
}

#[derive(Debug, Clone)]
pub enum Message {
    Handles(Option<Handles>),
    Open,
    Chosen(Option<PathBuf>),
    AddressDialog,
    AddressEdited(String),
    AddressConfirm,
    AddressCancel,
    OpenPath(PathBuf),
    OpenRecent(PathBuf),
    Player(u64, Event),
    Info(u64, Option<Box<Info>>),
    Frame(u64, frames::Frame),
    Strip(u64, frames::Frame),
    Tick,
    Wake,
    Leave,
    TogglePlay,
    Skip(i32),
    Step(bool),
    Volume(f32),
    ToggleMute,
    Seek(f32),
    SeekReleased,
    Hover(Option<u32>),
    ToggleRemaining,
    Window(Action),
    WindowDrag,
    Close,
    SpeedMenu,
    SetSpeed(f64),
    KeepPitch,
    TracksMenu,
    PickText(Option<String>),
    PickAudio(String),
    SubtitleOffset(i64),
    AddSubtitle,
    SubtitleChosen(Option<PathBuf>),
    ChaptersMenu,
    GoChapter(usize),
    MoreMenu,
    AspectMenu,
    ContextMenu,
    ToggleRepeat,
    ToggleOnTop,
    SetAspect(Aspect),
    Pip,
    PipAsk(bool),
    Command(Command),
    EnterTrim,
    TrimChanged(Trim),
    TrimReset,
    TrimApply,
    TrimCancel,
    TrimPlay,
    EnterClips,
    SelectClip(usize),
    HoverClip(Option<usize>),
    DragClip(Option<usize>),
    MoveClip(usize, usize),
    SplitClip,
    RemoveClip,
    Dropped(glide_sdk::glide_app::drop_target::Dropped),
    SidecarLines(u64, Vec<glide_video::media::Line>),
    AddClip,
    ClipChosen(Option<PathBuf>),
    ClipProbed(PathBuf, Option<Duration>),
    TurnClip(bool),
    FlipClip(bool),
    ClipsDone,
    ExportOpen,
    ExportName(String),
    ExportQuality(Quality),
    ExportCodec(Codec),
    ExportLossless(bool),
    ExportHdr(bool),
    ExportFolder,
    ExportFolderChosen(PathBuf),
    ExportStart,
    ExportProgress(Progress),
    ExportCancel,
    ExportDismiss,
    OpenSaved,
    RevealSaved,
    Inspector(bool),
    CopyInspector,
    CopyFrame,
    SaveFrame,
    FrameSaved(Option<PathBuf>),
    TextOnFrame,
    Recognised(u64, ocr::Outcome),
    OcrClose,
    CopyText(String),
    SearchText(String),
    CopyTime,
    ShowInFiles,
    OpenWith,
    SoundOnly,
    ToggleDetails,
    CopyCommand(String),
    Retry,
    FromStart,
    ToLive,
    Mpris(glide_video::mpris::Command),
    Cover(PathBuf, Option<PathBuf>),
    ForgetHistory,
    Shortcuts,
    Ignore,
}

#[allow(clippy::struct_excessive_bools)] // Independent window switches (inspector, drop hover, picture-in-picture)
pub struct App {
    pub paint: Paint,
    pub dark: Paint,
    pub lang: Lang,
    pub window: glide_sdk::window::Window,
    pub handles: Option<Option<Handles>>,
    pub surface: Option<Surface>,
    pub pending: Option<(String, Option<PathBuf>)>,
    pub memory: Memory,
    pub saver: Saver,
    pub recents: Vec<Recent>,
    pub media: Option<Media>,
    pub mode: Mode,
    pub chrome: Chrome,
    pub remaining: bool,
    pub address: Option<String>,
    pub exporting: Option<Exporting>,
    pub inspector: bool,
    pub ocr: Option<(u64, Ocr)>,
    pub mpris: Option<glide_video::mpris::Server>,
    pub generation: u64,
    pub track: u64,
    pub accent: Accent,
    pub ticks: u32,
    pub export_done_at: Option<Instant>,
    pub dock: glide_sdk::system::Dock,
    pub clipboard: Option<glide_video::clipboard::Clipboard>,
    pub dropping: bool,
    pub hovered_clip: Option<usize>,
    pub queued: Vec<Task<Message>>,
    pub pip: bool,
    pub aspect_sent: Option<(u32, u32)>,
    pub blur_sent: Option<Vec<window_traits::Region>>,
}

async fn work<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    let spawned = std::thread::Builder::new().name("glide-clips-work".into()).spawn(move || {
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
pub fn uri_of(path: &Path) -> String {
    glide_video::gstreamer::glib::filename_to_uri(path, None)
        .map_or_else(|_| format!("file://{}", path.to_string_lossy()), |uri| uri.to_string())
}

#[must_use]
pub fn title_of(uri: &str, path: Option<&Path>) -> String {
    if let Some(name) = path.and_then(Path::file_name) {
        return name.to_string_lossy().into_owned();
    }
    uri.rsplit('/').find(|part| !part.is_empty()).unwrap_or(uri).to_string()
}

pub const SIDECAR: &str = "glide-clips-sidecar";
const TOPLINE_INSET: f32 = 14.0;
const TOPLINE_GAP: f32 = 8.0;
const DISC: f32 = 28.0;
const DISC_TOP: f32 = TOPLINE_INSET + (TITLE_HEIGHT - DISC) / 2.0;
const TITLE_HEIGHT: f32 = 36.0;
pub const WINDOW_LEAST: iced::Size = iced::Size::new(480.0, 270.0);
pub const PIP_LEAST: iced::Size = iced::Size::new(384.0, 216.0);

#[must_use]
pub fn styled_sidecar(uri: &str) -> bool {
    std::path::Path::new(uri)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ass") || extension.eq_ignore_ascii_case("ssa"))
}

#[must_use]
pub fn player_subtitle(suburi: Option<&str>) -> Option<String> {
    suburi.filter(|uri| !styled_sidecar(uri)).map(str::to_string)
}

#[must_use]
pub fn sidecar_language(uri: &str) -> Option<String> {
    let name = uri.rsplit('/').next()?;
    let stem = name.rsplit_once('.')?.0;
    let code = stem.rsplit_once('.')?.1.to_ascii_lowercase();
    (2..=3).contains(&code.len()).then_some(code).filter(|code| code.chars().all(|letter| letter.is_ascii_alphabetic()))
}

#[must_use]
pub fn sidecar_track(suburi: Option<&str>) -> glide_video::media::Track {
    glide_video::media::Track {
        id: SIDECAR.into(),
        kind: TrackKind::Text,
        language: suburi.and_then(sidecar_language),
        title: None,
        codec: Some("ASS".into()),
        channels: None,
        format: None,
        external: true,
    }
}

#[must_use]
pub fn read_sidecar(uri: &str) -> Vec<glide_video::media::Line> {
    let Some(path) = glide_sdk::chooser::uri_path(uri) else { return Vec::new() };
    let Ok(bytes) = std::fs::read(&path) else { return Vec::new() };
    if bytes.len() > 16 * 1024 * 1024 {
        return Vec::new();
    }
    glide_video::media::ass_script(&String::from_utf8_lossy(&bytes))
}

#[must_use]
pub fn sidecar(path: &Path, lang: Lang) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let folder = path.parent()?;
    let wanted = if lang == Lang::Uk { ["uk", "ukr", "en"] } else { ["en", "eng", "uk"] };
    let mut found: Vec<(usize, PathBuf)> = std::fs::read_dir(folder)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter_map(|candidate| {
            let name = candidate.file_name()?.to_string_lossy().into_owned();
            let extension = candidate.extension()?.to_string_lossy().to_ascii_lowercase();
            if !matches!(extension.as_str(), "srt" | "ass" | "ssa" | "vtt" | "sub") {
                return None;
            }
            let middle = name.strip_prefix(&stem)?.strip_suffix(&format!(".{extension}"))?.to_string();
            let rank = match middle.trim_start_matches('.') {
                "" => 1,
                tag => wanted
                    .iter()
                    .position(|lang| tag.eq_ignore_ascii_case(lang))
                    .map_or(9, |at| if at < 2 { 0 } else { 2 }),
            };
            (middle.is_empty() || middle.starts_with('.')).then_some((rank, candidate))
        })
        .collect();
    found.sort();
    found.into_iter().next().map(|(_, path)| path)
}

#[must_use]
pub fn is_playable(path: &Path) -> bool {
    let Some(extension) = path.extension().map(|extension| extension.to_string_lossy().to_ascii_lowercase()) else {
        return false;
    };
    matches!(
        extension.as_str(),
        "mp4"
            | "m4v"
            | "mov"
            | "mkv"
            | "webm"
            | "avi"
            | "flv"
            | "3gp"
            | "ts"
            | "m2ts"
            | "mts"
            | "ogv"
            | "ogg"
            | "wmv"
            | "asf"
            | "mxf"
            | "mpg"
            | "mpeg"
            | "vob"
            | "mp3"
            | "m4a"
            | "aac"
            | "flac"
            | "wav"
            | "opus"
            | "oga"
            | "mka"
            | "wma"
            | "alac"
            | "aiff"
            | "m3u8"
            | "mpd"
    )
}

impl App {
    #[must_use]
    pub fn t(&self, key: Key) -> &'static str {
        key.text(self.lang)
    }

    pub(crate) fn refresh_recents(&mut self) -> Task<Message> {
        let found = self.memory.recent(memory::RECENT, Path::is_file);
        let covers: HashMap<PathBuf, Handle> = self
            .recents
            .iter()
            .filter_map(|recent| recent.cover.clone().map(|cover| (recent.path.clone(), cover)))
            .collect();
        self.recents =
            found.into_iter().map(|(path, entry)| Recent { cover: covers.get(&path).cloned(), path, entry }).collect();
        let tasks = self
            .recents
            .iter()
            .filter(|recent| recent.cover.is_none())
            .map(|recent| {
                let path = recent.path.clone();
                later(move || crate::thumbs::make(&path), {
                    let path = recent.path.clone();
                    move |made| Message::Cover(path.clone(), made)
                })
            })
            .collect::<Vec<_>>();
        Task::batch(tasks)
    }

    pub(crate) fn remember(&mut self) {
        let Some(media) = &self.media else { return };
        let Some(path) = &media.path else { return };
        if media.live || media.duration.is_none() {
            return;
        }
        let audio = media
            .selected_of(TrackKind::Audio)
            .and_then(|id| media.tracks.get(id))
            .and_then(|track| track.language.clone());
        let text = media
            .selected_of(TrackKind::Text)
            .and_then(|id| media.tracks.get(id))
            .map(|track| track.language.clone().unwrap_or_else(|| track.id.clone()));
        let entry = self.memory.entry(path);
        if media.loaded == crate::timeline::MAIN {
            entry.position = media.position.as_secs_f64();
        }
        entry.duration = media.duration.unwrap_or_default().as_secs_f64();
        entry.opened = memory::now();
        entry.volume = Some(media.volume);
        entry.rate = Some(media.rate);
        entry.audio = audio;
        entry.subtitles = Some(media.text_track.is_some() && text.is_some());
        entry.text = text;
        entry.offset = media.offset_ms;
        self.saver.save(&self.memory);
    }

    pub(crate) fn rect(&self) -> Rect {
        let size = self.window.size;
        let video = self.media.as_ref().and_then(Media::video_size);
        let Some((width, height)) = video.filter(|(width, height)| *width > 0 && *height > 0) else {
            return Rect::sized(size.width, size.height);
        };
        let aspect = width as f32 / height as f32;
        let aspect_mode = self.media.as_ref().map(|media| media.aspect).unwrap_or_default();
        match aspect_mode {
            Aspect::File => Rect::sized(size.width, size.height),
            Aspect::Fill | Aspect::Cinema => {
                let target = if aspect_mode == Aspect::Fill { size.width / size.height.max(1.0) } else { 2.39 };
                let (w, h) = if aspect_mode == Aspect::Fill {
                    if aspect > target {
                        (size.height * aspect, size.height)
                    } else {
                        (size.width, size.width / aspect)
                    }
                } else {
                    let w = size.width.max(size.height * target);
                    (w, w / aspect)
                };
                Rect {
                    x: ((size.width - w) / 2.0).round() as i32,
                    y: ((size.height - h) / 2.0).round() as i32,
                    width: w.round() as i32,
                    height: h.round() as i32,
                }
            }
        }
    }

    pub(crate) fn place(&self) {
        if let Some(player) = self.media.as_ref().and_then(|media| media.player.as_ref()) {
            player.place(self.rect());
        }
    }

    pub(crate) fn open(&mut self, uri: String, path: Option<PathBuf>) -> Task<Message> {
        self.open_with(uri, path, &Reopen::default())
    }

    fn ready_to_open(&mut self) -> bool {
        self.remember();
        if let Some(old) = &mut self.media {
            old.player = None;
            old.grabber = None;
            old.grabbers.clear();
            old.strip_grabbers.clear();
        }
        if self.handles.is_none() && self.surface.is_none() {
            return false;
        }
        if self.surface.is_none()
            && let Some(Some(handles)) = self.handles
        {
            match Surface::new(handles) {
                Ok(surface) => self.surface = Some(surface),
                Err(err) => tracing::warn!("no video surface: {err}"),
            }
        }
        true
    }

    pub(crate) fn open_with(&mut self, uri: String, path: Option<PathBuf>, reopen: &Reopen) -> Task<Message> {
        if !self.ready_to_open() {
            self.pending = Some((uri, path));
            return Task::none();
        }
        self.generation += 1;
        self.track += 1;
        self.mode = Mode::Play;
        self.exporting = None;
        self.ocr = None;
        self.inspector = false;
        let generation = self.generation;
        let mut remembered = path.as_deref().and_then(|path| self.memory.get(path).cloned());
        if let Some(resume) = reopen.resume {
            let entry = remembered.get_or_insert_with(memory::Entry::default);
            entry.position = resume.as_secs_f64();
            entry.duration = f64::MAX;
            entry.rate = Some(reopen.rate);
        }
        let subtitle = reopen
            .subtitle
            .clone()
            .or_else(|| path.as_deref().and_then(|path| sidecar(path, self.lang)).map(|sub| uri_of(&sub)));
        let target = self
            .surface
            .as_ref()
            .map(|surface| glide_video::sink::Target { display: surface.display(), surface: surface.handle() });
        let volume = self
            .media
            .as_ref()
            .map(|media| media.volume)
            .or_else(|| remembered.as_ref().and_then(|entry| entry.volume))
            .unwrap_or(0.7);
        let video_subtitles = false;
        let options = Options {
            target,
            rect: self.rect(),
            subtitle: player_subtitle(subtitle.as_deref()),
            keep_pitch: reopen.keep_pitch,
            volume,
            muted: false,
            video: true,
            video_subtitles,
        };
        let (player, failure) = match Player::open(&uri, &options) {
            Ok(player) => (Some(player), None),
            Err(failure) => (None, Some(failure)),
        };
        let events = player.as_ref().map(Player::events);
        if let Some(player) = &player {
            player.set_subtitle_offset(
                Duration::from_millis(remembered.as_ref().map_or(0, |entry| entry.offset.unsigned_abs())),
                remembered.as_ref().is_some_and(|entry| entry.offset < 0),
            );
        }
        let (grabber, results) = if path.is_some() {
            let (grabber, results) = Grabber::spawn(&uri, PREVIEW_WIDTH);
            (Some(grabber), Some(results))
        } else {
            (None, None)
        };
        let mut media = Media::new(uri.clone(), path, generation);
        media.player = player;
        media.rate = remembered.as_ref().and_then(|entry| entry.rate).unwrap_or(1.0);
        media.keep_pitch = reopen.keep_pitch;
        media.volume = volume;
        media.offset_ms = remembered.as_ref().map_or(0, |entry| entry.offset);
        media.failure = failure;
        media.grabber = grabber;
        media.remembered = remembered;
        media.suburi.clone_from(&subtitle);
        media.video_subtitles = video_subtitles;
        self.media = Some(media);
        self.chrome.shown = true;
        self.chrome.moved = Instant::now();
        let mut tasks = Vec::new();
        if let Some(events) = events {
            tasks.push(Task::run(events, move |event| Message::Player(generation, event)));
        }
        if let Some(results) = results {
            tasks.push(Task::run(results, move |frame| Message::Frame(generation, frame)));
        }
        if let Some(sidecar) = subtitle.filter(|uri| styled_sidecar(uri)) {
            tasks.push(self.load_sidecar(sidecar));
        }
        let probe = uri;
        tasks.push(later(
            move || glide_video::info::discover(&probe, Duration::from_secs(15)).ok().map(Box::new),
            move |info| Message::Info(generation, info),
        ));
        self.publish();
        Task::batch(tasks)
    }

    pub(crate) fn close_media(&mut self) {
        self.remember();
        let restored = self.set_pip(false);
        self.queued.push(restored);
        self.media = None;
        self.sync_aspect();
        self.mode = Mode::Play;
        self.exporting = None;
        self.ocr = None;
        self.inspector = false;
        if let Some(surface) = &mut self.surface {
            surface.inhibit_idle(false);
        }
        self.publish();
    }

    #[must_use]
    pub fn wanted_aspect(&self) -> (u32, u32) {
        let Some(media) = self.media.as_ref().filter(|media| media.showing_video()) else { return (0, 0) };
        match media.aspect {
            Aspect::Fill => (0, 0),
            Aspect::Cinema => (239, 100),
            Aspect::File => media.video_size().filter(|(width, height)| *width > 0 && *height > 0).unwrap_or((0, 0)),
        }
    }

    pub(crate) fn sync_aspect(&mut self) {
        let wanted = self.wanted_aspect();
        if self.aspect_sent != Some(wanted) {
            self.aspect_sent = Some(wanted);
            window_traits::request(window_traits::Call::Aspect(wanted.0, wanted.1));
        }
    }

    #[must_use]
    pub fn blur_regions(&self) -> Vec<window_traits::Region> {
        use crate::view::edit::{CLIPS_PANEL, PANEL_GAP, PANEL_RADIUS, TRIM_PANEL};
        if !self.media.as_ref().is_some_and(|media| media.showing_video() && media.failure.is_none()) {
            return Vec::new();
        }
        let covered = self.pip || self.ocr.is_some() || matches!(self.exporting, Some(Exporting::Form(_)));
        if covered {
            return Vec::new();
        }
        let size = self.window.size;
        let region = |x: f32, y: f32, width: f32, height: f32, radius: f32| window_traits::Region {
            x: x.round() as i32,
            y: y.round() as i32,
            width: width.round() as i32,
            height: height.round() as i32,
            radius: radius.round() as i32,
        };
        let panel = |height: f32| {
            region(PANEL_GAP, size.height - PANEL_GAP - height, size.width - 2.0 * PANEL_GAP, height, PANEL_RADIUS)
        };
        match self.mode {
            Mode::Trim { .. } => vec![panel(TRIM_PANEL)],
            Mode::Clips { .. } => vec![panel(CLIPS_PANEL)],
            Mode::Play if self.chrome.shown => {
                let hud = crate::view::hud_rect(size);
                let disc = |x: f32| region(x, DISC_TOP, DISC, DISC, DISC / 2.0);
                vec![
                    region(hud.x, hud.y, hud.width, hud.height, PANEL_RADIUS),
                    disc(TOPLINE_INSET),
                    disc(size.width - TOPLINE_INSET - DISC),
                    disc(size.width - TOPLINE_INSET - 2.0 * DISC - TOPLINE_GAP),
                ]
            }
            Mode::Play => Vec::new(),
        }
    }

    pub(crate) fn sync_blur(&mut self) {
        let wanted = self.blur_regions();
        if self.blur_sent.as_ref() != Some(&wanted) {
            self.blur_sent = Some(wanted.clone());
            window_traits::request(window_traits::Call::Blur(wanted));
        }
    }

    pub(crate) fn set_pip(&mut self, on: bool) -> Task<Message> {
        let video = self.media.as_ref().is_some_and(Media::showing_video);
        if on == self.pip || (on && !video) {
            return Task::none();
        }
        self.pip = on;
        self.sync_aspect();
        self.chrome.shown = true;
        self.chrome.moved = Instant::now();
        let least = if on { PIP_LEAST } else { WINDOW_LEAST };
        iced::window::latest()
            .and_then(move |id| iced::window::set_min_size(id, Some(least)))
            .chain(Task::done(Message::PipAsk(on)))
    }

    pub fn publish(&mut self) {
        let Some(server) = &mut self.mpris else { return };
        let state = match &self.media {
            Some(media) => glide_video::mpris::State {
                status: Some(media.playback),
                track: self.track,
                title: media.info.as_ref().and_then(|info| info.title.clone()).unwrap_or_else(|| media.title.clone()),
                artist: media
                    .chapter()
                    .and_then(|index| media.chapters.get(index).map(|chapter| (index, chapter)))
                    .map(|(index, chapter)| {
                        Key::ChapterLine
                            .text(self.lang)
                            .replace("{n}", &(index + 1).to_string())
                            .replace("{title}", &chapter.title)
                    })
                    .or_else(|| media.info.as_ref().and_then(|info| info.artist.clone()))
                    .unwrap_or_default(),
                album: media.info.as_ref().and_then(|info| info.album.clone()).unwrap_or_default(),
                art: media
                    .path
                    .as_deref()
                    .and_then(crate::thumbs::cached)
                    .filter(|art| art.is_file())
                    .map(|art| uri_of(&art)),
                length: media.duration.filter(|_| !media.live),
                position: media.position,
                rate: media.rate,
                volume: media.volume,
                chapters: media.chapters.len() > 1,
            },
            None => glide_video::mpris::State::default(),
        };
        server.update(state);
    }

    pub(crate) fn with_player(&self, act: impl FnOnce(&Player)) {
        if let Some(player) = self.media.as_ref().and_then(|media| media.player.as_ref()) {
            act(player);
        }
    }

    pub(crate) fn set_playing(&mut self, play: bool) {
        let Some(media) = &mut self.media else { return };
        if play && media.failure.as_ref().is_some_and(|failure| !matches!(failure, Failure::Missing { .. })) {
            return;
        }
        let edited = media.timeline.edited();
        let restart = play
            && if edited {
                media.at() >= media.timeline.total().saturating_sub(Duration::from_millis(50))
            } else {
                media.duration.is_some_and(|length| media.position + Duration::from_millis(250) >= length)
            };
        if restart {
            self.seek_timeline(Duration::ZERO, true);
        }
        let Some(media) = &mut self.media else { return };
        if let Some((at, _)) = media.handoff {
            media.handoff = Some((at, play));
        }
        if let Some(player) = &media.player {
            if play {
                player.play();
            } else {
                player.pause();
            }
        }
        self.chrome.moved = Instant::now();
        self.chrome.shown = true;
    }

    pub(crate) fn seek_timeline(&mut self, at: Duration, accurate: bool) {
        let Some(media) = &mut self.media else { return };
        let (index, source) = if media.timeline.edited() {
            match media.timeline.locate(at) {
                Some(found) => found,
                None => return,
            }
        } else {
            (0, at.min(media.duration.unwrap_or(at)))
        };
        let from = media.timeline.clips.get(index).map_or(crate::timeline::MAIN, |clip| clip.from);
        media.clip = index;
        media.position = source;
        media.cue = None;
        if from == media.loaded {
            if let Some(player) = &media.player {
                player.seek(source, accurate);
            }
        } else {
            let play = media.playing() || media.handoff.is_some_and(|(_, play)| play);
            self.switch_to(from, source, play);
        }
        if let Some(server) = &self.mpris {
            server.seeked(source);
        }
    }

    pub(crate) fn switch_to(&mut self, from: usize, at: Duration, play: bool) {
        let target = self
            .surface
            .as_ref()
            .map(|surface| glide_video::sink::Target { display: surface.display(), surface: surface.handle() });
        let rect = self.rect();
        self.generation += 1;
        let feed = self.generation;
        let Some(media) = &mut self.media else { return };
        let uri = media.timeline.uri(from, &media.uri).to_string();
        let options = Options {
            target,
            rect,
            subtitle: if from == crate::timeline::MAIN { player_subtitle(media.suburi.as_deref()) } else { None },
            keep_pitch: media.keep_pitch,
            volume: media.volume,
            muted: media.muted,
            video: !media.sound_only,
            video_subtitles: from == crate::timeline::MAIN && media.video_subtitles,
        };
        if media.loaded == crate::timeline::MAIN && media.restore.is_none() {
            media.restore = Some(media.selected.clone());
        }
        media.player = None;
        media.loaded = from;
        media.feed = feed;
        media.position = at;
        media.cue = None;
        match Player::open(&uri, &options) {
            Ok(player) => {
                let events = player.events();
                player.set_subtitle_offset(Duration::from_millis(media.offset_ms.unsigned_abs()), media.offset_ms < 0);
                media.player = Some(player);
                media.handoff = Some((at, play));
                self.queued.push(Task::run(events, move |event| Message::Player(feed, event)));
            }
            Err(failure) => {
                media.handoff = None;
                media.failure = Some(failure);
            }
        }
        self.orient();
    }

    pub(crate) fn advance(&mut self) -> bool {
        let Some(media) = &mut self.media else { return false };
        let next = media.clip + 1;
        let Some(following) = media.timeline.clips.get(next).copied() else { return false };
        media.clip = next;
        media.position = following.start;
        if following.from == media.loaded {
            if let Some(player) = &media.player {
                player.seek(following.start, true);
            }
            self.orient();
        } else {
            self.switch_to(following.from, following.start, true);
        }
        true
    }

    pub(crate) fn skip(&mut self, seconds: i32) {
        let Some(media) = &self.media else { return };
        let at = media.at();
        let target = if seconds < 0 {
            at.saturating_sub(Duration::from_secs(u64::from(seconds.unsigned_abs())))
        } else {
            at + Duration::from_secs(seconds as u64)
        };
        let target = target.min(media.length());
        self.seek_timeline(target, false);
    }

    pub(crate) fn frame_length(&self) -> Duration {
        let fps = self
            .media
            .as_ref()
            .and_then(|media| media.info.as_ref())
            .and_then(|info| info.video.as_ref())
            .map_or(25.0, |video| video.fps);
        Duration::from_secs_f64(1.0 / fps.max(1.0))
    }

    pub(crate) fn set_rate(&mut self, rate: f64) {
        let Some(media) = &mut self.media else { return };
        media.rate = rate;
        if let Some(player) = &mut media.player {
            player.set_rate(rate);
        }
        self.publish();
    }

    pub(crate) fn set_volume(&mut self, volume: f64) {
        let Some(media) = &mut self.media else { return };
        media.volume = volume.clamp(0.0, 1.0);
        media.muted = false;
        if let Some(player) = &media.player {
            player.set_volume(media.volume);
            player.set_muted(false);
        }
    }

    pub(crate) fn select_text(&mut self, pick: Option<String>) {
        let Some(media) = &mut self.media else { return };
        let ids = media.tracks.selection(&media.selected, TrackKind::Text, pick.as_deref());
        media.sidecar_on = pick.as_deref() == Some(SIDECAR);
        media.cue = None;
        if pick.is_some() {
            media.text_track = pick;
        }
        let for_player: Vec<String> = ids.iter().filter(|id| id.as_str() != SIDECAR).cloned().collect();
        media.selected.clone_from(&ids);
        if let Some(player) = media.player.as_ref().filter(|_| media.loaded == crate::timeline::MAIN) {
            player.select(&for_player);
        }
    }

    pub(crate) fn select_audio(&mut self, pick: &str) {
        let Some(media) = &mut self.media else { return };
        let ids = media.tracks.selection(&media.selected, TrackKind::Audio, Some(pick));
        if let Some(player) = media.player.as_ref().filter(|_| media.loaded == crate::timeline::MAIN) {
            player.select(&ids);
        }
    }

    pub(crate) fn cycle(&mut self, kind: TrackKind) {
        let Some(media) = &self.media else { return };
        let ids: Vec<String> = media.tracks.of(kind).map(|track| track.id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        let current = media.selected_of(kind).and_then(|id| ids.iter().position(|known| known == id));
        match kind {
            TrackKind::Text => {
                let next = match current {
                    Some(at) if at + 1 < ids.len() => Some(ids[at + 1].clone()),
                    Some(_) => None,
                    None => Some(ids[0].clone()),
                };
                self.select_text(next);
            }
            TrackKind::Audio => {
                let next = ids[current.map_or(0, |at| (at + 1) % ids.len())].clone();
                self.select_audio(&next);
            }
            TrackKind::Video => {}
        }
    }

    pub(crate) fn toggle_subtitles(&mut self) {
        let Some(media) = &self.media else { return };
        if media.selected_of(TrackKind::Text).is_some() {
            self.select_text(None);
        } else {
            let first = media.text_track.clone().or_else(|| media.text_tracks().first().map(|track| track.id.clone()));
            if first.is_some() {
                self.select_text(first);
            }
        }
    }

    pub(crate) fn apply_remembered_tracks(&mut self) {
        let Some(media) = &self.media else { return };
        let Some(entry) = media.remembered.clone() else { return };
        let audio = entry.audio.as_deref().and_then(|language| {
            media
                .tracks
                .of(TrackKind::Audio)
                .find(|track| track.language.as_deref() == Some(language))
                .map(|track| track.id.clone())
        });
        let text = entry.text.as_deref().and_then(|wanted| {
            media
                .tracks
                .of(TrackKind::Text)
                .find(|track| track.language.as_deref() == Some(wanted) || track.id == wanted)
                .map(|track| track.id.clone())
        });
        if let Some(id) = audio {
            self.select_audio(&id);
        }
        if entry.subtitles == Some(false) {
            self.select_text(None);
        } else if let Some(id) = text {
            self.select_text(Some(id));
        }
    }

    pub(crate) fn request_strip(&mut self) -> Task<Message> {
        let Some(media) = &mut self.media else { return Task::none() };
        let length = media.timeline.total();
        if length.is_zero() || media.path.is_none() {
            return Task::none();
        }
        let places: Vec<(usize, Duration)> = frames::strip_times(length, STRIP)
            .into_iter()
            .filter_map(|at| {
                let (index, source) = media.timeline.locate(at)?;
                Some((media.timeline.clips.get(index)?.from, source))
            })
            .collect();
        media.strip = vec![None; STRIP];
        media.frames.clear();
        media.grabbers.clear();
        media.strip_grabbers.clear();
        let generation = media.generation;
        let mut tasks = Vec::new();
        for from in media.timeline.sources() {
            let times: Vec<Duration> =
                places.iter().filter(|(source, _)| *source == from).map(|(_, time)| *time).collect();
            if times.is_empty() {
                continue;
            }
            let (grabber, results) = Grabber::spawn(media.timeline.uri(from, &media.uri), STRIP_WIDTH);
            grabber.request(from as u64, times, false);
            media.strip_grabbers.push(grabber);
            tasks.push(Task::run(results, move |frame| Message::Strip(generation, frame)));
        }
        Task::batch(tasks)
    }

    pub(crate) fn load_sidecar(&self, uri: String) -> Task<Message> {
        let generation = self.media.as_ref().map_or(self.generation, |media| media.generation);
        later(move || read_sidecar(&uri), move |lines| Message::SidecarLines(generation, lines))
    }

    pub(crate) fn sidecar_loaded(&mut self, generation: u64, lines: Vec<glide_video::media::Line>) {
        let Some(media) = self.media.as_mut().filter(|media| media.generation == generation) else { return };
        if lines.is_empty() {
            return;
        }
        media.sidecar_lines = lines;
        if media.tracks.get(SIDECAR).is_none() {
            media.tracks.push(sidecar_track(media.suburi.as_deref()));
        }
        let wanted = media.remembered.as_ref().is_none_or(|entry| entry.subtitles != Some(false));
        if wanted {
            self.select_text(Some(SIDECAR.to_string()));
        }
    }

    pub(crate) fn hover_grabber(&mut self, from: usize) -> Option<&Grabber> {
        let media = self.media.as_mut()?;
        if from == crate::timeline::MAIN {
            return media.grabber.as_ref();
        }
        if !media.grabbers.contains_key(&from) {
            let uri = media.timeline.uri(from, &media.uri).to_string();
            let (grabber, results) = Grabber::spawn(&uri, PREVIEW_WIDTH);
            let generation = media.generation;
            self.queued.push(Task::run(results, move |frame| Message::Frame(generation, frame)));
            media.grabbers.insert(from, grabber);
        }
        media.grabbers.get(&from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_languages_come_from_the_file_name() {
        assert_eq!(sidecar_language("file:///v/Карпати.uk.ass").as_deref(), Some("uk"));
        assert_eq!(sidecar_language("file:///v/film.eng.ssa").as_deref(), Some("eng"));
        assert_eq!(sidecar_language("file:///v/film.ass"), None);
        assert_eq!(sidecar_language("file:///v/film.2024.ass"), None);
        assert!(styled_sidecar("file:///v/A.ASS"));
        assert!(!styled_sidecar("file:///v/a.srt"));
        assert_eq!(player_subtitle(Some("file:///v/a.ass")), None);
        assert_eq!(player_subtitle(Some("file:///v/a.srt")).as_deref(), Some("file:///v/a.srt"));
    }
}

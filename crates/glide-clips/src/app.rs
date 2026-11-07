use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use glide_sdk::glide_app::menubar::Handles;
use glide_sdk::glide_theme::Accent;
use glide_sdk::widget::player::Trim;
use glide_sdk::window::{Action, Frame};
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
    pub strip_grabber: Option<Grabber>,
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
            strip_grabber: None,
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
        entry.position = media.position.as_secs_f64();
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

    pub(crate) fn open_with(&mut self, uri: String, path: Option<PathBuf>, reopen: &Reopen) -> Task<Message> {
        self.remember();
        if let Some(old) = &mut self.media {
            old.player = None;
            old.grabber = None;
            old.strip_grabber = None;
        }
        if self.handles.is_none() && self.surface.is_none() {
            self.pending = Some((uri, path));
            return Task::none();
        }
        if self.surface.is_none()
            && let Some(Some(handles)) = self.handles
        {
            match Surface::new(handles) {
                Ok(surface) => self.surface = Some(surface),
                Err(err) => tracing::warn!("no video surface: {err}"),
            }
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
        let options = Options {
            target,
            rect: self.rect(),
            subtitle: subtitle.clone(),
            keep_pitch: reopen.keep_pitch,
            volume,
            muted: false,
            video: true,
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
        media.suburi = subtitle;
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
        self.media = None;
        self.mode = Mode::Play;
        self.exporting = None;
        self.ocr = None;
        self.inspector = false;
        if let Some(surface) = &mut self.surface {
            surface.inhibit_idle(false);
        }
        self.publish();
    }

    pub(crate) fn resize_to_video(&self) -> Task<Message> {
        let Some((width, height)) = self.media.as_ref().and_then(Media::video_size) else { return Task::none() };
        if self.window.frame != Frame::Floating || width == 0 || height == 0 {
            return Task::none();
        }
        let aspect = width as f32 / height as f32;
        let (w, h) = if aspect >= 1.0 {
            let w = self.window.size.width.clamp(640.0, 1600.0);
            (w, (w / aspect).clamp(270.0, 1000.0))
        } else {
            let h = self.window.size.height.clamp(480.0, 900.0);
            ((h * aspect).max(270.0), h)
        };
        if (w - self.window.size.width).abs() < 2.0 && (h - self.window.size.height).abs() < 2.0 {
            return Task::none();
        }
        iced::window::latest().and_then(move |id| iced::window::resize(id, iced::Size::new(w.round(), h.round())))
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
        if let Some(player) = &media.player {
            if play {
                if media.timeline.edited()
                    && media.at() >= media.timeline.total().saturating_sub(Duration::from_millis(50))
                {
                    if let Some((index, source)) = media.timeline.locate(Duration::ZERO) {
                        media.clip = index;
                        player.seek(source, true);
                    }
                } else if !media.timeline.edited()
                    && media.duration.is_some_and(|length| media.position + Duration::from_millis(250) >= length)
                {
                    player.seek(Duration::ZERO, true);
                }
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
        media.clip = index;
        media.position = source;
        media.cue = None;
        if let Some(player) = &media.player {
            player.seek(source, accurate);
        }
        if let Some(server) = &self.mpris {
            server.seeked(source);
        }
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
        if pick.is_some() {
            media.text_track = pick;
        } else {
            media.cue = None;
        }
        if let Some(player) = &media.player {
            player.select(&ids);
        }
    }

    pub(crate) fn select_audio(&mut self, pick: &str) {
        let Some(media) = &mut self.media else { return };
        let ids = media.tracks.selection(&media.selected, TrackKind::Audio, Some(pick));
        if let Some(player) = &media.player {
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
        let times: Vec<Duration> = frames::strip_times(length, STRIP)
            .into_iter()
            .filter_map(|at| media.timeline.locate(at).map(|(_, source)| source))
            .collect();
        let (grabber, results) = Grabber::spawn(&media.uri, STRIP_WIDTH);
        grabber.request(0, times, false);
        media.strip = vec![None; STRIP];
        media.strip_grabber = Some(grabber);
        let generation = media.generation;
        Task::run(results, move |frame| Message::Strip(generation, frame))
    }
}

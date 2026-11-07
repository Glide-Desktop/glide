use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use glide_sdk::Shell;
use glide_sdk::widget::player::Trim;
use glide_sdk::window::Action;
use glide_video::export::{self, Progress};
use glide_video::frames;
use glide_video::media::{Failure, Kind, TrackKind};
use glide_video::player::{Event, Playback};
use iced::Task;
use iced::widget::image::Handle;

use crate::app::{App, ExportForm, Exporting, Message, Mode, Ocr, uri_of};
use crate::i18n::Key;
use crate::keys::{self, Command};
use crate::memory;

const SAVE_EVERY: u32 = 20;

#[must_use]
pub fn user_dir(key: &str, fallback: &str) -> Option<PathBuf> {
    let home = glide_config::home()?;
    let text = std::fs::read_to_string(glide_config::config_home()?.join("user-dirs.dirs")).unwrap_or_default();
    let found = text.lines().map(str::trim).filter(|line| !line.starts_with('#')).find_map(|line| {
        let (name, value) = line.split_once('=')?;
        if name.trim() != key {
            return None;
        }
        let value = value.trim().trim_matches('"');
        if let Some(rest) = value.strip_prefix("$HOME") {
            Some(home.join(rest.trim_start_matches('/')))
        } else {
            value.starts_with('/').then(|| PathBuf::from(value))
        }
    });
    Some(found.unwrap_or_else(|| home.join(fallback)))
}

fn rgba_handle(frame: frames::Frame) -> Handle {
    Handle::from_rgba(frame.width, frame.height, frame.rgba)
}

impl App {
    pub(crate) fn dispatch(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::Handles(handles) => {
                self.handles = Some(handles);
                let drops = handles.map_or_else(Task::none, |handles| {
                    Task::run(glide_sdk::glide_app::drop_target::events(&handles, "glide-clips"), Message::Dropped)
                });
                let opened = match self.pending.take() {
                    Some((uri, path)) => self.open_uri(uri, path),
                    None => Task::none(),
                };
                Task::batch([drops, opened])
            }
            Message::Dropped(dropped) => self.dropped(dropped, shell),
            Message::SidecarLines(generation, lines) => {
                self.sidecar_loaded(generation, lines);
                Task::none()
            }
            Message::Open => self.choose(),
            Message::Chosen(Some(path)) | Message::OpenPath(path) | Message::OpenRecent(path) => {
                self.open_path(path, shell)
            }
            Message::Chosen(None) | Message::Ignore => Task::none(),
            Message::AddressDialog => {
                self.address = Some(String::new());
                iced::widget::operation::focus(crate::view::ADDRESS_FIELD)
            }
            Message::AddressEdited(text) => {
                self.address = Some(text);
                Task::none()
            }
            Message::AddressConfirm => {
                let Some(address) =
                    self.address.take().map(|text| text.trim().to_string()).filter(|text| !text.is_empty())
                else {
                    return Task::none();
                };
                if let Some(path) = glide_sdk::chooser::uri_path(&address)
                    .or_else(|| address.starts_with('/').then(|| PathBuf::from(&address)))
                {
                    return self.open_path(path, shell);
                }
                self.open_uri(address, None)
            }
            Message::AddressCancel => {
                self.address = None;
                Task::none()
            }
            Message::Player(generation, event) => self.on_player(generation, event),
            Message::Info(generation, info) => self.on_info(generation, info),
            Message::Frame(generation, frame) => {
                if let Some(media) = self.media.as_mut().filter(|media| media.generation == generation) {
                    if frame.tag >= crate::app::CHAPTER_TAG {
                        media.chapter_frames.insert((frame.tag - crate::app::CHAPTER_TAG) as usize, rgba_handle(frame));
                    } else {
                        media.frames.insert(frame.tag as u32, rgba_handle(frame));
                    }
                }
                Task::none()
            }
            Message::Strip(generation, frame) => {
                if let Some(media) = self.media.as_mut().filter(|media| media.generation == generation) {
                    let times = frames::strip_times(media.timeline.total(), crate::app::STRIP);
                    let (at, from) = (frame.at, frame.tag as usize);
                    let index = times.iter().position(|time| {
                        media.timeline.locate(*time).is_some_and(|(clip, source)| {
                            source == at && media.timeline.clips.get(clip).is_some_and(|clip| clip.from == from)
                        })
                    });
                    if let Some(slot) = index.and_then(|index| media.strip.get_mut(index)) {
                        *slot = Some(rgba_handle(frame));
                    }
                }
                Task::none()
            }
            Message::Tick => self.tick(shell),
            Message::Wake => {
                self.chrome.shown = true;
                self.chrome.moved = Instant::now();
                self.chrome.pointer = shell.pointer();
                Task::none()
            }
            Message::Leave => {
                if self.media.as_ref().is_some_and(crate::app::Media::playing) && self.mode == Mode::Play {
                    self.chrome.shown = false;
                }
                Task::none()
            }
            other => self.controls(other, shell),
        }
    }

    fn controls(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::TogglePlay => {
                let playing = self.media.as_ref().is_some_and(crate::app::Media::playing);
                self.set_playing(!playing);
                Task::none()
            }
            Message::Skip(seconds) => {
                self.skip(seconds);
                Task::none()
            }
            Message::Step(forward) => {
                let frame = self.frame_length();
                self.with_player(|player| player.step(forward, frame));
                Task::none()
            }
            Message::Volume(volume) => {
                self.set_volume(f64::from(volume));
                Task::none()
            }
            Message::ToggleMute => {
                if let Some(media) = &mut self.media {
                    media.muted = !media.muted;
                    let muted = media.muted;
                    self.with_player(|player| player.set_muted(muted));
                }
                Task::none()
            }
            Message::Seek(fraction) => {
                let Some(media) = &self.media else { return Task::none() };
                let at = media.length().mul_f32(fraction.clamp(0.0, 1.0));
                self.seek_timeline(at, false);
                self.chrome.moved = Instant::now();
                Task::none()
            }
            Message::SeekReleased => {
                self.publish();
                Task::none()
            }
            Message::Hover(bucket) => self.hover(bucket),
            Message::ToggleRemaining => {
                self.remaining = !self.remaining;
                Task::none()
            }
            Message::Window(action) => {
                shell.window_action(action);
                Task::none()
            }
            Message::WindowDrag => {
                shell.window_action(Action::Drag);
                Task::none()
            }
            Message::Close => self.quit(shell),
            other => self.choices(other, shell),
        }
    }

    fn choices(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::SpeedMenu => self.menu(shell, crate::menus::Anchor::Speed),
            Message::TracksMenu => self.menu(shell, crate::menus::Anchor::Tracks),
            Message::ChaptersMenu => self.menu(shell, crate::menus::Anchor::Chapters),
            Message::MoreMenu => self.menu(shell, crate::menus::Anchor::More),
            Message::AspectMenu => self.menu(shell, crate::menus::Anchor::Aspect),
            Message::ContextMenu => {
                self.chrome.shown = true;
                shell.context_menu(crate::menus::context(self));
                Task::none()
            }
            Message::SetSpeed(rate) => {
                self.set_rate(rate);
                Task::none()
            }
            Message::KeepPitch => self.toggle_pitch(),
            Message::PickText(pick) => {
                self.select_text(pick);
                Task::none()
            }
            Message::PickAudio(id) => {
                self.select_audio(&id);
                Task::none()
            }
            Message::SubtitleOffset(delta) => {
                if let Some(media) = &mut self.media {
                    media.offset_ms = (media.offset_ms + delta).clamp(-10_000, 10_000);
                    let offset = media.offset_ms;
                    self.with_player(|player| {
                        player.set_subtitle_offset(Duration::from_millis(offset.unsigned_abs()), offset < 0);
                    });
                }
                Task::none()
            }
            Message::AddSubtitle => {
                let ask = glide_sdk::chooser::Ask {
                    purpose: glide_sdk::chooser::Purpose::Open,
                    title: self.t(Key::AddFromFile).trim_end_matches('…').to_string(),
                    accept: self.t(Key::Choose).into(),
                    name: None,
                    folder: self
                        .media
                        .as_ref()
                        .and_then(|media| media.path.as_ref())
                        .and_then(|path| path.parent())
                        .map(Path::to_path_buf),
                };
                let receiver = glide_sdk::chooser::spawn(ask);
                Task::perform(async move { receiver.await.ok() }, |chosen| match chosen {
                    Some(glide_sdk::chooser::Chosen::Paths(paths)) => Message::SubtitleChosen(paths.into_iter().next()),
                    _ => Message::Ignore,
                })
            }
            Message::SubtitleChosen(Some(path)) => self.reopen_with_subtitle(&path),
            Message::SubtitleChosen(None) => Task::none(),
            Message::GoChapter(index) => {
                let start =
                    self.media.as_ref().and_then(|media| media.chapters.get(index)).map(|chapter| chapter.start);
                if let Some(start) = start {
                    self.seek_source(start);
                }
                Task::none()
            }
            Message::ToggleRepeat => {
                if let Some(media) = &mut self.media {
                    media.repeat = !media.repeat;
                }
                Task::none()
            }
            Message::ToggleOnTop | Message::Pip => {
                shell.toast(self.t(Key::PipUnavailable));
                Task::none()
            }
            Message::SetAspect(aspect) => {
                if let Some(media) = &mut self.media {
                    media.aspect = aspect;
                }
                self.place();
                Task::none()
            }
            Message::Command(command) => self.command(command, shell),
            other => self.editing(other, shell),
        }
    }

    fn editing(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::EnterTrim => self.enter_trim(),
            Message::TrimChanged(change) => self.trim_changed(change),
            Message::TrimReset => {
                if let Mode::Trim { head, .. } = self.mode {
                    self.mode = Mode::Trim { start: 0.0, end: 1.0, head };
                }
                Task::none()
            }
            Message::TrimApply => self.trim_apply(),
            Message::TrimCancel | Message::ClipsDone => {
                self.mode = Mode::Play;
                Task::none()
            }
            Message::TrimPlay => {
                if let Mode::Trim { start, .. } = self.mode {
                    let at = self.media.as_ref().map_or(Duration::ZERO, |media| media.length().mul_f32(start));
                    self.seek_timeline(at, true);
                    self.set_playing(true);
                }
                Task::none()
            }
            Message::EnterClips => {
                self.with_player(glide_video::player::Player::pause);
                self.mode = Mode::Clips { dragging: None };
                self.request_strip()
            }
            Message::SelectClip(index) => {
                if let Some(media) = &mut self.media
                    && index < media.timeline.clips.len()
                {
                    media.timeline.selected = index;
                    let at = media.timeline.offset_of(index);
                    self.seek_timeline(at, true);
                    self.orient();
                }
                Task::none()
            }
            other => self.arranging(other, shell),
        }
    }

    fn arranging(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::HoverClip(index) => {
                self.hovered_clip = index;
                Task::none()
            }
            Message::ExportFolder => {
                let folder = match &self.exporting {
                    Some(Exporting::Form(form)) => Some(form.folder.clone()),
                    _ => None,
                };
                let ask = glide_sdk::chooser::Ask {
                    purpose: glide_sdk::chooser::Purpose::Folder,
                    title: self.t(Key::Where).into(),
                    accept: self.t(Key::Choose).into(),
                    name: None,
                    folder,
                };
                let receiver = glide_sdk::chooser::spawn(ask);
                Task::perform(async move { receiver.await.ok() }, |chosen| match chosen {
                    Some(glide_sdk::chooser::Chosen::Paths(paths)) => {
                        paths.into_iter().next().map_or(Message::Ignore, Message::ExportFolderChosen)
                    }
                    _ => Message::Ignore,
                })
            }
            Message::ExportFolderChosen(folder) => {
                if let Some(Exporting::Form(form)) = &mut self.exporting {
                    form.folder = folder;
                }
                Task::none()
            }
            Message::DragClip(dragging) => {
                if matches!(self.mode, Mode::Clips { .. }) {
                    self.mode = Mode::Clips { dragging };
                }
                Task::none()
            }
            Message::MoveClip(from, to) => {
                self.mode = Mode::Clips { dragging: None };
                if let Some(media) = &mut self.media
                    && media.timeline.reorder(from, to)
                {
                    return self.rearranged();
                }
                Task::none()
            }
            Message::SplitClip => self.split(),
            Message::RemoveClip => {
                if let Some(media) = &mut self.media {
                    let index = media.timeline.selected;
                    if media.timeline.remove(index) {
                        return self.rearranged();
                    }
                }
                Task::none()
            }
            Message::AddClip => self.choose_clip(),
            Message::ClipChosen(path) => {
                let Some(path) = path.filter(|path| path.is_file()) else { return Task::none() };
                Self::probe_clip(path)
            }
            Message::ClipProbed(path, length) => self.clip_probed(&path, length, shell),
            Message::TurnClip(clockwise) => {
                if let Some(media) = &mut self.media {
                    media.timeline.turn(clockwise);
                }
                self.orient();
                self.place();
                Task::none()
            }
            Message::FlipClip(horizontal) => {
                if let Some(media) = &mut self.media {
                    media.timeline.flip(horizontal);
                }
                self.orient();
                Task::none()
            }
            other => self.exporting(other, shell),
        }
    }

    fn exporting(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::ExportOpen => self.export_open(),
            Message::ExportName(name) => {
                if let Some(Exporting::Form(form)) = &mut self.exporting {
                    form.name = name;
                }
                Task::none()
            }
            Message::ExportQuality(quality) => {
                if let Some(Exporting::Form(form)) = &mut self.exporting {
                    form.quality = quality;
                }
                Task::none()
            }
            Message::ExportCodec(codec) => {
                if let Some(Exporting::Form(form)) = &mut self.exporting {
                    form.codec = codec;
                    if codec == export::Codec::H264 {
                        form.hdr = false;
                    }
                }
                Task::none()
            }
            Message::ExportLossless(on) => {
                if let Some(Exporting::Form(form)) = &mut self.exporting {
                    form.lossless = on;
                }
                Task::none()
            }
            Message::ExportHdr(on) => {
                if let Some(Exporting::Form(form)) = &mut self.exporting {
                    form.hdr = on && form.codec != export::Codec::H264;
                }
                Task::none()
            }
            other => self.finishing(other, shell),
        }
    }

    fn finishing(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::ExportStart => self.export_start(),
            Message::ExportProgress(progress) => self.export_progress(progress, shell),
            Message::ExportCancel => {
                match self.exporting.take() {
                    Some(Exporting::Running { handle, .. }) => {
                        handle.cancel();
                        self.exporting = None;
                    }
                    _ => self.exporting = None,
                }
                self.dock.progress(None);
                Task::none()
            }
            Message::ExportDismiss => {
                self.exporting = None;
                Task::none()
            }
            Message::OpenSaved => {
                if let Some(Exporting::Done(path)) = self.exporting.take() {
                    return self.open_path(path, shell);
                }
                Task::none()
            }
            Message::RevealSaved => {
                if let Some(Exporting::Done(path)) = &self.exporting {
                    let _ = glide_sdk::system::reveal(path, crate::APP_ID);
                }
                Task::none()
            }
            other => self.extras(other, shell),
        }
    }

    fn extras(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::Inspector(open) => {
                self.inspector = open;
                Task::none()
            }
            Message::CopyInspector => {
                let text = crate::view::states::inspector_rows(self)
                    .into_iter()
                    .map(|(key, value)| format!("{key}: {value}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                shell.toast(self.t(Key::CopiedInfo));
                iced::clipboard::write(text)
            }
            Message::CopyFrame => self.copy_frame(shell),
            Message::SaveFrame => self.save_frame(),
            Message::FrameSaved(saved) => {
                if saved.is_some() {
                    shell.toast(self.t(Key::SavedFrame));
                }
                Task::none()
            }
            Message::TextOnFrame => self.text_on_frame(),
            Message::Recognised(generation, outcome) => {
                if self.ocr.as_ref().is_some_and(|(asked, _)| *asked == generation) {
                    let state = match outcome {
                        crate::ocr::Outcome::Lines(lines) => Ocr::Lines(lines),
                        crate::ocr::Outcome::Missing => Ocr::Missing,
                        crate::ocr::Outcome::Failed => Ocr::Failed,
                    };
                    self.ocr = Some((generation, state));
                }
                Task::none()
            }
            Message::OcrClose => {
                self.ocr = None;
                Task::none()
            }
            Message::CopyText(text) => {
                shell.toast(self.t(Key::CopiedText));
                iced::clipboard::write(text)
            }
            Message::SearchText(text) => {
                let query: String =
                    text.chars().map(|character| if character.is_whitespace() { '+' } else { character }).collect();
                let _ = glide_sdk::system::open(&format!("https://duckduckgo.com/?q={query}"), crate::APP_ID);
                Task::none()
            }
            Message::CopyTime => {
                let Some(media) = &self.media else { return Task::none() };
                let text = glide_sdk::widget::player::clock(media.at(), true);
                shell.toast(self.t(Key::CopiedTime));
                iced::clipboard::write(text)
            }
            Message::ShowInFiles => {
                if let Some(path) = self.media.as_ref().and_then(|media| media.path.as_ref()) {
                    let _ = glide_sdk::system::reveal(path, crate::APP_ID);
                }
                Task::none()
            }
            Message::OpenWith => {
                if let Some(uri) = self.media.as_ref().map(|media| media.uri.clone()) {
                    let _ = std::thread::Builder::new().name("glide-clips-open-with".into()).spawn(move || {
                        if let Err(err) = crate::portal::open_with(&uri) {
                            tracing::debug!("could not ask for another app: {err}");
                        }
                    });
                }
                Task::none()
            }
            other => self.recovery(other, shell),
        }
    }

    fn recovery(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::SoundOnly => self.reopen_sound_only(),
            Message::ToggleDetails => {
                if let Some(media) = &mut self.media {
                    media.details = !media.details;
                }
                Task::none()
            }
            Message::CopyCommand(command) => {
                shell.toast(self.t(Key::CopiedText));
                iced::clipboard::write(command)
            }
            Message::Retry => {
                let Some(media) = &self.media else { return Task::none() };
                let (uri, path) = (media.uri.clone(), media.path.clone());
                self.open_uri(uri, path)
            }
            Message::FromStart => {
                if let Some(media) = &mut self.media {
                    media.failure = None;
                }
                self.seek_timeline(Duration::ZERO, true);
                self.set_playing(true);
                Task::none()
            }
            Message::ToLive => {
                let end = self
                    .media
                    .as_ref()
                    .and_then(|media| media.player.as_ref())
                    .and_then(glide_video::player::Player::duration);
                if let Some(end) = end {
                    self.seek_source(end.saturating_sub(Duration::from_secs(1)));
                }
                Task::none()
            }
            Message::Mpris(command) => self.on_mpris(command, shell),
            Message::Cover(path, made) => {
                if let Some(made) = made
                    && let Some(recent) = self.recents.iter_mut().find(|recent| recent.path == path)
                {
                    recent.cover = Some(Handle::from_path(made));
                }
                Task::none()
            }
            Message::ForgetHistory => {
                self.memory.forget();
                self.saver.save(&self.memory);
                self.recents.clear();
                Task::none()
            }
            Message::Shortcuts => {
                shell.dialog(crate::view::shortcuts(self));
                Task::none()
            }
            _ => Task::none(),
        }
    }

    pub(crate) fn open_uri(&mut self, uri: String, path: Option<PathBuf>) -> Task<Message> {
        self.open(uri, path)
    }

    pub(crate) fn open_path(&mut self, path: PathBuf, shell: &mut Shell<Message>) -> Task<Message> {
        if path.is_dir() || !path.exists() {
            shell.toast(self.t(Key::DropUnsupported));
            return Task::none();
        }
        self.open_uri(uri_of(&path), Some(path))
    }

    fn choose(&self) -> Task<Message> {
        let ask = glide_sdk::chooser::Ask {
            purpose: glide_sdk::chooser::Purpose::Open,
            title: self.t(Key::ChooseVideo).into(),
            accept: self.t(Key::Choose).into(),
            name: None,
            folder: user_dir("XDG_VIDEOS_DIR", "Videos"),
        };
        let receiver = glide_sdk::chooser::spawn(ask);
        Task::perform(async move { receiver.await.ok() }, |chosen| match chosen {
            Some(glide_sdk::chooser::Chosen::Paths(paths)) => Message::Chosen(paths.into_iter().next()),
            _ => Message::Chosen(None),
        })
    }

    pub(crate) fn quit(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        if let Some(Exporting::Running { handle, .. }) = &self.exporting {
            handle.cancel();
        }
        self.close_media();
        shell.exit();
        Task::none()
    }

    fn seek_source(&mut self, source: std::time::Duration) {
        let Some(media) = &self.media else { return };
        let at = if media.timeline.edited() {
            match media.timeline.clip_of(source, media.clip, media.loaded) {
                Some(index) => media.timeline.total_at(index, source),
                None => return,
            }
        } else {
            source
        };
        self.seek_timeline(at, true);
    }

    fn hover(&mut self, bucket: Option<u32>) -> Task<Message> {
        let Some(media) = &mut self.media else { return Task::none() };
        media.hover = bucket;
        let Some(bucket) = bucket.filter(|bucket| !media.frames.contains_key(bucket)) else { return Task::none() };
        let fraction = (bucket as f32 + 0.5) / crate::app::BUCKETS as f32;
        let at = media.length().mul_f32(fraction);
        let (from, source) = media.timeline.locate(at).map_or((crate::timeline::MAIN, at), |(index, source)| {
            (media.timeline.clips.get(index).map_or(crate::timeline::MAIN, |clip| clip.from), source)
        });
        if let Some(grabber) = self.hover_grabber(from) {
            grabber.request(u64::from(bucket), vec![source], false);
        }
        Task::none()
    }

    fn tick(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        self.ticks = self.ticks.wrapping_add(1);
        let now = Instant::now();
        let pointer = shell.pointer();
        if pointer != self.chrome.pointer {
            self.chrome.pointer = pointer;
            self.chrome.moved = now;
            if self.media.is_some() {
                self.chrome.shown = true;
            }
        }
        if let Some(Exporting::Done(_)) = &self.exporting
            && self.export_done_at.is_some_and(|at| at.elapsed() > Duration::from_secs(8))
        {
            self.exporting = None;
        }
        let Some(media) = &mut self.media else { return Task::none() };
        if let Some(player) = &media.player {
            if let Some(position) = player.position() {
                media.position = position;
            }
            if let Some(duration) = player.duration().filter(|duration| {
                !duration.is_zero() && media.loaded == crate::timeline::MAIN && media.handoff.is_none()
            }) {
                if media.duration != Some(duration) && !media.timeline.edited() {
                    media.timeline = crate::timeline::Timeline::whole(duration);
                }
                media.duration = Some(duration);
            }
            media.buffered = player.buffered().map_or(0.0, |buffered| buffered as f32);
        }
        if media.playing() {
            media.reached = media.reached.max(media.position);
        }
        if let Some((cue, shown)) = &media.cue
            && cue.duration.is_some_and(|length| shown.elapsed() > length + Duration::from_millis(100))
        {
            media.cue = None;
        }
        if media.sidecar_on && media.loaded == crate::timeline::MAIN {
            let shift = Duration::from_millis(media.offset_ms.unsigned_abs());
            let at = if media.offset_ms >= 0 { media.position.saturating_sub(shift) } else { media.position + shift };
            let text = glide_video::media::text_at(&media.sidecar_lines, at);
            if media.cue.as_ref().map(|(cue, _)| &cue.text) != text.as_ref() {
                media.cue = text.map(|text| (glide_video::player::Cue { text, duration: None }, now));
            }
        } else if media.sidecar_on {
            media.cue = None;
        }
        let boundary = media.timeline.edited().then(|| media.timeline.clips.get(media.clip).copied()).flatten();
        let ended = boundary.is_some_and(|clip| {
            media.playing() && media.handoff.is_none() && media.position + Duration::from_millis(60) >= clip.end
        });
        if ended {
            self.clip_ended();
        }
        if let Mode::Trim { end, .. } = self.mode
            && let Some(media) = &self.media
            && media.playing()
            && media.fraction() >= end
        {
            self.with_player(glide_video::player::Player::pause);
        }
        let playing = self.media.as_ref().is_some_and(crate::app::Media::playing);
        let quiet = now.duration_since(self.chrome.moved) >= crate::app::IDLE;
        if playing
            && quiet
            && self.mode == Mode::Play
            && !shell.has_dialog()
            && self.exporting.is_none()
            && self.ocr.is_none()
        {
            self.chrome.shown = false;
        }
        let video = self.media.as_ref().is_some_and(|media| media.kind == Kind::Video);
        if let Some(surface) = &mut self.surface
            && surface.inhibiting() != (playing && video)
        {
            surface.inhibit_idle(playing && video);
        }
        if self.ticks.is_multiple_of(SAVE_EVERY) && playing {
            self.remember();
        }
        self.publish();
        Task::none()
    }

    fn clip_ended(&mut self) {
        if self.advance() {
            return;
        }
        let repeat = self.media.as_ref().is_some_and(|media| media.repeat);
        if repeat {
            self.seek_timeline(Duration::ZERO, true);
        } else {
            self.with_player(glide_video::player::Player::pause);
        }
    }

    fn on_state(&mut self, playback: Playback) {
        let Some(media) = &mut self.media else { return };
        if playback != Playback::Stopped
            && let Some((at, play)) = media.handoff.take()
        {
            let rate = media.rate;
            let restore = if media.loaded == crate::timeline::MAIN { media.restore.take() } else { None };
            if let Some(player) = &mut media.player {
                if let Some(ids) = restore.filter(|ids| !ids.is_empty()) {
                    player.select(&ids);
                }
                player.seek(at, true);
                if (rate - 1.0).abs() > f64::EPSILON {
                    player.set_rate(rate);
                }
                if play {
                    player.play();
                }
            }
            media.playback = if play { Playback::Playing } else { Playback::Paused };
            self.publish();
            return;
        }
        let first = media.playback == Playback::Stopped && playback == Playback::Paused && !media.playing_since_open;
        media.playback = playback;
        if first {
            media.playing_since_open = true;
            let resume = media.remembered.as_ref().and_then(memory::Entry::resume);
            let rate = media.rate;
            if let Some(player) = &mut media.player {
                if let Some(at) = resume {
                    player.seek(at, false);
                    media.position = at;
                }
                player.play();
                if (rate - 1.0).abs() > f64::EPSILON {
                    player.set_rate(rate);
                }
            }
        }
        self.publish();
    }

    fn on_player(&mut self, feed: u64, event: Event) -> Task<Message> {
        let Some(media) = self.media.as_mut().filter(|media| media.feed == feed) else {
            return Task::none();
        };
        if media.loaded != crate::timeline::MAIN {
            return self.on_other_source(event);
        }
        match event {
            Event::State(playback) => {
                self.on_state(playback);
            }
            Event::Duration => {
                if let Some(duration) = media.player.as_ref().and_then(glide_video::player::Player::duration) {
                    media.duration = Some(duration);
                    if !media.timeline.edited() {
                        media.timeline = crate::timeline::Timeline::whole(duration);
                    }
                }
            }
            Event::Buffering(percent) => {
                media.buffering = (percent < 100).then_some(percent);
                if !media.live
                    && let Some(player) = &media.player
                {
                    if percent < 100 && media.playback == Playback::Playing {
                        player.pause();
                        media.buffer_hold = true;
                    } else if percent >= 100 && media.buffer_hold {
                        media.buffer_hold = false;
                        player.play();
                    }
                }
            }
            Event::SeekDone | Event::Title(_) => {}
            Event::Eos if media.timeline.edited() && media.clip + 1 < media.timeline.clips.len() => {
                self.clip_ended();
            }
            Event::Eos => {
                if media.repeat {
                    self.seek_timeline(Duration::ZERO, true);
                    self.set_playing(true);
                } else {
                    media.playback = Playback::Paused;
                    self.with_player(glide_video::player::Player::pause);
                    self.chrome.shown = true;
                }
                self.remember();
            }
            Event::Failed(failure) => {
                let late = media.reached > Duration::from_secs(2);
                media.failure = Some(match failure {
                    Failure::Broken(_) | Failure::Other(_) if late => Failure::Broken(String::new()),
                    other => other,
                });
                if matches!(media.failure, Some(Failure::Missing { .. })) && media.kind == Kind::Video {
                    media.sound_only = false;
                }
                self.chrome.shown = true;
            }
            Event::Chapters(chapters) => {
                if !chapters.is_empty() {
                    if let Some(grabber) = &media.grabber {
                        for (index, chapter) in chapters.iter().enumerate() {
                            grabber.request(
                                crate::app::CHAPTER_TAG + index as u64,
                                vec![chapter.start + Duration::from_secs(2)],
                                false,
                            );
                        }
                    }
                    media.chapters = chapters;
                }
            }
            Event::Tracks(tracks) => self.on_tracks(tracks),
            Event::Selected(ids) => self.on_selected(ids),
            Event::Subtitle(cue) => {
                if !media.sidecar_on {
                    media.cue = cue.map(|cue| (cue, Instant::now()));
                }
            }
            Event::Live(live) => media.live = live,
        }
        Task::none()
    }

    fn on_tracks(&mut self, tracks: glide_video::media::Tracks) {
        let Some(media) = &mut self.media else { return };
        media.kind = glide_video::player::kind_of(&tracks);
        if media.text_track.is_none() {
            media.text_track = tracks.of(TrackKind::Text).next().map(|track| track.id.clone());
        }
        let external = tracks.of(TrackKind::Text).find(|track| track.external).map(|track| track.id.clone());
        media.tracks = tracks;
        if !media.sidecar_lines.is_empty() {
            media.tracks.push(crate::app::sidecar_track(media.suburi.as_deref()));
        }
        let external = external.or_else(|| (!media.sidecar_lines.is_empty()).then(|| crate::app::SIDECAR.into()));
        if media.prefer_external
            && let Some(id) = external
        {
            media.prefer_external = false;
            self.select_text(Some(id));
        }
    }

    fn on_selected(&mut self, ids: Vec<String>) {
        let Some(media) = &mut self.media else { return };
        let first = media.selected.is_empty();
        media.selected = ids;
        if media.sidecar_on {
            let before = media.selected.len();
            media.selected.retain(|id| media.tracks.get(id).is_none_or(|track| track.kind != TrackKind::Text));
            if media.selected.len() != before
                && let Some(player) = &media.player
            {
                player.select(&media.selected);
            }
            media.selected.push(crate::app::SIDECAR.into());
        }
        if let Some(text) = media.selected_of(TrackKind::Text).map(str::to_string) {
            media.text_track = Some(text);
        }
        if first {
            self.apply_remembered_tracks();
        }
        self.follow_subtitle_style();
    }

    fn on_other_source(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::State(playback) => self.on_state(playback),
            Event::Eos => self.clip_ended(),
            Event::Failed(failure) => {
                if let Some(media) = &mut self.media {
                    media.failure = Some(failure);
                }
                self.chrome.shown = true;
            }
            Event::Buffering(percent) => {
                if let Some(media) = &mut self.media {
                    media.buffering = (percent < 100).then_some(percent);
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn on_info(&mut self, generation: u64, info: Option<Box<glide_video::info::Info>>) -> Task<Message> {
        let Some(media) = self.media.as_mut().filter(|media| media.generation == generation) else {
            return Task::none();
        };
        let Some(info) = info else { return Task::none() };
        if info.video.is_none() {
            media.kind = Kind::Audio;
        }
        media.live = info.live;
        if media.duration.is_none() {
            media.duration = info.duration;
            if let Some(duration) = info.duration {
                media.timeline = crate::timeline::Timeline::whole(duration);
            }
        }
        if media.chapters.is_empty() && !info.chapters.is_empty() {
            media.chapters.clone_from(&info.chapters);
            if let Some(grabber) = &media.grabber {
                for (index, chapter) in info.chapters.iter().enumerate() {
                    grabber.request(
                        crate::app::CHAPTER_TAG + index as u64,
                        vec![chapter.start + Duration::from_secs(2)],
                        false,
                    );
                }
            }
        }
        media.info = Some(*info);
        let cover = media.path.clone();
        self.place();
        let resize = self.resize_to_video();
        let thumb = match cover {
            Some(path) => later_cover(path),
            None => Task::none(),
        };
        Task::batch([resize, thumb])
    }

    fn command(&mut self, command: Command, shell: &mut Shell<Message>) -> Task<Message> {
        self.chrome.shown = true;
        self.chrome.moved = Instant::now();
        match command {
            Command::PlayPause => self.dispatch(Message::TogglePlay, shell),
            Command::Pause => {
                if let Some(media) = &self.media {
                    if (media.rate - 1.0).abs() > f64::EPSILON && media.playing() {
                        self.set_rate(1.0);
                    }
                    self.set_playing(false);
                }
                Task::none()
            }
            Command::Shuttle(forward) => {
                let rate = self.media.as_ref().map_or(1.0, |media| media.rate);
                let playing = self.media.as_ref().is_some_and(crate::app::Media::playing);
                let next = if playing {
                    keys::shuttle(rate, forward)
                } else if forward {
                    1.0
                } else {
                    -1.0
                };
                self.set_rate(next);
                self.set_playing(true);
                Task::none()
            }
            Command::Skip(seconds) => self.dispatch(Message::Skip(seconds), shell),
            Command::Step(forward) => self.dispatch(Message::Step(forward), shell),
            Command::Volume(delta) => {
                let volume = self.media.as_ref().map_or(0.7, |media| media.volume) + f64::from(delta);
                self.set_volume(volume);
                Task::none()
            }
            Command::Mute => self.dispatch(Message::ToggleMute, shell),
            Command::Fullscreen => self.dispatch(Message::Window(Action::ToggleFullscreen), shell),
            Command::Escape => {
                if self.ocr.is_some() {
                    self.ocr = None;
                } else if self.inspector {
                    self.inspector = false;
                } else if matches!(self.exporting, Some(Exporting::Form(_))) {
                    self.exporting = None;
                } else if self.mode != Mode::Play {
                    self.mode = Mode::Play;
                } else if self.window.frame == glide_sdk::window::Frame::Fullscreen {
                    shell.window_action(Action::ToggleFullscreen);
                }
                Task::none()
            }
            other => self.more_command(other, shell),
        }
    }

    fn more_command(&mut self, command: Command, shell: &mut Shell<Message>) -> Task<Message> {
        match command {
            Command::Subtitles => {
                self.toggle_subtitles();
                Task::none()
            }
            Command::NextText => {
                self.cycle(TrackKind::Text);
                Task::none()
            }
            Command::NextAudio => {
                self.cycle(TrackKind::Audio);
                Task::none()
            }
            Command::Slower | Command::Faster => {
                let rate = self.media.as_ref().map_or(1.0, |media| media.rate);
                self.set_rate(keys::step_speed(rate, command == Command::Faster));
                Task::none()
            }
            Command::NormalSpeed => {
                self.set_rate(1.0);
                Task::none()
            }
            Command::Chapter(forward) => {
                let Some(media) = &self.media else { return Task::none() };
                let current = media.chapter();
                let target = match (current, forward) {
                    (Some(at), true) => Some(at + 1),
                    (None, true) => Some(0),
                    (Some(at), false) => {
                        let into = media
                            .chapters
                            .get(at)
                            .map_or(Duration::ZERO, |chapter| media.position.saturating_sub(chapter.start));
                        Some(if into > Duration::from_secs(3) { at } else { at.saturating_sub(1) })
                    }
                    (None, false) => None,
                };
                match target.filter(|index| *index < media.chapters.len()) {
                    Some(index) => self.dispatch(Message::GoChapter(index), shell),
                    None => Task::none(),
                }
            }
            Command::Trim => self.enter_trim(),
            Command::Split => self.split(),
            Command::Export => self.export_open(),
            Command::Inspector => {
                self.inspector = !self.inspector;
                Task::none()
            }
            Command::Pip => self.dispatch(Message::Pip, shell),
            Command::Open => self.choose(),
            Command::OpenAddress => self.dispatch(Message::AddressDialog, shell),
            Command::Shortcuts => self.dispatch(Message::Shortcuts, shell),
            Command::MarkIn | Command::MarkOut => {
                if let Mode::Trim { start, end, head } = self.mode {
                    let least = 0.01;
                    self.mode = if command == Command::MarkIn {
                        Mode::Trim { start: head.min(end - least).max(0.0), end, head }
                    } else {
                        Mode::Trim { start, end: head.max(start + least).min(1.0), head }
                    };
                }
                Task::none()
            }
            Command::Confirm => match self.mode {
                Mode::Trim { .. } => self.trim_apply(),
                Mode::Clips { .. } => self.dispatch(Message::ClipsDone, shell),
                Mode::Play => Task::none(),
            },
            Command::Delete => self.dispatch(Message::RemoveClip, shell),
            Command::CopyFrame => self.copy_frame(shell),
            Command::TextOnFrame => self.text_on_frame(),
            Command::RotateLeft => self.dispatch(Message::TurnClip(false), shell),
            _ => Task::none(),
        }
    }

    fn on_mpris(&mut self, command: glide_video::mpris::Command, shell: &mut Shell<Message>) -> Task<Message> {
        use glide_video::mpris::Command as Remote;
        match command {
            Remote::PlayPause => self.dispatch(Message::TogglePlay, shell),
            Remote::Play => {
                self.set_playing(true);
                Task::none()
            }
            Remote::Pause | Remote::Stop => {
                self.set_playing(false);
                Task::none()
            }
            Remote::Next => self.command(Command::Chapter(true), shell),
            Remote::Previous => self.command(Command::Chapter(false), shell),
            Remote::SeekBy(micros) => {
                let Some(media) = &self.media else { return Task::none() };
                let offset = Duration::from_micros(micros.unsigned_abs());
                let at = if micros < 0 { media.at().saturating_sub(offset) } else { media.at() + offset };
                let at = at.min(media.length());
                self.seek_timeline(at, false);
                Task::none()
            }
            Remote::SetPosition(position) => {
                self.seek_source(position);
                Task::none()
            }
            Remote::Raise => {
                shell.window_action(Action::Activate);
                Task::none()
            }
            Remote::Quit => self.quit(shell),
        }
    }

    fn menu(&mut self, shell: &mut Shell<Message>, anchor: crate::menus::Anchor) -> Task<Message> {
        self.chrome.shown = true;
        self.chrome.moved = Instant::now();
        let (at, items) = crate::menus::build(self, anchor);
        shell.popover_at(at, items, crate::menus::POPOVER_LEAST);
        Task::none()
    }

    fn reopen(&self) -> Option<(String, Option<PathBuf>, crate::app::Reopen)> {
        let media = self.media.as_ref()?;
        let reopen = crate::app::Reopen {
            subtitle: media.suburi.clone(),
            keep_pitch: media.keep_pitch,
            resume: Some(media.position),
            rate: media.rate,
        };
        Some((media.uri.clone(), media.path.clone(), reopen))
    }

    fn reload(&mut self) {
        let Some(media) = &self.media else { return };
        if media.live {
            if let Some((uri, path, reopen)) = self.reopen() {
                let opened = self.open_with(uri, path, &reopen);
                self.queued.push(opened);
            }
            return;
        }
        let play = media.playing() || media.handoff.is_some_and(|(_, play)| play);
        let (loaded, position) = (media.loaded, media.position);
        self.switch_to(loaded, position, play);
    }

    fn toggle_pitch(&mut self) -> Task<Message> {
        let Some(media) = &mut self.media else { return Task::none() };
        media.keep_pitch = !media.keep_pitch;
        self.reload();
        Task::none()
    }

    fn reopen_with_subtitle(&mut self, subtitle: &Path) -> Task<Message> {
        let Some(media) = &mut self.media else { return Task::none() };
        let uri = uri_of(subtitle);
        media.suburi = Some(uri.clone());
        if crate::app::styled_sidecar(&uri) {
            media.sidecar_lines.clear();
            media.tracks.retain(|track| track.id != crate::app::SIDECAR);
            return self.load_sidecar(uri);
        }
        media.prefer_external = true;
        if media.loaded == crate::timeline::MAIN {
            self.reload();
            if let Some(media) = &mut self.media {
                media.restore = None;
            }
        }
        Task::none()
    }

    fn follow_subtitle_style(&mut self) {
        let Some(media) = &self.media else { return };
        if media.loaded != crate::timeline::MAIN || media.handoff.is_some() || media.sound_only {
            return;
        }
        let styled = media
            .selected_of(TrackKind::Text)
            .filter(|id| *id != crate::app::SIDECAR)
            .and_then(|id| media.tracks.get(id))
            .is_some_and(|track| {
                glide_video::player::styled_subtitles(track.codec.as_deref())
                    || glide_video::player::styled_subtitles(track.format.as_deref())
            });
        if styled != media.video_subtitles {
            if let Some(media) = &mut self.media {
                media.video_subtitles = styled;
            }
            self.reload();
        }
    }

    fn reopen_sound_only(&mut self) -> Task<Message> {
        let Some(media) = &mut self.media else { return Task::none() };
        media.failure = None;
        media.sound_only = true;
        let options = glide_video::player::Options {
            video: false,
            volume: media.volume,
            ..glide_video::player::Options::default()
        };
        match glide_video::player::Player::open(&media.uri, &options) {
            Ok(player) => {
                let events = player.events();
                self.generation += 1;
                let generation = self.generation;
                media.feed = generation;
                media.loaded = crate::timeline::MAIN;
                media.handoff = None;
                media.player = Some(player);
                media.playback = Playback::Stopped;
                media.playing_since_open = false;
                Task::run(events, move |event| Message::Player(generation, event))
            }
            Err(failure) => {
                media.failure = Some(failure);
                Task::none()
            }
        }
    }

    fn enter_trim(&mut self) -> Task<Message> {
        let Some(media) = &self.media else { return Task::none() };
        if media.live || media.length().is_zero() {
            return Task::none();
        }
        let head = media.fraction();
        self.set_playing(false);
        self.mode = Mode::Trim { start: 0.0, end: 1.0, head };
        self.request_strip()
    }

    fn trim_changed(&mut self, change: Trim) -> Task<Message> {
        let Mode::Trim { start, end, head } = self.mode else { return Task::none() };
        let length = self.media.as_ref().map_or(Duration::ZERO, crate::app::Media::length);
        match change {
            Trim::Start(value) => {
                self.mode = Mode::Trim { start: value, end, head: head.max(value) };
                self.seek_timeline(length.mul_f32(value), false);
            }
            Trim::End(value) => {
                self.mode = Mode::Trim { start, end: value, head: head.min(value) };
                self.seek_timeline(length.mul_f32(value), false);
            }
            Trim::Playhead(value) => {
                self.mode = Mode::Trim { start, end, head: value };
                self.seek_timeline(length.mul_f32(value), true);
            }
            Trim::Released => {
                let head = match self.mode {
                    Mode::Trim { head, .. } => head,
                    _ => head,
                };
                self.seek_timeline(length.mul_f32(head), true);
            }
        }
        Task::none()
    }

    fn trim_apply(&mut self) -> Task<Message> {
        let Mode::Trim { start, end, .. } = self.mode else { return Task::none() };
        self.mode = Mode::Play;
        let Some(media) = &mut self.media else { return Task::none() };
        let length = media.length();
        media.timeline.trim(length.mul_f32(start), length.mul_f32(end));
        media.clip = 0;
        self.seek_timeline(Duration::ZERO, true);
        Task::none()
    }

    fn dropped(
        &mut self, dropped: glide_sdk::glide_app::drop_target::Dropped, shell: &mut Shell<Message>,
    ) -> Task<Message> {
        use glide_sdk::glide_app::drop_target::Dropped;
        let entries = match dropped {
            Dropped::Hover(hover) => {
                self.dropping = hover.is_some();
                return Task::none();
            }
            Dropped::Uris(uris, _) => uris,
            Dropped::Text(text, _) => text.lines().map(str::trim).map(str::to_string).collect(),
        };
        self.dropping = false;
        let Some(target) = entries.iter().find_map(|entry| dropped_target(entry)) else {
            shell.toast(self.t(Key::DropUnsupported));
            return Task::none();
        };
        match target {
            (_, Some(path)) if matches!(self.mode, Mode::Clips { .. }) && self.media.is_some() => {
                Self::probe_clip(path)
            }
            (_, Some(path)) => self.open_path(path, shell),
            (uri, None) => self.open_uri(uri, None),
        }
    }

    fn probe_clip(path: PathBuf) -> Task<Message> {
        let uri = uri_of(&path);
        later(
            move || {
                let length =
                    glide_video::info::discover(&uri, Duration::from_secs(15)).ok().and_then(|info| info.duration);
                (path, length)
            },
            |(path, length)| Message::ClipProbed(path, length),
        )
    }

    fn rearranged(&mut self) -> Task<Message> {
        if let Some(media) = &self.media {
            let at = media.timeline.offset_of(media.timeline.selected);
            self.seek_timeline(at, true);
            self.orient();
        }
        self.request_strip()
    }

    fn choose_clip(&mut self) -> Task<Message> {
        self.set_playing(false);
        let folder = self
            .media
            .as_ref()
            .and_then(|media| media.path.as_deref())
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| user_dir("XDG_VIDEOS_DIR", "Videos"));
        let ask = glide_sdk::chooser::Ask {
            purpose: glide_sdk::chooser::Purpose::Open,
            title: self.t(Key::AddClip).into(),
            accept: self.t(Key::Choose).into(),
            name: None,
            folder,
        };
        let receiver = glide_sdk::chooser::spawn(ask);
        Task::perform(async move { receiver.await.ok() }, |chosen| match chosen {
            Some(glide_sdk::chooser::Chosen::Paths(paths)) => Message::ClipChosen(paths.into_iter().next()),
            _ => Message::ClipChosen(None),
        })
    }

    fn clip_probed(&mut self, path: &Path, length: Option<Duration>, shell: &mut Shell<Message>) -> Task<Message> {
        let Some(media) = &mut self.media else { return Task::none() };
        let added = length.is_some_and(|length| {
            media.timeline.append(crate::timeline::Source {
                uri: uri_of(path),
                title: crate::app::title_of(&uri_of(path), Some(path)),
                path: Some(path.to_path_buf()),
                length,
            })
        });
        if !added {
            shell.toast(self.t(Key::ClipUnreadable));
            return Task::none();
        }
        if !matches!(self.mode, Mode::Clips { .. }) {
            self.mode = Mode::Clips { dragging: None };
        }
        self.rearranged()
    }

    fn split(&mut self) -> Task<Message> {
        let Some(media) = &mut self.media else { return Task::none() };
        if media.live {
            return Task::none();
        }
        let at = media.at();
        if media.timeline.split(at) {
            media.clip = media.timeline.selected;
            if !matches!(self.mode, Mode::Clips { .. }) {
                self.mode = Mode::Clips { dragging: None };
            }
            return self.request_strip();
        }
        Task::none()
    }

    pub(crate) fn orient(&self) {
        let Some(media) = &self.media else { return };
        let transform = media.timeline.clips.get(media.clip).map(|clip| clip.transform).unwrap_or_default();
        if let Some(player) = &media.player {
            player.orient(transform);
        }
    }

    fn export_open(&mut self) -> Task<Message> {
        let Some(media) = &self.media else { return Task::none() };
        if media.path.is_none() || media.live {
            return Task::none();
        }
        self.set_playing(false);
        let media = self.media.as_ref().expect("checked above");
        let stem = media
            .path
            .as_deref()
            .and_then(Path::file_stem)
            .map_or_else(|| media.title.clone(), |stem| stem.to_string_lossy().into_owned());
        let name = if media.timeline.edited() { format!("{stem} — {}", self.t(Key::Trimmed)) } else { stem };
        let short = media.video_size().map_or(1080, |(width, height)| width.min(height));
        let quality = if media.kind == Kind::Audio {
            export::Quality::AudioOnly
        } else if short >= 2000 {
            export::Quality::P2160
        } else if short >= 1000 {
            export::Quality::P1080
        } else {
            export::Quality::P720
        };
        let folder = user_dir("XDG_VIDEOS_DIR", "Videos").unwrap_or_else(|| PathBuf::from("/tmp"));
        self.exporting = Some(Exporting::Form(ExportForm {
            name,
            folder,
            quality,
            codec: export::Codec::H264,
            lossless: false,
            hdr: false,
        }));
        Task::none()
    }

    fn export_start(&mut self) -> Task<Message> {
        let Some(Exporting::Form(form)) = &self.exporting else { return Task::none() };
        let Some(media) = &self.media else { return Task::none() };
        let segments = media.timeline.segments(&media.uri);
        let lossless = form.lossless && export::lossless_possible(&segments);
        let (width, height) = media.video_size().unwrap_or((0, 0));
        let mut job = export::Job {
            segments,
            output: PathBuf::new(),
            quality: form.quality,
            codec: form.codec,
            lossless,
            source_width: width,
            source_height: height,
        };
        let _ = std::fs::create_dir_all(&form.folder);
        let name = form.name.trim().to_string();
        let name = if name.is_empty() { media.title.clone() } else { name };
        job.output = export::unique(&form.folder, &name, export::extension(&job));
        let file = job.output.file_name().map_or_else(String::new, |file| file.to_string_lossy().into_owned());
        let (handle, receiver) = export::spawn(job);
        self.exporting =
            Some(Exporting::Running { handle, progress: 0.0, started: Instant::now(), hardware: false, name: file });
        self.dock.progress(Some(0.0));
        Task::run(receiver, Message::ExportProgress)
    }

    fn export_progress(&mut self, progress: Progress, shell: &mut Shell<Message>) -> Task<Message> {
        match progress {
            Progress::Fraction(fraction) => {
                if let Some(Exporting::Running { progress, .. }) = &mut self.exporting {
                    *progress = fraction;
                }
                self.dock.progress(Some(fraction));
            }
            Progress::Hardware(on) => {
                if let Some(Exporting::Running { hardware, .. }) = &mut self.exporting {
                    *hardware = on;
                }
            }
            Progress::Done(path) => {
                self.exporting = Some(Exporting::Done(path));
                self.export_done_at = Some(Instant::now());
                self.dock.progress(None);
            }
            Progress::Failed(reason) => {
                tracing::warn!("export failed: {reason}");
                self.exporting = None;
                self.dock.progress(None);
                shell.toast(self.t(Key::ExportFailed));
            }
            Progress::Cancelled => {
                self.exporting = None;
                self.dock.progress(None);
            }
        }
        Task::none()
    }

    fn current_frame(&self) -> Option<image::RgbaImage> {
        let sample = self.media.as_ref()?.player.as_ref()?.frame()?;
        crate::still::rgba(&sample)
    }

    fn copy_frame(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        let Some(frame) = self.current_frame() else { return Task::none() };
        let mut png = Vec::new();
        if image::DynamicImage::ImageRgba8(frame)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .is_err()
        {
            return Task::none();
        }
        if let Some(Some(handles)) = self.handles {
            self.clipboard.get_or_insert_with(|| glide_video::clipboard::Clipboard::new(handles)).offer_png(png);
            shell.toast(self.t(Key::CopiedFrame));
        }
        Task::none()
    }

    fn save_frame(&mut self) -> Task<Message> {
        let Some(frame) = self.current_frame() else { return Task::none() };
        let Some(media) = &self.media else { return Task::none() };
        let folder = user_dir("XDG_PICTURES_DIR", "Pictures").unwrap_or_else(|| PathBuf::from("/tmp"));
        let stem = media
            .path
            .as_deref()
            .and_then(Path::file_stem)
            .map_or_else(|| media.title.clone(), |stem| stem.to_string_lossy().into_owned());
        let time = glide_sdk::widget::player::clock(media.at(), true).replace(':', "-");
        let name = format!("{stem} {time}");
        later(
            move || {
                let _ = std::fs::create_dir_all(&folder);
                let path = export::unique(&folder, &name, "png");
                frame.save_with_format(&path, image::ImageFormat::Png).ok().map(|()| path)
            },
            Message::FrameSaved,
        )
    }

    fn text_on_frame(&mut self) -> Task<Message> {
        let Some(frame) = self.current_frame() else { return Task::none() };
        self.set_playing(false);
        self.generation += 1;
        let asked = self.generation;
        self.ocr = Some((asked, Ocr::Busy));
        let (width, height) = frame.dimensions();
        later(
            move || {
                let path = std::env::temp_dir().join(format!("glide-clips-frame-{}.png", std::process::id()));
                if frame.save_with_format(&path, image::ImageFormat::Png).is_err() {
                    return crate::ocr::Outcome::Failed;
                }
                let outcome = crate::ocr::recognise(&path, width, height);
                let _ = std::fs::remove_file(&path);
                outcome
            },
            move |outcome| Message::Recognised(asked, outcome),
        )
    }
}

fn later_cover(path: PathBuf) -> Task<Message> {
    let key = path.clone();
    Task::perform(
        async move {
            let (sender, receiver) = iced::futures::channel::oneshot::channel();
            let _ = std::thread::Builder::new().name("glide-clips-thumb".into()).spawn(move || {
                let _ = sender.send(crate::thumbs::make(&path));
            });
            receiver.await.ok().flatten()
        },
        move |made| Message::Cover(key, made),
    )
}

#[must_use]
pub fn dropped_target(entry: &str) -> Option<(String, Option<PathBuf>)> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    if entry.starts_with("file:") {
        let path = glide_sdk::chooser::uri_path(entry)?;
        return path.is_file().then(|| (uri_of(&path), Some(path)));
    }
    if entry.starts_with('/') {
        let path = PathBuf::from(entry);
        return path.is_file().then(|| (uri_of(&path), Some(path)));
    }
    let scheme = entry.split_once("://").map(|(scheme, _)| scheme.to_ascii_lowercase())?;
    matches!(scheme.as_str(), "http" | "https" | "rtsp" | "rtsps" | "rtmp" | "mms" | "srt" | "udp" | "rtp")
        .then(|| (entry.to_string(), None))
}

fn later<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static, done: impl Fn(T) -> Message + Send + 'static,
) -> Task<Message> {
    Task::perform(
        async move {
            let (sender, receiver) = iced::futures::channel::oneshot::channel();
            let _ = std::thread::Builder::new().name("glide-clips-work".into()).spawn(move || {
                let _ = sender.send(job());
            });
            receiver.await.ok()
        },
        move |result| result.map_or(Message::Ignore, &done),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_open_local_files_and_stream_addresses_only() {
        let file = std::env::temp_dir().join(format!("glide-clips-drop-{}.mkv", std::process::id()));
        std::fs::write(&file, b"x").unwrap();
        let uri = uri_of(&file);
        assert_eq!(dropped_target(&uri), Some((uri.clone(), Some(file.clone()))));
        assert_eq!(dropped_target(&file.to_string_lossy()), Some((uri, Some(file.clone()))));
        assert_eq!(dropped_target("file:///nonexistent/glide/a.mkv"), None);
        assert_eq!(dropped_target("file://remote/etc/hosts"), None);
        assert_eq!(
            dropped_target("https://tv.example.ua/live.m3u8"),
            Some(("https://tv.example.ua/live.m3u8".into(), None))
        );
        assert_eq!(dropped_target("javascript:alert(1)"), None);
        assert_eq!(dropped_target("ftp://example.org/a.mkv"), None);
        assert_eq!(dropped_target("  "), None);
        std::fs::remove_file(file).unwrap();
    }
}

use std::time::Duration;

use futures_util::{Stream, StreamExt};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSink;

use crate::media::{Chapter, Failure, Kind, Track, TrackKind, Tracks};
use crate::sink::{Rect, Target};

const SUBTITLE: &str = "glide-subtitle";
const NANOS: u64 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playback {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub text: String,
    pub duration: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    State(Playback),
    Duration,
    Buffering(u8),
    SeekDone,
    Eos,
    Failed(Failure),
    Chapters(Vec<Chapter>),
    Tracks(Tracks),
    Selected(Vec<String>),
    Subtitle(Option<Cue>),
    Title(String),
    Live(bool),
}

#[derive(Debug, Clone)]
pub struct Options {
    pub target: Option<Target>,
    pub rect: Rect,
    pub subtitle: Option<String>,
    pub keep_pitch: bool,
    pub volume: f64,
    pub muted: bool,
    pub video: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            target: None,
            rect: Rect::default(),
            subtitle: None,
            keep_pitch: true,
            volume: 1.0,
            muted: false,
            video: true,
        }
    }
}

pub struct Player {
    playbin: gst::Element,
    sink: Option<gst::Element>,
    rate: f64,
    collection: Option<gst::StreamCollection>,
}

fn element(name: &str) -> Result<gst::Element, Failure> {
    gst::ElementFactory::make(name).build().map_err(|_| Failure::Missing { what: name.to_string(), detail: None })
}

fn text_sink(bus: gst::Bus) -> gst::Element {
    let caps = gst::Caps::builder("text/x-raw").build();
    let mut all = caps;
    all.merge(gst::Caps::builder("application/x-ass").build());
    all.merge(gst::Caps::builder("application/x-ssa").build());
    let sink = AppSink::builder().caps(&all).sync(true).max_buffers(4).build();
    sink.set_callbacks(
        gstreamer_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let ass = sample
                    .caps()
                    .and_then(|caps| {
                        caps.structure(0).map(|structure| structure.name().as_str().starts_with("application/x-"))
                    })
                    .unwrap_or(false);
                let Some(buffer) = sample.buffer() else { return Ok(gst::FlowSuccess::Ok) };
                let Ok(map) = buffer.map_readable() else { return Ok(gst::FlowSuccess::Ok) };
                let raw = String::from_utf8_lossy(map.as_slice()).into_owned();
                let text = if ass { crate::media::ass_text(&raw) } else { crate::media::plain_text(&raw) };
                let duration = buffer.duration().map_or(0, gst::ClockTime::nseconds);
                let structure =
                    gst::Structure::builder(SUBTITLE).field("text", text).field("duration", duration).build();
                let _ = bus.post(gst::message::Application::new(structure));
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
    sink.upcast()
}

impl Player {
    pub fn open(uri: &str, options: &Options) -> Result<Self, Failure> {
        crate::init().map_err(Failure::Other)?;
        let playbin = element("playbin3")?;
        let bus = playbin.bus().ok_or_else(|| Failure::Other("no bus".into()))?;
        playbin.set_property("uri", uri);
        if let Some(subtitle) = &options.subtitle {
            playbin.set_property("suburi", subtitle);
        }
        let sink = if options.video {
            match options.target {
                Some(target) => Some(crate::sink::wayland(target, options.rect).map_err(Failure::Other)?),
                None => Some(
                    gst::ElementFactory::make("fakesink")
                        .property("sync", true)
                        .build()
                        .map_err(|err| Failure::Other(err.to_string()))?,
                ),
            }
        } else {
            None
        };
        if let Some(sink) = &sink {
            playbin.set_property("video-sink", sink);
        } else {
            playbin.set_property_from_str("flags", "audio+text+soft-volume+buffering");
        }
        playbin.set_property("text-sink", text_sink(bus));
        if options.keep_pitch
            && let Ok(tempo) = element("scaletempo")
        {
            playbin.set_property("audio-filter", tempo);
        }
        playbin.set_property("volume", options.volume.clamp(0.0, 1.0));
        playbin.set_property("mute", options.muted);
        if playbin.set_state(gst::State::Paused).is_err() {
            tracing::debug!("playbin refused to preroll; the bus carries the reason");
        }
        Ok(Self { playbin, sink, rate: 1.0, collection: None })
    }

    pub fn events(&self) -> impl Stream<Item = Event> + Send + 'static {
        let bus = self.playbin.bus().expect("playbin always has a bus");
        let playbin = self.playbin.downgrade();
        bus.stream().filter_map(move |message| {
            let event = playbin.upgrade().and_then(|playbin| translate(&playbin, &message));
            async move { event }
        })
    }

    #[must_use]
    pub fn poll(&self, timeout: Duration) -> Option<Event> {
        let bus = self.playbin.bus()?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let left = deadline.checked_duration_since(std::time::Instant::now())?;
            let message = bus.timed_pop(gst::ClockTime::from_nseconds(left.as_nanos() as u64))?;
            if let Some(event) = translate(&self.playbin, &message) {
                return Some(event);
            }
        }
    }

    pub fn play(&self) {
        let _ = self.playbin.set_state(gst::State::Playing);
    }

    pub fn pause(&self) {
        let _ = self.playbin.set_state(gst::State::Paused);
    }

    #[must_use]
    pub fn position(&self) -> Option<Duration> {
        self.playbin.query_position::<gst::ClockTime>().map(|time| Duration::from_nanos(time.nseconds()))
    }

    #[must_use]
    pub fn duration(&self) -> Option<Duration> {
        self.playbin.query_duration::<gst::ClockTime>().map(|time| Duration::from_nanos(time.nseconds()))
    }

    #[must_use]
    pub fn buffered(&self) -> Option<f64> {
        let mut query = gst::query::Buffering::new(gst::Format::Percent);
        if !self.playbin.query(&mut query) {
            return None;
        }
        let (_, stop, _) = query.range();
        let stop = stop.value();
        (stop >= 0).then(|| stop as f64 / f64::from(gst::ffi::GST_FORMAT_PERCENT_MAX as u32))
    }

    pub fn seek(&self, to: Duration, accurate: bool) {
        let flags = if accurate {
            gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE
        } else {
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT | gst::SeekFlags::SNAP_NEAREST
        };
        self.seek_rate(to, self.rate, flags);
    }

    fn seek_rate(&self, to: Duration, rate: f64, flags: gst::SeekFlags) {
        let at = gst::ClockTime::from_nseconds(to.as_nanos() as u64);
        let result = if rate >= 0.0 {
            self.playbin.seek(rate, flags, gst::SeekType::Set, at, gst::SeekType::None, gst::ClockTime::NONE)
        } else {
            self.playbin.seek(rate, flags, gst::SeekType::Set, gst::ClockTime::ZERO, gst::SeekType::Set, at)
        };
        if let Err(err) = result {
            tracing::debug!("seek: {err}");
        }
    }

    pub fn set_rate(&mut self, rate: f64) {
        if (rate - self.rate).abs() < f64::EPSILON {
            return;
        }
        self.rate = rate;
        let at = self.position().unwrap_or_default();
        self.seek_rate(at, rate, gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE);
    }

    #[must_use]
    pub fn rate(&self) -> f64 {
        self.rate
    }

    pub fn step(&self, forward: bool, frame: Duration) {
        self.pause();
        if forward {
            let event = gst::event::Step::new(gst::format::Buffers::ONE, 1.0, true, false);
            if !self.sink.as_ref().is_some_and(|sink| sink.send_event(event)) {
                let at = self.position().unwrap_or_default() + frame;
                self.seek(at, true);
            }
        } else {
            let at = self.position().unwrap_or_default().saturating_sub(frame);
            self.seek(at, true);
        }
    }

    pub fn set_volume(&self, volume: f64) {
        self.playbin.set_property("volume", volume.clamp(0.0, 1.0));
    }

    pub fn set_muted(&self, muted: bool) {
        self.playbin.set_property("mute", muted);
    }

    pub fn set_subtitle_offset(&self, offset: Duration, earlier: bool) {
        let nanos = offset.as_nanos() as i64;
        self.playbin.set_property("text-offset", if earlier { -nanos } else { nanos });
    }

    pub fn orient(&self, transform: crate::export::Transform) {
        let Some(sink) = self.sink.as_ref().filter(|sink| sink.has_property("rotate-method")) else { return };
        let nick = match transform.method() {
            "clockwise" => "90r",
            "rotate-180" => "180",
            "counterclockwise" => "90l",
            "horizontal-flip" => "horiz",
            "vertical-flip" => "vert",
            "upper-left-diagonal" => "ul-lr",
            "upper-right-diagonal" => "ur-ll",
            _ => "identity",
        };
        sink.set_property_from_str("rotate-method", nick);
    }

    pub fn place(&self, rect: Rect) {
        if let Some(sink) = &self.sink {
            crate::sink::place(sink, rect);
        }
    }

    pub fn remember(&mut self, collection: gst::StreamCollection) {
        self.collection = Some(collection);
    }

    pub fn select(&self, ids: &[String]) {
        let event = gst::event::SelectStreams::new(ids.iter().map(String::as_str));
        if !self.playbin.send_event(event) {
            tracing::debug!("stream selection was refused");
        }
    }

    #[must_use]
    pub fn frame(&self) -> Option<gst::Sample> {
        self.sink.as_ref().and_then(|sink| sink.property::<Option<gst::Sample>>("last-sample"))
    }

    #[must_use]
    pub fn decoder(&self) -> Option<(String, bool)> {
        let bin = self.playbin.dynamic_cast_ref::<gst::Bin>()?;
        bin.iterate_recurse().into_iter().filter_map(Result::ok).find_map(|element| {
            let factory = element.factory()?;
            let klass = factory.metadata("klass").unwrap_or_default().to_string();
            if !klass.contains("Decoder") || !klass.contains("Video") {
                return None;
            }
            let name = factory.name().to_string();
            let hardware = klass.contains("Hardware")
                || name.starts_with("va")
                || name.starts_with("nv")
                || name.starts_with("v4l2");
            Some((name, hardware))
        })
    }

    #[must_use]
    pub fn dropped(&self) -> Option<u64> {
        let sink = self.sink.as_ref().filter(|sink| sink.has_property("stats"))?;
        sink.property::<gst::Structure>("stats").get::<u64>("dropped").ok()
    }

    #[must_use]
    pub fn element(&self) -> &gst::Element {
        &self.playbin
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.playbin.set_state(gst::State::Null);
    }
}

fn translate(playbin: &gst::Element, message: &gst::Message) -> Option<Event> {
    use gst::MessageView;
    let from_playbin = message.src().is_some_and(|src| src == playbin);
    match message.view() {
        MessageView::StateChanged(change) if from_playbin => Some(Event::State(match change.current() {
            gst::State::Playing => Playback::Playing,
            gst::State::Paused => Playback::Paused,
            _ => Playback::Stopped,
        })),
        MessageView::DurationChanged(_) => Some(Event::Duration),
        MessageView::Buffering(buffering) => Some(Event::Buffering(buffering.percent().clamp(0, 100) as u8)),
        MessageView::AsyncDone(_) => Some(Event::SeekDone),
        MessageView::Eos(_) => Some(Event::Eos),
        MessageView::Error(error) => {
            Some(Event::Failed(crate::media::classify(&error.error(), error.debug().as_deref())))
        }
        MessageView::Element(element) => {
            let missing = gstreamer_pbutils::MissingPluginMessage::parse(element).ok()?;
            Some(Event::Failed(Failure::Missing {
                what: missing.description().to_string(),
                detail: Some(missing.installer_detail().to_string()),
            }))
        }
        MessageView::Toc(toc) => Some(Event::Chapters(crate::media::chapters(&toc.toc().0))),
        MessageView::StreamCollection(collection) => Some(Event::Tracks(tracks(&collection.stream_collection()))),
        MessageView::StreamsSelected(selected) => Some(Event::Selected(
            selected.streams().filter_map(|stream| stream.stream_id().map(|id| id.to_string())).collect(),
        )),
        MessageView::Tag(tag) => {
            let tags = tag.tags();
            tags.get::<gst::tags::Title>().map(|title| Event::Title(title.get().to_string()))
        }
        MessageView::Latency(_) => {
            let _ = playbin.dynamic_cast_ref::<gst::Bin>().map(gst::prelude::GstBinExt::recalculate_latency);
            None
        }
        MessageView::Application(application) => {
            let structure = application.structure()?;
            if structure.name() != SUBTITLE {
                return None;
            }
            let text = structure.get::<String>("text").ok()?;
            let nanos = structure.get::<u64>("duration").unwrap_or(0);
            if text.trim().is_empty() {
                return Some(Event::Subtitle(None));
            }
            let duration = (nanos > 0 && nanos < 60 * NANOS).then(|| Duration::from_nanos(nanos));
            Some(Event::Subtitle(Some(Cue { text, duration })))
        }
        _ => None,
    }
}

#[must_use]
pub fn tracks(collection: &gst::StreamCollection) -> Tracks {
    let mut tracks = Tracks::default();
    for stream in collection {
        let Some(id) = stream.stream_id().map(|id| id.to_string()) else { continue };
        let tags = stream.tags();
        let language =
            tags.as_ref().and_then(|tags| tags.get::<gst::tags::LanguageCode>()).map(|tag| tag.get().to_string());
        let title = tags.as_ref().and_then(|tags| tags.get::<gst::tags::Title>()).map(|tag| tag.get().to_string());
        let codec = tags
            .as_ref()
            .and_then(|tags| {
                tags.get::<gst::tags::AudioCodec>()
                    .or_else(|| tags.get::<gst::tags::SubtitleCodec>())
                    .or_else(|| tags.get::<gst::tags::VideoCodec>())
                    .or_else(|| tags.get::<gst::tags::Codec>())
            })
            .map(|tag| tag.get().to_string());
        let caps = stream.caps();
        let structure = caps.as_ref().and_then(|caps| caps.structure(0));
        let channels = structure.and_then(|structure| structure.get::<i32>("channels").ok());
        let format = structure.map(|structure| structure.name().to_string());
        let kind = match stream.stream_type() {
            kind if kind.contains(gst::StreamType::VIDEO) => TrackKind::Video,
            kind if kind.contains(gst::StreamType::AUDIO) => TrackKind::Audio,
            kind if kind.contains(gst::StreamType::TEXT) => TrackKind::Text,
            _ => continue,
        };
        let external =
            id.contains("suburi") || stream.stream_flags().contains(gst::StreamFlags::SPARSE) && id.contains("sub");
        tracks.push(Track { id, kind, language, title, codec, channels, format, external });
    }
    tracks
}

#[must_use]
pub fn kind_of(tracks: &Tracks) -> Kind {
    if tracks.video().next().is_some() { Kind::Video } else { Kind::Audio }
}

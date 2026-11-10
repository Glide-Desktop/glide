use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSrc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Turn {
    #[default]
    None,
    Right,
    Half,
    Left,
}

impl Turn {
    #[must_use]
    pub fn clockwise(self) -> Self {
        match self {
            Self::None => Self::Right,
            Self::Right => Self::Half,
            Self::Half => Self::Left,
            Self::Left => Self::None,
        }
    }

    #[must_use]
    pub fn counter(self) -> Self {
        match self {
            Self::None => Self::Left,
            Self::Left => Self::Half,
            Self::Half => Self::Right,
            Self::Right => Self::None,
        }
    }

    #[must_use]
    pub fn degrees(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Right => 90,
            Self::Half => 180,
            Self::Left => 270,
        }
    }

    #[must_use]
    pub fn sideways(self) -> bool {
        matches!(self, Self::Right | Self::Left)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Transform {
    pub turn: Turn,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl Transform {
    #[must_use]
    pub fn is_identity(self) -> bool {
        self == Self::default()
    }

    #[must_use]
    pub fn method(self) -> &'static str {
        let (turn, mirrored) = match (self.flip_h, self.flip_v) {
            (false, false) => (self.turn, false),
            (true, true) => (self.turn.clockwise().clockwise(), false),
            (true, false) => (self.turn, true),
            (false, true) => (self.turn.clockwise().clockwise(), true),
        };
        match (turn, mirrored) {
            (Turn::None, false) => "none",
            (Turn::Right, false) => "clockwise",
            (Turn::Half, false) => "rotate-180",
            (Turn::Left, false) => "counterclockwise",
            (Turn::None, true) => "horizontal-flip",
            (Turn::Right, true) => "upper-left-diagonal",
            (Turn::Half, true) => "vertical-flip",
            (Turn::Left, true) => "upper-right-diagonal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub uri: String,
    pub start: Duration,
    pub end: Duration,
    pub transform: Transform,
}

impl Segment {
    #[must_use]
    pub fn length(&self) -> Duration {
        self.end.saturating_sub(self.start)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quality {
    P2160,
    P1080,
    P720,
    AudioOnly,
}

impl Quality {
    #[must_use]
    pub fn height(self) -> Option<u32> {
        match self {
            Self::P2160 => Some(2160),
            Self::P1080 => Some(1080),
            Self::P720 => Some(720),
            Self::AudioOnly => None,
        }
    }

    #[must_use]
    pub fn video_bitrate(self, codec: Codec) -> u32 {
        let base = match self {
            Self::P2160 => 25_000,
            Self::P1080 => 10_000,
            Self::P720 => 5_000,
            Self::AudioOnly => 0,
        };
        match codec {
            Codec::H264 => base,
            Codec::Hevc => base * 6 / 10,
            Codec::Av1 => base / 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Codec {
    H264,
    Hevc,
    Av1,
}

pub const AUDIO_KBPS: u32 = 192;

#[must_use]
pub fn estimate(length: Duration, quality: Quality, codec: Codec) -> u64 {
    let kbps = u64::from(quality.video_bitrate(codec) + AUDIO_KBPS);
    (length.as_secs_f64() * kbps as f64 * 1000.0 / 8.0) as u64
}

#[derive(Debug, Clone)]
pub struct Job {
    pub segments: Vec<Segment>,
    pub output: PathBuf,
    pub quality: Quality,
    pub codec: Codec,
    pub lossless: bool,
    pub source_height: u32,
    pub source_width: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Progress {
    Fraction(f64),
    Hardware(bool),
    Done(PathBuf),
    Failed(String),
    Cancelled,
}

#[must_use]
pub fn container_of(uri: &str) -> &'static str {
    let extension = Path::new(uri).extension().map(|extension| extension.to_string_lossy().to_ascii_lowercase());
    if matches!(extension.as_deref(), Some("mkv" | "webm" | "mka")) { "mkv" } else { "mp4" }
}

#[must_use]
pub fn lossless_possible(segments: &[Segment]) -> bool {
    let Some(first) = segments.first() else { return false };
    segments.iter().all(|segment| segment.uri == first.uri && segment.length() > Duration::ZERO)
        && segments.iter().all(|segment| {
            segment.transform.is_identity()
                || (container_of(&segment.uri) == "mp4"
                    && !segment.transform.flip_h
                    && !segment.transform.flip_v
                    && segments.iter().all(|other| other.transform == segment.transform))
        })
}

#[must_use]
pub fn extension(job: &Job) -> &'static str {
    if job.quality == Quality::AudioOnly {
        "m4a"
    } else if job.lossless {
        job.segments.first().map_or("mp4", |first| container_of(&first.uri))
    } else {
        "mp4"
    }
}

#[must_use]
pub fn target_size(width: u32, height: u32, quality: Quality, sideways: bool) -> Option<(u32, u32)> {
    let limit = quality.height()?;
    let (width, height) = if sideways { (height, width) } else { (width, height) };
    if width == 0 || height == 0 {
        return None;
    }
    let short = width.min(height);
    let scale = (f64::from(limit) / f64::from(short)).min(1.0);
    let even = |value: f64| ((value / 2.0).round() as u32 * 2).max(2);
    Some((even(f64::from(width) * scale), even(f64::from(height) * scale)))
}

pub struct Handle {
    cancel: Arc<AtomicBool>,
}

impl Handle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[must_use]
pub fn spawn(job: Job) -> (Handle, UnboundedReceiver<Progress>) {
    let cancel = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = unbounded();
    let flag = cancel.clone();
    let spawned = std::thread::Builder::new().name("glide-video-export".into()).spawn(move || {
        let output = job.output.clone();
        let outcome = if job.lossless { remux(&job, &flag, &sender) } else { transcode(&job, &flag, &sender) };
        let last = match outcome {
            Ok(()) if flag.load(Ordering::Relaxed) => {
                let _ = std::fs::remove_file(&output);
                Progress::Cancelled
            }
            Ok(()) => Progress::Done(output),
            Err(err) => {
                let _ = std::fs::remove_file(&output);
                if flag.load(Ordering::Relaxed) { Progress::Cancelled } else { Progress::Failed(err) }
            }
        };
        let _ = sender.unbounded_send(last);
    });
    if let Err(err) = spawned {
        tracing::warn!("could not start the export: {err}");
    }
    (Handle { cancel }, receiver)
}

fn make(name: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(name).build().map_err(|_| format!("missing element {name}"))
}

fn first_of(names: &[&str]) -> Result<gst::Element, String> {
    names
        .iter()
        .find_map(|name| gst::ElementFactory::make(name).build().ok())
        .ok_or_else(|| format!("none of {names:?}"))
}

#[must_use]
pub fn encoder_names(codec: Codec) -> &'static [&'static str] {
    match codec {
        Codec::H264 => &["vah264enc", "x264enc", "openh264enc"],
        Codec::Hevc => &["vah265enc", "x265enc"],
        Codec::Av1 => &["vaav1enc", "svtav1enc", "av1enc"],
    }
}

fn video_encoder(codec: Codec, kbps: u32) -> Result<(gst::Element, bool), String> {
    let encoder = first_of(encoder_names(codec))?;
    let name = encoder.factory().map(|factory| factory.name().to_string()).unwrap_or_default();
    let hardware = name.starts_with("va");
    match name.as_str() {
        "x264enc" | "x265enc" => {
            encoder.set_property("bitrate", kbps);
            if name == "x264enc" {
                encoder.set_property_from_str("speed-preset", "faster");
            }
        }
        "svtav1enc" | "av1enc" => encoder.set_property("target-bitrate", kbps),
        "openh264enc" => encoder.set_property("bitrate", kbps * 1000),
        _ if hardware => encoder.set_property("bitrate", kbps),
        _ => {}
    }
    Ok((encoder, hardware))
}

fn parser(codec: Codec) -> &'static str {
    match codec {
        Codec::H264 => "h264parse",
        Codec::Hevc => "h265parse",
        Codec::Av1 => "av1parse",
    }
}

fn discard(pipeline: &gst::Pipeline, pad: &gst::Pad) {
    let Ok(sink) = gst::ElementFactory::make("fakesink").property("sync", false).property("async", false).build()
    else {
        return;
    };
    if pipeline.add(&sink).is_ok() {
        let _ = sink.sync_state_with_parent();
        if let Some(input) = sink.static_pad("sink") {
            let _ = pad.link(&input);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PadKind {
    Video,
    Audio,
    Other,
}

fn pad_kind(pad: &gst::Pad) -> PadKind {
    if let Some(kind) = pad.stream().map(|stream| stream.stream_type()) {
        if kind.contains(gst::StreamType::VIDEO) {
            return PadKind::Video;
        }
        if kind.contains(gst::StreamType::AUDIO) {
            return PadKind::Audio;
        }
        if !kind.is_empty() {
            return PadKind::Other;
        }
    }
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    match caps.structure(0).map(|structure| structure.name().to_string()) {
        Some(name) if name.starts_with("video/") => PadKind::Video,
        Some(name) if name.starts_with("audio/") => PadKind::Audio,
        _ => PadKind::Other,
    }
}

fn preroll(pipeline: &gst::Pipeline, linked: &AtomicUsize, expect: usize) -> Result<(), String> {
    if expect == 0 {
        return Ok(());
    }
    let bus = pipeline.bus().ok_or("no bus")?;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let (mut seen, mut ready) = (usize::MAX, false);
    loop {
        let now = linked.load(Ordering::SeqCst);
        if now != seen {
            seen = now;
            ready = false;
        }
        if now >= expect && ready {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return if now > 0 && now >= expect.min(1) { Ok(()) } else { Err("the source did not open".into()) };
        }
        let Some(message) = bus.timed_pop(gst::ClockTime::from_mseconds(50)) else { continue };
        match message.view() {
            gst::MessageView::AsyncDone(_) => ready = linked.load(Ordering::SeqCst) == seen && seen > 0,
            gst::MessageView::Error(err) => return Err(format!("{} ({:?})", err.error(), err.debug())),
            _ => {}
        }
    }
}

fn wait_eos(pipeline: &gst::Pipeline, cancel: &AtomicBool, mut tick: impl FnMut()) -> Result<(), String> {
    let bus = pipeline.bus().ok_or("no bus")?;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        tick();
        let Some(message) = bus.timed_pop(gst::ClockTime::from_mseconds(200)) else { continue };
        match message.view() {
            gst::MessageView::Eos(_) => return Ok(()),
            gst::MessageView::Error(err) => return Err(format!("{} ({:?})", err.error(), err.debug())),
            _ => {}
        }
    }
}

struct Encoder {
    pipeline: gst::Pipeline,
    video: Option<AppSrc>,
    audio: AppSrc,
}

fn encoder(
    job: &Job, size: Option<(u32, u32)>, fps: gst::Fraction, sender: &UnboundedSender<Progress>,
) -> Result<Encoder, String> {
    let pipeline = gst::Pipeline::new();
    let mux = make("mp4mux")?;
    let sink = gst::ElementFactory::make("filesink")
        .property("location", job.output.to_string_lossy().as_ref())
        .build()
        .map_err(|err| err.to_string())?;
    pipeline.add_many([&mux, &sink]).map_err(|err| err.to_string())?;
    mux.link(&sink).map_err(|err| err.to_string())?;
    let video = match size {
        Some((width, height)) => {
            let caps = gst::Caps::builder("video/x-raw")
                .field("format", "I420")
                .field("width", width as i32)
                .field("height", height as i32)
                .field("framerate", fps)
                .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
                .build();
            let source =
                AppSrc::builder().caps(&caps).format(gst::Format::Time).block(true).max_bytes(64 << 20).build();
            let convert = make("videoconvert")?;
            let (encode, hardware) = video_encoder(job.codec, job.quality.video_bitrate(job.codec))?;
            let _ = sender.unbounded_send(Progress::Hardware(hardware));
            let parse = make(parser(job.codec))?;
            pipeline.add_many([source.upcast_ref(), &convert, &encode, &parse]).map_err(|err| err.to_string())?;
            gst::Element::link_many([source.upcast_ref(), &convert, &encode, &parse]).map_err(|err| err.to_string())?;
            parse.link(&mux).map_err(|err| err.to_string())?;
            Some(source)
        }
        None => None,
    };
    let caps = gst::Caps::builder("audio/x-raw")
        .field("format", "S16LE")
        .field("layout", "interleaved")
        .field("rate", 48_000)
        .field("channels", 2)
        .build();
    let audio = AppSrc::builder().caps(&caps).format(gst::Format::Time).block(true).max_bytes(16 << 20).build();
    let convert = make("audioconvert")?;
    let aac = first_of(&["fdkaacenc", "avenc_aac"])?;
    if aac.has_property("bitrate") {
        aac.set_property("bitrate", (AUDIO_KBPS * 1000) as i32);
    }
    let parse = make("aacparse")?;
    pipeline.add_many([audio.upcast_ref(), &convert, &aac, &parse]).map_err(|err| err.to_string())?;
    gst::Element::link_many([audio.upcast_ref(), &convert, &aac, &parse]).map_err(|err| err.to_string())?;
    parse.link(&mux).map_err(|err| err.to_string())?;
    pipeline.set_state(gst::State::Playing).map_err(|err| err.to_string())?;
    Ok(Encoder { pipeline, video, audio })
}

fn probe(uri: &str) -> Option<(u32, u32, gst::Fraction, bool)> {
    let info = crate::info::discover(uri, Duration::from_secs(10)).ok()?;
    let has_audio = !info.audio.is_empty();
    let video = info.video?;
    let fps = if video.fps > 0.0 {
        gst::Fraction::approximate_f64(video.fps).unwrap_or(gst::Fraction::new(25, 1))
    } else {
        gst::Fraction::new(25, 1)
    };
    Some((video.width, video.height, fps, has_audio))
}

fn retime(sample: &gst::Sample, offset: gst::ClockTime) -> Option<gst::Buffer> {
    let buffer = sample.buffer()?;
    let segment = sample.segment()?.downcast_ref::<gst::ClockTime>()?;
    let pts = buffer.pts()?;
    let running = segment.to_running_time(pts)?;
    let mut copy = buffer.copy();
    let writable = copy.make_mut();
    writable.set_pts(running + offset);
    writable.set_dts(gst::ClockTime::NONE);
    Some(copy)
}

fn silence(audio: &AppSrc, from: gst::ClockTime, length: gst::ClockTime) {
    let chunk = gst::ClockTime::from_mseconds(100);
    let mut at = gst::ClockTime::ZERO;
    while at < length {
        let span = chunk.min(length - at);
        let frames = span.nseconds() * 48_000 / 1_000_000_000;
        let Ok(mut buffer) = gst::Buffer::with_size(frames as usize * 4) else { return };
        {
            let writable = buffer.get_mut().expect("fresh buffer");
            if let Ok(mut map) = writable.map_writable() {
                map.as_mut_slice().fill(0);
            }
            writable.set_pts(from + at);
            writable.set_duration(span);
        }
        if audio.push_buffer(buffer).is_err() {
            return;
        }
        at += span;
    }
}

fn transcode(job: &Job, cancel: &AtomicBool, sender: &UnboundedSender<Progress>) -> Result<(), String> {
    crate::init()?;
    let first = job.segments.first().ok_or("nothing to export")?;
    let probed = probe(&first.uri);
    let size = match (job.quality, probed) {
        (Quality::AudioOnly, _) | (_, None) => None,
        (quality, Some((width, height, _, _))) => target_size(width, height, quality, first.transform.turn.sideways()),
    };
    let fps = probed.map_or(gst::Fraction::new(25, 1), |probed| probed.2);
    let encoder = encoder(job, size, fps, sender)?;
    let total: Duration = job.segments.iter().map(Segment::length).sum();
    let mut offset = gst::ClockTime::ZERO;
    for segment in &job.segments {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let streams = crate::info::discover(&segment.uri, Duration::from_secs(10)).map_err(|err| format!("{err:?}"))?;
        let expect = usize::from(size.is_some() && streams.video.is_some()) + usize::from(!streams.audio.is_empty());
        let pass = Pass { segment, encoder: &encoder, size, fps, offset, total, expect };
        decode_into(&pass, cancel, sender)?;
        offset += gst::ClockTime::from_nseconds(segment.length().as_nanos() as u64);
    }
    if let Some(video) = &encoder.video {
        let _ = video.end_of_stream();
    }
    let _ = encoder.audio.end_of_stream();
    let outcome = wait_eos(&encoder.pipeline, cancel, || {});
    let _ = encoder.pipeline.set_state(gst::State::Null);
    outcome
}

fn video_chain((width, height): (u32, u32), fps: gst::Fraction, transform: Transform) -> Option<Vec<gst::Element>> {
    let caps = gst::Caps::builder("video/x-raw")
        .field("format", "I420")
        .field("width", width as i32)
        .field("height", height as i32)
        .field("framerate", fps)
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .build();
    [
        gst::ElementFactory::make("videoconvert").build(),
        gst::ElementFactory::make("videoflip").property_from_str("method", transform.method()).build(),
        gst::ElementFactory::make("videoscale").property("add-borders", true).build(),
        gst::ElementFactory::make("videorate").build(),
        gst::ElementFactory::make("videoconvert").build(),
        gst::ElementFactory::make("capsfilter").property("caps", &caps).build(),
    ]
    .into_iter()
    .map(Result::ok)
    .collect()
}

fn audio_chain() -> Option<Vec<gst::Element>> {
    let caps = gst::Caps::builder("audio/x-raw")
        .field("format", "S16LE")
        .field("layout", "interleaved")
        .field("rate", 48_000)
        .field("channels", 2)
        .build();
    [
        gst::ElementFactory::make("audioconvert").build(),
        gst::ElementFactory::make("audioresample").build(),
        gst::ElementFactory::make("capsfilter").property("caps", &caps).build(),
    ]
    .into_iter()
    .map(Result::ok)
    .collect()
}

fn pushing(target: AppSrc, offset: gst::ClockTime) -> gstreamer_app::AppSinkCallbacks {
    gstreamer_app::AppSinkCallbacks::builder()
        .new_sample(move |sink| {
            let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
            if let Some(buffer) = retime(&sample, offset) {
                target.push_buffer(buffer).map_err(|_| gst::FlowError::Flushing)?;
            }
            Ok(gst::FlowSuccess::Ok)
        })
        .build()
}

#[derive(Clone, Copy)]
struct Pass<'a> {
    segment: &'a Segment,
    encoder: &'a Encoder,
    size: Option<(u32, u32)>,
    fps: gst::Fraction,
    offset: gst::ClockTime,
    total: Duration,
    expect: usize,
}

fn decode_into(pass: &Pass<'_>, cancel: &AtomicBool, sender: &UnboundedSender<Progress>) -> Result<(), String> {
    let Pass { segment, encoder, size, fps, offset, total, expect } = *pass;
    let pipeline = gst::Pipeline::new();
    let source = gst::ElementFactory::make("uridecodebin3")
        .property("uri", &segment.uri)
        .build()
        .map_err(|err| err.to_string())?;
    pipeline.add(&source).map_err(|err| err.to_string())?;
    let audio_seen = Arc::new(AtomicBool::new(false));
    let linked = Arc::new(AtomicUsize::new(0));
    let video_target = encoder.video.clone();
    let audio_target = encoder.audio.clone();
    let transform = segment.transform;
    let weak = pipeline.downgrade();
    let seen = audio_seen.clone();
    let counter = linked.clone();
    source.connect_pad_added(move |_, pad| {
        let Some(pipeline) = weak.upgrade() else { return };
        let sink = AppSink::builder().sync(false).build();
        let picked = match pad_kind(pad) {
            PadKind::Video => size
                .zip(video_target.clone())
                .and_then(|(size, target)| video_chain(size, fps, transform).map(|chain| (chain, target))),
            PadKind::Audio => {
                seen.store(true, Ordering::Relaxed);
                audio_chain().map(|chain| (chain, audio_target.clone()))
            }
            PadKind::Other => None,
        };
        let Some((chain, target)) = picked else {
            discard(&pipeline, pad);
            return;
        };
        sink.set_callbacks(pushing(target, offset));
        let mut all: Vec<&gst::Element> = chain.iter().collect();
        all.push(sink.upcast_ref());
        if pipeline.add_many(all.iter().copied()).is_err() || gst::Element::link_many(all.iter().copied()).is_err() {
            return;
        }
        for element in &all {
            let _ = element.sync_state_with_parent();
        }
        if let Some(input) = chain.first().and_then(|first| first.static_pad("sink"))
            && pad.link(&input).is_ok()
        {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    });
    pipeline.set_state(gst::State::Paused).map_err(|err| err.to_string())?;
    if let Err(err) = preroll(&pipeline, &linked, expect) {
        let _ = pipeline.set_state(gst::State::Null);
        return Err(err);
    }
    let start = gst::ClockTime::from_nseconds(segment.start.as_nanos() as u64);
    let end = gst::ClockTime::from_nseconds(segment.end.as_nanos() as u64);
    pipeline
        .seek(1.0, gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, gst::SeekType::Set, start, gst::SeekType::Set, end)
        .map_err(|err| err.to_string())?;
    let _ = pipeline.state(gst::ClockTime::from_seconds(10));
    pipeline.set_state(gst::State::Playing).map_err(|err| err.to_string())?;
    let length = segment.length();
    let done_before = Duration::from_nanos(offset.nseconds());
    let outcome = wait_eos(&pipeline, cancel, || {
        let at = pipeline
            .query_position::<gst::ClockTime>()
            .map_or(Duration::ZERO, |at| Duration::from_nanos(at.nseconds()).saturating_sub(segment.start).min(length));
        let fraction = (done_before + at).as_secs_f64() / total.as_secs_f64().max(0.001);
        let _ = sender.unbounded_send(Progress::Fraction(fraction.clamp(0.0, 1.0)));
    });
    let _ = pipeline.set_state(gst::State::Null);
    if !audio_seen.load(Ordering::Relaxed) {
        silence(&encoder.audio, offset, gst::ClockTime::from_nseconds(length.as_nanos() as u64));
    }
    outcome
}

const PASSTHROUGH: &str = "video/x-h264; video/x-h265; video/x-av1; video/x-vp9; video/x-vp8; video/mpeg; \
                           audio/mpeg; audio/x-opus; audio/x-vorbis; audio/x-flac; audio/x-ac3; audio/x-eac3; \
                           audio/x-alac; audio/x-raw";

fn passthrough() -> gst::Caps {
    PASSTHROUGH.parse().unwrap_or_else(|_| gst::Caps::new_any())
}

fn remux(job: &Job, cancel: &AtomicBool, sender: &UnboundedSender<Progress>) -> Result<(), String> {
    crate::init()?;
    let first = job.segments.first().ok_or("nothing to export")?;
    let container = container_of(&first.uri);
    let pipeline = gst::Pipeline::new();
    let mux = make(if container == "mkv" { "matroskamux" } else { "qtmux" })?;
    let sink = gst::ElementFactory::make("filesink")
        .property("location", job.output.to_string_lossy().as_ref())
        .build()
        .map_err(|err| err.to_string())?;
    pipeline.add_many([&mux, &sink]).map_err(|err| err.to_string())?;
    mux.link(&sink).map_err(|err| err.to_string())?;
    let streams = streams_of(&first.uri)?;
    let mut sources = Vec::new();
    for caps in &streams {
        let source = AppSrc::builder().caps(caps).format(gst::Format::Time).block(true).max_bytes(32 << 20).build();
        pipeline.add(&source).map_err(|err| err.to_string())?;
        let pad = source.static_pad("src").ok_or("no src pad")?;
        let target = mux.compatible_pad(&pad, Some(caps)).ok_or_else(|| format!("the muxer refuses {caps}"))?;
        pad.link(&target).map_err(|err| format!("{err:?}"))?;
        sources.push(source);
    }
    if first.transform.turn != Turn::None {
        let orientation = format!("rotate-{}", first.transform.turn.degrees());
        let mut tags = gst::TagList::new();
        tags.get_mut().unwrap().add::<gst::tags::ImageOrientation>(&orientation.as_str(), gst::TagMergeMode::Replace);
        for source in &sources {
            let _ = source.static_pad("src").map(|pad| pad.push_event(gst::event::Tag::new(tags.clone())));
        }
    }
    pipeline.set_state(gst::State::Playing).map_err(|err| err.to_string())?;
    let total: Duration = job.segments.iter().map(Segment::length).sum();
    let mut offset = gst::ClockTime::ZERO;
    for segment in &job.segments {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let took = copy_segment(segment, &streams, &sources, offset, total, cancel, sender)?;
        offset += took;
    }
    for source in &sources {
        let _ = source.end_of_stream();
    }
    let outcome = wait_eos(&pipeline, cancel, || {});
    let _ = pipeline.set_state(gst::State::Null);
    outcome
}

fn expect_av(uri: &str) -> usize {
    crate::info::discover(uri, Duration::from_secs(10))
        .map_or(1, |info| usize::from(info.video.is_some()) + usize::from(!info.audio.is_empty()))
}

fn streams_of(uri: &str) -> Result<Vec<gst::Caps>, String> {
    let pipeline = gst::Pipeline::new();
    let source = gst::ElementFactory::make("uridecodebin3")
        .property("uri", uri)
        .property("caps", passthrough())
        .build()
        .map_err(|err| err.to_string())?;
    pipeline.add(&source).map_err(|err| err.to_string())?;
    let found = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink_found = found.clone();
    let linked = Arc::new(AtomicUsize::new(0));
    let counter = linked.clone();
    let weak = pipeline.downgrade();
    source.connect_pad_added(move |_, pad| {
        let Some(pipeline) = weak.upgrade() else { return };
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        let wanted = caps
            .structure(0)
            .is_some_and(|structure| structure.name().starts_with("video/") || structure.name().starts_with("audio/"));
        if !wanted {
            discard(&pipeline, pad);
            return;
        }
        let sink = AppSink::builder().sync(false).max_buffers(1).build();
        if pipeline.add(&sink).is_err() {
            return;
        }
        let _ = sink.sync_state_with_parent();
        if let Some(input) = sink.static_pad("sink")
            && pad.link(&input).is_ok()
        {
            counter.fetch_add(1, Ordering::SeqCst);
        }
        let store = sink_found.clone();
        sink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_preroll(move |sink| {
                    if let Ok(sample) = sink.pull_preroll()
                        && let Some(caps) = sample.caps()
                    {
                        store.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(caps.to_owned());
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
    });
    pipeline.set_state(gst::State::Paused).map_err(|err| err.to_string())?;
    let outcome = preroll(&pipeline, &linked, expect_av(uri));
    let _ = pipeline.set_state(gst::State::Null);
    outcome?;
    let mut caps = found.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    caps.sort_by_key(|caps| caps.structure(0).map(|structure| !structure.name().starts_with("video/")));
    if caps.is_empty() { Err("no streams to copy".into()) } else { Ok(caps) }
}

type Shared<T> = Arc<std::sync::Mutex<T>>;

fn copying(
    target: AppSrc, first: Shared<Option<gst::ClockTime>>, end: Shared<gst::ClockTime>, offset: gst::ClockTime,
) -> gstreamer_app::AppSinkCallbacks {
    gstreamer_app::AppSinkCallbacks::builder()
        .new_sample(move |sink| {
            let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
            let Some(buffer) = sample.buffer() else { return Ok(gst::FlowSuccess::Ok) };
            let Some(segment) = sample.segment().and_then(|segment| segment.downcast_ref::<gst::ClockTime>().cloned())
            else {
                return Ok(gst::FlowSuccess::Ok);
            };
            let running = |time: Option<gst::ClockTime>| {
                time.and_then(|time| segment.to_running_time_full(time)).and_then(gst::Signed::positive)
            };
            let Some(pts) = running(buffer.pts().or(buffer.dts())) else { return Ok(gst::FlowSuccess::Ok) };
            let base = *first.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get_or_insert(pts);
            let mut copy = buffer.copy();
            let writable = copy.make_mut();
            writable.set_pts(offset + pts.saturating_sub(base));
            writable.set_dts(running(buffer.dts()).map(|dts| offset + dts.saturating_sub(base)));
            let stop = offset + pts.saturating_sub(base) + buffer.duration().unwrap_or(gst::ClockTime::ZERO);
            {
                let mut last = end.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                *last = (*last).max(stop);
            }
            target.push_buffer(copy).map_err(|_| gst::FlowError::Flushing)?;
            Ok(gst::FlowSuccess::Ok)
        })
        .build()
}

fn copy_segment(
    segment: &Segment, streams: &[gst::Caps], sources: &[AppSrc], offset: gst::ClockTime, total: Duration,
    cancel: &AtomicBool, sender: &UnboundedSender<Progress>,
) -> Result<gst::ClockTime, String> {
    let pipeline = gst::Pipeline::new();
    let source = gst::ElementFactory::make("uridecodebin3")
        .property("uri", &segment.uri)
        .property("caps", passthrough())
        .build()
        .map_err(|err| err.to_string())?;
    pipeline.add(&source).map_err(|err| err.to_string())?;
    let first_pts: Arc<std::sync::Mutex<Option<gst::ClockTime>>> = Arc::new(std::sync::Mutex::new(None));
    let last_end: Arc<std::sync::Mutex<gst::ClockTime>> = Arc::new(std::sync::Mutex::new(gst::ClockTime::ZERO));
    let weak = pipeline.downgrade();
    let expect = streams.len();
    let streams = streams.to_vec();
    let targets = sources.to_vec();
    let (first_shared, end_shared) = (first_pts.clone(), last_end.clone());
    let linked = Arc::new(AtomicUsize::new(0));
    let counter = linked.clone();
    source.connect_pad_added(move |_, pad| {
        let Some(pipeline) = weak.upgrade() else { return };
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        let Some(name) = caps.structure(0).map(|structure| structure.name().to_string()) else { return };
        let Some(index) = streams
            .iter()
            .position(|known| known.structure(0).is_some_and(|structure| structure.name() == name.as_str()))
        else {
            discard(&pipeline, pad);
            return;
        };
        let sink = AppSink::builder().sync(false).build();
        sink.set_callbacks(copying(targets[index].clone(), first_shared.clone(), end_shared.clone(), offset));
        if pipeline.add(&sink).is_err() {
            return;
        }
        let _ = sink.sync_state_with_parent();
        if let Some(input) = sink.static_pad("sink")
            && pad.link(&input).is_ok()
        {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    });
    pipeline.set_state(gst::State::Paused).map_err(|err| err.to_string())?;
    if let Err(err) = preroll(&pipeline, &linked, expect) {
        let _ = pipeline.set_state(gst::State::Null);
        return Err(err);
    }
    let start = gst::ClockTime::from_nseconds(segment.start.as_nanos() as u64);
    let end = gst::ClockTime::from_nseconds(segment.end.as_nanos() as u64);
    pipeline
        .seek(
            1.0,
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT | gst::SeekFlags::SNAP_BEFORE,
            gst::SeekType::Set,
            start,
            gst::SeekType::Set,
            end,
        )
        .map_err(|err| err.to_string())?;
    let _ = pipeline.state(gst::ClockTime::from_seconds(10));
    *first_pts.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    pipeline.set_state(gst::State::Playing).map_err(|err| err.to_string())?;
    let done_before = Duration::from_nanos(offset.nseconds());
    let length = segment.length();
    let outcome = wait_eos(&pipeline, cancel, || {
        let at = pipeline
            .query_position::<gst::ClockTime>()
            .map_or(Duration::ZERO, |at| Duration::from_nanos(at.nseconds()).saturating_sub(segment.start).min(length));
        let fraction = (done_before + at).as_secs_f64() / total.as_secs_f64().max(0.001);
        let _ = sender.unbounded_send(Progress::Fraction(fraction.clamp(0.0, 1.0)));
    });
    let _ = pipeline.set_state(gst::State::Null);
    outcome?;
    let took = (*last_end.lock().unwrap_or_else(std::sync::PoisonError::into_inner)).saturating_sub(offset);
    Ok(took)
}

#[must_use]
pub fn unique(folder: &Path, stem: &str, extension: &str) -> PathBuf {
    let first = folder.join(format!("{stem}.{extension}"));
    if !first.exists() {
        return first;
    }
    (2..10_000)
        .map(|index| folder.join(format!("{stem} {index}.{extension}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(uri: &str, transform: Transform) -> Segment {
        Segment { uri: uri.into(), start: Duration::ZERO, end: Duration::from_secs(4), transform }
    }

    #[test]
    fn rotation_and_flips_map_to_one_videoflip_method() {
        assert_eq!(Transform::default().method(), "none");
        assert_eq!(Transform { turn: Turn::Right, ..Transform::default() }.method(), "clockwise");
        assert_eq!(Transform { flip_h: true, ..Transform::default() }.method(), "horizontal-flip");
        assert_eq!(Transform { flip_v: true, ..Transform::default() }.method(), "vertical-flip");
        assert_eq!(Transform { flip_h: true, flip_v: true, ..Transform::default() }.method(), "rotate-180");
        assert_eq!(Transform { turn: Turn::Half, flip_h: true, flip_v: true }.method(), "none");
    }

    #[test]
    fn turns_cycle_both_ways() {
        let mut turn = Turn::None;
        for _ in 0..4 {
            turn = turn.clockwise();
        }
        assert_eq!(turn, Turn::None);
        assert_eq!(Turn::None.counter(), Turn::Left);
        assert_eq!(Turn::Left.clockwise(), Turn::None);
    }

    #[test]
    fn lossless_needs_one_source_and_no_mirroring() {
        let plain = Transform::default();
        assert!(lossless_possible(&[seg("file:///a.mkv", plain), seg("file:///a.mkv", plain)]));
        assert!(!lossless_possible(&[seg("file:///a.mkv", plain), seg("file:///b.mkv", plain)]));
        let turned = Transform { turn: Turn::Right, ..Transform::default() };
        assert!(lossless_possible(&[seg("file:///a.mp4", turned)]));
        assert!(!lossless_possible(&[seg("file:///a.mkv", turned)]));
        assert!(!lossless_possible(&[seg("file:///a.mp4", Transform { flip_h: true, ..Transform::default() })]));
        assert!(!lossless_possible(&[]));
    }

    #[test]
    fn target_size_never_upscales_and_stays_even() {
        assert_eq!(target_size(3840, 2160, Quality::P1080, false), Some((1920, 1080)));
        assert_eq!(target_size(1280, 720, Quality::P2160, false), Some((1280, 720)));
        assert_eq!(target_size(1920, 1080, Quality::P720, true), Some((720, 1280)));
        assert_eq!(target_size(1001, 563, Quality::P720, false), Some((1002, 564)));
        assert_eq!(target_size(1920, 1080, Quality::AudioOnly, false), None);
    }

    #[test]
    fn estimates_scale_with_length_and_codec() {
        let length = Duration::from_secs(289);
        let h264 = estimate(length, Quality::P1080, Codec::H264);
        assert!((350_000_000..=380_000_000).contains(&h264), "{h264}");
        assert!(estimate(length, Quality::P1080, Codec::Av1) < estimate(length, Quality::P1080, Codec::Hevc));
        assert!(estimate(length, Quality::AudioOnly, Codec::H264) < 8_000_000);
    }

    #[test]
    fn unique_names_count_up() {
        let dir = std::env::temp_dir().join(format!("glide-video-unique-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.mp4"), b"x").unwrap();
        assert_eq!(unique(&dir, "a", "mp4"), dir.join("a 2.mp4"));
        assert_eq!(unique(&dir, "b", "mp4"), dir.join("b.mp4"));
    }
}

use std::fmt::Write as _;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use crate::formats::{Audio, PROFILE_CBP, Video};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Encoder {
    pub name: &'static str,
    pub hardware: bool,
}

const fn encoder_named(name: &'static str, hardware: bool) -> Encoder {
    Encoder { name, hardware }
}

pub(crate) const ENCODERS: [Encoder; 8] = [
    encoder_named("vah264enc", true),
    encoder_named("vah264lpenc", true),
    encoder_named("nvh264enc", true),
    encoder_named("qsvh264enc", true),
    encoder_named("vaapih264enc", true),
    encoder_named("v4l2h264enc", true),
    encoder_named("x264enc", false),
    encoder_named("openh264enc", false),
];
const AAC_ENCODERS: [&str; 3] = ["fdkaacenc", "avenc_aac", "voaacenc"];
const CAPTURE_ELEMENTS: [&str; 4] = ["pipewiresrc", "audioconvert", "audioresample", "capssetter"];
const REQUIRED: [(&str, &str); 10] = [
    ("appsrc", "gst-plugins-base"),
    ("videoconvert", "gst-plugins-base"),
    ("videoscale", "gst-plugins-base"),
    ("videorate", "gst-plugins-base"),
    ("videotestsrc", "gst-plugins-base"),
    ("h264parse", "gst-plugins-bad"),
    ("mpegtsmux", "gst-plugins-bad"),
    ("rtpmp2tpay", "gst-plugins-good"),
    ("udpsink", "gst-plugins-good"),
    ("fakesink", "gstreamer"),
];
const RTP_PAYLOAD_MP2T: u32 = 33;
pub(crate) const VIDEO_PID: u32 = 0x1011;
pub(crate) const AUDIO_PID: u32 = 0x1100;
pub(crate) const PMT_PID: u32 = 0x0100;
const SOUND_PREFIX: &str = "sound-";
const PROBE_SIZE: (u32, u32) = (1920, 1080);
const PROBE_WAIT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probed {
    pub encoder: Encoder,
    properties: Vec<&'static str>,
}

impl Probed {
    #[must_use]
    pub(crate) fn line(&self, kbps: u32, gop: u32) -> String {
        let mut line = self.encoder.name.to_string();
        for (key, value) in properties(self.encoder.name, kbps, gop) {
            if self.properties.contains(&key) {
                let _ = write!(line, " {key}={value}");
            }
        }
        line
    }

    #[cfg(test)]
    pub(crate) fn assumed(name: &str) -> Self {
        let encoder = *ENCODERS.iter().find(|encoder| encoder.name == name).expect("a known encoder");
        Self { encoder, properties: properties(name, 0, 0).into_iter().map(|(key, _)| key).collect() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub video: Video,
    pub audio: Option<Audio>,
    pub host: String,
    pub port: u16,
    pub frame_format: &'static str,
    pub input: (u32, u32),
    pub encoder: Probed,
}

impl Spec {
    #[must_use]
    pub(crate) fn bitrate_kbps(&self) -> u32 {
        let pixels = self.video.mode.width * self.video.mode.height * self.video.mode.fps;
        (pixels / 8_000).clamp(2_000, 20_000)
    }

    const fn profile(&self) -> &'static str {
        if self.video.profile == PROFILE_CBP { "constrained-baseline" } else { "high" }
    }
}

#[must_use]
pub(crate) fn aac_encoder(available: impl Fn(&str) -> bool) -> bool {
    available("aacparse") && AAC_ENCODERS.iter().any(|name| available(name))
}

#[must_use]
pub(crate) fn audio_capture(available: impl Fn(&str) -> bool) -> bool {
    CAPTURE_ELEMENTS.iter().all(|name| available(name))
}

fn properties(name: &str, kbps: u32, gop: u32) -> Vec<(&'static str, String)> {
    match name {
        "x264enc" => vec![
            ("tune", "zerolatency".into()),
            ("speed-preset", "ultrafast".into()),
            ("key-int-max", gop.to_string()),
            ("bitrate", kbps.to_string()),
            ("byte-stream", "true".into()),
        ],
        "openh264enc" => vec![
            ("complexity", "low".into()),
            ("gop-size", gop.to_string()),
            ("rate-control", "bitrate".into()),
            ("bitrate", (kbps * 1000).to_string()),
            ("enable-frame-skip", "true".into()),
        ],
        "vaapih264enc" => vec![("bitrate", kbps.to_string()), ("keyframe-period", gop.to_string())],
        "vah264enc" | "vah264lpenc" => vec![
            ("bitrate", kbps.to_string()),
            ("key-int-max", gop.to_string()),
            ("rate-control", "cbr".into()),
            ("target-usage", "7".into()),
        ],
        _ => Vec::new(),
    }
}

fn audio_branch(spec: &Spec, available: &impl Fn(&str) -> bool) -> Option<String> {
    if !audio_capture(available) {
        return None;
    }
    let source = format!(
        "pipewiresrc name={SOUND_PREFIX}source do-timestamp=true \
         stream-properties=\"props,stream.capture.sink=(string)true\" ! queue name={SOUND_PREFIX}queue"
    );
    let raw = |extra: &str| {
        format!(
            "audioconvert name={SOUND_PREFIX}convert ! audioresample name={SOUND_PREFIX}resample ! \
             capsfilter name={SOUND_PREFIX}raw caps=\"audio/x-raw,rate=48000,channels=2{extra}\""
        )
    };
    let pad = format!("mux.sink_{AUDIO_PID}");
    match spec.audio? {
        Audio::Lpcm48 => Some(format!(
            "{source} ! {} ! capssetter name={SOUND_PREFIX}lpcm join=false replace=true \
             caps=\"audio/x-lpcm,width=16,rate=48000,channels=2,dynamic_range=0,emphasis=false,mute=false\" ! {pad}",
            raw(",format=S16BE,layout=interleaved")
        )),
        Audio::Aac48 => {
            if !available("aacparse") {
                return None;
            }
            let aac = AAC_ENCODERS.iter().copied().find(|name| available(name))?;
            Some(format!(
                "{source} ! {} ! {aac} name={SOUND_PREFIX}aac ! aacparse name={SOUND_PREFIX}parse ! \
                 capsfilter name={SOUND_PREFIX}adts caps=\"audio/mpeg,stream-format=adts\" ! {pad}",
                raw("")
            ))
        }
    }
}

#[must_use]
pub fn description(spec: &Spec, available: impl Fn(&str) -> bool) -> String {
    let encoder = spec.encoder.line(spec.bitrate_kbps(), spec.video.mode.fps);
    let mode = spec.video.mode;
    let mut text = format!(
        "appsrc name=video is-live=true do-timestamp=true format=time block=false max-buffers=2 leaky-type=downstream \
         caps=\"video/x-raw,format={},width={},height={},framerate=0/1\" ! queue ! videoscale add-borders=true ! \
         videoconvert ! videorate ! video/x-raw,width={},height={},framerate={}/1,pixel-aspect-ratio=1/1 ! \
         {encoder} ! h264parse config-interval=-1 ! video/x-h264,stream-format=byte-stream,alignment=au,profile={} ! \
         mux.sink_{VIDEO_PID} ",
        spec.frame_format,
        spec.input.0,
        spec.input.1,
        mode.width,
        mode.height,
        mode.fps,
        spec.profile(),
    );
    if let Some(audio) = audio_branch(spec, &available) {
        let _ = write!(text, "{audio} ");
    }
    let _ = write!(
        text,
        "mpegtsmux name=mux alignment=7 prog-map=\"program_map,sink_{VIDEO_PID}=(int)1,sink_{AUDIO_PID}=(int)1,PMT_1=(uint){PMT_PID}\" ! \
         rtpmp2tpay pt={RTP_PAYLOAD_MP2T} ! udpsink host={} port={} sync=false async=false",
        spec.host, spec.port
    );
    text
}

#[must_use]
pub(crate) fn element_exists(name: &str) -> bool {
    gst::ElementFactory::find(name).is_some()
}

#[must_use]
pub(crate) fn missing_element(available: impl Fn(&str) -> bool) -> Option<String> {
    REQUIRED
        .iter()
        .find(|(name, _)| !available(name))
        .map(|(name, package)| format!("GStreamer has no {name} element; install {package}"))
}

#[must_use]
pub(crate) fn no_encoder(installed: &[&str]) -> String {
    if installed.is_empty() {
        "GStreamer has no H.264 encoder; install gst-plugin-va or gst-plugins-bad (vah264enc, openh264enc) \
         or gst-plugins-ugly (x264enc)"
            .into()
    } else {
        format!(
            "none of the installed H.264 encoders ({}) could encode a test picture; install gst-plugins-ugly \
             (x264enc) or gst-plugins-bad (openh264enc)",
            installed.join(", ")
        )
    }
}

pub(crate) fn first_working(works: impl Fn(Encoder) -> Option<Probed>) -> Result<Probed, String> {
    let mut installed = Vec::new();
    for encoder in ENCODERS {
        if !element_exists(encoder.name) {
            continue;
        }
        if let Some(probed) = works(encoder) {
            return Ok(probed);
        }
        installed.push(encoder.name);
    }
    Err(no_encoder(&installed))
}

fn runs(description: &str, limit: Duration) -> bool {
    let Ok(pipeline) = gst::parse::launch(description) else {
        return false;
    };
    let Some(bus) = pipeline.bus() else {
        return false;
    };
    if pipeline.set_state(gst::State::Playing).is_err() {
        let _ = pipeline.set_state(gst::State::Null);
        return false;
    }
    let limit = gst::ClockTime::from_mseconds(u64::try_from(limit.as_millis()).unwrap_or(u64::MAX));
    let finished = bus.timed_pop_filtered(limit, &[gst::MessageType::Eos, gst::MessageType::Error]);
    let _ = pipeline.set_state(gst::State::Null);
    finished.is_some_and(|message| matches!(message.view(), gst::MessageView::Eos(_)))
}

fn try_encoder(encoder: Encoder) -> Option<Probed> {
    let element = gst::ElementFactory::make(encoder.name).build().ok()?;
    let properties = properties(encoder.name, 0, 0)
        .into_iter()
        .map(|(key, _)| key)
        .filter(|key| element.find_property(key).is_some())
        .collect();
    drop(element);
    let probed = Probed { encoder, properties };
    let (width, height) = PROBE_SIZE;
    let description = format!(
        "videotestsrc num-buffers=2 ! video/x-raw,width={width},height={height},framerate=30/1 ! videoconvert ! {} ! \
         fakesink",
        probed.line(8_000, 30)
    );
    runs(&description, PROBE_WAIT).then_some(probed)
}

static PROBED: Mutex<Option<Probed>> = Mutex::new(None);

pub fn probe() -> Result<Probed, String> {
    let mut cached = PROBED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(probed) = cached.as_ref() {
        return Ok(probed.clone());
    }
    gst::init().map_err(|err| format!("GStreamer did not start: {err}"))?;
    if let Some(missing) = missing_element(element_exists) {
        return Err(missing);
    }
    let probed = first_working(try_encoder)?;
    *cached = Some(probed.clone());
    Ok(probed)
}

fn play(pipeline: &gst::Pipeline) -> Result<(), String> {
    if let Err(err) = pipeline.set_state(gst::State::Playing) {
        let _ = pipeline.set_state(gst::State::Null);
        return Err(err.to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Gate {
    pipeline: gst::Pipeline,
}

impl Gate {
    pub(crate) fn pause(&self) -> Result<(), String> {
        self.pipeline.set_state(gst::State::Paused).map(|_| ()).map_err(|err| err.to_string())
    }

    pub(crate) fn resume(&self) -> Result<(), String> {
        self.pipeline.set_state(gst::State::Playing).map(|_| ()).map_err(|err| err.to_string())
    }
}

fn from_sound(source: Option<&gst::Object>) -> bool {
    source.is_some_and(|object| object.name().starts_with(SOUND_PREFIX))
}

fn frame_caps(format: &str, (width, height): (u32, u32)) -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("format", format)
        .field("width", i32::try_from(width).unwrap_or(i32::MAX))
        .field("height", i32::try_from(height).unwrap_or(i32::MAX))
        .field("framerate", gst::Fraction::new(0, 1))
        .build()
}

pub struct Stream {
    pipeline: gst::Pipeline,
    video: gst_app::AppSrc,
    format: &'static str,
    size: (u32, u32),
}

impl Stream {
    pub fn start(spec: &Spec) -> Result<Self, String> {
        gst::init().map_err(|err| err.to_string())?;
        let pipeline = gst::parse::launch(&description(spec, element_exists))
            .map_err(|err| err.to_string())?
            .downcast::<gst::Pipeline>()
            .map_err(|_| "the description did not make a pipeline")?;
        let video = pipeline
            .by_name("video")
            .and_then(|element| element.downcast::<gst_app::AppSrc>().ok())
            .ok_or("the pipeline has no video source")?;
        play(&pipeline)?;
        Ok(Self { pipeline, video, format: spec.frame_format, size: spec.input })
    }

    #[must_use]
    pub(crate) fn gate(&self) -> Gate {
        Gate { pipeline: self.pipeline.clone() }
    }

    pub fn push_frame(&mut self, pixels: Vec<u8>, size: (u32, u32)) -> Result<(), String> {
        let expected = size.0 as usize * size.1 as usize * 4;
        if expected == 0 || pixels.len() != expected {
            return Err(format!("a {}×{} frame cannot hold {} bytes", size.0, size.1, pixels.len()));
        }
        if size != self.size {
            self.video.set_caps(Some(&frame_caps(self.format, size)));
            self.size = size;
        }
        self.video.push_buffer(gst::Buffer::from_mut_slice(pixels)).map(|_| ()).map_err(|err| err.to_string())
    }

    pub(crate) fn request_keyframe(&self) {
        let event = gstreamer_video::DownstreamForceKeyUnitEvent::builder().all_headers(true).build();
        self.video.send_event(event);
    }

    fn silence(&self) {
        let pad = self.pipeline.by_name("mux").and_then(|mux| mux.static_pad(&format!("sink_{AUDIO_PID}")));
        if let Some(pad) = pad {
            pad.send_event(gst::event::Eos::new());
        }
    }

    #[must_use]
    pub fn failure(&self) -> Option<String> {
        let bus = self.pipeline.bus()?;
        while let Some(message) = bus.pop() {
            if let gst::MessageView::Error(error) = message.view() {
                if from_sound(message.src()) {
                    self.silence();
                    continue;
                }
                return Some(error.error().to_string());
            }
        }
        None
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let _ = self.video.end_of_stream();
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::{CEA, PROFILE_CHP};

    fn spec(audio: Option<Audio>) -> Spec {
        Spec {
            video: Video { mode: CEA[5], profile: PROFILE_CBP, level: 0x02 },
            audio,
            host: "127.0.0.1".into(),
            port: 0,
            frame_format: "BGRx",
            input: (640, 480),
            encoder: Probed::assumed("openh264enc"),
        }
    }

    fn with(name: &str) -> Spec {
        Spec { encoder: Probed::assumed(name), ..spec(None) }
    }

    fn everything(_: &str) -> bool {
        true
    }

    #[test]
    fn hardware_encoders_are_tried_first_and_openh264_is_the_last_resort() {
        let names: Vec<&str> = ENCODERS.iter().map(|encoder| encoder.name).collect();
        assert_eq!(names.first(), Some(&"vah264enc"));
        assert_eq!(names.last(), Some(&"openh264enc"));
        let software = names.iter().position(|name| *name == "x264enc").unwrap();
        assert!(ENCODERS[..software].iter().all(|encoder| encoder.hardware));
        assert!(ENCODERS[software..].iter().all(|encoder| !encoder.hardware));
        for name in ["nvh264enc", "qsvh264enc", "v4l2h264enc"] {
            assert!(names.contains(&name), "{name} is a candidate");
            assert_eq!(Probed::assumed(name).line(3456, 30), name, "{name} gets no property nobody checked");
        }
    }

    #[test]
    fn an_encoder_that_fails_its_test_encode_gives_way_to_the_next_one() {
        let installed = |name: &str| element_exists(name);
        if gst::init().is_err() {
            eprintln!("GStreamer did not start; skipping");
            return;
        }
        let tried = Mutex::new(Vec::new());
        let outcome = first_working(|encoder| {
            tried.lock().unwrap().push(encoder.name);
            None
        });
        let tried = tried.into_inner().unwrap();
        assert!(tried.iter().all(|name| installed(name)), "only installed encoders are tried: {tried:?}");
        let reason = outcome.unwrap_err();
        if tried.is_empty() {
            assert!(reason.contains("install"), "{reason}");
        } else {
            assert!(reason.contains(tried[0]), "the failed encoders are named: {reason}");
        }
    }

    #[test]
    fn the_failure_names_what_to_install() {
        assert!(no_encoder(&[]).contains("gst-plugins-ugly (x264enc)"));
        assert!(no_encoder(&["vah264enc"]).contains("(vah264enc) could encode"));
        assert_eq!(missing_element(everything), None);
        let reason = missing_element(|name| name != "mpegtsmux").unwrap();
        assert_eq!(reason, "GStreamer has no mpegtsmux element; install gst-plugins-bad");
    }

    #[test]
    fn raw_video_format_is_negotiated_with_each_encoder_instead_of_forcing_i420() {
        for encoder in ENCODERS {
            let text = description(&with(encoder.name), everything);
            assert!(text.contains("videoconvert ! videorate ! video/x-raw,width="), "{text}");
            assert!(!text.contains("format=I420"), "{} must negotiate its input format: {text}", encoder.name);
        }
    }

    #[test]
    fn each_encoder_is_given_the_keyframe_property_it_really_has() {
        let line = |name: &str| Probed::assumed(name).line(3456, 30);
        assert_eq!(line("vaapih264enc"), "vaapih264enc bitrate=3456 keyframe-period=30");
        assert_eq!(line("vah264enc"), "vah264enc bitrate=3456 key-int-max=30 rate-control=cbr target-usage=7");
        assert_eq!(line("vah264lpenc"), "vah264lpenc bitrate=3456 key-int-max=30 rate-control=cbr target-usage=7");
        assert!(line("x264enc").contains(" key-int-max=30 "));
        assert!(line("openh264enc").contains(" gop-size=30 "));
        let text = description(&with("vaapih264enc"), everything);
        assert!(!text.contains("key-int-max"), "gstreamer-vaapi calls it keyframe-period: {text}");
    }

    #[test]
    fn a_property_the_driver_does_not_offer_is_left_out() {
        let probed = Probed { encoder: ENCODERS[0], properties: vec!["bitrate", "key-int-max", "target-usage"] };
        assert_eq!(probed.line(3456, 30), "vah264enc bitrate=3456 key-int-max=30 target-usage=7");
    }

    const CONDITIONAL: [&str; 1] = ["rate-control"];

    #[test]
    fn installed_encoders_know_every_property_they_are_given() {
        if gst::init().is_err() {
            eprintln!("GStreamer did not start; skipping");
            return;
        }
        for encoder in ENCODERS {
            let Ok(element) = gst::ElementFactory::make(encoder.name).build() else {
                eprintln!("{} cannot be made here; skipping", encoder.name);
                continue;
            };
            for (property, _) in properties(encoder.name, 3456, 30) {
                if encoder.name.starts_with("vah264") && CONDITIONAL.contains(&property) {
                    continue;
                }
                assert!(element.find_property(property).is_some(), "{} has no property {property}", encoder.name);
            }
        }
    }

    #[test]
    fn the_probe_settles_on_an_encoder_that_really_encodes() {
        if gst::init().is_err() || !element_exists("openh264enc") && !element_exists("x264enc") {
            eprintln!("no software H.264 encoder; skipping");
            return;
        }
        let probed = probe().unwrap();
        assert_eq!(probe().unwrap(), probed, "the probe runs once per process");
        assert!(runs(
            &format!("videotestsrc num-buffers=2 ! videoconvert ! {} ! fakesink", probed.line(2000, 30)),
            PROBE_WAIT
        ));
        assert!(!runs("videotestsrc num-buffers=2 ! video/x-raw,format=NOPE ! fakesink", PROBE_WAIT));
    }

    #[test]
    fn an_aac_encoder_is_one_of_the_three_the_pipeline_can_use() {
        let with_parser = |name: &'static str| move |wanted: &str| wanted == name || wanted == "aacparse";
        assert!(aac_encoder(with_parser("avenc_aac")));
        assert!(aac_encoder(with_parser("fdkaacenc")));
        assert!(!aac_encoder(with_parser("openh264enc")));
        assert!(!aac_encoder(|name: &str| name == "avenc_aac"), "no aacparse, no AAC");
    }

    #[test]
    fn sound_is_left_out_when_pipewiresrc_is_missing() {
        assert!(audio_capture(everything));
        let no_pipewire = |name: &str| name != "pipewiresrc";
        assert!(!audio_capture(no_pipewire));
        let text = description(&spec(Some(Audio::Lpcm48)), no_pipewire);
        assert!(!text.contains("pipewiresrc") && !text.contains("audio/x-lpcm"), "{text}");
    }

    #[test]
    fn a_pipeline_that_cannot_start_is_left_in_the_null_state() {
        if gst::init().is_err() {
            eprintln!("GStreamer did not start; skipping");
            return;
        }
        let broken = || {
            gst::parse::launch("filesrc location=/nonexistent/glide-cast-test ! fakesink")
                .unwrap()
                .downcast::<gst::Pipeline>()
                .unwrap()
        };
        let raw = broken();
        assert!(raw.set_state(gst::State::Playing).is_err());
        assert_ne!(raw.current_state(), gst::State::Null, "a failed start leaves the pipeline half started");
        let _ = raw.set_state(gst::State::Null);

        let pipeline = broken();
        assert!(play(&pipeline).is_err());
        assert_eq!(pipeline.current_state(), gst::State::Null);
    }

    fn live(port: u16) -> Option<Spec> {
        let Ok(encoder) = probe() else {
            eprintln!("no working GStreamer H.264 encoder; skipping");
            return None;
        };
        Some(Spec { video: Video { mode: CEA[0], ..spec(None).video }, port, encoder, ..spec(None) })
    }

    #[test]
    fn a_paused_stream_sends_nothing_and_a_resumed_one_goes_on() {
        let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = receiver.local_addr().unwrap().port();
        let Some(small) = live(port) else { return };
        let mut stream = Stream::start(&small).unwrap();
        let frame = vec![0x80; 640 * 480 * 4];
        let feed = |stream: &mut Stream, count: usize| {
            for _ in 0..count {
                let _ = stream.push_frame(frame.clone(), (640, 480));
                std::thread::sleep(std::time::Duration::from_millis(33));
            }
        };
        let mut packet = [0; 2048];
        receiver.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        feed(&mut stream, 15);
        receiver.recv(&mut packet).unwrap();

        let gate = stream.gate();
        assert_eq!(gate, stream.gate(), "every gate is the same pipeline");
        gate.pause().unwrap();
        receiver.set_read_timeout(Some(std::time::Duration::from_millis(400))).unwrap();
        while receiver.recv(&mut packet).is_ok() {}
        feed(&mut stream, 15);
        assert!(receiver.recv(&mut packet).is_err(), "a paused stream sends no packets");

        gate.resume().unwrap();
        stream.request_keyframe();
        receiver.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        feed(&mut stream, 15);
        receiver.recv(&mut packet).unwrap();
        assert_eq!(packet[12], 0x47, "the payload starts with a TS sync byte again");
        assert_eq!(stream.failure(), None);
    }

    #[test]
    fn the_stream_is_mpeg_ts_over_rtp_to_the_display() {
        let text = description(&Spec { port: 1028, ..spec(None) }, everything);
        assert!(text.contains("profile=constrained-baseline ! mux.sink_4113 "), "{text}");
        assert!(text.ends_with("rtpmp2tpay pt=33 ! udpsink host=127.0.0.1 port=1028 sync=false async=false"));
        assert!(text.contains("PMT_1=(uint)256"), "{text}");
        let high = Spec { video: Video { profile: PROFILE_CHP, ..spec(None).video }, ..spec(None) };
        assert!(description(&high, everything).contains("profile=high"));
    }

    #[test]
    fn lpcm_needs_no_encoder_but_aac_does() {
        let lpcm = description(&spec(Some(Audio::Lpcm48)), everything);
        assert!(lpcm.contains("audio/x-lpcm,width=16,rate=48000"), "{lpcm}");
        assert!(lpcm.contains("stream.capture.sink=(string)true"), "{lpcm}");
        assert!(lpcm.contains("! mux.sink_4352 "), "{lpcm}");
        let no_aac = |name: &str| !AAC_ENCODERS.contains(&name);
        let silent = description(&spec(Some(Audio::Aac48)), no_aac);
        assert!(!silent.contains("pipewiresrc"), "without an AAC encoder the stream goes out without sound");
        let only_avenc = |name: &str| name == "avenc_aac" || !AAC_ENCODERS.contains(&name);
        assert!(description(&spec(Some(Audio::Aac48)), only_avenc).contains("avenc_aac name=sound-aac ! aacparse"));
    }

    #[test]
    fn every_element_of_the_sound_branch_is_named_so_its_errors_spare_the_picture() {
        for audio in [Audio::Lpcm48, Audio::Aac48] {
            let branch = audio_branch(&spec(Some(audio)), &everything).unwrap();
            let elements: Vec<&str> = branch.split(" ! ").collect();
            let (pad, elements) = elements.split_last().unwrap();
            assert_eq!(*pad, "mux.sink_4352");
            for element in elements {
                assert!(element.contains(" name=sound-"), "{element} would end the whole stream on an error");
            }
        }
    }

    #[test]
    fn the_sound_branches_parse_into_real_pipelines() {
        if gst::init().is_err() || !audio_capture(element_exists) || !element_exists("openh264enc") {
            eprintln!("no GStreamer audio capture or openh264enc; skipping");
            return;
        }
        for audio in [Audio::Lpcm48, Audio::Aac48] {
            let text = description(&spec(Some(audio)), element_exists);
            let pipeline = gst::parse::launch(&text).unwrap_or_else(|err| panic!("{err}: {text}"));
            let pipeline = pipeline.downcast::<gst::Pipeline>().unwrap();
            if text.contains("pipewiresrc") {
                assert!(pipeline.by_name("sound-source").is_some());
                let mux = pipeline.by_name("mux").unwrap();
                assert!(mux.static_pad("sink_4352").is_some(), "sound reaches the mux on PID 0x1100");
            }
        }
    }

    #[test]
    fn the_bitrate_follows_the_pixels_per_second() {
        let at = |bit: usize| Spec { video: Video { mode: CEA[bit], ..spec(None).video }, ..spec(None) }.bitrate_kbps();
        assert_eq!(at(0), 2304, "640×480 at 60");
        assert_eq!(at(5), 3456, "1280×720 at 30");
        assert_eq!(at(8), 15552, "1920×1080 at 60");
    }

    fn pids(packet: &[u8]) -> Vec<u32> {
        packet[12..]
            .chunks_exact(188)
            .filter(|chunk| chunk[0] == 0x47)
            .map(|chunk| (u32::from(chunk[1] & 0x1f) << 8) | u32::from(chunk[2]))
            .collect()
    }

    #[test]
    fn frames_pushed_in_come_out_as_rtp_carrying_transport_stream() {
        let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        let port = receiver.local_addr().unwrap().port();
        let Some(small) = live(port) else { return };
        let mut stream = Stream::start(&small).unwrap();
        let frame = vec![0x80; 640 * 480 * 4];
        for _ in 0..15 {
            stream.push_frame(frame.clone(), (640, 480)).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(33));
        }
        let mut packet = [0; 2048];
        let size = receiver.recv(&mut packet).unwrap();
        assert!(size > 12 + 188, "an RTP packet carries at least one 188-byte TS packet");
        assert_eq!(packet[0] >> 6, 2, "RTP version 2");
        assert_eq!(packet[1] & 0x7f, 33, "MP2T payload type");
        assert_eq!(packet[12], 0x47, "the payload starts with a TS sync byte");
        let mut seen = pids(&packet[..size]);
        while let Ok(size) = receiver.recv(&mut packet) {
            seen.extend(pids(&packet[..size]));
            if seen.contains(&PMT_PID) && seen.contains(&VIDEO_PID) {
                break;
            }
        }
        assert!(seen.contains(&0), "the PAT is sent: {seen:?}");
        assert!(seen.contains(&PMT_PID), "the PMT goes out on PID 0x100 as WFD asks: {seen:?}");
        assert!(seen.contains(&VIDEO_PID), "video goes out on PID 0x1011 as WFD asks: {seen:?}");
        assert_eq!(stream.failure(), None);
    }

    #[test]
    fn a_new_frame_size_changes_the_source_caps_and_the_stream_goes_on() {
        let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        let port = receiver.local_addr().unwrap().port();
        let Some(small) = live(port) else { return };
        let mut stream = Stream::start(&small).unwrap();
        assert!(stream.push_frame(vec![0; 16], (640, 480)).is_err(), "a short frame is refused");
        for size in [(640, 480), (800, 500)] {
            let frame = vec![0x60; size.0 as usize * size.1 as usize * 4];
            for _ in 0..10 {
                stream.push_frame(frame.clone(), size).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(33));
            }
        }
        let caps = stream.video.caps().unwrap();
        assert_eq!(caps.structure(0).unwrap().get::<i32>("width"), Ok(800));
        let mut packet = [0; 2048];
        while receiver.recv(&mut packet).is_ok() {
            receiver.set_read_timeout(Some(std::time::Duration::from_millis(300))).unwrap();
        }
        let frame = vec![0x60; 800 * 500 * 4];
        receiver.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        for _ in 0..10 {
            stream.push_frame(frame.clone(), (800, 500)).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(33));
        }
        receiver.recv(&mut packet).unwrap();
        assert_eq!(stream.failure(), None);
    }

    #[test]
    fn a_sound_failure_ends_the_sound_but_not_the_picture() {
        let Some(small) = live(9) else { return };
        let stream = Stream::start(&small).unwrap();
        let bus = stream.pipeline.bus().unwrap();
        let sound = gst::ElementFactory::make("fakesrc").name("sound-source").build().unwrap();
        let _ = bus.post(gst::message::Error::builder(gst::CoreError::Failed, "no PipeWire").src(&sound).build());
        assert_eq!(stream.failure(), None, "a sound error leaves the stream running");
        let _ =
            bus.post(gst::message::Error::builder(gst::CoreError::Failed, "encoder died").src(&stream.video).build());
        assert_eq!(stream.failure().as_deref(), Some("encoder died"));
    }
}

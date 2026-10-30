use std::fmt::Write as _;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use crate::formats::{Audio, PROFILE_CBP, Video};

const ENCODERS: [&str; 5] = ["vah264enc", "vah264lpenc", "vaapih264enc", "x264enc", "openh264enc"];
const AAC_ENCODERS: [&str; 3] = ["fdkaacenc", "avenc_aac", "voaacenc"];
const RTP_PAYLOAD_MP2T: u32 = 33;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub video: Video,
    pub audio: Option<Audio>,
    pub host: String,
    pub port: u16,
    pub frame_format: &'static str,
    pub input: (u32, u32),
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
pub(crate) fn hardware_encoder(available: impl Fn(&str) -> bool) -> bool {
    ENCODERS[..3].iter().any(|name| available(name))
}

#[must_use]
pub(crate) fn aac_encoder(available: impl Fn(&str) -> bool) -> bool {
    AAC_ENCODERS.iter().any(|name| available(name))
}

fn encoder(spec: &Spec, available: &impl Fn(&str) -> bool) -> Option<String> {
    let kbps = spec.bitrate_kbps();
    let gop = spec.video.mode.fps;
    let name = ENCODERS.iter().copied().find(|name| available(name))?;
    Some(match name {
        "x264enc" => {
            format!("x264enc tune=zerolatency speed-preset=ultrafast key-int-max={gop} bitrate={kbps} byte-stream=true")
        }
        "openh264enc" => format!(
            "openh264enc complexity=low gop-size={gop} rate-control=bitrate bitrate={} enable-frame-skip=true",
            kbps * 1000
        ),
        "vaapih264enc" => format!("vaapih264enc bitrate={kbps} keyframe-period={gop}"),
        hardware => format!("{hardware} bitrate={kbps} key-int-max={gop}"),
    })
}

fn audio_branch(spec: &Spec, available: &impl Fn(&str) -> bool) -> Option<String> {
    let source = "pipewiresrc do-timestamp=true stream-properties=\"props,stream.capture.sink=true\"";
    let raw = "audioconvert ! audioresample ! audio/x-raw,rate=48000,channels=2";
    match spec.audio? {
        Audio::Lpcm48 => Some(format!(
            "{source} ! queue ! {raw},format=S16BE,layout=interleaved ! capssetter join=false replace=true \
             caps=\"audio/x-lpcm,width=16,rate=48000,channels=2,dynamic_range=0,emphasis=false,mute=false\" ! mux."
        )),
        Audio::Aac48 => {
            let aac = AAC_ENCODERS.iter().copied().find(|name| available(name))?;
            Some(format!("{source} ! queue ! {raw} ! {aac} ! aacparse ! audio/mpeg,stream-format=adts ! mux."))
        }
    }
}

#[must_use]
pub fn description(spec: &Spec, available: impl Fn(&str) -> bool) -> Option<String> {
    let encoder = encoder(spec, &available)?;
    let mode = spec.video.mode;
    let mut text = format!(
        "appsrc name=video is-live=true do-timestamp=true format=time block=false max-buffers=2 leaky-type=downstream \
         caps=\"video/x-raw,format={},width={},height={},framerate=0/1\" ! queue ! videoscale add-borders=true ! \
         videoconvert ! videorate ! video/x-raw,format=I420,width={},height={},framerate={}/1,pixel-aspect-ratio=1/1 ! \
         {encoder} ! h264parse config-interval=-1 ! video/x-h264,stream-format=byte-stream,alignment=au,profile={} ! mux. ",
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
        "mpegtsmux name=mux alignment=7 ! rtpmp2tpay pt={RTP_PAYLOAD_MP2T} ! udpsink host={} port={} sync=false async=false",
        spec.host, spec.port
    );
    Some(text)
}

#[must_use]
pub(crate) fn element_exists(name: &str) -> bool {
    gst::ElementFactory::find(name).is_some()
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

pub struct Stream {
    pipeline: gst::Pipeline,
    video: gst_app::AppSrc,
}

impl Stream {
    pub fn start(spec: &Spec) -> Result<Self, String> {
        gst::init().map_err(|err| err.to_string())?;
        let description = description(spec, element_exists).ok_or("no H.264 encoder is installed for GStreamer")?;
        let pipeline = gst::parse::launch(&description)
            .map_err(|err| err.to_string())?
            .downcast::<gst::Pipeline>()
            .map_err(|_| "the description did not make a pipeline")?;
        let video = pipeline
            .by_name("video")
            .and_then(|element| element.downcast::<gst_app::AppSrc>().ok())
            .ok_or("the pipeline has no video source")?;
        play(&pipeline)?;
        Ok(Self { pipeline, video })
    }

    #[must_use]
    pub(crate) fn gate(&self) -> Gate {
        Gate { pipeline: self.pipeline.clone() }
    }

    pub fn push_frame(&self, pixels: Vec<u8>) -> Result<(), String> {
        self.video.push_buffer(gst::Buffer::from_mut_slice(pixels)).map(|_| ()).map_err(|err| err.to_string())
    }

    pub(crate) fn request_keyframe(&self) {
        let event = gstreamer_video::DownstreamForceKeyUnitEvent::builder().all_headers(true).build();
        self.video.send_event(event);
    }

    #[must_use]
    pub fn failure(&self) -> Option<String> {
        let bus = self.pipeline.bus()?;
        while let Some(message) = bus.pop() {
            if let gst::MessageView::Error(error) = message.view() {
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
        }
    }

    #[test]
    fn hardware_encoders_win_and_openh264_is_the_last_resort() {
        let all = |_: &str| true;
        assert!(description(&spec(None), all).unwrap().contains("vah264enc bitrate="));
        let software = |name: &str| name == "openh264enc";
        let text = description(&spec(None), software).unwrap();
        assert!(text.contains("openh264enc complexity=low gop-size=30"), "{text}");
        assert!(!hardware_encoder(software));
        assert_eq!(description(&spec(None), |_: &str| false), None, "no encoder, no stream");
    }

    fn only(wanted: &'static str) -> impl Fn(&str) -> bool {
        move |name| name == wanted
    }

    #[test]
    fn each_encoder_is_given_the_keyframe_property_it_really_has() {
        let line = |name: &'static str| encoder(&spec(None), &only(name)).unwrap();
        assert_eq!(line("vaapih264enc"), "vaapih264enc bitrate=3456 keyframe-period=30");
        assert_eq!(line("vah264enc"), "vah264enc bitrate=3456 key-int-max=30");
        assert_eq!(line("vah264lpenc"), "vah264lpenc bitrate=3456 key-int-max=30");
        assert!(line("x264enc").contains(" key-int-max=30 "));
        assert!(line("openh264enc").contains(" gop-size=30 "));
        let text = description(&spec(None), only("vaapih264enc")).unwrap();
        assert!(!text.contains("key-int-max"), "gstreamer-vaapi calls it keyframe-period: {text}");
    }

    #[test]
    fn installed_encoders_know_every_property_they_are_given() {
        if gst::init().is_err() {
            eprintln!("GStreamer did not start; skipping");
            return;
        }
        for name in ENCODERS {
            let Ok(element) = gst::ElementFactory::make(name).build() else {
                eprintln!("{name} cannot be made here; skipping");
                continue;
            };
            let text = encoder(&spec(None), &only(name)).unwrap();
            for property in text.split_whitespace().filter_map(|token| token.split_once('=')).map(|(key, _)| key) {
                assert!(element.find_property(property).is_some(), "{name} has no property {property}: {text}");
            }
        }
    }

    #[test]
    fn an_aac_encoder_is_one_of_the_three_the_pipeline_can_use() {
        assert!(aac_encoder(only("avenc_aac")));
        assert!(aac_encoder(only("fdkaacenc")));
        assert!(!aac_encoder(only("openh264enc")));
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

    #[test]
    fn a_paused_stream_sends_nothing_and_a_resumed_one_goes_on() {
        if gst::init().is_err() || !element_exists("openh264enc") && !hardware_encoder(element_exists) {
            eprintln!("no GStreamer H.264 encoder; skipping");
            return;
        }
        let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = receiver.local_addr().unwrap().port();
        let small = Spec { video: Video { mode: CEA[0], ..spec(None).video }, port, ..spec(None) };
        let stream = Stream::start(&small).unwrap();
        let frame = vec![0x80; 640 * 480 * 4];
        let feed = |count: usize| {
            for _ in 0..count {
                let _ = stream.push_frame(frame.clone());
                std::thread::sleep(std::time::Duration::from_millis(33));
            }
        };
        let mut packet = [0; 2048];
        receiver.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        feed(15);
        receiver.recv(&mut packet).unwrap();

        let gate = stream.gate();
        assert_eq!(gate, stream.gate(), "every gate is the same pipeline");
        gate.pause().unwrap();
        receiver.set_read_timeout(Some(std::time::Duration::from_millis(400))).unwrap();
        while receiver.recv(&mut packet).is_ok() {}
        feed(15);
        assert!(receiver.recv(&mut packet).is_err(), "a paused stream sends no packets");

        gate.resume().unwrap();
        stream.request_keyframe();
        receiver.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        feed(15);
        receiver.recv(&mut packet).unwrap();
        assert_eq!(packet[12], 0x47, "the payload starts with a TS sync byte again");
        assert_eq!(stream.failure(), None);
    }

    #[test]
    fn the_stream_is_mpeg_ts_over_rtp_to_the_display() {
        let text = description(&Spec { port: 1028, ..spec(None) }, |_: &str| true).unwrap();
        assert!(text.contains("profile=constrained-baseline"));
        assert!(text.ends_with("mpegtsmux name=mux alignment=7 ! rtpmp2tpay pt=33 ! udpsink host=127.0.0.1 port=1028 sync=false async=false"));
        let high = Spec { video: Video { profile: PROFILE_CHP, ..spec(None).video }, ..spec(None) };
        assert!(description(&high, |_: &str| true).unwrap().contains("profile=high"));
    }

    #[test]
    fn lpcm_needs_no_encoder_but_aac_does() {
        let software = |name: &str| name == "openh264enc";
        assert!(
            description(&spec(Some(Audio::Lpcm48)), software).unwrap().contains("audio/x-lpcm,width=16,rate=48000")
        );
        let silent = description(&spec(Some(Audio::Aac48)), software).unwrap();
        assert!(!silent.contains("pipewiresrc"), "without an AAC encoder the stream goes out without sound");
        let with_aac = |name: &str| name == "openh264enc" || name == "avenc_aac";
        assert!(description(&spec(Some(Audio::Aac48)), with_aac).unwrap().contains("avenc_aac ! aacparse"));
    }

    #[test]
    fn the_bitrate_follows_the_pixels_per_second() {
        let at = |bit: usize| Spec { video: Video { mode: CEA[bit], ..spec(None).video }, ..spec(None) }.bitrate_kbps();
        assert_eq!(at(0), 2304, "640×480 at 60");
        assert_eq!(at(5), 3456, "1280×720 at 30");
        assert_eq!(at(8), 15552, "1920×1080 at 60");
    }

    #[test]
    fn frames_pushed_in_come_out_as_rtp_carrying_transport_stream() {
        if gst::init().is_err() || !element_exists("openh264enc") && !hardware_encoder(element_exists) {
            eprintln!("no GStreamer H.264 encoder; skipping");
            return;
        }
        let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        let port = receiver.local_addr().unwrap().port();
        let small = Spec { video: Video { mode: CEA[0], ..spec(None).video }, port, ..spec(None) };
        let stream = Stream::start(&small).unwrap();
        let frame = vec![0x80; 640 * 480 * 4];
        for _ in 0..15 {
            stream.push_frame(frame.clone()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(33));
        }
        let mut packet = [0; 2048];
        let size = receiver.recv(&mut packet).unwrap();
        assert!(size > 12 + 188, "an RTP packet carries at least one 188-byte TS packet");
        assert_eq!(packet[0] >> 6, 2, "RTP version 2");
        assert_eq!(packet[1] & 0x7f, 33, "MP2T payload type");
        assert_eq!(packet[12], 0x47, "the payload starts with a TS sync byte");
        assert_eq!(stream.failure(), None);
    }
}

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSink;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub tag: u64,
    pub at: Duration,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug)]
struct Request {
    tag: u64,
    times: Vec<Duration>,
    accurate: bool,
}

pub struct Grabber {
    sender: Sender<Request>,
}

impl Grabber {
    #[must_use]
    pub fn spawn(uri: &str, width: u32) -> (Self, UnboundedReceiver<Frame>) {
        let (sender, requests) = mpsc::channel();
        let (results, receiver) = unbounded();
        let uri = uri.to_string();
        let spawned = std::thread::Builder::new().name("glide-video-frames".into()).spawn(move || {
            if let Err(err) = run(&uri, width, &requests, &results) {
                tracing::debug!("frame grabber stopped: {err}");
            }
        });
        if let Err(err) = spawned {
            tracing::warn!("could not start the frame grabber: {err}");
        }
        (Self { sender }, receiver)
    }

    pub fn request(&self, tag: u64, times: Vec<Duration>, accurate: bool) {
        let _ = self.sender.send(Request { tag, times, accurate });
    }
}

fn build(uri: &str, width: u32) -> Result<(gst::Element, AppSink), String> {
    crate::init()?;
    let playbin = gst::ElementFactory::make("playbin3").build().map_err(|err| err.to_string())?;
    playbin.set_property("uri", uri);
    playbin.set_property_from_str("flags", "video");
    let bin = gst::Bin::new();
    let convert = gst::ElementFactory::make("videoconvert").build().map_err(|err| err.to_string())?;
    let scale = gst::ElementFactory::make("videoscale").build().map_err(|err| err.to_string())?;
    let caps = gst::Caps::builder("video/x-raw")
        .field("format", "RGBA")
        .field("width", width as i32)
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .build();
    let sink = AppSink::builder().caps(&caps).sync(false).max_buffers(1).drop(true).build();
    bin.add_many([&convert, &scale, sink.upcast_ref()]).map_err(|err| err.to_string())?;
    gst::Element::link_many([&convert, &scale, sink.upcast_ref()]).map_err(|err| err.to_string())?;
    let pad = convert.static_pad("sink").ok_or("no sink pad")?;
    bin.add_pad(&gst::GhostPad::with_target(&pad).map_err(|err| err.to_string())?).map_err(|err| err.to_string())?;
    playbin.set_property("video-sink", &bin);
    Ok((playbin, sink))
}

fn settle(playbin: &gst::Element) -> bool {
    playbin.state(gst::ClockTime::from_seconds(5)).0.is_ok()
}

fn grab(playbin: &gst::Element, sink: &AppSink, at: Duration, accurate: bool) -> Option<(u32, u32, Vec<u8>)> {
    let flags = if accurate {
        gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE
    } else {
        gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT | gst::SeekFlags::SNAP_NEAREST
    };
    playbin.seek_simple(flags, gst::ClockTime::from_nseconds(at.as_nanos() as u64)).ok()?;
    if !settle(playbin) {
        return None;
    }
    let sample = sink.try_pull_preroll(gst::ClockTime::from_seconds(3))?;
    let caps = sample.caps()?;
    let info = gstreamer_video::VideoInfo::from_caps(caps).ok()?;
    let buffer = sample.buffer()?;
    let map = buffer.map_readable().ok()?;
    let (width, height, stride) = (info.width(), info.height(), info.stride()[0] as usize);
    let row = width as usize * 4;
    let mut rgba = Vec::with_capacity(row * height as usize);
    for line in map.as_slice().chunks(stride).take(height as usize) {
        rgba.extend_from_slice(line.get(..row)?);
    }
    Some((width, height, rgba))
}

fn run(uri: &str, width: u32, requests: &Receiver<Request>, results: &UnboundedSender<Frame>) -> Result<(), String> {
    let (playbin, sink) = build(uri, width)?;
    playbin.set_state(gst::State::Paused).map_err(|err| err.to_string())?;
    if !settle(&playbin) {
        let _ = playbin.set_state(gst::State::Null);
        return Err("the file did not preroll".into());
    }
    while let Ok(mut request) = requests.recv() {
        while let Ok(newer) = requests.try_recv() {
            request = newer;
        }
        for at in request.times {
            if results.is_closed() {
                break;
            }
            if let Some((width, height, rgba)) = grab(&playbin, &sink, at, request.accurate) {
                let _ = results.unbounded_send(Frame { tag: request.tag, at, width, height, rgba });
            }
        }
        if results.is_closed() {
            break;
        }
    }
    let _ = playbin.set_state(gst::State::Null);
    Ok(())
}

#[must_use]
pub fn strip_times(duration: Duration, count: usize) -> Vec<Duration> {
    if count == 0 {
        return Vec::new();
    }
    let step = duration.as_secs_f64() / count as f64;
    (0..count).map(|index| Duration::from_secs_f64(step * (index as f64 + 0.5))).collect()
}

#[must_use]
pub fn bucket(at: Duration, duration: Duration) -> Duration {
    let size = (duration.as_secs_f64() / 200.0).clamp(1.0, 30.0);
    Duration::from_secs_f64((at.as_secs_f64() / size).floor() * size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_frames_sit_in_the_middle_of_their_slices() {
        let times = strip_times(Duration::from_secs(12), 4);
        assert_eq!(
            times,
            vec![
                Duration::from_secs_f64(1.5),
                Duration::from_secs_f64(4.5),
                Duration::from_secs_f64(7.5),
                Duration::from_secs_f64(10.5)
            ]
        );
        assert!(strip_times(Duration::from_secs(5), 0).is_empty());
    }

    #[test]
    fn preview_buckets_grow_with_the_length_but_stay_bounded() {
        assert_eq!(bucket(Duration::from_secs(61), Duration::from_mins(1)), Duration::from_secs(61));
        assert_eq!(bucket(Duration::from_secs(2534), Duration::from_mins(90)), Duration::from_secs(2511));
        assert_eq!(bucket(Duration::from_mins(150), Duration::from_hours(10)), Duration::from_mins(150));
    }
}

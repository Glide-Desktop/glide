#![allow(dead_code)] // Each test binary uses its own subset of these fixtures

use std::path::{Path, PathBuf};

use glide_video::gstreamer as gst;
use gst::prelude::*;

pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("glide-video-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

pub fn uri(path: &Path) -> String {
    gst::glib::filename_to_uri(path, None).unwrap().to_string()
}

pub fn fixture(name: &str, seconds: u32, chapters: &[(u64, &str)]) -> PathBuf {
    glide_video::init().unwrap();
    let path = scratch(name);
    if path.exists() {
        return path;
    }
    let mp4 = Path::new(name).extension().is_some_and(|extension| extension.eq_ignore_ascii_case("mp4"));
    let frames = seconds * 25;
    let samples = seconds * 48_000 / 960;
    let (mux, audio) = if mp4 { ("mp4mux", "avenc_aac ! aacparse") } else { ("matroskamux", "opusenc") };
    let description = format!(
        "videotestsrc num-buffers={frames} ! video/x-raw,width=320,height=180,framerate=25/1 ! \
         x264enc speed-preset=ultrafast key-int-max=25 ! h264parse ! {mux} name=mux ! filesink location={} \
         audiotestsrc num-buffers={samples} samplesperbuffer=960 ! audio/x-raw,rate=48000,channels=2 ! audioconvert ! {audio} ! mux.",
        path.display()
    );
    let pipeline = gst::parse::launch(&description).unwrap().downcast::<gst::Pipeline>().unwrap();
    if !chapters.is_empty() {
        let mut toc = gst::Toc::new(gst::TocScope::Global);
        let mut edition = gst::TocEntry::new(gst::TocEntryType::Edition, "edition");
        for (index, (start, title)) in chapters.iter().enumerate() {
            let mut entry = gst::TocEntry::new(gst::TocEntryType::Chapter, &format!("chapter{index}"));
            let stop = chapters.get(index + 1).map_or(u64::from(seconds), |next| next.0);
            let mut tags = gst::TagList::new();
            tags.get_mut().unwrap().add::<gst::tags::Title>(title, gst::TagMergeMode::Replace);
            let entry_mut = entry.get_mut().unwrap();
            entry_mut.set_start_stop_times((*start * 1_000_000_000) as i64, (stop * 1_000_000_000) as i64);
            entry_mut.set_tags(tags);
            edition.get_mut().unwrap().append_sub_entry(entry);
        }
        toc.get_mut().unwrap().append_entry(edition);
        let mux = pipeline.by_name("mux").unwrap();
        mux.dynamic_cast_ref::<gst::TocSetter>().unwrap().set_toc(Some(&toc));
    }
    pipeline.set_state(gst::State::Playing).unwrap();
    let bus = pipeline.bus().unwrap();
    let message = bus
        .timed_pop_filtered(gst::ClockTime::from_seconds(60), &[gst::MessageType::Eos, gst::MessageType::Error])
        .expect("fixture finished");
    assert!(matches!(message.view(), gst::MessageView::Eos(_)), "fixture failed: {message:?}");
    pipeline.set_state(gst::State::Null).unwrap();
    path
}

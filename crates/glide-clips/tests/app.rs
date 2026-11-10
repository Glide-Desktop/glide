use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use glide_clips::App;
use glide_clips::app::{Message, Mode};
use glide_sdk::widget::player::Trim;
use glide_sdk::{Harness, Look};
use glide_video::gstreamer as gst;
use glide_video::media::TrackKind;
use glide_video::player::Playback;
use gst::prelude::*;

fn isolate() -> PathBuf {
    let root = std::env::temp_dir().join(format!("glide-clips-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["state", "config", "cache", "data", "videos"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    // SAFETY: this binary holds a single test, and the variables are set before the harness or any worker thread reads the environment.
    unsafe {
        std::env::set_var("XDG_STATE_HOME", root.join("state"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_CACHE_HOME", root.join("cache"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent/glide-clips-test");
        std::env::set_var("WAYLAND_DISPLAY", "");
    }
    root
}

fn fixture(path: &Path, seconds: u32) {
    glide_video::init().unwrap();
    let frames = seconds * 25;
    let samples = seconds * 48_000 / 960;
    let description = format!(
        "videotestsrc num-buffers={frames} ! video/x-raw,width=320,height=180,framerate=25/1 ! \
         x264enc speed-preset=ultrafast key-int-max=25 ! h264parse ! matroskamux name=mux ! filesink location={} \
         audiotestsrc num-buffers={samples} samplesperbuffer=960 ! audio/x-raw,rate=48000,channels=2 ! audioconvert ! opusenc ! mux.",
        path.display()
    );
    let pipeline = gst::parse::launch(&description).unwrap().downcast::<gst::Pipeline>().unwrap();
    let mut toc = gst::Toc::new(gst::TocScope::Global);
    let mut edition = gst::TocEntry::new(gst::TocEntryType::Edition, "edition");
    for (index, (start, stop, title)) in [(0_u64, 3_u64, "Світанок"), (3, 6, "Туман")].into_iter().enumerate()
    {
        let mut entry = gst::TocEntry::new(gst::TocEntryType::Chapter, &format!("chapter{index}"));
        let mut tags = gst::TagList::new();
        tags.get_mut().unwrap().add::<gst::tags::Title>(&title, gst::TagMergeMode::Replace);
        let entry_mut = entry.get_mut().unwrap();
        entry_mut.set_start_stop_times((start * 1_000_000_000) as i64, (stop * 1_000_000_000) as i64);
        entry_mut.set_tags(tags);
        edition.get_mut().unwrap().append_sub_entry(entry);
    }
    toc.get_mut().unwrap().append_entry(edition);
    pipeline.by_name("mux").unwrap().dynamic_cast_ref::<gst::TocSetter>().unwrap().set_toc(Some(&toc));
    pipeline.set_state(gst::State::Playing).unwrap();
    let message = pipeline
        .bus()
        .unwrap()
        .timed_pop_filtered(gst::ClockTime::from_seconds(60), &[gst::MessageType::Eos, gst::MessageType::Error])
        .unwrap();
    assert!(matches!(message.view(), gst::MessageView::Eos(_)));
    pipeline.set_state(gst::State::Null).unwrap();
}

fn pump(harness: &mut Harness<App>, until: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let polled = harness.app().media.as_ref().and_then(|media| {
            let event = media.player.as_ref()?.poll(Duration::from_millis(50))?;
            Some((media.feed, event))
        });
        if let Some((generation, event)) = polled {
            harness.message(Message::Player(generation, event));
        }
        harness.message(Message::Tick);
        let _ = harness.view();
        if until(harness.app()) {
            return;
        }
    }
    panic!("the player never reached the expected state");
}

#[test]
fn a_video_opens_plays_and_is_edited_through_the_whole_app() {
    let root = isolate();
    let file = root.join("videos").join("Карпати.mkv");
    fixture(&file, 6);
    std::fs::write(root.join("videos").join("Карпати.uk.srt"), "1\n00:00:00,100 --> 00:00:05,000\nДивись, туман\n\n")
        .unwrap();
    let harness = &mut Harness::<App>::new(glide_clips::APP_ID, Look::default(), iced::Size::new(1280.0, 720.0));
    let _ = harness.view();
    harness.message(Message::OpenPath(file));
    harness.message(Message::Handles(None));
    assert!(harness.app().media.is_some(), "the file opens once the window has no surface to wait for");
    let generation = harness.app().media.as_ref().unwrap().generation;
    let info = glide_video::info::discover(&harness.app().media.as_ref().unwrap().uri, Duration::from_secs(10))
        .ok()
        .map(Box::new);
    harness.message(Message::Info(generation, info));
    let lines = glide_clips::app::read_sidecar(harness.app().media.as_ref().unwrap().suburi.as_deref().unwrap());
    assert_eq!(lines.len(), 1, "the SubRip file is read by the app, not by playbin");
    harness.message(Message::SidecarLines(generation, lines));

    pump(harness, |app| app.media.as_ref().is_some_and(|media| media.playback == Playback::Playing));
    let media = harness.app().media.as_ref().unwrap();
    assert_eq!(media.title, "Карпати.mkv");
    assert_eq!(media.chapters.len(), 2, "chapters come from the container");
    assert!(media.tracks.of(TrackKind::Text).next().is_some(), "the sidecar subtitle is a text track");
    assert!(media.suburi.as_deref().is_some_and(|uri| uri.ends_with(".uk.srt")));
    assert!((media.length().as_secs_f64() - 6.0).abs() < 0.3);

    harness.message(Message::TogglePlay);
    pump(harness, |app| app.media.as_ref().is_some_and(|media| media.playback == Playback::Paused));
    harness.message(Message::Seek(0.5));
    pump(harness, |app| app.media.as_ref().is_some_and(|media| (media.at().as_secs_f64() - 3.0).abs() < 0.6));
    assert_eq!(harness.app().media.as_ref().unwrap().chapter(), Some(1));

    let tracks = glide_clips::menus::tracks(harness.app());
    assert!(tracks.len() >= 5, "heading, Off, the subtitle, add and the delay stepper");
    let chapters = glide_clips::menus::chapters(harness.app());
    assert_eq!(chapters.len(), 3);
    assert_eq!(glide_clips::menus::speed(harness.app()).len(), 9);
    assert!(!glide_clips::menus::context(harness.app()).is_empty());
    assert!(!glide_clips::menus::more(harness.app()).is_empty());

    harness.message(Message::SetSpeed(1.5));
    assert!((harness.app().media.as_ref().unwrap().rate - 1.5).abs() < 1e-9);
    harness.message(Message::SetSpeed(1.0));

    harness.message(Message::EnterTrim);
    assert!(matches!(harness.app().mode, Mode::Trim { .. }));
    let _ = harness.view();
    harness.message(Message::TrimChanged(Trim::Start(0.25)));
    harness.message(Message::TrimChanged(Trim::End(0.75)));
    harness.message(Message::TrimApply);
    let media = harness.app().media.as_ref().unwrap();
    assert!(media.timeline.edited());
    assert!((media.timeline.total().as_secs_f64() - 3.0).abs() < 0.3, "{:?}", media.timeline.total());
    assert_eq!(harness.app().mode, Mode::Play);

    harness.message(Message::Seek(0.5));
    harness.message(Message::SplitClip);
    assert!(matches!(harness.app().mode, Mode::Clips { .. }));
    assert_eq!(harness.app().media.as_ref().unwrap().timeline.clips.len(), 2);
    let _ = harness.view();
    harness.message(Message::TurnClip(true));
    harness.message(Message::MoveClip(1, 0));
    assert_eq!(harness.app().media.as_ref().unwrap().timeline.clips.len(), 2);

    clips_from_another_file(harness, &root);
    harness.message(Message::ClipsDone);

    harness.message(Message::ExportOpen);
    assert!(matches!(harness.app().exporting, Some(glide_clips::app::Exporting::Form(_))));
    let _ = harness.view();
    harness.message(Message::ExportDismiss);

    harness.message(Message::Inspector(true));
    let rows = glide_clips::view::states::inspector_rows(harness.app());
    assert!(rows.iter().any(|(_, value)| value.contains("320 × 180")), "{rows:?}");
    let _ = harness.view();

    harness.message(Message::Inspector(false));
    assert_eq!(harness.app().wanted_aspect(), (320, 180), "the shell is asked to keep the video's shape");
    harness.message(Message::SetAspect(glide_clips::app::Aspect::Fill));
    assert_eq!(harness.app().wanted_aspect(), (0, 0), "filling the window frees the shape");
    harness.message(Message::SetAspect(glide_clips::app::Aspect::File));
    harness.app_mut().chrome.shown = true;
    let regions = harness.app().blur_regions();
    assert_eq!(regions.len(), 4, "the HUD and the three glass buttons of the top line: {regions:?}");
    assert_eq!((regions[0].x, regions[0].y, regions[0].width, regions[0].height), (320, 614, 640, 82));
    assert_eq!((regions[1].x, regions[1].y, regions[1].radius), (14, 18, 14));
    harness.app_mut().chrome.shown = false;
    assert!(harness.app().blur_regions().is_empty(), "hidden chrome needs no blur");
    harness.message(Message::Pip);
    assert!(harness.app().pip);
    assert!(harness.app().blur_regions().is_empty(), "picture in picture has no glass");
    let _ = harness.view();
    harness.message(Message::Command(glide_clips::keys::Command::Escape));
    assert!(!harness.app().pip, "Esc returns the picture to the window");
    harness.message(Message::ToggleOnTop);
    assert!(harness.app().media.as_ref().unwrap().on_top);
    harness.message(Message::ToggleOnTop);

    styled_script_next_to_the_video(harness, &root);
    closing_remembers(harness, &root);
}

fn clips_from_another_file(harness: &mut Harness<App>, root: &Path) {
    use glide_sdk::glide_app::drop_target::Dropped;
    let other = root.join("videos").join("Синевир.mkv");
    fixture(&other, 4);
    harness.message(Message::ClipProbed(other.clone(), None));
    assert_eq!(harness.app().media.as_ref().unwrap().timeline.clips.len(), 2, "an unreadable file adds nothing");
    harness.message(Message::ClipProbed(other, Some(Duration::from_secs(4))));
    let media = harness.app().media.as_ref().unwrap();
    assert_eq!(media.timeline.clips.len(), 3);
    assert_eq!(media.timeline.selected, 2);
    assert!((media.timeline.total().as_secs_f64() - 7.0).abs() < 0.3, "{:?}", media.timeline.total());
    let segments = media.timeline.segments(&media.uri);
    assert!(
        segments[2].uri.ends_with("%D0%A1%D0%B8%D0%BD%D0%B5%D0%B2%D0%B8%D1%80.mkv")
            || segments[2].uri.contains("Синевир")
    );
    assert_eq!(media.loaded, 1, "selecting the new clip loads its file");
    pump(harness, |app| app.media.as_ref().is_some_and(|media| media.handoff.is_none() && media.player.is_some()));
    let media = harness.app().media.as_ref().unwrap();
    assert!(media.failure.is_none(), "{:?}", media.failure);
    assert!(media.at() >= Duration::from_millis(2900), "the playhead sits at the third clip: {:?}", media.at());
    let _ = harness.view();
    harness.message(Message::SelectClip(0));
    assert_eq!(harness.app().media.as_ref().unwrap().loaded, 0, "the first clip goes back to the main file");
    pump(harness, |app| app.media.as_ref().is_some_and(|media| media.handoff.is_none()));
    let media = harness.app().media.as_ref().unwrap();
    let boundary = media.timeline.offset_of(2).as_secs_f64();
    let near = ((boundary - 0.5) / media.timeline.total().as_secs_f64()) as f32;
    harness.message(Message::Seek(near));
    assert_eq!(harness.app().media.as_ref().unwrap().loaded, 0);
    if !harness.app().media.as_ref().unwrap().playing() {
        harness.message(Message::TogglePlay);
    }
    pump(harness, |app| app.media.as_ref().is_some_and(|media| media.loaded == 1 && media.playing()));
    let media = harness.app().media.as_ref().unwrap();
    assert_eq!(media.clip, 2, "playback crossed into the clip from the other file");
    harness.message(Message::Dropped(Dropped::Hover(Some((10.0, 10.0)))));
    assert!(harness.app().dropping);
    let _ = harness.view();
    harness.message(Message::Dropped(Dropped::Hover(None)));
    assert!(!harness.app().dropping);
    harness.message(Message::Dropped(Dropped::Text("not a file".into(), (0.0, 0.0))));
    assert_eq!(harness.app().media.as_ref().unwrap().timeline.clips.len(), 3, "an unusable drop changes nothing");
    harness.message(Message::RemoveClip);
    let media = harness.app().media.as_ref().unwrap();
    assert_eq!(media.timeline.clips.len(), 2);
    assert_eq!(media.loaded, 0, "removing the playing clip from the other file returns to the main one");
}

fn styled_script_next_to_the_video(harness: &mut Harness<App>, root: &Path) {
    let styled = root.join("videos").join("Говерла.mkv");
    fixture(&styled, 4);
    let script = root.join("videos").join("Говерла.uk.ass");
    std::fs::write(
        &script,
        "[Script Info]\nScriptType: v4.00+\n\n[Events]\n\
         Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
         Dialogue: 0,0:00:00.00,0:00:03.50,Default,,0,0,0,,{\\an8}Туман, {\\i1}сходить{\\i0}\n",
    )
    .unwrap();
    harness.message(Message::OpenPath(styled));
    let media = harness.app().media.as_ref().unwrap();
    assert!(media.suburi.as_deref().is_some_and(|uri| uri.ends_with(".uk.ass")));
    let generation = media.generation;
    let lines = glide_clips::app::read_sidecar(media.suburi.as_deref().unwrap());
    assert_eq!(lines.len(), 1);
    harness.message(Message::SidecarLines(generation, lines));
    pump(harness, |app| app.media.as_ref().is_some_and(glide_clips::app::Media::playing));
    pump(harness, |app| {
        app.media.as_ref().is_some_and(|media| media.cue.as_ref().is_some_and(|(cue, _)| cue.text == "Туман, сходить"))
    });
    let media = harness.app().media.as_ref().unwrap();
    assert!(media.failure.is_none(), "an ASS file next to the video no longer breaks playback: {:?}", media.failure);
    assert_eq!(media.selected_of(TrackKind::Text), Some(glide_clips::app::SIDECAR));
    let tracks = glide_clips::menus::tracks(harness.app());
    let _ = harness.view();
    assert!(tracks.len() >= 5, "the script is listed with the subtitle tracks");
    harness.message(Message::PickText(None));
    assert!(harness.app().media.as_ref().unwrap().cue.is_none());
    harness.message(Message::Tick);
    assert!(harness.app().media.as_ref().unwrap().cue.is_none(), "switching the script off hides it");
}

fn closing_remembers(harness: &mut Harness<App>, root: &Path) {
    harness.message(Message::Close);
    assert!(harness.exiting());
    let saved = std::fs::read_to_string(root.join("state/glide/clips/files.toml"));
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut saved = saved.unwrap_or_default();
    while !saved.contains("Карпати.mkv") && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        saved = std::fs::read_to_string(root.join("state/glide/clips/files.toml")).unwrap_or_default();
    }
    assert!(saved.contains("Карпати.mkv"), "the position is remembered: {saved}");
}

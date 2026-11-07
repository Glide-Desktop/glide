mod common;

use std::time::{Duration, Instant};

use common::{fixture, scratch, uri};
use futures_util::StreamExt;
use glide_video::export::{self, Codec, Job, Progress, Quality, Segment, Transform, Turn};
use glide_video::frames::Grabber;
use glide_video::info;

fn finish(receiver: futures_channel::mpsc::UnboundedReceiver<Progress>) -> Vec<Progress> {
    futures_executor::block_on(receiver.collect::<Vec<_>>())
}

fn length(path: &std::path::Path) -> Duration {
    info::discover(&uri(path), Duration::from_secs(10)).unwrap().duration.unwrap()
}

#[test]
fn discovery_reads_streams_size_and_chapters() {
    let path = fixture("info.mkv", 3, &[(0, "Перший"), (1, "Другий")]);
    let found = info::discover(&uri(&path), Duration::from_secs(10)).unwrap();
    let video = found.video.as_ref().unwrap();
    assert_eq!((video.width, video.height), (320, 180));
    assert!((video.fps - 25.0).abs() < 0.01);
    assert_eq!(found.audio.len(), 1);
    assert_eq!(found.audio[0].channels, 2);
    assert_eq!(found.chapters.len(), 2);
    assert!(found.bytes.unwrap() > 0);
    assert!(found.bitrate().unwrap() > 0);
}

#[test]
fn grabber_returns_scaled_frames_for_each_time() {
    let path = fixture("frames.mkv", 3, &[]);
    let (grabber, results) = Grabber::spawn(&uri(&path), 160);
    grabber.request(7, vec![Duration::from_millis(500), Duration::from_secs(2)], false);
    let frames: Vec<_> = futures_executor::block_on(results.take(2).collect());
    drop(grabber);
    assert_eq!(frames.len(), 2);
    for frame in &frames {
        assert_eq!(frame.tag, 7);
        assert_eq!((frame.width, frame.height), (160, 90));
        assert_eq!(frame.rgba.len(), 160 * 90 * 4);
    }
}

#[test]
fn transcode_joins_rotated_segments() {
    let path = fixture("join.mkv", 4, &[]);
    let source = uri(&path);
    let output = scratch("joined.mp4");
    let _ = std::fs::remove_file(&output);
    let job = Job {
        segments: vec![
            Segment {
                uri: source.clone(),
                start: Duration::from_millis(500),
                end: Duration::from_millis(1500),
                transform: Transform::default(),
            },
            Segment {
                uri: source,
                start: Duration::from_secs(2),
                end: Duration::from_millis(3500),
                transform: Transform { turn: Turn::Right, ..Transform::default() },
            },
        ],
        output: output.clone(),
        quality: Quality::P720,
        codec: Codec::H264,
        lossless: false,
        source_width: 320,
        source_height: 180,
    };
    let started = Instant::now();
    let (_handle, receiver) = export::spawn(job);
    let progress = finish(receiver);
    assert!(matches!(progress.last(), Some(Progress::Done(done)) if *done == output), "{progress:?}");
    assert!(started.elapsed() < Duration::from_mins(1));
    let found = info::discover(&uri(&output), Duration::from_secs(10)).unwrap();
    let total = found.duration.unwrap().as_secs_f64();
    assert!((total - 2.5).abs() < 0.3, "{total}");
    let video = found.video.unwrap();
    assert_eq!((video.width, video.height), (320, 180));
    assert_eq!(found.audio.len(), 1);
    assert!(progress.iter().any(|step| matches!(step, Progress::Fraction(fraction) if *fraction > 0.0)));
}

#[test]
fn lossless_trim_snaps_to_keyframes() {
    for name in ["cut.mkv", "cut.mp4"] {
        let path = fixture(name, 4, &[]);
        let output = scratch(&format!("trimmed-{name}"));
        let _ = std::fs::remove_file(&output);
        let job = Job {
            segments: vec![Segment {
                uri: uri(&path),
                start: Duration::from_millis(1200),
                end: Duration::from_secs(3),
                transform: Transform::default(),
            }],
            output: output.clone(),
            quality: Quality::P1080,
            codec: Codec::H264,
            lossless: true,
            source_width: 320,
            source_height: 180,
        };
        let (_handle, receiver) = export::spawn(job);
        let progress = finish(receiver);
        assert!(matches!(progress.last(), Some(Progress::Done(_))), "{name}: {progress:?}");
        let seconds = length(&output).as_secs_f64();
        assert!((1.7..=2.3).contains(&seconds), "{name}: {seconds}");
    }
}

#[test]
fn audio_only_export_has_no_picture() {
    let path = fixture("sound.mkv", 2, &[]);
    let output = scratch("sound.m4a");
    let _ = std::fs::remove_file(&output);
    let job = Job {
        segments: vec![Segment {
            uri: uri(&path),
            start: Duration::ZERO,
            end: Duration::from_secs(2),
            transform: Transform::default(),
        }],
        output: output.clone(),
        quality: Quality::AudioOnly,
        codec: Codec::H264,
        lossless: false,
        source_width: 320,
        source_height: 180,
    };
    let (_handle, receiver) = export::spawn(job);
    let progress = finish(receiver);
    assert!(matches!(progress.last(), Some(Progress::Done(_))), "{progress:?}");
    let found = info::discover(&uri(&output), Duration::from_secs(10)).unwrap();
    assert!(found.video.is_none());
    assert_eq!(found.audio.len(), 1);
}

#[test]
fn cancelling_removes_the_partial_file() {
    let path = fixture("cancel.mkv", 4, &[]);
    let output = scratch("cancelled.mp4");
    let _ = std::fs::remove_file(&output);
    let job = Job {
        segments: vec![Segment {
            uri: uri(&path),
            start: Duration::ZERO,
            end: Duration::from_secs(4),
            transform: Transform::default(),
        }],
        output: output.clone(),
        quality: Quality::P1080,
        codec: Codec::H264,
        lossless: false,
        source_width: 320,
        source_height: 180,
    };
    let (handle, receiver) = export::spawn(job);
    handle.cancel();
    let progress = finish(receiver);
    assert_eq!(progress.last(), Some(&Progress::Cancelled));
    assert!(!output.exists());
}

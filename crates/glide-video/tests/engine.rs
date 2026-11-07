mod common;

use std::time::{Duration, Instant};

use common::{fixture, scratch, uri};
use glide_video::gstreamer as gst;
use glide_video::media::{Failure, TrackKind};
use glide_video::player::{Event, Options, Playback, Player};
use gst::prelude::*;

const WAIT: Duration = Duration::from_secs(10);

fn wait(player: &Player, mut matches: impl FnMut(&Event) -> bool) -> Vec<Event> {
    let deadline = Instant::now() + WAIT;
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        if let Some(event) = player.poll(Duration::from_millis(200)) {
            let done = matches(&event);
            seen.push(event);
            if done {
                return seen;
            }
        }
    }
    panic!("event not seen; got {seen:?}");
}

fn headless() -> Options {
    Options { volume: 0.0, muted: true, ..Options::default() }
}

#[test]
fn opens_paused_and_reports_tracks_and_duration() {
    let path = fixture("plain.mkv", 3, &[]);
    let player = Player::open(&uri(&path), &headless()).unwrap();
    let seen = wait(&player, |event| matches!(event, Event::State(Playback::Paused)));
    let tracks = seen.iter().find_map(|event| match event {
        Event::Tracks(tracks) => Some(tracks.clone()),
        _ => None,
    });
    let tracks = tracks.expect("a stream collection arrives before preroll");
    assert_eq!(tracks.of(TrackKind::Video).count(), 1);
    assert_eq!(tracks.of(TrackKind::Audio).count(), 1);
    let duration = player.duration().expect("duration after preroll");
    assert!((duration.as_secs_f64() - 3.0).abs() < 0.2, "{duration:?}");
}

#[test]
fn plays_seeks_and_reaches_the_end() {
    let path = fixture("seek.mkv", 3, &[]);
    let player = Player::open(&uri(&path), &headless()).unwrap();
    wait(&player, |event| matches!(event, Event::State(Playback::Paused)));
    player.seek(Duration::from_secs(2), true);
    let deadline = Instant::now() + WAIT;
    let mut at = None;
    while Instant::now() < deadline {
        let _ = player.poll(Duration::from_millis(50));
        at = player.position();
        if at.is_some_and(|at| (at.as_secs_f64() - 2.0).abs() < 0.1) {
            break;
        }
    }
    let at = at.expect("a position after the seek");
    assert!((at.as_secs_f64() - 2.0).abs() < 0.1, "{at:?}");
    player.play();
    wait(&player, |event| matches!(event, Event::Eos));
}

#[test]
fn speed_change_keeps_pitch_through_scaletempo() {
    let path = fixture("rate.mkv", 3, &[]);
    let mut player = Player::open(&uri(&path), &headless()).unwrap();
    wait(&player, |event| matches!(event, Event::State(Playback::Paused)));
    player.play();
    wait(&player, |event| matches!(event, Event::State(Playback::Playing)));
    player.set_rate(1.5);
    let bin = player.element().clone().downcast::<gst::Bin>().unwrap();
    let names: Vec<String> = bin
        .iterate_recurse()
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|element| element.factory().map(|factory| factory.name().to_string()))
        .collect();
    assert!(names.iter().any(|name| name == "scaletempo"), "{names:?}");
    assert!((player.rate() - 1.5).abs() < f64::EPSILON);
}

#[test]
fn chapters_come_from_the_container() {
    let path = fixture("chapters.mkv", 4, &[(0, "Світанок"), (2, "Туман")]);
    let player = Player::open(&uri(&path), &headless()).unwrap();
    let seen = wait(&player, |event| matches!(event, Event::Chapters(list) if !list.is_empty()));
    let Some(Event::Chapters(chapters)) = seen.last() else { unreachable!() };
    assert_eq!(chapters.len(), 2);
    assert_eq!(chapters[1].title, "Туман");
    assert_eq!(chapters[1].start, Duration::from_secs(2));
}

#[test]
fn sidecar_subtitles_arrive_as_cues() {
    let path = fixture("subs.mkv", 3, &[]);
    let srt = scratch("subs.srt");
    std::fs::write(&srt, "1\n00:00:00,200 --> 00:00:01,500\n<i>Дивись, туман</i>\n\n").unwrap();
    let options = Options { subtitle: Some(uri(&srt)), ..headless() };
    let player = Player::open(&uri(&path), &options).unwrap();
    wait(&player, |event| matches!(event, Event::State(Playback::Paused)));
    player.play();
    let seen = wait(&player, |event| matches!(event, Event::Subtitle(Some(_))));
    let Some(Event::Subtitle(Some(cue))) = seen.last() else { unreachable!() };
    assert_eq!(cue.text, "Дивись, туман");
    assert!(cue.duration.is_some_and(|duration| duration > Duration::from_secs(1)));
}

#[test]
fn a_missing_file_is_reported_as_not_found() {
    let player = Player::open(&uri(&scratch("absent.mkv")), &headless()).unwrap();
    let seen = wait(&player, |event| matches!(event, Event::Failed(_)));
    assert_eq!(seen.last(), Some(&Event::Failed(Failure::NotFound)));
}

#[test]
fn garbage_is_reported_as_broken_or_unknown() {
    let path = scratch("garbage.mp4");
    std::fs::write(&path, vec![0x5a; 4096]).unwrap();
    let player = Player::open(&uri(&path), &headless()).unwrap();
    let seen = wait(&player, |event| matches!(event, Event::Failed(_)));
    assert!(matches!(seen.last(), Some(Event::Failed(Failure::Broken(_) | Failure::Missing { .. }))), "{seen:?}");
}

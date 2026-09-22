use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use zbus::zvariant::{ObjectPath, OwnedValue, Value};

struct FakePlayer;

// The property signatures are fixed by the MPRIS interface: zbus requires
// `&self` receivers even where the fake has no state to read.
#[allow(clippy::unused_self)]
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl FakePlayer {
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let mut map = HashMap::new();
        map.insert(
            "mpris:trackid".to_string(),
            OwnedValue::try_from(Value::ObjectPath(
                ObjectPath::try_from("/org/glide/track/1").expect("a valid path"),
            ))
            .expect("value converts"),
        );
        map.insert(
            "xesam:title".to_string(),
            OwnedValue::try_from(Value::Str("Pisnya".into())).expect("value converts"),
        );
        map.insert(
            "xesam:artist".to_string(),
            OwnedValue::try_from(Value::Array(vec!["Hor".to_string()].into()))
                .expect("value converts"),
        );
        map.insert(
            "mpris:length".to_string(),
            OwnedValue::try_from(Value::I64(200_000_000)).expect("value converts"),
        );
        map
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        "Playing".to_string()
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        50_000_000
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }
}

fn session_bus_available() -> bool {
    zbus::blocking::Connection::session().is_ok()
}

fn wait_for_track(
    seen: &Arc<Mutex<Vec<glide_media::MediaEvent>>>,
    bus_name: &str,
    within: Duration,
) -> Option<glide_media::Track> {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        {
            let events = seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for event in events.iter() {
                if let glide_media::MediaEvent::Changed(found) = event
                    && found.bus == bus_name
                {
                    return Some((**found).clone());
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

#[test]
fn a_player_that_appears_after_we_started_is_still_found() {
    if !session_bus_available() {
        eprintln!("no session bus; skipping");
        return;
    }

    let seen: Arc<Mutex<Vec<glide_media::MediaEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink_seen = Arc::clone(&seen);
    let _handle = glide_media::start(Arc::new(move |event| {
        sink_seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(event);
    }))
    .expect("the media client starts");

    std::thread::sleep(Duration::from_millis(200));

    let bus_name = format!("org.mpris.MediaPlayer2.glidelate{}", std::process::id());
    let _player = zbus::blocking::connection::Builder::session()
        .expect("session bus")
        .name(bus_name.as_str())
        .expect("a free name")
        .serve_at("/org/mpris/MediaPlayer2", FakePlayer)
        .expect("serve")
        .build()
        .expect("player connection");

    let track = wait_for_track(&seen, &bus_name, Duration::from_secs(5));
    assert!(
        track.is_some(),
        "a player that starts after the compositor must still reach the bar"
    );
}

#[test]
fn a_player_on_the_bus_is_found_and_reported() {
    if !session_bus_available() {
        eprintln!("no session bus; skipping");
        return;
    }

    let bus_name = format!("org.mpris.MediaPlayer2.glidetest{}", std::process::id());
    let _player = zbus::blocking::connection::Builder::session()
        .expect("session bus")
        .name(bus_name.as_str())
        .expect("a free name")
        .serve_at("/org/mpris/MediaPlayer2", FakePlayer)
        .expect("serve")
        .build()
        .expect("player connection");

    let seen: Arc<Mutex<Vec<glide_media::MediaEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink_seen = Arc::clone(&seen);
    let _handle = glide_media::start(Arc::new(move |event| {
        sink_seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(event);
    }))
    .expect("the media client starts");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut track = None;
    while Instant::now() < deadline {
        {
            let events = seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for event in events.iter() {
                if let glide_media::MediaEvent::Changed(found) = event
                    && found.bus == bus_name
                {
                    track = Some(found.clone());
                }
            }
        }
        if track.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let track = track.expect("the fake player must be reported to the sink");
    assert_eq!(track.title, "Pisnya");
    assert_eq!(track.artist, "Hor");
    assert_eq!(track.status, glide_media::Playback::Playing);
    assert!(track.can_go_next);
    assert_eq!(track.length, Some(Duration::from_secs(200)));
    assert_eq!(track.position, Some(Duration::from_secs(50)));
    assert_eq!(track.progress(), Some(0.25));
    assert_eq!(track.track_id, "/org/glide/track/1");
}

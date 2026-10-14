use std::{
    collections::HashMap,
    sync::{
        Arc,
        mpsc::{self, RecvTimeoutError},
    },
    time::Duration,
};

use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use crate::art::ArtFetcher;
use crate::model::{
    Command, MediaEvent, OBJECT_PATH, PLAYER_INTERFACE, Playback, Track, artists_to_line, better_candidate,
    identity_from_bus, is_player_bus, seek_target,
};

pub type Sink = Arc<dyn Fn(MediaEvent) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Request {
    Command(Command),
    Seek(f32),
}

pub struct MediaHandle {
    requests: mpsc::Sender<Request>,
}

impl std::fmt::Debug for MediaHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("MediaHandle")
    }
}

impl MediaHandle {
    pub fn send(&self, command: Command) {
        self.request(Request::Command(command));
    }

    pub fn seek(&self, fraction: f32) {
        self.request(Request::Seek(fraction));
    }

    fn request(&self, request: Request) {
        if self.requests.send(request).is_err() {
            tracing::debug!("media worker has stopped; dropping {request:?}");
        }
    }
}

fn coalesce(requests: Vec<Request>) -> Vec<Request> {
    let last_seek = requests.iter().rposition(|request| matches!(request, Request::Seek(_)));
    requests
        .into_iter()
        .enumerate()
        .filter(|(index, request)| !matches!(request, Request::Seek(_)) || Some(*index) == last_seek)
        .map(|(_, request)| request)
        .collect()
}

fn seek_arguments(track: &Track, fraction: f32) -> Option<(ObjectPath<'static>, i64)> {
    if !fraction.is_finite() || track.track_id.is_empty() {
        return None;
    }
    let target = seek_target(track.length, fraction)?;
    let path = ObjectPath::try_from(track.track_id.clone()).ok()?;
    let micros = i64::try_from(target.as_micros()).ok()?;
    Some((path, micros))
}

fn player_call<'a>(bus: &'a str, method: &'a str) -> zbus::Result<zbus::message::Builder<'a>> {
    zbus::message::Message::method_call(OBJECT_PATH, method)?.destination(bus)?.interface(PLAYER_INTERFACE)
}

fn perform(connection: &zbus::blocking::Connection, current: Option<&Track>, request: Request) {
    let Some(track) = current else {
        return;
    };
    let bus = track.bus.as_str();
    let method = match request {
        Request::Command(command) => format!("{command:?}"),
        Request::Seek(_) => String::from("SetPosition"),
    };
    let message = match request {
        Request::Command(_) => player_call(bus, &method).and_then(|builder| builder.build(&())),
        Request::Seek(fraction) => {
            let Some(arguments) = seek_arguments(track, fraction) else {
                return;
            };
            player_call(bus, &method).and_then(|builder| builder.build(&arguments))
        }
    };
    match message {
        Ok(message) => {
            if let Err(err) = connection.send(&message) {
                tracing::debug!("failed to send {method}: {err}");
            }
        }
        Err(err) => tracing::debug!("failed to build {method}: {err}"),
    }
}

fn string_of(map: &HashMap<String, OwnedValue>, key: &str) -> String {
    map.get(key).and_then(|value| String::try_from(value.clone()).ok()).unwrap_or_default()
}

fn micros_of(map: &HashMap<String, OwnedValue>, key: &str) -> Option<Duration> {
    let micros = match &**map.get(key)? {
        Value::I64(inner) => u64::try_from(*inner).ok()?,
        Value::U64(inner) => *inner,
        Value::I32(inner) => u64::try_from(*inner).ok()?,
        Value::U32(inner) => u64::from(*inner),
        _ => return None,
    };
    Some(Duration::from_micros(micros))
}

fn artists_of(map: &HashMap<String, OwnedValue>) -> String {
    let Some(value) = map.get("xesam:artist") else {
        return String::new();
    };
    if let Ok(list) = Vec::<String>::try_from(value.clone()) {
        return artists_to_line(&list);
    }
    String::try_from(value.clone()).unwrap_or_default()
}

const ROOT_INTERFACE: &str = "org.mpris.MediaPlayer2";

type Properties = HashMap<String, OwnedValue>;

fn properties(connection: &zbus::blocking::Connection, bus: &str, interface: &str) -> Option<Properties> {
    connection
        .call_method(Some(bus), OBJECT_PATH, Some("org.freedesktop.DBus.Properties"), "GetAll", &(interface,))
        .ok()?
        .body()
        .deserialize()
        .ok()
}

fn read_track(connection: &zbus::blocking::Connection, bus: &str) -> Option<Track> {
    let player = properties(connection, bus, PLAYER_INTERFACE)?;
    let metadata = Properties::try_from(player.get("Metadata")?.try_clone().ok()?).ok()?;
    let flag = |key: &str| player.get(key).and_then(|value| bool::try_from(value).ok()).unwrap_or(false);
    Some(Track {
        bus: bus.to_string(),
        identity: identity_from_bus(bus),
        desktop_entry: properties(connection, bus, ROOT_INTERFACE)
            .map(|root| string_of(&root, "DesktopEntry"))
            .unwrap_or_default(),
        track_id: metadata
            .get("mpris:trackid")
            .and_then(|value| {
                ObjectPath::try_from(value.clone())
                    .map(|path| path.as_str().to_string())
                    .ok()
                    .or_else(|| String::try_from(value.clone()).ok())
            })
            .unwrap_or_default(),
        title: string_of(&metadata, "xesam:title"),
        artist: artists_of(&metadata),
        album: string_of(&metadata, "xesam:album"),
        art_url: string_of(&metadata, "mpris:artUrl"),
        length: micros_of(&metadata, "mpris:length"),
        position: micros_of(&player, "Position"),
        status: Playback::from_wire(&string_of(&player, "PlaybackStatus")),
        can_go_next: flag("CanGoNext"),
        can_go_previous: flag("CanGoPrevious"),
        can_pause: flag("CanPause"),
    })
}

fn list_players(connection: &zbus::blocking::Connection) -> Vec<String> {
    let names = zbus::blocking::fdo::DBusProxy::new(connection)
        .ok()
        .and_then(|proxy| proxy.list_names().ok())
        .unwrap_or_default();
    names.into_iter().filter(|name| is_player_bus(name.as_str())).map(|name| name.as_str().to_string()).collect()
}

fn choose(connection: &zbus::blocking::Connection) -> Option<Track> {
    list_players(connection)
        .iter()
        .filter_map(|bus| read_track(connection, bus))
        .filter(|track| track.status.is_active())
        .fold(None, |best, track| if better_candidate(best.as_ref(), &track) { Some(track) } else { best })
}

const SCAN_INTERVAL: Duration = Duration::from_millis(500);
const SETTLE_DELAY: Duration = Duration::from_millis(100);
const METHOD_TIMEOUT: Duration = Duration::from_secs(2);

pub fn start(sink: Sink) -> anyhow::Result<MediaHandle> {
    let connection = zbus::blocking::connection::Builder::session()?.method_timeout(METHOD_TIMEOUT).build()?;
    let (requests, inbox) = mpsc::channel();

    std::thread::Builder::new()
        .name("glide-mpris".to_string())
        .spawn(move || run_worker(&connection, &inbox, &sink))?;

    Ok(MediaHandle { requests })
}

fn run_worker(connection: &zbus::blocking::Connection, inbox: &mpsc::Receiver<Request>, sink: &Sink) {
    let mut last: Option<Track> = None;
    let mut wait = SCAN_INTERVAL;
    let mut art = ArtFetcher::start();
    publish(connection, &mut last, &mut art, sink);
    loop {
        let first = match inbox.recv_timeout(wait) {
            Ok(request) => Some(request),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let pending = coalesce(first.into_iter().chain(inbox.try_iter()).collect());
        for request in &pending {
            perform(connection, last.as_ref(), *request);
        }
        publish(connection, &mut last, &mut art, sink);
        wait = if pending.is_empty() { SCAN_INTERVAL } else { SETTLE_DELAY };
    }
}

fn publish(connection: &zbus::blocking::Connection, last: &mut Option<Track>, art: &mut ArtFetcher, sink: &Sink) {
    let track = choose(connection).map(|mut track| {
        art.localise(&mut track.art_url);
        track
    });

    match (track, last.as_ref()) {
        (None, None) => {}
        (None, Some(_)) => {
            *last = None;
            sink(MediaEvent::Gone);
        }
        (Some(track), Some(previous)) if previous.is_same_track(&track) && previous.status == track.status => {
            if previous.progress() != track.progress() {
                sink(MediaEvent::Progress(track.progress()));
                *last = Some(track);
            }
        }
        (Some(track), _) => {
            sink(MediaEvent::Changed(Box::new(track.clone())));
            *last = Some(track);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seekable_track() -> Track {
        Track {
            bus: "org.mpris.MediaPlayer2.test".to_string(),
            track_id: "/org/glide/track/1".to_string(),
            length: Some(Duration::from_secs(200)),
            ..Track::default()
        }
    }

    #[test]
    fn handle_calls_only_enqueue_requests_for_the_worker() {
        let (requests, inbox) = mpsc::channel();
        let handle = MediaHandle { requests };
        handle.send(Command::PlayPause);
        handle.seek(0.5);
        let received: Vec<Request> = inbox.try_iter().collect();
        assert_eq!(received, vec![Request::Command(Command::PlayPause), Request::Seek(0.5)]);
    }

    #[test]
    fn a_burst_of_seeks_collapses_to_the_last_one_and_commands_keep_their_order() {
        let pending = vec![
            Request::Seek(0.1),
            Request::Command(Command::Next),
            Request::Seek(0.2),
            Request::Command(Command::PlayPause),
            Request::Seek(0.3),
        ];
        assert_eq!(
            coalesce(pending),
            vec![Request::Command(Command::Next), Request::Command(Command::PlayPause), Request::Seek(0.3),]
        );
        assert!(coalesce(Vec::new()).is_empty());
    }

    #[test]
    fn a_seek_uses_the_cached_track_length_and_id() {
        let (path, micros) = seek_arguments(&seekable_track(), 0.25).expect("seekable");
        assert_eq!(path.as_str(), "/org/glide/track/1");
        assert_eq!(micros, 50_000_000);
    }

    #[test]
    fn a_seek_without_a_usable_track_is_dropped() {
        let no_id = Track { track_id: String::new(), ..seekable_track() };
        assert!(seek_arguments(&no_id, 0.5).is_none());

        let bad_id = Track { track_id: "not a path".to_string(), ..seekable_track() };
        assert!(seek_arguments(&bad_id, 0.5).is_none());

        let no_length = Track { length: None, ..seekable_track() };
        assert!(seek_arguments(&no_length, 0.5).is_none());

        assert!(seek_arguments(&seekable_track(), f32::NAN).is_none());
    }

    fn map(key: &str, value: Value<'_>) -> HashMap<String, OwnedValue> {
        HashMap::from([(key.to_string(), OwnedValue::try_from(value).expect("owned"))])
    }

    #[test]
    fn metadata_is_read_leniently() {
        let length = |value| micros_of(&map("mpris:length", value), "mpris:length");
        assert_eq!(length(Value::I64(2_000_000)), Some(Duration::from_secs(2)));
        assert_eq!(length(Value::U32(3_000_000)), Some(Duration::from_secs(3)));
        assert_eq!(length(Value::I64(-5)), None, "negative never wraps");
        assert_eq!(artists_of(&map("xesam:artist", Value::Str("Solo".into()))), "Solo");
        assert_eq!(string_of(&map("xesam:title", Value::I32(7)), "xesam:title"), "");
        assert_eq!(artists_of(&HashMap::new()), "");
    }
}

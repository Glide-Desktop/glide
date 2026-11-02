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
    identity_from_bus, identity_or_bus, is_mirror_bus, is_player_bus, is_relay_bus, seek_target,
};

pub type Sink = Arc<dyn Fn(MediaEvent) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Request {
    Command(Command),
    Seek(f32),
    SeekBy(i64),
    PauseAll,
}

#[derive(Debug)]
pub struct MediaHandle {
    requests: mpsc::Sender<Request>,
}

impl MediaHandle {
    pub fn send(&self, command: Command) {
        self.request(Request::Command(command));
    }

    pub fn seek(&self, fraction: f32) {
        self.request(Request::Seek(fraction));
    }

    pub fn seek_by(&self, offset: Duration, forward: bool) {
        let micros = i64::try_from(offset.as_micros()).unwrap_or(i64::MAX);
        self.request(Request::SeekBy(if forward { micros } else { -micros }));
    }

    pub fn pause_all(&self) {
        self.request(Request::PauseAll);
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

fn deliver(connection: &zbus::blocking::Connection, method: &str, message: zbus::Result<zbus::message::Message>) {
    match message {
        Ok(message) => {
            if let Err(err) = connection.send(&message) {
                tracing::debug!("failed to send {method}: {err}");
            }
        }
        Err(err) => tracing::debug!("failed to build {method}: {err}"),
    }
}

fn playing(players: impl IntoIterator<Item = (String, Playback)>) -> Vec<String> {
    players.into_iter().filter(|(_, status)| *status == Playback::Playing).map(|(bus, _)| bus).collect()
}

fn pausable(players: Vec<String>) -> Vec<String> {
    players.into_iter().filter(|bus| !is_relay_bus(bus)).collect()
}

fn playback_status(connection: &zbus::blocking::Connection, bus: &str) -> Playback {
    connection
        .call_method(
            Some(bus),
            OBJECT_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(PLAYER_INTERFACE, "PlaybackStatus"),
        )
        .ok()
        .and_then(|reply| reply.body().deserialize::<OwnedValue>().ok())
        .and_then(|value| String::try_from(value).ok())
        .map_or(Playback::Stopped, |status| Playback::from_wire(&status))
}

fn pause_all(connection: &zbus::blocking::Connection) {
    let statuses = pausable(list_players(connection)).into_iter().map(|bus| {
        let status = playback_status(connection, &bus);
        (bus, status)
    });
    for bus in playing(statuses) {
        deliver(connection, "Pause", player_call(&bus, "Pause").and_then(|builder| builder.build(&())));
    }
}

fn command_bus<'a>(current: Option<&'a Track>, remembered: Option<&'a str>, request: Request) -> Option<&'a str> {
    if let Some(track) = current.filter(|track| track.can_control) {
        return match request {
            Request::Seek(_) | Request::SeekBy(_) if !track.can_seek => None,
            _ => Some(track.bus.as_str()),
        };
    }
    match request {
        Request::Command(Command::Play | Command::PlayPause) => remembered,
        _ => None,
    }
}

fn perform(
    connection: &zbus::blocking::Connection, current: Option<&Track>, remembered: Option<&str>, request: Request,
) {
    if request == Request::PauseAll {
        pause_all(connection);
        return;
    }
    let Some(bus) = command_bus(current, remembered, request) else {
        tracing::debug!(?request, "no player takes this request");
        return;
    };
    match request {
        Request::PauseAll => {}
        Request::Command(command) => {
            let method = format!("{command:?}");
            deliver(connection, &method, player_call(bus, &method).and_then(|builder| builder.build(&())));
        }
        Request::Seek(fraction) => {
            let Some(arguments) = current.and_then(|track| seek_arguments(track, fraction)) else {
                return;
            };
            deliver(
                connection,
                "SetPosition",
                player_call(bus, "SetPosition").and_then(|builder| builder.build(&arguments)),
            );
        }
        Request::SeekBy(offset) => {
            deliver(connection, "Seek", player_call(bus, "Seek").and_then(|builder| builder.build(&(offset,))));
        }
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

enum Reading {
    Track(Box<Track>),
    Empty,
    Unreachable,
}

fn read_track(connection: &zbus::blocking::Connection, bus: &str) -> Reading {
    let Some(player) = properties(connection, bus, PLAYER_INTERFACE) else {
        return Reading::Unreachable;
    };
    track_from(bus, &player).map_or(Reading::Empty, |track| Reading::Track(Box::new(track)))
}

fn track_from(bus: &str, player: &Properties) -> Option<Track> {
    let metadata = Properties::try_from(player.get("Metadata")?.try_clone().ok()?).ok()?;
    let flag = |key: &str| player.get(key).and_then(|value| bool::try_from(value).ok()).unwrap_or(false);
    Some(Track {
        bus: bus.to_string(),
        identity: identity_from_bus(bus),
        desktop_entry: String::new(),
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
        position: micros_of(player, "Position"),
        status: Playback::from_wire(&string_of(player, "PlaybackStatus")),
        can_go_next: flag("CanGoNext"),
        can_go_previous: flag("CanGoPrevious"),
        can_seek: flag("CanSeek"),
        can_control: player.get("CanControl").and_then(|value| bool::try_from(value).ok()).unwrap_or(true),
    })
}

fn describe(connection: &zbus::blocking::Connection, track: &mut Track, last: Option<&Track>) {
    if let Some(last) = last.filter(|last| last.bus == track.bus && !last.desktop_entry.is_empty()) {
        track.identity.clone_from(&last.identity);
        track.desktop_entry.clone_from(&last.desktop_entry);
        return;
    }
    if let Some(root) = properties(connection, &track.bus, ROOT_INTERFACE) {
        track.identity = identity_or_bus(&string_of(&root, "Identity"), &track.bus);
        track.desktop_entry = string_of(&root, "DesktopEntry");
    }
}

fn list_players(connection: &zbus::blocking::Connection) -> Vec<String> {
    let names = zbus::blocking::fdo::DBusProxy::new(connection)
        .ok()
        .and_then(|proxy| proxy.list_names().ok())
        .unwrap_or_default();
    names.into_iter().filter(|name| is_player_bus(name.as_str())).map(|name| name.as_str().to_string()).collect()
}

fn select_track(tracks: impl IntoIterator<Item = Track>, preferred: Option<&str>) -> Option<Track> {
    tracks.into_iter().filter(|track| track.status.is_active()).fold(None, |best, track| {
        let keep_current = preferred == Some(track.bus.as_str())
            && best.as_ref().is_some_and(|best: &Track| best.status == track.status);
        if keep_current || better_candidate(best.as_ref(), &track) { Some(track) } else { best }
    })
}

fn choose(connection: &zbus::blocking::Connection, last: Option<&Track>) -> (Option<Track>, Vec<String>) {
    let listed = list_players(connection);
    let tracks: Vec<Track> = listed
        .iter()
        .filter(|bus| !is_mirror_bus(bus))
        .filter_map(|bus| match read_track(connection, bus) {
            Reading::Track(track) => Some(*track),
            Reading::Unreachable => last.filter(|last| last.bus == *bus).cloned(),
            Reading::Empty => None,
        })
        .collect();
    (select_track(tracks, last.map(|track| track.bus.as_str())), listed)
}

fn remembered_bus<'a>(remembered: Option<&'a str>, listed: &[String]) -> Option<&'a str> {
    remembered.filter(|bus| listed.iter().any(|name| name == bus))
}

const SCAN_INTERVAL: Duration = Duration::from_millis(500);
const SETTLE_DELAY: Duration = Duration::from_millis(100);
const METHOD_TIMEOUT: Duration = Duration::from_millis(300);

pub fn start(sink: Sink) -> anyhow::Result<MediaHandle> {
    let connection = zbus::blocking::connection::Builder::session()?.method_timeout(METHOD_TIMEOUT).build()?;
    let (requests, inbox) = mpsc::channel();

    std::thread::Builder::new()
        .name("glide-mpris".to_string())
        .spawn(move || Worker::new(&connection, &sink).run(&inbox))?;

    Ok(MediaHandle { requests })
}

struct Worker<'a> {
    connection: &'a zbus::blocking::Connection,
    sink: &'a Sink,
    art: ArtFetcher,
    last: Option<Track>,
    remembered: Option<String>,
    listed: Vec<String>,
}

impl<'a> Worker<'a> {
    fn new(connection: &'a zbus::blocking::Connection, sink: &'a Sink) -> Self {
        Self { connection, sink, art: ArtFetcher::start(), last: None, remembered: None, listed: Vec::new() }
    }

    fn run(mut self, inbox: &mpsc::Receiver<Request>) {
        let mut wait = SCAN_INTERVAL;
        self.publish();
        loop {
            let first = match inbox.recv_timeout(wait) {
                Ok(request) => Some(request),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            };
            let pending = coalesce(first.into_iter().chain(inbox.try_iter()).collect());
            for request in &pending {
                let remembered = remembered_bus(self.remembered.as_deref(), &self.listed);
                perform(self.connection, self.last.as_ref(), remembered, *request);
            }
            self.publish();
            wait = if pending.is_empty() { SCAN_INTERVAL } else { SETTLE_DELAY };
        }
    }

    fn publish(&mut self) {
        let (track, listed) = choose(self.connection, self.last.as_ref());
        let track = track.map(|mut track| {
            describe(self.connection, &mut track, self.last.as_ref());
            self.art.localise(&mut track.art_url);
            track
        });
        if let Some(track) = track.as_ref().filter(|track| track.can_control) {
            self.remembered = Some(track.bus.clone());
        }
        if let Some(event) = update_for(self.last.as_ref(), track.as_ref()) {
            (self.sink)(event);
        }
        self.last = track;
        self.listed = listed;
    }
}

fn update_for(previous: Option<&Track>, track: Option<&Track>) -> Option<MediaEvent> {
    match (previous, track) {
        (None, None) => None,
        (Some(_), None) => Some(MediaEvent::Gone),
        (Some(previous), Some(track)) if *previous == Track { position: previous.position, ..track.clone() } => {
            (previous.progress() != track.progress()).then(|| MediaEvent::Progress(track.progress()))
        }
        (_, Some(track)) => Some(MediaEvent::Changed(Box::new(track.clone()))),
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
    fn selection_keeps_the_current_player_when_bus_order_changes() {
        let current = Track { bus: "current".into(), status: Playback::Playing, ..Track::default() };
        let other = Track { bus: "other".into(), ..current.clone() };
        for tracks in [[current.clone(), other.clone()], [other.clone(), current.clone()]] {
            assert_eq!(select_track(tracks, Some("current")), Some(current.clone()));
        }
        let paused = Track { status: Playback::Paused, ..current };
        assert_eq!(select_track([other.clone(), paused.clone()], Some("current")), Some(other.clone()));
        assert_eq!(select_track([paused, other.clone()], Some("current")), Some(other.clone()));
        assert_eq!(select_track([other.clone()], Some("current")), Some(other));
    }

    #[test]
    fn relative_seeks_carry_a_signed_offset_and_pausing_everything_is_one_request() {
        let (requests, inbox) = mpsc::channel();
        let handle = MediaHandle { requests };
        handle.seek_by(Duration::from_secs(10), true);
        handle.seek_by(Duration::from_secs(10), false);
        handle.pause_all();
        let received: Vec<Request> = inbox.try_iter().collect();
        assert_eq!(received, vec![Request::SeekBy(10_000_000), Request::SeekBy(-10_000_000), Request::PauseAll]);
        assert_eq!(coalesce(received.clone()), received, "relative seeks add up, so none is dropped");
    }

    #[test]
    fn pausing_everything_reaches_every_playing_player_and_nothing_else() {
        let players = [
            ("org.mpris.MediaPlayer2.spotify".to_string(), Playback::Playing),
            ("org.mpris.MediaPlayer2.firefox".to_string(), Playback::Paused),
            ("org.mpris.MediaPlayer2.chromium".to_string(), Playback::Playing),
            ("org.mpris.MediaPlayer2.vlc".to_string(), Playback::Stopped),
        ];
        assert_eq!(playing(players), ["org.mpris.MediaPlayer2.spotify", "org.mpris.MediaPlayer2.chromium"]);
    }

    #[test]
    fn unplugging_headphones_pauses_only_local_players() {
        let names = [
            "org.mpris.MediaPlayer2.spotify",
            "org.mpris.MediaPlayer2.playerctld",
            "org.mpris.MediaPlayer2.kdeconnect.mpris_000001",
            "org.mpris.MediaPlayer2.plasma-browser-integration",
            "org.mpris.MediaPlayer2.firefox.instance_1_23",
        ];
        assert_eq!(
            pausable(names.iter().map(ToString::to_string).collect()),
            ["org.mpris.MediaPlayer2.spotify", "org.mpris.MediaPlayer2.firefox.instance_1_23"]
        );
    }

    #[test]
    fn play_still_reaches_a_stopped_player_while_it_is_on_the_bus() {
        let listed = vec!["org.mpris.MediaPlayer2.vlc".to_string()];
        let remembered = remembered_bus(Some("org.mpris.MediaPlayer2.vlc"), &listed);
        assert_eq!(remembered, Some("org.mpris.MediaPlayer2.vlc"));
        assert_eq!(remembered_bus(Some("org.mpris.MediaPlayer2.gone"), &listed), None);
        for command in [Command::Play, Command::PlayPause] {
            assert_eq!(command_bus(None, remembered, Request::Command(command)), remembered, "{command:?}");
        }
        for request in [Request::Command(Command::Next), Request::Command(Command::Stop), Request::Seek(0.5)] {
            assert_eq!(command_bus(None, remembered, request), None, "{request:?}");
        }
    }

    #[test]
    fn a_player_that_cannot_be_controlled_or_sought_is_not_sent_those_requests() {
        let able = Track { can_control: true, can_seek: true, ..seekable_track() };
        let bus = Some(able.bus.as_str());
        assert_eq!(command_bus(Some(&able), None, Request::Seek(0.5)), bus);
        assert_eq!(command_bus(Some(&able), None, Request::SeekBy(5)), bus);
        let fixed = Track { can_seek: false, ..able.clone() };
        assert_eq!(command_bus(Some(&fixed), None, Request::Seek(0.5)), None);
        assert_eq!(command_bus(Some(&fixed), None, Request::SeekBy(5)), None);
        assert_eq!(command_bus(Some(&fixed), None, Request::Command(Command::Next)), bus);
        let blind = Track { can_control: false, ..able };
        assert_eq!(command_bus(Some(&blind), None, Request::Command(Command::Next)), None);
        assert_eq!(command_bus(Some(&blind), Some("other"), Request::Command(Command::PlayPause)), Some("other"));
    }

    fn empty_metadata() -> OwnedValue {
        OwnedValue::try_from(Value::from(HashMap::<String, Value<'static>>::new())).expect("owned")
    }

    #[test]
    fn capabilities_are_read_from_the_player_properties() {
        let player = HashMap::from([
            ("Metadata".to_string(), empty_metadata()),
            ("CanSeek".to_string(), OwnedValue::from(true)),
            ("CanControl".to_string(), OwnedValue::from(false)),
        ]);
        let track = track_from("org.mpris.MediaPlayer2.io.bassi.Amberol", &player).expect("track");
        assert!(track.can_seek && !track.can_control);
        assert_eq!(track.identity, "Amberol");
        let lenient = HashMap::from([("Metadata".to_string(), empty_metadata())]);
        let track = track_from("org.mpris.MediaPlayer2.vlc", &lenient).expect("track");
        assert!(track.can_control && !track.can_seek, "a player that omits CanControl is still controlled");
    }

    #[test]
    fn mpris_method_names_are_the_command_names() {
        let names: Vec<String> =
            [Command::PlayPause, Command::Play, Command::Pause, Command::Stop, Command::Next, Command::Previous]
                .iter()
                .map(|command| format!("{command:?}"))
                .collect();
        assert_eq!(names, ["PlayPause", "Play", "Pause", "Stop", "Next", "Previous"]);
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

    #[test]
    fn only_a_moved_position_is_a_progress_update_and_any_other_field_is_a_change() {
        let first = Track { position: Some(Duration::from_secs(20)), ..seekable_track() };
        assert_eq!(update_for(None, None), None);
        assert_eq!(update_for(Some(&first), None), Some(MediaEvent::Gone));
        assert_eq!(update_for(None, Some(&first)), Some(MediaEvent::Changed(Box::new(first.clone()))));
        assert_eq!(update_for(Some(&first), Some(&first)), None);

        let moved = Track { position: Some(Duration::from_secs(50)), ..first.clone() };
        assert_eq!(update_for(Some(&first), Some(&moved)), Some(MediaEvent::Progress(Some(0.25))));

        let changes = [
            Track { length: Some(Duration::from_mins(5)), ..first.clone() },
            Track { track_id: "/org/glide/track/2".to_string(), ..first.clone() },
            Track { album: "Other".to_string(), ..first.clone() },
            Track { can_go_next: !first.can_go_next, ..first.clone() },
            Track { can_go_previous: !first.can_go_previous, ..first.clone() },
            Track { can_seek: !first.can_seek, ..first.clone() },
            Track { desktop_entry: "other".to_string(), ..first.clone() },
            Track { identity: "Other".to_string(), ..first.clone() },
            Track { status: Playback::Paused, ..first.clone() },
        ];
        for changed in changes {
            assert_eq!(
                update_for(Some(&first), Some(&changed)),
                Some(MediaEvent::Changed(Box::new(changed.clone()))),
                "{changed:?}"
            );
        }
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

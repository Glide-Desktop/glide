use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::Duration;

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;
use zbus::fdo::RequestNameFlags;
use zbus::names::BusName;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use crate::player::Playback;

const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    PlayPause,
    Play,
    Pause,
    Stop,
    Next,
    Previous,
    SeekBy(i64),
    SetPosition(Duration),
    Raise,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct State {
    pub status: Option<Playback>,
    pub track: u64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art: Option<String>,
    pub length: Option<Duration>,
    pub position: Duration,
    pub rate: f64,
    pub volume: f64,
    pub chapters: bool,
}

fn status(state: &State) -> &'static str {
    match state.status {
        Some(Playback::Playing) => "Playing",
        Some(Playback::Paused) => "Paused",
        _ => "Stopped",
    }
}

fn shown_rate(rate: f64) -> f64 {
    if rate.is_finite() && rate != 0.0 { rate } else { 1.0 }
}

fn owned(value: Value<'_>) -> Option<OwnedValue> {
    OwnedValue::try_from(value).ok()
}

fn metadata(state: &State) -> HashMap<String, OwnedValue> {
    let mut map = HashMap::new();
    if state.status.is_none() {
        return map;
    }
    let track = format!("/org/glide/Clips/track/{}", state.track);
    if let Ok(path) = ObjectPath::try_from(track)
        && let Some(value) = owned(Value::ObjectPath(path))
    {
        map.insert("mpris:trackid".into(), value);
    }
    let mut put = |key: &str, value: Value<'_>| {
        if let Some(value) = owned(value) {
            map.insert(key.into(), value);
        }
    };
    put("xesam:title", Value::from(state.title.clone()));
    if !state.artist.is_empty() {
        put("xesam:artist", Value::from(vec![state.artist.clone()]));
    }
    if !state.album.is_empty() {
        put("xesam:album", Value::from(state.album.clone()));
    }
    if let Some(art) = &state.art {
        put("mpris:artUrl", Value::from(art.clone()));
    }
    if let Some(length) = state.length {
        put("mpris:length", Value::I64(length.as_micros() as i64));
    }
    map
}

#[must_use]
pub fn changes(before: &State, after: &State) -> Vec<(&'static str, Value<'static>)> {
    let mut changed: Vec<(&'static str, Value<'static>)> = Vec::new();
    if status(before) != status(after) {
        changed.push(("PlaybackStatus", Value::from(status(after))));
    }
    let meta = (
        &before.track,
        &before.title,
        &before.artist,
        &before.album,
        &before.art,
        before.length,
        before.status.is_some(),
    );
    let now =
        (&after.track, &after.title, &after.artist, &after.album, &after.art, after.length, after.status.is_some());
    if meta != now {
        let map: HashMap<String, Value<'static>> =
            metadata(after).into_iter().map(|(key, value)| (key, Value::from(value))).collect();
        changed.push(("Metadata", Value::from(map)));
    }
    if (shown_rate(before.rate) - shown_rate(after.rate)).abs() > f64::EPSILON {
        changed.push(("Rate", Value::F64(shown_rate(after.rate))));
    }
    if (before.volume - after.volume).abs() > f64::EPSILON {
        changed.push(("Volume", Value::F64(after.volume)));
    }
    if before.chapters != after.chapters {
        changed.push(("CanGoNext", Value::Bool(after.chapters)));
        changed.push(("CanGoPrevious", Value::Bool(after.chapters)));
    }
    let loaded = after.status.is_some();
    if before.status.is_some() != loaded {
        changed.push(("CanPlay", Value::Bool(loaded)));
        changed.push(("CanPause", Value::Bool(loaded)));
    }
    let seekable = after.length.is_some();
    if before.length.is_some() != seekable {
        changed.push(("CanSeek", Value::Bool(seekable)));
    }
    changed
}

struct Root {
    identity: String,
    desktop: String,
    commands: UnboundedSender<Command>,
}

#[allow(clippy::unused_self)] // MPRIS fixes these as properties of the object; zbus needs a receiver for each
#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {
        let _ = self.commands.unbounded_send(Command::Raise);
    }

    fn quit(&self) {
        let _ = self.commands.unbounded_send(Command::Quit);
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> String {
        self.identity.clone()
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        self.desktop.clone()
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        ["file", "http", "https", "rtsp"].map(String::from).to_vec()
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Player {
    state: Arc<Mutex<State>>,
    commands: UnboundedSender<Command>,
}

impl Player {
    fn read(&self) -> State {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn send(&self, command: Command) {
        let _ = self.commands.unbounded_send(command);
    }
}

#[allow(clippy::unused_self, clippy::needless_pass_by_value)] // Signatures are fixed by the MPRIS interface that zbus generates
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        self.send(Command::Next);
    }

    fn previous(&self) {
        self.send(Command::Previous);
    }

    fn pause(&self) {
        self.send(Command::Pause);
    }

    fn play_pause(&self) {
        self.send(Command::PlayPause);
    }

    fn stop(&self) {
        self.send(Command::Stop);
    }

    fn play(&self) {
        self.send(Command::Play);
    }

    fn seek(&self, offset: i64) {
        self.send(Command::SeekBy(offset));
    }

    fn set_position(&self, track: ObjectPath<'_>, position: i64) {
        let current = format!("/org/glide/Clips/track/{}", self.read().track);
        if track.as_str() == current && position >= 0 {
            self.send(Command::SetPosition(Duration::from_micros(position as u64)));
        }
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        status(&self.read()).into()
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        shown_rate(self.read().rate)
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        metadata(&self.read())
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.read().volume
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        self.read().position.as_micros() as i64
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        0.5
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        2.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.read().chapters
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.read().chapters
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.read().status.is_some()
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.read().status.is_some()
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.read().length.is_some()
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

enum Note {
    State(State),
    Seeked(Duration),
}

pub struct Server {
    state: Arc<Mutex<State>>,
    last: State,
    notes: mpsc::Sender<Note>,
}

impl Server {
    pub fn start(identity: &str, desktop: &str) -> Result<(Self, UnboundedReceiver<Command>), String> {
        Self::start_on(Builder::session().map_err(|err| err.to_string())?, identity, desktop)
    }

    pub fn start_on(
        builder: Builder<'_>, identity: &str, desktop: &str,
    ) -> Result<(Self, UnboundedReceiver<Command>), String> {
        let (commands, receiver) = unbounded();
        let state = Arc::new(Mutex::new(State::default()));
        let root = Root { identity: identity.into(), desktop: desktop.into(), commands: commands.clone() };
        let player = Player { state: state.clone(), commands };
        let connection = builder
            .serve_at(PATH, root)
            .and_then(|builder| builder.serve_at(PATH, player))
            .and_then(Builder::build)
            .map_err(|err| err.to_string())?;
        let name = format!("org.mpris.MediaPlayer2.{}.instance{}", desktop.replace('-', "_"), std::process::id());
        connection
            .request_name_with_flags(name.as_str(), RequestNameFlags::DoNotQueue.into())
            .map_err(|err| err.to_string())?;
        let (notes, inbox) = mpsc::channel();
        let shared = state.clone();
        std::thread::Builder::new()
            .name("glide-video-mpris".into())
            .spawn(move || announce(&connection, &shared, &inbox))
            .map_err(|err| err.to_string())?;
        Ok((Self { state, last: State::default(), notes }, receiver))
    }

    pub fn update(&mut self, state: State) {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).position = state.position;
        let mut compare = state.clone();
        compare.position = self.last.position;
        if compare == self.last {
            return;
        }
        self.last = state.clone();
        let _ = self.notes.send(Note::State(state));
    }

    pub fn seeked(&self, position: Duration) {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).position = position;
        let _ = self.notes.send(Note::Seeked(position));
    }
}

fn announce(connection: &Connection, shared: &Mutex<State>, inbox: &mpsc::Receiver<Note>) {
    for note in inbox {
        match note {
            Note::State(state) => {
                let before =
                    std::mem::replace(&mut *shared.lock().unwrap_or_else(PoisonError::into_inner), state.clone());
                let properties: HashMap<&str, Value<'_>> = changes(&before, &state).into_iter().collect();
                if properties.is_empty() {
                    continue;
                }
                let body = (PLAYER, properties, Vec::<&str>::new());
                if let Err(err) = connection.emit_signal(
                    None::<BusName<'_>>,
                    PATH,
                    "org.freedesktop.DBus.Properties",
                    "PropertiesChanged",
                    &body,
                ) {
                    tracing::debug!(?err, "could not announce a player change");
                }
            }
            Note::Seeked(position) => {
                let body = (position.as_micros() as i64,);
                if let Err(err) = connection.emit_signal(None::<BusName<'_>>, PATH, PLAYER, "Seeked", &body) {
                    tracing::debug!(?err, "could not announce a seek");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing() -> State {
        State {
            status: Some(Playback::Playing),
            track: 1,
            title: "Карпати з висоти".into(),
            artist: "Розділ 3 · Чорногірський хребет".into(),
            length: Some(Duration::from_mins(90)),
            rate: 1.0,
            volume: 0.7,
            chapters: true,
            ..State::default()
        }
    }

    #[test]
    fn the_server_answers_mpris_calls_on_a_private_bus() {
        use std::io::BufRead;
        let Ok(mut daemon) = std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            return;
        };
        let mut address = String::new();
        if let Some(out) = daemon.stdout.take() {
            let _ = std::io::BufReader::new(out).read_line(&mut address);
        }
        let address = address.trim().to_string();
        let builder = Builder::address(address.as_str()).unwrap();
        let (mut server, mut commands) = Server::start_on(builder, "Clips", "glide-clips").unwrap();
        server.update(playing());
        let client = Builder::address(address.as_str()).unwrap().build().unwrap();
        let name = format!("org.mpris.MediaPlayer2.glide_clips.instance{}", std::process::id());
        let proxy = zbus::blocking::Proxy::new(&client, name.as_str(), PATH, PLAYER).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut status = String::new();
        while std::time::Instant::now() < deadline && status != "Playing" {
            status = proxy.get_property::<String>("PlaybackStatus").unwrap_or_default();
            std::thread::sleep(Duration::from_millis(20));
        }
        let _: () = proxy.call("PlayPause", &()).unwrap();
        let _: () = proxy.call("Seek", &(-10_000_000_i64,)).unwrap();
        let first = futures_executor::block_on(futures_util::StreamExt::next(&mut commands));
        let second = futures_executor::block_on(futures_util::StreamExt::next(&mut commands));
        let length: i64 = proxy
            .get_property::<HashMap<String, OwnedValue>>("Metadata")
            .ok()
            .and_then(|map| map.get("mpris:length").cloned())
            .and_then(|value| i64::try_from(value).ok())
            .unwrap_or_default();
        let _ = daemon.kill();
        assert_eq!(status, "Playing");
        assert_eq!(first, Some(Command::PlayPause));
        assert_eq!(second, Some(Command::SeekBy(-10_000_000)));
        assert_eq!(length, 5_400_000_000);
    }

    #[test]
    fn a_moved_position_alone_announces_nothing() {
        let before = playing();
        let after = State { position: Duration::from_secs(5), ..playing() };
        assert!(changes(&before, &after).is_empty());
    }

    #[test]
    fn pausing_announces_only_the_status() {
        let after = State { status: Some(Playback::Paused), ..playing() };
        let names: Vec<_> = changes(&playing(), &after).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, vec!["PlaybackStatus"]);
    }

    #[test]
    fn opening_a_file_tells_clients_that_cache_properties_what_they_may_now_do() {
        let names: Vec<_> = changes(&State::default(), &playing()).into_iter().map(|(name, _)| name).collect();
        for expected in ["PlaybackStatus", "Metadata", "CanPlay", "CanPause", "CanSeek", "CanGoNext"] {
            assert!(names.contains(&expected), "{expected} in {names:?}");
        }
        let live = State { length: None, ..playing() };
        let names: Vec<_> = changes(&playing(), &live).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, vec!["Metadata", "CanSeek"]);
    }

    #[test]
    fn a_new_chapter_rewrites_the_metadata() {
        let after = State { artist: "Розділ 4 · Озеро Несамовите".into(), ..playing() };
        let names: Vec<_> = changes(&playing(), &after).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, vec!["Metadata"]);
    }

    #[test]
    fn metadata_carries_length_in_microseconds_and_the_title() {
        let map = metadata(&playing());
        assert_eq!(i64::try_from(map["mpris:length"].clone()).unwrap(), 5_400_000_000);
        assert_eq!(String::try_from(map["xesam:title"].clone()).unwrap(), "Карпати з висоти");
        assert!(metadata(&State::default()).is_empty());
    }
}

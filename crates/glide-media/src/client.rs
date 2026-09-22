use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use zbus::zvariant::{OwnedValue, Value};

use crate::model::{
    Command, MediaEvent, OBJECT_PATH, PLAYER_INTERFACE, Playback, Track, artists_to_line,
    better_candidate, identity_from_bus, is_player_bus,
};

pub type Sink = Arc<dyn Fn(MediaEvent) + Send + Sync>;

pub struct MediaHandle {
    connection: zbus::blocking::Connection,
    current: Arc<Mutex<Option<String>>>,
    wake: Arc<std::sync::atomic::AtomicBool>,
}

impl std::fmt::Debug for MediaHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("MediaHandle")
    }
}

impl MediaHandle {
    fn bus(&self) -> Option<String> {
        self.current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn poke(&self) {
        self.wake.store(true, std::sync::atomic::Ordering::Release);
    }

    pub fn send(&self, command: Command) {
        let Some(bus) = self.bus() else {
            return;
        };
        let call = zbus::message::Message::method_call(OBJECT_PATH, command.method())
            .and_then(|builder| builder.destination(bus.as_str()))
            .and_then(|builder| builder.interface(PLAYER_INTERFACE))
            .and_then(|builder| builder.build(&()));
        match call {
            Ok(message) => {
                if let Err(err) = self.connection.send(&message) {
                    tracing::debug!("failed to send {}: {err}", command.method());
                }
            }
            Err(err) => tracing::debug!("failed to build {}: {err}", command.method()),
        }
        self.poke();
    }

    #[must_use]
    pub fn poll(&self) -> Option<Track> {
        let bus = self.bus()?;
        read_track(&self.connection, &bus)
    }

    pub fn seek(&self, fraction: f32) {
        let Some(bus) = self.bus() else {
            return;
        };
        let Some(track) = read_track(&self.connection, &bus) else {
            return;
        };
        let Some(target) = crate::model::seek_target(track.length, fraction) else {
            return;
        };
        if track.track_id.is_empty() {
            return;
        }
        let Ok(path) = zbus::zvariant::ObjectPath::try_from(track.track_id) else {
            return;
        };
        let Ok(micros) = i64::try_from(target.as_micros()) else {
            return;
        };

        let call = zbus::message::Message::method_call(OBJECT_PATH, "SetPosition")
            .and_then(|builder| builder.destination(bus.as_str()))
            .and_then(|builder| builder.interface(PLAYER_INTERFACE))
            .and_then(|builder| builder.build(&(path, micros)));
        match call {
            Ok(message) => {
                if let Err(err) = self.connection.send(&message) {
                    tracing::debug!("failed to seek: {err}");
                }
            }
            Err(err) => tracing::debug!("failed to build SetPosition: {err}"),
        }
        self.poke();
    }
}

fn string_of(map: &HashMap<String, OwnedValue>, key: &str) -> String {
    map.get(key)
        .and_then(|value| String::try_from(value.clone()).ok())
        .unwrap_or_default()
}

fn micros_of(map: &HashMap<String, OwnedValue>, key: &str) -> Option<Duration> {
    micros_from(map.get(key)?)
}

fn micros_from(value: &OwnedValue) -> Option<Duration> {
    let micros = match &**value {
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

fn property(connection: &zbus::blocking::Connection, bus: &str, name: &str) -> Option<OwnedValue> {
    let proxy = zbus::blocking::Proxy::new(
        connection,
        bus,
        OBJECT_PATH,
        "org.freedesktop.DBus.Properties",
    )
    .ok()?;
    proxy.call("Get", &(PLAYER_INTERFACE, name)).ok()
}

fn boolean(connection: &zbus::blocking::Connection, bus: &str, name: &str) -> bool {
    property(connection, bus, name)
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false)
}

fn read_track(connection: &zbus::blocking::Connection, bus: &str) -> Option<Track> {
    let metadata = property(connection, bus, "Metadata")?;
    let metadata = HashMap::<String, OwnedValue>::try_from(metadata).ok()?;

    let status = property(connection, bus, "PlaybackStatus")
        .and_then(|value| String::try_from(value).ok())
        .map_or(Playback::Stopped, |value| Playback::from_wire(&value));

    let position = property(connection, bus, "Position").and_then(|value| micros_from(&value));

    Some(Track {
        bus: bus.to_string(),
        identity: identity_from_bus(bus),
        track_id: metadata
            .get("mpris:trackid")
            .and_then(|value| {
                zbus::zvariant::ObjectPath::try_from(value.clone())
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
        position,
        status,
        can_go_next: boolean(connection, bus, "CanGoNext"),
        can_go_previous: boolean(connection, bus, "CanGoPrevious"),
        can_pause: boolean(connection, bus, "CanPause"),
    })
}

fn list_players(connection: &zbus::blocking::Connection) -> Vec<String> {
    let proxy = zbus::blocking::fdo::DBusProxy::new(connection);
    let Ok(proxy) = proxy else {
        return Vec::new();
    };
    let Ok(names) = proxy.list_names() else {
        return Vec::new();
    };
    names
        .into_iter()
        .filter(|name| is_player_bus(name.as_str()))
        .map(|name| name.as_str().to_string())
        .collect()
}

fn choose(connection: &zbus::blocking::Connection) -> Option<Track> {
    let mut best: Option<Track> = None;
    for bus in list_players(connection) {
        let Some(track) = read_track(connection, &bus) else {
            continue;
        };
        if !track.status.is_active() {
            continue;
        }
        if better_candidate(best.as_ref(), &track) || best.is_none() {
            best = Some(track);
        }
    }
    best
}

const TICK: Duration = Duration::from_millis(100);
const TICKS_PER_SCAN: u32 = 5;

pub fn start(sink: Sink) -> anyhow::Result<MediaHandle> {
    let connection = zbus::blocking::Connection::session()?;
    let current: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let wake = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let handle = MediaHandle {
        connection: connection.clone(),
        current: Arc::clone(&current),
        wake: Arc::clone(&wake),
    };

    std::thread::Builder::new()
        .name("glide-mpris".to_string())
        .spawn(move || {
            let mut last: Option<Track> = None;
            let mut ticks = 0u32;
            publish(&connection, &current, &mut last, &sink);
            loop {
                std::thread::sleep(TICK);
                ticks += 1;
                let poked = wake.swap(false, std::sync::atomic::Ordering::AcqRel);
                if !poked && ticks < TICKS_PER_SCAN {
                    continue;
                }
                ticks = 0;
                publish(&connection, &current, &mut last, &sink);
            }
        })?;

    Ok(handle)
}

fn publish(
    connection: &zbus::blocking::Connection,
    current: &Arc<Mutex<Option<String>>>,
    last: &mut Option<Track>,
    sink: &Sink,
) {
    let track = choose(connection);
    {
        let mut slot = current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *slot = track.as_ref().map(|track| track.bus.clone());
    }

    match (&track, last.as_ref()) {
        (None, None) => {}
        (None, Some(_)) => {
            *last = None;
            sink(MediaEvent::Gone);
        }
        (Some(track), None) => {
            *last = Some(track.clone());
            sink(MediaEvent::Changed(Box::new(track.clone())));
        }
        (Some(track), Some(previous)) => {
            if previous.is_same_track(track) && previous.status == track.status {
                if previous.progress() != track.progress() {
                    *last = Some(track.clone());
                    sink(MediaEvent::Progress(track.progress()));
                }
            } else {
                *last = Some(track.clone());
                sink(MediaEvent::Changed(Box::new(track.clone())));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_strings_default_rather_than_failing() {
        let map = HashMap::new();
        assert_eq!(string_of(&map, "xesam:title"), "");
        assert_eq!(artists_of(&map), "");
    }

    #[test]
    fn a_length_is_read_from_any_integer_width() {
        let mut map = HashMap::new();
        map.insert(
            "mpris:length".to_string(),
            OwnedValue::try_from(Value::I64(2_000_000)).expect("value converts"),
        );
        assert_eq!(
            micros_of(&map, "mpris:length"),
            Some(Duration::from_secs(2))
        );

        map.insert(
            "mpris:length".to_string(),
            OwnedValue::try_from(Value::U32(3_000_000)).expect("value converts"),
        );
        assert_eq!(
            micros_of(&map, "mpris:length"),
            Some(Duration::from_secs(3))
        );
    }

    #[test]
    fn a_negative_length_is_refused_rather_than_wrapping() {
        let mut map = HashMap::new();
        map.insert(
            "mpris:length".to_string(),
            OwnedValue::try_from(Value::I64(-5)).expect("value converts"),
        );
        assert_eq!(micros_of(&map, "mpris:length"), None);
    }

    #[test]
    fn a_single_artist_string_is_accepted_as_well_as_a_list() {
        let mut map = HashMap::new();
        map.insert(
            "xesam:artist".to_string(),
            OwnedValue::try_from(Value::Str("Solo".into())).expect("value converts"),
        );
        assert_eq!(artists_of(&map), "Solo");
    }

    #[test]
    fn a_metadata_key_of_the_wrong_type_does_not_panic() {
        let mut map = HashMap::new();
        map.insert(
            "xesam:title".to_string(),
            OwnedValue::try_from(Value::I32(7)).expect("value converts"),
        );
        assert_eq!(string_of(&map, "xesam:title"), "");
    }
}

use std::collections::{BTreeSet, HashMap};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use zbus::blocking::{Connection, MessageIterator, Proxy, fdo::DBusProxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const BLUEZ: &str = "org.bluez";
const TRANSPORT: &str = "org.bluez.MediaTransport1";
const DEVICE: &str = "org.bluez.Device1";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const RULES: [&str; 2] = [
    "type='signal',sender='org.bluez',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged'",
    "type='signal',sender='org.bluez',interface='org.freedesktop.DBus.ObjectManager'",
];
const SETTLE: Duration = Duration::from_millis(1500);
const STREAMING_GRACE: Duration = Duration::from_secs(3);
const LOCAL_EDIT: Duration = Duration::from_millis(1500);
const SINK_DEBOUNCE: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Bluetooth,
    Jack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    HeadsetVolume,
    Unplugged(Output),
}

#[derive(Debug, Clone)]
struct Transport {
    device: String,
    active: bool,
    settled_at: Instant,
}

#[derive(Debug, Default)]
pub struct Transports {
    known: HashMap<String, Transport>,
    last_active: HashMap<String, Instant>,
}

impl Transports {
    pub fn seed(&mut self, path: &str, device: &str, state: &str, now: Instant) {
        let transport = Transport { device: device.to_string(), active: state == "active", settled_at: now };
        self.known.insert(path.to_string(), transport);
    }

    pub fn appeared(&mut self, path: &str, device: &str, state: &str, now: Instant) {
        self.seed(path, device, state, now + SETTLE);
    }

    fn leave_active(&mut self, path: &str, now: Instant) {
        if let Some(transport) = self.known.get(path).filter(|transport| transport.active) {
            self.last_active.insert(transport.device.clone(), now);
        }
    }

    pub fn changed(&mut self, path: &str, state: Option<&str>, volume: bool, now: Instant) -> bool {
        if let Some(state) = state {
            if state != "active" {
                self.leave_active(path, now);
            }
            if let Some(transport) = self.known.get_mut(path) {
                transport.active = state == "active";
                transport.settled_at = now + SETTLE;
            }
            return false;
        }
        volume && self.known.get(path).is_some_and(|transport| now >= transport.settled_at)
    }

    pub fn removed(&mut self, path: &str, now: Instant) {
        self.leave_active(path, now);
        self.known.remove(path);
    }

    pub fn device_disconnected(&mut self, device: &str, now: Instant) -> bool {
        let streaming = self.known.values().any(|transport| transport.device == device && transport.active);
        let recently =
            self.last_active.remove(device).is_some_and(|at| now.saturating_duration_since(at) <= STREAMING_GRACE);
        self.known.retain(|_, transport| transport.device != device);
        streaming || recently
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outputs {
    pub default: String,
    pub on_headphones: BTreeSet<String>,
    pub plugged: BTreeSet<String>,
}

fn headphone_port(port: &serde_json::Value) -> bool {
    let text = |key: &str| port.get(key).and_then(serde_json::Value::as_str).unwrap_or_default().to_lowercase();
    let kind = text("type");
    let name = text("name");
    matches!(kind.as_str(), "headphones" | "headset") || name.contains("headphone") || name.contains("headset")
}

fn available(port: &serde_json::Value) -> bool {
    port.get("availability").and_then(serde_json::Value::as_str) != Some("not available")
}

#[must_use]
pub fn read_outputs(json: &str, default: &str) -> Outputs {
    let mut outputs = Outputs { default: default.to_string(), ..Outputs::default() };
    let Ok(serde_json::Value::Array(sinks)) = serde_json::from_str::<serde_json::Value>(json) else {
        return outputs;
    };
    for sink in &sinks {
        let name = sink.get("name").and_then(serde_json::Value::as_str).unwrap_or_default();
        if name.is_empty() || name.starts_with("bluez_") {
            continue;
        }
        let ports = sink.get("ports").and_then(serde_json::Value::as_array).map(Vec::as_slice).unwrap_or_default();
        let active = sink.get("active_port").and_then(serde_json::Value::as_str).unwrap_or_default();
        let headphone_ports = || ports.iter().filter(|port| headphone_port(port) && available(port));
        if headphone_ports().next().is_some() {
            outputs.plugged.insert(name.to_string());
        }
        let on_active =
            headphone_ports().any(|port| port.get("name").and_then(serde_json::Value::as_str) == Some(active));
        if on_active {
            outputs.on_headphones.insert(name.to_string());
        }
    }
    outputs
}

#[must_use]
pub fn unplugged(previous: &Outputs, current: &Outputs) -> bool {
    previous.on_headphones.contains(&previous.default) && !current.plugged.contains(&previous.default)
}

fn transport_facts(properties: &HashMap<String, OwnedValue>) -> (String, String) {
    let device = properties
        .get("Device")
        .and_then(|value| OwnedObjectPath::try_from(value.clone()).ok())
        .map(|path| path.to_string())
        .unwrap_or_default();
    let state = properties.get("State").and_then(|value| String::try_from(value.clone()).ok()).unwrap_or_default();
    (device, state)
}

type Managed = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn seed(connection: &Connection, transports: &mut Transports) {
    let objects = Proxy::new(connection, BLUEZ, "/", OBJECT_MANAGER)
        .and_then(|manager| manager.call_method("GetManagedObjects", &()))
        .and_then(|reply| reply.body().deserialize::<Managed>());
    let Ok(objects) = objects else {
        return;
    };
    let now = Instant::now();
    for (path, interfaces) in objects {
        if let Some(properties) = interfaces.get(TRANSPORT) {
            let (device, state) = transport_facts(properties);
            transports.seed(path.as_str(), &device, &state, now);
        }
    }
}

fn bluez_event(message: &zbus::Message, transports: &mut Transports) -> Option<Event> {
    let header = message.header();
    let now = Instant::now();
    match header.member()?.as_str() {
        "PropertiesChanged" => {
            let path = header.path()?.to_string();
            let (interface, changed, _) =
                message.body().deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>().ok()?;
            match interface.as_str() {
                TRANSPORT => {
                    let state = changed.get("State").and_then(|value| String::try_from(value.clone()).ok());
                    transports
                        .changed(&path, state.as_deref(), changed.contains_key("Volume"), now)
                        .then_some(Event::HeadsetVolume)
                }
                DEVICE => {
                    let connected = changed.get("Connected").and_then(|value| bool::try_from(value.clone()).ok());
                    (connected == Some(false) && transports.device_disconnected(&path, now))
                        .then_some(Event::Unplugged(Output::Bluetooth))
                }
                _ => None,
            }
        }
        "InterfacesAdded" => {
            let (path, interfaces) =
                message.body().deserialize::<(OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>)>().ok()?;
            let (device, state) = transport_facts(interfaces.get(TRANSPORT)?);
            transports.appeared(path.as_str(), &device, &state, now);
            None
        }
        "InterfacesRemoved" => {
            let (path, interfaces) = message.body().deserialize::<(OwnedObjectPath, Vec<String>)>().ok()?;
            if interfaces.iter().any(|name| name == TRANSPORT) {
                transports.removed(path.as_str(), now);
            }
            None
        }
        _ => None,
    }
}

fn watch_bluez(sink: impl Fn(Event) + Send + 'static) -> anyhow::Result<()> {
    let connection = Connection::system()?;
    let bus = DBusProxy::new(&connection)?;
    for rule in RULES {
        bus.add_match_rule(zbus::MatchRule::try_from(rule)?)?;
    }
    std::thread::Builder::new().name("glide-headset-bt".into()).spawn(move || {
        let mut transports = Transports::default();
        seed(&connection, &mut transports);
        for message in MessageIterator::from(&connection) {
            if let Some(event) = message.ok().and_then(|message| bluez_event(&message, &mut transports)) {
                sink(event);
            }
        }
    })?;
    Ok(())
}

fn pactl(args: &[&str]) -> Option<String> {
    let output = Command::new("pactl").args(args).env("LC_ALL", "C").stdin(Stdio::null()).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn snapshot() -> Outputs {
    let default = pactl(&["get-default-sink"]).map(|name| name.trim().to_string()).unwrap_or_default();
    pactl(&["--format=json", "list", "sinks"]).map_or_else(Outputs::default, |json| read_outputs(&json, &default))
}

#[must_use]
pub fn touches_outputs(line: &str) -> bool {
    line.contains(" on sink #") || line.contains(" on card #") || line.contains(" on server")
}

fn watch_jack(sink: impl Fn(Event) + Send + 'static) -> anyhow::Result<()> {
    let mut child = Command::new("pactl")
        .arg("subscribe")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("pactl subscribe has no output"))?;
    let (lines, inbox) = mpsc::channel::<()>();
    std::thread::Builder::new().name("glide-headset-pactl".into()).spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if touches_outputs(&line) && lines.send(()).is_err() {
                break;
            }
        }
        let _ = child.kill();
        let _ = child.wait();
    })?;
    std::thread::Builder::new().name("glide-headset-jack".into()).spawn(move || {
        let mut previous = snapshot();
        while inbox.recv().is_ok() {
            while inbox.recv_timeout(SINK_DEBOUNCE).is_ok() {}
            let current = snapshot();
            if unplugged(&previous, &current) {
                sink(Event::Unplugged(Output::Jack));
            }
            previous = current;
        }
        tracing::info!("pactl subscribe ended; headphone unplugging is no longer watched");
    })?;
    Ok(())
}

pub fn start(sink: impl Fn(Event) + Send + Sync + Clone + 'static) {
    if let Err(err) = watch_bluez(sink.clone()) {
        tracing::info!("Bluetooth headsets are not watched ({err}); is BlueZ running?");
    }
    if let Err(err) = watch_jack(sink) {
        tracing::info!("headphone jacks are not watched ({err}); is pactl installed?");
    }
}

#[derive(Debug, Default)]
pub struct Session {
    local_edit: Option<Instant>,
    pub pause_on_unplug: bool,
}

impl Session {
    pub fn local_volume_edit(&mut self, now: Instant) {
        self.local_edit = Some(now);
    }

    #[must_use]
    pub fn edited_here(&self, now: Instant) -> bool {
        self.local_edit.is_some_and(|at| now.saturating_duration_since(at) <= LOCAL_EDIT)
    }
}

impl crate::GlideState {
    pub fn handle_headset(&mut self, event: Event) {
        let now = Instant::now();
        match event {
            Event::HeadsetVolume => {
                if self.session.headset.edited_here(now) {
                    return;
                }
                self.session.osd.expect(crate::osd::Kind::Volume, now);
                crate::system_info::poke();
            }
            Event::Unplugged(output) => {
                if !self.session.headset.pause_on_unplug {
                    return;
                }
                tracing::info!(?output, "headphones went away; pausing what is playing");
                if let Some(media) = self.media.as_ref() {
                    media.pause_all();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUDS: &str = "/org/bluez/hci0/dev_AA";
    const FD: &str = "/org/bluez/hci0/dev_AA/sep1/fd0";

    #[test]
    fn a_swipe_on_settled_buds_is_a_headset_volume_change() {
        let start = Instant::now();
        let mut transports = Transports::default();
        transports.seed(FD, BUDS, "active", start);
        assert!(transports.changed(FD, None, true, start), "buds that were connected before Glide started count");
        assert!(
            !transports.changed("/org/bluez/hci0/dev_BB/sep1/fd0", None, true, start),
            "unknown transports are not trusted"
        );
        assert!(!transports.changed(FD, None, false, start), "a change without the volume is not one");
    }

    #[test]
    fn connecting_and_starting_playback_never_count_as_a_swipe() {
        let start = Instant::now();
        let mut transports = Transports::default();
        transports.appeared(FD, BUDS, "idle", start);
        assert!(
            !transports.changed(FD, None, true, start + Duration::from_millis(500)),
            "PipeWire sets the volume on connect"
        );
        assert!(transports.changed(FD, None, true, start + SETTLE), "after the settle window it is the headset");
        let playing = start + Duration::from_secs(10);
        assert!(!transports.changed(FD, Some("active"), true, playing), "acquiring the transport rewrites the volume");
        assert!(!transports.changed(FD, None, true, playing + Duration::from_millis(400)));
        assert!(transports.changed(FD, None, true, playing + SETTLE));
    }

    #[test]
    fn only_a_headset_that_was_playing_pauses_the_music_when_it_disconnects() {
        let start = Instant::now();
        let mut silent = Transports::default();
        silent.seed(FD, BUDS, "idle", start);
        assert!(!silent.device_disconnected(BUDS, start), "a paired headset that was not playing changes nothing");

        let mut playing = Transports::default();
        playing.seed(FD, BUDS, "active", start);
        assert!(playing.device_disconnected(BUDS, start));

        let mut torn_down = Transports::default();
        torn_down.seed(FD, BUDS, "active", start);
        torn_down.removed(FD, start + Duration::from_millis(100));
        assert!(
            torn_down.device_disconnected(BUDS, start + Duration::from_millis(300)),
            "BlueZ removes the transport first"
        );

        let mut long_ago = Transports::default();
        long_ago.seed(FD, BUDS, "active", start);
        long_ago.changed(FD, Some("idle"), false, start);
        assert!(!long_ago.device_disconnected(BUDS, start + STREAMING_GRACE + Duration::from_secs(1)));
    }

    fn sinks(headphones: &str, active: &str) -> String {
        format!(
            r#"[{{"name":"alsa_output.analog","active_port":"{active}","ports":[
                {{"name":"[Out] Speaker","type":"Speaker","availability":"availability unknown"}},
                {{"name":"[Out] Headphones","type":"Headphones","availability":"{headphones}"}}]}},
               {{"name":"bluez_output.AA.1","active_port":"headset-output","ports":[
                {{"name":"headset-output","type":"Headset","availability":"available"}}]}}]"#
        )
    }

    #[test]
    fn pulling_the_jack_out_of_the_default_output_is_an_unplug() {
        let plugged = read_outputs(&sinks("available", "[Out] Headphones"), "alsa_output.analog");
        assert!(
            plugged.on_headphones.contains("alsa_output.analog")
                && !plugged.on_headphones.contains("bluez_output.AA.1")
        );
        let pulled = read_outputs(&sinks("not available", "[Out] Speaker"), "alsa_output.analog");
        assert!(unplugged(&plugged, &pulled));
        let gone = read_outputs("[]", "");
        assert!(unplugged(&plugged, &gone), "a USB headset or a UCM sink can vanish with its jack");
    }

    #[test]
    fn switching_ports_by_hand_or_playing_elsewhere_is_not_an_unplug() {
        let plugged = read_outputs(&sinks("available", "[Out] Headphones"), "alsa_output.analog");
        let switched = read_outputs(&sinks("available", "[Out] Speaker"), "alsa_output.analog");
        assert!(!unplugged(&plugged, &switched), "the headphones are still in");
        let elsewhere = read_outputs(&sinks("available", "[Out] Headphones"), "alsa_output.hdmi");
        let pulled = read_outputs(&sinks("not available", "[Out] Speaker"), "alsa_output.hdmi");
        assert!(!unplugged(&elsewhere, &pulled), "music on another output keeps playing");
        assert_eq!(read_outputs("not json", "x"), Outputs { default: "x".into(), ..Outputs::default() });
    }

    #[test]
    fn only_sink_card_and_server_events_wake_the_jack_watch() {
        assert!(touches_outputs("Event 'change' on sink #56"));
        assert!(touches_outputs("Event 'remove' on card #45"));
        assert!(touches_outputs("Event 'change' on server #-1"));
        assert!(!touches_outputs("Event 'change' on sink-input #12"));
        assert!(!touches_outputs("Event 'new' on client #18738"));
    }

    #[test]
    fn glides_own_volume_edits_hold_the_headset_indicator_back() {
        let now = Instant::now();
        let mut session = Session::default();
        assert!(!session.edited_here(now));
        session.local_volume_edit(now);
        assert!(session.edited_here(now + Duration::from_millis(800)));
        assert!(!session.edited_here(now + LOCAL_EDIT + Duration::from_millis(1)));
    }
}

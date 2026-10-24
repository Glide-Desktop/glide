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
const BATTERY: &str = "org.bluez.Battery1";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
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
pub enum Mode {
    Music,
    Calls,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Profile {
    pub mode: Option<Mode>,
    pub codec: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Headset {
    pub mac: String,
    pub name: String,
    pub icon: String,
    pub battery: Option<u8>,
    pub address: String,
    pub volume: Option<u8>,
    pub profile: Profile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    HeadsetVolume,
    Unplugged(Output),
    Connected { headset: Headset, announce: bool },
    Battery { mac: String, level: u8 },
    Volume { mac: String, level: u8 },
    Profile { mac: String, profile: Profile },
    Disconnected { mac: String },
}

const CODECS: [&str; 12] =
    ["sbc_xq", "sbc", "aac", "aptx_hd", "aptx_ll", "aptx", "ldac", "opus_05", "opus", "lc3", "msbc", "cvsd"];

#[must_use]
pub fn parse_profile(active: &str) -> Profile {
    let active = active.trim().to_ascii_lowercase();
    let mode = if active.starts_with("a2dp") {
        Some(Mode::Music)
    } else if active.starts_with("headset") || active.starts_with("handsfree") || active.contains("head-unit") {
        Some(Mode::Calls)
    } else {
        None
    };
    let codec = CODECS
        .iter()
        .find(|codec| active.ends_with(&format!("-{codec}")) || active.ends_with(&format!("_{codec}")))
        .map(|codec| codec.replace('_', "-").to_ascii_uppercase())
        .or_else(|| (mode == Some(Mode::Music)).then(|| "SBC".to_string()));
    Profile { mode, codec }
}

#[must_use]
pub fn card_profile(cards: &str, mac: &str) -> Option<Profile> {
    let serde_json::Value::Array(cards) = serde_json::from_str::<serde_json::Value>(cards).ok()? else {
        return None;
    };
    let wanted = mac.to_ascii_uppercase();
    cards
        .iter()
        .find(|card| {
            card.get("name")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|name| name.to_ascii_uppercase().contains(&wanted))
        })
        .and_then(|card| card.get("active_profile").and_then(serde_json::Value::as_str))
        .map(parse_profile)
}

fn read_profile(mac: &str) -> Option<Profile> {
    pactl(&["--format=json", "list", "cards"]).and_then(|cards| card_profile(&cards, mac))
}

#[must_use]
pub fn volume_pct(raw: u16) -> u8 {
    u8::try_from((u32::from(raw.min(127)) * 100 + 63) / 127).unwrap_or(100)
}

fn transport_volume(properties: &HashMap<String, OwnedValue>) -> Option<u8> {
    properties.get("Volume").and_then(|value| u16::try_from(value.clone()).ok()).map(volume_pct)
}

#[must_use]
pub fn mac_of(path: &str) -> Option<&str> {
    path.rsplit('/').next()?.strip_prefix("dev_").filter(|mac| !mac.is_empty())
}

fn is_audio(icon: &str) -> bool {
    matches!(icon, "audio-headset" | "audio-headphones")
}

#[must_use]
pub fn headset_from(path: &str, device: &HashMap<String, OwnedValue>, battery: Option<u8>) -> Option<Headset> {
    let text = |key: &str| device.get(key).and_then(|value| String::try_from(value.clone()).ok()).unwrap_or_default();
    let icon = text("Icon");
    if !is_audio(&icon) {
        return None;
    }
    let mac = mac_of(path)?.to_string();
    let name = [text("Alias"), text("Name")].into_iter().find(|name| !name.trim().is_empty()).unwrap_or(mac.clone());
    let address = Some(text("Address")).filter(|address| !address.is_empty()).unwrap_or_else(|| mac.replace('_', ":"));
    Some(Headset { mac, name, icon, battery, address, volume: None, profile: Profile::default() })
}

fn percentage(properties: &HashMap<String, OwnedValue>) -> Option<u8> {
    properties.get("Percentage").and_then(|value| u8::try_from(value.clone()).ok())
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

    #[must_use]
    pub fn device_of(&self, path: &str) -> Option<String> {
        self.known.get(path).map(|transport| transport.device.clone())
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

fn seed(connection: &Connection, transports: &mut Transports) -> Vec<Event> {
    let objects = Proxy::new(connection, BLUEZ, "/", OBJECT_MANAGER)
        .and_then(|manager| manager.call_method("GetManagedObjects", &()))
        .and_then(|reply| reply.body().deserialize::<Managed>());
    let Ok(objects) = objects else {
        return Vec::new();
    };
    let now = Instant::now();
    let mut volumes: HashMap<String, u8> = HashMap::new();
    for (path, interfaces) in &objects {
        if let Some(properties) = interfaces.get(TRANSPORT) {
            let (device, state) = transport_facts(properties);
            transports.seed(path.as_str(), &device, &state, now);
            if let Some(volume) = transport_volume(properties) {
                volumes.insert(device, volume);
            }
        }
    }
    let mut events = Vec::new();
    for (path, interfaces) in &objects {
        let connected = interfaces
            .get(DEVICE)
            .and_then(|device| device.get("Connected"))
            .and_then(|value| bool::try_from(value.clone()).ok());
        let battery = interfaces.get(BATTERY).and_then(percentage);
        if connected == Some(true)
            && let Some(mut headset) =
                interfaces.get(DEVICE).and_then(|device| headset_from(path.as_str(), device, battery))
        {
            headset.volume = volumes.get(path.as_str()).copied();
            headset.profile = read_profile(&headset.mac).unwrap_or_default();
            events.push(Event::Connected { headset, announce: false });
        }
    }
    events
}

fn profile_event(device: &str) -> Option<Event> {
    let mac = mac_of(device)?;
    read_profile(mac).map(|profile| Event::Profile { mac: mac.to_string(), profile })
}

fn all_properties(connection: &Connection, path: &str, interface: &str) -> Option<HashMap<String, OwnedValue>> {
    Proxy::new(connection, BLUEZ, path, PROPERTIES)
        .and_then(|proxy| proxy.call_method("GetAll", &(interface,)))
        .and_then(|reply| reply.body().deserialize::<HashMap<String, OwnedValue>>())
        .ok()
}

fn connected_headset(connection: &Connection, path: &str) -> Option<Event> {
    let device = all_properties(connection, path, DEVICE)?;
    let battery = all_properties(connection, path, BATTERY).as_ref().and_then(percentage);
    headset_from(path, &device, battery).map(|headset| Event::Connected { headset, announce: true })
}

fn bluez_event(connection: &Connection, message: &zbus::Message, transports: &mut Transports) -> Vec<Event> {
    let header = message.header();
    let now = Instant::now();
    let Some(member) = header.member() else {
        return Vec::new();
    };
    match member.as_str() {
        "PropertiesChanged" => {
            let Some(path) = header.path().map(ToString::to_string) else {
                return Vec::new();
            };
            let Ok((interface, changed, _)) =
                message.body().deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
            else {
                return Vec::new();
            };
            match interface.as_str() {
                TRANSPORT => {
                    let state = changed.get("State").and_then(|value| String::try_from(value.clone()).ok());
                    let swiped = transports.changed(&path, state.as_deref(), changed.contains_key("Volume"), now);
                    let device = transports.device_of(&path);
                    let level = transport_volume(&changed)
                        .zip(device.as_deref().and_then(mac_of))
                        .map(|(level, mac)| Event::Volume { mac: mac.to_string(), level });
                    let profile = state.and(device.as_deref()).and_then(profile_event);
                    swiped.then_some(Event::HeadsetVolume).into_iter().chain(level).chain(profile).collect()
                }
                DEVICE => match changed.get("Connected").and_then(|value| bool::try_from(value.clone()).ok()) {
                    Some(false) => {
                        let unplugged = transports.device_disconnected(&path, now);
                        let gone = mac_of(&path).map(|mac| Event::Disconnected { mac: mac.to_string() });
                        unplugged.then_some(Event::Unplugged(Output::Bluetooth)).into_iter().chain(gone).collect()
                    }
                    Some(true) => connected_headset(connection, &path).into_iter().collect(),
                    None => Vec::new(),
                },
                BATTERY => percentage(&changed)
                    .zip(mac_of(&path))
                    .map(|(level, mac)| Event::Battery { mac: mac.to_string(), level })
                    .into_iter()
                    .collect(),
                _ => Vec::new(),
            }
        }
        "InterfacesAdded" => {
            let Ok((path, interfaces)) =
                message.body().deserialize::<(OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>)>()
            else {
                return Vec::new();
            };
            let mut events = Vec::new();
            if let Some(transport) = interfaces.get(TRANSPORT) {
                let (device, state) = transport_facts(transport);
                transports.appeared(path.as_str(), &device, &state, now);
                let level = transport_volume(transport)
                    .zip(mac_of(&device))
                    .map(|(level, mac)| Event::Volume { mac: mac.to_string(), level });
                events.extend(level.into_iter().chain(profile_event(&device)));
            }
            events.extend(
                interfaces
                    .get(BATTERY)
                    .and_then(percentage)
                    .zip(mac_of(path.as_str()))
                    .map(|(level, mac)| Event::Battery { mac: mac.to_string(), level }),
            );
            events
        }
        "InterfacesRemoved" => {
            if let Ok((path, interfaces)) = message.body().deserialize::<(OwnedObjectPath, Vec<String>)>()
                && interfaces.iter().any(|name| name == TRANSPORT)
            {
                transports.removed(path.as_str(), now);
            }
            Vec::new()
        }
        _ => Vec::new(),
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
        for event in seed(&connection, &mut transports) {
            sink(event);
        }
        for message in MessageIterator::from(&connection) {
            let Ok(message) = message else {
                continue;
            };
            for event in bluez_event(&connection, &message, &mut transports) {
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
    pub(crate) fn run_headset_action(&mut self, action: crate::topbar::TopbarAction) {
        use crate::qs_net::{NetRequest, request};
        use crate::topbar::{OpenPopup, TopbarAction};
        match action {
            TopbarAction::ToggleHeadset => {
                self.topbar.toggle_popup(OpenPopup::Headset);
                if self.topbar.open_popup() == Some(OpenPopup::Headset) {
                    request(NetRequest::ScanOutputs);
                }
            }
            TopbarAction::HeadsetUseOutput => {
                if let Some(output) = self.topbar.headset_output().filter(|output| !output.active).cloned() {
                    self.topbar.begin_connect(&output.name);
                    request(NetRequest::UseOutput { name: output.name });
                }
            }
            TopbarAction::HeadsetDisconnect => {
                if let Some(address) = self.topbar.headset().map(|headset| headset.address.clone()) {
                    self.topbar.close_popup();
                    request(NetRequest::Bluetooth { mac: address, connect: false });
                }
            }
            TopbarAction::HeadsetSettings => {
                self.topbar.close_popup();
                crate::qs_net::open_settings_panel("bluetooth");
            }
            _ => return,
        }
        self.queue_redraw();
    }

    pub fn handle_headset(&mut self, event: Event) {
        let now = Instant::now();
        match event {
            Event::HeadsetVolume => {
                if self.session.headset.edited_here(now) {
                    return;
                }
                self.session.osd.expect(crate::osd::Kind::Volume, now);
                crate::system_info::poke(crate::system_info::Topics::AUDIO);
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
            Event::Connected { headset, announce } => {
                tracing::info!(name = headset.name, battery = ?headset.battery, "a headset is connected");
                if announce && !self.screen_locked() {
                    let icon = self
                        .dock
                        .icons
                        .load_named(&headset.icon, crate::notifications::ICON_SOURCE_PX)
                        .map(|bytes| std::sync::Arc::new(bytes.to_vec()));
                    let alert = crate::topbar::IslandAlert {
                        id: 0,
                        heading: headset.name.clone(),
                        summary: connected_line(headset.battery),
                        icon,
                    };
                    self.topbar.show_alert(alert, now);
                }
                self.topbar.set_headset(headset);
                self.queue_redraw();
            }
            Event::Battery { mac, level } => {
                if self.topbar.update_headset(&mac, |headset| headset.battery = Some(level)) {
                    self.queue_redraw();
                }
            }
            Event::Volume { mac, level } => {
                if self.topbar.update_headset(&mac, |headset| headset.volume = Some(level)) {
                    self.queue_redraw();
                }
            }
            Event::Profile { mac, profile } => {
                if self.topbar.update_headset(&mac, |headset| headset.profile = profile) {
                    self.queue_redraw();
                }
            }
            Event::Disconnected { mac } => {
                if self.topbar.forget_headset(&mac) {
                    self.queue_redraw();
                }
            }
        }
    }
}

fn connected_line(battery: Option<u8>) -> String {
    use crate::i18n::{Key, t};
    let connected = t(Key::HeadsetConnected);
    battery.map_or_else(
        || connected.to_string(),
        |level| format!("{connected} · {}", t(Key::HeadsetBattery).replace("{pct}", &level.to_string())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUDS: &str = "/org/bluez/hci0/dev_AA";
    const FD: &str = "/org/bluez/hci0/dev_AA/sep1/fd0";

    fn device(icon: &str, alias: &str) -> HashMap<String, OwnedValue> {
        let text = |value: &str| OwnedValue::try_from(zbus::zvariant::Value::from(value)).expect("a string value");
        HashMap::from([("Icon".to_string(), text(icon)), ("Alias".to_string(), text(alias))])
    }

    #[test]
    fn only_headphones_and_headsets_count_and_they_are_named_by_their_alias() {
        assert_eq!(mac_of(BUDS), Some("AA"));
        assert_eq!(mac_of("/org/bluez/hci0"), None);
        let buds = headset_from(BUDS, &device("audio-headset", "AirPods Pro"), Some(80)).expect("a headset");
        assert_eq!(
            buds,
            Headset {
                mac: "AA".into(),
                name: "AirPods Pro".into(),
                icon: "audio-headset".into(),
                battery: Some(80),
                address: "AA".into(),
                volume: None,
                profile: Profile::default(),
            }
        );
        assert!(headset_from(BUDS, &device("input-mouse", "MX Master"), None).is_none(), "a mouse is no headset");
        let unnamed = headset_from(BUDS, &device("audio-headphones", " "), None).expect("headphones");
        assert_eq!(unnamed.name, "AA", "the address stands in for a missing name");
    }

    #[test]
    fn the_card_profile_names_the_mode_and_the_codec() {
        assert_eq!(parse_profile("a2dp-sink-aac"), Profile { mode: Some(Mode::Music), codec: Some("AAC".into()) });
        assert_eq!(parse_profile("a2dp-sink"), Profile { mode: Some(Mode::Music), codec: Some("SBC".into()) });
        assert_eq!(
            parse_profile("a2dp-sink-sbc_xq"),
            Profile { mode: Some(Mode::Music), codec: Some("SBC-XQ".into()) }
        );
        assert_eq!(
            parse_profile("headset-head-unit-msbc"),
            Profile { mode: Some(Mode::Calls), codec: Some("MSBC".into()) }
        );
        assert_eq!(parse_profile("off"), Profile::default());
        let cards = r#"[{"name":"bluez_card.AA_BB","active_profile":"a2dp-sink-ldac"},{"name":"alsa_card.pci","active_profile":"output:analog-stereo"}]"#;
        assert_eq!(card_profile(cards, "aa_bb").and_then(|profile| profile.codec), Some("LDAC".into()));
        assert_eq!(card_profile(cards, "CC_DD"), None);
        assert_eq!((volume_pct(0), volume_pct(64), volume_pct(127), volume_pct(400)), (0, 50, 100, 100));
    }

    #[test]
    fn the_connect_glance_names_the_battery_only_when_bluez_reports_it() {
        use crate::i18n::{Key, t};
        assert_eq!(connected_line(None), t(Key::HeadsetConnected));
        let with_battery = connected_line(Some(64));
        assert!(with_battery.starts_with(t(Key::HeadsetConnected)) && with_battery.contains("64%"), "{with_battery}");
    }

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

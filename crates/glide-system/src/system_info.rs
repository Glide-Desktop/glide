use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

pub use crate::status::{Privacy, SystemStatus};
pub use glide_shell::battery::{
    BatteryHealth, BatteryInfo, BatteryState, PowerProfile, parse_power_profile, read_battery,
};
pub use glide_shell::scroll::{CONTINUOUS_STEP_DISTANCE, ScrollAccumulator, ScrollDelta, WHEEL_CLICK_V120};

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Topics: u16 {
        const AUDIO = 1;
        const BACKLIGHT = 1 << 1;
        const BATTERY = 1 << 2;
        const NETWORK = 1 << 3;
        const BLUETOOTH = 1 << 4;
        const RADIOS = 1 << 5;
        const POWER = 1 << 6;
        const PRIVACY = 1 << 7;
        const CAMERA = 1 << 8;
        const MOTION = 1 << 9;
    }
}

impl Topics {
    const FAST: Self = Self::AUDIO.union(Self::BACKLIGHT).union(Self::BATTERY);
}

fn refresh(status: &mut SystemStatus, topics: Topics, privacy: &mut PrivacySources, pipewire: Option<Privacy>) {
    if topics.contains(Topics::BATTERY) {
        let battery = read_battery();
        status.battery_pct = battery.as_ref().map(|info| info.pct);
        status.battery_charging = battery.as_ref().is_some_and(|info| info.state == BatteryState::Charging);
        status.battery = battery;
    }
    if topics.contains(Topics::AUDIO) {
        let volume = read_volume();
        status.volume_available = volume.is_some();
        (status.volume_pct, status.volume_muted) = volume.unwrap_or((status.volume_pct, false));
        let mic = read_mic();
        status.mic_available = mic.is_some();
        (status.mic_pct, status.mic_muted) = mic.unwrap_or((status.mic_pct, false));
    }
    if topics.contains(Topics::BACKLIGHT) {
        let backlight = read_backlight();
        status.brightness_available = backlight.is_some();
        status.brightness_pct = backlight.map_or(status.brightness_pct, |backlight| backlight.percent());
        status.keyboard_brightness = read_keyboard_backlight(Path::new("/sys/class/leds"));
    }
    if topics.contains(Topics::POWER) {
        status.power_profile = poll_stdout("powerprofilesctl", &["get"]).and_then(|text| parse_power_profile(&text));
    }
    if topics.intersects(Topics::POWER | Topics::BATTERY)
        && let Some(battery) = status.battery.as_mut()
    {
        battery.profile = status.power_profile;
    }
    if topics.contains(Topics::RADIOS) {
        status.airplane = airplane_from(&read_radios(Path::new("/sys/class/rfkill")));
    }
    if topics.contains(Topics::NETWORK) {
        (status.wifi_enabled, status.wifi_level, status.wifi_ssid) = read_wifi();
        status.vpn_active = read_vpn_active();
    }
    if topics.intersects(Topics::BLUETOOTH | Topics::RADIOS) {
        status.bluetooth_on = read_bluetooth();
        status.bluetooth_connected = status.bluetooth_on && read_bluetooth_connected();
    }
    if topics.intersects(Topics::PRIVACY | Topics::CAMERA) {
        status.privacy = privacy.refresh(topics, pipewire);
    }
    if topics.contains(Topics::MOTION) {
        status.reduced_motion = reduced_motion();
    }
}

const POLL_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

fn poll_stdout(program: &str, args: &[&str]) -> Option<String> {
    crate::spawn::run_with_timeout(program, args, POLL_COMMAND_TIMEOUT)
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn set_power_profile(profile: PowerProfile) {
    if let Err(err) = crate::spawn_shell_command(&format!("powerprofilesctl set {}", profile.key())) {
        tracing::warn!("power profile change failed to start: {err}");
    }
}

#[must_use]
pub fn read_radios(root: &Path) -> Vec<(String, bool)> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let kind = fs::read_to_string(path.join("type")).ok()?.trim().to_string();
            let soft = fs::read_to_string(path.join("soft")).ok()?.trim() == "1";
            Some((kind, soft))
        })
        .collect()
}

#[must_use]
pub fn airplane_from(radios: &[(String, bool)]) -> bool {
    let wireless: Vec<bool> = radios
        .iter()
        .filter(|(kind, _)| matches!(kind.as_str(), "wlan" | "bluetooth" | "wwan" | "uwb" | "wimax" | "nfc"))
        .map(|(_, soft)| *soft)
        .collect();
    !wireless.is_empty() && wireless.iter().all(|soft| *soft)
}

pub fn set_airplane(on: bool) {
    let command = if on { "rfkill block all" } else { "rfkill unblock all" };
    if let Err(err) = crate::spawn_shell_command(command) {
        tracing::warn!("airplane mode toggle failed to start: {err}");
    }
}

#[must_use]
pub fn parse_connected_devices(text: &str) -> bool {
    text.lines().any(|line| line.trim_start().starts_with("Device "))
}

fn read_bluetooth_connected() -> bool {
    poll_stdout("bluetoothctl", &["devices", "Connected"]).is_some_and(|text| parse_connected_devices(&text))
}

#[must_use]
pub fn parse_vpn_active(text: &str) -> bool {
    text.lines().any(|line| {
        let Some((kind, state)) = line.trim().rsplit_once(':') else {
            return false;
        };
        matches!(kind, "vpn" | "wireguard") && state == "activated"
    })
}

fn read_vpn_active() -> bool {
    poll_stdout("nmcli", &["-t", "-f", "TYPE,STATE", "con", "show", "--active"])
        .is_some_and(|text| parse_vpn_active(&text))
}

fn node_app_name(props: &serde_json::Value) -> String {
    ["application.name", "application.process.binary", "node.name"]
        .into_iter()
        .find_map(|key| props.get(key).and_then(serde_json::Value::as_str))
        .unwrap_or_default()
        .to_string()
}

fn prop_is_true(props: &serde_json::Value, key: &str) -> bool {
    match props.get(key) {
        Some(serde_json::Value::Bool(on)) => *on,
        Some(serde_json::Value::String(text)) => text == "true",
        _ => false,
    }
}

fn is_camera_source(props: &serde_json::Value) -> bool {
    let has = |key: &str| props.get(key).is_some();
    let name = props.get("node.name").and_then(serde_json::Value::as_str).unwrap_or_default();
    has("device.api")
        || has("api.v4l2.path")
        || has("api.libcamera.path")
        || name.starts_with("v4l2_")
        || name.starts_with("libcamera_")
}

fn tidy(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names.dedup();
    names
}

#[must_use]
pub fn parse_pw_dump(text: &str) -> Privacy {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Array(objects)) => privacy_from(&objects),
        _ => Privacy::default(),
    }
}

#[must_use]
pub fn privacy_from<'a>(objects: impl IntoIterator<Item = &'a serde_json::Value>) -> Privacy {
    let null = serde_json::Value::Null;
    let mut nodes = std::collections::HashMap::new();
    let mut links = Vec::new();
    for object in objects {
        let kind = object.get("type").and_then(serde_json::Value::as_str).unwrap_or_default();
        let id = object.get("id").and_then(serde_json::Value::as_u64);
        let info = object.get("info").unwrap_or(&null);
        match (kind, id) {
            ("PipeWire:Interface:Node", Some(id)) => {
                let props = info.get("props").unwrap_or(&null);
                let class = props.get("media.class").and_then(serde_json::Value::as_str).unwrap_or_default();
                let running = info.get("state").and_then(serde_json::Value::as_str) == Some("running");
                nodes.insert(id, (class, running, props));
            }
            ("PipeWire:Interface:Link", _) => {
                let output = info.get("output-node-id").and_then(serde_json::Value::as_u64);
                let input = info.get("input-node-id").and_then(serde_json::Value::as_u64);
                if let (Some(output), Some(input)) = (output, input) {
                    links.push((output, input));
                }
            }
            _ => {}
        }
    }

    let mut privacy = Privacy::default();
    for (class, running, props) in nodes.values() {
        if *class == "Stream/Input/Audio"
            && *running
            && !prop_is_true(props, "stream.capture.sink")
            && !prop_is_true(props, "stream.monitor")
        {
            privacy.microphone.push(node_app_name(props));
        }
    }
    for (id, (class, running, props)) in &nodes {
        if *class != "Video/Source" {
            continue;
        }
        let consumers: Vec<String> = links
            .iter()
            .filter(|(output, _)| output == id)
            .filter_map(|(_, input)| nodes.get(input))
            .filter(|(class, running, _)| class.starts_with("Stream/Input/Video") && *running)
            .map(|(_, _, props)| node_app_name(props))
            .collect();
        if consumers.is_empty() && !running {
            continue;
        }
        let list = if is_camera_source(props) { &mut privacy.camera } else { &mut privacy.screen };
        if consumers.is_empty() {
            list.push(String::new());
        } else {
            list.extend(consumers);
        }
    }
    privacy.microphone = tidy(privacy.microphone);
    privacy.camera = tidy(privacy.camera);
    privacy.screen = tidy(privacy.screen);
    privacy
}

fn camera_openers() -> Vec<String> {
    let has_camera = fs::read_dir("/sys/class/video4linux").is_ok_and(|mut devices| devices.next().is_some());
    let Some(processes) = has_camera.then(|| fs::read_dir("/proc").ok()).flatten() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for process in processes.flatten() {
        let path = process.path();
        if !process.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(fds) = fs::read_dir(path.join("fd")) else {
            continue;
        };
        let opened = fds
            .flatten()
            .any(|fd| fs::read_link(fd.path()).is_ok_and(|target| target.to_string_lossy().starts_with("/dev/video")));
        if opened {
            let comm = fs::read_to_string(path.join("comm")).unwrap_or_default();
            names.push(comm.trim().to_string());
        }
    }
    names
}

#[must_use]
pub fn merge_privacy(mut privacy: Privacy, openers: &[String]) -> Privacy {
    let openers = openers.iter().filter(|name| !matches!(name.as_str(), "pipewire" | "wireplumber"));
    let openers: Vec<String> = openers.cloned().collect();
    if !openers.is_empty() {
        privacy.camera.retain(|name| !name.is_empty());
        privacy.camera.extend(openers);
        privacy.camera = tidy(std::mem::take(&mut privacy.camera));
    }
    privacy
}

#[derive(Debug, Default)]
struct PrivacySources {
    pipewire: Privacy,
    openers: Vec<String>,
}

impl PrivacySources {
    fn refresh(&mut self, topics: Topics, pipewire: Option<Privacy>) -> Privacy {
        if topics.contains(Topics::PRIVACY) {
            self.pipewire = pipewire
                .unwrap_or_else(|| poll_stdout("pw-dump", &[]).map(|text| parse_pw_dump(&text)).unwrap_or_default());
        }
        if topics.contains(Topics::CAMERA) {
            self.openers = camera_openers();
        }
        merge_privacy(self.pipewire.clone(), &self.openers)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WifiLine {
    Network(String),
    Label(crate::i18n::Key),
}

#[must_use]
pub fn wifi_summary(radio: Option<bool>, level: u8, ssid: Option<String>) -> (bool, u8, WifiLine) {
    use crate::i18n::Key;
    match (radio, ssid) {
        (Some(false), _) => (false, 0, WifiLine::Label(Key::Off)),
        (_, Some(ssid)) => (true, level.max(1), WifiLine::Network(ssid)),
        (Some(true), None) => (true, 0, WifiLine::Label(Key::NotConnected)),
        (None, None) if level > 0 => (true, level, WifiLine::Network(String::new())),
        (None, None) => (false, 0, WifiLine::Label(Key::NotConnected)),
    }
}

#[must_use]
pub fn parse_radio_state(text: &str) -> Option<bool> {
    match text.trim() {
        state if state.eq_ignore_ascii_case("enabled") => Some(true),
        state if state.eq_ignore_ascii_case("disabled") => Some(false),
        _ => None,
    }
}

#[must_use]
pub fn parse_active_ssid(text: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix("yes:"))
        .find(|ssid| !ssid.is_empty())
        .map(|ssid| ssid.replace("\\:", ":").replace("\\\\", "\\"))
}

fn read_wifi() -> (bool, u8, String) {
    let radio = read_wifi_radio();
    let ssid = if radio == Some(false) { None } else { fetch_active_ssid() };
    let (enabled, level, line) = wifi_summary(radio, read_wifi_level(), ssid);
    let text = match line {
        WifiLine::Network(ssid) => ssid,
        WifiLine::Label(key) => crate::i18n::t(key).to_string(),
    };
    (enabled, level, text)
}

fn read_wifi_level() -> u8 {
    let mut level = 0u8;
    if let Ok(content) = fs::read_to_string("/proc/net/wireless") {
        for line in content.lines().skip(2) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let link_str = parts[2].trim_end_matches('.');
                if let Ok(quality) = link_str.parse::<f32>() {
                    let ratio = (quality / 70.0).clamp(0.0, 1.0);
                    level = if ratio < 0.1 {
                        1
                    } else if ratio < 0.4 {
                        2
                    } else if ratio < 0.7 {
                        3
                    } else {
                        4
                    };
                    break;
                }
            }
        }
    }

    if level == 0
        && let Ok(entries) = fs::read_dir("/sys/class/net")
    {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("wl") {
                let operstate = fs::read_to_string(entry.path().join("operstate")).unwrap_or_default();
                if operstate.trim().eq_ignore_ascii_case("up") {
                    level = 3;
                    break;
                }
            }
        }
    }

    level
}

fn read_wifi_radio() -> Option<bool> {
    parse_radio_state(&poll_stdout("nmcli", &["radio", "wifi"])?)
}

fn fetch_active_ssid() -> Option<String> {
    parse_active_ssid(&poll_stdout("nmcli", &["-t", "-f", "active,ssid", "dev", "wifi", "list", "--rescan", "no"])?)
}

#[derive(Debug, Clone, Copy)]
struct AudioEndpoint {
    wpctl: &'static str,
    pactl_volume: &'static str,
    pactl_mute: &'static str,
    pactl_set_volume: &'static str,
    pactl_set_mute: &'static str,
    pactl_target: &'static str,
}

const SINK: AudioEndpoint = AudioEndpoint {
    wpctl: "@DEFAULT_AUDIO_SINK@",
    pactl_volume: "get-sink-volume",
    pactl_mute: "get-sink-mute",
    pactl_set_volume: "set-sink-volume",
    pactl_set_mute: "set-sink-mute",
    pactl_target: "@DEFAULT_SINK@",
};

const SOURCE: AudioEndpoint = AudioEndpoint {
    wpctl: "@DEFAULT_AUDIO_SOURCE@",
    pactl_volume: "get-source-volume",
    pactl_mute: "get-source-mute",
    pactl_set_volume: "set-source-volume",
    pactl_set_mute: "set-source-mute",
    pactl_target: "@DEFAULT_SOURCE@",
};

#[must_use]
pub fn parse_wpctl_volume(text: &str) -> Option<(u8, bool)> {
    let level = text.trim().strip_prefix("Volume:")?;
    let pct = level
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f32>().ok())
        .map_or(50, |val| (val * 100.0).round().clamp(0.0, f32::from(u8::MAX)) as u8);
    Some((pct, level.contains("[MUTED]")))
}

#[must_use]
pub fn parse_pactl_volume(text: &str) -> Option<u8> {
    let percent = text.split('/').nth(1)?.trim().strip_suffix('%')?.trim().parse::<u32>().ok()?;
    Some(u8::try_from(percent).unwrap_or(u8::MAX))
}

fn read_level(endpoint: AudioEndpoint) -> Option<(u8, bool)> {
    if let Some(level) =
        poll_stdout("wpctl", &["get-volume", endpoint.wpctl]).and_then(|text| parse_wpctl_volume(&text))
    {
        return Some(level);
    }

    if let Some(text) = poll_stdout("pactl", &[endpoint.pactl_volume, endpoint.pactl_target]) {
        let pct = parse_pactl_volume(&text).unwrap_or(50);
        let muted = poll_stdout("pactl", &[endpoint.pactl_mute, endpoint.pactl_target])
            .is_some_and(|out| parse_pactl_mute(&out));
        return Some((pct, muted));
    }

    None
}

fn read_volume() -> Option<(u8, bool)> {
    read_level(SINK)
}

fn read_mic() -> Option<(u8, bool)> {
    read_level(SOURCE)
}

#[must_use]
pub fn parse_pactl_mute(text: &str) -> bool {
    text.trim().strip_prefix("Mute:").is_some_and(|value| value.trim() == "yes")
}

#[must_use]
pub fn backlight_type_rank(kind: &str) -> u8 {
    match kind.trim() {
        "firmware" => 0,
        "platform" => 1,
        "raw" => 2,
        _ => 3,
    }
}

struct Backlight {
    name: String,
    current: u64,
    max: u64,
}

impl Backlight {
    fn percent(&self) -> u8 {
        ((self.current as f64 * 100.0) / self.max as f64).round().clamp(0.0, 100.0) as u8
    }
}

fn read_backlight() -> Option<Backlight> {
    let entries = fs::read_dir(Path::new("/sys/class/backlight")).ok()?;
    let mut found: Vec<(u8, Backlight)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let read = |file: &str| fs::read_to_string(path.join(file)).ok().and_then(|s| s.trim().parse::<u64>().ok());
            let max = read("max_brightness").filter(|max| *max > 0)?;
            let current = read("brightness")?;
            let rank = fs::read_to_string(path.join("type")).map_or(3, |kind| backlight_type_rank(&kind));
            let name = entry.file_name().to_string_lossy().into_owned();
            Some((rank, Backlight { name, current, max }))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
    found.into_iter().next().map(|(_, backlight)| backlight)
}

#[must_use]
pub fn read_keyboard_backlight(leds: &Path) -> Option<u8> {
    let mut entries: Vec<_> = fs::read_dir(leds)
        .ok()?
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with("::kbd_backlight"))
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    entries.iter().find_map(|path| {
        let read = |file: &str| fs::read_to_string(path.join(file)).ok().and_then(|s| s.trim().parse::<u64>().ok());
        let max = read("max_brightness").filter(|max| *max > 0)?;
        let current = read("brightness")?;
        Some(((current as f64 * 100.0) / max as f64).round().clamp(0.0, 100.0) as u8)
    })
}

#[must_use]
pub fn parse_bluetooth_powered(text: &str) -> Option<bool> {
    text.lines().find_map(|line| line.trim().strip_prefix("Powered:")).map(|value| value.trim() == "yes")
}

#[must_use]
pub fn bluetooth_on(controller: Option<&str>, rfkill_unblocked: impl FnOnce() -> bool) -> bool {
    controller.and_then(parse_bluetooth_powered).unwrap_or_else(rfkill_unblocked)
}

fn rfkill_bluetooth_unblocked() -> bool {
    let rfkill_path = Path::new("/sys/class/rfkill");
    let Ok(entries) = fs::read_dir(rfkill_path) else {
        return false;
    };

    let mut found_bt = false;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_bt = fs::read_to_string(path.join("type")).is_ok_and(|t| t.trim().eq_ignore_ascii_case("bluetooth"));
        if is_bt {
            found_bt = true;
            let soft = fs::read_to_string(path.join("soft")).is_ok_and(|s| s.trim() == "1");
            let hard = fs::read_to_string(path.join("hard")).is_ok_and(|s| s.trim() == "1");
            if soft || hard {
                return false;
            }
        }
    }

    found_bt
}

fn read_bluetooth() -> bool {
    if fs::read_dir("/sys/class/bluetooth").map_or(true, |mut adapters| adapters.next().is_none()) {
        return false;
    }
    bluetooth_on(poll_stdout("bluetoothctl", &["show"]).as_deref(), rfkill_bluetooth_unblocked)
}

pub const FAST_INTERVAL: Duration = Duration::from_secs(1);
pub const SLOW_INTERVAL: Duration = Duration::from_secs(6);
pub const BATTERY_DRIFT: Duration = Duration::from_secs(30);
pub const NETWORK_SPACING: Duration = Duration::from_secs(1);
const POKE_SETTLE: Duration = Duration::from_millis(120);
const TOPIC_SLOTS: usize = 16;

#[derive(Debug, Default)]
struct Schedule {
    last: [Option<Instant>; TOPIC_SLOTS],
}

const fn slot(topic: Topics) -> usize {
    topic.bits().trailing_zeros() as usize
}

impl Schedule {
    fn interval(topic: Topics, hooked: Topics) -> Option<Duration> {
        if hooked.contains(topic) {
            (topic == Topics::BATTERY).then_some(BATTERY_DRIFT)
        } else if Topics::FAST.contains(topic) {
            Some(FAST_INTERVAL)
        } else {
            Some(SLOW_INTERVAL)
        }
    }

    fn spacing(topic: Topics) -> Duration {
        if topic == Topics::NETWORK { NETWORK_SPACING } else { Duration::ZERO }
    }

    fn due(&self, hooked: Topics, now: Instant) -> Topics {
        Topics::all()
            .iter()
            .filter(|topic| match self.last[slot(*topic)] {
                None => true,
                Some(at) => Self::interval(*topic, hooked).is_some_and(|interval| now >= at + interval),
            })
            .collect()
    }

    fn held(&self, dirty: Topics, now: Instant) -> Topics {
        dirty
            .iter()
            .filter(|topic| self.last[slot(*topic)].is_some_and(|at| now < at + Self::spacing(*topic)))
            .collect()
    }

    fn next(&self, hooked: Topics, held: Topics) -> Option<Instant> {
        let polled = Topics::all().iter().filter_map(|topic| {
            let at = self.last[slot(topic)]?;
            Some(at + Self::interval(topic, hooked)?)
        });
        let spaced = held.iter().filter_map(|topic| Some(self.last[slot(topic)]? + Self::spacing(topic)));
        polled.chain(spaced).min()
    }

    fn done(&mut self, topics: Topics, now: Instant) {
        for topic in topics.iter() {
            self.last[slot(topic)] = Some(now);
        }
    }
}

#[derive(Debug)]
struct Hub {
    dirty: Topics,
    hooked: Topics,
    pipewire: Option<Privacy>,
}

static HUB: (Mutex<Hub>, Condvar) =
    (Mutex::new(Hub { dirty: Topics::empty(), hooked: Topics::empty(), pipewire: None }), Condvar::new());

fn hub() -> MutexGuard<'static, Hub> {
    HUB.0.lock().unwrap_or_else(PoisonError::into_inner)
}

fn edit_hub(edit: impl FnOnce(&mut Hub)) {
    edit(&mut hub());
    HUB.1.notify_one();
}

pub fn poke(topics: Topics) {
    edit_hub(|hub| hub.dirty |= topics);
}

pub fn hook(topics: Topics) {
    edit_hub(|hub| {
        hub.hooked |= topics;
        hub.dirty |= topics;
    });
}

pub fn unhook(topics: Topics) {
    edit_hub(|hub| {
        hub.hooked -= topics;
        hub.dirty |= topics;
        if topics.contains(Topics::PRIVACY) {
            hub.pipewire = None;
        }
    });
}

pub fn publish_pipewire(privacy: Privacy) {
    edit_hub(|hub| {
        hub.pipewire = Some(privacy);
        hub.dirty |= Topics::PRIVACY;
    });
}

struct Batch {
    topics: Topics,
    pipewire: Option<Privacy>,
}

fn next_batch(schedule: &Schedule) -> Batch {
    let mut hub = hub();
    let mut settled = false;
    loop {
        let now = Instant::now();
        let held = schedule.held(hub.dirty, now);
        let ready = hub.dirty - held;
        if !ready.is_empty() && !settled {
            drop(hub);
            std::thread::sleep(POKE_SETTLE);
            hub = self::hub();
            settled = true;
            continue;
        }
        let topics = ready | schedule.due(hub.hooked, now);
        if !topics.is_empty() {
            hub.dirty -= topics;
            let pipewire = if hub.hooked.contains(Topics::PRIVACY) { hub.pipewire.clone() } else { None };
            return Batch { topics, pipewire };
        }
        hub = match schedule.next(hub.hooked, held) {
            Some(at) => HUB
                .1
                .wait_timeout(hub, at.saturating_duration_since(now))
                .map_or_else(|poisoned| poisoned.into_inner().0, |(guard, _)| guard),
            None => HUB.1.wait(hub).unwrap_or_else(PoisonError::into_inner),
        };
    }
}

pub fn start_poller(sink: impl Fn(SystemStatus) + Send + 'static) {
    let spawned = std::thread::Builder::new().name("glide-system-status".to_string()).spawn(move || {
        let mut status = SystemStatus { user_name: read_user_name(), ..SystemStatus::default() };
        let mut schedule = Schedule::default();
        let mut privacy = PrivacySources::default();
        let mut sent = None;
        loop {
            let batch = next_batch(&schedule);
            refresh(&mut status, batch.topics, &mut privacy, batch.pipewire);
            schedule.done(batch.topics, Instant::now());
            if sent.as_ref() != Some(&status) {
                sink(status.clone());
                sent = Some(status.clone());
            }
        }
    });
    if let Err(err) = spawned {
        tracing::warn!("system status poller did not start: {err}");
    }
}

fn read_user_name() -> String {
    let full_name = read_gecos_name().filter(|name| !name.is_empty());
    if let Some(name) = full_name {
        return name;
    }
    let user = std::env::var("USER").unwrap_or_else(|_| crate::i18n::t(crate::i18n::Key::User).to_string());
    let mut chars = user.chars();
    chars.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + chars.as_str())
}

fn read_gecos_name() -> Option<String> {
    // SAFETY: `getpwuid` returns a pointer into libc-owned static storage (or
    // null); the pointer is only dereferenced after the null check, and the
    // GECOS field is copied out before any other libc call can overwrite it.
    let gecos = unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_gecos.is_null() {
            return None;
        }
        std::ffi::CStr::from_ptr((*entry).pw_gecos).to_string_lossy().into_owned()
    };
    let full = gecos.split(',').next().unwrap_or_default().trim();
    (!full.is_empty()).then(|| full.to_string())
}

fn run(program: &str, args: &[&str]) -> std::io::Result<u32> {
    let mut command = Command::new(program);
    command.args(args);
    crate::spawn_detached(command)
}

pub fn set_wifi(on: bool) {
    let _ = run("nmcli", &["radio", "wifi", if on { "on" } else { "off" }]);
}

pub fn set_bluetooth(on: bool) {
    let command = if on {
        "rfkill unblock bluetooth; sleep 0.5; timeout 10 bluetoothctl power on"
    } else {
        "timeout 10 bluetoothctl power off; rfkill block bluetooth"
    };
    if let Err(err) = crate::spawn_shell_command(command) {
        tracing::warn!("bluetooth toggle failed to start: {err}");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Volume,
    ToggleMute,
    MicVolume,
    ToggleMicMute,
    Brightness,
}

pub const CONTROL_APPLY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PendingControls {
    volume: Option<u8>,
    toggle_mute: bool,
    mic: Option<u8>,
    toggle_mic_mute: bool,
    brightness: Option<u8>,
}

impl PendingControls {
    pub fn push(&mut self, control: Control, value: u8) {
        match control {
            Control::Volume => {
                self.volume = Some(value);
                self.toggle_mute = false;
            }
            Control::ToggleMute => self.toggle_mute = !self.toggle_mute,
            Control::MicVolume => {
                self.mic = Some(value);
                self.toggle_mic_mute = false;
            }
            Control::ToggleMicMute => self.toggle_mic_mute = !self.toggle_mic_mute,
            Control::Brightness => self.brightness = Some(value),
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.volume.is_none()
            && !self.toggle_mute
            && self.mic.is_none()
            && !self.toggle_mic_mute
            && self.brightness.is_none()
    }

    pub fn drain(&mut self) -> Vec<(Control, u8)> {
        let volume = self.volume.take().map(|value| (Control::Volume, value));
        let mute = std::mem::take(&mut self.toggle_mute).then_some((Control::ToggleMute, 0));
        let mic = self.mic.take().map(|value| (Control::MicVolume, value));
        let mic_mute = std::mem::take(&mut self.toggle_mic_mute).then_some((Control::ToggleMicMute, 0));
        let brightness = self.brightness.take().map(|value| (Control::Brightness, value));
        volume.into_iter().chain(mute).chain(mic).chain(mic_mute).chain(brightness).collect()
    }
}

#[must_use]
pub fn next_apply_delay(
    last_applied: Option<std::time::Instant>, now: std::time::Instant, interval: std::time::Duration,
) -> std::time::Duration {
    last_applied.map_or(std::time::Duration::ZERO, |last| interval.saturating_sub(now.saturating_duration_since(last)))
}

pub fn run_control_worker(
    requests: &std::sync::mpsc::Receiver<(Control, u8)>, interval: std::time::Duration,
    mut apply: impl FnMut(Control, u8),
) {
    use std::sync::mpsc::RecvTimeoutError;
    let mut pending = PendingControls::default();
    let mut last_applied = None;
    loop {
        if pending.is_empty() {
            match requests.recv() {
                Ok((control, value)) => pending.push(control, value),
                Err(_) => return,
            }
        }
        let deadline = std::time::Instant::now() + next_apply_delay(last_applied, std::time::Instant::now(), interval);
        let mut disconnected = false;
        loop {
            let now = std::time::Instant::now();
            if now >= deadline {
                break;
            }
            match requests.recv_timeout(deadline - now) {
                Ok((control, value)) => pending.push(control, value),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        while let Ok((control, value)) = requests.try_recv() {
            pending.push(control, value);
        }
        for (control, value) in pending.drain() {
            apply(control, value);
        }
        last_applied = Some(std::time::Instant::now());
        if disconnected {
            return;
        }
    }
}

static CONTROL_REQUESTS: std::sync::OnceLock<Option<std::sync::mpsc::Sender<(Control, u8)>>> =
    std::sync::OnceLock::new();

fn request_control(control: Control, value: u8) {
    let sender = CONTROL_REQUESTS.get_or_init(|| {
        if cfg!(test) {
            return None;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("glide-controls".to_string())
            .spawn(move || {
                run_control_worker(&rx, CONTROL_APPLY_INTERVAL, |control, value| match control {
                    Control::Volume => apply_level(SINK, value),
                    Control::ToggleMute => apply_mute_toggle(SINK),
                    Control::MicVolume => apply_level(SOURCE, value),
                    Control::ToggleMicMute => apply_mute_toggle(SOURCE),
                    Control::Brightness => apply_brightness_pct(value),
                });
            })
            .inspect_err(|err| tracing::warn!("control worker did not start: {err}"))
            .ok()
            .map(|_| tx)
    });
    if let Some(sender) = sender {
        let _ = sender.send((control, value));
    }
}

fn run_blocking(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn run_blocking_or(primary: (&str, &[&str]), fallback: (&str, &[&str])) {
    if !run_blocking(primary.0, primary.1) {
        run_blocking(fallback.0, fallback.1);
    }
}

pub fn set_volume_pct(pct: u8) {
    request_control(Control::Volume, pct);
}

pub fn set_mic_pct(pct: u8) {
    request_control(Control::MicVolume, pct);
}

fn apply_level(endpoint: AudioEndpoint, pct: u8) {
    let frac = (f32::from(pct) / 100.0).clamp(0.0, 1.5);
    let frac = format!("{frac:.2}");
    let percent = format!("{pct}%");
    run_blocking_or(
        ("wpctl", &["set-volume", endpoint.wpctl, &frac]),
        ("pactl", &[endpoint.pactl_set_volume, endpoint.pactl_target, &percent]),
    );
    let mute = if pct > 0 { "0" } else { "1" };
    run_blocking_or(
        ("wpctl", &["set-mute", endpoint.wpctl, mute]),
        ("pactl", &[endpoint.pactl_set_mute, endpoint.pactl_target, mute]),
    );
}

pub fn toggle_volume_mute() {
    request_control(Control::ToggleMute, 0);
}

pub fn toggle_mic_mute() {
    request_control(Control::ToggleMicMute, 0);
}

fn apply_mute_toggle(endpoint: AudioEndpoint) {
    run_blocking_or(
        ("wpctl", &["set-mute", endpoint.wpctl, "toggle"]),
        ("pactl", &[endpoint.pactl_set_mute, endpoint.pactl_target, "toggle"]),
    );
}

pub fn set_brightness_pct(pct: u8) {
    request_control(Control::Brightness, pct);
}

fn apply_brightness_pct(pct: u8) {
    if let Some(backlight) = read_backlight() {
        let val = ((u64::from(pct) * backlight.max) / 100).max(1).to_string();
        let applied = run_blocking(
            "busctl",
            &[
                "call",
                "org.freedesktop.login1",
                "/org/freedesktop/login1/session/auto",
                "org.freedesktop.login1.Session",
                "SetBrightness",
                "ssu",
                "backlight",
                &backlight.name,
                &val,
            ],
        );
        if applied {
            return;
        }
        let percent = format!("{pct}%");
        if run_blocking("brightnessctl", &["--device", &backlight.name, "set", &percent]) {
            return;
        }
    }

    if !run_blocking("brightnessctl", &["set", &format!("{pct}%")]) {
        tracing::debug!("no backlight control succeeded");
    }
}

#[must_use]
pub fn reduced_motion() -> bool {
    if std::env::var("GLIDE_REDUCED_MOTION").is_ok_and(|value| value == "1") {
        return true;
    }
    poll_stdout("gsettings", &["get", "org.gnome.desktop.interface", "enable-animations"])
        .is_some_and(|text| text.trim() == "false")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopAppearance {
    pub appearance: glide_theme::Appearance,
    pub accent: glide_theme::Accent,
    pub icon_theme: Option<String>,
    pub cursor_theme: Option<String>,
    pub cursor_size: Option<u32>,
    pub font_family: Option<String>,
    pub wallpaper: Option<std::path::PathBuf>,
    pub toolkits: glide_config::toolkits::Support,
}

const INTERFACE: &str = "org.gnome.desktop.interface";
const BACKGROUND: &str = "org.gnome.desktop.background";
pub const INTERFACE_FONT_POINTS: u32 = 11;

#[must_use]
pub fn gvariant_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[must_use]
pub fn file_uri(path: &Path) -> Option<String> {
    path.is_absolute().then(|| glide_config::uri::file_uri(path))
}

#[must_use]
pub fn gsettings_extras(wanted: &DesktopAppearance) -> Vec<(&'static str, &'static str, String)> {
    fn named(value: Option<&String>) -> Option<&str> {
        value.map(|value| value.trim()).filter(|value| !value.is_empty())
    }
    let mut extras = Vec::new();
    if let Some(theme) = named(wanted.cursor_theme.as_ref()) {
        extras.push((INTERFACE, "cursor-theme", gvariant_string(theme)));
    }
    if let Some(size) = wanted.cursor_size {
        extras.push((INTERFACE, "cursor-size", size.to_string()));
    }
    if let Some(family) = named(wanted.font_family.as_ref()) {
        extras.push((INTERFACE, "font-name", gvariant_string(&format!("{family} {INTERFACE_FONT_POINTS}"))));
    }
    if let Some(uri) = wanted.wallpaper.as_deref().and_then(file_uri) {
        extras.push((BACKGROUND, "picture-uri", gvariant_string(&uri)));
        extras.push((BACKGROUND, "picture-uri-dark", gvariant_string(&uri)));
    }
    extras
}

#[must_use]
pub fn activation_environment(wanted: &DesktopAppearance) -> Vec<String> {
    let mut vars = Vec::new();
    if let Some(theme) = wanted.cursor_theme.as_deref().map(str::trim).filter(|theme| !theme.is_empty()) {
        vars.push(format!("XCURSOR_THEME={theme}"));
    }
    if let Some(size) = wanted.cursor_size {
        vars.push(format!("XCURSOR_SIZE={size}"));
    }
    vars
}

#[must_use]
pub const fn gnome_color_scheme(appearance: glide_theme::Appearance) -> &'static str {
    match appearance {
        glide_theme::Appearance::Light => "prefer-light",
        glide_theme::Appearance::Dark => "prefer-dark",
    }
}

#[must_use]
pub const fn gnome_accent(accent: glide_theme::Accent) -> &'static str {
    match accent {
        glide_theme::Accent::Blue => "blue",
        glide_theme::Accent::Teal => "teal",
        glide_theme::Accent::Green => "green",
        glide_theme::Accent::Amber => "yellow",
        glide_theme::Accent::Orange => "orange",
        glide_theme::Accent::Red => "red",
        glide_theme::Accent::Purple => "purple",
        glide_theme::Accent::Graphite => "slate",
    }
}

#[must_use]
pub fn gtk_theme_for(current: &str) -> Option<&'static str> {
    let current = current.trim().trim_matches('\'');
    matches!(current, "" | "Adwaita" | "Adwaita-dark" | "adwaita" | "adwaita-dark").then_some("Adwaita")
}

static SYNCED_APPEARANCE: std::sync::Mutex<Option<DesktopAppearance>> = std::sync::Mutex::new(None);

fn gsettings_set(key: &str, value: &str) {
    gsettings_set_in(INTERFACE, key, value);
}

fn gsettings_set_in(schema: &str, key: &str, value: &str) {
    let _ = Command::new("gsettings")
        .args(["set", schema, key, value])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

pub fn sync_desktop_appearance(wanted: DesktopAppearance) {
    if cfg!(test) {
        return;
    }
    {
        let Ok(mut last) = SYNCED_APPEARANCE.lock() else {
            return;
        };
        if last.as_ref() == Some(&wanted) {
            return;
        }
        *last = Some(wanted.clone());
    }
    let worker = APPEARANCE_WORKER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<DesktopAppearance>();
        std::thread::Builder::new()
            .name("glide-gsettings".to_string())
            .spawn(move || {
                while let Ok(first) = rx.recv() {
                    apply_desktop_appearance(&latest(first, &rx));
                }
            })
            .inspect_err(|err| tracing::warn!("the GSettings worker did not start: {err}"))
            .ok()
            .map(|_| tx)
    });
    if let Some(worker) = worker {
        let _ = worker.send(wanted);
    }
}

static APPEARANCE_WORKER: std::sync::OnceLock<Option<std::sync::mpsc::Sender<DesktopAppearance>>> =
    std::sync::OnceLock::new();

fn latest<T>(first: T, queue: &std::sync::mpsc::Receiver<T>) -> T {
    queue.try_iter().last().unwrap_or(first)
}

fn apply_desktop_appearance(wanted: &DesktopAppearance) {
    gsettings_set("color-scheme", gnome_color_scheme(wanted.appearance));
    gsettings_set("accent-color", gnome_accent(wanted.accent));
    let current = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "gtk-theme"])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default();
    let theme = crate::theme_export::gtk3_theme(&current, wanted.appearance, wanted.toolkits)
        .or_else(|| gtk_theme_for(&current).map(ToString::to_string));
    if let Some(theme) = theme.as_deref() {
        gsettings_set("gtk-theme", theme);
    }
    crate::theme_export::export(wanted, theme.as_deref());
    if let Some(icons) = wanted.icon_theme.as_deref().filter(|name| !name.is_empty()) {
        gsettings_set("icon-theme", icons);
    }
    for (schema, key, value) in gsettings_extras(wanted) {
        gsettings_set_in(schema, key, &value);
    }
    let mut environment = activation_environment(wanted);
    if std::env::var_os("QT_QPA_PLATFORMTHEME").is_none()
        && let Some(theme) = crate::theme_export::qt_platform_theme()
    {
        environment.push(format!("QT_QPA_PLATFORMTHEME={theme}"));
    }
    if let Some(style) = crate::theme_export::qt_style_override() {
        environment.push(format!("QT_STYLE_OVERRIDE={style}"));
    }
    if !environment.is_empty() {
        let _ = Command::new("dbus-update-activation-environment")
            .arg("--systemd")
            .args(&environment)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    tracing::info!(
        appearance = ?wanted.appearance,
        accent = ?wanted.accent,
        "synced desktop appearance to GSettings"
    );
}

pub fn open_settings() {
    let _ = crate::spawn_shell_command(
        "glide-settings || gnome-control-center || systemsettings || xfce4-settings-manager",
    );
}

pub fn open_power_settings() {
    if let Err(err) = run("glide-settings", &["power"]) {
        tracing::warn!("could not open glide-settings power: {err}");
    }
}

pub fn open_storage_settings() {
    if let Err(err) = run("glide-settings", &["storage"]) {
        tracing::warn!("could not open glide-settings storage: {err}");
    }
}

fn run_power_command(command: &str) {
    if let Err(err) = crate::spawn_shell_command(command) {
        tracing::warn!(command, "power command failed to start: {err}");
    }
}

pub fn power_off() {
    run_power_command("systemctl poweroff || loginctl poweroff");
}

pub fn reboot() {
    run_power_command("systemctl reboot || loginctl reboot");
}

pub fn suspend() {
    run_power_command("systemctl suspend || loginctl suspend");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn the_gsettings_worker_applies_only_the_newest_queued_appearance() {
        let (tx, rx) = std::sync::mpsc::channel();
        assert_eq!(latest("dark", &rx), "dark");
        for wanted in ["light", "dark", "light"] {
            tx.send(wanted).unwrap();
        }
        assert_eq!(latest("dark", &rx), "light", "toggling twice quickly ends where the user stopped");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_burst_keeps_the_latest_value_per_control_and_mutes_stay_ordered() {
        let mut pending = PendingControls::default();
        for value in [10, 20, 30, 40] {
            pending.push(Control::Volume, value);
        }
        pending.push(Control::Brightness, 70);
        pending.push(Control::Brightness, 55);
        assert_eq!(pending.drain(), vec![(Control::Volume, 40), (Control::Brightness, 55)]);
        assert!(pending.is_empty() && pending.drain().is_empty());

        pending.push(Control::Volume, 40);
        pending.push(Control::ToggleMute, 0);
        assert_eq!(pending.drain(), vec![(Control::Volume, 40), (Control::ToggleMute, 0)]);
        pending.push(Control::ToggleMute, 0);
        pending.push(Control::Volume, 25);
        assert_eq!(pending.drain(), vec![(Control::Volume, 25)], "a volume supersedes a mute");
        pending.push(Control::Volume, 60);
        pending.push(Control::ToggleMute, 0);
        pending.push(Control::ToggleMute, 0);
        assert_eq!(pending.drain(), vec![(Control::Volume, 60)], "two toggles cancel out");
        pending.push(Control::ToggleMute, 0);
        pending.push(Control::ToggleMute, 0);
        assert!(pending.is_empty());
    }

    #[test]
    fn the_microphone_queues_beside_the_speakers_without_touching_them() {
        let mut pending = PendingControls::default();
        pending.push(Control::ToggleMute, 0);
        pending.push(Control::MicVolume, 30);
        pending.push(Control::MicVolume, 45);
        assert_eq!(pending.drain(), vec![(Control::ToggleMute, 0), (Control::MicVolume, 45)]);
        pending.push(Control::ToggleMicMute, 0);
        pending.push(Control::MicVolume, 20);
        assert_eq!(pending.drain(), vec![(Control::MicVolume, 20)], "a level supersedes a mute");
        pending.push(Control::ToggleMicMute, 0);
        assert!(!pending.is_empty());
        assert_eq!(pending.drain(), vec![(Control::ToggleMicMute, 0)]);
    }

    #[test]
    fn a_wpctl_level_needs_its_volume_line() {
        assert_eq!(parse_wpctl_volume("Volume: 0.65\n"), Some((65, false)));
        assert_eq!(parse_wpctl_volume("Volume: 0.40 [MUTED]\n"), Some((40, true)));
        assert_eq!(parse_wpctl_volume("Volume: 1.30\n"), Some((130, false)));
        assert_eq!(parse_wpctl_volume("Volume: 1.50 [MUTED]\n"), Some((150, true)));
        assert_eq!(parse_wpctl_volume("Volume: 9.00\n"), Some((255, false)), "capped at what a u8 holds");
        assert_eq!(parse_wpctl_volume("Node '99999' not found\n"), None, "wpctl exits 0 for a missing node");
        assert_eq!(parse_wpctl_volume(""), None);
    }

    #[test]
    fn a_pactl_level_above_the_u8_range_saturates_instead_of_reporting_fifty() {
        let line = |pct: u32| {
            format!("Volume: front-left: 65536 / {pct}% / 0.00 dB,   front-right: 65536 / {pct}% / 0.00 dB\n")
        };
        assert_eq!(parse_pactl_volume(&line(65)), Some(65));
        assert_eq!(parse_pactl_volume(&line(130)), Some(130));
        assert_eq!(parse_pactl_volume(&line(255)), Some(255));
        assert_eq!(parse_pactl_volume(&line(300)), Some(255));
        assert_eq!(parse_pactl_volume("Volume: mono: 0 / 0% / -inf dB\n"), Some(0));
        assert_eq!(parse_pactl_volume("Volume: unreadable\n"), None);
        assert_eq!(parse_pactl_volume(""), None);
    }

    #[test]
    fn bluez_decides_when_it_answers_and_rfkill_only_when_it_does_not() {
        let off = "Controller AA:BB:CC:DD:EE:FF (public)\n\tName: laptop\n\tPowered: no\n\tPowerState: off\n";
        let on = "Controller AA:BB:CC:DD:EE:FF (public)\n\tName: laptop\n\tPowered: yes\n\tPowerState: on\n";
        assert!(!bluetooth_on(Some(off), || true), "an explicit Powered: no is not overridden by an unblocked switch");
        assert!(bluetooth_on(Some(on), || false), "BlueZ saying yes wins over a stale rfkill scan");
        assert!(bluetooth_on(None, || true), "bluetoothctl gave no answer: the rfkill state decides");
        assert!(!bluetooth_on(None, || false));
        assert!(bluetooth_on(Some("No default controller available\n"), || true));
        assert!(!bluetooth_on(Some("PowerState: on\n"), || false), "PowerState is not the Powered line");
        assert!(!bluetooth_on(Some(off), || panic!("rfkill must not be read when BlueZ answered")));
        assert_eq!(parse_bluetooth_powered(on), Some(true));
        assert_eq!(parse_bluetooth_powered(off), Some(false));
        assert_eq!(parse_bluetooth_powered(""), None);
    }

    #[test]
    fn the_worker_applies_a_queued_burst_once_with_the_final_value() {
        let (tx, rx) = std::sync::mpsc::channel();
        for value in 0..50 {
            tx.send((Control::Volume, value)).unwrap();
        }
        tx.send((Control::Brightness, 5)).unwrap();
        tx.send((Control::Brightness, 9)).unwrap();
        drop(tx);
        let mut applied = Vec::new();
        run_control_worker(&rx, Duration::from_millis(1), |control, value| {
            applied.push((control, value));
        });
        assert_eq!(applied, vec![(Control::Volume, 49), (Control::Brightness, 9)]);
    }

    #[test]
    fn the_worker_never_applies_a_stale_volume_after_a_mute() {
        let (tx, rx) = std::sync::mpsc::channel();
        for value in [10, 20, 30] {
            tx.send((Control::Volume, value)).unwrap();
        }
        tx.send((Control::ToggleMute, 0)).unwrap();
        drop(tx);
        let mut applied = Vec::new();
        run_control_worker(&rx, Duration::from_millis(1), |control, value| {
            applied.push((control, value));
        });
        assert_eq!(applied, vec![(Control::Volume, 30), (Control::ToggleMute, 0)]);
    }

    #[test]
    fn applies_are_spaced_by_the_interval() {
        let start = Instant::now();
        let interval = Duration::from_millis(33);
        assert_eq!(next_apply_delay(None, start, interval), Duration::ZERO);
        assert_eq!(
            next_apply_delay(Some(start), start + Duration::from_millis(10), interval),
            Duration::from_millis(23)
        );
        assert_eq!(next_apply_delay(Some(start), start + Duration::from_millis(50), interval), Duration::ZERO);
    }

    #[test]
    fn command_outputs_parse_to_their_states() {
        assert!(parse_pactl_mute("Mute: yes\n"));
        assert!(!parse_pactl_mute("Mute: no\n") && !parse_pactl_mute("Stumm: ja\n"));
        let rank = backlight_type_rank;
        assert!(rank("firmware\n") < rank("platform") && rank("platform") < rank("raw"));
        assert!(rank("raw") < rank("bogus"));
        assert_eq!(parse_radio_state("enabled\n"), Some(true));
        assert_eq!(parse_radio_state("disabled\n"), Some(false));
        assert_eq!(parse_radio_state(""), None);
        let listing = "no:Neighbour\nyes:\nyes:Home\\:5G\n";
        assert_eq!(parse_active_ssid(listing), Some("Home:5G".to_string()));
        assert_eq!(parse_active_ssid("no:Other\n"), None);
        assert!(parse_vpn_active("802-11-wireless:activated\nvpn:activated\n"));
        assert!(parse_vpn_active("wireguard:activated\n"));
        assert!(!parse_vpn_active("vpn:activating\n802-3-ethernet:activated\n"));
        assert!(!parse_vpn_active(""));
        assert!(parse_connected_devices("Device 00:11:22:33:44:55 WH-1000XM4\n"));
        assert!(!parse_connected_devices(""));
        assert!(!parse_connected_devices("No default controller available\n"));
    }

    #[test]
    fn an_enabled_radio_without_a_network_is_on_but_not_connected() {
        use crate::i18n::Key;
        assert_eq!(wifi_summary(Some(true), 0, None), (true, 0, WifiLine::Label(Key::NotConnected)));
        assert_eq!(wifi_summary(Some(false), 3, None), (false, 0, WifiLine::Label(Key::Off)));
        assert_eq!(wifi_summary(Some(true), 0, Some("Home".into())), (true, 1, WifiLine::Network("Home".into())));
        assert_eq!(wifi_summary(None, 3, None), (true, 3, WifiLine::Network(String::new())));
    }

    const PW_DUMP: &str = r#"[
        {"id": 30, "type": "PipeWire:Interface:Node", "info": {"state": "running",
            "props": {"media.class": "Audio/Source", "node.name": "alsa_input"}}},
        {"id": 40, "type": "PipeWire:Interface:Node", "info": {"state": "running",
            "props": {"media.class": "Stream/Input/Audio", "application.name": "Firefox"}}},
        {"id": 41, "type": "PipeWire:Interface:Node", "info": {"state": "running",
            "props": {"media.class": "Stream/Input/Audio", "application.name": "Meter",
                      "stream.capture.sink": true}}},
        {"id": 42, "type": "PipeWire:Interface:Node", "info": {"state": "suspended",
            "props": {"media.class": "Stream/Input/Audio", "application.name": "Idle"}}},
        {"id": 50, "type": "PipeWire:Interface:Node", "info": {"state": "running",
            "props": {"media.class": "Video/Source", "node.name": "v4l2_input.pci",
                      "device.api": "v4l2"}}},
        {"id": 51, "type": "PipeWire:Interface:Node", "info": {"state": "running",
            "props": {"media.class": "Stream/Input/Video", "application.name": "Zoom"}}},
        {"id": 60, "type": "PipeWire:Interface:Node", "info": {"state": "running",
            "props": {"media.class": "Video/Source", "node.name": "xdpw_stream"}}},
        {"id": 61, "type": "PipeWire:Interface:Node", "info": {"state": "running",
            "props": {"media.class": "Stream/Input/Video", "application.process.binary": "obs"}}},
        {"id": 70, "type": "PipeWire:Interface:Link",
            "info": {"output-node-id": 50, "input-node-id": 51, "state": "active"}},
        {"id": 71, "type": "PipeWire:Interface:Link",
            "info": {"output-node-id": 60, "input-node-id": 61, "state": "active"}},
        {"id": 80, "type": "PipeWire:Interface:Client", "info": {"props": {}}}
    ]"#;

    #[test]
    fn pw_dump_attributes_running_capture_streams_to_their_apps() {
        let privacy = parse_pw_dump(PW_DUMP);
        assert_eq!(privacy.microphone, vec!["Firefox".to_string()]);
        assert_eq!(privacy.camera, vec!["Zoom".to_string()]);
        assert_eq!(privacy.screen, vec!["obs".to_string()]);
        let idle = PW_DUMP.replace("\"running\"", "\"idle\"");
        for quiet in ["[]", "not json", idle.as_str()] {
            assert_eq!(parse_pw_dump(quiet), Privacy::default());
        }
    }

    #[test]
    fn a_running_source_without_a_known_consumer_still_counts() {
        let lonely = r#"[{"id": 5, "type": "PipeWire:Interface:Node", "info": {"state": "running",
            "props": {"media.class": "Video/Source", "node.name": "libcamera_input"}}}]"#;
        assert_eq!(parse_pw_dump(lonely).camera, vec![String::new()]);
    }

    #[test]
    fn camera_openers_name_the_camera_unless_they_are_pipewire_itself() {
        let pipewire = Privacy { camera: vec![String::new()], ..Privacy::default() };
        let openers = ["zoom".to_string(), "wireplumber".to_string(), "zoom".to_string()];
        assert_eq!(merge_privacy(pipewire.clone(), &openers).camera, vec!["zoom".to_string()]);
        let plumbing = ["pipewire".to_string()];
        assert_eq!(merge_privacy(pipewire.clone(), &plumbing), pipewire, "PipeWire probing its own devices");
    }

    #[test]
    fn unhooked_topics_poll_at_the_old_rates_and_hooked_ones_wait_for_their_hook() {
        let start = Instant::now();
        let mut schedule = Schedule::default();
        assert_eq!(schedule.due(Topics::empty(), start), Topics::all(), "the first sample reads everything");
        schedule.done(Topics::all(), start);
        assert_eq!(schedule.due(Topics::empty(), start + FAST_INTERVAL), Topics::FAST);
        assert_eq!(schedule.due(Topics::empty(), start + SLOW_INTERVAL), Topics::all());
        assert_eq!(schedule.next(Topics::empty(), Topics::empty()), Some(start + FAST_INTERVAL));

        let hooked = Topics::all();
        assert_eq!(schedule.due(hooked, start + SLOW_INTERVAL), Topics::empty(), "hooks replace the polling");
        assert_eq!(schedule.due(hooked, start + BATTERY_DRIFT), Topics::BATTERY, "the estimate drifts without events");
        assert_eq!(schedule.next(hooked, Topics::empty()), Some(start + BATTERY_DRIFT));
        let some = Topics::AUDIO | Topics::PRIVACY;
        assert_eq!(schedule.due(some, start + FAST_INTERVAL), Topics::FAST - Topics::AUDIO);
    }

    #[test]
    fn a_burst_of_network_signals_is_read_at_most_once_a_second() {
        let start = Instant::now();
        let mut schedule = Schedule::default();
        schedule.done(Topics::NETWORK | Topics::AUDIO, start);
        let soon = start + Duration::from_millis(200);
        let dirty = Topics::NETWORK | Topics::AUDIO;
        assert_eq!(schedule.held(dirty, soon), Topics::NETWORK, "other topics are never held back");
        assert_eq!(schedule.held(dirty, start + NETWORK_SPACING), Topics::empty());
        assert_eq!(schedule.next(Topics::all(), Topics::NETWORK), Some(start + NETWORK_SPACING));
    }

    fn appearance() -> DesktopAppearance {
        DesktopAppearance {
            appearance: glide_theme::Appearance::Dark,
            accent: glide_theme::Accent::Blue,
            icon_theme: None,
            cursor_theme: None,
            cursor_size: None,
            font_family: None,
            wallpaper: None,
            toolkits: glide_config::toolkits::Support::default(),
        }
    }

    #[test]
    fn nothing_configured_leaves_the_desktop_settings_alone() {
        assert!(gsettings_extras(&appearance()).is_empty());
        assert!(activation_environment(&appearance()).is_empty());
        let blank =
            DesktopAppearance { cursor_theme: Some("  ".into()), font_family: Some(String::new()), ..appearance() };
        assert!(gsettings_extras(&blank).is_empty());
    }

    #[test]
    fn cursor_font_and_wallpaper_reach_gsettings_quoted() {
        let wanted = DesktopAppearance {
            cursor_theme: Some("Bibata-Modern-Ice".into()),
            cursor_size: Some(32),
            font_family: Some("Inter".into()),
            wallpaper: Some("/home/me/Pictures/sea & sky.png".into()),
            ..appearance()
        };
        assert_eq!(
            gsettings_extras(&wanted),
            vec![
                (INTERFACE, "cursor-theme", "'Bibata-Modern-Ice'".to_string()),
                (INTERFACE, "cursor-size", "32".to_string()),
                (INTERFACE, "font-name", "'Inter 11'".to_string()),
                (BACKGROUND, "picture-uri", "'file:///home/me/Pictures/sea%20%26%20sky.png'".to_string()),
                (BACKGROUND, "picture-uri-dark", "'file:///home/me/Pictures/sea%20%26%20sky.png'".to_string()),
            ]
        );
        assert_eq!(activation_environment(&wanted), ["XCURSOR_THEME=Bibata-Modern-Ice", "XCURSOR_SIZE=32"]);
    }

    #[test]
    fn strings_are_escaped_for_gvariant_and_uris_need_absolute_paths() {
        assert_eq!(gvariant_string(r"it's a \ test"), r"'it\'s a \\ test'");
        assert_eq!(file_uri(Path::new("relative.png")), None);
        assert_eq!(
            file_uri(Path::new("/tmp/Шпалери.jpg")).as_deref(),
            Some("file:///tmp/%D0%A8%D0%BF%D0%B0%D0%BB%D0%B5%D1%80%D0%B8.jpg")
        );
    }

    #[test]
    fn airplane_mode_means_every_wireless_radio_is_soft_blocked() {
        let radio = |kind: &str, soft| (kind.to_string(), soft);
        assert!(airplane_from(&[radio("wlan", true), radio("bluetooth", true)]));
        assert!(!airplane_from(&[radio("wlan", true), radio("bluetooth", false)]));
        assert!(!airplane_from(&[]), "no radios, nothing to fly with");
        assert!(!airplane_from(&[radio("gps", true)]), "only wireless radios count");

        let root = std::env::temp_dir().join(format!("glide-rfkill-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (name, kind, soft) in [("rfkill0", "wlan", "1"), ("rfkill1", "bluetooth", "1")] {
            let dir = root.join(name);
            fs::create_dir_all(&dir).expect("scratch radio");
            fs::write(dir.join("type"), format!("{kind}\n")).expect("type");
            fs::write(dir.join("soft"), format!("{soft}\n")).expect("soft");
        }
        assert!(airplane_from(&read_radios(&root)));
        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn power_profiles_cycle_through_all_three() {
        let start = PowerProfile::Balanced;
        assert_eq!(start.next().next().next(), start);
        assert_eq!(parse_power_profile(start.next().key()), Some(PowerProfile::PowerSaver));
    }

    #[test]
    fn the_keyboard_backlight_is_read_as_a_percentage_from_its_led() {
        let leds = std::env::temp_dir().join(format!("glide-leds-{}", std::process::id()));
        let _ = fs::remove_dir_all(&leds);
        for (name, current, max) in [("input3::capslock", "1", "1"), ("tpacpi::kbd_backlight", "1", "2")] {
            let dir = leds.join(name);
            fs::create_dir_all(&dir).expect("scratch led");
            fs::write(dir.join("brightness"), current).expect("brightness");
            fs::write(dir.join("max_brightness"), max).expect("max");
        }
        assert_eq!(read_keyboard_backlight(&leds), Some(50));
        fs::remove_dir_all(leds.join("tpacpi::kbd_backlight")).expect("cleanup");
        assert_eq!(read_keyboard_backlight(&leds), None, "a caps lock led is not a backlight");
        fs::remove_dir_all(&leds).expect("cleanup");
        assert_eq!(read_keyboard_backlight(&leds), None);
    }
}

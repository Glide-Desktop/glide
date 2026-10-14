use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::power::PowerAction;
use crate::state::GlideState;
use crate::topbar::{Detail, PanelAction, PowerDispatch, PromptOutcome, RowTarget};

pub const RESCAN_INTERVAL: Duration = Duration::from_secs(10);
const SCAN_TIMEOUT: Duration = Duration::from_secs(20);
const WIFI_CONNECT_TIMEOUT: Duration = Duration::from_secs(45);
const BLUETOOTH_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_STEP: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiNetwork {
    pub ssid: String,
    pub secured: bool,
    pub signal: u8,
    pub connected: bool,
    pub saved: bool,
}

impl WifiNetwork {
    #[must_use]
    pub const fn level(&self) -> u8 {
        match self.signal {
            0 => 0,
            1..=24 => 1,
            25..=49 => 2,
            50..=74 => 3,
            _ => 4,
        }
    }

    #[must_use]
    pub const fn icon(&self) -> &'static str {
        match self.level() {
            0 => "network-wireless-signal-none",
            1 => "network-wireless-signal-weak",
            2 => "network-wireless-signal-ok",
            3 => "network-wireless-signal-good",
            _ => "network-wireless-signal-excellent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtKind {
    Headphones,
    Mouse,
    Keyboard,
    Phone,
    Other,
}

impl BtKind {
    #[must_use]
    pub fn from_icon(icon: &str) -> Self {
        let icon = icon.trim();
        if icon.starts_with("audio-") {
            Self::Headphones
        } else if icon == "input-mouse" || icon == "input-tablet" {
            Self::Mouse
        } else if icon == "input-keyboard" {
            Self::Keyboard
        } else if icon == "phone" {
            Self::Phone
        } else {
            Self::Other
        }
    }

    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Headphones => "audio-headphones",
            Self::Mouse => "input-mouse",
            Self::Keyboard => "input-keyboard",
            Self::Phone => "phone",
            Self::Other => "bluetooth",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtDevice {
    pub mac: String,
    pub name: String,
    pub kind: BtKind,
    pub connected: bool,
    pub battery: Option<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BtInfo {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub connected: bool,
    pub battery: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    WrongPassword,
    Failed,
}

#[must_use]
pub fn split_terse(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                if let Some(next) = chars.next()
                    && let Some(field) = fields.last_mut()
                {
                    field.push(next);
                }
            }
            ':' => fields.push(String::new()),
            other => {
                if let Some(field) = fields.last_mut() {
                    field.push(other);
                }
            }
        }
    }
    fields
}

#[must_use]
pub fn parse_wifi_list(text: &str) -> Vec<WifiNetwork> {
    let mut networks: Vec<WifiNetwork> = Vec::new();
    for line in text.lines() {
        let fields = split_terse(line);
        let [in_use, ssid, security, signal, ..] = fields.as_slice() else {
            continue;
        };
        if ssid.trim().is_empty() {
            continue;
        }
        let security = security.trim();
        let network = WifiNetwork {
            ssid: ssid.clone(),
            secured: !(security.is_empty() || security == "--"),
            signal: signal.trim().parse::<u8>().unwrap_or(0).min(100),
            connected: in_use.trim() == "*",
            saved: false,
        };
        match networks.iter_mut().find(|known| known.ssid == network.ssid) {
            Some(known) => {
                known.connected |= network.connected;
                known.secured |= network.secured;
                known.signal = known.signal.max(network.signal);
            }
            None => networks.push(network),
        }
    }
    networks
}

#[must_use]
pub fn parse_saved_wifi(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let fields = split_terse(line);
            let [name, kind, ..] = fields.as_slice() else {
                return None;
            };
            let wireless = matches!(kind.trim(), "802-11-wireless" | "wifi");
            (wireless && !name.is_empty()).then(|| name.clone())
        })
        .collect()
}

#[must_use]
pub fn parse_wifi_device(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let fields = split_terse(line);
        let [device, kind, state, ..] = fields.as_slice() else {
            return None;
        };
        (kind.trim() == "wifi" && state.trim() == "connected").then(|| device.clone())
    })
}

pub fn mark_saved(networks: &mut [WifiNetwork], saved: &[String]) {
    for network in networks {
        network.saved = saved.contains(&network.ssid);
    }
}

pub fn sort_networks(networks: &mut [WifiNetwork]) {
    networks.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(b.signal.cmp(&a.signal))
            .then_with(|| a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase()))
    });
}

#[must_use]
pub fn parse_bt_devices(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("Device ")?;
            let (mac, name) = rest.split_once(' ').unwrap_or((rest, ""));
            let valid = mac.len() == 17 && mac.split(':').count() == 6;
            valid.then(|| {
                let name = name.trim();
                let name = if name.is_empty() { mac } else { name };
                (mac.to_string(), name.to_string())
            })
        })
        .collect()
}

#[must_use]
pub fn parse_bt_info(text: &str) -> BtInfo {
    let mut info = BtInfo::default();
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Alias" => info.name = Some(value.to_string()),
            "Name" if info.name.is_none() => info.name = Some(value.to_string()),
            "Icon" => info.icon = Some(value.to_string()),
            "Connected" => info.connected = value == "yes",
            "Battery Percentage" => {
                info.battery = value
                    .split_once('(')
                    .and_then(|(_, tail)| tail.trim_end_matches(')').trim().parse::<u8>().ok())
                    .map(|pct| pct.min(100));
            }
            _ => {}
        }
    }
    info
}

#[must_use]
pub fn device_from(mac: String, listed_name: String, info: &BtInfo) -> BtDevice {
    BtDevice {
        name: info.name.clone().filter(|name| !name.is_empty()).unwrap_or(listed_name),
        kind: info.icon.as_deref().map_or(BtKind::Other, BtKind::from_icon),
        connected: info.connected,
        battery: info.battery,
        mac,
    }
}

pub fn sort_devices(devices: &mut [BtDevice]) {
    devices
        .sort_by(|a, b| b.connected.cmp(&a.connected).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
}

#[must_use]
pub fn classify_wifi_failure(stderr: &str, had_password: bool) -> ConnectError {
    let text = stderr.to_ascii_lowercase();
    let secrets = text.contains("secrets were required")
        || text.contains("802-11-wireless-security.psk")
        || text.contains("no secrets");
    if secrets || (had_password && text.contains("password")) {
        ConnectError::WrongPassword
    } else {
        ConnectError::Failed
    }
}

#[must_use]
pub fn bluetooth_succeeded(status_ok: bool, stdout: &str) -> bool {
    status_ok && !stdout.contains("Failed") && !stdout.contains("not available")
}

pub enum NetRequest {
    ScanWifi,
    ScanBluetooth,
    ConnectWifi { ssid: String, password: Option<String>, saved: bool },
    DisconnectWifi { ssid: String },
    Bluetooth { mac: String, connect: bool },
}

impl std::fmt::Debug for NetRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScanWifi => f.write_str("ScanWifi"),
            Self::ScanBluetooth => f.write_str("ScanBluetooth"),
            Self::ConnectWifi { ssid, password, saved } => f
                .debug_struct("ConnectWifi")
                .field("ssid", ssid)
                .field("password", &password.as_ref().map(|_| "<redacted>"))
                .field("saved", saved)
                .finish(),
            Self::DisconnectWifi { ssid } => f.debug_struct("DisconnectWifi").field("ssid", ssid).finish(),
            Self::Bluetooth { mac, connect } => {
                f.debug_struct("Bluetooth").field("mac", mac).field("connect", connect).finish()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetEvent {
    Wifi(Vec<WifiNetwork>),
    WifiResult { ssid: String, result: Result<(), ConnectError> },
    Bluetooth(Vec<BtDevice>),
    BluetoothResult { mac: String, result: Result<(), ConnectError> },
}

type Sink = Box<dyn Fn(NetEvent) + Send + Sync>;

static SINK: OnceLock<Sink> = OnceLock::new();
static WIFI_SCANNING: AtomicBool = AtomicBool::new(false);
static BLUETOOTH_SCANNING: AtomicBool = AtomicBool::new(false);

pub fn install(sink: impl Fn(NetEvent) + Send + Sync + 'static) {
    if SINK.set(Box::new(sink)).is_err() {
        tracing::warn!("the quick settings network worker was installed twice");
    }
}

fn emit(event: NetEvent) {
    if let Some(sink) = SINK.get() {
        sink(event);
    }
}

pub fn request(request: NetRequest) {
    if SINK.get().is_none() {
        return;
    }
    let flag = match request {
        NetRequest::ScanWifi => Some(&WIFI_SCANNING),
        NetRequest::ScanBluetooth => Some(&BLUETOOTH_SCANNING),
        _ => None,
    };
    if let Some(flag) = flag
        && flag.swap(true, Ordering::AcqRel)
    {
        return;
    }
    tracing::debug!(?request, "quick settings network request");
    let spawned = std::thread::Builder::new().name("glide-qs-net".to_string()).spawn(move || {
        run_request(request);
        if let Some(flag) = flag {
            flag.store(false, Ordering::Release);
        }
    });
    if let Err(err) = spawned {
        tracing::warn!("quick settings network worker did not start: {err}");
        if let Some(flag) = flag {
            flag.store(false, Ordering::Release);
        }
    }
}

fn run_request(request: NetRequest) {
    match request {
        NetRequest::ScanWifi => emit(NetEvent::Wifi(scan_wifi())),
        NetRequest::ScanBluetooth => emit(NetEvent::Bluetooth(scan_bluetooth())),
        NetRequest::ConnectWifi { ssid, password, saved } => {
            let result = connect_wifi(&ssid, password.as_deref(), saved);
            if let Err(error) = result {
                tracing::info!(ssid, ?error, "wi-fi connection failed");
            }
            emit(NetEvent::WifiResult { ssid, result });
            emit(NetEvent::Wifi(scan_wifi()));
        }
        NetRequest::DisconnectWifi { ssid } => {
            let result = disconnect_wifi();
            emit(NetEvent::WifiResult { ssid, result });
            emit(NetEvent::Wifi(scan_wifi()));
        }
        NetRequest::Bluetooth { mac, connect } => {
            let verb = if connect { "connect" } else { "disconnect" };
            let result = run_with_timeout("bluetoothctl", &[verb, &mac], BLUETOOTH_CONNECT_TIMEOUT)
                .filter(|output| bluetooth_succeeded(output.status.success(), &String::from_utf8_lossy(&output.stdout)))
                .map(|_| ())
                .ok_or(ConnectError::Failed);
            emit(NetEvent::BluetoothResult { mac, result });
            emit(NetEvent::Bluetooth(scan_bluetooth()));
        }
    }
}

pub fn run_with_timeout(program: &str, args: &[&str], timeout: Duration) -> Option<Output> {
    run_with_input(program, args, None, timeout)
}

fn run_with_input(program: &str, args: &[&str], input: Option<&str>, timeout: Duration) -> Option<Output> {
    let mut child = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    if let Some(input) = input
        && let Some(mut stdin) = child.stdin.take()
    {
        let _ = stdin.write_all(input.as_bytes());
        let _ = stdin.write_all(b"\n");
    }
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL_STEP),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    Some(Output { status, stdout: stdout.join().unwrap_or_default(), stderr: stderr.join().unwrap_or_default() })
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

fn wifi_connect_args(ssid: &str, has_password: bool, saved: bool) -> Vec<&str> {
    match (has_password, saved) {
        (true, _) => vec!["--ask", "dev", "wifi", "connect", ssid],
        (false, true) => vec!["con", "up", "id", ssid],
        (false, false) => vec!["dev", "wifi", "connect", ssid],
    }
}

fn stdout_of(program: &str, args: &[&str]) -> Option<String> {
    run_with_timeout(program, args, SCAN_TIMEOUT)
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn scan_wifi() -> Vec<WifiNetwork> {
    let Some(list) = stdout_of(
        "nmcli",
        &["-t", "-f", "IN-USE,SSID,SECURITY,SIGNAL,BARS", "dev", "wifi", "list", "--rescan", "auto"],
    ) else {
        return Vec::new();
    };
    let mut networks = parse_wifi_list(&list);
    let saved = stdout_of("nmcli", &["-t", "-f", "NAME,TYPE", "con", "show"])
        .map(|text| parse_saved_wifi(&text))
        .unwrap_or_default();
    mark_saved(&mut networks, &saved);
    sort_networks(&mut networks);
    networks
}

fn connect_wifi(ssid: &str, password: Option<&str>, saved: bool) -> Result<(), ConnectError> {
    let args = wifi_connect_args(ssid, password.is_some(), saved);
    let Some(output) = run_with_input("nmcli", &args, password, WIFI_CONNECT_TIMEOUT) else {
        return Err(ConnectError::Failed);
    };
    if output.status.success() {
        return Ok(());
    }
    Err(classify_wifi_failure(&String::from_utf8_lossy(&output.stderr), password.is_some()))
}

fn disconnect_wifi() -> Result<(), ConnectError> {
    let device = stdout_of("nmcli", &["-t", "-f", "DEVICE,TYPE,STATE", "dev"])
        .and_then(|text| parse_wifi_device(&text))
        .ok_or(ConnectError::Failed)?;
    run_with_timeout("nmcli", &["dev", "disconnect", &device], WIFI_CONNECT_TIMEOUT)
        .filter(|output| output.status.success())
        .map(|_| ())
        .ok_or(ConnectError::Failed)
}

fn scan_bluetooth() -> Vec<BtDevice> {
    let listed = stdout_of("bluetoothctl", &["devices", "Paired"])
        .map(|text| parse_bt_devices(&text))
        .filter(|devices| !devices.is_empty())
        .or_else(|| stdout_of("bluetoothctl", &["paired-devices"]).map(|text| parse_bt_devices(&text)))
        .unwrap_or_default();
    let mut devices: Vec<BtDevice> = listed
        .into_iter()
        .map(|(mac, name)| {
            let info = stdout_of("bluetoothctl", &["info", &mac]).map(|text| parse_bt_info(&text)).unwrap_or_default();
            device_from(mac, name, &info)
        })
        .collect();
    sort_devices(&mut devices);
    devices
}

#[must_use]
pub const fn scan_for(detail: Detail) -> Option<NetRequest> {
    match detail {
        Detail::Wifi => Some(NetRequest::ScanWifi),
        Detail::Bluetooth => Some(NetRequest::ScanBluetooth),
        Detail::Battery => None,
    }
}

fn open_settings_panel(slug: &str) {
    let mut command = Command::new("glide-settings");
    command.arg(slug);
    if let Err(err) = crate::spawn_detached(command) {
        tracing::warn!("could not open glide-settings {slug}: {err}");
    }
}

impl GlideState {
    pub fn handle_net_event(&mut self, event: NetEvent) {
        if self.topbar.apply_net_event(event) {
            self.queue_redraw();
        }
    }

    pub fn rescan_quick_settings_panel(&mut self) {
        if let Some(scan) = self.topbar.detail().and_then(scan_for) {
            request(scan);
        }
    }

    pub(crate) fn run_power_button(&mut self, action: PowerAction) {
        self.topbar.close_popup();
        match crate::topbar::power_dispatch(action) {
            PowerDispatch::Now => self.perform_power_action(action),
            PowerDispatch::Confirm => self.open_power_dialog(Some(action)),
        }
        self.queue_redraw();
    }

    pub(crate) fn run_qs_panel_action(&mut self, action: PanelAction) {
        match action {
            PanelAction::Open(detail) => {
                if self.topbar.open_detail(detail)
                    && let Some(scan) = scan_for(detail)
                {
                    request(scan);
                }
            }
            PanelAction::Back => {
                self.topbar.close_detail();
            }
            PanelAction::Toggle => match self.topbar.detail() {
                Some(Detail::Wifi) => {
                    self.handle_qs_action(crate::topbar::TopbarAction::QsToggleWifi);
                    request(NetRequest::ScanWifi);
                }
                Some(Detail::Bluetooth) => {
                    self.handle_qs_action(crate::topbar::TopbarAction::QsToggleBluetooth);
                    request(NetRequest::ScanBluetooth);
                }
                Some(Detail::Battery) | None => {}
            },
            PanelAction::Row(index) => self.activate_detail_row(index),
            PanelAction::FocusPassword => self.topbar.focus_password(),
            PanelAction::Settings => {
                let detail = self.topbar.detail();
                self.topbar.close_popup();
                match detail {
                    Some(Detail::Battery) => crate::system_info::open_power_settings(),
                    Some(Detail::Bluetooth) => open_settings_panel("bluetooth"),
                    _ => open_settings_panel("network"),
                }
            }
        }
        self.queue_redraw();
    }

    fn activate_detail_row(&mut self, index: u16) {
        match self.topbar.detail_row(index) {
            Some(RowTarget::Network(network)) if network.connected => {
                self.topbar.begin_disconnect(&network.ssid);
                request(NetRequest::DisconnectWifi { ssid: network.ssid });
            }
            Some(RowTarget::Network(network)) if network.saved || !network.secured => {
                self.topbar.begin_connect(&network.ssid);
                request(NetRequest::ConnectWifi { ssid: network.ssid, password: None, saved: network.saved });
            }
            Some(RowTarget::Network(network)) => self.topbar.toggle_prompt(&network.ssid),
            Some(RowTarget::Device(device)) => {
                self.topbar.begin_connect(&device.mac);
                request(NetRequest::Bluetooth { connect: !device.connected, mac: device.mac });
            }
            None => {}
        }
    }

    pub(crate) fn run_qs_field_key(&mut self, key: crate::topbar::FieldKey) {
        match self.topbar.prompt_key(key) {
            PromptOutcome::Unchanged => return,
            PromptOutcome::Changed | PromptOutcome::Cancelled => {}
            PromptOutcome::Submit { ssid, password } => {
                self.topbar.begin_connect(&ssid);
                request(NetRequest::ConnectWifi { ssid, password: Some(password), saved: false });
            }
        }
        self.queue_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terse_fields_keep_escaped_colons_and_backslashes() {
        assert_eq!(split_terse(r"*:Cafe\: Guest:WPA2:81:▂▄▆█"), ["*", "Cafe: Guest", "WPA2", "81", "▂▄▆█"]);
        assert_eq!(split_terse(r"a\\b:c"), [r"a\b", "c"]);
        assert_eq!(split_terse(" ::--:0:"), [" ", "", "--", "0", ""]);
    }

    #[test]
    fn duplicate_ssids_collapse_to_the_strongest_and_hidden_ones_are_dropped() {
        let text = [
            " :Home:WPA2:40:▂▄__",
            "*:Home:WPA2:72:▂▄▆_",
            " :Home:WPA2:55:▂▄▆_",
            " ::WPA2:90:▂▄▆█",
            " :Library:--:30:▂▄__",
            r" :Lab\:5G:WPA3:12:▂___",
        ]
        .join("\n");
        let networks = parse_wifi_list(&text);
        assert_eq!(networks.len(), 3);
        let home = &networks[0];
        assert_eq!(home.ssid, "Home");
        assert_eq!(home.signal, 72);
        assert!(home.connected && home.secured);
        assert!(!networks[1].secured, "-- means an open network");
        assert_eq!(networks[2].ssid, "Lab:5G");
    }

    #[test]
    fn the_connected_network_sorts_first_then_by_signal() {
        let network = |ssid: &str, signal: u8, connected: bool| WifiNetwork {
            ssid: ssid.to_string(),
            secured: true,
            signal,
            connected,
            saved: false,
        };
        let mut networks = vec![
            network("b", 90, false),
            network("weak-home", 20, true),
            network("a", 90, false),
            network("c", 95, false),
        ];
        sort_networks(&mut networks);
        let order: Vec<&str> = networks.iter().map(|n| n.ssid.as_str()).collect();
        assert_eq!(order, ["weak-home", "c", "a", "b"]);
    }

    #[test]
    fn saved_profiles_are_only_wireless_ones() {
        let saved = parse_saved_wifi("Home:802-11-wireless\nWired:802-3-ethernet\nCafe\\: Guest:wifi\n");
        assert_eq!(saved, ["Home", "Cafe: Guest"]);
        let mut networks = parse_wifi_list(" :Cafe\\: Guest:WPA2:50:x\n :Other:WPA2:50:x");
        mark_saved(&mut networks, &saved);
        assert!(networks[0].saved);
        assert!(!networks[1].saved);
    }

    #[test]
    fn the_connected_wifi_device_is_found_among_others() {
        let text = "enp3s0:ethernet:connected\nwlan0:wifi:connected\nlo:loopback:unmanaged\n";
        assert_eq!(parse_wifi_device(text).as_deref(), Some("wlan0"));
        assert_eq!(parse_wifi_device("wlan0:wifi:disconnected"), None);
    }

    #[test]
    fn signal_strength_maps_onto_four_bars_and_an_icon() {
        let network =
            |signal: u8| WifiNetwork { ssid: String::new(), secured: false, signal, connected: false, saved: false };
        assert_eq!(network(0).icon(), "network-wireless-signal-none");
        assert_eq!(network(10).level(), 1);
        assert_eq!(network(49).level(), 2);
        assert_eq!(network(50).level(), 3);
        assert_eq!(network(100).icon(), "network-wireless-signal-excellent");
    }

    #[test]
    fn paired_devices_parse_with_names_containing_spaces() {
        let text = "Device 11:22:33:44:55:66 WH-1000XM4 Headphones\nDevice AA:BB:CC:DD:EE:FF\nnoise line\n";
        let devices = parse_bt_devices(text);
        assert_eq!(
            devices,
            [
                ("11:22:33:44:55:66".to_string(), "WH-1000XM4 Headphones".to_string()),
                ("AA:BB:CC:DD:EE:FF".to_string(), "AA:BB:CC:DD:EE:FF".to_string()),
            ]
        );
    }

    #[test]
    fn device_info_reads_connection_icon_and_battery() {
        let text = "Device 11:22:33:44:55:66 (public)\n\tName: WH-1000XM4\n\tAlias: My Phones\n\tIcon: audio-headset\n\tPaired: yes\n\tConnected: yes\n\tBattery Percentage: 0x50 (80)\n";
        let info = parse_bt_info(text);
        assert_eq!(info.name.as_deref(), Some("My Phones"));
        assert!(info.connected);
        assert_eq!(info.battery, Some(80));
        let device = device_from("11:22:33:44:55:66".into(), "listed".into(), &info);
        assert_eq!(device.kind, BtKind::Headphones);
        assert_eq!(device.kind.icon(), "audio-headphones");
        assert_eq!(parse_bt_info("\tConnected: no\n"), BtInfo::default());
    }

    #[test]
    fn device_icons_fall_back_to_bluetooth() {
        assert_eq!(BtKind::from_icon("input-mouse"), BtKind::Mouse);
        assert_eq!(BtKind::from_icon("input-keyboard"), BtKind::Keyboard);
        assert_eq!(BtKind::from_icon("phone"), BtKind::Phone);
        assert_eq!(BtKind::from_icon("input-gaming").icon(), "bluetooth");
    }

    #[test]
    fn connected_devices_sort_first_then_by_name() {
        let device = |name: &str, connected: bool| BtDevice {
            mac: String::new(),
            name: name.to_string(),
            kind: BtKind::Other,
            connected,
            battery: None,
        };
        let mut devices = vec![device("zeta", false), device("Mouse", true), device("alpha", false)];
        sort_devices(&mut devices);
        let order: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(order, ["Mouse", "alpha", "zeta"]);
    }

    #[test]
    fn missing_secrets_read_as_a_wrong_password() {
        assert_eq!(
            classify_wifi_failure(
                "Error: Connection activation failed: Secrets were required, but not provided.",
                true
            ),
            ConnectError::WrongPassword
        );
        assert_eq!(classify_wifi_failure("Error: No network with SSID 'x' found.", false), ConnectError::Failed);
    }

    #[test]
    fn bluetoothctl_failures_are_detected_in_its_output() {
        assert!(bluetooth_succeeded(true, "Attempting to connect\nConnection successful\n"));
        assert!(!bluetooth_succeeded(true, "Failed to connect: org.bluez.Error.Failed\n"));
        assert!(!bluetooth_succeeded(false, ""));
    }

    #[test]
    fn a_connect_request_never_prints_its_password() {
        let request = NetRequest::ConnectWifi { ssid: "Home".into(), password: Some("hunter22".into()), saved: false };
        let printed = format!("{request:?}");
        assert!(printed.contains("Home"));
        assert!(!printed.contains("hunter22"));
    }

    #[test]
    fn a_wifi_password_goes_over_stdin_not_argv() {
        assert_eq!(wifi_connect_args("Home", true, false), ["--ask", "dev", "wifi", "connect", "Home"]);
        assert_eq!(wifi_connect_args("Home", true, true), ["--ask", "dev", "wifi", "connect", "Home"]);
        assert_eq!(wifi_connect_args("Home", false, true), ["con", "up", "id", "Home"]);
        let output = run_with_input("cat", &[], Some("hunter22"), Duration::from_secs(5)).expect("cat runs");
        assert_eq!(output.stdout, b"hunter22\n");
    }

    #[test]
    fn output_larger_than_a_pipe_buffer_does_not_stall_the_child() {
        let started = Instant::now();
        let output = run_with_timeout("head", &["-c", "200000", "/dev/zero"], Duration::from_secs(10))
            .expect("head finishes before the timeout");
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 200_000);
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}

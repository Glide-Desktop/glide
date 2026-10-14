use std::collections::HashMap;
use std::time::{Duration, Instant};

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, Value};

use super::run;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPoint {
    pub ssid: String,
    pub signal: u8,
    pub secured: bool,
    pub in_use: bool,
    pub known: bool,
    pub profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wired {
    pub device: String,
    pub connected: bool,
    pub connection: String,
}

#[allow(clippy::struct_excessive_bools)] // Independent radio and availability facts read from nmcli and rfkill
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkState {
    pub available: bool,
    pub wifi_enabled: bool,
    pub wifi_hardware: bool,
    pub wifi_device: Option<String>,
    pub networks: Vec<AccessPoint>,
    pub wired: Vec<Wired>,
    pub profiles: Vec<Profile>,
    pub airplane: bool,
    pub rfkill: bool,
}

impl NetworkState {
    #[must_use]
    pub fn active(&self) -> Option<&AccessPoint> {
        self.networks.iter().find(|network| network.in_use)
    }

    #[must_use]
    pub fn hotspot(&self) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.name == HOTSPOT_NAME && profile.active)
    }

    #[must_use]
    pub fn active_profile(&self, device: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.active && profile.device == device)
    }

    pub fn saved_wifi(&self) -> impl Iterator<Item = &Profile> {
        self.profiles.iter().filter(|profile| profile.kind == WIFI_KIND && profile.name != HOTSPOT_NAME)
    }
}

pub const WIFI_KIND: &str = "802-11-wireless";
pub const ETHERNET_KIND: &str = "802-3-ethernet";
pub const HOTSPOT_NAME: &str = "Glide Hotspot";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Profile {
    pub name: String,
    pub uuid: String,
    pub kind: String,
    pub device: String,
    pub active: bool,
    pub autoconnect: bool,
}

#[must_use]
pub fn parse_connections(text: &str) -> Vec<Profile> {
    text.lines()
        .filter_map(|line| match split_terse(line).as_slice() {
            [name, uuid, kind, device, active, autoconnect] if !uuid.is_empty() => Some(Profile {
                name: name.clone(),
                uuid: uuid.clone(),
                kind: kind.clone(),
                device: device.clone(),
                active: active == "yes",
                autoconnect: autoconnect == "yes",
            }),
            _ => None,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Radios {
    pub wifi_hardware: bool,
    pub wifi: bool,
}

#[must_use]
pub fn parse_radios(text: &str) -> Option<Radios> {
    let line = text.lines().next()?;
    let fields = split_terse(line);
    let [wifi_hardware, wifi, ..] = fields.as_slice() else {
        return None;
    };
    Some(Radios { wifi_hardware: wifi_hardware == "enabled", wifi: wifi == "enabled" })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfkillDevice {
    pub kind: String,
    pub soft: bool,
    pub hard: bool,
}

#[must_use]
pub fn airplane_mode(devices: &[RfkillDevice]) -> bool {
    !devices.is_empty() && devices.iter().all(|device| device.soft || device.hard)
}

fn read_rfkill() -> Vec<RfkillDevice> {
    let Ok(entries) = std::fs::read_dir("/sys/class/rfkill") else {
        return Vec::new();
    };
    let read = |path: &std::path::Path, name: &str| {
        std::fs::read_to_string(path.join(name)).map(|text| text.trim().to_string()).unwrap_or_default()
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            RfkillDevice {
                kind: read(&path, "type"),
                soft: read(&path, "soft") == "1",
                hard: read(&path, "hard") == "1",
            }
        })
        .collect()
}

pub fn set_airplane(on: bool) -> Result<String, String> {
    let (rfkill, radio) = if on { ("block", "off") } else { ("unblock", "on") };
    let blocked = run::status("rfkill", &[rfkill, "all"]);
    let radios = run::status("nmcli", &["radio", "all", radio]);
    blocked.or(radios)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Metered {
    #[default]
    Auto,
    Yes,
    No,
}

impl Metered {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "yes" => Self::Yes,
            "no" => Self::No,
            _ => Self::Auto,
        }
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Auto => "unknown",
            Self::Yes => "yes",
            Self::No => "no",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProfileSettings {
    pub metered: Metered,
    pub autoconnect: bool,
    pub hidden: bool,
    pub key_mgmt: String,
    pub mode: String,
    pub ipv4_method: String,
    pub custom_dns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Details {
    pub device: String,
    pub mac: String,
    pub speed: String,
    pub ipv4: Vec<String>,
    pub gateway: String,
    pub dns: Vec<String>,
    pub ipv6: Vec<String>,
    pub frequency: String,
    pub channel: String,
    pub security: String,
    pub settings: Option<ProfileSettings>,
}

fn terse_pairs(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(|line| {
        let (key, value) = line.split_once(':')?;
        let key = key.split('[').next().unwrap_or(key);
        Some((key.trim(), value.trim()))
    })
}

fn list(value: &str) -> Vec<String> {
    value
        .split([',', ' ', '|'])
        .map(str::trim)
        .filter(|item| !item.is_empty() && *item != "--")
        .map(str::to_string)
        .collect()
}

fn known(value: &str) -> String {
    match value.trim() {
        "" | "--" | "unknown" => String::new(),
        value => value.to_string(),
    }
}

#[must_use]
pub fn parse_device_show(text: &str) -> Details {
    let mut details = Details::default();
    for (key, value) in terse_pairs(text) {
        match key {
            "GENERAL.DEVICE" => details.device = value.to_string(),
            "GENERAL.HWADDR" => details.mac = known(value),
            "CAPABILITIES.SPEED" => details.speed = known(value),
            "IP4.ADDRESS" => details.ipv4.extend(list(value)),
            "IP4.GATEWAY" => details.gateway = known(value),
            "IP4.DNS" | "IP6.DNS" => details.dns.extend(list(value)),
            "IP6.ADDRESS" => details.ipv6.extend(list(value).into_iter().filter(|addr| !addr.starts_with("fe80:"))),
            _ => {}
        }
    }
    details
}

#[must_use]
pub fn parse_profile_settings(text: &str) -> ProfileSettings {
    let mut settings = ProfileSettings::default();
    for (key, value) in terse_pairs(text) {
        match key {
            "connection.metered" => settings.metered = Metered::parse(value),
            "connection.autoconnect" => settings.autoconnect = value == "yes",
            "802-11-wireless.hidden" => settings.hidden = value == "yes",
            "802-11-wireless.mode" => settings.mode = known(value),
            "802-11-wireless-security.key-mgmt" => settings.key_mgmt = known(value),
            "ipv4.method" => settings.ipv4_method = known(value),
            "ipv4.dns" => settings.custom_dns = list(value),
            _ => {}
        }
    }
    settings
}

#[must_use]
pub fn parse_active_radio(text: &str) -> Option<(String, String, String)> {
    text.lines().find_map(|line| match split_terse(line).as_slice() {
        [in_use, frequency, channel, security] if in_use.trim() == "*" => {
            Some((known(frequency), known(channel), known(security)))
        }
        _ => None,
    })
}

#[must_use]
pub fn read_details(device: Option<&str>, uuid: Option<&str>, wifi: bool) -> Details {
    let mut details = device
        .and_then(|device| {
            run::output(
                "nmcli",
                &[
                    "-t",
                    "-f",
                    "GENERAL.DEVICE,GENERAL.HWADDR,CAPABILITIES.SPEED,IP4.ADDRESS,IP4.GATEWAY,IP4.DNS,IP6.ADDRESS,IP6.DNS",
                    "device",
                    "show",
                    device,
                ],
            )
        })
        .map(|text| parse_device_show(&text))
        .unwrap_or_default();
    if wifi
        && device.is_some()
        && let Some((frequency, channel, security)) =
            run::output("nmcli", &["-t", "-f", "IN-USE,FREQ,CHAN,SECURITY", "device", "wifi", "list", "--rescan", "no"])
                .as_deref()
                .and_then(parse_active_radio)
    {
        details.frequency = frequency;
        details.channel = channel;
        details.security = security;
    }
    details.settings = uuid.and_then(|uuid| {
        run::output(
            "nmcli",
            &[
                "-t",
                "-f",
                "connection.metered,connection.autoconnect,802-11-wireless.hidden,802-11-wireless.mode,802-11-wireless-security.key-mgmt,ipv4.method,ipv4.dns",
                "connection",
                "show",
                "uuid",
                uuid,
            ],
        )
        .map(|text| parse_profile_settings(&text))
    });
    details
}

fn modify(uuid: &str, key: &str, value: &str) -> Result<String, String> {
    run::status("nmcli", &["connection", "modify", "uuid", uuid, key, value])
}

pub fn set_metered(uuid: &str, metered: Metered) -> Result<String, String> {
    modify(uuid, "connection.metered", metered.key())
}

pub fn set_autoconnect(uuid: &str, on: bool) -> Result<String, String> {
    modify(uuid, "connection.autoconnect", if on { "yes" } else { "no" })
}

#[must_use]
pub fn parse_dns_list(text: &str) -> Option<Vec<String>> {
    let items = list(text);
    items.iter().all(|item| item.parse::<std::net::IpAddr>().is_ok()).then_some(items)
}

pub fn set_dns(uuid: &str, device: Option<&str>, servers: &[String]) -> Result<String, String> {
    let (v4, v6): (Vec<&str>, Vec<&str>) =
        servers.iter().map(String::as_str).partition(|server| server.parse::<std::net::Ipv4Addr>().is_ok());
    let v4 = v4.join(",");
    let v6 = v6.join(",");
    let ignore_v4 = if v4.is_empty() { "no" } else { "yes" };
    let ignore_v6 = if v6.is_empty() { "no" } else { "yes" };
    run::status(
        "nmcli",
        &[
            "connection",
            "modify",
            "uuid",
            uuid,
            "ipv4.dns",
            &v4,
            "ipv4.ignore-auto-dns",
            ignore_v4,
            "ipv6.dns",
            &v6,
            "ipv6.ignore-auto-dns",
            ignore_v6,
        ],
    )?;
    match device.filter(|device| !device.is_empty()) {
        Some(device) => run::status("nmcli", &["device", "reapply", device]),
        None => Ok(String::new()),
    }
}

pub fn reveal_password(uuid: &str) -> Result<String, String> {
    let secret =
        run::status("nmcli", &["-s", "-g", "802-11-wireless-security.psk", "connection", "show", "uuid", uuid])?;
    if secret.is_empty() { Err("NetworkManager returned no password for this network".to_string()) } else { Ok(secret) }
}

pub fn stop_profile(uuid: &str) -> Result<String, String> {
    run::status("nmcli", &["connection", "down", "uuid", uuid])
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(pub String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(…)")
    }
}

impl Secret {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    #[must_use]
    pub fn take(&mut self) -> String {
        std::mem::take(&mut self.0)
    }
}

#[must_use]
pub fn password_from_bytes(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    bytes.iter().map(|byte| ALPHABET[usize::from(*byte) % ALPHABET.len()] as char).collect()
}

#[must_use]
pub fn generate_password() -> Option<String> {
    use std::io::Read;
    let mut bytes = [0u8; 12];
    std::fs::File::open("/dev/urandom").and_then(|mut file| file.read_exact(&mut bytes)).ok()?;
    Some(password_from_bytes(&bytes))
}

pub const MAX_SSID_BYTES: usize = 32;

#[must_use]
pub fn valid_ssid(ssid: &str) -> bool {
    let ssid = ssid.trim();
    !ssid.is_empty() && ssid.len() <= MAX_SSID_BYTES
}

#[must_use]
pub fn valid_psk(password: &str) -> bool {
    let len = password.chars().count();
    (8..=63).contains(&len) && password.is_ascii()
}

#[must_use]
pub fn split_terse(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    fields.last_mut().expect("fields is never empty").push(next);
                }
            }
            ':' => fields.push(String::new()),
            _ => fields.last_mut().expect("fields is never empty").push(c),
        }
    }
    fields
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownNetwork {
    pub uuid: String,
    pub ssid: String,
}

#[must_use]
pub fn parse_ssid(text: &str) -> Option<String> {
    let ssid = text.strip_suffix('\n').unwrap_or(text);
    (!ssid.is_empty()).then(|| ssid.to_string())
}

#[must_use]
pub fn parse_networks(text: &str, known: &[KnownNetwork]) -> Vec<AccessPoint> {
    let mut networks: Vec<AccessPoint> = Vec::new();
    for line in text.lines() {
        let fields = split_terse(line);
        let [in_use, ssid, signal, security] = fields.as_slice() else {
            continue;
        };
        if ssid.is_empty() {
            continue;
        }
        let profile = known.iter().find(|network| network.ssid == *ssid).map(|network| network.uuid.clone());
        let point = AccessPoint {
            ssid: ssid.clone(),
            signal: signal.trim().parse().unwrap_or(0),
            secured: !security.trim().is_empty() && security.trim() != "--",
            in_use: in_use.trim() == "*",
            known: profile.is_some(),
            profile,
        };
        match networks.iter_mut().find(|other| other.ssid == point.ssid) {
            Some(other) => {
                other.in_use |= point.in_use;
                other.signal = other.signal.max(point.signal);
            }
            None => networks.push(point),
        }
    }
    networks.sort_by(|a, b| {
        b.in_use.cmp(&a.in_use).then(b.known.cmp(&a.known)).then(b.signal.cmp(&a.signal)).then(a.ssid.cmp(&b.ssid))
    });
    networks
}

#[must_use]
pub fn parse_devices(text: &str) -> (Option<String>, Vec<Wired>) {
    let mut wifi = None;
    let mut wired = Vec::new();
    for line in text.lines() {
        let fields = split_terse(line);
        let [device, kind, state, connection] = fields.as_slice() else {
            continue;
        };
        match kind.as_str() {
            "wifi" if wifi.is_none() => wifi = Some(device.clone()),
            "ethernet" if state != "unavailable" && state != "unmanaged" => wired.push(Wired {
                device: device.clone(),
                connected: state == "connected",
                connection: connection.clone(),
            }),
            _ => {}
        }
    }
    (wifi, wired)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rescan {
    No,
    Auto,
    Yes,
}

impl Rescan {
    #[must_use]
    pub const fn arg(self) -> &'static str {
        match self {
            Self::No => "no",
            Self::Auto => "auto",
            Self::Yes => "yes",
        }
    }
}

#[must_use]
pub fn read(rescan: Rescan) -> NetworkState {
    let Some(radios) = run::output("nmcli", &["-t", "radio"]).as_deref().and_then(parse_radios) else {
        return NetworkState::default();
    };
    let wifi_enabled = radios.wifi;
    let (wifi_device, wired) = run::output("nmcli", &["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device"])
        .map(|text| parse_devices(&text))
        .unwrap_or_default();
    let profiles = read_profiles();
    let rfkill = read_rfkill();
    let known = known_networks(&profiles);
    let networks = if wifi_enabled {
        run::output(
            "nmcli",
            &["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", rescan.arg()],
        )
        .map(|text| parse_networks(&text, &known))
        .unwrap_or_default()
    } else {
        Vec::new()
    };
    NetworkState {
        available: true,
        wifi_enabled,
        wifi_hardware: radios.wifi_hardware,
        wifi_device,
        networks: networks.into_iter().filter(|point| point.ssid != HOTSPOT_NAME).collect(),
        wired,
        profiles,
        airplane: airplane_mode(&rfkill),
        rfkill: !rfkill.is_empty(),
    }
}

fn read_profiles() -> Vec<Profile> {
    run::output("nmcli", &["-t", "-f", "NAME,UUID,TYPE,DEVICE,ACTIVE,AUTOCONNECT", "connection", "show"])
        .map(|text| parse_connections(&text))
        .unwrap_or_default()
}

fn known_networks(profiles: &[Profile]) -> Vec<KnownNetwork> {
    profiles
        .iter()
        .filter(|profile| profile.kind == WIFI_KIND && profile.name != HOTSPOT_NAME)
        .map(|profile| {
            let ssid = run::output(
                "nmcli",
                &["-e", "no", "-g", "802-11-wireless.ssid", "connection", "show", "uuid", &profile.uuid],
            )
            .and_then(|text| parse_ssid(&text))
            .unwrap_or_else(|| profile.name.clone());
            KnownNetwork { uuid: profile.uuid.clone(), ssid }
        })
        .collect()
}

pub fn set_wifi(enabled: bool) -> Result<String, String> {
    run::status("nmcli", &["radio", "wifi", if enabled { "on" } else { "off" }])
}

#[derive(Clone, PartialEq, Eq)]
pub enum ConnectPlan<'a> {
    Nmcli(Vec<&'a str>),
    AddAndActivate { ssid: &'a str, psk: &'a str },
}

impl std::fmt::Debug for ConnectPlan<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nmcli(args) => f.debug_tuple("Nmcli").field(args).finish(),
            Self::AddAndActivate { ssid, .. } => {
                f.debug_struct("AddAndActivate").field("ssid", ssid).finish_non_exhaustive()
            }
        }
    }
}

#[must_use]
pub fn connect_plan<'a>(ssid: &'a str, profile: Option<&'a str>, password: Option<&'a str>) -> ConnectPlan<'a> {
    let password = password.filter(|password| !password.is_empty());
    match (profile, password) {
        (_, Some(psk)) => ConnectPlan::AddAndActivate { ssid, psk },
        (Some(uuid), None) => ConnectPlan::Nmcli(vec!["connection", "up", "uuid", uuid]),
        (None, None) => ConnectPlan::Nmcli(vec!["device", "wifi", "connect", ssid]),
    }
}

pub type Settings<'a> = HashMap<&'static str, HashMap<&'static str, Value<'a>>>;

#[must_use]
pub fn wireless_settings<'a>(ssid: &'a str, psk: &'a str, infer_security: bool) -> Settings<'a> {
    let mut security = HashMap::from([("psk", Value::from(psk))]);
    if !infer_security {
        security.insert("key-mgmt", Value::from("wpa-psk"));
    }
    HashMap::from([
        ("802-11-wireless", HashMap::from([("ssid", Value::from(ssid.as_bytes()))])),
        ("802-11-wireless-security", security),
    ])
}

pub const WRONG_PASSWORD: &str = "Wrong password; check it and try again";

#[must_use]
pub fn failure_reason(code: u32) -> &'static str {
    match code {
        7..=11 => WRONG_PASSWORD,
        53 => "The network could not be found",
        _ => "Connection activation failed",
    }
}

pub fn connect(ssid: &str, profile: Option<&str>, password: Option<&str>) -> Result<String, String> {
    match connect_plan(ssid, profile, password) {
        ConnectPlan::Nmcli(args) => run::status("nmcli", &args),
        ConnectPlan::AddAndActivate { ssid, psk } => add_and_activate(ssid, psk).map_err(bus_error),
    }
}

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const DEVICE_TYPE_WIFI: u32 = 2;
const DEVICE_STATE_FAILED: u32 = 120;
const ACTIVE_ACTIVATED: u32 = 2;
const ACTIVE_DEACTIVATING: u32 = 3;
const ACTIVE_DEACTIVATED: u32 = 4;
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(100);

enum Failure {
    Bus(zbus::Error),
    Message(String),
}

impl From<zbus::Error> for Failure {
    fn from(err: zbus::Error) -> Self {
        Self::Bus(err)
    }
}

fn proxy<'a>(bus: &Connection, path: &'a OwnedObjectPath, interface: &'static str) -> zbus::Result<Proxy<'a>> {
    zbus::blocking::proxy::Builder::new(bus)
        .destination(NM)?
        .path(path.as_str())?
        .interface(interface)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
}

fn wifi_device(bus: &Connection) -> Result<OwnedObjectPath, Failure> {
    let nm = Proxy::new(bus, NM, NM_PATH, NM)?;
    let devices: Vec<OwnedObjectPath> = nm.call("GetDevices", &())?;
    devices
        .into_iter()
        .find(|path| {
            proxy(bus, path, "org.freedesktop.NetworkManager.Device")
                .and_then(|device| device.get_property::<u32>("DeviceType"))
                .is_ok_and(|kind| kind == DEVICE_TYPE_WIFI)
        })
        .ok_or_else(|| Failure::Message("No Wi-Fi device found".to_string()))
}

fn access_point(bus: &Connection, device: &OwnedObjectPath, ssid: &str) -> Option<OwnedObjectPath> {
    let wireless = proxy(bus, device, "org.freedesktop.NetworkManager.Device.Wireless").ok()?;
    let points: Vec<OwnedObjectPath> = wireless.get_property("AccessPoints").ok()?;
    points
        .into_iter()
        .filter_map(|path| {
            let (name, strength) = {
                let point = proxy(bus, &path, "org.freedesktop.NetworkManager.AccessPoint").ok()?;
                let name: Vec<u8> = point.get_property("Ssid").ok()?;
                (name, point.get_property::<u8>("Strength").unwrap_or(0))
            };
            (name == ssid.as_bytes()).then_some((strength, path))
        })
        .max_by_key(|(strength, _)| *strength)
        .map(|(_, path)| path)
}

fn root_path() -> OwnedObjectPath {
    OwnedObjectPath::try_from("/").expect("\"/\" is a valid object path")
}

#[must_use]
pub fn hidden_settings<'a>(ssid: &'a str, psk: Option<&'a str>) -> Settings<'a> {
    let mut settings = HashMap::from([(
        "802-11-wireless",
        HashMap::from([("ssid", Value::from(ssid.as_bytes())), ("hidden", Value::from(true))]),
    )]);
    if let Some(psk) = psk {
        settings.insert(
            "802-11-wireless-security",
            HashMap::from([("key-mgmt", Value::from("wpa-psk")), ("psk", Value::from(psk))]),
        );
    }
    settings
}

#[must_use]
pub fn hotspot_settings<'a>(ssid: &'a str, psk: &'a str) -> Settings<'a> {
    HashMap::from([
        (
            "connection",
            HashMap::from([
                ("id", Value::from(HOTSPOT_NAME)),
                ("type", Value::from(WIFI_KIND)),
                ("autoconnect", Value::from(false)),
            ]),
        ),
        ("802-11-wireless", HashMap::from([("ssid", Value::from(ssid.as_bytes())), ("mode", Value::from("ap"))])),
        (
            "802-11-wireless-security",
            HashMap::from([
                ("key-mgmt", Value::from("wpa-psk")),
                ("psk", Value::from(psk)),
                ("proto", Value::from(vec!["rsn"])),
                ("pairwise", Value::from(vec!["ccmp"])),
                ("group", Value::from(vec!["ccmp"])),
            ]),
        ),
        ("ipv4", HashMap::from([("method", Value::from("shared"))])),
        ("ipv6", HashMap::from([("method", Value::from("ignore"))])),
    ])
}

fn bus_error(err: Failure) -> String {
    match err {
        Failure::Bus(err) => format!("NetworkManager: {err}"),
        Failure::Message(message) => message,
    }
}

pub fn connect_hidden(ssid: &str, psk: Option<&str>) -> Result<String, String> {
    if !valid_ssid(ssid) {
        return Err(format!("A network name is 1 to {MAX_SSID_BYTES} bytes long"));
    }
    let psk = psk.filter(|psk| !psk.is_empty());
    match psk {
        None => run::status("nmcli", &["device", "wifi", "connect", ssid, "hidden", "yes"]),
        Some(psk) => activate_on_wifi(hidden_settings(ssid, Some(psk))),
    }
}

fn activate_on_wifi(settings: Settings<'_>) -> Result<String, String> {
    (|| {
        let bus = Connection::system()?;
        let device = wifi_device(&bus)?;
        activate_new(&bus, &device, &root_path(), settings)
    })()
    .map_err(bus_error)
}

pub fn start_hotspot(ssid: &str, psk: &str) -> Result<String, String> {
    if !valid_ssid(ssid) || !valid_psk(psk) {
        return Err("The hotspot needs a name and a password of 8 to 63 characters".to_string());
    }
    for profile in read_profiles() {
        if profile.name == HOTSPOT_NAME {
            let _ = forget(&profile.uuid);
        }
    }
    activate_on_wifi(hotspot_settings(ssid, psk))
}

fn activate_new(
    bus: &Connection, device: &OwnedObjectPath, specific: &OwnedObjectPath, settings: Settings<'_>,
) -> Result<String, Failure> {
    let nm = Proxy::new(bus, NM, NM_PATH, NM)?;
    let (profile, active): (OwnedObjectPath, OwnedObjectPath) =
        nm.call("AddAndActivateConnection", &(settings, device, specific))?;
    match wait_for_activation(bus, device, &active) {
        Ok(()) => Ok(String::new()),
        Err(message) => {
            if let Ok(saved) = proxy(bus, &profile, "org.freedesktop.NetworkManager.Settings.Connection") {
                let _: zbus::Result<()> = saved.call("Delete", &());
            }
            Err(Failure::Message(message))
        }
    }
}

fn add_and_activate(ssid: &str, psk: &str) -> Result<String, Failure> {
    let bus = Connection::system()?;
    let device = wifi_device(&bus)?;
    let point = access_point(&bus, &device, ssid);
    let settings = wireless_settings(ssid, psk, point.is_some());
    let specific = point.unwrap_or_else(root_path);
    activate_new(&bus, &device, &specific, settings)
}

fn wait_for_activation(bus: &Connection, device: &OwnedObjectPath, active: &OwnedObjectPath) -> Result<(), String> {
    let device = proxy(bus, device, "org.freedesktop.NetworkManager.Device").map_err(|err| err.to_string())?;
    let started = Instant::now();
    let mut reason = 0;
    loop {
        if let Ok((state, code)) = device.get_property::<(u32, u32)>("StateReason")
            && state == DEVICE_STATE_FAILED
            && code != 0
        {
            reason = code;
        }
        let state = proxy(bus, active, "org.freedesktop.NetworkManager.Connection.Active")
            .and_then(|connection| connection.get_property::<u32>("State"))
            .unwrap_or(ACTIVE_DEACTIVATED);
        if state == ACTIVE_ACTIVATED {
            return Ok(());
        }
        if state == ACTIVE_DEACTIVATING || state == ACTIVE_DEACTIVATED || started.elapsed() > ACTIVATION_TIMEOUT {
            return Err(failure_reason(reason).to_string());
        }
        std::thread::sleep(POLL);
    }
}

pub fn disconnect(device: &str) -> Result<String, String> {
    run::status("nmcli", &["device", "disconnect", device])
}

pub fn forget(uuid: &str) -> Result<String, String> {
    run::status("nmcli", &["connection", "delete", "uuid", uuid])
}

pub const PROXY: &str = "org.gnome.system.proxy";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProxyMode {
    #[default]
    None,
    Manual,
    Auto,
}

impl ProxyMode {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text {
            "manual" => Self::Manual,
            "auto" => Self::Auto,
            _ => Self::None,
        }
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Manual => "manual",
            Self::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProxyEndpoint {
    pub host: String,
    pub port: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProxySettings {
    pub available: bool,
    pub mode: ProxyMode,
    pub autoconfig_url: String,
    pub http: ProxyEndpoint,
    pub https: ProxyEndpoint,
    pub socks: ProxyEndpoint,
    pub ignore_hosts: String,
}

const PROXY_KINDS: [&str; 3] = ["http", "https", "socks"];

impl ProxySettings {
    #[must_use]
    pub fn from_snapshot(snapshot: &super::gsettings::Snapshot) -> Self {
        let endpoint = |kind: &str| {
            let schema = format!("{PROXY}.{kind}");
            let port = snapshot.int(&schema, "port").unwrap_or(0);
            ProxyEndpoint {
                host: snapshot.string(&schema, "host").unwrap_or_default(),
                port: if port > 0 { port.to_string() } else { String::new() },
            }
        };
        Self {
            available: snapshot.has_schema(PROXY),
            mode: ProxyMode::parse(&snapshot.string(PROXY, "mode").unwrap_or_default()),
            autoconfig_url: snapshot.string(PROXY, "autoconfig-url").unwrap_or_default(),
            http: endpoint("http"),
            https: endpoint("https"),
            socks: endpoint("socks"),
            ignore_hosts: snapshot.strings(PROXY, "ignore-hosts").join(", "),
        }
    }

    #[must_use]
    pub fn endpoint(&self, kind: usize) -> &ProxyEndpoint {
        match kind {
            0 => &self.http,
            1 => &self.https,
            _ => &self.socks,
        }
    }

    pub fn endpoint_mut(&mut self, kind: usize) -> &mut ProxyEndpoint {
        match kind {
            0 => &mut self.http,
            1 => &mut self.https,
            _ => &mut self.socks,
        }
    }

    #[must_use]
    pub fn invalid_port(&self) -> Option<usize> {
        (0..PROXY_KINDS.len()).find(|kind| {
            let port = self.endpoint(*kind).port.trim();
            !port.is_empty() && port.parse::<u16>().map_or(true, |port| port == 0)
        })
    }

    #[must_use]
    pub fn changes(&self) -> Vec<(String, &'static str, String)> {
        let mut changes = vec![(PROXY.to_string(), "mode", super::gsettings::quote(self.mode.key()))];
        match self.mode {
            ProxyMode::None => {}
            ProxyMode::Auto => {
                changes.push((
                    PROXY.to_string(),
                    "autoconfig-url",
                    super::gsettings::quote(self.autoconfig_url.trim()),
                ));
            }
            ProxyMode::Manual => {
                for (index, kind) in PROXY_KINDS.iter().enumerate() {
                    let endpoint = self.endpoint(index);
                    let schema = format!("{PROXY}.{kind}");
                    changes.push((schema.clone(), "host", super::gsettings::quote(endpoint.host.trim())));
                    changes.push((schema, "port", endpoint.port.trim().parse::<u16>().unwrap_or(0).to_string()));
                }
                let hosts: Vec<String> = self
                    .ignore_hosts
                    .split(',')
                    .map(str::trim)
                    .filter(|host| !host.is_empty())
                    .map(str::to_string)
                    .collect();
                changes.push((PROXY.to_string(), "ignore-hosts", super::gsettings::string_array(&hosts)));
            }
        }
        changes
    }
}

#[must_use]
pub fn read_proxy() -> ProxySettings {
    ProxySettings::from_snapshot(&super::gsettings::Snapshot::read(&[PROXY]))
}

pub fn write_proxy(settings: &ProxySettings) -> Result<String, String> {
    for (schema, key, value) in settings.changes() {
        super::gsettings::set(&schema, key, &value)?;
    }
    Ok(String::new())
}

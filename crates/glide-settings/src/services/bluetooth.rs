use std::time::Duration;

use super::run;

#[allow(clippy::struct_excessive_bools)] // Mirrors BlueZ's independent device properties
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BtDevice {
    pub address: String,
    pub name: String,
    pub icon: String,
    pub paired: bool,
    pub connected: bool,
    pub trusted: bool,
    pub blocked: bool,
    pub battery: Option<u8>,
}

#[allow(clippy::struct_excessive_bools)] // Mirrors BlueZ's independent adapter properties
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Adapter {
    pub address: String,
    pub name: String,
    pub alias: String,
    pub powered: bool,
    pub blocked: bool,
    pub discoverable: bool,
    pub pairable: bool,
    pub discoverable_timeout: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BluetoothState {
    pub available: bool,
    pub powered: bool,
    pub blocked: bool,
    pub alias: String,
    pub adapter: Adapter,
    pub devices: Vec<BtDevice>,
}

impl BluetoothState {
    pub fn paired(&self) -> impl Iterator<Item = &BtDevice> {
        self.devices.iter().filter(|device| device.paired)
    }

    pub fn nearby(&self) -> impl Iterator<Item = &BtDevice> {
        self.devices.iter().filter(|device| !device.paired && !is_anonymous(device))
    }

    #[must_use]
    pub fn device(&self, address: &str) -> Option<&BtDevice> {
        self.devices.iter().find(|device| device.address == address)
    }
}

#[must_use]
pub fn is_anonymous(device: &BtDevice) -> bool {
    let name = device.name.replace('-', ":");
    name.is_empty() || name.eq_ignore_ascii_case(&device.address)
}

fn hex_or_decimal(text: &str) -> Option<u32> {
    if let Some(inner) = text.split_once('(').and_then(|(_, rest)| rest.split_once(')')).map(|(inner, _)| inner.trim())
    {
        return inner.parse().ok();
    }
    let text = text.trim();
    match text.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

#[must_use]
pub fn parse_adapter(text: &str) -> Option<Adapter> {
    let first = text.trim_start().lines().next()?;
    let address = first.strip_prefix("Controller ")?;
    let address = address.split_whitespace().next().unwrap_or_default();
    let (powered, blocked, alias) = parse_show(text)?;
    Some(Adapter {
        address: address.to_string(),
        name: field(text, "Name").unwrap_or_default().to_string(),
        alias,
        powered,
        blocked,
        discoverable: yes(text, "Discoverable"),
        pairable: yes(text, "Pairable"),
        discoverable_timeout: field(text, "DiscoverableTimeout").and_then(hex_or_decimal).unwrap_or(0),
    })
}

#[must_use]
pub fn parse_battery(text: &str) -> Option<u8> {
    let value = field(text, "Battery Percentage").and_then(hex_or_decimal)?;
    u8::try_from(value.min(100)).ok()
}

#[must_use]
pub fn device_kind(icon: &str) -> DeviceKind {
    match icon {
        "audio-headset" | "audio-headphones" => DeviceKind::Headphones,
        "audio-card" | "audio-speakers" => DeviceKind::Speaker,
        "input-keyboard" => DeviceKind::Keyboard,
        "input-mouse" => DeviceKind::Mouse,
        "input-tablet" => DeviceKind::Tablet,
        "input-gaming" => DeviceKind::Gamepad,
        "phone" => DeviceKind::Phone,
        "computer" => DeviceKind::Computer,
        _ => DeviceKind::Other,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Headphones,
    Speaker,
    Keyboard,
    Mouse,
    Tablet,
    Gamepad,
    Phone,
    Computer,
    Other,
}

fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| line.trim().strip_prefix(key).and_then(|rest| rest.strip_prefix(':')).map(str::trim))
}

fn yes(text: &str, key: &str) -> bool {
    field(text, key) == Some("yes")
}

#[must_use]
pub fn parse_show(text: &str) -> Option<(bool, bool, String)> {
    if !text.trim_start().starts_with("Controller") {
        return None;
    }
    let powered = yes(text, "Powered");
    let blocked = field(text, "PowerState").is_some_and(|state| state.contains("blocked"));
    let alias = field(text, "Alias").or_else(|| field(text, "Name")).unwrap_or_default().to_string();
    Some((powered, blocked, alias))
}

#[must_use]
pub fn parse_devices(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("Device ")?;
            let (address, name) = rest.split_once(' ').unwrap_or((rest, rest));
            (address.len() == 17).then(|| (address.to_string(), name.trim().to_string()))
        })
        .collect()
}

#[must_use]
pub fn parse_info(address: &str, fallback: &str, text: &str) -> BtDevice {
    BtDevice {
        address: address.to_string(),
        name: field(text, "Alias").or_else(|| field(text, "Name")).unwrap_or(fallback).to_string(),
        icon: field(text, "Icon").unwrap_or_default().to_string(),
        paired: yes(text, "Paired"),
        connected: yes(text, "Connected"),
        trusted: yes(text, "Trusted"),
        blocked: yes(text, "Blocked"),
        battery: parse_battery(text),
    }
}

const QUERY_DEADLINE: Duration = Duration::from_secs(10);
const ACTION_DEADLINE: Duration = Duration::from_secs(30);

fn query(args: &[&str]) -> Option<String> {
    run::output_within("bluetoothctl", args, QUERY_DEADLINE)
}

fn act(args: &[&str]) -> Result<String, String> {
    run::status_within("bluetoothctl", args, ACTION_DEADLINE)
}

fn bluez_running() -> bool {
    zbus::blocking::Connection::system()
        .and_then(|bus| zbus::blocking::fdo::DBusProxy::new(&bus))
        .and_then(|proxy| proxy.name_has_owner(zbus::names::BusName::try_from("org.bluez")?).map_err(zbus::Error::from))
        .unwrap_or(false)
}

#[must_use]
pub fn read() -> BluetoothState {
    if !bluez_running() {
        return BluetoothState::default();
    }
    let Some(adapter) = query(&["show"]).as_deref().and_then(parse_adapter) else {
        return BluetoothState::default();
    };
    let (powered, blocked, alias) = (adapter.powered, adapter.blocked, adapter.alias.clone());
    let mut devices: Vec<BtDevice> = query(&["devices"])
        .map(|text| parse_devices(&text))
        .unwrap_or_default()
        .into_iter()
        .map(|(address, name)| {
            let info = query(&["info", &address]).unwrap_or_default();
            parse_info(&address, &name, &info)
        })
        .collect();
    devices.sort_by(|a, b| b.connected.cmp(&a.connected).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    BluetoothState { available: true, powered, blocked, alias, adapter, devices }
}

pub fn set_trusted(address: &str, trusted: bool) -> Result<String, String> {
    act(&[if trusted { "trust" } else { "untrust" }, address])
}

pub fn set_blocked(address: &str, blocked: bool) -> Result<String, String> {
    act(&[if blocked { "block" } else { "unblock" }, address])
}

const fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

pub fn set_discoverable(on: bool) -> Result<String, String> {
    act(&["discoverable", on_off(on)])
}

pub fn set_pairable(on: bool) -> Result<String, String> {
    act(&["pairable", on_off(on)])
}

#[must_use]
pub fn clean_alias(name: &str) -> Option<String> {
    let name: String = name.chars().filter(|c| !c.is_control()).collect();
    let name = name.trim();
    (!name.is_empty() && name.len() <= 248).then(|| name.to_string())
}

pub fn set_adapter_alias(name: &str) -> Result<String, String> {
    let name = clean_alias(name).ok_or_else(|| "Enter a name".to_string())?;
    act(&["system-alias", &name])
}

pub fn set_device_alias(address: &str, name: &str) -> Result<String, String> {
    use zbus::zvariant::{OwnedObjectPath, OwnedValue};
    type Objects = std::collections::HashMap<
        OwnedObjectPath,
        std::collections::HashMap<String, std::collections::HashMap<String, OwnedValue>>,
    >;
    let name = clean_alias(name).ok_or_else(|| "Enter a name".to_string())?;
    let result = (|| -> zbus::Result<()> {
        let bus = zbus::blocking::Connection::system()?;
        let manager = zbus::blocking::Proxy::new(&bus, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager")?;
        let objects: Objects = manager.call("GetManagedObjects", &())?;
        let path = objects
            .iter()
            .find(|(_, interfaces)| {
                interfaces.get("org.bluez.Device1").is_some_and(|props| {
                    props
                        .get("Address")
                        .and_then(|value| <&str>::try_from(value).ok())
                        .is_some_and(|found| found.eq_ignore_ascii_case(address))
                })
            })
            .map(|(path, _)| path.clone())
            .ok_or_else(|| zbus::Error::Failure(format!("{address} is not known to BlueZ")))?;
        let device = zbus::blocking::Proxy::new(&bus, "org.bluez", path, "org.bluez.Device1")?;
        device.set_property("Alias", name.as_str()).map_err(zbus::Error::from)
    })();
    result.map(|()| String::new()).map_err(|err| format!("BlueZ: {err}"))
}

pub fn set_powered(on: bool) -> Result<String, String> {
    if on {
        let _ = run::status("rfkill", &["unblock", "bluetooth"]);
    }
    act(&["power", on_off(on)])
}

pub fn scan() -> Result<String, String> {
    act(&["--timeout", "20", "scan", "on"])
}

pub fn pair_and_connect(address: &str) -> Result<String, String> {
    act(&["--timeout", "20", "pair", address])?;
    let _ = act(&["trust", address]);
    connect(address)
}

pub fn connect(address: &str) -> Result<String, String> {
    act(&["--timeout", "15", "connect", address])
}

pub fn disconnect(address: &str) -> Result<String, String> {
    act(&["disconnect", address])
}

pub fn remove(address: &str) -> Result<String, String> {
    act(&["remove", address])
}

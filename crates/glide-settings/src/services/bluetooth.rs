use std::collections::HashMap;
use std::time::Duration;

use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

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

pub type Properties = HashMap<String, OwnedValue>;
pub type Objects = HashMap<OwnedObjectPath, HashMap<String, Properties>>;

fn text(props: &Properties, key: &str) -> Option<String> {
    props.get(key).and_then(|value| <&str>::try_from(value).ok()).map(str::to_string)
}

fn flag(props: &Properties, key: &str) -> bool {
    props.get(key).and_then(|value| bool::try_from(value).ok()).unwrap_or(false)
}

#[must_use]
pub fn from_objects(objects: &Objects) -> BluetoothState {
    let mut adapters: Vec<(&OwnedObjectPath, &Properties)> = objects
        .iter()
        .filter_map(|(path, interfaces)| interfaces.get("org.bluez.Adapter1").map(|props| (path, props)))
        .collect();
    adapters.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    let Some((adapter_path, props)) = adapters.first().copied() else {
        return BluetoothState::default();
    };
    let name = text(props, "Name").unwrap_or_default();
    let adapter = Adapter {
        address: text(props, "Address").unwrap_or_default(),
        alias: text(props, "Alias").unwrap_or_else(|| name.clone()),
        name,
        powered: flag(props, "Powered"),
        blocked: text(props, "PowerState").is_some_and(|state| state.contains("blocked")),
        discoverable: flag(props, "Discoverable"),
        pairable: flag(props, "Pairable"),
        discoverable_timeout: props.get("DiscoverableTimeout").and_then(|v| u32::try_from(v).ok()).unwrap_or(0),
    };
    let mut devices: Vec<BtDevice> = objects
        .values()
        .filter_map(|interfaces| {
            let props = interfaces.get("org.bluez.Device1")?;
            if let Some(Value::ObjectPath(owner)) = props.get("Adapter").map(|value| &**value)
                && owner.as_str() != adapter_path.as_str()
            {
                return None;
            }
            let address = text(props, "Address")?;
            let battery = interfaces
                .get("org.bluez.Battery1")
                .and_then(|battery| battery.get("Percentage"))
                .and_then(|value| u8::try_from(value).ok())
                .map(|level| level.min(100));
            Some(BtDevice {
                name: text(props, "Alias").or_else(|| text(props, "Name")).unwrap_or_else(|| address.clone()),
                icon: text(props, "Icon").unwrap_or_default(),
                paired: flag(props, "Paired"),
                connected: flag(props, "Connected"),
                trusted: flag(props, "Trusted"),
                blocked: flag(props, "Blocked"),
                battery,
                address,
            })
        })
        .collect();
    devices.sort_by(|a, b| b.connected.cmp(&a.connected).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    BluetoothState {
        available: true,
        powered: adapter.powered,
        blocked: adapter.blocked,
        alias: adapter.alias.clone(),
        adapter,
        devices,
    }
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

const ACTION_DEADLINE: Duration = Duration::from_secs(30);

fn act(args: &[&str]) -> Result<String, String> {
    run::status_within("bluetoothctl", args, ACTION_DEADLINE)
}

fn managed_objects() -> zbus::Result<Objects> {
    let bus = zbus::blocking::Connection::system()?;
    let manager = zbus::blocking::Proxy::new(&bus, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager")?;
    manager.call("GetManagedObjects", &())
}

#[must_use]
pub fn read() -> BluetoothState {
    managed_objects().map(|objects| from_objects(&objects)).unwrap_or_default()
}

pub(crate) fn set_trusted(address: &str, trusted: bool) -> Result<String, String> {
    act(&[if trusted { "trust" } else { "untrust" }, address])
}

pub(crate) fn set_blocked(address: &str, blocked: bool) -> Result<String, String> {
    act(&[if blocked { "block" } else { "unblock" }, address])
}

const fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

pub(crate) fn set_discoverable(on: bool) -> Result<String, String> {
    act(&["discoverable", on_off(on)])
}

pub(crate) fn set_pairable(on: bool) -> Result<String, String> {
    act(&["pairable", on_off(on)])
}

#[must_use]
pub fn clean_alias(name: &str) -> Option<String> {
    let name: String = name.chars().filter(|c| !c.is_control()).collect();
    let name = name.trim();
    (!name.is_empty() && name.len() <= 248).then(|| name.to_string())
}

pub(crate) fn set_adapter_alias(name: &str) -> Result<String, String> {
    let name = clean_alias(name).ok_or_else(|| "Enter a name".to_string())?;
    act(&["system-alias", &name])
}

pub(crate) fn set_device_alias(address: &str, name: &str) -> Result<String, String> {
    let name = clean_alias(name).ok_or_else(|| "Enter a name".to_string())?;
    let result = (|| -> zbus::Result<()> {
        let objects = managed_objects()?;
        let bus = zbus::blocking::Connection::system()?;
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

pub(crate) fn set_powered(on: bool) -> Result<String, String> {
    if on {
        let _ = run::status("rfkill", &["unblock", "bluetooth"]);
    }
    act(&["power", on_off(on)])
}

pub fn scan() -> Result<String, String> {
    act(&["--timeout", "20", "scan", "on"])
}

pub(crate) fn pair_and_connect(address: &str) -> Result<String, String> {
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

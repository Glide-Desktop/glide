use std::collections::HashMap;
use std::time::{Duration, Instant};

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value};

use crate::wfd;

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIFI_P2P: &str = "org.freedesktop.NetworkManager.Device.WifiP2P";
const PEER: &str = "org.freedesktop.NetworkManager.WifiP2PPeer";
const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const DEVICE_TYPE_WIFI_P2P: u32 = 30;
const ACTIVATED: u32 = 2;
const DEACTIVATED: u32 = 4;
const POLL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub path: OwnedObjectPath,
    pub name: String,
    pub address: String,
    pub strength: u8,
    pub display: Option<wfd::DeviceInfo>,
}

impl Peer {
    #[must_use]
    pub fn is_display(&self) -> bool {
        self.display.is_some_and(|info| info.role.shows_video())
    }
}

#[must_use]
pub fn sort_peers(mut peers: Vec<Peer>) -> Vec<Peer> {
    peers.sort_by(|a, b| {
        b.is_display().cmp(&a.is_display()).then(b.strength.cmp(&a.strength)).then(a.name.cmp(&b.name))
    });
    peers
}

type Settings<'a> = HashMap<&'static str, HashMap<&'static str, Value<'a>>>;

#[must_use]
pub fn connection_settings(peer_address: &str) -> Settings<'static> {
    let connection = HashMap::from([
        ("type", Value::from("wifi-p2p")),
        ("id", Value::from("Glide screen mirroring")),
        ("autoconnect", Value::from(false)),
    ]);
    let p2p = HashMap::from([
        ("peer", Value::from(peer_address.to_string())),
        ("wfd-ies", Value::from(wfd::source_subelements())),
    ]);
    let ipv4 = HashMap::from([("method", Value::from("auto"))]);
    let ipv6 = HashMap::from([("method", Value::from("ignore"))]);
    HashMap::from([("connection", connection), ("wifi-p2p", p2p), ("ipv4", ipv4), ("ipv6", ipv6)])
}

fn failed(err: impl std::fmt::Display) -> String {
    err.to_string()
}

pub struct P2p {
    bus: Connection,
    device: OwnedObjectPath,
}

impl P2p {
    pub fn open() -> Result<Self, String> {
        let bus = Connection::system().map_err(failed)?;
        let nm = Proxy::new(&bus, NM, NM_PATH, NM).map_err(failed)?;
        let devices: Vec<OwnedObjectPath> = nm.call("GetDevices", &()).map_err(failed)?;
        for device in devices {
            let kind: u32 = Proxy::new(&bus, NM, device.as_str(), DEVICE)
                .and_then(|proxy| proxy.get_property("DeviceType"))
                .map_err(failed)?;
            if kind == DEVICE_TYPE_WIFI_P2P {
                return Ok(Self { bus, device });
            }
        }
        Err("NetworkManager has no Wi-Fi P2P device; the Wi-Fi card or driver does not support Wi-Fi Direct".into())
    }

    fn p2p(&self) -> Result<Proxy<'_>, String> {
        Proxy::new(&self.bus, NM, self.device.as_str(), WIFI_P2P).map_err(failed)
    }

    pub fn start_find(&self, seconds: i32) -> Result<(), String> {
        let options = HashMap::from([("timeout", Value::from(seconds.clamp(1, 600)))]);
        self.p2p()?.call::<_, _, ()>("StartFind", &(options,)).map_err(failed)
    }

    pub fn stop_find(&self) -> Result<(), String> {
        self.p2p()?.call::<_, _, ()>("StopFind", &()).map_err(failed)
    }

    pub fn peers(&self) -> Result<Vec<Peer>, String> {
        let paths: Vec<OwnedObjectPath> = self.p2p()?.get_property("Peers").map_err(failed)?;
        let mut peers = Vec::new();
        for path in paths {
            let (name, address, strength, ies) = {
                let proxy = Proxy::new(&self.bus, NM, path.as_str(), PEER).map_err(failed)?;
                (
                    proxy.get_property::<String>("Name").unwrap_or_default(),
                    proxy.get_property::<String>("HwAddress").unwrap_or_default(),
                    proxy.get_property::<u8>("Strength").unwrap_or_default(),
                    proxy.get_property::<Vec<u8>>("WfdIEs").unwrap_or_default(),
                )
            };
            peers.push(Peer { name, address, strength, display: wfd::device_info(&ies), path });
        }
        Ok(sort_peers(peers))
    }

    pub fn connect(&self, peer: &Peer, timeout: Duration) -> Result<OwnedObjectPath, String> {
        let nm = Proxy::new(&self.bus, NM, NM_PATH, NM).map_err(failed)?;
        let options = HashMap::from([("persist", Value::from("volatile"))]);
        let specific = ObjectPath::try_from(peer.path.as_str()).map_err(failed)?;
        let (_, active, _): (OwnedObjectPath, OwnedObjectPath, HashMap<String, zbus::zvariant::OwnedValue>) = nm
            .call(
                "AddAndActivateConnection2",
                &(connection_settings(&peer.address), self.device.as_ref(), specific, options),
            )
            .map_err(failed)?;
        let deadline = Instant::now() + timeout;
        loop {
            let state: u32 = Proxy::new(&self.bus, NM, active.as_str(), ACTIVE)
                .and_then(|proxy| proxy.get_property("State"))
                .unwrap_or(DEACTIVATED);
            match state {
                ACTIVATED => return Ok(active),
                DEACTIVATED => return Err("the display refused the Wi-Fi Direct connection".into()),
                _ if Instant::now() >= deadline => {
                    let _ = self.disconnect(&active);
                    return Err("the display did not accept the connection in time".into());
                }
                _ => std::thread::sleep(POLL),
            }
        }
    }

    pub fn disconnect(&self, active: &OwnedObjectPath) -> Result<(), String> {
        let nm = Proxy::new(&self.bus, NM, NM_PATH, NM).map_err(failed)?;
        nm.call::<_, _, ()>("DeactivateConnection", &(active.as_ref(),)).map_err(failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(name: &str, strength: u8, display: Option<wfd::DeviceInfo>) -> Peer {
        Peer {
            path: OwnedObjectPath::try_from(format!("/org/freedesktop/NetworkManager/WifiP2PPeer/{strength}")).unwrap(),
            name: name.into(),
            address: "aa:bb:cc:dd:ee:ff".into(),
            strength,
            display,
        }
    }

    #[test]
    fn displays_come_first_then_the_strongest_signal() {
        let tv = wfd::device_info(&[0x00, 0x00, 0x06, 0x00, 0x11, 0x1c, 0x44, 0x00, 0x32]);
        let phone = wfd::device_info(&[0x00, 0x00, 0x06, 0x00, 0x10, 0x1c, 0x44, 0x00, 0x32]);
        let sorted = sort_peers(vec![peer("Phone", 90, phone), peer("Printer", 80, None), peer("TV", 40, tv)]);
        let names: Vec<&str> = sorted.iter().map(|peer| peer.name.as_str()).collect();
        assert_eq!(names, ["TV", "Phone", "Printer"]);
        assert!(sorted[0].is_display() && !sorted[1].is_display(), "a phone that is only a source is no display");
    }

    #[test]
    fn the_connection_carries_glides_wfd_ies_and_is_never_saved_as_autoconnect() {
        let settings = connection_settings("aa:bb:cc:dd:ee:ff");
        assert_eq!(settings["connection"]["type"], Value::from("wifi-p2p"));
        assert_eq!(settings["connection"]["autoconnect"], Value::from(false));
        assert_eq!(settings["wifi-p2p"]["peer"], Value::from("aa:bb:cc:dd:ee:ff"));
        assert_eq!(settings["wifi-p2p"]["wfd-ies"], Value::from(wfd::source_subelements()));
        assert_eq!(settings["ipv6"]["method"], Value::from("ignore"));
    }
}

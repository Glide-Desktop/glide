use std::collections::HashMap;

use zbus::{
    blocking::{Connection, MessageIterator, Proxy, fdo::DBusProxy},
    zvariant::{OwnedObjectPath, OwnedValue},
};

const UDISKS: &str = "org.freedesktop.UDisks2";
const BLOCK: &str = "org.freedesktop.UDisks2.Block";
const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const RULE: &str = "type='signal',sender='org.freedesktop.UDisks2',interface='org.freedesktop.DBus.ObjectManager'";
pub const OPEN: &str = "open-volume";
const FIRST_ID: u32 = u32::MAX - 2000;
const SLOTS: u32 = 400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub path: String,
    pub label: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Added(Volume),
    Removed(String),
}

#[derive(Debug, Default)]
pub struct Toasts {
    shown: Vec<(String, u32)>,
    next: u32,
}

impl Toasts {
    pub fn open(&mut self, path: &str) -> u32 {
        if let Some((_, id)) = self.shown.iter().find(|(shown, _)| shown == path) {
            return *id;
        }
        let id = FIRST_ID + self.next % SLOTS;
        self.next = self.next.wrapping_add(1);
        self.shown.retain(|(_, shown)| *shown != id);
        self.shown.push((path.to_string(), id));
        id
    }

    pub fn close(&mut self, path: &str) -> Option<u32> {
        let index = self.shown.iter().position(|(shown, _)| shown == path)?;
        Some(self.shown.swap_remove(index).1)
    }

    #[must_use]
    pub fn path(&self, id: u32) -> Option<&str> {
        self.shown.iter().find(|(_, shown)| *shown == id).map(|(path, _)| path.as_str())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockFacts {
    pub ignore: bool,
    pub system: bool,
    pub label: String,
    pub size: u64,
    pub mounted: bool,
}

#[must_use]
pub fn announce(path: &str, facts: &BlockFacts) -> Option<Volume> {
    (!facts.ignore && !facts.system && !facts.mounted).then(|| Volume {
        path: path.to_string(),
        label: facts.label.trim().to_string(),
        size: facts.size,
    })
}

fn facts(connection: &Connection, path: &str) -> zbus::Result<BlockFacts> {
    let block = Proxy::new(connection, UDISKS, path, BLOCK)?;
    let filesystem = Proxy::new(connection, UDISKS, path, FILESYSTEM)?;
    let mounts: Vec<Vec<u8>> = filesystem.get_property("MountPoints").unwrap_or_default();
    Ok(BlockFacts {
        ignore: block.get_property("HintIgnore")?,
        system: block.get_property("HintSystem")?,
        label: block.get_property("IdLabel").unwrap_or_default(),
        size: block.get_property("Size").unwrap_or_default(),
        mounted: !mounts.is_empty(),
    })
}

fn event(connection: &Connection, message: &zbus::Message) -> Option<Event> {
    let header = message.header();
    if header.interface().is_none_or(|name| name.as_str() != OBJECT_MANAGER) {
        return None;
    }
    match header.member()?.as_str() {
        "InterfacesAdded" => {
            let (path, interfaces) =
                message.body().deserialize::<(OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>)>().ok()?;
            if !interfaces.contains_key(FILESYSTEM) {
                return None;
            }
            let facts = facts(connection, path.as_str()).ok()?;
            announce(path.as_str(), &facts).map(Event::Added)
        }
        "InterfacesRemoved" => {
            let (path, interfaces) = message.body().deserialize::<(OwnedObjectPath, Vec<String>)>().ok()?;
            interfaces.iter().any(|name| name == FILESYSTEM).then(|| Event::Removed(path.to_string()))
        }
        _ => None,
    }
}

pub fn start(sink: impl Fn(Event) + Send + 'static) -> anyhow::Result<()> {
    let connection = Connection::system()?;
    DBusProxy::new(&connection)?.add_match_rule(zbus::MatchRule::try_from(RULE)?)?;
    std::thread::Builder::new().name("glide-removable".into()).spawn(move || {
        for message in MessageIterator::from(&connection) {
            if let Some(event) = message.ok().and_then(|message| event(&connection, &message)) {
                sink(event);
            }
        }
    })?;
    Ok(())
}

fn mount_point(raw: &[u8]) -> String {
    String::from_utf8_lossy(raw.strip_suffix(&[0]).unwrap_or(raw)).into_owned()
}

fn mount(path: &str) -> zbus::Result<String> {
    let connection = Connection::system()?;
    let filesystem = Proxy::new(&connection, UDISKS, path, FILESYSTEM)?;
    let mounts: Vec<Vec<u8>> = filesystem.get_property("MountPoints").unwrap_or_default();
    if let Some(first) = mounts.first() {
        return Ok(mount_point(first));
    }
    let options: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
    filesystem.call_method("Mount", &(options,))?.body().deserialize::<String>()
}

pub fn open(path: String) {
    let spawned = std::thread::Builder::new().name("glide-mount".into()).spawn(move || match mount(&path) {
        Ok(folder) => {
            let mut files = std::process::Command::new("glide-files");
            files.arg(&folder);
            if let Err(err) = crate::spawn_detached(files) {
                tracing::warn!(%err, folder, "could not open the drive in Files");
            }
        }
        Err(err) => tracing::warn!(%err, path, "could not mount the drive"),
    });
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not start the mount thread");
    }
}

impl crate::GlideState {
    pub fn handle_removable(&mut self, event: Event) {
        use crate::i18n::{Key, t};
        match event {
            Event::Added(volume) => {
                tracing::info!(path = volume.path, label = volume.label, "a removable drive was connected");
                let id = self.session.volumes.open(&volume.path);
                let name = if volume.label.is_empty() { t(Key::RemovableDrive).to_string() } else { volume.label };
                let notice = glide_notify::Notification {
                    id,
                    app_name: "Glide".to_string(),
                    app_icon: "drive-removable-media".to_string(),
                    summary: t(Key::RemovableSummary).replace("{device}", &name),
                    body: crate::disk_space::human(volume.size),
                    actions: vec![glide_notify::Action { key: OPEN.into(), label: t(Key::RemovableOpen).into() }],
                    timeout: Some(std::time::Duration::from_secs(15)),
                    ..glide_notify::Notification::default()
                };
                self.notifications.push(&mut self.dock.icons, notice, true);
                self.refresh_topbar();
                self.queue_redraw();
            }
            Event::Removed(path) => {
                if let Some(id) = self.session.volumes.close(&path) {
                    self.notifications.dismiss(id);
                    self.queue_redraw();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_unmounted_user_drive_is_announced() {
        let usb = BlockFacts { label: " KINGSTON ".into(), size: 32, ..BlockFacts::default() };
        let volume = announce("/org/freedesktop/UDisks2/block_devices/sdb1", &usb).expect("announced");
        assert_eq!(volume.label, "KINGSTON");
        for facts in [
            BlockFacts { system: true, ..usb.clone() },
            BlockFacts { ignore: true, ..usb.clone() },
            BlockFacts { mounted: true, ..usb },
        ] {
            assert_eq!(announce("/x", &facts), None, "{facts:?}");
        }
    }

    #[test]
    fn each_drive_keeps_one_toast_until_it_is_removed() {
        let mut toasts = Toasts::default();
        let first = toasts.open("/sdb1");
        assert_eq!(toasts.open("/sdb1"), first, "a repeated add reuses the toast");
        let second = toasts.open("/sdc1");
        assert_ne!(first, second);
        assert!(crate::notifications::is_local(first));
        assert_eq!(toasts.path(second), Some("/sdc1"));
        assert_eq!(toasts.close("/sdb1"), Some(first));
        assert_eq!(toasts.close("/sdb1"), None);
        assert_eq!(toasts.path(first), None);
    }

    #[test]
    fn a_mount_point_drops_its_trailing_nul() {
        assert_eq!(mount_point(b"/run/media/u/KINGSTON\0"), "/run/media/u/KINGSTON");
        assert_eq!(mount_point(b"/mnt"), "/mnt");
    }
}

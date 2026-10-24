use std::collections::HashMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use zbus::blocking::Connection;
use zbus::zvariant::{DynamicDeserialize, DynamicType, OwnedObjectPath, OwnedValue, Value};

use super::entry::format_bytes;
use crate::i18n::{Key, Lang};

const SERVICE: &str = "org.freedesktop.UDisks2";
const ROOT: &str = "/org/freedesktop/UDisks2";
const MANAGER_PATH: &str = "/org/freedesktop/UDisks2/Manager";
const MANAGER: &str = "org.freedesktop.UDisks2.Manager";
const BLOCK: &str = "org.freedesktop.UDisks2.Block";
const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";
const DRIVE: &str = "org.freedesktop.UDisks2.Drive";
const ENCRYPTED: &str = "org.freedesktop.UDisks2.Encrypted";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const ERROR_PREFIX: &str = "org.freedesktop.UDisks2.Error.";

pub type Properties = HashMap<String, OwnedValue>;
pub type Interfaces = HashMap<String, Properties>;
pub type Objects = HashMap<OwnedObjectPath, Interfaces>;

#[allow(clippy::struct_excessive_bools)] // Independent udisks2 drive and block flags
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub object: String,
    pub device: PathBuf,
    pub label: String,
    pub fs_type: String,
    pub size: u64,
    pub mount: Option<PathBuf>,
    pub drive: Option<String>,
    pub removable: bool,
    pub can_power_off: bool,
    pub ejectable: bool,
    pub locked: bool,
    pub backing: Option<String>,
}

impl Volume {
    #[must_use]
    pub fn name(&self, lang: Lang) -> String {
        if self.label.trim().is_empty() {
            Key::VolumeFallbackName.text(lang).replace("{}", &format_bytes(self.size, lang))
        } else {
            self.label.clone()
        }
    }

    #[must_use]
    pub fn can_remove(&self) -> bool {
        self.drive.is_some() && (self.can_power_off || self.ejectable)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountMode {
    Normal,
    Fallback,
    ReadOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mounted {
    pub path: PathBuf,
    pub mode: MountMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attempt {
    pub driver: Option<&'static str>,
    pub read_only: bool,
}

impl Attempt {
    #[must_use]
    pub fn mode(self, requested_read_only: bool) -> MountMode {
        if self.read_only && !requested_read_only {
            MountMode::ReadOnly
        } else if self.driver.is_some() {
            MountMode::Fallback
        } else {
            MountMode::Normal
        }
    }
}

#[must_use]
pub fn mount_attempts(fs_type: &str, read_only: bool) -> Vec<Attempt> {
    let drivers: &[Option<&'static str>] = if fs_type == "ntfs" { &[None, Some("ntfs")] } else { &[None] };
    let modes: &[bool] = if read_only { &[true] } else { &[false, true] };
    modes.iter().flat_map(|&read_only| drivers.iter().map(move |&driver| Attempt { driver, read_only })).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskError {
    Unavailable,
    NotAuthorized,
    Cancelled,
    Busy,
    MissingTool(String),
    Unrepaired,
    Failed(String),
}

impl DiskError {
    #[must_use]
    pub fn classify(name: &str, detail: Option<&str>) -> Self {
        match name.strip_prefix(ERROR_PREFIX) {
            Some("NotAuthorizedDismissed" | "Cancelled") => Self::Cancelled,
            Some("NotAuthorized" | "NotAuthorizedCanObtain") => Self::NotAuthorized,
            Some("DeviceBusy") => Self::Busy,
            Some(_) => Self::Failed(detail.unwrap_or_default().trim().to_string()),
            None => Self::Unavailable,
        }
    }

    fn retryable(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
}

impl From<zbus::Error> for DiskError {
    fn from(err: zbus::Error) -> Self {
        match err {
            zbus::Error::MethodError(name, detail, _) => Self::classify(name.as_str(), detail.as_deref()),
            _ => Self::Unavailable,
        }
    }
}

fn get<'a>(properties: &'a Properties, key: &str) -> Option<&'a OwnedValue> {
    properties.get(key)
}

fn flag(properties: &Properties, key: &str) -> bool {
    get(properties, key).and_then(|value| bool::try_from(value).ok()).unwrap_or(false)
}

fn string(properties: &Properties, key: &str) -> String {
    get(properties, key).and_then(|value| <&str>::try_from(value).ok()).unwrap_or_default().to_string()
}

fn number(properties: &Properties, key: &str) -> u64 {
    get(properties, key).and_then(|value| u64::try_from(value).ok()).unwrap_or(0)
}

fn bytes_to_path(mut bytes: Vec<u8>) -> Option<PathBuf> {
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    if bytes.is_empty() {
        return None;
    }
    Some(PathBuf::from(OsString::from_vec(bytes)))
}

fn byte_path(properties: &Properties, key: &str) -> Option<PathBuf> {
    let value = get(properties, key)?.try_clone().ok()?;
    bytes_to_path(Vec::<u8>::try_from(value).ok()?)
}

fn mount_points(properties: &Properties) -> Vec<PathBuf> {
    get(properties, "MountPoints")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Vec::<Vec<u8>>::try_from(value).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(bytes_to_path)
        .collect()
}

fn object_path(properties: &Properties, key: &str) -> Option<String> {
    let value = get(properties, key)?;
    let path = match &**value {
        Value::ObjectPath(path) => path.as_str().to_string(),
        _ => return None,
    };
    (path != "/").then_some(path)
}

#[must_use]
pub fn volumes_from(objects: &Objects) -> Vec<Volume> {
    let mut volumes: Vec<Volume> = objects
        .iter()
        .filter_map(|(path, interfaces)| {
            let block = interfaces.get(BLOCK)?;
            let filesystem = interfaces.get(FILESYSTEM);
            let locked =
                interfaces.get(ENCRYPTED).is_some_and(|encrypted| object_path(encrypted, "CleartextDevice").is_none());
            if (filesystem.is_none() && !locked) || flag(block, "HintIgnore") || flag(block, "HintSystem") {
                return None;
            }
            let device = byte_path(block, "PreferredDevice").or_else(|| byte_path(block, "Device"))?;
            let backing = object_path(block, "CryptoBackingDevice");
            let backing_block = backing.as_deref().and_then(|backing| {
                objects
                    .iter()
                    .find(|(path, _)| path.as_str() == backing)
                    .and_then(|(_, interfaces)| interfaces.get(BLOCK))
            });
            let drive =
                object_path(block, "Drive").or_else(|| backing_block.and_then(|block| object_path(block, "Drive")));
            let label = Some(string(block, "IdLabel"))
                .filter(|label| !label.is_empty())
                .or_else(|| backing_block.map(|block| string(block, "IdLabel")))
                .unwrap_or_default();
            let drive_properties = drive
                .as_deref()
                .and_then(|drive| {
                    objects.iter().find(|(path, _)| path.as_str() == drive).map(|(_, interfaces)| interfaces)
                })
                .and_then(|interfaces| interfaces.get(DRIVE));
            let drive_flag = |key| drive_properties.is_some_and(|drive| flag(drive, key));
            Some(Volume {
                object: path.as_str().to_string(),
                device,
                label,
                fs_type: string(block, "IdType"),
                size: number(block, "Size"),
                mount: filesystem.and_then(|filesystem| mount_points(filesystem).into_iter().next()),
                drive,
                locked,
                backing,
                removable: drive_flag("Removable") || drive_flag("MediaRemovable"),
                can_power_off: drive_flag("CanPowerOff"),
                ejectable: drive_flag("Ejectable"),
            })
        })
        .collect();
    volumes.sort_by(|a, b| a.device.cmp(&b.device));
    volumes
}

fn options(pairs: &[(&'static str, &'static str)]) -> HashMap<&'static str, Value<'static>> {
    pairs.iter().map(|&(key, value)| (key, Value::from(value))).collect()
}

fn listen(sender: &iced::futures::channel::mpsc::UnboundedSender<()>) -> zbus::Result<()> {
    let connection = Connection::system()?;
    let rule =
        zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).sender(SERVICE)?.path_namespace(ROOT)?.build();
    let signals = zbus::blocking::MessageIterator::for_match_rule(rule, &connection, Some(64))?;
    if sender.unbounded_send(()).is_err() {
        return Ok(());
    }
    for signal in signals {
        if signal.is_err() {
            continue;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
        if sender.unbounded_send(()).is_err() {
            break;
        }
    }
    Ok(())
}

#[must_use]
pub fn changes() -> iced::futures::channel::mpsc::UnboundedReceiver<()> {
    let (sender, receiver) = iced::futures::channel::mpsc::unbounded();
    let _ = std::thread::Builder::new().name(String::from("glide-files-udisks")).spawn(move || {
        let _ = listen(&sender);
    });
    receiver
}

#[derive(Debug, Default)]
pub struct Disks {
    connection: Mutex<Option<Connection>>,
}

impl Disks {
    fn connection(&self) -> Result<Connection, DiskError> {
        let mut slot = self.connection.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(connection) = slot.as_ref() {
            return Ok(connection.clone());
        }
        let connection = Connection::system().map_err(|_| DiskError::Unavailable)?;
        *slot = Some(connection.clone());
        Ok(connection)
    }

    fn call<B, R>(&self, path: &str, interface: &str, method: &str, body: &B) -> Result<R, DiskError>
    where
        B: serde::Serialize + DynamicType,
        R: for<'d> DynamicDeserialize<'d>,
    {
        let connection = self.connection()?;
        let reply = connection.call_method(Some(SERVICE), path, Some(interface), method, body).map_err(|err| {
            let err = DiskError::from(err);
            if err == DiskError::Unavailable {
                *self.connection.lock().unwrap_or_else(PoisonError::into_inner) = None;
            }
            err
        })?;
        reply.body().deserialize().map_err(|err| DiskError::Failed(err.to_string()))
    }

    pub fn volumes(&self) -> Result<Vec<Volume>, DiskError> {
        let objects: Objects = self.call(ROOT, OBJECT_MANAGER, "GetManagedObjects", &())?;
        Ok(volumes_from(&objects))
    }

    pub fn mount(&self, volume: &Volume, read_only: bool) -> Result<Mounted, DiskError> {
        let mut last = DiskError::Unavailable;
        for attempt in mount_attempts(&volume.fs_type, read_only) {
            let mut pairs = Vec::new();
            if let Some(driver) = attempt.driver {
                pairs.push(("fstype", driver));
            }
            if attempt.read_only {
                pairs.push(("options", "ro"));
            }
            match self.call::<_, String>(&volume.object, FILESYSTEM, "Mount", &(options(&pairs),)) {
                Ok(path) => {
                    return Ok(Mounted { path: PathBuf::from(path), mode: attempt.mode(read_only) });
                }
                Err(err) if err.retryable() => last = err,
                Err(err) => return Err(err),
            }
        }
        Err(last)
    }

    pub(crate) fn unmount(&self, volume: &Volume) -> Result<(), DiskError> {
        self.call::<_, ()>(&volume.object, FILESYSTEM, "Unmount", &(options(&[]),))?;
        if let Some(backing) = &volume.backing {
            let _ = self.call::<_, ()>(backing, ENCRYPTED, "Lock", &(options(&[]),));
        }
        Ok(())
    }

    pub fn unlock(&self, volume: &Volume, passphrase: &str) -> Result<Mounted, DiskError> {
        let cleartext: OwnedObjectPath = self.call(&volume.object, ENCRYPTED, "Unlock", &(passphrase, options(&[])))?;
        let object = cleartext.as_str().to_string();
        for _ in 0..20 {
            let found = self.volumes()?.into_iter().find(|candidate| candidate.object == object);
            if let Some(opened) = found {
                return match opened.mount.clone() {
                    Some(path) => Ok(Mounted { path, mode: MountMode::Normal }),
                    None => self.mount(&opened, false),
                };
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Err(DiskError::Failed(String::from("no filesystem after unlocking")))
    }

    pub(crate) fn eject(&self, volume: &Volume) -> Result<(), DiskError> {
        let siblings: Vec<Volume> = self
            .volumes()?
            .into_iter()
            .filter(|other| {
                other.mount.is_some()
                    && (other.object == volume.object || (volume.drive.is_some() && other.drive == volume.drive))
            })
            .collect();
        for sibling in &siblings {
            self.unmount(sibling)?;
        }
        let Some(drive) = volume.drive.as_deref() else {
            return Ok(());
        };
        let method = if volume.can_power_off {
            "PowerOff"
        } else if volume.ejectable {
            "Eject"
        } else {
            return Ok(());
        };
        self.call::<_, ()>(drive, DRIVE, method, &(options(&[]),))
    }

    pub(crate) fn repair(&self, volume: &Volume) -> Result<Mounted, DiskError> {
        let (supported, tool): (bool, String) =
            self.call(MANAGER_PATH, MANAGER, "CanRepair", &(volume.fs_type.as_str(),))?;
        if !supported {
            return Err(DiskError::MissingTool(tool));
        }
        if volume.mount.is_some() {
            self.unmount(volume)?;
        }
        let repaired: bool = self.call(&volume.object, FILESYSTEM, "Repair", &(options(&[]),))?;
        if !repaired {
            return Err(DiskError::Unrepaired);
        }
        self.mount(volume, false)
    }
}

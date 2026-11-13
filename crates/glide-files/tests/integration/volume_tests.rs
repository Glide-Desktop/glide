use std::collections::HashMap;
use std::path::{Path, PathBuf};

use glide_files::fs::volumes::{Attempt, Interfaces, Objects, Properties};
use glide_files::fs::{DiskError, MountMode, mount_attempts, volumes_from};
use glide_files::i18n::{self, Lang};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

const DRIVE_PATH: &str = "/org/freedesktop/UDisks2/drives/Kingston_DataTraveler";

fn owned(value: Value<'_>) -> OwnedValue {
    OwnedValue::try_from(value).expect("owned value")
}

fn bytes(text: &str) -> Vec<u8> {
    let mut raw = text.as_bytes().to_vec();
    raw.push(0);
    raw
}

fn object(path: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(path).expect("object path")
}

fn block(device: &str, label: &str, fs_type: &str, drive: &str) -> Properties {
    HashMap::from([
        (String::from("Device"), owned(Value::from(bytes(device)))),
        (String::from("IdLabel"), owned(Value::from(label))),
        (String::from("IdType"), owned(Value::from(fs_type))),
        (String::from("Size"), owned(Value::from(31_457_280_000_u64))),
        (String::from("HintIgnore"), owned(Value::from(false))),
        (String::from("HintSystem"), owned(Value::from(false))),
        (String::from("Drive"), owned(Value::from(ObjectPath::try_from(drive).expect("drive")))),
    ])
}

fn filesystem(mounts: &[&str]) -> Properties {
    let points: Vec<Vec<u8>> = mounts.iter().map(|mount| bytes(mount)).collect();
    HashMap::from([(String::from("MountPoints"), owned(Value::from(points)))])
}

fn partition(block: Properties, filesystem: Option<Properties>) -> Interfaces {
    let mut interfaces = HashMap::from([(String::from("org.freedesktop.UDisks2.Block"), block)]);
    if let Some(filesystem) = filesystem {
        interfaces.insert(String::from("org.freedesktop.UDisks2.Filesystem"), filesystem);
    }
    interfaces
}

fn usb_drive() -> Interfaces {
    HashMap::from([(
        String::from("org.freedesktop.UDisks2.Drive"),
        HashMap::from([
            (String::from("Removable"), owned(Value::from(true))),
            (String::from("CanPowerOff"), owned(Value::from(true))),
            (String::from("Ejectable"), owned(Value::from(false))),
        ]),
    )])
}

fn objects() -> Objects {
    let mut system = block("/dev/nvme0n1p2", "", "ext4", "/");
    system.insert(String::from("HintSystem"), owned(Value::from(true)));
    HashMap::from([
        (object(DRIVE_PATH), usb_drive()),
        (
            object("/org/freedesktop/UDisks2/block_devices/sdb1"),
            partition(block("/dev/sdb1", "", "vfat", "/"), Some(filesystem(&["/run/media/artem/CARD"]))),
        ),
        (
            object("/org/freedesktop/UDisks2/block_devices/sda1"),
            partition(block("/dev/sda1", "redman", "ntfs", DRIVE_PATH), Some(filesystem(&[]))),
        ),
        (object("/org/freedesktop/UDisks2/block_devices/sda"), partition(block("/dev/sda", "", "", DRIVE_PATH), None)),
        (object("/org/freedesktop/UDisks2/block_devices/nvme0n1p2"), partition(system, Some(filesystem(&["/"])))),
    ])
}

#[test]
fn udisks_objects_become_user_visible_volumes_only() {
    let volumes = volumes_from(&objects());
    let devices: Vec<&Path> = volumes.iter().map(|v| v.device.as_path()).collect();
    assert_eq!(devices, [Path::new("/dev/sda1"), Path::new("/dev/sdb1")]);

    let usb = &volumes[0];
    assert_eq!(usb.object, "/org/freedesktop/UDisks2/block_devices/sda1");
    assert_eq!(usb.label, "redman");
    assert_eq!(usb.fs_type, "ntfs");
    assert_eq!(usb.mount, None);
    assert_eq!(usb.drive.as_deref(), Some(DRIVE_PATH));
    assert!(usb.removable && usb.can_power_off && !usb.ejectable);
    assert!(usb.can_remove());
    assert_eq!(usb.name(Lang::En), "redman");

    let card = &volumes[1];
    assert_eq!(card.mount, Some(PathBuf::from("/run/media/artem/CARD")), "mount point bytes lose their trailing NUL");
    assert_eq!(card.drive, None, "the root object path means no drive");
    assert!(!card.can_remove());
    assert!(card.name(Lang::En).ends_with(" Volume"), "{}", card.name(Lang::En));
    assert!(card.name(Lang::Uk).starts_with("Том "), "the noun leads in Ukrainian: {}", card.name(Lang::Uk));
}

#[test]
fn ntfs_mounts_fall_back_to_ntfs_3g_then_read_only() {
    let attempt = |driver, read_only| Attempt { driver, read_only };
    assert_eq!(
        mount_attempts("ntfs", false),
        [attempt(None, false), attempt(Some("ntfs"), false), attempt(None, true), attempt(Some("ntfs"), true),]
    );
    assert_eq!(mount_attempts("vfat", false), [attempt(None, false), attempt(None, true)]);
    assert_eq!(mount_attempts("ntfs", true), [attempt(None, true), attempt(Some("ntfs"), true)]);
}

#[test]
fn degraded_mounts_are_reported_unless_requested() {
    let attempt = |driver, read_only| Attempt { driver, read_only };
    assert_eq!(attempt(None, false).mode(false), MountMode::Normal);
    assert_eq!(attempt(Some("ntfs"), false).mode(false), MountMode::Fallback);
    assert_eq!(attempt(None, true).mode(false), MountMode::ReadOnly);
    assert_eq!(attempt(None, true).mode(true), MountMode::Normal);
    assert_eq!(attempt(Some("ntfs"), true).mode(true), MountMode::Fallback);
}

#[test]
fn udisks_errors_are_classified() {
    let classify = |name: &str| DiskError::classify(name, Some(" wrong fs type \n"));
    assert_eq!(classify("org.freedesktop.UDisks2.Error.NotAuthorizedDismissed"), DiskError::Cancelled);
    assert_eq!(classify("org.freedesktop.UDisks2.Error.NotAuthorizedCanObtain"), DiskError::NotAuthorized);
    assert_eq!(classify("org.freedesktop.UDisks2.Error.DeviceBusy"), DiskError::Busy);
    assert_eq!(classify("org.freedesktop.UDisks2.Error.Failed"), DiskError::Failed(String::from("wrong fs type")));
    assert_eq!(classify("org.freedesktop.DBus.Error.ServiceUnknown"), DiskError::Unavailable);
    assert_eq!(
        i18n::disk_error(&DiskError::MissingTool(String::from("ntfsfix")), Lang::En),
        "this tool isn’t installed: ntfsfix"
    );
}

fn encrypted(cleartext: &str) -> Properties {
    HashMap::from([(
        String::from("CleartextDevice"),
        owned(Value::from(ObjectPath::try_from(cleartext).expect("path"))),
    )])
}

#[test]
fn a_locked_luks_drive_is_listed_and_its_unlocked_filesystem_inherits_the_drive() {
    const LUKS: &str = "/org/freedesktop/UDisks2/block_devices/sdb1";
    const CLEAR: &str = "/org/freedesktop/UDisks2/block_devices/dm_2d0";
    let mut locked_block = block("/dev/sdb1", "Vault", "crypto_LUKS", DRIVE_PATH);
    locked_block.insert(String::from("Size"), owned(Value::from(1_000_u64)));
    let mut locked = partition(locked_block.clone(), None);
    locked.insert(String::from("org.freedesktop.UDisks2.Encrypted"), encrypted("/"));
    let objects: Objects = HashMap::from([(object(LUKS), locked), (object(DRIVE_PATH), usb_drive())]);
    let volumes = volumes_from(&objects);
    assert_eq!(volumes.len(), 1);
    assert!(volumes[0].locked);
    assert!(volumes[0].mount.is_none());

    let mut open = partition(locked_block, None);
    open.insert(String::from("org.freedesktop.UDisks2.Encrypted"), encrypted(CLEAR));
    let mut clear_block = block("/dev/dm-0", "", "ext4", "/");
    clear_block
        .insert(String::from("CryptoBackingDevice"), owned(Value::from(ObjectPath::try_from(LUKS).expect("path"))));
    let objects: Objects = HashMap::from([
        (object(LUKS), open),
        (object(CLEAR), partition(clear_block, Some(filesystem(&["/run/media/me/Vault"])))),
        (object(DRIVE_PATH), usb_drive()),
    ]);
    let volumes = volumes_from(&objects);
    assert_eq!(volumes.len(), 1, "the opened container itself is hidden");
    let clear = &volumes[0];
    assert!(!clear.locked);
    assert_eq!(clear.label, "Vault");
    assert_eq!(clear.drive.as_deref(), Some(DRIVE_PATH));
    assert_eq!(clear.backing.as_deref(), Some(LUKS));
    assert!(clear.can_remove());
}

#[test]
fn internal_partitions_are_offered_unless_the_system_uses_them() {
    let internal = |device: &str, label: &str, fs_type: &str, mounts: &[&str], kind: Option<&str>| {
        let mut properties = block(device, label, fs_type, "/");
        properties.insert(String::from("HintSystem"), owned(Value::from(true)));
        let mut interfaces = partition(properties, Some(filesystem(mounts)));
        if let Some(kind) = kind {
            interfaces.insert(
                String::from("org.freedesktop.UDisks2.Partition"),
                HashMap::from([(String::from("Type"), owned(Value::from(kind)))]),
            );
        }
        interfaces
    };
    let at = |name: &str| object(&format!("/org/freedesktop/UDisks2/block_devices/{name}"));
    let objects: Objects = HashMap::from([
        (at("nvme0n1p1"), internal("/dev/nvme0n1p1", "EFI", "vfat", &[], Some("C12A7328-F81F-11D2-BA4B-00A0C93EC93B"))),
        (at("nvme0n1p2"), internal("/dev/nvme0n1p2", "root", "ext4", &["/"], None)),
        (at("nvme0n1p3"), internal("/dev/nvme0n1p3", "home", "ext4", &["/home"], None)),
        (at("nvme0n1p4"), internal("/dev/nvme0n1p4", "Windows", "ntfs", &[], None)),
        (at("sdc1"), internal("/dev/sdc1", "Data", "ext4", &["/run/media/me/Data"], None)),
    ]);
    let shown: Vec<String> = volumes_from(&objects).into_iter().map(|volume| volume.label).collect();
    assert_eq!(shown, ["Windows", "Data"], "the firmware partition and what the system runs from stay hidden");
    assert_eq!(
        DiskError::classify("org.freedesktop.UDisks2.Error.AlreadyMounted", None),
        DiskError::AlreadyMounted,
        "a volume someone else mounted is opened, not retried with other drivers"
    );
}

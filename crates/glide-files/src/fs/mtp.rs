use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SYSFS: &str = "/sys/bus/usb/devices";
const FUSE_TOOLS: [&str; 2] = ["jmtpfs", "simple-mtpfs"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phone {
    pub id: String,
    pub name: String,
    pub serial: String,
    pub camera: bool,
    pub bus: u32,
    pub dev: u32,
    pub mount: Option<PathBuf>,
}

impl Phone {
    #[must_use]
    pub fn uri(&self) -> String {
        let scheme = if self.camera { "gphoto2" } else { "mtp" };
        format!("{scheme}://[usb:{:03},{:03}]/", self.bus, self.dev)
    }
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).map(|text| text.trim().to_string()).unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Mtp,
    Camera,
}

fn kind(device: &Path) -> Option<Kind> {
    let name = device.file_name()?.to_string_lossy().into_owned();
    let mut found = None;
    for interface in std::fs::read_dir(device).ok()?.flatten().map(|entry| entry.path()) {
        let is_interface =
            interface.file_name().is_some_and(|child| child.to_string_lossy().starts_with(&format!("{name}:")));
        if !is_interface {
            continue;
        }
        let label = read(&interface, "interface");
        if label.to_ascii_uppercase().contains("MTP") {
            return Some(Kind::Mtp);
        }
        if read(&interface, "bInterfaceClass") == "06" {
            found = Some(Kind::Camera);
        }
    }
    found
}

#[must_use]
pub fn devices_in(root: &Path) -> Vec<Phone> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut phones: Vec<Phone> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.file_name().is_some_and(|name| !name.to_string_lossy().contains(':')))
        .filter_map(|device| {
            let kind = kind(&device)?;
            let (bus, dev) = (read(&device, "busnum").parse().ok()?, read(&device, "devnum").parse().ok()?);
            let (maker, product) = (read(&device, "manufacturer"), read(&device, "product"));
            let name = match (maker.is_empty(), product.is_empty()) {
                (false, false) if !product.starts_with(&maker) => format!("{maker} {product}"),
                (_, false) => product,
                (false, true) => maker,
                (true, true) => String::new(),
            };
            Some(Phone {
                id: format!("{bus}:{dev}"),
                name,
                serial: read(&device, "serial"),
                camera: kind == Kind::Camera,
                bus,
                dev,
                mount: None,
            })
        })
        .collect();
    phones.sort_by(|a, b| a.id.cmp(&b.id));
    phones
}

fn gvfs_root() -> PathBuf {
    // SAFETY: getuid has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/run/user/{uid}/gvfs"))
}

fn fuse_root() -> Option<PathBuf> {
    glide_config::cache_home().map(|cache| cache.join("glide/mtp"))
}

fn fuse_dir(phone: &Phone) -> Option<PathBuf> {
    fuse_root().map(|root| root.join(phone.id.replace(':', "-")))
}

fn host_matches(entry: &str, phone: &Phone) -> bool {
    let scheme = if phone.camera { "gphoto2:host=" } else { "mtp:host=" };
    let Some(host) = entry.strip_prefix(scheme) else {
        return false;
    };
    let usb = format!("%5Busb%3A{:03}%2C{:03}%5D", phone.bus, phone.dev);
    host == usb || (!phone.serial.is_empty() && host.contains(&phone.serial))
}

fn mounted_fuse(dir: &Path) -> bool {
    std::fs::read_to_string("/proc/self/mounts").is_ok_and(|mounts| {
        mounts.lines().filter_map(|line| line.split(' ').nth(1)).any(|point| Path::new(point) == dir)
    })
}

#[must_use]
pub fn find_mount(phone: &Phone) -> Option<PathBuf> {
    if let Ok(entries) = std::fs::read_dir(gvfs_root()) {
        let found = entries
            .flatten()
            .find(|entry| host_matches(&entry.file_name().to_string_lossy(), phone))
            .map(|entry| entry.path());
        if found.is_some() {
            return found;
        }
    }
    fuse_dir(phone).filter(|dir| mounted_fuse(dir))
}

#[must_use]
pub fn scan() -> Vec<Phone> {
    let mut phones = devices_in(Path::new(SYSFS));
    for phone in &mut phones {
        phone.mount = find_mount(phone);
    }
    phones
}

fn installed(tool: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(tool).is_file()))
}

fn quiet(program: &str, args: &[&str]) -> Result<(), String> {
    let output =
        Command::new(program).args(args).stdin(Stdio::null()).output().map_err(|err| format!("{program}: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if text.is_empty() { format!("{program} failed") } else { text })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    NoTool,
    Failed(String),
}

pub fn open(phone: &Phone) -> Result<PathBuf, OpenError> {
    if let Some(path) = find_mount(phone) {
        return Ok(path);
    }
    if installed("gio") {
        quiet("gio", &["mount", &phone.uri()]).map_err(OpenError::Failed)?;
        return find_mount(phone).ok_or_else(|| OpenError::Failed(String::from("gio mount")));
    }
    let tool = FUSE_TOOLS.into_iter().find(|tool| installed(tool)).filter(|_| !phone.camera);
    let (Some(tool), Some(dir)) = (tool, fuse_dir(phone)) else {
        return Err(OpenError::NoTool);
    };
    std::fs::create_dir_all(&dir).map_err(|err| OpenError::Failed(err.to_string()))?;
    let target = dir.to_string_lossy().into_owned();
    let device = format!("-device={},{}", phone.bus, phone.dev);
    let args: Vec<&str> = if tool == "jmtpfs" { vec![&device, &target] } else { vec![&target] };
    quiet(tool, &args).map_err(OpenError::Failed)?;
    Ok(dir)
}

pub fn close(phone: &Phone) -> Result<(), String> {
    let Some(mount) = phone.mount.clone().or_else(|| find_mount(phone)) else {
        return Ok(());
    };
    if mount.starts_with(gvfs_root()) {
        return quiet("gio", &["mount", "-u", &super::file_uri(&mount)]);
    }
    let target = mount.to_string_lossy().into_owned();
    let unmounted = quiet("fusermount3", &["-u", &target]).or_else(|_| quiet("fusermount", &["-u", &target]));
    let _ = std::fs::remove_dir(&mount);
    unmounted
}

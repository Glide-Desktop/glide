use std::os::fd::RawFd;
use std::path::Path;

use super::CameraError;
use super::v4l2::{
    Capability, V4L2_CAP_DEVICE_CAPS, V4L2_CAP_VIDEO_CAPTURE, V4L2_CAP_VIDEO_M2M, V4L2_CAP_VIDEO_M2M_MPLANE,
    V4L2_CAP_VIDEO_OUTPUT, V4L2_CAP_VIDEO_OUTPUT_MPLANE,
};

pub const VIDEO_MAJOR: u64 = 81;

pub const VIRTUAL_DRIVER_MARKERS: [&str; 9] =
    ["loopback", "akvcam", "vivid", "vimc", "vicodec", "vim2m", "visl", "virtual", "dummy"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceIdentity {
    pub driver: String,
    pub card: String,
    pub bus_info: String,
    pub caps: u32,
    pub sysfs_path: String,
    pub has_parent: bool,
}

fn untrusted(msg: String) -> CameraError {
    CameraError::Untrusted(msg)
}

pub fn check_device_identity(id: &DeviceIdentity) -> Result<(), CameraError> {
    let driver = id.driver.to_lowercase();
    if VIRTUAL_DRIVER_MARKERS.iter().any(|m| driver.contains(m)) {
        return Err(untrusted(format!("virtual V4L2 driver {:?} ({})", id.driver, id.card)));
    }
    if id.bus_info.to_lowercase().contains("loopback") {
        return Err(untrusted(format!("loopback bus {:?}", id.bus_info)));
    }
    if id.caps & V4L2_CAP_VIDEO_CAPTURE == 0 {
        return Err(untrusted("node does not support video capture".into()));
    }
    let writable =
        V4L2_CAP_VIDEO_OUTPUT | V4L2_CAP_VIDEO_OUTPUT_MPLANE | V4L2_CAP_VIDEO_M2M | V4L2_CAP_VIDEO_M2M_MPLANE;
    if id.caps & writable != 0 {
        return Err(untrusted("node also accepts frames from userspace (output/m2m)".into()));
    }
    if id.sysfs_path.is_empty() {
        return Err(untrusted("cannot resolve sysfs entry".into()));
    }
    if id.sysfs_path.contains("/devices/virtual/") {
        return Err(untrusted(format!("virtual sysfs device {}", id.sysfs_path)));
    }
    if !id.has_parent {
        return Err(untrusted(format!("no backing hardware device for {}", id.sysfs_path)));
    }
    Ok(())
}

pub fn validate_opened_device(fd: RawFd, cap: &Capability) -> Result<(), CameraError> {
    // SAFETY: `libc::stat` is plain old data, for which all-zero bytes are a valid value.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `st` is a valid, writable `stat` buffer for the duration of the call.
    if unsafe { libc::fstat(fd, &raw mut st) } != 0 {
        return Err(CameraError::Device(format!("fstat video device: {}", std::io::Error::last_os_error())));
    }
    if st.st_mode & libc::S_IFMT != libc::S_IFCHR {
        return Err(untrusted("not a character device".into()));
    }
    let (major, minor) = (libc::major(st.st_rdev) as u64, libc::minor(st.st_rdev) as u64);
    if major != VIDEO_MAJOR {
        return Err(untrusted(format!("not a video4linux device (major {major})")));
    }

    let mut id = DeviceIdentity::default();
    if let Ok(resolved) = std::fs::canonicalize(format!("/sys/dev/char/{major}:{minor}")) {
        id.has_parent = Path::new(&resolved).join("device").exists();
        id.sysfs_path = resolved.to_string_lossy().into_owned();
    }
    id.driver = cap.driver();
    id.card = cap.card();
    id.bus_info = cap.bus_info();
    id.caps = if cap.capabilities & V4L2_CAP_DEVICE_CAPS != 0 { cap.device_caps } else { cap.capabilities };
    check_device_identity(&id)
}

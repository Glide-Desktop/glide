use std::time::Duration;

use glide_face::camera::v4l2::{
    V4L2_CAP_STREAMING, V4L2_CAP_VIDEO_CAPTURE, V4L2_CAP_VIDEO_M2M, V4L2_CAP_VIDEO_OUTPUT, probe,
};
use glide_face::camera::validate::{DeviceIdentity, check_device_identity};
use glide_face::camera::{Camera, CameraError, MockCamera, V4l2Camera};

type Mutation = Box<dyn Fn(&mut DeviceIdentity)>;

#[test]
fn mock_camera_no_duplicate_frames() {
    let mut cam = MockCamera::new(640, 480);
    cam.start().unwrap();
    let f1 = cam.wait_frame(Duration::from_millis(500)).unwrap();
    let f2 = cam.wait_frame(Duration::from_millis(500)).unwrap();
    assert!(f2.sequence > f1.sequence, "expected a newer frame");
    cam.stop().unwrap();
    assert!(!cam.is_running());
    assert_eq!(cam.wait_frame(Duration::from_millis(50)).unwrap_err(), CameraError::NotRunning);
}

#[test]
fn v4l2_not_opened_until_start() {
    let mut cam = V4l2Camera::new("/dev/video999", 640, 480);
    assert!(!cam.is_open(), "device must not be opened in constructor");
    assert!(cam.start().is_err(), "expected error for missing device");
    assert!(!cam.is_running());
    assert!(!cam.is_open());
}

#[test]
fn device_identity_checks() {
    let uvc = DeviceIdentity {
        driver: "uvcvideo".into(),
        card: "Integrated Camera".into(),
        bus_info: "usb-0000:00:14.0-6".into(),
        caps: V4L2_CAP_VIDEO_CAPTURE | V4L2_CAP_STREAMING,
        sysfs_path: "/sys/devices/pci0000:00/0000:00:14.0/usb1/1-6/1-6:1.0/video4linux/video0".into(),
        has_parent: true,
    };
    check_device_identity(&uvc).expect("real UVC camera must be accepted");
    let cases: Vec<(&str, Mutation)> = vec![
        (
            "v4l2loopback driver",
            Box::new(|id| {
                id.driver = "v4l2 loopback".into();
                id.card = "OBS Virtual Camera".into();
                id.bus_info = "platform:v4l2loopback-000".into();
            }),
        ),
        ("akvcam driver", Box::new(|id| id.driver = "akvcam".into())),
        ("vivid test driver", Box::new(|id| id.driver = "vivid".into())),
        ("loopback bus", Box::new(|id| id.bus_info = "platform:v4l2loopback-003".into())),
        ("output capable", Box::new(|id| id.caps |= V4L2_CAP_VIDEO_OUTPUT)),
        ("m2m node", Box::new(|id| id.caps |= V4L2_CAP_VIDEO_M2M)),
        ("no capture", Box::new(|id| id.caps = V4L2_CAP_STREAMING)),
        ("virtual sysfs node", Box::new(|id| id.sysfs_path = "/sys/devices/virtual/video4linux/video10".into())),
        ("no hardware parent", Box::new(|id| id.has_parent = false)),
        ("unresolved sysfs", Box::new(|id| id.sysfs_path.clear())),
    ];
    for (name, mutate) in cases {
        let mut id = uvc.clone();
        mutate(&mut id);
        assert!(matches!(check_device_identity(&id), Err(CameraError::Untrusted(_))), "{name}: expected Untrusted");
    }
}

#[test]
fn a_missing_node_is_not_listed_as_a_camera() {
    assert_eq!(probe("/dev/video999"), None);
}

#[test]
fn v4l2_hardware_start_stop() {
    if std::env::var("GLIDE_FACE_HW_TEST").as_deref() != Ok("1") {
        eprintln!("set GLIDE_FACE_HW_TEST=1 to run against /dev/video0");
        return;
    }
    let mut cam = V4l2Camera::new("/dev/video0", 640, 480);
    for i in 0..2 {
        cam.start().unwrap_or_else(|e| panic!("cycle {i} start: {e}"));
        let f = cam.wait_frame(Duration::from_secs(3)).unwrap_or_else(|e| panic!("cycle {i} frame: {e}"));
        eprintln!("cycle {i}: {}x{} {} seq={} decode={:?}", f.width, f.height, f.format, f.sequence, f.capture_time);
        cam.stop().unwrap();
        assert!(!cam.is_open(), "device still open after stop");
    }
}

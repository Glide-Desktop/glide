use std::ffi::CString;
use std::os::fd::RawFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::validate::validate_opened_device;
use super::{Camera, CameraError, Frame, FrameSlot};
use crate::biometrics::RgbaImage;
use glide_config::facekey::Camera as CameraInfo;

pub const V4L2_CAP_VIDEO_CAPTURE: u32 = 0x0000_0001;
pub const V4L2_CAP_VIDEO_OUTPUT: u32 = 0x0000_0002;
pub const V4L2_CAP_VIDEO_M2M_MPLANE: u32 = 0x0000_4000;
pub const V4L2_CAP_VIDEO_OUTPUT_MPLANE: u32 = 0x0000_2000;
pub const V4L2_CAP_VIDEO_M2M: u32 = 0x0000_8000;
pub const V4L2_CAP_STREAMING: u32 = 0x0400_0000;
pub const V4L2_CAP_DEVICE_CAPS: u32 = 0x8000_0000;

const V4L2_BUF_TYPE_VIDEO_CAPTURE: u32 = 1;
const V4L2_MEMORY_MMAP: u32 = 1;
const V4L2_FIELD_ANY: u32 = 0;

const fn fourcc(a: u8, b: u8, c: u8, d: u8) -> u32 {
    (a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)
}
pub const PIX_FMT_MJPEG: u32 = fourcc(b'M', b'J', b'P', b'G');
pub const PIX_FMT_YUYV: u32 = fourcc(b'Y', b'U', b'Y', b'V');

const VIDIOC_QUERYCAP: libc::c_ulong = 0x8068_5600;
const VIDIOC_G_FMT: libc::c_ulong = 0xc0d0_5604;
const VIDIOC_S_FMT: libc::c_ulong = 0xc0d0_5605;
const VIDIOC_REQBUFS: libc::c_ulong = 0xc014_5608;
const VIDIOC_QUERYBUF: libc::c_ulong = 0xc058_5609;
const VIDIOC_QBUF: libc::c_ulong = 0xc058_560f;
const VIDIOC_DQBUF: libc::c_ulong = 0xc058_5611;
const VIDIOC_STREAMON: libc::c_ulong = 0x4004_5612;
const VIDIOC_STREAMOFF: libc::c_ulong = 0x4004_5613;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Capability {
    pub driver_raw: [u8; 16],
    pub card_raw: [u8; 32],
    pub bus_info_raw: [u8; 32],
    pub version: u32,
    pub capabilities: u32,
    pub device_caps: u32,
    pub reserved: [u32; 3],
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

impl Capability {
    #[must_use]
    pub fn driver(&self) -> String {
        cstr(&self.driver_raw)
    }

    #[must_use]
    pub fn card(&self) -> String {
        cstr(&self.card_raw)
    }

    #[must_use]
    pub fn bus_info(&self) -> String {
        cstr(&self.bus_info_raw)
    }
}

#[repr(C)]
struct Format {
    type_: u32,
    _pad: u32,
    fmt: [u32; 50],
}

#[repr(C)]
struct RequestBuffers {
    count: u32,
    type_: u32,
    memory: u32,
    capabilities: u32,
    flags_reserved: u32,
}

#[repr(C)]
struct Buffer {
    index: u32,
    type_: u32,
    bytesused: u32,
    flags: u32,
    field: u32,
    timestamp: libc::timeval,
    timecode: [u32; 4],
    sequence: u32,
    memory: u32,
    m: u64,
    length: u32,
    reserved2: u32,
    request_fd: u32,
}

const _: () = assert!(std::mem::size_of::<Capability>() == 104);
const _: () = assert!(std::mem::size_of::<Format>() == 208);
const _: () = assert!(std::mem::size_of::<RequestBuffers>() == 20);
const _: () = assert!(std::mem::size_of::<Buffer>() == 88);

fn xioctl<T>(fd: RawFd, req: libc::c_ulong, arg: &mut T) -> std::io::Result<()> {
    loop {
        // SAFETY: `arg` is a valid, exclusively borrowed value of the struct the request expects.
        let r = unsafe { libc::ioctl(fd, req as _, std::ptr::from_mut::<T>(arg)) };
        if r != -1 {
            return Ok(());
        }
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::EINTR) {
            return Err(e);
        }
    }
}

fn zeroed<T>() -> T {
    // SAFETY: only used for the `repr(C)` V4L2 structs above, which are plain integers and arrays.
    unsafe { std::mem::zeroed() }
}

struct Mapping {
    ptr: *mut libc::c_void,
    len: usize,
}

struct Stream {
    fd: RawFd,
    maps: Vec<Mapping>,
    streaming: bool,
    width: usize,
    height: usize,
    pixfmt: u32,
}

// SAFETY: the mappings are only touched by the capture thread that owns the stream.
unsafe impl Send for Stream {}

impl Drop for Stream {
    fn drop(&mut self) {
        if self.streaming {
            let mut t = V4L2_BUF_TYPE_VIDEO_CAPTURE as i32;
            let _ = xioctl(self.fd, VIDIOC_STREAMOFF, &mut t);
        }
        for m in self.maps.drain(..) {
            // SAFETY: `m` is a live mapping created by `mmap` with this length, unmapped once.
            unsafe { libc::munmap(m.ptr, m.len) };
        }
        let mut req = RequestBuffers {
            count: 0,
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: V4L2_MEMORY_MMAP,
            capabilities: 0,
            flags_reserved: 0,
        };
        let _ = xioctl(self.fd, VIDIOC_REQBUFS, &mut req);
        // SAFETY: the stream owns `fd` and closes it exactly once, here.
        unsafe { libc::close(self.fd) };
    }
}

enum OpenOutcome {
    Ok(Stream),
    Retry(String),
    Fatal(CameraError),
}

fn open_with_format(path: &str, w: usize, h: usize, pixfmt: u32) -> OpenOutcome {
    let Ok(cpath) = CString::new(path) else {
        return OpenOutcome::Fatal(CameraError::Device(format!("invalid path {path:?}")));
    };
    // SAFETY: `cpath` is a valid NUL-terminated string that outlives the call.
    let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK | libc::O_CLOEXEC) };
    if fd < 0 {
        return OpenOutcome::Retry(std::io::Error::last_os_error().to_string());
    }
    let mut stream = Stream { fd, maps: Vec::new(), streaming: false, width: w, height: h, pixfmt };

    let mut cap: Capability = zeroed();
    if let Err(e) = xioctl(fd, VIDIOC_QUERYCAP, &mut cap) {
        return OpenOutcome::Retry(format!("query capabilities: {e}"));
    }
    if let Err(e) = validate_opened_device(fd, &cap) {
        return OpenOutcome::Fatal(e);
    }

    let mut fmt: Format = zeroed();
    fmt.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
    fmt.fmt[0] = w as u32;
    fmt.fmt[1] = h as u32;
    fmt.fmt[2] = pixfmt;
    fmt.fmt[3] = V4L2_FIELD_ANY;
    if let Err(e) = xioctl(fd, VIDIOC_S_FMT, &mut fmt) {
        return OpenOutcome::Retry(format!("set format: {e}"));
    }
    let mut got: Format = zeroed();
    got.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
    if xioctl(fd, VIDIOC_G_FMT, &mut got).is_err() || got.fmt[2] != pixfmt {
        return OpenOutcome::Retry("driver selected another pixel format".into());
    }
    stream.width = got.fmt[0] as usize;
    stream.height = got.fmt[1] as usize;

    let mut req = RequestBuffers {
        count: 2,
        type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
        memory: V4L2_MEMORY_MMAP,
        capabilities: 0,
        flags_reserved: 0,
    };
    if let Err(e) = xioctl(fd, VIDIOC_REQBUFS, &mut req) {
        return OpenOutcome::Fatal(CameraError::Device(format!("request buffers: {e}")));
    }
    for i in 0..req.count {
        let mut buf: Buffer = zeroed();
        buf.index = i;
        buf.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        buf.memory = V4L2_MEMORY_MMAP;
        if let Err(e) = xioctl(fd, VIDIOC_QUERYBUF, &mut buf) {
            return OpenOutcome::Fatal(CameraError::Device(format!("query buffer: {e}")));
        }
        // SAFETY: maps the driver buffer described by QUERYBUF; the result is checked below.
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                buf.length as usize,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                libc::off_t::from(buf.m as u32),
            )
        };
        if ptr == libc::MAP_FAILED {
            return OpenOutcome::Fatal(CameraError::Device(format!("mmap: {}", std::io::Error::last_os_error())));
        }
        stream.maps.push(Mapping { ptr, len: buf.length as usize });
        if let Err(e) = xioctl(fd, VIDIOC_QBUF, &mut buf) {
            return OpenOutcome::Fatal(CameraError::Device(format!("queue buffer: {e}")));
        }
    }
    let mut t = V4L2_BUF_TYPE_VIDEO_CAPTURE as i32;
    if let Err(e) = xioctl(fd, VIDIOC_STREAMON, &mut t) {
        return OpenOutcome::Fatal(CameraError::Device(format!("start V4L2 stream: {e}")));
    }
    stream.streaming = true;
    OpenOutcome::Ok(stream)
}

pub struct V4l2Camera {
    device_path: String,
    requested_w: usize,
    requested_h: usize,
    actual_w: usize,
    actual_h: usize,
    format_name: &'static str,
    slot: Arc<FrameSlot>,
    stop: Arc<AtomicBool>,
    seq: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl V4l2Camera {
    #[must_use]
    pub fn new(device: &str, width: usize, height: usize) -> Self {
        let w = if width == 0 { 640 } else { width };
        let h = if height == 0 { 480 } else { height };
        Self {
            device_path: if device.is_empty() { "/dev/video0".into() } else { device.into() },
            requested_w: w,
            requested_h: h,
            actual_w: w,
            actual_h: h,
            format_name: "",
            slot: Arc::new(FrameSlot::default()),
            stop: Arc::new(AtomicBool::new(false)),
            seq: Arc::new(AtomicU64::new(0)),
            thread: None,
        }
    }

    #[must_use]
    pub fn is_open(&self) -> bool {
        self.thread.is_some()
    }

    fn open_device(&self) -> Result<(Stream, &'static str), CameraError> {
        let mut first_err: Option<String> = None;
        for (pf, name) in [(PIX_FMT_MJPEG, "MJPEG"), (PIX_FMT_YUYV, "YUYV")] {
            match open_with_format(&self.device_path, self.requested_w, self.requested_h, pf) {
                OpenOutcome::Ok(s) => return Ok((s, name)),
                OpenOutcome::Retry(e) => {
                    first_err.get_or_insert(e);
                }
                OpenOutcome::Fatal(CameraError::Untrusted(e)) => {
                    return Err(CameraError::Untrusted(format!("open {}: {e}", self.device_path)));
                }
                OpenOutcome::Fatal(e) => return Err(CameraError::Device(format!("open {}: {e}", self.device_path))),
            }
        }
        let e = first_err.unwrap_or_else(|| "no supported pixel format (MJPEG/YUYV)".into());
        Err(CameraError::Device(format!("open {}: {e}", self.device_path)))
    }
}

fn capture_loop(stream: &Stream, name: &'static str, slot: &FrameSlot, stop: &AtomicBool, seq: &AtomicU64) {
    let (w, h) = (stream.width, stream.height);
    let mut raw = Vec::new();
    while !stop.load(Ordering::SeqCst) {
        let mut pfd = libc::pollfd { fd: stream.fd, events: libc::POLLIN, revents: 0 };
        // SAFETY: `pfd` is one valid, writable pollfd for the duration of the call.
        let r = unsafe { libc::poll(&raw mut pfd, 1, 100) };
        if r < 0 {
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break;
        }
        if r == 0 {
            continue;
        }
        if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 && pfd.revents & libc::POLLIN == 0 {
            slot.fail(CameraError::Device("camera stream ended unexpectedly".into()));
            return;
        }
        let mut buf: Buffer = zeroed();
        buf.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        buf.memory = V4L2_MEMORY_MMAP;
        if let Err(e) = xioctl(stream.fd, VIDIOC_DQBUF, &mut buf) {
            if e.raw_os_error() == Some(libc::EAGAIN) {
                continue;
            }
            slot.fail(CameraError::Device("camera stream ended unexpectedly".into()));
            return;
        }
        let start = Instant::now();
        let len = (buf.bytesused as usize).min(stream.maps.get(buf.index as usize).map_or(0, |m| m.len));
        if let Some(m) = stream.maps.get(buf.index as usize) {
            raw.clear();
            // SAFETY: the dequeued buffer is mapped for `m.len` bytes and `len` never exceeds it.
            raw.extend_from_slice(unsafe { std::slice::from_raw_parts(m.ptr as *const u8, len) });
        }
        if xioctl(stream.fd, VIDIOC_QBUF, &mut buf).is_err() {
            slot.fail(CameraError::Device("camera stream ended unexpectedly".into()));
            return;
        }
        if raw.is_empty() {
            continue;
        }
        let image = match stream.pixfmt {
            PIX_FMT_MJPEG => match RgbaImage::decode(&raw) {
                Ok(img) => img,
                Err(_) => continue,
            },
            PIX_FMT_YUYV => {
                if raw.len() < w * h * 2 {
                    continue;
                }
                let mut img = RgbaImage::new(w, h);
                decode_yuyv_to_rgba(&raw, &mut img.pix, w, h);
                img
            }
            _ => continue,
        };
        let (fw, fh) = (image.width, image.height);
        slot.publish(Frame {
            image: Arc::new(image),
            width: fw,
            height: fh,
            format: name,
            timestamp: start,
            sequence: seq.fetch_add(1, Ordering::SeqCst) + 1,
            capture_time: start.elapsed(),
        });
    }
}

pub fn decode_yuyv_to_rgba(src: &[u8], dst: &mut [u8], w: usize, h: usize) {
    let clamp = |v: i32| v.clamp(0, 255) as u8;
    for (i, (s, d)) in src.chunks_exact(4).zip(dst.chunks_exact_mut(8)).enumerate() {
        if i >= w * h / 2 {
            break;
        }
        let (y0, u, y1, v) = (i32::from(s[0]), i32::from(s[1]) - 128, i32::from(s[2]), i32::from(s[3]) - 128);
        let rv = (1436 * v) >> 10;
        let guv = (352 * u + 731 * v) >> 10;
        let bu = (1815 * u) >> 10;
        d.copy_from_slice(&[
            clamp(y0 + rv),
            clamp(y0 - guv),
            clamp(y0 + bu),
            255,
            clamp(y1 + rv),
            clamp(y1 - guv),
            clamp(y1 + bu),
            255,
        ]);
    }
}

impl Camera for V4l2Camera {
    fn start(&mut self) -> Result<(), CameraError> {
        if self.thread.is_some() && self.slot.is_running() {
            return Ok(());
        }
        self.stop()?;
        self.slot.drain();
        let (stream, name) = self.open_device()?;
        self.actual_w = stream.width;
        self.actual_h = stream.height;
        self.format_name = name;
        self.stop.store(false, Ordering::SeqCst);
        self.slot.set_running(true);
        let (slot, stop, seq) = (self.slot.clone(), self.stop.clone(), self.seq.clone());
        self.thread = Some(
            std::thread::Builder::new()
                .name("v4l2-capture".into())
                .spawn(move || capture_loop(&stream, name, &slot, &stop, &seq))
                .map_err(|e| CameraError::Device(format!("spawn capture thread: {e}")))?,
        );
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CameraError> {
        let Some(t) = self.thread.take() else {
            return Ok(());
        };
        let had_error = self.slot.error();
        self.stop.store(true, Ordering::SeqCst);
        let _ = t.join();
        if let Some(e) = had_error {
            self.slot.fail(e);
        } else {
            self.slot.set_running(false);
        }
        self.slot.drain();
        Ok(())
    }

    fn try_frame(&self) -> Option<Frame> {
        self.slot.take()
    }

    fn wait_frame(&self, timeout: Duration) -> Result<Frame, CameraError> {
        self.slot.wait(timeout)
    }

    fn err(&self) -> Option<CameraError> {
        self.slot.error()
    }

    fn device_name(&self) -> String {
        self.device_path.clone()
    }

    fn resolution(&self) -> (usize, usize) {
        (self.actual_w, self.actual_h)
    }

    fn pixel_format(&self) -> String {
        self.format_name.to_string()
    }

    fn is_running(&self) -> bool {
        self.slot.is_running()
    }
}

impl Drop for V4l2Camera {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[must_use]
pub fn probe(path: &str) -> Option<CameraInfo> {
    let cpath = CString::new(path).ok()?;
    // SAFETY: `cpath` is a valid NUL-terminated string that outlives the call.
    let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) };
    if fd < 0 {
        return None;
    }
    let mut cap: Capability = zeroed();
    let queried = xioctl(fd, VIDIOC_QUERYCAP, &mut cap);
    let verdict = queried.as_ref().ok().map(|()| validate_opened_device(fd, &cap));
    // SAFETY: `fd` was opened above and is closed exactly once here.
    unsafe { libc::close(fd) };
    queried.ok()?;
    let caps = if cap.capabilities & V4L2_CAP_DEVICE_CAPS != 0 { cap.device_caps } else { cap.capabilities };
    if caps & V4L2_CAP_VIDEO_CAPTURE == 0 {
        return None;
    }
    let refusal = verdict.and_then(Result::err).map(|error| error.to_string());
    Some(CameraInfo { path: path.to_string(), name: cap.card(), trusted: refusal.is_none(), refusal })
}

#[must_use]
pub fn list() -> Vec<CameraInfo> {
    let mut nodes: Vec<(u32, String)> = std::fs::read_dir("/dev")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let index = name.strip_prefix("video")?.parse().ok()?;
            Some((index, format!("/dev/{name}")))
        })
        .collect();
    nodes.sort();
    nodes.into_iter().filter_map(|(_, path)| probe(&path)).collect()
}

pub mod mock;
pub mod v4l2;
pub mod validate;

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::biometrics::RgbaImage;

pub use mock::MockCamera;
pub use v4l2::V4l2Camera;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CameraError {
    #[error("camera is not running")]
    NotRunning,
    #[error("camera frame timeout")]
    Timeout,
    #[error("untrusted video device: {0}")]
    Untrusted(String),
    #[error("{0}")]
    Device(String),
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub image: Arc<RgbaImage>,
    pub width: usize,
    pub height: usize,
    pub format: &'static str,
    pub timestamp: Instant,
    pub sequence: u64,
    pub capture_time: Duration,
}

pub trait Camera: Send {
    fn start(&mut self) -> Result<(), CameraError>;
    fn stop(&mut self) -> Result<(), CameraError>;
    fn try_frame(&self) -> Option<Frame>;
    fn wait_frame(&self, timeout: Duration) -> Result<Frame, CameraError>;
    fn err(&self) -> Option<CameraError>;
    fn device_name(&self) -> String;
    fn resolution(&self) -> (usize, usize);
    fn pixel_format(&self) -> String;
    fn is_running(&self) -> bool;
}

pub fn open(device: &str, width: usize, height: usize) -> Result<Box<dyn Camera>, CameraError> {
    if device == "mock" {
        return Ok(Box::new(MockCamera::new(width, height)));
    }
    Ok(Box::new(V4l2Camera::new(device, width, height)))
}

#[derive(Default)]
pub(crate) struct FrameSlot {
    inner: Mutex<SlotState>,
    cond: Condvar,
}

#[derive(Default)]
struct SlotState {
    frame: Option<Frame>,
    error: Option<CameraError>,
    running: bool,
}

impl FrameSlot {
    fn state(&self) -> MutexGuard<'_, SlotState> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn publish(&self, f: Frame) {
        let mut s = self.state();
        s.frame = Some(f);
        self.cond.notify_all();
    }

    pub(crate) fn take(&self) -> Option<Frame> {
        self.state().frame.take()
    }

    pub(crate) fn set_running(&self, running: bool) {
        let mut s = self.state();
        s.running = running;
        if running {
            s.error = None;
        }
        self.cond.notify_all();
    }

    pub(crate) fn fail(&self, err: CameraError) {
        let mut s = self.state();
        s.error = Some(err);
        s.running = false;
        self.cond.notify_all();
    }

    pub(crate) fn drain(&self) {
        self.state().frame = None;
    }

    pub(crate) fn is_running(&self) -> bool {
        self.state().running
    }

    pub(crate) fn error(&self) -> Option<CameraError> {
        self.state().error.clone()
    }

    pub(crate) fn wait(&self, timeout: Duration) -> Result<Frame, CameraError> {
        let deadline = Instant::now() + timeout;
        let mut s = self.state();
        loop {
            if !s.running {
                return Err(s.error.clone().unwrap_or(CameraError::NotRunning));
            }
            if let Some(f) = s.frame.take() {
                return Ok(f);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(s.error.clone().unwrap_or(CameraError::Timeout));
            }
            s = self.cond.wait_timeout(s, deadline - now).unwrap_or_else(PoisonError::into_inner).0;
        }
    }
}

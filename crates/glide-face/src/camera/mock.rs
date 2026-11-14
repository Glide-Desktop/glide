use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::{Camera, CameraError, Frame, FrameSlot};
use crate::biometrics::RgbaImage;

pub struct MockCamera {
    w: usize,
    h: usize,
    slot: Arc<FrameSlot>,
    stop: Arc<AtomicBool>,
    seq: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl MockCamera {
    #[must_use]
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w: if w == 0 { 640 } else { w },
            h: if h == 0 { 480 } else { h },
            slot: Arc::new(FrameSlot::default()),
            stop: Arc::new(AtomicBool::new(false)),
            seq: Arc::new(AtomicU64::new(0)),
            thread: None,
        }
    }
}

impl Camera for MockCamera {
    fn start(&mut self) -> Result<(), CameraError> {
        if self.thread.is_some() {
            return Ok(());
        }
        self.stop.store(false, Ordering::SeqCst);
        self.slot.set_running(true);
        let (slot, stop, seq, w, h) = (self.slot.clone(), self.stop.clone(), self.seq.clone(), self.w, self.h);
        self.thread = Some(std::thread::spawn(move || {
            let mut base = RgbaImage::new(w, h);
            for px in base.pix.chunks_exact_mut(4) {
                px.copy_from_slice(&[20, 25, 35, 255]);
            }
            let base = Arc::new(base);
            while !stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(33));
                slot.publish(Frame {
                    image: Arc::new((*base).clone()),
                    width: w,
                    height: h,
                    format: "MOCK",
                    timestamp: Instant::now(),
                    sequence: seq.fetch_add(1, Ordering::SeqCst) + 1,
                    capture_time: Duration::ZERO,
                });
            }
        }));
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CameraError> {
        let Some(t) = self.thread.take() else { return Ok(()) };
        self.slot.set_running(false);
        self.stop.store(true, Ordering::SeqCst);
        let _ = t.join();
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
        None
    }

    fn device_name(&self) -> String {
        "mock".into()
    }

    fn resolution(&self) -> (usize, usize) {
        (self.w, self.h)
    }

    fn pixel_format(&self) -> String {
        "MOCK".into()
    }

    fn is_running(&self) -> bool {
        self.slot.is_running()
    }
}

impl Drop for MockCamera {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

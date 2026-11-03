use std::time::{Duration, Instant};

use smithay::{
    backend::{
        drm::DrmDeviceFd,
        renderer::gles::{GlesRenderer, ffi},
    },
    reexports::drm::Device as _,
};

pub const SUMMARY_PERIOD: Duration = Duration::from_secs(10);

fn gl_string(gl: &ffi::Gles2, name: ffi::types::GLenum) -> String {
    // SAFETY: `GetString` is called on the renderer's current context with a valid enum; the returned pointer is a
    // NUL-terminated string owned by the driver that stays valid while the context lives, and it is copied at once.
    unsafe {
        let ptr = gl.GetString(name);
        if ptr.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr(ptr.cast()).to_string_lossy().into_owned()
    }
}

#[must_use]
pub fn vendor_family(driver: &str, gl_vendor: &str) -> &'static str {
    let driver = driver.to_ascii_lowercase();
    let vendor = gl_vendor.to_ascii_lowercase();
    if driver.contains("nvidia") || vendor.contains("nvidia") {
        "nvidia"
    } else if driver == "amdgpu" || driver == "radeon" || vendor.contains("amd") || vendor.contains("ati") {
        "amd"
    } else if driver == "i915" || driver == "xe" || vendor.contains("intel") {
        "intel"
    } else {
        "other"
    }
}

pub fn log_device(device: &DrmDeviceFd, renderer: &mut GlesRenderer, explicit_sync: bool, dmabuf_formats: usize) {
    let (driver, driver_desc, driver_date) = device.get_driver().map_or_else(
        |err| (format!("unknown ({err})"), String::new(), String::new()),
        |driver| {
            (
                driver.name().to_string_lossy().into_owned(),
                driver.description().to_string_lossy().into_owned(),
                driver.date().to_string_lossy().into_owned(),
            )
        },
    );
    let gl = renderer
        .with_context(|gl| (gl_string(gl, ffi::VENDOR), gl_string(gl, ffi::RENDERER), gl_string(gl, ffi::VERSION)))
        .unwrap_or_default();
    let family = vendor_family(&driver, &gl.0);
    tracing::info!(
        target: "glide::gpu",
        driver,
        driver_desc,
        driver_date,
        family,
        gl_vendor = gl.0,
        gl_renderer = gl.1,
        gl_version = gl.2,
        explicit_sync,
        dmabuf_formats,
        "primary GPU"
    );
    if !explicit_sync {
        let consequence = if family == "nvidia" {
            "NVIDIA clients (Chrome, Electron, Xwayland) may flicker or show torn frames; it needs driver 555+ and Linux 6.6+"
        } else {
            "clients rely on implicit sync"
        };
        tracing::warn!(target: "glide::gpu", "the GPU does not support syncobj eventfd; explicit sync is off. {consequence}");
    }
}

pub fn log_output(name: &str, format: impl std::fmt::Debug, modifiers: &[impl std::fmt::Debug]) {
    tracing::info!(target: "glide::gpu", output = name, ?format, ?modifiers, "output swapchain");
}

#[derive(Debug, Default)]
pub struct Throttle {
    window_start: Option<Instant>,
    suppressed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Report {
    First,
    Summary(u32),
    Quiet,
}

impl Throttle {
    pub fn hit(&mut self, now: Instant) -> Report {
        match self.window_start {
            None => {
                self.window_start = Some(now);
                Report::First
            }
            Some(start) if now.saturating_duration_since(start) >= SUMMARY_PERIOD => {
                let suppressed = std::mem::take(&mut self.suppressed);
                self.window_start = Some(now);
                if suppressed == 0 { Report::First } else { Report::Summary(suppressed + 1) }
            }
            Some(_) => {
                self.suppressed += 1;
                Report::Quiet
            }
        }
    }

    pub const fn clear(&mut self) {
        self.window_start = None;
        self.suppressed = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeating_error_is_logged_once_then_summed_up_every_period() {
        let start = Instant::now();
        let mut throttle = Throttle::default();
        assert_eq!(throttle.hit(start), Report::First);
        assert_eq!(throttle.hit(start + Duration::from_millis(16)), Report::Quiet);
        assert_eq!(throttle.hit(start + Duration::from_millis(32)), Report::Quiet);
        assert_eq!(throttle.hit(start + SUMMARY_PERIOD), Report::Summary(3), "two hidden plus this one");
        assert_eq!(throttle.hit(start + SUMMARY_PERIOD * 3), Report::First, "a fresh burst after a quiet period");
        throttle.clear();
        assert_eq!(throttle.hit(start + SUMMARY_PERIOD * 3), Report::First);
    }

    #[test]
    fn the_gpu_family_comes_from_the_kernel_driver_or_the_gl_vendor() {
        assert_eq!(vendor_family("nvidia-drm", "NVIDIA Corporation"), "nvidia");
        assert_eq!(vendor_family("amdgpu", "AMD"), "amd");
        assert_eq!(vendor_family("i915", "Intel"), "intel");
        assert_eq!(vendor_family("nouveau", "Mesa"), "other");
    }
}

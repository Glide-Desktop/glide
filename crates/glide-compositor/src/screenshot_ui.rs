use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use smithay::{
    output::Output,
    reexports::calloop::{LoopHandle, channel},
};

use crate::{backend::screenshot::Saved, state::GlideState};

pub const NOTICE_ID: u32 = u32::MAX - 1;
pub const OPEN: &str = "open-screenshot";
pub const SHOW: &str = "show-screenshot";
const NOTICE_TIMEOUT: Duration = Duration::from_secs(4);
const PEAK: f32 = 0.6;
const RISE: f32 = 0.25;

#[must_use]
pub fn flash_alpha(fraction: f32) -> f32 {
    use glide_theme::motion::E_OUT;
    if !(0.0..1.0).contains(&fraction) {
        return 0.0;
    }
    if fraction < RISE {
        PEAK * E_OUT.ease(fraction / RISE)
    } else {
        PEAK * (1.0 - E_OUT.ease((fraction - RISE) / (1.0 - RISE)))
    }
}

#[derive(Debug, Default)]
pub struct Flash {
    active: Option<(Output, Instant, Duration)>,
}

impl Flash {
    pub fn start(&mut self, output: Output, now: Instant, duration: Duration) {
        self.active = (!duration.is_zero()).then_some((output, now, duration));
    }

    #[must_use]
    pub fn alpha_on(&self, output: &Output, now: Instant) -> f32 {
        let Some((flashing, started, duration)) = self.active.as_ref() else {
            return 0.0;
        };
        if flashing != output {
            return 0.0;
        }
        let elapsed = now.saturating_duration_since(*started);
        let fraction = (elapsed.as_secs_f64() / duration.as_secs_f64()) as f32;
        flash_alpha(fraction)
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        let Some((_, started, duration)) = self.active.as_ref() else {
            return false;
        };
        if now.saturating_duration_since(*started) >= *duration {
            self.active = None;
        }
        true
    }
}

pub fn save_in_background(loop_handle: &LoopHandle<'static, GlideState>, pixels: Vec<u8>, size: (i32, i32)) {
    run_in_background(loop_handle, move || {
        crate::backend::screenshot::save(pixels, size.0, size.1).map_err(|err| format!("{err:#}"))
    });
}

pub fn run_in_background(
    loop_handle: &LoopHandle<'static, GlideState>, work: impl FnOnce() -> Result<Saved, String> + Send + 'static,
) {
    let (sender, receiver) = channel::channel::<Result<Saved, String>>();
    let spawned = std::thread::Builder::new().name("glide-screenshot".into()).spawn(move || {
        let result = work();
        if sender.send(result).is_err() {
            tracing::debug!("the compositor went away before the screenshot was saved");
        }
    });
    if let Err(err) = spawned {
        tracing::error!(?err, "failed to start the screenshot encoder");
        return;
    }
    let inserted = loop_handle.insert_source(receiver, |event, (), state| {
        if let channel::Event::Msg(result) = event {
            state.screenshot_finished(result);
        }
    });
    if let Err(err) = inserted {
        tracing::error!(?err, "failed to watch the screenshot encoder");
    }
}

#[must_use]
pub fn target_output<T: Clone>(
    outputs: &[(T, smithay::utils::Rectangle<i32, smithay::utils::Logical>)],
    pointer: smithay::utils::Point<f64, smithay::utils::Logical>,
) -> Option<T> {
    outputs
        .iter()
        .find(|(_, area)| area.to_f64().contains(pointer))
        .or_else(|| outputs.first())
        .map(|(output, _)| output.clone())
}

impl GlideState {
    pub fn request_screenshot(&mut self) {
        let pointer = self.seat.get_pointer().map(|pointer| pointer.current_location()).unwrap_or_default();
        let outputs: Vec<_> = self
            .space
            .outputs()
            .filter_map(|output| Some((output.clone(), self.space.output_geometry(output)?)))
            .collect();
        self.pending_screenshot = target_output(&outputs, pointer);
        tracing::info!(output = ?self.pending_screenshot.as_ref().map(Output::name), "screenshot requested");
        self.queue_redraw();
    }

    pub fn open_screenshot(&mut self, in_folder: bool) {
        self.notifications.dismiss(NOTICE_ID);
        self.queue_redraw();
        let Some(path) = self.session.last_screenshot.clone() else {
            return;
        };
        let token = self.mint_granted_token(None);
        if in_folder {
            let spawned = std::thread::Builder::new().name("glide-show-screenshot".into()).spawn(move || {
                if let Err(err) = glide_app::integration::show_in_files(&path, &token) {
                    tracing::warn!(%err, "no file manager showed the screenshot");
                    let folder = path.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
                    let mut command = std::process::Command::new("xdg-open");
                    command.arg(folder).env("XDG_ACTIVATION_TOKEN", &token).env("DESKTOP_STARTUP_ID", &token);
                    if let Err(err) = crate::spawn_detached(command) {
                        tracing::warn!(%err, "could not open the screenshot folder");
                    }
                }
            });
            if let Err(err) = spawned {
                tracing::warn!(%err, "could not start the file manager call");
            }
            return;
        }
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        if let Err(err) = self.spawn_user_process(command) {
            tracing::warn!(%err, "could not open the screenshot");
        }
    }

    pub fn request_screenshot_for(&mut self, waiter: std::os::unix::net::UnixStream) {
        self.session.screenshot_waiters.push(waiter);
        self.request_screenshot();
    }

    fn screenshot_finished(&mut self, result: Result<Saved, String>) {
        use crate::i18n::{Key, t};
        let answer = match &result {
            Ok(Saved { path: Some(path), .. }) => glide_ipc::Response::Screenshot { path: path.display().to_string() },
            Ok(Saved { path: None, .. }) => {
                glide_ipc::Response::Error { message: "the screenshot was not saved".into() }
            }
            Err(err) => glide_ipc::Response::Error { message: err.clone() },
        };
        for waiter in std::mem::take(&mut self.session.screenshot_waiters) {
            crate::ipc::reply(&waiter, &answer);
        }
        let mut actions = Vec::new();
        let (summary, body, preview) = match result {
            Ok(Saved { path, png, preview }) => {
                smithay::wayland::selection::data_device::set_data_device_selection(
                    &self.display_handle,
                    &self.seat,
                    vec![crate::clipboard::PNG_MIME.to_string()],
                    crate::clipboard::Owned::Png(Arc::from(png)),
                );
                if let Some(path) = path {
                    tracing::info!(path = %path.display(), "screenshot saved");
                    actions = vec![
                        glide_notify::Action { key: OPEN.into(), label: t(Key::ShotOpen).into() },
                        glide_notify::Action { key: SHOW.into(), label: t(Key::ShotShowInFolder).into() },
                    ];
                    self.session.last_screenshot = Some(path.clone());
                    (crate::i18n::screenshot_saved(), path.display().to_string(), preview)
                } else {
                    tracing::info!("screenshot copied to the clipboard");
                    (t(Key::ShotCopied), t(Key::ShotCopiedBody).to_string(), preview)
                }
            }
            Err(err) => {
                tracing::error!(%err, "failed to save the screenshot");
                (crate::i18n::screenshot_failed(), err, None)
            }
        };
        let app_icon = self
            .session
            .last_screenshot
            .as_ref()
            .map_or_else(|| "applets-screenshooter".to_string(), |path| path.display().to_string());
        let notice = glide_notify::Notification {
            id: NOTICE_ID,
            app_name: "Glide".to_string(),
            app_icon,
            image_data: preview,
            summary: summary.to_string(),
            body,
            actions,
            transient: true,
            timeout: Some(NOTICE_TIMEOUT),
            ..glide_notify::Notification::default()
        };
        self.notifications.push(&mut self.dock.icons, notice, true);
        self.refresh_topbar();
        self.queue_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(name: &str) -> Output {
        Output::new(
            name.to_string(),
            smithay::output::PhysicalProperties {
                size: (0, 0).into(),
                subpixel: smithay::output::Subpixel::Unknown,
                make: String::new(),
                model: String::new(),
                serial_number: String::new(),
            },
        )
    }

    #[test]
    fn the_screenshot_is_taken_of_the_output_under_the_pointer() {
        use smithay::utils::{Point, Rectangle};
        let outputs = [
            ("eDP-1", Rectangle::new((0, 0).into(), (1920, 1080).into())),
            ("HDMI-A-1", Rectangle::new((1920, 0).into(), (2560, 1440).into())),
        ];
        assert_eq!(target_output(&outputs, Point::from((2500.0, 700.0))), Some("HDMI-A-1"));
        assert_eq!(target_output(&outputs, Point::from((10.0, 10.0))), Some("eDP-1"));
        assert_eq!(target_output(&outputs, Point::from((-5.0, 5000.0))), Some("eDP-1"), "off every output");
        assert_eq!(target_output::<&str>(&[], Point::from((0.0, 0.0))), None);
    }

    #[test]
    fn the_flash_rises_to_its_peak_and_fades_back_to_nothing() {
        assert!(flash_alpha(0.0).abs() < 1e-4);
        assert!((flash_alpha(RISE) - PEAK).abs() < 1e-3);
        assert!(flash_alpha(0.999) < 0.05);
        assert!(flash_alpha(1.0).abs() < f32::EPSILON);
        let peak = (0..=100)
            .map(|step| {
                let fraction = step as f32 / 100.0;
                flash_alpha(fraction)
            })
            .fold(0.0_f32, f32::max);
        assert!(peak <= PEAK + 1e-4, "the flash overshot to {peak}");
    }

    #[test]
    fn the_flash_only_covers_the_captured_output() {
        let mut flash = Flash::default();
        let now = Instant::now();
        let shot = output("DP-1");
        flash.start(shot.clone(), now, glide_theme::motion::T2);
        let mid = now + glide_theme::motion::T2.mul_f32(RISE);
        assert!(flash.alpha_on(&shot, mid) > 0.5);
        assert!(flash.alpha_on(&output("HDMI-A-1"), mid).abs() < f32::EPSILON);
    }

    #[test]
    fn the_flash_asks_for_one_last_frame_after_it_ends_then_goes_quiet() {
        let mut flash = Flash::default();
        let now = Instant::now();
        let shot = output("DP-1");
        flash.start(shot.clone(), now, glide_theme::motion::T2);
        assert!(flash.tick(now));
        let after = now + glide_theme::motion::T2 * 2;
        assert!(flash.tick(after), "the frame that clears the overlay");
        assert!(!flash.tick(after));
        assert!(flash.alpha_on(&shot, after).abs() < f32::EPSILON);
    }

    #[test]
    fn disabled_animations_never_flash() {
        let mut flash = Flash::default();
        let now = Instant::now();
        let shot = output("DP-1");
        flash.start(shot.clone(), now, Duration::ZERO);
        assert!(flash.alpha_on(&shot, now).abs() < f32::EPSILON);
        assert!(!flash.tick(now));
    }

    #[test]
    fn the_screenshot_notice_never_collides_with_the_logout_hint() {
        assert_ne!(NOTICE_ID, u32::MAX);
    }
}

use std::os::unix::net::UnixStream;

use smithay::output::Output;
use smithay::utils::{Logical, Point, Rectangle, Size};

use crate::state::GlideState;

#[derive(Debug)]
pub struct Picker {
    pub output: Output,
    pub area: Rectangle<i32, Logical>,
    waiters: Vec<UnixStream>,
}

#[derive(Debug)]
pub struct Capture {
    pub output: Output,
    pub spot: Rectangle<f64, Logical>,
    pub waiters: Vec<UnixStream>,
}

pub use glide_shell::raster::physical_crop;

#[must_use]
pub fn color_at(pixels: &[u8], width: i32, x: i32, y: i32) -> Option<(f64, f64, f64)> {
    let index = usize::try_from(y).ok()? * usize::try_from(width).ok()? + usize::try_from(x).ok()?;
    let pixel = pixels.get(index * 4..index * 4 + 4)?;
    Some((f64::from(pixel[2]) / 255.0, f64::from(pixel[1]) / 255.0, f64::from(pixel[0]) / 255.0))
}

fn refuse(waiter: &UnixStream, message: &str) {
    crate::ipc::reply(waiter, &glide_ipc::Response::Error { message: message.to_string() });
}

impl GlideState {
    pub fn start_picker(&mut self, waiter: UnixStream) {
        if self.screen_locked() || self.session.picker.is_some() || self.shot_active() {
            refuse(&waiter, "the screen cannot be picked from right now");
            return;
        }
        let pointer = self.seat.get_pointer().map(|pointer| pointer.current_location()).unwrap_or_default();
        let outputs: Vec<_> = self
            .space
            .outputs()
            .filter_map(|output| Some((output.clone(), self.space.output_geometry(output)?)))
            .collect();
        let Some((output, area)) =
            outputs.iter().find(|(_, area)| area.to_f64().contains(pointer)).or_else(|| outputs.first()).cloned()
        else {
            refuse(&waiter, "no output to pick from");
            return;
        };
        self.session.picker = Some(Picker { output, area, waiters: vec![waiter] });
        self.shell_cursor = Some(smithay::input::pointer::CursorIcon::Crosshair);
        self.queue_redraw();
    }

    #[must_use]
    pub fn picker_active(&self) -> bool {
        self.session.picker.is_some()
    }

    pub fn picker_motion(&mut self, _pos: Point<f64, Logical>) -> bool {
        if self.session.picker.is_none() {
            return false;
        }
        self.shell_cursor = Some(smithay::input::pointer::CursorIcon::Crosshair);
        true
    }

    pub fn picker_button(&mut self, pos: Point<f64, Logical>) -> bool {
        let Some(picker) = self.session.picker.take() else {
            return false;
        };
        let spot = Rectangle::new(pos - picker.area.loc.to_f64(), Size::from((1.0, 1.0)));
        self.pending_screenshot = Some(picker.output.clone());
        self.session.capture = Some(Capture { output: picker.output, spot, waiters: picker.waiters });
        self.shell_cursor = None;
        self.queue_redraw();
        true
    }

    pub fn picker_release(&mut self, _pos: Point<f64, Logical>) -> bool {
        self.session.picker.is_some()
    }

    pub fn cancel_picker(&mut self) {
        if let Some(picker) = self.session.picker.take() {
            for waiter in &picker.waiters {
                refuse(waiter, "cancelled");
            }
            self.shell_cursor = None;
            self.queue_redraw();
        }
    }

    pub fn deliver_capture(&mut self, capture: &Capture, pixels: &[u8], size: (i32, i32), scale: f64) {
        let answer = physical_crop(capture.spot, scale, size)
            .and_then(|(x, y, _, _)| color_at(pixels, size.0, x, y))
            .map_or_else(
                || glide_ipc::Response::Error { message: "no colour under the pointer".into() },
                |(red, green, blue)| glide_ipc::Response::Color { red, green, blue },
            );
        for waiter in &capture.waiters {
            crate::ipc::reply(waiter, &answer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_logical_crop_becomes_a_clamped_physical_region() {
        let crop = Rectangle::new(Point::from((10.0, 20.0)), Size::from((100.0, 50.0)));
        assert_eq!(physical_crop(crop, 2.0, (1000, 1000)), Some((20, 40, 200, 100)));
        assert_eq!(physical_crop(crop, 2.0, (100, 1000)), Some((20, 40, 80, 100)), "clamped to the frame");
        let outside = Rectangle::new(Point::from((900.0, 0.0)), Size::from((10.0, 10.0)));
        assert_eq!(physical_crop(outside, 1.0, (100, 100)), None);
    }

    #[test]
    fn colours_are_read_as_rgb() {
        let width = 3;
        let mut pixels = Vec::new();
        for index in 0..9_u8 {
            pixels.extend_from_slice(&[index, 0, 255, 255]);
        }
        let (red, green, blue) = color_at(&pixels, width, 2, 0).expect("pixel");
        assert!((red - 1.0).abs() < 1e-9 && green.abs() < 1e-9 && (blue - 2.0 / 255.0).abs() < 1e-9);
        assert_eq!(color_at(&pixels, width, 5, 5), None);
    }
}

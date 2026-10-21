use std::os::unix::net::UnixStream;

use smithay::backend::renderer::element::Id;
use smithay::output::Output;
use smithay::utils::{Logical, Physical, Point, Rectangle, Size};

use crate::state::GlideState;

const MIN_AREA: f64 = 4.0;
pub const DIM: [f32; 4] = [0.0, 0.0, 0.0, 0.35];
pub const BORDER: i32 = 2;

type Rects = Vec<Rectangle<i32, Physical>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickMode {
    Area,
    Color,
}

#[derive(Debug)]
pub struct Picker {
    pub mode: PickMode,
    pub output: Output,
    pub area: Rectangle<i32, Logical>,
    origin: Option<Point<f64, Logical>>,
    current: Point<f64, Logical>,
    waiters: Vec<UnixStream>,
    pub ids: [Id; 8],
}

#[derive(Debug)]
pub struct Capture {
    pub output: Output,
    pub mode: PickMode,
    pub crop: Option<Rectangle<f64, Logical>>,
    pub waiters: Vec<UnixStream>,
}

impl Picker {
    #[must_use]
    pub fn selection(&self) -> Option<Rectangle<f64, Logical>> {
        let origin = self.origin?;
        let (x0, x1) = (origin.x.min(self.current.x), origin.x.max(self.current.x));
        let (y0, y1) = (origin.y.min(self.current.y), origin.y.max(self.current.y));
        Some(Rectangle::new(Point::from((x0, y0)), Size::from((x1 - x0, y1 - y0))))
    }

    fn local(&self, rect: Rectangle<f64, Logical>) -> Rectangle<f64, Logical> {
        Rectangle::new(rect.loc - self.area.loc.to_f64(), rect.size)
    }
}

#[must_use]
pub fn physical_crop(crop: Rectangle<f64, Logical>, scale: f64, size: (i32, i32)) -> Option<(i32, i32, i32, i32)> {
    let x0 = (crop.loc.x * scale).floor().max(0.0) as i32;
    let y0 = (crop.loc.y * scale).floor().max(0.0) as i32;
    let x1 = (((crop.loc.x + crop.size.w) * scale).ceil() as i32).min(size.0);
    let y1 = (((crop.loc.y + crop.size.h) * scale).ceil() as i32).min(size.1);
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
}

#[must_use]
pub fn crop_bgra(pixels: &[u8], width: i32, (x, y, w, h): (i32, i32, i32, i32)) -> Vec<u8> {
    let (Ok(width), Ok(x), Ok(y), Ok(w), Ok(h)) =
        (usize::try_from(width), usize::try_from(x), usize::try_from(y), usize::try_from(w), usize::try_from(h))
    else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(w * h * 4);
    for row in y..y + h {
        let start = (row * width + x) * 4;
        if let Some(line) = pixels.get(start..start + w * 4) {
            out.extend_from_slice(line);
        }
    }
    out
}

#[must_use]
pub fn color_at(pixels: &[u8], width: i32, x: i32, y: i32) -> Option<(f64, f64, f64)> {
    let index = usize::try_from(y).ok()? * usize::try_from(width).ok()? + usize::try_from(x).ok()?;
    let pixel = pixels.get(index * 4..index * 4 + 4)?;
    Some((f64::from(pixel[2]) / 255.0, f64::from(pixel[1]) / 255.0, f64::from(pixel[0]) / 255.0))
}

fn refuse(waiter: &UnixStream, message: &str) {
    crate::ipc::reply(waiter, &glide_ipc::Response::Error { message: message.to_string() });
}

#[must_use]
pub fn overlay_rects(picker: &Picker, scale: f64) -> (Rects, Rects) {
    if picker.mode == PickMode::Color {
        return (Vec::new(), Vec::new());
    }
    let to_physical = |rect: Rectangle<f64, Logical>| rect.to_physical(scale).to_i32_round::<i32>();
    let full = to_physical(Rectangle::from_size(picker.area.size.to_f64()));
    let Some(selection) = picker.selection().map(|rect| to_physical(picker.local(rect))) else {
        return (vec![full], Vec::new());
    };
    let (sx, sy, sw, sh) = (selection.loc.x, selection.loc.y, selection.size.w, selection.size.h);
    let rect = |x: i32, y: i32, w: i32, h: i32| Rectangle::new(Point::from((x, y)), Size::from((w.max(0), h.max(0))));
    let dim = vec![
        rect(0, 0, full.size.w, sy),
        rect(0, sy + sh, full.size.w, full.size.h - sy - sh),
        rect(0, sy, sx, sh),
        rect(sx + sw, sy, full.size.w - sx - sw, sh),
    ];
    let b = BORDER;
    let border = vec![
        rect(sx - b, sy - b, sw + 2 * b, b),
        rect(sx - b, sy + sh, sw + 2 * b, b),
        rect(sx - b, sy, b, sh),
        rect(sx + sw, sy, b, sh),
    ];
    (dim, border)
}

impl GlideState {
    pub fn start_picker(&mut self, mode: PickMode, waiter: UnixStream) {
        if self.screen_locked() || self.session.picker.is_some() {
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
        self.session.picker = Some(Picker {
            mode,
            output,
            area,
            origin: None,
            current: pointer,
            waiters: vec![waiter],
            ids: std::array::from_fn(|_| Id::new()),
        });
        self.queue_redraw();
    }

    #[must_use]
    pub fn picker_active(&self) -> bool {
        self.session.picker.is_some()
    }

    pub fn picker_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        let Some(picker) = self.session.picker.as_mut() else {
            return false;
        };
        let area = picker.area.to_f64();
        picker.current = Point::from((
            pos.x.clamp(area.loc.x, area.loc.x + area.size.w),
            pos.y.clamp(area.loc.y, area.loc.y + area.size.h),
        ));
        self.shell_cursor = Some(smithay::input::pointer::CursorIcon::Crosshair);
        self.queue_redraw();
        true
    }

    pub fn picker_button(&mut self, pos: Point<f64, Logical>) -> bool {
        let Some(picker) = self.session.picker.as_mut() else {
            return false;
        };
        picker.current = pos;
        match picker.mode {
            PickMode::Area => picker.origin = Some(pos),
            PickMode::Color => {
                let spot = Rectangle::new(pos, Size::from((1.0, 1.0)));
                self.finish_picker(Some(spot));
            }
        }
        self.queue_redraw();
        true
    }

    pub fn picker_release(&mut self, pos: Point<f64, Logical>) -> bool {
        let Some(picker) = self.session.picker.as_mut() else {
            return false;
        };
        if picker.mode != PickMode::Area || picker.origin.is_none() {
            return true;
        }
        picker.current = pos;
        let selection = picker.selection().filter(|rect| rect.size.w >= MIN_AREA && rect.size.h >= MIN_AREA);
        self.finish_picker(selection);
        true
    }

    pub fn confirm_picker(&mut self) {
        if self.session.picker.as_ref().is_some_and(|picker| picker.mode == PickMode::Area) {
            let selection = self.session.picker.as_ref().and_then(Picker::selection);
            self.finish_picker(selection.filter(|rect| rect.size.w >= MIN_AREA && rect.size.h >= MIN_AREA));
        }
    }

    pub fn cancel_picker(&mut self) {
        if let Some(picker) = self.session.picker.take() {
            for waiter in &picker.waiters {
                refuse(waiter, "cancelled");
            }
            self.queue_redraw();
        }
    }

    fn finish_picker(&mut self, selection: Option<Rectangle<f64, Logical>>) {
        let Some(picker) = self.session.picker.take() else {
            return;
        };
        let crop = selection.map(|rect| picker.local(rect));
        self.pending_screenshot = Some(picker.output.clone());
        self.session.capture =
            Some(Capture { output: picker.output, mode: picker.mode, crop, waiters: picker.waiters });
        self.queue_redraw();
    }

    pub fn deliver_capture(&mut self, capture: Capture, pixels: Vec<u8>, size: (i32, i32), scale: f64) {
        let region = capture.crop.and_then(|crop| physical_crop(crop, scale, size));
        if capture.mode == PickMode::Color {
            let answer = region.and_then(|(x, y, _, _)| color_at(&pixels, size.0, x, y)).map_or_else(
                || glide_ipc::Response::Error { message: "no colour under the pointer".into() },
                |(red, green, blue)| glide_ipc::Response::Color { red, green, blue },
            );
            for waiter in &capture.waiters {
                crate::ipc::reply(waiter, &answer);
            }
            return;
        }
        self.session.screenshot_waiters.extend(capture.waiters);
        self.screenshot_flash.start(capture.output, std::time::Instant::now(), glide_theme::motion::t2());
        match region {
            Some(region) => {
                let cropped = crop_bgra(&pixels, size.0, region);
                crate::screenshot_ui::save_in_background(&self.loop_handle, cropped, (region.2, region.3));
            }
            None => crate::screenshot_ui::save_in_background(&self.loop_handle, pixels, size),
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
    fn cropping_keeps_rows_and_colours_read_as_rgb() {
        let width = 3;
        let mut pixels = Vec::new();
        for index in 0..9_u8 {
            pixels.extend_from_slice(&[index, 0, 255, 255]);
        }
        assert_eq!(
            crop_bgra(&pixels, width, (1, 1, 2, 2)),
            [4, 0, 255, 255, 5, 0, 255, 255, 7, 0, 255, 255, 8, 0, 255, 255]
        );
        let (red, green, blue) = color_at(&pixels, width, 2, 0).expect("pixel");
        assert!((red - 1.0).abs() < 1e-9 && green.abs() < 1e-9 && (blue - 2.0 / 255.0).abs() < 1e-9);
        assert_eq!(color_at(&pixels, width, 5, 5), None);
    }
}

use std::time::Duration;

use glide_theme::Color;
use glide_theme::motion::{self, E_IO};

use crate::raster::{Canvas, Rect, rect};

pub(super) const STRIP_W: i32 = 27;
pub(super) const STRIP_H: i32 = 12;
pub(super) const CARD_W: i32 = 34;
pub(super) const CARD_H: i32 = 16;
pub(super) const SHEEN_W: i32 = 4;

const GAP: i32 = 1;
const OUTLINE: f32 = 0.45;
const SHEEN: f32 = 0.55;
const BOLT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M9.4 1.2 3.4 9.1h4.1l-1.1 5.7 6.2-8.1H8.4z\" fill=\"#000\"/></svg>";
const BOLT_HALO: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M9.4 1.2 3.4 9.1h4.1l-1.1 5.7 6.2-8.1H8.4z\" fill=\"#000\" stroke=\"#000\" stroke-width=\"2.6\" stroke-linejoin=\"round\"/></svg>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Cell {
    pub body: Rect,
    pub tip: Rect,
    pub inner: Rect,
}

impl Cell {
    #[must_use]
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        let tip_w = (h / 6).max(2);
        let tip_h = (h * 2 / 5).max(3);
        let body = rect(x, y, w - tip_w - GAP, h);
        let tip = rect(x + body.size.w + GAP, y + (h - tip_h) / 2, tip_w, tip_h);
        let inset = (h / 6).max(2);
        let inner = rect(body.loc.x + inset, body.loc.y + inset, body.size.w - inset * 2, body.size.h - inset * 2);
        Self { body, tip, inner }
    }

    #[must_use]
    pub fn level_right(&self, pct: u8) -> i32 {
        let filled = self.inner.size.w * i32::from(pct.min(100)) / 100;
        let filled = if pct > 0 { filled.max(2) } else { 0 };
        self.inner.loc.x + filled
    }

    fn radius(&self) -> f32 {
        self.body.size.h as f32 * 0.3
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Look {
    pub pct: u8,
    pub charging: bool,
    pub ink: Color,
    pub fill: Color,
}

pub(super) fn draw(c: &mut Canvas<'_>, cell: &Cell, look: Look) {
    let shell = look.ink.with_alpha(look.ink.0[3] * OUTLINE);
    c.stroke(cell.body, 1.25, cell.radius(), shell);
    c.fill(cell.tip, shell, 1.0);
    let right = cell.level_right(look.pct);
    if right > cell.inner.loc.x {
        let level = rect(cell.inner.loc.x, cell.inner.loc.y, right - cell.inner.loc.x, cell.inner.size.h);
        c.fill(level, look.fill, (cell.radius() - 1.5).max(1.0));
    }
    if look.charging {
        let side = cell.body.size.h + 2;
        let at = (cell.body.loc.x + (cell.body.size.w - side) / 2, cell.body.loc.y - 1);
        knock_out(c, BOLT_HALO, side, at);
        if let Some(pixels) = crate::paint::render_svg_tinted(BOLT.as_bytes(), side * c.scale, look.ink) {
            crate::paint::blit_square_over(&mut c.buf, c.w, c.h, &pixels, side, at.0, at.1, c.scale);
        }
    }
}

fn knock_out(c: &mut Canvas<'_>, svg: &str, side: i32, at: (i32, i32)) {
    let px = side * c.scale;
    let Some(mask) = crate::paint::render_svg(svg.as_bytes(), px) else {
        return;
    };
    let (width, height) = (c.w * c.scale, c.h * c.scale);
    let (left, top) = (at.0 * c.scale, at.1 * c.scale);
    for row in 0..px {
        let y = top + row;
        if !(0..height).contains(&y) {
            continue;
        }
        for col in 0..px {
            let x = left + col;
            let Ok(source) = usize::try_from((row * px + col) * 4 + 3) else {
                continue;
            };
            let alpha = mask.get(source).copied().map_or(0, u32::from);
            if alpha == 0 || !(0..width).contains(&x) {
                continue;
            }
            let Ok(base) = usize::try_from((y * width + x) * 4) else {
                continue;
            };
            for byte in c.buf.iter_mut().skip(base).take(4) {
                *byte = u8::try_from(u32::from(*byte) * (255 - alpha) / 255).unwrap_or(0);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Cycle {
    pub travel: Duration,
    pub rest: Duration,
}

impl Cycle {
    #[must_use]
    pub fn current() -> Option<Self> {
        let unit = motion::t4();
        (!unit.is_zero()).then(|| Self { travel: unit * 3, rest: unit * 2 })
    }

    #[must_use]
    pub fn progress(self, elapsed: Duration) -> Option<f64> {
        let period = self.travel + self.rest;
        let into = Duration::from_nanos(u64::try_from(elapsed.as_nanos() % period.as_nanos().max(1)).unwrap_or(0));
        (into < self.travel).then(|| E_IO.ease_f64(into.as_secs_f64() / self.travel.as_secs_f64()))
    }
}

#[must_use]
pub(super) fn sheen(cell: &Cell, pct: u8, progress: f64) -> Option<Rect> {
    let start = cell.level_right(pct) - SHEEN_W;
    let end = cell.inner.loc.x + cell.inner.size.w;
    if end - start <= SHEEN_W {
        return None;
    }
    let x = start + (f64::from(end - start) * progress.clamp(0.0, 1.0)).round() as i32;
    let width = (end - x).min(SHEEN_W);
    (width > 0).then(|| rect(x, cell.inner.loc.y, width, cell.inner.size.h))
}

#[must_use]
pub(super) fn sheen_colour(fill: Color) -> Color {
    fill.with_alpha(fill.0[3] * SHEEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_level_grows_with_the_charge_and_never_hides_a_live_one() {
        let cell = Cell::new(0, 0, STRIP_W, STRIP_H);
        assert_eq!(cell.level_right(0), cell.inner.loc.x);
        assert_eq!(cell.level_right(1), cell.inner.loc.x + 2, "one percent still shows");
        assert_eq!(cell.level_right(100), cell.inner.loc.x + cell.inner.size.w);
        assert_eq!(cell.level_right(250), cell.level_right(100));
        assert!(cell.tip.loc.x > cell.body.loc.x + cell.body.size.w);
        assert_eq!(cell.tip.loc.x + cell.tip.size.w, STRIP_W);
    }

    #[test]
    fn the_sheen_travels_from_under_the_level_to_the_tip() {
        let cell = Cell::new(0, 0, STRIP_W, STRIP_H);
        let start = sheen(&cell, 30, 0.0).expect("a sheen at the start");
        assert_eq!(start.loc.x + start.size.w, cell.level_right(30), "it starts hidden under the level");
        let end = sheen(&cell, 30, 1.0);
        let inner_right = cell.inner.loc.x + cell.inner.size.w;
        assert!(end.is_none_or(|end| end.loc.x + end.size.w <= inner_right), "it never leaves the cell");
        assert_eq!(sheen(&cell, 100, 0.5), None, "a full battery has nowhere to fill");
    }

    #[test]
    fn the_cycle_travels_then_rests() {
        let cycle = Cycle { travel: Duration::from_millis(300), rest: Duration::from_millis(200) };
        assert_eq!(cycle.progress(Duration::ZERO), Some(0.0));
        assert!(cycle.progress(Duration::from_millis(150)).is_some_and(|p| p > 0.0 && p < 1.0));
        assert_eq!(cycle.progress(Duration::from_millis(400)), None, "resting");
        assert_eq!(cycle.progress(Duration::from_millis(500)), Some(0.0), "and round again");
    }

    #[test]
    fn a_charging_battery_draws_its_bolt() {
        let cell = Cell::new(0, 0, STRIP_W, STRIP_H);
        let look =
            |charging| Look { pct: 60, charging, ink: Color([0.0, 0.0, 0.0, 1.0]), fill: Color([0.0, 0.5, 0.0, 1.0]) };
        let mut plain = Canvas::new(STRIP_W, STRIP_H, 2).expect("a canvas");
        draw(&mut plain, &cell, look(false));
        let mut charging = Canvas::new(STRIP_W, STRIP_H, 2).expect("a canvas");
        draw(&mut charging, &cell, look(true));
        assert_ne!(plain.buf, charging.buf);
    }
}

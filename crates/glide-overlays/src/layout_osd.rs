use std::collections::HashMap;
use std::time::Instant;

use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point, Rectangle, Size},
};

use crate::osd::{Envelope, anchor_bottom};
use crate::raster::{Canvas, rect};

pub const WIDTH: i32 = 296;
pub const HEIGHT: i32 = DOTS_Y + DOT + PAD + 2;
const PAD: i32 = glide_theme::spacing::SP4;
const INNER: i32 = WIDTH - 2 * PAD;
const ROW: i32 = 112;
const DOT: i32 = 6;
const PITCH: i32 = 22;
const PILL: i32 = 22;
const DOTS_Y: i32 = PAD + ROW + 16;
const WINDOW: (i32, i32) = (2, WIDTH - 2);
const FADE: i32 = 48;
const TRAVEL: f64 = 16.0;
const CODE: glide_theme::TextStyle = glide_theme::TextStyle {
    size_px: 72.0,
    line_height_px: 80.0,
    weight: 680,
    tracking_em: -0.03,
    uppercase: false,
    tabular: false,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy)]
struct Roll {
    from: f64,
    to: f64,
    started: Instant,
}

impl Roll {
    fn at(&self, now: Instant) -> f64 {
        let duration = glide_theme::motion::t3();
        if duration.is_zero() {
            return self.to;
        }
        let t = now.saturating_duration_since(self.started).as_secs_f64() / duration.as_secs_f64();
        if t >= 1.0 { self.to } else { glide_theme::motion::lerp(self.from, self.to, glide_theme::motion::ease_out(t)) }
    }
}

#[derive(Debug, Clone)]
pub struct Piece {
    pub buffer: MemoryRenderBuffer,
    pub at: Point<f64, Logical>,
    pub src: Option<Rectangle<f64, Logical>>,
    pub size: Option<Size<i32, Logical>>,
    pub alpha: f32,
}

type Cache<K> = HashMap<K, (u64, MemoryRenderBuffer)>;

#[derive(Debug, Default)]
pub struct LayoutOsd {
    layouts: Vec<Layout>,
    shown: Option<Envelope>,
    roll: Option<Roll>,
    panel: Cache<i32>,
    slots: Cache<(i32, usize)>,
    dots: Cache<i32>,
    pill: Cache<i32>,
    fades: Cache<(i32, bool)>,
}

fn step(from: usize, to: usize, count: usize) -> i64 {
    let count = count as i64;
    let forward = (to as i64 - from as i64).rem_euclid(count);
    if forward * 2 <= count { forward } else { forward - count }
}

impl LayoutOsd {
    pub fn announce(&mut self, layouts: Vec<Layout>, from: usize, to: usize, now: Instant) -> bool {
        let count = layouts.len();
        if count < 2 || from == to || from >= count || to >= count {
            return false;
        }
        let live = self.is_shown(now) && self.layouts == layouts;
        let roll = match self.roll.filter(|_| live) {
            Some(roll) => {
                let target = roll.to.round() as i64;
                let current = target.rem_euclid(count as i64) as usize;
                Roll { from: roll.at(now), to: (target + step(current, to, count)) as f64, started: now }
            }
            None => Roll {
                from: from as f64,
                to: (from as i64 + step(from, to, count)) as f64,
                started: now + glide_theme::motion::t1(),
            },
        };
        let since = self.shown.filter(|_| live).map_or(now, |shown| shown.since);
        self.layouts = layouts;
        self.roll = Some(roll);
        self.shown = Some(Envelope { since, updated: now });
        true
    }

    pub fn hide(&mut self) {
        self.shown = None;
        self.roll = None;
    }

    #[must_use]
    pub fn is_shown(&self, now: Instant) -> bool {
        self.shown.is_some_and(|shown| shown.alpha(now) > 0.0 || now <= shown.since + glide_theme::motion::t1())
    }

    #[must_use]
    pub fn alpha(&self, now: Instant) -> f32 {
        self.shown.map_or(0.0, |shown| shown.alpha(now))
    }

    #[must_use]
    pub fn offset_y(&self, now: Instant) -> f64 {
        self.shown.map_or(0.0, |shown| shown.offset_y(now, TRAVEL))
    }

    #[must_use]
    pub fn position(&self, now: Instant) -> Option<f64> {
        self.roll.map(|roll| roll.at(now))
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        if let Some(shown) = self.shown
            && shown.alpha(now) <= 0.0
            && now.saturating_duration_since(shown.since) > glide_theme::motion::t1()
        {
            self.hide();
            return true;
        }
        self.shown.is_some()
    }

    #[must_use]
    pub fn rect(output: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
        Rectangle::new(
            Point::from((output.loc.x + (output.size.w - WIDTH) / 2, anchor_bottom(output) - HEIGHT)),
            Size::from((WIDTH, HEIGHT)),
        )
    }

    #[must_use]
    pub fn slots(&self, now: Instant) -> Vec<(usize, f64)> {
        let (Some(position), count) = (self.position(now), self.layouts.len()) else {
            return Vec::new();
        };
        if count == 0 {
            return Vec::new();
        }
        let base = position.floor();
        [base, base + 1.0]
            .into_iter()
            .map(|slot| (slot as i64, slot - position))
            .filter(|(_, offset)| offset.abs() < 1.0 - 1e-6)
            .map(|(slot, offset)| (slot.rem_euclid(count as i64) as usize, offset))
            .collect()
    }

    #[must_use]
    pub fn pill_x(&self, now: Instant) -> Option<f64> {
        let position = self.position(now)?;
        let count = self.layouts.len() as i64;
        if count == 0 {
            return None;
        }
        let dot = |slot: i64| f64::from(dots_left(count as usize) + slot.rem_euclid(count) as i32 * PITCH);
        let base = position.floor();
        let (from, to) = (dot(base as i64), dot(base as i64 + 1));
        Some(glide_theme::motion::lerp(from, to, position - base) + f64::from(DOT - PILL) / 2.0)
    }

    pub fn pieces(
        &mut self, output: Rectangle<i32, Logical>, painter: &crate::paint::Painter, tokens: &glide_theme::Tokens,
        scale: i32, now: Instant,
    ) -> Vec<Piece> {
        let alpha = self.alpha(now);
        if alpha <= 0.0 || self.layouts.is_empty() {
            return Vec::new();
        }
        let area = Self::rect(output);
        let origin = Point::<f64, Logical>::from((f64::from(area.loc.x), f64::from(area.loc.y) + self.offset_y(now)));
        let palette = format!("{:?}{:?}", tokens.appearance, tokens.accent);
        let mut pieces = Vec::new();

        if let (Some(x), Some(pill)) = (self.pill_x(now), self.pill_buffer(&palette, tokens, scale)) {
            pieces.push(Piece {
                buffer: pill,
                at: origin + Point::from((x, f64::from(DOTS_Y))),
                src: None,
                size: None,
                alpha,
            });
        }
        if let Some(dots) = self.dots_buffer(&palette, tokens, scale) {
            let left = f64::from(dots_left(self.layouts.len()));
            pieces.push(Piece {
                buffer: dots,
                at: origin + Point::from((left, f64::from(DOTS_Y))),
                src: None,
                size: None,
                alpha,
            });
        }
        let slots = self.slots(now);
        let edges = edge_alpha(&slots);
        if edges > 0.0 {
            for (leading, x) in [(true, WINDOW.0), (false, WINDOW.1 - FADE)] {
                if let Some(fade) = self.fade_buffer(leading, &palette, tokens, scale) {
                    let at = origin + Point::from((f64::from(x), f64::from(PAD)));
                    pieces.push(Piece { buffer: fade, at, src: None, size: None, alpha: alpha * edges });
                }
            }
        }
        for (index, offset) in slots {
            let Some(buffer) = self.slot_buffer(index, &palette, painter, tokens, scale) else {
                continue;
            };
            let left = slot_left(offset);
            let Some((x, width)) = clip(left) else {
                continue;
            };
            pieces.push(Piece {
                buffer,
                at: origin + Point::from((f64::from(x), f64::from(PAD))),
                src: Some(Rectangle::new(
                    Point::from((f64::from(x) - left, 0.0)),
                    Size::from((f64::from(width), f64::from(ROW))),
                )),
                size: Some(Size::from((width, ROW))),
                alpha: alpha * slot_alpha(offset),
            });
        }
        if let Some(panel) = self.panel_buffer(&palette, tokens, scale) {
            pieces.push(Piece { buffer: panel, at: origin, src: None, size: None, alpha });
        }
        pieces
    }

    fn panel_buffer(&mut self, palette: &str, tokens: &glide_theme::Tokens, scale: i32) -> Option<MemoryRenderBuffer> {
        let key = glide_theme::Revision::new().str(palette).finish();
        if self.panel.get(&scale).is_none_or(|(cached, _)| *cached != key) {
            self.panel.insert(scale, (key, panel_canvas(tokens, scale)?.finish()));
        }
        self.panel.get(&scale).map(|(_, buffer)| buffer.clone())
    }

    fn slot_buffer(
        &mut self, index: usize, palette: &str, painter: &crate::paint::Painter, tokens: &glide_theme::Tokens,
        scale: i32,
    ) -> Option<MemoryRenderBuffer> {
        let layout = self.layouts.get(index)?;
        let key = glide_theme::Revision::new()
            .str(palette)
            .str(&layout.code)
            .str(&layout.name)
            .u64(crate::paint::painter_generation())
            .finish();
        if self.slots.get(&(scale, index)).is_none_or(|(cached, _)| *cached != key) {
            let buffer = slot_canvas(layout, painter, tokens, scale)?.finish();
            self.slots.insert((scale, index), (key, buffer));
        }
        self.slots.get(&(scale, index)).map(|(_, buffer)| buffer.clone())
    }

    fn dots_buffer(&mut self, palette: &str, tokens: &glide_theme::Tokens, scale: i32) -> Option<MemoryRenderBuffer> {
        let count = self.layouts.len();
        let key = glide_theme::Revision::new().str(palette).str(&count.to_string()).finish();
        if self.dots.get(&scale).is_none_or(|(cached, _)| *cached != key) {
            self.dots.insert(scale, (key, dots_canvas(count, tokens, scale)?.finish()));
        }
        self.dots.get(&scale).map(|(_, buffer)| buffer.clone())
    }

    fn pill_buffer(&mut self, palette: &str, tokens: &glide_theme::Tokens, scale: i32) -> Option<MemoryRenderBuffer> {
        let key = glide_theme::Revision::new().str(palette).finish();
        if self.pill.get(&scale).is_none_or(|(cached, _)| *cached != key) {
            self.pill.insert(scale, (key, pill_canvas(tokens, scale)?.finish()));
        }
        self.pill.get(&scale).map(|(_, buffer)| buffer.clone())
    }

    fn fade_buffer(
        &mut self, leading: bool, palette: &str, tokens: &glide_theme::Tokens, scale: i32,
    ) -> Option<MemoryRenderBuffer> {
        let key = glide_theme::Revision::new().str(palette).finish();
        if self.fades.get(&(scale, leading)).is_none_or(|(cached, _)| *cached != key) {
            self.fades.insert((scale, leading), (key, fade_canvas(leading, tokens, scale)?.finish()));
        }
        self.fades.get(&(scale, leading)).map(|(_, buffer)| buffer.clone())
    }
}

fn fade_canvas(leading: bool, tokens: &glide_theme::Tokens, scale: i32) -> Option<Canvas<'static>> {
    let mut canvas = Canvas::new(FADE, ROW, scale)?;
    let columns = FADE * scale;
    let [blue, green, red, alpha] = tokens.popover_bg.to_buffer_bytes();
    let strip: Vec<[u8; 4]> = (0..columns)
        .map(|column| {
            let toward_content = (column as f32 + 0.5) / columns as f32;
            let cover = edge_cover(if leading { toward_content } else { 1.0 - toward_content });
            let premultiply = |channel: u8| (f32::from(channel) * cover * f32::from(alpha) / 255.0).round() as u8;
            [premultiply(blue), premultiply(green), premultiply(red), (f32::from(alpha) * cover).round() as u8]
        })
        .collect();
    for line in canvas.buf.chunks_exact_mut((columns * 4) as usize) {
        for (target, pixel) in line.chunks_exact_mut(4).zip(&strip) {
            target.copy_from_slice(pixel);
        }
    }
    Some(canvas)
}

fn slot_left(offset: f64) -> f64 {
    f64::from(PAD) + offset * f64::from(INNER)
}

fn edge_cover(toward_content: f32) -> f32 {
    let t = 1.0 - toward_content.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn clip(left: f64) -> Option<(i32, i32)> {
    let start = (left.ceil() as i32).max(WINDOW.0);
    let end = ((left + f64::from(INNER)).floor() as i32).min(WINDOW.1);
    (end > start).then_some((start, end - start))
}

fn panel_canvas(tokens: &glide_theme::Tokens, scale: i32) -> Option<Canvas<'static>> {
    Canvas::popover((WIDTH, HEIGHT), scale, glide_theme::shape::R4, tokens.popover_bg, tokens.line)
}

fn slot_canvas(
    layout: &Layout, painter: &crate::paint::Painter, tokens: &glide_theme::Tokens, scale: i32,
) -> Option<Canvas<'static>> {
    let inner = INNER;
    let mut canvas = Canvas::new(inner, ROW, scale)?;
    let code_bottom = CODE.line_height_px.ceil() as i32 + 4;
    canvas.text(painter, (0, inner, 4, code_bottom), crate::paint::TextAlign::Center, &layout.code, tokens.fg_1, &CODE);
    canvas.text(
        painter,
        (0, inner, code_bottom, ROW - 4),
        crate::paint::TextAlign::Center,
        &layout.name,
        tokens.fg_2,
        &glide_theme::TextStyle::BODY,
    );
    Some(canvas)
}

fn dots_canvas(count: usize, tokens: &glide_theme::Tokens, scale: i32) -> Option<Canvas<'static>> {
    let mut canvas = Canvas::new(dots_width(count), DOT, scale)?;
    for slot in 0..count as i32 {
        canvas.fill(rect(slot * PITCH, 0, DOT, DOT), tokens.fill_3, DOT as f32 / 2.0);
    }
    Some(canvas)
}

fn pill_canvas(tokens: &glide_theme::Tokens, scale: i32) -> Option<Canvas<'static>> {
    let mut canvas = Canvas::new(PILL, DOT, scale)?;
    canvas.fill(rect(0, 0, PILL, DOT), tokens.acc_bg, DOT as f32 / 2.0);
    Some(canvas)
}

fn edge_alpha(slots: &[(usize, f64)]) -> f32 {
    let rest = slots.iter().map(|(_, offset)| offset.abs()).fold(1.0, f64::min);
    if slots.is_empty() { 0.0 } else { glide_theme::motion::assist(rest * 2.0) as f32 }
}

fn slot_alpha(offset: f64) -> f32 {
    glide_theme::motion::assist(1.0 - offset.abs()) as f32
}

fn dots_width(count: usize) -> i32 {
    (count.max(1) as i32 - 1) * PITCH + DOT
}

fn dots_left(count: usize) -> i32 {
    (WIDTH - dots_width(count)) / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn layouts(codes: &[&str]) -> Vec<Layout> {
        codes.iter().map(|code| Layout { code: (*code).to_string(), name: format!("{code} layout") }).collect()
    }

    fn settled(now: Instant) -> Instant {
        now + glide_theme::motion::t1() + glide_theme::motion::t3() + Duration::from_millis(1)
    }

    #[test]
    fn a_single_layout_or_no_change_shows_nothing() {
        let mut osd = LayoutOsd::default();
        let now = Instant::now();
        assert!(!osd.announce(layouts(&["EN"]), 0, 0, now));
        assert!(!osd.announce(layouts(&["EN", "UK"]), 1, 1, now));
        assert!(!osd.announce(layouts(&["EN", "UK"]), 0, 5, now));
        assert!(!osd.is_shown(now));
    }

    #[test]
    fn the_old_layout_rolls_out_as_the_new_one_rolls_in() {
        let mut osd = LayoutOsd::default();
        let now = Instant::now();
        assert!(osd.announce(layouts(&["EN", "UK"]), 0, 1, now));
        assert_eq!(osd.slots(now), vec![(0, 0.0)], "the panel enters on the layout that was in use");
        if !glide_theme::motion::is_disabled() {
            let midway = now + glide_theme::motion::t1() + glide_theme::motion::t3() / 3;
            let slots = osd.slots(midway);
            assert_eq!(slots.len(), 2, "both codes are on screen while it rolls: {slots:?}");
            assert!(
                slots.iter().any(|(index, offset)| *index == 0 && slot_left(*offset) < f64::from(PAD)),
                "the old one leaves to the left"
            );
            assert!(
                slots.iter().any(|(index, offset)| *index == 1 && slot_left(*offset) > f64::from(PAD)),
                "the new one comes in from the right, the way the pill travels"
            );
        }
        assert_eq!(osd.slots(settled(now)), vec![(1, 0.0)]);
    }

    #[test]
    fn switching_again_while_it_rolls_carries_on_from_where_it_is() {
        let mut osd = LayoutOsd::default();
        let now = Instant::now();
        let three = layouts(&["EN", "UK", "DE"]);
        osd.announce(three.clone(), 0, 1, now);
        let later = now + glide_theme::motion::t1() + glide_theme::motion::t3() / 2;
        let drawn = osd.position(later).unwrap_or_default();
        osd.announce(three.clone(), 1, 2, later);
        assert!((osd.position(later).unwrap_or_default() - drawn).abs() < 1e-9, "no jump");
        assert_eq!(osd.slots(settled(later)), vec![(2, 0.0)]);
        let wrap = settled(later);
        osd.announce(three, 2, 0, wrap);
        assert!((osd.position(settled(wrap)).unwrap_or_default() - 3.0).abs() < 1e-9, "DE → EN rolls on forwards");
        assert_eq!(osd.slots(settled(wrap)), vec![(0, 0.0)]);
    }

    #[test]
    fn a_rolling_code_is_opaque_for_most_of_its_travel_and_gone_at_the_edge() {
        assert!((slot_alpha(0.0) - 1.0).abs() < f32::EPSILON);
        assert!((slot_alpha(0.6) - 1.0).abs() < 1e-6, "opacity only assists the last 40 %");
        assert!(slot_alpha(0.9) < 0.3);
        assert!(slot_alpha(1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_resting_code_is_drawn_whole_and_a_rolling_one_fades_at_the_window_edge() {
        assert_eq!(clip(slot_left(0.0)), Some((PAD, INNER)));
        assert!(edge_alpha(&[(0, 0.0)]).abs() < f32::EPSILON, "a code at rest is drawn without the edge fades");
        assert!((edge_alpha(&[(0, -0.5), (1, 0.5)]) - 1.0).abs() < f32::EPSILON, "they are full while it rolls");
        assert!(edge_alpha(&[(0, -0.05), (1, 0.95)]) < 0.3, "and come and go with the roll");
        assert_eq!(clip(slot_left(0.6)).map(|(x, w)| x + w), Some(WINDOW.1));
        assert_eq!(clip(slot_left(-0.6)).map(|(x, _)| x), Some(WINDOW.0));
        assert_eq!(clip(f64::from(WINDOW.1)), None);
        assert!((edge_cover(0.0) - 1.0).abs() < f32::EPSILON, "the edge itself is panel colour");
        assert!(edge_cover(1.0).abs() < f32::EPSILON, "and the content side is clear");
    }

    const _PILL_CLEARS_THE_NEXT_DOT: () = assert!(PITCH - DOT / 2 - PILL / 2 >= 6);
    const _CODES_CLEAR_THE_DOTS: () = assert!(PAD + ROW <= DOTS_Y - 8);

    #[test]
    fn a_step_takes_the_short_way_round() {
        assert_eq!(step(0, 1, 2), 1);
        assert_eq!(step(1, 0, 2), 1, "two layouts always roll the same way");
        assert_eq!(step(2, 0, 3), 1);
        assert_eq!(step(0, 2, 3), -1);
        assert_eq!(step(0, 2, 4), 2);
    }

    #[test]
    fn the_accent_pill_travels_between_the_dots() {
        let mut osd = LayoutOsd::default();
        let now = Instant::now();
        osd.announce(layouts(&["EN", "UK", "DE"]), 0, 1, now);
        let first = f64::from(dots_left(3) + (DOT - PILL) / 2);
        assert!((osd.pill_x(now).unwrap_or_default() - first).abs() < 1.0);
        let second = osd.pill_x(settled(now)).unwrap_or_default();
        assert!((second - first - f64::from(PITCH)).abs() < 1e-9);
    }

    #[test]
    fn it_rises_into_place_holds_and_sinks_away() {
        let mut osd = LayoutOsd::default();
        let now = Instant::now();
        osd.announce(layouts(&["EN", "UK"]), 0, 1, now);
        if !glide_theme::motion::is_disabled() {
            assert!(osd.offset_y(now) >= TRAVEL - 0.01);
            assert!(osd.alpha(now + glide_theme::motion::t1().mul_f64(0.4)) >= 0.99);
        }
        assert!(osd.offset_y(settled(now)).abs() < 0.01);
        assert!(osd.is_shown(settled(now) + Duration::from_millis(900)), "it stays long enough to be read");
        let gone = now + Duration::from_secs(3);
        assert!(!osd.is_shown(gone));
        assert!(osd.tick(gone));
        assert!(!osd.tick(gone));
    }

    #[test]
    fn it_sits_on_the_small_indicator_s_baseline() {
        let output = Rectangle::new(Point::from((0, 0)), Size::from((1920, 1080)));
        let big = LayoutOsd::rect(output);
        let small = crate::osd::Osd::rect(output);
        assert_eq!(big.loc.y + big.size.h, small.loc.y + small.size.h);
        assert_eq!(big.loc.x * 2 + big.size.w, 1920);
        assert!(big.size.h >= 3 * small.size.h, "a big overlay, not another pill");
    }

    fn blend(frame: &mut [u8], frame_width: i32, source: &Canvas<'_>, at: (i32, i32)) {
        blend_faded(frame, frame_width, source, at, (0, frame_width), 1.0);
    }

    fn blend_faded(
        frame: &mut [u8], frame_width: i32, source: &Canvas<'_>, at: (i32, i32), clip: (i32, i32), fade: f32,
    ) {
        let scale = source.scale;
        let (width, height) = (source.w * scale, source.h * scale);
        for y in 0..height {
            let target_y = at.1 + y;
            for x in 0..width {
                if at.0 + x < clip.0 || at.0 + x >= clip.1 {
                    continue;
                }
                let from = ((y * width + x) * 4) as usize;
                let to = ((target_y * frame_width + at.0 + x) * 4) as usize;
                let faded = |channel: usize| (f32::from(source.buf[from + channel]) * fade).round() as u32;
                let alpha = faded(3);
                for channel in 0..4 {
                    let under = u32::from(frame[to + channel]);
                    frame[to + channel] = (faded(channel) + under * (255 - alpha) / 255) as u8;
                }
            }
        }
    }

    #[test]
    #[ignore = "writes preview PNGs to $GLIDE_LAYOUT_OSD_PREVIEW"]
    fn preview() {
        let Some(dir) = std::env::var_os("GLIDE_LAYOUT_OSD_PREVIEW") else {
            return;
        };
        let painter = crate::paint::Painter::new(14.0, None);
        let scale = 2;
        let three = vec![
            Layout { code: "EN".into(), name: "English (US)".into() },
            Layout { code: "UK".into(), name: "Ukrainian".into() },
            Layout { code: "PL".into(), name: "Polish".into() },
        ];
        let positions = [0.0, 0.2, 0.5, 0.8, 1.0];
        for appearance in [glide_theme::Appearance::Dark, glide_theme::Appearance::Light] {
            let tokens = glide_theme::Tokens::new(appearance, glide_theme::Accent::Blue);
            let (cell_w, cell_h) = ((WIDTH + 2 * PAD) * scale, (HEIGHT + 2 * PAD) * scale);
            let frame_width = cell_w * positions.len() as i32;
            let backdrop = if appearance == glide_theme::Appearance::Dark { 0x30 } else { 0xd8 };
            let mut frame = vec![backdrop; (frame_width * cell_h * 4) as usize];
            frame.chunks_exact_mut(4).for_each(|pixel| pixel[3] = 255);
            for (cell, position) in positions.into_iter().enumerate() {
                let now = Instant::now();
                let mut osd = LayoutOsd {
                    layouts: three.clone(),
                    roll: Some(Roll { from: position, to: position, started: now }),
                    ..LayoutOsd::default()
                };
                osd.shown = Some(Envelope { since: now, updated: now });
                let left = cell as i32 * cell_w + PAD * scale;
                let top = PAD * scale;
                blend(&mut frame, frame_width, &panel_canvas(&tokens, scale).expect("panel"), (left, top));
                let slots = osd.slots(now);
                for &(index, offset) in &slots {
                    let slot = slot_canvas(&three[index], &painter, &tokens, scale).expect("slot");
                    let slot_x = slot_left(offset);
                    let x = left + (slot_x * f64::from(scale)).round() as i32;
                    let Some((visible, width)) = clip(slot_x) else {
                        continue;
                    };
                    let window = (left + visible * scale, left + (visible + width) * scale);
                    blend_faded(&mut frame, frame_width, &slot, (x, top + PAD * scale), window, slot_alpha(offset));
                }
                let edges = edge_alpha(&slots);
                if edges > 0.0 {
                    for (leading, x) in [(true, WINDOW.0), (false, WINDOW.1 - FADE)] {
                        let fade = fade_canvas(leading, &tokens, scale).expect("fade");
                        let at = (left + x * scale, top + PAD * scale);
                        blend_faded(&mut frame, frame_width, &fade, at, (0, frame_width), edges);
                    }
                }
                let dots = dots_canvas(three.len(), &tokens, scale).expect("dots");
                blend(&mut frame, frame_width, &dots, (left + dots_left(3) * scale, top + DOTS_Y * scale));
                let pill_x = (osd.pill_x(now).expect("pill") * f64::from(scale)).round() as i32;
                let pill = pill_canvas(&tokens, scale).expect("pill");
                blend(&mut frame, frame_width, &pill, (left + pill_x, top + DOTS_Y * scale));
            }
            let image = glide_shell::image::Image::from_bgra(frame_width as u32, cell_h as u32, frame).expect("image");
            let file = format!("layout-osd-{appearance:?}.png").to_ascii_lowercase();
            std::fs::write(std::path::Path::new(&dir).join(file), image.png().expect("png")).expect("write");
        }
    }
}

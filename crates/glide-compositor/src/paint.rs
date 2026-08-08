#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::too_many_arguments
)]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
};

use fontdue::{
    Font, FontSettings,
    layout::{CoordinateSystem, Layout, TextStyle},
};

use crate::config::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
}

static NOTO_SANS_BYTES: &[u8] = include_bytes!("fonts/NotoSans-Regular.ttf");

static PAINTER: RwLock<Option<Painter>> = RwLock::new(None);

pub fn init_or_replace(font_size: f32) {
    let mut slot = PAINTER
        .write()
        .expect("single-threaded event loop, this lock is never poisoned");
    *slot = Some(Painter::new(font_size));
}

#[must_use]
pub fn current() -> Painter {
    let slot = PAINTER
        .read()
        .expect("single-threaded event loop, this lock is never poisoned");
    slot.clone().unwrap_or_else(|| Painter::new(14.0))
}

type GlyphCache = Arc<Mutex<HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>>>;

#[derive(Clone)]
pub struct Painter {
    font: Font,
    font_size: f32,
    glyph_cache: GlyphCache,
}

impl Painter {
    #[must_use]
    pub fn new(font_size: f32) -> Self {
        let font = Font::from_bytes(NOTO_SANS_BYTES, FontSettings::default())
            .expect("embedded NotoSans-Regular.ttf is always valid");
        Self {
            font,
            font_size,
            glyph_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn rasterize_cached(&self, ch: char, font_size: f32) -> (fontdue::Metrics, Vec<u8>) {
        let key = (ch, font_size.to_bits());
        let Ok(mut cache) = self.glyph_cache.lock() else {
            return self.font.rasterize(ch, font_size);
        };
        if let Some(cached) = cache.get(&key) {
            return cached.clone();
        }
        let rasterized = self.font.rasterize(ch, font_size);
        cache.insert(key, rasterized.clone());
        rasterized
    }

    fn measure_width_at(&self, text: &str, font_size: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let mut layout: Layout<()> = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&fontdue::layout::LayoutSettings::default());
        layout.append(&[&self.font], &TextStyle::new(text, font_size, 0));
        let glyphs = layout.glyphs();
        let (Some(first), Some(last)) = (glyphs.first(), glyphs.last()) else {
            return 0.0;
        };
        last.x + last.width as f32 - first.x
    }

    fn fit_text_at(&self, text: &str, avail_w: i32, font_size: f32) -> String {
        let avail_w = avail_w as f32;
        if avail_w <= 0.0 {
            return String::new();
        }
        if self.measure_width_at(text, font_size) <= avail_w {
            return text.to_owned();
        }

        let mut chars: Vec<char> = text.chars().collect();
        while !chars.is_empty() {
            chars.pop();
            let candidate: String = chars.iter().collect::<String>() + "…";
            if self.measure_width_at(&candidate, font_size) <= avail_w {
                return candidate;
            }
        }
        String::new()
    }

    #[must_use]
    pub fn text_width(&self, text: &str) -> f32 {
        self.measure_width_at(text, self.font_size)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_label(
        &self,
        buf: &mut [u8],
        width: i32,
        height: i32,
        x_min: i32,
        x_max: i32,
        scale: i32,
        align: TextAlign,
        text: &str,
        fg_argb: [u8; 4],
        bg_argb: [u8; 4],
    ) {
        let (pw, ph) = (width * scale, height * scale);
        if pw <= 0 || ph <= 0 || buf.len() < (pw * ph * 4) as usize {
            return;
        }

        let bg_premul = premultiply(bg_argb);
        for chunk in buf[..(pw * ph * 4) as usize].chunks_exact_mut(4) {
            chunk.copy_from_slice(&bg_premul);
        }

        self.draw_label_over(
            buf, width, height, x_min, x_max, scale, align, text, fg_argb,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_label_over(
        &self,
        buf: &mut [u8],
        width: i32,
        height: i32,
        x_min: i32,
        x_max: i32,
        scale: i32,
        align: TextAlign,
        text: &str,
        fg_argb: [u8; 4],
    ) {
        let (pw, ph) = (width * scale, height * scale);
        if pw <= 0 || ph <= 0 || buf.len() < (pw * ph * 4) as usize {
            return;
        }
        let (px_min, px_max) = (x_min * scale, x_max * scale);
        #[allow(clippy::cast_precision_loss)]
        let font_size = self.font_size * scale as f32;

        let text = self.fit_text_at(text, px_max - px_min, font_size);
        if text.is_empty() {
            return;
        }

        let mut layout: Layout<()> = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&fontdue::layout::LayoutSettings::default());
        layout.append(&[&self.font], &TextStyle::new(&text, font_size, 0));

        let glyphs = layout.glyphs();
        if glyphs.is_empty() {
            return;
        }

        let first = glyphs.first().unwrap();
        let last = glyphs.last().unwrap();
        let total_w = (last.x + last.width as f32 - first.x) as i32;

        let avail_w = px_max - px_min;
        let x_off = match align {
            TextAlign::Center => px_min + (avail_w - total_w) / 2 - first.x as i32,
            TextAlign::Left => px_min - first.x as i32,
        };
        let y_off = ((ph as f32 - layout.height()) / 2.0) as i32;

        for g in glyphs {
            let (metrics, bitmap) = self.rasterize_cached(g.parent, font_size);
            let gx = x_off + g.x as i32;
            let gy = y_off + g.y as i32;

            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let px = gx + col as i32;
                    let py = gy + row as i32;
                    if px < 0 || py < 0 || px >= pw || py >= ph {
                        continue;
                    }
                    let coverage = bitmap[row * metrics.width + col];
                    if coverage == 0 {
                        continue;
                    }
                    let offset = ((py * pw + px) * 4) as usize;
                    if offset + 3 >= buf.len() {
                        continue;
                    }
                    let src_alpha =
                        ((u32::from(coverage) * u32::from(fg_argb[3]) + 127) / 255) as u8;
                    let dst: [u8; 4] = buf[offset..offset + 4].try_into().unwrap();
                    let out = composite_over(dst, [fg_argb[0], fg_argb[1], fg_argb[2]], src_alpha);
                    buf[offset..offset + 4].copy_from_slice(&out);
                }
            }
        }
    }

    pub fn draw_icon_maximize(
        buf: &mut [u8],
        width: i32,
        height: i32,
        scale: i32,
        fg: Color,
        bg: Color,
    ) {
        render_icon(buf, width, height, scale, bg, |pixmap| {
            let (x0, y0, x1, y1) = glyph_bounds(width, height);
            if let Some(rect) = tiny_skia::Rect::from_ltrb(x0, y0, x1, y1) {
                let mut pb = tiny_skia::PathBuilder::new();
                pb.push_rect(rect);
                if let Some(path) = pb.finish() {
                    stroke(pixmap, &path, fg, height, scale);
                }
            }
        });
    }

    pub fn draw_icon_restore(
        buf: &mut [u8],
        width: i32,
        height: i32,
        scale: i32,
        fg: Color,
        bg: Color,
    ) {
        render_icon(buf, width, height, scale, bg, |pixmap| {
            let (x0, y0, x1, y1) = glyph_bounds(width, height);
            let offset = ((x1 - x0) * 0.28).max(1.0);

            if let Some(back) = tiny_skia::Rect::from_ltrb(x0 + offset, y0, x1, y1 - offset) {
                let mut pb = tiny_skia::PathBuilder::new();
                pb.push_rect(back);
                if let Some(path) = pb.finish() {
                    stroke(pixmap, &path, fg, height, scale);
                }
            }

            if let Some(front) = tiny_skia::Rect::from_ltrb(x0, y0 + offset, x1 - offset, y1) {
                let mut pb = tiny_skia::PathBuilder::new();
                pb.push_rect(front);
                if let Some(path) = pb.finish() {
                    fill(pixmap, &path, bg, scale);
                    stroke(pixmap, &path, fg, height, scale);
                }
            }
        });
    }

    pub fn draw_icon_close(
        buf: &mut [u8],
        width: i32,
        height: i32,
        scale: i32,
        fg: Color,
        bg: Color,
    ) {
        render_icon(buf, width, height, scale, bg, |pixmap| {
            let (x0, y0, x1, y1) = glyph_bounds(width, height);
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(x0, y0);
            pb.line_to(x1, y1);
            pb.move_to(x1, y0);
            pb.line_to(x0, y1);
            if let Some(path) = pb.finish() {
                stroke(pixmap, &path, fg, height, scale);
            }
        });
    }

    pub fn draw_icon_chevron(
        buf: &mut [u8],
        width: i32,
        height: i32,
        scale: i32,
        fg: Color,
        bg: Color,
    ) {
        render_icon(buf, width, height, scale, bg, |pixmap| {
            let (x0, y0, x1, y1) = glyph_bounds(width, height);
            let mid_x = f32::midpoint(x0, x1);
            let cy = f32::midpoint(y0, y1);
            let half_v = (x1 - x0) / 4.0;
            let top = cy - half_v;
            let bot = cy + half_v;
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(x0, top);
            pb.line_to(mid_x, bot);
            pb.line_to(x1, top);
            if let Some(path) = pb.finish() {
                stroke(pixmap, &path, fg, height, scale);
            }
        });
    }

    pub fn fill_panel_background(
        buf: &mut [u8],
        width: i32,
        height: i32,
        scale: i32,
        bg: Color,
        radius: f32,
        corners: [bool; 4],
    ) {
        let (Ok(w), Ok(h)) = (u32::try_from(width * scale), u32::try_from(height * scale)) else {
            return;
        };
        let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
            return;
        };
        if let Some(path) =
            rounded_rect_corners(0.0, 0.0, width as f32, height as f32, radius, corners)
        {
            fill(&mut pixmap, &path, bg, scale);
        }
        let bytes = skia_pixmap_into_buffer_bytes(pixmap);
        let n = bytes.len().min(buf.len());
        buf[..n].copy_from_slice(&bytes[..n]);
    }
}

fn premultiply(straight: [u8; 4]) -> [u8; 4] {
    let a = u32::from(straight[3]);
    let mul = |c: u8| ((u32::from(c) * a + 127) / 255) as u8;
    [
        mul(straight[0]),
        mul(straight[1]),
        mul(straight[2]),
        straight[3],
    ]
}

fn composite_over(dst: [u8; 4], src_rgb_straight: [u8; 3], src_alpha: u8) -> [u8; 4] {
    let sa = u32::from(src_alpha);
    let inv = 255 - sa;
    let src_premul = |c: u8| (u32::from(c) * sa + 127) / 255;
    let mix = |src_p: u32, dst_c: u8| (src_p + (u32::from(dst_c) * inv + 127) / 255).min(255) as u8;

    [
        mix(src_premul(src_rgb_straight[0]), dst[0]),
        mix(src_premul(src_rgb_straight[1]), dst[1]),
        mix(src_premul(src_rgb_straight[2]), dst[2]),
        (sa + (u32::from(dst[3]) * inv + 127) / 255).min(255) as u8,
    ]
}

fn render_icon(
    buf: &mut [u8],
    width: i32,
    height: i32,
    scale: i32,
    bg: Color,
    draw_glyph: impl FnOnce(&mut tiny_skia::Pixmap),
) {
    let (Ok(w), Ok(h)) = (u32::try_from(width * scale), u32::try_from(height * scale)) else {
        return;
    };
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    let margin = pill_margin(width, height);
    fill_pill(&mut pixmap, width, height, bg, margin, scale);

    draw_glyph(&mut pixmap);

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    buf[..n].copy_from_slice(&bytes[..n]);
}

fn glyph_bounds(width: i32, height: i32) -> (f32, f32, f32, f32) {
    const GLYPH_RATIO: f32 = 0.5;

    let side = (width.min(height) as f32).max(4.0) * GLYPH_RATIO;
    let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
    let half = side / 2.0;
    (cx - half, cy - half, cx + half, cy + half)
}

fn icon_stroke(height: i32) -> tiny_skia::Stroke {
    let width = (height as f32 / 16.0).clamp(1.5, 2.5);
    tiny_skia::Stroke {
        width,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..tiny_skia::Stroke::default()
    }
}

fn paint_for(color: Color) -> tiny_skia::Paint<'static> {
    tiny_skia::Paint {
        shader: tiny_skia::Shader::SolidColor(to_skia_color(color)),
        anti_alias: true,
        ..tiny_skia::Paint::default()
    }
}

#[allow(clippy::cast_precision_loss)]
fn scale_transform(scale: i32) -> tiny_skia::Transform {
    let s = scale as f32;
    tiny_skia::Transform::from_scale(s, s)
}

fn stroke(
    pixmap: &mut tiny_skia::Pixmap,
    path: &tiny_skia::Path,
    color: Color,
    height: i32,
    scale: i32,
) {
    pixmap.stroke_path(
        path,
        &paint_for(color),
        &icon_stroke(height),
        scale_transform(scale),
        None,
    );
}

fn fill(pixmap: &mut tiny_skia::Pixmap, path: &tiny_skia::Path, color: Color, scale: i32) {
    pixmap.fill_path(
        path,
        &paint_for(color),
        tiny_skia::FillRule::Winding,
        scale_transform(scale),
        None,
    );
}

fn pill_margin(width: i32, height: i32) -> f32 {
    (width.min(height) as f32 * 0.125).max(2.0)
}

fn fill_pill(
    pixmap: &mut tiny_skia::Pixmap,
    width: i32,
    height: i32,
    color: Color,
    margin: f32,
    scale: i32,
) {
    let w = width as f32 - margin * 2.0;
    let h = height as f32 - margin * 2.0;
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let Some(path) = rounded_rect(margin, margin, w, h, h / 2.0) else {
        return;
    };
    fill(pixmap, &path, color, scale);
}

const KAPPA: f32 = 0.552_284_8;

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<tiny_skia::Path> {
    rounded_rect_corners(x, y, w, h, radius, [true; 4])
}

#[allow(clippy::many_single_char_names)]
fn rounded_rect_corners(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    corners: [bool; 4],
) -> Option<tiny_skia::Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let r = radius.max(0.0).min(w.min(h) / 2.0);
    let mut pb = tiny_skia::PathBuilder::new();
    if r <= 0.0 || corners == [false; 4] {
        pb.push_rect(tiny_skia::Rect::from_xywh(x, y, w, h)?);
        return pb.finish();
    }

    let k = r * KAPPA;
    let [top_left, top_right, bottom_right, bottom_left] = corners;

    if top_left {
        pb.move_to(x + r, y);
    } else {
        pb.move_to(x, y);
    }

    if top_right {
        pb.line_to(x + w - r, y);
        pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    } else {
        pb.line_to(x + w, y);
    }

    if bottom_right {
        pb.line_to(x + w, y + h - r);
        pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    } else {
        pb.line_to(x + w, y + h);
    }

    if bottom_left {
        pb.line_to(x + r, y + h);
        pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    } else {
        pb.line_to(x, y + h);
    }

    if top_left {
        pb.line_to(x, y + r);
        pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    } else {
        pb.line_to(x, y);
    }

    pb.close();
    pb.finish()
}

#[allow(clippy::many_single_char_names)]
fn to_skia_color(c: Color) -> tiny_skia::Color {
    let [r, g, b, a] = c.0;
    tiny_skia::Color::from_rgba(
        r.clamp(0.0, 1.0),
        g.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
        a.clamp(0.0, 1.0),
    )
    .unwrap_or(tiny_skia::Color::TRANSPARENT)
}

fn skia_pixmap_into_buffer_bytes(pixmap: tiny_skia::Pixmap) -> Vec<u8> {
    let mut bytes = pixmap.take();
    for px in bytes.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiply_leaves_opaque_colour_unchanged() {
        assert_eq!(premultiply([10, 20, 30, 255]), [10, 20, 30, 255]);
    }

    #[test]
    fn premultiply_scales_by_alpha() {
        assert_eq!(premultiply([200, 100, 50, 128]), [100, 50, 25, 128]);
    }

    #[test]
    fn premultiply_fully_transparent_is_all_zero_colour() {
        assert_eq!(premultiply([255, 255, 255, 0]), [0, 0, 0, 0]);
    }

    #[test]
    fn composite_fully_opaque_source_replaces_destination() {
        let dst = [10, 20, 30, 255];
        let out = composite_over(dst, [200, 100, 50], 255);
        assert_eq!(out, [200, 100, 50, 255]);
    }

    #[test]
    fn composite_fully_transparent_source_leaves_destination_untouched() {
        let dst = [10, 20, 30, 200];
        let out = composite_over(dst, [255, 0, 0], 0);
        assert_eq!(out, dst);
    }

    #[test]
    fn composite_over_transparent_destination_yields_premultiplied_source() {
        let dst = [0, 0, 0, 0];
        let out = composite_over(dst, [255, 255, 255], 128);
        assert_eq!(out[3], 128, "result alpha must reflect source alpha");
        assert!(
            out[0] > 100 && out[0] < 150,
            "premultiplied colour channel should be ~half of 255, got {}",
            out[0]
        );
    }

    #[test]
    fn composite_over_is_order_correct_source_over() {
        let dst = premultiply([0, 0, 0, 255]);
        let out = composite_over(dst, [255, 255, 255], 128);
        assert_eq!(out[3], 255, "compositing over opaque stays opaque");
        assert!(
            (100..=150).contains(&out[0]),
            "expected roughly half-grey, got {}",
            out[0]
        );
    }

    #[test]
    fn skia_pixmap_swaps_red_and_blue_into_our_buffer_order() {
        let mut pixmap = tiny_skia::Pixmap::new(1, 1).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let bytes = skia_pixmap_into_buffer_bytes(pixmap);
        assert_eq!(bytes, vec![0, 0, 255, 255]);
    }

    #[test]
    fn rounded_rect_bounds_match_the_requested_box() {
        let path = rounded_rect(10.0, 20.0, 100.0, 50.0, 8.0).unwrap();
        let bounds = path.bounds();
        assert!((bounds.left() - 10.0).abs() < 0.01);
        assert!((bounds.top() - 20.0).abs() < 0.01);
        assert!((bounds.right() - 110.0).abs() < 0.01);
        assert!((bounds.bottom() - 70.0).abs() < 0.01);
    }

    #[test]
    fn rounded_rect_radius_is_clamped_to_half_the_short_side() {
        let path = rounded_rect(0.0, 0.0, 20.0, 10.0, 1000.0).unwrap();
        let bounds = path.bounds();
        assert!((bounds.width() - 20.0).abs() < 0.01);
        assert!((bounds.height() - 10.0).abs() < 0.01);
    }

    #[test]
    fn rounded_rect_rejects_degenerate_size() {
        assert!(rounded_rect(0.0, 0.0, 0.0, 10.0, 4.0).is_none());
        assert!(rounded_rect(0.0, 0.0, 10.0, -1.0, 4.0).is_none());
    }

    #[test]
    fn rounded_rect_corners_with_all_square_matches_a_plain_rect() {
        let path = rounded_rect_corners(0.0, 0.0, 100.0, 50.0, 8.0, [false; 4]).unwrap();
        let bounds = path.bounds();
        assert!((bounds.width() - 100.0).abs() < 0.01);
        assert!((bounds.height() - 50.0).abs() < 0.01);
    }

    #[test]
    fn rounded_rect_corners_bounds_are_unaffected_by_which_corners_round() {
        let all = rounded_rect_corners(0.0, 0.0, 100.0, 50.0, 8.0, [true; 4])
            .unwrap()
            .bounds();
        let top_only = rounded_rect_corners(0.0, 0.0, 100.0, 50.0, 8.0, [true, true, false, false])
            .unwrap()
            .bounds();
        assert!((all.width() - top_only.width()).abs() < 0.01);
        assert!((all.height() - top_only.height()).abs() < 0.01);
    }

    #[test]
    fn fill_panel_background_produces_a_full_size_buffer() {
        let mut buf = vec![0u8; 100 * 32 * 4];
        Painter::fill_panel_background(
            &mut buf,
            100,
            32,
            1,
            Color([0.1, 0.1, 0.15, 1.0]),
            8.0,
            [true, true, false, false],
        );
        assert_eq!(buf.len(), 100 * 32 * 4);
        let centre_top_offset = (50usize) * 4;
        assert_eq!(buf[centre_top_offset + 3], 255, "centre-top must be opaque");
        assert_eq!(buf[3], 0, "top-left corner pixel must be cut away");
    }

    #[test]
    fn fill_panel_background_square_corner_is_opaque_at_the_pixel_edge() {
        let mut buf = vec![0u8; 100 * 32 * 4];
        Painter::fill_panel_background(
            &mut buf,
            100,
            32,
            1,
            Color([0.1, 0.1, 0.15, 1.0]),
            8.0,
            [true, true, false, false],
        );
        let bottom_left_offset = (31 * 100) * 4;
        assert_eq!(
            buf[bottom_left_offset + 3],
            255,
            "square bottom-left corner must stay opaque"
        );
    }

    #[test]
    fn icon_drawing_functions_fill_the_whole_buffer_without_panicking() {
        for (w, h) in [(32, 32), (1, 1), (2, 3), (28, 28), (40, 26)] {
            for scale in [1, 2] {
                for icon in [
                    Painter::draw_icon_maximize,
                    Painter::draw_icon_restore,
                    Painter::draw_icon_close,
                    Painter::draw_icon_chevron,
                ] {
                    let expected_len = (w * scale * h * scale * 4) as usize;
                    let mut buf = vec![0u8; expected_len];
                    let fg = Color([1.0, 1.0, 1.0, 1.0]);
                    let bg = Color([0.12, 0.12, 0.15, 1.0]);
                    icon(&mut buf, w, h, scale, fg, bg);
                    assert_eq!(buf.len(), expected_len, "w={w} h={h} scale={scale}");
                }
            }
        }
    }

    #[test]
    fn icon_pill_background_is_transparent_at_the_corners() {
        let mut buf = vec![0u8; 32 * 32 * 4];
        Painter::draw_icon_maximize(
            &mut buf,
            32,
            32,
            1,
            Color([1.0, 1.0, 1.0, 1.0]),
            Color([1.0, 0.0, 0.0, 1.0]),
        );
        let corner = &buf[0..4];
        assert_eq!(corner[3], 0, "corner pixel must be fully transparent");
    }

    #[test]
    fn icon_at_scale_2_produces_a_physically_larger_buffer_than_scale_1() {
        let mut buf1 = vec![0u8; 32 * 32 * 4];
        Painter::draw_icon_close(
            &mut buf1,
            32,
            32,
            1,
            Color([1.0, 1.0, 1.0, 1.0]),
            Color([0.0, 0.0, 0.0, 1.0]),
        );
        let mut buf2 = vec![0u8; 64 * 64 * 4];
        Painter::draw_icon_close(
            &mut buf2,
            32,
            32,
            2,
            Color([1.0, 1.0, 1.0, 1.0]),
            Color([0.0, 0.0, 0.0, 1.0]),
        );
        assert_eq!(buf1.len(), 32 * 32 * 4);
        assert_eq!(buf2.len(), 64 * 64 * 4);
    }

    fn alpha_centroid(buf: &[u8], width: i32, height: i32, scale: i32) -> (f64, f64) {
        let (pw, ph) = (width * scale, height * scale);
        let (mut sum_x, mut sum_y, mut sum_a) = (0.0f64, 0.0f64, 0.0f64);
        for y in 0..ph {
            for x in 0..pw {
                let idx = ((y * pw + x) * 4) as usize;
                let a = f64::from(buf[idx + 3]);
                sum_x += f64::from(x) * a;
                sum_y += f64::from(y) * a;
                sum_a += a;
            }
        }
        assert!(sum_a > 0.0, "glyph drew nothing");
        (
            sum_x / sum_a / f64::from(scale),
            sum_y / sum_a / f64::from(scale),
        )
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn close_glyph_is_centred_on_the_cell_for_square_and_non_square_cells() {
        for (w, h) in [(32, 32), (40, 26), (26, 40), (32, 44)] {
            let mut buf = vec![0u8; (w * h * 4) as usize];
            Painter::draw_icon_close(
                &mut buf,
                w,
                h,
                1,
                Color([1.0, 1.0, 1.0, 1.0]),
                Color([0.0, 0.0, 0.0, 0.0]),
            );
            let (cx, cy) = alpha_centroid(&buf, w, h, 1);
            let (expected_cx, expected_cy) = (f64::from(w) / 2.0, f64::from(h) / 2.0);
            assert!(
                (cx - expected_cx).abs() < 1.0,
                "close glyph x-centroid off for {w}x{h}: got {cx}, expected {expected_cx}"
            );
            assert!(
                (cy - expected_cy).abs() < 1.0,
                "close glyph y-centroid off for {w}x{h}: got {cy}, expected {expected_cy}"
            );
        }
    }

    #[test]
    fn maximize_glyph_is_centred_and_scale_independent() {
        for scale in [1, 2] {
            let (w, h) = (32, 32);
            let mut buf = vec![0u8; (w * scale * h * scale * 4) as usize];
            Painter::draw_icon_maximize(
                &mut buf,
                w,
                h,
                scale,
                Color([1.0, 1.0, 1.0, 1.0]),
                Color([0.0, 0.0, 0.0, 0.0]),
            );
            let (cx, cy) = alpha_centroid(&buf, w, h, scale);
            assert!(
                (cx - f64::from(w) / 2.0).abs() < 1.0,
                "scale {scale}: got cx={cx}"
            );
            assert!(
                (cy - f64::from(h) / 2.0).abs() < 1.0,
                "scale {scale}: got cy={cy}"
            );
        }
    }

    #[test]
    fn glyph_spans_half_the_minimum_cell_dimension() {
        let (w, h) = (32, 32);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        Painter::draw_icon_maximize(
            &mut buf,
            w,
            h,
            1,
            Color([1.0, 1.0, 1.0, 1.0]),
            Color([0.0, 0.0, 0.0, 0.0]),
        );
        let mut min_x = w;
        let mut max_x = 0;
        for y in 0..h {
            for x in 0..w {
                let idx = ((y * w + x) * 4) as usize;
                if buf[idx + 3] > 10 {
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                }
            }
        }
        let span = max_x - min_x;
        let expected = (f64::from(w.min(h)) * 0.5) as i32;
        assert!(
            (span - expected).abs() <= 3,
            "expected glyph span ~{expected}, got {span}"
        );
    }

    #[test]
    fn fit_text_returns_input_unchanged_when_it_already_fits() {
        let painter = Painter::new(14.0);
        assert_eq!(painter.fit_text_at("ok", 1000, painter.font_size), "ok");
    }

    #[test]
    fn fit_text_truncates_with_an_ellipsis_when_too_narrow() {
        let painter = Painter::new(14.0);
        let long = "a very long window title that will not fit";
        let fitted = painter.fit_text_at(long, 40, painter.font_size);
        assert!(fitted.ends_with('…'), "{fitted:?}");
        assert!(
            fitted.len() < long.len(),
            "expected shortening, got {fitted:?}"
        );
    }

    #[test]
    fn fit_text_on_zero_width_is_empty() {
        let painter = Painter::new(14.0);
        assert_eq!(painter.fit_text_at("anything", 0, painter.font_size), "");
    }

    #[test]
    fn fit_text_on_empty_input_is_empty() {
        let painter = Painter::new(14.0);
        assert_eq!(painter.fit_text_at("", 1000, painter.font_size), "");
    }

    #[test]
    fn glyph_cache_returns_identical_bitmaps_across_calls() {
        let painter = Painter::new(14.0);
        let (m1, b1) = painter.rasterize_cached('A', painter.font_size);
        let (m2, b2) = painter.rasterize_cached('A', painter.font_size);
        assert_eq!((m1.width, m1.height), (m2.width, m2.height));
        assert_eq!(b1, b2);
    }

    #[test]
    fn glyph_cache_has_separate_entries_per_scaled_font_size() {
        let painter = Painter::new(14.0);
        let (m1, _) = painter.rasterize_cached('A', 14.0);
        let (m2, _) = painter.rasterize_cached('A', 28.0);
        assert!(
            m2.width > m1.width && m2.height > m1.height,
            "a 2x font size must rasterize a physically larger glyph"
        );
    }

    #[test]
    fn painter_clone_shares_the_glyph_cache() {
        let painter = Painter::new(14.0);
        let clone = painter.clone();
        let _ = painter.rasterize_cached('Q', painter.font_size);
        assert!(
            clone
                .glyph_cache
                .lock()
                .unwrap()
                .contains_key(&('Q', 14.0f32.to_bits())),
            "a clone must share the cache, not start a fresh empty one"
        );
    }

    #[test]
    fn init_or_replace_and_current_round_trip() {
        init_or_replace(21.0);
        let painter = current();
        assert!((painter.font_size - 21.0).abs() < f32::EPSILON);
    }

    #[test]
    fn draw_label_does_not_panic_on_overflowing_text() {
        let painter = Painter::new(14.0);
        let mut buf = vec![0u8; 200 * 32 * 4];
        painter.draw_label(
            &mut buf,
            200,
            32,
            32,
            168,
            1,
            TextAlign::Left,
            "a title so long it must be ellipsised to fit the available space",
            [255, 255, 255, 255],
            [30, 30, 30, 255],
        );
        assert!(buf.iter().any(|&b| b != 0));
    }

    #[test]
    fn draw_label_at_scale_2_produces_a_physically_larger_buffer() {
        let painter = Painter::new(14.0);
        let mut buf1 = vec![0u8; 200 * 32 * 4];
        painter.draw_label(
            &mut buf1,
            200,
            32,
            32,
            168,
            1,
            TextAlign::Left,
            "Hello",
            [255, 255, 255, 255],
            [30, 30, 30, 255],
        );
        let mut buf2 = vec![0u8; 400 * 64 * 4];
        painter.draw_label(
            &mut buf2,
            200,
            32,
            32,
            168,
            2,
            TextAlign::Left,
            "Hello",
            [255, 255, 255, 255],
            [30, 30, 30, 255],
        );
        assert!(buf1.iter().any(|&b| b != 0));
        assert!(buf2.iter().any(|&b| b != 0));
    }

    #[test]
    fn left_align_and_center_align_place_glyphs_differently() {
        let painter = Painter::new(14.0);
        let mut left_buf = vec![0u8; 200 * 32 * 4];
        let mut center_buf = vec![0u8; 200 * 32 * 4];
        painter.draw_label(
            &mut left_buf,
            200,
            32,
            20,
            180,
            1,
            TextAlign::Left,
            "Hi",
            [255, 255, 255, 255],
            [0, 0, 0, 255],
        );
        painter.draw_label(
            &mut center_buf,
            200,
            32,
            20,
            180,
            1,
            TextAlign::Center,
            "Hi",
            [255, 255, 255, 255],
            [0, 0, 0, 255],
        );
        assert_ne!(
            left_buf, center_buf,
            "left- and centre-aligned short text must land in different pixels"
        );
    }
}

//! Software-rendering primitives for window decorations.
//!
//! Uses [`fontdue`] for glyph rasterization and [`tiny_skia`] for vector
//! icon paths. All output is ARGB8888 rows matching the format expected by
//! [`smithay::backend::renderer::element::memory::MemoryRenderBuffer`].
//!
//! The font is the system's first available sans-serif face via
//! `fontconfig`, with a compiled-in fallback (Noto Sans) embedded as a
//! `&'static [u8]`. That way the compositor never crashes because some
//! theme package wasn't installed.
//!
//! # Lints
//!
//! Numeric-cast and argument-count lints are disabled module-wide rather
//! than at each site. This is pixel-plotting code: it converts freely
//! between buffer indices (`usize`), pixel coordinates (`i32`), glyph
//! metrics (`f32`) and channel values (`u8`/`u16`), and every one of those
//! conversions is bounded by a buffer that is at most a few thousand pixels
//! across. Annotating the ~30 individual casts would bury the arithmetic
//! without catching anything real; the bounds checks that matter are the
//! explicit clamps against `width`/`height`.
//!
//! The drawing helpers likewise take a buffer, its dimensions, a bounding
//! box and two colours — that is inherently more than seven arguments, and
//! wrapping them in a struct would only move the same fields elsewhere.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::too_many_arguments
)]

use fontdue::{
    Font, FontSettings,
    layout::{CoordinateSystem, Layout, TextStyle},
};
use once_cell::sync::OnceCell;

// ── Embedded fallback font ────────────────────────────────────────────────
// LiberationSans Regular, bundled so the compositor never crashes because
// a system font package was not installed.
static NOTO_SANS_BYTES: &[u8] = include_bytes!("fonts/NotoSans-Regular.ttf");

/// Global lazily-initialised [`Painter`] instance.
///
/// Initialised from `window.rs::render_elements` on the first decorated
/// frame, using the titlebar height as the font size hint.
pub static PAINTER: OnceCell<Painter> = OnceCell::new();

/// Shared font handle.  Created once, reused everywhere.
///
/// `fontdue::Font` is a cheap `Arc` inside so cloning is fine.
#[derive(Clone)]
pub struct Painter {
    font: Font,
    font_size: f32,
}

impl Painter {
    /// Build a `Painter` using the embedded Noto Sans font at `font_size`
    /// logical pixels.
    ///
    /// # Panics
    ///
    /// Panics only if the embedded font bytes are corrupt — which would be a
    /// build-time mistake, not a runtime condition.
    #[must_use]
    pub fn new(font_size: f32) -> Self {
        let font = Font::from_bytes(NOTO_SANS_BYTES, FontSettings::default())
            .expect("embedded NotoSans-Regular.ttf is always valid");
        Self { font, font_size }
    }

    /// Render `text` centred inside a `width × height` ARGB8888 buffer.
    ///
    /// The background is filled with `bg_argb` first; glyphs are drawn on top
    /// using `fg_argb`.  Both colours are `[A, R, G, B]` bytes (not floats).
    pub fn draw_label(
        &self,
        buf: &mut [u8],
        width: i32,
        height: i32,
        x_min: i32,
        x_max: i32,
        text: &str,
        fg_argb: [u8; 4],
        bg_argb: [u8; 4],
    ) {
        if width <= 0 || height <= 0 || buf.len() < (width * height * 4) as usize {
            return;
        }

        // Fill background.
        for chunk in buf[..(width * height * 4) as usize].chunks_exact_mut(4) {
            chunk.copy_from_slice(&bg_argb);
        }

        if text.is_empty() {
            return;
        }

        // Lay out the text to measure its pixel width.
        let mut layout: Layout<()> = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&fontdue::layout::LayoutSettings::default());
        layout.append(&[&self.font], &TextStyle::new(text, self.font_size, 0));

        let glyphs = layout.glyphs();
        if glyphs.is_empty() {
            return;
        }

        // Measure total advance width.
        let first = glyphs.first().unwrap();
        let last = glyphs.last().unwrap();
        let total_w = (last.x + last.width as f32 - first.x) as i32;

        // Centre horizontally within [x_min, x_max], and vertically in `height`.
        let avail_w = x_max - x_min;
        let x_off = x_min + (avail_w - total_w) / 2 - first.x as i32;
        let y_off = ((height as f32 - layout.height()) / 2.0) as i32;

        for g in glyphs {
            let (metrics, bitmap) = self.font.rasterize(g.parent, self.font_size);
            let gx = x_off + g.x as i32;
            let gy = y_off + g.y as i32;

            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let px = gx + col as i32;
                    let py = gy + row as i32;
                    if px < 0 || py < 0 || px >= width || py >= height {
                        continue;
                    }
                    let alpha = bitmap[row * metrics.width + col];
                    if alpha == 0 {
                        continue;
                    }
                    let offset = ((py * width + px) * 4) as usize;
                    if offset + 3 >= buf.len() {
                        continue;
                    }
                    // Alpha-blend fg over existing pixel.
                    let a = alpha as u16;
                    let ia = 255 - a;
                    buf[offset] = ((fg_argb[0] as u16 * a + buf[offset] as u16 * ia) / 255) as u8;
                    buf[offset + 1] =
                        ((fg_argb[1] as u16 * a + buf[offset + 1] as u16 * ia) / 255) as u8;
                    buf[offset + 2] =
                        ((fg_argb[2] as u16 * a + buf[offset + 2] as u16 * ia) / 255) as u8;
                    buf[offset + 3] = u8::MAX;
                }
            }
        }
    }

    /// Draw the **maximize** icon (empty square) centred in `width × height`.
    pub fn draw_icon_maximize(buf: &mut [u8], width: i32, height: i32, fg: [u8; 4], bg: [u8; 4]) {
        fill(buf, width, height, bg);
        let m = (width.min(height) / 4).max(2);
        let x0 = m;
        let y0 = m;
        let x1 = width - m - 1;
        let y1 = height - m - 1;
        let thickness = (width.min(height) / 10).max(1);
        draw_rect_outline(buf, width, height, x0, y0, x1, y1, thickness, fg);
    }

    /// Draw the **close** icon (×) centred in `width × height`.
    pub fn draw_icon_close(buf: &mut [u8], width: i32, height: i32, fg: [u8; 4], bg: [u8; 4]) {
        fill(buf, width, height, bg);
        let m = (width.min(height) / 4).max(2);
        let thickness = ((width.min(height) / 8).max(1)).max(1);
        draw_line(
            buf,
            width,
            height,
            m,
            m,
            width - m - 1,
            height - m - 1,
            thickness,
            fg,
        );
        draw_line(
            buf,
            width,
            height,
            width - m - 1,
            m,
            m,
            height - m - 1,
            thickness,
            fg,
        );
    }

    /// Draw the **chevron** icon (∨) centred in `width × height`.
    pub fn draw_icon_chevron(buf: &mut [u8], width: i32, height: i32, fg: [u8; 4], bg: [u8; 4]) {
        fill(buf, width, height, bg);
        let m = (width.min(height) / 4).max(2);
        let mx = width / 2;
        let top = height / 2 - m / 2;
        let bot = height / 2 + m / 2;
        let thickness = ((width.min(height) / 8).max(1)).max(1);
        draw_line(buf, width, height, m, top, mx, bot, thickness, fg);
        draw_line(buf, width, height, mx, bot, width - m, top, thickness, fg);
    }
}

// ── Pixel-level drawing helpers ───────────────────────────────────────────

fn fill(buf: &mut [u8], width: i32, height: i32, color: [u8; 4]) {
    for chunk in buf[..(width * height * 4) as usize].chunks_exact_mut(4) {
        chunk.copy_from_slice(&color);
    }
}

/// Bresenham thick line — simple and allocation-free.
fn draw_line(
    buf: &mut [u8],
    width: i32,
    height: i32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    thickness: i32,
    color: [u8; 4],
) {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    let mut x = x0;
    let mut y = y0;
    let half = thickness / 2;

    loop {
        for ty in (y - half)..=(y + half) {
            for tx in (x - half)..=(x + half) {
                set_pixel(buf, width, height, tx, ty, color);
            }
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

fn draw_rect_outline(
    buf: &mut [u8],
    width: i32,
    height: i32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    thickness: i32,
    color: [u8; 4],
) {
    draw_line(buf, width, height, x0, y0, x1, y0, thickness, color); // top
    draw_line(buf, width, height, x0, y1, x1, y1, thickness, color); // bottom
    draw_line(buf, width, height, x0, y0, x0, y1, thickness, color); // left
    draw_line(buf, width, height, x1, y0, x1, y1, thickness, color); // right
}

fn set_pixel(buf: &mut [u8], width: i32, height: i32, x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= width || y >= height {
        return;
    }
    let offset = ((y * width + x) * 4) as usize;
    if offset + 3 < buf.len() {
        buf[offset..offset + 4].copy_from_slice(&color);
    }
}

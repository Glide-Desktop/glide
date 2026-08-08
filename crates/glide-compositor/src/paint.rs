//! Software-rendering primitives for window decorations.
//!
//! Two rendering paths, kept deliberately separate:
//!
//! - **Text** ([`Painter::draw_label`]) uses [`fontdue`] to rasterize
//!   glyphs to coverage bitmaps, hand-blended into the buffer. fontdue has
//!   no notion of a destination buffer or compositing — it only turns a
//!   character into a coverage mask — so blending it in is on us.
//! - **Icons** (`draw_icon_*`) are anti-aliased vector paths built and
//!   rasterized by [`tiny_skia`]: strokes with round caps/joins for the
//!   close/chevron/maximize/restore glyphs, and a filled rounded rect for
//!   each button's pill-shaped background.
//!
//! Both paths converge on the same destination format: ARGB8888 rows,
//! premultiplied alpha, matching
//! [`smithay::backend::renderer::element::memory::MemoryRenderBuffer`].
//! tiny-skia's own pixel format is premultiplied *RGBA* (red first); ours
//! is *BGRA* (blue first) — [`skia_pixmap_into_buffer_bytes`] is the one
//! place that conversion happens, exactly the same discipline
//! [`crate::config::Color::to_buffer_bytes`] documents for theme colours,
//! applied to a whole rendered image instead of a single one.
//!
//! The font is always the compiled-in fallback (Noto Sans), embedded as a
//! `&'static [u8]`. There is no `fontconfig` lookup of an installed system
//! font; embedding one font unconditionally is what guarantees the
//! compositor never fails to render text because a theme package wasn't
//! installed.
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

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
};

use fontdue::{
    Font, FontSettings,
    layout::{CoordinateSystem, Layout, TextStyle},
};

use crate::config::Color;

/// Horizontal positioning for [`Painter::draw_label`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
}

// ── Embedded fallback font ────────────────────────────────────────────────
// Noto Sans Regular, bundled so the compositor never crashes because a
// system font package was not installed.
static NOTO_SANS_BYTES: &[u8] = include_bytes!("fonts/NotoSans-Regular.ttf");

/// Shared, replaceable [`Painter`] instance used by every window's
/// decorations and by the overview.
///
/// A `static` rather than a `GlideState` field because the two render call
/// sites that need it — [`crate::window::GlideWindow`]'s
/// `AsRenderElements` impl and the overview's render pass — receive only a
/// renderer, never compositor state (that is all `AsRenderElements` and
/// friends are ever handed, by Smithay's own trait signatures); there is
/// nowhere else to reach a `Painter` from.
///
/// Unlike the `OnceCell` this used to be, the lock lets [`init_or_replace`]
/// rebuild it — called once at startup and again on every `glidectl
/// reload` by [`crate::config::Config::apply_to_state`] — so a changed
/// `[decorations]` font size actually takes effect, instead of whichever
/// call site happened to render the first decorated frame winning by
/// accident (the previous version's actual bug).
static PAINTER: RwLock<Option<Painter>> = RwLock::new(None);

/// Builds a fresh [`Painter`] at `font_size` and installs it as the shared
/// instance, replacing whatever was there before.
pub fn init_or_replace(font_size: f32) {
    let mut slot = PAINTER
        .write()
        .expect("single-threaded event loop, this lock is never poisoned");
    *slot = Some(Painter::new(font_size));
}

/// The current shared [`Painter`], cloned out of the lock — cheap, see
/// [`Painter`]'s own docs on why. Falls back to a fresh default instance
/// rather than panicking if called before [`init_or_replace`] ever ran;
/// in practice `main.rs` calls `Config::apply_to_state`, which calls
/// `init_or_replace`, before the event loop starts and before any window
/// can be mapped, so this is a defensive fallback rather than a path that
/// fires in a real session.
#[must_use]
pub fn current() -> Painter {
    let slot = PAINTER
        .read()
        .expect("single-threaded event loop, this lock is never poisoned");
    slot.clone().unwrap_or_else(|| Painter::new(14.0))
}

/// Rasterized glyph bitmaps, keyed by character and font size (as bits, so
/// the key is hashable). Shared via `Arc` so cloning a [`Painter`] shares
/// the cache rather than starting a fresh, cold one — titles are redrawn
/// every frame the titlebar repaints (see `decoration.rs`'s `redraw` docs),
/// so a cold cache on every clone would defeat the point.
type GlyphCache = Arc<Mutex<HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>>>;

/// Shared font handle, plus a rasterization cache.  Created once, reused
/// everywhere.
///
/// `fontdue::Font` is a cheap `Arc` inside, and `glyph_cache` is
/// `Arc`-shared explicitly, so cloning a whole `Painter` is cheap and
/// shares both.
#[derive(Clone)]
pub struct Painter {
    font: Font,
    font_size: f32,
    glyph_cache: GlyphCache,
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
        Self {
            font,
            font_size,
            glyph_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Rasterizes `ch`, reusing a cached bitmap if this exact `(char,
    /// font_size)` pair was rasterized before.
    ///
    /// Falls back to rasterizing uncached (rather than panicking) if the
    /// cache lock is unavailable for any reason — correctness never
    /// depends on the cache, only performance does.
    fn rasterize_cached(&self, ch: char) -> (fontdue::Metrics, Vec<u8>) {
        let key = (ch, self.font_size.to_bits());
        let Ok(mut cache) = self.glyph_cache.lock() else {
            return self.font.rasterize(ch, self.font_size);
        };
        if let Some(cached) = cache.get(&key) {
            return cached.clone();
        }
        let rasterized = self.font.rasterize(ch, self.font_size);
        cache.insert(key, rasterized.clone());
        rasterized
    }

    /// Laid-out pixel width of `text` at this painter's font size, `0.0`
    /// for empty text.
    fn measure_width(&self, text: &str) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let mut layout: Layout<()> = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&fontdue::layout::LayoutSettings::default());
        layout.append(&[&self.font], &TextStyle::new(text, self.font_size, 0));
        let glyphs = layout.glyphs();
        let (Some(first), Some(last)) = (glyphs.first(), glyphs.last()) else {
            return 0.0;
        };
        last.x + last.width as f32 - first.x
    }

    /// Shortens `text` with a trailing ellipsis so it fits within
    /// `avail_w` pixels, if it doesn't already fit as-is.
    ///
    /// Character-by-character rather than a binary search: titles are
    /// short, a handful of extra `measure_width` calls is nowhere near a
    /// hot path (this only runs when the label actually needs repainting —
    /// see `decoration.rs`'s dirty-checked `set_title`), and dropping one
    /// character and re-measuring is the simplest thing that is obviously
    /// correct.
    fn fit_text(&self, text: &str, avail_w: i32) -> String {
        let avail_w = avail_w as f32;
        if avail_w <= 0.0 {
            return String::new();
        }
        if self.measure_width(text) <= avail_w {
            return text.to_owned();
        }

        let mut chars: Vec<char> = text.chars().collect();
        while !chars.is_empty() {
            chars.pop();
            let candidate: String = chars.iter().collect::<String>() + "…";
            if self.measure_width(&candidate) <= avail_w {
                return candidate;
            }
        }
        // Not even a bare ellipsis fits: better to draw nothing than a
        // half-clipped glyph.
        String::new()
    }

    /// Laid-out pixel width of `text` at this painter's font size.
    ///
    /// Public wrapper around the same measuring logic [`Self::draw_label`]
    /// uses internally, for callers that need to size a UI element around
    /// a piece of text *before* drawing it — the chevron dropdown's width,
    /// for instance.
    #[must_use]
    pub fn text_width(&self, text: &str) -> f32 {
        self.measure_width(text)
    }

    /// Render `text` inside a `width × height` ARGB8888 buffer, horizontally
    /// positioned per `align` within `[x_min, x_max]` and centred vertically
    /// in `height`.
    ///
    /// The background is filled with `bg_argb` first; glyphs are drawn on
    /// top using `fg_argb`, alpha-blended by the font's own per-pixel
    /// anti-aliasing coverage *and* `fg_argb`'s own alpha channel. Both
    /// colours are **straight** (not premultiplied) `[B, G, R, A]` bytes —
    /// see [`crate::config::Color::to_buffer_bytes`], the canonical way to
    /// produce them from a theme colour. This method premultiplies
    /// internally, once, at the point each pixel is actually written; the
    /// buffer this leaves behind is what [`MemoryRenderBuffer`]'s
    /// `Argb8888` format expects to hold.
    ///
    /// `text` is truncated with a trailing ellipsis if it doesn't fit
    /// within `[x_min, x_max]` — see [`Self::fit_text`].
    pub fn draw_label(
        &self,
        buf: &mut [u8],
        width: i32,
        height: i32,
        x_min: i32,
        x_max: i32,
        align: TextAlign,
        text: &str,
        fg_argb: [u8; 4],
        bg_argb: [u8; 4],
    ) {
        if width <= 0 || height <= 0 || buf.len() < (width * height * 4) as usize {
            return;
        }

        // Fill background. Premultiplied: every pixel in this buffer,
        // background or glyph, must agree on that convention, or the two
        // fights each other the moment either colour has alpha < 255.
        let bg_premul = premultiply(bg_argb);
        for chunk in buf[..(width * height * 4) as usize].chunks_exact_mut(4) {
            chunk.copy_from_slice(&bg_premul);
        }

        let text = self.fit_text(text, x_max - x_min);
        if text.is_empty() {
            return;
        }

        // Lay out the (possibly truncated) text to measure its pixel width.
        let mut layout: Layout<()> = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&fontdue::layout::LayoutSettings::default());
        layout.append(&[&self.font], &TextStyle::new(&text, self.font_size, 0));

        let glyphs = layout.glyphs();
        if glyphs.is_empty() {
            return;
        }

        // Measure total advance width.
        let first = glyphs.first().unwrap();
        let last = glyphs.last().unwrap();
        let total_w = (last.x + last.width as f32 - first.x) as i32;

        let avail_w = x_max - x_min;
        let x_off = match align {
            // Centred within [x_min, x_max].
            TextAlign::Center => x_min + (avail_w - total_w) / 2 - first.x as i32,
            // Flush against x_min.
            TextAlign::Left => x_min - first.x as i32,
        };
        let y_off = ((height as f32 - layout.height()) / 2.0) as i32;

        for g in glyphs {
            let (metrics, bitmap) = self.rasterize_cached(g.parent);
            let gx = x_off + g.x as i32;
            let gy = y_off + g.y as i32;

            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let px = gx + col as i32;
                    let py = gy + row as i32;
                    if px < 0 || py < 0 || px >= width || py >= height {
                        continue;
                    }
                    let coverage = bitmap[row * metrics.width + col];
                    if coverage == 0 {
                        continue;
                    }
                    let offset = ((py * width + px) * 4) as usize;
                    if offset + 3 >= buf.len() {
                        continue;
                    }
                    // Combined source alpha: the font's own per-pixel
                    // anti-aliasing coverage *and* the requested foreground
                    // alpha both attenuate how much glyph colour lands —
                    // dropping either term was what let a translucent
                    // titlebar get an opaque hole punched through it by
                    // its own title text (every touched pixel forced to
                    // alpha 255 regardless of what was asked for).
                    let src_alpha =
                        ((u32::from(coverage) * u32::from(fg_argb[3]) + 127) / 255) as u8;
                    let dst: [u8; 4] = buf[offset..offset + 4].try_into().unwrap();
                    let out = composite_over(dst, [fg_argb[0], fg_argb[1], fg_argb[2]], src_alpha);
                    buf[offset..offset + 4].copy_from_slice(&out);
                }
            }
        }
    }

    /// Draw the **maximize** icon (empty square) into `buf`.
    ///
    /// `fg`/`bg` are straight colour — tiny-skia's own native input
    /// format, matching [`crate::config::Color`]'s representation exactly,
    /// so unlike [`Self::draw_label`] no premultiplication happens here:
    /// tiny-skia does it internally when it rasterizes each fill/stroke.
    pub fn draw_icon_maximize(buf: &mut [u8], width: i32, height: i32, fg: Color, bg: Color) {
        render_icon(buf, width, height, bg, |pixmap| {
            let (m, w, h) = icon_bounds(width, height);
            if let Some(rect) = tiny_skia::Rect::from_ltrb(m, m, w - m, h - m) {
                let mut pb = tiny_skia::PathBuilder::new();
                pb.push_rect(rect);
                if let Some(path) = pb.finish() {
                    stroke(pixmap, &path, fg, height);
                }
            }
        });
    }

    /// Draw the **restore** icon (two overlapping squares) into `buf` —
    /// shown on the maximize button instead of [`Self::draw_icon_maximize`]
    /// once the window actually is maximized, so the button always
    /// communicates what pressing it will do next rather than what state
    /// the window happens to be in already.
    pub fn draw_icon_restore(buf: &mut [u8], width: i32, height: i32, fg: Color, bg: Color) {
        render_icon(buf, width, height, bg, |pixmap| {
            let (m, w, h) = icon_bounds(width, height);
            let offset = ((h - m * 2.0) * 0.28).max(1.0);

            // Back square: shifted up-right.
            if let Some(back) = tiny_skia::Rect::from_ltrb(m + offset, m, w, h - offset) {
                let mut pb = tiny_skia::PathBuilder::new();
                pb.push_rect(back);
                if let Some(path) = pb.finish() {
                    stroke(pixmap, &path, fg, height);
                }
            }

            // Front square: shifted down-left. Filled with the button's own
            // background first, so it visually cuts into the back square's
            // stroke instead of merely overlapping it, then outlined.
            if let Some(front) = tiny_skia::Rect::from_ltrb(m, m + offset, w - offset, h) {
                let mut pb = tiny_skia::PathBuilder::new();
                pb.push_rect(front);
                if let Some(path) = pb.finish() {
                    fill(pixmap, &path, bg);
                    stroke(pixmap, &path, fg, height);
                }
            }
        });
    }

    /// Draw the **close** icon (×) into `buf`.
    pub fn draw_icon_close(buf: &mut [u8], width: i32, height: i32, fg: Color, bg: Color) {
        render_icon(buf, width, height, bg, |pixmap| {
            let (m, w, h) = icon_bounds(width, height);
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(m, m);
            pb.line_to(w - m, h - m);
            pb.move_to(w - m, m);
            pb.line_to(m, h - m);
            if let Some(path) = pb.finish() {
                stroke(pixmap, &path, fg, height);
            }
        });
    }

    /// Draw the **chevron** icon (∨) into `buf`.
    pub fn draw_icon_chevron(buf: &mut [u8], width: i32, height: i32, fg: Color, bg: Color) {
        render_icon(buf, width, height, bg, |pixmap| {
            let (m, w, h) = icon_bounds(width, height);
            let mid_x = w / 2.0;
            let top = h / 2.0 - m / 2.0;
            let bot = h / 2.0 + m / 2.0;
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(m, top);
            pb.line_to(mid_x, bot);
            pb.line_to(w - m, top);
            if let Some(path) = pb.finish() {
                stroke(pixmap, &path, fg, height);
            }
        });
    }
}

// ── Alpha compositing (text path only) ────────────────────────────────────
//
// `MemoryRenderBuffer`'s `Argb8888` pixels are expected to hold
// premultiplied alpha (as is standard for Wayland/DRM shm buffers). Every
// theme colour, however, is authored and stored as *straight* alpha — a
// user writing `background = "#1e1e2680"` means "this colour at 50%
// opacity", not "this colour already scaled down by its own alpha". The two
// functions below are where `draw_label` crosses that boundary; the icon
// path below crosses the equivalent boundary through tiny-skia instead
// (see `to_skia_color`/`skia_pixmap_into_buffer_bytes`).

/// Converts a straight-alpha colour into its premultiplied form.
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

/// Composites a *straight*-colour, `src_alpha`-opaque source over an
/// already-*premultiplied* destination pixel, "source-over", producing a
/// new premultiplied pixel.
///
/// `src_alpha` is the source's total opacity at this pixel — for text this
/// is the font's per-pixel anti-aliasing coverage combined with the
/// requested foreground colour's own alpha, so both attenuate the result
/// instead of either being silently dropped.
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

// ── Icons: tiny-skia bridge ────────────────────────────────────────────────

/// Draws `bg`, at a small inset, as a filled rounded-rect "pill" — the
/// button's background — then hands the pixmap to `draw_glyph` to add the
/// icon on top, and writes the result into `buf`.
///
/// Everything outside the pill stays fully transparent (a fresh
/// [`tiny_skia::Pixmap`] starts that way), which is what lets it read as a
/// badge sitting on the titlebar rather than a hard-edged block: the
/// titlebar's own background element is what shows through underneath —
/// see `window.rs`'s render element ordering, where the titlebar
/// background is composited furthest back.
fn render_icon(
    buf: &mut [u8],
    width: i32,
    height: i32,
    bg: Color,
    draw_glyph: impl FnOnce(&mut tiny_skia::Pixmap),
) {
    let (Ok(w), Ok(h)) = (u32::try_from(width), u32::try_from(height)) else {
        return;
    };
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    let margin = pill_margin(width, height);
    fill_pill(&mut pixmap, width, height, bg, margin);

    draw_glyph(&mut pixmap);

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    buf[..n].copy_from_slice(&bytes[..n]);
}

/// Inset margin, and the resulting far edge coordinates, for a glyph drawn
/// inside a `width × height` icon buffer. Derived from `height` — per-icon
/// callers no longer each recompute `width.min(height)` by hand.
fn icon_bounds(width: i32, height: i32) -> (f32, f32, f32) {
    let m = (height as f32 / 4.0).max(2.0);
    (m, width as f32 - m, height as f32 - m)
}

/// Stroke width for icon glyphs: thin and consistent regardless of
/// titlebar height, rather than the old Bresenham path's chunky
/// `height / 8`\-ish thickness — a real anti-aliased hairline reads as
/// crisp at any of these sizes, where a thick blocky one only looked
/// "intentional" by accident.
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

fn stroke(pixmap: &mut tiny_skia::Pixmap, path: &tiny_skia::Path, color: Color, height: i32) {
    pixmap.stroke_path(
        path,
        &paint_for(color),
        &icon_stroke(height),
        tiny_skia::Transform::identity(),
        None,
    );
}

fn fill(pixmap: &mut tiny_skia::Pixmap, path: &tiny_skia::Path, color: Color) {
    pixmap.fill_path(
        path,
        &paint_for(color),
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );
}

/// How far a button's pill background is inset from the full cell it is
/// drawn in.
fn pill_margin(width: i32, height: i32) -> f32 {
    (width.min(height) as f32 * 0.125).max(2.0)
}

/// Fills a fully-rounded ("stadium") pill inset by `margin` from the
/// `width × height` cell, or does nothing if the inset leaves no room.
fn fill_pill(pixmap: &mut tiny_skia::Pixmap, width: i32, height: i32, color: Color, margin: f32) {
    let w = width as f32 - margin * 2.0;
    let h = height as f32 - margin * 2.0;
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let Some(path) = rounded_rect(margin, margin, w, h, h / 2.0) else {
        return;
    };
    fill(pixmap, &path, color);
}

/// The standard constant for approximating a quarter-circle arc with one
/// cubic Bézier: a corner of radius `r` is drawn by offsetting each cubic's
/// control points by `r * KAPPA` along the tangent.
const KAPPA: f32 = 0.552_284_8;

/// Builds a rounded-rectangle path via cubic-Bézier corners. `radius` is
/// clamped so it can never exceed half of whichever of `w`/`h` is smaller,
/// which would otherwise self-intersect.
#[allow(clippy::many_single_char_names)]
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<tiny_skia::Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let r = radius.max(0.0).min(w.min(h) / 2.0);
    let mut pb = tiny_skia::PathBuilder::new();
    if r <= 0.0 {
        pb.push_rect(tiny_skia::Rect::from_xywh(x, y, w, h)?);
        return pb.finish();
    }

    let k = r * KAPPA;

    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

/// Converts a straight-alpha [`Color`] into tiny-skia's own native colour
/// type — also straight-alpha, so unlike the buffer-byte conversions
/// elsewhere in this module, this one carries no channel-order or
/// premultiplication concern of its own. Falls back to fully transparent
/// on out-of-range input, which cannot happen for anything parsed via
/// [`Color`]'s own `from_hex`, but keeps this total rather than panicking.
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

/// Converts a tiny-skia [`tiny_skia::Pixmap`]'s premultiplied `RGBA` bytes
/// into the premultiplied `BGRA` bytes [`MemoryRenderBuffer`]'s
/// `Argb8888` format expects — the same red/blue swap
/// [`crate::config::Color::to_buffer_bytes`] documents for a single
/// colour, applied here to an already-rendered image. This is the only
/// place tiny-skia's byte order and ours meet.
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
        // Half alpha halves every colour channel; alpha itself is
        // untouched (straight alpha stays in byte 3 regardless).
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
        // This is exactly the text-over-a-translucent-titlebar case: the
        // background fill left `dst` fully transparent, and a 50%-alpha
        // glyph pixel lands on it. The result must carry that 50% through
        // to its own alpha, not come out fully opaque.
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
        // Blending white at 50% alpha over opaque black must land roughly
        // halfway — a wrong blend factor (e.g. src/dst swapped) would miss
        // this by a wide margin, not just by rounding.
        let dst = premultiply([0, 0, 0, 255]); // opaque black
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
        // Regression test for the same channel-swap bug class
        // `to_buffer_bytes_is_b_g_r_a_not_a_r_g_b` guards in config.rs:
        // opaque red must come out as buffer bytes [0, 0, 255, 255]
        // (B, G, R, A), not [255, 0, 0, 255].
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
        // A radius larger than the shape would self-intersect; the path
        // must still come out as a well-formed, non-degenerate rect.
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
    fn icon_drawing_functions_fill_the_whole_buffer_without_panicking() {
        // Smoke test: every icon function must produce exactly
        // width*height*4 bytes and never panic, across a range of sizes
        // including tiny/odd ones a real titlebar height could produce.
        for (w, h) in [(32, 32), (1, 1), (2, 3), (28, 28), (40, 26)] {
            for icon in [
                Painter::draw_icon_maximize,
                Painter::draw_icon_restore,
                Painter::draw_icon_close,
                Painter::draw_icon_chevron,
            ] {
                let mut buf = vec![0u8; (w * h * 4) as usize];
                let fg = Color([1.0, 1.0, 1.0, 1.0]);
                let bg = Color([0.12, 0.12, 0.15, 1.0]);
                icon(&mut buf, w, h, fg, bg);
                assert_eq!(buf.len(), (w * h * 4) as usize, "w={w} h={h}");
            }
        }
    }

    #[test]
    fn icon_pill_background_is_transparent_at_the_corners() {
        // The whole point of the pill: corners stay transparent so the
        // titlebar's own background shows through underneath, rather than
        // a hard-edged coloured block covering the full cell.
        let mut buf = vec![0u8; 32 * 32 * 4];
        Painter::draw_icon_maximize(
            &mut buf,
            32,
            32,
            Color([1.0, 1.0, 1.0, 1.0]),
            Color([1.0, 0.0, 0.0, 1.0]),
        );
        let corner = &buf[0..4]; // top-left pixel
        assert_eq!(corner[3], 0, "corner pixel must be fully transparent");
    }

    #[test]
    fn fit_text_returns_input_unchanged_when_it_already_fits() {
        let painter = Painter::new(14.0);
        assert_eq!(painter.fit_text("ok", 1000), "ok");
    }

    #[test]
    fn fit_text_truncates_with_an_ellipsis_when_too_narrow() {
        let painter = Painter::new(14.0);
        let long = "a very long window title that will not fit";
        let fitted = painter.fit_text(long, 40);
        assert!(fitted.ends_with('…'), "{fitted:?}");
        assert!(
            fitted.len() < long.len(),
            "expected shortening, got {fitted:?}"
        );
    }

    #[test]
    fn fit_text_on_zero_width_is_empty() {
        let painter = Painter::new(14.0);
        assert_eq!(painter.fit_text("anything", 0), "");
    }

    #[test]
    fn fit_text_on_empty_input_is_empty() {
        let painter = Painter::new(14.0);
        assert_eq!(painter.fit_text("", 1000), "");
    }

    #[test]
    fn glyph_cache_returns_identical_bitmaps_across_calls() {
        let painter = Painter::new(14.0);
        let (m1, b1) = painter.rasterize_cached('A');
        let (m2, b2) = painter.rasterize_cached('A');
        assert_eq!((m1.width, m1.height), (m2.width, m2.height));
        assert_eq!(b1, b2);
    }

    #[test]
    fn painter_clone_shares_the_glyph_cache() {
        let painter = Painter::new(14.0);
        let clone = painter.clone();
        let _ = painter.rasterize_cached('Q');
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
            TextAlign::Left,
            "a title so long it must be ellipsised to fit the available space",
            [255, 255, 255, 255],
            [30, 30, 30, 255],
        );
        // Background must have been painted at least (buffer isn't left
        // all-zero) — a weak but real smoke assertion that something drew.
        assert!(buf.iter().any(|&b| b != 0));
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

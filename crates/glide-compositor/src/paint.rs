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

    /// Rasterizes `ch` at `font_size`, reusing a cached bitmap if this
    /// exact `(char, font_size)` pair was rasterized before.
    ///
    /// `font_size` is an explicit parameter rather than always
    /// `self.font_size` so a `HiDPI` draw call can rasterize at
    /// `self.font_size * scale` — the cache key already includes it, so a
    /// higher-scale request simply lands in its own cache entries rather
    /// than colliding with (or reusing, blurrily) the logical-scale ones.
    ///
    /// Falls back to rasterizing uncached (rather than panicking) if the
    /// cache lock is unavailable for any reason — correctness never
    /// depends on the cache, only performance does.
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

    /// Laid-out pixel width of `text` at `font_size`, `0.0` for empty text.
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

    /// Shortens `text` with a trailing ellipsis so it fits within
    /// `avail_w` pixels at `font_size`, if it doesn't already fit as-is.
    ///
    /// Character-by-character rather than a binary search: titles are
    /// short, a handful of extra `measure_width_at` calls is nowhere near a
    /// hot path (this only runs when the label actually needs repainting —
    /// see `decoration.rs`'s dirty-checked `set_title`), and dropping one
    /// character and re-measuring is the simplest thing that is obviously
    /// correct.
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
        // Not even a bare ellipsis fits: better to draw nothing than a
        // half-clipped glyph.
        String::new()
    }

    /// Laid-out pixel width of `text` at this painter's *logical* font
    /// size — deliberately not scale-aware, unlike [`Self::draw_label`]:
    /// callers use this to size a UI element in logical pixels *before*
    /// drawing it (the chevron dropdown's width, for instance), and
    /// logical layout must stay scale-independent the same way the rest of
    /// the compositor's hit-testing and geometry does.
    #[must_use]
    pub fn text_width(&self, text: &str) -> f32 {
        self.measure_width_at(text, self.font_size)
    }

    /// Render `text` inside a buffer sized for a `width × height` *logical*
    /// area at `scale` — i.e. `width * scale` by `height * scale` physical
    /// pixels, which is what `buf` must actually be sized to — horizontally
    /// positioned per `align` within logical `[x_min, x_max]` and centred
    /// vertically.
    ///
    /// `scale` is what makes this HiDPI-correct: text is laid out and
    /// rasterized at `self.font_size * scale`, not upscaled after the fact
    /// from a logical-resolution rendering, which is what would otherwise
    /// leave titlebar text and menu labels looking soft on any output
    /// whose scale isn't exactly `1`. `scale = 1` reproduces the prior
    /// (logical-only) behaviour exactly — this is not a new code path for
    /// the common case, just a generalization of the existing one.
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
    /// within `[x_min, x_max]` — see [`Self::fit_text_at`].
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

        // Fill background. Premultiplied: every pixel in this buffer,
        // background or glyph, must agree on that convention, or the two
        // fights each other the moment either colour has alpha < 255.
        let bg_premul = premultiply(bg_argb);
        for chunk in buf[..(pw * ph * 4) as usize].chunks_exact_mut(4) {
            chunk.copy_from_slice(&bg_premul);
        }

        self.draw_label_over(
            buf, width, height, x_min, x_max, scale, align, text, fg_argb,
        );
    }

    /// Draws `text` exactly as [`Self::draw_label`] does — including the
    /// same `width`/`height`/`x_min`/`x_max` logical-pixels-at-`scale`
    /// convention — *without* filling the background first, for a caller
    /// that already painted its own background (a rounded panel row, say —
    /// see [`Self::fill_panel_background`]) and would otherwise have it
    /// clobbered by a flat rectangular fill.
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

        // Lay out the (possibly truncated) text to measure its pixel width.
        let mut layout: Layout<()> = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&fontdue::layout::LayoutSettings::default());
        layout.append(&[&self.font], &TextStyle::new(&text, font_size, 0));

        let glyphs = layout.glyphs();
        if glyphs.is_empty() {
            return;
        }

        // Measure total advance width.
        let first = glyphs.first().unwrap();
        let last = glyphs.last().unwrap();
        let total_w = (last.x + last.width as f32 - first.x) as i32;

        let avail_w = px_max - px_min;
        let x_off = match align {
            // Centred within [px_min, px_max].
            TextAlign::Center => px_min + (avail_w - total_w) / 2 - first.x as i32,
            // Flush against px_min.
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

    /// Draw the **maximize** icon (empty square) into `buf`, sized for a
    /// `width × height` logical cell at `scale` — see [`render_icon`]'s
    /// docs on the same convention [`Self::draw_label`] uses.
    ///
    /// `fg`/`bg` are straight colour — tiny-skia's own native input
    /// format, matching [`crate::config::Color`]'s representation exactly,
    /// so unlike [`Self::draw_label`] no premultiplication happens here:
    /// tiny-skia does it internally when it rasterizes each fill/stroke.
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

    /// Draw the **restore** icon (two overlapping squares) into `buf` —
    /// shown on the maximize button instead of [`Self::draw_icon_maximize`]
    /// once the window actually is maximized, so the button always
    /// communicates what pressing it will do next rather than what state
    /// the window happens to be in already. Same `width × height` at
    /// `scale` convention as [`Self::draw_icon_maximize`].
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

            // Back square: shifted up-right.
            if let Some(back) = tiny_skia::Rect::from_ltrb(x0 + offset, y0, x1, y1 - offset) {
                let mut pb = tiny_skia::PathBuilder::new();
                pb.push_rect(back);
                if let Some(path) = pb.finish() {
                    stroke(pixmap, &path, fg, height, scale);
                }
            }

            // Front square: shifted down-left. Filled with the button's own
            // background first, so it visually cuts into the back square's
            // stroke instead of merely overlapping it, then outlined.
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

    /// Draw the **close** icon (×) into `buf`. Same `width × height` at
    /// `scale` convention as [`Self::draw_icon_maximize`].
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

    /// Draw the **chevron** icon (∨) into `buf`. Same `width × height` at
    /// `scale` convention as [`Self::draw_icon_maximize`].
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

    /// Fills `buf` with `bg`, rounded at whichever corners `corners` marks
    /// (`[top_left, top_right, bottom_right, bottom_left]`) — spans the
    /// *entire* buffer, unlike [`Self::draw_icon_maximize`]'s inset pill,
    /// since panel rows must tile edge-to-edge with no gap between them.
    /// Used for the chevron dropdown's rows: pair with
    /// [`Self::draw_label_over`] afterward to add each row's text without
    /// it clobbering this background with its own flat fill. Same
    /// `width × height` at `scale` convention as
    /// [`Self::draw_icon_maximize`].
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
        // Path built in logical coordinates, same as the icons; `fill`
        // applies the scale transform at rasterization time.
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
/// `width`/`height` are *logical* pixels; the pixmap (and `buf`, which
/// must be sized to match) is allocated at `width * scale` by
/// `height * scale` physical pixels, with every path below built in the
/// logical coordinate space and rasterized through a `scale` transform —
/// see [`stroke`]/[`fill`]. That's what makes icon glyphs and pill
/// backgrounds crisp rather than blurrily upscaled on any output whose
/// scale isn't exactly `1`; `scale = 1` reproduces the prior behaviour
/// exactly.
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

/// The `(x_min, y_min, x_max, y_max)` bounds of a glyph drawn inside a
/// `width × height` icon buffer: a square, `GLYPH_RATIO` of
/// `min(width, height)` on a side, centred exactly on the cell's centre —
/// not merely inset from every edge by a fixed margin, which (for a
/// non-square cell, or from a caller re-deriving the far edge instead of
/// using the one already returned) is what previously left every glyph but
/// [`Painter::draw_icon_restore`] centred up and to the left of where it
/// should be, and at half the intended size. Every glyph is built from
/// this single box so they can never end up centred on different points,
/// or sized inconsistently with each other, again.
fn glyph_bounds(width: i32, height: i32) -> (f32, f32, f32, f32) {
    // Half the button, same ratio GNOME/KDE/Windows all converge on for a
    // header-bar button's glyph (see the module's own design notes).
    const GLYPH_RATIO: f32 = 0.5;

    let side = (width.min(height) as f32).max(4.0) * GLYPH_RATIO;
    let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
    let half = side / 2.0;
    (cx - half, cy - half, cx + half, cy + half)
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

/// Scale transform used to rasterize a logical-coordinate path onto a
/// `scale`-times-larger physical pixmap. `Transform::identity()` at
/// `scale = 1` — this is not a new code path for the common case, the
/// same identity transform every fill/stroke used before scale-awareness
/// existed.
#[allow(clippy::cast_precision_loss)]
fn scale_transform(scale: i32) -> tiny_skia::Transform {
    let s = scale as f32;
    tiny_skia::Transform::from_scale(s, s)
}

/// Strokes `path` (built in *logical* coordinates) with `icon_stroke`,
/// rasterized through `scale_transform(scale)` — which is what scales the
/// stroke width up by `scale` too, automatically: tiny-skia strokes in the
/// path's own coordinate space and applies the transform to the resulting
/// outline, the same as any other vector graphics transform convention.
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

/// Fills `path` (built in *logical* coordinates), rasterized through
/// `scale_transform(scale)` — see [`stroke`]'s docs on the same
/// convention.
fn fill(pixmap: &mut tiny_skia::Pixmap, path: &tiny_skia::Path, color: Color, scale: i32) {
    pixmap.fill_path(
        path,
        &paint_for(color),
        tiny_skia::FillRule::Winding,
        scale_transform(scale),
        None,
    );
}

/// How far a button's pill background is inset from the full cell it is
/// drawn in.
fn pill_margin(width: i32, height: i32) -> f32 {
    (width.min(height) as f32 * 0.125).max(2.0)
}

/// Fills a fully-rounded ("stadium") pill inset by `margin` from the
/// `width × height` logical cell, or does nothing if the inset leaves no
/// room. `width`/`height`/`margin` are logical; `scale` is forwarded to
/// [`fill`] for the rasterization transform.
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

/// The standard constant for approximating a quarter-circle arc with one
/// cubic Bézier: a corner of radius `r` is drawn by offsetting each cubic's
/// control points by `r * KAPPA` along the tangent.
const KAPPA: f32 = 0.552_284_8;

/// Builds a rounded-rectangle path via cubic-Bézier corners, all four
/// rounded. `radius` is clamped so it can never exceed half of whichever
/// of `w`/`h` is smaller, which would otherwise self-intersect.
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<tiny_skia::Path> {
    rounded_rect_corners(x, y, w, h, radius, [true; 4])
}

/// Builds a rectangle path with a per-corner choice of rounded vs. square,
/// `corners = [top_left, top_right, bottom_right, bottom_left]`.
///
/// Used for panels tiled from multiple rows stacked edge-to-edge (the
/// chevron dropdown) — only the panel's true outer corners should read as
/// rounded; a boundary between two rows must stay a hard edge, or the
/// panel would look like a stack of separate pills instead of one piece.
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
    fn rounded_rect_corners_with_all_square_matches_a_plain_rect() {
        let path = rounded_rect_corners(0.0, 0.0, 100.0, 50.0, 8.0, [false; 4]).unwrap();
        let bounds = path.bounds();
        assert!((bounds.width() - 100.0).abs() < 0.01);
        assert!((bounds.height() - 50.0).abs() < 0.01);
    }

    #[test]
    fn rounded_rect_corners_bounds_are_unaffected_by_which_corners_round() {
        // Rounding only cuts *into* the corners; the bounding box (the
        // straight edges' extent) is identical regardless of which corners
        // are selected.
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
        // Centre of the top edge is inside the panel regardless of corner
        // rounding (only the corners themselves are cut away) — must be
        // opaque, not left transparent like an icon's pill margin.
        let centre_top_offset = (50usize) * 4;
        assert_eq!(buf[centre_top_offset + 3], 255, "centre-top must be opaque");
        // A true corner pixel, on a rounded corner, must stay transparent.
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
            [true, true, false, false], // bottom corners square
        );
        // Bottom-left corner, unrounded: must be opaque, unlike the
        // rounded top-left tested above.
        let bottom_left_offset = (31 * 100) * 4;
        assert_eq!(
            buf[bottom_left_offset + 3],
            255,
            "square bottom-left corner must stay opaque"
        );
    }

    #[test]
    fn icon_drawing_functions_fill_the_whole_buffer_without_panicking() {
        // Smoke test: every icon function must produce exactly
        // (width*scale)*(height*scale)*4 bytes and never panic, across a
        // range of logical sizes (including tiny/odd ones a real titlebar
        // height could produce) and scales (1 = the common case today; 2
        // exercises the HiDPI path with nothing else changed).
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
        // The whole point of the pill: corners stay transparent so the
        // titlebar's own background shows through underneath, rather than
        // a hard-edged coloured block covering the full cell.
        let mut buf = vec![0u8; 32 * 32 * 4];
        Painter::draw_icon_maximize(
            &mut buf,
            32,
            32,
            1,
            Color([1.0, 1.0, 1.0, 1.0]),
            Color([1.0, 0.0, 0.0, 1.0]),
        );
        let corner = &buf[0..4]; // top-left pixel
        assert_eq!(corner[3], 0, "corner pixel must be fully transparent");
    }

    #[test]
    fn icon_at_scale_2_produces_a_physically_larger_buffer_than_scale_1() {
        // The point of scale-awareness: a HiDPI buffer is rasterized at
        // (width*scale, height*scale) physical pixels, not a
        // logical-resolution buffer stretched afterward.
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

    /// Alpha-weighted centroid of every pixel in a straight-alpha BGRA
    /// buffer, in logical pixels (`scale` divides the physical coordinates
    /// back out). Used below to check a glyph is actually centred in its
    /// cell — a plain "did it draw something" smoke test would have
    /// missed the double-margin bug entirely, since every glyph still drew
    /// *a* shape, just off-centre and at half size.
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
    #[allow(clippy::similar_names)] // expected_cx/expected_cy: an x/y pair, not a typo risk
    fn close_glyph_is_centred_on_the_cell_for_square_and_non_square_cells() {
        // Regression test for the double-margin bug: `icon_bounds` already
        // returned the far *edge*, but every caller but `draw_icon_restore`
        // subtracted the margin from it a second time, centring the glyph
        // up-and-left of the cell centre instead of on it — worst on a
        // non-square cell, where the old margin was derived from `height`
        // alone and so ignored `width` entirely.
        for (w, h) in [(32, 32), (40, 26), (26, 40), (32, 44)] {
            let mut buf = vec![0u8; (w * h * 4) as usize];
            Painter::draw_icon_close(
                &mut buf,
                w,
                h,
                1,
                Color([1.0, 1.0, 1.0, 1.0]),
                Color([0.0, 0.0, 0.0, 0.0]), // transparent bg isolates the glyph
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
        // Same centring check as the close glyph, plus: the centroid in
        // *logical* pixels must land in the same place at scale 1 and
        // scale 2, since `alpha_centroid` already divides the physical
        // coordinates back down — if it didn't, a HiDPI titlebar would be
        // rendering its glyphs off-centre even after the scale-1 fix.
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
        // Pins the 50% ratio every shipping desktop's header-bar buttons
        // converge on (see this module's design notes) — and would have
        // caught the old bug's *other* half: even where a glyph happened
        // to be reachable, it was drawn at half the intended size.
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
        // A HiDPI draw call rasterizes at `font_size * scale`; that must
        // land in its own cache entry, not silently reuse (or collide
        // with) the logical-scale bitmap.
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
        // Background must have been painted at least (buffer isn't left
        // all-zero) — a weak but real smoke assertion that something drew.
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

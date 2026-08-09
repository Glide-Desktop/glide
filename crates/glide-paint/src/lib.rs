#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::too_many_arguments
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use cosmic_text::{
    Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics, Shaping, SwashCache, Weight,
    Wrap, fontdb,
};

use glide_theme::{Color, TextStyle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
}

mod wallpaper;
pub use wallpaper::{
    compose_wallpaper, decode_wallpaper, draw_wallpaper, draw_wallpaper_into,
    embedded_wallpaper_bytes, fit_transform, init_wallpaper,
};

mod blur;

mod border;
pub use border::{ShadowLayerSpec, draw_inset_border, draw_outset_shadow_and_border};

static NOTO_SANS_BYTES: &[u8] = include_bytes!("fonts/NotoSans-Regular.ttf");

const DEFAULT_FAMILY: &str = "Adwaita Sans";

static PAINTER: RwLock<Option<Arc<Painter>>> = RwLock::new(None);

pub fn init_or_replace(font_size: f32) {
    let mut slot = PAINTER
        .write()
        .expect("single-threaded event loop, this lock is never poisoned");
    *slot = Some(Arc::new(Painter::new(font_size)));
}

#[must_use]
pub fn current() -> Arc<Painter> {
    let slot = PAINTER
        .read()
        .expect("single-threaded event loop, this lock is never poisoned");
    slot.clone().unwrap_or_else(|| Arc::new(Painter::new(14.0)))
}

const DEFAULT_WEIGHT: u16 = Weight::NORMAL.0;
const DEFAULT_TRACKING_EM: f32 = 0.0;

const MAX_CACHED_TEXTS_PER_STYLE: usize = 512;

type StyleKey = (u32, u16, u32);

fn style_key(font_size: f32, weight: u16, tracking_em: f32) -> StyleKey {
    (font_size.to_bits(), weight, tracking_em.to_bits())
}

pub struct Painter {
    font_system: Mutex<FontSystem>,
    swash_cache: Mutex<SwashCache>,
    width_cache: Mutex<HashMap<StyleKey, HashMap<String, f32>>>,
    font_size: f32,
    family: String,
}

struct LineMetrics {
    width: f32,
    baseline_y: f32,
    max_ascent: f32,
    max_descent: f32,
}

impl Painter {
    #[must_use]
    pub fn new(font_size: f32) -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        db.load_font_data(NOTO_SANS_BYTES.to_vec());
        Self::from_db(db, font_size)
    }

    #[cfg(test)]
    #[must_use]
    fn with_embedded_font_only(font_size: f32) -> Self {
        let mut db = fontdb::Database::new();
        db.load_font_data(NOTO_SANS_BYTES.to_vec());
        Self::from_db(db, font_size)
    }

    fn from_db(db: fontdb::Database, font_size: f32) -> Self {
        let font_system = FontSystem::new_with_locale_and_db("en-US".to_string(), db);
        Self {
            font_system: Mutex::new(font_system),
            swash_cache: Mutex::new(SwashCache::new()),
            width_cache: Mutex::new(HashMap::new()),
            font_size,
            family: DEFAULT_FAMILY.to_string(),
        }
    }

    fn shape(
        &self,
        font_system: &mut FontSystem,
        text: &str,
        font_size: f32,
        weight: u16,
        tracking_em: f32,
    ) -> Buffer {
        let metrics = Metrics::new(font_size, font_size * 1.2);
        let mut buffer = Buffer::new(font_system, metrics);
        buffer.set_wrap(Wrap::None);
        let mut attrs = Attrs::new()
            .family(Family::Name(&self.family))
            .weight(Weight(weight));
        if tracking_em != DEFAULT_TRACKING_EM {
            attrs = attrs.letter_spacing(tracking_em);
        }
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(font_system, false);
        buffer
    }

    fn line_metrics(font_system: &mut FontSystem, buffer: &mut Buffer) -> LineMetrics {
        let (width, max_ascent, max_descent) = buffer
            .line_layout(font_system, 0)
            .and_then(|lines| lines.first())
            .map_or((0.0, 0.0, 0.0), |line| {
                (line.w, line.max_ascent, line.max_descent)
            });
        let baseline_y = buffer.layout_runs().next().map_or(0.0, |run| run.line_y);
        LineMetrics {
            width,
            baseline_y,
            max_ascent,
            max_descent,
        }
    }

    fn measure_width_uncached(
        &self,
        text: &str,
        font_size: f32,
        weight: u16,
        tracking_em: f32,
    ) -> f32 {
        let mut font_system = self
            .font_system
            .lock()
            .expect("single-threaded event loop, this lock is never poisoned");
        let mut buffer = self.shape(&mut font_system, text, font_size, weight, tracking_em);
        Self::line_metrics(&mut font_system, &mut buffer).width
    }

    fn measure_width_cached(
        &self,
        text: &str,
        font_size: f32,
        weight: u16,
        tracking_em: f32,
    ) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let key = style_key(font_size, weight, tracking_em);

        if let Some(width) = self
            .width_cache
            .lock()
            .expect("single-threaded event loop, this lock is never poisoned")
            .get(&key)
            .and_then(|by_text| by_text.get(text))
        {
            return *width;
        }

        let width = self.measure_width_uncached(text, font_size, weight, tracking_em);

        let mut cache = self
            .width_cache
            .lock()
            .expect("single-threaded event loop, this lock is never poisoned");
        let by_text = cache.entry(key).or_default();
        if by_text.len() >= MAX_CACHED_TEXTS_PER_STYLE {
            by_text.clear();
        }
        by_text.insert(text.to_owned(), width);

        width
    }

    fn fit_text_at(
        &self,
        text: &str,
        avail_w: i32,
        font_size: f32,
        weight: u16,
        tracking_em: f32,
    ) -> String {
        let avail_w = avail_w as f32;
        if avail_w <= 0.0 {
            return String::new();
        }
        if self.measure_width_cached(text, font_size, weight, tracking_em) <= avail_w {
            return text.to_owned();
        }

        let chars: Vec<char> = text.chars().collect();
        let candidate_at = |k: usize| -> String { chars[..k].iter().collect::<String>() + "…" };
        let fits = |k: usize| -> bool {
            self.measure_width_cached(&candidate_at(k), font_size, weight, tracking_em) <= avail_w
        };

        if chars.is_empty() || !fits(0) {
            return String::new();
        }

        let mut lo = 0usize;
        let mut hi = chars.len() - 1;
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if fits(mid) {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        candidate_at(lo)
    }

    #[must_use]
    pub fn text_width(&self, text: &str) -> f32 {
        self.measure_width_cached(text, self.font_size, DEFAULT_WEIGHT, DEFAULT_TRACKING_EM)
    }

    #[must_use]
    pub fn text_width_styled(&self, text: &str, style: &TextStyle) -> f32 {
        let text = Self::apply_case(text, style);
        self.measure_width_cached(&text, style.size_px, style.weight, style.tracking_em)
    }

    fn apply_case<'a>(text: &'a str, style: &TextStyle) -> std::borrow::Cow<'a, str> {
        if style.uppercase {
            std::borrow::Cow::Owned(text.to_uppercase())
        } else {
            std::borrow::Cow::Borrowed(text)
        }
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
        #[allow(clippy::cast_precision_loss)]
        let font_size = self.font_size * scale as f32;
        self.draw_label_at(
            buf,
            width,
            height,
            x_min,
            x_max,
            0,
            height,
            scale,
            align,
            text,
            fg_argb,
            font_size,
            DEFAULT_WEIGHT,
            DEFAULT_TRACKING_EM,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_label_over_styled(
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
        style: &TextStyle,
    ) {
        let text = Self::apply_case(text, style);
        #[allow(clippy::cast_precision_loss)]
        let font_size = style.size_px * scale as f32;
        self.draw_label_at(
            buf,
            width,
            height,
            x_min,
            x_max,
            0,
            height,
            scale,
            align,
            &text,
            fg_argb,
            font_size,
            style.weight,
            style.tracking_em,
        );
    }

    /// Like `draw_label_over_styled`, but vertically centred within
    /// `[y_min, y_max)` instead of the whole `(0, height)` canvas — for
    /// placing more than one line of text into a single shared buffer (a
    /// popover menu's rows, say), where each line needs to land in its own
    /// band rather than all collapsing onto the buffer's own centre.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_label_over_styled_in_band(
        &self,
        buf: &mut [u8],
        width: i32,
        height: i32,
        x_min: i32,
        x_max: i32,
        y_min: i32,
        y_max: i32,
        scale: i32,
        align: TextAlign,
        text: &str,
        fg_argb: [u8; 4],
        style: &TextStyle,
    ) {
        let text = Self::apply_case(text, style);
        #[allow(clippy::cast_precision_loss)]
        let font_size = style.size_px * scale as f32;
        self.draw_label_at(
            buf,
            width,
            height,
            x_min,
            x_max,
            y_min,
            y_max,
            scale,
            align,
            &text,
            fg_argb,
            font_size,
            style.weight,
            style.tracking_em,
        );
    }

    #[allow(clippy::too_many_arguments, clippy::similar_names)]
    fn draw_label_at(
        &self,
        buf: &mut [u8],
        width: i32,
        height: i32,
        x_min: i32,
        x_max: i32,
        y_min: i32,
        y_max: i32,
        scale: i32,
        align: TextAlign,
        text: &str,
        fg_argb: [u8; 4],
        font_size: f32,
        weight: u16,
        tracking_em: f32,
    ) {
        let (pw, ph) = (width * scale, height * scale);
        if pw <= 0 || ph <= 0 || buf.len() < (pw * ph * 4) as usize {
            return;
        }
        let (px_min, px_max) = (x_min * scale, x_max * scale);
        let (py_min, py_max) = (y_min * scale, y_max * scale);

        let text = self.fit_text_at(text, px_max - px_min, font_size, weight, tracking_em);
        if text.is_empty() {
            return;
        }

        let mut font_system = self
            .font_system
            .lock()
            .expect("single-threaded event loop, this lock is never poisoned");
        let mut swash_cache = self
            .swash_cache
            .lock()
            .expect("single-threaded event loop, this lock is never poisoned");

        let mut buffer = self.shape(&mut font_system, &text, font_size, weight, tracking_em);
        let metrics = Self::line_metrics(&mut font_system, &mut buffer);
        if metrics.width <= 0.0 {
            return;
        }
        let total_w = metrics.width as i32;

        let avail_w = px_max - px_min;
        let x_off = match align {
            TextAlign::Center => px_min + (avail_w - total_w) / 2,
            TextAlign::Left => px_min,
        };
        let content_h = metrics.max_ascent + metrics.max_descent;
        let band_h = (py_max - py_min) as f32;
        let y_off =
            py_min + ((band_h - content_h) / 2.0 - metrics.baseline_y + metrics.max_ascent) as i32;

        let fg = TextColor::rgba(fg_argb[0], fg_argb[1], fg_argb[2], fg_argb[3]);
        buffer.draw(
            &mut font_system,
            &mut swash_cache,
            fg,
            |x, y, gw, gh, color| {
                let (r, g, b, a) = color.as_rgba_tuple();
                if a == 0 {
                    return;
                }
                for row in 0..gh as i32 {
                    for col in 0..gw as i32 {
                        let px = x_off + x + col;
                        let py = y_off + y + row;
                        if px < 0 || py < 0 || px >= pw || py >= ph {
                            continue;
                        }
                        let offset = ((py * pw + px) * 4) as usize;
                        if offset + 3 >= buf.len() {
                            continue;
                        }
                        let dst: [u8; 4] = buf[offset..offset + 4].try_into().unwrap();
                        let out = composite_over(dst, [r, g, b], a);
                        buf[offset..offset + 4].copy_from_slice(&out);
                    }
                }
            },
        );
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
            let w = x1 - x0;
            let h = y1 - y0;
            let rx = (w * (1.6 / 7.6)).max(1.0); // 1.6 out of 7.6 width
            if let Some(path) = rounded_rect(x0, y0, w, h, rx) {
                stroke(pixmap, &path, fg, height, scale);
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
            let rx = ((x1 - offset - x0) * (1.6 / 7.6)).max(1.0);

            if let Some(path) =
                rounded_rect(x0 + offset, y0, x1 - offset - x0, y1 - offset - y0, rx)
            {
                stroke(pixmap, &path, fg, height, scale);
            }

            if let Some(path) =
                rounded_rect(x0, y0 + offset, x1 - offset - x0, y1 - offset - y0, rx)
            {
                fill(pixmap, &path, bg, scale);
                stroke(pixmap, &path, fg, height, scale);
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

    /// Fills a rounded rect *within* an already-populated buffer, blended
    /// over whatever is already there — unlike `fill_panel_background`,
    /// which always owns the whole `(0, 0, width, height)` canvas and
    /// overwrites it outright. This is what lets a single combined buffer
    /// (the popover panel) hold its own background *and* a per-row hover
    /// highlight painted on top of it, instead of one buffer per row.
    pub fn fill_rounded_rect_over(
        buf: &mut [u8],
        canvas_width: i32,
        canvas_height: i32,
        rect_x: i32,
        rect_y: i32,
        rect_width: i32,
        rect_height: i32,
        scale: i32,
        color: Color,
        radius: f32,
    ) {
        let (Ok(cw), Ok(ch)) = (
            u32::try_from(canvas_width * scale),
            u32::try_from(canvas_height * scale),
        ) else {
            return;
        };
        let Some(mut pixmap) = tiny_skia::PixmapMut::from_bytes(buf, cw, ch) else {
            return;
        };
        let Some(path) = rounded_rect(
            (rect_x * scale) as f32,
            (rect_y * scale) as f32,
            (rect_width * scale) as f32,
            (rect_height * scale) as f32,
            radius * scale as f32,
        ) else {
            return;
        };
        // `buf` is already in this crate's BGRA output order (the swap
        // normally happens once, in `skia_pixmap_into_buffer_bytes`, when a
        // *fresh* tiny-skia Pixmap is exported) — wrapping it directly with
        // `PixmapMut` skips that whole allocate/export/copy cycle for a
        // solid fill, but means the fill color itself needs its R/B
        // channels swapped going in, so what tiny-skia writes as "RGBA"
        // lands correctly as BGRA in memory.
        let [r, g, b, a] = color.0;
        let bgra_color = tiny_skia::Color::from_rgba(
            b.clamp(0.0, 1.0),
            g.clamp(0.0, 1.0),
            r.clamp(0.0, 1.0),
            a.clamp(0.0, 1.0),
        )
        .unwrap_or(tiny_skia::Color::TRANSPARENT);
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(bgra_color),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }
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

    if let Some(path) = rounded_rect(
        0.0,
        0.0,
        width as f32,
        height as f32,
        glide_theme::shape::BUTTON_RADIUS,
    ) {
        fill(&mut pixmap, &path, bg, scale);
    }

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
        let dst = [0, 0, 0, 255];
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
    fn fill_rounded_rect_over_preserves_bgra_byte_order() {
        let (w, h) = (20, 20);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let red = Color([1.0, 0.0, 0.0, 1.0]);
        Painter::fill_rounded_rect_over(&mut buf, w, h, 0, 0, w, h, 1, red, 0.0);
        let center = ((h / 2 * w + w / 2) * 4) as usize;
        assert_eq!(
            &buf[center..center + 4],
            &[0, 0, 255, 255],
            "opaque red must store R in byte 2, matching Color::to_buffer_bytes' BGRA order"
        );
    }

    #[test]
    fn fill_rounded_rect_over_only_touches_the_rect_it_covers() {
        let (w, h) = (40, 20);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        Painter::fill_rounded_rect_over(
            &mut buf,
            w,
            h,
            20,
            0,
            20,
            h,
            1,
            Color([1.0, 1.0, 1.0, 1.0]),
            0.0,
        );
        let left_untouched = ((h / 2 * w + 5) * 4) as usize;
        let right_filled = ((h / 2 * w + 30) * 4) as usize;
        assert_eq!(buf[left_untouched + 3], 0, "left half must stay untouched");
        assert_eq!(
            buf[right_filled + 3],
            255,
            "right half (the rect) must be filled"
        );
    }

    #[test]
    fn fill_rounded_rect_over_blends_over_existing_content_not_replacing_the_canvas() {
        let (w, h) = (20, 20);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        Painter::fill_panel_background(
            &mut buf,
            w,
            h,
            1,
            Color([0.0, 0.0, 1.0, 1.0]),
            0.0,
            [true; 4],
        );
        let before = buf.clone();
        Painter::fill_rounded_rect_over(
            &mut buf,
            w,
            h,
            0,
            0,
            w / 2,
            h,
            1,
            Color([1.0, 0.0, 0.0, 0.5]),
            0.0,
        );
        assert_ne!(buf, before, "the overlay must have changed something");
        let right_half_untouched = ((h / 2 * w + 3 * w / 4) * 4) as usize;
        assert_eq!(
            &buf[right_half_untouched..right_half_untouched + 4],
            &before[right_half_untouched..right_half_untouched + 4],
            "the right half, outside the overlay rect, must be exactly what fill_panel_background left"
        );
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
        let painter = Painter::with_embedded_font_only(14.0);
        assert_eq!(
            painter.fit_text_at(
                "ok",
                1000,
                painter.font_size,
                DEFAULT_WEIGHT,
                DEFAULT_TRACKING_EM
            ),
            "ok"
        );
    }

    #[test]
    fn fit_text_truncates_with_an_ellipsis_when_too_narrow() {
        let painter = Painter::with_embedded_font_only(14.0);
        let long = "a very long window title that will not fit";
        let fitted = painter.fit_text_at(
            long,
            40,
            painter.font_size,
            DEFAULT_WEIGHT,
            DEFAULT_TRACKING_EM,
        );
        assert!(fitted.ends_with('…'), "{fitted:?}");
        assert!(
            fitted.len() < long.len(),
            "expected shortening, got {fitted:?}"
        );
    }

    #[test]
    fn fit_text_truncation_matches_a_naive_linear_search() {
        let painter = Painter::with_embedded_font_only(14.0);
        let long = "a very long window title that will not fit in the available space";
        for avail_w in [10, 30, 60, 100, 200] {
            let binary = painter.fit_text_at(
                long,
                avail_w,
                painter.font_size,
                DEFAULT_WEIGHT,
                DEFAULT_TRACKING_EM,
            );

            let chars: Vec<char> = long.chars().collect();
            let mut linear = String::new();
            let mut probe = chars.clone();
            while !probe.is_empty() {
                probe.pop();
                let candidate: String = probe.iter().collect::<String>() + "…";
                if painter.measure_width_cached(
                    &candidate,
                    painter.font_size,
                    DEFAULT_WEIGHT,
                    DEFAULT_TRACKING_EM,
                ) <= avail_w as f32
                {
                    linear = candidate;
                    break;
                }
            }
            assert_eq!(binary, linear, "avail_w={avail_w}");
        }
    }

    #[test]
    fn fit_text_on_zero_width_is_empty() {
        let painter = Painter::with_embedded_font_only(14.0);
        assert_eq!(
            painter.fit_text_at(
                "anything",
                0,
                painter.font_size,
                DEFAULT_WEIGHT,
                DEFAULT_TRACKING_EM
            ),
            ""
        );
    }

    #[test]
    fn fit_text_on_empty_input_is_empty() {
        let painter = Painter::with_embedded_font_only(14.0);
        assert_eq!(
            painter.fit_text_at(
                "",
                1000,
                painter.font_size,
                DEFAULT_WEIGHT,
                DEFAULT_TRACKING_EM
            ),
            ""
        );
    }

    #[test]
    fn measure_width_grows_with_a_larger_font_size() {
        let painter = Painter::with_embedded_font_only(14.0);
        let small =
            painter.measure_width_cached("Hello", 14.0, DEFAULT_WEIGHT, DEFAULT_TRACKING_EM);
        let large =
            painter.measure_width_cached("Hello", 28.0, DEFAULT_WEIGHT, DEFAULT_TRACKING_EM);
        assert!(
            large > small,
            "a 2x font size must measure a wider line, got small={small} large={large}"
        );
    }

    #[test]
    fn measure_width_cache_returns_the_same_value_as_an_uncached_measurement() {
        let painter = Painter::with_embedded_font_only(14.0);
        let cached =
            painter.measure_width_cached("Hello, world", 14.0, DEFAULT_WEIGHT, DEFAULT_TRACKING_EM);
        let again =
            painter.measure_width_cached("Hello, world", 14.0, DEFAULT_WEIGHT, DEFAULT_TRACKING_EM);
        let uncached = painter.measure_width_uncached(
            "Hello, world",
            14.0,
            DEFAULT_WEIGHT,
            DEFAULT_TRACKING_EM,
        );
        assert!((cached - uncached).abs() < f32::EPSILON);
        assert!(
            (cached - again).abs() < f32::EPSILON,
            "a cache hit must not drift"
        );
    }

    #[test]
    fn style_key_is_distinct_per_weight_and_tracking() {
        let base = style_key(14.0, 400, 0.0);
        assert_ne!(
            base,
            style_key(14.0, 700, 0.0),
            "weight must change the key"
        );
        assert_ne!(
            base,
            style_key(14.0, 400, 0.1),
            "tracking must change the key"
        );
        assert_ne!(base, style_key(15.0, 400, 0.0), "size must change the key");
        assert_eq!(
            base,
            style_key(14.0, 400, 0.0),
            "identical inputs must collide"
        );
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn measure_width_cache_is_isolated_by_tracking() {
        let painter = Painter::with_embedded_font_only(14.0);
        let regular = painter.measure_width_cached("Hello, world", 14.0, 400, 0.0);
        let tracked = painter.measure_width_cached("Hello, world", 14.0, 400, 0.3);
        assert_ne!(regular, tracked, "letter-spacing must measure differently");
    }

    #[test]
    fn text_width_styled_uppercases_when_the_style_asks_for_it() {
        let painter = Painter::with_embedded_font_only(14.0);
        let lower = painter.text_width_styled("micro", &TextStyle::MICRO);
        let upper = painter.text_width_styled("MICRO", &TextStyle::MICRO);
        assert!(
            (lower - upper).abs() < 0.01,
            "MICRO must uppercase its input before measuring, so both spellings measure the same"
        );
    }

    #[test]
    fn text_width_styled_reflects_the_roles_own_size_not_the_painters_default() {
        let painter = Painter::with_embedded_font_only(16.0);
        let small = painter.text_width_styled("Overview", &TextStyle::CAP);
        let large = painter.text_width_styled("Overview", &TextStyle::DISPLAY);
        assert!(
            large > small,
            "DISPLAY (36px) must measure wider than CAP (12px), got small={small} large={large}"
        );
    }

    #[test]
    fn draw_label_over_is_deterministic_across_repeated_calls() {
        let painter = Painter::with_embedded_font_only(14.0);
        let render = || {
            let mut buf = vec![0u8; 200 * 32 * 4];
            painter.draw_label_over(
                &mut buf,
                200,
                32,
                32,
                168,
                1,
                TextAlign::Left,
                "Hello",
                [255, 255, 255, 255],
            );
            buf
        };
        assert_eq!(
            render(),
            render(),
            "shaping and rasterising the same text twice must produce identical pixels"
        );
    }

    #[test]
    fn init_or_replace_and_current_round_trip() {
        init_or_replace(21.0);
        let painter = current();
        assert!((painter.font_size - 21.0).abs() < f32::EPSILON);

        let painter_again = current();
        assert!(
            Arc::ptr_eq(&painter, &painter_again),
            "repeated current() calls must share one Painter allocation, \
             not deep-copy the embedded font on every call"
        );
    }

    #[test]
    fn draw_label_over_does_not_panic_on_overflowing_text() {
        let painter = Painter::with_embedded_font_only(14.0);
        let mut buf = vec![0u8; 200 * 32 * 4];
        painter.draw_label_over(
            &mut buf,
            200,
            32,
            32,
            168,
            1,
            TextAlign::Left,
            "a title so long it must be ellipsised to fit the available space",
            [255, 255, 255, 255],
        );
        assert!(buf.iter().any(|&b| b != 0));
    }

    #[test]
    fn draw_label_over_at_scale_2_produces_a_physically_larger_buffer() {
        let painter = Painter::with_embedded_font_only(14.0);
        let mut buf1 = vec![0u8; 200 * 32 * 4];
        painter.draw_label_over(
            &mut buf1,
            200,
            32,
            32,
            168,
            1,
            TextAlign::Left,
            "Hello",
            [255, 255, 255, 255],
        );
        let mut buf2 = vec![0u8; 400 * 64 * 4];
        painter.draw_label_over(
            &mut buf2,
            200,
            32,
            32,
            168,
            2,
            TextAlign::Left,
            "Hello",
            [255, 255, 255, 255],
        );
        assert!(buf1.iter().any(|&b| b != 0));
        assert!(buf2.iter().any(|&b| b != 0));
    }

    #[test]
    fn left_align_and_center_align_place_glyphs_differently() {
        let painter = Painter::with_embedded_font_only(14.0);
        let mut left_buf = vec![0u8; 200 * 32 * 4];
        let mut center_buf = vec![0u8; 200 * 32 * 4];
        painter.draw_label_over(
            &mut left_buf,
            200,
            32,
            20,
            180,
            1,
            TextAlign::Left,
            "Hi",
            [255, 255, 255, 255],
        );
        painter.draw_label_over(
            &mut center_buf,
            200,
            32,
            20,
            180,
            1,
            TextAlign::Center,
            "Hi",
            [255, 255, 255, 255],
        );
        assert_ne!(
            left_buf, center_buf,
            "left- and centre-aligned short text must land in different pixels"
        );
    }
}

#[cfg(test)]
mod golden_tests {
    use super::*;

    fn goldens_dir(category: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/goldens")
            .join(category)
    }

    fn bgra_premul_to_rgba_premul(buf: &[u8]) -> Vec<u8> {
        let mut out = buf.to_vec();
        for px in out.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        out
    }

    #[must_use]
    fn check_golden_rgba_premul(
        category: &str,
        name: &str,
        width: u32,
        height: u32,
        rgba_premul: &[u8],
    ) -> Option<String> {
        let mut pixmap = tiny_skia::Pixmap::new(width, height)
            .unwrap_or_else(|| panic!("golden {name}: {width}x{height} must be nonzero"));
        let len = pixmap.data().len();
        pixmap.data_mut().copy_from_slice(&rgba_premul[..len]);
        let png = pixmap
            .encode_png()
            .unwrap_or_else(|e| panic!("golden {name}: failed to encode png: {e}"));

        let dir = goldens_dir(category);
        let path = dir.join(format!("{name}.png"));
        if let Ok(existing) = std::fs::read(&path) {
            if existing == png {
                return None;
            }
            let actual_path = dir.join(format!("{name}.actual.png"));
            std::fs::write(&actual_path, &png)
                .unwrap_or_else(|e| panic!("golden {name}: failed to write actual: {e}"));
            return Some(format!(
                "golden mismatch for {name}: wrote {actual_path:?} for review; \
                 if the change is intentional, replace {path:?} with it"
            ));
        }
        std::fs::create_dir_all(&dir)
            .unwrap_or_else(|e| panic!("golden {name}: failed to create dir: {e}"));
        std::fs::write(&path, &png)
            .unwrap_or_else(|e| panic!("golden {name}: failed to write baseline: {e}"));
        Some(format!(
            "golden {name} did not exist; created a new baseline at {path:?} — \
             review it, then re-run tests and commit it"
        ))
    }

    fn assert_all_goldens(failures: &[String]) {
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    fn corner_name(corners: [bool; 4]) -> String {
        let bit = |b: bool| i32::from(b);
        format!(
            "tl{}_tr{}_br{}_bl{}",
            bit(corners[0]),
            bit(corners[1]),
            bit(corners[2]),
            bit(corners[3])
        )
    }

    #[test]
    fn fill_panel_background_every_corner_combination() {
        let (width, height) = (64, 24);
        let mut failures = Vec::new();
        for tl in [false, true] {
            for tr in [false, true] {
                for br in [false, true] {
                    for bl in [false, true] {
                        let corners = [tl, tr, br, bl];
                        let mut buf = vec![0u8; (width * height * 4) as usize];
                        Painter::fill_panel_background(
                            &mut buf,
                            width,
                            height,
                            1,
                            Color([0.14, 0.16, 0.22, 1.0]),
                            8.0,
                            corners,
                        );
                        let rgba = bgra_premul_to_rgba_premul(&buf);
                        failures.extend(check_golden_rgba_premul(
                            "chrome",
                            &format!("fill_panel_background_{}", corner_name(corners)),
                            width as u32,
                            height as u32,
                            &rgba,
                        ));
                    }
                }
            }
        }
        assert_all_goldens(&failures);
    }

    type IconFn = fn(&mut [u8], i32, i32, i32, Color, Color);

    #[test]
    fn icons_at_scale_1_and_2() {
        let fg = Color([1.0, 1.0, 1.0, 1.0]);
        let bg = Color([0.12, 0.12, 0.15, 1.0]);
        let icons: [(&str, IconFn); 3] = [
            ("maximize", Painter::draw_icon_maximize),
            ("restore", Painter::draw_icon_restore),
            ("close", Painter::draw_icon_close),
        ];
        let mut failures = Vec::new();
        for (label, icon) in icons {
            for scale in [1, 2] {
                let (w, h) = (32, 32);
                let mut buf = vec![0u8; (w * scale * h * scale * 4) as usize];
                icon(&mut buf, w, h, scale, fg, bg);
                let rgba = bgra_premul_to_rgba_premul(&buf);
                failures.extend(check_golden_rgba_premul(
                    "chrome",
                    &format!("icon_{label}_scale{scale}"),
                    (w * scale) as u32,
                    (h * scale) as u32,
                    &rgba,
                ));
            }
        }
        assert_all_goldens(&failures);
    }

    #[test]
    fn draw_label_over_ascii_and_cyrillic() {
        let painter = Painter::with_embedded_font_only(16.0);
        let (width, height) = (120, 32);
        let mut failures = Vec::new();
        for (name, text) in [
            ("draw_label_over_ascii", "Hello world"),
            ("draw_label_over_cyrillic", "Привіт світ"),
        ] {
            let mut buf = vec![0u8; (width * height * 4) as usize];
            painter.draw_label_over(
                &mut buf,
                width,
                height,
                4,
                width - 4,
                1,
                TextAlign::Left,
                text,
                [255, 255, 255, 255],
            );
            failures.extend(check_golden_rgba_premul(
                "text",
                name,
                width as u32,
                height as u32,
                &buf,
            ));
        }
        assert_all_goldens(&failures);
    }

    #[test]
    fn window_shadow_and_border_focused_and_idle() {
        let (buf_w, buf_h) = (220, 160);
        let (window_w, window_h) = (140, 80);
        let mut failures = Vec::new();

        for (name, layers, border_width, border_color) in [
            (
                "window_shadow_idle",
                [
                    ShadowLayerSpec {
                        dy: 3.30,
                        blur: 7.68,
                        color: Color([0.0, 0.0, 0.0, 0.26]),
                    },
                    ShadowLayerSpec {
                        dy: 13.76,
                        blur: 32.0,
                        color: Color([0.0, 0.0, 0.0, 0.4]),
                    },
                ],
                1.0,
                Color([1.0, 1.0, 1.0, 0.05]),
            ),
            (
                "window_shadow_focused",
                [
                    ShadowLayerSpec {
                        dy: 7.02,
                        blur: 16.32,
                        color: Color([0.0, 0.0, 0.0, 0.39]),
                    },
                    ShadowLayerSpec {
                        dy: 29.24,
                        blur: 68.0,
                        color: Color([0.0, 0.0, 0.0, 0.6]),
                    },
                ],
                1.0,
                Color([1.0, 1.0, 1.0, 0.10]),
            ),
        ] {
            let mut buf = vec![0u8; (buf_w * buf_h * 4) as usize];
            draw_outset_shadow_and_border(
                &mut buf,
                buf_w,
                buf_h,
                window_w as f32,
                window_h as f32,
                &layers,
                border_width,
                border_color,
                12.0,
            );
            let rgba = bgra_premul_to_rgba_premul(&buf);
            failures.extend(check_golden_rgba_premul(
                "chrome",
                name,
                buf_w as u32,
                buf_h as u32,
                &rgba,
            ));
        }
        assert_all_goldens(&failures);
    }
}

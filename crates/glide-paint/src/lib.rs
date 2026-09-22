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
    Right,
}

mod wallpaper;
pub use wallpaper::{
    compose_wallpaper, decode_wallpaper, desaturate_in_place, draw_wallpaper, draw_wallpaper_into,
    embedded_wallpaper_bytes, fit_transform, init_wallpaper, wallpaper_generation,
};

mod blur;
pub use blur::box_blur_rgba;

mod border;
pub use border::{
    SHADOW_DY_RATIO, ShadowLayerSpec, blit_rounded_backdrop, draw_inset_border,
    draw_outset_shadow_and_border, shadow_fade_margin,
};

static EMBEDDED_FONT_BYTES: &[u8] = include_bytes!("fonts/LiberationSans-Regular.ttf");

const DEFAULT_FAMILY: &str = "Adwaita Sans";

static PAINTER: RwLock<Option<Arc<Painter>>> = RwLock::new(None);
static PAINTER_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SYSTEM_FONTS: RwLock<Option<Arc<fontdb::Database>>> = RwLock::new(None);

fn system_font_database() -> Arc<fontdb::Database> {
    if let Some(db) = SYSTEM_FONTS
        .read()
        .expect("single-threaded event loop, this lock is never poisoned")
        .clone()
    {
        return db;
    }

    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    db.load_font_data(EMBEDDED_FONT_BYTES.to_vec());
    let db = Arc::new(db);

    let mut slot = SYSTEM_FONTS
        .write()
        .expect("single-threaded event loop, this lock is never poisoned");
    *slot = Some(Arc::clone(&db));
    db
}

pub fn init_or_replace(font_size: f32, family: Option<&str>) {
    let painter = Arc::new(Painter::new(font_size, family));
    let mut slot = PAINTER
        .write()
        .expect("single-threaded event loop, this lock is never poisoned");
    *slot = Some(painter);
    PAINTER_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[must_use]
pub fn painter_generation() -> u64 {
    PAINTER_GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

#[must_use]
pub fn current() -> Arc<Painter> {
    if let Some(painter) = PAINTER
        .read()
        .expect("single-threaded event loop, this lock is never poisoned")
        .clone()
    {
        return painter;
    }
    init_or_replace(14.0, None);
    PAINTER
        .read()
        .expect("single-threaded event loop, this lock is never poisoned")
        .clone()
        .expect("init_or_replace just populated the slot")
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
    pub fn new(font_size: f32, family: Option<&str>) -> Self {
        let shared = system_font_database();
        Self::from_db((*shared).clone(), font_size, family)
    }

    #[cfg(test)]
    #[must_use]
    fn with_embedded_font_only(font_size: f32) -> Self {
        let mut db = fontdb::Database::new();
        db.load_font_data(EMBEDDED_FONT_BYTES.to_vec());
        Self::from_db(db, font_size, None)
    }

    fn from_db(db: fontdb::Database, font_size: f32, family: Option<&str>) -> Self {
        let locale = sys_locale::get_locale().unwrap_or_else(|| "en-US".to_string());
        let font_system = FontSystem::new_with_locale_and_db(locale, db);
        Self {
            font_system: Mutex::new(font_system),
            swash_cache: Mutex::new(SwashCache::new()),
            width_cache: Mutex::new(HashMap::new()),
            font_size,
            family: family
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(DEFAULT_FAMILY)
                .to_string(),
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
            TextAlign::Right => px_max - total_w,
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
            let rx = (w * (1.6 / 7.6)).max(1.0);
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

#[allow(clippy::too_many_arguments)]
pub fn draw_coffee_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    centre_x: f32,
    centre_y: f32,
    glyph_side: f32,
    stroke_width: f32,
    scale: i32,
    fg: Color,
    steam: bool,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if glyph_side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    let side = glyph_side * scale_f;
    let (cx, cy) = (centre_x * scale_f, centre_y * scale_f);

    let body_w = side * 0.52;
    let body_h = side * 0.44;
    let left = cx - body_w / 2.0;
    let right = cx + body_w / 2.0;
    let top = cy - body_h * 0.30;
    let bottom = top + body_h;

    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(left, top);
    pb.line_to(left + body_w * 0.10, bottom);
    pb.line_to(right - body_w * 0.10, bottom);
    pb.line_to(right, top);
    pb.close();

    let handle_r = body_h * 0.30;
    let handle_cy = top + body_h * 0.34;
    pb.move_to(right - body_w * 0.02, handle_cy - handle_r);
    pb.cubic_to(
        right + handle_r * 1.6,
        handle_cy - handle_r,
        right + handle_r * 1.6,
        handle_cy + handle_r,
        right - body_w * 0.08,
        handle_cy + handle_r,
    );

    if steam {
        for offset in [-body_w * 0.22, 0.0, body_w * 0.22] {
            let x = cx + offset;
            let y0 = top - side * 0.10;
            pb.move_to(x, y0);
            pb.line_to(x, y0 - side * 0.16);
        }
    }

    if let Some(path) = pb.finish() {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: stroke_width * scale_f,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_search_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    centre_x: f32,
    centre_y: f32,
    glyph_side: f32,
    stroke_width: f32,
    scale: i32,
    fg: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if glyph_side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    let side = glyph_side * scale_f;
    let (cx, cy) = (centre_x * scale_f, centre_y * scale_f);

    let radius = side * 0.32;
    let lens = (cx - side * 0.5 + radius, cy - side * 0.5 + radius);
    let handle_from = (
        lens.0 + radius * std::f32::consts::FRAC_1_SQRT_2,
        lens.1 + radius * std::f32::consts::FRAC_1_SQRT_2,
    );
    let handle_to = (cx + side * 0.5, cy + side * 0.5);

    let mut pb = tiny_skia::PathBuilder::new();
    pb.push_circle(lens.0, lens.1, radius);
    pb.move_to(handle_from.0, handle_from.1);
    pb.line_to(handle_to.0, handle_to.1);

    if let Some(path) = pb.finish() {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: stroke_width * scale_f,
            line_cap: tiny_skia::LineCap::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

pub fn draw_close_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    glyph_side: f32,
    stroke_width: f32,
    scale: i32,
    fg: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if glyph_side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let (cx, cy) = (
        canvas_width as f32 * scale_f / 2.0,
        canvas_height as f32 * scale_f / 2.0,
    );
    let half = glyph_side * scale_f / 2.0;

    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(cx - half, cy - half);
    pb.line_to(cx + half, cy + half);
    pb.move_to(cx + half, cy - half);
    pb.line_to(cx - half, cy + half);
    if let Some(path) = pb.finish() {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: stroke_width * scale_f,
            line_cap: tiny_skia::LineCap::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[must_use]
pub fn render_svg(data: &[u8], side: i32) -> Option<Vec<u8>> {
    let side_u32 = u32::try_from(side).ok()?;
    if side_u32 == 0 {
        return None;
    }

    let tree = resvg::usvg::Tree::from_data(data, &resvg::usvg::Options::default()).ok()?;
    let intrinsic = tree.size();
    if intrinsic.width() <= 0.0 || intrinsic.height() <= 0.0 {
        return None;
    }

    #[allow(clippy::cast_precision_loss)]
    let target = side as f32;
    let scale = (target / intrinsic.width()).min(target / intrinsic.height());
    let (dx, dy) = (
        (target - intrinsic.width() * scale) / 2.0,
        (target - intrinsic.height() * scale) / 2.0,
    );

    let mut pixmap = tiny_skia::Pixmap::new(side_u32, side_u32)?;
    let transform = tiny_skia::Transform::from_translate(dx, dy).pre_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    Some(skia_pixmap_into_buffer_bytes(pixmap))
}

#[must_use]
pub fn render_png(data: &[u8], side: i32) -> Option<Vec<u8>> {
    let side_u32 = u32::try_from(side).ok()?;
    if side_u32 == 0 {
        return None;
    }

    let source = tiny_skia::Pixmap::decode_png(data).ok()?;
    #[allow(clippy::cast_precision_loss)]
    let target = side as f32;
    #[allow(clippy::cast_precision_loss)]
    let scale = (target / source.width() as f32).min(target / source.height() as f32);
    #[allow(clippy::cast_precision_loss)]
    let (dx, dy) = (
        (target - source.width() as f32 * scale) / 2.0,
        (target - source.height() as f32 * scale) / 2.0,
    );

    let mut pixmap = tiny_skia::Pixmap::new(side_u32, side_u32)?;
    pixmap.draw_pixmap(
        0,
        0,
        source.as_ref(),
        &tiny_skia::PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..tiny_skia::PixmapPaint::default()
        },
        tiny_skia::Transform::from_translate(dx, dy).pre_scale(scale, scale),
        None,
    );

    Some(skia_pixmap_into_buffer_bytes(pixmap))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockGlyph {
    Terminal,
    Files,
    Settings,
    Music,
    Mail,
    Generic,
    Grid,
}

fn push_rounded_rect(
    pb: &mut tiny_skia::PathBuilder,
    x_pos: f32,
    y_pos: f32,
    width: f32,
    height: f32,
    radius: f32,
) {
    let r = radius.max(0.0).min(width.min(height) / 2.0);
    if r <= 0.0 {
        if let Some(rect) = tiny_skia::Rect::from_xywh(x_pos, y_pos, width, height) {
            pb.push_rect(rect);
        }
        return;
    }
    let k = r * KAPPA;
    pb.move_to(x_pos + r, y_pos);
    pb.line_to(x_pos + width - r, y_pos);
    pb.cubic_to(
        x_pos + width - r + k,
        y_pos,
        x_pos + width,
        y_pos + r - k,
        x_pos + width,
        y_pos + r,
    );
    pb.line_to(x_pos + width, y_pos + height - r);
    pb.cubic_to(
        x_pos + width,
        y_pos + height - r + k,
        x_pos + width - r + k,
        y_pos + height,
        x_pos + width - r,
        y_pos + height,
    );
    pb.line_to(x_pos + r, y_pos + height);
    pb.cubic_to(
        x_pos + r - k,
        y_pos + height,
        x_pos,
        y_pos + height - r + k,
        x_pos,
        y_pos + height - r,
    );
    pb.line_to(x_pos, y_pos + r);
    pb.cubic_to(x_pos, y_pos + r - k, x_pos + r - k, y_pos, x_pos + r, y_pos);
    pb.close();
}

#[allow(clippy::too_many_lines)]
pub fn draw_dock_glyph(
    buf: &mut [u8],
    width: i32,
    height: i32,
    scale: i32,
    glyph: DockGlyph,
    color: Color,
) {
    let (Ok(pixel_width), Ok(pixel_height)) =
        (u32::try_from(width * scale), u32::try_from(height * scale))
    else {
        return;
    };
    let Some(mut pixmap) = tiny_skia::Pixmap::new(pixel_width, pixel_height) else {
        return;
    };

    if glyph == DockGlyph::Grid {
        #[allow(clippy::cast_precision_loss)]
        let grid_unit = (width.min(height) as f32 * scale as f32) / 2.0 / 18.0;
        #[allow(clippy::cast_precision_loss)]
        let origin_x = (width as f32 * scale as f32 - 18.0 * grid_unit) / 2.0;
        #[allow(clippy::cast_precision_loss)]
        let origin_y = (height as f32 * scale as f32 - 18.0 * grid_unit) / 2.0;
        let mut pb = tiny_skia::PathBuilder::new();
        for &cy in &[1.0_f32, 7.0, 13.0] {
            for &cx in &[1.0_f32, 7.0, 13.0] {
                push_rounded_rect(
                    &mut pb,
                    origin_x + cx * grid_unit,
                    origin_y + cy * grid_unit,
                    4.0 * grid_unit,
                    4.0 * grid_unit,
                    1.2 * grid_unit,
                );
            }
        }
        if let Some(path) = pb.finish() {
            let paint = tiny_skia::Paint {
                shader: tiny_skia::Shader::SolidColor(to_skia_color(color)),
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
        let bytes = skia_pixmap_into_buffer_bytes(pixmap);
        let n = bytes.len().min(buf.len());
        crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
        return;
    }

    #[allow(clippy::cast_precision_loss)]
    let unit = (width.min(height) as f32 * scale as f32) / 2.0 / 20.0;
    #[allow(clippy::cast_precision_loss)]
    let origin_x = (width as f32 * scale as f32 - 20.0 * unit) / 2.0;
    #[allow(clippy::cast_precision_loss)]
    let origin_y = (height as f32 * scale as f32 - 20.0 * unit) / 2.0;
    let p = |x: f32, y: f32| (origin_x + x * unit, origin_y + y * unit);

    let mut pb = tiny_skia::PathBuilder::new();
    match glyph {
        DockGlyph::Terminal => {
            push_rounded_rect(
                &mut pb,
                origin_x + 2.5 * unit,
                origin_y + 3.5 * unit,
                15.0 * unit,
                13.0 * unit,
                2.5 * unit,
            );
            let (mx, my) = p(6.0, 8.0);
            pb.move_to(mx, my);
            let (lx, ly) = p(8.5, 10.0);
            pb.line_to(lx, ly);
            let (ex, ey) = p(6.0, 12.0);
            pb.line_to(ex, ey);
            let (ux, uy) = p(10.5, 12.5);
            pb.move_to(ux, uy);
            let (vx, vy) = p(14.5, 12.5);
            pb.line_to(vx, vy);
        }
        DockGlyph::Files => {
            let (sx, sy) = p(2.5, 5.0);
            pb.move_to(sx, sy);
            let (ax, ay) = p(7.5, 5.0);
            pb.line_to(ax, ay);
            let (bx, by) = p(9.0, 7.0);
            pb.line_to(bx, by);
            let (cx, cy) = p(17.5, 7.0);
            pb.line_to(cx, cy);
            let (dx, dy) = p(17.5, 15.5);
            pb.line_to(dx, dy);
            let (ex, ey) = p(2.5, 15.5);
            pb.line_to(ex, ey);
            pb.close();
        }
        DockGlyph::Settings => {
            let (cx, cy) = p(10.0, 10.0);
            pb.push_circle(cx, cy, 2.8 * unit);
            for (x1, y1, x2, y2) in [
                (10.0, 2.0, 10.0, 4.2),
                (10.0, 15.8, 10.0, 18.0),
                (18.0, 10.0, 15.8, 10.0),
                (4.2, 10.0, 2.0, 10.0),
                (15.7, 4.3, 14.1, 5.9),
                (5.9, 14.1, 4.3, 15.7),
                (15.7, 15.7, 14.1, 14.1),
                (5.9, 5.9, 4.3, 4.3),
            ] {
                let (ax, ay) = p(x1, y1);
                let (bx, by) = p(x2, y2);
                pb.move_to(ax, ay);
                pb.line_to(bx, by);
            }
        }
        DockGlyph::Music => {
            let (sx, sy) = p(7.5, 14.0);
            pb.move_to(sx, sy);
            let (ax, ay) = p(7.5, 5.0);
            pb.line_to(ax, ay);
            let (bx, by) = p(14.5, 3.5);
            pb.line_to(bx, by);
            let (cx, cy) = p(14.5, 13.0);
            pb.line_to(cx, cy);
            let (d1x, d1y) = p(5.8, 14.2);
            pb.push_circle(d1x, d1y, 1.8 * unit);
            let (d2x, d2y) = p(12.8, 12.7);
            pb.push_circle(d2x, d2y, 1.8 * unit);
        }
        DockGlyph::Mail => {
            push_rounded_rect(
                &mut pb,
                origin_x + 2.5 * unit,
                origin_y + 4.5 * unit,
                15.0 * unit,
                11.0 * unit,
                2.5 * unit,
            );
            let (ax, ay) = p(2.5, 6.5);
            pb.move_to(ax, ay);
            let (bx, by) = p(10.0, 11.0);
            pb.line_to(bx, by);
            let (cx, cy) = p(17.5, 6.5);
            pb.line_to(cx, cy);
        }
        DockGlyph::Generic => {
            push_rounded_rect(
                &mut pb,
                origin_x + 3.5 * unit,
                origin_y + 3.5 * unit,
                13.0 * unit,
                13.0 * unit,
                2.5 * unit,
            );
        }
        DockGlyph::Grid => {}
    }

    if let Some(path) = pb.finish() {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(color)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: 1.5 * unit,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[allow(clippy::too_many_arguments)]
pub fn fill_rounded_rect_gradient_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    rect_x: i32,
    rect_y: i32,
    rect_width: i32,
    rect_height: i32,
    scale: i32,
    from: Color,
    to: Color,
    radius: f32,
) {
    let (Ok(pixel_width), Ok(pixel_height)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    let Some(mut pixmap) = tiny_skia::Pixmap::new(pixel_width, pixel_height) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let (x, y, rw, rh) = (
        rect_x as f32 * scale_f,
        rect_y as f32 * scale_f,
        rect_width as f32 * scale_f,
        rect_height as f32 * scale_f,
    );

    let Some(path) = rounded_rect(x, y, rw, rh, radius * scale_f) else {
        return;
    };

    let angle = 150.0_f32.to_radians();
    let (dx, dy) = (angle.sin(), -angle.cos());
    let half = (rw.hypot(rh)) / 2.0;
    let (cx, cy) = (x + rw / 2.0, y + rh / 2.0);
    let start = tiny_skia::Point::from_xy(cx - dx * half, cy - dy * half);
    let end = tiny_skia::Point::from_xy(cx + dx * half, cy + dy * half);

    let Some(shader) = tiny_skia::LinearGradient::new(
        start,
        end,
        vec![
            tiny_skia::GradientStop::new(0.0, to_skia_color(from)),
            tiny_skia::GradientStop::new(1.0, to_skia_color(to)),
        ],
        tiny_skia::SpreadMode::Pad,
        tiny_skia::Transform::identity(),
    ) else {
        return;
    };

    let paint = tiny_skia::Paint {
        shader,
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

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaGlyph {
    Play,
    Pause,
    Previous,
    Next,
}

#[allow(clippy::too_many_arguments)]
pub fn draw_media_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    box_x: i32,
    box_y: i32,
    box_side: i32,
    scale: i32,
    fg: Color,
    glyph: MediaGlyph,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let side = box_side as f32 * scale_f;
    #[allow(clippy::cast_precision_loss)]
    let ox = box_x as f32 * scale_f;
    #[allow(clippy::cast_precision_loss)]
    let oy = box_y as f32 * scale_f;

    let glyph_side = side * 0.56;
    let left = ox + (side - glyph_side) / 2.0;
    let top = oy + (side - glyph_side) / 2.0;
    let right = left + glyph_side;
    let bottom = top + glyph_side;
    let mid_y = f32::midpoint(top, bottom);

    let mut pb = tiny_skia::PathBuilder::new();
    match glyph {
        MediaGlyph::Play => {
            pb.move_to(left + glyph_side * 0.12, top);
            pb.line_to(right, mid_y);
            pb.line_to(left + glyph_side * 0.12, bottom);
            pb.close();
        }
        MediaGlyph::Pause => {
            let bar = glyph_side * 0.30;
            pb.push_rect(
                tiny_skia::Rect::from_ltrb(left, top, left + bar, bottom).unwrap_or(
                    tiny_skia::Rect::from_xywh(left, top, bar.max(0.01), glyph_side.max(0.01))
                        .unwrap_or(tiny_skia::Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap()),
                ),
            );
            if let Some(rect) = tiny_skia::Rect::from_ltrb(right - bar, top, right, bottom) {
                pb.push_rect(rect);
            }
        }
        MediaGlyph::Previous => {
            let bar = glyph_side * 0.20;
            if let Some(rect) = tiny_skia::Rect::from_ltrb(left, top, left + bar, bottom) {
                pb.push_rect(rect);
            }
            pb.move_to(right, top);
            pb.line_to(left + bar + glyph_side * 0.06, mid_y);
            pb.line_to(right, bottom);
            pb.close();
        }
        MediaGlyph::Next => {
            let bar = glyph_side * 0.20;
            if let Some(rect) = tiny_skia::Rect::from_ltrb(right - bar, top, right, bottom) {
                pb.push_rect(rect);
            }
            pb.move_to(left, top);
            pb.line_to(right - bar - glyph_side * 0.06, mid_y);
            pb.line_to(left, bottom);
            pb.close();
        }
    }

    let Some(path) = pb.finish() else {
        return;
    };
    let paint = tiny_skia::Paint {
        shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
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

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[must_use]
pub fn decode_cover(data: &[u8], side: i32) -> Option<Vec<u8>> {
    if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        return render_png(data, side);
    }
    if data.starts_with(&[0xff, 0xd8]) {
        return render_jpeg(data, side);
    }
    render_png(data, side).or_else(|| render_jpeg(data, side))
}

#[must_use]
pub fn render_jpeg(data: &[u8], side: i32) -> Option<Vec<u8>> {
    let mut decoder = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(data));
    decoder.decode_headers().ok()?;
    let (width, height) = decoder.dimensions()?;
    let pixels = decoder.decode().ok()?;

    let (Ok(src_w), Ok(src_h)) = (u32::try_from(width), u32::try_from(height)) else {
        return None;
    };
    if src_w == 0 || src_h == 0 {
        return None;
    }
    let channels = pixels.len() / (src_w as usize * src_h as usize);
    if channels != 3 && channels != 1 {
        return None;
    }

    let mut source = tiny_skia::Pixmap::new(src_w, src_h)?;
    for (index, chunk) in source.pixels_mut().iter_mut().enumerate() {
        let offset = index * channels;
        if offset + channels > pixels.len() {
            break;
        }
        let (r, g, b) = if channels == 1 {
            (pixels[offset], pixels[offset], pixels[offset])
        } else {
            (pixels[offset], pixels[offset + 1], pixels[offset + 2])
        };
        *chunk = tiny_skia::PremultipliedColorU8::from_rgba(b, g, r, 255)?;
    }

    let side_u32 = u32::try_from(side).ok()?;
    if side_u32 == 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let target = side as f32;
    #[allow(clippy::cast_precision_loss)]
    let scale = (target / src_w as f32).min(target / src_h as f32);
    #[allow(clippy::cast_precision_loss)]
    let (dx, dy) = (
        (target - src_w as f32 * scale) / 2.0,
        (target - src_h as f32 * scale) / 2.0,
    );

    let mut pixmap = tiny_skia::Pixmap::new(side_u32, side_u32)?;
    pixmap.draw_pixmap(
        0,
        0,
        source.as_ref(),
        &tiny_skia::PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..tiny_skia::PixmapPaint::default()
        },
        tiny_skia::Transform::from_translate(dx, dy).pre_scale(scale, scale),
        None,
    );
    Some(pixmap.take())
}

#[must_use]
pub fn dominant_colour(buffer: &[u8]) -> Option<Color> {
    let mut best: Option<(f32, [f32; 3])> = None;
    let mut fallback: Option<[f32; 3]> = None;

    for pixel in buffer.chunks_exact(4) {
        if pixel[3] < 128 {
            continue;
        }
        let b = f32::from(pixel[0]) / 255.0;
        let g = f32::from(pixel[1]) / 255.0;
        let r = f32::from(pixel[2]) / 255.0;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let chroma = max - min;
        if fallback.is_none() {
            fallback = Some([r, g, b]);
        }
        if max <= 0.0 {
            continue;
        }
        let saturation = chroma / max;
        let score = saturation * (1.0 - (max - 0.62).abs());
        if best.is_none_or(|(current, _)| score > current) {
            best = Some((score, [r, g, b]));
        }
    }

    let rgb = best.map(|(_, rgb)| rgb).or(fallback)?;
    Some(Color([rgb[0], rgb[1], rgb[2], 1.0]))
}

#[must_use]
pub fn fit_for_contrast(colour: Color, min_luma: f32, max_luma: f32) -> Color {
    let [r, g, b, a] = colour.0;
    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if luma >= min_luma && luma <= max_luma {
        return colour;
    }
    let target = luma.clamp(min_luma, max_luma);
    if luma <= f32::EPSILON {
        return Color([target, target, target, a]);
    }
    let gain = target / luma;
    Color([
        (r * gain).clamp(0.0, 1.0),
        (g * gain).clamp(0.0, 1.0),
        (b * gain).clamp(0.0, 1.0),
        a,
    ])
}

pub fn draw_moon_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    glyph_side: f32,
    scale: i32,
    fg: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if glyph_side <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let cx = (canvas_width as f32 / 2.0) * scale_f;
    #[allow(clippy::cast_precision_loss)]
    let cy = (canvas_height as f32 / 2.0) * scale_f;
    let radius = glyph_side * scale_f / 2.0;

    let mut pb1 = tiny_skia::PathBuilder::new();
    pb1.push_circle(cx, cy, radius);
    let Some(path1) = pb1.finish() else {
        return;
    };

    let mut pb2 = tiny_skia::PathBuilder::new();
    pb2.push_circle(cx + radius * 0.52, cy - radius * 0.40, radius * 0.86);
    let Some(path2) = pb2.finish() else {
        return;
    };

    let paint = tiny_skia::Paint {
        shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
        anti_alias: true,
        ..tiny_skia::Paint::default()
    };
    pixmap.fill_path(
        &path1,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );

    let clear_paint = tiny_skia::Paint {
        blend_mode: tiny_skia::BlendMode::Clear,
        anti_alias: true,
        ..tiny_skia::Paint::default()
    };
    pixmap.fill_path(
        &path2,
        &clear_paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_tick_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    glyph_side: f32,
    stroke_width: f32,
    scale: i32,
    fg: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if glyph_side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let cx = (canvas_width as f32 / 2.0) * scale_f;
    #[allow(clippy::cast_precision_loss)]
    let cy = (canvas_height as f32 / 2.0) * scale_f;
    let unit = (glyph_side * scale_f) / 14.0;
    let ox = cx - 7.0 * unit;
    let oy = cy - 7.0 * unit;

    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(ox + 2.6 * unit, oy + 7.4 * unit);
    pb.line_to(ox + 5.4 * unit, oy + 10.2 * unit);
    pb.line_to(ox + 11.4 * unit, oy + 3.8 * unit);

    if let Some(path) = pb.finish() {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: stroke_width * scale_f,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_chevron_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    glyph_side: f32,
    stroke_width: f32,
    point_up: bool,
    scale: i32,
    fg: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if glyph_side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let cx = (canvas_width as f32 / 2.0) * scale_f;
    #[allow(clippy::cast_precision_loss)]
    let cy = (canvas_height as f32 / 2.0) * scale_f;
    let unit = (glyph_side * scale_f) / 10.0;
    let ox = cx - 5.0 * unit;
    let oy = cy - 5.0 * unit;

    let mut pb = tiny_skia::PathBuilder::new();
    if point_up {
        pb.move_to(ox + 2.6 * unit, oy + 6.0 * unit);
        pb.line_to(ox + 5.0 * unit, oy + 3.6 * unit);
        pb.line_to(ox + 7.4 * unit, oy + 6.0 * unit);
    } else {
        pb.move_to(ox + 2.6 * unit, oy + 4.0 * unit);
        pb.line_to(ox + 5.0 * unit, oy + 6.4 * unit);
        pb.line_to(ox + 7.4 * unit, oy + 4.0 * unit);
    }

    if let Some(path) = pb.finish() {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: stroke_width * scale_f,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_wifi_bars_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    level: u8,
    scale: i32,
    fg: Color,
    muted_fg: Color,
) {
    let bar_w = 2.5_f32;
    let gap = 1.5_f32;
    let heights = [3.0_f32, 5.5_f32, 8.0_f32, 11.0_f32];
    let total_w = 4.0 * bar_w + 3.0 * gap;
    #[allow(clippy::cast_precision_loss)]
    let start_x = (canvas_width as f32 - total_w) / 2.0;
    #[allow(clippy::cast_precision_loss)]
    let bottom_y = f32::midpoint(canvas_height as f32, 11.0);

    for (i, &h) in heights.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x = start_x + (i as f32) * (bar_w + gap);
        let y = bottom_y - h;
        #[allow(clippy::cast_possible_truncation)]
        let color = if (i as u8) < level { fg } else { muted_fg };
        Painter::fill_rounded_rect_over(
            buf,
            canvas_width,
            canvas_height,
            x.round() as i32,
            y.round() as i32,
            bar_w.round() as i32,
            h.round() as i32,
            scale,
            color,
            1.0,
        );
    }
}

pub struct BatteryRenderArgs {
    pub canvas_width: i32,
    pub canvas_height: i32,
    pub pct: u8,
    pub charging: bool,
    pub scale: i32,
    pub fg: Color,
    pub ok_color: Color,
    pub danger_color: Color,
}

pub fn draw_battery_glyph_over(buf: &mut [u8], args: &BatteryRenderArgs) {
    let batt_w = 22;
    let batt_h = 11;
    let x = (args.canvas_width - batt_w - 3) / 2;
    let y = (args.canvas_height - batt_h) / 2;

    stroke_rounded_rect_over(
        buf,
        args.canvas_width,
        args.canvas_height,
        x,
        y,
        batt_w,
        batt_h,
        args.scale,
        1.3,
        3.5,
        args.fg,
    );

    let tip_w = 2;
    let tip_h = 5;
    let tip_x = x + batt_w;
    let tip_y = y + (batt_h - tip_h) / 2;
    Painter::fill_rounded_rect_over(
        buf,
        args.canvas_width,
        args.canvas_height,
        tip_x,
        tip_y,
        tip_w,
        tip_h,
        args.scale,
        args.fg,
        1.5,
    );

    let max_fill_w = 16_f32;
    let pct_clamped = args.pct.min(100);
    #[allow(clippy::cast_precision_loss)]
    let fill_w = ((max_fill_w * f32::from(pct_clamped)) / 100.0).max(2.0);
    let fill_color = if args.charging {
        args.ok_color
    } else if pct_clamped <= 20 {
        args.danger_color
    } else {
        args.fg
    };

    #[allow(clippy::cast_possible_truncation)]
    Painter::fill_rounded_rect_over(
        buf,
        args.canvas_width,
        args.canvas_height,
        x + 2,
        y + 2,
        fill_w.round() as i32,
        batt_h - 4,
        args.scale,
        fill_color,
        1.5,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn draw_speaker_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    side: f32,
    stroke_width: f32,
    muted: bool,
    scale: i32,
    fg: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let cx = (canvas_width as f32 / 2.0) * scale_f;
    #[allow(clippy::cast_precision_loss)]
    let cy = (canvas_height as f32 / 2.0) * scale_f;
    let unit = (side * scale_f) / 18.0;
    let ox = cx - 9.0 * unit;
    let oy = cy - 9.0 * unit;

    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(ox + 3.0 * unit, oy + 6.6 * unit);
    pb.line_to(ox + 5.6 * unit, oy + 6.6 * unit);
    pb.line_to(ox + 9.0 * unit, oy + 3.6 * unit);
    pb.line_to(ox + 9.0 * unit, oy + 14.4 * unit);
    pb.line_to(ox + 5.6 * unit, oy + 11.4 * unit);
    pb.line_to(ox + 3.0 * unit, oy + 11.4 * unit);
    pb.close();

    if muted {
        pb.move_to(ox + 12.0 * unit, oy + 6.6 * unit);
        pb.line_to(ox + 16.0 * unit, oy + 11.4 * unit);
        pb.move_to(ox + 16.0 * unit, oy + 6.6 * unit);
        pb.line_to(ox + 12.0 * unit, oy + 11.4 * unit);
    } else {
        pb.move_to(ox + 11.6 * unit, oy + 6.8 * unit);
        pb.quad_to(
            ox + 13.6 * unit,
            oy + 9.0 * unit,
            ox + 11.6 * unit,
            oy + 11.2 * unit,
        );
    }

    if let Some(path) = pb.finish() {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: stroke_width * scale_f,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_wifi_waves_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    side: f32,
    stroke_width: f32,
    scale: i32,
    fg: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let cx = (canvas_width as f32 / 2.0) * scale_f;
    #[allow(clippy::cast_precision_loss)]
    let cy = (canvas_height as f32 / 2.0) * scale_f;
    let unit = (side * scale_f) / 18.0;
    let ox = cx - 9.0 * unit;
    let oy = cy - 9.0 * unit;

    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(ox + 2.0 * unit, oy + 6.4 * unit);
    pb.quad_to(
        ox + 9.0 * unit,
        oy + 1.8 * unit,
        ox + 16.0 * unit,
        oy + 6.4 * unit,
    );

    pb.move_to(ox + 4.6 * unit, oy + 9.2 * unit);
    pb.quad_to(
        ox + 9.0 * unit,
        oy + 5.8 * unit,
        ox + 13.4 * unit,
        oy + 9.2 * unit,
    );

    pb.move_to(ox + 7.0 * unit, oy + 12.0 * unit);
    pb.quad_to(
        ox + 9.0 * unit,
        oy + 9.8 * unit,
        ox + 11.0 * unit,
        oy + 12.0 * unit,
    );

    let mut pb_dot = tiny_skia::PathBuilder::new();
    pb_dot.push_circle(ox + 9.0 * unit, oy + 14.6 * unit, 1.2 * unit);

    let paint = tiny_skia::Paint {
        shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
        anti_alias: true,
        ..tiny_skia::Paint::default()
    };
    let stroke = tiny_skia::Stroke {
        width: stroke_width * scale_f,
        line_cap: tiny_skia::LineCap::Round,
        ..tiny_skia::Stroke::default()
    };
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }
    if let Some(dot) = pb_dot.finish() {
        pixmap.fill_path(
            &dot,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_bluetooth_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    side: f32,
    stroke_width: f32,
    scale: i32,
    fg: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let cx = (canvas_width as f32 / 2.0) * scale_f;
    #[allow(clippy::cast_precision_loss)]
    let cy = (canvas_height as f32 / 2.0) * scale_f;
    let unit = (side * scale_f) / 18.0;
    let ox = cx - 9.0 * unit;
    let oy = cy - 9.0 * unit;

    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(ox + 6.0 * unit, oy + 5.4 * unit);
    pb.line_to(ox + 12.0 * unit, oy + 12.0 * unit);
    pb.line_to(ox + 9.0 * unit, oy + 14.4 * unit);
    pb.line_to(ox + 9.0 * unit, oy + 3.6 * unit);
    pb.line_to(ox + 12.0 * unit, oy + 6.0 * unit);
    pb.line_to(ox + 6.0 * unit, oy + 12.6 * unit);

    if let Some(path) = pb.finish() {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: stroke_width * scale_f,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_camera_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    side: f32,
    stroke_width: f32,
    scale: i32,
    fg: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let cx = (canvas_width as f32 / 2.0) * scale_f;
    #[allow(clippy::cast_precision_loss)]
    let cy = (canvas_height as f32 / 2.0) * scale_f;
    let unit = (side * scale_f) / 18.0;
    let ox = cx - 9.0 * unit;
    let oy = cy - 9.0 * unit;

    let mut pb = tiny_skia::PathBuilder::new();
    if let Some(r) =
        tiny_skia::Rect::from_xywh(ox + 2.5 * unit, oy + 5.0 * unit, 13.0 * unit, 9.0 * unit)
    {
        pb.push_rect(r);
    }
    pb.move_to(ox + 6.5 * unit, oy + 5.0 * unit);
    pb.line_to(ox + 7.5 * unit, oy + 3.2 * unit);
    pb.line_to(ox + 10.5 * unit, oy + 3.2 * unit);
    pb.line_to(ox + 11.5 * unit, oy + 5.0 * unit);
    pb.push_circle(ox + 9.0 * unit, oy + 9.5 * unit, 2.5 * unit);

    if let Some(path) = pb.finish() {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: stroke_width * scale_f,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_sun_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    side: f32,
    stroke_width: f32,
    scale: i32,
    fg: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let cx = (canvas_width as f32 / 2.0) * scale_f;
    #[allow(clippy::cast_precision_loss)]
    let cy = (canvas_height as f32 / 2.0) * scale_f;
    let unit = (side * scale_f) / 18.0;
    let ox = cx - 9.0 * unit;
    let oy = cy - 9.0 * unit;

    let mut pb = tiny_skia::PathBuilder::new();
    pb.push_circle(ox + 9.0 * unit, oy + 9.0 * unit, 3.2 * unit);
    pb.move_to(ox + 9.0 * unit, oy + 1.8 * unit);
    pb.line_to(ox + 9.0 * unit, oy + 3.6 * unit);
    pb.move_to(ox + 9.0 * unit, oy + 14.4 * unit);
    pb.line_to(ox + 9.0 * unit, oy + 16.2 * unit);
    pb.move_to(ox + 1.8 * unit, oy + 9.0 * unit);
    pb.line_to(ox + 3.6 * unit, oy + 9.0 * unit);
    pb.move_to(ox + 14.4 * unit, oy + 9.0 * unit);
    pb.line_to(ox + 16.2 * unit, oy + 9.0 * unit);
    pb.move_to(ox + 3.9 * unit, oy + 3.9 * unit);
    pb.line_to(ox + 5.2 * unit, oy + 5.2 * unit);
    pb.move_to(ox + 12.8 * unit, oy + 12.8 * unit);
    pb.line_to(ox + 14.1 * unit, oy + 14.1 * unit);
    pb.move_to(ox + 14.1 * unit, oy + 3.9 * unit);
    pb.line_to(ox + 12.8 * unit, oy + 5.2 * unit);
    pb.move_to(ox + 5.2 * unit, oy + 12.8 * unit);
    pb.line_to(ox + 3.9 * unit, oy + 14.1 * unit);

    if let Some(path) = pb.finish() {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: stroke_width * scale_f,
            line_cap: tiny_skia::LineCap::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_keyboard_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    side: f32,
    stroke_width: f32,
    scale: i32,
    fg: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let cx = (canvas_width as f32 / 2.0) * scale_f;
    #[allow(clippy::cast_precision_loss)]
    let cy = (canvas_height as f32 / 2.0) * scale_f;
    let unit = (side * scale_f) / 18.0;
    let ox = cx - 9.0 * unit;
    let oy = cy - 9.0 * unit;

    let mut pb = tiny_skia::PathBuilder::new();
    if let Some(r) =
        tiny_skia::Rect::from_xywh(ox + 2.0 * unit, oy + 4.2 * unit, 14.0 * unit, 9.6 * unit)
    {
        pb.push_rect(r);
    }
    pb.move_to(ox + 5.0 * unit, oy + 7.2 * unit);
    pb.line_to(ox + 5.5 * unit, oy + 7.2 * unit);
    pb.move_to(ox + 8.5 * unit, oy + 7.2 * unit);
    pb.line_to(ox + 9.5 * unit, oy + 7.2 * unit);
    pb.move_to(ox + 12.5 * unit, oy + 7.2 * unit);
    pb.line_to(ox + 13.0 * unit, oy + 7.2 * unit);
    pb.move_to(ox + 5.5 * unit, oy + 10.4 * unit);
    pb.line_to(ox + 12.5 * unit, oy + 10.4 * unit);

    if let Some(path) = pb.finish() {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: stroke_width * scale_f,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_gear_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    side: f32,
    stroke_width: f32,
    scale: i32,
    fg: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let cx = (canvas_width as f32 / 2.0) * scale_f;
    #[allow(clippy::cast_precision_loss)]
    let cy = (canvas_height as f32 / 2.0) * scale_f;
    let unit = (side * scale_f) / 18.0;

    let mut pb = tiny_skia::PathBuilder::new();
    pb.push_circle(cx, cy, 2.6 * unit);
    for i in 0..6 {
        #[allow(clippy::cast_precision_loss)]
        let a = (i as f32) * (std::f32::consts::PI / 3.0);
        let cos = a.cos();
        let sin = a.sin();
        pb.move_to(cx + cos * 4.4 * unit, cy + sin * 4.4 * unit);
        pb.line_to(cx + cos * 6.6 * unit, cy + sin * 6.6 * unit);
    }

    if let Some(path) = pb.finish() {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: stroke_width * scale_f,
            line_cap: tiny_skia::LineCap::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_lock_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    side: f32,
    stroke_width: f32,
    scale: i32,
    fg: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let cx = (canvas_width as f32 / 2.0) * scale_f;
    #[allow(clippy::cast_precision_loss)]
    let cy = (canvas_height as f32 / 2.0) * scale_f;
    let unit = (side * scale_f) / 18.0;
    let ox = cx - 9.0 * unit;
    let oy = cy - 9.0 * unit;

    let mut pb = tiny_skia::PathBuilder::new();
    if let Some(r) =
        tiny_skia::Rect::from_xywh(ox + 3.4 * unit, oy + 7.8 * unit, 11.2 * unit, 7.4 * unit)
    {
        pb.push_rect(r);
    }
    pb.move_to(ox + 5.9 * unit, oy + 7.8 * unit);
    pb.line_to(ox + 5.9 * unit, oy + 5.6 * unit);
    pb.quad_to(
        ox + 9.0 * unit,
        oy + 2.5 * unit,
        ox + 12.1 * unit,
        oy + 5.6 * unit,
    );
    pb.line_to(ox + 12.1 * unit, oy + 7.8 * unit);

    if let Some(path) = pb.finish() {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: stroke_width * scale_f,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_power_glyph_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    side: f32,
    stroke_width: f32,
    scale: i32,
    fg: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    #[allow(clippy::cast_precision_loss)]
    let cx = (canvas_width as f32 / 2.0) * scale_f;
    #[allow(clippy::cast_precision_loss)]
    let cy = (canvas_height as f32 / 2.0) * scale_f;
    let unit = (side * scale_f) / 18.0;
    let ox = cx - 9.0 * unit;
    let oy = cy - 9.0 * unit;

    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(ox + 9.0 * unit, oy + 2.6 * unit);
    pb.line_to(ox + 9.0 * unit, oy + 8.8 * unit);

    pb.move_to(ox + 5.0 * unit, oy + 5.0 * unit);
    pb.quad_to(
        ox + 1.0 * unit,
        oy + 10.0 * unit,
        ox + 5.5 * unit,
        oy + 14.5 * unit,
    );
    pb.quad_to(
        ox + 9.0 * unit,
        oy + 16.5 * unit,
        ox + 12.5 * unit,
        oy + 14.5 * unit,
    );
    pb.quad_to(
        ox + 17.0 * unit,
        oy + 10.0 * unit,
        ox + 13.0 * unit,
        oy + 5.0 * unit,
    );

    if let Some(path) = pb.finish() {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(fg)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: stroke_width * scale_f,
            line_cap: tiny_skia::LineCap::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
}

pub fn blit_square_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    src: &[u8],
    src_side: i32,
    dst_x: i32,
    dst_y: i32,
    scale: i32,
) {
    let (Ok(cw), Ok(ch)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    let Ok(side) = u32::try_from(src_side * scale) else {
        return;
    };
    if side == 0 || src.len() < (side as usize) * (side as usize) * 4 {
        return;
    }
    let Some(source) = tiny_skia::PixmapRef::from_bytes(src, side, side) else {
        return;
    };
    let Some(mut pixmap) = tiny_skia::PixmapMut::from_bytes(buf, cw, ch) else {
        return;
    };
    pixmap.draw_pixmap(
        dst_x * scale,
        dst_y * scale,
        source,
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn blit_rect_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    src: &[u8],
    src_width: i32,
    src_height: i32,
    dst_x: i32,
    dst_y: i32,
    scale: i32,
) {
    let (Ok(cw), Ok(ch)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    let (Ok(sw), Ok(sh)) = (
        u32::try_from(src_width * scale),
        u32::try_from(src_height * scale),
    ) else {
        return;
    };
    if sw == 0 || sh == 0 || src.len() < (sw as usize) * (sh as usize) * 4 {
        return;
    }
    let Some(source) = tiny_skia::PixmapRef::from_bytes(src, sw, sh) else {
        return;
    };
    let Some(mut pixmap) = tiny_skia::PixmapMut::from_bytes(buf, cw, ch) else {
        return;
    };
    pixmap.draw_pixmap(
        dst_x * scale,
        dst_y * scale,
        source,
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn stroke_rounded_rect_over(
    buf: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    rect_x: i32,
    rect_y: i32,
    rect_width: i32,
    rect_height: i32,
    scale: i32,
    stroke_width: f32,
    radius: f32,
    color: Color,
) {
    let (Ok(w), Ok(h)) = (
        u32::try_from(canvas_width * scale),
        u32::try_from(canvas_height * scale),
    ) else {
        return;
    };
    if stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    let inset = stroke_width * scale_f / 2.0;
    #[allow(clippy::cast_precision_loss)]
    let path = rounded_rect(
        rect_x as f32 * scale_f + inset,
        rect_y as f32 * scale_f + inset,
        rect_width as f32 * scale_f - inset * 2.0,
        rect_height as f32 * scale_f - inset * 2.0,
        (radius * scale_f - inset).max(0.0),
    );
    if let Some(path) = path {
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(color)),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let stroke = tiny_skia::Stroke {
            width: stroke_width * scale_f,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    crate::border::composite_premul_over(&mut buf[..n], &bytes[..n]);
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
        init_or_replace(21.0, None);
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
mod cover_tests {
    use super::*;

    fn solid(r: u8, g: u8, b: u8) -> Vec<u8> {
        let mut buffer = Vec::new();
        for _ in 0..64 {
            buffer.extend_from_slice(&[b, g, r, 255]);
        }
        buffer
    }

    #[test]
    fn a_vivid_cover_yields_its_vivid_hue_rather_than_an_average() {
        let mut buffer = solid(40, 40, 42);
        buffer.extend_from_slice(&[40, 160, 230, 255]);
        let colour = dominant_colour(&buffer).expect("a colour is found");
        assert!(colour.0[0] > colour.0[2], "the orange channel must lead");
    }

    #[test]
    fn a_fully_transparent_cover_reports_nothing() {
        let buffer = vec![0u8; 64 * 4];
        assert!(dominant_colour(&buffer).is_none());
    }

    #[test]
    fn a_grey_cover_still_reports_a_colour_to_clamp_later() {
        let colour = dominant_colour(&solid(130, 130, 130)).expect("grey is still a colour");
        assert!((colour.0[0] - colour.0[1]).abs() < 0.01);
    }

    #[test]
    fn a_colour_already_in_range_is_left_untouched() {
        let colour = Color([0.6, 0.5, 0.3, 1.0]);
        assert_eq!(fit_for_contrast(colour, 0.35, 0.8), colour);
    }

    #[test]
    fn a_too_dark_colour_is_lifted_into_readable_range() {
        let lifted = fit_for_contrast(Color([0.05, 0.06, 0.2, 1.0]), 0.35, 0.8);
        let luma = 0.2126 * lifted.0[0] + 0.7152 * lifted.0[1] + 0.0722 * lifted.0[2];
        assert!(luma >= 0.34, "luma {luma} is still too dark");
    }

    #[test]
    fn a_blown_out_colour_is_pulled_back_down() {
        let pulled = fit_for_contrast(Color([0.98, 0.99, 0.97, 1.0]), 0.35, 0.8);
        let luma = 0.2126 * pulled.0[0] + 0.7152 * pulled.0[1] + 0.0722 * pulled.0[2];
        assert!(luma <= 0.81, "luma {luma} is still blown out");
    }

    #[test]
    fn pure_black_is_lifted_without_dividing_by_zero() {
        let lifted = fit_for_contrast(Color([0.0, 0.0, 0.0, 1.0]), 0.35, 0.8);
        assert!(lifted.0[0] > 0.3);
        assert!(lifted.0[0].is_finite());
    }

    #[test]
    fn a_cover_that_is_neither_png_nor_jpeg_is_refused() {
        assert!(decode_cover(b"not an image at all", 32).is_none());
    }

    #[test]
    fn an_empty_cover_is_refused() {
        assert!(decode_cover(&[], 32).is_none());
        assert!(render_jpeg(&[], 32).is_none());
    }

    #[test]
    fn a_truncated_jpeg_header_does_not_panic() {
        assert!(decode_cover(&[0xff, 0xd8, 0xff, 0xe0], 32).is_none());
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
                None,
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

    #[test]
    fn draw_dock_glyph_renders_all_variants_without_panicking() {
        let glyphs = [
            DockGlyph::Terminal,
            DockGlyph::Files,
            DockGlyph::Settings,
            DockGlyph::Music,
            DockGlyph::Mail,
            DockGlyph::Generic,
            DockGlyph::Grid,
        ];
        let color = Color([1.0, 1.0, 1.0, 1.0]);
        for glyph in glyphs {
            for scale in [1, 2] {
                let side = 42;
                let mut buf = vec![0u8; (side * scale * side * scale * 4) as usize];
                draw_dock_glyph(&mut buf, side, side, scale, glyph, color);
                assert!(buf.iter().any(|&b| b > 0));
            }
        }
    }
}

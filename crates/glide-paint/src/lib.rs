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
    Attrs, Buffer, Color as TextColor, Family, FeatureTag, FontFeatures, FontSystem, Metrics, Shaping, SwashCache,
    Weight, Wrap, fontdb,
};

use glide_theme::{Color, TextStyle};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

mod wallpaper;
pub use wallpaper::{
    compose_wallpaper, decode_wallpaper, desaturate_in_place, draw_wallpaper_into, embedded_wallpaper_bytes,
    fit_transform, init_wallpaper, load_wallpaper, saturate_in_place, wallpaper_generation,
};

mod blur;
pub use blur::box_blur_rgba;

mod border;
pub use border::{
    OutsetShadow, SHADOW_DY_RATIO, ShadowLayerSpec, blit_rounded_backdrop, css_blur_box_radius, draw_inset_border,
    draw_outset_shadow_and_border, shadow_fade_margin,
};

static EMBEDDED_FONT_BYTES: &[u8] = include_bytes!("fonts/LiberationSans-Regular.ttf");

pub use glide_theme::font::ui_family;

fn single_line(text: &str) -> std::borrow::Cow<'_, str> {
    if text.contains(|c: char| c.is_whitespace() && c != ' ') {
        std::borrow::Cow::Owned(text.split_whitespace().collect::<Vec<_>>().join(" "))
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

const NEVER_POISONED: &str = "single-threaded event loop, this lock is never poisoned";

static PAINTER: RwLock<Option<Arc<Painter>>> = RwLock::new(None);
static PAINTER_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SYSTEM_FONTS: RwLock<Option<Arc<fontdb::Database>>> = RwLock::new(None);

fn system_font_database() -> Arc<fontdb::Database> {
    if let Some(db) = SYSTEM_FONTS.read().expect(NEVER_POISONED).clone() {
        return db;
    }

    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    db.load_font_data(EMBEDDED_FONT_BYTES.to_vec());
    let db = Arc::new(db);

    let mut slot = SYSTEM_FONTS.write().expect(NEVER_POISONED);
    *slot = Some(Arc::clone(&db));
    db
}

pub fn init_or_replace(font_size: f32, family: Option<&str>) {
    let painter = Arc::new(Painter::new(font_size, family));
    let mut slot = PAINTER.write().expect(NEVER_POISONED);
    *slot = Some(painter);
    PAINTER_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[must_use]
pub fn painter_generation() -> u64 {
    PAINTER_GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

#[must_use]
pub fn raster_revision(theme_revision: u64) -> u64 {
    glide_theme::Revision::new().u64(theme_revision).u64(painter_generation()).finish()
}

#[must_use]
pub fn current() -> Arc<Painter> {
    if let Some(painter) = PAINTER.read().expect(NEVER_POISONED).clone() {
        return painter;
    }
    init_or_replace(14.0, None);
    PAINTER.read().expect(NEVER_POISONED).clone().expect("init_or_replace just populated the slot")
}

const DEFAULT_WEIGHT: u16 = Weight::NORMAL.0;
const DEFAULT_TRACKING_EM: f32 = 0.0;

const MAX_CACHED_TEXTS_PER_STYLE: usize = 512;

pub const TABULAR_FAMILY: &str = "JetBrains Mono";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FaceRole {
    Ui,
    Mono,
}

type StyleKey = (FaceRole, u32, u16, u32, bool);

#[must_use]
pub(crate) fn tabular_family<'a>(available: impl IntoIterator<Item = &'a str>) -> Option<&'static str> {
    available.into_iter().any(|family| family.trim().eq_ignore_ascii_case(TABULAR_FAMILY)).then_some(TABULAR_FAMILY)
}

fn style_key_with(role: FaceRole, font_size: f32, weight: u16, tracking_em: f32, tabular: bool) -> StyleKey {
    (role, font_size.to_bits(), weight, tracking_em.to_bits(), tabular)
}

pub struct Painter {
    font_system: Mutex<FontSystem>,
    swash_cache: Mutex<SwashCache>,
    width_cache: Mutex<HashMap<StyleKey, HashMap<String, f32>>>,
    font_size: f32,
    family: String,
    mono_family: Option<&'static str>,
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
        let mono_family =
            tabular_family(db.faces().flat_map(|face| face.families.iter().map(|(name, _)| name.as_str())));
        let family = ui_family(family, db.faces().flat_map(|face| face.families.iter().map(|(name, _)| name.as_str())));
        let font_system = FontSystem::new_with_locale_and_db(locale, db);
        Self {
            font_system: Mutex::new(font_system),
            swash_cache: Mutex::new(SwashCache::new()),
            width_cache: Mutex::new(HashMap::new()),
            font_size,
            family,
            mono_family,
        }
    }

    const fn face_role(&self, tabular: bool) -> FaceRole {
        if tabular && self.mono_family.is_some() { FaceRole::Mono } else { FaceRole::Ui }
    }

    fn family_for(&self, role: FaceRole) -> &str {
        match (role, self.mono_family) {
            (FaceRole::Mono, Some(mono)) => mono,
            _ => &self.family,
        }
    }

    fn shape(
        &self, font_system: &mut FontSystem, text: &str, font_size: f32, weight: u16, tracking_em: f32, tabular: bool,
    ) -> Buffer {
        let metrics = Metrics::new(font_size, font_size * 1.2);
        let mut buffer = Buffer::new(font_system, metrics);
        buffer.set_wrap(Wrap::None);
        let mut attrs =
            Attrs::new().family(Family::Name(self.family_for(self.face_role(tabular)))).weight(Weight(weight));
        if tracking_em != DEFAULT_TRACKING_EM {
            attrs = attrs.letter_spacing(tracking_em);
        }
        if tabular {
            let mut features = FontFeatures::new();
            features.enable(FeatureTag::new(b"tnum"));
            features.disable(FeatureTag::KERNING);
            attrs = attrs.font_features(features);
        }
        buffer.set_text(&single_line(text), &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(font_system, false);
        buffer
    }

    fn line_metrics(font_system: &mut FontSystem, buffer: &mut Buffer) -> LineMetrics {
        let (width, max_ascent, max_descent) = buffer
            .line_layout(font_system, 0)
            .and_then(|lines| lines.first())
            .map_or((0.0, 0.0, 0.0), |line| (line.w, line.max_ascent, line.max_descent));
        let baseline_y = buffer.layout_runs().next().map_or(0.0, |run| run.line_y);
        LineMetrics { width, baseline_y, max_ascent, max_descent }
    }

    fn measure_width_uncached(&self, text: &str, font_size: f32, weight: u16, tracking_em: f32, tabular: bool) -> f32 {
        let mut font_system = self.font_system.lock().expect(NEVER_POISONED);
        let mut buffer = self.shape(&mut font_system, text, font_size, weight, tracking_em, tabular);
        Self::line_metrics(&mut font_system, &mut buffer).width
    }

    fn measure_width_cached_with(
        &self, text: &str, font_size: f32, weight: u16, tracking_em: f32, tabular: bool,
    ) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let key = style_key_with(self.face_role(tabular), font_size, weight, tracking_em, tabular);

        if let Some(width) =
            self.width_cache.lock().expect(NEVER_POISONED).get(&key).and_then(|by_text| by_text.get(text))
        {
            return *width;
        }

        let width = self.measure_width_uncached(text, font_size, weight, tracking_em, tabular);

        let mut cache = self.width_cache.lock().expect(NEVER_POISONED);
        let by_text = cache.entry(key).or_default();
        if by_text.len() >= MAX_CACHED_TEXTS_PER_STYLE {
            by_text.clear();
        }
        by_text.insert(text.to_owned(), width);

        width
    }

    fn fit_text_at(
        &self, text: &str, avail_w: i32, font_size: f32, weight: u16, tracking_em: f32, tabular: bool,
    ) -> String {
        let text = &*single_line(text);
        let avail_w = avail_w as f32;
        if avail_w <= 0.0 {
            return String::new();
        }
        let measure =
            |candidate: &str| self.measure_width_cached_with(candidate, font_size, weight, tracking_em, tabular);
        if measure(text) <= avail_w {
            return text.to_owned();
        }

        let starts: Vec<usize> = text.grapheme_indices(true).map(|(start, _)| start).collect();
        let candidate_at = |k: usize| -> String { text[..starts[k]].to_owned() + "…" };
        let fits = |k: usize| -> bool { measure(&candidate_at(k)) <= avail_w };

        if starts.is_empty() || !fits(0) {
            return String::new();
        }

        let mut lo = 0usize;
        let mut hi = starts.len() - 1;
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
        self.measure_width_cached_with(text, self.font_size, DEFAULT_WEIGHT, DEFAULT_TRACKING_EM, false)
    }

    #[must_use]
    pub fn text_width_styled(&self, text: &str, style: &TextStyle) -> f32 {
        let text = Self::apply_case(text, style);
        self.measure_width_cached_with(&text, style.size_px, style.weight, style.tracking_em, style.tabular)
    }

    fn apply_case<'a>(text: &'a str, style: &TextStyle) -> std::borrow::Cow<'a, str> {
        if style.uppercase { std::borrow::Cow::Owned(text.to_uppercase()) } else { std::borrow::Cow::Borrowed(text) }
    }

    #[cfg(test)]
    fn draw_label_over(
        &self, buf: &mut [u8], width: i32, height: i32, x_min: i32, x_max: i32, scale: i32, align: TextAlign,
        text: &str, fg_argb: [u8; 4],
    ) {
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
            false,
        );
    }

    pub fn draw_label_over_styled(
        &self, buf: &mut [u8], width: i32, height: i32, x_min: i32, x_max: i32, scale: i32, align: TextAlign,
        text: &str, fg_argb: [u8; 4], style: &TextStyle,
    ) {
        self.draw_label_over_styled_in_band(
            buf, width, height, x_min, x_max, 0, height, scale, align, text, fg_argb, style,
        );
    }

    pub fn draw_label_over_styled_in_band(
        &self, buf: &mut [u8], width: i32, height: i32, x_min: i32, x_max: i32, y_min: i32, y_max: i32, scale: i32,
        align: TextAlign, text: &str, fg_argb: [u8; 4], style: &TextStyle,
    ) {
        let text = Self::apply_case(text, style);
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
            style.tabular,
        );
    }

    #[allow(clippy::similar_names)]
    fn draw_label_at(
        &self, buf: &mut [u8], width: i32, height: i32, x_min: i32, x_max: i32, y_min: i32, y_max: i32, scale: i32,
        align: TextAlign, text: &str, fg_argb: [u8; 4], font_size: f32, weight: u16, tracking_em: f32, tabular: bool,
    ) {
        let (pw, ph) = (width * scale, height * scale);
        if pw <= 0 || ph <= 0 || buf.len() < (pw * ph * 4) as usize {
            return;
        }
        let (px_min, px_max) = (x_min * scale, x_max * scale);
        let (py_min, py_max) = (y_min * scale, y_max * scale);

        let text = self.fit_text_at(text, px_max - px_min, font_size, weight, tracking_em, tabular);
        if text.is_empty() {
            return;
        }

        let mut font_system = self.font_system.lock().expect(NEVER_POISONED);
        let mut swash_cache = self.swash_cache.lock().expect(NEVER_POISONED);

        let mut buffer = self.shape(&mut font_system, &text, font_size, weight, tracking_em, tabular);
        let metrics = Self::line_metrics(&mut font_system, &mut buffer);
        if metrics.width <= 0.0 {
            return;
        }
        let total_w = metrics.width.ceil() as i32;

        let avail_w = px_max - px_min;
        let x_off = match align {
            TextAlign::Center => px_min + (avail_w - total_w) / 2,
            TextAlign::Left => px_min,
            TextAlign::Right => px_max - total_w,
        };
        let content_h = metrics.max_ascent + metrics.max_descent;
        let band_h = (py_max - py_min) as f32;
        let y_off = py_min + ((band_h - content_h) / 2.0 - metrics.baseline_y + metrics.max_ascent) as i32;

        let fg = TextColor::rgba(fg_argb[0], fg_argb[1], fg_argb[2], fg_argb[3]);
        let opacity = u32::from(fg_argb[3]);
        buffer.draw(&mut font_system, &mut swash_cache, fg, |x, y, gw, gh, color| {
            let (r, g, b, coverage) = color.as_rgba_tuple();
            let a = ((u32::from(coverage) * opacity + 127) / 255) as u8;
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
        });
    }

    pub fn draw_icon_maximize(buf: &mut [u8], width: i32, height: i32, scale: i32, fg: Color, bg: Color) {
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

    pub fn draw_icon_restore(buf: &mut [u8], width: i32, height: i32, scale: i32, fg: Color, bg: Color) {
        render_icon(buf, width, height, scale, bg, |pixmap| {
            let (x0, y0, x1, y1) = glyph_bounds(width, height);
            let offset = ((x1 - x0) * 0.28).max(1.0);
            let rx = ((x1 - offset - x0) * (1.6 / 7.6)).max(1.0);

            if let Some(path) = rounded_rect(x0 + offset, y0, x1 - offset - x0, y1 - offset - y0, rx) {
                stroke(pixmap, &path, fg, height, scale);
            }

            if let Some(path) = rounded_rect(x0, y0 + offset, x1 - offset - x0, y1 - offset - y0, rx) {
                fill(pixmap, &path, bg, scale);
                stroke(pixmap, &path, fg, height, scale);
            }
        });
    }

    pub fn draw_icon_close(buf: &mut [u8], width: i32, height: i32, scale: i32, fg: Color, bg: Color) {
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
        buf: &mut [u8], width: i32, height: i32, scale: i32, bg: Color, radius: f32, corners: [bool; 4],
    ) {
        let Some(mut pixmap) = cleared(buf, width, height, scale) else {
            return;
        };
        if let Some(path) = rounded_rect_corners(0.0, 0.0, width as f32, height as f32, radius, corners) {
            fill(&mut pixmap, &path, bg, scale);
        }
    }

    pub fn fill_rounded_rect_over(
        buf: &mut [u8], canvas_width: i32, canvas_height: i32, rect_x: i32, rect_y: i32, rect_width: i32,
        rect_height: i32, scale: i32, color: Color, radius: f32,
    ) {
        let Some(mut pixmap) = over(buf, canvas_width, canvas_height, scale) else {
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
        pixmap.fill_path(
            &path,
            &paint_for(Color([b, g, r, a])),
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

pub fn draw_coffee_glyph_over(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, centre_x: f32, centre_y: f32, glyph_side: f32,
    stroke_width: f32, scale: i32, fg: Color, steam: bool,
) {
    if glyph_side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = over(buf, canvas_width, canvas_height, scale) else {
        return;
    };
    let scale_f = scale as f32;
    let side = glyph_side * scale_f;
    let (cx, cy) = (centre_x * scale_f, centre_y * scale_f);

    let body_w = side * 0.50;
    let body_h = side * 0.42;
    let left = cx - body_w / 2.0;
    let right = cx + body_w / 2.0;
    let top = cy - body_h * 0.25;
    let bottom = top + body_h;
    let corner_r = body_w * 0.32;

    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(left, top);
    pb.line_to(left, bottom - corner_r);
    pb.quad_to(left, bottom, left + corner_r, bottom);
    pb.line_to(right - corner_r, bottom);
    pb.quad_to(right, bottom, right, bottom - corner_r);
    pb.line_to(right, top);
    pb.close();

    let handle_w = body_w * 0.30;
    let handle_top = top + body_h * 0.18;
    let handle_bottom = top + body_h * 0.72;
    pb.move_to(right, handle_top);
    pb.quad_to(right + handle_w, handle_top, right + handle_w, f32::midpoint(handle_top, handle_bottom));
    pb.quad_to(right + handle_w, handle_bottom, right, handle_bottom);

    if steam {
        let steam_h = side * 0.14;
        let y_bot = top - side * 0.08;
        let y_top = y_bot - steam_h;
        for offset in [-body_w * 0.24, 0.0, body_w * 0.24] {
            polyline(&mut pb, [(cx + offset, y_bot), (cx + offset, y_top)]);
        }
    }

    let stroke = round_stroke(stroke_width * scale_f, tiny_skia::LineJoin::Round);
    stroke_solid(&mut pixmap, pb.finish(), bgr(fg), &stroke);
}

pub fn draw_search_glyph_over(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, centre_x: f32, centre_y: f32, glyph_side: f32,
    stroke_width: f32, scale: i32, fg: Color,
) {
    if glyph_side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = over(buf, canvas_width, canvas_height, scale) else {
        return;
    };
    let scale_f = scale as f32;
    let side = glyph_side * scale_f;
    let (cx, cy) = (centre_x * scale_f, centre_y * scale_f);

    let radius = side * 0.32;
    let lens = (cx - side * 0.5 + radius, cy - side * 0.5 + radius);
    let handle_from =
        (lens.0 + radius * std::f32::consts::FRAC_1_SQRT_2, lens.1 + radius * std::f32::consts::FRAC_1_SQRT_2);
    let mut pb = tiny_skia::PathBuilder::new();
    pb.push_circle(lens.0, lens.1, radius);
    polyline(&mut pb, [handle_from, (cx + side * 0.5, cy + side * 0.5)]);

    let stroke = round_stroke(stroke_width * scale_f, tiny_skia::LineJoin::Miter);
    stroke_solid(&mut pixmap, pb.finish(), bgr(fg), &stroke);
}

pub fn draw_close_glyph_over(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, glyph_side: f32, stroke_width: f32, scale: i32, fg: Color,
) {
    if glyph_side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = over(buf, canvas_width, canvas_height, scale) else {
        return;
    };
    let scale_f = scale as f32;
    let (cx, cy) = (canvas_width as f32 * scale_f / 2.0, canvas_height as f32 * scale_f / 2.0);
    let half = glyph_side * scale_f / 2.0;

    let mut pb = tiny_skia::PathBuilder::new();
    polyline(&mut pb, [(cx - half, cy - half), (cx + half, cy + half)]);
    polyline(&mut pb, [(cx + half, cy - half), (cx - half, cy + half)]);
    let stroke = round_stroke(stroke_width * scale_f, tiny_skia::LineJoin::Miter);
    stroke_solid(&mut pixmap, pb.finish(), bgr(fg), &stroke);
}

#[must_use]
pub fn render_svg(data: &[u8], side: i32) -> Option<Vec<u8>> {
    let side_u32 = square_side(side)?;
    let tree = resvg::usvg::Tree::from_data(data, &resvg::usvg::Options::default()).ok()?;
    let intrinsic = tree.size();
    if intrinsic.width() <= 0.0 || intrinsic.height() <= 0.0 {
        return None;
    }
    let mut pixmap = tiny_skia::Pixmap::new(side_u32, side_u32)?;
    let transform = fit_square_transform(intrinsic.width(), intrinsic.height(), side, true);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    if pixmap.pixels().iter().all(|px| px.alpha() == 0) {
        return None;
    }
    Some(skia_pixmap_into_buffer_bytes(pixmap))
}

#[must_use]
pub fn render_svg_tinted(data: &[u8], side: i32, tint: Color) -> Option<Vec<u8>> {
    let mut bytes = render_svg(data, side)?;
    tint_buffer(&mut bytes, tint);
    Some(bytes)
}

#[must_use]
pub fn render_png_tinted(data: &[u8], side: i32, tint: Color) -> Option<Vec<u8>> {
    let mut bytes = render_png(data, side)?;
    tint_buffer(&mut bytes, tint);
    Some(bytes)
}

fn tint_buffer(bytes: &mut [u8], tint: Color) {
    let [r, g, b, a] = tint.0;
    for pixel in bytes.chunks_exact_mut(4) {
        let alpha = f32::from(pixel[3]) / 255.0;
        let channel = |c: f32| (c * alpha * a * 255.0).round() as u8;
        let out_alpha = (a * alpha * 255.0).round() as u8;
        pixel.copy_from_slice(&[channel(b), channel(g), channel(r), out_alpha]);
    }
}

fn halve_pixmap(source: &tiny_skia::Pixmap) -> Option<tiny_skia::Pixmap> {
    let (w, h) = (source.width() / 2, source.height() / 2);
    let mut out = tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    let src = source.data();
    let stride = source.width() as usize * 4;
    let out_stride = out.width() as usize * 4;
    let dst = out.data_mut();
    for y in 0..h as usize {
        for x in 0..w as usize {
            let a = (2 * y) * stride + (2 * x) * 4;
            let b = a + stride;
            for c in 0..4 {
                let sum = u16::from(src[a + c])
                    + u16::from(src[a + 4 + c])
                    + u16::from(src[b + c])
                    + u16::from(src[b + 4 + c]);
                dst[y * out_stride + x * 4 + c] = ((sum + 2) / 4) as u8;
            }
        }
    }
    Some(out)
}

#[must_use]
pub fn render_png(data: &[u8], side: i32) -> Option<Vec<u8>> {
    let side_u32 = square_side(side)?;
    let mut source = tiny_skia::Pixmap::decode_png(data).ok()?;
    while source.width().min(source.height()) >= side_u32 * 2 {
        source = halve_pixmap(&source)?;
    }
    let (w, h) = (source.width() as f32, source.height() as f32);
    let transform = fit_square_transform(w, h, side, true);
    draw_bilinear(source.as_ref(), side_u32, transform).map(skia_pixmap_into_buffer_bytes)
}

fn square_side(side: i32) -> Option<u32> {
    u32::try_from(side).ok().filter(|&side| side > 0)
}

fn fit_square_transform(src_w: f32, src_h: f32, side: i32, snap: bool) -> tiny_skia::Transform {
    let target = side as f32;
    let scale = (target / src_w).min(target / src_h);
    let (dx, dy) = ((target - src_w * scale) / 2.0, (target - src_h * scale) / 2.0);
    let (dx, dy) = if snap { (dx.round(), dy.round()) } else { (dx, dy) };
    tiny_skia::Transform::from_translate(dx, dy).pre_scale(scale, scale)
}

fn draw_bilinear(
    source: tiny_skia::PixmapRef, side: u32, transform: tiny_skia::Transform,
) -> Option<tiny_skia::Pixmap> {
    let mut pixmap = tiny_skia::Pixmap::new(side, side)?;
    let paint =
        tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..tiny_skia::PixmapPaint::default() };
    pixmap.draw_pixmap(0, 0, source, &paint, transform, None);
    Some(pixmap)
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

pub fn draw_dock_glyph(buf: &mut [u8], width: i32, height: i32, scale: i32, glyph: DockGlyph, color: Color) {
    let Some(mut pixmap) = over(buf, width, height, scale) else {
        return;
    };
    let grid = if glyph == DockGlyph::Grid { 18.0 } else { 20.0 };
    let unit = (width.min(height) as f32 * scale as f32) / 2.0 / grid;
    let origin_x = (width as f32 * scale as f32 - grid * unit) / 2.0;
    let origin_y = (height as f32 * scale as f32 - grid * unit) / 2.0;
    let p = |x: f32, y: f32| (origin_x + x * unit, origin_y + y * unit);
    let mut pb = tiny_skia::PathBuilder::new();
    let mut rounded = |x: f32, y: f32, w: f32, h: f32, r: f32| {
        let (x, y) = p(x, y);
        if let Some(path) = rounded_rect(x, y, w * unit, h * unit, r * unit) {
            pb.push_path(&path);
        }
    };

    match glyph {
        DockGlyph::Grid => {
            for cy in [1.0, 7.0, 13.0] {
                for cx in [1.0, 7.0, 13.0] {
                    rounded(cx, cy, 4.0, 4.0, 1.2);
                }
            }
            fill_solid(&mut pixmap, pb.finish(), bgr(color));
            return;
        }
        DockGlyph::Terminal => {
            rounded(2.5, 3.5, 15.0, 13.0, 2.5);
            polyline(&mut pb, [p(6.0, 8.0), p(8.5, 10.0), p(6.0, 12.0)]);
            polyline(&mut pb, [p(10.5, 12.5), p(14.5, 12.5)]);
        }
        DockGlyph::Files => {
            polyline(
                &mut pb,
                [(2.5, 5.0), (7.5, 5.0), (9.0, 7.0), (17.5, 7.0), (17.5, 15.5), (2.5, 15.5)].map(|(x, y)| p(x, y)),
            );
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
                polyline(&mut pb, [p(x1, y1), p(x2, y2)]);
            }
        }
        DockGlyph::Music => {
            polyline(&mut pb, [p(7.5, 14.0), p(7.5, 5.0), p(14.5, 3.5), p(14.5, 13.0)]);
            for (x, y) in [(5.8, 14.2), (12.8, 12.7)] {
                let (x, y) = p(x, y);
                pb.push_circle(x, y, 1.8 * unit);
            }
        }
        DockGlyph::Mail => {
            rounded(2.5, 4.5, 15.0, 11.0, 2.5);
            polyline(&mut pb, [p(2.5, 6.5), p(10.0, 11.0), p(17.5, 6.5)]);
        }
        DockGlyph::Generic => rounded(3.5, 3.5, 13.0, 13.0, 2.5),
    }

    let stroke = round_stroke(1.5 * unit, tiny_skia::LineJoin::Round);
    stroke_solid(&mut pixmap, pb.finish(), bgr(color), &stroke);
}

pub fn fill_rounded_rect_gradient_over(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, rect_x: i32, rect_y: i32, rect_width: i32, rect_height: i32,
    scale: i32, from: Color, to: Color, radius: f32,
) {
    let Some(mut pixmap) = over(buf, canvas_width, canvas_height, scale) else {
        return;
    };
    let scale_f = scale as f32;
    let (x, y, rw, rh) =
        (rect_x as f32 * scale_f, rect_y as f32 * scale_f, rect_width as f32 * scale_f, rect_height as f32 * scale_f);

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
            tiny_skia::GradientStop::new(0.0, to_skia_color(bgr(from))),
            tiny_skia::GradientStop::new(1.0, to_skia_color(bgr(to))),
        ],
        tiny_skia::SpreadMode::Pad,
        tiny_skia::Transform::identity(),
    ) else {
        return;
    };

    let paint = tiny_skia::Paint { shader, anti_alias: true, ..tiny_skia::Paint::default() };
    pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
}

#[must_use]
pub fn decode_cover(data: &[u8], side: i32) -> Option<Vec<u8>> {
    if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        return render_png(data, side);
    }
    if data.starts_with(&[0xff, 0xd8]) {
        return render_jpeg(data, side);
    }
    if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        return render_webp(data, side);
    }
    render_png(data, side).or_else(|| render_jpeg(data, side))
}

#[must_use]
pub(crate) fn render_webp(data: &[u8], side: i32) -> Option<Vec<u8>> {
    fit_into_square(wallpaper::decode_webp(data)?, side)
}

#[must_use]
pub(crate) fn render_jpeg(data: &[u8], side: i32) -> Option<Vec<u8>> {
    fit_into_square(wallpaper::decode_jpeg(data)?, side)
}

fn fit_into_square(mut source: tiny_skia::Pixmap, side: i32) -> Option<Vec<u8>> {
    let side_u32 = square_side(side)?;
    for px in source.data_mut().chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let (w, h) = (source.width() as f32, source.height() as f32);
    let transform = fit_square_transform(w, h, side, false);
    draw_bilinear(source.as_ref(), side_u32, transform).map(tiny_skia::Pixmap::take)
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
    Color([(r * gain).clamp(0.0, 1.0), (g * gain).clamp(0.0, 1.0), (b * gain).clamp(0.0, 1.0), a])
}

pub fn draw_moon_glyph_over(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, glyph_side: f32, scale: i32, fg: Color,
) {
    let (w, h, round) = (canvas_width, canvas_height, tiny_skia::LineJoin::Round);
    draw_stroked_glyph(buf, w, h, glyph_side, 1.4, scale, fg, 18.0, round, |g| {
        g.move_to(14.2, 10.4);
        g.cubic_to(12.0, 4.8, 6.2, 4.2, 3.8, 7.6);
        g.cubic_to(3.2, 12.0, 7.4, 15.6, 12.2, 14.8);
        g.cubic_to(13.8, 14.2, 14.8, 12.6, 14.2, 10.4);
        g.pb.close();
    });
}

pub fn draw_tick_glyph_over(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, glyph_side: f32, stroke_width: f32, scale: i32, fg: Color,
) {
    let (w, h, round) = (canvas_width, canvas_height, tiny_skia::LineJoin::Round);
    draw_stroked_glyph(buf, w, h, glyph_side, stroke_width, scale, fg, 14.0, round, |g| {
        g.polyline(&[(2.6, 7.4), (5.4, 10.2), (11.4, 3.8)]);
    });
}

pub fn draw_chevron_glyph_over(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, glyph_side: f32, stroke_width: f32, point_up: bool,
    scale: i32, fg: Color,
) {
    let (w, h, round) = (canvas_width, canvas_height, tiny_skia::LineJoin::Round);
    draw_stroked_glyph(buf, w, h, glyph_side, stroke_width, scale, fg, 10.0, round, |g| {
        if point_up {
            g.polyline(&[(2.6, 6.0), (5.0, 3.6), (7.4, 6.0)]);
        } else {
            g.polyline(&[(2.6, 4.0), (5.0, 6.4), (7.4, 4.0)]);
        }
    });
}

pub fn draw_speaker_glyph_over(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, side: f32, stroke_width: f32, muted: bool, scale: i32,
    fg: Color,
) {
    let (w, h, round) = (canvas_width, canvas_height, tiny_skia::LineJoin::Round);
    draw_stroked_glyph(buf, w, h, side, stroke_width, scale, fg, 18.0, round, |g| {
        g.polyline(&[(3.0, 6.6), (5.6, 6.6), (9.0, 3.6), (9.0, 14.4), (5.6, 11.4), (3.0, 11.4)]);
        g.pb.close();
        if muted {
            g.segments(&[(12.2, 6.2, 15.6, 11.8), (15.6, 6.2, 12.2, 11.8)]);
        } else {
            g.move_to(11.8, 6.6);
            g.quad_to(14.0, 9.0, 11.8, 11.4);
            g.move_to(14.2, 4.8);
            g.quad_to(17.2, 9.0, 14.2, 13.2);
        }
    });
}

pub(crate) fn draw_wifi_waves_level_over(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, side: f32, stroke_width: f32, level: u8, scale: i32,
    fg: Color, muted_fg: Color,
) {
    if side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = over(buf, canvas_width, canvas_height, scale) else {
        return;
    };
    let colour_from = |min_level: u8| if level >= min_level { fg } else { muted_fg };
    let stroke = round_stroke(stroke_width * scale as f32, tiny_skia::LineJoin::Miter);
    let new_glyph = || GlyphPath::new(canvas_width, canvas_height, side, scale, 18.0);
    for (min_level, x0, y, ctrl_y, x1) in
        [(4, 2.0, 6.4, 1.8, 16.0), (3, 4.6, 9.2, 5.8, 13.4), (2, 7.0, 12.0, 9.8, 11.0)]
    {
        let mut arc = new_glyph();
        arc.move_to(x0, y);
        arc.quad_to(9.0, ctrl_y, x1, y);
        stroke_solid(&mut pixmap, arc.pb.finish(), bgr(colour_from(min_level)), &stroke);
    }
    let mut dot = new_glyph();
    dot.circle(9.0, 14.6, 1.2);
    fill_solid(&mut pixmap, dot.pb.finish(), bgr(colour_from(1)));
}

pub fn draw_wifi_waves_glyph_over(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, side: f32, stroke_width: f32, scale: i32, fg: Color,
) {
    draw_wifi_waves_level_over(buf, canvas_width, canvas_height, side, stroke_width, 4, scale, fg, fg);
}

macro_rules! stroked_glyphs {
    ($($name:ident, $join:ident, |$g:ident| $body:block)*) => {
        $(
            pub fn $name(
                buf: &mut [u8],
                canvas_width: i32,
                canvas_height: i32,
                side: f32,
                stroke_width: f32,
                scale: i32,
                fg: Color,
            ) {
                let (w, h, join) = (canvas_width, canvas_height, tiny_skia::LineJoin::$join);
                draw_stroked_glyph(buf, w, h, side, stroke_width, scale, fg, 18.0, join, |$g| $body);
            }
        )*
    };
}

stroked_glyphs! {
    draw_bluetooth_glyph_over, Round, |g| {
        g.polyline(&[
            (6.0, 5.4),
            (12.0, 12.0),
            (9.0, 14.4),
            (9.0, 3.6),
            (12.0, 6.0),
            (6.0, 12.6),
        ]);
    }
    draw_camera_glyph_over, Round, |g| {
        g.quad_box(2.5, 5.2, 13.0, 8.8, 1.6);
        g.polyline(&[(6.8, 5.2), (7.6, 3.4), (10.4, 3.4), (11.2, 5.2)]);
        g.circle(9.0, 9.6, 2.5);
        g.circle(13.2, 7.0, 0.6);
    }
    draw_sun_glyph_over, Miter, |g| {
        g.circle(9.0, 9.0, 3.2);
        g.segments(&[
            (9.0, 1.8, 9.0, 3.6),
            (9.0, 14.4, 9.0, 16.2),
            (1.8, 9.0, 3.6, 9.0),
            (14.4, 9.0, 16.2, 9.0),
            (3.9, 3.9, 5.2, 5.2),
            (12.8, 12.8, 14.1, 14.1),
            (14.1, 3.9, 12.8, 5.2),
            (5.2, 12.8, 3.9, 14.1),
        ]);
    }
    draw_keyboard_glyph_over, Round, |g| {
        let (x, y) = g.pt(2.0, 4.2);
        if let Some(r) = tiny_skia::Rect::from_xywh(x, y, 14.0 * g.unit, 9.6 * g.unit) {
            g.pb.push_rect(r);
        }
        g.segments(&[
            (5.0, 7.2, 5.5, 7.2),
            (8.5, 7.2, 9.5, 7.2),
            (12.5, 7.2, 13.0, 7.2),
            (5.5, 10.4, 12.5, 10.4),
        ]);
    }
    draw_gear_glyph_over, Round, |g| {
        let (cx, cy, unit) = (g.cx, g.cy, g.unit);
        g.pb.push_circle(cx, cy, 2.2 * unit);
        let teeth = 6;
        let (r_in, r_out) = (4.4 * unit, 6.4 * unit);
        let half_tooth = std::f32::consts::PI / 18.0;
        let step = std::f32::consts::PI * 2.0 / (teeth as f32);
        let at = |angle: f32, r: f32| (cx + angle.cos() * r, cy + angle.sin() * r);
        let points = (0..teeth).flat_map(|i| {
            let mid = (i as f32) * step;
            let (a1, a2) = (mid - half_tooth, mid + half_tooth);
            [
                at(a1, r_in),
                at(a1, r_out),
                at(a2, r_out),
                at(a2, r_in),
                at(mid + step - half_tooth, r_in),
            ]
        });
        polyline(&mut g.pb, points);
        g.pb.close();
    }
    draw_lock_glyph_over, Round, |g| {
        g.quad_box(3.6, 7.6, 10.8, 7.4, 1.8);
        g.polyline(&[(6.0, 7.6), (6.0, 5.2)]);
        g.cubic_to(6.0, 2.2, 12.0, 2.2, 12.0, 5.2);
        g.line_to(12.0, 7.6);
        g.circle(9.0, 10.5, 0.9);
        g.polyline(&[(9.0, 11.4), (9.0, 12.8)]);
    }
    draw_power_glyph_over, Miter, |g| {
        g.segments(&[(9.0, 2.6, 9.0, 8.8)]);
        g.move_to(5.0, 5.0);
        g.quad_to(1.0, 10.0, 5.5, 14.5);
        g.quad_to(9.0, 16.5, 12.5, 14.5);
        g.quad_to(17.0, 10.0, 13.0, 5.0);
    }
}

#[must_use]
pub(crate) fn resample_square(src: &[u8], src_px: i32, dst_px: i32) -> Option<Vec<u8>> {
    let (src_u, dst_u) = (u32::try_from(src_px).ok()?, u32::try_from(dst_px).ok()?);
    if src_u == 0 || dst_u == 0 || src.len() < (src_u * src_u * 4) as usize {
        return None;
    }
    if src_u == dst_u {
        return Some(src[..(src_u * src_u * 4) as usize].to_vec());
    }
    let mut source = tiny_skia::Pixmap::new(src_u, src_u)?;
    source.data_mut().copy_from_slice(&src[..(src_u * src_u * 4) as usize]);
    while source.width() >= dst_u * 2 {
        source = halve_pixmap(&source)?;
    }
    let factor = dst_u as f32 / source.width() as f32;
    let transform = tiny_skia::Transform::from_scale(factor, factor);
    draw_bilinear(source.as_ref(), dst_u, transform).map(tiny_skia::Pixmap::take)
}

pub fn blit_icon_over(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, src: &[u8], src_px: i32, side: i32, dst_x: i32, dst_y: i32,
    scale: i32,
) {
    if let Some(icon) = resample_square(src, src_px, side * scale) {
        blit_square_over(buf, canvas_width, canvas_height, &icon, side, dst_x, dst_y, scale);
    }
}

pub fn blit_square_over(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, src: &[u8], src_side: i32, dst_x: i32, dst_y: i32,
    scale: i32,
) {
    blit_rect_over(buf, canvas_width, canvas_height, src, src_side, src_side, dst_x, dst_y, scale);
}

pub fn blit_rect_over(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, src: &[u8], src_width: i32, src_height: i32, dst_x: i32,
    dst_y: i32, scale: i32,
) {
    let (Ok(sw), Ok(sh)) = (u32::try_from(src_width * scale), u32::try_from(src_height * scale)) else {
        return;
    };
    if sw == 0 || sh == 0 || src.len() < (sw as usize) * (sh as usize) * 4 {
        return;
    }
    let Some(source) = tiny_skia::PixmapRef::from_bytes(src, sw, sh) else {
        return;
    };
    let Some(mut pixmap) = over(buf, canvas_width, canvas_height, scale) else {
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

pub fn stroke_rounded_rect_over(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, rect_x: i32, rect_y: i32, rect_width: i32, rect_height: i32,
    scale: i32, stroke_width: f32, radius: f32, color: Color,
) {
    if stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = over(buf, canvas_width, canvas_height, scale) else {
        return;
    };
    let scale_f = scale as f32;
    let inset = stroke_width * scale_f / 2.0;
    let path = rounded_rect(
        rect_x as f32 * scale_f + inset,
        rect_y as f32 * scale_f + inset,
        rect_width as f32 * scale_f - inset * 2.0,
        rect_height as f32 * scale_f - inset * 2.0,
        (radius * scale_f - inset).max(0.0),
    );
    let stroke = tiny_skia::Stroke { width: stroke_width * scale_f, ..tiny_skia::Stroke::default() };
    stroke_solid(&mut pixmap, path, bgr(color), &stroke);
}

fn render_icon(
    buf: &mut [u8], width: i32, height: i32, scale: i32, bg: Color,
    draw_glyph: impl FnOnce(&mut tiny_skia::PixmapMut<'_>),
) {
    let Some(mut pixmap) = cleared(buf, width, height, scale) else {
        return;
    };
    if let Some(path) = rounded_rect(0.0, 0.0, width as f32, height as f32, glide_theme::shape::R2) {
        fill(&mut pixmap, &path, bg, scale);
    }
    draw_glyph(&mut pixmap);
}

fn glyph_bounds(width: i32, height: i32) -> (f32, f32, f32, f32) {
    const GLYPH_RATIO: f32 = 0.5;

    let side = (width.min(height) as f32).max(4.0) * GLYPH_RATIO;
    let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
    let half = side / 2.0;
    (cx - half, cy - half, cx + half, cy + half)
}

fn paint_for(color: Color) -> tiny_skia::Paint<'static> {
    tiny_skia::Paint {
        shader: tiny_skia::Shader::SolidColor(to_skia_color(color)),
        anti_alias: true,
        ..tiny_skia::Paint::default()
    }
}

fn scale_transform(scale: i32) -> tiny_skia::Transform {
    let s = scale as f32;
    tiny_skia::Transform::from_scale(s, s)
}

fn stroke(pixmap: &mut tiny_skia::PixmapMut<'_>, path: &tiny_skia::Path, color: Color, height: i32, scale: i32) {
    let width = (height as f32 / 16.0).clamp(1.5, 2.5);
    let stroke = round_stroke(width, tiny_skia::LineJoin::Round);
    pixmap.stroke_path(path, &paint_for(bgr(color)), &stroke, scale_transform(scale), None);
}

fn fill(pixmap: &mut tiny_skia::PixmapMut<'_>, path: &tiny_skia::Path, color: Color, scale: i32) {
    pixmap.fill_path(path, &paint_for(bgr(color)), tiny_skia::FillRule::Winding, scale_transform(scale), None);
}

const KAPPA: f32 = 0.552_284_8;

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<tiny_skia::Path> {
    rounded_rect_corners(x, y, w, h, radius, [true; 4])
}

#[allow(clippy::many_single_char_names)]
fn rounded_rect_corners(x: f32, y: f32, w: f32, h: f32, radius: f32, corners: [bool; 4]) -> Option<tiny_skia::Path> {
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
    tiny_skia::Color::from_rgba(r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0), a.clamp(0.0, 1.0))
        .unwrap_or(tiny_skia::Color::TRANSPARENT)
}

fn skia_pixmap_into_buffer_bytes(pixmap: tiny_skia::Pixmap) -> Vec<u8> {
    let mut bytes = pixmap.take();
    for px in bytes.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    bytes
}

fn copy_pixmap_into(buf: &mut [u8], pixmap: tiny_skia::Pixmap) {
    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    buf[..n].copy_from_slice(&bytes[..n]);
}

fn over(buf: &mut [u8], canvas_width: i32, canvas_height: i32, scale: i32) -> Option<tiny_skia::PixmapMut<'_>> {
    let (width, height) = (u32::try_from(canvas_width * scale).ok()?, u32::try_from(canvas_height * scale).ok()?);
    let len = usize::try_from(u64::from(width) * u64::from(height) * 4).ok()?;
    tiny_skia::PixmapMut::from_bytes(buf.get_mut(..len)?, width, height)
}

fn cleared(buf: &mut [u8], canvas_width: i32, canvas_height: i32, scale: i32) -> Option<tiny_skia::PixmapMut<'_>> {
    let mut pixmap = over(buf, canvas_width, canvas_height, scale)?;
    pixmap.fill(tiny_skia::Color::TRANSPARENT);
    Some(pixmap)
}

fn bgr(Color([r, g, b, a]): Color) -> Color {
    Color([b, g, r, a])
}

fn round_stroke(width: f32, line_join: tiny_skia::LineJoin) -> tiny_skia::Stroke {
    tiny_skia::Stroke { width, line_cap: tiny_skia::LineCap::Round, line_join, ..tiny_skia::Stroke::default() }
}

fn stroke_solid(
    pixmap: &mut tiny_skia::PixmapMut<'_>, path: Option<tiny_skia::Path>, color: Color, stroke: &tiny_skia::Stroke,
) {
    if let Some(path) = path {
        pixmap.stroke_path(&path, &paint_for(color), stroke, tiny_skia::Transform::identity(), None);
    }
}

fn fill_solid(pixmap: &mut tiny_skia::PixmapMut<'_>, path: Option<tiny_skia::Path>, color: Color) {
    if let Some(path) = path {
        pixmap.fill_path(
            &path,
            &paint_for(color),
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }
}

fn polyline(pb: &mut tiny_skia::PathBuilder, points: impl IntoIterator<Item = (f32, f32)>) {
    for (i, (x, y)) in points.into_iter().enumerate() {
        if i == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
}

struct GlyphPath {
    pb: tiny_skia::PathBuilder,
    cx: f32,
    cy: f32,
    unit: f32,
    ox: f32,
    oy: f32,
}

impl GlyphPath {
    fn new(canvas_width: i32, canvas_height: i32, side: f32, scale: i32, grid: f32) -> Self {
        let scale_f = scale as f32;
        let cx = (canvas_width as f32 / 2.0) * scale_f;
        let cy = (canvas_height as f32 / 2.0) * scale_f;
        let unit = (side * scale_f) / grid;
        Self { pb: tiny_skia::PathBuilder::new(), cx, cy, unit, ox: cx - grid / 2.0 * unit, oy: cy - grid / 2.0 * unit }
    }

    fn pt(&self, x: f32, y: f32) -> (f32, f32) {
        (self.ox + x * self.unit, self.oy + y * self.unit)
    }

    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.pt(x, y);
        self.pb.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.pt(x, y);
        self.pb.line_to(x, y);
    }

    fn polyline(&mut self, points: &[(f32, f32)]) {
        let points: Vec<_> = points.iter().map(|&(x, y)| self.pt(x, y)).collect();
        polyline(&mut self.pb, points);
    }

    fn segments(&mut self, segments: &[(f32, f32, f32, f32)]) {
        for &(x1, y1, x2, y2) in segments {
            self.polyline(&[(x1, y1), (x2, y2)]);
        }
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let ((x1, y1), (x, y)) = (self.pt(x1, y1), self.pt(x, y));
        self.pb.quad_to(x1, y1, x, y);
    }

    fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let ((x1, y1), (x2, y2), (x, y)) = (self.pt(x1, y1), self.pt(x2, y2), self.pt(x, y));
        self.pb.cubic_to(x1, y1, x2, y2, x, y);
    }

    fn circle(&mut self, x: f32, y: f32, r: f32) {
        let (x, y) = self.pt(x, y);
        self.pb.push_circle(x, y, r * self.unit);
    }

    fn quad_box(&mut self, left: f32, top: f32, width: f32, height: f32, radius: f32) {
        let (bx, by) = self.pt(left, top);
        let (bw, bh, r) = (width * self.unit, height * self.unit, radius * self.unit);
        let pb = &mut self.pb;
        pb.move_to(bx + r, by);
        pb.line_to(bx + bw - r, by);
        pb.quad_to(bx + bw, by, bx + bw, by + r);
        pb.line_to(bx + bw, by + bh - r);
        pb.quad_to(bx + bw, by + bh, bx + bw - r, by + bh);
        pb.line_to(bx + r, by + bh);
        pb.quad_to(bx, by + bh, bx, by + bh - r);
        pb.line_to(bx, by + r);
        pb.quad_to(bx, by, bx + r, by);
    }
}

fn draw_stroked_glyph(
    buf: &mut [u8], canvas_width: i32, canvas_height: i32, side: f32, stroke_width: f32, scale: i32, fg: Color,
    grid: f32, line_join: tiny_skia::LineJoin, build: impl FnOnce(&mut GlyphPath),
) {
    if side <= 0.0 || stroke_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = over(buf, canvas_width, canvas_height, scale) else {
        return;
    };
    let mut glyph = GlyphPath::new(canvas_width, canvas_height, side, scale, grid);
    build(&mut glyph);
    let stroke = round_stroke(stroke_width * scale as f32, line_join);
    stroke_solid(&mut pixmap, glyph.pb.finish(), bgr(fg), &stroke);
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Color = Color([1.0, 1.0, 1.0, 1.0]);
    const CLEAR: Color = Color([0.0; 4]);

    #[test]
    fn composite_over_blends_straight_source_onto_premultiplied_destination() {
        assert_eq!(composite_over([10, 20, 30, 255], [200, 100, 50], 255), [200, 100, 50, 255]);
        assert_eq!(composite_over([10, 20, 30, 200], [255, 0, 0], 0), [10, 20, 30, 200]);
        let half = composite_over([0, 0, 0, 0], [255, 255, 255], 128);
        assert!(half[3] == 128 && (100..150).contains(&half[0]));
        let grey = composite_over([0, 0, 0, 255], [255, 255, 255], 128);
        assert!(grey[3] == 255 && (100..=150).contains(&grey[0]));

        let mut pixmap = tiny_skia::Pixmap::new(1, 1).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        assert_eq!(skia_pixmap_into_buffer_bytes(pixmap), vec![0, 0, 255, 255]);
    }

    #[test]
    fn rounded_rects_fill_their_box_whatever_the_corners_and_radius() {
        for (corners, radius) in
            [([true; 4], 8.0), ([false; 4], 8.0), ([true, true, false, false], 8.0), ([true; 4], 1000.0)]
        {
            let b = rounded_rect_corners(10.0, 20.0, 100.0, 50.0, radius, corners).unwrap().bounds();
            assert!((b.left() - 10.0).abs() < 0.01 && (b.top() - 20.0).abs() < 0.01);
            assert!((b.width() - 100.0).abs() < 0.01 && (b.height() - 50.0).abs() < 0.01);
        }
        assert!(rounded_rect(0.0, 0.0, 0.0, 10.0, 4.0).is_none());
        assert!(rounded_rect(0.0, 0.0, 10.0, -1.0, 4.0).is_none());
    }

    #[test]
    fn panels_cut_only_their_rounded_corners() {
        let mut buf = vec![0u8; 100 * 32 * 4];
        let corners = [true, true, false, false];
        Painter::fill_panel_background(&mut buf, 100, 32, 1, WHITE, 8.0, corners);
        let alpha = |x: usize, y: usize| buf[(y * 100 + x) * 4 + 3];
        assert_eq!((alpha(50, 0), alpha(0, 0), alpha(0, 31)), (255, 0, 255));
    }

    #[test]
    fn rounded_rect_over_blends_bgra_inside_its_rect_only() {
        let (w, h) = (40, 20);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let red = Color([1.0, 0.0, 0.0, 1.0]);
        Painter::fill_rounded_rect_over(&mut buf, w, h, 20, 0, 20, h, 1, red, 0.0);
        let at = |x: i32| ((h / 2 * w + x) * 4) as usize;
        assert_eq!(&buf[at(30)..at(30) + 4], &[0, 0, 255, 255]);
        assert_eq!(buf[at(5) + 3], 0);

        let before = buf.clone();
        let blue = Color([0.0, 0.0, 1.0, 0.5]);
        Painter::fill_rounded_rect_over(&mut buf, w, h, 20, 0, 10, h, 1, blue, 0.0);
        assert_ne!(buf[at(25)..at(25) + 4], before[at(25)..at(25) + 4]);
        assert_eq!(buf[at(35)..at(35) + 4], before[at(35)..at(35) + 4]);
    }

    #[test]
    fn window_icons_survive_tiny_cells_and_sit_on_a_rounded_pill() {
        for (w, h) in [(1, 1), (2, 3), (28, 28), (40, 26)] {
            for scale in [1, 2] {
                for icon in [Painter::draw_icon_maximize, Painter::draw_icon_restore, Painter::draw_icon_close] {
                    let mut buf = vec![0u8; (w * scale * h * scale * 4) as usize];
                    icon(&mut buf, w, h, scale, WHITE, Color([0.1, 0.1, 0.1, 1.0]));
                }
            }
        }
        let mut buf = vec![0u8; 32 * 32 * 4];
        Painter::draw_icon_maximize(&mut buf, 32, 32, 1, WHITE, Color([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(buf[3], 0);
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
        (sum_x / sum_a / f64::from(scale), sum_y / sum_a / f64::from(scale))
    }

    #[test]
    fn window_glyphs_are_centred_half_cell_marks_at_every_scale() {
        type Icon = fn(&mut [u8], i32, i32, i32, Color, Color);
        let cases: [(Icon, i32, i32, i32); 6] = [
            (Painter::draw_icon_close, 32, 32, 1),
            (Painter::draw_icon_close, 40, 26, 1),
            (Painter::draw_icon_close, 26, 40, 1),
            (Painter::draw_icon_close, 32, 44, 1),
            (Painter::draw_icon_maximize, 32, 32, 1),
            (Painter::draw_icon_maximize, 32, 32, 2),
        ];
        for (icon, w, h, scale) in cases {
            let mut buf = vec![0u8; (w * scale * h * scale * 4) as usize];
            icon(&mut buf, w, h, scale, WHITE, CLEAR);
            let (cx, cy) = alpha_centroid(&buf, w, h, scale);
            assert!((cx - f64::from(w) / 2.0).abs() < 1.0, "{w}x{h}@{scale}");
            assert!((cy - f64::from(h) / 2.0).abs() < 1.0, "{w}x{h}@{scale}");
        }

        let mut buf = vec![0u8; 32 * 32 * 4];
        Painter::draw_icon_maximize(&mut buf, 32, 32, 1, WHITE, CLEAR);
        let inked: Vec<usize> = (0..32 * 32).filter(|i| buf[i * 4 + 3] > 10).map(|i| i % 32).collect();
        let span = inked.iter().max().unwrap() - inked.iter().min().unwrap();
        assert!(span.abs_diff(16) <= 3, "span {span}");
    }

    fn fit(painter: &Painter, text: &str, avail_w: i32) -> String {
        let size = painter.font_size;
        painter.fit_text_at(text, avail_w, size, DEFAULT_WEIGHT, DEFAULT_TRACKING_EM, false)
    }

    #[test]
    fn fit_text_ellipsises_exactly_like_a_linear_search() {
        let painter = Painter::with_embedded_font_only(14.0);
        assert_eq!(fit(&painter, "ok", 1000), "ok");
        assert_eq!(fit(&painter, "anything", 0), "");
        assert_eq!(fit(&painter, "", 1000), "");
        let long = "a very long window title that will not fit in the available space";
        let graphemes: Vec<&str> = long.graphemes(true).collect();
        for avail_w in [10, 30, 60, 100, 200] {
            let linear = (0..graphemes.len())
                .rev()
                .map(|k| graphemes[..k].concat() + "…")
                .find(|candidate| painter.text_width(candidate) <= avail_w as f32)
                .unwrap_or_default();
            assert_eq!(fit(&painter, long, avail_w), linear, "avail_w={avail_w}");
        }
    }

    #[test]
    fn fit_text_never_splits_a_grapheme() {
        let painter = Painter::with_embedded_font_only(14.0);
        let text = "Cafe\u{301} 👩\u{200d}💻 family 👨\u{200d}👩\u{200d}👧 e\u{301}e\u{301}e\u{301} done";
        let full = painter.text_width(text).ceil() as i32;
        for avail_w in 1..full {
            let fitted = fit(&painter, text, avail_w);
            let kept = fitted.strip_suffix('…').unwrap_or(&fitted);
            assert!(text.starts_with(kept), "avail_w={avail_w}: {fitted:?}");
            assert!(
                text.grapheme_indices(true).any(|(start, _)| start == kept.len()),
                "avail_w={avail_w}: cut inside a grapheme: {fitted:?}"
            );
        }
    }

    #[test]
    fn labels_stay_on_one_line_whatever_whitespace_they_carry() {
        let painter = Painter::with_embedded_font_only(14.0);
        let flat = painter.text_width("first line second line");
        assert_eq!(painter.text_width("first line\nsecond line"), flat);
        assert_eq!(painter.text_width("first line\r\n\tsecond  line"), flat);
        assert_ne!(painter.text_width("two  spaces"), painter.text_width("two spaces"));
        assert_eq!(fit(&painter, "short\nnote", 1000), "short note");
        assert!(!fit(&painter, "a much longer first line\nand more", 60).contains('\n'));
    }

    #[test]
    fn widths_follow_size_tracking_case_and_survive_the_cache() {
        let painter = Painter::with_embedded_font_only(14.0);
        let measure = |text: &str, size: f32, tracking: f32| {
            painter.measure_width_cached_with(text, size, DEFAULT_WEIGHT, tracking, false)
        };
        assert!(measure("Hello", 28.0, 0.0) > measure("Hello", 14.0, 0.0));
        assert_ne!(measure("Hello", 14.0, 0.0), measure("Hello", 14.0, 0.3));
        let uncached = painter.measure_width_uncached("Hello", 14.0, DEFAULT_WEIGHT, 0.0, false);
        assert!((measure("Hello", 14.0, 0.0) - uncached).abs() < f32::EPSILON);

        let styled = |text, style| painter.text_width_styled(text, style);
        assert!((styled("micro", &TextStyle::MICRO) - styled("MICRO", &TextStyle::MICRO)).abs() < 0.01);
        assert!(styled("Overview", &TextStyle::DISPLAY) > styled("Overview", &TextStyle::CAP));
    }

    #[test]
    fn style_keys_separate_every_shaping_input() {
        let base = style_key_with(FaceRole::Ui, 14.0, 400, 0.0, false);
        assert_eq!(base, style_key_with(FaceRole::Ui, 14.0, 400, 0.0, false));
        for other in [
            style_key_with(FaceRole::Mono, 14.0, 400, 0.0, false),
            style_key_with(FaceRole::Ui, 15.0, 400, 0.0, false),
            style_key_with(FaceRole::Ui, 14.0, 700, 0.0, false),
            style_key_with(FaceRole::Ui, 14.0, 400, 0.1, false),
            style_key_with(FaceRole::Ui, 14.0, 400, 0.0, true),
        ] {
            assert_ne!(base, other);
        }
    }

    #[test]
    fn jetbrains_mono_is_picked_for_tabular_text_only_when_installed() {
        assert_eq!(tabular_family(["Adwaita Sans", "JetBrains Mono", "Noto Sans"]), Some(TABULAR_FAMILY));
        assert_eq!(tabular_family(["jetbrains mono"]), Some(TABULAR_FAMILY));
        assert_eq!(tabular_family(["Adwaita Sans", "JetBrains Mono NL"]), None);
        assert_eq!(tabular_family(std::iter::empty()), None);

        let painter = Painter::with_embedded_font_only(14.0);
        assert_eq!(painter.mono_family, None);
        assert_eq!(painter.family_for(painter.face_role(true)), "Liberation Sans");
        let wide = painter.text_width_styled("00:00", &TextStyle::NUM);
        let narrow = painter.text_width_styled("11:11", &TextStyle::NUM);
        assert!((wide - narrow).abs() < 0.5, "{wide} vs {narrow}");
    }

    #[test]
    fn a_painter_with_the_monospace_family_draws_tabular_text_in_it() {
        let mut painter = Painter::with_embedded_font_only(14.0);
        painter.mono_family = Some(TABULAR_FAMILY);
        assert_eq!(painter.face_role(false), FaceRole::Ui);
        assert_eq!(painter.face_role(true), FaceRole::Mono);
        assert_eq!(painter.family_for(FaceRole::Mono), TABULAR_FAMILY);
        assert_eq!(painter.family_for(FaceRole::Ui), "Liberation Sans");
        let wide = painter.text_width_styled("00:00", &TextStyle::NUM);
        let narrow = painter.text_width_styled("11:11", &TextStyle::NUM);
        assert!((wide - narrow).abs() < 0.5, "{wide} vs {narrow}");
    }

    fn label(painter: &Painter, x: (i32, i32), scale: i32, align: TextAlign, text: &str) -> Vec<u8> {
        let mut buf = vec![0u8; (200 * 32 * scale * scale * 4) as usize];
        painter.draw_label_over(&mut buf, 200, 32, x.0, x.1, scale, align, text, [255; 4]);
        buf
    }

    #[test]
    fn labels_are_deterministic_scaled_ellipsised_and_aligned() {
        let painter = Painter::with_embedded_font_only(14.0);
        let render = |scale, align, text| label(&painter, (20, 180), scale, align, text);
        let hello = render(1, TextAlign::Left, "Hello");
        assert_eq!(hello, render(1, TextAlign::Left, "Hello"));
        assert!(hello.iter().any(|&b| b != 0));
        let doubled = render(2, TextAlign::Left, "Hello");
        assert!(doubled.len() == 400 * 64 * 4 && doubled.iter().any(|&b| b != 0));
        let long = "a title so long it must be ellipsised to fit the available space";
        assert!(render(1, TextAlign::Left, long).iter().any(|&b| b != 0));
        assert_ne!(render(1, TextAlign::Left, "Hi"), render(1, TextAlign::Center, "Hi"));
    }

    #[test]
    fn a_translucent_text_colour_paints_translucent_glyphs() {
        let painter = Painter::with_embedded_font_only(14.0);
        let densest = |alpha: u8| {
            let mut buf = vec![0u8; 200 * 32 * 4];
            painter.draw_label_over(&mut buf, 200, 32, 20, 180, 1, TextAlign::Left, "Hello", [255, 255, 255, alpha]);
            buf.chunks_exact(4).map(|px| px[3]).max().unwrap_or(0)
        };
        let (opaque, half) = (densest(255), densest(128));
        assert!(opaque > 200, "solid text reaches full coverage, got {opaque}");
        assert!(half.abs_diff(opaque / 2) <= 2, "fg-2 and fg-3 are alpha tokens, got {half} of {opaque}");
        assert_eq!(densest(0), 0);
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
    fn transparent_covers_report_nothing_and_grey_ones_still_report() {
        assert!(dominant_colour(&[0u8; 64 * 4]).is_none());
        let colour = dominant_colour(&solid(130, 130, 130)).unwrap();
        assert!((colour.0[0] - colour.0[1]).abs() < 0.01);
    }

    #[test]
    fn fit_for_contrast_moves_only_out_of_range_colours_into_range() {
        let colour = Color([0.6, 0.5, 0.3, 1.0]);
        assert_eq!(fit_for_contrast(colour, 0.35, 0.8), colour);
        let luma = |c: Color| 0.2126 * c.0[0] + 0.7152 * c.0[1] + 0.0722 * c.0[2];
        assert!(luma(fit_for_contrast(Color([0.05, 0.06, 0.2, 1.0]), 0.35, 0.8)) >= 0.34);
        assert!(luma(fit_for_contrast(Color([0.98, 0.99, 0.97, 1.0]), 0.35, 0.8)) <= 0.81);
        let black = fit_for_contrast(Color([0.0, 0.0, 0.0, 1.0]), 0.35, 0.8);
        assert!(black.0[0] > 0.3 && black.0[0].is_finite());
    }

    #[test]
    fn unrecognised_empty_and_truncated_covers_are_refused() {
        assert!(decode_cover(b"not an image at all", 32).is_none());
        assert!(decode_cover(&[], 32).is_none());
        assert!(render_jpeg(&[], 32).is_none());
        assert!(render_webp(&[], 32).is_none());
        assert!(decode_cover(&[0xff, 0xd8, 0xff, 0xe0], 32).is_none());
    }

    #[test]
    fn a_webp_cover_is_decoded_with_premultiplied_alpha() {
        let rgba: Vec<u8> = [[255, 0, 0, 255], [0, 0, 255, 128]].into_iter().cycle().take(4).flatten().collect();
        let mut encoded = Vec::new();
        image_webp::WebPEncoder::new(&mut encoded)
            .encode(&rgba, 2, 2, image_webp::ColorType::Rgba8)
            .expect("lossless encode of a 2x2 image");

        let decoded = decode_cover(&encoded, 4).expect("webp is recognised");
        assert_eq!(decoded.len(), 4 * 4 * 4);
        assert!(
            decoded.chunks_exact(4).all(|px| px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3]),
            "every channel stays within its alpha"
        );
        assert!(decoded.chunks_exact(4).any(|px| px[2] > 200 && px[3] == 255));
    }
}

#[cfg(test)]
mod golden_tests {
    use super::*;

    fn goldens_dir(category: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/goldens").join(category)
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
        category: &str, name: &str, width: u32, height: u32, rgba_premul: &[u8],
    ) -> Option<String> {
        let mut pixmap = tiny_skia::Pixmap::new(width, height)
            .unwrap_or_else(|| panic!("golden {name}: {width}x{height} must be nonzero"));
        let len = pixmap.data().len();
        pixmap.data_mut().copy_from_slice(&rgba_premul[..len]);
        let png = pixmap.encode_png().unwrap_or_else(|e| panic!("golden {name}: failed to encode png: {e}"));

        let dir = goldens_dir(category);
        let path = dir.join(format!("{name}.png"));
        if let Ok(existing) = std::fs::read(&path) {
            if existing == png {
                return None;
            }
            let actual_path = dir.join(format!("{name}.actual.png"));
            std::fs::write(&actual_path, &png).unwrap_or_else(|e| panic!("golden {name}: failed to write actual: {e}"));
            return Some(format!(
                "golden mismatch for {name}: wrote {actual_path:?} for review; \
                 if the change is intentional, replace {path:?} with it"
            ));
        }
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("golden {name}: failed to create dir: {e}"));
        std::fs::write(&path, &png).unwrap_or_else(|e| panic!("golden {name}: failed to write baseline: {e}"));
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
        format!("tl{}_tr{}_br{}_bl{}", bit(corners[0]), bit(corners[1]), bit(corners[2]), bit(corners[3]))
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
        for (name, text) in [("draw_label_over_ascii", "Hello world"), ("draw_label_over_cyrillic", "Привіт світ")]
        {
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
            failures.extend(check_golden_rgba_premul("text", name, width as u32, height as u32, &buf));
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
                    ShadowLayerSpec { dy: 3.30, blur: 7.68, color: Color([0.0, 0.0, 0.0, 0.26]) },
                    ShadowLayerSpec { dy: 13.76, blur: 32.0, color: Color([0.0, 0.0, 0.0, 0.4]) },
                ],
                1.0,
                Color([1.0, 1.0, 1.0, 0.05]),
            ),
            (
                "window_shadow_focused",
                [
                    ShadowLayerSpec { dy: 7.02, blur: 16.32, color: Color([0.0, 0.0, 0.0, 0.39]) },
                    ShadowLayerSpec { dy: 29.24, blur: 68.0, color: Color([0.0, 0.0, 0.0, 0.6]) },
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
            failures.extend(check_golden_rgba_premul("chrome", name, buf_w as u32, buf_h as u32, &rgba));
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

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement};
use smithay::utils::{Logical, Point, Rectangle, Size};
use tiny_skia::{FillRule, Paint, PathBuilder, PixmapMut, PixmapRef, Transform as Affine};

use super::compose::Image;
use super::model::{Area, Pos};
use super::render::{self, Glyph, Palette, Piece};
use crate::i18n::Key;
use crate::paint::{Painter, TextAlign};
use crate::raster::{Canvas, rect};
use glide_theme::{Color, TextStyle};

pub const PREVIEW_TIMEOUT: Duration = Duration::from_secs(5);
pub const CARD_W: f64 = 196.0;
pub const CARD_H: f64 = 132.0;
pub const MARGIN: f64 = 24.0;
pub const DISMISS_SWIPE: f64 = 48.0;
pub const TRAVEL: f64 = 32.0;
pub const THUMB_PAD: f64 = 6.0;
pub const BTN_H: f64 = 24.0;
const TIP_H: i32 = 24;
const TIP_GAP: f64 = 6.0;

const DURATION_STYLE: TextStyle =
    TextStyle { size_px: 11.0, line_height_px: 14.0, weight: 600, tracking_em: 0.0, uppercase: false, tabular: true };

const ACTION_STYLE: TextStyle = TextStyle {
    size_px: 11.0,
    line_height_px: 14.0,
    weight: 560,
    tracking_em: -0.01,
    uppercase: false,
    tabular: false,
};

#[derive(Debug, Clone, PartialEq)]
pub enum PreviewKind {
    Screenshot { edit_snapshot: Option<Arc<Image>> },
    Recording { duration: Duration },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewAction {
    Open,
    Trim,
    ShowInFolder,
    Dismiss,
}

pub type PreviewControl = PreviewAction;

#[derive(Debug, Clone, PartialEq)]
pub enum PreviewOutcome {
    Nothing,
    Redraw,
    Activate { path: PathBuf, kind: PreviewKind },
    ShowInFolder(PathBuf),
    Dismiss,
}

pub struct Preview {
    pub image: Arc<Image>,
    pub path: PathBuf,
    pub kind: PreviewKind,
    pub hovered: bool,
    pub hover_control: Option<PreviewControl>,
    pub drag_origin: Option<Pos>,
    pub drag_offset_x: f32,
    pub dragging: bool,
    pub started: Instant,
    pub dismiss_at: Instant,
    pub closing: bool,
    remaining_on_hover: Option<Duration>,
}

impl Preview {
    #[must_use]
    pub fn screenshot(image: Arc<Image>, path: PathBuf, edit_snapshot: Option<Arc<Image>>) -> Self {
        let now = glide_ui::clock::now();
        Self {
            image,
            path,
            kind: PreviewKind::Screenshot { edit_snapshot },
            hovered: false,
            hover_control: None,
            drag_origin: None,
            drag_offset_x: 0.0,
            dragging: false,
            started: now,
            dismiss_at: now + PREVIEW_TIMEOUT,
            closing: false,
            remaining_on_hover: None,
        }
    }

    #[must_use]
    pub fn recording(image: Arc<Image>, path: PathBuf, duration: Duration) -> Self {
        let now = glide_ui::clock::now();
        Self {
            image,
            path,
            kind: PreviewKind::Recording { duration },
            hovered: false,
            hover_control: None,
            drag_origin: None,
            drag_offset_x: 0.0,
            dragging: false,
            started: now,
            dismiss_at: now + PREVIEW_TIMEOUT,
            closing: false,
            remaining_on_hover: None,
        }
    }

    #[must_use]
    pub fn card_rect(&self, work_area: Area) -> Area {
        let x = work_area.loc.x + work_area.size.w - MARGIN - CARD_W + f64::from(self.drag_offset_x);
        let y = work_area.loc.y + work_area.size.h - MARGIN - CARD_H;
        Rectangle::new(Point::from((x, y)), Size::from((CARD_W, CARD_H)))
    }

    #[must_use]
    pub fn action_button_rect(&self, card: Area) -> Area {
        let (w, h) = (54.0, BTN_H);
        let x = card.loc.x + card.size.w - THUMB_PAD - w;
        let y = card.loc.y + card.size.h - THUMB_PAD - h;
        Rectangle::new(Point::from((x, y)), Size::from((w, h)))
    }

    #[must_use]
    pub fn folder_button_rect(&self, card: Area) -> Area {
        let (w, h) = (26.0, BTN_H);
        let x = card.loc.x + card.size.w - THUMB_PAD - 54.0 - 4.0 - w;
        let y = card.loc.y + card.size.h - THUMB_PAD - h;
        Rectangle::new(Point::from((x, y)), Size::from((w, h)))
    }

    #[must_use]
    pub fn dismiss_button_rect(&self, card: Area) -> Area {
        let (w, h) = (20.0, 20.0);
        let x = card.loc.x + card.size.w - 10.0 - w;
        let y = card.loc.y - 6.0;
        Rectangle::new(Point::from((x, y)), Size::from((w, h)))
    }

    #[must_use]
    pub fn control_at(&self, pos: Pos, work_area: Area) -> Option<PreviewControl> {
        let card = self.card_rect(work_area);
        if self.dismiss_button_rect(card).contains(pos) {
            return Some(PreviewControl::Dismiss);
        }
        if !card.contains(pos) {
            return None;
        }
        if self.action_button_rect(card).contains(pos) {
            return Some(match self.kind {
                PreviewKind::Screenshot { .. } => PreviewControl::Open,
                PreviewKind::Recording { .. } => PreviewControl::Trim,
            });
        }
        if self.folder_button_rect(card).contains(pos) {
            return Some(PreviewControl::ShowInFolder);
        }
        None
    }

    pub fn motion(&mut self, pos: Pos, work_area: Area) -> PreviewOutcome {
        let card = self.card_rect(work_area);
        let is_over = card.contains(pos) || self.dismiss_button_rect(card).contains(pos);
        let hover_changed = self.hovered != is_over;
        if is_over && !self.hovered {
            self.hovered = true;
            let now = glide_ui::clock::now();
            let remaining = self.dismiss_at.saturating_duration_since(now);
            self.remaining_on_hover = Some(remaining);
        } else if !is_over && self.hovered && !self.dragging {
            self.hovered = false;
            let now = glide_ui::clock::now();
            if let Some(remaining) = self.remaining_on_hover.take() {
                self.dismiss_at = now + remaining;
            }
        }

        let new_control = if self.hovered { self.control_at(pos, work_area) } else { None };
        let control_changed = self.hover_control != new_control;
        self.hover_control = new_control;

        if let Some(origin) = self.drag_origin {
            let dx = (pos.x - origin.x) as f32;
            if dx > 0.0 {
                self.drag_offset_x = dx;
                self.dragging = dx >= 4.0;
                return PreviewOutcome::Redraw;
            }
        }

        if hover_changed || control_changed { PreviewOutcome::Redraw } else { PreviewOutcome::Nothing }
    }

    pub fn press(&mut self, pos: Pos, work_area: Area) -> PreviewOutcome {
        let card = self.card_rect(work_area);
        let is_over = card.contains(pos) || self.dismiss_button_rect(card).contains(pos);
        if !is_over {
            return PreviewOutcome::Nothing;
        }
        let control = self.control_at(pos, work_area);
        if control.is_none() {
            self.drag_origin = Some(pos);
        }
        PreviewOutcome::Redraw
    }

    pub fn release(&mut self, pos: Pos, work_area: Area) -> PreviewOutcome {
        let card = self.card_rect(work_area);
        let is_over = card.contains(pos) || self.dismiss_button_rect(card).contains(pos);
        self.drag_origin = None;

        if self.drag_offset_x >= DISMISS_SWIPE as f32 {
            self.closing = true;
            self.dragging = false;
            return PreviewOutcome::Dismiss;
        }

        if self.dragging {
            self.drag_offset_x = 0.0;
            self.dragging = false;
            if !is_over && self.hovered {
                self.hovered = false;
                let now = glide_ui::clock::now();
                if let Some(remaining) = self.remaining_on_hover.take() {
                    self.dismiss_at = now + remaining;
                }
            }
            return PreviewOutcome::Redraw;
        }

        self.drag_offset_x = 0.0;
        if !is_over {
            return PreviewOutcome::Nothing;
        }

        match self.control_at(pos, work_area) {
            Some(PreviewControl::Dismiss) => {
                self.closing = true;
                PreviewOutcome::Dismiss
            }
            Some(PreviewControl::ShowInFolder) => PreviewOutcome::ShowInFolder(self.path.clone()),
            Some(PreviewControl::Open | PreviewControl::Trim) => {
                PreviewOutcome::Activate { path: self.path.clone(), kind: self.kind.clone() }
            }
            None => PreviewOutcome::Activate { path: self.path.clone(), kind: self.kind.clone() },
        }
    }

    #[must_use]
    pub fn next_wake(&self) -> Option<Instant> {
        if self.hovered || self.dragging || self.closing {
            return None;
        }
        Some(self.dismiss_at)
    }

    #[must_use]
    pub fn is_expired(&self) -> bool {
        !self.hovered && !self.dragging && glide_ui::clock::now() >= self.dismiss_at
    }

    pub fn close(&mut self) {
        self.closing = true;
    }
}

pub struct PreviewCache {
    pub reveal: glide_ui::Tween,
    pub hover: glide_ui::HoverAnimation<PreviewControl>,
    pub hover_since: Option<Instant>,
    card: Option<KeyedBuffer>,
    tip: HashMap<i32, KeyedBuffer>,
    closing: bool,
}

struct KeyedBuffer {
    key: u64,
    buffer: MemoryRenderBuffer,
    size: (i32, i32),
}

impl Default for PreviewCache {
    fn default() -> Self {
        Self {
            reveal: glide_ui::Tween::at(0.0),
            hover: glide_ui::HoverAnimation::default(),
            hover_since: None,
            card: None,
            tip: HashMap::new(),
            closing: false,
        }
    }
}

impl PreviewCache {
    pub fn reveal(&mut self) {
        self.closing = false;
        self.reveal.jump(0.0);
        self.reveal.retarget(1.0, glide_theme::motion::t3(), glide_theme::motion::ease_out);
    }

    pub fn conceal(&mut self) {
        let from = self.openness();
        self.closing = true;
        self.reveal.jump(from);
        self.reveal.retarget(0.0, glide_theme::motion::t2(), glide_theme::motion::ease_io);
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let scene = if self.closing { glide_theme::motion::t2() } else { glide_theme::motion::t3() };
        self.reveal.is_animating(scene) || self.hover.is_animating(glide_theme::motion::t1())
    }

    #[must_use]
    pub fn openness(&self) -> f64 {
        let (duration, ease): (Duration, fn(f64) -> f64) = if self.closing {
            (glide_theme::motion::t2(), glide_theme::motion::ease_io)
        } else {
            (glide_theme::motion::t3(), glide_theme::motion::ease_out)
        };
        self.reveal.value(duration, ease).clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn next_wake(&self, preview: &Preview) -> Option<Instant> {
        let tip_wake = self.hover_since.and_then(|since| {
            let due = since + glide_theme::component::TOOLTIP_DELAY;
            (due > glide_ui::clock::now() && self.hover.target().is_some()).then_some(due)
        });
        match (tip_wake, preview.next_wake()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        }
    }
}

#[allow(clippy::too_many_arguments)] // Self-contained floating preview render pass
pub fn render_preview<R>(
    cache: &mut PreviewCache, preview: &Preview, renderer: &mut R, painter: &Painter, palette: &Palette,
    work_area: Area, scale: f64, theme_revision: u64, output_scale: i32,
) -> Vec<Piece<R>>
where
    R: crate::raster::ShellRenderer,
{
    let mut pieces = Vec::new();
    let openness = cache.openness();
    if openness <= 0.0 {
        return pieces;
    }

    let alpha = glide_theme::motion::assist(openness) as f32;
    let travel = (TRAVEL * (1.0 - openness)).round();
    let card_rect = preview.card_rect(work_area);
    let origin = Point::from((card_rect.loc.x + travel, card_rect.loc.y));

    let margin = render::bar_margin();

    let key = glide_theme::Revision::new()
        .u64(theme_revision)
        .bool(palette.appearance_dark)
        .bool(preview.hovered)
        .u64(match preview.hover_control {
            Some(PreviewControl::Open) => 1,
            Some(PreviewControl::Trim) => 2,
            Some(PreviewControl::ShowInFolder) => 3,
            Some(PreviewControl::Dismiss) => 4,
            None => 0,
        })
        .u64(match preview.kind {
            PreviewKind::Screenshot { .. } => 0,
            PreviewKind::Recording { duration } => duration.as_secs(),
        })
        .finish();

    if cache.card.as_ref().is_none_or(|c| c.key != key)
        && let Some(canvas) = draw_preview_card(preview, palette, painter, output_scale)
    {
        let size = (canvas.w, canvas.h);
        cache.card = Some(KeyedBuffer { key, buffer: canvas.finish(), size });
    }

    if let Some(card_buf) = &cache.card {
        let at = origin - Point::from((f64::from(margin), f64::from(margin)));
        pieces.extend(render_buffer_piece(renderer, &card_buf.buffer, at, alpha, scale));
    }

    if preview.hovered
        && let Some(control) = preview.hover_control
        && let Some(since) = cache.hover_since.map(glide_ui::clock::since)
        && since >= glide_theme::component::TOOLTIP_DELAY
    {
        let tip_key = match control {
            PreviewControl::Open => Key::ShotPreviewOpen,
            PreviewControl::Trim => Key::ShotPreviewTrim,
            PreviewControl::ShowInFolder => Key::ShotPreviewFolder,
            PreviewControl::Dismiss => Key::ShotPreviewDismiss,
        };
        let text = crate::i18n::t(tip_key);
        let tip_rev = glide_theme::Revision::new().u64(theme_revision).str(text).finish();
        let tip_entry = cache.tip.entry(output_scale).or_insert_with(|| KeyedBuffer {
            key: 0,
            buffer: MemoryRenderBuffer::default(),
            size: (0, 0),
        });
        if tip_entry.key != tip_rev
            && let Some(canvas) = draw_tip_canvas(text, palette, painter, output_scale)
        {
            let size = (canvas.w, canvas.h);
            *tip_entry = KeyedBuffer { key: tip_rev, buffer: canvas.finish(), size };
        }
        if tip_entry.key == tip_rev {
            let tip_w = f64::from(tip_entry.size.0);
            let tip_x = (origin.x + card_rect.size.w - tip_w - 4.0).max(work_area.loc.x + 4.0);
            let tip_y = origin.y - TIP_GAP - f64::from(TIP_H);
            let tip_alpha = alpha;
            pieces.extend(render_buffer_piece(
                renderer,
                &tip_entry.buffer,
                Point::from((tip_x, tip_y)),
                tip_alpha,
                scale,
            ));
        }
    }

    pieces
}

fn draw_preview_card(preview: &Preview, palette: &Palette, painter: &Painter, scale: i32) -> Option<Canvas<'static>> {
    let width = CARD_W as i32;
    let height = CARD_H as i32;
    let margin = render::bar_margin();
    let radius = glide_theme::shape::R3;
    let appearance =
        if palette.appearance_dark { glide_theme::Appearance::Dark } else { glide_theme::Appearance::Light };
    let layers = glide_theme::elevation::layers(glide_theme::elevation::Level::El3, appearance);
    let mut canvas = crate::raster::shadow((width, height), margin, radius, &layers, scale)?;

    let panel = rect(margin, margin, width, height);
    canvas.fill(panel, palette.panel, radius);
    canvas.stroke(panel, 1.0, radius, palette.line);

    let thumb_margin = THUMB_PAD as i32;
    let thumb_x = margin + thumb_margin;
    let thumb_y = margin + thumb_margin;
    let thumb_w = width - thumb_margin * 2;
    let thumb_h = height - thumb_margin * 2 - (BTN_H as i32 + 2);
    let thumb_rect = rect(thumb_x, thumb_y, thumb_w, thumb_h);
    canvas.fill(thumb_rect, palette.fill, glide_theme::shape::R2);

    let (recording_info, pill_x, pill_y) = match &preview.kind {
        PreviewKind::Recording { duration } => {
            let secs = duration.as_secs();
            let mm = secs / 60;
            let ss = secs % 60;
            let text = format!("{mm:02}:{ss:02}");
            let text_w = painter.text_width_styled(&text, &DURATION_STYLE).ceil() as i32;
            let pill_w = text_w + 22;
            let pill_h = 18;
            let px = margin + thumb_margin + 6;
            let py = margin + thumb_margin + thumb_h - pill_h - 6;
            let pill_rect = rect(px, py, pill_w, pill_h);
            canvas.fill(pill_rect, palette.inverse_bg.with_alpha(0.85), glide_theme::shape::R1);

            let text_rect = rect(px + 14, py + 1, pill_w - 16, pill_h - 2);
            canvas.text_in(painter, text_rect, TextAlign::Left, &text, palette.inverse_fg, &DURATION_STYLE);
            (true, px, py)
        }
        PreviewKind::Screenshot { .. } => (false, 0, 0),
    };

    let b_y = margin + height - thumb_margin - BTN_H as i32;

    let (act_key, act_is_hover) = match preview.kind {
        PreviewKind::Screenshot { .. } => (Key::ShotPreviewOpen, preview.hover_control == Some(PreviewControl::Open)),
        PreviewKind::Recording { .. } => (Key::ShotPreviewTrim, preview.hover_control == Some(PreviewControl::Trim)),
    };
    let act_w = 54;
    let act_x = margin + width - thumb_margin - act_w;
    let act_rect = rect(act_x, b_y, act_w, BTN_H as i32);
    let (act_fill, act_ink) =
        if act_is_hover { (palette.accent, palette.accent_fg) } else { (palette.soft, palette.soft_fg) };
    canvas.fill(act_rect, act_fill, glide_theme::shape::R2);
    canvas.text_in(painter, act_rect, TextAlign::Center, crate::i18n::t(act_key), act_ink, &ACTION_STYLE);

    let f_w = 26;
    let f_x = act_x - 4 - f_w;
    let f_rect = rect(f_x, b_y, f_w, BTN_H as i32);
    let f_is_hover = preview.hover_control == Some(PreviewControl::ShowInFolder);
    let (f_fill, f_ink) = if f_is_hover { (palette.soft, palette.soft_fg) } else { (palette.fill, palette.muted) };
    canvas.fill(f_rect, f_fill, glide_theme::shape::R2);

    let close_d = 20;
    let close_x = margin + width - 10 - close_d;
    let close_y = margin - 6;
    let close_rect = rect(close_x, close_y, close_d, close_d);
    let c_is_hover = preview.hover_control == Some(PreviewControl::Dismiss);
    let (c_fill, c_ink) =
        if c_is_hover { (palette.danger, Color([1.0, 1.0, 1.0, 1.0])) } else { (palette.panel, palette.muted) };
    canvas.fill(close_rect, c_fill, glide_theme::shape::R_ROUND);
    canvas.stroke(close_rect, 1.0, glide_theme::shape::R_ROUND, palette.line);

    let s = scale as f32;
    if let Some(mut pixmap) = pixmap_from_canvas(&mut canvas) {
        let thumb_box = (thumb_x as f32 * s, thumb_y as f32 * s, thumb_w as f32 * s, thumb_h as f32 * s);
        draw_thumbnail_into(&mut pixmap, &preview.image, thumb_box, glide_theme::shape::R2 * s);

        if recording_info {
            let triangle_at = (pill_x as f32 + 5.0, pill_y as f32 + 4.5, 9.0);
            draw_play_triangle(&mut pixmap, triangle_at, s, palette.inverse_fg);
        }

        let folder_glyph_at = (f_x as f32 + (f_w as f32 - 16.0) / 2.0, b_y as f32 + (BTN_H as f32 - 16.0) / 2.0, 16.0);
        render::draw_glyph(&mut pixmap, Glyph::Save, folder_glyph_at, f_ink, f_fill);

        let close_glyph_at = (close_x as f32 + 3.0, close_y as f32 + 3.0, 14.0);
        render::draw_glyph(&mut pixmap, Glyph::Close, close_glyph_at, c_ink, c_fill);
    }

    Some(canvas)
}

fn pixmap_from_canvas<'a>(canvas: &'a mut Canvas<'static>) -> Option<PixmapMut<'a>> {
    let (w, h) = ((canvas.w * canvas.scale) as u32, (canvas.h * canvas.scale) as u32);
    PixmapMut::from_bytes(&mut canvas.buf, w, h)
}

fn draw_thumbnail_into(pixmap: &mut PixmapMut<'_>, image: &Image, (dx, dy, dw, dh): (f32, f32, f32, f32), radius: f32) {
    let Some(src_ref) = PixmapRef::from_bytes(&image.data, image.width, image.height) else { return };
    let sx = dw / image.width as f32;
    let sy = dh / image.height as f32;
    let scale = sx.max(sy);
    let offset_x = dx + (dw - image.width as f32 * scale) / 2.0;
    let offset_y = dy + (dh - image.height as f32 * scale) / 2.0;
    let transform = Affine::from_scale(scale, scale).post_translate(offset_x, offset_y);

    let mut clip_path = PathBuilder::new();
    clip_path.move_to(dx + radius, dy);
    clip_path.line_to(dx + dw - radius, dy);
    clip_path.quad_to(dx + dw, dy, dx + dw, dy + radius);
    clip_path.line_to(dx + dw, dy + dh - radius);
    clip_path.quad_to(dx + dw, dy + dh, dx + dw - radius, dy + dh);
    clip_path.line_to(dx + radius, dy + dh);
    clip_path.quad_to(dx, dy + dh, dx, dy + dh - radius);
    clip_path.line_to(dx, dy + radius);
    clip_path.quad_to(dx, dy, dx + radius, dy);
    clip_path.close();

    let Some(clip) = clip_path.finish() else { return };

    let paint = Paint {
        anti_alias: true,
        shader: tiny_skia::Pattern::new(
            src_ref,
            tiny_skia::SpreadMode::Pad,
            tiny_skia::FilterQuality::Bilinear,
            1.0,
            transform,
        ),
        ..Default::default()
    };
    pixmap.fill_path(&clip, &paint, FillRule::Winding, Affine::identity(), None);
}

fn draw_play_triangle(pixmap: &mut PixmapMut<'_>, (pos_x, pos_y, size): (f32, f32, f32), scale: f32, color: Color) {
    let (px, py, psize) = (pos_x * scale, pos_y * scale, size * scale);
    let mut builder = PathBuilder::new();
    builder.move_to(px, py);
    builder.line_to(px + psize, py + psize / 2.0);
    builder.line_to(px, py + psize);
    builder.close();
    if let Some(path) = builder.finish() {
        let [red, green, blue, alpha] = color.0;
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            (blue * 255.0).round() as u8,
            (green * 255.0).round() as u8,
            (red * 255.0).round() as u8,
            (alpha * 255.0).round() as u8,
        );
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, FillRule::Winding, Affine::identity(), None);
    }
}

fn draw_tip_canvas(text: &str, palette: &Palette, painter: &Painter, scale: i32) -> Option<Canvas<'static>> {
    let width = painter.text_width_styled(text, &TextStyle::CAP).ceil() as i32 + 16;
    let mut canvas =
        Canvas::popover((width, TIP_H), scale, glide_theme::shape::R2, palette.inverse_bg, palette.inverse_bg)?;
    canvas.text_in(painter, canvas.full(), TextAlign::Center, text, palette.inverse_fg, &TextStyle::CAP);
    Some(canvas)
}

fn render_buffer_piece<R>(
    renderer: &mut R, buffer: &MemoryRenderBuffer, at: Point<f64, Logical>, alpha: f32, scale: f64,
) -> Option<Piece<R>>
where
    R: crate::raster::ShellRenderer,
{
    let loc = at.to_physical(scale);
    MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        Point::from((loc.x.round(), loc.y.round())),
        buffer,
        Some(alpha),
        None,
        None,
        Kind::Unspecified,
    )
    .ok()
    .map(Piece::Memory)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_image() -> Arc<Image> {
        let w = 80;
        let h = 60;
        let mut data = vec![255u8; w * h * 4];
        for (i, chunk) in data.chunks_exact_mut(4).enumerate() {
            chunk[0] = (i % 255) as u8;
            chunk[1] = 128;
            chunk[2] = 200;
            chunk[3] = 255;
        }
        Arc::new(Image { width: w as u32, height: h as u32, data })
    }

    #[test]
    fn preview_layout_and_card_rect() {
        let work_area = Rectangle::new(Point::from((0.0, 0.0)), Size::from((1920.0, 1080.0)));
        let preview = Preview::screenshot(sample_image(), PathBuf::from("/tmp/shot.png"), None);
        let card = preview.card_rect(work_area);
        assert_eq!(card.size.w, CARD_W);
        assert_eq!(card.size.h, CARD_H);
        assert_eq!(card.loc.x, 1920.0 - MARGIN - CARD_W);
        assert_eq!(card.loc.y, 1080.0 - MARGIN - CARD_H);
    }

    #[test]
    fn hover_timer_pause_and_wake() {
        let work_area = Rectangle::new(Point::from((0.0, 0.0)), Size::from((1920.0, 1080.0)));
        let mut preview = Preview::screenshot(sample_image(), PathBuf::from("/tmp/shot.png"), None);
        assert!(preview.next_wake().is_some());

        let card = preview.card_rect(work_area);
        let inside = Point::from((card.loc.x + 10.0, card.loc.y + 10.0));
        let outcome = preview.motion(inside, work_area);
        assert_eq!(outcome, PreviewOutcome::Redraw);
        assert!(preview.hovered);
        assert_eq!(preview.next_wake(), None);

        let outside = Point::from((card.loc.x - 50.0, card.loc.y - 50.0));
        let outcome = preview.motion(outside, work_area);
        assert_eq!(outcome, PreviewOutcome::Redraw);
        assert!(!preview.hovered);
        assert!(preview.next_wake().is_some());
    }

    #[test]
    fn swipe_right_dismiss() {
        let work_area = Rectangle::new(Point::from((0.0, 0.0)), Size::from((1920.0, 1080.0)));
        let mut preview = Preview::screenshot(sample_image(), PathBuf::from("/tmp/shot.png"), None);
        let card = preview.card_rect(work_area);
        let start = Point::from((card.loc.x + 20.0, card.loc.y + 20.0));

        assert_eq!(preview.press(start, work_area), PreviewOutcome::Redraw);
        let swipe = Point::from((start.x + DISMISS_SWIPE + 10.0, start.y));
        assert_eq!(preview.motion(swipe, work_area), PreviewOutcome::Redraw);
        assert!(preview.dragging);
        assert_eq!(preview.release(swipe, work_area), PreviewOutcome::Dismiss);
        assert!(preview.closing);
    }

    #[test]
    fn activate_on_click() {
        let work_area = Rectangle::new(Point::from((0.0, 0.0)), Size::from((1920.0, 1080.0)));
        let mut preview = Preview::recording(sample_image(), PathBuf::from("/tmp/rec.mp4"), Duration::from_secs(12));
        let card = preview.card_rect(work_area);
        let thumb_point = Point::from((card.loc.x + 20.0, card.loc.y + 20.0));

        assert_eq!(preview.press(thumb_point, work_area), PreviewOutcome::Redraw);
        let outcome = preview.release(thumb_point, work_area);
        assert_eq!(
            outcome,
            PreviewOutcome::Activate {
                path: PathBuf::from("/tmp/rec.mp4"),
                kind: PreviewKind::Recording { duration: Duration::from_secs(12) },
            }
        );
    }

    #[test]
    fn show_in_folder_button_click() {
        let work_area = Rectangle::new(Point::from((0.0, 0.0)), Size::from((1920.0, 1080.0)));
        let mut preview = Preview::screenshot(sample_image(), PathBuf::from("/tmp/shot.png"), None);
        let card = preview.card_rect(work_area);
        let f_btn = preview.folder_button_rect(card);
        let btn_center = Point::from((f_btn.loc.x + f_btn.size.w / 2.0, f_btn.loc.y + f_btn.size.h / 2.0));

        preview.motion(btn_center, work_area);
        assert_eq!(preview.hover_control, Some(PreviewControl::ShowInFolder));
        preview.press(btn_center, work_area);
        let outcome = preview.release(btn_center, work_area);
        assert_eq!(outcome, PreviewOutcome::ShowInFolder(PathBuf::from("/tmp/shot.png")));
    }

    #[test]
    fn preview_cache_motion_and_openness() {
        let mut cache = PreviewCache::default();
        assert_eq!(cache.openness(), 0.0);
        cache.reveal();
        assert!(cache.is_animating());
        cache.reveal.jump(1.0);
        assert_eq!(cache.openness(), 1.0);
        cache.conceal();
        assert!(cache.is_animating());
    }
}

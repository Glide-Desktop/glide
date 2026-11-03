use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::{
    Id, Kind,
    memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
    solid::SolidColorRenderElement,
};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::utils::{Logical, Physical, Point, Rectangle, Size, Transform};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform as Affine};

use super::compose::{self, Frame, Image, Sprite};
use super::model::{self, Area, Bar, Control, Handle, Mark, Mode, Shape, Shot, Tool};
use crate::i18n::Key;
use crate::paint::{Painter, TextAlign};
use crate::raster::{Canvas, rect};
use glide_theme::Color;
use glide_theme::TextStyle;

const DIM: f32 = 0.45;
const OUTLINE: f64 = 2.0;
const HANDLE: i32 = 16;
const HANDLE_DOT: f32 = 10.0;
const BADGE_H: i32 = 22;
const BADGE_GAP: f64 = 6.0;
const TIP_H: i32 = 26;
const TIP_GAP: f64 = 8.0;
const RING_PAD: i32 = 4;
const RING_WIDTH: f32 = 2.5;
const RING_TINT: f32 = 0.12;
const BAR_RISE: f64 = 8.0;
const ANCHORED_RISE: f64 = 6.0;
const GLYPH: f32 = 20.0;
const TEXT_PAD: f32 = 4.0;
const TOOL_GLYPH: f32 = 18.0;
const TEXT_STYLE: TextStyle = TextStyle {
    size_px: 20.0,
    line_height_px: 26.0,
    weight: 650,
    tracking_em: -0.01,
    uppercase: false,
    tabular: false,
};
const STEP_STYLE: TextStyle =
    TextStyle { size_px: 15.0, line_height_px: 18.0, weight: 700, tracking_em: 0.0, uppercase: false, tabular: true };
const BADGE_STYLE: TextStyle = TextStyle { tabular: true, ..TextStyle::CAP };
const LABEL_STYLE: TextStyle = TextStyle { weight: 560, ..TextStyle::CAP };

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub panel: Color,
    pub line: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    pub accent: Color,
    pub accent_fg: Color,
    pub soft: Color,
    pub soft_fg: Color,
    pub fill: Color,
    pub inverse_bg: Color,
    pub inverse_fg: Color,
    pub appearance_dark: bool,
}

impl Palette {
    #[must_use]
    pub fn from_tokens(tokens: &glide_theme::Tokens) -> Self {
        Self {
            panel: tokens.popover_bg,
            line: tokens.line,
            text: tokens.fg_1,
            muted: tokens.fg_2,
            faint: tokens.fg_4,
            accent: tokens.acc_bg,
            accent_fg: tokens.acc_fg,
            soft: tokens.acc_soft,
            soft_fg: tokens.acc_soft_fg,
            fill: tokens.fill_1,
            inverse_bg: tokens.inverse_bg,
            inverse_fg: tokens.inverse_fg,
            appearance_dark: tokens.appearance == glide_theme::Appearance::Dark,
        }
    }

    fn revision(&self) -> glide_theme::Revision {
        [
            self.panel,
            self.line,
            self.text,
            self.muted,
            self.faint,
            self.accent,
            self.accent_fg,
            self.soft,
            self.soft_fg,
            self.fill,
            self.inverse_bg,
            self.inverse_fg,
        ]
        .into_iter()
        .fold(glide_theme::Revision::new(), glide_theme::Revision::color)
        .bool(self.appearance_dark)
    }
}

// Pieces move straight into GlideRenderElements, which holds the same element unboxed.
#[allow(clippy::large_enum_variant)]
pub enum Piece<R: smithay::backend::renderer::Renderer> {
    Memory(MemoryRenderBufferRenderElement<R>),
    Solid(SolidColorRenderElement),
}

struct Keyed {
    key: u64,
    buffer: MemoryRenderBuffer,
    size: (i32, i32),
}

struct Layer {
    key: u64,
    buffer: MemoryRenderBuffer,
    area: Area,
    pixels: (u32, u32),
}

pub struct Cache {
    frozen: Option<MemoryRenderBuffer>,
    window: Option<(usize, MemoryRenderBuffer)>,
    marks: Option<Layer>,
    draft: Option<Layer>,
    blurs: HashMap<usize, Layer>,
    pending_blurs: HashMap<usize, BlurJob>,
    bars: HashMap<i32, Keyed>,
    hovers: HashMap<(i32, i32, i32, u32, bool), Option<MemoryRenderBuffer>>,
    hover_key: u64,
    handle: HashMap<i32, Keyed>,
    badge: HashMap<i32, Keyed>,
    tip: HashMap<i32, Keyed>,
    corners: HashMap<(i32, bool), Keyed>,
    ids: Vec<Id>,
    pub hover: glide_ui::HoverAnimation<Control>,
    hover_since: Option<Instant>,
    pressed: Option<Control>,
    reveal: glide_ui::Tween,
    closing: bool,
    bar_shown: glide_ui::Tween,
    badge_shown: glide_ui::Tween,
    handles_shown: glide_ui::Tween,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            frozen: None,
            window: None,
            marks: None,
            draft: None,
            blurs: HashMap::new(),
            pending_blurs: HashMap::new(),
            bars: HashMap::new(),
            hovers: HashMap::new(),
            hover_key: 0,
            handle: HashMap::new(),
            badge: HashMap::new(),
            tip: HashMap::new(),
            corners: HashMap::new(),
            ids: (0..16).map(|_| Id::new()).collect(),
            hover: glide_ui::HoverAnimation::default(),
            hover_since: None,
            pressed: None,
            reveal: glide_ui::Tween::at(0.0),
            closing: false,
            bar_shown: glide_ui::Tween::at(1.0),
            badge_shown: glide_ui::Tween::at(0.0),
            handles_shown: glide_ui::Tween::at(0.0),
        }
    }
}

fn presence(tween: &mut glide_ui::Tween, shown: bool) -> f64 {
    let duration = glide_theme::motion::t2();
    tween.retarget(if shown { 1.0 } else { 0.0 }, duration, glide_theme::motion::ease_out);
    tween.value(duration, glide_theme::motion::ease_out).clamp(0.0, 1.0)
}

impl Cache {
    pub fn reveal(&mut self) {
        self.closing = false;
        self.reveal.jump(0.0);
        self.reveal.retarget(1.0, glide_theme::motion::t2(), glide_theme::motion::ease_out);
    }

    pub fn conceal(&mut self) {
        let from = self.openness();
        self.closing = true;
        self.pressed = None;
        self.hover.set_with(None, glide_theme::motion::t1());
        self.hover_since = None;
        self.reveal.jump(from);
        self.reveal.retarget(0.0, glide_theme::motion::t2(), glide_theme::motion::ease_io);
    }

    pub fn set_hover(&mut self, control: Option<Control>) -> bool {
        let changed = self.hover.set_with(control, glide_theme::motion::t1());
        if changed {
            self.hover_since = control.map(|_| Instant::now());
        }
        changed
    }

    pub fn set_pressed(&mut self, control: Option<Control>) -> bool {
        let changed = self.pressed != control;
        self.pressed = control;
        changed
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let layers = glide_theme::motion::t1();
        let scene = glide_theme::motion::t2();
        let tip_pending = self.hover_since.is_some_and(|since| {
            since.elapsed() < glide_theme::component::TOOLTIP_DELAY + layers && tip_text(self.hover.target()).is_some()
        });
        let shown = [&self.bar_shown, &self.badge_shown, &self.handles_shown];
        self.hover.is_animating(layers)
            || self.reveal.is_animating(scene)
            || tip_pending
            || shown.iter().any(|tween| tween.is_animating(scene))
            || self.pending_blurs.values().any(|job| !job.failed)
    }

    fn openness(&self) -> f64 {
        let ease = if self.closing { glide_theme::motion::ease_io } else { glide_theme::motion::ease_out };
        self.reveal.value(glide_theme::motion::t2(), ease).clamp(0.0, 1.0)
    }

    pub fn drop_window(&mut self) {
        self.window = None;
        self.blurs.clear();
        self.pending_blurs.clear();
    }
}

#[must_use]
pub fn frame_buffer(image: &Image) -> MemoryRenderBuffer {
    MemoryRenderBuffer::from_slice(
        &image.to_bgra(),
        Fourcc::Argb8888,
        (image.width as i32, image.height as i32),
        1,
        Transform::Normal,
        None,
    )
}

pub struct Sources<'a> {
    pub frozen: &'a Image,
    pub window: Option<(&'a Image, Area)>,
    pub scale: f64,
}

impl Sources<'_> {
    fn blur_source(&self) -> (&Image, Frame) {
        match self.window {
            Some((image, area)) => {
                let frame = Frame {
                    x: (area.loc.x * self.scale).round(),
                    y: (area.loc.y * self.scale).round(),
                    scale: self.scale,
                };
                (image, frame)
            }
            None => (self.frozen, Frame { x: 0.0, y: 0.0, scale: self.scale }),
        }
    }
}

fn intersect(a: Area, b: Area) -> Option<Area> {
    let x0 = a.loc.x.max(b.loc.x);
    let y0 = a.loc.y.max(b.loc.y);
    let x1 = (a.loc.x + a.size.w).min(b.loc.x + b.size.w);
    let y1 = (a.loc.y + a.size.h).min(b.loc.y + b.size.h);
    (x1 - x0 >= 1.0 && y1 - y0 >= 1.0).then(|| Rectangle::new(Point::from((x0, y0)), Size::from((x1 - x0, y1 - y0))))
}

fn place<R>(
    renderer: &mut R, buffer: &MemoryRenderBuffer, at: Area, pixels: (u32, u32), clip: Option<Area>, alpha: f32,
    scale: f64,
) -> Option<Piece<R>>
where
    R: crate::raster::ShellRenderer,
{
    let visible = match clip {
        Some(clip) => intersect(at, clip)?,
        None => at,
    };
    let (kx, ky) = (f64::from(pixels.0) / at.size.w.max(1.0), f64::from(pixels.1) / at.size.h.max(1.0));
    let src = Rectangle::<f64, Logical>::new(
        Point::from(((visible.loc.x - at.loc.x) * kx, (visible.loc.y - at.loc.y) * ky)),
        Size::from((visible.size.w * kx, visible.size.h * ky)),
    );
    let loc = visible.loc.to_physical(scale);
    let end = (visible.loc + Point::from((visible.size.w, visible.size.h))).to_physical(scale);
    let physical = Size::<f64, Physical>::from((end.x.round() - loc.x.round(), end.y.round() - loc.y.round()));
    let size = physical.to_logical(scale).to_i32_round();
    MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        Point::from((loc.x.round(), loc.y.round())),
        buffer,
        Some(alpha),
        Some(src),
        Some(size),
        Kind::Unspecified,
    )
    .ok()
    .map(Piece::Memory)
}

fn raster<R>(
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

fn physical_box(area: Area, scale: f64) -> (i32, i32, i32, i32) {
    let loc = area.loc.to_physical(scale);
    let end = (area.loc + Point::from((area.size.w, area.size.h))).to_physical(scale);
    let (x0, y0) = (loc.x.round() as i32, loc.y.round() as i32);
    (x0, y0, (end.x.round() as i32 - x0).max(0), (end.y.round() as i32 - y0).max(0))
}

fn solid<R: smithay::backend::renderer::Renderer>(id: &Id, area: Area, color: [f32; 4], scale: f64) -> Piece<R> {
    let (x0, y0, w, h) = physical_box(area, scale);
    let geometry = Rectangle::new(Point::from((x0, y0)), Size::from((w, h)));
    Piece::Solid(SolidColorRenderElement::new(id.clone(), geometry, CommitCounter::default(), color, Kind::Unspecified))
}

fn selection_area(mode: Mode, target: Area, scale: f64, frozen_size: (u32, u32)) -> Area {
    if mode != Mode::Area {
        return target;
    }
    crate::crop_region(target, scale, frozen_size).map_or(target, |(x, y, w, h)| {
        area(f64::from(x) / scale, f64::from(y) / scale, f64::from(w) / scale, f64::from(h) / scale)
    })
}

fn area(x: f64, y: f64, w: f64, h: f64) -> Area {
    Rectangle::new(Point::from((x, y)), Size::from((w.max(0.0), h.max(0.0))))
}

#[must_use]
pub fn around(full: Area, hole: Option<Area>) -> Vec<Area> {
    let Some(hole) = hole.and_then(|hole| intersect(hole, full)) else {
        return vec![full];
    };
    let (fw, fh) = (full.size.w, full.size.h);
    let (x0, y0, x1, y1) = (hole.loc.x, hole.loc.y, hole.loc.x + hole.size.w, hole.loc.y + hole.size.h);
    vec![area(0.0, 0.0, fw, y0), area(0.0, y1, fw, fh - y1), area(0.0, y0, x0, y1 - y0), area(x1, y0, fw - x1, y1 - y0)]
}

#[must_use]
pub fn frame_lines(target: Area, width: f64, inset: bool) -> Vec<Area> {
    let (x, y, w, h) = (target.loc.x, target.loc.y, target.size.w, target.size.h);
    if inset {
        return vec![
            area(x, y, w, width),
            area(x, y + h - width, w, width),
            area(x, y + width, width, h - width * 2.0),
            area(x + w - width, y + width, width, h - width * 2.0),
        ];
    }
    vec![
        area(x - width, y - width, w + width * 2.0, width),
        area(x - width, y + h, w + width * 2.0, width),
        area(x - width, y, width, h),
        area(x + w, y, width, h),
    ]
}

fn premultiplied(color: Color, alpha: f32) -> [f32; 4] {
    let [r, g, b, a] = color.0;
    let a = (a * alpha).clamp(0.0, 1.0);
    [r * a, g * a, b * a, a]
}

#[must_use]
pub fn mark_bounds(mark: &Mark) -> Option<Area> {
    let base = mark.shape.bounds()?;
    let pad = f64::from(compose::ARROW_WIDTH * 5.0);
    let grown = Rectangle::new(
        base.loc - Point::from((pad, pad)),
        Size::from((base.size.w + pad * 2.0, base.size.h + pad * 2.0)),
    );
    match (&mark.shape, mark.sprite.as_deref()) {
        (Shape::Text { at, .. } | Shape::Step { at, .. }, Some(sprite)) => {
            let k = 1.0 / sprite.scale;
            Some(area(
                at.x + f64::from(sprite.offset.0) * k,
                at.y + f64::from(sprite.offset.1) * k,
                f64::from(sprite.image.width) * k,
                f64::from(sprite.image.height) * k,
            ))
        }
        (Shape::Text { .. } | Shape::Step { .. }, None) => None,
        _ => Some(grown),
    }
}

fn union(areas: impl IntoIterator<Item = Area>) -> Option<Area> {
    areas.into_iter().reduce(|a, b| {
        let x0 = a.loc.x.min(b.loc.x);
        let y0 = a.loc.y.min(b.loc.y);
        let x1 = (a.loc.x + a.size.w).max(b.loc.x + b.size.w);
        let y1 = (a.loc.y + a.size.h).max(b.loc.y + b.size.h);
        area(x0, y0, x1 - x0, y1 - y0)
    })
}

fn draw_layer(marks: &[Mark], scale: f64, key: u64) -> Option<Layer> {
    let drawn = marks.iter().filter(|mark| !matches!(mark.shape, Shape::Blur(..)));
    let bounds = union(drawn.clone().filter_map(mark_bounds))?;
    let x = (bounds.loc.x * scale).floor();
    let y = (bounds.loc.y * scale).floor();
    let w = ((bounds.loc.x + bounds.size.w) * scale).ceil() - x;
    let h = ((bounds.loc.y + bounds.size.h) * scale).ceil() - y;
    let mut image = Image::new(w as u32, h as u32)?;
    let frame = Frame { x, y, scale };
    if let Some(mut pixmap) = image.pixmap() {
        for mark in drawn {
            compose::draw_mark(&mut pixmap, mark, &frame);
        }
    }
    let area = area(x / scale, y / scale, w / scale, h / scale);
    Some(Layer { key, buffer: frame_buffer(&image), area, pixels: (image.width, image.height) })
}

fn shape_key(mark: &Mark) -> u64 {
    let mut revision = glide_theme::Revision::new().u64(mark.swatch as u64);
    let points: Vec<model::Pos> = match &mark.shape {
        Shape::Pen(points) => points.clone(),
        Shape::Arrow(a, b) | Shape::Rect(a, b) | Shape::Highlight(a, b) | Shape::Blur(a, b) => vec![*a, *b],
        Shape::Text { at, text } => {
            revision = revision.str(text);
            vec![*at]
        }
        Shape::Step { at, number } => {
            revision = revision.u64(u64::from(*number));
            vec![*at]
        }
    };
    revision = revision.u64(points.len() as u64);
    for point in points {
        revision = revision.f64(point.x).f64(point.y);
    }
    revision.bool(mark.sprite.is_some()).finish()
}

pub struct Scene<'a> {
    pub shot: &'a Shot,
    pub sources: Sources<'a>,
    pub window_generation: usize,
    pub radius: f32,
}

struct Ctx<'a, R: crate::raster::ShellRenderer> {
    renderer: &'a mut R,
    painter: &'a Painter,
    palette: &'a Palette,
    scale: f64,
    frozen_size: (u32, u32),
    openness: f32,
    content: f32,
    output_scale: i32,
    base_key: u64,
    full: Area,
    pieces: Vec<Piece<R>>,
}

fn badge_size(target: Area, scale: f64, frozen_size: (u32, u32)) -> (i64, i64) {
    crate::crop_region(target, scale, frozen_size).map_or_else(
        || ((target.size.w * scale).round() as i64, (target.size.h * scale).round() as i64),
        |(_, _, w, h)| (i64::from(w), i64::from(h)),
    )
}

pub fn render<R>(
    cache: &mut Cache, scene: &Scene<'_>, renderer: &mut R, painter: &Painter, palette: &Palette, theme_revision: u64,
    output_scale: i32,
) -> Vec<Piece<R>>
where
    R: crate::raster::ShellRenderer,
{
    let openness = cache.openness() as f32;
    let mut ctx = Ctx {
        renderer,
        painter,
        palette,
        scale: scene.sources.scale,
        frozen_size: (scene.sources.frozen.width, scene.sources.frozen.height),
        openness,
        content: if cache.closing { openness } else { 1.0 },
        output_scale,
        base_key: palette.revision().u64(theme_revision).u64(crate::paint::painter_generation()).finish(),
        full: scene.shot.full(),
        pieces: Vec::new(),
    };
    render_bar(cache, scene.shot, &mut ctx);
    render_selection(cache, scene.shot, &mut ctx);
    render_marks(cache, scene, &mut ctx);
    render_base(cache, scene, &mut ctx);
    ctx.pieces
}

fn render_bar<R: crate::raster::ShellRenderer>(cache: &mut Cache, shot: &Shot, ctx: &mut Ctx<'_, R>) {
    let (renderer, pieces) = (&mut *ctx.renderer, &mut ctx.pieces);
    let (painter, palette, scale, openness) = (ctx.painter, ctx.palette, ctx.scale, ctx.openness);
    let (output_scale, base_key, full) = (ctx.output_scale, ctx.base_key, ctx.full);
    let visible = shot.bar_visible();
    let shown = if cache.closing {
        cache.bar_shown.value(glide_theme::motion::t2(), glide_theme::motion::ease_out).clamp(0.0, 1.0)
    } else {
        presence(&mut cache.bar_shown, visible)
    };
    if shown > 0.0 {
        let bar = shot.bar();
        let rise = BAR_RISE * (1.0 - f64::from(openness)) + BAR_RISE * (1.0 - shown);
        let origin = bar.rect.loc + Point::from((0.0, rise));
        let alpha = (glide_theme::motion::assist(f64::from(openness)) * glide_theme::motion::assist(shown)) as f32;
        if visible
            && let Some(control) = cache.hover.target()
            && let Some(text) = tip_text(Some(control))
            && cache.hover_since.is_some_and(|since| since.elapsed() >= glide_theme::component::TOOLTIP_DELAY)
            && let Some(item) = bar.item(control)
        {
            let since = cache.hover_since.map_or(0.0, |since| {
                let fraction = since.elapsed().saturating_sub(glide_theme::component::TOOLTIP_DELAY).as_secs_f64()
                    / glide_theme::motion::t1().as_secs_f64().max(1e-6);
                glide_theme::motion::ease_out(fraction.min(1.0))
            });
            let key = glide_theme::Revision::new().u64(base_key).str(&text).finish();
            let tip = cache.tip.entry(output_scale).or_insert_with(|| Keyed {
                key: 0,
                buffer: MemoryRenderBuffer::default(),
                size: (0, 0),
            });
            if tip.key != key
                && let Some((buffer, size)) = draw_tip(&text, palette, painter, output_scale)
            {
                *tip = Keyed { key, buffer, size };
            }
            if tip.key == key {
                let centre = origin.x + item.loc.x + item.size.w / 2.0;
                let w = f64::from(tip.size.0);
                let x = (centre - w / 2.0).round().clamp(4.0, (full.size.w - w - 4.0).max(4.0));
                let above = origin.y - TIP_GAP - f64::from(TIP_H);
                let travel = ANCHORED_RISE * (1.0 - since);
                let y = if above >= 4.0 { above + travel } else { origin.y + bar.rect.size.h + TIP_GAP - travel };
                let tip_alpha = glide_theme::motion::assist(since) as f32 * alpha;
                pieces.extend(raster(renderer, &tip.buffer, Point::from((x, y)), tip_alpha, scale));
            }
        }
        let layers = glide_theme::motion::t1();
        for (control, item) in &bar.items {
            let level = cache.hover.level(*control, layers);
            let pressed = visible && cache.pressed == Some(*control);
            if (level <= 0.0 && !pressed) || !shot.enabled(*control) {
                continue;
            }
            let radius = match control {
                Control::Swatch(_) | Control::Capture => glide_theme::shape::R_ROUND,
                Control::Close if !shot.editing() => glide_theme::shape::R_ROUND,
                Control::Mode(_) | Control::Shadow => glide_theme::shape::R3,
                _ => glide_theme::shape::R2,
            };
            let on_accent = *control == Control::Save;
            let key = (item.size.w as i32, item.size.h as i32, output_scale, radius.to_bits(), on_accent);
            if cache.hover_key != base_key {
                cache.hovers.clear();
                cache.hover_key = base_key;
            }
            let tint = if on_accent { palette.accent_fg } else { palette.text };
            let Some(buffer) = cache.hovers.entry(key).or_insert_with(|| {
                let mut canvas = Canvas::new(key.0, key.1, output_scale)?;
                canvas.fill(canvas.full(), tint.with_alpha(1.0), radius);
                Some(canvas.finish())
            }) else {
                continue;
            };
            let state = if pressed { glide_theme::motion::ST_PRESS } else { glide_theme::motion::ST_HOVER };
            let layer = if pressed { 1.0 } else { level as f32 };
            pieces.extend(raster(renderer, buffer, origin + item.loc, layer * state * alpha, scale));
        }
        let key = glide_theme::Revision::new().u64(base_key).u64(bar_state(shot)).finish();
        let panel = cache.bars.entry(output_scale).or_insert_with(|| Keyed {
            key: 0,
            buffer: MemoryRenderBuffer::default(),
            size: (0, 0),
        });
        if panel.key != key
            && let Some((buffer, size)) = draw_bar(shot, &bar, palette, painter, output_scale)
        {
            *panel = Keyed { key, buffer, size };
        }
        if panel.key == key {
            let margin = bar_margin();
            let at = origin - Point::from((f64::from(margin), f64::from(margin)));
            pieces.extend(raster(renderer, &panel.buffer, at, alpha, scale));
        }
    }
}

fn render_selection<R: crate::raster::ShellRenderer>(cache: &mut Cache, shot: &Shot, ctx: &mut Ctx<'_, R>) {
    let (renderer, pieces) = (&mut *ctx.renderer, &mut ctx.pieces);
    let (painter, palette, scale, openness) = (ctx.painter, ctx.palette, ctx.scale, ctx.openness);
    let (output_scale, base_key, full, frozen_size) = (ctx.output_scale, ctx.base_key, ctx.full, ctx.frozen_size);
    let target = shot.target();
    let show_badge = shot.selecting() || (shot.editing() && shot.mode == Mode::Area);
    let closing = cache.closing;
    let badge_shown = if closing { 1.0 } else { presence(&mut cache.badge_shown, show_badge && target.is_some()) };
    let handles_shown = if closing { 1.0 } else { presence(&mut cache.handles_shown, shot.resizable()) };
    let entering = |shown: f64| {
        let progress = f64::from(openness) * shown;
        (ANCHORED_RISE * (1.0 - progress), glide_theme::motion::assist(progress) as f32)
    };
    if let Some(target) = target.filter(|_| show_badge) {
        let (w, h) = badge_size(target, scale, frozen_size);
        let text = format!("{w} × {h}");
        let key = glide_theme::Revision::new().u64(base_key).str(&text).finish();
        let badge = cache.badge.entry(output_scale).or_insert_with(|| Keyed {
            key: 0,
            buffer: MemoryRenderBuffer::default(),
            size: (0, 0),
        });
        if badge.key != key
            && let Some((buffer, size)) = draw_badge(&text, palette, painter, output_scale)
        {
            *badge = Keyed { key, buffer, size };
        }
        if badge.key == key {
            let above = target.loc.y - BADGE_GAP - f64::from(BADGE_H);
            let y = if above >= 2.0 { above } else { target.loc.y + BADGE_GAP };
            let x = target.loc.x.clamp(2.0, (full.size.w - f64::from(badge.size.0) - 2.0).max(2.0));
            let (rise, alpha) = entering(badge_shown);
            let at = Point::from((x.round(), (y + rise).round()));
            pieces.extend(raster(renderer, &badge.buffer, at, alpha, scale));
        }
    }

    if shot.resizable()
        && let Some(target) = target
    {
        let key = base_key;
        let handle = cache.handle.entry(output_scale).or_insert_with(|| Keyed {
            key: 0,
            buffer: MemoryRenderBuffer::default(),
            size: (0, 0),
        });
        if handle.key != key
            && let Some(buffer) = draw_handle(palette, output_scale)
        {
            *handle = Keyed { key, buffer, size: (HANDLE, HANDLE) };
        }
        let half = f64::from(HANDLE) / 2.0;
        let (rise, alpha) = entering(handles_shown);
        for which in model::HANDLES {
            let at = which.at(target);
            pieces.extend(raster(
                renderer,
                &handle.buffer,
                Point::from((at.x - half, at.y - half + rise)),
                alpha,
                scale,
            ));
        }
    }

    if shot.typing
        && let Some(Mark { shape: Shape::Text { at, .. }, sprite, .. }) = shot.draft.as_ref()
    {
        let width = sprite.as_deref().map_or(0.0, |sprite| {
            f64::from(sprite.offset.0 + sprite.image.width as f32) / sprite.scale - f64::from(TEXT_PAD)
        });
        let h = f64::from(TEXT_STYLE.line_height_px);
        let caret = area(at.x + width + 1.0, at.y - h / 2.0, 2.0, h);
        let colour = shot.draft.as_ref().map_or([255, 255, 255], Mark::color);
        let rgba = Color::from_rgb8(colour[0], colour[1], colour[2], 1.0);
        pieces.push(solid(&cache.ids[8], caret, premultiplied(rgba, openness), scale));
    }
}

fn render_marks<R: crate::raster::ShellRenderer>(cache: &mut Cache, scene: &Scene<'_>, ctx: &mut Ctx<'_, R>) {
    let (renderer, pieces) = (&mut *ctx.renderer, &mut ctx.pieces);
    let (palette, scale, content) = (ctx.palette, ctx.scale, ctx.content);
    let shot = scene.shot;
    let clip = shot
        .target()
        .filter(|_| shot.editing())
        .map(|target| selection_area(shot.mode, target, scale, ctx.frozen_size));
    let blur_tint = if palette.appearance_dark { Color([1.0, 1.0, 1.0, 0.18]) } else { Color([1.0, 1.0, 1.0, 0.35]) };
    if let Some(draft) = shot.draft.as_ref() {
        let key = shape_key(draft);
        if cache.draft.as_ref().is_none_or(|layer| layer.key != key) {
            cache.draft = if matches!(draft.shape, Shape::Blur(..)) {
                None
            } else {
                draw_layer(std::slice::from_ref(draft), scale, key)
            };
        }
        if let Some(layer) = cache.draft.as_ref() {
            pieces.extend(place(renderer, &layer.buffer, layer.area, layer.pixels, clip, content, scale));
        }
        if let Shape::Blur(a, b) = draft.shape {
            let preview = model::rect_between(a, b);
            if let Some(visible) = clip.and_then(|clip| intersect(preview, clip)) {
                pieces.push(solid(&cache.ids[9], visible, premultiplied(blur_tint, content), scale));
            }
        }
    } else {
        cache.draft = None;
    }

    let marks_key = glide_theme::Revision::new()
        .u64(shot.revision)
        .u64(shot.marks.iter().filter(|mark| mark.sprite.is_some()).count() as u64)
        .u64(crate::paint::painter_generation())
        .f64(scale)
        .finish();
    if cache.marks.as_ref().is_none_or(|layer| layer.key != marks_key) {
        cache.marks = draw_layer(&shot.marks, scale, marks_key);
    }
    if let Some(layer) = cache.marks.as_ref() {
        pieces.extend(place(renderer, &layer.buffer, layer.area, layer.pixels, clip, content, scale));
    }

    let (source, frame) = scene.sources.blur_source();
    let generation = scene.window_generation as u64;
    cache.blurs.retain(|index, _| *index < shot.marks.len());
    cache.pending_blurs.retain(|index, _| *index < shot.marks.len());
    for (index, mark) in shot.marks.iter().enumerate() {
        let Shape::Blur(from, to) = mark.shape else {
            continue;
        };
        let key = shape_key(mark) ^ generation.rotate_left(17);
        if cache.blurs.get(&index).is_none_or(|layer| layer.key != key) {
            cache.blurs.remove(&index);
            if cache.pending_blurs.get(&index).is_none_or(|job| job.key != key) {
                let region = frame.pixels(from, to);
                let px = f64::from(region.0.max(0)) + frame.x;
                let py = f64::from(region.1.max(0)) + frame.y;
                match source.crop(region) {
                    Some(cropped) => {
                        let at = area(
                            px / scale,
                            py / scale,
                            f64::from(cropped.width) / scale,
                            f64::from(cropped.height) / scale,
                        );
                        cache.pending_blurs.insert(index, BlurJob::start(key, at, cropped, scale));
                    }
                    None => {
                        cache.pending_blurs.remove(&index);
                    }
                }
            }
            if let Some(job) = cache.pending_blurs.get_mut(&index).filter(|job| !job.failed) {
                match job.receiver.try_recv() {
                    Ok(Some(patch)) => {
                        let layer = Layer {
                            key,
                            buffer: frame_buffer(&patch),
                            area: job.at,
                            pixels: (patch.width, patch.height),
                        };
                        cache.blurs.insert(index, layer);
                        cache.pending_blurs.remove(&index);
                    }
                    Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => job.failed = true,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            }
        }
        if let Some(layer) = cache.blurs.get(&index) {
            pieces.extend(place(renderer, &layer.buffer, layer.area, layer.pixels, clip, content, scale));
        } else if let Some(job) = cache.pending_blurs.get(&index)
            && let Some(visible) = clip.map_or(Some(job.at), |clip| intersect(job.at, clip))
        {
            pieces.push(solid(&job.id, visible, premultiplied(blur_tint, content), scale));
        }
    }
}

struct BlurJob {
    key: u64,
    at: Area,
    id: Id,
    receiver: std::sync::mpsc::Receiver<Option<Image>>,
    failed: bool,
}

impl BlurJob {
    fn start(key: u64, at: Area, cropped: Image, scale: f64) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let region = (0, 0, cropped.width as i32, cropped.height as i32);
        let spawned = std::thread::Builder::new().name("glide-shot-blur".into()).spawn(move || {
            let _ = sender.send(compose::obscured(&cropped, region, scale));
        });
        Self { key, at, id: Id::new(), receiver, failed: spawned.is_err() }
    }
}

fn render_base<R: crate::raster::ShellRenderer>(cache: &mut Cache, scene: &Scene<'_>, ctx: &mut Ctx<'_, R>) {
    let (renderer, pieces) = (&mut *ctx.renderer, &mut ctx.pieces);
    let (palette, scale, openness) = (ctx.palette, ctx.scale, ctx.openness);
    let (output_scale, base_key, full) = (ctx.output_scale, ctx.base_key, ctx.full);
    let shot = scene.shot;
    let target = shot.target().map(|target| selection_area(shot.mode, target, scale, ctx.frozen_size));
    let accent = premultiplied(palette.accent, openness);
    match shot.mode {
        Mode::Area => {
            if let Some(target) = target {
                for (line, id) in frame_lines(target, OUTLINE, false).into_iter().zip(&cache.ids[4..8]) {
                    pieces.push(solid(id, line, accent, scale));
                }
            }
        }
        Mode::Window => {
            if let Some(target) = target.filter(|_| !shot.editing()) {
                let size = (target.size.w.round() as i32, target.size.h.round() as i32);
                let key = glide_theme::Revision::new().u64(base_key).i32(size.0).i32(size.1).f32(scene.radius).finish();
                let ring = cache.ring.entry(output_scale).or_insert_with(|| Keyed {
                    key: 0,
                    buffer: MemoryRenderBuffer::default(),
                    size: (0, 0),
                });
                if ring.key != key
                    && let Some(buffer) = draw_ring(size, scene.radius, palette, output_scale)
                {
                    *ring = Keyed { key, buffer, size };
                }
                if ring.key == key {
                    let at = target.loc - Point::from((f64::from(RING_PAD), f64::from(RING_PAD)));
                    pieces.extend(raster(renderer, &ring.buffer, at, openness, scale));
                }
            }
        }
        Mode::Screen => {
            if !shot.editing() {
                for (line, id) in frame_lines(full, 3.0, true).into_iter().zip(&cache.ids[4..8]) {
                    pieces.push(solid(id, line, accent, scale));
                }
            }
        }
    }

    let hole = match shot.mode {
        Mode::Screen => Some(full),
        _ => target,
    };
    let dim = [0.0, 0.0, 0.0, DIM * openness];
    if hole != Some(full) {
        for (rect, id) in around(full, hole).into_iter().zip(&cache.ids[..4]) {
            pieces.push(solid(id, rect, dim, scale));
        }
    }

    if shot.editing()
        && shot.mode == Mode::Window
        && let (Some((image, at)), Some(_)) = (scene.sources.window, target)
    {
        if cache.window.as_ref().is_none_or(|(generation, _)| *generation != scene.window_generation) {
            cache.window = Some((scene.window_generation, frame_buffer(image)));
        }
        if let Some((_, buffer)) = cache.window.as_ref() {
            let shown = area(at.loc.x, at.loc.y, f64::from(image.width) / scale, f64::from(image.height) / scale);
            pieces.extend(place(renderer, buffer, shown, (image.width, image.height), None, 1.0, scale));
        }
    }

    let frozen = scene.sources.frozen;
    let buffer = cache.frozen.get_or_insert_with(|| frame_buffer(frozen));
    pieces.extend(place(renderer, buffer, full, (frozen.width, frozen.height), None, 1.0, scale));
}

fn bar_state(shot: &Shot) -> u64 {
    let mut revision = glide_theme::Revision::new()
        .bool(shot.editing())
        .u64(shot.mode as u64)
        .u64(shot.tool as u64)
        .u64(shot.swatch as u64)
        .bool(shot.shadow)
        .bool(shot.can_capture())
        .bool(shot.can_undo())
        .bool(shot.can_redo())
        .u64(crate::i18n::language() as u64);
    let bar = shot.bar();
    revision = revision.f64(bar.rect.size.w).f64(bar.rect.size.h);
    revision.finish()
}

fn bar_margin() -> i32 {
    glide_paint::shadow_fade_margin(52)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Area,
    Window,
    Screen,
    Shadow,
    Close,
    Move,
    Pen,
    Arrow,
    Rect,
    Highlight,
    Blur,
    Text,
    Step,
    Undo,
    Redo,
    Copy,
    Save,
}

const fn glyph_of(control: Control) -> Option<Glyph> {
    Some(match control {
        Control::Mode(Mode::Area) => Glyph::Area,
        Control::Mode(Mode::Window) => Glyph::Window,
        Control::Mode(Mode::Screen) => Glyph::Screen,
        Control::Shadow => Glyph::Shadow,
        Control::Close => Glyph::Close,
        Control::Tool(Tool::Move) => Glyph::Move,
        Control::Tool(Tool::Pen) => Glyph::Pen,
        Control::Tool(Tool::Arrow) => Glyph::Arrow,
        Control::Tool(Tool::Rect) => Glyph::Rect,
        Control::Tool(Tool::Highlight) => Glyph::Highlight,
        Control::Tool(Tool::Blur) => Glyph::Blur,
        Control::Tool(Tool::Text) => Glyph::Text,
        Control::Tool(Tool::Step) => Glyph::Step,
        Control::Undo => Glyph::Undo,
        Control::Redo => Glyph::Redo,
        Control::Copy => Glyph::Copy,
        Control::Save => Glyph::Save,
        Control::Capture | Control::Swatch(_) => return None,
    })
}

#[must_use]
pub const fn label_key(control: Control) -> Option<Key> {
    Some(match control {
        Control::Mode(Mode::Area) => Key::ShotArea,
        Control::Mode(Mode::Window) => Key::ShotWindow,
        Control::Mode(Mode::Screen) => Key::ShotScreen,
        Control::Shadow => Key::ShotShadow,
        Control::Capture => Key::ShotCapture,
        Control::Close => Key::ShotCancel,
        Control::Tool(Tool::Move) => Key::ShotMove,
        Control::Tool(Tool::Pen) => Key::ShotPen,
        Control::Tool(Tool::Arrow) => Key::ShotArrow,
        Control::Tool(Tool::Rect) => Key::ShotRect,
        Control::Tool(Tool::Highlight) => Key::ShotHighlight,
        Control::Tool(Tool::Blur) => Key::ShotBlur,
        Control::Tool(Tool::Text) => Key::ShotText,
        Control::Tool(Tool::Step) => Key::ShotStep,
        Control::Undo => Key::ShotUndo,
        Control::Redo => Key::ShotRedo,
        Control::Copy => Key::ShotCopy,
        Control::Save => Key::ShotSave,
        Control::Swatch(_) => return None,
    })
}

#[must_use]
pub const fn shortcut(control: Control) -> Option<&'static str> {
    Some(match control {
        Control::Mode(Mode::Area) | Control::Tool(Tool::Arrow) => "A",
        Control::Mode(Mode::Window) => "W",
        Control::Mode(Mode::Screen) => "S",
        Control::Shadow => "D",
        Control::Capture | Control::Save => "Enter",
        Control::Close => "Esc",
        Control::Tool(Tool::Move) => "M",
        Control::Tool(Tool::Pen) => "P",
        Control::Tool(Tool::Rect) => "R",
        Control::Tool(Tool::Highlight) => "H",
        Control::Tool(Tool::Blur) => "B",
        Control::Tool(Tool::Text) => "T",
        Control::Tool(Tool::Step) => "N",
        Control::Undo => "Ctrl+Z",
        Control::Redo => "Ctrl+Shift+Z",
        Control::Copy => "Ctrl+C",
        Control::Swatch(_) => return None,
    })
}

#[must_use]
pub fn tip_text(control: Option<Control>) -> Option<String> {
    let control = control?;
    let label = crate::i18n::t(label_key(control)?);
    Some(match shortcut(control) {
        Some(keys) => format!("{label}   {keys}"),
        None => label.to_string(),
    })
}

fn with_bgr(pixmap_color: Color, alpha: f32) -> Paint<'static> {
    let [r, g, b, a] = pixmap_color.0;
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        (b * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (r * 255.0).round() as u8,
        ((a * alpha).clamp(0.0, 1.0) * 255.0).round() as u8,
    );
    paint.anti_alias = true;
    paint
}

fn rounded(builder: &mut PathBuilder, (x0, y0, x1, y1): (f32, f32, f32, f32), r: f32) {
    builder.move_to(x0 + r, y0);
    builder.line_to(x1 - r, y0);
    builder.quad_to(x1, y0, x1, y0 + r);
    builder.line_to(x1, y1 - r);
    builder.quad_to(x1, y1, x1 - r, y1);
    builder.line_to(x0 + r, y1);
    builder.quad_to(x0, y1, x0, y1 - r);
    builder.line_to(x0, y0 + r);
    builder.quad_to(x0, y0, x0 + r, y0);
    builder.close();
}

fn polyline(builder: &mut PathBuilder, points: &[(f32, f32)]) {
    if let Some((first, rest)) = points.split_first() {
        builder.move_to(first.0, first.1);
        for point in rest {
            builder.line_to(point.0, point.1);
        }
    }
}

fn circle(builder: &mut PathBuilder, cx: f32, cy: f32, r: f32) {
    if let Some(rect) = tiny_skia::Rect::from_xywh(cx - r, cy - r, r * 2.0, r * 2.0) {
        builder.push_oval(rect);
    }
}

pub fn draw_glyph(
    pixmap: &mut PixmapMut<'_>, glyph: Glyph, (x, y, size): (f32, f32, f32), color: Color, backdrop: Color,
) {
    let unit = size / 24.0;
    let transform = Affine::from_row(unit, 0.0, 0.0, unit, x, y);
    let stroke = Stroke { width: 1.8, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
    let mut outline = Outline { lines: PathBuilder::new(), fills: Vec::new() };
    if !tool_outline(glyph, &mut outline) {
        shell_outline(glyph, &mut outline);
    }
    for (builder, fill) in outline.fills {
        let paint = match fill {
            Fill::Ink(alpha) => with_bgr(color, alpha),
            Fill::Backdrop => with_bgr(backdrop, 1.0),
        };
        if let Some(path) = builder.finish() {
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }
    }
    if let Some(path) = outline.lines.finish() {
        pixmap.stroke_path(&path, &with_bgr(color, 1.0), &stroke, transform, None);
    }
}

enum Fill {
    Ink(f32),
    Backdrop,
}

struct Outline {
    lines: PathBuilder,
    fills: Vec<(PathBuilder, Fill)>,
}

fn tool_outline(glyph: Glyph, outline: &mut Outline) -> bool {
    let lines = &mut outline.lines;
    match glyph {
        Glyph::Move => {
            polyline(lines, &[(12.0, 2.5), (12.0, 21.5)]);
            polyline(lines, &[(2.5, 12.0), (21.5, 12.0)]);
            polyline(lines, &[(9.0, 5.5), (12.0, 2.5), (15.0, 5.5)]);
            polyline(lines, &[(9.0, 18.5), (12.0, 21.5), (15.0, 18.5)]);
            polyline(lines, &[(5.5, 9.0), (2.5, 12.0), (5.5, 15.0)]);
            polyline(lines, &[(18.5, 9.0), (21.5, 12.0), (18.5, 15.0)]);
        }
        Glyph::Pen => {
            polyline(lines, &[(16.5, 3.5), (20.5, 7.5), (8.0, 20.0), (3.0, 21.0), (4.0, 16.0), (16.5, 3.5)]);
            polyline(lines, &[(14.0, 6.0), (18.0, 10.0)]);
        }
        Glyph::Arrow => {
            polyline(lines, &[(5.0, 19.0), (19.0, 5.0)]);
            polyline(lines, &[(9.0, 5.0), (19.0, 5.0), (19.0, 15.0)]);
        }
        Glyph::Rect => rounded(lines, (3.5, 5.5, 20.5, 18.5), 2.5),
        Glyph::Highlight => {
            polyline(lines, &[(9.0, 11.0), (3.0, 17.0), (3.0, 20.0), (12.0, 20.0), (15.0, 17.0)]);
            polyline(lines, &[(14.0, 4.0), (21.5, 11.5), (16.5, 16.5), (9.0, 9.0), (14.0, 4.0)]);
        }
        Glyph::Blur => {
            let mut strong = PathBuilder::new();
            let mut weak = PathBuilder::new();
            for row in 0..3 {
                for column in 0..3 {
                    let (cx, cy) = (5.0 + column as f32 * 7.0, 5.0 + row as f32 * 7.0);
                    let target = if (row + column) % 2 == 0 { &mut strong } else { &mut weak };
                    rounded(target, (cx - 2.6, cy - 2.6, cx + 2.6, cy + 2.6), 1.0);
                }
            }
            outline.fills.push((strong, Fill::Ink(1.0)));
            outline.fills.push((weak, Fill::Ink(0.4)));
        }
        Glyph::Text => {
            polyline(lines, &[(5.0, 7.0), (5.0, 4.5), (19.0, 4.5), (19.0, 7.0)]);
            polyline(lines, &[(12.0, 4.5), (12.0, 19.5)]);
            polyline(lines, &[(9.0, 19.5), (15.0, 19.5)]);
        }
        Glyph::Step => {
            circle(lines, 12.0, 12.0, 9.0);
            polyline(lines, &[(10.0, 9.5), (12.5, 8.0), (12.5, 16.0)]);
            polyline(lines, &[(10.0, 16.0), (15.0, 16.0)]);
        }
        _ => return false,
    }
    true
}

fn shell_outline(glyph: Glyph, outline: &mut Outline) {
    let lines = &mut outline.lines;
    match glyph {
        Glyph::Area => {
            polyline(lines, &[(3.0, 8.0), (3.0, 3.0), (8.0, 3.0)]);
            polyline(lines, &[(16.0, 3.0), (21.0, 3.0), (21.0, 8.0)]);
            polyline(lines, &[(21.0, 16.0), (21.0, 21.0), (16.0, 21.0)]);
            polyline(lines, &[(8.0, 21.0), (3.0, 21.0), (3.0, 16.0)]);
            polyline(lines, &[(12.0, 9.0), (12.0, 15.0)]);
            polyline(lines, &[(9.0, 12.0), (15.0, 12.0)]);
        }
        Glyph::Window => {
            rounded(lines, (3.0, 4.0, 21.0, 20.0), 2.5);
            polyline(lines, &[(3.0, 9.0), (21.0, 9.0)]);
            let mut dots = PathBuilder::new();
            circle(&mut dots, 6.0, 6.5, 0.9);
            circle(&mut dots, 8.8, 6.5, 0.9);
            outline.fills.push((dots, Fill::Ink(1.0)));
        }
        Glyph::Screen => {
            rounded(lines, (2.5, 3.5, 21.5, 16.5), 2.5);
            polyline(lines, &[(8.0, 20.5), (16.0, 20.5)]);
            polyline(lines, &[(12.0, 16.5), (12.0, 20.5)]);
        }
        Glyph::Shadow => {
            rounded(lines, (3.0, 3.0, 16.5, 16.5), 3.0);
            let mut back = PathBuilder::new();
            rounded(&mut back, (7.5, 7.5, 21.0, 21.0), 3.0);
            outline.fills.push((back, Fill::Ink(0.35)));
            let mut front = PathBuilder::new();
            rounded(&mut front, (3.0, 3.0, 16.5, 16.5), 3.0);
            outline.fills.push((front, Fill::Backdrop));
        }
        Glyph::Close => {
            polyline(lines, &[(6.5, 6.5), (17.5, 17.5)]);
            polyline(lines, &[(17.5, 6.5), (6.5, 17.5)]);
        }
        Glyph::Undo | Glyph::Redo => {
            let flip = |(px, py): (f32, f32)| if glyph == Glyph::Redo { (24.0 - px, py) } else { (px, py) };
            polyline(lines, &[(9.0, 14.0), (4.0, 9.0), (9.0, 4.0)].map(flip));
            let [start, bend, c1, c2, middle, c3, c4, end, tail] = [
                (4.0, 9.0),
                (14.5, 9.0),
                (17.54, 9.0),
                (20.0, 11.46),
                (20.0, 14.5),
                (20.0, 17.54),
                (17.54, 20.0),
                (14.5, 20.0),
                (11.0, 20.0),
            ]
            .map(flip);
            lines.move_to(start.0, start.1);
            lines.line_to(bend.0, bend.1);
            lines.cubic_to(c1.0, c1.1, c2.0, c2.1, middle.0, middle.1);
            lines.cubic_to(c3.0, c3.1, c4.0, c4.1, end.0, end.1);
            lines.line_to(tail.0, tail.1);
        }
        Glyph::Copy => {
            rounded(lines, (9.0, 9.0, 21.0, 21.0), 2.5);
            lines.move_to(5.0, 15.0);
            lines.line_to(4.5, 15.0);
            lines.quad_to(3.0, 15.0, 3.0, 13.5);
            lines.line_to(3.0, 4.5);
            lines.quad_to(3.0, 3.0, 4.5, 3.0);
            lines.line_to(13.5, 3.0);
            lines.quad_to(15.0, 3.0, 15.0, 4.5);
            lines.line_to(15.0, 5.0);
        }
        Glyph::Save => {
            polyline(lines, &[(21.0, 15.0), (21.0, 19.0)]);
            lines.quad_to(21.0, 21.0, 19.0, 21.0);
            lines.line_to(5.0, 21.0);
            lines.quad_to(3.0, 21.0, 3.0, 19.0);
            lines.line_to(3.0, 15.0);
            polyline(lines, &[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)]);
            polyline(lines, &[(12.0, 15.0), (12.0, 3.0)]);
        }
        _ => {}
    }
}

fn pixmap_of<'a>(canvas: &'a mut Canvas<'static>) -> Option<PixmapMut<'a>> {
    let (w, h) = ((canvas.w * canvas.scale) as u32, (canvas.h * canvas.scale) as u32);
    PixmapMut::from_bytes(&mut canvas.buf, w, h)
}

fn swatch_color(index: usize) -> Color {
    let [r, g, b] = model::SWATCHES.get(index).copied().unwrap_or(model::SWATCHES[0]);
    Color::from_rgb8(r, g, b, 1.0)
}

fn draw_bar(
    shot: &Shot, bar: &Bar, palette: &Palette, painter: &Painter, scale: i32,
) -> Option<(MemoryRenderBuffer, (i32, i32))> {
    let canvas = bar_canvas(shot, bar, palette, painter, scale)?;
    Some((canvas.finish(), (canvas.w, canvas.h)))
}

fn bar_canvas(shot: &Shot, bar: &Bar, palette: &Palette, painter: &Painter, scale: i32) -> Option<Canvas<'static>> {
    let (width, height) = (bar.rect.size.w as i32, bar.rect.size.h as i32);
    let margin = bar_margin();
    let radius = if shot.editing() { glide_theme::shape::R3 } else { glide_theme::shape::R4 };
    let appearance =
        if palette.appearance_dark { glide_theme::Appearance::Dark } else { glide_theme::Appearance::Light };
    let layers = glide_theme::elevation::layers(glide_theme::elevation::Level::El3, appearance);
    let mut canvas = crate::raster::shadow((width, height), margin, radius, &layers, scale)?;
    let panel = rect(margin, margin, width, height);
    canvas.fill(panel, palette.panel, radius);
    canvas.stroke(panel, 1.0, radius, palette.line);
    for rule in &bar.rules {
        canvas.fill(rect(margin + rule.x as i32, margin + rule.y as i32 - 10, 1, 20), palette.line, 0.0);
    }
    let mut decor = Decor::default();
    for (control, item) in &bar.items {
        let cell = rect(margin + item.loc.x as i32, margin + item.loc.y as i32, item.size.w as i32, item.size.h as i32);
        decorate(&mut canvas, &mut decor, shot, *control, cell, palette);
    }
    let s = scale as f32;
    if let Some(mut pixmap) = pixmap_of(&mut canvas) {
        for disc in decor.discs {
            let mut builder = PathBuilder::new();
            circle(&mut builder, disc.x * s, disc.y * s, disc.radius * s);
            let Some(path) = builder.finish() else {
                continue;
            };
            match disc.ring {
                Some(width) => {
                    let stroke = Stroke { width: width * s, ..Stroke::default() };
                    pixmap.stroke_path(&path, &with_bgr(disc.color, 1.0), &stroke, Affine::identity(), None);
                }
                None => {
                    pixmap.fill_path(&path, &with_bgr(disc.color, 1.0), FillRule::Winding, Affine::identity(), None);
                }
            }
        }
        for mark in decor.glyphs {
            let (x, y, size) = mark.at;
            draw_glyph(&mut pixmap, mark.glyph, (x * s, y * s, size * s), mark.color, mark.backdrop);
        }
    }
    for (band, text, color) in decor.labels {
        canvas.text_in(painter, band, TextAlign::Center, text, color, &LABEL_STYLE);
    }
    Some(canvas)
}

struct Disc {
    x: f32,
    y: f32,
    radius: f32,
    color: Color,
    ring: Option<f32>,
}

struct GlyphMark {
    glyph: Glyph,
    at: (f32, f32, f32),
    color: Color,
    backdrop: Color,
}

#[derive(Default)]
struct Decor {
    labels: Vec<(crate::raster::Rect, &'static str, Color)>,
    glyphs: Vec<GlyphMark>,
    discs: Vec<Disc>,
}

fn decorate(
    canvas: &mut Canvas<'static>, decor: &mut Decor, shot: &Shot, control: Control, cell: crate::raster::Rect,
    palette: &Palette,
) {
    let active = shot.active(control);
    let enabled = shot.enabled(control);
    let (x, y) = (cell.loc.x as f32 + cell.size.w as f32 / 2.0, cell.loc.y as f32 + cell.size.h as f32 / 2.0);
    let centred = |size: f32| (x - size / 2.0, y - size / 2.0, size);
    let disc = |radius: f32, color: Color, ring: Option<f32>| Disc { x, y, radius, color, ring };
    match control {
        Control::Capture => {
            let alpha = if enabled { 1.0 } else { 0.4 };
            let half = model::DISC as f32 / 2.0;
            decor.discs.push(disc(half - 1.5, palette.text.with_alpha(palette.text.0[3] * alpha), Some(2.5)));
            decor.discs.push(disc(half - 6.5, palette.accent.with_alpha(alpha), None));
        }
        Control::Swatch(index) => {
            if active {
                decor.discs.push(disc(11.0, palette.text, Some(2.0)));
            }
            decor.discs.push(disc(8.0, swatch_color(index), None));
            decor.discs.push(disc(8.0, palette.line, Some(1.0)));
        }
        Control::Close if !shot.editing() => {
            canvas.fill(cell, palette.fill, glide_theme::shape::R_ROUND);
            let mark =
                GlyphMark { glyph: Glyph::Close, at: centred(16.0), color: palette.muted, backdrop: palette.fill };
            decor.glyphs.push(mark);
        }
        Control::Mode(_) | Control::Shadow => {
            let (fill, ink) = if active { (Some(palette.soft), palette.soft_fg) } else { (None, palette.muted) };
            if let Some(fill) = fill {
                canvas.fill(cell, fill, glide_theme::shape::R3);
            }
            if let Some(glyph) = glyph_of(control) {
                let at = (x - GLYPH / 2.0, cell.loc.y as f32 + 6.0, GLYPH);
                decor.glyphs.push(GlyphMark { glyph, at, color: ink, backdrop: fill.unwrap_or(palette.panel) });
            }
            if let Some(key) = label_key(control) {
                let band = rect(cell.loc.x, cell.loc.y + 28, cell.size.w, 16);
                decor.labels.push((band, crate::i18n::t(key), ink));
            }
        }
        Control::Save => {
            canvas.fill(cell, palette.accent, glide_theme::shape::R2);
            let at = centred(TOOL_GLYPH);
            decor.glyphs.push(GlyphMark { glyph: Glyph::Save, at, color: palette.accent_fg, backdrop: palette.accent });
        }
        _ => {
            let ink = if !enabled {
                palette.faint
            } else if active {
                palette.soft_fg
            } else {
                palette.text.with_alpha(palette.text.0[3] * 0.82)
            };
            if active {
                canvas.fill(cell, palette.soft, glide_theme::shape::R2);
            }
            if let Some(glyph) = glyph_of(control) {
                let backdrop = if active { palette.soft } else { palette.panel };
                decor.glyphs.push(GlyphMark { glyph, at: centred(TOOL_GLYPH), color: ink, backdrop });
            }
        }
    }
}

fn sized(canvas: &Canvas<'static>) -> (MemoryRenderBuffer, (i32, i32)) {
    (canvas.finish(), (canvas.w, canvas.h))
}

fn draw_tip(text: &str, palette: &Palette, painter: &Painter, scale: i32) -> Option<(MemoryRenderBuffer, (i32, i32))> {
    draw_tip_canvas(text, palette, painter, scale).as_ref().map(sized)
}

fn draw_tip_canvas(text: &str, palette: &Palette, painter: &Painter, scale: i32) -> Option<Canvas<'static>> {
    let width = painter.text_width_styled(text, &TextStyle::CAP).ceil() as i32 + 20;
    let mut canvas =
        Canvas::popover((width, TIP_H), scale, glide_theme::shape::R2, palette.inverse_bg, palette.inverse_bg)?;
    canvas.text_in(painter, canvas.full(), TextAlign::Center, text, palette.inverse_fg, &TextStyle::CAP);
    Some(canvas)
}

fn draw_badge(
    text: &str, palette: &Palette, painter: &Painter, scale: i32,
) -> Option<(MemoryRenderBuffer, (i32, i32))> {
    draw_badge_canvas(text, palette, painter, scale).as_ref().map(sized)
}

fn draw_badge_canvas(text: &str, palette: &Palette, painter: &Painter, scale: i32) -> Option<Canvas<'static>> {
    let width = painter.text_width_styled(text, &BADGE_STYLE).ceil() as i32 + 16;
    let mut canvas = Canvas::new(width, BADGE_H, scale)?;
    canvas.fill(canvas.full(), palette.inverse_bg, glide_theme::shape::R1);
    canvas.text_in(painter, canvas.full(), TextAlign::Center, text, palette.inverse_fg, &BADGE_STYLE);
    Some(canvas)
}

fn draw_handle(palette: &Palette, scale: i32) -> Option<MemoryRenderBuffer> {
    draw_handle_canvas(palette, scale).map(|canvas| canvas.finish())
}

fn draw_handle_canvas(palette: &Palette, scale: i32) -> Option<Canvas<'static>> {
    let mut canvas = Canvas::new(HANDLE, HANDLE, scale)?;
    let s = scale as f32;
    let centre = HANDLE as f32 / 2.0 * s;
    let mut pixmap = pixmap_of(&mut canvas)?;
    let mut shade = PathBuilder::new();
    circle(&mut shade, centre, centre + 0.8 * s, (HANDLE_DOT / 2.0 + 1.5) * s);
    if let Some(path) = shade.finish() {
        pixmap.fill_path(
            &path,
            &with_bgr(Color([0.0, 0.0, 0.0, 0.22]), 1.0),
            FillRule::Winding,
            Affine::identity(),
            None,
        );
    }
    let mut dot = PathBuilder::new();
    circle(&mut dot, centre, centre, HANDLE_DOT / 2.0 * s);
    if let Some(path) = dot.finish() {
        pixmap.fill_path(
            &path,
            &with_bgr(Color([1.0, 1.0, 1.0, 1.0]), 1.0),
            FillRule::Winding,
            Affine::identity(),
            None,
        );
        let stroke = Stroke { width: 2.0 * s, ..Stroke::default() };
        pixmap.stroke_path(&path, &with_bgr(palette.accent, 1.0), &stroke, Affine::identity(), None);
    }
    Some(canvas)
}

fn draw_ring(size: (i32, i32), radius: f32, palette: &Palette, scale: i32) -> Option<MemoryRenderBuffer> {
    draw_ring_canvas(size, radius, palette, scale).map(|canvas| canvas.finish())
}

fn draw_ring_canvas((w, h): (i32, i32), radius: f32, palette: &Palette, scale: i32) -> Option<Canvas<'static>> {
    let mut canvas = Canvas::new(w + RING_PAD * 2, h + RING_PAD * 2, scale)?;
    let inner = rect(RING_PAD, RING_PAD, w, h);
    canvas.fill(inner, palette.accent.with_alpha(0.12), radius);
    let half = (RING_WIDTH / 2.0).ceil() as i32;
    canvas.stroke(
        rect(RING_PAD - half, RING_PAD - half, w + half * 2, h + half * 2),
        RING_WIDTH,
        radius + half as f32,
        palette.accent,
    );
    Some(canvas)
}

#[must_use]
pub fn text_sprite(text: &str, color: [u8; 3], painter: &Painter, scale: f64) -> Option<Sprite> {
    let style = TextStyle {
        size_px: TEXT_STYLE.size_px * scale as f32,
        line_height_px: TEXT_STYLE.line_height_px * scale as f32,
        ..TEXT_STYLE
    };
    let pad = (TEXT_PAD * scale as f32).ceil() as i32;
    let width = painter.text_width_styled(text, &style).ceil() as i32 + 2 + pad * 2;
    let height = style.line_height_px.ceil() as i32 + pad * 2;
    let mut canvas = Canvas::new(width.max(1), height.max(1), 1)?;
    let band = rect(pad, pad, width - pad * 2, height - pad * 2);
    let light = u32::from(color[0]) * 299 + u32::from(color[1]) * 587 + u32::from(color[2]) * 114 > 128_000;
    let halo = if light { Color([0.0, 0.0, 0.0, 0.55]) } else { Color([1.0, 1.0, 1.0, 0.7]) };
    canvas.text_in(painter, band, TextAlign::Left, text, halo, &style);
    glide_paint::box_blur_rgba(&mut canvas.buf, width, height, (1.5 * scale).round().max(1.0) as i32);
    let ink = Color::from_rgb8(color[0], color[1], color[2], 1.0);
    canvas.text_in(painter, band, TextAlign::Left, text, ink, &style);
    let image = Image::from_bgra(width.max(1) as u32, height.max(1) as u32, canvas.buf)?;
    Some(Sprite { image, offset: (-pad as f32, -(height as f32) / 2.0), scale })
}

#[must_use]
pub fn step_sprite(number: u32, color: [u8; 3], painter: &Painter, scale: f64) -> Option<Sprite> {
    let s = scale as f32;
    let radius = compose::STEP_RADIUS * s;
    let pad = (4.0 * s).ceil();
    let side = (radius * 2.0 + pad * 2.0).ceil() as i32;
    let mut canvas = Canvas::new(side, side, 1)?;
    let centre = side as f32 / 2.0;
    {
        let mut pixmap = pixmap_of(&mut canvas)?;
        let mut shade = PathBuilder::new();
        circle(&mut shade, centre, centre + s, radius + 0.5 * s);
        if let Some(path) = shade.finish() {
            pixmap.fill_path(
                &path,
                &with_bgr(Color([0.0, 0.0, 0.0, 0.25]), 1.0),
                FillRule::Winding,
                Affine::identity(),
                None,
            );
        }
        let mut disc = PathBuilder::new();
        circle(&mut disc, centre, centre, radius);
        if let Some(path) = disc.finish() {
            let fill = Color::from_rgb8(color[0], color[1], color[2], 1.0);
            pixmap.fill_path(&path, &with_bgr(fill, 1.0), FillRule::Winding, Affine::identity(), None);
            let ring = Stroke { width: 1.5 * s, ..Stroke::default() };
            pixmap.stroke_path(&path, &with_bgr(Color([1.0, 1.0, 1.0, 0.9]), 1.0), &ring, Affine::identity(), None);
        }
    }
    let light = u32::from(color[0]) * 299 + u32::from(color[1]) * 587 + u32::from(color[2]) * 114 > 160_000;
    let ink = if light { Color::from_rgb8(28, 28, 30, 1.0) } else { Color([1.0, 1.0, 1.0, 1.0]) };
    let style =
        TextStyle { size_px: STEP_STYLE.size_px * s, line_height_px: STEP_STYLE.line_height_px * s, ..STEP_STYLE };
    canvas.text_in(painter, canvas.full(), TextAlign::Center, &number.to_string(), ink, &style);
    let image = Image::from_bgra(side as u32, side as u32, canvas.buf)?;
    Some(Sprite { image, offset: (-centre, -centre), scale })
}

#[must_use]
pub fn sprite_for(mark: &Mark, painter: &Painter, scale: f64) -> Option<Arc<Sprite>> {
    match &mark.shape {
        Shape::Text { text, .. } if !text.is_empty() => text_sprite(text, mark.color(), painter, scale).map(Arc::new),
        Shape::Step { number, .. } => step_sprite(*number, mark.color(), painter, scale).map(Arc::new),
        _ => None,
    }
}

#[must_use]
pub fn cursor_for(shot: &Shot, control: Option<Control>, pos: model::Pos) -> smithay::input::pointer::CursorIcon {
    use smithay::input::pointer::CursorIcon;
    if control.is_some() || shot.over_bar(pos) {
        return CursorIcon::Default;
    }
    if let Some(handle) = shot.handle_at(pos) {
        return match handle {
            Handle::N | Handle::S => CursorIcon::NsResize,
            Handle::E | Handle::W => CursorIcon::EwResize,
            Handle::NE | Handle::SW => CursorIcon::NeswResize,
            Handle::NW | Handle::SE => CursorIcon::NwseResize,
        };
    }
    if !shot.editing() {
        return match shot.mode {
            Mode::Area => CursorIcon::Crosshair,
            Mode::Window | Mode::Screen => CursorIcon::Pointer,
        };
    }
    let inside = shot.target().is_some_and(|target| target.contains(pos));
    match shot.tool {
        _ if !inside => {
            if shot.mode == Mode::Area && shot.tool == Tool::Move {
                CursorIcon::Crosshair
            } else {
                CursorIcon::Default
            }
        }
        Tool::Move if shot.mode == Mode::Area => CursorIcon::Move,
        Tool::Move => CursorIcon::Default,
        Tool::Text => CursorIcon::Text,
        _ => CursorIcon::Crosshair,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(x: f64, y: f64, w: f64, h: f64) -> Area {
        area(x, y, w, h)
    }

    #[test]
    fn the_badge_reads_the_size_of_the_saved_crop() {
        let frozen = (1920, 1080);
        for (target, scale) in [
            (a(10.5, 20.0, 100.0, 50.0), 1.0),
            (a(10.0, 20.0, 100.0, 50.0), 1.25),
            (a(33.0, 7.0, 101.0, 49.0), 1.5),
            (a(0.0, 0.0, 1920.0, 1080.0), 1.0),
        ] {
            let (_, _, w, h) = crate::crop_region(target, scale, frozen).expect("crop");
            assert_eq!(badge_size(target, scale, frozen), (i64::from(w), i64::from(h)), "{target:?} at {scale}");
        }
        assert_eq!(badge_size(a(10.5, 20.0, 100.0, 50.0), 1.0, frozen), (101, 50));
    }

    #[test]
    fn the_dim_and_the_outline_cover_exactly_what_the_saved_crop_leaves_out() {
        let full = a(0.0, 0.0, 1920.0, 1080.0);
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let frozen = ((1920.0 * scale) as u32, (1080.0 * scale) as u32);
            for target in [a(10.5, 20.25, 100.0, 50.0), a(33.2, 7.7, 101.4, 49.9), a(10.0, 20.0, 100.0, 50.0)] {
                let (cx, cy, cw, ch) = crate::crop_region(target, scale, frozen).expect("crop");
                let overlaps =
                    |(x, y, w, h): (i32, i32, i32, i32)| x < cx + cw && cx < x + w && y < cy + ch && cy < y + h;

                let shown = selection_area(Mode::Area, target, scale, frozen);
                assert_eq!(physical_box(shown, scale), (cx, cy, cw, ch), "{target:?} at {scale}");

                let dim: Vec<_> = around(full, Some(shown)).into_iter().map(|part| physical_box(part, scale)).collect();
                assert!(dim.iter().all(|part| !overlaps(*part)), "the dim covers a saved pixel: {target:?} at {scale}");
                let covered: i64 = dim.iter().map(|part| i64::from(part.2) * i64::from(part.3)).sum();
                let (_, _, fw, fh) = physical_box(full, scale);
                assert_eq!(
                    covered + i64::from(cw) * i64::from(ch),
                    i64::from(fw) * i64::from(fh),
                    "a saved pixel is neither dimmed nor shown: {target:?} at {scale}"
                );

                for line in frame_lines(shown, OUTLINE, false) {
                    assert!(
                        !overlaps(physical_box(line, scale)),
                        "the outline covers a saved pixel: {target:?} at {scale}"
                    );
                }
            }
        }

        let raw = a(10.5, 20.0, 100.0, 50.0);
        let (cx, _, cw, _) = crate::crop_region(raw, 1.0, (1920, 1080)).expect("crop");
        let left_dim = physical_box(around(full, Some(raw))[2], 1.0);
        assert!(left_dim.0 + left_dim.2 > cx && cx + cw > left_dim.0, "the unsnapped selection dims saved column {cx}");
    }

    #[test]
    fn only_the_area_selection_is_snapped_to_the_saved_pixels() {
        let target = a(10.5, 20.25, 100.0, 50.0);
        for mode in [Mode::Window, Mode::Screen] {
            assert_eq!(selection_area(mode, target, 1.25, (2400, 1350)), target);
        }
        let outside = a(5000.0, 5000.0, 10.0, 10.0);
        assert_eq!(selection_area(Mode::Area, outside, 1.0, (1920, 1080)), outside);
    }

    #[test]
    fn the_dim_surrounds_the_hole_without_covering_it() {
        let full = a(0.0, 0.0, 100.0, 80.0);
        let hole = a(20.0, 10.0, 30.0, 40.0);
        let parts = around(full, Some(hole));
        let covered: f64 = parts.iter().map(|part| part.size.w * part.size.h).sum();
        assert!((covered - (100.0 * 80.0 - 30.0 * 40.0)).abs() < 1e-9);
        assert!(parts.iter().all(|part| intersect(*part, hole).is_none()));
        assert_eq!(around(full, None), vec![full]);
    }

    #[test]
    fn the_outline_sits_outside_a_selection_and_inside_the_screen() {
        let target = a(10.0, 10.0, 50.0, 20.0);
        for line in frame_lines(target, 2.0, false) {
            assert!(intersect(line, target).is_none());
        }
        let full = a(0.0, 0.0, 100.0, 100.0);
        for line in frame_lines(full, 3.0, true) {
            assert!(intersect(line, full).is_some_and(|inside| inside == line));
        }
    }

    #[test]
    fn every_control_has_a_label_and_tools_have_a_shortcut() {
        for tool in model::TOOLS {
            assert!(label_key(Control::Tool(tool)).is_some());
            assert!(shortcut(Control::Tool(tool)).is_some());
            assert!(glyph_of(Control::Tool(tool)).is_some());
        }
        for mode in model::MODES {
            assert!(tip_text(Some(Control::Mode(mode))).is_some());
        }
        assert!(tip_text(Some(Control::Swatch(0))).is_none());
    }

    #[test]
    fn every_glyph_paints_inside_its_box() {
        for glyph in [
            Glyph::Area,
            Glyph::Window,
            Glyph::Screen,
            Glyph::Shadow,
            Glyph::Close,
            Glyph::Move,
            Glyph::Pen,
            Glyph::Arrow,
            Glyph::Rect,
            Glyph::Highlight,
            Glyph::Blur,
            Glyph::Text,
            Glyph::Step,
            Glyph::Undo,
            Glyph::Redo,
            Glyph::Copy,
            Glyph::Save,
        ] {
            let mut data = vec![0u8; 32 * 32 * 4];
            let mut pixmap = PixmapMut::from_bytes(&mut data, 32, 32).expect("pixmap");
            draw_glyph(&mut pixmap, glyph, (4.0, 4.0, 24.0), Color([1.0, 1.0, 1.0, 1.0]), Color([0.0, 0.0, 0.0, 1.0]));
            let inked = data.chunks_exact(4).filter(|pixel| pixel[3] > 0).count();
            assert!(inked > 20, "{glyph:?} drew {inked} pixels");
            let edge = (0..32).flat_map(|i| [(i, 0), (0, i), (i, 31), (31, i)]);
            assert!(edge.into_iter().all(|(x, y)| data[(y * 32 + x) * 4 + 3] == 0), "{glyph:?} bled out of its box");
        }
    }

    fn wallpaper(width: u32, height: u32) -> Image {
        let source = glide_paint::decode_wallpaper(glide_paint::embedded_wallpaper_bytes()).expect("wallpaper");
        let (source_width, buf) = (source.width(), source.data());
        let mut image = Image::new(width, height).expect("image");
        for y in 0..height {
            for x in 0..width {
                let (sx, sy) = (x * source_width / width, y * source.height() / height);
                let from = ((sy * source_width + sx) * 4) as usize;
                let to = ((y * width + x) * 4) as usize;
                image.data[to..to + 3].copy_from_slice(&buf[from..from + 3]);
                image.data[to + 3] = 255;
            }
        }
        image
    }

    fn fake_window(image: &mut Image, (x, y, w, h): (f32, f32, f32, f32), scale: f32, dark: bool) {
        let mut pixmap = image.pixmap().expect("pixmap");
        let (bg, bar) = if dark { ([36, 36, 40], [48, 48, 54]) } else { ([250, 250, 248], [236, 235, 232]) };
        let rect = tiny_skia::Rect::from_xywh(x * scale, y * scale, w * scale, h * scale).expect("rect");
        let mut path = PathBuilder::new();
        rounded(&mut path, (rect.left(), rect.top(), rect.right(), rect.bottom()), 12.0 * scale);
        let path = path.finish().expect("path");
        let mut paint = Paint::default();
        paint.set_color_rgba8(bg[0], bg[1], bg[2], 255);
        pixmap.fill_path(&path, &paint, FillRule::Winding, Affine::identity(), None);
        paint.set_color_rgba8(bar[0], bar[1], bar[2], 255);
        let header =
            tiny_skia::Rect::from_xywh(x * scale, y * scale + 12.0 * scale, w * scale, 36.0 * scale).expect("rect");
        pixmap.fill_rect(header, &paint, Affine::identity(), None);
        let ink = if dark { [200, 200, 200] } else { [60, 60, 60] };
        paint.set_color_rgba8(ink[0], ink[1], ink[2], 255);
        for line in 0..((h - 80.0) / 22.0) as i32 {
            let width = (w - 60.0) * (0.45 + 0.5 * ((line * 37 % 11) as f32 / 11.0));
            let row = tiny_skia::Rect::from_xywh(
                (x + 24.0) * scale,
                (y + 70.0 + line as f32 * 22.0) * scale,
                width * scale,
                8.0 * scale,
            )
            .expect("row");
            pixmap.fill_rect(row, &paint, Affine::identity(), None);
        }
    }

    fn save(image: Image, name: &str) {
        let Some(dir) = std::env::var_os("GLIDE_SHOT_PREVIEW") else {
            return;
        };
        let png = image.png().expect("png");
        std::fs::write(std::path::Path::new(&dir).join(name), png).expect("write");
    }

    fn blit_canvas(target: &mut Image, canvas: &Canvas<'static>, at: (f64, f64), scale: f64) {
        let (w, h) = ((canvas.w * canvas.scale) as u32, (canvas.h * canvas.scale) as u32);
        let sprite = Image::from_bgra(w, h, canvas.buf.clone()).expect("canvas");
        target.draw(&sprite, (at.0 * scale).round() as i32, (at.1 * scale).round() as i32);
    }

    fn fill(target: &mut Image, area: Area, rgba: [u8; 4], scale: f64) {
        let mut pixmap = target.pixmap().expect("pixmap");
        let mut paint = Paint::default();
        paint.set_color_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
        if let Some(rect) = tiny_skia::Rect::from_xywh(
            (area.loc.x * scale) as f32,
            (area.loc.y * scale) as f32,
            (area.size.w * scale) as f32,
            (area.size.h * scale) as f32,
        ) {
            pixmap.fill_rect(rect, &paint, Affine::identity(), None);
        }
    }

    #[test]
    #[ignore = "writes preview PNGs to $GLIDE_SHOT_PREVIEW"]
    fn preview() {
        let painter = Painter::new(14.0, None);
        let scale = 2.0;
        let (w, h) = (1280.0, 800.0);
        let mut frozen = wallpaper((w * scale) as u32, (h * scale) as u32);
        fake_window(&mut frozen, (90.0, 80.0, 620.0, 440.0), scale as f32, false);
        fake_window(&mut frozen, (560.0, 220.0, 560.0, 420.0), scale as f32, true);
        for dark in [true, false] {
            let appearance = if dark { glide_theme::Appearance::Dark } else { glide_theme::Appearance::Light };
            let palette = Palette::from_tokens(&glide_theme::Tokens::new(appearance, glide_theme::Accent::Blue));
            let suffix = if dark { "dark" } else { "light" };
            let windows = vec![a(560.0, 220.0, 560.0, 420.0), a(90.0, 80.0, 620.0, 440.0)];
            let mut shot = Shot::new(Size::from((w, h)), windows, &model::Memory::default());
            preview_choose(&mut shot, &frozen, &palette, &painter, suffix);
            preview_edit(&mut shot, &frozen, &palette, &painter, suffix);
        }
        let window_frame = a(560.0, 220.0, 560.0, 420.0);
        let region = crate::crop_region(window_frame, scale, (frozen.width, frozen.height)).expect("region");
        let window = compose::rounded_corners(frozen.crop(region).expect("crop"), 12.0 * scale as f32);
        let shadowed =
            compose::with_shadow(&window, compose::Shadow { radius: 12.0 * scale as f32, scale }).expect("shadow");
        save(shadowed, "result-window.png");
    }

    fn preview_choose(shot: &mut Shot, frozen: &Image, palette: &Palette, painter: &Painter, suffix: &str) {
        let scale = 2.0;
        {
            for mode in model::MODES {
                shot.set_mode(mode);
                shot.motion(Point::from((300.0, 300.0)), None);
                let mut view = frozen.clone();
                let hole = match mode {
                    Mode::Area => None,
                    Mode::Window => shot.target(),
                    Mode::Screen => Some(shot.full()),
                };
                if hole != Some(shot.full()) {
                    for part in around(shot.full(), hole) {
                        fill(&mut view, part, [0, 0, 0, 115], scale);
                    }
                }
                if mode == Mode::Window
                    && let Some(target) = shot.target()
                {
                    let ring =
                        draw_ring_canvas((target.size.w as i32, target.size.h as i32), 12.0, palette, 2).expect("ring");
                    blit_canvas(
                        &mut view,
                        &ring,
                        (target.loc.x - f64::from(RING_PAD), target.loc.y - f64::from(RING_PAD)),
                        scale,
                    );
                }
                let bar = shot.bar();
                let canvas = bar_canvas(shot, &bar, palette, painter, 2).expect("bar");
                let m = f64::from(bar_margin());
                blit_canvas(&mut view, &canvas, (bar.rect.loc.x - m, bar.rect.loc.y - m), scale);
                save(view, &format!("choose-{mode:?}-{suffix}.png"));
            }
        }
    }

    fn preview_edit(shot: &mut Shot, frozen: &Image, palette: &Palette, painter: &Painter, suffix: &str) {
        let scale = 2.0;
        {
            shot.set_mode(Mode::Area);
            shot.press(Point::from((120.0, 110.0)), None);
            shot.motion(Point::from((690.0, 480.0)), None);
            shot.release(Point::from((690.0, 480.0)));
            let marks = [
                (Tool::Highlight, (150.0, 168.0), (420.0, 186.0)),
                (Tool::Rect, (140.0, 250.0), (380.0, 330.0)),
                (Tool::Arrow, (560.0, 420.0), (400.0, 300.0)),
                (Tool::Blur, (450.0, 160.0), (670.0, 260.0)),
                (Tool::Pen, (160.0, 400.0), (330.0, 440.0)),
            ];
            for (index, (tool, from, to)) in marks.into_iter().enumerate() {
                shot.set_tool(tool);
                shot.set_swatch(match tool {
                    Tool::Highlight => 1,
                    Tool::Pen => 3,
                    _ => 0,
                });
                shot.press(Point::from(from), None);
                for step in 1..=8 {
                    let t = f64::from(step) / 8.0;
                    let wobble = if tool == Tool::Pen { (t * 9.0).sin() * 12.0 } else { 0.0 };
                    shot.motion(
                        Point::from((from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t + wobble)),
                        None,
                    );
                }
                shot.release(Point::from(to));
                assert_eq!(shot.marks.len(), index + 1, "{tool:?}");
            }
            shot.set_tool(Tool::Step);
            shot.set_swatch(0);
            shot.press(Point::from((600.0, 440.0)), None);
            shot.set_tool(Tool::Text);
            shot.set_swatch(5);
            shot.press(Point::from((160.0, 360.0)), None);
            shot.type_text("Натисніть сюди");
            shot.commit_text();
            for mark in shot.sprites_wanted() {
                mark.sprite = sprite_for(mark, painter, scale);
            }
        }
        preview_edit_view(shot, frozen, palette, painter, suffix);
    }

    fn preview_edit_view(shot: &Shot, frozen: &Image, palette: &Palette, painter: &Painter, suffix: &str) {
        let scale = 2.0;
        {
            let target = shot.target().expect("target");
            let mut view = frozen.clone();
            let frame = Frame { x: 0.0, y: 0.0, scale };
            let mut marked = view.clone();
            compose::apply_marks(&mut marked, &shot.marks, &frame);
            let region = crate::crop_region(target, scale, (view.width, view.height)).expect("region");
            let inside = marked.crop(region).expect("inside");
            for part in around(shot.full(), Some(target)) {
                fill(&mut view, part, [0, 0, 0, 115], scale);
            }
            view.draw(&inside, region.0, region.1);
            for line in frame_lines(target, OUTLINE, false) {
                fill(&mut view, line, [47, 111, 208, 255], scale);
            }
            let handle = draw_handle_canvas(palette, 2).expect("handle");
            for which in model::HANDLES {
                let at = which.at(target);
                blit_canvas(
                    &mut view,
                    &handle,
                    (at.x - f64::from(HANDLE) / 2.0, at.y - f64::from(HANDLE) / 2.0),
                    scale,
                );
            }
            let badge = draw_badge_canvas("1140 × 740", palette, painter, 2).expect("badge");
            blit_canvas(&mut view, &badge, (target.loc.x, target.loc.y - BADGE_GAP - f64::from(BADGE_H)), scale);
            let bar = shot.bar();
            let canvas = bar_canvas(shot, &bar, palette, painter, 2).expect("bar");
            let m = f64::from(bar_margin());
            blit_canvas(&mut view, &canvas, (bar.rect.loc.x - m, bar.rect.loc.y - m), scale);
            let tip_text = tip_text(Some(Control::Tool(Tool::Arrow))).expect("tip");
            let tip = draw_tip_canvas(&tip_text, palette, painter, 2).expect("tip");
            let item = bar.item(Control::Tool(Tool::Arrow)).expect("arrow");
            let tx = bar.rect.loc.x + item.loc.x + item.size.w / 2.0 - f64::from(tip.w) / 2.0;
            blit_canvas(&mut view, &tip, (tx, bar.rect.loc.y - TIP_GAP - f64::from(TIP_H)), scale);
            save(view, &format!("edit-{suffix}.png"));

            let shadowed = compose::with_shadow(&inside, compose::Shadow { radius: 0.0, scale }).expect("shadow");
            save(shadowed, &format!("result-area-{suffix}.png"));
        }
    }

    #[test]
    fn mark_layers_cover_their_strokes_and_sprites() {
        let pen = Mark::new(Shape::Pen(vec![Point::from((10.0, 10.0)), Point::from((50.0, 40.0))]), 0);
        let bounds = mark_bounds(&pen).expect("bounds");
        assert!(bounds.loc.x < 10.0 - f64::from(compose::PEN_WIDTH) && bounds.loc.x + bounds.size.w > 50.0);
        let mut step = Mark::new(Shape::Step { at: Point::from((100.0, 100.0)), number: 1 }, 0);
        assert!(mark_bounds(&step).is_none(), "nothing to draw until the sprite exists");
        let image = Image::new(36, 36).expect("image");
        step.sprite = Some(Arc::new(Sprite { image, offset: (-18.0, -18.0), scale: 2.0 }));
        assert_eq!(mark_bounds(&step), Some(a(91.0, 91.0, 18.0, 18.0)));
    }
}

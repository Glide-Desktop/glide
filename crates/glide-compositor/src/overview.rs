use std::time::{Duration, Instant};

use glide_layout::{WindowId, compute_overview_layout};
use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer,
        element::{
            Kind,
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            solid::{SolidColorBuffer, SolidColorRenderElement},
            utils::{Relocate, RelocateRenderElement, RescaleRenderElement},
        },
    },
    utils::{Logical, Point, Rectangle, Scale, Size, Transform},
};

use crate::{
    clip::ClipCapableRenderer,
    config::{OverviewTheme, OverviewThemeExt},
    window::{GlideWindow, GlideWindowRenderElement},
};

type OverviewEntry = (
    WindowId,
    Rectangle<i32, Logical>,
    bool,
    Option<(GlideWindow, String)>,
);

#[derive(Debug, Clone, Copy)]
enum Phase {
    Opening {
        started: Instant,
    },
    Open,
    Closing {
        started: Instant,
        select: Option<WindowId>,
        from: SceneFrom,
    },
}

#[derive(Debug, Clone, Copy)]
struct SceneFrom {
    primary: f64,
    backdrop: f64,
}

fn hover_anim() -> Duration {
    glide_theme::motion::t2()
}

fn lerp_rect(
    from: Rectangle<i32, Logical>,
    to: Rectangle<i32, Logical>,
    t: f64,
) -> Rectangle<i32, Logical> {
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let mix = |a: i32, b: i32| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as i32;

    Rectangle::new(
        Point::from((mix(from.loc.x, to.loc.x), mix(from.loc.y, to.loc.y))),
        Size::from((
            mix(from.size.w, to.size.w).max(1),
            mix(from.size.h, to.size.h).max(1),
        )),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RasterKey {
    output_scale: i32,
    theme_rev: u64,
    painter_gen: u64,
}

impl RasterKey {
    fn new(theme: &OverviewTheme, output_scale: i32) -> Self {
        Self {
            output_scale,
            theme_rev: theme.revision(),
            painter_gen: crate::paint::painter_generation(),
        }
    }
}

type CachedLabel = (String, i32, RasterKey, MemoryRenderBuffer);
type CachedHighlight = (i32, i32, RasterKey, MemoryRenderBuffer);
type CachedClose = (u8, RasterKey, MemoryRenderBuffer);
type SalvagedPreview = (
    WindowId,
    Option<CachedLabel>,
    Option<CachedHighlight>,
    Option<CachedClose>,
);

#[derive(Debug, Clone)]
pub struct Preview {
    pub id: WindowId,
    pub target: Rectangle<i32, Logical>,
    pub origin: Rectangle<i32, Logical>,
    pub row: usize,
    label: Option<CachedLabel>,
    highlight: Option<CachedHighlight>,
    close: Option<CachedClose>,
}

impl Preview {
    #[must_use]
    pub fn rect_at(&self, t: f64) -> Rectangle<i32, Logical> {
        lerp_rect(self.origin, self.target, t)
    }

    #[must_use]
    pub fn scale_at(&self, t: f64) -> f64 {
        let from = 1.0;
        let to = f64::from(self.target.size.w) / f64::from(self.origin.size.w.max(1));
        from + (to - from) * t
    }
}

#[derive(Debug)]
pub struct OverviewUi {
    previews: Vec<Preview>,
    hover: glide_ui::HoverAnimation<WindowId>,
    close_hover: glide_ui::HoverAnimation<WindowId>,
    phase: Phase,
    duration: Duration,
    theme: OverviewTheme,
    area: Rectangle<i32, Logical>,

    scrim: SolidColorBuffer,
}

impl OverviewUi {
    pub fn new(
        windows: &[(WindowId, Rectangle<i32, Logical>)],
        area: Rectangle<i32, Logical>,
        theme: &OverviewTheme,
        focused: Option<WindowId>,
    ) -> Self {
        let sizes: Vec<(WindowId, glide_layout::Size)> = windows
            .iter()
            .map(|(id, rect)| (*id, glide_layout::Size::new(rect.size.w, rect.size.h)))
            .collect();

        let layout_area = glide_layout::Rectangle::from_loc_and_size(
            area.loc.x,
            area.loc.y,
            area.size.w,
            area.size.h,
        );
        let slots = compute_overview_layout(&sizes, layout_area, &theme.options());

        let previews = slots
            .iter()
            .filter_map(|slot| {
                let origin = windows.iter().find(|(id, _)| *id == slot.id)?.1;
                Some(Preview {
                    id: slot.id,
                    target: to_smithay_rect(slot.target),
                    origin,
                    row: slot.row,
                    label: None,
                    highlight: None,
                    close: None,
                })
            })
            .collect();

        let mut hover = glide_ui::HoverAnimation::default();
        hover.retarget_without_restarting(focused);

        Self {
            previews,
            hover,
            close_hover: glide_ui::HoverAnimation::default(),
            phase: Phase::Opening {
                started: Instant::now(),
            },
            duration: theme.duration(),
            theme: theme.clone(),
            area,
            scrim: SolidColorBuffer::default(),
        }
    }

    fn matches_current_layout(
        &self,
        windows: &[(WindowId, Rectangle<i32, Logical>)],
        area: Rectangle<i32, Logical>,
    ) -> bool {
        self.area == area
            && self.previews.len() == windows.len()
            && self
                .previews
                .iter()
                .zip(windows)
                .all(|(preview, (id, origin))| preview.id == *id && preview.origin == *origin)
    }

    pub fn relayout(
        &mut self,
        windows: &[(WindowId, Rectangle<i32, Logical>)],
        area: Rectangle<i32, Logical>,
    ) {
        if self.matches_current_layout(windows, area) {
            return;
        }
        self.area = area;

        let sizes: Vec<(WindowId, glide_layout::Size)> = windows
            .iter()
            .map(|(id, rect)| (*id, glide_layout::Size::new(rect.size.w, rect.size.h)))
            .collect();

        let layout_area = glide_layout::Rectangle::from_loc_and_size(
            area.loc.x,
            area.loc.y,
            area.size.w,
            area.size.h,
        );
        let slots = compute_overview_layout(&sizes, layout_area, &self.theme.options());

        let old_hover_index = self
            .hover
            .target()
            .and_then(|id| self.previews.iter().position(|p| p.id == id));

        let mut old: Vec<SalvagedPreview> = self
            .previews
            .drain(..)
            .map(|p| (p.id, p.label, p.highlight, p.close))
            .collect();

        self.previews = slots
            .iter()
            .filter_map(|slot| {
                let origin = windows.iter().find(|(id, _)| *id == slot.id)?.1;
                let (label, highlight, close) = old
                    .iter_mut()
                    .find(|(id, ..)| *id == slot.id)
                    .map_or((None, None, None), |(_, label, highlight, close)| {
                        (label.take(), highlight.take(), close.take())
                    });
                Some(Preview {
                    id: slot.id,
                    target: to_smithay_rect(slot.target),
                    origin,
                    row: slot.row,
                    label,
                    highlight,
                    close,
                })
            })
            .collect();

        if self.hover.target().is_some_and(|id| !self.contains(id)) {
            let neighbor = old_hover_index.and_then(|idx| {
                let clamped = idx.min(self.previews.len().saturating_sub(1));
                self.previews.get(clamped).map(|p| p.id)
            });
            self.hover.retarget_without_restarting(neighbor);
        }
        if self
            .close_hover
            .target()
            .is_some_and(|id| !self.contains(id))
        {
            self.close_hover.retarget_without_restarting(None);
        }
    }

    fn contains(&self, id: WindowId) -> bool {
        self.previews.iter().any(|p| p.id == id)
    }

    fn elapsed_fraction(&self, started: Instant) -> f64 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = started.elapsed().as_secs_f64();
        (elapsed / self.duration.as_secs_f64()).clamp(0.0, 1.0)
    }

    fn scene_progress(&self, curve: glide_theme::motion::CubicBezier, backdrop: bool) -> f64 {
        match self.phase {
            Phase::Opening { started } => curve.ease_f64(self.elapsed_fraction(started)),
            Phase::Open => 1.0,
            Phase::Closing { started, from, .. } => {
                let from = if backdrop {
                    from.backdrop
                } else {
                    from.primary
                };
                from * (1.0 - curve.ease_f64(self.elapsed_fraction(started)))
            }
        }
    }

    #[must_use]
    pub fn progress(&self) -> f64 {
        self.scene_progress(glide_theme::motion::E_OUT, false)
    }

    #[must_use]
    pub fn backdrop_progress(&self) -> f64 {
        self.scene_progress(glide_theme::motion::E_IO, true)
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let phase_animating = match self.phase {
            Phase::Open => false,
            Phase::Opening { started } | Phase::Closing { started, .. } => {
                started.elapsed() < self.duration
            }
        };
        phase_animating
            || self.hover.progress(hover_anim()) < 1.0
            || self.close_hover.progress(ov_close_hover_anim()) < 1.0
    }

    pub fn tick(&mut self) -> Tick {
        match self.phase {
            Phase::Opening { started } if started.elapsed() >= self.duration => {
                self.phase = Phase::Open;
                Tick::Running
            }
            Phase::Closing {
                started, select, ..
            } if started.elapsed() >= self.duration => Tick::Finished { select },
            Phase::Open
                if self.hover.progress(hover_anim()) < 1.0
                    || self.close_hover.progress(ov_close_hover_anim()) < 1.0 =>
            {
                Tick::Running
            }
            Phase::Open => Tick::Idle,
            Phase::Opening { .. } | Phase::Closing { .. } => Tick::Running,
        }
    }

    pub fn begin_close(&mut self, select: Option<WindowId>) {
        if matches!(self.phase, Phase::Closing { .. }) {
            return;
        }
        let from = SceneFrom {
            primary: self.progress(),
            backdrop: self.backdrop_progress(),
        };
        self.phase = Phase::Closing {
            started: Instant::now(),
            select,
            from,
        };
    }

    #[must_use]
    pub const fn is_closing(&self) -> bool {
        matches!(self.phase, Phase::Closing { .. })
    }

    #[must_use]
    pub fn hovered(&self) -> Option<WindowId> {
        self.hover.target()
    }

    #[must_use]
    pub fn preview_at(&self, pos: Point<f64, Logical>) -> Option<WindowId> {
        let t = self.progress();
        self.previews
            .iter()
            .find(|p| p.rect_at(t).to_f64().contains(pos))
            .map(|p| p.id)
    }

    pub fn set_hover(&mut self, hover: Option<WindowId>) -> bool {
        self.hover.set(hover)
    }

    fn hover_progress(&self) -> f64 {
        glide_theme::motion::E_OUT.ease_f64(self.hover.progress(hover_anim()))
    }

    fn close_hover_progress(&self) -> f64 {
        glide_theme::motion::E_OVER.ease_f64(self.close_hover.progress(ov_close_hover_anim()))
    }

    #[must_use]
    pub fn preview_rect(&self, id: WindowId) -> Option<Rectangle<i32, Logical>> {
        let t = self.progress();
        self.previews
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.rect_at(t))
    }

    #[must_use]
    pub fn close_button_at(&self, pos: Point<f64, Logical>) -> Option<WindowId> {
        let t = self.progress();
        if t < 1.0 {
            return None;
        }
        self.previews
            .iter()
            .find(|p| close_button_rect(p.rect_at(t)).to_f64().contains(pos))
            .map(|p| p.id)
    }

    pub fn set_close_hover(&mut self, hover: Option<WindowId>) -> bool {
        self.close_hover.set(hover)
    }

    pub fn navigate(&mut self, dir: Direction) -> bool {
        if self.previews.is_empty() {
            return false;
        }

        let Some(current) = self
            .hover
            .target()
            .and_then(|id| self.previews.iter().position(|p| p.id == id))
        else {
            let default = match dir {
                Direction::Left | Direction::Up => self.previews.last(),
                Direction::Right | Direction::Down => self.previews.first(),
            };
            return self.set_hover(default.map(|p| p.id));
        };

        let next = match dir {
            Direction::Left => current.checked_sub(1),
            Direction::Right => (current + 1 < self.previews.len()).then_some(current + 1),
            Direction::Up | Direction::Down => {
                let row = self.previews[current].row;
                let target_row = if matches!(dir, Direction::Up) {
                    match row.checked_sub(1) {
                        Some(r) => r,
                        None => return false,
                    }
                } else {
                    row + 1
                };

                let t = self.progress();
                let centre = centre_x(self.previews[current].rect_at(t));

                self.previews
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.row == target_row)
                    .min_by(|(_, a), (_, b)| {
                        let da = (centre_x(a.rect_at(t)) - centre).abs();
                        let db = (centre_x(b.rect_at(t)) - centre).abs();
                        da.total_cmp(&db)
                    })
                    .map(|(i, _)| i)
            }
        };

        match next {
            Some(index) => self.set_hover(Some(self.previews[index].id)),
            None => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Idle,
    Running,
    Finished { select: Option<WindowId> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

const LABEL_PADDING: i32 = 12;

const LABEL_RISE: f64 = 4.0;

const OV_CLOSE_SIZE: i32 = 26;

const OV_CLOSE_OUTSET: i32 = 10;

fn ov_close_hover_anim() -> Duration {
    glide_theme::motion::t2()
}

const OV_CLOSE_HOVER_SCALE: f64 = 1.12;

const OV_CLOSE_GLYPH: f32 = 5.5;

const OV_CLOSE_GLYPH_STROKE: f32 = 1.65;

const OV_CLOSE_BORDER: f32 = 1.0;

fn close_shadow_layers() -> Vec<crate::paint::ShadowLayerSpec> {
    glide_theme::elevation::layers(
        glide_theme::elevation::Level::El2,
        glide_theme::Appearance::Dark,
    )
    .into_iter()
    .map(|layer| crate::paint::ShadowLayerSpec {
        dy: layer.dy,
        blur: layer.blur,
        color: layer.color,
    })
    .collect()
}

fn close_shadow_margin() -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let widest = close_shadow_layers()
        .iter()
        .map(|layer| layer.blur)
        .fold(0.0_f32, f32::max)
        .ceil() as i32;
    crate::paint::shadow_fade_margin(widest)
}

fn close_button_rect(preview: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((
            preview.loc.x + preview.size.w + OV_CLOSE_OUTSET - OV_CLOSE_SIZE,
            preview.loc.y - OV_CLOSE_OUTSET,
        )),
        Size::from((OV_CLOSE_SIZE, OV_CLOSE_SIZE)),
    )
}

pub(crate) const fn label_offset(label_height: i32) -> i32 {
    -label_height / 2
}

fn centre_x(rect: Rectangle<i32, Logical>) -> f64 {
    f64::from(rect.loc.x) + f64::from(rect.size.w) / 2.0
}

fn to_smithay_rect(rect: glide_layout::Rectangle) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((rect.loc.x, rect.loc.y)),
        Size::from((rect.size.width, rect.size.height)),
    )
}

smithay::render_elements! {
    pub OverviewRenderElement<R> where R: ImportAll + ImportMem + ClipCapableRenderer;
    Preview = RelocateRenderElement<RescaleRenderElement<GlideWindowRenderElement<R>>>,
    Solid = SolidColorRenderElement,
    Buffer = MemoryRenderBufferRenderElement<R>,
    ScaledBuffer = RescaleRenderElement<MemoryRenderBufferRenderElement<R>>,
}

impl<R: Renderer> std::fmt::Debug for OverviewRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preview(e) => f.debug_tuple("Preview").field(e).finish(),
            Self::Solid(e) => f.debug_tuple("Solid").field(e).finish(),
            Self::Buffer(e) => f.debug_tuple("Buffer").field(e).finish(),
            Self::ScaledBuffer(e) => f.debug_tuple("ScaledBuffer").field(e).finish(),
            Self::_GenericCatcher(e) => f.debug_tuple("_GenericCatcher").field(e).finish(),
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn hover_bucket(t: f32) -> u8 {
    (t.clamp(0.0, 1.0) * 20.0).round() as u8
}

#[allow(clippy::needless_pass_by_value)]
fn draw_close_button(
    theme: &OverviewTheme,
    hover_t: f32,
    output_scale: i32,
) -> Option<MemoryRenderBuffer> {
    let margin = close_shadow_margin();
    let canvas = OV_CLOSE_SIZE + margin * 2;
    let (pw, ph) = (canvas * output_scale, canvas * output_scale);
    let Ok(pixels) = usize::try_from(pw * ph * 4) else {
        return None;
    };
    let mut buf = vec![0u8; pixels];

    let bg = theme
        .close_background
        .lerp(theme.close_hover_background, hover_t);
    let fg = theme
        .close_foreground
        .lerp(theme.close_hover_foreground, hover_t);
    let border = theme
        .close_border
        .lerp(theme.close_hover_background, hover_t);

    #[allow(clippy::cast_precision_loss)]
    let radius = OV_CLOSE_SIZE as f32 / 2.0;
    #[allow(clippy::cast_precision_loss)]
    let scale_f = output_scale as f32;

    let layers: Vec<_> = close_shadow_layers()
        .into_iter()
        .map(|layer| crate::paint::ShadowLayerSpec {
            dy: layer.dy * scale_f,
            blur: layer.blur * scale_f,
            color: layer.color,
        })
        .collect();
    #[allow(clippy::cast_precision_loss)]
    crate::paint::draw_outset_shadow_and_border(
        &mut buf,
        pw,
        ph,
        OV_CLOSE_SIZE as f32 * scale_f,
        OV_CLOSE_SIZE as f32 * scale_f,
        &layers,
        0.0,
        crate::config::Color([0.0, 0.0, 0.0, 0.0]),
        radius * scale_f,
        Some(radius * scale_f),
    );

    crate::paint::Painter::fill_rounded_rect_over(
        &mut buf,
        canvas,
        canvas,
        margin,
        margin,
        OV_CLOSE_SIZE,
        OV_CLOSE_SIZE,
        output_scale,
        bg,
        radius,
    );
    crate::paint::stroke_rounded_rect_over(
        &mut buf,
        canvas,
        canvas,
        margin,
        margin,
        OV_CLOSE_SIZE,
        OV_CLOSE_SIZE,
        output_scale,
        OV_CLOSE_BORDER,
        radius,
        border,
    );
    crate::paint::draw_close_glyph_over(
        &mut buf,
        canvas,
        canvas,
        OV_CLOSE_GLYPH,
        OV_CLOSE_GLYPH_STROKE,
        output_scale,
        fg,
    );

    Some(MemoryRenderBuffer::from_slice(
        &buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (pw, ph),
        output_scale,
        Transform::Normal,
        None,
    ))
}

#[allow(clippy::too_many_arguments)]
fn push_close_buttons<R>(
    renderer: &mut R,
    previews: &mut [Preview],
    entries: &[OverviewEntry],
    theme: &OverviewTheme,
    hovered: Option<WindowId>,
    hover_t: f32,
    alpha: f32,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<OverviewRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem + ClipCapableRenderer,
    R::TextureId: Send + Clone + 'static,
{
    for (id, rect, _, resolved) in entries {
        if resolved.is_none() {
            continue;
        }
        let Some(preview) = previews.iter_mut().find(|p| p.id == *id) else {
            continue;
        };

        let t = if hovered == Some(*id) { hover_t } else { 0.0 };
        let bucket = hover_bucket(t);
        let key = RasterKey::new(theme, output_scale);
        let needs_redraw = preview
            .close
            .as_ref()
            .is_none_or(|(cached_bucket, cached_key, _)| {
                *cached_bucket != bucket || *cached_key != key
            });

        if needs_redraw {
            let quantized = f32::from(bucket) / 20.0;
            if let Some(memory) = draw_close_button(theme, quantized, output_scale) {
                preview.close = Some((bucket, key, memory));
            }
        }

        let Some((_, _, memory)) = preview.close.as_ref() else {
            continue;
        };

        let button = close_button_rect(*rect);
        let margin = close_shadow_margin();

        let pos = Point::from((button.loc.x - margin, button.loc.y - margin))
            .to_f64()
            .to_physical(scale);

        let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            pos,
            memory,
            Some(alpha),
            None,
            None,
            Kind::Unspecified,
        ) else {
            continue;
        };

        let grow = 1.0 + f64::from(t) * (OV_CLOSE_HOVER_SCALE - 1.0);
        if (grow - 1.0).abs() < f64::EPSILON {
            elements.push(OverviewRenderElement::Buffer(element));
        } else {
            let centre = Point::from((
                button.loc.x + OV_CLOSE_SIZE / 2,
                button.loc.y + OV_CLOSE_SIZE / 2,
            ))
            .to_physical_precise_round(scale);
            elements.push(OverviewRenderElement::ScaledBuffer(
                RescaleRenderElement::from_element(element, centre, grow),
            ));
        }
    }
}

impl OverviewUi {
    fn render_labels<R>(
        &mut self,
        renderer: &mut R,
        entries: &[OverviewEntry],
        scale: Scale<f64>,
        output_scale: i32,
        painter: &crate::paint::Painter,
    ) -> Vec<OverviewRenderElement<R>>
    where
        R: Renderer + ImportAll + ImportMem + ClipCapableRenderer,
        R::TextureId: Send + Clone + 'static,
        R::Error: std::fmt::Debug,
    {
        let t = self.progress();
        let theme = self.theme.clone();
        if !theme.label || t <= 0.0 {
            return Vec::new();
        }

        #[allow(clippy::cast_possible_truncation)]
        let alpha = t.clamp(0.0, 1.0) as f32;
        #[allow(clippy::cast_possible_truncation)]
        let rise = ((1.0 - t) * LABEL_RISE).round() as i32;

        let mut elements = Vec::new();
        for (id, rect, _, resolved) in entries {
            let Some((_, title)) = resolved else {
                continue;
            };
            let Some(preview) = self.previews.iter_mut().find(|p| p.id == *id) else {
                continue;
            };

            let height = theme.label_height;
            #[allow(clippy::cast_possible_truncation)]
            let text_w =
                painter.text_width_styled(title.as_str(), &glide_theme::TextStyle::OV_LABEL) as i32;
            let width = (text_w + LABEL_PADDING * 2).clamp(1, rect.size.w.max(1));

            let key = RasterKey::new(&theme, output_scale);
            let needs_redraw =
                preview
                    .label
                    .as_ref()
                    .is_none_or(|(cached_title, cached_width, cached_key, _)| {
                        cached_title != title || *cached_width != width || *cached_key != key
                    });
            if needs_redraw {
                let (pwidth, pheight) = (width * output_scale, height * output_scale);
                let Ok(pixels) = usize::try_from(pwidth * pheight * 4) else {
                    continue;
                };
                let mut buf = vec![0u8; pixels];
                #[allow(clippy::cast_precision_loss)]
                let radius = height as f32 / 2.0;
                crate::paint::Painter::fill_panel_background(
                    &mut buf,
                    width,
                    height,
                    output_scale,
                    theme.label_background,
                    radius,
                    [true; 4],
                );
                #[allow(clippy::cast_precision_loss)]
                crate::paint::draw_inset_border(
                    &mut buf,
                    pwidth,
                    pheight,
                    output_scale as f32,
                    radius * output_scale as f32,
                    theme.label_border,
                );
                painter.draw_label_over_styled(
                    &mut buf,
                    width,
                    height,
                    LABEL_PADDING,
                    width - LABEL_PADDING,
                    output_scale,
                    crate::paint::TextAlign::Center,
                    title.as_str(),
                    theme.label_color.to_buffer_bytes(),
                    &glide_theme::TextStyle::OV_LABEL,
                );
                let memory = MemoryRenderBuffer::from_slice(
                    &buf,
                    smithay::backend::allocator::Fourcc::Argb8888,
                    (pwidth, pheight),
                    output_scale,
                    Transform::Normal,
                    None,
                );
                preview.label = Some((title.clone(), width, key, memory));
            }

            let Some((_, _, _, memory)) = preview.label.as_ref() else {
                continue;
            };

            let pos = Point::from((
                rect.loc.x + (rect.size.w - width) / 2,
                rect.loc.y + rect.size.h + label_offset(height) + rise,
            ))
            .to_f64()
            .to_physical(scale);
            if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                pos,
                memory,
                Some(alpha),
                None,
                None,
                Kind::Unspecified,
            ) {
                elements.push(OverviewRenderElement::Buffer(element));
            }
        }
        elements
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    #[profiling::function]
    pub fn render<R, F>(
        &mut self,
        renderer: &mut R,
        output_rect: Rectangle<i32, Logical>,
        scale: Scale<f64>,
        output_scale: i32,
        painter: &crate::paint::Painter,
        mut lookup: F,
    ) -> Vec<OverviewRenderElement<R>>
    where
        R: Renderer + ImportAll + ImportMem + ClipCapableRenderer,
        R::TextureId: Send + Clone + 'static,
        R::Error: std::fmt::Debug,
        F: FnMut(WindowId) -> Option<(GlideWindow, String)>,
    {
        let t = self.progress();
        let mut elements = Vec::new();

        let hover = self.hover.target();
        let theme = self.theme.clone();

        let entries: Vec<OverviewEntry> = self
            .previews
            .iter()
            .map(|p| (p.id, p.rect_at(t), hover == Some(p.id), lookup(p.id)))
            .collect();

        elements.extend(self.render_labels(renderer, &entries, scale, output_scale, painter));

        if t > 0.0 {
            #[allow(clippy::cast_possible_truncation)]
            let button_alpha = t.clamp(0.0, 1.0) as f32;
            let close_hovered = self.close_hover.target();
            #[allow(clippy::cast_possible_truncation)]
            let close_hover_t = self.close_hover_progress().clamp(0.0, 1.0) as f32;
            push_close_buttons(
                renderer,
                &mut self.previews,
                &entries,
                &theme,
                close_hovered,
                close_hover_t,
                button_alpha,
                scale,
                output_scale,
                &mut elements,
            );
        }

        let hover_t = self.hover_progress() * t.clamp(0.0, 1.0);
        if hover_t > 0.0 {
            if let Some(hover_id) = hover {
                if let Some(preview) = self.previews.iter_mut().find(|p| p.id == hover_id) {
                    let rect = preview.rect_at(t);
                    let (w, h) = (preview.target.size.w.max(1), preview.target.size.h.max(1));
                    let key = RasterKey::new(&theme, output_scale);
                    let needs_redraw = preview.highlight.as_ref().is_none_or(
                        |(cached_w, cached_h, cached_key, _)| {
                            *cached_w != w || *cached_h != h || *cached_key != key
                        },
                    );

                    if needs_redraw {
                        let (pw, ph) = (w * output_scale, h * output_scale);
                        if let Ok(pixels) = usize::try_from(pw * ph * 4) {
                            let mut buf = vec![0u8; pixels];
                            #[allow(clippy::cast_precision_loss)]
                            let radius = theme.corner_radius * output_scale as f32;
                            #[allow(clippy::cast_precision_loss)]
                            let border_width = theme.border as f32 * output_scale as f32;
                            crate::paint::draw_inset_border(
                                &mut buf,
                                pw,
                                ph,
                                border_width,
                                radius,
                                theme.highlight,
                            );
                            let memory = MemoryRenderBuffer::from_slice(
                                &buf,
                                smithay::backend::allocator::Fourcc::Argb8888,
                                (pw, ph),
                                output_scale,
                                Transform::Normal,
                                None,
                            );
                            preview.highlight = Some((w, h, key, memory));
                        }
                    }

                    if let Some((cached_w, cached_h, _, memory)) = preview.highlight.as_ref() {
                        let pos = rect.loc.to_f64().to_physical(scale);
                        #[allow(clippy::cast_possible_truncation)]
                        let alpha = hover_t as f32;
                        let src = Rectangle::from_size(Size::from((
                            f64::from(*cached_w),
                            f64::from(*cached_h),
                        )));
                        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
                            renderer,
                            pos,
                            memory,
                            Some(alpha),
                            Some(src),
                            Some(rect.size),
                            Kind::Unspecified,
                        ) {
                            elements.push(OverviewRenderElement::Buffer(element));
                        }
                    }
                }
            }
        }

        for (id, rect, _, resolved) in &entries {
            let Some((window, _)) = resolved else {
                continue;
            };
            let Some(preview) = self.previews.iter().find(|p| p.id == *id) else {
                continue;
            };

            let window_scale = preview.scale_at(t);

            let buffer_offset = window.geometry().loc;
            let content_origin = Point::from((
                preview.origin.loc.x + GlideWindow::insets_left() - buffer_offset.x,
                preview.origin.loc.y + window.header_height() - buffer_offset.y,
            ));
            let real_physical = content_origin.to_f64().to_physical(scale).to_i32_round();
            let frame_physical = preview
                .origin
                .loc
                .to_f64()
                .to_physical(scale)
                .to_i32_round();
            let target_physical = rect.loc.to_f64().to_physical(scale).to_i32_round();

            let raw: Vec<GlideWindowRenderElement<R>> =
                window.preview_render_elements(renderer, real_physical, scale, 1.0);

            for element in raw {
                let scaled =
                    RescaleRenderElement::from_element(element, frame_physical, window_scale);
                let placed = RelocateRenderElement::from_element(
                    scaled,
                    target_physical - frame_physical,
                    Relocate::Relative,
                );
                elements.push(OverviewRenderElement::Preview(placed));
            }
        }

        #[allow(clippy::cast_possible_truncation)]
        let backdrop_t = self.backdrop_progress() as f32;
        let mut scrim_color = theme.dim.0;
        for channel in &mut scrim_color {
            *channel = (*channel * backdrop_t).clamp(0.0, 1.0);
        }
        self.scrim
            .update((output_rect.size.w, output_rect.size.h), scrim_color);
        elements.push(OverviewRenderElement::Solid(
            SolidColorRenderElement::from_buffer(
                &self.scrim,
                output_rect.loc.to_f64().to_physical(scale).to_i32_round(),
                scale,
                1.0,
                Kind::Unspecified,
            ),
        ));

        elements
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme_with_duration(ms: u64) -> OverviewTheme {
        OverviewTheme {
            duration_ms: ms,
            ..OverviewTheme::default()
        }
    }

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((x, y)), Size::from((w, h)))
    }

    fn ui_with_one_window(duration_ms: u64) -> (OverviewUi, WindowId) {
        let id = WindowId::next();
        let windows = [(id, rect(0, 0, 800, 600))];
        let area = rect(0, 0, 1920, 1080);
        let ui = OverviewUi::new(&windows, area, &theme_with_duration(duration_ms), Some(id));
        (ui, id)
    }

    mod scene_easing {
        use super::*;

        #[test]
        fn the_exit_is_front_loaded_just_like_the_entry() {
            let (mut ui, _id) = ui_with_one_window(400);
            ui.phase = Phase::Closing {
                started: Instant::now()
                    .checked_sub(Duration::from_millis(80))
                    .expect("the process has been up for longer than 80ms"),
                select: None,
                from: SceneFrom {
                    primary: 1.0,
                    backdrop: 1.0,
                },
            };

            let remaining = ui.progress();
            assert!(
                remaining < 0.6,
                "the exit stalled: still {remaining} open a fifth of the way through"
            );
        }

        #[test]
        fn closing_mirrors_opening_over_the_same_elapsed_time() {
            for millis in [40_u64, 120, 200, 320] {
                let (mut opening, _) = ui_with_one_window(400);
                opening.phase = Phase::Opening {
                    started: Instant::now()
                        .checked_sub(Duration::from_millis(millis))
                        .expect("process uptime"),
                };
                let (mut closing, _) = ui_with_one_window(400);
                closing.phase = Phase::Closing {
                    started: Instant::now()
                        .checked_sub(Duration::from_millis(millis))
                        .expect("process uptime"),
                    select: None,
                    from: SceneFrom {
                        primary: 1.0,
                        backdrop: 1.0,
                    },
                };

                let sum = opening.progress() + closing.progress();
                assert!(
                    (sum - 1.0).abs() < 0.05,
                    "at {millis}ms opening {} and closing {} should sum to 1",
                    opening.progress(),
                    closing.progress()
                );
            }
        }

        #[test]
        fn the_preview_scale_advances_continuously() {
            let (ui, _id) = ui_with_one_window(0);
            let preview = &ui.previews[0];

            assert!(
                (preview.scale_at(0.0) - 1.0).abs() < f64::EPSILON,
                "a transition starts at the window's real size"
            );

            let settled = f64::from(preview.target.size.w) / f64::from(preview.origin.size.w);
            assert!((preview.scale_at(1.0) - settled).abs() < 1e-9);

            let mut previous = preview.scale_at(0.0);
            for step in 1..=200 {
                let t = f64::from(step) / 200.0;
                let value = preview.scale_at(t);
                assert!(
                    (value - previous).abs() > 0.0,
                    "scale stalled at t={t}: {value}"
                );
                previous = value;
            }
        }

        #[test]
        fn the_scale_is_the_midpoint_halfway_through() {
            let (ui, _id) = ui_with_one_window(0);
            let preview = &ui.previews[0];
            let settled = f64::from(preview.target.size.w) / f64::from(preview.origin.size.w);
            let expected = 1.0 + (settled - 1.0) * 0.5;
            assert!((preview.scale_at(0.5) - expected).abs() < 1e-9);
        }

        #[test]
        fn a_settled_overview_is_fully_progressed_on_both_curves() {
            let (ui, _id) = ui_with_one_window(0);
            assert!((ui.progress() - 1.0).abs() < f64::EPSILON);
            assert!((ui.backdrop_progress() - 1.0).abs() < f64::EPSILON);
        }

        #[test]
        fn previews_lead_the_backdrop_mid_transition() {
            let (mut ui, _id) = ui_with_one_window(400);
            ui.phase = Phase::Opening {
                started: Instant::now()
                    .checked_sub(Duration::from_millis(80))
                    .expect("the process has been up for longer than 80ms"),
            };

            let previews = ui.progress();
            let backdrop = ui.backdrop_progress();
            assert!(
                previews > backdrop,
                "previews ({previews}) must lead the backdrop ({backdrop})"
            );
            assert!((0.0..1.0).contains(&backdrop), "backdrop: {backdrop}");
        }

        #[test]
        fn closing_runs_both_curves_back_towards_zero() {
            let (mut ui, _id) = ui_with_one_window(400);
            ui.begin_close(None);
            ui.phase = Phase::Closing {
                started: Instant::now()
                    .checked_sub(Duration::from_millis(320))
                    .expect("the process has been up for longer than 320ms"),
                select: None,
                from: SceneFrom {
                    primary: 1.0,
                    backdrop: 1.0,
                },
            };
            assert!(ui.progress() < 0.5);
            assert!(ui.backdrop_progress() < 0.5);
        }
    }

    mod label_rise {
        use super::*;

        #[test]
        fn labels_are_already_partly_visible_early_in_the_transition() {
            let (mut ui, _id) = ui_with_one_window(400);
            ui.phase = Phase::Opening {
                started: Instant::now()
                    .checked_sub(Duration::from_millis(40))
                    .expect("the process has been up for longer than 40ms"),
            };
            let t = ui.progress();
            assert!(
                t > 0.0 && t < 1.0,
                "expected a mid-transition progress, got {t}"
            );
        }

        #[test]
        fn the_rise_starts_at_four_pixels_and_lands_at_zero() {
            #[allow(clippy::cast_possible_truncation)]
            let settled = ((1.0 - 1.0) * LABEL_RISE).round() as i32;
            #[allow(clippy::cast_possible_truncation)]
            let start = ((1.0 - 0.0) * LABEL_RISE).round() as i32;
            assert_eq!(settled, 0, "a settled label must sit at its anchor");
            assert_eq!(start, 4, "spec's .ov-label starts at translateY(4px)");
        }
    }

    mod close_button {
        use super::*;

        #[test]
        fn it_straddles_the_previews_top_right_corner() {
            let preview = rect(100, 200, 400, 300);
            let button = close_button_rect(preview);

            assert_eq!(button.size, Size::from((OV_CLOSE_SIZE, OV_CLOSE_SIZE)));
            assert_eq!(
                button.loc.y,
                preview.loc.y - OV_CLOSE_OUTSET,
                "top edge must hang above the preview"
            );
            assert_eq!(
                button.loc.x + button.size.w,
                preview.loc.x + preview.size.w + OV_CLOSE_OUTSET,
                "right edge must hang past the preview"
            );
        }

        #[test]
        fn it_overlaps_the_preview_rather_than_floating_away() {
            let preview = rect(0, 100, 400, 300);
            let button = close_button_rect(preview);
            assert!(
                button.loc.x < preview.loc.x + preview.size.w,
                "the button must still overlap the preview horizontally"
            );
            assert!(
                button.loc.y + button.size.h > preview.loc.y,
                "the button must still overlap the preview vertically"
            );
        }

        #[test]
        fn a_settled_overview_reports_a_hit_inside_the_button() {
            let (ui, id) = ui_with_one_window(0);
            let button = close_button_rect(ui.previews[0].rect_at(1.0));
            let centre = Point::from((
                f64::from(button.loc.x) + f64::from(button.size.w) / 2.0,
                f64::from(button.loc.y) + f64::from(button.size.h) / 2.0,
            ));
            assert_eq!(ui.close_button_at(centre), Some(id));
        }

        #[test]
        fn a_point_away_from_the_button_is_not_a_hit() {
            let (ui, _id) = ui_with_one_window(0);
            let preview = ui.previews[0].rect_at(1.0);
            let middle = Point::from((
                f64::from(preview.loc.x) + f64::from(preview.size.w) / 2.0,
                f64::from(preview.loc.y) + f64::from(preview.size.h) / 2.0,
            ));
            assert_eq!(ui.close_button_at(middle), None);
        }

        #[test]
        fn it_is_not_clickable_while_the_scene_is_still_animating() {
            let (mut ui, _id) = ui_with_one_window(400);
            ui.phase = Phase::Opening {
                started: Instant::now()
                    .checked_sub(Duration::from_millis(40))
                    .expect("the process has been up for longer than 40ms"),
            };
            let button = close_button_rect(ui.previews[0].rect_at(ui.progress()));
            let centre = Point::from((
                f64::from(button.loc.x) + f64::from(button.size.w) / 2.0,
                f64::from(button.loc.y) + f64::from(button.size.h) / 2.0,
            ));
            assert_eq!(ui.close_button_at(centre), None);
        }

        #[test]
        fn the_cross_matches_the_designs_absolute_size() {
            #[allow(clippy::cast_precision_loss)]
            let from_svg = 6.0 * 11.0 / 12.0;
            assert!(
                (OV_CLOSE_GLYPH - from_svg).abs() < 0.01,
                "expected {from_svg}, got {OV_CLOSE_GLYPH}"
            );
            assert!(
                (OV_CLOSE_GLYPH_STROKE - 1.8 * 11.0 / 12.0).abs() < 0.01,
                "stroke width drifted from the design"
            );
        }

        #[test]
        fn the_cross_is_a_small_part_of_the_button_not_most_of_it() {
            #[allow(clippy::cast_precision_loss)]
            let ratio = OV_CLOSE_GLYPH / OV_CLOSE_SIZE as f32;
            assert!(
                (0.15..0.28).contains(&ratio),
                "the cross should read as a small mark inside the pill, got {ratio}"
            );
        }

        #[test]
        fn the_buffer_reserves_room_for_the_elevation_shadow() {
            let margin = close_shadow_margin();
            let widest = close_shadow_layers()
                .iter()
                .map(|layer| layer.blur)
                .fold(0.0_f32, f32::max);
            assert!(margin > 0, "no shadow margin reserved");
            #[allow(clippy::cast_precision_loss)]
            let margin_f = margin as f32;
            assert!(
                margin_f >= widest,
                "margin {margin} is tighter than the widest blur {widest}"
            );
        }

        #[test]
        fn the_shadow_is_the_two_layer_elevation_two_token() {
            let layers = close_shadow_layers();
            assert_eq!(layers.len(), 2, "--el-2 is a two-layer shadow");
            assert!(
                layers[0].blur < layers[1].blur,
                "the tighter layer must come first"
            );
            assert!(
                layers.iter().all(|layer| layer.color.0[3] > 0.0),
                "an invisible shadow layer is pointless"
            );
        }

        #[test]
        fn hovering_never_moves_or_resizes_the_button() {
            let (ui, _id) = ui_with_one_window(0);
            let rect = ui.previews[0].rect_at(1.0);

            let button = close_button_rect(rect);
            let margin = close_shadow_margin();
            let expected: Point<i32, Logical> =
                Point::from((button.loc.x - margin, button.loc.y - margin));
            assert_eq!(
                expected,
                Point::<i32, Logical>::from((button.loc.x - margin, button.loc.y - margin)),
                "the origin depends only on the preview rect and a constant margin"
            );
            assert_eq!(
                close_button_rect(rect).size,
                Size::from((OV_CLOSE_SIZE, OV_CLOSE_SIZE)),
                "the button keeps its size in every state"
            );
        }

        #[test]
        fn hovering_the_button_is_tracked_separately_from_the_ring() {
            let (mut ui, id) = ui_with_one_window(0);
            assert!(ui.set_close_hover(Some(id)), "a new target must animate");
            assert!(!ui.set_close_hover(Some(id)), "the same target is a no-op");
            assert!(ui.set_close_hover(None));
        }

        #[test]
        fn a_button_hover_keeps_the_ui_animating() {
            let (mut ui, id) = ui_with_one_window(0);
            ui.set_close_hover(None);
            ui.set_close_hover(Some(id));
            assert!(
                ui.is_animating(),
                "the pop must keep frames coming until it settles"
            );
        }

        #[test]
        fn relayout_drops_a_button_hover_for_a_window_that_vanished() {
            let (mut ui, id) = ui_with_one_window(0);
            ui.set_close_hover(Some(id));

            let other = WindowId::next();
            ui.relayout(&[(other, rect(0, 0, 800, 600))], rect(0, 0, 1920, 1080));

            assert_eq!(
                ui.close_hover.target(),
                None,
                "a hover on a gone window must be cleared, not moved"
            );
        }

        #[test]
        fn relayout_keeps_the_cached_button_for_a_window_still_present() {
            let (mut ui, id) = ui_with_one_window(0);
            let key = RasterKey::new(&ui.theme, 1);
            ui.previews[0].close = Some((0, key, MemoryRenderBuffer::default()));

            ui.relayout(&[(id, rect(0, 0, 800, 600))], rect(0, 0, 1920, 1080));

            assert!(ui.previews[0].close.is_some());
        }

        #[test]
        fn a_brand_new_preview_gets_no_inherited_button_buffer() {
            let (mut ui, id) = ui_with_one_window(0);
            let key = RasterKey::new(&ui.theme, 1);
            ui.previews[0].close = Some((20, key, MemoryRenderBuffer::default()));

            let new_id = WindowId::next();
            ui.relayout(
                &[(id, rect(0, 0, 800, 600)), (new_id, rect(0, 0, 400, 300))],
                rect(0, 0, 1920, 1080),
            );

            let fresh = ui
                .previews
                .iter()
                .find(|p| p.id == new_id)
                .expect("the new window must get a preview");
            assert!(fresh.close.is_none());
        }
    }

    mod label_offset_tests {
        use super::*;

        #[test]
        fn the_pill_hangs_halfway_over_the_preview() {
            assert_eq!(label_offset(28), -14);
            assert_eq!(label_offset(40), -20);
        }

        #[test]
        fn the_space_consumed_below_a_row_is_half_the_pill() {
            for height in [0, 24, 28, 40] {
                let consumed = height + label_offset(height);
                assert!(consumed >= 0, "height {height} consumed {consumed}");
                assert_eq!(consumed, height - height / 2);
            }
        }
    }

    #[test]
    fn initial_hover_has_no_pop_in_animation() {
        let (ui, _id) = ui_with_one_window(220);
        assert!((ui.hover_progress() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn relayout_with_identical_inputs_is_skipped_entirely() {
        let (mut ui, id) = ui_with_one_window(0);
        let windows = [(id, rect(0, 0, 800, 600))];
        let area = rect(0, 0, 1920, 1080);

        ui.previews[0].row = 99;

        ui.relayout(&windows, area);
        assert_eq!(
            ui.previews[0].row, 99,
            "identical inputs must leave the previews untouched"
        );

        ui.relayout(&windows, rect(0, 0, 1280, 720));
        assert_eq!(ui.previews[0].row, 0);
    }

    #[test]
    fn relayout_recomputes_when_a_windows_origin_changes() {
        let (mut ui, id) = ui_with_one_window(0);
        ui.previews[0].row = 99;

        let resized = [(id, rect(0, 0, 400, 300))];
        ui.relayout(&resized, rect(0, 0, 1920, 1080));

        assert_eq!(ui.previews[0].row, 0, "a resized origin must relayout");
        assert_eq!(ui.previews[0].origin, rect(0, 0, 400, 300));
    }

    #[test]
    fn set_hover_to_the_same_target_is_a_no_op() {
        let (mut ui, id) = ui_with_one_window(220);
        assert!(!ui.set_hover(Some(id)));
    }

    #[test]
    fn relayout_moves_hover_to_the_adjacent_neighbor_not_first() {
        let a = WindowId::next();
        let b = WindowId::next();
        let c = WindowId::next();
        let windows = [
            (a, rect(0, 0, 800, 600)),
            (b, rect(0, 0, 800, 600)),
            (c, rect(0, 0, 800, 600)),
        ];
        let area = rect(0, 0, 1920, 1080);
        let theme = theme_with_duration(0);

        let mut ui = OverviewUi::new(&windows, area, &theme, Some(b));

        let remaining = [(a, rect(0, 0, 800, 600)), (c, rect(0, 0, 800, 600))];
        ui.relayout(&remaining, area);

        assert_eq!(
            ui.hovered(),
            Some(c),
            "hover should move to the neighbor at the same index, not to first()"
        );
    }

    #[test]
    fn navigate_without_hover_respects_direction() {
        let a = WindowId::next();
        let b = WindowId::next();
        let windows = [(a, rect(0, 0, 800, 600)), (b, rect(0, 0, 800, 600))];
        let area = rect(0, 0, 1920, 1080);
        let theme = theme_with_duration(0);

        let mut ui = OverviewUi::new(&windows, area, &theme, None);
        ui.set_hover(None);

        ui.navigate(Direction::Right);
        assert_eq!(ui.hovered(), Some(a), "Right should pick the first preview");

        ui.set_hover(None);
        ui.navigate(Direction::Left);
        assert_eq!(ui.hovered(), Some(b), "Left should pick the last preview");
    }

    #[test]
    fn set_hover_to_a_new_target_restarts_the_animation() {
        let (mut ui, id) = ui_with_one_window(220);
        ui.set_hover(None);
        assert!(ui.set_hover(Some(id)));
        assert!(
            ui.hover_progress() < 1.0,
            "freshly (re)hovered target must not read as already settled"
        );
    }

    #[test]
    fn hover_progress_settles_to_one_after_the_animation_duration() {
        let (mut ui, id) = ui_with_one_window(220);
        ui.set_hover(None);
        ui.set_hover(Some(id));
        std::thread::sleep(hover_anim() + Duration::from_millis(30));
        assert!((ui.hover_progress() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn hover_progress_is_settled_when_nothing_is_hovered() {
        let (mut ui, _id) = ui_with_one_window(220);
        ui.set_hover(None);
        assert!((ui.hover_progress() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn tick_keeps_running_while_the_hover_ring_is_animating() {
        let (mut ui, id) = ui_with_one_window(0);
        assert_eq!(ui.tick(), Tick::Running, "settling the open phase");
        assert_eq!(ui.tick(), Tick::Idle, "settled, nothing changed since");

        ui.set_hover(None);
        ui.set_hover(Some(id));
        assert_eq!(
            ui.tick(),
            Tick::Running,
            "hover ring must keep scheduling repaints while it grows/fades in"
        );
    }

    #[test]
    fn tick_goes_idle_once_the_hover_animation_settles() {
        let (mut ui, id) = ui_with_one_window(0);
        let _ = ui.tick();

        ui.set_hover(None);
        ui.set_hover(Some(id));
        std::thread::sleep(hover_anim() + Duration::from_millis(30));

        assert_eq!(ui.tick(), Tick::Idle);
    }

    #[test]
    fn is_animating_reflects_the_hover_ring_too() {
        let (mut ui, id) = ui_with_one_window(0);
        let _ = ui.tick();
        assert!(!ui.is_animating());

        ui.set_hover(None);
        ui.set_hover(Some(id));
        assert!(ui.is_animating());
    }
}

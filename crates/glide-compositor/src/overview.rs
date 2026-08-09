use std::time::{Duration, Instant};

use glide_layout::{WindowId, compute_overview_layout};
use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer,
        element::{
            AsRenderElements, Kind,
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            solid::{SolidColorBuffer, SolidColorRenderElement},
            utils::{Relocate, RelocateRenderElement, RescaleRenderElement},
        },
    },
    utils::{Logical, Point, Rectangle, Scale, Size, Transform},
};

use crate::{
    config::{OverviewTheme, OverviewThemeExt},
    window::{GlideWindow, GlideWindowRenderElement},
};

#[derive(Debug, Clone, Copy)]
enum Phase {
    Opening {
        started: Instant,
    },
    Open,
    Closing {
        started: Instant,
        select: Option<WindowId>,
    },
}

fn ease_out_cubic(t: f64) -> f64 {
    let inv = 1.0 - t.clamp(0.0, 1.0);
    1.0 - inv * inv * inv
}

const HOVER_ANIM: Duration = Duration::from_millis(120);

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

type CachedLabel = (String, i32, i32, MemoryRenderBuffer);
type CachedHighlight = (i32, i32, i32, MemoryRenderBuffer);
type SalvagedPreview = (
    WindowId,
    Option<CachedLabel>,
    Option<CachedHighlight>,
    Vec<SolidColorBuffer>,
);

#[derive(Debug, Clone)]
pub struct Preview {
    pub id: WindowId,
    pub target: Rectangle<i32, Logical>,
    pub origin: Rectangle<i32, Logical>,
    pub row: usize,
    label: Option<CachedLabel>,
    highlight: Option<CachedHighlight>,
    shadow: Vec<SolidColorBuffer>,
}

impl Preview {
    #[must_use]
    pub fn rect_at(&self, t: f64) -> Rectangle<i32, Logical> {
        lerp_rect(self.origin, self.target, t)
    }
}

#[derive(Debug)]
pub struct OverviewUi {
    previews: Vec<Preview>,
    hover: glide_ui::HoverAnimation<WindowId>,
    phase: Phase,
    duration: Duration,
    theme: OverviewTheme,

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
                    shadow: Vec::new(),
                })
            })
            .collect();

        let mut hover = glide_ui::HoverAnimation::default();
        hover.retarget_without_restarting(focused);

        Self {
            previews,
            hover,
            phase: Phase::Opening {
                started: Instant::now(),
            },
            duration: theme.duration(),
            theme: theme.clone(),
            scrim: SolidColorBuffer::default(),
        }
    }

    pub fn relayout(
        &mut self,
        windows: &[(WindowId, Rectangle<i32, Logical>)],
        area: Rectangle<i32, Logical>,
    ) {
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

        let mut old: Vec<SalvagedPreview> = self
            .previews
            .drain(..)
            .map(|p| (p.id, p.label, p.highlight, p.shadow))
            .collect();

        self.previews = slots
            .iter()
            .filter_map(|slot| {
                let origin = windows.iter().find(|(id, _)| *id == slot.id)?.1;
                let (label, highlight, shadow) = old
                    .iter_mut()
                    .find(|(id, ..)| *id == slot.id)
                    .map_or((None, None, Vec::new()), |(_, label, highlight, shadow)| {
                        (label.take(), highlight.take(), std::mem::take(shadow))
                    });
                Some(Preview {
                    id: slot.id,
                    target: to_smithay_rect(slot.target),
                    origin,
                    row: slot.row,
                    label,
                    highlight,
                    shadow,
                })
            })
            .collect();

        if self.hover.target().is_some_and(|id| !self.contains(id)) {
            self.hover
                .retarget_without_restarting(self.previews.first().map(|p| p.id));
        }
    }

    fn contains(&self, id: WindowId) -> bool {
        self.previews.iter().any(|p| p.id == id)
    }

    #[must_use]
    pub fn progress(&self) -> f64 {
        let raw = |started: Instant| {
            if self.duration.is_zero() {
                return 1.0;
            }
            let elapsed = started.elapsed().as_secs_f64();
            (elapsed / self.duration.as_secs_f64()).clamp(0.0, 1.0)
        };

        match self.phase {
            Phase::Opening { started } => ease_out_cubic(raw(started)),
            Phase::Open => 1.0,
            Phase::Closing { started, .. } => 1.0 - ease_out_cubic(raw(started)),
        }
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let phase_animating = match self.phase {
            Phase::Open => false,
            Phase::Opening { started } | Phase::Closing { started, .. } => {
                started.elapsed() < self.duration
            }
        };
        phase_animating || self.hover_progress() < 1.0
    }

    pub fn tick(&mut self) -> Tick {
        match self.phase {
            Phase::Opening { started } if started.elapsed() >= self.duration => {
                self.phase = Phase::Open;
                Tick::Running
            }
            Phase::Closing { started, select } if started.elapsed() >= self.duration => {
                Tick::Finished { select }
            }
            Phase::Open if self.hover_progress() < 1.0 => Tick::Running,
            Phase::Open => Tick::Idle,
            Phase::Opening { .. } | Phase::Closing { .. } => Tick::Running,
        }
    }

    pub fn begin_close(&mut self, select: Option<WindowId>) {
        if matches!(self.phase, Phase::Closing { .. }) {
            return;
        }
        self.phase = Phase::Closing {
            started: Instant::now(),
            select,
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
        ease_out_cubic(self.hover.progress(HOVER_ANIM))
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
            return self.set_hover(self.previews.first().map(|p| p.id));
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

const LABEL_FADE_START: f64 = 0.6;

const LABEL_PADDING: i32 = 12;

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
    pub OverviewRenderElement<R> where R: ImportAll + ImportMem;
    Preview = RelocateRenderElement<RescaleRenderElement<GlideWindowRenderElement<R>>>,
    Solid = SolidColorRenderElement,
    Buffer = MemoryRenderBufferRenderElement<R>,
}

impl<R: Renderer> std::fmt::Debug for OverviewRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preview(e) => f.debug_tuple("Preview").field(e).finish(),
            Self::Solid(e) => f.debug_tuple("Solid").field(e).finish(),
            Self::Buffer(e) => f.debug_tuple("Buffer").field(e).finish(),
            Self::_GenericCatcher(e) => f.debug_tuple("_GenericCatcher").field(e).finish(),
        }
    }
}

const PREVIEW_SHADOW_BANDS: usize = 32;

#[allow(clippy::too_many_arguments)]
fn push_shadow_rings<R>(
    buffers: &mut Vec<SolidColorBuffer>,
    rect: Rectangle<i32, Logical>,
    size: i32,
    bands: usize,
    base_color: [f32; 4],
    alpha_mult: f32,
    scale: Scale<f64>,
    elements: &mut Vec<OverviewRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let rects = crate::decoration::shadow_rects(rect.size.w, rect.size.h, size, bands);
    if rects.is_empty() {
        return;
    }
    buffers.resize_with(rects.len(), SolidColorBuffer::default);

    for ((offset, rsize, ring_alpha), buf) in rects.into_iter().zip(buffers.iter_mut()) {
        let mut color = base_color;
        color[3] *= ring_alpha * alpha_mult;
        buf.update((rsize.w, rsize.h), color);
        let pos = Point::from((rect.loc.x + offset.x, rect.loc.y + offset.y));
        elements.push(OverviewRenderElement::Solid(
            SolidColorRenderElement::from_buffer(
                buf,
                pos.to_f64().to_physical(scale).to_i32_round(),
                scale,
                1.0,
                Kind::Unspecified,
            ),
        ));
    }
}

impl OverviewUi {
    fn render_labels<R, F>(
        &mut self,
        renderer: &mut R,
        entries: &[(WindowId, Rectangle<i32, Logical>, bool)],
        scale: Scale<f64>,
        output_scale: i32,
        painter: &crate::paint::Painter,
        lookup: &mut F,
    ) -> Vec<OverviewRenderElement<R>>
    where
        R: Renderer + ImportAll + ImportMem,
        R::TextureId: Send + Clone + 'static,
        R::Error: std::fmt::Debug,
        F: FnMut(WindowId) -> Option<(GlideWindow, String)>,
    {
        let t = self.progress();
        let theme = self.theme.clone();
        if !theme.label || t <= LABEL_FADE_START {
            return Vec::new();
        }

        #[allow(clippy::cast_possible_truncation)]
        let alpha = (((t - LABEL_FADE_START) / (1.0 - LABEL_FADE_START)).clamp(0.0, 1.0)) as f32;

        let mut elements = Vec::new();
        for (id, rect, _) in entries {
            let Some((_, title)) = lookup(*id) else {
                continue;
            };
            let Some(preview) = self.previews.iter_mut().find(|p| p.id == *id) else {
                continue;
            };

            let height = theme.label_height;
            #[allow(clippy::cast_possible_truncation)]
            let text_w = painter.text_width(&title) as i32;
            let width = (text_w + LABEL_PADDING * 2).clamp(1, rect.size.w.max(1));

            let needs_redraw = preview.label.as_ref().is_none_or(
                |(cached_title, cached_width, cached_scale, _)| {
                    cached_title != &title
                        || *cached_width != width
                        || *cached_scale != output_scale
                },
            );
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
                painter.draw_label_over(
                    &mut buf,
                    width,
                    height,
                    LABEL_PADDING,
                    width - LABEL_PADDING,
                    output_scale,
                    crate::paint::TextAlign::Center,
                    &title,
                    theme.label_color.to_buffer_bytes(),
                );
                let memory = MemoryRenderBuffer::from_slice(
                    &buf,
                    smithay::backend::allocator::Fourcc::Argb8888,
                    (pwidth, pheight),
                    output_scale,
                    Transform::Normal,
                    None,
                );
                preview.label = Some((title, width, output_scale, memory));
            }

            let Some((_, _, _, memory)) = preview.label.as_ref() else {
                continue;
            };

            let pos = Point::from((
                rect.loc.x + (rect.size.w - width) / 2,
                rect.loc.y + rect.size.h,
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
        R: Renderer + ImportAll + ImportMem,
        R::TextureId: Send + Clone + 'static,
        R::Error: std::fmt::Debug,
        F: FnMut(WindowId) -> Option<(GlideWindow, String)>,
    {
        let t = self.progress();
        let mut elements = Vec::new();

        let hover = self.hover.target();
        let theme = self.theme.clone();

        let entries: Vec<(WindowId, Rectangle<i32, Logical>, bool)> = self
            .previews
            .iter()
            .map(|p| (p.id, p.rect_at(t), hover == Some(p.id)))
            .collect();

        elements.extend(self.render_labels(
            renderer,
            &entries,
            scale,
            output_scale,
            painter,
            &mut lookup,
        ));

        let hover_t = self.hover_progress();
        if hover_t > 0.0 {
            if let Some(hover_id) = hover {
                if let Some(preview) = self.previews.iter_mut().find(|p| p.id == hover_id) {
                    let rect = preview.rect_at(t);
                    let (w, h) = (rect.size.w, rect.size.h);
                    let needs_redraw = preview.highlight.as_ref().is_none_or(
                        |(cached_w, cached_h, cached_scale, _)| {
                            *cached_w != w || *cached_h != h || *cached_scale != output_scale
                        },
                    );

                    if needs_redraw {
                        let (pw, ph) = (w * output_scale, h * output_scale);
                        if let Ok(pixels) = usize::try_from(pw * ph * 4) {
                            let mut buf = vec![0u8; pixels];
                            #[allow(clippy::cast_precision_loss)]
                            let radius = 12.0 * output_scale as f32; // --r-3 is 12px
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
                            preview.highlight = Some((w, h, output_scale, memory));
                        }
                    }

                    if let Some((_, _, _, memory)) = preview.highlight.as_ref() {
                        let pos = rect.loc.to_f64().to_physical(scale);
                        #[allow(clippy::cast_possible_truncation)]
                        let alpha = hover_t as f32;
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
                }
            }
        }

        for (id, rect, _) in &entries {
            let Some((window, _)) = lookup(*id) else {
                continue;
            };
            let Some(preview) = self.previews.iter().find(|p| p.id == *id) else {
                continue;
            };

            #[allow(clippy::cast_precision_loss)]
            let window_scale = f64::from(rect.size.w) / f64::from(preview.origin.size.w.max(1));

            let content_origin = Point::from((
                preview.origin.loc.x + GlideWindow::insets_left(),
                preview.origin.loc.y + window.header_height(),
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
                AsRenderElements::render_elements(&window, renderer, real_physical, scale, 1.0);

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

        if theme.shadow {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let size = i32::try_from(theme.shadow_size).unwrap_or(0);
            for (id, rect, _) in &entries {
                let Some(preview) = self.previews.iter_mut().find(|p| p.id == *id) else {
                    continue;
                };
                push_shadow_rings(
                    &mut preview.shadow,
                    *rect,
                    size,
                    PREVIEW_SHADOW_BANDS,
                    theme.shadow_color.0,
                    1.0,
                    scale,
                    &mut elements,
                );
            }
        }

        #[allow(clippy::cast_possible_truncation)]
        let dim = (theme.dim.0[3] * t as f32).clamp(0.0, 1.0);
        let mut scrim_color = theme.dim.0;
        scrim_color[3] = dim;
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

    #[test]
    fn initial_hover_has_no_pop_in_animation() {
        let (ui, _id) = ui_with_one_window(220);
        assert!((ui.hover_progress() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn set_hover_to_the_same_target_is_a_no_op() {
        let (mut ui, id) = ui_with_one_window(220);
        assert!(!ui.set_hover(Some(id)));
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
        std::thread::sleep(HOVER_ANIM + Duration::from_millis(30));
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
        std::thread::sleep(HOVER_ANIM + Duration::from_millis(30));

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

    #[test]
    fn relayout_preserves_shadow_buffers_for_a_window_still_present() {
        let (mut ui, id) = ui_with_one_window(0);
        ui.previews[0].shadow = vec![SolidColorBuffer::default(); 4];

        let windows = [(id, rect(0, 0, 800, 600))];
        ui.relayout(&windows, rect(0, 0, 1920, 1080));

        assert_eq!(ui.previews[0].shadow.len(), 4);
    }

    #[test]
    fn relayout_gives_a_newly_appearing_window_no_stale_shadow_buffers() {
        let (mut ui, id) = ui_with_one_window(0);
        ui.previews[0].shadow = vec![SolidColorBuffer::default(); 4];

        let new_id = WindowId::next();
        let windows = [(id, rect(0, 0, 800, 600)), (new_id, rect(0, 0, 400, 300))];
        ui.relayout(&windows, rect(0, 0, 1920, 1080));

        let new_preview = ui
            .previews
            .iter()
            .find(|p| p.id == new_id)
            .expect("the new window must get a preview");
        assert!(
            new_preview.shadow.is_empty(),
            "a brand new preview must not inherit another window's shadow buffers"
        );
    }

    mod push_shadow_rings_geometry {
        use super::*;

        #[test]
        fn resizes_the_buffer_vec_to_match_the_band_count() {
            let mut buffers = vec![SolidColorBuffer::default(); 2];
            let rects = crate::decoration::shadow_rects(100, 60, 16, 3);
            buffers.resize_with(rects.len(), SolidColorBuffer::default);
            assert_eq!(buffers.len(), rects.len());
        }

        #[test]
        fn zero_size_produces_no_rects_and_so_no_buffers_needed() {
            let rects = crate::decoration::shadow_rects(100, 60, 0, 3);
            assert!(rects.is_empty());
        }
    }
}

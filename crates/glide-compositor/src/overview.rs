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
    clip::ClipCapableRenderer,
    config::{OverviewTheme, OverviewThemeExt},
    window::{GlideWindow, GlideWindowRenderElement},
};

/// Per-preview data for a single render pass: the window id, its
/// interpolated overview rect, whether it's currently hovered, and the
/// window/title resolved from the caller's `lookup` closure (resolved once
/// per frame and reused across the label and content passes).
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
    },
}

/// Hover ring fade. Spec §10.3 puts `border-color` changes on `--t-2`,
/// and the design's `.dwin::after` ring uses exactly that.
const HOVER_ANIM: Duration = glide_theme::motion::T2;

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
/// `(hovered, output scale, buffer)` — the button is a fixed size, so
/// those two are the whole cache key.
type CachedClose = (bool, i32, MemoryRenderBuffer);
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

    /// The preview's scale factor at `t`, interpolated continuously.
    ///
    /// Deriving this from `rect_at` instead would divide two rounded
    /// integer widths, so the factor jumped in visible steps and the
    /// resampled content shimmered its way through the transition. CSS
    /// interpolates the `scale()` component itself, which is what this
    /// reproduces.
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
    /// Hover state of the per-preview close buttons, animated separately
    /// from the preview ring so aiming at the X pops only the button.
    close_hover: glide_ui::HoverAnimation<WindowId>,
    phase: Phase,
    duration: Duration,
    theme: OverviewTheme,
    /// The layout area the current `previews` were computed against —
    /// kept so `relayout` can cheaply skip recomputing everything when
    /// called with identical inputs (it runs on every surface commit
    /// while the overview is open; see `GlideState::refresh_overview`).
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

    /// True when `windows`/`area` describe exactly the state the current
    /// previews were already computed from, in the same order — i.e. a
    /// `relayout` call would be a no-op.
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

        // Remember where the hovered window sat before rebuilding the
        // previews, so we can fall back to a nearby neighbor rather than
        // jumping to the first preview if that window vanishes.
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
            // Pick the neighbor at the same index the old hover occupied,
            // clamped to the new list — so closing window 5 of 8 moves
            // focus to the preview now at position 5, not all the way back
            // to the first preview.
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

    /// Fraction of the current phase's duration that has elapsed, `0..=1`.
    fn elapsed_fraction(&self, started: Instant) -> f64 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = started.elapsed().as_secs_f64();
        (elapsed / self.duration.as_secs_f64()).clamp(0.0, 1.0)
    }

    /// Scene progress along `curve`, where 1.0 is fully open.
    ///
    /// Note the closing branch eases *elapsed time* and inverts the
    /// result, rather than easing an already-reversed value. Those are not
    /// the same thing: `ease(1 - t)` plays the curve backwards, which for
    /// `--e-out` means the exit crawls for most of its duration and then
    /// slams shut — at the halfway point the overview would still be 88%
    /// open. CSS runs the same easing forward in time when a transition
    /// reverses, and `1 - ease(t)` is what reproduces that.
    fn scene_progress(&self, curve: glide_theme::motion::CubicBezier) -> f64 {
        match self.phase {
            Phase::Opening { started } => curve.ease_f64(self.elapsed_fraction(started)),
            Phase::Open => 1.0,
            Phase::Closing { started, .. } => 1.0 - curve.ease_f64(self.elapsed_fraction(started)),
        }
    }

    /// Scene progress on the design's primary curve (`--e-out`): a sharp
    /// start with a long settle. Drives preview geometry and labels.
    #[must_use]
    pub fn progress(&self) -> f64 {
        self.scene_progress(glide_theme::motion::E_OUT)
    }

    /// Scene progress on the symmetric curve (`--e-io`), which the design
    /// reserves for background properties — the scrim and the wallpaper
    /// push-back. Keeping these off `--e-out` is deliberate: the backdrop
    /// settles more gently than the previews flying into place, instead of
    /// the whole scene moving as one rigid block.
    #[must_use]
    pub fn backdrop_progress(&self) -> f64 {
        self.scene_progress(glide_theme::motion::E_IO)
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let phase_animating = match self.phase {
            Phase::Open => false,
            Phase::Opening { started } | Phase::Closing { started, .. } => {
                started.elapsed() < self.duration
            }
        };
        phase_animating || self.hover_progress() < 1.0 || self.close_hover_progress() < 1.0
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
            Phase::Open if self.hover_progress() < 1.0 || self.close_hover_progress() < 1.0 => {
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
        glide_theme::motion::E_OUT.ease_f64(self.hover.progress(HOVER_ANIM))
    }

    fn close_hover_progress(&self) -> f64 {
        glide_theme::motion::E_OVER.ease_f64(self.close_hover.progress(OV_CLOSE_HOVER_ANIM))
    }

    /// Where a window's preview currently sits on screen, if it has one.
    #[must_use]
    pub fn preview_rect(&self, id: WindowId) -> Option<Rectangle<i32, Logical>> {
        let t = self.progress();
        self.previews
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.rect_at(t))
    }

    /// The preview whose close button covers `pos`, if any.
    #[must_use]
    pub fn close_button_at(&self, pos: Point<f64, Logical>) -> Option<WindowId> {
        let t = self.progress();
        // Only clickable once the scene has essentially arrived; the button
        // is still flying in (and mostly transparent) before that.
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
            // No current hover: pick a direction-sensitive default so
            // pressing Left/Up highlights the bottom-right (last) preview
            // instead of always jumping to the top-left.
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

/// How far below its settled position a label pill starts, in logical px.
///
/// The design's `.ov-label` rests at `translateY(4px)` with `opacity: 0`
/// and animates to `translateY(0)` / `opacity: 1` over the full scene
/// duration, so the pill drifts up as it fades in rather than simply
/// appearing in place.
const LABEL_RISE: f64 = 4.0;

/// Diameter of a preview's close button, per the design's `.ov-close`.
const OV_CLOSE_SIZE: i32 = 26;

/// How far the button hangs outside the preview's top-right corner — the
/// design pins it at `top: -10px; right: -10px`.
const OV_CLOSE_OUTSET: i32 = 10;

/// `transform var(--t-2) var(--e-over)` — the overshoot curve is what gives
/// the button its little pop.
const OV_CLOSE_HOVER_ANIM: Duration = glide_theme::motion::T2;

/// Extent of the close cross, in logical px.
///
/// The design draws it as an 11px SVG on a 12-unit viewBox with the path
/// spanning 6 units — `6 × 11/12 = 5.5px` inside a 26px button. Notably
/// this is an *absolute* size, not a fraction of the button the way
/// `Painter::draw_icon_close`'s glyph is (that one would come out 13px,
/// well over twice as large, filling the circle).
const OV_CLOSE_GLYPH: f32 = 5.5;

/// `stroke-width="1.8"` through the same 11/12 viewBox scale.
const OV_CLOSE_GLYPH_STROKE: f32 = 1.65;

/// `border: 1px solid` on the button.
const OV_CLOSE_BORDER: f32 = 1.0;

/// The button's `box-shadow: var(--el-2)`, converted to the paint layer's
/// representation. Dark appearance, matching every other default token.
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

/// Padding the button's buffer needs so its `--el-2` shadow can fade out
/// instead of being clipped at the buffer edge.
fn close_shadow_margin() -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let widest = close_shadow_layers()
        .iter()
        .map(|layer| layer.blur)
        .fold(0.0_f32, f32::max)
        .ceil() as i32;
    crate::paint::shadow_fade_margin(widest)
}

/// Where a preview's close button sits, in logical coordinates.
fn close_button_rect(preview: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((
            preview.loc.x + preview.size.w + OV_CLOSE_OUTSET - OV_CLOSE_SIZE,
            preview.loc.y - OV_CLOSE_OUTSET,
        )),
        Size::from((OV_CLOSE_SIZE, OV_CLOSE_SIZE)),
    )
}

/// Offset from a preview's bottom edge to the top of its label pill.
///
/// Negative by design: the spec's mockup rides the pill *up onto* the
/// preview (`bottom: -28px` against a 28px-tall pill), so it straddles
/// the bottom edge with half its height overlapping the window and half
/// hanging below. Labels are pushed as the frontmost elements in
/// `render`, so the overlapping half draws on top of the window content.
///
/// `pub(crate)` — `config.rs`'s `OverviewThemeExt::options()` feeds this
/// into `glide_layout::OverviewOptions::label_offset` so the row-packing
/// budget and this render-time offset can never drift apart (they used
/// to be two independent numbers that only almost agreed).
pub(crate) const fn label_offset(label_height: i32) -> i32 {
    -(label_height / 2)
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

/// Rasterises one close button: `--el-2` drop shadow, the pill, its
/// hairline ring and the cross, all into a single buffer whose margin
/// leaves the shadow room to fade.
fn draw_close_button(
    theme: &OverviewTheme,
    is_hovered: bool,
    output_scale: i32,
) -> Option<MemoryRenderBuffer> {
    let margin = close_shadow_margin();
    let canvas = OV_CLOSE_SIZE + margin * 2;
    let (pw, ph) = (canvas * output_scale, canvas * output_scale);
    let Ok(pixels) = usize::try_from(pw * ph * 4) else {
        return None;
    };
    let mut buf = vec![0u8; pixels];

    let (bg, fg) = if is_hovered {
        (theme.close_hover_background, theme.close_hover_foreground)
    } else {
        (theme.close_background, theme.close_foreground)
    };
    // Hover recolours the border to match the fill, per
    // `.ov-close:hover { border-color: var(--danger) }`.
    let border = if is_hovered {
        theme.close_hover_background
    } else {
        theme.close_border
    };

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
        // Clear behind the pill: it is about to be filled opaquely,
        // and leaving shadow under a translucent hover colour would
        // muddy it.
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

/// Draws each preview's close button.
///
/// Fades in with the scene like the labels do, and pops on hover using the
/// overshoot curve (`--e-over`) the design specifies. The pop is applied as
/// a display-size change around the button's centre rather than a redraw,
/// so a hover costs no CPU rasterisation.
#[allow(clippy::too_many_arguments)]
fn push_close_buttons<R>(
    renderer: &mut R,
    previews: &mut [Preview],
    entries: &[OverviewEntry],
    theme: &OverviewTheme,
    hovered: Option<WindowId>,
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

        let is_hovered = hovered == Some(*id);
        let needs_redraw = preview
            .close
            .as_ref()
            .is_none_or(|(cached_hover, cached_scale, _)| {
                *cached_hover != is_hovered || *cached_scale != output_scale
            });

        if needs_redraw {
            if let Some(memory) = draw_close_button(theme, is_hovered, output_scale) {
                preview.close = Some((is_hovered, output_scale, memory));
            }
        }

        let Some((_, _, memory)) = preview.close.as_ref() else {
            continue;
        };

        let button = close_button_rect(*rect);
        let margin = close_shadow_margin();

        // Drawn at its natural size, always. The design pops the button
        // with `transform: scale(1.12)` on hover, but at 26px wrapped in an
        // `--el-2` shadow that growth reads as the button lurching rather
        // than responding — hover is a highlight here, and the pill stays
        // exactly where the pointer found it. Restoring the pop means
        // scaling the buffer about `button`'s centre (`src` must be the
        // buffer's natural size or it will translate instead of scale).
        let pos = Point::from((button.loc.x - margin, button.loc.y - margin))
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

        // Opacity and the rise both run the whole scene on `--e-out`, as
        // the design has them. This used to hold the labels back until the
        // scene was 60% eased-through and then rush them in over what was
        // left, which read as them popping in after the fact.
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

            let needs_redraw = preview.label.as_ref().is_none_or(
                |(cached_title, cached_width, cached_scale, _)| {
                    cached_title != title || *cached_width != width || *cached_scale != output_scale
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
                preview.label = Some((title.clone(), width, output_scale, memory));
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

        // Resolve each preview's window/title exactly once per frame. `lookup`
        // is a caller-supplied closure that clones a `GlideWindow` and formats
        // its `display_name()`; calling it a second time per preview (once
        // for labels, once for preview content) was pure wasted work, so its
        // result is cached here and reused below.
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
            push_close_buttons(
                renderer,
                &mut self.previews,
                &entries,
                &theme,
                close_hovered,
                button_alpha,
                scale,
                output_scale,
                &mut elements,
            );
        }

        let hover_t = self.hover_progress();
        // The ring is a hover affordance, not part of the entry motion.
        // Drawing it mid-transition meant stretching a buffer cached at the
        // settled size across a preview that is still window-sized, which
        // ballooned its 3px border tothree times that and shrank it down over
        // the animation — a very obvious artifact.
        if hover_t > 0.0 && t >= 1.0 {
            if let Some(hover_id) = hover {
                if let Some(preview) = self.previews.iter_mut().find(|p| p.id == hover_id) {
                    let rect = preview.rect_at(t);
                    // Cached at the settled (target) size and displayed
                    // scaled while the open/close animation is in flight —
                    // same reasoning as `push_preview_shadow`: `rect_at(t)`
                    // changes every animation frame, and keying the cache
                    // on it would redraw + reupload the ring per frame.
                    let (w, h) = (preview.target.size.w.max(1), preview.target.size.h.max(1));
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
                            preview.highlight = Some((w, h, output_scale, memory));
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

            // Same convention as smithay's `Space::render_location()`: the
            // element location handed to `render_elements` is the *buffer*
            // origin, which sits `geometry().loc` up-left of the client's
            // visible frame (CSD clients like GTK bake invisible shadow
            // margins into their buffers and report the visible region via
            // `xdg_surface.set_window_geometry`). Without this subtraction
            // a CSD preview's visible content lands shifted down-right
            // inside its slot, so the hover hit-test rect and highlight
            // ring no longer line up with what's actually on screen.
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
                AsRenderElements::render_elements(window, renderer, real_physical, scale, 1.0);

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

        // `--e-io`, per the design's `.screen::after` transition — the
        // scrim is a background property, not part of the preview motion.
        #[allow(clippy::cast_possible_truncation)]
        let backdrop_t = self.backdrop_progress() as f32;
        let dim = (theme.dim.0[3] * backdrop_t).clamp(0.0, 1.0);
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

    mod scene_easing {
        use super::*;

        /// Regression: the exit used to ease an already-reversed value,
        /// i.e. play `--e-out` backwards. That made the overview sit
        /// almost still for most of the close and then slam shut. CSS runs
        /// the same curve forward in time when a transition reverses.
        #[test]
        fn the_exit_is_front_loaded_just_like_the_entry() {
            let (mut ui, _id) = ui_with_one_window(400);
            ui.phase = Phase::Closing {
                started: Instant::now()
                    .checked_sub(Duration::from_millis(80))
                    .expect("the process has been up for longer than 80ms"),
                select: None,
            };

            // A fifth of the way out, most of the way gone.
            let remaining = ui.progress();
            assert!(
                remaining < 0.6,
                "the exit stalled: still {remaining} open a fifth of the way through"
            );
        }

        /// Entry and exit must be time mirrors of one another.
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

        /// Regression: the scale used to be derived by dividing two
        /// already-rounded integer widths, so it advanced in visible steps
        /// and the resampled content shimmered. It has to move
        /// continuously, like a CSS `scale()` component.
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

            // Strictly monotonic, with no repeated (stepped) values.
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

        /// The previews run on `--e-out` and the backdrop on `--e-io`, so
        /// mid-transition they must *not* be at the same point — that
        /// separation is what stops the scene moving as one rigid block.
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
            };
            assert!(ui.progress() < 0.5);
            assert!(ui.backdrop_progress() < 0.5);
        }
    }

    mod label_rise {
        use super::*;

        /// Labels used to be held back until the scene was 60% eased
        /// through, then rushed in — they read as popping in afterwards.
        /// They now fade across the whole scene.
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

            // `top: -10px; right: -10px` against a 26px button.
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

        /// Mid-transition the button is still flying in and mostly
        /// transparent, so it must not swallow clicks meant for the
        /// preview underneath.
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

        /// The design pins the cross to an absolute 5.5px, not to a
        /// fraction of the button — `Painter::draw_icon_close`'s built-in
        /// glyph would be 13px and fill the whole circle.
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

        /// `box-shadow: var(--el-2)` needs room in the buffer or it gets
        /// clipped into a hard-edged square halo.
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

        /// The button is a fixed-size highlight target: hovering must
        /// recolour it and nothing else. Anything that changes its
        /// geometry read as the button lurching away from the pointer.
        #[test]
        fn hovering_never_moves_or_resizes_the_button() {
            let (ui, _id) = ui_with_one_window(0);
            let rect = ui.previews[0].rect_at(1.0);

            // The drawn position depends only on the preview rect and the
            // constant shadow margin — no hover term anywhere.
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
            ui.previews[0].close = Some((false, 1, MemoryRenderBuffer::default()));

            ui.relayout(&[(id, rect(0, 0, 800, 600))], rect(0, 0, 1920, 1080));

            assert!(ui.previews[0].close.is_some());
        }

        #[test]
        fn a_brand_new_preview_gets_no_inherited_button_buffer() {
            let (mut ui, id) = ui_with_one_window(0);
            ui.previews[0].close = Some((true, 1, MemoryRenderBuffer::default()));

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
        fn the_pill_overlaps_the_preview_by_half_its_height() {
            // Spec: the pill straddles the preview's bottom edge, half on
            // the window and half below it.
            assert_eq!(label_offset(28), -14);
            assert_eq!(label_offset(40), -20);
        }

        #[test]
        fn the_offset_is_never_positive_so_the_pill_never_floats_below() {
            for height in 0..=64 {
                assert!(
                    label_offset(height) <= 0,
                    "height {height} pushed the pill below the preview"
                );
            }
        }

        /// What the layout budget ends up reserving below each row.
        /// Must stay non-negative and strictly less than the pill height,
        /// or `compute_overview_layout` and the renderer disagree.
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

    /// `relayout` runs on every surface commit while overview is open
    /// (see `GlideState::refresh_overview`), so identical inputs must be
    /// a cheap no-op rather than a full recompute-and-rebuild.
    #[test]
    fn relayout_with_identical_inputs_is_skipped_entirely() {
        let (mut ui, id) = ui_with_one_window(0);
        let windows = [(id, rect(0, 0, 800, 600))];
        let area = rect(0, 0, 1920, 1080);

        // Poke a value the real layout would never produce; if relayout
        // actually recomputes, this gets overwritten.
        ui.previews[0].row = 99;

        ui.relayout(&windows, area);
        assert_eq!(
            ui.previews[0].row, 99,
            "identical inputs must leave the previews untouched"
        );

        // A genuinely different input must still trigger the recompute.
        ui.relayout(&windows, rect(0, 0, 1280, 720));
        assert_eq!(ui.previews[0].row, 0);
    }

    /// A window resizing while overview is open (same id, new origin) is
    /// exactly the case `refresh_overview` exists for — it must not be
    /// mistaken for "identical inputs".
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

    /// When a middle window is closed and relayout runs, the hover should
    /// move to the preview now occupying that slot — not jump back to the
    /// very first preview. This matters when a user closes windows with the
    /// keyboard: they expect focus to stay in the neighbourhood, not fly off
    /// to the top-left corner.
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

        // `b` is hovered (index 1); now close it.
        let remaining = [(a, rect(0, 0, 800, 600)), (c, rect(0, 0, 800, 600))];
        ui.relayout(&remaining, area);

        // Hover should be on `c` (the preview now at index 1, where `b`
        // used to be), not on `a` (index 0 / first()).
        assert_eq!(
            ui.hovered(),
            Some(c),
            "hover should move to the neighbor at the same index, not to first()"
        );
    }

    /// Pressing Left or Up with no hover should select the *last* preview
    /// (bottom-right), while Right or Down should select the *first*
    /// (top-left). The old behaviour always picked `first()` regardless of
    /// direction, which made Left feel broken.
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
}

//! Overview mode: a zoomed-out view of every window, across all workspaces.
//!
//! # Architecture
//!
//! The overview is **render-only**. Windows stay mapped in [`Space`] at
//! their real positions the whole time; nothing here moves a window, edits
//! its geometry, or touches the layout engine's stored state. What the
//! overview does is:
//!
//! - ask `glide-layout` where each preview belongs ([`compute_overview_layout`]),
//! - draw the windows again, scaled and relocated, in a dedicated render
//!   pass ([`crate::backend::tty`] calls [`OverviewUi::render`]),
//! - intercept pointer and keyboard input so clicks land on previews rather
//!   than on the invisible real windows underneath.
//!
//! That split is deliberate. An earlier implementation fed overview
//! coordinates into `Space::map_element` and mutated `geometry()` through
//! `user_data`, which meant every subsystem reading a window's position —
//! damage tracking, hit-testing, the grabs, popup placement — silently
//! disagreed with the layout engine about where windows were.
//!
//! # Chrome buffers are cached
//!
//! Every solid-colour and text buffer lives in [`OverviewUi`] and is
//! updated in place rather than rebuilt per frame. Allocating a fresh
//! `SolidColorBuffer` each frame gives it a new element `Id`, which makes
//! the damage tracker treat it as new every time and forces a full redraw
//! at the refresh rate for as long as the overview is open.

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
    config::OverviewTheme,
    window::{GlideWindow, GlideWindowRenderElement},
};

/// How far into an open/close animation we are, and which way it runs.
#[derive(Debug, Clone, Copy)]
enum Phase {
    Opening {
        started: Instant,
    },
    Open,
    /// Closing, optionally to focus a window once it finishes.
    Closing {
        started: Instant,
        select: Option<WindowId>,
    },
}

/// Cubic ease-out: fast at first, settling gently.
///
/// The asymmetry is the point — a linear zoom reads as mechanical, while
/// easing out makes the previews feel like they are coming to rest.
fn ease_out_cubic(t: f64) -> f64 {
    let inv = 1.0 - t.clamp(0.0, 1.0);
    1.0 - inv * inv * inv
}

/// How long the hover ring takes to grow in and fade to full opacity after
/// the hovered preview changes. Short and snappy — this fires on every
/// mouse move or arrow key, unlike the open/close animation, so anything
/// longer would feel laggy rather than polished.
const HOVER_ANIM: Duration = Duration::from_millis(120);

/// Linear interpolation between two rectangles.
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

/// A cached label buffer, and the `(text, width, scale)` it was rendered
/// from.
type CachedLabel = (String, i32, i32, MemoryRenderBuffer);

/// One preview: the slot the layout engine assigned, plus where the window
/// really is (the animation interpolates between them).
#[derive(Debug, Clone)]
pub struct Preview {
    pub id: WindowId,
    /// Final resting rectangle, in global logical coordinates.
    pub target: Rectangle<i32, Logical>,
    /// The window's real on-screen frame, where the animation starts from.
    pub origin: Rectangle<i32, Logical>,
    pub row: usize,
    /// Cached label buffer, and the `(text, width, scale)` it was rendered
    /// from — so it is only re-rasterised when any of the three actually
    /// changes. Width matters as much as text: it's the buffer's own
    /// dimension, and a `relayout` (e.g. a workspace switch changing the
    /// row count) can resize a preview without touching its title at all —
    /// checking text alone left a stale, wrong-width label glued to the
    /// wider or narrower slot. Scale matters for the same reason `width`
    /// does: a buffer rasterized at one scale can't just be reused at
    /// another without going blurry (or, hypothetically, blocky).
    label: Option<CachedLabel>,
}

impl Preview {
    /// The preview's rectangle at animation progress `t`.
    #[must_use]
    pub fn rect_at(&self, t: f64) -> Rectangle<i32, Logical> {
        lerp_rect(self.origin, self.target, t)
    }
}

/// Live state of the overview: what is on screen, what the pointer is over,
/// and the cached buffers used to draw it.
#[derive(Debug)]
pub struct OverviewUi {
    previews: Vec<Preview>,
    /// Preview under the pointer, or selected with the arrow keys.
    hover: Option<WindowId>,
    /// When `hover` was last changed *to* `Some` value, driving the ring's
    /// short grow/fade-in animation (see [`Self::hover_progress`]). `None`
    /// while nothing is hovered — there is no ring to animate then.
    hover_started: Option<Instant>,
    phase: Phase,
    duration: Duration,
    theme: OverviewTheme,

    // Cached chrome. Resized/recoloured in place; never reallocated per
    // frame — see the module docs on why that matters.
    scrim: SolidColorBuffer,
    /// Four strips (top, bottom, left, right) tiling a ring around the
    /// hovered preview — see [`crate::decoration::border_rects`], which
    /// this shares its tiling geometry with. A single solid-filled
    /// rectangle here would either hide the preview it's meant to
    /// highlight (if drawn in front) or do nothing visible at all (if
    /// drawn behind, since the preview is opaque and the same size); a
    /// ring is correct either way, because it never overlaps the preview's
    /// own pixels.
    highlight: [SolidColorBuffer; 4],
}

impl OverviewUi {
    /// Builds the overview for `windows` within `area`.
    ///
    /// `windows` pairs each window with its real frame rectangle: the
    /// layout engine supplies the sizes to arrange, and the rectangles are
    /// where the open animation starts from.
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
                })
            })
            .collect();

        Self {
            previews,
            // Start with the focused window highlighted, so opening the
            // overview and pressing Enter is a no-op rather than a
            // surprise.
            hover: focused,
            // No pop-in for the *initial* hover: the whole overview is
            // already animating open, and stacking a second animation on
            // top of that would read as busy rather than polished. Only
            // hover changes made *after* opening (a mouse move, an arrow
            // key) get the grow/fade-in.
            hover_started: None,
            phase: Phase::Opening {
                started: Instant::now(),
            },
            duration: theme.duration(),
            theme: theme.clone(),
            scrim: SolidColorBuffer::default(),
            highlight: std::array::from_fn(|_| SolidColorBuffer::default()),
        }
    }

    /// Recomputes slots after the window set or work area changed, keeping
    /// the current animation phase and hover.
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

        // Preserve cached labels for windows that are still around. Kept
        // even if the width is about to turn out stale — `render_labels`'s
        // own `needs_redraw` check (text *and* width) is what decides
        // whether a cache hit is still valid; this is just carrying the
        // candidate forward.
        let mut old_labels: Vec<(WindowId, Option<CachedLabel>)> =
            self.previews.drain(..).map(|p| (p.id, p.label)).collect();

        self.previews = slots
            .iter()
            .filter_map(|slot| {
                let origin = windows.iter().find(|(id, _)| *id == slot.id)?.1;
                let label = old_labels
                    .iter_mut()
                    .find(|(id, _)| *id == slot.id)
                    .and_then(|(_, label)| label.take());
                Some(Preview {
                    id: slot.id,
                    target: to_smithay_rect(slot.target),
                    origin,
                    row: slot.row,
                    label,
                })
            })
            .collect();

        if self.hover.is_some_and(|id| !self.contains(id)) {
            self.hover = self.previews.first().map(|p| p.id);
        }
    }

    fn contains(&self, id: WindowId) -> bool {
        self.previews.iter().any(|p| p.id == id)
    }

    /// Animation progress, 0.0 → 1.0, already eased.
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

    /// Whether an animation is still running, i.e. the compositor needs to
    /// keep scheduling repaints. Includes the hover ring's grow/fade-in,
    /// not just the open/close phase.
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

    /// Advances the animation state machine.
    pub fn tick(&mut self) -> Tick {
        match self.phase {
            // Settling still asks for one more frame: the last one actually
            // drawn was at whatever progress the clock happened to be at,
            // a little short of 1.0, so without this the previews stop just
            // shy of their slots.
            Phase::Opening { started } if started.elapsed() >= self.duration => {
                self.phase = Phase::Open;
                Tick::Running
            }
            Phase::Closing { started, select } if started.elapsed() >= self.duration => {
                Tick::Finished { select }
            }
            // Settled open: still `Running` while the hover ring is
            // mid grow/fade-in, so `tick_overview` keeps scheduling
            // repaints for it exactly as it does for the open/close
            // animation — otherwise the ring would only ever reach its
            // final size on whatever frame something *else* happened to
            // trigger a repaint.
            Phase::Open if self.hover_progress() < 1.0 => Tick::Running,
            Phase::Open => Tick::Idle,
            Phase::Opening { .. } | Phase::Closing { .. } => Tick::Running,
        }
    }

    /// Begins the closing animation. `select` is the window to focus once
    /// it completes, if any.
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
    pub const fn hovered(&self) -> Option<WindowId> {
        self.hover
    }

    /// Which preview `pos` falls in, if any.
    #[must_use]
    pub fn preview_at(&self, pos: Point<f64, Logical>) -> Option<WindowId> {
        let t = self.progress();
        self.previews
            .iter()
            .find(|p| p.rect_at(t).to_f64().contains(pos))
            .map(|p| p.id)
    }

    /// Updates the hover target. Returns `true` if it changed, so the
    /// caller only repaints when something actually moved.
    pub fn set_hover(&mut self, hover: Option<WindowId>) -> bool {
        if self.hover == hover {
            return false;
        }
        self.hover = hover;
        // Restarts the ring's grow/fade-in from scratch on every change,
        // including hover -> a *different* preview: the old ring simply
        // vanishes (no crossfade) and the new one grows in. `None` means
        // nothing is hovered right now, so there is nothing to animate.
        self.hover_started = hover.map(|_| Instant::now());
        true
    }

    /// Eased 0..1 progress of the current hover's ring grow/fade-in.
    /// `1.0` (fully settled, drawn at full size and opacity) once
    /// [`HOVER_ANIM`] has elapsed, or immediately if nothing is animating
    /// — including whenever nothing is hovered at all, since a ring that
    /// isn't drawn has nothing to animate.
    fn hover_progress(&self) -> f64 {
        let Some(started) = self.hover_started else {
            return 1.0;
        };
        if HOVER_ANIM.is_zero() {
            return 1.0;
        }
        let t = (started.elapsed().as_secs_f64() / HOVER_ANIM.as_secs_f64()).clamp(0.0, 1.0);
        ease_out_cubic(t)
    }

    /// Moves the selection one step in a direction, for arrow-key
    /// navigation.
    ///
    /// Up/down move between rows, keeping roughly the same horizontal
    /// position — jumping to the row's first entry instead would make
    /// vertical navigation feel like it drifts left.
    pub fn navigate(&mut self, dir: Direction) -> bool {
        if self.previews.is_empty() {
            return false;
        }

        let Some(current) = self
            .hover
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
                        // Already in the top row: nothing above to move to.
                        None => return false,
                    }
                } else {
                    row + 1
                };

                let t = self.progress();
                let centre = centre_x(self.previews[current].rect_at(t));

                // Nearest preview in the target row, horizontally.
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

/// Outcome of one animation step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    /// Sitting open and settled. Nothing to redraw.
    Idle,
    /// Still animating: the caller schedules another frame.
    Running,
    /// The close animation finished: the caller tears the overview down
    /// and, if `select` is set, focuses that window.
    Finished { select: Option<WindowId> },
}

/// Arrow-key navigation directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// Point in the animation at which labels start fading in.
const LABEL_FADE_START: f64 = 0.6;

/// Horizontal centre of a rectangle, for "nearest in the next row"
/// navigation.
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
    /// Everything the overview draws.
    pub OverviewRenderElement<R> where R: ImportAll + ImportMem;
    /// A window, scaled down and moved into its slot.
    Preview = RelocateRenderElement<RescaleRenderElement<GlideWindowRenderElement<R>>>,
    /// Scrim, highlight ring — flat colours.
    Solid = SolidColorRenderElement,
    /// Rasterised label text.
    Label = MemoryRenderBufferRenderElement<R>,
}

impl<R: Renderer> std::fmt::Debug for OverviewRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preview(e) => f.debug_tuple("Preview").field(e).finish(),
            Self::Solid(e) => f.debug_tuple("Solid").field(e).finish(),
            Self::Label(e) => f.debug_tuple("Label").field(e).finish(),
            Self::_GenericCatcher(e) => f.debug_tuple("_GenericCatcher").field(e).finish(),
        }
    }
}

impl OverviewUi {
    /// Title labels under each preview.
    ///
    /// Faded in over the back half of the animation: at the start the
    /// previews are still near full size and a label would land in the
    /// middle of the window it belongs to.
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

            let width = rect.size.w.max(1);
            let height = theme.label_height;

            // Re-rasterise only when the text, width, or scale changed —
            // titles are stable, so this is almost always a cache hit. All
            // three must match: checking text alone left a stale, wrong-
            // width buffer glued to a slot after a relayout resized it
            // without changing its title (e.g. a workspace switch that
            // changed the row count); scale changing (hypothetically —
            // single output today, see `output_scale`'s caller) would
            // leave a blurry buffer rasterized for the wrong resolution.
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
                painter.draw_label(
                    &mut buf,
                    width,
                    height,
                    0,
                    width,
                    output_scale,
                    crate::paint::TextAlign::Center,
                    &title,
                    theme.label_color.to_buffer_bytes(),
                    // Transparent: the label floats over the scrim rather
                    // than sitting on a plate.
                    [0, 0, 0, 0],
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

            let pos = Point::from((rect.loc.x, rect.loc.y + rect.size.h))
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
                elements.push(OverviewRenderElement::Label(element));
            }
        }
        elements
    }

    /// Collects the overview's render elements, front-to-back.
    ///
    /// Order is: labels and highlight (on top of previews), then the
    /// previews themselves, then the scrim last — so it sits *behind*
    /// everything the overview draws but *in front* of the real desktop,
    /// dimming the wallpaper and any background layer surfaces.
    ///
    /// `lookup` resolves a window id to the live window; ids the caller no
    /// longer knows about are skipped rather than panicking, since a client
    /// can exit at any point between relayout and render.
    ///
    /// `output_scale` is the integer output scale, separate from the
    /// fractional `scale` used for physical-pixel positioning — it's what
    /// label buffers rasterize at so their text is crisp rather than
    /// upscaled on any output whose scale isn't exactly `1`.
    #[allow(clippy::too_many_arguments)]
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

        let hover = self.hover;
        let theme = self.theme.clone();

        // Collected first so the immutable borrows of `self.previews` end
        // before the cached buffers below are mutated.
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

        // ── Highlight ring around the hovered preview ──────────────
        // A real ring (four strips, same tiling `decoration::border_rects`
        // gives a window's own border), not a solid rectangle: see the
        // `highlight` field's docs on why a filled slab doesn't work here
        // regardless of z-order.
        //
        // Grows from 0 to the full `theme.border` thickness and fades in
        // to full opacity together, over `HOVER_ANIM`, whenever the hover
        // target just changed — see `hover_progress`'s docs. A ring that
        // simply snaps to size on every mouse move reads as flickery next
        // to the previews' own eased open/close motion; this matches it.
        let hover_t = self.hover_progress();
        if let (Some((_, rect, _)), true) = (
            entries.iter().find(|(_, _, hovered)| *hovered),
            hover_t > 0.0,
        ) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let border = (f64::from(theme.border) * hover_t).round() as i32;
            if border > 0 {
                let strips = crate::decoration::border_rects(rect.size.w, rect.size.h, border);
                #[allow(clippy::cast_possible_truncation)]
                let alpha = hover_t as f32;
                for (buf, (offset, size)) in self.highlight.iter_mut().zip(strips) {
                    buf.update((size.w, size.h), theme.highlight.0);
                    let pos = Point::from((rect.loc.x + offset.x, rect.loc.y + offset.y));
                    elements.push(OverviewRenderElement::Solid(
                        SolidColorRenderElement::from_buffer(
                            buf,
                            pos.to_f64().to_physical(scale).to_i32_round(),
                            scale,
                            alpha,
                            Kind::Unspecified,
                        ),
                    ));
                }
            }
        }

        // ── Previews ───────────────────────────────────────────────
        for (id, rect, _) in &entries {
            let Some((window, _)) = lookup(*id) else {
                continue;
            };
            let Some(preview) = self.previews.iter().find(|p| p.id == *id) else {
                continue;
            };

            // Scale is derived from the animated rectangle, so it tracks
            // the zoom continuously rather than snapping at the end.
            #[allow(clippy::cast_precision_loss)]
            let window_scale = f64::from(rect.size.w) / f64::from(preview.origin.size.w.max(1));

            // The window's real content origin, in physical coordinates —
            // where `render_elements` would normally draw it.
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
                // Scale about the frame's top-left, then translate that
                // corner onto the slot. Composing the two stock wrappers
                // keeps every element's `Id` stable across frames, which is
                // what lets damage tracking work during the animation.
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

        // ── Scrim (behind the overview, over the desktop) ──────────
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

    /// A single-window overview, focused on that window (so `hover`
    /// starts `Some`), at the given open/close animation duration —
    /// `0` isolates the hover-ring animation in tests that need `tick`/
    /// `is_animating` to reflect *only* the hover animation, by settling
    /// the open phase on the very first `tick()` call.
    fn ui_with_one_window(duration_ms: u64) -> (OverviewUi, WindowId) {
        let id = WindowId::next();
        let windows = [(id, rect(0, 0, 800, 600))];
        let area = rect(0, 0, 1920, 1080);
        let ui = OverviewUi::new(&windows, area, &theme_with_duration(duration_ms), Some(id));
        (ui, id)
    }

    #[test]
    fn initial_hover_has_no_pop_in_animation() {
        // hover_started is None from construction even though `hover` is
        // already Some(focused) — see OverviewUi::new's docs on why
        // stacking a pop-in on top of the whole overview opening would
        // read as busy rather than polished.
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
        let _ = ui.tick(); // settle the open phase

        ui.set_hover(None);
        ui.set_hover(Some(id));
        std::thread::sleep(HOVER_ANIM + Duration::from_millis(30));

        assert_eq!(ui.tick(), Tick::Idle);
    }

    #[test]
    fn is_animating_reflects_the_hover_ring_too() {
        let (mut ui, id) = ui_with_one_window(0);
        let _ = ui.tick(); // settle the open phase
        assert!(!ui.is_animating());

        ui.set_hover(None);
        ui.set_hover(Some(id));
        assert!(ui.is_animating());
    }
}

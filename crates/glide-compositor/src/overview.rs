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
    /// Cached label buffer, and the text it was rendered from — so it is
    /// only re-rasterised when the title actually changes.
    label: Option<(String, MemoryRenderBuffer)>,
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
    phase: Phase,
    duration: Duration,
    theme: OverviewTheme,

    // Cached chrome. Resized/recoloured in place; never reallocated per
    // frame — see the module docs on why that matters.
    scrim: SolidColorBuffer,
    highlight: SolidColorBuffer,
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
            phase: Phase::Opening {
                started: Instant::now(),
            },
            duration: theme.duration(),
            theme: theme.clone(),
            scrim: SolidColorBuffer::default(),
            highlight: SolidColorBuffer::default(),
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

        // Preserve cached labels for windows that are still around.
        let mut old_labels: Vec<(WindowId, Option<(String, MemoryRenderBuffer)>)> =
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
    /// keep scheduling repaints.
    #[must_use]
    pub fn is_animating(&self) -> bool {
        match self.phase {
            Phase::Open => false,
            Phase::Opening { started } | Phase::Closing { started, .. } => {
                started.elapsed() < self.duration
            }
        }
    }

    /// Advances the animation state machine.
    pub fn tick(&mut self) -> Tick {
        match self.phase {
            Phase::Opening { started } if started.elapsed() >= self.duration => {
                self.phase = Phase::Open;
                Tick::Running
            }
            Phase::Closing { started, select } if started.elapsed() >= self.duration => {
                Tick::Finished { select }
            }
            _ => Tick::Running,
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
        true
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
    /// Still animating, or sitting open. Nothing for the caller to do
    /// beyond scheduling the next frame.
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

            // Re-rasterise only when the text or the width changed —
            // titles are stable, so this is almost always a cache hit.
            let needs_redraw = preview
                .label
                .as_ref()
                .is_none_or(|(cached, _)| cached != &title);
            if needs_redraw {
                let Ok(pixels) = usize::try_from(width * height * 4) else {
                    continue;
                };
                let mut buf = vec![0u8; pixels];
                painter.draw_label(
                    &mut buf,
                    width,
                    height,
                    0,
                    width,
                    &title,
                    theme.label_color.as_argb_bytes(),
                    // Transparent: the label floats over the scrim rather
                    // than sitting on a plate.
                    [0, 0, 0, 0],
                );
                let memory = MemoryRenderBuffer::from_slice(
                    &buf,
                    smithay::backend::allocator::Fourcc::Argb8888,
                    (width, height),
                    1,
                    Transform::Normal,
                    None,
                );
                preview.label = Some((title, memory));
            }

            let Some((_, memory)) = preview.label.as_ref() else {
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
    pub fn render<R, F>(
        &mut self,
        renderer: &mut R,
        output_rect: Rectangle<i32, Logical>,
        scale: Scale<f64>,
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

        elements.extend(self.render_labels(renderer, &entries, scale, painter, &mut lookup));

        // ── Highlight ring behind the hovered preview ──────────────
        if let Some((_, rect, _)) = entries.iter().find(|(_, _, hovered)| *hovered) {
            let border = theme.border;
            let ring = Rectangle::new(
                Point::from((rect.loc.x - border, rect.loc.y - border)),
                Size::from((rect.size.w + border * 2, rect.size.h + border * 2)),
            );
            self.highlight
                .update((ring.size.w, ring.size.h), theme.highlight.0);
            elements.push(OverviewRenderElement::Solid(
                SolidColorRenderElement::from_buffer(
                    &self.highlight,
                    ring.loc.to_f64().to_physical(scale).to_i32_round(),
                    scale,
                    1.0,
                    Kind::Unspecified,
                ),
            ));
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

//! [`GlideWindow`]: a thin wrapper around Smithay's [`Window`].
//!
//! Exists so window decorations have somewhere to live: `SpaceElement` and
//! `AsRenderElements` are foreign traits and `Window` is a foreign type, so
//! the orphan rule forbids implementing decoration-aware versions of either
//! directly on `Window`. Wrapping it in a local type is the standard way
//! around that — this is the same shape as anvil's own `WindowElement`.
//!
//! `Deref<Target = Window>` means every call site that only ever needed
//! `Window`'s own methods (`.toplevel()`, `.geometry()`, `.clone()`, …) keeps
//! compiling unchanged; only `SpaceElement`/`AsRenderElements`, which are
//! genuinely decoration-aware, are overridden here.
//!
//! # Coordinate convention
//!
//! [`smithay::desktop::Space`] positions a `GlideWindow` at its *content*
//! origin (see [`crate::GlideState::reconcile_space`]), not its frame's —
//! matching M4.2's `Insets`/`content_geometry` split, which was written
//! first and is what this was made to fit rather than the other way
//! around.
//!
//! Two consequences follow, and both are easy to get backwards (an earlier
//! version of this file did, in both places, before a titlebar existed to
//! actually click and reveal it):
//!
//! - [`SpaceElement::geometry`] is **not** extended for the titlebar —
//!   see its own doc comment below for why touching it breaks
//!   `render_location`. Only [`SpaceElement::bbox`] grows, *upward* (a
//!   negative `loc.y`, larger `size.h`), since content sits at the mapped
//!   origin and the titlebar is drawn above it, not below.
//! - In [`AsRenderElements::render_elements`], the content therefore
//!   renders at the `location` `Space` hands in, unshifted, and it is the
//!   *decoration* that gets the offset — upward, again.
//!
//! This is the mirror image of anvil's own `WindowElement`, which maps at
//! the *frame* origin and shifts its content down instead; both are valid
//! choices, just not interchangeable once code on either side of the split
//! has been written against one of them.

use std::ops::Deref;

use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer,
        element::{
            AsRenderElements,
            memory::MemoryRenderBufferRenderElement,
            solid::SolidColorRenderElement,
            surface::WaylandSurfaceRenderElement,
        },
    },
    desktop::{Window, space::SpaceElement},
    output::Output,
    render_elements,
    utils::{IsAlive, Logical, Physical, Point, Rectangle, Scale},
};

use crate::decoration;

/// A managed window, wrapping Smithay's [`Window`].
///
/// `Eq`/`Hash` forward to `Window`'s own (identity-based, via the client's
/// surface id), which is what lets it be used as a `HashMap` key exactly
/// like `Window` was.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GlideWindow(Window);

impl GlideWindow {
    pub fn new(window: Window) -> Self {
        Self(window)
    }

    /// The wrapped [`Window`]. Rarely needed directly — `Deref` covers most
    /// call sites — but kept for the handful of APIs that specifically want
    /// a `Window` rather than anything `Deref`-coercible to one, and for
    /// code that must avoid any ambiguity with `GlideWindow`'s own
    /// `SpaceElement::geometry` (which, unlike `Window::geometry`, includes
    /// the titlebar).
    #[must_use]
    pub fn window(&self) -> &Window {
        &self.0
    }

    /// Current titlebar height, `0` if undecorated.
    ///
    /// A small, self-contained `RefCell` read: safe to call from
    /// `SpaceElement`/`AsRenderElements` methods that must not hold the
    /// borrow across anything else, since the borrow it takes internally is
    /// dropped before this returns.
    fn header_height(&self) -> i32 {
        self.try_decoration_state()
            .map_or(0, |state| state.header.height())
    }
}

impl Deref for GlideWindow {
    type Target = Window;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl IsAlive for GlideWindow {
    fn alive(&self) -> bool {
        self.0.alive()
    }
}

// ── Overview transform ─────────────────────────────────────────────

/// All the data `render_elements` needs to draw a window inside the
/// Overview grid.  Stored in the `Window`'s `user_data` via a `Mutex`.
///
/// The transform is purely visual: `Space::map_element` always receives
/// the window's *real* content origin — nothing in the layout engine or
/// the compositor state machine is aware of the overview.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OverviewTransform {
    /// Down-scale factor (0.0–1.0) applied to the window's rendering.
    pub scale: f64,
    /// The window's real content origin in global logical coordinates
    /// (what the layout engine stores).
    pub real_content_loc: Point<i32, Logical>,
    /// Where the window's content should appear in the overview grid,
    /// in global logical coordinates.
    pub overview_content_loc: Point<i32, Logical>,
    /// Cell background rectangle in global logical coordinates.
    pub cell_rect: Rectangle<i32, Logical>,
    /// Whether this window is currently focused (draws a highlight ring).
    pub focused: bool,
}

impl GlideWindow {
    pub(crate) fn set_overview_state(&self, state: Option<OverviewTransform>) {
        self.0
            .user_data()
            .insert_if_missing(|| std::sync::Mutex::new(None::<OverviewTransform>));
        let mutex = self
            .0
            .user_data()
            .get::<std::sync::Mutex<Option<OverviewTransform>>>()
            .unwrap();
        *mutex.lock().unwrap() = state;
    }

    pub(crate) fn overview_state(&self) -> Option<OverviewTransform> {
        self.0
            .user_data()
            .get::<std::sync::Mutex<Option<OverviewTransform>>>()
            .and_then(|m| *m.lock().unwrap())
    }
}

// ── SpaceElement ───────────────────────────────────────────────────

impl SpaceElement for GlideWindow {
    /// Deliberately **not** extended for the titlebar, unlike `bbox` below.
    ///
    /// Smithay's own convention (see [`SpaceElement::geometry`]'s docs) is
    /// that `geometry` excludes decorations — it is what `Space` uses to
    /// resolve `render_location` (`self.location - geometry().loc`) back to
    /// where a client's surface actually draws, and what
    /// `Space::element_geometry` hands to callers like `unconstrain_popup`
    /// that want the content area, not the frame. Extending it here shifted
    /// `render_location`, and with it every mapped surface, by the titlebar
    /// height — content rendered `header_height` too low.
    ///
    /// This is never mutated for overview — the window's real geometry is
    /// always reported so that `Space` computes `render_location` correctly.
    fn geometry(&self) -> Rectangle<i32, Logical> {
        SpaceElement::geometry(&self.0)
    }

    /// The visual bounding box: content + titlebar + resize border, and
    /// when overview is active, the cell background + shadow area too.
    ///
    /// This must cover every pixel the element can paint so Smithay's
    /// damage tracker invalidates the right regions.
    fn bbox(&self) -> Rectangle<i32, Logical> {
        let mut bbox = SpaceElement::bbox(&self.0);
        let height = self.header_height();
        if height > 0 {
            // Titlebar above the content, resize band around the whole
            // frame. The bbox has to cover both — `Space::element_under`
            // pre-filters on it, so anything outside the bbox can never
            // receive input no matter what `is_in_input_region` says.
            let band = decoration::RESIZE_BORDER;
            bbox.loc.x -= band;
            bbox.loc.y -= height + band;
            bbox.size.w += band * 2;
            bbox.size.h += height + band * 2;
        }
        if let Some(ov) = self.overview_state() {
            // bbox is relative to element_location (= real_content_loc).
            // We need to cover the cell + shadow so damage tracking works.
            let cell_local_x = ov.cell_rect.loc.x - ov.real_content_loc.x;
            let cell_local_y = ov.cell_rect.loc.y - ov.real_content_loc.y;
            let shadow_margin = 12;

            let min_x = bbox.loc.x.min(cell_local_x);
            let min_y = bbox.loc.y.min(cell_local_y);
            let max_x = (bbox.loc.x + bbox.size.w)
                .max(cell_local_x + ov.cell_rect.size.w + shadow_margin);
            let max_y = (bbox.loc.y + bbox.size.h)
                .max(cell_local_y + ov.cell_rect.size.h + shadow_margin);

            bbox.loc.x = min_x;
            bbox.loc.y = min_y;
            bbox.size.w = max_x - min_x;
            bbox.size.h = max_y - min_y;
        }
        bbox
    }

    fn is_in_input_region(&self, point: &Point<f64, Logical>) -> bool {
        // `point` is relative to the content origin; the decoration's own
        // origin sits `header_height` above it (see the module docs on the
        // coordinate convention). Routed through `HeaderBar::contains`
        // rather than a bare strip check so the open chevron menu — which
        // extends *below* the strip — counts as ours too.
        let in_decoration = self.try_decoration_state().is_some_and(|deco| {
            let rel = *point + Point::from((0.0, f64::from(deco.header.height())));
            deco.header.contains(rel)
        });
        in_decoration || SpaceElement::is_in_input_region(&self.0, point)
    }

    fn z_index(&self) -> u8 {
        SpaceElement::z_index(&self.0)
    }

    fn set_activate(&self, activated: bool) {
        SpaceElement::set_activate(&self.0, activated);
    }

    fn output_enter(&self, output: &Output, overlap: Rectangle<i32, Logical>) {
        SpaceElement::output_enter(&self.0, output, overlap);
    }

    fn output_leave(&self, output: &Output) {
        SpaceElement::output_leave(&self.0, output);
    }

    fn refresh(&self) {
        SpaceElement::refresh(&self.0);
    }
}

// ── Render elements ────────────────────────────────────────────────

render_elements! {
    /// Everything a `GlideWindow` can put on screen: its own Wayland surface
    /// content plus (when decorated) the titlebar elements — buttons with
    /// icons, a title label, and (when open) the dropdown menu rows.
    pub GlideWindowRenderElement<R> where R: ImportAll + ImportMem;
    Content = WaylandSurfaceRenderElement<R>,
    Decoration = MemoryRenderBufferRenderElement<R>,
    Background = SolidColorRenderElement,
}

impl<R: Renderer> std::fmt::Debug for GlideWindowRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Content(e) => f.debug_tuple("Content").field(e).finish(),
            Self::Decoration(e) => f.debug_tuple("Decoration").field(e).finish(),
            Self::Background(e) => f.debug_tuple("Background").field(e).finish(),
            Self::_GenericCatcher(e) => f.debug_tuple("_GenericCatcher").field(e).finish(),
        }
    }
}

impl<R> AsRenderElements<R> for GlideWindow
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
    R::Error: std::fmt::Debug,
{
    type RenderElement = GlideWindowRenderElement<R>;

    fn render_elements<C: From<Self::RenderElement>>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Vec<C> {
        let painter = crate::paint::PAINTER.get_or_init(|| {
            let h = self.header_height();
            let font_px = (f64::from(h).max(14.0) * 0.45) as f32;
            crate::paint::Painter::new(font_px.clamp(10.0, 18.0))
        });

        // ── Overview path ──────────────────────────────────────────
        //
        // The window is still mapped at its REAL position in Space, so
        // `location` is the real render-location.  We compute a delta to
        // shift every element to the overview grid position and apply a
        // down-scale so the window fits inside its cell.
        if let Some(ov) = self.overview_state() {
            return self.render_overview(renderer, location, scale, alpha, painter, ov);
        }

        // ── Normal (non-overview) path ─────────────────────────────
        let header_height = self.header_height();
        let header_location =
            location - Point::from((0, header_height)).to_physical_precise_round(scale);

        let mut elements = Vec::new();

        // Decorations first: `render_frame` wants elements front-to-back,
        // and the titlebar sits on top of the window's own content.
        let deco_elements: Vec<C> = decoration::frame_buffers(self, painter)
            .into_iter()
            .filter_map(|deco_buf| {
                let physical_offset = deco_buf.offset.to_physical_precise_round(scale);
                let pos = (header_location + physical_offset).to_f64();
                MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    pos,
                    &deco_buf.buf,
                    Some(alpha),
                    None,
                    None,
                    smithay::backend::renderer::element::Kind::Unspecified,
                )
                .map_err(|e| tracing::warn!("failed to upload decoration buffer: {:?}", e))
                .ok()
            })
            .map(GlideWindowRenderElement::Decoration)
            .map(C::from)
            .collect();

        elements.extend(deco_elements);

        let content_elements: Vec<GlideWindowRenderElement<R>> =
            AsRenderElements::render_elements(&self.0, renderer, location, scale, alpha);
        elements.extend(content_elements.into_iter().map(C::from));

        elements
    }
}

// ── Overview rendering (private) ───────────────────────────────────

impl GlideWindow {
    /// Renders a single window in overview-grid mode.
    ///
    /// Everything here is purely visual: no state is mutated, no layout
    /// geometry changed.  The render order (front-to-back) is:
    ///
    /// 1. Decorations (titlebar)
    /// 2. Client content (wl_surface tree)
    /// 3. Highlight ring (focused window only)
    /// 4. Cell background (desktop color)
    /// 5. Soft shadow (3 layers, fading outward)
    fn render_overview<R, C>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        output_scale: Scale<f64>,
        alpha: f32,
        painter: &crate::paint::Painter,
        ov: OverviewTransform,
    ) -> Vec<C>
    where
        R: Renderer + ImportAll + ImportMem,
        R::TextureId: Send + Clone + 'static,
        R::Error: std::fmt::Debug,
        C: From<GlideWindowRenderElement<R>>,
    {
        let window_scale = ov.scale;
        let actual_scale = output_scale * Scale::from(window_scale);

        // `location` = (real_content_loc − geo_loc) in physical coords.
        // We want content at `overview_content_loc`.  The scaled buffer
        // shifts `geometry().loc` by `geo_loc * window_scale` instead of
        // `geo_loc`, so we compensate.
        let geo_loc = SpaceElement::geometry(&self.0).loc;
        let delta_x = ov.overview_content_loc.x - ov.real_content_loc.x
            + geo_loc.x
            - (geo_loc.x as f64 * window_scale).round() as i32;
        let delta_y = ov.overview_content_loc.y - ov.real_content_loc.y
            + geo_loc.y
            - (geo_loc.y as f64 * window_scale).round() as i32;
        let delta_physical = Point::from((delta_x, delta_y))
            .to_f64()
            .to_physical(output_scale)
            .to_i32_round();
        let overview_loc = location + delta_physical;

        // Cell position in physical output-relative coords.
        // For any global logical point P:
        //   P_physical = location + (P − real_content_loc + geo_loc) × output_scale
        let cell_off = Point::from((
            ov.cell_rect.loc.x - ov.real_content_loc.x + geo_loc.x,
            ov.cell_rect.loc.y - ov.real_content_loc.y + geo_loc.y,
        ));
        let cell_phys_loc = location
            + cell_off
                .to_f64()
                .to_physical(output_scale)
                .to_i32_round();
        let cell_phys_size = ov
            .cell_rect
            .size
            .to_f64()
            .to_physical(output_scale)
            .to_i32_round();

        // Header location: above content at the overview position.
        let header_height = self.header_height();
        let header_loc =
            overview_loc - Point::from((0, header_height)).to_physical_precise_round(actual_scale);

        let mut elements: Vec<C> = Vec::new();

        // 1. Decorations (front — on top of everything).
        let deco_elements: Vec<C> = decoration::frame_buffers(self, painter)
            .into_iter()
            .filter_map(|deco_buf| {
                let physical_offset = deco_buf.offset.to_physical_precise_round(actual_scale);
                let pos = (header_loc + physical_offset).to_f64();
                MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    pos,
                    &deco_buf.buf,
                    Some(alpha),
                    None,
                    None,
                    smithay::backend::renderer::element::Kind::Unspecified,
                )
                .map_err(|e| tracing::warn!("failed to upload decoration buffer: {:?}", e))
                .ok()
            })
            .map(GlideWindowRenderElement::Decoration)
            .map(C::from)
            .collect();
        elements.extend(deco_elements);

        // 2. Client content (behind decorations).
        let content: Vec<GlideWindowRenderElement<R>> =
            AsRenderElements::render_elements(&self.0, renderer, overview_loc, actual_scale, alpha);
        elements.extend(content.into_iter().map(C::from));

        // 3. Focus highlight ring (behind content, in front of cell bg).
        if ov.focused {
            let border_px = (3.0 * output_scale.x).round() as i32;
            let ring_loc = Point::from((
                cell_phys_loc.x - border_px,
                cell_phys_loc.y - border_px,
            ));
            let ring_size = (
                cell_phys_size.w + border_px * 2,
                cell_phys_size.h + border_px * 2,
            );
            let ring_buf =
                smithay::backend::renderer::element::solid::SolidColorBuffer::new(
                    ring_size,
                    [0.35, 0.65, 1.0, 0.9],
                );
            elements.push(
                GlideWindowRenderElement::Background(SolidColorRenderElement::from_buffer(
                    &ring_buf,
                    ring_loc,
                    Scale::from(1.0),
                    alpha,
                    smithay::backend::renderer::element::Kind::Unspecified,
                ))
                .into(),
            );
        }

        // 4. Cell background (desktop color — behind content).
        let cell_buf = smithay::backend::renderer::element::solid::SolidColorBuffer::new(
            (cell_phys_size.w, cell_phys_size.h),
            [0.160_784_3, 0.325_490_2, 0.588_235_3, 1.0],
        );
        elements.push(
            GlideWindowRenderElement::Background(SolidColorRenderElement::from_buffer(
                &cell_buf,
                cell_phys_loc,
                Scale::from(1.0),
                alpha,
                smithay::backend::renderer::element::Kind::Unspecified,
            ))
            .into(),
        );

        // 5. Soft shadow: three layers at increasing offset and
        //    decreasing opacity.  Pushed last → behind everything.
        for &(offset_px, shadow_alpha) in &[(1_i32, 0.16_f32), (4, 0.10), (8, 0.05)] {
            let px = (offset_px as f64 * output_scale.x).round() as i32;
            let shadow_loc = Point::from((cell_phys_loc.x + px, cell_phys_loc.y + px));
            let shadow_buf =
                smithay::backend::renderer::element::solid::SolidColorBuffer::new(
                    (cell_phys_size.w, cell_phys_size.h),
                    [0.0, 0.0, 0.0, shadow_alpha],
                );
            elements.push(
                GlideWindowRenderElement::Background(SolidColorRenderElement::from_buffer(
                    &shadow_buf,
                    shadow_loc,
                    Scale::from(1.0),
                    alpha,
                    smithay::backend::renderer::element::Kind::Unspecified,
                ))
                .into(),
            );
        }

        elements
    }
}

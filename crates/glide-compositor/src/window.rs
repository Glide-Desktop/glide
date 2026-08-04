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

impl GlideWindow {
    pub fn set_overview_state(&self, state: Option<(f64, Rectangle<i32, Logical>, Point<i32, Logical>)>) {
        self.0.user_data().insert_if_missing(|| std::sync::Mutex::new(None::<(f64, Rectangle<i32, Logical>, Point<i32, Logical>)>));
        let mutex = self.0.user_data().get::<std::sync::Mutex<Option<(f64, Rectangle<i32, Logical>, Point<i32, Logical>)>>>().unwrap();
        *mutex.lock().unwrap() = state;
    }

    pub(crate) fn overview_state(&self) -> Option<(f64, Rectangle<i32, Logical>, Point<i32, Logical>)> {
        self.0.user_data().get::<std::sync::Mutex<Option<(f64, Rectangle<i32, Logical>, Point<i32, Logical>)>>>().and_then(|m| *m.lock().unwrap())
    }
}

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
    fn geometry(&self) -> Rectangle<i32, Logical> {
        let mut geo = SpaceElement::geometry(&self.0);
        if let Some((scale, _, _)) = self.overview_state() {
            geo.loc.x = (geo.loc.x as f64 * scale).round() as i32;
            geo.loc.y = (geo.loc.y as f64 * scale).round() as i32;
            geo.size.w = (geo.size.w as f64 * scale).round() as i32;
            geo.size.h = (geo.size.h as f64 * scale).round() as i32;
        }
        geo
    }

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
        if let Some((scale, _, _)) = self.overview_state() {
            bbox.loc.x = (bbox.loc.x as f64 * scale).round() as i32;
            bbox.loc.y = (bbox.loc.y as f64 * scale).round() as i32;
            bbox.size.w = (bbox.size.w as f64 * scale).round() as i32;
            bbox.size.h = (bbox.size.h as f64 * scale).round() as i32;
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
        let window_scale = self.overview_state().map(|(s, _, _)| s).unwrap_or(1.0);
        let actual_scale = scale * Scale::from(window_scale);

        // `location` is where `Space` resolves the *content* origin to (see
        // the module docs): the window's own content renders here
        // unshifted, and the titlebar — occupying the strip directly above
        // it — is what needs the offset, upward by its height.
        let header_height = self.header_height();
        let header_location =
            location - Point::from((0, header_height)).to_physical_precise_round(actual_scale);

        let painter = crate::paint::PAINTER.get_or_init(|| {
            let font_px = (f64::from(header_height).max(14.0) * 0.45) as f32;
            crate::paint::Painter::new(font_px.clamp(10.0, 18.0))
        });

        let mut elements = Vec::new();

        // Decorations first: `render_frame` wants elements front-to-back,
        // and the titlebar sits on top of the window's own content.
        let deco_elements: Vec<C> = decoration::frame_buffers(self, painter)
            .into_iter()
            .filter_map(|deco_buf| {
                let physical_offset = deco_buf.offset.to_physical_precise_round(actual_scale);
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
            AsRenderElements::render_elements(&self.0, renderer, location, actual_scale, alpha);
        elements.extend(content_elements.into_iter().map(C::from));

        // If in overview, draw the cell background and a shadow behind the window.
        // Since `render_frame` draws front-to-back, elements pushed LAST will be BEHIND.
        if let Some((_, cell_logical_rect, window_global_loc)) = self.overview_state() {
            // Calculate physical location of the cell relative to the window's physical location
            let delta = cell_logical_rect.loc - window_global_loc;
            let cell_physical_loc = location + delta.to_f64().to_physical(scale).to_i32_round();
            let cell_physical_size = cell_logical_rect.size.to_f64().to_physical(scale).to_i32_round();

            let shadow_rect = smithay::utils::Rectangle::new(
                smithay::utils::Point::from((cell_physical_loc.x + (10.0 * scale.x).round() as i32, cell_physical_loc.y + (10.0 * scale.y).round() as i32)),
                cell_physical_size,
            );
            let shadow_buf = smithay::backend::renderer::element::solid::SolidColorBuffer::new(
                (shadow_rect.size.w, shadow_rect.size.h),
                [0.0, 0.0, 0.0, 0.5],
            );
            elements.push(
                GlideWindowRenderElement::Background(SolidColorRenderElement::from_buffer(
                    &shadow_buf,
                    shadow_rect.loc,
                    Scale::from(1.0), // solid buffer is created in physical size
                    alpha,
                    smithay::backend::renderer::element::Kind::Unspecified,
                ))
                .into()
            );

            // Cell background (desktop color)
            let cell_buf = smithay::backend::renderer::element::solid::SolidColorBuffer::new(
                (cell_physical_size.w, cell_physical_size.h),
                [0.160_784_3, 0.325_490_2, 0.588_235_3, 1.0], // FILL_COLOR
            );
            elements.push(
                GlideWindowRenderElement::Background(SolidColorRenderElement::from_buffer(
                    &cell_buf,
                    cell_physical_loc,
                    Scale::from(1.0),
                    alpha,
                    smithay::backend::renderer::element::Kind::Unspecified,
                ))
                .into()
            );
        }

        elements
    }
}

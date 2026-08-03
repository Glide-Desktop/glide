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
        element::{AsRenderElements, surface::WaylandSurfaceRenderElement},
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
        self.decoration_state().header.height()
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
        SpaceElement::geometry(&self.0)
    }

    fn bbox(&self) -> Rectangle<i32, Logical> {
        let mut bbox = SpaceElement::bbox(&self.0);
        let height = self.header_height();
        bbox.loc.y -= height;
        bbox.size.h += height;
        bbox
    }

    fn is_in_input_region(&self, point: &Point<f64, Logical>) -> bool {
        // `point` is relative to the content origin; the decoration's own
        // origin sits `header_height` above it (see the module docs on the
        // coordinate convention). Routed through `HeaderBar::contains`
        // rather than a bare strip check so the open chevron menu — which
        // extends *below* the strip — counts as ours too.
        let in_decoration = {
            let deco = self.decoration_state();
            let rel = *point + Point::from((0.0, f64::from(deco.header.height())));
            deco.header.contains(rel)
        };
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
    /// Everything a `GlideWindow` can put on screen: its own content, plus
    /// (when decorated) the solid-color titlebar drawn on top of it.
    pub GlideWindowRenderElement<R> where R: ImportAll + ImportMem;
    Content = WaylandSurfaceRenderElement<R>,
    Decoration = smithay::backend::renderer::element::solid::SolidColorRenderElement,
}

impl<R: Renderer> std::fmt::Debug for GlideWindowRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Content(e) => f.debug_tuple("Content").field(e).finish(),
            Self::Decoration(e) => f.debug_tuple("Decoration").field(e).finish(),
            Self::_GenericCatcher(e) => f.debug_tuple("_GenericCatcher").field(e).finish(),
        }
    }
}

impl<R> AsRenderElements<R> for GlideWindow
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Clone + 'static,
{
    type RenderElement = GlideWindowRenderElement<R>;

    fn render_elements<C: From<Self::RenderElement>>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Vec<C> {
        // `location` is where `Space` resolves the *content* origin to (see
        // the module docs): the window's own content renders here
        // unshifted, and the titlebar — occupying the strip directly above
        // it — is what needs the offset, upward by its height.
        let header_location =
            location - Point::from((0, self.header_height())).to_physical_precise_round(scale);

        // Decorations first: `render_frame` wants elements front-to-back,
        // and the titlebar sits on top of the window's own content.
        let mut elements: Vec<C> = decoration::render_elements(self, header_location, scale, alpha)
            .into_iter()
            .map(GlideWindowRenderElement::Decoration)
            .map(C::from)
            .collect();

        let content_elements: Vec<GlideWindowRenderElement<R>> =
            AsRenderElements::render_elements(&self.0, renderer, location, scale, alpha);
        elements.extend(content_elements.into_iter().map(C::from));

        elements
    }
}

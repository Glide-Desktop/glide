//! [`GlideWindow`]: a thin wrapper around Smithay's [`Window`].
//!
//! Exists purely so window decorations (M4.3+) have somewhere to live:
//! `SpaceElement` and `AsRenderElements` are foreign traits and `Window` is a
//! foreign type, so the orphan rule forbids implementing decoration-aware
//! versions of them directly on `Window`. Wrapping it in a local type is the
//! standard way around that — this is the same shape as anvil's own
//! `WindowElement`.
//!
//! For now (M4.1) this is a pure pass-through with no decoration logic of
//! its own: `Deref<Target = Window>` means every existing call site that
//! used to hold a `Window` keeps compiling unchanged, and `SpaceElement`/
//! `AsRenderElements` just forward to `Window`'s own impls. Decoration state
//! and rendering are added on top of this in M4.2/M4.3 without having to
//! touch every call site a second time.

use std::ops::Deref;

use smithay::{
    backend::renderer::{
        ImportAll, Renderer,
        element::{AsRenderElements, surface::WaylandSurfaceRenderElement},
    },
    desktop::{Window, space::SpaceElement},
    output::Output,
    utils::{IsAlive, Logical, Physical, Point, Rectangle, Scale},
};

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
    /// a `Window` rather than anything `Deref`-coercible to one.
    #[must_use]
    pub fn window(&self) -> &Window {
        &self.0
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
    fn geometry(&self) -> Rectangle<i32, Logical> {
        SpaceElement::geometry(&self.0)
    }

    fn bbox(&self) -> Rectangle<i32, Logical> {
        SpaceElement::bbox(&self.0)
    }

    fn is_in_input_region(&self, point: &Point<f64, Logical>) -> bool {
        SpaceElement::is_in_input_region(&self.0, point)
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

impl<R> AsRenderElements<R> for GlideWindow
where
    R: Renderer + ImportAll,
    R::TextureId: Clone + 'static,
{
    type RenderElement = WaylandSurfaceRenderElement<R>;

    fn render_elements<C: From<Self::RenderElement>>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Vec<C> {
        AsRenderElements::render_elements(&self.0, renderer, location, scale, alpha)
    }
}

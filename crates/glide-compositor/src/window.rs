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
            AsRenderElements, memory::MemoryRenderBufferRenderElement,
            solid::SolidColorRenderElement, surface::WaylandSurfaceRenderElement,
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
    pub(crate) fn header_height(&self) -> i32 {
        self.try_decoration_state()
            .map_or(0, |state| state.header.height())
    }

    /// The window's `app_id` and `title`, as last set by the client.
    ///
    /// Shared by the IPC listings and the overview's labels so both name a
    /// window the same way.
    #[must_use]
    pub fn identity(&self) -> (Option<String>, Option<String>) {
        let Some(toplevel) = self.toplevel() else {
            return (None, None);
        };

        smithay::wayland::compositor::with_states(toplevel.wl_surface(), |states| {
            states
                .data_map
                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                .map_or((None, None), |data| {
                    let guard = data
                        .lock()
                        .expect("single-threaded event loop, this mutex is never poisoned");
                    (guard.app_id.clone(), guard.title.clone())
                })
        })
    }

    /// A human-readable name for the window: its title, falling back to the
    /// app id, then to a placeholder — an overview preview always gets a
    /// label, even from a client that set neither.
    #[must_use]
    pub fn display_name(&self) -> String {
        let (app_id, title) = self.identity();
        title
            .filter(|t| !t.trim().is_empty())
            .or(app_id)
            .unwrap_or_else(|| "Untitled".to_string())
    }

    /// Left decoration inset. Always `0` today — Glide's titlebar reserves
    /// space at the top only — but named rather than hardcoded at call
    /// sites so adding side borders later is a change in one place.
    pub(crate) const fn insets_left() -> i32 {
        0
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
    fn geometry(&self) -> Rectangle<i32, Logical> {
        SpaceElement::geometry(&self.0)
    }

    /// The visual bounding box: content + titlebar + resize border.
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
            // Titlebar heights are tens of pixels; the product is nowhere
            // near f32's precision limits.
            #[allow(clippy::cast_possible_truncation)]
            let font_px = (f64::from(h).max(14.0) * 0.45) as f32;
            crate::paint::Painter::new(font_px.clamp(10.0, 18.0))
        });

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

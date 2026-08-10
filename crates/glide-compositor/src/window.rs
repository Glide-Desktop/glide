use std::ops::Deref;

use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer,
        element::{
            AsRenderElements, Element, memory::MemoryRenderBufferRenderElement,
            surface::WaylandSurfaceRenderElement,
        },
    },
    desktop::{Window, space::SpaceElement},
    output::Output,
    render_elements,
    utils::{IsAlive, Logical, Physical, Point, Rectangle, Scale, Size},
};

use crate::{
    clip::{ClipCapableRenderer, ClippedSurfaceElement, RoundedClip},
    decoration,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GlideWindow(Window);

impl GlideWindow {
    pub fn new(window: Window) -> Self {
        Self(window)
    }

    #[must_use]
    pub fn window(&self) -> &Window {
        &self.0
    }

    pub(crate) fn header_height(&self) -> i32 {
        self.try_decoration_state()
            .map_or(0, |state| state.header.height())
    }

    pub(crate) fn shadow_margin(&self) -> i32 {
        self.try_decoration_state()
            .map_or(0, |state| state.header.shadow_margin())
    }

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

    #[must_use]
    pub fn display_name(&self) -> String {
        let (app_id, title) = self.identity();
        title
            .filter(|t| !t.trim().is_empty())
            .or(app_id)
            .unwrap_or_else(|| "Untitled".to_string())
    }

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

impl SpaceElement for GlideWindow {
    fn geometry(&self) -> Rectangle<i32, Logical> {
        SpaceElement::geometry(&self.0)
    }

    fn bbox(&self) -> Rectangle<i32, Logical> {
        let mut bbox = SpaceElement::bbox(&self.0);
        let height = self.header_height();
        let band = if height > 0 {
            decoration::RESIZE_BORDER.max(self.shadow_margin())
        } else {
            decoration::RESIZE_BORDER
        };
        bbox.loc.x -= band;
        bbox.loc.y -= height + band;
        bbox.size.w += band * 2;
        bbox.size.h += height + band * 2;
        bbox
    }

    fn is_in_input_region(&self, point: &Point<f64, Logical>) -> bool {
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
    pub GlideWindowRenderElement<R> where R: ImportAll + ImportMem + ClipCapableRenderer;
    Content = WaylandSurfaceRenderElement<R>,
    ClippedContent = ClippedSurfaceElement<R>,
    Decoration = MemoryRenderBufferRenderElement<R>,
}

impl<R: Renderer> std::fmt::Debug for GlideWindowRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Content(e) => f.debug_tuple("Content").field(e).finish(),
            Self::ClippedContent(e) => f.debug_tuple("ClippedContent").field(e).finish(),
            Self::Decoration(e) => f.debug_tuple("Decoration").field(e).finish(),
            Self::_GenericCatcher(e) => f.debug_tuple("_GenericCatcher").field(e).finish(),
        }
    }
}

impl GlideWindow {
    /// The rounded outline this window's client content has to be cut
    /// against, in physical px, or `None` when it needs no cutting.
    ///
    /// The rect is the **whole frame**, titlebar included, so the two top
    /// corners of the rounding fall on the titlebar buffer — which already
    /// draws its own rounded top and covers them — and only the bottom two
    /// actually bite into client pixels. Maximised and undecorated windows
    /// opt out: the former is square by design, and the latter draws its
    /// own corners client-side, so clipping would round them twice.
    fn content_clip_frame(
        &self,
        frame_location: Point<i32, Physical>,
        scale: Scale<f64>,
    ) -> Option<(Rectangle<i32, Physical>, f32)> {
        let state = self.try_decoration_state()?;
        if !state.header.is_decorated() || state.header.maximized {
            return None;
        }
        let radius = state.header.theme.corner_radius;
        if radius <= 0.0 {
            return None;
        }
        drop(state);

        let content = self.geometry().size;
        let frame_logical = Size::from((content.w, content.h + self.header_height()));
        #[allow(clippy::cast_possible_truncation)]
        let radius_physical = (f64::from(radius) * scale.y) as f32;

        Some((
            Rectangle::new(
                frame_location,
                frame_logical.to_physical_precise_round(scale),
            ),
            radius_physical,
        ))
    }
}

impl GlideWindow {
    fn render_elements_impl<R, C>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
        include_box_deco: bool,
    ) -> Vec<C>
    where
        R: Renderer + ImportAll + ImportMem + ClipCapableRenderer,
        R::TextureId: Send + Clone + 'static,
        R::Error: std::fmt::Debug,
        C: From<GlideWindowRenderElement<R>>,
    {
        let painter = crate::paint::current();

        let header_height = self.header_height();
        let header_location =
            location - Point::from((0, header_height)).to_physical_precise_round(scale);

        let mut elements = Vec::new();

        let deco_elements: Vec<C> = decoration::frame_buffers(self, &painter)
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

        if include_box_deco {
            // Rounded first, and real-world output scales are tiny integers.
            #[allow(clippy::cast_possible_truncation)]
            let int_scale = (scale.x.round() as i32).max(1);
            if let Some(deco_buf) = decoration::shadow_and_border_buffer(self, int_scale) {
                let physical_offset = deco_buf.offset.to_physical_precise_round(scale);
                let pos = (header_location + physical_offset).to_f64();
                if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    pos,
                    &deco_buf.buf,
                    Some(alpha),
                    None,
                    None,
                    smithay::backend::renderer::element::Kind::Unspecified,
                ) {
                    elements.push(C::from(GlideWindowRenderElement::Decoration(element)));
                }
            }
        }

        // Client content last (i.e. furthest back of this window's own
        // layers), with its corners cut to the frame outline when the
        // renderer can do it.
        let clip_frame = self
            .content_clip_frame(header_location, scale)
            .filter(|_| renderer.prepare_rounded_clip());

        let content_elements: Vec<WaylandSurfaceRenderElement<R>> =
            AsRenderElements::render_elements(&self.0, renderer, location, scale, alpha);

        for element in content_elements {
            // Built per element: a surface's own destination is the frame
            // of reference the clip has to be expressed in, and a window's
            // subsurfaces each have their own.
            let clip = clip_frame.and_then(|(frame, radius)| {
                let dst = element.geometry(scale);
                crate::clip::clip_applies(element.transform(), dst)
                    .then(|| RoundedClip::from_frame(dst, frame, radius))
                    .flatten()
                    .filter(RoundedClip::is_visible)
            });

            let element = match clip {
                Some(clip) => GlideWindowRenderElement::ClippedContent(ClippedSurfaceElement::new(
                    element, clip,
                )),
                None => GlideWindowRenderElement::Content(element),
            };
            elements.push(C::from(element));
        }

        elements
    }

    /// Render elements for an overview preview: content + titlebar, but
    /// *without* the window's own drop-shadow/border buffer. A preview is
    /// rescaled wholesale, so including that buffer would draw a second,
    /// denser (scaled-down) shadow on top of the overview's own per-preview
    /// shadow, and its blur margin would leak far outside the preview's
    /// slot into neighbouring previews and labels.
    pub(crate) fn preview_render_elements<R>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Vec<GlideWindowRenderElement<R>>
    where
        R: Renderer + ImportAll + ImportMem + ClipCapableRenderer,
        R::TextureId: Send + Clone + 'static,
        R::Error: std::fmt::Debug,
    {
        self.render_elements_impl(renderer, location, scale, alpha, false)
    }
}

impl<R> AsRenderElements<R> for GlideWindow
where
    R: Renderer + ImportAll + ImportMem + ClipCapableRenderer,
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
        self.render_elements_impl(renderer, location, scale, alpha, true)
    }
}

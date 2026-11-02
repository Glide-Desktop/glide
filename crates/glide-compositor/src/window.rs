use std::{
    cell::RefCell,
    ops::Deref,
    time::{Duration, Instant},
};

use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer,
        element::{
            AsRenderElements, Element, Kind,
            memory::MemoryRenderBufferRenderElement,
            surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
            utils::{Relocate, RelocateRenderElement, RescaleRenderElement},
        },
    },
    desktop::{PopupManager, Window, space::SpaceElement},
    output::Output,
    render_elements,
    utils::{IsAlive, Logical, Physical, Point, Rectangle, Scale, Size},
};

use crate::{
    clip::{ClipCapableRenderer, ClippedMemoryElement, ClippedSurfaceElement, RoundedClip},
    decoration,
    raster::ShellRenderer,
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
        self.try_decoration_state().map_or(0, |state| state.header.height())
    }

    pub(crate) fn shadow_margin(&self) -> i32 {
        self.try_decoration_state()
            .map_or(0, |state| if state.header.is_framed() { state.header.shadow_margin() } else { 0 })
    }

    pub(crate) fn is_framed(&self) -> bool {
        self.try_decoration_state().is_some_and(|state| state.header.is_framed())
    }

    #[must_use]
    pub fn is_animating_geometry(&self) -> bool {
        self.open_animation().is_some() || self.geometry_animation().is_some()
    }

    #[must_use]
    pub fn frames_itself(&self) -> bool {
        self.identity().0.as_deref().is_some_and(crate::decoration::glide_csd_app)
    }

    #[must_use]
    pub fn identity(&self) -> (Option<String>, Option<String>) {
        let Some(toplevel) = self.toplevel() else {
            return (None, None);
        };

        smithay::wayland::compositor::with_states(toplevel.wl_surface(), |states| {
            states.data_map.get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>().map_or((None, None), |data| {
                let guard = data.lock().expect("single-threaded event loop, this mutex is never poisoned");
                (guard.app_id.clone(), guard.title.clone())
            })
        })
    }

    #[must_use]
    pub fn display_name(&self) -> String {
        let (app_id, title) = self.identity();
        title.filter(|t| !t.trim().is_empty()).or(app_id).unwrap_or_else(|| "Untitled".to_string())
    }

    pub(crate) fn begin_open_animation(&self, origin: Option<Rectangle<i32, Logical>>, frame: Rectangle<i32, Logical>) {
        if open_scale_duration().is_zero() {
            return;
        }

        let (start_scale, start_offset) = match origin {
            Some(origin) if frame.size.w > 0 && frame.size.h > 0 => (
                f64::from(origin.size.w) / f64::from(frame.size.w),
                Point::from((origin.loc.x - frame.loc.x, origin.loc.y - frame.loc.y)),
            ),
            _ => (OPEN_START_SCALE, Point::from((0, 0))),
        };
        self.store_animation(OpenAnimation { started: Instant::now(), start_scale, start_offset });
    }

    fn store_animation<T: 'static>(&self, animation: T) {
        self.0.user_data().insert_if_missing(|| RefCell::new(Option::<T>::None));
        if let Some(cell) = self.0.user_data().get::<RefCell<Option<T>>>()
            && let Ok(mut slot) = cell.try_borrow_mut()
        {
            *slot = Some(animation);
        }
    }

    fn with_animation<T: 'static, U>(&self, read: impl FnOnce(&T) -> Option<U>) -> Option<U> {
        let cell = self.0.user_data().get::<RefCell<Option<T>>>()?;
        let animation = cell.try_borrow().ok()?;
        read(animation.as_ref()?)
    }

    pub(crate) fn open_animation(&self) -> Option<OpenProgress> {
        self.with_animation(|animation: &OpenAnimation| {
            let elapsed = running(animation.started, open_scale_duration())?;
            Some(OpenProgress::at(elapsed, animation.start_scale, animation.start_offset))
        })
    }

    pub(crate) fn is_open_animating(&self) -> bool {
        self.open_animation().is_some()
    }

    pub(crate) fn begin_geometry_animation(&self, from: Rectangle<i32, Logical>, to: Rectangle<i32, Logical>) {
        self.begin_geometry_animation_over(from, to, geometry_duration());
    }

    pub(crate) fn begin_geometry_animation_over(
        &self, from: Rectangle<i32, Logical>, to: Rectangle<i32, Logical>, duration: Duration,
    ) {
        if duration.is_zero() || from == to || to.size.w <= 0 || to.size.h <= 0 || from.size.w <= 0 || from.size.h <= 0
        {
            return;
        }
        self.store_animation(GeometryAnimation { started: Instant::now(), duration, from, to });
    }

    pub(crate) fn cancel_geometry_animation(&self) {
        if let Some(cell) = self.0.user_data().get::<RefCell<Option<GeometryAnimation>>>()
            && let Ok(mut slot) = cell.try_borrow_mut()
        {
            *slot = None;
        }
    }

    pub(crate) fn geometry_animation(&self) -> Option<GeometryProgress> {
        self.with_animation(|animation: &GeometryAnimation| {
            let duration = animation.duration;
            let elapsed = running(animation.started, duration)?;
            Some(GeometryProgress::at(elapsed, duration, animation.from, animation.to, self.frame_size()))
        })
    }

    pub(crate) fn animated_frame(&self) -> Option<Rectangle<i32, Logical>> {
        self.with_animation(|animation: &GeometryAnimation| {
            let duration = animation.duration;
            let elapsed = running(animation.started, duration)?;
            let t = glide_theme::motion::E_OUT.ease_f64(elapsed.as_secs_f64() / duration.as_secs_f64());
            let mix = |a: i32, b: i32| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as i32;
            let (from, to) = (animation.from, animation.to);
            Some(Rectangle::new(
                Point::from((mix(from.loc.x, to.loc.x), mix(from.loc.y, to.loc.y))),
                Size::from((mix(from.size.w, to.size.w).max(1), mix(from.size.h, to.size.h).max(1))),
            ))
        })
    }

    fn frame_size(&self) -> Size<i32, Logical> {
        frame_of(self.geometry().size, self.header_height())
    }

    pub(crate) fn is_geometry_animating(&self) -> bool {
        self.geometry_animation().is_some()
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn decoration_buffer_scale(&self, scale: Scale<f64>) -> i32 {
        (scale.x.max(scale.y).ceil() as i32).max(1)
    }
}

struct OpenAnimation {
    started: Instant,
    start_scale: f64,
    start_offset: Point<i32, Logical>,
}

struct GeometryAnimation {
    started: Instant,
    duration: Duration,
    from: Rectangle<i32, Logical>,
    to: Rectangle<i32, Logical>,
}

fn frame_of(content: Size<i32, Logical>, header: i32) -> Size<i32, Logical> {
    Size::from((content.w, content.h + header))
}

fn running(started: Instant, duration: Duration) -> Option<Duration> {
    let elapsed = started.elapsed();
    (!duration.is_zero() && elapsed < duration).then_some(elapsed)
}

pub(crate) fn geometry_duration() -> Duration {
    glide_theme::motion::t3()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GeometryProgress {
    pub(crate) scale: Scale<f64>,
    pub(crate) offset: Point<i32, Logical>,
}

impl GeometryProgress {
    fn at(
        elapsed: Duration, duration: Duration, from: Rectangle<i32, Logical>, to: Rectangle<i32, Logical>,
        live_size: Size<i32, Logical>,
    ) -> Self {
        let fraction = (elapsed.as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0);
        let t = glide_theme::motion::E_OUT.ease_f64(fraction);

        let mix = |a: i32, b: i32| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as i32;

        let current_w = mix(from.size.w, to.size.w).max(1);
        let current_h = mix(from.size.h, to.size.h).max(1);

        Self {
            scale: Scale::from((
                f64::from(current_w) / f64::from(live_size.w.max(1)),
                f64::from(current_h) / f64::from(live_size.h.max(1)),
            )),
            offset: Point::from((mix(from.loc.x, to.loc.x) - to.loc.x, mix(from.loc.y, to.loc.y) - to.loc.y)),
        }
    }
}

fn open_scale_duration() -> Duration {
    glide_theme::motion::t4()
}

#[cfg(test)]
fn open_alpha_duration() -> Duration {
    open_scale_duration().mul_f64(glide_theme::motion::OPACITY_LEAD)
}

const OPEN_START_SCALE: f64 = 0.86;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpenProgress {
    pub(crate) scale: f64,
    pub(crate) offset: Point<i32, Logical>,
    pub(crate) alpha: f32,
}

impl OpenProgress {
    fn at(elapsed: Duration, start_scale: f64, start_offset: Point<i32, Logical>) -> Self {
        let fraction = |total: Duration| {
            if total.is_zero() { 1.0 } else { (elapsed.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0) }
        };

        let scale_progress = fraction(open_scale_duration());
        let scale_t = glide_theme::motion::E_OUT.ease_f64(scale_progress);
        let alpha_t = glide_theme::motion::assist(scale_progress);

        let offset = Point::from((
            (f64::from(start_offset.x) * (1.0 - scale_t)).round() as i32,
            (f64::from(start_offset.y) * (1.0 - scale_t)).round() as i32,
        ));

        Self { scale: start_scale + (1.0 - start_scale) * scale_t, offset, alpha: alpha_t as f32 }
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
        let band = if self.is_framed() {
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
    pub GlideWindowLayer<R> where R: ImportAll + ImportMem + ClipCapableRenderer;
    Content = WaylandSurfaceRenderElement<R>,
    ClippedContent = ClippedSurfaceElement<R>,
    Decoration = MemoryRenderBufferRenderElement<R>,
    ClippedDecoration = ClippedMemoryElement<R>,
    ScaledDecoration = RescaleRenderElement<MemoryRenderBufferRenderElement<R>>,
}

render_elements! {
    pub GlideWindowRenderElement<R> where R: ImportAll + ImportMem + ClipCapableRenderer;
    Plain = GlideWindowLayer<R>,
    Scaled = RelocateRenderElement<RescaleRenderElement<GlideWindowLayer<R>>>,
}

impl<R: Renderer> std::fmt::Debug for GlideWindowLayer<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Content(e) => f.debug_tuple("Content").field(e).finish(),
            Self::ClippedContent(e) => f.debug_tuple("ClippedContent").field(e).finish(),
            Self::Decoration(e) => f.debug_tuple("Decoration").field(e).finish(),
            Self::ClippedDecoration(e) => f.debug_tuple("ClippedDecoration").field(e).finish(),
            Self::ScaledDecoration(e) => f.debug_tuple("ScaledDecoration").field(e).finish(),
            Self::_GenericCatcher(e) => f.debug_tuple("_GenericCatcher").field(e).finish(),
        }
    }
}

impl<R: Renderer> std::fmt::Debug for GlideWindowRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plain(e) => f.debug_tuple("Plain").field(e).finish(),
            Self::Scaled(e) => f.debug_tuple("Scaled").field(e).finish(),
            Self::_GenericCatcher(e) => f.debug_tuple("_GenericCatcher").field(e).finish(),
        }
    }
}

impl GlideWindow {
    fn content_clip_frame(
        &self, frame_location: Point<i32, Physical>, scale: Scale<f64>,
    ) -> Option<(Rectangle<i32, Physical>, f32)> {
        let state = self.try_decoration_state()?;
        if !state.header.is_framed() || state.header.maximized {
            return None;
        }
        let radius = state.header.theme.corner_radius;
        if radius <= 0.0 {
            return None;
        }
        drop(state);

        let content = self.geometry().size;
        let frame_logical = Size::from((content.w, content.h + self.header_height()));
        let radius_physical = (f64::from(radius) * scale.y) as f32;

        Some((Rectangle::new(frame_location, frame_logical.to_physical_precise_round(scale)), radius_physical))
    }

    fn preview_clip_frame(
        &self, location: Point<i32, Physical>, scale: Scale<f64>, shape: PreviewShape,
    ) -> Option<(Rectangle<i32, Physical>, f32)> {
        if shape.radius <= 0.0 || shape.scale <= 0.0 {
            return None;
        }
        let geometry = self.geometry();
        let header = self.header_height();
        let origin = location + geometry.loc.to_physical_precise_round(scale)
            - Point::<i32, Logical>::from((0, header)).to_physical_precise_round(scale);
        let frame_logical = Size::from((geometry.size.w, geometry.size.h + header));
        let radius_physical = (f64::from(shape.radius) / shape.scale * scale.y) as f32;
        Some((Rectangle::new(origin, frame_logical.to_physical_precise_round(scale)), radius_physical))
    }

    fn surface_elements<R>(
        &self, renderer: &mut R, location: Point<i32, Physical>, scale: Scale<f64>, alpha: f32,
    ) -> Vec<WaylandSurfaceRenderElement<R>>
    where
        R: ShellRenderer + ClipCapableRenderer,
        R::Error: std::fmt::Debug,
    {
        let Some(toplevel) = self.0.toplevel() else {
            return AsRenderElements::render_elements(&self.0, renderer, location, scale, alpha);
        };
        let surface = toplevel.wl_surface();
        let mut elements = Vec::new();
        for (popup, offset) in PopupManager::popups_for_surface(surface) {
            let travel = crate::surface_fade::travel(popup.wl_surface());
            let travel_shift = Point::<i32, Logical>::from((0, travel.round() as i32)).to_physical_precise_round(scale);
            let shift =
                (self.0.geometry().loc + offset - popup.geometry().loc).to_physical_precise_round(scale) + travel_shift;
            let fade = crate::surface_fade::alpha(popup.wl_surface());
            elements.extend(render_elements_from_surface_tree(
                renderer,
                popup.wl_surface(),
                location + shift,
                scale,
                alpha * fade,
                Kind::Unspecified,
            ));
        }
        let root: Vec<WaylandSurfaceRenderElement<R>> =
            render_elements_from_surface_tree(renderer, surface, location, scale, alpha, Kind::Unspecified);
        if root.is_empty() {
            elements.extend(destroyed_surface_element(renderer, surface, location, scale, alpha));
        } else {
            elements.extend(root);
        }
        elements
    }

    fn content_layers<R>(
        &self, renderer: &mut R, location: Point<i32, Physical>, clip_frame: Option<(Rectangle<i32, Physical>, f32)>,
        scale: Scale<f64>, alpha: f32,
    ) -> Vec<GlideWindowLayer<R>>
    where
        R: ShellRenderer + ClipCapableRenderer,
        R::Error: std::fmt::Debug,
    {
        let content_elements = self.surface_elements(renderer, location, scale, alpha);

        content_elements
            .into_iter()
            .map(|element| {
                let clip = clip_frame.and_then(|(frame, radius)| {
                    let dst = element.geometry(scale);
                    crate::clip::clip_applies(element.transform(), dst)
                        .then(|| RoundedClip::from_frame(dst, frame, radius))
                        .flatten()
                        .filter(RoundedClip::is_visible)
                });

                match clip {
                    Some(clip) => GlideWindowLayer::ClippedContent(ClippedSurfaceElement::new(element, clip)),
                    None => GlideWindowLayer::Content(element),
                }
            })
            .collect()
    }

    fn chrome_layers<R>(
        &self, renderer: &mut R, painter: &crate::paint::Painter, header_location: Point<i32, Physical>,
        scale: Scale<f64>, alpha: f32, chrome_clip: Option<(Rectangle<i32, Physical>, f32)>,
    ) -> (Vec<GlideWindowLayer<R>>, Vec<GlideWindowLayer<R>>)
    where
        R: ShellRenderer + ClipCapableRenderer,
        R::Error: std::fmt::Debug,
    {
        let buffer_scale = self.decoration_buffer_scale(scale);
        let (overlays, chrome): (Vec<_>, Vec<_>) = decoration::frame_buffers(self, painter, buffer_scale)
            .into_iter()
            .filter(|deco_buf| deco_buf.alpha > 0.0)
            .partition(|deco_buf| deco_buf.overlay);
        let upload = |renderer: &mut R, buffers: Vec<decoration::DecorationBuffer>, clip| {
            buffers
                .iter()
                .filter_map(|deco_buf| Self::decoration_layer(renderer, deco_buf, header_location, scale, alpha, clip))
                .collect::<Vec<_>>()
        };
        (upload(renderer, overlays, None), upload(renderer, chrome, chrome_clip))
    }

    fn decoration_layer<R>(
        renderer: &mut R, deco_buf: &decoration::DecorationBuffer, header_location: Point<i32, Physical>,
        scale: Scale<f64>, alpha: f32, clip_frame: Option<(Rectangle<i32, Physical>, f32)>,
    ) -> Option<GlideWindowLayer<R>>
    where
        R: ShellRenderer + ClipCapableRenderer,
        R::Error: std::fmt::Debug,
    {
        let physical_offset = deco_buf.offset.to_physical_precise_round(scale);
        let pos = (header_location + physical_offset).to_f64();
        let element = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            pos,
            &deco_buf.buf,
            Some(alpha * deco_buf.alpha),
            None,
            None,
            smithay::backend::renderer::element::Kind::Unspecified,
        )
        .map_err(|e| tracing::warn!("failed to upload decoration buffer: {:?}", e))
        .ok()?;

        if let Some(factor) = deco_buf.scale {
            return Some(GlideWindowLayer::ScaledDecoration(RescaleRenderElement::from_element(
                element,
                header_location + physical_offset,
                factor,
            )));
        }
        let clip = clip_frame.and_then(|(frame, radius)| {
            RoundedClip::from_frame(element.geometry(scale), frame, radius).filter(RoundedClip::is_visible)
        });
        Some(match clip {
            Some(clip) => GlideWindowLayer::ClippedDecoration(ClippedMemoryElement::new(element, clip)),
            None => GlideWindowLayer::Decoration(element),
        })
    }

    fn render_elements_impl<R, C>(
        &self, renderer: &mut R, location: Point<i32, Physical>, scale: Scale<f64>, alpha: f32,
        preview: Option<PreviewShape>, animated: bool,
    ) -> Vec<C>
    where
        R: ShellRenderer + ClipCapableRenderer,
        R::Error: std::fmt::Debug,
        C: From<GlideWindowRenderElement<R>>,
    {
        let painter = crate::paint::current();
        let open = self.open_animation().filter(|_| animated);
        let geometry = self.geometry_animation().filter(|_| animated);

        let alpha = alpha * open.map_or(1.0, |progress| progress.alpha);

        let header_height = self.header_height();
        let header_location = location - Point::from((0, header_height)).to_physical_precise_round(scale);

        let clip_frame = match preview {
            Some(shape) => self.preview_clip_frame(location, scale, shape),
            None => self.content_clip_frame(header_location, scale),
        }
        .filter(|_| renderer.prepare_rounded_clip());
        let chrome_clip = clip_frame.filter(|_| preview.is_some());

        let content = self.content_layers(renderer, location, clip_frame, scale, alpha);
        if content.is_empty() {
            return Vec::new();
        }
        let (overlays, chrome) = self.chrome_layers(renderer, &painter, header_location, scale, alpha, chrome_clip);
        let mut layers: Vec<GlideWindowLayer<R>> = overlays;
        layers.extend(content);
        layers.extend(chrome);

        if preview.is_none() {
            let buffer_scale = self.decoration_buffer_scale(scale);
            let box_deco = decoration::shadow_and_border_buffer(self, buffer_scale);
            let fade = decoration::shadow_and_border_fade(self, buffer_scale).map(|(mut buf, fade_alpha)| {
                buf.alpha = fade_alpha;
                buf
            });
            layers.extend(fade.iter().chain(&box_deco).filter_map(|deco_buf| {
                Self::decoration_layer(renderer, deco_buf, header_location, scale, alpha, None)
            }));
        }

        let mut factor = Scale::from((1.0, 1.0));
        let mut travel = Point::<i32, Logical>::from((0, 0));
        if let Some(progress) = open {
            factor.x *= progress.scale;
            factor.y *= progress.scale;
            travel += progress.offset;
        }
        if let Some(progress) = geometry {
            factor.x *= progress.scale.x;
            factor.y *= progress.scale.y;
            travel += progress.offset;
        }

        let transformed = (factor.x - 1.0).abs() > f64::EPSILON
            || (factor.y - 1.0).abs() > f64::EPSILON
            || travel != Point::from((0, 0));

        if transformed {
            let travel = travel.to_physical_precise_round(scale);
            let frame_origin = header_location + self.geometry().loc.to_physical_precise_round(scale);
            layers
                .into_iter()
                .map(|layer| {
                    let scaled = RescaleRenderElement::from_element(layer, frame_origin, factor);
                    C::from(GlideWindowRenderElement::Scaled(RelocateRenderElement::from_element(
                        scaled,
                        travel,
                        Relocate::Relative,
                    )))
                })
                .collect()
        } else {
            layers.into_iter().map(|layer| C::from(GlideWindowRenderElement::Plain(layer))).collect()
        }
    }
}

fn destroyed_surface_element<R>(
    renderer: &mut R, surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    location: Point<i32, Physical>, scale: Scale<f64>, alpha: f32,
) -> Option<WaylandSurfaceRenderElement<R>>
where
    R: ShellRenderer + ClipCapableRenderer,
    R::Error: std::fmt::Debug,
{
    smithay::wayland::compositor::with_states(surface, |states| {
        let offset = states
            .data_map
            .get::<smithay::backend::renderer::utils::RendererSurfaceStateUserData>()?
            .lock()
            .ok()?
            .view()?
            .offset;
        let location = location.to_f64() + offset.to_f64().to_physical(scale);
        WaylandSurfaceRenderElement::from_surface(renderer, surface, states, location, alpha, Kind::Unspecified)
            .ok()
            .flatten()
    })
}

impl<R> AsRenderElements<R> for GlideWindow
where
    R: ShellRenderer + ClipCapableRenderer,
    R::Error: std::fmt::Debug,
{
    type RenderElement = GlideWindowRenderElement<R>;

    fn render_elements<C: From<Self::RenderElement>>(
        &self, renderer: &mut R, location: Point<i32, Physical>, scale: Scale<f64>, alpha: f32,
    ) -> Vec<C> {
        self.render_elements_impl(renderer, location, scale, alpha, None, true)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PreviewShape {
    pub(crate) radius: f32,
    pub(crate) scale: f64,
}

impl GlideWindow {
    pub(crate) fn preview_render_elements<R>(
        &self, renderer: &mut R, location: Point<i32, Physical>, scale: Scale<f64>, alpha: f32, shape: PreviewShape,
    ) -> Vec<GlideWindowRenderElement<R>>
    where
        R: ShellRenderer + ClipCapableRenderer,
        R::Error: std::fmt::Debug,
    {
        self.render_elements_impl(renderer, location, scale, alpha, Some(shape), true)
    }

    pub(crate) fn still_render_elements<R>(
        &self, renderer: &mut R, location: Point<i32, Physical>, scale: Scale<f64>,
    ) -> Vec<GlideWindowRenderElement<R>>
    where
        R: ShellRenderer + ClipCapableRenderer,
        R::Error: std::fmt::Debug,
    {
        self.render_elements_impl(renderer, location, scale, 1.0, None, false)
    }
}

#[cfg(test)]
mod open_animation_tests {
    use super::*;

    #[test]
    fn it_starts_at_the_designs_scale_and_fully_transparent() {
        let start = OpenProgress::at(Duration::ZERO, OPEN_START_SCALE, Point::from((0, 0)));
        assert!((start.scale - OPEN_START_SCALE).abs() < 1e-9);
        assert!(start.alpha.abs() < 1e-6);
    }

    #[test]
    fn it_lands_exactly_on_one_when_the_longer_track_finishes() {
        let settled = OpenProgress::at(open_scale_duration(), OPEN_START_SCALE, Point::from((0, 0)));
        assert!((settled.scale - 1.0).abs() < 1e-9);
        assert!((settled.alpha - 1.0).abs() < 1e-6);
    }

    #[test]
    fn opacity_finishes_before_the_scale_does() {
        assert!(open_alpha_duration() < open_scale_duration());

        let at_alpha_end = OpenProgress::at(open_alpha_duration(), OPEN_START_SCALE, Point::from((0, 0)));
        assert!((at_alpha_end.alpha - 1.0).abs() < 1e-6, "alpha should be done within 40 % of scale motion");
        assert!(at_alpha_end.scale < 1.0, "the scale should still have ground to cover");
    }

    #[test]
    fn the_window_only_ever_grows_towards_its_real_size() {
        let mut previous = OpenProgress::at(Duration::ZERO, OPEN_START_SCALE, Point::from((0, 0))).scale;
        for step in 1..=100 {
            let elapsed = open_scale_duration().mul_f64(f64::from(step) / 100.0);
            let scale = OpenProgress::at(elapsed, OPEN_START_SCALE, Point::from((0, 0))).scale;
            assert!(scale >= previous - 1e-9, "scale went backwards at step {step}: {scale} < {previous}");
            assert!((OPEN_START_SCALE..=1.0).contains(&scale), "scale left its range at step {step}: {scale}");
            previous = scale;
        }
    }

    #[test]
    fn a_launch_from_a_tile_starts_at_the_tile() {
        let frame: Rectangle<i32, Logical> = Rectangle::new(Point::from((400, 300)), Size::from((800, 600)));
        let tile: Rectangle<i32, Logical> = Rectangle::new(Point::from((900, 1000)), Size::from((42, 42)));

        let start_scale = f64::from(tile.size.w) / f64::from(frame.size.w);
        let start_offset = Point::from((tile.loc.x - frame.loc.x, tile.loc.y - frame.loc.y));

        let begin = OpenProgress::at(Duration::ZERO, start_scale, start_offset);
        assert!((begin.scale - start_scale).abs() < 1e-9);
        assert_eq!(begin.offset, start_offset, "it must begin at the tile");

        let settled = OpenProgress::at(open_scale_duration(), start_scale, start_offset);
        assert!((settled.scale - 1.0).abs() < 1e-9);
        assert_eq!(settled.offset, Point::from((0, 0)), "and land exactly where the layout put it");
    }

    #[test]
    fn a_launch_with_no_tile_does_not_travel() {
        for step in 0..=10 {
            let elapsed = open_scale_duration().mul_f64(f64::from(step) / 10.0);
            let progress = OpenProgress::at(elapsed, OPEN_START_SCALE, Point::from((0, 0)));
            assert_eq!(progress.offset, Point::from((0, 0)));
        }
    }

    #[test]
    fn the_grow_is_front_loaded_like_the_curve_it_rides() {
        let quarter = OpenProgress::at(open_scale_duration().mul_f64(0.25), OPEN_START_SCALE, Point::from((0, 0)));
        let travelled = (quarter.scale - OPEN_START_SCALE) / (1.0 - OPEN_START_SCALE);
        assert!(travelled > 0.5, "only {travelled} of the grow done a quarter of the way in");
    }
}

#[cfg(test)]
mod geometry_animation_tests {
    use super::*;

    use crate::raster::rect;

    fn maximize() -> (Rectangle<i32, Logical>, Rectangle<i32, Logical>) {
        (rect(400, 300, 800, 600), rect(0, 0, 1920, 1080))
    }

    #[test]
    fn it_begins_exactly_on_the_old_rect() {
        let (from, to) = maximize();
        let start = GeometryProgress::at(Duration::ZERO, Duration::from_millis(320), from, to, to.size);

        assert!((start.scale.x - 800.0 / 1920.0).abs() < 1e-9);
        assert!((start.scale.y - 600.0 / 1080.0).abs() < 1e-9);
        assert_eq!(
            start.offset,
            Point::from((400, 300)),
            "the offset is relative to the window's real, final position"
        );
    }

    #[test]
    fn it_lands_exactly_on_the_new_rect() {
        let (from, to) = maximize();
        let duration = Duration::from_millis(320);
        let settled = GeometryProgress::at(duration, duration, from, to, to.size);

        assert!((settled.scale.x - 1.0).abs() < 1e-9);
        assert!((settled.scale.y - 1.0).abs() < 1e-9);
        assert_eq!(settled.offset, Point::from((0, 0)));
    }

    #[test]
    fn width_and_height_scale_independently() {
        let from = rect(0, 0, 400, 600);
        let to = rect(0, 0, 1200, 600);
        let duration = Duration::from_millis(320);
        let mid = GeometryProgress::at(duration.mul_f64(0.5), duration, from, to, to.size);

        assert!(mid.scale.x < 1.0, "still widening");
        assert!((mid.scale.y - 1.0).abs() < 1e-9, "height never changed, so it must not be scaled: {}", mid.scale.y);
    }

    #[test]
    fn it_never_reverses_and_stays_within_the_two_rects() {
        let (from, to) = maximize();
        let duration = Duration::from_millis(320);
        let mut previous = 0.0;
        for step in 0..=100 {
            let elapsed = duration.mul_f64(f64::from(step) / 100.0);
            let progress = GeometryProgress::at(elapsed, duration, from, to, to.size);
            assert!(progress.scale.x >= previous - 1e-9, "scale went backwards at step {step}");
            assert!(
                (800.0 / 1920.0 - 1e-9..=1.0 + 1e-9).contains(&progress.scale.x),
                "scale left its range at step {step}: {}",
                progress.scale.x
            );
            previous = progress.scale.x;
        }
    }

    #[test]
    fn it_rides_the_same_front_loaded_curve_as_the_ghost_it_dissolves_with() {
        let (from, to) = maximize();
        let duration = Duration::from_millis(320);
        let quarter = GeometryProgress::at(duration.mul_f64(0.25), duration, from, to, to.size);

        let start_scale = 800.0 / 1920.0;
        let travelled = (quarter.scale.x - start_scale) / (1.0 - start_scale);
        let expected = glide_theme::motion::E_OUT.ease_f64(0.25);
        assert!((travelled - expected).abs() < 0.01, "expected {expected} of the travel done, got {travelled}");
    }

    #[test]
    fn a_titled_window_is_measured_by_its_frame_so_it_is_never_stretched_by_the_titlebar() {
        let (from, to) = maximize();
        let duration = Duration::from_millis(320);
        let content = Size::from((to.size.w, to.size.h - 32));
        let settled = GeometryProgress::at(duration, duration, from, to, frame_of(content, 32));
        assert!((settled.scale.y - 1.0).abs() < 1e-9, "settled at {}", settled.scale.y);

        let slide = rect(-1920, 0, 1920, 1080);
        let mid = GeometryProgress::at(duration / 2, duration, slide, to, frame_of(content, 32));
        assert!((mid.scale.x - 1.0).abs() < 1e-9 && (mid.scale.y - 1.0).abs() < 1e-9, "a slide only travels");
    }

    #[test]
    fn a_shrink_is_handled_as_readily_as_a_grow() {
        let (to, from) = maximize();
        let duration = Duration::from_millis(320);
        let start = GeometryProgress::at(Duration::ZERO, duration, from, to, to.size);

        assert!(start.scale.x > 1.0, "un-maximizing starts larger than the target: {}", start.scale.x);
        let settled = GeometryProgress::at(duration, duration, from, to, to.size);
        assert!((settled.scale.x - 1.0).abs() < 1e-9);
    }
}

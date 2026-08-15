use std::{
    cell::RefCell,
    ops::Deref,
    time::{Duration, Instant},
};

use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer,
        element::{
            AsRenderElements, Element,
            memory::MemoryRenderBufferRenderElement,
            surface::WaylandSurfaceRenderElement,
            utils::{Relocate, RelocateRenderElement, RescaleRenderElement},
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

    pub(crate) fn begin_open_animation(
        &self,
        origin: Option<Rectangle<i32, Logical>>,
        frame: Rectangle<i32, Logical>,
    ) {
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

        self.0
            .user_data()
            .insert_if_missing(|| RefCell::new(Option::<OpenAnimation>::None));
        if let Some(cell) = self.0.user_data().get::<RefCell<Option<OpenAnimation>>>() {
            if let Ok(mut slot) = cell.try_borrow_mut() {
                *slot = Some(OpenAnimation {
                    started: Instant::now(),
                    start_scale,
                    start_offset,
                });
            }
        }
    }

    pub(crate) fn open_animation(&self) -> Option<OpenProgress> {
        let cell = self.0.user_data().get::<RefCell<Option<OpenAnimation>>>()?;
        let animation = cell.try_borrow().ok()?;
        let animation = animation.as_ref()?;
        let duration = open_scale_duration();
        let elapsed = animation.started.elapsed();
        if duration.is_zero() || elapsed >= duration {
            return None;
        }
        Some(OpenProgress::at(
            elapsed,
            animation.start_scale,
            animation.start_offset,
        ))
    }

    pub(crate) fn is_open_animating(&self) -> bool {
        self.open_animation().is_some()
    }

    pub(crate) fn begin_geometry_animation(
        &self,
        from: Rectangle<i32, Logical>,
        to: Rectangle<i32, Logical>,
    ) {
        if geometry_duration().is_zero()
            || from == to
            || to.size.w <= 0
            || to.size.h <= 0
            || from.size.w <= 0
            || from.size.h <= 0
        {
            return;
        }

        self.0
            .user_data()
            .insert_if_missing(|| RefCell::new(Option::<GeometryAnimation>::None));
        if let Some(cell) = self
            .0
            .user_data()
            .get::<RefCell<Option<GeometryAnimation>>>()
        {
            if let Ok(mut slot) = cell.try_borrow_mut() {
                *slot = Some(GeometryAnimation {
                    started: Instant::now(),
                    from,
                    to,
                });
            }
        }
    }

    pub(crate) fn geometry_animation(&self) -> Option<GeometryProgress> {
        let cell = self
            .0
            .user_data()
            .get::<RefCell<Option<GeometryAnimation>>>()?;
        let animation = cell.try_borrow().ok()?;
        let animation = animation.as_ref()?;
        let duration = geometry_duration();
        let elapsed = animation.started.elapsed();
        if duration.is_zero() || elapsed >= duration {
            return None;
        }
        let live_size = self.geometry().size;
        Some(GeometryProgress::at(
            elapsed,
            duration,
            animation.from,
            animation.to,
            live_size,
        ))
    }

    pub(crate) fn is_geometry_animating(&self) -> bool {
        self.geometry_animation().is_some()
    }

    #[allow(clippy::cast_possible_truncation, clippy::unused_self)]
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
    from: Rectangle<i32, Logical>,
    to: Rectangle<i32, Logical>,
}

fn geometry_duration() -> Duration {
    glide_theme::motion::t3()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GeometryProgress {
    pub(crate) scale: Scale<f64>,
    pub(crate) offset: Point<i32, Logical>,
}

impl GeometryProgress {
    fn at(
        elapsed: Duration,
        duration: Duration,
        from: Rectangle<i32, Logical>,
        to: Rectangle<i32, Logical>,
        live_size: Size<i32, Logical>,
    ) -> Self {
        let fraction = (elapsed.as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0);
        let t = glide_theme::motion::E_OUT.ease_f64(fraction);

        #[allow(clippy::cast_possible_truncation)]
        let mix =
            |a: i32, b: i32| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as i32;

        let current_w = mix(from.size.w, to.size.w).max(1);
        let current_h = mix(from.size.h, to.size.h).max(1);

        Self {
            scale: Scale::from((
                f64::from(current_w) / f64::from(live_size.w.max(1)),
                f64::from(current_h) / f64::from(live_size.h.max(1)),
            )),
            offset: Point::from((
                mix(from.loc.x, to.loc.x) - to.loc.x,
                mix(from.loc.y, to.loc.y) - to.loc.y,
            )),
        }
    }
}

fn open_scale_duration() -> Duration {
    glide_theme::motion::t4()
}

fn open_alpha_duration() -> Duration {
    glide_theme::motion::t3()
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
            if total.is_zero() {
                1.0
            } else {
                (elapsed.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
            }
        };

        let scale_t = glide_theme::motion::E_OUT.ease_f64(fraction(open_scale_duration()));
        let alpha_t = glide_theme::motion::E_IO.ease_f64(fraction(open_alpha_duration()));

        #[allow(clippy::cast_possible_truncation)]
        let offset = Point::from((
            (f64::from(start_offset.x) * (1.0 - scale_t)).round() as i32,
            (f64::from(start_offset.y) * (1.0 - scale_t)).round() as i32,
        ));

        #[allow(clippy::cast_possible_truncation)]
        Self {
            scale: start_scale + (1.0 - start_scale) * scale_t,
            offset,
            alpha: alpha_t as f32,
        }
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
    pub GlideWindowLayer<R> where R: ImportAll + ImportMem + ClipCapableRenderer;
    Content = WaylandSurfaceRenderElement<R>,
    ClippedContent = ClippedSurfaceElement<R>,
    Decoration = MemoryRenderBufferRenderElement<R>,
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
    fn push_focus_fade<R>(
        &self,
        renderer: &mut R,
        header_location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
        layers: &mut Vec<GlideWindowLayer<R>>,
    ) where
        R: Renderer + ImportAll + ImportMem + ClipCapableRenderer,
        R::TextureId: Send + Clone + 'static,
        R::Error: std::fmt::Debug,
    {
        let Some((fade, fade_alpha)) = decoration::shadow_and_border_fade(self) else {
            return;
        };
        let physical_offset = fade.offset.to_physical_precise_round(scale);
        let pos = (header_location + physical_offset).to_f64();
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            pos,
            &fade.buf,
            Some(alpha * fade_alpha),
            None,
            None,
            smithay::backend::renderer::element::Kind::Unspecified,
        ) {
            layers.push(GlideWindowLayer::Decoration(element));
        }
    }

    fn content_layers<R>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        header_location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Vec<GlideWindowLayer<R>>
    where
        R: Renderer + ImportAll + ImportMem + ClipCapableRenderer,
        R::TextureId: Send + Clone + 'static,
        R::Error: std::fmt::Debug,
    {
        let clip_frame = self
            .content_clip_frame(header_location, scale)
            .filter(|_| renderer.prepare_rounded_clip());

        let content_elements: Vec<WaylandSurfaceRenderElement<R>> =
            AsRenderElements::render_elements(&self.0, renderer, location, scale, alpha);

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
                    Some(clip) => {
                        GlideWindowLayer::ClippedContent(ClippedSurfaceElement::new(element, clip))
                    }
                    None => GlideWindowLayer::Content(element),
                }
            })
            .collect()
    }

    fn chrome_layers<R>(
        &self,
        renderer: &mut R,
        painter: &crate::paint::Painter,
        header_location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Vec<GlideWindowLayer<R>>
    where
        R: Renderer + ImportAll + ImportMem + ClipCapableRenderer,
        R::TextureId: Send + Clone + 'static,
        R::Error: std::fmt::Debug,
    {
        decoration::frame_buffers(self, painter)
            .into_iter()
            .filter(|deco_buf| deco_buf.alpha > 0.0)
            .filter_map(|deco_buf| {
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

                Some(match deco_buf.scale {
                    Some(factor) => {
                        GlideWindowLayer::ScaledDecoration(RescaleRenderElement::from_element(
                            element,
                            header_location + physical_offset,
                            factor,
                        ))
                    }
                    None => GlideWindowLayer::Decoration(element),
                })
            })
            .collect()
    }

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

        let alpha = alpha * self.open_animation().map_or(1.0, |progress| progress.alpha);

        let header_height = self.header_height();
        let header_location =
            location - Point::from((0, header_height)).to_physical_precise_round(scale);

        let mut layers: Vec<GlideWindowLayer<R>> =
            self.content_layers(renderer, location, header_location, scale, alpha);

        layers.extend(self.chrome_layers(renderer, &painter, header_location, scale, alpha));

        self.push_focus_fade(renderer, header_location, scale, alpha, &mut layers);

        if include_box_deco {
            let int_scale = self.decoration_buffer_scale(scale);
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
                    layers.push(GlideWindowLayer::Decoration(element));
                }
            }
        }

        let open = self.open_animation();
        let geometry = self.geometry_animation();

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

        let elements: Vec<C> = if transformed {
            let travel = travel.to_physical_precise_round(scale);
            layers
                .into_iter()
                .map(|layer| {
                    let scaled = RescaleRenderElement::from_element(layer, header_location, factor);
                    C::from(GlideWindowRenderElement::Scaled(
                        RelocateRenderElement::from_element(scaled, travel, Relocate::Relative),
                    ))
                })
                .collect()
        } else {
            layers
                .into_iter()
                .map(|layer| C::from(GlideWindowRenderElement::Plain(layer)))
                .collect()
        };

        elements
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

impl GlideWindow {
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
        let settled =
            OpenProgress::at(open_scale_duration(), OPEN_START_SCALE, Point::from((0, 0)));
        assert!((settled.scale - 1.0).abs() < 1e-9);
        assert!((settled.alpha - 1.0).abs() < 1e-6);
    }

    #[test]
    fn opacity_finishes_before_the_scale_does() {
        assert!(open_alpha_duration() < open_scale_duration());

        let at_alpha_end =
            OpenProgress::at(open_alpha_duration(), OPEN_START_SCALE, Point::from((0, 0)));
        assert!(
            (at_alpha_end.alpha - 1.0).abs() < 1e-6,
            "alpha should be done at --t-3"
        );
        assert!(
            at_alpha_end.scale < 1.0,
            "the scale should still have ground to cover"
        );
    }

    #[test]
    fn the_window_only_ever_grows_towards_its_real_size() {
        let mut previous =
            OpenProgress::at(Duration::ZERO, OPEN_START_SCALE, Point::from((0, 0))).scale;
        for step in 1..=100 {
            let elapsed = open_scale_duration().mul_f64(f64::from(step) / 100.0);
            let scale = OpenProgress::at(elapsed, OPEN_START_SCALE, Point::from((0, 0))).scale;
            assert!(
                scale >= previous - 1e-9,
                "scale went backwards at step {step}: {scale} < {previous}"
            );
            assert!(
                (OPEN_START_SCALE..=1.0).contains(&scale),
                "scale left its range at step {step}: {scale}"
            );
            previous = scale;
        }
    }

    #[test]
    fn a_launch_from_a_tile_starts_at_the_tile() {
        let frame: Rectangle<i32, Logical> =
            Rectangle::new(Point::from((400, 300)), Size::from((800, 600)));
        let tile: Rectangle<i32, Logical> =
            Rectangle::new(Point::from((900, 1000)), Size::from((42, 42)));

        let start_scale = f64::from(tile.size.w) / f64::from(frame.size.w);
        let start_offset = Point::from((tile.loc.x - frame.loc.x, tile.loc.y - frame.loc.y));

        let begin = OpenProgress::at(Duration::ZERO, start_scale, start_offset);
        assert!((begin.scale - start_scale).abs() < 1e-9);
        assert_eq!(begin.offset, start_offset, "it must begin at the tile");

        let settled = OpenProgress::at(open_scale_duration(), start_scale, start_offset);
        assert!((settled.scale - 1.0).abs() < 1e-9);
        assert_eq!(
            settled.offset,
            Point::from((0, 0)),
            "and land exactly where the layout put it"
        );
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
        let quarter = OpenProgress::at(
            open_scale_duration().mul_f64(0.25),
            OPEN_START_SCALE,
            Point::from((0, 0)),
        );
        let travelled = (quarter.scale - OPEN_START_SCALE) / (1.0 - OPEN_START_SCALE);
        assert!(
            travelled > 0.5,
            "only {travelled} of the grow done a quarter of the way in"
        );
    }
}

#[cfg(test)]
mod geometry_animation_tests {
    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((x, y)), Size::from((w, h)))
    }

    fn maximize() -> (Rectangle<i32, Logical>, Rectangle<i32, Logical>) {
        (rect(400, 300, 800, 600), rect(0, 0, 1920, 1080))
    }

    #[test]
    fn it_begins_exactly_on_the_old_rect() {
        let (from, to) = maximize();
        let start = GeometryProgress::at(Duration::ZERO, Duration::from_millis(320), from, to);

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
        let settled = GeometryProgress::at(duration, duration, from, to);

        assert!((settled.scale.x - 1.0).abs() < 1e-9);
        assert!((settled.scale.y - 1.0).abs() < 1e-9);
        assert_eq!(settled.offset, Point::from((0, 0)));
    }

    #[test]
    fn width_and_height_scale_independently() {
        let from = rect(0, 0, 400, 600);
        let to = rect(0, 0, 1200, 600);
        let duration = Duration::from_millis(320);
        let mid = GeometryProgress::at(duration.mul_f64(0.5), duration, from, to);

        assert!(mid.scale.x < 1.0, "still widening");
        assert!(
            (mid.scale.y - 1.0).abs() < 1e-9,
            "height never changed, so it must not be scaled: {}",
            mid.scale.y
        );
    }

    #[test]
    fn it_never_reverses_and_stays_within_the_two_rects() {
        let (from, to) = maximize();
        let duration = Duration::from_millis(320);
        let mut previous = 0.0;
        for step in 0..=100 {
            let elapsed = duration.mul_f64(f64::from(step) / 100.0);
            let progress = GeometryProgress::at(elapsed, duration, from, to);
            assert!(
                progress.scale.x >= previous - 1e-9,
                "scale went backwards at step {step}"
            );
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
        let quarter = GeometryProgress::at(duration.mul_f64(0.25), duration, from, to);

        let start_scale = 800.0 / 1920.0;
        let travelled = (quarter.scale.x - start_scale) / (1.0 - start_scale);
        let expected = glide_theme::motion::E_OUT.ease_f64(0.25);
        assert!(
            (travelled - expected).abs() < 0.01,
            "expected {expected} of the travel done, got {travelled}"
        );
    }

    #[test]
    fn a_shrink_is_handled_as_readily_as_a_grow() {
        let (to, from) = maximize();
        let duration = Duration::from_millis(320);
        let start = GeometryProgress::at(Duration::ZERO, duration, from, to);

        assert!(
            start.scale.x > 1.0,
            "un-maximizing starts larger than the target: {}",
            start.scale.x
        );
        let settled = GeometryProgress::at(duration, duration, from, to);
        assert!((settled.scale.x - 1.0).abs() < 1e-9);
    }
}

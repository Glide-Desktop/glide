use std::{
    ops::Deref,
    time::{Duration, Instant},
};

use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer,
        element::{
            AsRenderElements, Element, memory::MemoryRenderBufferRenderElement,
            surface::WaylandSurfaceRenderElement, utils::RescaleRenderElement,
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

    /// Starts the appear animation. Called once, when the window is first
    /// tracked; re-tracking an existing window leaves the original start
    /// time alone so a re-map cannot restart a half-finished grow.
    pub(crate) fn begin_open_animation(&self) {
        self.0.user_data().insert_if_missing(|| OpenAnimation {
            started: Instant::now(),
        });
    }

    /// Where the appear animation currently stands, or `None` once it has
    /// settled (or never started).
    pub(crate) fn open_animation(&self) -> Option<OpenProgress> {
        let animation = self.0.user_data().get::<OpenAnimation>()?;
        let elapsed = animation.started.elapsed();
        if elapsed >= OPEN_SCALE_DURATION {
            return None;
        }
        Some(OpenProgress::at(elapsed))
    }

    /// Whether this window still owes frames to its appear animation.
    pub(crate) fn is_open_animating(&self) -> bool {
        self.open_animation().is_some()
    }
}

/// Marker stored in the window's user data the first time it is tracked.
struct OpenAnimation {
    started: Instant,
}

/// `.dwin` scales up over `--t-4` …
const OPEN_SCALE_DURATION: Duration = glide_theme::motion::T4;
/// … while fading in over the shorter `--t-3`.
const OPEN_ALPHA_DURATION: Duration = glide_theme::motion::T3;
/// The design's `appear()` starts a window at `transform: scale(.86)`.
const OPEN_START_SCALE: f64 = 0.86;

/// A sample of the appear animation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpenProgress {
    /// Uniform scale about the window's top-left corner.
    pub(crate) scale: f64,
    /// Opacity multiplier for every layer of the window.
    pub(crate) alpha: f32,
}

impl OpenProgress {
    fn at(elapsed: Duration) -> Self {
        let fraction = |total: Duration| {
            if total.is_zero() {
                1.0
            } else {
                (elapsed.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
            }
        };

        // Transform rides `--e-out`, opacity the symmetric `--e-io`, per
        // `.dwin.anim-tf`.
        let scale_t = glide_theme::motion::E_OUT.ease_f64(fraction(OPEN_SCALE_DURATION));
        let alpha_t = glide_theme::motion::E_IO.ease_f64(fraction(OPEN_ALPHA_DURATION));

        #[allow(clippy::cast_possible_truncation)]
        Self {
            scale: OPEN_START_SCALE + (1.0 - OPEN_START_SCALE) * scale_t,
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
    /// One drawn layer of a window: its client content (optionally corner
    /// clipped) or a piece of Glide's own chrome.
    pub GlideWindowLayer<R> where R: ImportAll + ImportMem + ClipCapableRenderer;
    Content = WaylandSurfaceRenderElement<R>,
    ClippedContent = ClippedSurfaceElement<R>,
    Decoration = MemoryRenderBufferRenderElement<R>,
}

render_elements! {
    /// A window layer, optionally scaled for the open animation.
    ///
    /// The scale has to wrap the finished layers rather than being folded
    /// into their geometry: chrome and content must grow as one piece, and
    /// `RescaleRenderElement` is what keeps the titlebar, border and
    /// surface locked together while the window swells into place.
    pub GlideWindowRenderElement<R> where R: ImportAll + ImportMem + ClipCapableRenderer;
    Plain = GlideWindowLayer<R>,
    Scaled = RescaleRenderElement<GlideWindowLayer<R>>,
}

impl<R: Renderer> std::fmt::Debug for GlideWindowLayer<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Content(e) => f.debug_tuple("Content").field(e).finish(),
            Self::ClippedContent(e) => f.debug_tuple("ClippedContent").field(e).finish(),
            Self::Decoration(e) => f.debug_tuple("Decoration").field(e).finish(),
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
    ) -> Vec<C>
    where
        R: Renderer + ImportAll + ImportMem + ClipCapableRenderer,
        R::TextureId: Send + Clone + 'static,
        R::Error: std::fmt::Debug,
        C: From<GlideWindowRenderElement<R>>,
    {
        let painter = crate::paint::current();

        // The design fades a window in over `--t-3` while it scales over
        // `--t-4`, so opacity finishes first and the last stretch of the
        // grow happens fully opaque.
        let alpha = alpha * self.open_animation().map_or(1.0, |progress| progress.alpha);

        let header_height = self.header_height();
        let header_location =
            location - Point::from((0, header_height)).to_physical_precise_round(scale);

        let mut layers: Vec<GlideWindowLayer<R>> = Vec::new();

        let deco_layers: Vec<GlideWindowLayer<R>> = decoration::frame_buffers(self, &painter)
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
            .map(GlideWindowLayer::Decoration)
            .collect();

        layers.extend(deco_layers);

        {
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
                    layers.push(GlideWindowLayer::Decoration(element));
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
                Some(clip) => {
                    GlideWindowLayer::ClippedContent(ClippedSurfaceElement::new(element, clip))
                }
                None => GlideWindowLayer::Content(element),
            };
            layers.push(element);
        }

        // Wrap the finished layers in the open animation, if one is running.
        let open = self.open_animation();
        let elements: Vec<C> = match open {
            Some(progress) if progress.scale < 1.0 => layers
                .into_iter()
                .map(|layer| {
                    C::from(GlideWindowRenderElement::Scaled(
                        RescaleRenderElement::from_element(layer, header_location, progress.scale),
                    ))
                })
                .collect(),
            _ => layers
                .into_iter()
                .map(|layer| C::from(GlideWindowRenderElement::Plain(layer)))
                .collect(),
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
        self.render_elements_impl(renderer, location, scale, alpha)
    }
}

#[cfg(test)]
mod open_animation_tests {
    use super::*;

    #[test]
    fn it_starts_at_the_designs_scale_and_fully_transparent() {
        let start = OpenProgress::at(Duration::ZERO);
        assert!((start.scale - OPEN_START_SCALE).abs() < 1e-9);
        assert!(start.alpha.abs() < 1e-6);
    }

    #[test]
    fn it_lands_exactly_on_one_when_the_longer_track_finishes() {
        let settled = OpenProgress::at(OPEN_SCALE_DURATION);
        assert!((settled.scale - 1.0).abs() < 1e-9);
        assert!((settled.alpha - 1.0).abs() < 1e-6);
    }

    /// Opacity runs on the shorter `--t-3`, so a window is fully opaque
    /// while the last stretch of the grow is still playing.
    #[test]
    fn opacity_finishes_before_the_scale_does() {
        assert!(OPEN_ALPHA_DURATION < OPEN_SCALE_DURATION);

        let at_alpha_end = OpenProgress::at(OPEN_ALPHA_DURATION);
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
        let mut previous = OpenProgress::at(Duration::ZERO).scale;
        for step in 1..=100 {
            let elapsed = OPEN_SCALE_DURATION.mul_f64(f64::from(step) / 100.0);
            let scale = OpenProgress::at(elapsed).scale;
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

    /// `--e-out` is front-loaded, so most of the grow is over early and
    /// the tail is a gentle settle rather than a lurch at the end.
    #[test]
    fn the_grow_is_front_loaded_like_the_curve_it_rides() {
        let quarter = OpenProgress::at(OPEN_SCALE_DURATION.mul_f64(0.25));
        let travelled = (quarter.scale - OPEN_START_SCALE) / (1.0 - OPEN_START_SCALE);
        assert!(
            travelled > 0.5,
            "only {travelled} of the grow done a quarter of the way in"
        );
    }
}

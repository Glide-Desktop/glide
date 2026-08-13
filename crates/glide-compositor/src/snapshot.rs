//! Animations that play out on a captured picture of a window.
//!
//! Two things need this. A **closing** window has to keep being drawn
//! after its `wl_surface` is gone, and by then there is nothing left to
//! draw from: Glide, like every smithay compositor, rebuilds its render
//! elements each frame out of live surface state rather than keeping a
//! retained scene graph the way Mutter's Clutter actors or `KWin`'s
//! ref-counted `deleted` windows do. An **un-maximizing** window has the
//! opposite problem — it is still very much alive, but the client resizes
//! asynchronously, so there is no moment at which the compositor can
//! interpolate its real geometry smoothly.
//!
//! Both are solved the same way: capture the window as it looks right
//! now, then animate that still picture from where it was to where it is
//! going while the live window takes over underneath. This is the
//! approach niri and Hyprland take, for the same reasons.

use std::time::{Duration, Instant};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            Bind, Offscreen, Renderer,
            damage::OutputDamageTracker,
            element::{Id, Kind, texture::TextureRenderElement},
            gles::{GlesRenderer, GlesTexture},
        },
    },
    utils::{Logical, Point, Rectangle, Scale, Size, Transform},
};

use glide_theme::motion::CubicBezier;

use crate::window::GlideWindow;

/// `.dwin.anim-fast` — the design closes a window over `--t-2`.
const CLOSE_DURATION: Duration = glide_theme::motion::T2;

/// `closeWin()` animates to `transform: scale(.94)`.
const CLOSE_END_SCALE: f64 = 0.94;

/// `.dwin.anim-geo` — geometry changes run over `--t-3`.
const MORPH_DURATION: Duration = glide_theme::motion::T3;

/// Upper bound on simultaneously animating snapshots.
///
/// Each pins an offscreen texture for the length of its animation, so a
/// burst could otherwise spike video memory. Beyond this the oldest is
/// dropped, which just means it finishes instantly — the behaviour
/// everything had before any of this existed.
const MAX_CONCURRENT: usize = 8;

/// The element type a snapshot contributes.
pub type SnapshotRenderElement = TextureRenderElement<GlesTexture>;

/// One captured window being animated from one rect to another.
#[derive(Debug)]
struct Snapshot {
    id: Id,
    texture: GlesTexture,
    /// What the texture covers, in logical px: the window's frame plus
    /// `margin` of padding on every side for its shadow.
    natural: Size<i32, Logical>,
    /// Padding baked into `natural` around the frame.
    margin: i32,
    /// Scale the texture was captured at.
    buffer_scale: i32,

    /// Frame rect at the start and end of the animation.
    from: Rectangle<i32, Logical>,
    to: Rectangle<i32, Logical>,
    from_alpha: f32,
    to_alpha: f32,

    duration: Duration,
    curve: CubicBezier,
    started: Instant,
}

impl Snapshot {
    fn eased(&self) -> f64 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = self.started.elapsed().as_secs_f64();
        let t = (elapsed / self.duration.as_secs_f64()).clamp(0.0, 1.0);
        self.curve.ease_f64(t)
    }

    fn is_finished(&self) -> bool {
        self.started.elapsed() >= self.duration
    }

    /// Where the frame sits, and how opaque it is, right now.
    fn sample(&self) -> (Rectangle<i32, Logical>, f32) {
        let t = self.eased();
        (
            lerp_rect(self.from, self.to, t),
            lerp_alpha(self.from_alpha, self.to_alpha, t),
        )
    }
}

/// Interpolates a frame rect. Split out so the geometry can be tested
/// without a GL context to build a texture from.
fn lerp_rect(
    from: Rectangle<i32, Logical>,
    to: Rectangle<i32, Logical>,
    t: f64,
) -> Rectangle<i32, Logical> {
    #[allow(clippy::cast_possible_truncation)]
    let mix = |a: i32, b: i32| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as i32;

    Rectangle::new(
        Point::from((mix(from.loc.x, to.loc.x), mix(from.loc.y, to.loc.y))),
        Size::from((
            mix(from.size.w, to.size.w).max(1),
            mix(from.size.h, to.size.h).max(1),
        )),
    )
}

fn lerp_alpha(from: f32, to: f32, t: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let alpha = (f64::from(from) + (f64::from(to) - f64::from(from)) * t) as f32;
    alpha.clamp(0.0, 1.0)
}

/// Where a closing window's frame ends up: 94% of its size, anchored at
/// the top-left corner because `.dwin` sets `transform-origin: top left`.
fn close_target(from: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    #[allow(clippy::cast_possible_truncation)]
    let shrunk = Size::from((
        (f64::from(from.size.w) * CLOSE_END_SCALE).round() as i32,
        (f64::from(from.size.h) * CLOSE_END_SCALE).round() as i32,
    ));
    Rectangle::new(from.loc, shrunk)
}

/// Every snapshot currently animating.
#[derive(Debug, Default)]
pub struct Snapshots {
    entries: Vec<Snapshot>,
}

impl Snapshots {
    /// Captures `window` and shrinks it away, per the design's `closeWin`.
    ///
    /// `target_frame` is where the window's frame sits on screen right
    /// now — normally its real geometry, but the preview slot when it is
    /// closed from the overview.
    ///
    /// Must be called while the window's surfaces are still alive. Returns
    /// whether a capture was taken; on failure the caller should drop the
    /// window as it always did.
    pub fn push_close(
        &mut self,
        renderer: &mut GlesRenderer,
        window: &GlideWindow,
        target_frame: Rectangle<i32, Logical>,
        scale: Scale<f64>,
        output_scale: i32,
    ) -> bool {
        self.push(
            renderer,
            window,
            target_frame,
            close_target(target_frame),
            1.0,
            0.0,
            CLOSE_DURATION,
            glide_theme::motion::E_IO,
            scale,
            output_scale,
        )
    }

    /// Captures `window` as it looks now and morphs that picture into
    /// `to`, fading out as the live window settles into place underneath.
    ///
    /// Used when a window leaves maximized or fullscreen: the client
    /// resizes on its own schedule, so its real geometry cannot be
    /// interpolated — the snapshot covers the gap.
    pub fn push_morph(
        &mut self,
        renderer: &mut GlesRenderer,
        window: &GlideWindow,
        from: Rectangle<i32, Logical>,
        to: Rectangle<i32, Logical>,
        scale: Scale<f64>,
        output_scale: i32,
    ) -> bool {
        self.push(
            renderer,
            window,
            from,
            to,
            1.0,
            0.0,
            MORPH_DURATION,
            glide_theme::motion::E_OUT,
            scale,
            output_scale,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        renderer: &mut GlesRenderer,
        window: &GlideWindow,
        from: Rectangle<i32, Logical>,
        to: Rectangle<i32, Logical>,
        from_alpha: f32,
        to_alpha: f32,
        duration: Duration,
        curve: CubicBezier,
        scale: Scale<f64>,
        output_scale: i32,
    ) -> bool {
        let Some(capture) = capture_window(renderer, window, scale) else {
            return false;
        };

        if self.entries.len() >= MAX_CONCURRENT {
            self.entries.remove(0);
        }

        self.entries.push(Snapshot {
            id: Id::new(),
            texture: capture.texture,
            natural: capture.size,
            margin: capture.margin,
            buffer_scale: output_scale,
            from,
            to,
            from_alpha,
            to_alpha,
            duration,
            curve,
            started: Instant::now(),
        });
        true
    }

    /// Drops snapshots whose animation has run its course. Returns whether
    /// any are still going.
    pub fn tick(&mut self) -> bool {
        self.entries.retain(|entry| !entry.is_finished());
        !self.entries.is_empty()
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        self.entries.iter().any(|entry| !entry.is_finished())
    }

    /// Forgets every capture, so no texture outlives its GL context.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Elements for the animating snapshots, front-to-back.
    #[must_use]
    pub fn render_elements(
        &self,
        renderer: &GlesRenderer,
        scale: Scale<f64>,
    ) -> Vec<SnapshotRenderElement> {
        self.entries
            .iter()
            .map(|entry| {
                let (frame, alpha) = entry.sample();

                // The capture is `frame + margin` on every side, so the
                // padding has to shrink by the same factor as the frame.
                let captured_frame_width = entry.natural.w - entry.margin * 2;
                let factor = if captured_frame_width > 0 {
                    f64::from(frame.size.w) / f64::from(captured_frame_width)
                } else {
                    1.0
                };
                #[allow(clippy::cast_possible_truncation)]
                let scaled_margin = (f64::from(entry.margin) * factor).round() as i32;
                #[allow(clippy::cast_possible_truncation)]
                let display = Size::from((
                    (f64::from(entry.natural.w) * factor).round() as i32,
                    (f64::from(entry.natural.h) * factor).round() as i32,
                ));
                let location =
                    Point::from((frame.loc.x - scaled_margin, frame.loc.y - scaled_margin))
                        .to_f64()
                        .to_physical(scale);

                // `src` must be spelled out. Left as `None` it defaults to
                // the *destination* size, which makes the element sample a
                // larger region instead of scaling the texture — the same
                // smithay footgun that made the wallpaper slide instead of
                // zooming.
                let src = Rectangle::from_size(Size::from((
                    f64::from(entry.natural.w),
                    f64::from(entry.natural.h),
                )));

                TextureRenderElement::from_static_texture(
                    entry.id.clone(),
                    renderer.context_id(),
                    location,
                    entry.texture.clone(),
                    entry.buffer_scale,
                    Transform::Normal,
                    Some(alpha),
                    Some(src),
                    Some(display),
                    // Translucent throughout, so it never occludes.
                    None,
                    Kind::Unspecified,
                )
            })
            .collect()
    }
}

/// A captured window, plus the margin left around its frame so the shadow
/// made it into the texture.
struct Capture {
    texture: GlesTexture,
    size: Size<i32, Logical>,
    margin: i32,
}

/// Renders `window` — chrome, shadow and content — into a fresh texture.
fn capture_window(
    renderer: &mut GlesRenderer,
    window: &GlideWindow,
    scale: Scale<f64>,
) -> Option<Capture> {
    let content = window.geometry().size;
    let header = window.header_height();
    let margin = window.shadow_margin();

    let size = Size::from((content.w + margin * 2, content.h + header + margin * 2));
    if size.w <= 0 || size.h <= 0 {
        return None;
    }
    let physical = size.to_physical_precise_ceil(scale);
    if physical.w <= 0 || physical.h <= 0 {
        return None;
    }

    let mut texture: GlesTexture = renderer
        .create_buffer(Fourcc::Abgr8888, (physical.w, physical.h).into())
        .ok()?;
    let mut target = renderer.bind(&mut texture).ok()?;

    // `render_elements` is handed the *content* origin; it derives the
    // frame origin from that by stepping back over the titlebar.
    let content_origin = Point::from((margin, margin + header)).to_physical_precise_round(scale);
    let elements: Vec<crate::window::GlideWindowRenderElement<GlesRenderer>> =
        smithay::backend::renderer::element::AsRenderElements::render_elements(
            window,
            renderer,
            content_origin,
            scale,
            1.0,
        );

    let mut tracker = OutputDamageTracker::new((physical.w, physical.h), scale, Transform::Normal);
    tracker
        .render_output(renderer, &mut target, 0, &elements, [0.0, 0.0, 0.0, 0.0])
        .ok()?;
    drop(target);

    Some(Capture {
        texture,
        size,
        margin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((x, y)), Size::from((w, h)))
    }

    #[test]
    fn a_fresh_collection_has_nothing_to_animate() {
        let mut snapshots = Snapshots::default();
        assert!(!snapshots.is_animating());
        assert!(!snapshots.tick(), "an empty collection is never running");
    }

    mod geometry {
        use super::*;

        #[test]
        fn the_ends_of_an_interpolation_are_exact() {
            let from = rect(0, 0, 1920, 1080);
            let to = rect(400, 300, 800, 600);
            assert_eq!(lerp_rect(from, to, 0.0), from);
            assert_eq!(lerp_rect(from, to, 1.0), to);
        }

        /// Leaving maximized moves *and* resizes, so both corners of the
        /// rect have to travel — not just its size.
        #[test]
        fn position_and_size_interpolate_together() {
            let from = rect(0, 0, 1920, 1080);
            let to = rect(400, 300, 800, 600);
            let mid = lerp_rect(from, to, 0.5);

            assert_eq!(mid.loc, Point::from((200, 150)));
            assert_eq!(mid.size, Size::from((1360, 840)));
        }

        #[test]
        fn a_rect_never_collapses_to_nothing() {
            let mid = lerp_rect(rect(0, 0, 1, 1), rect(0, 0, 0, 0), 1.0);
            assert!(mid.size.w >= 1 && mid.size.h >= 1);
        }

        #[test]
        fn alpha_runs_between_its_endpoints_and_stays_in_range() {
            assert!((lerp_alpha(1.0, 0.0, 0.0) - 1.0).abs() < 1e-6);
            assert!(lerp_alpha(1.0, 0.0, 1.0).abs() < 1e-6);
            assert!((lerp_alpha(1.0, 0.0, 0.5) - 0.5).abs() < 1e-6);
            assert!((0.0..=1.0).contains(&lerp_alpha(1.0, 0.0, 5.0)));
        }
    }

    mod close {
        use super::*;

        /// `closeWin()` shrinks to 94% about the top-left corner.
        #[test]
        fn it_shrinks_about_the_top_left_corner() {
            let from = rect(40, 60, 200, 100);
            let to = close_target(from);

            assert_eq!(to.loc, from.loc, "the corner must not move");
            assert_eq!(to.size, Size::from((188, 94)));
            assert!(to.size.w < from.size.w && to.size.h < from.size.h);
        }

        #[test]
        fn closing_is_quicker_than_a_geometry_morph() {
            assert!(CLOSE_DURATION < MORPH_DURATION);
        }
    }

    mod morph {
        use super::*;

        /// The morph rides `--e-out`, which is front-loaded: most of the
        /// travel is over early and the tail is a settle.
        #[test]
        fn the_morph_is_front_loaded() {
            let travelled = glide_theme::motion::E_OUT.ease_f64(0.25);
            assert!(
                travelled > 0.5,
                "only {travelled} of the way at a quarter of the duration"
            );
        }

        /// A window leaving maximized has to end up exactly where the
        /// layout put it, or the live window would visibly jump when the
        /// snapshot finishes.
        #[test]
        fn a_morph_lands_exactly_on_the_layouts_rect() {
            let to = rect(400, 300, 800, 600);
            assert_eq!(lerp_rect(rect(0, 0, 1920, 1080), to, 1.0), to);
        }
    }

    /// Where a restored window lands under the pointer. Mirrors the maths
    /// in `GlideState::restore_for_move`, which cannot be exercised
    /// directly without a full compositor.
    mod grip {
        use super::*;

        fn restored_x(from: Rectangle<i32, Logical>, pointer_x: f64, restored_w: i32) -> i32 {
            let grip = (pointer_x - f64::from(from.loc.x)) / f64::from(from.size.w);
            #[allow(clippy::cast_possible_truncation)]
            {
                (pointer_x - grip * f64::from(restored_w)).round() as i32
            }
        }

        #[test]
        fn grabbing_the_middle_centres_the_restored_window_on_the_pointer() {
            let maximized = rect(0, 0, 1920, 1080);
            let x = restored_x(maximized, 960.0, 800);
            assert_eq!(x, 960 - 400, "the window should straddle the pointer");
        }

        #[test]
        fn grabbing_near_an_edge_keeps_the_pointer_near_that_edge() {
            let maximized = rect(0, 0, 1920, 1080);

            // 10% in from the left: the restored window's left edge should
            // sit just left of the pointer, not leap across the screen.
            let left = restored_x(maximized, 192.0, 800);
            assert_eq!(left, 192 - 80);

            // 90% across: the pointer stays near the right edge.
            let right = restored_x(maximized, 1728.0, 800);
            assert_eq!(right, 1728 - 720);
        }

        /// The pointer must always end up somewhere over the restored
        /// window, whichever part of a maximized titlebar was grabbed.
        #[test]
        fn the_pointer_always_lands_on_the_restored_window() {
            let maximized = rect(0, 0, 1920, 1080);
            let restored_w = 800;
            for step in 0..=100 {
                let pointer_x = f64::from(step) / 100.0 * 1920.0;
                let x = restored_x(maximized, pointer_x, restored_w);
                #[allow(clippy::cast_possible_truncation)]
                let pointer = pointer_x.round() as i32;
                assert!(
                    (x..=x + restored_w).contains(&pointer),
                    "pointer {pointer} fell outside the restored window at {x}"
                );
            }
        }
    }
}

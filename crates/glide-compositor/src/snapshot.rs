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

use glide_theme::motion::{CubicBezier, E_IO, E_OUT};

use crate::window::GlideWindow;

fn close_duration() -> Duration {
    glide_theme::motion::t2()
}

const CLOSE_END_SCALE: f64 = 0.94;

fn morph_duration() -> Duration {
    glide_theme::motion::t3()
}

fn flight_duration() -> Duration {
    glide_theme::motion::t4()
}

pub fn workspace_duration() -> Duration {
    glide_theme::motion::t4()
}

const MAX_CONCURRENT: usize = 24;

pub type SnapshotRenderElement = TextureRenderElement<GlesTexture>;

#[derive(Debug, Clone, Copy)]
struct Track {
    from: Rectangle<i32, Logical>,
    to: Rectangle<i32, Logical>,
    to_alpha: f32,
    duration: Duration,
    curve: CubicBezier,
}

impl Track {
    const fn new(
        from: Rectangle<i32, Logical>, to: Rectangle<i32, Logical>, to_alpha: f32, duration: Duration,
        curve: CubicBezier,
    ) -> Self {
        Self { from, to, to_alpha, duration, curve }
    }
}

#[derive(Debug)]
struct Snapshot {
    id: Id,
    texture: GlesTexture,
    natural: Size<i32, Logical>,
    margin: i32,
    buffer_scale: i32,
    track: Track,
    started: Instant,
}

impl Snapshot {
    fn eased(&self) -> f64 {
        let duration = self.track.duration;
        if duration.is_zero() {
            return 1.0;
        }
        let elapsed = self.started.elapsed().as_secs_f64();
        let t = (elapsed / duration.as_secs_f64()).clamp(0.0, 1.0);
        self.track.curve.ease_f64(t)
    }

    fn is_finished(&self) -> bool {
        self.started.elapsed() >= self.track.duration
    }

    fn sample(&self) -> (Rectangle<i32, Logical>, f32) {
        let t = self.eased();
        let Track { from, to, to_alpha, .. } = self.track;
        (lerp_rect(from, to, t), lerp_alpha(1.0, to_alpha, t))
    }
}

fn lerp_rect(from: Rectangle<i32, Logical>, to: Rectangle<i32, Logical>, t: f64) -> Rectangle<i32, Logical> {
    let mix = |a: i32, b: i32| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as i32;

    Rectangle::new(
        Point::from((mix(from.loc.x, to.loc.x), mix(from.loc.y, to.loc.y))),
        Size::from((mix(from.size.w, to.size.w).max(1), mix(from.size.h, to.size.h).max(1))),
    )
}

fn lerp_alpha(from: f32, to: f32, t: f64) -> f32 {
    let alpha = (f64::from(from) + (f64::from(to) - f64::from(from)) * t) as f32;
    alpha.clamp(0.0, 1.0)
}

fn close_target(from: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    let shrunk = Size::from((
        (f64::from(from.size.w) * CLOSE_END_SCALE).round() as i32,
        (f64::from(from.size.h) * CLOSE_END_SCALE).round() as i32,
    ));
    let offset = Point::from(((from.size.w - shrunk.w) / 2, (from.size.h - shrunk.h) / 2));
    Rectangle::new(from.loc + offset, shrunk)
}

#[derive(Debug, Default)]
pub struct Snapshots {
    entries: Vec<Snapshot>,
}

impl Snapshots {
    pub fn push_close(
        &mut self, renderer: &mut GlesRenderer, window: &GlideWindow, target_frame: Rectangle<i32, Logical>,
        scale: Scale<f64>, output_scale: i32,
    ) -> bool {
        let to = close_target(target_frame);
        let track = Track::new(target_frame, to, 0.0, close_duration(), E_IO);
        self.push(renderer, window, track, scale, output_scale)
    }

    pub fn push_morph(
        &mut self, renderer: &mut GlesRenderer, window: &GlideWindow, from: Rectangle<i32, Logical>,
        to: Rectangle<i32, Logical>, scale: Scale<f64>, output_scale: i32,
    ) -> bool {
        let track = Track::new(from, to, 0.0, morph_duration(), E_OUT);
        self.push(renderer, window, track, scale, output_scale)
    }

    pub fn push_slide(
        &mut self, renderer: &mut GlesRenderer, window: &GlideWindow, from: Rectangle<i32, Logical>,
        to: Rectangle<i32, Logical>, scale: Scale<f64>, output_scale: i32,
    ) -> bool {
        let track = Track::new(from, to, 1.0, workspace_duration(), E_OUT);
        self.push(renderer, window, track, scale, output_scale)
    }

    pub fn push_flight(
        &mut self, renderer: &mut GlesRenderer, window: &GlideWindow, from: Rectangle<i32, Logical>,
        to: Rectangle<i32, Logical>, scale: Scale<f64>, output_scale: i32,
    ) -> bool {
        let track = Track::new(from, to, 0.0, flight_duration(), E_IO);
        self.push(renderer, window, track, scale, output_scale)
    }

    fn push(
        &mut self, renderer: &mut GlesRenderer, window: &GlideWindow, track: Track, scale: Scale<f64>,
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
            track,
            started: Instant::now(),
        });
        true
    }

    pub fn tick(&mut self) -> bool {
        self.entries.retain(|entry| !entry.is_finished());
        !self.entries.is_empty()
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        self.entries.iter().any(|entry| !entry.is_finished())
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    #[must_use]
    pub fn render_elements(
        &self, renderer: &GlesRenderer, scale: Scale<f64>, output_geometry: Rectangle<i32, Logical>,
    ) -> Vec<SnapshotRenderElement> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let (frame, alpha) = entry.sample();

                let factor = |shown: i32, natural: i32| {
                    let captured = natural - entry.margin * 2;
                    if captured > 0 { f64::from(shown) / f64::from(captured) } else { 1.0 }
                };
                let factor_x = factor(frame.size.w, entry.natural.w);
                let factor_y = factor(frame.size.h, entry.natural.h);
                let stretch = |v: i32, f: f64| (f64::from(v) * f).round() as i32;
                let display = Size::from((stretch(entry.natural.w, factor_x), stretch(entry.natural.h, factor_y)));
                let global = Point::from((
                    frame.loc.x - stretch(entry.margin, factor_x),
                    frame.loc.y - stretch(entry.margin, factor_y),
                ));
                if !output_geometry.overlaps(Rectangle::new(global, display)) {
                    return None;
                }
                let location = (global - output_geometry.loc).to_f64().to_physical(scale);

                let src = Rectangle::from_size(Size::from((f64::from(entry.natural.w), f64::from(entry.natural.h))));

                Some(TextureRenderElement::from_static_texture(
                    entry.id.clone(),
                    renderer.context_id(),
                    location,
                    entry.texture.clone(),
                    entry.buffer_scale,
                    Transform::Normal,
                    Some(alpha),
                    Some(src),
                    Some(display),
                    None,
                    Kind::Unspecified,
                ))
            })
            .collect()
    }
}

struct Capture {
    texture: GlesTexture,
    size: Size<i32, Logical>,
    margin: i32,
}

fn capture_window(renderer: &mut GlesRenderer, window: &GlideWindow, scale: Scale<f64>) -> Option<Capture> {
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

    let mut texture: GlesTexture = renderer.create_buffer(Fourcc::Abgr8888, (physical.w, physical.h).into()).ok()?;
    let mut target = renderer.bind(&mut texture).ok()?;

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
    tracker.render_output(renderer, &mut target, 0, &elements, [0.0, 0.0, 0.0, 0.0]).ok()?;
    drop(target);

    Some(Capture { texture, size, margin })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::raster::rect;

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

        #[test]
        fn it_shrinks_about_the_centre() {
            let from = rect(40, 60, 200, 100);
            let to = close_target(from);

            assert_eq!(to.size, Size::from((188, 94)));
            assert_eq!(to.loc, Point::from((46, 63)));
            assert!(to.size.w < from.size.w && to.size.h < from.size.h);
        }

        #[test]
        fn the_centre_stays_put_for_any_frame() {
            for (w, h) in [(1, 1), (7, 3), (199, 101), (800, 600), (1921, 1079)] {
                let from = rect(-300, 25, w, h);
                let to = close_target(from);
                let centre_from_x2 = (from.loc.x * 2 + from.size.w, from.loc.y * 2 + from.size.h);
                let centre_to_x2 = (to.loc.x * 2 + to.size.w, to.loc.y * 2 + to.size.h);
                assert!(
                    (centre_from_x2.0 - centre_to_x2.0).abs() <= 1 && (centre_from_x2.1 - centre_to_x2.1).abs() <= 1,
                    "{w}x{h}: centre moved from {centre_from_x2:?} to {centre_to_x2:?} (doubled)"
                );
            }
        }

        #[test]
        fn closing_is_quicker_than_a_geometry_morph() {
            assert!(close_duration() < morph_duration());
        }
    }

    mod morph {
        use super::*;

        #[test]
        fn the_morph_is_front_loaded() {
            let travelled = glide_theme::motion::E_OUT.ease_f64(0.25);
            assert!(travelled > 0.5, "only {travelled} of the way at a quarter of the duration");
        }

        #[test]
        fn a_morph_lands_exactly_on_the_layouts_rect() {
            let to = rect(400, 300, 800, 600);
            assert_eq!(lerp_rect(rect(0, 0, 1920, 1080), to, 1.0), to);
        }
    }

    mod grip {
        use super::*;

        fn restored_x(from: Rectangle<i32, Logical>, pointer_x: f64, restored_w: i32) -> i32 {
            let grip = (pointer_x - f64::from(from.loc.x)) / f64::from(from.size.w);
            (pointer_x - grip * f64::from(restored_w)).round() as i32
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

            let left = restored_x(maximized, 192.0, 800);
            assert_eq!(left, 192 - 80);

            let right = restored_x(maximized, 1728.0, 800);
            assert_eq!(right, 1728 - 720);
        }

        #[test]
        fn the_pointer_always_lands_on_the_restored_window() {
            let maximized = rect(0, 0, 1920, 1080);
            let restored_w = 800;
            for step in 0..=100 {
                let pointer_x = f64::from(step) / 100.0 * 1920.0;
                let x = restored_x(maximized, pointer_x, restored_w);
                let pointer = pointer_x.round() as i32;
                assert!(
                    (x..=x + restored_w).contains(&pointer),
                    "pointer {pointer} fell outside the restored window at {x}"
                );
            }
        }
    }
}

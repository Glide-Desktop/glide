use std::time::{Duration, Instant};

use glide_layout::{WindowId, WorkspaceId};
use smithay::utils::{Logical, Point, Rectangle, Size};

pub const SLOP: f64 = 6.0;
pub const LIFTED_WIDTH: f64 = 240.0;
pub const OVER_SHRINK: f64 = 0.8;
pub const DWELL: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Workspace(WorkspaceId),
    New,
    Edge(Side),
    Page,
}

#[derive(Debug, Clone)]
pub struct Drag {
    pub id: WindowId,
    start: Point<f64, Logical>,
    pointer: Point<f64, Logical>,
    grab: (f64, f64),
    tile: Size<f64, Logical>,
    lifted: bool,
    lift: glide_ui::Tween,
    over: glide_ui::Tween,
    target: Option<Target>,
    dwell: Option<Instant>,
}

fn lift_duration() -> Duration {
    glide_theme::motion::t2()
}

impl Drag {
    #[must_use]
    pub fn press(id: WindowId, pointer: Point<f64, Logical>, tile: Rectangle<i32, Logical>) -> Self {
        let tile_f = tile.to_f64();
        let fraction = |offset: f64, extent: f64| if extent > 0.0 { (offset / extent).clamp(0.0, 1.0) } else { 0.5 };
        Self {
            id,
            start: pointer,
            pointer,
            grab: (
                fraction(pointer.x - tile_f.loc.x, tile_f.size.w),
                fraction(pointer.y - tile_f.loc.y, tile_f.size.h),
            ),
            tile: tile_f.size,
            lifted: false,
            lift: glide_ui::Tween::at(0.0),
            over: glide_ui::Tween::at(0.0),
            target: None,
            dwell: None,
        }
    }

    pub fn motion(&mut self, pointer: Point<f64, Logical>) -> bool {
        self.pointer = pointer;
        if !self.lifted {
            let (dx, dy) = (pointer.x - self.start.x, pointer.y - self.start.y);
            if dx.hypot(dy) < SLOP {
                return false;
            }
            self.lifted = true;
            self.lift.retarget(1.0, lift_duration(), glide_theme::motion::ease_out);
        }
        true
    }

    #[must_use]
    pub const fn is_lifted(&self) -> bool {
        self.lifted
    }

    #[must_use]
    pub const fn target(&self) -> Option<Target> {
        self.target
    }

    pub fn set_target(&mut self, target: Option<Target>, now: Instant) -> bool {
        if self.target == target {
            return false;
        }
        self.target = target;
        self.dwell = matches!(target, Some(Target::Edge(_))).then_some(now);
        let over = matches!(target, Some(Target::Workspace(_) | Target::New));
        self.over.retarget(if over { 1.0 } else { 0.0 }, lift_duration(), glide_theme::motion::ease_out);
        true
    }

    pub fn dwell_elapsed(&mut self, now: Instant) -> Option<Side> {
        let Some(Target::Edge(side)) = self.target else {
            return None;
        };
        let since = self.dwell?;
        if now.saturating_duration_since(since) < DWELL {
            return None;
        }
        self.dwell = None;
        Some(side)
    }

    #[must_use]
    pub fn dwell_pending(&self) -> bool {
        self.dwell.is_some()
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        self.lift.is_animating(lift_duration()) || self.over.is_animating(lift_duration())
    }

    #[must_use]
    pub fn scale(&self) -> f64 {
        let lifted = (LIFTED_WIDTH / self.tile.w.max(1.0)).min(1.0);
        let lift = self.lift.value(lift_duration(), glide_theme::motion::ease_out);
        let over = self.over.value(lift_duration(), glide_theme::motion::ease_out);
        (1.0 + (lifted - 1.0) * lift) * (1.0 + (OVER_SHRINK - 1.0) * over)
    }

    #[must_use]
    pub fn rect(&self) -> Rectangle<f64, Logical> {
        let scale = self.scale();
        let size = Size::from((self.tile.w * scale, self.tile.h * scale));
        let loc = Point::from((self.pointer.x - self.grab.0 * size.w, self.pointer.y - self.grab.1 * size.h));
        Rectangle::new(loc, size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((100, 100)), Size::from((600, 400)))
    }

    fn at(x: f64, y: f64) -> Point<f64, Logical> {
        Point::from((x, y))
    }

    #[test]
    fn a_press_that_never_travels_six_pixels_stays_a_click() {
        let mut drag = Drag::press(WindowId::next(), at(400.0, 300.0), tile());
        assert!(!drag.motion(at(404.0, 303.0)));
        assert!(!drag.is_lifted());
        assert!(drag.motion(at(407.0, 300.0)));
        assert!(drag.is_lifted());
        assert!(drag.motion(at(400.0, 300.0)), "once lifted it stays lifted");
    }

    #[test]
    fn the_lifted_tile_keeps_the_point_it_was_grabbed_by_under_the_pointer() {
        let mut drag = Drag::press(WindowId::next(), at(250.0, 200.0), tile());
        drag.motion(at(900.0, 700.0));
        let rect = drag.rect();
        let grabbed = (rect.loc.x + rect.size.w * 0.25, rect.loc.y + rect.size.h * 0.25);
        assert!((grabbed.0 - 900.0).abs() < 1e-6 && (grabbed.1 - 700.0).abs() < 1e-6, "{rect:?}");
        if glide_theme::motion::is_disabled() {
            assert!((rect.size.w - LIFTED_WIDTH).abs() < 1e-6, "it lifts to its carried width");
        }
        assert!(rect.size.w <= tile().size.w as f64);
    }

    #[test]
    fn a_target_under_the_tile_shrinks_it_and_an_edge_does_not() {
        let mut drag = Drag::press(WindowId::next(), at(250.0, 200.0), tile());
        drag.motion(at(300.0, 300.0));
        let now = Instant::now();
        assert!(drag.set_target(Some(Target::Workspace(WorkspaceId(7))), now));
        assert!(!drag.set_target(Some(Target::Workspace(WorkspaceId(7))), now), "the same target is a no-op");
        assert!(!drag.dwell_pending(), "only edges dwell");
        assert!(drag.set_target(Some(Target::Edge(Side::Right)), now));
        assert!(drag.dwell_pending());
    }

    #[test]
    fn an_edge_switches_the_page_once_after_the_dwell() {
        let mut drag = Drag::press(WindowId::next(), at(250.0, 200.0), tile());
        drag.motion(at(10.0, 300.0));
        let now = Instant::now();
        drag.set_target(Some(Target::Edge(Side::Left)), now);
        assert_eq!(drag.dwell_elapsed(now + DWELL / 2), None);
        assert_eq!(drag.dwell_elapsed(now + DWELL), Some(Side::Left));
        assert_eq!(drag.dwell_elapsed(now + DWELL * 3), None, "one switch per visit to the edge");
        drag.set_target(None, now);
        drag.set_target(Some(Target::Edge(Side::Left)), now + DWELL * 4);
        assert_eq!(drag.dwell_elapsed(now + DWELL * 5), Some(Side::Left), "a new visit dwells again");
    }
}

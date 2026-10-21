use std::time::{Duration, Instant};

use smithay::utils::{Logical, Point, Rectangle};

use crate::snap::SnapZone;
use crate::state::GlideState;

pub const DWELL: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragTarget {
    PreviousWorkspace,
    NextWorkspace,
    DockItem(usize),
}

#[derive(Debug, Default)]
pub struct DragState {
    pub icon: Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>,
    pub touch_at: Option<Point<f64, Logical>>,
    pub active: bool,
    dwell: DragDwell,
    pointer: Point<f64, Logical>,
    icon_offset: Point<i32, Logical>,
    pub dock_drop: crate::dock_drop::DockDrop,
}

impl DragState {
    pub fn set_icon(
        &mut self, icon: Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>,
    ) -> Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface> {
        self.icon_offset = Point::default();
        std::mem::replace(&mut self.icon, icon)
    }

    pub fn move_icon(&mut self, delta: Point<i32, Logical>) {
        self.icon_offset += delta;
    }

    #[must_use]
    pub fn icon_offset(&self) -> Point<i32, Logical> {
        self.icon_offset
    }
}

#[derive(Debug, Default)]
pub struct DragDwell {
    target: Option<DragTarget>,
    since: Option<Instant>,
    fired: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DwellStep {
    Idle,
    Started,
    Waiting,
    Fire(DragTarget),
}

impl DragDwell {
    pub fn update(&mut self, target: Option<DragTarget>, now: Instant) -> DwellStep {
        if target != self.target {
            self.target = target;
            self.since = target.map(|_| now);
            self.fired = false;
            return if target.is_some() { DwellStep::Started } else { DwellStep::Idle };
        }
        match (self.target, self.since) {
            (Some(target), Some(since)) if !self.fired => {
                if now.saturating_duration_since(since) >= DWELL {
                    self.fired = true;
                    DwellStep::Fire(target)
                } else {
                    DwellStep::Waiting
                }
            }
            _ => DwellStep::Idle,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[must_use]
pub fn edge_target(
    pointer: Point<f64, Logical>, output: Rectangle<i32, Logical>, outputs: &[Rectangle<i32, Logical>],
) -> Option<DragTarget> {
    match crate::snap::zone_at_on(pointer, output, output, outputs)? {
        SnapZone::Left => Some(DragTarget::PreviousWorkspace),
        SnapZone::Right => Some(DragTarget::NextWorkspace),
        SnapZone::Maximize => None,
    }
}

impl GlideState {
    pub fn drag_started(&mut self) {
        self.drag.active = true;
        self.drag.dwell.reset();
    }

    pub fn drag_finished(&mut self) {
        self.drag.active = false;
        self.drag.dwell.reset();
        self.drag.dock_drop = crate::dock_drop::DockDrop::default();
    }

    pub fn drag_assist_motion(&mut self, pointer: Point<f64, Logical>) {
        if !self.drag.active {
            return;
        }
        self.drag.pointer = pointer;
        self.prefetch_dock_drop(pointer);
        let target = self.drag_target(pointer);
        match self.drag.dwell.update(target, Instant::now()) {
            DwellStep::Started => self.schedule_drag_dwell(),
            DwellStep::Fire(target) => self.fire_drag_target(target),
            DwellStep::Idle | DwellStep::Waiting => {}
        }
    }

    fn drag_target(&self, pointer: Point<f64, Logical>) -> Option<DragTarget> {
        let area = self.dock_area();
        if self.dock.contains(area, pointer) {
            return self.dock.item_at(area, pointer).map(DragTarget::DockItem);
        }
        let at = pointer.to_i32_floor::<i32>();
        let output = crate::state::logical_rect(self.layout.output_geometry_at(glide_layout::Point::new(at.x, at.y)));
        let outputs: Vec<Rectangle<i32, Logical>> =
            self.layout.outputs().iter().map(|o| crate::state::logical_rect(o.geometry)).collect();
        edge_target(pointer, output, &outputs)
    }

    fn schedule_drag_dwell(&self) {
        use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
        let timer = Timer::from_duration(DWELL);
        let inserted = self.loop_handle.insert_source(timer, |_, (), state| {
            let pointer = state.drag.pointer;
            state.drag_assist_motion(pointer);
            TimeoutAction::Drop
        });
        if let Err(err) = inserted {
            tracing::warn!(?err, "failed to schedule the drag dwell check");
        }
    }

    fn fire_drag_target(&mut self, target: DragTarget) {
        let active = self.active_workspace_index().unwrap_or(0);
        let count = self.layout.workspace_count();
        match target {
            DragTarget::PreviousWorkspace if active > 0 => {
                self.switch_workspace_animated(glide_layout::WorkspaceTarget::Previous);
            }
            DragTarget::NextWorkspace if active + 1 < count => {
                self.switch_workspace_animated(glide_layout::WorkspaceTarget::Next);
            }
            DragTarget::DockItem(index) => {
                let has_windows =
                    self.dock.items().get(index).is_some_and(|item| !self.windows_for_app(&item.app_id).is_empty());
                if has_windows {
                    self.activate_dock_item(index);
                }
            }
            DragTarget::PreviousWorkspace | DragTarget::NextWorkspace => {}
        }
        self.queue_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, w: i32) -> Rectangle<i32, Logical> {
        Rectangle::new((x, 0).into(), (w, 1000).into())
    }

    #[test]
    fn a_target_fires_once_after_the_dwell_and_again_only_after_leaving() {
        let mut dwell = DragDwell::default();
        let t0 = Instant::now();
        let edge = Some(DragTarget::NextWorkspace);
        assert_eq!(dwell.update(edge, t0), DwellStep::Started);
        assert_eq!(dwell.update(edge, t0 + DWELL / 2), DwellStep::Waiting);
        assert_eq!(dwell.update(edge, t0 + DWELL), DwellStep::Fire(DragTarget::NextWorkspace));
        assert_eq!(dwell.update(edge, t0 + DWELL * 3), DwellStep::Idle);
        assert_eq!(dwell.update(None, t0 + DWELL * 4), DwellStep::Idle);
        assert_eq!(dwell.update(edge, t0 + DWELL * 5), DwellStep::Started);
        assert_eq!(dwell.update(edge, t0 + DWELL * 6), DwellStep::Fire(DragTarget::NextWorkspace));
    }

    #[test]
    fn moving_to_another_target_restarts_the_dwell() {
        let mut dwell = DragDwell::default();
        let t0 = Instant::now();
        dwell.update(Some(DragTarget::DockItem(1)), t0);
        assert_eq!(dwell.update(Some(DragTarget::DockItem(2)), t0 + DWELL), DwellStep::Started);
        assert_eq!(
            dwell.update(Some(DragTarget::DockItem(2)), t0 + DWELL * 2),
            DwellStep::Fire(DragTarget::DockItem(2))
        );
    }

    #[test]
    fn the_icon_offset_adds_up_and_starts_over_with_the_next_icon() {
        let mut drag = DragState::default();
        drag.move_icon((10, 8).into());
        drag.move_icon((-4, 0).into());
        assert_eq!(drag.icon_offset(), (6, 8).into());
        assert!(drag.set_icon(None).is_none());
        assert_eq!(drag.icon_offset(), Point::default(), "a new icon starts with its corner on the hotspot");
    }

    #[test]
    fn only_outer_side_edges_flip_workspaces() {
        let single = [rect(0, 1920)];
        assert_eq!(edge_target((2.0, 500.0).into(), single[0], &single), Some(DragTarget::PreviousWorkspace));
        assert_eq!(edge_target((1918.0, 500.0).into(), single[0], &single), Some(DragTarget::NextWorkspace));
        assert_eq!(edge_target((900.0, 500.0).into(), single[0], &single), None);
        assert_eq!(edge_target((900.0, 2.0).into(), single[0], &single), None);
        let pair = [rect(0, 1920), rect(1920, 1920)];
        assert_eq!(
            edge_target((1918.0, 500.0).into(), pair[0], &pair),
            None,
            "an edge shared with another output is for crossing, not flipping"
        );
    }
}

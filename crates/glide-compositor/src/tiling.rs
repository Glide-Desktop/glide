use smithay::utils::{Logical, Point, Rectangle, Size};

use crate::snap::{SnapZone, zone_rect};

type Rect = Rectangle<i32, Logical>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    const fn zone(self) -> SnapZone {
        match self {
            Self::Left => SnapZone::Left,
            Self::Right => SnapZone::Right,
        }
    }

    const fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[must_use]
pub fn neighbour(output: Rect, outputs: &[(Rect, Rect)], side: Side) -> Option<(Rect, Rect)> {
    let overlaps =
        |other: Rect| other.loc.y < output.loc.y + output.size.h && output.loc.y < other.loc.y + other.size.h;
    outputs
        .iter()
        .copied()
        .filter(|(other, _)| *other != output && overlaps(*other))
        .filter_map(|(other, area)| {
            let gap = match side {
                Side::Left => output.loc.x - (other.loc.x + other.size.w),
                Side::Right => other.loc.x - (output.loc.x + output.size.w),
            };
            (gap >= 0).then_some((gap, (other, area)))
        })
        .min_by_key(|(gap, _)| *gap)
        .map(|(_, found)| found)
}

#[must_use]
pub fn tile(window: Rect, area: Rect, side: Side, beyond: Option<Rect>) -> Rect {
    let here = zone_rect(side.zone(), area);
    match beyond {
        Some(next) if window == here => zone_rect(side.opposite().zone(), next),
        _ => here,
    }
}

#[must_use]
pub fn carry(window: Rect, from: Rect, to: Rect) -> Rect {
    let size = Size::from((window.size.w.min(to.size.w), window.size.h.min(to.size.h)));
    let offset = window.loc - from.loc;
    let x = (to.loc.x + offset.x).clamp(to.loc.x, to.loc.x + to.size.w - size.w);
    let y = (to.loc.y + offset.y).clamp(to.loc.y, to.loc.y + to.size.h - size.h);
    Rectangle::new(Point::from((x, y)), size)
}

fn layout_rect(rect: Rect) -> glide_layout::Rectangle {
    glide_layout::Rectangle::from_loc_and_size(rect.loc.x, rect.loc.y, rect.size.w, rect.size.h)
}

impl crate::GlideState {
    fn focused_frame(&self) -> Option<(glide_layout::WindowId, Rect, bool)> {
        let id = self.layout.focused_window()?;
        let state = self.layout.window_state(id)?;
        (!state.fullscreen).then(|| (id, crate::state::logical_rect(state.geometry), state.maximized))
    }

    fn outputs_with_areas(&self) -> Vec<(Rect, Rect)> {
        self.layout
            .outputs()
            .iter()
            .map(|output| (crate::state::logical_rect(output.geometry), crate::state::logical_rect(output.work_area)))
            .collect()
    }

    fn output_of(&self, window: Rect) -> (Rect, Rect) {
        let centre = glide_layout::Point::new(window.loc.x + window.size.w / 2, window.loc.y + window.size.h / 2);
        (
            crate::state::logical_rect(self.layout.output_geometry_at(centre)),
            crate::state::logical_rect(self.layout.work_area_at(centre)),
        )
    }

    fn place(&mut self, id: glide_layout::WindowId, target: Rect, unmaximize: bool) {
        self.morph_geometry(id, |state| {
            if unmaximize {
                state.layout.set_maximized(id, false);
            }
            state.layout.set_window_geometry(id, layout_rect(target));
        });
    }

    pub fn tile_focused(&mut self, side: Side) {
        let Some((id, frame, maximized)) = self.focused_frame() else {
            return;
        };
        let (output, area) = self.output_of(frame);
        let beyond = neighbour(output, &self.outputs_with_areas(), side).map(|(_, area)| area);
        let target = tile(frame, area, side, beyond);
        let untiled =
            self.layout.window_state(id).and_then(|state| state.saved_geometry).unwrap_or_else(|| layout_rect(frame));
        self.session.tiled_from.entry(id).or_insert(untiled);
        tracing::info!(?id, ?side, "tiling the focused window");
        self.place(id, target, maximized);
    }

    pub fn maximize_focused(&mut self) {
        if let Some((id, _, false)) = self.focused_frame() {
            self.morph_geometry(id, |state| {
                state.layout.set_maximized(id, true);
            });
        }
    }

    pub fn restore_focused(&mut self) {
        let Some((id, _, maximized)) = self.focused_frame() else {
            return;
        };
        if maximized {
            self.morph_geometry(id, |state| {
                state.layout.set_maximized(id, false);
            });
        } else if let Some(previous) = self.session.tiled_from.remove(&id) {
            self.place(id, crate::state::logical_rect(previous), false);
        } else {
            self.minimize_with_flight(id);
        }
    }

    pub fn move_focused_to_output(&mut self, side: Side) {
        let Some((id, frame, maximized)) = self.focused_frame() else {
            return;
        };
        let (output, area) = self.output_of(frame);
        let Some((_, next_area)) = neighbour(output, &self.outputs_with_areas(), side) else {
            return;
        };
        tracing::info!(?id, ?side, "moving the focused window to the next monitor");
        if maximized {
            self.morph_geometry(id, |state| {
                state.layout.set_maximized(id, false);
                let restored = state.layout.window_state(id).map(|window| window.geometry);
                if let Some(restored) = restored {
                    let moved = carry(crate::state::logical_rect(restored), area, next_area);
                    state.layout.set_window_geometry(id, layout_rect(moved));
                }
                state.layout.set_maximized(id, true);
            });
        } else {
            self.session.tiled_from.remove(&id);
            self.place(id, carry(frame, area, next_area), false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rectangle::new(Point::from((x, y)), Size::from((w, h)))
    }

    #[test]
    fn tiling_takes_a_half_and_a_second_press_crosses_to_the_next_monitor() {
        let area = rect(0, 32, 1920, 1048);
        let window = rect(300, 200, 800, 600);
        let left = tile(window, area, Side::Left, None);
        assert_eq!(left, rect(0, 32, 960, 1048));
        assert_eq!(tile(left, area, Side::Left, None), left, "no monitor beyond: it stays put");
        let beyond = rect(-2560, 0, 2560, 1440);
        assert_eq!(tile(left, area, Side::Left, Some(beyond)), rect(-1280, 0, 1280, 1440), "the right half over there");
        assert_eq!(tile(window, area, Side::Right, None), rect(960, 32, 960, 1048));
    }

    #[test]
    fn the_neighbour_is_the_nearest_monitor_on_that_side_that_shares_rows() {
        let laptop = rect(0, 0, 1920, 1080);
        let outputs = [
            (laptop, laptop),
            (rect(1920, 0, 2560, 1080), rect(1920, 32, 2560, 1048)),
            (rect(-1280, 0, 1280, 1024), rect(-1280, 0, 1280, 1024)),
            (rect(0, 1080, 1920, 1080), rect(0, 1080, 1920, 1080)),
        ];
        let right = neighbour(laptop, &outputs, Side::Right).expect("a monitor on the right");
        assert_eq!(right, (rect(1920, 0, 2560, 1080), rect(1920, 32, 2560, 1048)), "with its own work area");
        assert_eq!(neighbour(laptop, &outputs, Side::Left).map(|(output, _)| output), Some(rect(-1280, 0, 1280, 1024)));
        let below = rect(0, 1080, 1920, 1080);
        assert_eq!(neighbour(below, &outputs, Side::Right), None, "a monitor beside the one above is not beside this");
    }

    #[test]
    fn a_carried_window_keeps_its_offset_and_fits_the_new_monitor() {
        let from = rect(0, 0, 1920, 1080);
        let to = rect(1920, 0, 1280, 720);
        assert_eq!(carry(rect(100, 50, 800, 600), from, to), rect(2020, 50, 800, 600));
        assert_eq!(carry(rect(1000, 500, 1600, 900), from, to), rect(1920, 0, 1280, 720), "shrunk and clamped");
        assert_eq!(carry(rect(1500, 600, 400, 300), from, to), rect(2800, 420, 400, 300), "pulled back inside");
    }
}

use smithay::{
    input::pointer::{CursorImageStatus, Focus, GrabStartData},
    utils::{Logical, Point, Rectangle, Serial},
};

use crate::grabs::{MoveSurfaceGrab, ResizeSurfaceGrab, resize_grab::ResizeEdge};

pub const BTN_LEFT: u32 = 0x110;
pub const BTN_RIGHT: u32 = 0x111;

#[must_use]
pub fn resize_edges(window: Rectangle<i32, Logical>, pointer: Point<f64, Logical>) -> ResizeEdge {
    let centre_x = f64::from(window.loc.x) + f64::from(window.size.w) / 2.0;
    let centre_y = f64::from(window.loc.y) + f64::from(window.size.h) / 2.0;
    let horizontal = if pointer.x < centre_x { ResizeEdge::LEFT } else { ResizeEdge::RIGHT };
    let vertical = if pointer.y < centre_y { ResizeEdge::TOP } else { ResizeEdge::BOTTOM };
    horizontal | vertical
}

#[derive(Debug, Default)]
pub struct SuperTap {
    armed: bool,
}

impl SuperTap {
    pub fn press(&mut self, is_super: bool, other_modifiers: bool) {
        self.armed = is_super && !other_modifiers;
    }

    pub fn interrupt(&mut self) {
        self.armed = false;
    }

    pub fn release(&mut self, is_super: bool) -> bool {
        is_super && std::mem::take(&mut self.armed)
    }
}

#[must_use]
pub fn is_super(raw_syms: &[smithay::input::keyboard::Keysym], modified: smithay::input::keyboard::Keysym) -> bool {
    use smithay::input::keyboard::Keysym;
    [Keysym::Super_L, Keysym::Super_R].iter().any(|key| *key == modified || raw_syms.contains(key))
}

impl crate::GlideState {
    pub fn super_drag(&mut self, pointer: Point<f64, Logical>, button: u32, serial: Serial) -> bool {
        if button != BTN_LEFT && button != BTN_RIGHT {
            return false;
        }
        let logo = self.seat.get_keyboard().is_some_and(|keyboard| keyboard.modifier_state().logo);
        if !logo || self.screen_locked() || self.layout.overview_active() || self.launcher.is_some() {
            return false;
        }
        let Some((window, location)) = self.space.element_under(pointer).map(|(window, at)| (window.clone(), at))
        else {
            return false;
        };
        if self.window_id(&window).and_then(|id| self.layout.window_state(id)).is_none_or(|state| state.fullscreen) {
            return false;
        }
        let Some(handle) = self.seat.get_pointer() else {
            return false;
        };
        self.finish_workspace_swipe();
        let start_data = GrabStartData { focus: None, button, location: pointer };
        if button == BTN_LEFT {
            let pending_restore = self.is_size_locked(&window);
            window.set_dragging(true);
            let grab = MoveSurfaceGrab { start_data, window, initial_window_location: location, pending_restore };
            handle.set_grab(self, grab, serial, Focus::Clear);
        } else {
            if self.is_size_locked(&window) {
                return true;
            }
            let rect = Rectangle::new(location, window.geometry().size);
            let edges = resize_edges(rect, pointer);
            let grab = ResizeSurfaceGrab::start(start_data, window, edges, rect);
            handle.set_grab(self, grab, serial, Focus::Clear);
            self.cursor_status = CursorImageStatus::Named(crate::decoration::resize_cursor(edges));
        }
        self.session.super_tap.interrupt();
        self.queue_redraw();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_super_right_drag_resizes_from_the_nearest_corner() {
        let window = Rectangle::new(Point::from((100, 100)), (800, 600).into());
        assert_eq!(resize_edges(window, Point::from((150.0, 150.0))), ResizeEdge::TOP_LEFT);
        assert_eq!(resize_edges(window, Point::from((850.0, 650.0))), ResizeEdge::BOTTOM_RIGHT);
        assert_eq!(resize_edges(window, Point::from((850.0, 150.0))), ResizeEdge::TOP_RIGHT);
        assert_eq!(resize_edges(window, Point::from((150.0, 650.0))), ResizeEdge::BOTTOM_LEFT);
    }

    #[test]
    fn a_lone_super_tap_fires_but_not_after_a_chord_or_a_click() {
        let mut tap = SuperTap::default();
        tap.press(true, false);
        assert!(tap.release(true));
        assert!(!tap.release(true), "one tap, one action");

        tap.press(true, false);
        tap.press(false, false);
        assert!(!tap.release(true), "Super+A is a shortcut, not a tap");

        tap.press(true, false);
        tap.interrupt();
        assert!(!tap.release(true), "Super+drag is not a tap");

        tap.press(true, true);
        assert!(!tap.release(true), "Ctrl+Super is not a tap");
    }
}

use smithay::{
    input::pointer::PointerHandle,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point},
    wayland::{
        pointer_constraints::{PointerConstraintsHandler, with_pointer_constraint},
        seat::WaylandFocus,
    },
};

use crate::GlideState;

impl PointerConstraintsHandler for GlideState {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        let Some(current_focus) = pointer.current_focus() else {
            return;
        };
        if current_focus.wl_surface().as_deref() == Some(surface) {
            with_pointer_constraint(surface, pointer, |constraint| {
                constraint.expect("just created").activate();
            });
        }
    }

    fn remove_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        if !with_pointer_constraint(surface, pointer, |constraint| constraint.is_none()) {
            return;
        }
        let Some((hint_surface, hint_location)) = &self.cursor_position_hint else {
            return;
        };
        let origin: Point<f64, Logical> = self
            .space
            .elements()
            .find_map(|window| {
                (window.wl_surface().as_deref() == Some(hint_surface)).then(|| window.geometry())
            })
            .unwrap_or_default()
            .loc
            .to_f64();
        pointer.set_location(origin + *hint_location);
        self.cursor_position_hint = None;
    }

    fn cursor_position_hint(
        &mut self,
        surface: &WlSurface,
        pointer: &PointerHandle<Self>,
        location: Point<f64, Logical>,
    ) {
        let is_active_lock = with_pointer_constraint(surface, pointer, |constraint| {
            constraint.is_some_and(|c| c.is_active())
        });
        if is_active_lock {
            self.cursor_position_hint = Some((surface.clone(), location));
        }
    }
}

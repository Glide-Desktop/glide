use smithay::{
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Rectangle},
    wayland::{
        input_method::{InputMethodHandler, PopupSurface},
        keyboard_shortcuts_inhibit::{
            KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState, KeyboardShortcutsInhibitor,
        },
    },
};

use crate::state::GlideState;

impl InputMethodHandler for GlideState {
    fn new_popup(&mut self, surface: PopupSurface) {
        if let Err(err) = self.popups.track_popup(surface.into()) {
            tracing::warn!(?err, "failed to track an input-method popup");
        }
        self.queue_redraw();
    }

    fn dismiss_popup(&mut self, _surface: PopupSurface) {
        self.queue_redraw();
    }

    fn popup_repositioned(&mut self, _surface: PopupSurface) {
        self.queue_redraw();
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        self.space
            .elements()
            .find(|window| crate::state::window_surface(window) == parent)
            .and_then(|window| {
                let location = self.space.element_location(window)?;
                Some(Rectangle::new(location, window.geometry().size))
            })
            .unwrap_or_default()
    }
}

impl KeyboardShortcutsInhibitHandler for GlideState {
    fn keyboard_shortcuts_inhibit_state(&mut self) -> &mut KeyboardShortcutsInhibitState {
        &mut self.protocols.keyboard_shortcuts_inhibit_state
    }

    fn new_inhibitor(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        inhibitor.activate();
        tracing::info!("a client took over the keyboard shortcuts");
    }
}

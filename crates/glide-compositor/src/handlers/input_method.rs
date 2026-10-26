use smithay::{
    reexports::{
        wayland_protocols::wp::text_input::zv3::server::zwp_text_input_v3::{ContentHint, ContentPurpose},
        wayland_server::protocol::wl_surface::WlSurface,
    },
    utils::{Logical, Rectangle},
    wayland::{
        input_method::{InputMethodHandler, PopupSurface},
        keyboard_shortcuts_inhibit::{
            KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState, KeyboardShortcutsInhibitor,
        },
        text_input::TextInputActivation,
    },
};

use crate::{focused_field::Field, state::GlideState};

impl TextInputActivation for GlideState {
    fn activated(&mut self, content_type: Option<(ContentHint, ContentPurpose)>) {
        let field = Field::from_content(content_type);
        let app = self.layout.focused_window().and_then(|id| self.dock_key(id));
        tracing::info!(kind = ?field.kind, sensitive = field.sensitive, app = app.as_deref().unwrap_or("-"), "text field focused");
        self.session.text_field = Some(field);
        self.offer_code();
        self.offer_passwords();
    }

    fn deactivated(&mut self) {
        self.session.text_field = None;
        self.note_login_offer(false);
    }
}

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
        self.request_shortcut_inhibit(inhibitor);
    }
}

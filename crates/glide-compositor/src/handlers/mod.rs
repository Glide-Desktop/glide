mod compositor;
pub mod ext_workspace;
pub mod foreign_toplevel_list;
pub mod idle_notify;
mod input_method;
pub mod layer_shell;
mod pointer_constraints;
mod security_context;
mod selection;
mod session_lock;
pub mod wlr_gamma_control;
pub mod wlr_output_management;
pub mod wlr_output_power;
pub mod xdg_activation;
mod xdg_shell;

use smithay::input::dnd::{DnDGrab, DndGrabHandler, DndTarget, GrabType, Source};
use smithay::input::pointer::Focus;
use smithay::input::tablet::TabletSeatHandler;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Serial};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{
    DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler, set_data_device_focus,
};
use smithay::wayland::selection::primary_selection::set_primary_focus;

use crate::{GlideState, focus::PointerFocusTarget};

impl SeatHandler for GlideState {
    type KeyboardFocus = WlSurface;
    type PointerFocus = PointerFocusTarget;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<GlideState> {
        &mut self.protocols.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: smithay::input::pointer::CursorImageStatus) {
        self.cursor_status = image;
    }

    fn led_state_changed(&mut self, _seat: &Seat<Self>, _led_state: smithay::input::keyboard::LedState) {
        self.sync_keyboard_leds();
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client.clone());
        set_primary_focus(dh, seat, client);
    }
}

impl TabletSeatHandler for GlideState {
    type ToolFocus = WlSurface;
}

impl SelectionHandler for GlideState {
    type SelectionUserData = crate::clipboard::Owned;

    fn send_selection(
        &mut self, _ty: smithay::wayland::selection::SelectionTarget, _mime_type: String, fd: std::os::fd::OwnedFd,
        _seat: Seat<Self>, user_data: &Self::SelectionUserData,
    ) {
        crate::clipboard::write(user_data.clone(), fd);
    }
}

impl DataDeviceHandler for GlideState {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.protocols.data_device_state
    }
}

impl DndGrabHandler for GlideState {
    fn dropped(
        &mut self, target: Option<DndTarget<'_, Self>>, _validated: bool, _seat: Seat<Self>,
        location: Point<f64, Logical>,
    ) {
        if target.is_none() {
            self.finish_dock_drop(location);
        }
        self.drag_finished();
        self.clear_dnd_icon();
    }

    fn cancelled(&mut self, _seat: Seat<Self>, _location: Point<f64, Logical>) {
        self.drag_finished();
        self.clear_dnd_icon();
    }
}

impl GlideState {
    fn clear_dnd_icon(&mut self) {
        self.drag.touch_at = None;
        if self.drag.set_icon(None).is_some() {
            self.queue_redraw();
        }
    }
}

impl WaylandDndGrabHandler for GlideState {
    fn dnd_requested<S: Source>(
        &mut self, source: S, icon: Option<WlSurface>, seat: Seat<Self>, serial: Serial, type_: GrabType,
    ) {
        let shared = std::sync::Arc::new(source);
        self.drag.dock_drop.source = Some(shared.clone());
        let source = crate::dock_drop::SharedSource(shared);
        match type_ {
            GrabType::Pointer => {
                let Some(ptr) = seat.get_pointer() else {
                    tracing::warn!("drag requested on a seat without a pointer");
                    source.cancel();
                    return;
                };
                let Some(start_data) = ptr.grab_start_data() else {
                    tracing::warn!("drag requested without an active pointer grab");
                    source.cancel();
                    return;
                };

                self.drag.set_icon(icon);
                self.drag_started();
                let grab = DnDGrab::new_pointer(&self.display_handle, start_data, source, seat);
                ptr.set_grab(self, grab, serial, Focus::Keep);
                self.queue_redraw();
            }
            GrabType::Touch => {
                let Some(touch) = seat.get_touch() else {
                    tracing::warn!("drag requested on a seat without touch");
                    source.cancel();
                    return;
                };
                let Some(start_data) = touch.grab_start_data() else {
                    tracing::warn!("drag requested without an active touch grab");
                    source.cancel();
                    return;
                };
                self.drag.set_icon(icon);
                self.drag.touch_at = Some(start_data.location);
                self.drag_started();
                let grab = DnDGrab::new_touch(&self.display_handle, start_data, source, seat);
                touch.set_grab(self, grab, serial);
                self.queue_redraw();
            }
        }
    }
}

impl OutputHandler for GlideState {
    fn output_bound(
        &mut self, _output: smithay::output::Output,
        _wl_output: smithay::reexports::wayland_server::protocol::wl_output::WlOutput,
    ) {
        self.sync_ext_workspaces();
    }
}

impl smithay::wayland::xdg_foreign::XdgForeignHandler for GlideState {
    fn xdg_foreign_state(&mut self) -> &mut smithay::wayland::xdg_foreign::XdgForeignState {
        &mut self.protocols.xdg_foreign_state
    }
}

const BELL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

#[must_use]
pub fn bell_due(last: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    last.is_none_or(|last| now.saturating_duration_since(last) >= BELL_INTERVAL)
}

impl smithay::wayland::xdg_system_bell::XdgSystemBellHandler for GlideState {
    fn ring(&mut self, _surface: Option<WlSurface>) {
        let now = std::time::Instant::now();
        if !self.notifications.policy().sounds || !bell_due(self.session.bell, now) {
            return;
        }
        self.session.bell = Some(now);
        crate::sound::play(glide_notify::Sound::Named("bell".into()));
    }
}

impl smithay::wayland::xdg_toplevel_icon::XdgToplevelIconHandler for GlideState {
    fn set_icon(
        &mut self, _toplevel: smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::XdgToplevel,
        wl_surface: WlSurface,
    ) {
        let name = smithay::wayland::compositor::with_states(&wl_surface, |states| {
            let mut cached = states.cached_state.get::<smithay::wayland::xdg_toplevel_icon::ToplevelIconCachedState>();
            cached.pending().icon_name().map(str::to_string)
        });
        let Some(id) = self.window_id_for_surface(&wl_surface) else {
            return;
        };
        let app_id = self.window_map.get(&id).and_then(|window| window.identity().0);
        for key in app_id.into_iter().chain(self.dock_key(id)) {
            crate::icon::set_alias(&key, name.as_deref());
        }
        self.refresh_dock();
        self.queue_redraw();
    }
}

smithay::delegate_dispatch2!(GlideState);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bell_rung_in_a_burst_sounds_once() {
        let start = std::time::Instant::now();
        assert!(bell_due(None, start));
        assert!(!bell_due(Some(start), start + std::time::Duration::from_millis(100)));
        assert!(bell_due(Some(start), start + BELL_INTERVAL));
    }
}

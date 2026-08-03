mod compositor;
mod xdg_shell;

use smithay::input::dnd::{DnDGrab, DndGrabHandler, GrabType, Source};
use smithay::input::pointer::Focus;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Serial;
use smithay::wayland::output::OutputHandler;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{
    DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler, set_data_device_focus,
};

use crate::{GlideState, focus::PointerFocusTarget};

//
// Wl Seat
//

impl SeatHandler for GlideState {
    type KeyboardFocus = WlSurface;
    // Titlebars are compositor-drawn, not `WlSurface`s, and still need to
    // receive enter/motion/button like any other pointer target — see
    // `crate::focus` for why this is an enum rather than a bare surface.
    type PointerFocus = PointerFocusTarget;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<GlideState> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        image: smithay::input::pointer::CursorImageStatus,
    ) {
        self.cursor_status = image;
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);
    }
}

//
// Wl Data Device
//

impl SelectionHandler for GlideState {
    type SelectionUserData = ();
}

impl DataDeviceHandler for GlideState {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl DndGrabHandler for GlideState {}
impl WaylandDndGrabHandler for GlideState {
    fn dnd_requested<S: Source>(
        &mut self,
        source: S,
        _icon: Option<WlSurface>,
        seat: Seat<Self>,
        serial: Serial,
        type_: GrabType,
    ) {
        match type_ {
            GrabType::Pointer => {
                let ptr = seat
                    .get_pointer()
                    .expect("a pointer was added in GlideState::new");
                // The Wayland DnD protocol only allows starting a drag from
                // an already-active implicit pointer grab (e.g. the client's
                // own button-press grab on the drag source), so one must
                // exist here.
                let start_data = ptr
                    .grab_start_data()
                    .expect("dnd_requested is only called while a pointer grab is active");

                // create a dnd grab to start the operation
                let grab = DnDGrab::new_pointer(&self.display_handle, start_data, source, seat);
                ptr.set_grab(self, grab, serial, Focus::Keep);
            }
            GrabType::Touch => {
                // smallvil lacks touch handling
                source.cancel();
            }
        }
    }
}

//
// Wl Output & Xdg Output
//

impl OutputHandler for GlideState {}

smithay::delegate_dispatch2!(GlideState);

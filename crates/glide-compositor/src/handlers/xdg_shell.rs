use smithay::{
    desktop::{
        PopupKind, PopupManager, Space, Window, find_popup_root_surface, get_popup_toplevel_coords,
    },
    input::{
        Seat,
        pointer::{Focus, GrabStartData as PointerGrabStartData},
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            Resource,
            protocol::{wl_seat, wl_surface::WlSurface},
        },
    },
    utils::{Rectangle, Serial},
    wayland::{
        compositor::with_states,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            XdgToplevelSurfaceData,
        },
    },
};

use crate::{
    GlideState,
    grabs::{MoveSurfaceGrab, ResizeSurfaceGrab},
    state::window_surface,
};

impl XdgShellHandler for GlideState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        tracing::info!("new toplevel");
        let window = Window::new_wayland_window(surface);
        self.space.map_element(window, (0, 0), false);
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            let geometry = positioner.get_geometry();
            state.geometry = geometry;
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial) {
        // The protocol guarantees `seat` references a live object: a client
        // cannot reference a destroyed/foreign id, so dispatch would have
        // rejected the request before it ever reached this handler.
        let seat: Seat<GlideState> =
            Seat::from_resource(&seat).expect("wl_seat resource is guaranteed live by protocol");

        let wl_surface = surface.wl_surface();

        if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
            let pointer = seat
                .get_pointer()
                .expect("a pointer was added in GlideState::new");

            // `check_grab` already confirmed this surface's client holds the
            // current pointer grab, i.e. the window backing it is focused
            // and therefore mapped.
            let window = self
                .space
                .elements()
                .find(|w| window_surface(w) == wl_surface)
                .expect("check_grab confirmed this surface's window is focused, hence mapped")
                .clone();
            let initial_window_location = self
                .space
                .element_location(&window)
                .expect("window was just found in space.elements(), so it has a location");

            let grab = MoveSurfaceGrab {
                start_data,
                window,
                initial_window_location,
            };

            pointer.set_grab(self, grab, serial, Focus::Clear);
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let seat: Seat<GlideState> =
            Seat::from_resource(&seat).expect("wl_seat resource is guaranteed live by protocol");

        let wl_surface = surface.wl_surface();

        if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
            let pointer = seat
                .get_pointer()
                .expect("a pointer was added in GlideState::new");

            let window = self
                .space
                .elements()
                .find(|w| window_surface(w) == wl_surface)
                .expect("check_grab confirmed this surface's window is focused, hence mapped")
                .clone();
            let initial_window_location = self
                .space
                .element_location(&window)
                .expect("window was just found in space.elements(), so it has a location");
            let initial_window_size = window.geometry().size;

            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Resizing);
            });

            surface.send_pending_configure();

            let grab = ResizeSurfaceGrab::start(
                start_data,
                window,
                edges.into(),
                Rectangle::new(initial_window_location, initial_window_size),
            );

            pointer.set_grab(self, grab, serial, Focus::Clear);
        }
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        // Popup grabs (keeping a popup chain focused and closing it on
        // outside clicks) aren't implemented yet. Out of scope until window
        // management (M2) needs it -- right now nothing creates a popup
        // chain deep enough to make the lack of a grab noticeable.
    }
}

fn check_grab(
    seat: &Seat<GlideState>,
    surface: &WlSurface,
    serial: Serial,
) -> Option<PointerGrabStartData<GlideState>> {
    let pointer = seat.get_pointer()?;

    // Check that this surface has a click grab.
    if !pointer.has_grab(serial) {
        return None;
    }

    let start_data = pointer.grab_start_data()?;

    let (focus, _) = start_data.focus.as_ref()?;
    // If the focus was for a different surface, ignore the request.
    if !focus.id().same_client_as(&surface.id()) {
        return None;
    }

    Some(start_data)
}

/// Should be called on `WlSurface::commit`
pub fn handle_commit(popups: &mut PopupManager, space: &Space<Window>, surface: &WlSurface) {
    // Handle toplevel commits.
    if let Some(window) = space
        .elements()
        .find(|w| window_surface(w) == surface)
        .cloned()
    {
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .expect("smithay inserts XdgToplevelSurfaceData when the toplevel is created")
                .lock()
                .expect("single-threaded event loop, this mutex is never poisoned")
                .initial_configure_sent
        });

        if !initial_configure_sent {
            window
                .toplevel()
                .expect("only Window::new_wayland_window is used, so toplevel() is always Some")
                .send_configure();
        }
    }

    // Handle popup commits.
    popups.commit(surface);
    if let Some(popup) = popups.find_popup(surface) {
        match popup {
            PopupKind::Xdg(ref xdg) => {
                if !xdg.is_initial_configure_sent() {
                    // NOTE: This should never fail as the initial configure is always
                    // allowed.
                    xdg.send_configure().expect("initial configure failed");
                }
            }
            PopupKind::InputMethod(ref _input_method) => {}
        }
    }
}

impl GlideState {
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(window) = self.space.elements().find(|w| window_surface(w) == &root) else {
            return;
        };

        // No output is mapped before the first connector is set up in
        // device_added(), and in principle none may ever be mapped on
        // headless hardware -- unconstraining a popup without a screen to
        // fit it into has nothing meaningful to do, so skip it rather than
        // panicking.
        let Some(output) = self.space.outputs().next() else {
            return;
        };
        let output_geo = self
            .space
            .output_geometry(output)
            .expect("output came from space.outputs(), so it has a geometry");
        let window_geo = self
            .space
            .element_geometry(window)
            .expect("window came from space.elements(), so it has a geometry");

        // The target geometry for the positioner should be relative to its parent's geometry, so
        // we will compute that here.
        let mut target = output_geo;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}

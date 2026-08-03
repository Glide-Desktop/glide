use smithay::{
    desktop::{PopupKind, Window, find_popup_root_surface, get_popup_toplevel_coords},
    input::{
        Seat,
        pointer::{Focus, GrabStartData as PointerGrabStartData},
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            Resource,
            protocol::{wl_output::WlOutput, wl_seat, wl_surface::WlSurface},
        },
    },
    utils::{Rectangle, Serial},
    wayland::{
        compositor::with_states,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            XdgToplevelSurfaceData,
            dialog::{ToplevelDialogHint, XdgDialogHandler},
        },
    },
};

use crate::{
    GlideState,
    grabs::{MoveSurfaceGrab, ResizeSurfaceGrab},
    state::{layout_size, window_surface},
};

impl XdgShellHandler for GlideState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        tracing::info!("new toplevel");
        // Read the parent before the surface is moved into the window: a
        // toplevel with a parent is a dialog, and must land on the parent's
        // workspace rather than being exiled to a new one.
        let parent = surface
            .parent()
            .and_then(|parent| self.window_id_for_surface(&parent));

        self.track_window(Window::new_wayland_window(surface), parent);
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        tracing::info!("toplevel destroyed");
        self.untrack_window(surface.wl_surface());
    }

    fn parent_changed(&mut self, surface: ToplevelSurface) {
        // A client may set (or clear) its parent after mapping, e.g. when a
        // window is promoted to a dialog of another. Follow the parent so the
        // pair does not end up split across workspaces.
        let Some(id) = self.window_id_for_surface(surface.wl_surface()) else {
            return;
        };
        let Some(parent) = surface
            .parent()
            .and_then(|parent| self.window_id_for_surface(&parent))
        else {
            return;
        };
        let Some(target) = self.layout.window_state(parent).map(|s| s.workspace_id) else {
            return;
        };

        if self
            .layout
            .move_window_to_workspace(id, glide_layout::WorkspaceTarget::Id(target))
        {
            self.reconcile_space();
        }
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        self.set_toplevel_maximized(&surface, true);
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        self.set_toplevel_maximized(&surface, false);
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, _output: Option<WlOutput>) {
        // The requested output is ignored: Glide is single-output for now, so
        // there is only one screen a window can go fullscreen on.
        self.set_toplevel_fullscreen(&surface, true);
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        self.set_toplevel_fullscreen(&surface, false);
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
pub fn handle_commit(state: &mut GlideState, surface: &WlSurface) {
    let window = state
        .window_map
        .values()
        .find(|w| window_surface(w) == surface)
        .cloned();

    // Handle toplevel commits.
    if let Some(window) = window {
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

        if let Some(id) = state.window_id(&window) {
            // Only the commit that first gives the window a real size needs a
            // reconcile: that is when the layout finally has something to
            // centre. Reconciling on every commit would re-apply the stored
            // geometry constantly, undoing interactive moves mid-drag.
            let placed = state.layout.update_window_size(id, layout_size(&window));
            if placed {
                state.reconcile_space();
            }
        }
    }

    // Handle popup commits.
    let popups = &mut state.popups;
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

impl XdgDialogHandler for GlideState {
    /// A toplevel was marked as a dialog (or modal dialog) after mapping.
    ///
    /// `set_parent` is the usual way a dialog is tied to its parent, and
    /// `parent_changed` already handles that. This covers the case where the
    /// hint arrives *after* the window was placed — the dialog may by then be
    /// sitting on a workspace of its own, which for a modal is actively
    /// harmful: the parent would be unusable while its dialog is elsewhere.
    fn dialog_hint_changed(&mut self, toplevel: ToplevelSurface, hint: ToplevelDialogHint) {
        tracing::info!(?hint, "toplevel dialog hint changed");

        if !matches!(hint, ToplevelDialogHint::Dialog | ToplevelDialogHint::Modal) {
            return;
        }

        self.parent_changed(toplevel);
    }
}

impl GlideState {
    /// Applies a client's own maximize/unmaximize request through the layout.
    ///
    /// Routed via `glide-layout` rather than poked straight into the toplevel
    /// so that the saved pre-maximize geometry, and the compositor's idea of
    /// where the window lives, stay in one place.
    fn set_toplevel_maximized(&mut self, surface: &ToplevelSurface, maximized: bool) {
        let Some(id) = self.window_id_for_surface(surface.wl_surface()) else {
            return;
        };
        if self.layout.set_maximized(id, maximized) {
            self.reconcile_space();
        } else {
            // Nothing changed, but the protocol still requires an answer to
            // the request, otherwise the client waits forever.
            surface.send_pending_configure();
        }
    }

    fn set_toplevel_fullscreen(&mut self, surface: &ToplevelSurface, fullscreen: bool) {
        let Some(id) = self.window_id_for_surface(surface.wl_surface()) else {
            return;
        };
        if self.layout.set_fullscreen(id, fullscreen) {
            self.reconcile_space();
        } else {
            surface.send_pending_configure();
        }
    }

    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(window) = self.space.elements().find(|w| window_surface(w) == &root) else {
            return;
        };

        let window_geo = self
            .space
            .element_geometry(window)
            .expect("window came from space.elements(), so it has a geometry");

        let output = self
            .space
            .outputs()
            .find(|o| {
                self.space
                    .output_geometry(o)
                    .is_some_and(|geo| geo.contains(window_geo.loc))
            })
            .or_else(|| self.space.outputs().next());

        let Some(output) = output else {
            return;
        };
        let output_geo = self
            .space
            .output_geometry(output)
            .expect("output came from space.outputs(), so it has a geometry");

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

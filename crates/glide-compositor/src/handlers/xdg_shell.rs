use smithay::{
    desktop::{PopupKind, Window, find_popup_root_surface, get_popup_toplevel_coords},
    input::{
        Seat,
        pointer::{Focus, GrabStartData as PointerGrabStartData},
    },
    reexports::{
        wayland_protocols::xdg::{
            decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode,
            shell::server::xdg_toplevel,
        },
        wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::{
            Mode as KdeMode, OrgKdeKwinServerDecoration,
        },
        wayland_server::{
            Resource, WEnum,
            protocol::{wl_output::WlOutput, wl_seat, wl_surface::WlSurface},
        },
    },
    utils::{Rectangle, Serial},
    wayland::{
        compositor::with_states,
        shell::{
            kde::decoration::{KdeDecorationHandler, KdeDecorationState},
            xdg::{
                PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
                XdgToplevelSurfaceData,
                decoration::XdgDecorationHandler,
                dialog::{ToplevelDialogHint, XdgDialogHandler},
            },
        },
    },
};

use crate::{
    GlideState,
    grabs::{MoveSurfaceGrab, ResizeSurfaceGrab},
    state::{layout_size, window_surface},
    window::GlideWindow,
};

impl XdgShellHandler for GlideState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.protocols.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        tracing::info!("new toplevel");
        let parent = surface
            .parent()
            .and_then(|parent| self.window_id_for_surface(&parent));

        self.track_window(
            GlideWindow::new(Window::new_wayland_window(surface)),
            parent,
        );
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        tracing::info!("toplevel destroyed");
        self.untrack_window(surface.wl_surface());
        self.cleanup_dead_windows();
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        let Some(id) = self.window_id_for_surface(surface.wl_surface()) else {
            return;
        };
        let Some(window) = self.window_map.get(&id) else {
            return;
        };
        let title = with_states(surface.wl_surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().ok())
                .and_then(|guard| guard.title.clone())
        });
        if let Some(title) = title
            && let Some(mut deco) = window.try_decoration_state_mut()
        {
            deco.header.set_title(title);
        }
        self.queue_redraw();
    }

    fn parent_changed(&mut self, surface: ToplevelSurface) {
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

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}
}

fn check_grab(
    seat: &Seat<GlideState>,
    surface: &WlSurface,
    serial: Serial,
) -> Option<PointerGrabStartData<GlideState>> {
    let pointer = seat.get_pointer()?;

    if !pointer.has_grab(serial) {
        return None;
    }

    let start_data = pointer.grab_start_data()?;

    let (focus, _) = start_data.focus.as_ref()?;
    let focus_surface = focus.as_surface()?;
    if !focus_surface.id().same_client_as(&surface.id()) {
        return None;
    }

    Some(start_data)
}

pub fn handle_commit(state: &mut GlideState, surface: &WlSurface) {
    let window = state
        .window_map
        .values()
        .find(|w| window_surface(w) == surface)
        .cloned();

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
            let (title, app_id) = with_states(surface, |states| {
                states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .map(|data| {
                        let guard = data.lock().unwrap();
                        (
                            guard.title.clone().unwrap_or_default(),
                            guard.app_id.clone().unwrap_or_default(),
                        )
                    })
                    .unwrap_or_default()
            });

            if let Some(handle) = state.foreign_toplevel_handles.get(&id) {
                handle.send_title(&title);
                handle.send_app_id(&app_id);
                handle.send_done();
            }

            let placed = state.layout.update_window_size(id, layout_size(&window));
            if placed {
                state.reconcile_space();
            } else {
                // `update_window_size` already kept `WindowState.geometry`
                // in sync with this commit's live buffer size above
                // (that happens regardless of `placed`) — but for an
                // already-mapped window resizing in place, nothing else
                // re-derives from that new size. In particular, overview
                // mode's cached preview rects are a snapshot taken when it
                // was last (re)laid out, so a window resizing while
                // overview is open would otherwise leave its highlight
                // ring and label sized/positioned for its old dimensions
                // until something unrelated happened to trigger a full
                // `reconcile_space()`. `refresh_overview()` is a no-op
                // whenever overview isn't actually open, so this is cheap
                // the rest of the time.
                state.refresh_overview();
            }
        }
    }

    let popups = &mut state.popups;
    popups.commit(surface);
    if let Some(popup) = popups.find_popup(surface) {
        match popup {
            PopupKind::Xdg(ref xdg) => {
                if !xdg.is_initial_configure_sent() {
                    xdg.send_configure().expect("initial configure failed");
                }
            }
            PopupKind::InputMethod(ref _input_method) => {}
        }
    }
}

impl XdgDecorationHandler for GlideState {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(DecorationMode::ServerSide);
        });
        toplevel.send_pending_configure();

        if let Some(id) = self.window_id_for_surface(toplevel.wl_surface()) {
            self.set_window_decorated(id, true);
        }
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: DecorationMode) {
        let server_side = mode == DecorationMode::ServerSide;

        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(if server_side {
                DecorationMode::ServerSide
            } else {
                DecorationMode::ClientSide
            });
        });
        toplevel.send_pending_configure();

        if let Some(id) = self.window_id_for_surface(toplevel.wl_surface()) {
            self.set_window_decorated(id, server_side);
        }
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(DecorationMode::ServerSide);
        });
        toplevel.send_pending_configure();

        if let Some(id) = self.window_id_for_surface(toplevel.wl_surface()) {
            self.set_window_decorated(id, true);
        }
    }
}

impl KdeDecorationHandler for GlideState {
    fn kde_decoration_state(&self) -> &KdeDecorationState {
        &self.protocols.kde_decoration_state
    }

    fn request_mode(
        &mut self,
        surface: &WlSurface,
        decoration: &OrgKdeKwinServerDecoration,
        mode: WEnum<KdeMode>,
    ) {
        let WEnum::Value(mode) = mode else {
            return;
        };

        decoration.mode(mode);

        if let Some(id) = self.window_id_for_surface(surface) {
            self.set_window_decorated(id, mode == KdeMode::Server);
        }
    }
}

impl XdgDialogHandler for GlideState {
    fn dialog_hint_changed(&mut self, toplevel: ToplevelSurface, hint: ToplevelDialogHint) {
        tracing::info!(?hint, "toplevel dialog hint changed");

        if !matches!(hint, ToplevelDialogHint::Dialog | ToplevelDialogHint::Modal) {
            return;
        }

        self.parent_changed(toplevel);
    }
}

impl GlideState {
    fn set_toplevel_maximized(&mut self, surface: &ToplevelSurface, maximized: bool) {
        let Some(id) = self.window_id_for_surface(surface.wl_surface()) else {
            return;
        };
        if self.layout.set_maximized(id, maximized) {
            self.reconcile_space();
        } else {
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

        let mut target = output_geo;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}

use smithay::{
    desktop::{PopupKind, Window, find_popup_root_surface, get_popup_toplevel_coords},
    input::{
        Seat,
        pointer::{Focus, GrabStartData as PointerGrabStartData},
    },
    reexports::{
        wayland_protocols::xdg::{
            decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode, shell::server::xdg_toplevel,
        },
        wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::{
            Mode as KdeMode, OrgKdeKwinServerDecoration,
        },
        wayland_server::{
            WEnum,
            protocol::{wl_output::WlOutput, wl_seat, wl_surface::WlSurface},
        },
    },
    utils::{Logical, Point, Rectangle, Serial},
    wayland::{
        compositor::{get_parent, with_states},
        shell::{
            kde::decoration::{KdeDecorationHandler, KdeDecorationState},
            xdg::{
                PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState, XdgToplevelSurfaceData,
                decoration::XdgDecorationHandler,
                dialog::{ToplevelDialogHint, XdgDialogHandler},
            },
        },
    },
};

use crate::{
    GlideState,
    grabs::{MoveSurfaceGrab, ResizeSurfaceGrab, TouchMoveGrab, TouchResizeGrab},
    state::{layout_size, toplevel_title_and_app_id, window_surface},
    window::GlideWindow,
};

impl XdgShellHandler for GlideState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.protocols.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        tracing::info!("new toplevel");
        let parent = surface.parent().and_then(|parent| self.window_id_for_surface(&parent));

        self.track_window(GlideWindow::new(Window::new_wayland_window(surface)), parent);
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        tracing::info!("toplevel destroyed");
        self.untrack_window(surface.wl_surface());
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        let Some(id) = self.window_id_for_surface(surface.wl_surface()) else {
            return;
        };
        let Some(window) = self.window_map.get(&id) else {
            return;
        };
        let (title, _) = toplevel_title_and_app_id(surface.wl_surface());
        if let Some(title) = title.clone()
            && let Some(mut deco) = window.try_decoration_state_mut()
        {
            deco.header.set_title(title);
        }
        self.ipc_subscribers
            .broadcast(&glide_ipc::Event::TitleChanged { id: id.as_u64(), title: title.unwrap_or_default() });
        self.refresh_dock();
        self.queue_redraw();
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        let Some(id) = self.window_id_for_surface(surface.wl_surface()) else {
            return;
        };

        let app_id = toplevel_title_and_app_id(surface.wl_surface()).1.unwrap_or_default();
        if let Some(handle) = self.foreign_toplevel_handles.get(&id) {
            handle.send_app_id(&app_id);
            handle.send_done();
        }
        self.ipc_subscribers.broadcast(&glide_ipc::Event::AppIdChanged { id: id.as_u64(), app_id });

        if self.refresh_dock() {
            self.queue_redraw();
        }
    }

    fn parent_changed(&mut self, surface: ToplevelSurface) {
        let Some(id) = self.window_id_for_surface(surface.wl_surface()) else {
            return;
        };
        let Some(parent) = surface.parent().and_then(|parent| self.window_id_for_surface(&parent)) else {
            return;
        };
        let Some(target) = self.layout.window_state(parent).map(|s| s.workspace_id) else {
            return;
        };

        if self.layout.move_window_to_workspace(id, glide_layout::WorkspaceTarget::Id(target)) {
            self.reconcile_space();
        }
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        self.set_toplevel_maximized(&surface, true);
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        self.set_toplevel_maximized(&surface, false);
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, output: Option<WlOutput>) {
        let target = output
            .as_ref()
            .and_then(smithay::output::Output::from_resource)
            .and_then(|output| self.space.output_geometry(&output))
            .map(|area| glide_layout::Point::new(area.loc.x + area.size.w / 2, area.loc.y + area.size.h / 2));
        self.set_toplevel_state(&surface, |layout, id| layout.set_fullscreen_on(id, true, target));
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        self.set_toplevel_fullscreen(&surface, false);
    }

    fn minimize_request(&mut self, surface: ToplevelSurface) {
        let Some(id) = self.window_id_for_surface(surface.wl_surface()) else {
            return;
        };
        self.minimize_with_flight(id);
    }

    fn show_window_menu(
        &mut self, surface: ToplevelSurface, _seat: wl_seat::WlSeat, _serial: Serial, location: Point<i32, Logical>,
    ) {
        let Some(id) = self.window_id_for_surface(surface.wl_surface()) else {
            return;
        };
        let Some(window) = self.window_map.get(&id) else {
            return;
        };
        let Some(mut deco) = window.try_decoration_state_mut() else {
            return;
        };
        deco.header.open_menu_at_x(location.x);
        drop(deco);
        self.queue_redraw();
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(&mut self, surface: PopupSurface, positioner: PositionerState, token: u32) {
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
            let Some(pointer) = seat.get_pointer() else {
                return;
            };
            let Some((window, location, pending_restore)) = self.begin_move(wl_surface, pointer.current_location())
            else {
                return;
            };
            pointer.set_grab(
                self,
                MoveSurfaceGrab::new(start_data, window, location, pending_restore),
                serial,
                Focus::Clear,
            );
        } else if let Some(start_data) = check_touch_grab(&seat, wl_surface, serial) {
            let Some(touch) = seat.get_touch() else {
                return;
            };
            let Some((window, location, pending_restore)) = self.begin_move(wl_surface, start_data.location) else {
                return;
            };
            touch.set_grab(self, TouchMoveGrab::new(start_data, window, location, pending_restore), serial);
        }
    }

    fn resize_request(
        &mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial, edges: xdg_toplevel::ResizeEdge,
    ) {
        let seat: Seat<GlideState> =
            Seat::from_resource(&seat).expect("wl_seat resource is guaranteed live by protocol");
        let wl_surface = surface.wl_surface();

        if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
            let Some(pointer) = seat.get_pointer() else {
                return;
            };
            let Some((window, rect)) = self.begin_resize(&surface) else {
                return;
            };
            pointer.set_grab(
                self,
                ResizeSurfaceGrab::start(start_data, window, edges.into(), rect),
                serial,
                Focus::Clear,
            );
        } else if let Some(start_data) = check_touch_grab(&seat, wl_surface, serial) {
            let Some(touch) = seat.get_touch() else {
                return;
            };
            let Some((window, rect)) = self.begin_resize(&surface) else {
                return;
            };
            touch.set_grab(self, TouchResizeGrab::start(start_data, window, edges.into(), rect), serial);
        }
    }

    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        use smithay::desktop::{PopupKeyboardGrab, PopupPointerGrab, PopupUngrabStrategy};

        let Some(seat) = Seat::<Self>::from_resource(&seat) else {
            return;
        };
        let kind = PopupKind::Xdg(surface);
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        let Ok(mut grab) = self.popups.grab_popup(root, kind, &seat, serial) else {
            return;
        };
        if self.screen_locked() {
            grab.ungrab(PopupUngrabStrategy::All);
            return;
        }

        if let Some(keyboard) = seat.get_keyboard() {
            if keyboard.is_grabbed()
                && !(keyboard.has_grab(serial) || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            keyboard.set_focus(self, grab.current_grab(), serial);
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }
        if let Some(pointer) = seat.get_pointer() {
            if pointer.is_grabbed()
                && !(pointer.has_grab(serial)
                    || pointer.has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial())))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
    }
}

impl GlideState {
    fn begin_move(
        &mut self, surface: &WlSurface, location: Point<f64, Logical>,
    ) -> Option<(GlideWindow, Point<i32, Logical>, bool)> {
        self.finish_workspace_swipe();
        let window = self.mapped_window(surface)?;
        self.restore_for_move(&window, location);
        let initial = self.space.element_location(&window)?;
        let pending_restore = self.is_size_locked(&window);
        Some((window, initial, pending_restore))
    }

    fn begin_resize(&mut self, surface: &ToplevelSurface) -> Option<(GlideWindow, Rectangle<i32, Logical>)> {
        self.finish_workspace_swipe();
        let window = self.mapped_window(surface.wl_surface())?;
        let location = self.space.element_location(&window)?;
        if self.is_size_locked(&window) {
            return None;
        }
        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Resizing);
        });
        surface.send_pending_configure();
        Some((window.clone(), Rectangle::new(location, window.geometry().size)))
    }
}

fn root_surface(surface: &WlSurface) -> WlSurface {
    let mut root = surface.clone();
    while let Some(parent) = get_parent(&root) {
        root = parent;
    }
    root
}

fn check_touch_grab(
    seat: &Seat<GlideState>, surface: &WlSurface, serial: Serial,
) -> Option<smithay::input::touch::GrabStartData<GlideState>> {
    let touch = seat.get_touch()?;
    if !touch.has_grab(serial) {
        return None;
    }
    let start_data = touch.grab_start_data()?;
    let (focus, _) = start_data.focus.as_ref()?;
    (&root_surface(focus) == surface).then_some(start_data)
}

fn check_grab(
    seat: &Seat<GlideState>, surface: &WlSurface, serial: Serial,
) -> Option<PointerGrabStartData<GlideState>> {
    let pointer = seat.get_pointer()?;

    if !pointer.has_grab(serial) {
        return None;
    }

    let start_data = pointer.grab_start_data()?;

    let (focus, _) = start_data.focus.as_ref()?;
    let mut focus_root = focus.as_surface()?.clone();
    while let Some(parent) = get_parent(&focus_root) {
        focus_root = parent;
    }
    if &focus_root != surface {
        return None;
    }

    Some(start_data)
}

pub fn handle_commit(state: &mut GlideState, surface: &WlSurface) {
    let window = state.window_map.values().find(|w| window_surface(w) == surface).cloned();

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
            let (title, app_id) = toplevel_title_and_app_id(surface);
            let (title, app_id) = (title.unwrap_or_default(), app_id.unwrap_or_default());
            if let Some(handle) = state.foreign_toplevel_handles.get(&id)
                && (handle.title() != title || handle.app_id() != app_id)
            {
                handle.send_title(&title);
                handle.send_app_id(&app_id);
                handle.send_done();
            }

            if state.layout.update_window_size(id, layout_size(&window)) {
                state.begin_window_open_animation(id, &window);
                state.reconcile_space();
            } else {
                state.refresh_overview();
            }
        }
    }

    let popups = &mut state.popups;
    popups.commit(surface);
    if let Some(popup) = popups.find_popup(surface) {
        match popup {
            PopupKind::Xdg(ref xdg) => {
                if !xdg.is_initial_configure_sent()
                    && let Err(err) = xdg.send_configure()
                {
                    tracing::warn!(?err, "initial popup configure failed, dismissing the popup");
                    xdg.send_popup_done();
                }
            }
            PopupKind::InputMethod(ref _input_method) => {}
        }
    }
}

impl XdgDecorationHandler for GlideState {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        self.apply_decoration_mode(&toplevel, true);
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: DecorationMode) {
        self.apply_decoration_mode(&toplevel, mode == DecorationMode::ServerSide);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.apply_decoration_mode(&toplevel, true);
    }
}

impl KdeDecorationHandler for GlideState {
    fn kde_decoration_state(&self) -> &KdeDecorationState {
        &self.protocols.kde_decoration_state
    }

    fn request_mode(&mut self, surface: &WlSurface, decoration: &OrgKdeKwinServerDecoration, mode: WEnum<KdeMode>) {
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
    fn apply_decoration_mode(&mut self, toplevel: &ToplevelSurface, server_side: bool) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode =
                Some(if server_side { DecorationMode::ServerSide } else { DecorationMode::ClientSide });
        });
        toplevel.send_pending_configure();
        if let Some(id) = self.window_id_for_surface(toplevel.wl_surface()) {
            self.set_window_decorated(id, server_side);
        }
    }

    fn set_toplevel_maximized(&mut self, surface: &ToplevelSurface, maximized: bool) {
        self.set_toplevel_state(surface, |layout, id| layout.set_maximized(id, maximized));
    }

    fn set_toplevel_fullscreen(&mut self, surface: &ToplevelSurface, fullscreen: bool) {
        self.set_toplevel_state(surface, |layout, id| layout.set_fullscreen(id, fullscreen));
    }

    fn set_toplevel_state(
        &mut self, surface: &ToplevelSurface,
        apply: impl FnOnce(&mut glide_layout::Layout, glide_layout::WindowId) -> bool,
    ) {
        let Some(id) = self.window_id_for_surface(surface.wl_surface()) else {
            return;
        };
        if self.can_morph_state_change(id, surface) {
            let mut changed = false;
            self.morph_geometry(id, |state| changed = apply(&mut state.layout, id));
            if !changed {
                surface.send_configure();
            }
            return;
        }
        let changed = apply(&mut self.layout, id);
        self.answer_state_request(surface, changed);
    }

    fn can_morph_state_change(&self, id: glide_layout::WindowId, surface: &ToplevelSurface) -> bool {
        surface.is_initial_configure_sent()
            && self.layout.is_visible(id)
            && self.layout.window_state(id).is_some_and(|state| state.is_placed)
            && self.window_map.get(&id).is_some_and(|window| self.space.element_location(window).is_some())
    }

    fn answer_state_request(&mut self, surface: &ToplevelSurface, changed: bool) {
        if changed {
            self.reconcile_space();
        } else if surface.is_initial_configure_sent() {
            surface.send_configure();
        }
    }

    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(window_geo) = self.mapped_window(&root).and_then(|window| self.space.element_geometry(&window)) else {
            return;
        };

        let output = self
            .space
            .outputs()
            .find(|o| self.space.output_geometry(o).is_some_and(|geo| geo.contains(window_geo.loc)))
            .or_else(|| self.space.outputs().next());

        let Some(output) = output else {
            return;
        };
        let output_geo =
            self.space.output_geometry(output).expect("output came from space.outputs(), so it has a geometry");

        let mut target = output_geo;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}

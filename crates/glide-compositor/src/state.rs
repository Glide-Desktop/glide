use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    sync::Arc,
};

use smithay::{
    desktop::{PopupManager, Space, WindowSurfaceType},
    input::{
        Seat, SeatState,
        keyboard::{Keycode, XkbConfig},
        pointer::CursorImageStatus,
    },
    output::Output,
    reexports::{
        calloop::{Interest, LoopHandle, LoopSignal, Mode, PostAction, generic::Generic},
        wayland_protocols::xdg::shell::server::xdg_toplevel::State as XdgState,
        wayland_server::{
            Display, DisplayHandle,
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
        },
    },
    utils::{Logical, Point},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::xdg::{ToplevelState, XdgShellState, dialog::XdgDialogState},
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use crate::{backend::tty::TtyData, window::GlideWindow};

pub struct GlideState {
    pub backend_data: TtyData,

    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,

    pub space: Space<GlideWindow>,
    pub loop_signal: LoopSignal,
    /// Used by backends (currently just tty) to schedule follow-up work from
    /// inside event source callbacks, e.g. a repaint timer after a `VBlank`.
    pub loop_handle: LoopHandle<'static, Self>,

    // Smithay State
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    /// `xdg-dialog-v1`: lets clients mark a toplevel as a (modal) dialog, so
    /// it can be kept with its parent instead of exiled to a new workspace.
    pub xdg_dialog_state: XdgDialogState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<GlideState>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,

    pub seat: Seat<Self>,

    pub layout: glide_layout::Layout,
    pub window_map: HashMap<glide_layout::WindowId, GlideWindow>,
    pub window_to_id: HashMap<GlideWindow, glide_layout::WindowId>,
    /// Stable layout ids for outputs.
    ///
    /// Allocated once per physical output and reused, so an output keeps its
    /// identity across rescans. Minting a fresh id on every sync would make
    /// the layout treat one monitor as an endless series of new ones.
    pub output_ids: HashMap<Output, glide_layout::OutputId>,
    pub keybinds: Vec<crate::input::ResolvedKeybind>,

    /// What the pointer cursor should currently look like. Clients set this
    /// via `wl_pointer.set_cursor` when the pointer is over them; otherwise
    /// it stays at the themed default.
    pub cursor_status: CursorImageStatus,

    /// Keys currently suppressed (not forwarded to the focused client)
    /// because their press triggered a compositor keybinding. Tracked so the
    /// matching release is suppressed too, avoiding "stuck key" bugs.
    ///
    /// Keyed by *keycode* rather than keysym on purpose: the keysym of a
    /// release can differ from that of its own press once modifiers are
    /// let go first (e.g. Ctrl+Alt+F2 presses as `XF86Switch_VT_2` but
    /// releases as plain `F2`), which would leak an unpaired release.
    pub suppressed_keys: Vec<Keycode>,
}

impl GlideState {
    /// `loop_handle` and `loop_signal` are taken already-extracted from the
    /// caller's `EventLoop` (rather than `&mut EventLoop<Self>` itself) on
    /// purpose: `EventLoop<'l, Data>` carries an explicit lifetime since
    /// calloop 0.14, and threading `&mut EventLoop<Self>` through here would
    /// force this function to be generic over an arbitrary caller-supplied
    /// `'l` that can never be proven to satisfy `LoopHandle`'s `'static`
    /// bound. Calling `.handle()`/`.get_signal()` at the call site instead
    /// lets normal inference resolve the event loop's own lifetime to
    /// `'static` directly (this is also the pattern anvil's `AnvilState::init`
    /// uses).
    ///
    /// # Panics
    ///
    /// Panics if the default XKB keymap fails to compile, if a `wayland-N`
    /// socket cannot be created in `XDG_RUNTIME_DIR`, or if either event
    /// source fails to register with the event loop. None of these are
    /// expected to fail under normal operation.
    #[must_use]
    pub fn new(
        backend_data: TtyData,
        loop_handle: LoopHandle<'static, Self>,
        loop_signal: LoopSignal,
        display: Display<Self>,
    ) -> Self {
        let start_time = std::time::Instant::now();

        let dh = display.handle();

        // Here we initialize implementations of some wayland protocols
        // Some of them require us to implement traits on the GlideState state,
        // you can find those implementations in the `crate::handlers` module

        // Initialize protocols needed for displaying windows
        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let xdg_dialog_state = XdgDialogState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let popups = PopupManager::default();

        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);

        // Data device is responsible for clipboard and drag-and-drop
        let data_device_state = DataDeviceState::new::<Self>(&dh);

        // A seat is a group of keyboards, pointer and touch devices.
        // A seat typically has a pointer and maintains a keyboard focus and a pointer focus.
        let seat_name = backend_data.seat_name();
        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, seat_name);

        // Notify clients that we have a keyboard. Real hardware hot-plug
        // (adding/removing keyboards after startup) is out of scope for now;
        // we assume one is always present, matching the libinput/DRM tty
        // backend's own current lack of hot-plug handling.
        seat.add_keyboard(XkbConfig::default(), 200, 25)
            .expect("the default XKB keymap should always compile");

        // Notify clients that we have a pointer (mouse)
        // Here we assume that there is always pointer plugged in
        seat.add_pointer();

        // A space represents a two-dimensional plane. Windows and Outputs can be mapped onto it.
        //
        // Windows get a position and stacking order through mapping.
        // Outputs become views of a part of the Space and can be rendered via Space::render_output.
        let space = Space::default();

        // Setup a wayland socket that will be used to accept clients
        let socket_name = Self::init_wayland_listener(display, &loop_handle);

        Self {
            backend_data,

            start_time,
            display_handle: dh,

            space,
            loop_signal,
            loop_handle,
            socket_name,

            compositor_state,
            xdg_shell_state,
            xdg_dialog_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            popups,
            seat,
            layout: glide_layout::Layout::new(),
            window_map: HashMap::new(),
            window_to_id: HashMap::new(),
            output_ids: HashMap::new(),
            keybinds: Vec::new(),
            cursor_status: CursorImageStatus::default_named(),
            suppressed_keys: Vec::new(),
        }
    }

    fn init_wayland_listener(
        display: Display<Self>,
        loop_handle: &LoopHandle<'static, Self>,
    ) -> OsString {
        // Creates a new listening socket, automatically choosing the next available `wayland` socket name.
        let listening_socket = ListeningSocketSource::new_auto()
            .expect("failed to create a wayland-N socket in XDG_RUNTIME_DIR");

        // Get the name of the listening socket.
        // Clients will connect to this socket.
        let socket_name = listening_socket.socket_name().to_os_string();

        loop_handle
            .insert_source(listening_socket, move |client_stream, (), state| {
                // Inside the callback, you should insert the client into the display.
                //
                // You may also associate some data with the client when inserting the client.
                state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .expect("a freshly accepted client stream is always a valid new client");
            })
            .expect("failed to register the wayland listening socket with the event loop");

        // You also need to add the display itself to the event loop, so that client events will be processed by wayland-server.
        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
                    // SAFETY: this callback owns the only reference to `display`
                    // (moved into the closure by `Generic::new`) for the
                    // lifetime of this event source, so it is never dropped
                    // out from under `get_mut()`.
                    unsafe {
                        display.get_mut().dispatch_clients(state).expect(
                            "client protocol dispatch should not fail under normal operation",
                        );
                    }
                    Ok(PostAction::Continue)
                },
            )
            .expect("failed to register the wayland display with the event loop");

        socket_name
    }

    /// Makes Smithay's [`Space`] match what [`glide_layout::Layout`] says
    /// should be on screen.
    ///
    /// The layout engine is the single source of truth; this walks its
    /// decisions and applies them. It is deliberately idempotent, so callers
    /// can just run it after any state change without tracking deltas.
    ///
    /// # Panics
    ///
    /// Panics if no keyboard was added to the seat; `GlideState::new` always
    /// adds one, so in practice this never fires.
    pub fn reconcile_space(&mut self) {
        // Windows on the active workspace, bottom-to-top. The order matters:
        // `map_element` re-stacks, so applying them in this sequence is what
        // gives the workspace its Z-order.
        let visible: Vec<glide_layout::WindowState> =
            self.layout.visible_windows().cloned().collect();
        let visible_ids: HashSet<glide_layout::WindowId> =
            visible.iter().map(|state| state.id).collect();

        // Drop anything the layout no longer shows. Unmapping (rather than
        // moving off-screen) makes Smithay send `wl_surface.leave`, which is
        // what stops a client on a hidden workspace from rendering.
        let mapped: Vec<GlideWindow> = self.space.elements().cloned().collect();
        for window in mapped {
            let hidden = self
                .window_to_id
                .get(&window)
                .is_none_or(|id| !visible_ids.contains(id));
            if hidden {
                self.space.unmap_elem(&window);
            }
        }

        let focused = self.layout.focused_window();
        for state in &visible {
            let Some(window) = self.window_map.get(&state.id).cloned() else {
                continue;
            };

            // Space positions the client's own surface, which is the
            // *content* area — decorations (M4.3+) are drawn by us, offset
            // from this by the window's insets, not part of what Space or
            // the client itself ever sees as its location.
            let content = state.content_geometry();
            let location = Point::from((content.loc.x, content.loc.y));
            self.space.map_element(window.clone(), location, false);
            Self::apply_window_state(&window, state, focused == Some(state.id));
        }

        self.apply_keyboard_focus();
    }

    /// Pushes activation and maximized/fullscreen state (and, only for those,
    /// an explicit size) to the client.
    ///
    /// A plain floating window is deliberately left to size itself: Glide
    /// positions windows but does not dictate their dimensions, so sending a
    /// size here would both override the client's own choice and fight any
    /// resize it initiates. Maximized and fullscreen are the exceptions,
    /// because there the compositor genuinely is choosing the geometry — and
    /// what it tells the client is the *content* size, insets already
    /// excluded, since the client only ever draws its own content area.
    fn apply_window_state(window: &GlideWindow, state: &glide_layout::WindowState, focused: bool) {
        let toplevel = window
            .toplevel()
            .expect("only Window::new_wayland_window is used, so toplevel() is always Some");

        let mut changed = false;
        toplevel.with_pending_state(|pending| {
            // Drives the client's own focused styling (active title bar,
            // visible text cursor). Derived from the layout's focus so there
            // is exactly one answer to "which window is active".
            changed |= set_state_flag(pending, XdgState::Activated, focused);
            changed |= set_state_flag(pending, XdgState::Maximized, state.maximized);
            changed |= set_state_flag(pending, XdgState::Fullscreen, state.fullscreen);

            let content = state.content_geometry();
            let size = (state.maximized || state.fullscreen)
                .then(|| smithay::utils::Size::from((content.size.width, content.size.height)));
            if pending.size != size {
                pending.size = size;
                changed = true;
            }
        });

        if changed {
            toplevel.send_pending_configure();
        }
    }

    /// Points the keyboard at whatever the layout says is focused.
    fn apply_keyboard_focus(&mut self) {
        let target = self
            .layout
            .focused_window()
            .and_then(|id| self.window_map.get(&id))
            .map(|window| window_surface(window).clone());

        // Smithay no-ops when the focus is unchanged, so calling this on every
        // reconcile does not spam clients with enter/leave pairs.
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let keyboard = self
            .seat
            .get_keyboard()
            .expect("a keyboard was added in GlideState::new");
        keyboard.set_focus(self, target, serial);
    }

    /// Registers a newly mapped toplevel with the layout engine.
    ///
    /// Returns the id the layout assigned to it.
    pub fn track_window(&mut self, window: GlideWindow, parent: Option<glide_layout::WindowId>) {
        let id = glide_layout::WindowId::next();
        let size = layout_size(&window);

        if let Some(parent) = parent {
            self.layout.add_window_with_parent(id, size, parent);
        } else {
            self.layout.add_window(id, size);
        }

        self.window_map.insert(id, window.clone());
        self.window_to_id.insert(window, id);
        self.reconcile_space();
    }

    /// Removes a window from the layout and the scene.
    ///
    /// Driven by `toplevel_destroyed` rather than by scanning for dead
    /// surfaces, so a closed window's workspace is torn down immediately
    /// instead of lingering until something else happens to reconcile.
    pub fn untrack_window(&mut self, surface: &WlSurface) {
        let Some((&id, window)) = self
            .window_map
            .iter()
            .find(|(_, window)| window_surface(window) == surface)
        else {
            return;
        };

        let window = window.clone();
        self.space.unmap_elem(&window);
        self.window_to_id.remove(&window);
        self.window_map.remove(&id);
        self.layout.remove_window(id);

        self.reconcile_space();
    }

    /// Layout id for a mapped window, if it is tracked.
    #[must_use]
    pub fn window_id(&self, window: &GlideWindow) -> Option<glide_layout::WindowId> {
        self.window_to_id.get(window).copied()
    }

    /// Layout id for the window backing `surface`, if any.
    #[must_use]
    pub fn window_id_for_surface(&self, surface: &WlSurface) -> Option<glide_layout::WindowId> {
        self.window_map
            .iter()
            .find(|(_, window)| window_surface(window) == surface)
            .map(|(&id, _)| id)
    }

    /// Focuses a window through the layout engine and applies the result.
    ///
    /// The only sanctioned way to change focus. Setting keyboard focus
    /// directly would desync the layout, leaving keybindings acting on a
    /// different window than the one the user sees as active.
    pub fn focus_window(&mut self, id: glide_layout::WindowId) {
        if self.layout.focus_window(id) {
            self.reconcile_space();
        }
    }

    /// Tells the layout engine which outputs currently exist.
    ///
    /// Must be called after any change to the set of mapped outputs — both
    /// connect and disconnect — because the layout uses them to place windows
    /// and to rescue ones that a departing monitor would otherwise strand
    /// off-screen.
    pub fn sync_layout_outputs(&mut self) {
        let outputs: Vec<Output> = self.space.outputs().cloned().collect();

        // Forget outputs that are gone, so their ids are not held forever.
        self.output_ids.retain(|output, _| outputs.contains(output));

        let infos: Vec<glide_layout::OutputInfo> = outputs
            .into_iter()
            .map(|output| {
                let geometry = self.space.output_geometry(&output).unwrap_or_default();
                let id = *self
                    .output_ids
                    .entry(output)
                    .or_insert_with(glide_layout::OutputId::next);

                let rect = glide_layout::Rectangle::from_loc_and_size(
                    geometry.loc.x,
                    geometry.loc.y,
                    geometry.size.w,
                    geometry.size.h,
                );

                glide_layout::OutputInfo {
                    id,
                    geometry: rect,
                    // No layer-shell surfaces to reserve space for yet, so
                    // the usable area is the whole output. `arrange_layers`
                    // (M4.6) will shrink this by each panel's exclusive zone.
                    work_area: rect,
                }
            })
            .collect();

        tracing::info!(outputs = infos.len(), "layout outputs synchronized");
        self.layout.outputs_changed(&infos);
    }

    /// Records an interactive move so the layout agrees with what is on screen.
    ///
    /// Interactive move and resize write straight into [`Space`] for latency
    /// (a drag has to track the pointer frame by frame), so the layout has to
    /// be told after the fact — otherwise the next reconcile would snap the
    /// window back to its pre-drag position.
    ///
    /// `content_location` is where the grab placed the window's *surface* in
    /// `Space` — the content, not the frame (see [`Self::reconcile_space`]).
    /// Converted back to a frame location before being stored, since that is
    /// what [`glide_layout::WindowState::geometry`] represents.
    pub fn record_window_move(
        &mut self,
        window: &GlideWindow,
        content_location: Point<i32, Logical>,
    ) {
        let Some(id) = self.window_id(window) else {
            return;
        };
        let Some(state) = self.layout.window_state(id) else {
            return;
        };

        let insets = state.insets;
        let mut geometry = state.geometry;
        geometry.loc = glide_layout::Point::new(
            content_location.x - insets.left,
            content_location.y - insets.top,
        );
        self.layout.set_window_geometry(id, geometry);
    }

    /// Records an interactive resize. See [`Self::record_window_move`].
    pub fn record_window_resize(&mut self, window: &GlideWindow) {
        let Some(id) = self.window_id(window) else {
            return;
        };
        let Some(content_location) = self.space.element_location(window) else {
            return;
        };
        let Some(state) = self.layout.window_state(id) else {
            return;
        };

        let insets = state.insets;
        let content_size = layout_size(window);
        let frame_loc = glide_layout::Point::new(
            content_location.x - insets.left,
            content_location.y - insets.top,
        );
        let frame_size = glide_layout::Size::new(
            content_size.width + insets.width(),
            content_size.height + insets.height(),
        );
        self.layout
            .set_window_geometry(id, glide_layout::Rectangle::new(frame_loc, frame_size));
    }

    /// Clears focus (a click on the desktop background).
    ///
    /// Routed through the layout like every other focus change, so the
    /// windows' `Activated` state is updated by the reconcile that follows.
    pub fn clear_focus(&mut self) {
        self.layout.clear_focus();
        self.reconcile_space();
    }

    #[must_use]
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(s, p)| (s, (p + location).to_f64()))
            })
    }
}

/// Sets or clears one `xdg_toplevel` state flag, reporting whether it changed.
///
/// Returning "did anything change" is what lets the caller send a configure
/// only when there is something to configure; a configure per reconcile would
/// have clients re-rendering constantly.
fn set_state_flag(pending: &mut ToplevelState, flag: XdgState, wanted: bool) -> bool {
    if wanted == pending.states.contains(flag) {
        return false;
    }

    if wanted {
        pending.states.set(flag);
    } else {
        pending.states.unset(flag);
    }
    true
}

/// A window's current size in the layout engine's own units.
///
/// Zero until the client has attached a buffer, which is exactly what
/// [`glide_layout::Layout`] uses to defer placement: a window cannot be
/// centred before it has a size.
pub(crate) fn layout_size(window: &GlideWindow) -> glide_layout::Size {
    let size = window.geometry().size;
    glide_layout::Size::new(size.w, size.h)
}

/// The `wl_surface` backing a [`Window`]'s toplevel.
///
/// [`Window::new_wayland_window`] is the only constructor used anywhere in
/// Glide (there is no `XWayland` support), so [`Window::toplevel`] is always
/// `Some`. Centralized here rather than repeating `.toplevel().unwrap()` at
/// every call site (handlers, grabs, input) so that adding a second kind of
/// window later only needs updating this one assumption.
pub(crate) fn window_surface(window: &GlideWindow) -> &WlSurface {
    window
        .toplevel()
        .expect("only Window::new_wayland_window is used, so toplevel() is always Some")
        .wl_surface()
}

/// Data associated with a wayland client that connects to `GlideState`.
/// One instance of this type per client.
#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

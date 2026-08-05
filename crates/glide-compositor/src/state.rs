use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    sync::Arc,
};

use smithay::{
    desktop::{LayerSurface, PopupManager, Space, WindowSurfaceType, layer_map_for_output},
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
    utils::{IsAlive, Logical, Point, Rectangle},
    wayland::{
        compositor::{CompositorClientState, CompositorState, with_states},
        cursor_shape::CursorShapeManagerState,
        fractional_scale::{
            FractionalScaleHandler, FractionalScaleManagerState, with_fractional_scale,
        },
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::kde::decoration::KdeDecorationState,
        shell::wlr_layer::{Layer as WlrLayer, WlrLayerShellState},
        shell::xdg::{
            ToplevelState, XdgShellState, XdgToplevelSurfaceData, decoration::XdgDecorationState,
            dialog::XdgDialogState,
        },
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
    /// `cursor-shape-v1`: lets clients name the cursor they want ("text",
    /// "grab", ...) instead of uploading their own surface for it, so the
    /// pointer keeps one consistent look across applications and matches
    /// the shapes our own decorations use.
    pub cursor_shape_state: CursorShapeManagerState,
    /// `xdg-decoration`: how clients negotiate who draws window decorations.
    /// Glide prefers server-side (its whole decoration stack exists for
    /// that), but honors a client that insists on drawing its own.
    pub xdg_decoration_state: XdgDecorationState,
    /// The KDE flavor of the same negotiation, for clients that predate (or
    /// don't speak) `xdg-decoration`.
    pub kde_decoration_state: KdeDecorationState,
    /// `wlr-layer-shell`: panels, launchers, wallpapers, notifications.
    pub layer_shell_state: WlrLayerShellState,
    /// `fractional-scale-v1`: lets clients render at non-integer scales.
    /// Glide reports 1.0 for now (nothing sets another output scale yet),
    /// but advertising the protocol keeps scale-aware clients on their
    /// crisp path once that changes.
    pub fractional_scale_state: FractionalScaleManagerState,
    /// `viewporter`: lets clients crop/scale their own buffers. Fully
    /// handled inside Smithay's renderer — no compositor code beyond the
    /// global.
    pub viewporter_state: smithay::wayland::viewporter::ViewporterState,
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
    /// Current decoration colors/height, applied to every window mapped
    /// after a config (re)load. See [`crate::config::Config::apply_to_state`].
    pub decoration_theme: crate::config::DecorationTheme,
    /// Overview look and feel, refreshed on config reload alongside the
    /// decoration theme.
    pub overview_theme: crate::config::OverviewTheme,

    /// What the pointer cursor should currently look like. Clients set this
    /// via `wl_pointer.set_cursor` when the pointer is over them; otherwise
    /// it stays at the themed default.
    pub cursor_status: CursorImageStatus,

    /// A layer surface currently holding keyboard focus (e.g. a launcher
    /// the user clicked into). Checked by `apply_keyboard_focus` ahead of
    /// the layout's focused window: layer surfaces are not windows, so the
    /// layout knows nothing about them, and without this every reconcile
    /// would yank the keyboard back to a window mid-typing.
    pub layer_focus: Option<LayerSurface>,

    /// Live overview state, present exactly while the overview is on
    /// screen (including its closing animation).
    ///
    /// Kept beside `layout.overview_active()` rather than inside the layout
    /// engine because it is all presentation: cached pixel buffers, an
    /// animation clock, the hovered preview. The layout owns the *fact*
    /// that the overview is open; this owns what that looks like.
    pub overview: Option<crate::overview::OverviewUi>,

    /// Keys currently suppressed (not forwarded to the focused client)
    /// because their press triggered a compositor keybinding. Tracked so the
    /// matching release is suppressed too, avoiding "stuck key" bugs.
    ///
    /// Keyed by *keycode* rather than keysym on purpose: the keysym of a
    /// release can differ from that of its own press once modifiers are
    /// let go first (e.g. Ctrl+Alt+F2 presses as `XF86Switch_VT_2` but
    /// releases as plain `F2`), which would leak an unpaired release.
    pub suppressed_keys: Vec<Keycode>,

    // M5 Protocol States
    pub foreign_toplevel_list_state:
        smithay::wayland::foreign_toplevel_list::ForeignToplevelListState,
    pub foreign_toplevel_handles: HashMap<
        glide_layout::WindowId,
        smithay::wayland::foreign_toplevel_list::ForeignToplevelHandle,
    >,
    pub idle_notifier_state: smithay::wayland::idle_notify::IdleNotifierState<GlideState>,
    pub idle_inhibit_state: smithay::wayland::idle_inhibit::IdleInhibitManagerState,
    pub idle_inhibited_surfaces: HashSet<WlSurface>,
    pub wlr_output_manager_state: crate::handlers::wlr_output_management::WlrOutputManagerState,
    pub ext_workspace_state: crate::handlers::ext_workspace::ExtWorkspaceManagerState,
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
        let cursor_shape_state = CursorShapeManagerState::new::<Self>(&dh);
        let xdg_decoration_state = XdgDecorationState::new::<Self>(&dh);
        let kde_decoration_state = KdeDecorationState::new::<Self>(
            &dh,
            smithay::reexports::wayland_protocols_misc::server_decoration::server
                ::org_kde_kwin_server_decoration_manager::Mode::Server,
        );
        let layer_shell_state = WlrLayerShellState::new::<Self>(&dh);
        let fractional_scale_state = FractionalScaleManagerState::new::<Self>(&dh);
        let viewporter_state = smithay::wayland::viewporter::ViewporterState::new::<Self>(&dh);
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

        // M5 Protocol Initializations
        let foreign_toplevel_list_state =
            smithay::wayland::foreign_toplevel_list::ForeignToplevelListState::new::<Self>(&dh);
        let foreign_toplevel_handles = HashMap::new();
        let idle_notifier_state =
            smithay::wayland::idle_notify::IdleNotifierState::new(&dh, loop_handle.clone());
        let idle_inhibit_state =
            smithay::wayland::idle_inhibit::IdleInhibitManagerState::new::<Self>(&dh);
        let wlr_output_manager_state =
            crate::handlers::wlr_output_management::WlrOutputManagerState::new::<Self>(&dh);
        let ext_workspace_state =
            crate::handlers::ext_workspace::ExtWorkspaceManagerState::new::<Self>(&dh);

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
            cursor_shape_state,
            xdg_decoration_state,
            kde_decoration_state,
            layer_shell_state,
            fractional_scale_state,
            viewporter_state,
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
            decoration_theme: crate::config::DecorationTheme::default(),
            overview_theme: crate::config::OverviewTheme::default(),
            cursor_status: CursorImageStatus::default_named(),
            layer_focus: None,
            overview: None,
            suppressed_keys: Vec::new(),

            foreign_toplevel_list_state,
            foreign_toplevel_handles,
            idle_notifier_state,
            idle_inhibit_state,
            idle_inhibited_surfaces: HashSet::new(),
            wlr_output_manager_state,
            ext_workspace_state,
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
        self.cleanup_dead_windows();

        // Keep the overview's slots in step with the window set. Cheap
        // when it is closed (an early return), and this is the one place
        // every window add/remove/minimise funnels through, so hooking it
        // here beats remembering to call it from each of them.
        self.refresh_overview();

        // Windows on the active workspace, bottom-to-top. The order matters:
        // `map_element` re-stacks, so applying them in this sequence is what
        // gives the workspace its Z-order.
        let visible: Vec<glide_layout::WindowState> = self.layout.visible_windows().collect();
        let visible_ids: HashSet<glide_layout::WindowId> =
            visible.iter().map(|state| state.id).collect();

        let focused = self.layout.focused_window();

        // ── Unmap hidden windows ───────────────────────────────────
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

        // ── Map visible windows ────────────────────────────────────
        for state in &visible {
            let Some(window) = self.window_map.get(&state.id).cloned() else {
                continue;
            };

            let is_focused = focused == Some(state.id);

            let content = state.content_geometry();
            let location = Point::from((content.loc.x, content.loc.y));

            // Space positions the client's own surface, which is the
            // *content* area — decorations are drawn by us, offset from
            // this by the window's insets, not part of what Space or the
            // client itself ever sees as its location.
            self.space.map_element(window.clone(), location, false);
            Self::apply_window_state(&window, state, is_focused);

            // Keeps titlebar colors and height in sync with config reloads
            // and focus changes — see `GlideWindow::sync_decoration`'s docs
            // for why this has to happen here rather than at render time.
            //
            // Fullscreen shows no decoration at all (mirroring how
            // `content_geometry` ignores insets for it): without forcing the
            // height to 0 here, the titlebar would still hit-test and render
            // in the strip above the output's top edge.
            let decoration_height = if state.fullscreen {
                0
            } else {
                state.insets.top
            };
            window.sync_decoration(&self.decoration_theme, decoration_height, is_focused);
        }

        self.apply_keyboard_focus();
    }

    /// Scans for and removes any dead window instances (e.g. from ungraceful
    /// client socket disconnections) from `window_map`, `window_to_id`, `space`,
    /// and `layout`.
    pub fn cleanup_dead_windows(&mut self) {
        let dead: Vec<(glide_layout::WindowId, GlideWindow)> = self
            .window_map
            .iter()
            .filter(|(_, window)| !window.alive())
            .map(|(&id, window)| (id, window.clone()))
            .collect();

        if dead.is_empty() {
            return;
        }

        for (id, window) in dead {
            self.space.unmap_elem(&window);
            self.window_to_id.remove(&window);
            self.window_map.remove(&id);
            self.layout.remove_window(id);
            if let Some(handle) = self.foreign_toplevel_handles.remove(&id) {
                handle.send_closed();
                self.foreign_toplevel_list_state.remove_toplevel(&handle);
            }
        }

        self.idle_inhibited_surfaces
            .retain(smithay::utils::IsAlive::alive);
        if self.idle_inhibited_surfaces.is_empty() {
            self.idle_notifier_state.set_is_inhibited(false);
        }

        self.suppressed_keys.clear();
        self.ext_workspace_state
            .sync_workspaces(&self.display_handle, &self.layout);
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
        let Some(toplevel) = window.toplevel() else {
            return;
        };

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

    /// Points the keyboard at whatever should have it: a focused layer
    /// surface first (launchers and the like — see [`Self::layer_focus`]),
    /// otherwise the layout's focused window.
    fn apply_keyboard_focus(&mut self) {
        // A dead layer surface must not pin focus forever: the client may
        // have exited without any event that would otherwise clear this.
        if self.layer_focus.as_ref().is_some_and(|l| !l.alive()) {
            self.layer_focus = None;
        }

        let target = self
            .layer_focus
            .as_ref()
            .map(|layer| layer.wl_surface().clone())
            .or_else(|| {
                self.layout
                    .focused_window()
                    .and_then(|id| self.window_map.get(&id))
                    .map(|window| window_surface(window).clone())
            });

        // Smithay no-ops when the focus is unchanged, so calling this on every
        // reconcile does not spam clients with enter/leave pairs.
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let keyboard = self
            .seat
            .get_keyboard()
            .expect("a keyboard was added in GlideState::new");

        let current_focus = keyboard.current_focus();
        if current_focus != target {
            self.suppressed_keys.clear();
        }

        keyboard.set_focus(self, target, serial);
    }

    /// Registers a newly mapped toplevel with the layout engine.
    ///
    /// Returns the id the layout assigned to it.
    #[expect(
        clippy::missing_panics_doc,
        clippy::needless_pass_by_value,
        reason = "single-threaded event loop mutexes are never poisoned; window ownership matches track_window caller design"
    )]
    pub fn track_window(&mut self, window: GlideWindow, parent: Option<glide_layout::WindowId>) {
        let id = glide_layout::WindowId::next();
        let size = layout_size(&window);

        if let Some(parent) = parent {
            self.layout.add_window_with_parent(id, size, parent);
        } else {
            self.layout.add_window(id, size);
        }

        // By default, Wayland clients are responsible for drawing their own
        // decorations (ClientSide). We only decorate them if they explicitly
        // negotiate ServerSide via the xdg-decoration protocol (handled in
        // `request_mode`).
        self.layout
            .set_window_insets(id, glide_layout::Insets::NONE);

        self.window_map.insert(id, window.clone());
        self.window_to_id.insert(window.clone(), id);

        let (title, app_id) = window.toplevel().map_or_else(
            || (String::new(), String::new()),
            |toplevel| {
                with_states(toplevel.wl_surface(), |states| {
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
                })
            },
        );
        let handle = self
            .foreign_toplevel_list_state
            .new_toplevel::<Self>(title, app_id);
        self.foreign_toplevel_handles.insert(id, handle);

        self.reconcile_space();
        self.ext_workspace_state
            .sync_workspaces(&self.display_handle, &self.layout);
    }

    /// Removes a window from the layout and the scene.
    ///
    /// Driven by `toplevel_destroyed` rather than by scanning for dead
    /// surfaces, so a closed window's workspace is torn down immediately
    /// instead of lingering until something else happens to reconcile.
    pub fn untrack_window(&mut self, surface: &WlSurface) {
        let id_opt = self
            .window_map
            .iter()
            .find(|(_, window)| window_surface(window) == surface)
            .map(|(&id, _)| id);

        if let Some(id) = id_opt {
            if let Some(window) = self.window_map.remove(&id) {
                self.space.unmap_elem(&window);
                self.window_to_id.remove(&window);
                self.layout.remove_window(id);
                if let Some(handle) = self.foreign_toplevel_handles.remove(&id) {
                    handle.send_closed();
                    self.foreign_toplevel_list_state.remove_toplevel(&handle);
                }
            }
        }

        self.cleanup_dead_windows();
        self.reconcile_space();
        self.ext_workspace_state
            .sync_workspaces(&self.display_handle, &self.layout);
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

    /// The layer surface backing `surface`, if it is one, on any output.
    #[must_use]
    pub fn layer_for_surface(&self, surface: &WlSurface) -> Option<LayerSurface> {
        self.space.outputs().find_map(|output| {
            layer_map_for_output(output)
                .layer_for_surface(surface, WindowSurfaceType::ALL)
                .cloned()
        })
    }

    /// A mapped `Top`/`Overlay` layer surface demanding exclusive keyboard
    /// focus, if any. Checked per key event rather than tracked as state:
    /// exclusivity is the layer's own (double-buffered) property and can
    /// change on any commit, so a snapshot would inevitably go stale.
    #[must_use]
    pub fn exclusive_layer_surface(&self) -> Option<LayerSurface> {
        self.space.outputs().find_map(|output| {
            let map = layer_map_for_output(output);
            map.layers()
                .rev()
                .find(|layer| {
                    matches!(layer.layer(), WlrLayer::Top | WlrLayer::Overlay)
                        && layer.layer_surface().with_cached_state(|state| {
                            state.keyboard_interactivity
                                == smithay::wayland::shell::wlr_layer::KeyboardInteractivity::Exclusive
                        })
                })
                .cloned()
        })
    }

    /// Public wrapper over [`Self::apply_keyboard_focus`] for the input
    /// path, which needs to push a just-acquired layer focus to the seat
    /// before dispatching the key that triggered it.
    pub fn apply_keyboard_focus_public(&mut self) {
        self.apply_keyboard_focus();
    }

    // ── Overview ───────────────────────────────────────────────────

    /// Frame rectangles of every window the overview shows, in overview
    /// order — what [`crate::overview::OverviewUi`] arranges and animates
    /// from.
    fn overview_entries(&self) -> Vec<(glide_layout::WindowId, Rectangle<i32, Logical>)> {
        self.layout
            .overview_windows()
            .into_iter()
            .filter(|state| self.window_map.contains_key(&state.id))
            .map(|state| {
                let frame = state.frame_geometry();
                (
                    state.id,
                    Rectangle::new(
                        Point::from((frame.loc.x, frame.loc.y)),
                        smithay::utils::Size::from((frame.size.width, frame.size.height)),
                    ),
                )
            })
            .collect()
    }

    /// The area previews are arranged in: the primary output's work area,
    /// so panels keep their space in the overview too.
    fn overview_area(&self) -> Rectangle<i32, Logical> {
        let area = self.layout.primary_output_work_area();
        Rectangle::new(
            Point::from((area.loc.x, area.loc.y)),
            smithay::utils::Size::from((area.size.width, area.size.height)),
        )
    }

    /// Opens the overview. No-op if it is already showing.
    pub fn open_overview(&mut self) {
        if !self.layout.open_overview() {
            return;
        }

        let entries = self.overview_entries();
        let area = self.overview_area();
        self.overview = Some(crate::overview::OverviewUi::new(
            &entries,
            area,
            &self.overview_theme,
            self.layout.focused_window(),
        ));

        tracing::info!(previews = entries.len(), "overview opened");
        self.reconcile_space();
    }

    /// Starts closing the overview, focusing `select` once the animation
    /// finishes. Returns `false` if the overview was not open.
    ///
    /// The teardown itself happens in [`Self::tick_overview`] so the
    /// closing animation gets to play; callers that need the overview gone
    /// *now* (a window vanishing out from under it) use
    /// [`Self::close_overview`].
    pub fn begin_close_overview(&mut self, select: Option<glide_layout::WindowId>) -> bool {
        if !self.layout.overview_active() {
            return false;
        }
        if let Some(ui) = self.overview.as_mut() {
            ui.begin_close(select);
            true
        } else {
            // No UI to animate (config disabled it, or it never built):
            // drop straight out.
            self.close_overview()
        }
    }

    /// Tears the overview down immediately, skipping any animation.
    /// Returns `true` if it was open.
    pub fn close_overview(&mut self) -> bool {
        self.overview = None;
        self.layout.close_overview()
    }

    /// Drives the overview's animation. Called once per frame.
    ///
    /// Returns `true` if a repaint is needed — either the animation is
    /// still running, or it just finished and the scene changed.
    pub fn tick_overview(&mut self) -> bool {
        let Some(ui) = self.overview.as_mut() else {
            return false;
        };

        match ui.tick() {
            crate::overview::Tick::Idle => false,
            crate::overview::Tick::Running => true,
            crate::overview::Tick::Finished { select } => {
                self.close_overview();
                if let Some(id) = select {
                    self.focus_window(id);
                } else {
                    self.reconcile_space();
                }
                true
            }
        }
    }

    /// Rebuilds the overview's slots after the window set changed.
    pub fn refresh_overview(&mut self) {
        if self.overview.is_none() {
            return;
        }
        let entries = self.overview_entries();
        let area = self.overview_area();
        if let Some(ui) = self.overview.as_mut() {
            ui.relayout(&entries, area);
        }
    }

    /// Focuses a window through the layout engine and applies the result.
    ///
    /// The only sanctioned way to change focus. Setting keyboard focus
    /// directly would desync the layout, leaving keybindings acting on a
    /// different window than the one the user sees as active.
    pub fn focus_window(&mut self, id: glide_layout::WindowId) {
        // Focusing a window is also how a layer surface *loses* the
        // keyboard: click a window, the launcher's turn is over.
        self.layer_focus = None;
        // Picking a window is also how the overview is dismissed.
        let closed = self.close_overview();
        if self.layout.focus_window(id) || closed {
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

                // What layer-shell panels have not claimed for themselves.
                // The zone is output-relative; the layout wants it in
                // global coordinates like everything else it sees.
                // (Computed before `output` is consumed by the id map's
                // `entry` below.)
                let zone = layer_map_for_output(&output).non_exclusive_zone();

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
                let work_area = glide_layout::Rectangle::from_loc_and_size(
                    geometry.loc.x + zone.loc.x,
                    geometry.loc.y + zone.loc.y,
                    zone.size.w,
                    zone.size.h,
                );

                glide_layout::OutputInfo {
                    id,
                    geometry: rect,
                    work_area,
                }
            })
            .collect();

        tracing::info!(outputs = infos.len(), "layout outputs synchronized");
        self.layout.outputs_changed(&infos);
        self.wlr_output_manager_state
            .sync_outputs(&self.display_handle, &self.space);
    }

    /// Switches a window between server-side decorations and none, keeping
    /// the client's content exactly where it is (the layout rebuilds the
    /// frame around it — see [`glide_layout::Layout::set_window_insets`]).
    ///
    /// The single implementation behind both decoration protocols
    /// (`xdg-decoration` and the KDE flavor): whichever one the client
    /// speaks, the effect on the layout is identical.
    pub fn set_window_decorated(&mut self, id: glide_layout::WindowId, decorated: bool) {
        let insets = if decorated {
            let height = i32::try_from(self.decoration_theme.height).unwrap_or(i32::MAX);
            glide_layout::Insets {
                top: height,
                left: 0,
                right: 0,
                bottom: 0,
            }
        } else {
            glide_layout::Insets::NONE
        };

        if self.layout.set_window_insets(id, insets) {
            self.reconcile_space();
        }
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
        // Nothing drags windows while the overview is up: previews are
        // render-only and the grabs are suppressed, so a stray call here
        // would be writing a bogus position into the layout.
        if self.layout.overview_active() {
            return;
        }
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
        // Nothing drags windows while the overview is up: previews are
        // render-only and the grabs are suppressed, so a stray call here
        // would be writing a bogus position into the layout.
        if self.layout.overview_active() {
            return;
        }
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
        self.layer_focus = None;
        self.layout.clear_focus();
        self.reconcile_space();
    }

    /// Finds what the pointer is over at `pos`: a layer surface, a client
    /// surface, a window's decoration, or nothing.
    ///
    /// Stacking follows the wlr-layer-shell model: `Overlay` and `Top`
    /// layers sit above every window, `Bottom` and `Background` below them
    /// — matching the order `space_render_elements` draws in, so what you
    /// click is always what you see.
    ///
    /// Returns the target together with *its own* absolute origin — not the
    /// window's — since Smithay computes the location handed to the
    /// target's `enter`/`motion` by subtracting exactly this from the
    /// pointer's own position (see `PointerInternal::motion`). For a
    /// decoration hit that means the titlebar's own top-left, which sits
    /// `header_height` above the window's content origin — see the
    /// coordinate convention documented on [`crate::window`].
    #[must_use]
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(crate::focus::PointerFocusTarget, Point<f64, Logical>)> {
        let (output, output_loc) = self.space.outputs().find_map(|o| {
            let geometry = self.space.output_geometry(o)?;
            geometry
                .to_f64()
                .contains(pos)
                .then(|| (o.clone(), geometry.loc.to_f64()))
        })?;

        let upper = Self::layer_surface_under(&output, output_loc, pos, true);
        if upper.is_some() {
            return upper;
        }

        if let Some(hit) = self.window_target_under(pos) {
            return Some(hit);
        }

        Self::layer_surface_under(&output, output_loc, pos, false)
    }

    /// The window (content or decoration) under `pos`, if any.
    ///
    /// The decoration check goes through [`HeaderBar::contains`], never a
    /// bare "is y within the titlebar strip": with the chevron menu open,
    /// the decoration extends *below* the strip over the window's own
    /// content, and a strip-only test routed clicks on the menu straight to
    /// the client surface underneath it.
    ///
    /// [`HeaderBar::contains`]: crate::decoration::HeaderBar::contains
    fn window_target_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(crate::focus::PointerFocusTarget, Point<f64, Logical>)> {
        let (window, location) = self.space.element_under(pos)?;
        let content_origin = location.to_f64();

        let decoration_hit = {
            if let Some(deco) = window.try_decoration_state() {
                let header_height = f64::from(deco.header.height());
                let decoration_origin = content_origin - Point::from((0.0, header_height));
                deco.header
                    .contains(pos - decoration_origin)
                    .then_some(decoration_origin)
            } else {
                None
            }
        };
        if let Some(decoration_origin) = decoration_hit {
            return Some((
                crate::focus::PointerFocusTarget::from(window.clone()),
                decoration_origin,
            ));
        }

        window
            .surface_under(pos - content_origin, WindowSurfaceType::ALL)
            .map(|(s, p)| {
                (
                    crate::focus::PointerFocusTarget::from(s),
                    p.to_f64() + content_origin,
                )
            })
    }

    /// The layer surface under `pos` in either the upper (`Overlay`+`Top`)
    /// or lower (`Bottom`+`Background`) pair of layers.
    fn layer_surface_under(
        output: &Output,
        output_loc: Point<f64, Logical>,
        pos: Point<f64, Logical>,
        upper: bool,
    ) -> Option<(crate::focus::PointerFocusTarget, Point<f64, Logical>)> {
        let layers = layer_map_for_output(output);
        let local = pos - output_loc;

        let (first, second) = if upper {
            (WlrLayer::Overlay, WlrLayer::Top)
        } else {
            (WlrLayer::Bottom, WlrLayer::Background)
        };

        let layer = layers
            .layer_under(first, local)
            .or_else(|| layers.layer_under(second, local))?;
        let layer_loc = layers.layer_geometry(layer)?.loc.to_f64();

        layer
            .surface_under(local - layer_loc, WindowSurfaceType::ALL)
            .map(|(surface, loc)| {
                (
                    crate::focus::PointerFocusTarget::from(surface),
                    loc.to_f64() + layer_loc + output_loc,
                )
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

impl FractionalScaleHandler for GlideState {
    /// Tell every surface that asks the scale of the output it will show on.
    ///
    /// Single-output and integer-scale-1 today, so this is constant — but
    /// answering (rather than leaving the default silence) is what commits
    /// clients to the fractional-scale path, so they react correctly the
    /// day an output reports something else.
    fn new_fractional_scale(
        &mut self,
        surface: smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        let scale = self
            .space
            .outputs()
            .next()
            .map_or(1.0, |output| output.current_scale().fractional_scale());

        smithay::wayland::compositor::with_states(&surface, |states| {
            with_fractional_scale(states, |fractional| {
                fractional.set_preferred_scale(scale);
            });
        });
    }
}

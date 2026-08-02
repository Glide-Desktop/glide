use std::{ffi::OsString, sync::Arc};

use smithay::{
    desktop::{PopupManager, Space, Window, WindowSurfaceType},
    input::{
        Seat, SeatState,
        keyboard::{Keycode, XkbConfig},
        pointer::CursorImageStatus,
    },
    reexports::{
        calloop::{Interest, LoopHandle, LoopSignal, Mode, PostAction, generic::Generic},
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
        shell::xdg::XdgShellState,
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use crate::backend::tty::TtyData;

pub struct GlideState {
    pub backend_data: TtyData,

    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,

    pub space: Space<Window>,
    pub loop_signal: LoopSignal,
    /// Used by backends (currently just tty) to schedule follow-up work from
    /// inside event source callbacks, e.g. a repaint timer after a `VBlank`.
    pub loop_handle: LoopHandle<'static, Self>,

    // Smithay State
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<GlideState>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,

    pub seat: Seat<Self>,

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
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            popups,
            seat,
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

/// The `wl_surface` backing a [`Window`]'s toplevel.
///
/// [`Window::new_wayland_window`] is the only constructor used anywhere in
/// Glide (there is no `XWayland` support), so [`Window::toplevel`] is always
/// `Some`. Centralized here rather than repeating `.toplevel().unwrap()` at
/// every call site (handlers, grabs, input) so that adding a second kind of
/// window later only needs updating this one assumption.
pub(crate) fn window_surface(window: &Window) -> &WlSurface {
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

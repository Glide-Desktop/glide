use std::{ffi::OsString, sync::Arc};

use smithay::{
    desktop::{PopupManager, Space, Window, WindowSurfaceType},
    input::{Seat, SeatState, keyboard::Keycode, pointer::CursorImageStatus},
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

/// Implemented by a backend driving the display and input.
///
/// `GlideState` is generic over the backend so backend-specific data can live
/// alongside the shared compositor state without leaking into protocol
/// handler implementations, which are written once against `GlideState<B>`
/// for any `B: Backend`.
///
/// Note that [`TtyData`](crate::backend::tty::TtyData) is currently the only
/// implementor: the nested winit backend was removed once Glide started
/// running exclusively from a bare tty.
pub trait Backend {
    /// Name of the seat this backend is bound to, as known to logind/seatd
    /// (e.g. `"seat0"`). Used both as the `wl_seat` name advertised to
    /// clients and to assign libinput devices to the correct physical seat.
    fn seat_name(&self) -> String;

    /// Switch to a different virtual terminal.
    ///
    /// Not optional in practice: taking control of the session puts the tty
    /// into `K_OFF`, so the kernel stops handling `Ctrl+Alt+Fn` itself. If
    /// this does nothing, a running compositor leaves no way off the tty.
    fn change_vt(&mut self, vt: i32);
}

pub struct GlideState<BackendData: Backend + 'static> {
    pub backend_data: BackendData,

    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,

    pub space: Space<Window>,
    pub loop_signal: LoopSignal,
    /// Used by backends (currently just tty) to schedule follow-up work from
    /// inside event source callbacks, e.g. a repaint timer after a VBlank.
    pub loop_handle: LoopHandle<'static, Self>,

    // Smithay State
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<GlideState<BackendData>>,
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

impl<BackendData: Backend + 'static> GlideState<BackendData> {
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
    pub fn new(
        backend_data: BackendData,
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

        // Notify clients that we have a keyboard, for the sake of the example we assume that keyboard is always present.
        // You may want to track keyboard hot-plug in real compositor.
        seat.add_keyboard(Default::default(), 200, 25).unwrap();

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
        let listening_socket = ListeningSocketSource::new_auto().unwrap();

        // Get the name of the listening socket.
        // Clients will connect to this socket.
        let socket_name = listening_socket.socket_name().to_os_string();

        loop_handle
            .insert_source(listening_socket, move |client_stream, _, state| {
                // Inside the callback, you should insert the client into the display.
                //
                // You may also associate some data with the client when inserting the client.
                state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .unwrap();
            })
            .expect("Failed to init the wayland event source.");

        // You also need to add the display itself to the event loop, so that client events will be processed by wayland-server.
        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
                    // Safety: we don't drop the display
                    unsafe {
                        display.get_mut().dispatch_clients(state).unwrap();
                    }
                    Ok(PostAction::Continue)
                },
            )
            .unwrap();

        socket_name
    }

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

/// Data associated with a wayland client that connects to GlideState.
/// One instance of this type per client.
#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

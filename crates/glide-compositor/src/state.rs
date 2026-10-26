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
        fractional_scale::{FractionalScaleHandler, FractionalScaleManagerState, with_fractional_scale},
        output::OutputManagerState,
        pointer_constraints::PointerConstraintsState,
        presentation::PresentationState,
        relative_pointer::RelativePointerManagerState,
        security_context::SecurityContextState,
        selection::{
            data_device::DataDeviceState, ext_data_control::DataControlState as ExtDataControlState,
            primary_selection::PrimarySelectionState, wlr_data_control::DataControlState as WlrDataControlState,
        },
        shell::kde::decoration::KdeDecorationState,
        shell::wlr_layer::{Layer as WlrLayer, WlrLayerShellState},
        shell::xdg::{
            ToplevelState, XdgShellState, XdgToplevelSurfaceData, decoration::XdgDecorationState,
            dialog::XdgDialogState,
        },
        shm::ShmState,
        socket::ListeningSocketSource,
        xdg_activation::XdgActivationState,
    },
};

use crate::{backend::tty::TtyData, window::GlideWindow};

#[allow(clippy::struct_field_names)]
pub struct Protocols {
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub xdg_dialog_state: XdgDialogState,
    pub cursor_shape_state: CursorShapeManagerState,
    pub xdg_decoration_state: XdgDecorationState,
    pub kde_decoration_state: KdeDecorationState,
    pub layer_shell_state: WlrLayerShellState,
    pub fractional_scale_state: FractionalScaleManagerState,
    pub viewporter_state: smithay::wayland::viewporter::ViewporterState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<GlideState>,
    pub data_device_state: DataDeviceState,
    pub primary_selection_state: PrimarySelectionState,
    pub wlr_data_control_state: WlrDataControlState,
    pub ext_data_control_state: ExtDataControlState,
    pub foreign_toplevel_list_state: smithay::wayland::foreign_toplevel_list::ForeignToplevelListState,
    pub idle_notifier_state: smithay::wayland::idle_notify::IdleNotifierState<GlideState>,
    pub idle_inhibit_state: smithay::wayland::idle_inhibit::IdleInhibitManagerState,
    pub wlr_output_manager_state: crate::handlers::wlr_output_management::WlrOutputManagerState,
    pub ext_workspace_state: crate::handlers::ext_workspace::ExtWorkspaceManagerState,
    pub xdg_activation_state: XdgActivationState,
    pub security_context_state: SecurityContextState,
    pub relative_pointer_manager_state: RelativePointerManagerState,
    pub presentation_state: PresentationState,
    pub pointer_constraints_state: PointerConstraintsState,
    pub session_lock_state: smithay::wayland::session_lock::SessionLockManagerState,
    pub text_input_state: smithay::wayland::text_input::TextInputManagerState,
    pub input_method_state: smithay::wayland::input_method::InputMethodManagerState,
    pub virtual_keyboard_state: smithay::wayland::virtual_keyboard::VirtualKeyboardManagerState,
    pub keyboard_shortcuts_inhibit_state: smithay::wayland::keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitState,
    pub pointer_gestures_state: smithay::wayland::pointer_gestures::PointerGesturesState,
    pub wlr_output_power_state: crate::handlers::wlr_output_power::WlrOutputPowerState,
    pub wlr_gamma_control_state: crate::handlers::wlr_gamma_control::WlrGammaControlState,
    pub xdg_foreign_state: smithay::wayland::xdg_foreign::XdgForeignState,
    pub system_bell_state: smithay::wayland::xdg_system_bell::XdgSystemBellState,
    pub toplevel_icon_manager: smithay::wayland::xdg_toplevel_icon::XdgToplevelIconManager,
    pub background_effect_state: smithay::wayland::background_effect::BackgroundEffectState,
    pub single_pixel_buffer_state: smithay::wayland::single_pixel_buffer::SinglePixelBufferState,
    pub content_type_state: smithay::wayland::content_type::ContentTypeState,
}

impl Protocols {
    fn new(
        dh: &DisplayHandle, loop_handle: &LoopHandle<'static, GlideState>, seat_state: SeatState<GlideState>,
        clock_id: u32, gate: &Arc<crate::privilege::Gate>,
    ) -> Self {
        use glide_config::permissions::Access;
        use smithay::wayland::{
            foreign_toplevel_list::ForeignToplevelListState, idle_inhibit::IdleInhibitManagerState,
            idle_notify::IdleNotifierState, input_method::InputMethodManagerState,
            keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitState,
            pointer_gestures::PointerGesturesState, session_lock::SessionLockManagerState,
            text_input::TextInputManagerState, viewporter::ViewporterState,
            virtual_keyboard::VirtualKeyboardManagerState,
        };
        use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration_manager::Mode as KdeMode;
        use crate::handlers::{
            ext_workspace::ExtWorkspaceManagerState, wlr_gamma_control::WlrGammaControlState,
            wlr_output_management::WlrOutputManagerState, wlr_output_power::WlrOutputPowerState,
        };

        let compositor_state = CompositorState::new::<GlideState>(dh);
        let xdg_shell_state = XdgShellState::new::<GlideState>(dh);
        let xdg_dialog_state = XdgDialogState::new::<GlideState>(dh);
        let cursor_shape_state = CursorShapeManagerState::new::<GlideState>(dh);
        let xdg_decoration_state = XdgDecorationState::new::<GlideState>(dh);
        let kde_decoration_state = KdeDecorationState::new::<GlideState>(dh, KdeMode::Server);
        let layer_shell_state = WlrLayerShellState::new_with_filter::<GlideState, _>(dh, client_is_unrestricted);
        let fractional_scale_state = FractionalScaleManagerState::new::<GlideState>(dh);
        let viewporter_state = ViewporterState::new::<GlideState>(dh);
        let shm_state = ShmState::new::<GlideState>(dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<GlideState>(dh);
        let data_device_state = DataDeviceState::new::<GlideState>(dh);
        let primary_selection_state = PrimarySelectionState::new::<GlideState>(dh);
        let unrestricted = client_is_unrestricted;
        let guard = |access: Access| {
            let gate = Arc::clone(gate);
            move |client: &smithay::reexports::wayland_server::Client| gate.permits(access, client)
        };
        Self {
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
            wlr_data_control_state: WlrDataControlState::new::<GlideState, _>(
                dh,
                Some(&primary_selection_state),
                guard(Access::Clipboard),
            ),
            ext_data_control_state: ExtDataControlState::new::<GlideState, _>(
                dh,
                Some(&primary_selection_state),
                guard(Access::Clipboard),
            ),
            primary_selection_state,
            foreign_toplevel_list_state: ForeignToplevelListState::new_with_filter::<GlideState>(dh, unrestricted),
            idle_notifier_state: IdleNotifierState::new(dh, loop_handle.clone()),
            idle_inhibit_state: IdleInhibitManagerState::new::<GlideState>(dh),
            wlr_output_manager_state: WlrOutputManagerState::new::<GlideState>(dh),
            ext_workspace_state: ExtWorkspaceManagerState::new::<GlideState>(dh),
            xdg_activation_state: XdgActivationState::new::<GlideState>(dh),
            security_context_state: SecurityContextState::new::<GlideState, _>(dh, unrestricted),
            relative_pointer_manager_state: RelativePointerManagerState::new::<GlideState>(dh),
            presentation_state: PresentationState::new::<GlideState>(dh, clock_id),
            pointer_constraints_state: PointerConstraintsState::new::<GlideState>(dh),
            text_input_state: TextInputManagerState::new::<GlideState>(dh),
            input_method_state: InputMethodManagerState::new::<GlideState, _>(dh, guard(Access::Input)),
            virtual_keyboard_state: VirtualKeyboardManagerState::new::<GlideState, _>(dh, guard(Access::Input)),
            keyboard_shortcuts_inhibit_state: KeyboardShortcutsInhibitState::new::<GlideState>(dh),
            pointer_gestures_state: PointerGesturesState::new::<GlideState>(dh),
            wlr_output_power_state: WlrOutputPowerState::new::<GlideState>(dh),
            wlr_gamma_control_state: WlrGammaControlState::new::<GlideState>(dh),
            session_lock_state: SessionLockManagerState::new::<GlideState, _>(dh, unrestricted),
            xdg_foreign_state: smithay::wayland::xdg_foreign::XdgForeignState::new::<GlideState>(dh),
            system_bell_state: smithay::wayland::xdg_system_bell::XdgSystemBellState::new::<GlideState>(dh),
            toplevel_icon_manager: smithay::wayland::xdg_toplevel_icon::XdgToplevelIconManager::new::<GlideState>(dh),
            background_effect_state: smithay::wayland::background_effect::BackgroundEffectState::new::<GlideState>(dh),
            single_pixel_buffer_state: smithay::wayland::single_pixel_buffer::SinglePixelBufferState::new::<GlideState>(
                dh,
            ),
            content_type_state: smithay::wayland::content_type::ContentTypeState::new::<GlideState>(dh),
        }
    }
}

pub struct SessionLock {
    pub surfaces: HashMap<Output, smithay::wayland::session_lock::LockSurface>,
    pub confirmation: Option<smithay::wayland::session_lock::SessionLocker>,
    pub queued: HashSet<Output>,
    pub presented: HashSet<Output>,
}

impl SessionLock {
    #[must_use]
    pub fn new(confirmation: smithay::wayland::session_lock::SessionLocker) -> Self {
        Self {
            surfaces: HashMap::new(),
            confirmation: Some(confirmation),
            queued: HashSet::new(),
            presented: HashSet::new(),
        }
    }

    pub fn confirm(&mut self) {
        if let Some(confirmation) = self.confirmation.take() {
            confirmation.lock();
        }
    }

    pub fn frame_queued(&mut self, output: &Output) {
        if self.confirmation.is_some() {
            self.queued.insert(output.clone());
        }
    }

    pub fn frame_presented<'a>(&mut self, output: &Output, outputs: impl IntoIterator<Item = &'a Output>) {
        if self.queued.contains(output) {
            self.presented.insert(output.clone());
        }
        self.confirm_if_all_presented(outputs);
    }

    pub fn confirm_if_all_presented<'a>(&mut self, outputs: impl IntoIterator<Item = &'a Output>) {
        if self.confirmation.is_some() && outputs.into_iter().all(|output| self.presented.contains(output)) {
            self.confirm();
        }
    }

    pub fn forget_output(&mut self, output: &Output) {
        self.surfaces.remove(output);
        self.queued.remove(output);
        self.presented.remove(output);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionPolicy {
    pub configured_scale: f32,
    pub respect_system: bool,
}

impl Default for MotionPolicy {
    fn default() -> Self {
        Self { configured_scale: 1.0, respect_system: true }
    }
}

#[derive(Debug, Default)]
pub struct Gestures {
    pub target: crate::input::SwipeTarget,
    pub config: crate::config::GestureConfig,
    pub clients: crate::ipc::IpcSubscribers,
    pub practice: Option<crate::input::Practice>,
    pub holding: bool,
    pub app_switching: bool,
    pub pinching: bool,
    pub pinch_scale: f64,
    pub shown_desktop: Vec<glide_layout::WindowId>,
    pub edge: Option<crate::input::EdgeSwipe>,
    pub touch_points: usize,
}

pub struct GlideState {
    pub backend_data: TtyData,

    pub start_time: std::time::Instant,
    pub clock: smithay::utils::Clock<smithay::utils::Monotonic>,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,

    pub space: Space<GlideWindow>,
    pub loop_signal: LoopSignal,
    pub loop_handle: LoopHandle<'static, Self>,

    pub protocols: Protocols,
    pub popups: PopupManager,

    pub seat: Seat<Self>,

    pub layout: glide_layout::Layout,
    pub window_map: HashMap<glide_layout::WindowId, GlideWindow>,
    pub window_to_id: HashMap<GlideWindow, glide_layout::WindowId>,
    pub output_ids: HashMap<Output, glide_layout::OutputId>,
    pub keybinds: Vec<crate::input::ResolvedKeybind>,
    pub decoration_theme: crate::config::DecorationTheme,
    pub overview_theme: crate::config::OverviewTheme,
    pub background_color: crate::config::Color,
    pub wallpaper_fit: crate::config::WallpaperFit,

    pub cursor_status: CursorImageStatus,
    pub cursor_position_hint: Option<(WlSurface, Point<f64, Logical>)>,

    pub layer_focus: Option<LayerSurface>,

    pub overview: Option<crate::overview::OverviewUi>,
    pub touchpad_gesture: Option<crate::input::TouchpadGestureTracker>,
    pub workspace_swipe: Option<crate::input::WorkspaceSwipeState>,
    pub gestures: Gestures,

    pub suppressed_keys: Vec<Keycode>,

    pub foreign_toplevel_handles:
        HashMap<glide_layout::WindowId, smithay::wayland::foreign_toplevel_list::ForeignToplevelHandle>,
    pub idle_inhibited_surfaces: HashSet<WlSurface>,
    pub windows_awaiting_open_animation: HashSet<glide_layout::WindowId>,
    pub snapshots: crate::snapshot::Snapshots,
    pub dock: crate::dock::Dock,
    pub app_directory: crate::app_index::AppDirectory,
    pub dock_keys: crate::dock_glue::DockKeyCache,
    pub launcher_entries: HashMap<String, crate::launcher_entry::EntryState>,
    pub topbar: crate::topbar::Topbar,
    pub notifications: crate::notifications::Notifications,
    pub media: Option<glide_media::MediaHandle>,
    pub media_art: crate::media_art::MediaArt,
    pub screenshot_flash: crate::screenshot_ui::Flash,
    pub app_menus: crate::app_menu::AppMenus,
    pub tray: crate::tray::TrayService,
    pub screencopy: crate::screencopy::ScreencopyState,
    pub launcher: Option<crate::launcher::Launcher>,
    pub launcher_theme: crate::launcher::LauncherTheme,
    pub launcher_icons: crate::icon::IconLoader,
    pub switcher: Option<crate::switcher::Switcher>,
    pub switcher_names: HashMap<String, Option<String>>,
    pub power_dialog: Option<crate::power_dialog::PowerDialog>,
    pub power_countdown: Option<smithay::reexports::calloop::RegistrationToken>,
    pub focus_history: Vec<glide_layout::WindowId>,
    pub ipc_subscribers: crate::ipc::IpcSubscribers,

    pub pending_screenshot: Option<Output>,
    pub active_slider_drag: Option<crate::topbar::Slot>,
    pub announced_workspace: Option<glide_layout::WorkspaceId>,
    pub synced_workspaces: Option<(Vec<glide_layout::WorkspaceId>, glide_layout::WorkspaceId)>,
    pub snap_preview: Option<crate::snap::SnapPreview>,
    pub snap_dwell: crate::snap::EdgeDwell,
    pub edge_snapping: bool,
    pub launcher_options: crate::launcher::search::Options,
    pub super_tap_enabled: bool,
    pub hot_corner: crate::snap::HotCorner,
    pub drag: crate::drag_assist::DragState,
    pub max_volume_pct: u8,
    pub shell_palette: (glide_theme::Appearance, glide_theme::Accent),
    pub night_light: crate::config::NightLightConfig,
    pub night_light_scheduled: Option<bool>,
    pub motion: MotionPolicy,

    pub font_family: Option<String>,
    pub outputs_blanked: bool,
    pub idle: crate::idle::Idle,
    pub wake_guard: crate::idle::WakeGuard<smithay::backend::input::TouchSlot>,
    pub input_config: crate::config::InputConfig,
    pub smooth_wheel: crate::smooth_wheel::SmoothWheel,
    pub layout_memory: crate::layout_scope::LayoutMemory,
    pub input_devices: Vec<smithay::reexports::input::Device>,
    pub output_configs: Vec<crate::config::OutputConfig>,
    pub session_lock: Option<SessionLock>,
    pub applied_config: crate::config::AppliedConfig,
    pub greeter: Option<crate::greeter::GreeterUi>,
    pub greeter_handle: Option<crate::greeter::GreetdHandle>,
    pub lock_auth: Option<crate::greeter::pam::Authenticator>,
    pub session: SessionServices,
    pub handoff: Option<crate::handoff::Handoff>,
    pub shell_cursor: Option<smithay::input::pointer::CursorIcon>,
}

#[derive(Default)]
pub struct SessionServices {
    pub logind: Option<crate::logind::Handle>,
    pub screensaver: Option<crate::screensaver::Handle>,
    pub shell_bus: Option<crate::shell_bus::Handle>,
    pub launch_intent: Option<std::time::Instant>,
    pub last_user_input: Option<std::time::Instant>,
    pub screenshot_waiters: Vec<std::os::unix::net::UnixStream>,
    pub picker: Option<crate::screen_picker::Picker>,
    pub shot: Option<crate::shot::Session>,
    pub shot_memory: crate::shot::model::Memory,
    pub window_capture: Option<crate::shot::WindowCapture>,
    pub last_screenshot: Option<std::path::PathBuf>,
    pub search_providers: Option<crate::launcher::providers::Handle>,
    pub search_providers_started: bool,
    pub quicklists: HashMap<String, crate::launcher_entry::Quicklist>,
    pub modal_windows: std::collections::HashSet<glide_layout::WindowId>,
    pub framed_pids: crate::framed::FramedClients,
    pub capture: Option<crate::screen_picker::Capture>,
    pub qr_scan: Option<crate::qr_scan::Scan>,
    pub end: crate::session_end::SessionEnd,
    pub battery_alert: crate::battery_alert::BatteryAlert,
    pub battery_saver: crate::battery_alert::Saver,
    pub announced_config_keys: Vec<String>,
    pub prompts: crate::agent_prompt::Prompts,
    pub bell: Option<std::time::Instant>,
    pub volumes: crate::removable::Toasts,
    pub tiled_from: HashMap<glide_layout::WindowId, glide_layout::Rectangle>,
    pub osd: crate::osd::Osd,
    pub super_tap: crate::super_keys::SuperTap,
    pub lid_closed: bool,
    pub appearance_schedule: bool,
    pub lid_hidden: Vec<Output>,
    pub arrangement: crate::config::Arrangement,
    pub primary_output: Option<String>,
    pub mirrors: Vec<Output>,
    pub shortcuts: crate::shortcut_guard::ShortcutGuard,
    pub netbird: crate::netbird::Session,
    pub headset: crate::headset::Session,
    pub virtual_keymaps: crate::handlers::virtual_keyboard::VirtualKeymaps,
    pub text_field: Option<crate::focused_field::Field>,
    pub otp: Option<crate::otp::Code>,
    pub keychain: crate::keychain::Keychain,
    pub autofill: Option<crate::autofill::Offer>,
    pub passwords: crate::passwords_applet::Session,
    pub asks: crate::agent_prompt::Asks,
    pub polkit: crate::polkit::Polkit,
    pub cast: crate::cast::Cast,
    pub gate: Arc<crate::privilege::Gate>,
    pub permission_prompts: crate::privilege::Pending,
}

impl GlideState {
    #[must_use]
    pub fn new(
        backend_data: TtyData, loop_handle: LoopHandle<'static, Self>, loop_signal: LoopSignal, display: Display<Self>,
    ) -> Self {
        let start_time = std::time::Instant::now();
        let clock = smithay::utils::Clock::new();

        let dh = display.handle();
        let mut seat_state = SeatState::new();
        let seat = Self::new_seat(&mut seat_state, &dh, backend_data.seat_name());

        let socket_name = Self::init_wayland_listener(display, &loop_handle);
        let gate = Arc::new(crate::privilege::Gate::load());
        let screencopy = crate::screencopy::ScreencopyState::new::<Self>(&dh, Arc::clone(&gate));
        let app_menus = crate::app_menu::AppMenus::new::<Self>(&dh);
        let protocols = Protocols::new(&dh, &loop_handle, seat_state, clock.id() as u32, &gate);

        Self {
            backend_data,
            start_time,
            clock,
            display_handle: dh,
            space: Space::default(),
            loop_signal,
            loop_handle,
            socket_name,
            protocols,
            popups: PopupManager::default(),
            seat,
            layout: glide_layout::Layout::new(),
            window_map: HashMap::new(),
            window_to_id: HashMap::new(),
            output_ids: HashMap::new(),
            keybinds: Vec::new(),
            decoration_theme: crate::config::DecorationTheme::default(),
            overview_theme: crate::config::OverviewTheme::default(),
            background_color: crate::config::GeneralConfig::default().background,
            wallpaper_fit: crate::config::WallpaperFit::default(),
            cursor_status: CursorImageStatus::default_named(),
            cursor_position_hint: None,
            layer_focus: None,
            overview: None,
            touchpad_gesture: None,
            workspace_swipe: None,
            gestures: Gestures::default(),
            suppressed_keys: Vec::new(),
            foreign_toplevel_handles: HashMap::new(),
            idle_inhibited_surfaces: HashSet::new(),
            windows_awaiting_open_animation: HashSet::new(),
            snapshots: crate::snapshot::Snapshots::default(),
            dock: crate::dock::Dock::new(&crate::config::DockConfig::default()),
            app_directory: crate::app_index::AppDirectory::default(),
            dock_keys: crate::dock_glue::DockKeyCache::default(),
            launcher_entries: HashMap::new(),
            topbar: crate::topbar::Topbar::new(crate::topbar::TopbarTheme::default()),
            media: None,
            media_art: crate::media_art::MediaArt::default(),
            screenshot_flash: crate::screenshot_ui::Flash::default(),
            app_menus,
            tray: crate::tray::TrayService::default(),
            notifications: crate::notifications::Notifications::new(),
            screencopy,
            launcher: None,
            launcher_theme: crate::launcher::LauncherTheme::default(),
            launcher_icons: crate::icon::IconLoader::default(),
            switcher: None,
            switcher_names: HashMap::new(),
            power_dialog: None,
            power_countdown: None,
            focus_history: Vec::new(),
            ipc_subscribers: crate::ipc::IpcSubscribers::default(),
            pending_screenshot: None,
            announced_workspace: None,
            synced_workspaces: None,
            snap_preview: None,
            snap_dwell: crate::snap::EdgeDwell::default(),
            edge_snapping: true,
            launcher_options: crate::launcher::search::Options::default(),
            super_tap_enabled: true,
            hot_corner: crate::snap::HotCorner::default(),
            drag: crate::drag_assist::DragState::default(),
            max_volume_pct: 100,
            shell_palette: (glide_theme::Appearance::Dark, glide_theme::Accent::Blue),
            night_light: crate::config::NightLightConfig::default(),
            night_light_scheduled: None,
            motion: MotionPolicy::default(),
            active_slider_drag: None,
            font_family: None,
            outputs_blanked: false,
            idle: crate::idle::Idle::new(std::time::Instant::now()),
            wake_guard: crate::idle::WakeGuard::default(),
            input_config: crate::config::InputConfig::default(),
            smooth_wheel: crate::smooth_wheel::SmoothWheel::default(),
            layout_memory: crate::layout_scope::LayoutMemory::default(),
            input_devices: Vec::new(),
            output_configs: Vec::new(),
            session_lock: None,
            applied_config: crate::config::AppliedConfig::default(),
            greeter: None,
            greeter_handle: None,
            lock_auth: None,
            session: SessionServices { gate, ..SessionServices::default() },
            handoff: None,
            shell_cursor: None,
        }
    }

    fn new_seat(seat_state: &mut SeatState<Self>, dh: &DisplayHandle, name: String) -> Seat<Self> {
        let mut seat = seat_state.new_wl_seat(dh, name);
        seat.add_keyboard(XkbConfig::default(), 200, 25).expect("the default XKB keymap should always compile");
        seat.add_pointer();
        seat.add_touch();
        smithay::wayland::text_input::TextInputSeat::text_input(&seat).set_compositor_input_method(true);
        seat
    }

    fn init_wayland_listener(display: Display<Self>, loop_handle: &LoopHandle<'static, Self>) -> OsString {
        let listening_socket =
            ListeningSocketSource::new_auto().expect("failed to create a wayland-N socket in XDG_RUNTIME_DIR");

        let socket_name = listening_socket.socket_name().to_os_string();

        loop_handle
            .insert_source(listening_socket, move |client_stream, (), state| {
                let identity = crate::identity::Identity::of_stream(&client_stream);
                let client_state = ClientState { identity, ..ClientState::default() };
                if let Err(err) = state.display_handle.insert_client(client_stream, Arc::new(client_state)) {
                    tracing::warn!("dropping new wayland client: {err}");
                }
            })
            .expect("failed to register the wayland listening socket with the event loop");

        loop_handle
            .insert_source(Generic::new(display, Interest::READ, Mode::Level), |_, display, state| {
                // SAFETY: the display is owned by this event source and never dropped while it is registered.
                let dispatched = unsafe { display.get_mut().dispatch_clients(state) };
                if let Err(err) = dispatched {
                    tracing::warn!("wayland client dispatch failed: {err}");
                }
                Ok(PostAction::Continue)
            })
            .expect("failed to register the wayland display with the event loop");

        socket_name
    }

    pub fn reconcile_space(&mut self) {
        self.cleanup_dead_windows();

        self.refresh_overview();
        self.refresh_dock();
        self.refresh_topbar();
        self.announce_workspace();
        self.sync_workspaces_if_changed();

        let visible: Vec<glide_layout::WindowState> = self.layout.visible_windows().collect();
        let visible_ids: HashSet<glide_layout::WindowId> = visible.iter().map(|state| state.id).collect();

        let focused = self.layout.focused_window();

        let mapped: Vec<GlideWindow> = self.space.elements().cloned().collect();
        for window in mapped {
            let hidden = self.window_to_id.get(&window).is_none_or(|id| !visible_ids.contains(id));
            if hidden {
                self.space.unmap_elem(&window);
            }
        }

        for (id, window) in &self.window_map {
            if visible_ids.contains(id) {
                continue;
            }
            if let Some(state) = self.layout.window_state(*id) {
                Self::configure_hidden_window(window, state);
            }
        }

        for state in &visible {
            let Some(window) = self.window_map.get(&state.id).cloned() else {
                continue;
            };

            let is_focused = focused == Some(state.id);

            let content = state.content_geometry();
            let location = Point::from((content.loc.x, content.loc.y));

            self.space.map_element(window.clone(), location, false);
            let work_area = self.layout.work_area_for_window(state.id);
            Self::apply_window_state(&window, state, is_focused, work_area);

            let decoration_height = if state.fullscreen { 0 } else { state.insets.top };
            let framed = !state.fullscreen
                && decoration_height == 0
                && (window.frames_itself()
                    || (!self.session.framed_pids.is_empty()
                        && self.window_pid(&window).is_some_and(|pid| self.session.framed_pids.contains(pid))));
            window.sync_decoration(&self.decoration_theme, decoration_height, is_focused, state.maximized, framed);
        }

        self.apply_keyboard_focus();
    }

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
            self.forget_window(id, &window);
        }

        self.idle_inhibited_surfaces.retain(smithay::utils::IsAlive::alive);
        self.refresh_idle_inhibit();

        self.suppressed_keys.clear();
        self.sync_workspaces_if_changed();
    }

    fn configure_hidden_window(window: &GlideWindow, state: &glide_layout::WindowState) {
        let Some(toplevel) = window.toplevel() else {
            return;
        };
        if Self::update_pending_state(toplevel, state, false, None) && toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }

    fn apply_window_state(
        window: &GlideWindow, state: &glide_layout::WindowState, focused: bool, work_area: glide_layout::Rectangle,
    ) {
        let Some(toplevel) = window.toplevel() else {
            return;
        };
        let bounds = content_bounds(work_area, state.insets);
        let mut changed = false;
        toplevel.with_pending_state(|pending| {
            if pending.bounds != Some(bounds) {
                pending.bounds = Some(bounds);
                changed = true;
            }
        });
        if Self::update_pending_state(toplevel, state, focused, Some(work_area)) || changed {
            toplevel.send_pending_configure();
        }
    }

    fn update_pending_state(
        toplevel: &smithay::wayland::shell::xdg::ToplevelSurface, state: &glide_layout::WindowState, focused: bool,
        work_area: Option<glide_layout::Rectangle>,
    ) -> bool {
        let tiled = work_area
            .filter(|_| !state.maximized && !state.fullscreen)
            .map_or([false; 4], |area| tiled_edges(state.geometry, area));
        let mut changed = false;
        toplevel.with_pending_state(|pending| {
            changed |= set_state_flag(pending, XdgState::Activated, focused);
            changed |= set_state_flag(pending, XdgState::Maximized, state.maximized);
            changed |= set_state_flag(pending, XdgState::Fullscreen, state.fullscreen);
            for (flag, on) in [XdgState::TiledLeft, XdgState::TiledRight, XdgState::TiledTop, XdgState::TiledBottom]
                .into_iter()
                .zip(tiled)
            {
                changed |= set_state_flag(pending, flag, on);
            }

            let content = state.content_geometry();
            let size = state.is_placed.then(|| smithay::utils::Size::from((content.size.width, content.size.height)));
            if pending.size != size {
                pending.size = size;
                changed = true;
            }
        });
        changed
    }

    pub fn focus_lock_surface(&mut self) {
        let Some(lock) = self.session_lock.as_ref() else {
            return;
        };
        let target = self
            .space
            .outputs()
            .next()
            .and_then(|output| lock.surfaces.get(output))
            .or_else(|| lock.surfaces.values().next())
            .map(|surface| surface.wl_surface().clone());
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        if keyboard.current_focus() != target {
            let serial = smithay::utils::SERIAL_COUNTER.next_serial();
            keyboard.set_focus(self, target, serial);
        }
    }

    #[must_use]
    pub const fn screen_locked(&self) -> bool {
        self.session_lock.is_some() || self.greeter.is_some()
    }

    pub fn apply_keyboard_focus(&mut self) {
        if self.session_lock.is_some() {
            self.focus_lock_surface();
            return;
        }
        if self.greeter.is_some() {
            if let Some(keyboard) = self.seat.get_keyboard()
                && keyboard.current_focus().is_some()
            {
                let serial = smithay::utils::SERIAL_COUNTER.next_serial();
                keyboard.set_focus(self, None, serial);
            }
            return;
        }
        if self.layer_focus.as_ref().is_some_and(|l| !l.alive()) {
            self.layer_focus = None;
        }

        let target = self.layer_focus.as_ref().map(|layer| layer.wl_surface().clone()).or_else(|| {
            self.layout
                .focused_window()
                .and_then(|id| self.window_map.get(&id))
                .map(|window| window_surface(window).clone())
        });

        if self.layer_focus.is_none()
            && let Some(id) = self.layout.focused_window()
            && self.focus_history.first() != Some(&id)
        {
            self.focus_history.retain(|seen| *seen != id);
            self.focus_history.insert(0, id);
        }

        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let keyboard = self.seat.get_keyboard().expect("a keyboard was added in GlideState::new");

        let current_focus = keyboard.current_focus();
        if current_focus != target {
            self.suppressed_keys.clear();
            let focused_window_id = self.layer_focus.is_none().then(|| self.layout.focused_window()).flatten();
            self.ipc_subscribers.broadcast(&glide_ipc::Event::FocusChanged {
                id: focused_window_id.map(glide_layout::WindowId::as_u64),
            });
        }

        keyboard.set_focus(self, target, serial);
        self.sync_layout_scope();
    }

    #[expect(clippy::needless_pass_by_value, reason = "window ownership matches track_window caller design")]
    pub fn track_window(&mut self, window: GlideWindow, parent: Option<glide_layout::WindowId>) {
        let id = glide_layout::WindowId::next();
        let size = layout_size(&window);
        if let Some(pointer) = self.seat.get_pointer() {
            let at = pointer.current_location();
            self.layout.set_placement_anchor(glide_layout::Point::new(at.x as i32, at.y as i32));
        }

        if let Some(parent) = parent {
            self.layout.add_window_with_parent(id, size, parent);
        } else {
            self.layout.add_window(id, size);
        }

        self.layout.set_window_insets(id, glide_layout::Insets::NONE);

        self.window_map.insert(id, window.clone());
        self.window_to_id.insert(window.clone(), id);
        self.windows_awaiting_open_animation.insert(id);
        self.ipc_subscribers.broadcast(&glide_ipc::Event::WindowOpened { id: id.as_u64() });

        let (title, app_id) =
            window.toplevel().map(|toplevel| toplevel_title_and_app_id(toplevel.wl_surface())).unwrap_or_default();
        let handle = self
            .protocols
            .foreign_toplevel_list_state
            .new_toplevel::<Self>(title.unwrap_or_default(), app_id.unwrap_or_default());
        self.foreign_toplevel_handles.insert(id, handle);

        self.reconcile_space();
    }

    pub fn untrack_window(&mut self, surface: &WlSurface) {
        if let Some(id) = self.window_id_for_surface(surface)
            && let Some(window) = self.window_map.remove(&id)
        {
            self.capture_closing_window(id, &window);
            self.forget_window(id, &window);
        }
        self.cleanup_dead_windows();
        self.reconcile_space();
    }

    fn forget_window(&mut self, id: glide_layout::WindowId, window: &GlideWindow) {
        self.space.unmap_elem(window);
        self.window_to_id.remove(window);
        self.window_map.remove(&id);
        self.windows_awaiting_open_animation.remove(&id);
        self.session.tiled_from.remove(&id);
        self.focus_history.retain(|seen| *seen != id);
        self.layout_memory.retain(|key| *key != crate::layout_scope::ScopeKey::Window(id.as_u64()));
        if let Some(switcher) = self.switcher.as_mut() {
            switcher.remove(id);
        }
        self.layout.remove_window(id);
        if let Some(handle) = self.foreign_toplevel_handles.remove(&id) {
            handle.send_closed();
            self.protocols.foreign_toplevel_list_state.remove_toplevel(&handle);
        }
        self.ipc_subscribers.broadcast(&glide_ipc::Event::WindowClosed { id: id.as_u64() });
    }

    #[must_use]
    pub fn window_id(&self, window: &GlideWindow) -> Option<glide_layout::WindowId> {
        self.window_to_id.get(window).copied()
    }

    #[must_use]
    pub fn is_size_locked(&self, window: &GlideWindow) -> bool {
        self.window_id(window)
            .and_then(|id| self.layout.window_state(id))
            .is_some_and(|state| state.maximized || state.fullscreen)
    }

    #[must_use]
    pub fn mapped_window(&self, surface: &WlSurface) -> Option<GlideWindow> {
        self.space.elements().find(|window| window_surface(window) == surface).cloned()
    }

    #[must_use]
    pub fn window_id_for_surface(&self, surface: &WlSurface) -> Option<glide_layout::WindowId> {
        self.window_map.iter().find(|(_, window)| window_surface(window) == surface).map(|(&id, _)| id)
    }

    #[must_use]
    pub fn layer_for_surface(&self, surface: &WlSurface) -> Option<LayerSurface> {
        self.space
            .outputs()
            .find_map(|output| layer_map_for_output(output).layer_for_surface(surface, WindowSurfaceType::ALL).cloned())
    }

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

    fn overview_entries(&self) -> Vec<(glide_layout::WindowId, Rectangle<i32, Logical>)> {
        let workspaces = self.layout.workspace_ids();
        let active = self.layout.active_workspace();
        let index_of = |id: glide_layout::WorkspaceId| workspaces.iter().position(|ws| *ws == id);
        let active_index = index_of(active);
        let width = self.widest_output().unwrap_or(0);

        self.layout
            .overview_windows()
            .into_iter()
            .filter(|state| self.window_map.contains_key(&state.id))
            .map(|state| {
                let mut rect = logical_rect(state.frame_geometry());
                if let (Some(own), Some(active)) = (index_of(state.workspace_id), active_index) {
                    let delta = i32::try_from(own).unwrap_or(0) - i32::try_from(active).unwrap_or(0);
                    rect.loc.x += delta.saturating_mul(width);
                }
                (state.id, rect)
            })
            .collect()
    }

    fn widest_output(&self) -> Option<i32> {
        self.space
            .outputs()
            .filter_map(|output| self.space.output_geometry(output))
            .map(|geometry| geometry.size.w)
            .max()
    }

    fn new_overview(&mut self) -> Option<(crate::overview::OverviewUi, usize)> {
        if let Some(launcher) = self.launcher.as_mut() {
            launcher.begin_close();
        }
        if !self.layout.open_overview() {
            return None;
        }
        let entries = self.overview_entries();
        let mut ui = crate::overview::OverviewUi::new(
            &entries,
            self.dock_area(),
            &self.overview_theme,
            self.layout.focused_window(),
        );
        ui.set_workspaces(self.layout.workspace_ids(), self.layout.active_workspace());
        Some((ui, entries.len()))
    }

    fn relayout_overview(&mut self, entries: &[(glide_layout::WindowId, Rectangle<i32, Logical>)]) {
        let area = self.dock_area();
        if let Some(ui) = self.overview.as_mut() {
            ui.set_workspaces(self.layout.workspace_ids(), self.layout.active_workspace());
            ui.relayout(entries, area);
        }
    }

    pub fn overview_switch_workspace(&mut self, workspace: glide_layout::WorkspaceId) {
        if workspace == self.layout.active_workspace() {
            return;
        }
        self.finish_workspace_swipe();
        self.layout.switch_to_workspace(glide_layout::WorkspaceTarget::Id(workspace));
        let entries = self.overview_entries();
        self.relayout_overview(&entries);
        self.reconcile_space();
        self.queue_redraw();
    }

    pub fn open_overview(&mut self) {
        let Some((ui, previews)) = self.new_overview() else {
            return;
        };
        self.overview = Some(ui);
        tracing::info!(previews, "overview opened");
        self.reconcile_space();
    }

    pub fn begin_close_overview(&mut self, select: Option<glide_layout::WindowId>) -> bool {
        if !self.layout.overview_active() {
            return false;
        }
        if let Some(ui) = self.overview.as_mut() {
            if ui.is_closing() {
                return true;
            }
            ui.begin_close(select);
            let target_workspace = select
                .and_then(|id| self.layout.window_state(id))
                .map(|state| state.workspace_id)
                .filter(|ws| *ws != self.layout.active_workspace());
            if let Some(ws) = target_workspace {
                self.finish_workspace_swipe();
                self.layout.switch_to_workspace(glide_layout::WorkspaceTarget::Id(ws));
                let entries = self.overview_entries();
                self.relayout_overview(&entries);
            }
            true
        } else {
            self.close_overview()
        }
    }

    pub fn close_overview(&mut self) -> bool {
        self.overview = None;
        self.layout.close_overview()
    }

    pub fn begin_interactive_overview(&mut self) {
        if self.overview.is_some() {
            return;
        }
        let Some((mut ui, previews)) = self.new_overview() else {
            return;
        };
        ui.set_interactive_progress(0.0);
        self.overview = Some(ui);
        tracing::info!(previews, "interactive overview opened");
        self.reconcile_space();
    }

    #[must_use]
    pub fn output_width_at_pointer(&self) -> f64 {
        let at = self.seat.get_pointer().map(|pointer| pointer.current_location().to_i32_round());
        at.and_then(|at| {
            self.space
                .outputs()
                .filter_map(|output| self.space.output_geometry(output))
                .find(|geometry| geometry.contains(at))
        })
        .map(|geometry| geometry.size.w)
        .or_else(|| self.widest_output())
        .map_or(1920.0, f64::from)
    }

    pub(crate) fn apply_swipe_offset(&mut self, active: usize, eff_x: f64, screen_width: f64) {
        let (offset, width) = (eff_x.round() as i32, screen_width.round() as i32);
        let place = |state: &mut Self, windows: &[glide_layout::WindowState], shift: i32| {
            for window_state in windows {
                if let Some(window) = state.window_map.get(&window_state.id).cloned() {
                    let content = window_state.content_geometry();
                    let location = Point::from((content.loc.x + shift, content.loc.y));
                    state.space.map_element(window, location, false);
                }
            }
        };
        let hide = |state: &mut Self, windows: &[glide_layout::WindowState]| {
            for window_state in windows {
                if let Some(window) = state.window_map.get(&window_state.id).cloned() {
                    state.space.unmap_elem(&window);
                }
            }
        };

        let current = self.layout.workspace_windows_at(active);
        let next = self.layout.workspace_windows_at(active + 1);
        let previous = active.checked_sub(1).map(|index| self.layout.workspace_windows_at(index)).unwrap_or_default();

        place(self, &current, offset);
        match offset.cmp(&0) {
            std::cmp::Ordering::Less => {
                hide(self, &previous);
                place(self, &next, offset + width);
            }
            std::cmp::Ordering::Greater => {
                hide(self, &next);
                place(self, &previous, offset - width);
            }
            std::cmp::Ordering::Equal => {
                hide(self, &next);
                hide(self, &previous);
            }
        }
    }

    pub fn finish_workspace_swipe(&mut self) {
        let Some(swipe) = self.workspace_swipe.take() else {
            return;
        };
        if let crate::input::WorkspaceSwipePhase::Settling { target_workspace: Some(target), .. } = swipe.phase {
            self.layout.switch_to_workspace(target);
        }
        self.reconcile_space();
    }

    pub fn tick_workspace_swipe(&mut self) -> bool {
        let Some(swipe) = self.workspace_swipe.as_mut() else {
            return false;
        };

        match swipe.phase {
            crate::input::WorkspaceSwipePhase::Tracking { effective_x, .. } => {
                let (active, width) = (swipe.active_workspace, swipe.screen_width);
                self.apply_swipe_offset(active, effective_x, width);
                true
            }
            crate::input::WorkspaceSwipePhase::Settling { started, release, target_workspace, .. } => {
                let finished = release.duration.is_zero() || started.elapsed() >= release.duration;
                if finished {
                    let target = target_workspace;
                    self.workspace_swipe = None;
                    if let Some(target) = target {
                        self.layout.switch_to_workspace(target);
                    }
                    self.reconcile_space();
                    false
                } else {
                    let (active, width) = (swipe.active_workspace, swipe.screen_width);
                    let eff_x = -swipe.current_progress() * width;
                    self.apply_swipe_offset(active, eff_x, width);
                    true
                }
            }
        }
    }

    pub fn tick_overview(&mut self) -> bool {
        self.watch_overview_close();
        let swipe_animating = self.tick_workspace_swipe();
        let Some(ui) = self.overview.as_mut() else {
            return swipe_animating;
        };

        match ui.tick() {
            crate::overview::Tick::Idle => swipe_animating,
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

    pub fn apply_output_configs(&mut self) {
        self.apply_arrangement();
        let mut outputs: Vec<Output> = self.space.outputs().cloned().collect();
        let names: Vec<String> = outputs.iter().map(Output::name).collect();
        let primary = crate::mirror::shell_first(&names, self.session.primary_output.as_deref());
        outputs.sort_by_key(|output| {
            (Some(output.name()) != primary, !crate::lid::is_internal(&output.name()), output.name())
        });
        if primary.is_some() && names.first() != primary.as_ref() {
            tracing::info!(?primary, "the chosen primary display takes the dock and the top bar");
            for output in &outputs {
                self.space.unmap_output(output);
            }
        }
        let mut next_x = 0;
        for output in outputs {
            let identity = crate::config::OutputIdentity::of(&output);
            let config = crate::config::output_config(&self.output_configs, &identity).cloned();
            let configured = config.as_ref().and_then(|config| config.scale).filter(|scale| *scale > 0.0);
            let scale = configured.or_else(|| {
                output.current_mode().map(|mode| {
                    let size = output.physical_properties().size;
                    auto_output_scale((size.w, size.h), (mode.size.w, mode.size.h))
                })
            });
            if let Some(scale) = scale {
                output.change_current_state(None, None, Some(smithay::output::Scale::Fractional(scale)), None);
            }
            let position: Point<i32, Logical> = config
                .as_ref()
                .and_then(|config| config.position)
                .map_or_else(|| Point::from((next_x, 0)), |p| Point::from((p[0], p[1])));
            output.change_current_state(None, None, None, Some(position));
            self.space.map_output(&output, position);

            let logical_width = output.current_mode().map_or(0, |mode| {
                let fractional = output.current_scale().fractional_scale();
                (f64::from(mode.size.w) / fractional).round() as i32
            });
            next_x = next_x.max(position.x + logical_width);
        }
        self.snapshots.clear();
        self.sync_layout_outputs();
        self.reconcile_space();
        self.queue_redraw();
    }

    #[must_use]
    pub fn is_idle_inhibited(&self) -> bool {
        self.idle_inhibited_surfaces.iter().any(smithay::utils::IsAlive::alive)
            || self.session.screensaver.as_ref().is_some_and(crate::screensaver::Handle::inhibited)
    }

    #[must_use]
    pub fn idle_inhibitor_apps(&self) -> Vec<String> {
        let mut apps: Vec<String> = self
            .idle_inhibited_surfaces
            .iter()
            .filter(|surface| smithay::utils::IsAlive::alive(*surface))
            .filter_map(|surface| self.surface_app_id(surface))
            .collect();
        apps.extend(self.session.screensaver.as_ref().map(crate::screensaver::Handle::apps).unwrap_or_default());
        apps.sort();
        apps.dedup();
        apps
    }

    pub fn refresh_idle_inhibit(&mut self) {
        let inhibited = self.is_idle_inhibited();
        self.protocols.idle_notifier_state.set_is_inhibited(inhibited);
    }

    pub fn handle_screensaver_event(&mut self, event: crate::screensaver::Event) {
        use crate::screensaver::Event;
        match event {
            Event::Inhibited(inhibited) => {
                if let Some(screensaver) = self.session.screensaver.as_mut() {
                    screensaver.set_inhibited(inhibited);
                }
                self.refresh_idle_inhibit();
            }
            Event::Lock => self.lock_screen(),
            Event::Activity => {
                self.idle.activity(std::time::Instant::now());
                if self.outputs_blanked {
                    self.set_outputs_blanked(false);
                }
            }
        }
    }

    #[must_use]
    pub fn active_layout(&mut self) -> Option<(usize, String)> {
        let keyboard = self.seat.get_keyboard()?;
        Some(keyboard.with_xkb_state(self, |context| {
            let xkb = context.xkb().lock().expect("single-threaded event loop, this lock is never poisoned");
            let layout = xkb.active_layout();
            (layout.0 as usize, xkb.layout_name(layout).to_string())
        }))
    }

    #[must_use]
    pub fn active_layout_index(&mut self) -> Option<usize> {
        let keyboard = self.seat.get_keyboard()?;
        Some(keyboard.with_xkb_state(self, |context| {
            let xkb = context.xkb().lock().expect("single-threaded event loop, this lock is never poisoned");
            xkb.active_layout().0 as usize
        }))
    }

    #[must_use]
    pub fn keyboard_layouts(&mut self) -> Option<(Vec<String>, usize)> {
        let keyboard = self.seat.get_keyboard()?;
        Some(keyboard.with_xkb_state(self, |context| {
            let xkb = context.xkb().lock().expect("single-threaded event loop, this lock is never poisoned");
            let names = xkb.layouts().map(|layout| crate::topbar::short_layout_name(xkb.layout_name(layout))).collect();
            (names, xkb.active_layout().0 as usize)
        }))
    }

    fn edit_layout(&mut self, mut edit: impl FnMut(&mut smithay::input::keyboard::XkbContext<'_>)) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        keyboard.with_xkb_state(self, |mut context| edit(&mut context));
        self.layout_switched();
    }

    pub fn layout_switched(&mut self) {
        self.broadcast_layout();
        self.sync_topbar_layout();
    }

    pub fn sync_topbar_layout(&mut self) {
        if let Some((names, active)) = self.keyboard_layouts()
            && self.topbar.set_layouts(names, active)
        {
            self.queue_redraw();
        }
    }

    pub fn cycle_keyboard_layout(&mut self) {
        #[allow(clippy::redundant_closure_for_method_calls)] // the method path is not late-bound
        self.edit_layout(|context| context.cycle_next_layout());
    }

    pub fn set_keyboard_layout(&mut self, index: usize) {
        let Ok(index) = u32::try_from(index) else {
            return;
        };
        self.edit_layout(|context| context.set_layout(smithay::input::keyboard::Layout(index)));
    }

    fn layout_scope_key(&self) -> Option<crate::layout_scope::ScopeKey> {
        use crate::layout_scope::{LayoutScope, ScopeKey};
        let focused = self.layer_focus.is_none().then(|| self.layout.focused_window()).flatten();
        match self.layout_memory.scope() {
            LayoutScope::Global => None,
            LayoutScope::Desktop => Some(ScopeKey::Desktop(self.layout.active_workspace().as_u64())),
            LayoutScope::Window => focused.map(|id| ScopeKey::Window(id.as_u64())),
            LayoutScope::Application => {
                let id = focused?;
                let app_id = self.window_map.get(&id).and_then(|window| window.identity().0);
                Some(app_id.map_or(ScopeKey::Window(id.as_u64()), ScopeKey::App))
            }
        }
    }

    fn sync_layout_scope(&mut self) {
        let Some((active, _)) = self.active_layout() else {
            return;
        };
        let active = u32::try_from(active).unwrap_or(0);
        let key = self.layout_scope_key();
        if let Some(index) = self.layout_memory.enter(key, active) {
            self.set_keyboard_layout(index as usize);
        }
    }

    #[must_use]
    fn focused_app(&self) -> (String, String) {
        let identity = self
            .layout
            .focused_window()
            .and_then(|id| self.window_map.get(&id))
            .map(crate::window::GlideWindow::identity);
        let Some((app_id, title)) = identity else {
            return (String::new(), String::new());
        };
        let app_id = app_id.unwrap_or_default();
        let title = title.unwrap_or_default();
        let name = crate::topbar::app_display_name(&app_id, &title);
        (app_id, name)
    }

    pub fn refresh_topbar(&mut self) -> bool {
        let now = crate::topbar::calendar::LocalNow::now();
        let (app_id, app_name) = self.focused_app();

        let mut dirty = now.is_some_and(|now| self.topbar.set_now(now));
        dirty |= self.topbar.tick_island(std::time::Instant::now());
        dirty |= self.sync_screen_capture();
        dirty |= self.topbar.set_app(app_id, app_name);
        dirty |= self.topbar.set_dnd(self.notifications.dnd());
        dirty |= self.topbar.set_unread(self.notifications.unread());
        if self.topbar.open_popup() == Some(crate::topbar::OpenPopup::NotificationCenter) {
            let hide = self.notifications.policy().system.private_while_sharing && self.topbar.sharing_screen();
            dirty |= self.topbar.set_nc_items(crate::notifications::redacted(self.notifications.nc_items(), hide));
        }
        dirty |= self.refresh_app_menu();
        dirty
    }

    pub fn sync_screen_capture(&mut self) -> bool {
        let mut recording = self.screencopy.recording_clients(std::time::Instant::now());
        if let Some(display) = self.session.cast.label() {
            recording.push(crate::i18n::t(crate::i18n::Key::CastIndicator).replace("{name}", &display));
        }
        self.topbar.set_screen_capture(recording)
    }

    pub fn handle_system_status(&mut self, mut status: crate::system_info::SystemStatus) {
        status.night_light_on = self.topbar.system_status().night_light_on;
        self.topbar.keep_dragged_levels(&mut status);
        if let Some(handoff) = self.handoff.as_mut() {
            handoff.mark_status();
        }
        let discharging =
            status.battery.is_some_and(|battery| battery.state == crate::system_info::BatteryState::Discharging);
        let levels = crate::osd::Levels {
            volume: status.volume_available.then_some(status.volume_pct),
            muted: status.volume_muted,
            brightness: status.brightness_available.then_some(status.brightness_pct),
            keyboard: status.keyboard_brightness,
        };
        if self.session.osd.status(levels, std::time::Instant::now()) {
            self.queue_redraw();
        }
        let alert = self.session.battery_alert.update(status.battery_pct, discharging, self.topbar.battery_low_pct());
        if let Some(profile) = self.session.battery_saver.update(
            self.topbar.battery_saver(),
            status.battery_pct,
            discharging,
            self.topbar.battery_low_pct(),
            status.power_profile,
        ) {
            tracing::info!(?profile, "switching the power profile for the battery");
            crate::system_info::set_power_profile(profile);
        }
        let pct = status.battery_pct.unwrap_or_default();
        if self.topbar.set_system_status(status) {
            self.apply_motion_scale();
            self.queue_redraw();
        }
        if let Some(level) = alert {
            self.announce_battery(level, pct);
        }
    }

    fn announce_battery(&mut self, level: crate::battery_alert::Level, pct: u8) {
        use crate::{
            battery_alert::Level,
            i18n::{Key, t},
        };
        let critical = level == Level::Critical;
        tracing::warn!(pct, critical, "battery is running low");
        let notice = glide_notify::Notification {
            id: BATTERY_NOTICE_ID,
            app_name: "Glide".to_string(),
            app_icon: "battery-caution-symbolic".to_string(),
            summary: t(if critical { Key::BatteryCritical } else { Key::BatteryLow }).to_string(),
            body: format!("{pct}% · {}", t(Key::BatteryPlugIn)),
            urgency: if critical { glide_notify::Urgency::Critical } else { glide_notify::Urgency::Normal },
            transient: true,
            timeout: (!critical).then_some(BATTERY_NOTICE_TIMEOUT),
            ..glide_notify::Notification::default()
        };
        self.notifications.push(&mut self.dock.icons, notice, true);
        self.refresh_topbar();
        self.queue_redraw();
    }

    pub fn apply_motion_scale(&self) {
        let reduced = self.motion.respect_system && self.topbar.system_status().reduced_motion;
        let scale = if reduced { 0.0 } else { self.motion.configured_scale };
        glide_theme::motion::set_scale(scale);
    }

    pub fn toggle_dark_theme(&mut self) {
        let appearance = match self.shell_palette.0 {
            glide_theme::Appearance::Dark => glide_theme::Appearance::Light,
            glide_theme::Appearance::Light => glide_theme::Appearance::Dark,
        };
        match crate::config::Config::try_load() {
            Ok(mut config) => {
                config.topbar.appearance = appearance;
                config.topbar.appearance_schedule = false;
                config.apply_to_state(self);
                crate::config::persist_appearance(appearance);
            }
            Err(message) => {
                tracing::warn!(%message, "the appearance stays until glide.toml reads again");
            }
        }
    }

    pub fn tick_appearance_schedule(&mut self) -> bool {
        if !self.session.appearance_schedule {
            return false;
        }
        let Some(minute) = crate::topbar::local_minute_of_day() else {
            return false;
        };
        if crate::config::scheduled_appearance(minute) == self.shell_palette.0 {
            return false;
        }
        match crate::config::Config::try_load() {
            Ok(config) => {
                config.apply_to_state(self);
                true
            }
            Err(message) => {
                tracing::warn!(%message, "the scheduled appearance change waits for a readable glide.toml");
                self.session.appearance_schedule = false;
                false
            }
        }
    }

    pub fn tick_night_light(&mut self) -> bool {
        let Some(minute) = crate::topbar::local_minute_of_day() else {
            return false;
        };
        let wanted = self.night_light.wants_on(minute);
        if wanted == self.night_light_scheduled {
            return false;
        }
        self.night_light_scheduled = wanted;
        let Some(on) = wanted else {
            return false;
        };
        if self.topbar.system_status().night_light_on == on {
            return false;
        }
        self.update_system_status(|status| status.night_light_on = on);
        self.apply_night_light(on);
        true
    }

    pub fn apply_device_configs(&mut self) {
        self.apply_input_config();
        self.apply_output_modes();
        self.apply_output_configs();
    }

    pub fn set_night_light_config(&mut self, config: crate::config::NightLightConfig) {
        let retint = self.night_light.temperature != config.temperature;
        self.night_light = config;
        if retint && self.topbar.system_status().night_light_on {
            self.apply_night_light(true);
        }
    }

    pub fn set_night_light(&mut self, on: bool) {
        self.update_system_status(|status| status.night_light_on = on);
        self.apply_night_light(on);
        self.queue_redraw();
    }

    pub fn apply_night_light(&mut self, on: bool) {
        let outputs: Vec<Output> = self.space.outputs().cloned().collect();
        let tint = crate::config::night_light_tint(self.night_light.temperature);
        for output in outputs {
            if on {
                let Some(size) = self.gamma_size(&output).map(|s| s as usize) else {
                    continue;
                };
                if size < 2 {
                    continue;
                }
                let ramp = crate::config::night_light_ramp(size, tint);
                self.apply_gamma(&output, &ramp);
            } else {
                self.reset_gamma(&output);
            }
        }
    }

    pub fn handle_media_event(&mut self, event: glide_media::MediaEvent) {
        let dirty = match event {
            glide_media::MediaEvent::Gone => {
                self.media_art.clear();
                self.topbar.set_media(None)
            }
            glide_media::MediaEvent::Changed(track) => {
                let widget = self.media_art.show(&track, self.shell_palette);
                self.topbar.set_media(Some(widget))
            }
            glide_media::MediaEvent::Progress(progress) => {
                self.media_art.set_progress(progress);
                self.topbar.set_media_progress(progress)
            }
        };
        if dirty {
            self.queue_redraw();
        }
    }

    pub fn handle_media_art(&mut self, url: &str, art: crate::media_art::DecodedArt) {
        if let Some(widget) = self.media_art.accept(url, art)
            && self.topbar.set_media(Some(widget))
        {
            self.queue_redraw();
        }
    }

    pub fn run_media_command(&mut self, command: glide_media::Command) {
        if let Some(handle) = self.media.as_ref() {
            handle.send(command);
        }
    }

    pub fn run_media_key(&mut self, key: crate::input::MediaKey) {
        use crate::input::MediaKey;
        use glide_media::Command;
        const SEEK_STEP: std::time::Duration = std::time::Duration::from_secs(10);
        let Some(handle) = self.media.as_ref() else {
            tracing::debug!(?key, "no media player link; the media key does nothing");
            return;
        };
        match key {
            MediaKey::PlayPause => handle.send(Command::PlayPause),
            MediaKey::Play => handle.send(Command::Play),
            MediaKey::Pause => handle.send(Command::Pause),
            MediaKey::Stop => handle.send(Command::Stop),
            MediaKey::Next => handle.send(Command::Next),
            MediaKey::Previous => handle.send(Command::Previous),
            MediaKey::Forward => handle.seek_by(SEEK_STEP, true),
            MediaKey::Rewind => handle.seek_by(SEEK_STEP, false),
        }
    }

    pub fn handle_notify_event(&mut self, event: glide_notify::NotifyEvent) {
        let dirty = match event {
            glide_notify::NotifyEvent::Added(notification) => {
                self.remember_code(&notification);
                let sound = self.notifications.audible(&notification);
                let shown = self.notifications.push(&mut self.dock.icons, *notification, false);
                if let Some(sound) = sound.filter(|_| shown) {
                    crate::sound::play(sound);
                }
                shown
            }
            glide_notify::NotifyEvent::Replaced(notification) => {
                self.notifications.push(&mut self.dock.icons, *notification, true)
            }
            glide_notify::NotifyEvent::Closed(id, reason) => {
                if self.topbar.alert().is_some_and(|alert| alert.id == id) {
                    self.topbar.dismiss_alert();
                }
                self.notifications.withdraw(id, reason)
            }
            glide_notify::NotifyEvent::InhibitedChanged(on) => {
                self.notifications.set_inhibited(on);
                false
            }
        };
        self.show_glances();
        let topbar_dirty = self.refresh_topbar();
        if dirty || topbar_dirty {
            self.queue_redraw();
        }
    }

    pub fn sync_notice(&mut self) {
        let fullscreen = self.fullscreen_focused_on(self.output_rect());
        let covered = self.topbar.set_fullscreen(fullscreen);
        let notice = self.topbar.set_notice(self.notifications.notice());
        if covered || notice {
            self.queue_redraw();
        }
    }

    pub fn show_glances(&mut self) {
        self.sync_notice();
        if self.screen_locked() {
            return;
        }
        let mut shown = false;
        for glance in self.notifications.take_glances() {
            let current = self.topbar.alert().is_some_and(|alert| alert.id == glance.item.id);
            if glance.replaces && !current {
                continue;
            }
            let alert = crate::topbar::IslandAlert::from_item(&glance.item);
            shown |= self.topbar.show_alert(alert, std::time::Instant::now());
        }
        if shown {
            self.queue_redraw();
        }
    }

    pub fn run_notification_action(&mut self, action: crate::notifications::NotificationAction) {
        use crate::notifications::NotificationAction;
        match action {
            NotificationAction::Activate(id) => {
                self.activate_notification(id);
            }
            NotificationAction::Dismiss(id) => {
                self.notifications.remove(id, glide_notify::CloseReason::Dismissed, true);
            }
            NotificationAction::Button(id, index) => {
                let token = self.mint_granted_token(None);
                self.notifications.press_button(id, index, Some(&token));
            }
            NotificationAction::OpenReply(id) => {
                self.notifications.open_reply(id);
            }
            NotificationAction::SendReply(_) => {
                self.notifications.send_reply();
            }
            NotificationAction::Configure(id) => {
                self.notifications.configure(id, None);
            }
        }
        self.refresh_topbar();
        self.queue_redraw();
    }

    fn update_system_status(&mut self, update: impl FnOnce(&mut crate::system_info::SystemStatus)) {
        let mut status = self.topbar.system_status().clone();
        update(&mut status);
        self.topbar.set_system_status(status);
    }

    pub(crate) fn handle_qs_action(&mut self, action: crate::topbar::TopbarAction) {
        use crate::topbar::TopbarAction;
        match action {
            TopbarAction::QsToggleWifi => {
                let was_on = self.topbar.system_status().wifi_enabled;
                crate::system_info::set_wifi(!was_on);
                let label = if was_on { crate::i18n::Key::Off } else { crate::i18n::Key::Connecting };
                self.update_system_status(|status| {
                    status.wifi_enabled = !was_on;
                    status.wifi_level = 0;
                    status.wifi_ssid = crate::i18n::t(label).to_string();
                });
            }
            TopbarAction::QsToggleBluetooth => {
                let was_on = self.topbar.system_status().bluetooth_on;
                crate::system_info::set_bluetooth(!was_on);
                self.update_system_status(|status| status.bluetooth_on = !was_on);
            }
            TopbarAction::QsToggleNetbird => self.toggle_netbird(),
            TopbarAction::QsToggleNightLight => {
                let on = !self.topbar.system_status().night_light_on;
                self.update_system_status(|status| status.night_light_on = on);
                self.apply_night_light(on);
            }
            TopbarAction::QsToggleAirplane => {
                let on = !self.topbar.system_status().airplane;
                crate::system_info::set_airplane(on);
                self.update_system_status(|status| {
                    status.airplane = on;
                    if on {
                        status.wifi_enabled = false;
                        status.bluetooth_on = false;
                    }
                });
            }
            TopbarAction::QsSetPowerProfile(profile) => {
                if self.topbar.system_status().power_profile != Some(profile) {
                    crate::system_info::set_power_profile(profile);
                    self.update_system_status(|status| status.power_profile = Some(profile));
                }
            }
            TopbarAction::QsToggleDarkTheme => self.toggle_dark_theme(),
            TopbarAction::QsArmPower(power) => {
                self.topbar.arm_power(power);
            }
            TopbarAction::QsDisarmPower => {
                self.topbar.disarm_power();
            }
            TopbarAction::QsVolumeMute => {
                self.session.headset.local_volume_edit(std::time::Instant::now());
                crate::system_info::toggle_volume_mute();
                self.update_system_status(|status| status.volume_muted = !status.volume_muted);
            }
            TopbarAction::QsVolumeAdjust(pct) => {
                self.session.headset.local_volume_edit(std::time::Instant::now());
                let pct = pct.min(self.max_volume_pct);
                crate::system_info::set_volume_pct(pct);
                self.update_system_status(|status| {
                    status.volume_pct = pct;
                    status.volume_muted = false;
                });
            }
            TopbarAction::QsMicMute => {
                crate::system_info::toggle_mic_mute();
                self.update_system_status(|status| status.mic_muted = !status.mic_muted);
            }
            TopbarAction::QsMicAdjust(pct) => {
                crate::system_info::set_mic_pct(pct);
                self.update_system_status(|status| {
                    status.mic_pct = pct;
                    status.mic_muted = pct == 0;
                });
            }
            TopbarAction::QsToggleDrawer(drawer) => {
                if self.topbar.toggle_drawer(drawer) && self.topbar.drawer().is_some() {
                    crate::qs_net::request(crate::qs_net::scan_drawer(drawer));
                }
            }
            TopbarAction::QsUseDevice(drawer, index) => {
                if let Some(device) = self.topbar.drawer_device(drawer, index).filter(|device| !device.active) {
                    self.topbar.begin_connect(&device.name);
                    crate::qs_net::request(crate::qs_net::use_device(drawer, device.name));
                }
            }
            TopbarAction::QsBrightnessAdjust(pct) => {
                crate::system_info::set_brightness_pct(pct);
                self.update_system_status(|status| status.brightness_pct = pct);
            }
            TopbarAction::QsScreenshot => {
                self.topbar.close_popup();
                self.open_shot_after_menus();
            }
            TopbarAction::QsOpenSettings => {
                self.topbar.close_popup();
                crate::system_info::open_settings();
            }
            TopbarAction::QsPower(power) => self.run_power_button(power),
            TopbarAction::QsPanel(panel) => self.run_qs_panel_action(panel),
            TopbarAction::Tray(click) => self.run_tray_click(click),
            _ => {}
        }
    }

    pub fn run_topbar_action(&mut self, action: crate::topbar::TopbarAction) {
        use crate::topbar::{OpenPopup, TopbarAction};
        let opens_island = matches!(action, TopbarAction::ToggleNotificationCenter | TopbarAction::ToggleQuickSettings);
        if opens_island && (self.session.polkit.prompt.is_some() || self.session.keychain.prompt.is_some()) {
            if self.topbar.peek_notice() {
                self.queue_redraw();
            }
            return;
        }
        match action {
            TopbarAction::Polkit(control) => self.polkit_control(control),
            TopbarAction::Keychain(control) => self.keychain_control(control),
            TopbarAction::MediaPlayPause => self.run_media_command(glide_media::Command::PlayPause),
            TopbarAction::MediaNext => self.run_media_command(glide_media::Command::Next),
            TopbarAction::MediaPrevious => self.run_media_command(glide_media::Command::Previous),
            TopbarAction::MediaSeek(pct) => {
                if let Some(handle) = self.media.as_ref() {
                    handle.seek(f32::from(pct) / 100.0);
                }
            }
            TopbarAction::ToggleMediaPlayer => {
                self.topbar.toggle_popup(OpenPopup::MediaPlayer);
            }
            TopbarAction::CycleLayout => {
                self.cycle_keyboard_layout();
                self.refresh_topbar();
            }
            TopbarAction::SelectLayout(index) => {
                self.set_keyboard_layout(index);
                self.topbar.close_popup();
                self.refresh_topbar();
            }
            TopbarAction::ToggleCaffeine => {
                let on = !self.topbar.caffeinated();
                self.topbar.set_caffeinated(on);
                tracing::info!(caffeinated = on, "idle blanking toggled from the topbar");
                if on {
                    self.idle.activity(std::time::Instant::now());
                }
            }
            TopbarAction::ToggleDoNotDisturb => {
                let on = !self.topbar.dnd();
                self.notifications.set_dnd(on);
                self.topbar.set_dnd(on);
                tracing::info!(dnd = on, "do not disturb toggled from the topbar");
            }
            TopbarAction::ToggleNotificationCenter => {
                self.topbar.toggle_popup(OpenPopup::NotificationCenter);
                if self.topbar.open_popup() == Some(OpenPopup::NotificationCenter) {
                    self.notifications.mark_read();
                    self.refresh_topbar();
                }
            }
            TopbarAction::ToggleLanguageMenu => {
                self.topbar.toggle_popup(OpenPopup::LanguageMenu);
            }
            TopbarAction::ToggleQuickSettings => {
                self.topbar.toggle_popup(OpenPopup::QuickSettings);
                if self.topbar.open_popup() == Some(OpenPopup::QuickSettings) {
                    crate::qs_net::request(crate::qs_net::NetRequest::ScanOutputs);
                    crate::qs_net::request(crate::qs_net::NetRequest::ScanInputs);
                }
            }
            TopbarAction::ClearAllNotifications
            | TopbarAction::ActivateNotification(_)
            | TopbarAction::DismissNotification(_)
            | TopbarAction::DismissIslandAlert
            | TopbarAction::Notification(_)
            | TopbarAction::PeekNotice => self.run_notification_topbar_action(action),
            TopbarAction::OpenInputSettings => {
                self.topbar.close_popup();
                let _ = crate::spawn_shell_command(
                    "glide-settings keyboard || gnome-control-center keyboard || kcmshell6 kcm_keyboard",
                );
            }
            TopbarAction::ClosePopup => {
                self.topbar.close_popup();
            }
            TopbarAction::OpenAppMenu(index) => self.open_app_menu(index),
            TopbarAction::AppMenuRow(path) => self.app_menu_row(path),
            TopbarAction::ToggleLauncher => {
                self.topbar.close_popup();
                self.toggle_launcher();
            }
            TopbarAction::TogglePrivacy => {
                self.topbar.toggle_popup(OpenPopup::Privacy);
            }
            TopbarAction::ToggleHeadset
            | TopbarAction::HeadsetUseOutput
            | TopbarAction::HeadsetDisconnect
            | TopbarAction::HeadsetSettings => self.run_headset_action(action),
            action if action.for_passwords() => self.run_passwords_action(action),
            TopbarAction::CalendarShift(delta) => {
                self.topbar.shift_calendar(i32::from(delta));
            }
            TopbarAction::CalendarToday => {
                self.topbar.calendar_today();
            }
            other => self.handle_qs_action(other),
        }
        self.queue_redraw();
    }

    fn run_notification_topbar_action(&mut self, action: crate::topbar::TopbarAction) {
        use crate::topbar::TopbarAction;
        match action {
            TopbarAction::ClearAllNotifications => {
                self.notifications.clear_all();
                self.refresh_topbar();
            }
            TopbarAction::ActivateNotification(id) => {
                self.activate_notification(id);
                self.refresh_topbar();
            }
            TopbarAction::DismissNotification(id) => {
                self.notifications.dismiss(id);
                self.refresh_topbar();
            }
            TopbarAction::DismissIslandAlert => {
                self.topbar.dismiss_alert();
            }
            TopbarAction::Notification(action) => {
                self.run_notification_action(action);
                self.sync_notice();
            }
            TopbarAction::PeekNotice => {
                self.topbar.peek_notice();
            }
            _ => {}
        }
    }

    pub fn broadcast_layout(&mut self) {
        let Some((index, name)) = self.active_layout() else {
            return;
        };
        self.ipc_subscribers.broadcast(&glide_ipc::Event::LayoutChanged { index, name });
    }

    pub fn apply_input_config(&mut self) {
        self.layout_memory.set_policy(self.input_config.layout_scope, self.input_config.new_window_layout);
        let rebuild = self.applied_config.keymap_changed(&self.input_config);
        let numlock_changed = self.applied_config.numlock_changed(self.input_config.numlock);
        if let Some(keyboard) = self.seat.get_keyboard() {
            if rebuild && let Err(err) = keyboard.set_xkb_config(self, self.input_config.clone().xkb()) {
                tracing::error!(?err, "failed to apply the xkb config");
            }
            if rebuild {
                self.session.virtual_keymaps.seat_keymap_changed();
                self.sync_topbar_layout();
            }
            if rebuild || numlock_changed {
                let current = keyboard.modifier_state();
                let kept = if rebuild { locks_only(current) } else { current };
                let mods = smithay::input::keyboard::ModifiersState { num_lock: self.input_config.numlock, ..kept };
                replace_modifiers(&keyboard, self, mods, !rebuild);
            }
            keyboard.change_repeat_info(self.input_config.repeat_rate, self.input_config.repeat_delay);
        }
        let mut devices = std::mem::take(&mut self.input_devices);
        for device in &mut devices {
            self.configure_input_device(device);
        }
        self.input_devices = devices;
        self.sync_keyboard_leds();
    }

    pub fn sync_keyboard_leds(&mut self) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let leds = crate::input::libinput_leds(keyboard.led_state());
        for device in &mut self.input_devices {
            if device.has_capability(smithay::reexports::input::DeviceCapability::Keyboard) {
                device.led_update(leds);
            }
        }
    }

    pub fn release_held_keys(&mut self, reason: &'static str) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let Some((held, stuck)) = release_keyboard(&keyboard, self) else {
            return;
        };
        tracing::info!(held, stuck, reason, "released keys the keyboard still counted as pressed");
        self.suppressed_keys.clear();
        self.sync_greeter_layout();
        self.queue_redraw();
    }

    pub fn configure_input_device(&self, device: &mut smithay::reexports::input::Device) {
        use crate::config::{ClickMethodConfig, ScrollMethodConfig};
        use smithay::reexports::input::{AccelProfile, ClickMethod, ScrollMethod};

        let config = &self.input_config;
        let touchpad = crate::config::is_touchpad(device);
        let pointer = config.pointer_settings(touchpad);
        if device.config_tap_finger_count() > 0 {
            let _ = device.config_tap_set_enabled(config.tap_to_click);
            let _ = device.config_tap_set_drag_enabled(config.tap_and_drag);
        }
        if device.config_scroll_has_natural_scroll() {
            let _ = device.config_scroll_set_natural_scroll_enabled(pointer.natural_scroll);
        }
        if device.config_accel_is_available() {
            let profile = match pointer.accel_profile {
                crate::config::AccelProfileConfig::Adaptive => AccelProfile::Adaptive,
                crate::config::AccelProfileConfig::Flat => AccelProfile::Flat,
            };
            let _ = device.config_accel_set_profile(profile);
            let _ = device.config_accel_set_speed(pointer.accel_speed);
        }
        if device.config_left_handed_is_available() {
            let _ = device.config_left_handed_set(config.left_handed);
        }
        if device.config_middle_emulation_is_available() {
            let _ = device.config_middle_emulation_set_enabled(config.middle_emulation);
        }
        if device.config_dwt_is_available() {
            let _ = device.config_dwt_set_enabled(config.disable_while_typing);
        }
        if touchpad {
            let method = match config.scroll_method {
                ScrollMethodConfig::TwoFinger => ScrollMethod::TwoFinger,
                ScrollMethodConfig::Edge => ScrollMethod::Edge,
                ScrollMethodConfig::None => ScrollMethod::NoScroll,
            };
            if method == ScrollMethod::NoScroll || device.config_scroll_methods().contains(&method) {
                let _ = device.config_scroll_set_method(method);
            }
        }
        let click = match config.click_method {
            Some(ClickMethodConfig::Areas) => Some(ClickMethod::ButtonAreas),
            Some(ClickMethodConfig::Clickfinger) => Some(ClickMethod::Clickfinger),
            None => device.config_click_default_method(),
        };
        if let Some(click) = click
            && device.config_click_methods().contains(&click)
        {
            let _ = device.config_click_set_method(click);
        }
    }

    #[must_use]
    pub fn any_button_hover_animating(&self) -> bool {
        self.window_map.values().any(|window| {
            window.try_decoration_state().is_some_and(|state| {
                state.header.is_hover_animating()
                    || state.box_deco_fade.as_ref().is_some_and(|fade| fade.alpha().is_some())
            })
        })
    }

    fn capture_closing_window(&mut self, id: glide_layout::WindowId, window: &GlideWindow) {
        let overview_rect = self.overview.as_ref().and_then(|ui| ui.preview_rect(id));
        let Some(target_frame) = overview_rect.or_else(|| self.frame_rect(id)) else {
            return;
        };
        self.push_snapshot(|snapshots, renderer, scale, output_scale| {
            snapshots.push_close(renderer, window, target_frame, scale, output_scale)
        });
    }

    fn push_snapshot(
        &mut self,
        push: impl FnOnce(
            &mut crate::snapshot::Snapshots,
            &mut smithay::backend::renderer::gles::GlesRenderer,
            smithay::utils::Scale<f64>,
            i32,
        ) -> bool,
    ) {
        let Some((scale, output_scale)) = self.output_scales() else {
            return;
        };
        let Some(device) = self.backend_data.device.as_mut() else {
            return;
        };
        push(&mut self.snapshots, &mut device.gles_renderer, scale, output_scale);
    }

    pub fn restore_for_move(&mut self, window: &GlideWindow, pointer: Point<f64, Logical>) -> bool {
        if !self.is_size_locked(window) {
            return false;
        }
        let Some(id) = self.window_id(window) else {
            return false;
        };
        let Some(from) = self.frame_rect(id) else {
            return false;
        };

        self.layout.set_fullscreen(id, false);
        self.layout.set_maximized(id, false);

        let Some(restored) = self.layout.window_state(id).map(glide_layout::WindowState::frame_geometry) else {
            return false;
        };

        let grip = if from.size.w > 0 { (pointer.x - f64::from(from.loc.x)) / f64::from(from.size.w) } else { 0.5 };
        let new_x = (pointer.x - grip * f64::from(restored.size.width)).round() as i32;
        let new_y = from.loc.y;

        self.layout.set_window_geometry(
            id,
            glide_layout::Rectangle::new(glide_layout::Point::new(new_x, new_y), restored.size),
        );
        self.reconcile_space();

        let to = self.frame_rect(id).unwrap_or(from);
        self.push_snapshot(|snapshots, renderer, scale, output_scale| {
            snapshots.push_morph(renderer, window, from, to, scale, output_scale)
        });

        true
    }

    pub fn begin_interactive_launcher(&mut self, progress: f64) {
        if self.launcher.is_none() {
            let area = self.launcher_area();
            let mut launcher = crate::launcher::Launcher::new(Vec::new(), area)
                .with_scene(self.overview_theme.duration())
                .with_options(self.launcher_options.clone());
            launcher.set_windows(self.open_windows_for_search());
            launcher.set_providers(self.search_providers());
            self.launcher = Some(launcher);
            self.spawn_launcher_loader();
            self.arm_launcher_caret();
        }
        if let Some(launcher) = self.launcher.as_mut() {
            launcher.set_interactive_progress(progress);
        }
        self.queue_redraw();
    }

    pub fn toggle_launcher(&mut self) {
        match self.launcher.as_mut() {
            Some(launcher) if !launcher.is_closing() => launcher.begin_close(),
            Some(launcher) => launcher.reopen(),
            None => {
                self.begin_close_overview(None);
                let area = self.launcher_area();
                let mut launcher = crate::launcher::Launcher::new(Vec::new(), area)
                    .with_scene(self.overview_theme.duration())
                    .with_options(self.launcher_options.clone());
                launcher.set_windows(self.open_windows_for_search());
                launcher.set_providers(self.search_providers());
                self.launcher = Some(launcher);
                self.spawn_launcher_loader();
            }
        }
        self.arm_launcher_caret();
        self.queue_redraw();
    }

    fn spawn_launcher_loader(&mut self) {
        let icon_theme = self.dock.icon_theme();
        let output_scale = self.space.outputs().next().map_or(1, |output| output.current_scale().integer_scale());

        let (sender, receiver) = smithay::reexports::calloop::channel::channel();
        std::thread::spawn(move || {
            let apps = crate::icon::list_applications();
            let history = crate::launcher::history::LaunchHistory::load();
            let mut loader = crate::icon::IconLoader::new(icon_theme);
            let side = crate::launcher::icon_side() * output_scale.max(1);
            let pixels: Vec<Option<Vec<u8>>> =
                apps.iter().map(|app| loader.load_named(&app.icon, side).map(<[u8]>::to_vec)).collect();
            let recent = crate::launcher::recent::load();
            let _ = sender.send((apps, pixels, output_scale, history, recent));
        });

        let result = self.loop_handle.insert_source(receiver, |event, (), state| {
            if let smithay::reexports::calloop::channel::Event::Msg((apps, pixels, scale, history, recent)) = event {
                tracing::info!(count = apps.len(), recent = recent.len(), "launcher apps loaded");
                if let Some(launcher) = state.launcher.as_mut() {
                    launcher.set_history(history);
                    launcher.set_recent(recent);
                    launcher.set_apps(apps, &pixels, scale);
                }
                state.queue_redraw();
            }
        });
        if let Err(err) = result {
            tracing::error!(?err, "failed to register the launcher loader");
        }
    }

    pub fn tick_launcher(&mut self) -> bool {
        let Some(launcher) = self.launcher.as_mut() else {
            return false;
        };
        if !launcher.tick() {
            self.launcher = None;
            return true;
        }
        launcher.is_animating()
    }

    fn switcher_candidates(&self) -> Vec<crate::switcher::Candidate> {
        let mut order: Vec<glide_layout::WindowId> =
            self.focus_history.iter().copied().filter(|id| self.layout.window_state(*id).is_some()).collect();
        let active = self.layout.active_workspace();
        let mut rest: Vec<(bool, usize, glide_layout::WindowId)> = Vec::new();
        for (ws_index, workspace) in self.layout.workspaces().enumerate() {
            for id in workspace.windows.iter().rev() {
                if !order.contains(id) {
                    rest.push((workspace.id != active, ws_index, *id));
                }
            }
        }
        rest.sort_by_key(|(other, ws_index, _)| (*other, *ws_index));
        order.extend(rest.into_iter().map(|(_, _, id)| id));

        order
            .into_iter()
            .filter_map(|id| {
                let window = self.window_map.get(&id)?;
                let (app_id, title) = window.identity();
                Some(crate::switcher::Candidate {
                    id,
                    app_id: app_id.unwrap_or_default(),
                    title: title.unwrap_or_default(),
                })
            })
            .collect()
    }

    fn cached_app_name(&self, app_id: &str) -> Option<String> {
        if let Some(Some(name)) = self.switcher_names.get(&app_id.to_lowercase()) {
            return Some(name.clone());
        }
        self.dock
            .pinned_items()
            .iter()
            .find(|item| item.app_id.eq_ignore_ascii_case(app_id))
            .map(|item| item.name.clone())
    }

    fn switcher_app_entries(&mut self, candidates: &[crate::switcher::Candidate]) -> Vec<crate::switcher::Entry> {
        let mut missing: Vec<String> = Vec::new();
        let entries = crate::switcher::app_entries(candidates, |app_id| {
            let name = self.cached_app_name(app_id);
            if name.is_none() && !self.switcher_names.contains_key(&app_id.to_lowercase()) {
                missing.push(app_id.to_string());
            }
            name
        });
        self.resolve_app_names(missing);
        entries
    }

    fn resolve_app_names(&mut self, missing: Vec<String>) {
        if missing.is_empty() {
            return;
        }
        for app_id in &missing {
            self.switcher_names.insert(app_id.to_lowercase(), None);
        }
        let (sender, receiver) = smithay::reexports::calloop::channel::channel();
        std::thread::spawn(move || {
            let names: Vec<(String, Option<String>)> = missing
                .into_iter()
                .map(|app_id| {
                    let name = crate::icon::find_application(&app_id).map(|app| app.name);
                    (app_id, name)
                })
                .collect();
            let _ = sender.send(names);
        });
        let result = self.loop_handle.insert_source(receiver, |event, (), state| {
            if let smithay::reexports::calloop::channel::Event::Msg(names) = event {
                let mut relabelled = false;
                for (app_id, name) in names {
                    if let (Some(switcher), Some(name)) = (state.switcher.as_mut(), &name) {
                        relabelled |= switcher.relabel_app(&app_id, name);
                    }
                    state.switcher_names.insert(app_id.to_lowercase(), name);
                }
                if relabelled {
                    state.queue_redraw();
                }
            }
        });
        if let Err(err) = result {
            tracing::error!(?err, "failed to register the app name resolver");
        }
    }

    pub fn open_switcher(&mut self, backward: bool, mode: crate::switcher::Mode, trigger: crate::switcher::Trigger) {
        if let Some(switcher) = self.switcher.as_mut()
            && !switcher.is_closing()
        {
            if switcher.step(if backward { -1 } else { 1 }) {
                self.queue_redraw();
            }
            return;
        }
        if self.screen_locked() || self.layout.overview_active() {
            return;
        }
        let candidates = self.switcher_candidates();
        let focused = self.layout.focused_window();
        let entries = match mode {
            crate::switcher::Mode::Apps => self.switcher_app_entries(&candidates),
            crate::switcher::Mode::Windows => {
                let Some(anchor) = focused.or_else(|| candidates.first().map(|c| c.id)) else {
                    return;
                };
                crate::switcher::sibling_entries(&candidates, anchor)
            }
        };
        if entries.is_empty() {
            return;
        }
        let focused_first =
            focused.is_some_and(|id| entries.first().is_some_and(|entry| entry.windows.first() == Some(&id)));
        let selected = match (backward, focused_first) {
            (true, _) => entries.len() - 1,
            (false, true) => 1,
            (false, false) => 0,
        };
        self.switcher = Some(crate::switcher::Switcher::new(entries, selected, mode, trigger));
        self.queue_redraw();
    }

    #[must_use]
    pub fn current_trigger(&self) -> crate::switcher::Trigger {
        self.seat
            .get_keyboard()
            .map(|keyboard| crate::input::held_trigger(&keyboard.modifier_state()))
            .unwrap_or_default()
    }

    pub fn switcher_show_app_windows(&mut self) {
        let Some(anchor) = self
            .switcher
            .as_ref()
            .filter(|switcher| !switcher.is_closing() && switcher.mode() == crate::switcher::Mode::Apps)
            .and_then(crate::switcher::Switcher::selected_id)
        else {
            return;
        };
        let candidates = self.switcher_candidates();
        let entries = crate::switcher::sibling_entries(&candidates, anchor);
        if entries.is_empty() {
            return;
        }
        let selected = usize::from(entries.len() > 1 && self.layout.focused_window() == Some(entries[0].id));
        if let Some(switcher) = self.switcher.as_mut() {
            switcher.replace(entries, selected, crate::switcher::Mode::Windows);
        }
        self.queue_redraw();
    }

    pub fn switcher_close_app(&mut self) {
        let Some(entry) = self
            .switcher
            .as_mut()
            .filter(|switcher| !switcher.is_closing())
            .and_then(crate::switcher::Switcher::remove_selected)
        else {
            return;
        };
        let windows = self.app_windows(&entry);
        for id in windows {
            self.send_close_to(id);
        }
        self.close_switcher_if_empty();
        self.queue_redraw();
    }

    pub fn switcher_close_window(&mut self) {
        let Some(id) = self
            .switcher
            .as_ref()
            .filter(|switcher| !switcher.is_closing())
            .and_then(crate::switcher::Switcher::selected_id)
        else {
            return;
        };
        if let Some(switcher) = self.switcher.as_mut() {
            switcher.remove(id);
        }
        self.send_close_to(id);
        self.close_switcher_if_empty();
        self.queue_redraw();
    }

    pub fn switcher_hide_app(&mut self) {
        let Some(entry) = self
            .switcher
            .as_ref()
            .filter(|switcher| !switcher.is_closing())
            .and_then(crate::switcher::Switcher::selected_entry)
            .cloned()
        else {
            return;
        };
        let windows = self.app_windows(&entry);
        for id in windows {
            if self.layout.window_state(id).is_some_and(|state| !state.minimized) {
                self.minimize_with_flight(id);
            }
        }
        self.queue_redraw();
    }

    fn app_windows(&self, entry: &crate::switcher::Entry) -> Vec<glide_layout::WindowId> {
        if entry.app_id.is_empty() {
            return entry.windows.clone();
        }
        self.focused_app_windows_of(&entry.app_id)
    }

    fn focused_app_windows_of(&self, app_id: &str) -> Vec<glide_layout::WindowId> {
        self.window_map
            .iter()
            .filter(|(_, window)| window.identity().0.as_deref() == Some(app_id))
            .map(|(id, _)| *id)
            .collect()
    }

    pub(crate) fn send_close_to(&self, id: glide_layout::WindowId) {
        if let Some(window) = self.window_map.get(&id)
            && let Some(toplevel) = window.toplevel()
        {
            toplevel.send_close();
        }
    }

    fn close_switcher_if_empty(&mut self) {
        if let Some(switcher) = self.switcher.as_mut()
            && switcher.entries().is_empty()
        {
            switcher.begin_close();
        }
    }

    pub fn close_focused_app(&mut self) {
        let Some(focused) = self.layout.focused_window() else {
            return;
        };
        let app_id =
            self.window_map.get(&focused).and_then(|window| window.identity().0).filter(|app_id| !app_id.is_empty());
        let windows = app_id.map_or_else(|| vec![focused], |app_id| self.focused_app_windows_of(&app_id));
        tracing::info!(count = windows.len(), "closing the focused app");
        for id in windows {
            self.send_close_to(id);
        }
    }

    #[must_use]
    pub fn switcher_open(&self) -> bool {
        self.switcher.as_ref().is_some_and(|switcher| !switcher.is_closing())
    }

    pub fn step_switcher(&mut self, delta: i32) {
        if let Some(switcher) = self.switcher.as_mut()
            && switcher.step(delta)
        {
            self.queue_redraw();
        }
    }

    pub fn cancel_switcher(&mut self) {
        if let Some(switcher) = self.switcher.as_mut() {
            switcher.begin_close();
            self.queue_redraw();
        }
    }

    pub fn commit_switcher(&mut self) {
        let Some(switcher) = self.switcher.as_mut().filter(|s| !s.is_closing()) else {
            return;
        };
        let windows = switcher.selected_entry().map(|entry| entry.windows.clone()).unwrap_or_default();
        switcher.begin_close();
        self.queue_redraw();
        let visible =
            windows.iter().copied().find(|id| self.layout.window_state(*id).is_some_and(|state| !state.minimized));
        let Some(id) = visible.or_else(|| windows.first().copied()) else {
            return;
        };
        self.activate_window_animated(id);
    }

    pub fn activate_window_animated(&mut self, id: glide_layout::WindowId) {
        let Some(state) = self.layout.window_state(id) else {
            return;
        };
        let (workspace, minimized) = (state.workspace_id, state.minimized);
        if workspace != self.layout.active_workspace() {
            self.switch_workspace_animated(glide_layout::WorkspaceTarget::Id(workspace));
            self.sync_ext_workspaces();
        }
        if minimized {
            self.minimize_with_flight(id);
        }
        self.focus_window(id);
    }

    pub fn switcher_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        let area = self.launcher_area();
        let Some(switcher) = self.switcher.as_mut().filter(|s| !s.is_closing()) else {
            return false;
        };
        let geometry = switcher.geometry(area);
        if switcher.openness() > 0.0
            && let Some(index) = geometry.cell_at(switcher.entries().len(), pos)
            && switcher.select(index)
        {
            self.queue_redraw();
        }
        true
    }

    pub fn switcher_button(&mut self, pos: Point<f64, Logical>) -> bool {
        let area = self.launcher_area();
        let Some(switcher) = self.switcher.as_mut().filter(|s| !s.is_closing()) else {
            return false;
        };
        let geometry = switcher.geometry(area);
        match geometry.cell_at(switcher.entries().len(), pos) {
            Some(index) => {
                switcher.select(index);
                self.commit_switcher();
            }
            None => self.cancel_switcher(),
        }
        true
    }

    pub fn tick_switcher(&mut self) -> bool {
        let Some(switcher) = self.switcher.as_mut() else {
            return false;
        };
        if !switcher.tick() {
            self.switcher = None;
            return true;
        }
        switcher.is_animating()
    }

    fn launcher_app(&self, result: usize) -> Option<crate::icon::DesktopApp> {
        let launcher = self.launcher.as_ref()?;
        let index = launcher.app_index(result)?;
        launcher.apps().get(index).cloned()
    }

    #[must_use]
    pub fn launcher_app_is_pinned(&self, result: usize) -> bool {
        self.launcher_app(result).is_some_and(|app| self.dock.is_app_pinned(&app.id))
    }

    pub fn run_launcher_menu_item(&mut self, result: usize, item: crate::launcher::LauncherMenuItem) {
        match item {
            crate::launcher::LauncherMenuItem::Open => self.activate_launcher_app(result),
            crate::launcher::LauncherMenuItem::Pin | crate::launcher::LauncherMenuItem::Unpin => {
                let Some(app) = self.launcher_app(result) else {
                    return;
                };
                let changed = if matches!(item, crate::launcher::LauncherMenuItem::Pin) {
                    self.dock.pin_app(&app.id, &app.name, &app.command)
                } else {
                    self.dock.unpin_app(&app.id)
                };
                if changed {
                    crate::config::save_dock_pins(self.dock.pinned_items());
                    self.refresh_dock();
                }
                self.queue_redraw();
            }
        }
    }

    pub fn activate_launcher_app(&mut self, result: usize) {
        let index = self.launcher.as_ref().and_then(|launcher| launcher.app_index(result));
        if let Some(index) = index {
            self.activate_launcher_entry(crate::launcher::search::Entry::App(index));
        }
    }

    #[must_use]
    pub fn launcher_area(&self) -> Rectangle<i32, Logical> {
        self.dock_area()
    }

    #[must_use]
    pub fn output_rect(&self) -> Rectangle<i32, Logical> {
        logical_rect(self.layout.primary_output_geometry())
    }

    #[must_use]
    pub fn dock_area(&self) -> Rectangle<i32, Logical> {
        logical_rect(self.layout.primary_output_work_area())
    }

    #[must_use]
    pub fn focused_fullscreen_center(&self) -> Option<Point<i32, Logical>> {
        if self.layout.overview_active() {
            return None;
        }
        self.layout
            .focused_window()
            .and_then(|id| self.layout.window_state(id))
            .filter(|window| window.fullscreen && !window.minimized)
            .map(|window| {
                let frame = window.frame_geometry();
                Point::from((frame.loc.x + frame.size.width / 2, frame.loc.y + frame.size.height / 2))
            })
    }

    #[must_use]
    pub fn fullscreen_focused_on(&self, output: Rectangle<i32, Logical>) -> bool {
        self.focused_fullscreen_center().is_some_and(|center| output.contains(center))
    }

    #[must_use]
    pub fn topbar_suppressed_on(&self, output: Rectangle<i32, Logical>) -> bool {
        !self.topbar.has_open_popup() && self.fullscreen_focused_on(output)
    }

    #[must_use]
    pub fn dock_suppressed(&self) -> bool {
        self.dock.autohide() == crate::config::DockAutohide::Never
            && self.dock.menu_index().is_none()
            && !self.dock.is_dragging()
            && self.fullscreen_focused_on(self.output_rect())
    }

    #[must_use]
    pub fn dock_obstructed(&self) -> bool {
        let dock = self.dock.rect(self.dock_area());
        self.layout
            .visible_windows()
            .any(|state| !state.minimized && logical_rect(state.frame_geometry()).overlaps(dock))
    }

    pub fn update_dock_autohide(&mut self) -> bool {
        if !self.dock.is_enabled() {
            return false;
        }
        let pointer = self.seat.get_pointer().map(|pointer| pointer.current_location());
        let area = self.dock_area();
        let in_zone = !self.screen_locked()
            && !self.layout.overview_active()
            && pointer.is_some_and(|pos| self.dock.in_reveal_zone(area, pos));
        let obstructed = self.dock_obstructed();
        self.dock.update_autohide(obstructed, in_zone)
    }

    pub fn toggle_dock_pin(&mut self, index: usize) {
        let app_id = self
            .dock
            .items()
            .get(index)
            .filter(|item| item.action == crate::dock::DockAction::Activate)
            .map(|item| item.app_id.clone());
        let Some(app_id) = app_id else {
            return;
        };

        let identity = self.pin_identity(&app_id);
        if !self.dock.toggle_pinned(index, identity) {
            return;
        }

        crate::config::save_dock_pins(self.dock.pinned_items());
        self.refresh_dock();
        self.queue_redraw();
    }

    pub fn drop_dock_tile(&mut self, index: usize, target: usize) {
        let from = if self.dock.is_pinned(index) {
            index
        } else {
            let app_id = self.dock.items().get(index).map(|item| item.app_id.clone()).unwrap_or_default();
            let identity = self.pin_identity(&app_id);
            if !self.dock.toggle_pinned(index, identity) {
                self.queue_redraw();
                return;
            }
            self.dock.pinned_items().len() - 1
        };
        self.dock.move_pinned(from, target);
        crate::config::save_dock_pins(self.dock.pinned_items());
        self.refresh_dock();
        self.queue_redraw();
    }

    pub fn begin_window_open_animation(&self, id: glide_layout::WindowId, window: &GlideWindow) {
        if self.layout.window_state(id).is_some_and(|s| s.maximized || s.fullscreen) {
            return;
        }
        let Some(frame) = self.frame_rect(id) else {
            return;
        };
        let origin = self.dock_key(id).and_then(|key| self.dock.tile_rect_for_app(self.dock_area(), &key));
        window.begin_open_animation(origin, frame);
    }

    fn frame_rect(&self, id: glide_layout::WindowId) -> Option<Rectangle<i32, Logical>> {
        Some(logical_rect(self.layout.window_state(id)?.frame_geometry()))
    }

    fn push_morph(&mut self, window: &GlideWindow, from: Rectangle<i32, Logical>, to: Rectangle<i32, Logical>) {
        window.begin_geometry_animation(from, to);
        self.push_snapshot(|snapshots, renderer, scale, output_scale| {
            snapshots.push_morph(renderer, window, from, to, scale, output_scale)
        });
    }

    pub fn morph_geometry<F>(&mut self, id: glide_layout::WindowId, mutate: F)
    where
        F: FnOnce(&mut Self),
    {
        let before = self.frame_rect(id);
        let window = self.window_map.get(&id).cloned();

        mutate(self);
        self.reconcile_space();

        let (Some(window), Some(from), Some(to)) = (window, before, self.frame_rect(id)) else {
            return;
        };
        if to != from {
            self.push_morph(&window, from, to);
        }
    }

    #[must_use]
    pub fn any_window_geometry_animating(&self) -> bool {
        self.window_map.values().any(crate::window::GlideWindow::is_geometry_animating)
    }

    pub fn switch_workspace_animated(&mut self, target: glide_layout::WorkspaceTarget) {
        self.finish_workspace_swipe();
        let before_index = self.active_workspace_index();
        let before: Vec<(glide_layout::WindowId, Rectangle<i32, Logical>)> =
            self.layout.visible_windows().filter_map(|state| Some((state.id, self.frame_rect(state.id)?))).collect();

        self.layout.switch_to_workspace(target);
        self.reconcile_space();

        let after_index = self.active_workspace_index();
        if after_index == before_index {
            return;
        }

        let Some(travel) = self.workspace_travel(before_index, after_index) else {
            return;
        };

        let after: Vec<(glide_layout::WindowId, Rectangle<i32, Logical>)> =
            self.layout.visible_windows().filter_map(|state| Some((state.id, self.frame_rect(state.id)?))).collect();

        for (id, rect) in &after {
            let Some(window) = self.window_map.get(id).cloned() else {
                continue;
            };
            let mut from = *rect;
            from.loc.x -= travel;
            window.begin_geometry_animation(from, *rect);
        }

        let arriving: HashSet<glide_layout::WindowId> = after.iter().map(|(id, _)| *id).collect();
        for (id, rect) in before {
            if arriving.contains(&id) {
                continue;
            }
            let Some(window) = self.window_map.get(&id).cloned() else {
                continue;
            };
            let mut to = rect;
            to.loc.x += travel;
            self.push_snapshot(|snapshots, renderer, scale, output_scale| {
                snapshots.push_slide(renderer, &window, rect, to, scale, output_scale)
            });
        }
    }

    #[must_use]
    pub fn active_workspace_index(&self) -> Option<usize> {
        let active = self.layout.active_workspace();
        self.layout.workspaces().position(|workspace| workspace.id == active)
    }

    fn workspace_travel(&self, before: Option<usize>, after: Option<usize>) -> Option<i32> {
        let (before, after) = (before?, after?);
        let width = self.widest_output()?;
        if width <= 0 {
            return None;
        }
        Some(if after > before { -width } else { width })
    }

    pub fn toggle_show_desktop(&mut self) {
        let hidden = std::mem::take(&mut self.gestures.shown_desktop);
        let restorable: Vec<_> =
            hidden.into_iter().filter(|id| self.layout.window_state(*id).is_some_and(|s| s.minimized)).collect();
        if !restorable.is_empty() {
            for id in restorable.iter().rev() {
                self.minimize_with_flight(*id);
            }
            if let Some(&top) = restorable.first() {
                self.focus_window(top);
            }
            return;
        }
        let visible: Vec<_> = self
            .active_workspace_index()
            .map(|index| self.layout.workspace_windows_at(index))
            .unwrap_or_default()
            .into_iter()
            .filter(|state| !state.minimized)
            .map(|state| state.id)
            .collect();
        for &id in &visible {
            self.minimize_with_flight(id);
        }
        self.gestures.shown_desktop = visible;
    }

    pub fn minimize_with_flight(&mut self, id: glide_layout::WindowId) {
        let restoring = self.layout.window_state(id).is_some_and(|state| state.minimized);
        if restoring {
            let origin = self.minimize_target(id);
            self.layout.toggle_minimize(id);
            self.reconcile_space();

            if let (Some(origin), Some(frame), Some(window)) =
                (origin, self.frame_rect(id), self.window_map.get(&id).cloned())
            {
                window.begin_open_animation(Some(origin), frame);
            }
            return;
        }

        if let (Some(from), Some(window), Some(to)) =
            (self.frame_rect(id), self.window_map.get(&id).cloned(), self.minimize_target(id))
        {
            self.push_snapshot(|snapshots, renderer, scale, output_scale| {
                snapshots.push_flight(renderer, &window, from, to, scale, output_scale)
            });
        }

        self.layout.toggle_minimize(id);
        self.reconcile_space();
    }

    fn minimize_target(&self, id: glide_layout::WindowId) -> Option<Rectangle<i32, Logical>> {
        let area = self.dock_area();
        if !self.dock.is_enabled() {
            return None;
        }

        let tile = self.dock_key(id).and_then(|key| self.dock.tile_rect_for_app(area, &key));
        if tile.is_some() {
            return tile;
        }

        let dock = self.dock.rect(area);
        let side = dock.size.h.max(1);
        Some(Rectangle::new(
            Point::from((dock.loc.x + (dock.size.w - side) / 2, dock.loc.y)),
            smithay::utils::Size::from((side, side)),
        ))
    }

    pub(crate) fn output_scales(&self) -> Option<(smithay::utils::Scale<f64>, i32)> {
        let output = self.space.outputs().next()?;
        Some((
            smithay::utils::Scale::from(output.current_scale().fractional_scale()),
            output.current_scale().integer_scale(),
        ))
    }

    #[must_use]
    pub fn any_window_open_animating(&self) -> bool {
        self.window_map.values().any(crate::window::GlideWindow::is_open_animating)
    }

    pub fn refresh_painter_font(&self) {
        let font_px = (f64::from(self.decoration_theme.height).max(14.0) * 0.45) as f32;
        crate::paint::init_or_replace(font_px.clamp(10.0, 18.0), self.font_family.as_deref());
    }

    pub fn close_from_overview(&mut self, id: glide_layout::WindowId) {
        use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
        tracing::info!(window = ?id, "closing from the overview");
        self.send_close_to(id);
        let newest = self.window_map.keys().max().copied().unwrap_or(id);
        let Some(ui) = self.overview.as_mut() else {
            return;
        };
        ui.watch_close(id, newest);
        let timer = Timer::from_duration(crate::overview::close::GRACE);
        let inserted = self.loop_handle.insert_source(timer, |_, (), state| {
            state.watch_overview_close();
            TimeoutAction::Drop
        });
        if let Err(err) = inserted {
            tracing::warn!(?err, "failed to schedule the overview close check");
        }
    }

    fn watch_overview_close(&mut self) {
        if !self.overview.as_ref().is_some_and(crate::overview::OverviewUi::awaits_close) {
            return;
        }
        let live: Vec<crate::overview::close::Live> = self
            .window_map
            .iter()
            .map(|(id, window)| crate::overview::close::Live {
                id: *id,
                parent: window
                    .toplevel()
                    .and_then(smithay::wayland::shell::xdg::ToplevelSurface::parent)
                    .and_then(|parent| self.window_id_for_surface(&parent)),
                pid: self.window_pid(window),
            })
            .collect();
        let Some(id) = self.overview.as_mut().and_then(|ui| ui.refused_close(&live)) else {
            return;
        };
        tracing::info!(window = ?id, "a window closed from the overview is still open; focusing it");
        self.begin_close_overview(Some(id));
        self.queue_redraw();
    }

    #[must_use]
    pub fn overview_apps(&self) -> HashMap<glide_layout::WindowId, String> {
        let Some(ui) = self.overview.as_ref() else {
            return HashMap::new();
        };
        ui.windows()
            .filter_map(|id| Some((id, self.dock_key(id)?)))
            .filter(|(_, app)| crate::dock::is_pinnable(app))
            .collect()
    }

    pub fn refresh_overview(&mut self) {
        if self.overview.is_none() {
            return;
        }
        let entries = self.overview_entries();
        if entries.is_empty() {
            self.begin_close_overview(None);
        } else {
            self.relayout_overview(&entries);
        }
    }

    #[must_use]
    pub fn modal_child_of(&self, id: glide_layout::WindowId) -> Option<glide_layout::WindowId> {
        self.session.modal_windows.iter().copied().find(|child| {
            *child != id
                && self
                    .window_map
                    .get(child)
                    .and_then(|window| window.toplevel()?.parent())
                    .and_then(|parent| self.window_id_for_surface(&parent))
                    == Some(id)
        })
    }

    pub fn focus_window(&mut self, id: glide_layout::WindowId) {
        let mut id = id;
        for _ in 0..8 {
            match self.modal_child_of(id) {
                Some(child) => id = child,
                None => break,
            }
        }
        self.layer_focus = None;
        let closed = self.close_overview();
        if self.layout.focus_window(id) || closed {
            self.reconcile_space();
        }
    }

    fn announce_workspace(&mut self) {
        let active = self.layout.active_workspace();
        if self.announced_workspace == Some(active) {
            return;
        }
        self.announced_workspace = Some(active);
        self.ipc_subscribers.broadcast(&glide_ipc::Event::WorkspaceChanged { id: active.as_u64() });
    }

    fn sync_workspaces_if_changed(&mut self) {
        let snapshot = (self.layout.workspace_ids(), self.layout.active_workspace());
        if self.synced_workspaces.as_ref() == Some(&snapshot) {
            return;
        }
        self.sync_ext_workspaces();
        let live: Vec<u64> = snapshot.0.iter().map(|id| id.as_u64()).collect();
        self.layout_memory.retain(|key| match key {
            crate::layout_scope::ScopeKey::Desktop(id) => live.contains(id),
            _ => true,
        });
        self.synced_workspaces = Some(snapshot);
    }

    pub(crate) fn sync_ext_workspaces(&mut self) {
        self.protocols.ext_workspace_state.sync_workspaces(&self.display_handle, &self.layout, &self.space);
    }

    pub fn sync_layout_outputs(&mut self) {
        self.ipc_subscribers.broadcast(&glide_ipc::Event::OutputChanged);
        let outputs: Vec<Output> = self.space.outputs().cloned().collect();

        self.output_ids.retain(|output, _| outputs.contains(output));

        let infos: Vec<glide_layout::OutputInfo> = outputs
            .into_iter()
            .map(|output| {
                let geometry = self.space.output_geometry(&output).unwrap_or_default();

                let zone = layer_map_for_output(&output).non_exclusive_zone();

                let id = *self.output_ids.entry(output).or_insert_with(glide_layout::OutputId::next);

                let rect = glide_layout::Rectangle::from_loc_and_size(
                    geometry.loc.x,
                    geometry.loc.y,
                    geometry.size.w,
                    geometry.size.h,
                );
                let reserved = crate::topbar::HEIGHT;
                let work_area = glide_layout::Rectangle::from_loc_and_size(
                    geometry.loc.x + zone.loc.x,
                    geometry.loc.y + zone.loc.y + reserved,
                    zone.size.w,
                    (zone.size.h - reserved).max(1),
                );

                glide_layout::OutputInfo { id, geometry: rect, work_area }
            })
            .collect();

        tracing::info!(outputs = infos.len(), "layout outputs synchronized");
        self.layout.outputs_changed(&infos);
        let heads = self.output_heads();
        self.protocols.wlr_output_manager_state.sync_outputs(&self.display_handle, heads);
        self.sync_ext_workspaces();
    }

    pub fn set_window_decorated(&mut self, id: glide_layout::WindowId, decorated: bool) {
        let insets = if decorated {
            glide_layout::Insets {
                top: i32::try_from(self.decoration_theme.height).unwrap_or(i32::MAX),
                ..glide_layout::Insets::NONE
            }
        } else {
            glide_layout::Insets::NONE
        };

        if self.layout.set_window_insets(id, insets) {
            self.reconcile_space();
        }
    }

    pub fn update_snap_preview(&mut self, pointer: Point<f64, Logical>) {
        if !self.edge_snapping {
            if let Some(preview) = self.snap_preview.as_mut().filter(|p| p.is_armed()) {
                preview.dismiss();
                self.queue_redraw();
            }
            return;
        }
        let work_area = self.work_area_at(pointer);
        let at = pointer.to_i32_floor::<i32>();
        let output = logical_rect(self.layout.output_geometry_at(glide_layout::Point::new(at.x, at.y)));
        let outputs: Vec<Rectangle<i32, Logical>> =
            self.layout.outputs().iter().map(|o| logical_rect(o.geometry)).collect();
        let candidate = crate::snap::zone_at_on(pointer, work_area, output, &outputs);
        let zone = match self.snap_dwell.update(candidate, pointer, std::time::Instant::now()) {
            crate::snap::Dwell::Armed(zone) => Some(zone),
            crate::snap::Dwell::Started => {
                self.schedule_snap_dwell_check();
                None
            }
            crate::snap::Dwell::Pending | crate::snap::Dwell::Idle => None,
        };
        let armed = self.snap_preview.as_ref().filter(|preview| preview.is_armed()).map(|preview| preview.zone);
        if zone == armed {
            return;
        }
        match (zone, self.snap_preview.as_mut()) {
            (None, Some(preview)) => preview.dismiss(),
            (None, None) => return,
            (Some(zone), Some(preview)) => {
                preview.retarget(zone, crate::snap::zone_rect(zone, work_area));
            }
            (Some(zone), None) => {
                self.snap_preview = Some(crate::snap::SnapPreview::new(zone, crate::snap::zone_rect(zone, work_area)));
            }
        }
        self.queue_redraw();
    }

    fn schedule_snap_dwell_check(&self) {
        use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
        let timer = Timer::from_duration(crate::snap::DWELL);
        let inserted = self.loop_handle.insert_source(timer, |_, (), state| {
            if let Some(pointer) = state.snap_dwell.pointer() {
                state.update_snap_preview(pointer);
            }
            TimeoutAction::Drop
        });
        if let Err(err) = inserted {
            tracing::warn!(?err, "failed to schedule the snap dwell check");
        }
    }

    pub fn settle_snap_preview(&mut self) -> bool {
        let Some(preview) = self.snap_preview.as_ref() else {
            return false;
        };
        if !preview.is_visible() {
            self.snap_preview = None;
            return false;
        }
        preview.is_animating()
    }

    pub fn settle_moved_window(&mut self, window: &GlideWindow, pointer: Point<f64, Logical>) {
        if self.layout.overview_active() {
            return;
        }
        let Some(id) = self.window_id(window) else {
            return;
        };
        let at = pointer.to_i32_floor::<i32>();
        let Some(settled) = self.layout.settled_after_move(id, glide_layout::Point::new(at.x, at.y)) else {
            return;
        };
        self.morph_geometry(id, |state| {
            state.layout.set_window_geometry(id, settled);
        });
    }

    pub fn apply_snap(&mut self, window: &GlideWindow) -> bool {
        let Some(preview) = self.snap_preview.as_mut() else {
            return false;
        };
        if !preview.is_armed() {
            return false;
        }
        preview.dismiss();
        let zone = preview.zone;
        let rect = preview.rect;
        self.queue_redraw();
        let Some(id) = self.window_id(window) else {
            return false;
        };
        let target = glide_layout::Rectangle::from_loc_and_size(rect.loc.x, rect.loc.y, rect.size.w, rect.size.h);
        self.morph_geometry(id, |state| {
            if zone == crate::snap::SnapZone::Maximize {
                state.layout.set_maximized(id, true);
            } else {
                state.layout.set_window_geometry(id, target);
            }
        });
        true
    }

    #[must_use]
    pub fn work_area_at(&self, pointer: Point<f64, Logical>) -> Rectangle<i32, Logical> {
        let at = pointer.to_i32_floor::<i32>();
        logical_rect(self.layout.work_area_at(glide_layout::Point::new(at.x, at.y)))
    }

    pub fn record_window_move(&mut self, window: &GlideWindow, content_location: Point<i32, Logical>) {
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
        geometry.loc = glide_layout::Point::new(content_location.x - insets.left, content_location.y - insets.top);
        self.layout.set_window_geometry(id, geometry);
    }

    pub fn record_window_resize(&mut self, window: &GlideWindow, moved_to: Option<Point<i32, Logical>>) {
        if self.layout.overview_active() || self.workspace_swipe.is_some() {
            return;
        }
        let Some(id) = self.window_id(window) else {
            return;
        };
        let Some(state) = self.layout.window_state(id) else {
            return;
        };
        if state.maximized || state.fullscreen || !state.is_placed {
            return;
        }

        let insets = state.insets;
        let content_size = layout_size(window);
        let frame = match moved_to {
            Some(at) => glide_layout::Rectangle::from_loc_and_size(at.x, at.y, content_size.width, content_size.height)
                .outset(insets),
            None => glide_layout::Rectangle::new(state.geometry.loc, content_size.grow(insets)),
        };
        self.layout.set_window_geometry(id, frame);
    }

    pub fn clear_focus(&mut self) {
        self.layer_focus = None;
        self.layout.clear_focus();
        self.reconcile_space();
    }

    #[must_use]
    pub fn surface_under(
        &self, pos: Point<f64, Logical>,
    ) -> Option<(crate::focus::PointerFocusTarget, Point<f64, Logical>)> {
        let (output, output_loc) = self.space.outputs().find_map(|o| {
            let geometry = self.space.output_geometry(o)?;
            geometry.to_f64().contains(pos).then(|| (o.clone(), geometry.loc.to_f64()))
        })?;

        if let Some(lock) = self.session_lock.as_ref() {
            let surface = lock.surfaces.get(&output)?;
            return Some((crate::focus::PointerFocusTarget::from(surface.wl_surface().clone()), output_loc));
        }

        let upper = Self::layer_surface_under(&output, output_loc, pos, true);
        if upper.is_some() {
            return upper;
        }

        if let Some(hit) = self.window_target_under(pos) {
            return Some(hit);
        }

        Self::layer_surface_under(&output, output_loc, pos, false)
    }

    fn window_target_under(
        &self, pos: Point<f64, Logical>,
    ) -> Option<(crate::focus::PointerFocusTarget, Point<f64, Logical>)> {
        let (window, location) = self.space.element_under(pos)?;
        let content_origin = location.to_f64();

        let decoration_hit = {
            if let Some(deco) = window.try_decoration_state() {
                let header_height = f64::from(deco.header.height());
                let decoration_origin = content_origin - Point::from((0.0, header_height));
                deco.header.contains(pos - decoration_origin).then_some(decoration_origin)
            } else {
                None
            }
        };
        if let Some(decoration_origin) = decoration_hit {
            return Some((crate::focus::PointerFocusTarget::from(window.clone()), decoration_origin));
        }

        window
            .surface_under(pos - content_origin, WindowSurfaceType::ALL)
            .map(|(s, p)| (crate::focus::PointerFocusTarget::from(s), p.to_f64() + content_origin))
    }

    fn layer_surface_under(
        output: &Output, output_loc: Point<f64, Logical>, pos: Point<f64, Logical>, upper: bool,
    ) -> Option<(crate::focus::PointerFocusTarget, Point<f64, Logical>)> {
        let layers = layer_map_for_output(output);
        let local = pos - output_loc;

        let (first, second) =
            if upper { (WlrLayer::Overlay, WlrLayer::Top) } else { (WlrLayer::Bottom, WlrLayer::Background) };

        let layer = layers.layer_under(first, local).or_else(|| layers.layer_under(second, local))?;
        let layer_loc = layers.layer_geometry(layer)?.loc.to_f64();

        layer.surface_under(local - layer_loc, WindowSurfaceType::ALL).map(|(surface, loc)| {
            (crate::focus::PointerFocusTarget::from(surface), loc.to_f64() + layer_loc + output_loc)
        })
    }

    pub fn init_greeter(
        &mut self, event_loop: &mut smithay::reexports::calloop::EventLoop<'static, Self>,
    ) -> anyhow::Result<()> {
        use crate::greeter::{GreetdCommand, GreetdReply, GreeterStatus};
        use smithay::reexports::calloop::channel;

        let users = crate::greeter::list_users();
        let sessions = crate::greeter::list_sessions();
        let mut ui = crate::greeter::GreeterUi::new(users, sessions, self.shell_palette.1);
        ui.restore(&crate::greeter::LastLogin::load());

        let (sender, receiver) = channel::channel::<GreetdReply>();
        let handle = crate::greeter::ipc::start(move |reply| {
            let _ = sender.send(reply);
        });
        if let Some(ref h) = handle
            && let Some(user) = ui.active_user()
        {
            h.send(GreetdCommand::CreateSession(user.username.clone()));
        }

        self.greeter = Some(ui);
        self.greeter_handle = handle;
        self.sync_greeter_layout();

        let inserted = event_loop.handle().insert_source(receiver, |event, (), state| {
            if let channel::Event::Msg(reply) = event {
                state.handle_greetd_reply(reply);
            }
        });
        if let Err(err) = inserted {
            tracing::error!("failed to watch greetd channel: {err}");
        }

        let (status_tx, status_rx) = channel::channel::<GreeterStatus>();
        let spawned = std::thread::Builder::new().name("glide-greeter-status".into()).spawn(move || {
            while status_tx.send(crate::greeter::system::read_status()).is_ok() {
                std::thread::sleep(std::time::Duration::from_secs(10));
            }
        });
        if let Err(err) = spawned {
            tracing::warn!(?err, "failed to spawn the greeter status poller");
        }
        let inserted = event_loop.handle().insert_source(status_rx, |event, (), state| {
            if let channel::Event::Msg(status) = event
                && let Some(greeter) = state.greeter.as_mut()
            {
                greeter.set_status(status);
                state.queue_redraw();
            }
        });
        if let Err(err) = inserted {
            tracing::error!("failed to watch the greeter status channel: {err}");
        }

        Ok(())
    }

    pub fn sync_greeter_layout(&mut self) {
        let label = self
            .keyboard_layouts()
            .and_then(|(names, active)| (names.len() > 1).then(|| names.get(active).cloned().unwrap_or_default()));
        let caps_lock = self.seat.get_keyboard().is_some_and(|keyboard| keyboard.modifier_state().caps_lock);
        if let Some(greeter) = self.greeter.as_mut() {
            greeter.set_keyboard_layout(label.unwrap_or_default());
            greeter.set_caps_lock(caps_lock);
        }
    }

    fn send_greetd(&self, command: crate::greeter::GreetdCommand) {
        if let Some(handle) = self.greeter_handle.as_ref() {
            handle.send(command);
        }
    }

    pub fn handle_greetd_reply(&mut self, reply: crate::greeter::GreetdReply) {
        use crate::greeter::{AuthMessageType, GreetdCommand, GreetdReply};
        let Some(greeter) = self.greeter.as_mut() else {
            return;
        };
        match reply {
            GreetdReply::Connected => tracing::info!("connected to greetd socket"),
            GreetdReply::Prompt { message_type, message } => match message_type {
                AuthMessageType::Secret | AuthMessageType::Visible => {
                    let secret = message_type == AuthMessageType::Secret;
                    if let Some(answer) = greeter.on_question(secret, message) {
                        self.send_greetd(GreetdCommand::Respond(Some(answer)));
                    }
                }
                AuthMessageType::Info | AuthMessageType::Error => {
                    greeter.on_notice(message, message_type == AuthMessageType::Error);
                    self.send_greetd(GreetdCommand::Respond(None));
                }
            },
            GreetdReply::Authenticated => {
                tracing::info!("greetd authenticated successfully");
                greeter.set_auth_success();
                if let Some(session) = greeter.active_session() {
                    tracing::info!(session = ?session.name, cmd = ?session.exec, "starting session via greetd");
                    let command = GreetdCommand::StartSession { cmd: session.exec.clone(), env: session.env() };
                    self.send_greetd(command);
                }
            }
            GreetdReply::SessionStarted => {
                tracing::info!("session started; greeter fades out and exits");
                if let Some(last) = greeter.last_login() {
                    last.save();
                }
                greeter.leave();
                self.handoff = Some(crate::handoff::Handoff::arriving());
            }
            GreetdReply::Error { auth, message } => {
                tracing::warn!(auth, %message, "greetd error");
                if auth {
                    greeter.set_auth_denied(None, crate::i18n::t(crate::i18n::Key::GreeterWrongPassword));
                    let user = greeter.active_user().map(|u| u.username.clone());
                    if let Some(user) = user {
                        self.send_greetd(GreetdCommand::CreateSession(user));
                    }
                } else {
                    greeter.set_auth_error(message, true);
                }
            }
            GreetdReply::Disconnected => {
                tracing::warn!("disconnected from greetd socket");
                greeter.set_unavailable();
            }
        }
        self.queue_redraw();
    }

    pub fn lock_screen(&mut self) {
        if self.screen_locked() {
            return;
        }
        tracing::info!("locking the screen");
        if glide_config::passwords::Prefs::load().lock_with_screen {
            self.lock_keychain();
        }
        self.abort_keychain_unlock();
        self.cancel_all_polkit();
        let user = crate::greeter::current_user(&self.topbar.system_status().user_name);
        if self.lock_auth.is_none() {
            self.lock_auth = self.start_lock_auth(&user.username);
        }

        if let Some(launcher) = self.launcher.as_mut() {
            launcher.begin_close();
        }
        self.begin_close_overview(None);
        self.cancel_power_dialog();
        self.power_dialog = None;
        self.switcher = None;
        self.topbar.close_popup();
        self.dock.close_menu();
        self.snapshots.clear();

        let mut ui = crate::greeter::GreeterUi::lock(user, self.shell_palette.1);
        ui.set_status(self.lock_status());
        self.greeter = Some(ui);
        self.sync_greeter_layout();
        self.break_grabs_for_lock();
        self.apply_keyboard_focus();
        self.release_held_keys("screen locked");
        if let Some(pointer) = self.seat.get_pointer() {
            let location = pointer.current_location();
            pointer.motion(
                self,
                None,
                &smithay::input::pointer::MotionEvent {
                    location,
                    serial: smithay::utils::SERIAL_COUNTER.next_serial(),
                    time: smithay::backend::input::InputTime::now(),
                },
            );
            pointer.frame(self);
        }
        self.queue_redraw();
    }

    pub fn unlock_screen(&mut self) {
        if let Some(greeter) = self.greeter.as_mut().filter(|greeter| greeter.is_lock()) {
            greeter.set_auth_success();
            greeter.leave();
            self.queue_redraw();
        }
    }

    #[must_use]
    pub fn shell_snapshot(&self) -> crate::shell_bus::Snapshot {
        crate::shell_bus::Snapshot {
            appearance: self.shell_palette.0,
            accent: self.shell_palette.1,
            language: crate::i18n::language().key(),
            animation_scale: f64::from(glide_theme::motion::scale()),
            reduced_motion: self.motion.respect_system && self.topbar.system_status().reduced_motion,
            font_family: self.font_family.clone().unwrap_or_default(),
            icon_theme: self.dock.icon_theme().unwrap_or_default(),
            locked: self.screen_locked(),
            active_workspace: self.active_workspace_index().and_then(|index| u32::try_from(index).ok()).unwrap_or(0),
        }
    }

    pub fn sync_shell_bus(&mut self) {
        if self.session.shell_bus.is_none() {
            return;
        }
        let snapshot = self.shell_snapshot();
        if let Some(handle) = self.session.shell_bus.as_mut() {
            handle.publish(snapshot);
        }
    }

    pub fn handle_shell_bus_event(&mut self, event: crate::shell_bus::Event) {
        match event {
            crate::shell_bus::Event::Token { pid, reply } => reply.send(self.mint_activation_token_for(pid, None)),
            crate::shell_bus::Event::Frame { pid } => {
                if self.session.framed_pids.register(pid, crate::framed::process_alive) {
                    tracing::info!(pid, "windows of this client get the shell's frame");
                    self.reconcile_space();
                    self.queue_redraw();
                }
            }
            crate::shell_bus::Event::Lock => self.lock_screen(),
            crate::shell_bus::Event::CopySecret { pid, secret, clear_after, reply } => {
                let allowed = self.holds_focus(pid);
                if allowed {
                    let private = glide_config::passwords::Prefs::load().private_clipboard;
                    self.place_secret(&secret, private, clear_after);
                }
                reply.send(allowed);
            }
            crate::shell_bus::Event::Osd(reading) => {
                self.session.osd.show(reading, std::time::Instant::now());
                self.queue_redraw();
            }
            crate::shell_bus::Event::QuickSettings(detail) => {
                if self.screen_locked() {
                    return;
                }
                if self.topbar.open_popup() != Some(crate::topbar::OpenPopup::QuickSettings) {
                    self.run_topbar_action(crate::topbar::TopbarAction::ToggleQuickSettings);
                }
                if let Some(detail) = detail {
                    self.run_qs_panel_action(crate::topbar::PanelAction::Open(detail));
                }
                self.queue_redraw();
            }
        }
    }

    pub fn sync_session_hints(&mut self) {
        let (locked, idle) = (self.screen_locked(), self.idle.is_idle());
        self.notifications.set_locked(locked);
        let sharing = self.topbar.sharing_screen();
        self.notifications.set_inhibited(sharing && self.notifications.policy().system.quiet_while_sharing);
        if let Some(logind) = self.session.logind.as_mut() {
            logind.set_locked(locked);
            logind.set_idle(idle);
        }
        let active = locked || self.outputs_blanked;
        if let Some(screensaver) = self.session.screensaver.as_mut() {
            screensaver.set_active(active);
        }
    }

    pub fn tick_idle(&mut self) {
        use crate::{idle::Step, system_info::BatteryState};
        let inhibited = self.is_idle_inhibited() || self.topbar.caffeinated() || self.session.cast.keeps_awake();
        let on_battery =
            self.topbar.system_status().battery.is_some_and(|battery| battery.state == BatteryState::Discharging);
        for step in self.idle.poll(std::time::Instant::now(), inhibited, on_battery).into_iter().flatten() {
            match step {
                Step::Blank => {
                    tracing::info!("idle timeout reached; blanking outputs");
                    self.set_outputs_blanked(true);
                }
                Step::Lock if crate::greeter::pam::available() => {
                    tracing::info!("idle lock delay reached");
                    self.lock_screen();
                }
                Step::Lock => {
                    tracing::warn!("not locking on idle: PAM is unavailable, so the screen could not be unlocked");
                }
                Step::Suspend => {
                    tracing::info!(on_battery, "idle suspend timeout reached");
                    crate::system_info::suspend();
                }
            }
        }
    }

    pub fn locked_power_key(&mut self) {
        use crate::greeter::PowerAction;
        let Some(greeter) = self.greeter.as_mut() else {
            tracing::info!("power key under an external screen locker; suspending");
            crate::system_info::suspend();
            return;
        };
        if greeter.armed() == Some(PowerAction::ShutDown) {
            self.greeter_power(PowerAction::ShutDown);
            return;
        }
        greeter.arm_power(PowerAction::ShutDown);
        self.queue_redraw();
    }

    pub fn confirm_greeter_power(&mut self) -> bool {
        let Some(action) = self.greeter.as_ref().and_then(crate::greeter::GreeterUi::confirmable) else {
            return false;
        };
        self.greeter_power(action);
        true
    }

    fn start_lock_auth(&self, user: &str) -> Option<crate::greeter::pam::Authenticator> {
        use crate::greeter::pam::{Authenticator, Verdict};
        use smithay::reexports::calloop::channel;

        let (sender, receiver) = channel::channel::<Verdict>();
        let inserted = self.loop_handle.insert_source(receiver, |event, (), state| {
            if let channel::Event::Msg(verdict) = event {
                state.handle_lock_verdict(verdict);
            }
        });
        if let Err(err) = inserted {
            tracing::error!("failed to watch the lock screen authenticator: {err}");
            return None;
        }
        Authenticator::start(user, move |verdict| {
            let _ = sender.send(verdict);
        })
        .inspect_err(|err| tracing::error!(?err, "failed to start the lock screen authenticator"))
        .ok()
    }

    fn handle_lock_verdict(&mut self, verdict: crate::greeter::pam::Verdict) {
        use crate::{
            greeter::pam::Verdict,
            i18n::{Key, t},
        };
        let Some(greeter) = self.greeter.as_mut().filter(|greeter| greeter.is_lock()) else {
            return;
        };
        match verdict {
            Verdict::Granted => {
                tracing::info!("screen unlocked");
                greeter.set_auth_success();
                greeter.leave();
            }
            Verdict::Denied(message) => {
                tracing::info!("lock screen authentication failed");
                greeter.set_auth_denied(message, t(Key::GreeterWrongPassword));
            }
            Verdict::Unavailable => {
                tracing::error!("lock screen authentication is unavailable");
                greeter.set_auth_error(t(Key::LockUnavailable).to_string(), false);
            }
        }
        self.queue_redraw();
    }

    fn submit_lock_password(&mut self) {
        use crate::i18n::{Key, t};
        let Some(greeter) = self.greeter.as_mut() else {
            return;
        };
        if greeter.submit() == crate::greeter::Submit::Nothing {
            return;
        }
        let sent = greeter
            .take_queued()
            .is_some_and(|password| self.lock_auth.as_ref().is_some_and(|auth| auth.check(password)));
        if !sent {
            greeter.set_auth_error(t(Key::LockUnavailable).to_string(), false);
        }
        self.queue_redraw();
    }

    fn lock_status(&self) -> crate::greeter::GreeterStatus {
        let status = self.topbar.system_status();
        crate::greeter::GreeterStatus {
            battery: status.battery_pct.map(|pct| (pct, status.battery_charging)),
            wifi: (status.wifi_enabled && !status.wifi_ssid.is_empty()).then_some(status.wifi_level),
        }
    }

    fn lock_media(&self) -> Option<crate::greeter::LockMedia> {
        let media = self.topbar.media()?;
        let subtitle = [media.artist.trim(), media.source.trim()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        Some(crate::greeter::LockMedia {
            title: media.title.clone(),
            subtitle,
            playing: media.playing,
            accent: media.accent,
        })
    }

    fn lock_notices(&self) -> Option<crate::greeter::LockNotices> {
        let count = self.notifications.unread();
        if count == 0 && self.greeter.as_ref().is_some_and(|greeter| greeter.notice_count() == 0) {
            return None;
        }
        let mut apps: Vec<String> = Vec::new();
        for item in self.notifications.nc_items() {
            let app = item.app_name.trim().to_string();
            if !app.is_empty() && !apps.contains(&app) {
                apps.push(app);
            }
            if apps.len() == 3 {
                break;
            }
        }
        Some(crate::greeter::LockNotices { count, apps })
    }

    fn finish_unlock(&mut self) {
        self.greeter = None;
        self.apply_keyboard_focus();
        self.reconcile_space();
        self.queue_redraw();
    }

    pub fn submit_greeter_password(&mut self) {
        use crate::greeter::{GreetdCommand, Submit};
        if self.greeter.as_ref().is_some_and(crate::greeter::GreeterUi::is_lock) {
            self.submit_lock_password();
            return;
        }
        let standalone = self.greeter_handle.is_none();
        let Some(greeter) = self.greeter.as_mut() else {
            return;
        };
        if greeter.is_unavailable() {
            greeter.set_unavailable();
            self.queue_redraw();
            return;
        }
        match greeter.submit() {
            Submit::Nothing => return,
            _ if standalone => {
                tracing::warn!("standalone greeter mode: simulating authentication success");
                greeter.take_queued();
                greeter.set_auth_success();
                greeter.leave();
                self.handoff = Some(crate::handoff::Handoff::arriving());
            }
            Submit::Send(password) => self.send_greetd(GreetdCommand::Respond(Some(password))),
            Submit::CreateSession(user) => self.send_greetd(GreetdCommand::CreateSession(user)),
            Submit::Queued => {}
        }
        self.queue_redraw();
    }

    pub fn greeter_cycle_user(&mut self) {
        let user = self.greeter.as_mut().and_then(crate::greeter::GreeterUi::cycle_user);
        if let Some(user) = user {
            self.send_greetd(crate::greeter::GreetdCommand::CreateSession(user));
            self.queue_redraw();
        }
    }

    pub fn greeter_select_user(&mut self, index: usize) {
        let user = self.greeter.as_mut().and_then(|greeter| greeter.select_user(index));
        if let Some(user) = user {
            self.send_greetd(crate::greeter::GreetdCommand::CreateSession(user));
            self.queue_redraw();
        }
    }

    pub fn greeter_power(&mut self, action: crate::greeter::PowerAction) {
        use crate::greeter::PowerAction;
        if let Some(greeter) = self.greeter.as_mut() {
            greeter.close_menus();
            greeter.power_started(action);
        }
        match action {
            PowerAction::Sleep => crate::system_info::suspend(),
            PowerAction::Restart => self.request_session_end(crate::power::PowerAction::Restart),
            PowerAction::ShutDown => self.request_session_end(crate::power::PowerAction::ShutDown),
        }
        self.queue_redraw();
    }

    pub fn tick_greeter(&mut self) -> bool {
        let locked = self.greeter.as_ref().is_some_and(crate::greeter::GreeterUi::is_lock);
        let lock_status = locked.then(|| self.lock_status());
        let media = if locked { self.lock_media() } else { None };
        let notices = locked.then(|| self.lock_notices()).flatten();
        let Some(greeter) = self.greeter.as_mut() else {
            return false;
        };
        if let Some(status) = lock_status {
            greeter.set_status(status);
            greeter.set_media(media);
            if let Some(notices) = notices {
                greeter.set_notices(notices);
            }
            if self.session.end.is_pending() {
                greeter.keep_toast();
            }
            let animating = greeter.tick();
            if greeter.finished_leaving() {
                self.finish_unlock();
                return true;
            }
            return animating;
        }
        let animating = greeter.tick();
        let covered = self.handoff.as_ref().is_none_or(crate::handoff::Handoff::fully_covered);
        if greeter.finished_leaving() && covered {
            self.loop_signal.stop();
        }
        animating
    }

    pub fn tick_login(&mut self) -> bool {
        let greeter = self.tick_greeter();
        let handoff = self.tick_handoff();
        greeter || handoff
    }

    pub fn tick_handoff(&mut self) -> bool {
        let Some(handoff) = self.handoff.as_mut() else {
            return false;
        };
        if handoff.tick() {
            return true;
        }
        self.handoff = None;
        self.queue_redraw();
        false
    }

    #[must_use]
    pub fn handoff_blocks_input(&self) -> bool {
        self.greeter.is_none() && self.handoff.as_ref().is_some_and(crate::handoff::Handoff::blocks_input)
    }
}

#[must_use]
pub fn logical_rect(rect: glide_layout::Rectangle) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((rect.loc.x, rect.loc.y)),
        smithay::utils::Size::from((rect.size.width, rect.size.height)),
    )
}

#[must_use]
pub fn content_bounds(
    work_area: glide_layout::Rectangle, insets: glide_layout::Insets,
) -> smithay::utils::Size<i32, Logical> {
    smithay::utils::Size::from((
        (work_area.size.width - insets.width()).max(1),
        (work_area.size.height - insets.height()).max(1),
    ))
}

#[must_use]
pub fn tiled_edges(frame: glide_layout::Rectangle, area: glide_layout::Rectangle) -> [bool; 4] {
    let left = frame.loc.x == area.loc.x;
    let right = frame.loc.x + frame.size.width == area.loc.x + area.size.width;
    let top = frame.loc.y == area.loc.y;
    let bottom = frame.loc.y + frame.size.height == area.loc.y + area.size.height;
    if top && bottom && left != right { [left, right, true, true] } else { [false; 4] }
}

#[must_use]
pub fn locks_only(mods: smithay::input::keyboard::ModifiersState) -> smithay::input::keyboard::ModifiersState {
    smithay::input::keyboard::ModifiersState {
        caps_lock: mods.caps_lock,
        num_lock: mods.num_lock,
        ..Default::default()
    }
}

#[must_use]
pub const fn holds_modifiers(mods: &smithay::input::keyboard::ModifiersState) -> bool {
    mods.ctrl || mods.alt || mods.shift || mods.logo || mods.iso_level3_shift || mods.iso_level5_shift
}

fn layout_of<D: smithay::input::SeatHandler + 'static>(
    keyboard: &smithay::input::keyboard::KeyboardHandle<D>, data: &mut D,
) -> u32 {
    keyboard.with_xkb_state(data, |context| {
        context.xkb().lock().expect("single-threaded event loop, this lock is never poisoned").active_layout().0
    })
}

pub fn replace_modifiers<D: smithay::input::SeatHandler + 'static>(
    keyboard: &smithay::input::keyboard::KeyboardHandle<D>, data: &mut D,
    mods: smithay::input::keyboard::ModifiersState, keep_layout: bool,
) {
    let layout = keep_layout.then(|| layout_of(keyboard, data));
    if keyboard.set_modifier_state(mods) != 0 {
        keyboard.advertise_modifier_state(data);
    }
    if let Some(layout) = layout
        && layout_of(keyboard, data) != layout
    {
        keyboard.with_xkb_state(data, |mut context| context.set_layout(smithay::input::keyboard::Layout(layout)));
    }
}

pub fn release_keyboard<D: smithay::input::SeatHandler + 'static>(
    keyboard: &smithay::input::keyboard::KeyboardHandle<D>, data: &mut D,
) -> Option<(usize, bool)> {
    let held = keyboard.pressed_keys().len();
    let stuck = holds_modifiers(&keyboard.modifier_state());
    if held == 0 && !stuck {
        return None;
    }
    keyboard.release_source(data, smithay::input::keyboard::KeyboardSource::MAIN);
    replace_modifiers(keyboard, data, locks_only(keyboard.modifier_state()), true);
    Some((held, stuck))
}

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

pub(crate) fn layout_size(window: &GlideWindow) -> glide_layout::Size {
    let size = window.geometry().size;
    glide_layout::Size::new(size.w, size.h)
}

pub(crate) fn toplevel_title_and_app_id(surface: &WlSurface) -> (Option<String>, Option<String>) {
    with_states(surface, |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|data| data.lock().ok())
            .map(|guard| (guard.title.clone(), guard.app_id.clone()))
            .unwrap_or_default()
    })
}

pub(crate) fn window_surface(window: &GlideWindow) -> &WlSurface {
    window.toplevel().expect("only Window::new_wayland_window is used, so toplevel() is always Some").wl_surface()
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
    pub security_context: Option<smithay::wayland::security_context::SecurityContext>,
    pub identity: crate::identity::Identity,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

pub fn client_is_unrestricted(client: &smithay::reexports::wayland_server::Client) -> bool {
    client.get_data::<ClientState>().is_none_or(|state| state.security_context.is_none())
}

impl FractionalScaleHandler for GlideState {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self.space.outputs().next().map_or(1.0, |output| output.current_scale().fractional_scale());
        with_states(&surface, |states| {
            with_fractional_scale(states, |fractional| {
                fractional.set_preferred_scale(scale);
            });
        });
    }
}

pub const BATTERY_NOTICE_ID: u32 = u32::MAX - 3;
const BATTERY_NOTICE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const BOGUS_PHYSICAL_SIZES: [(i32, i32); 6] = [(1600, 900), (1600, 1000), (160, 90), (160, 100), (16, 9), (16, 10)];
const LAPTOP_DIAGONAL_INCHES: f64 = 20.0;
const LAPTOP_TARGET_DPI: f64 = 135.0;
const DESKTOP_TARGET_DPI: f64 = 110.0;
const MIN_LOGICAL_WIDTH: f64 = 1024.0;
const MIN_LOGICAL_HEIGHT: f64 = 600.0;

#[must_use]
pub fn auto_output_scale(physical_mm: (i32, i32), mode_px: (i32, i32)) -> f64 {
    let (mm_w, mm_h) = physical_mm;
    let (px_w, px_h) = mode_px;
    if mm_w <= 0 || mm_h <= 0 || px_w <= 0 || px_h <= 0 {
        return 1.0;
    }
    if BOGUS_PHYSICAL_SIZES.contains(&(mm_w, mm_h)) || BOGUS_PHYSICAL_SIZES.contains(&(mm_h, mm_w)) {
        return 1.0;
    }
    let diagonal_in = f64::from(mm_w).hypot(f64::from(mm_h)) / 25.4;
    let diagonal_px = f64::from(px_w).hypot(f64::from(px_h));
    let target = if diagonal_in < LAPTOP_DIAGONAL_INCHES { LAPTOP_TARGET_DPI } else { DESKTOP_TARGET_DPI };
    let ideal = diagonal_px / diagonal_in / target;
    let candidate = if ideal >= 2.75 {
        3.0
    } else if ideal >= 1.75 {
        2.0
    } else if ideal >= 1.35 {
        1.5
    } else {
        1.0
    };
    [3.0, 2.0, 1.5]
        .into_iter()
        .filter(|scale| *scale <= candidate)
        .find(|scale| f64::from(px_w) / scale >= MIN_LOGICAL_WIDTH && f64::from(px_h) / scale >= MIN_LOGICAL_HEIGHT)
        .unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod keyboard_state {
        use super::super::{layout_of, release_keyboard, replace_modifiers};
        use smithay::backend::input::{InputTime, KeyState};
        use smithay::input::keyboard::{
            FilterResult, KeyboardHandle, Keycode, Keysym, Layout, ModifiersState, XkbConfig,
        };
        use smithay::input::{Seat, SeatHandler, SeatState};
        use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;

        const SHIFT: u32 = 50;
        const A: u32 = 38;

        struct Probe {
            seat_state: SeatState<Self>,
        }

        impl SeatHandler for Probe {
            type KeyboardFocus = WlSurface;
            type PointerFocus = WlSurface;
            type TouchFocus = WlSurface;

            fn seat_state(&mut self) -> &mut SeatState<Self> {
                &mut self.seat_state
            }
        }

        impl smithay::wayland::pointer_constraints::PointerConstraintsHandler for Probe {}

        impl smithay::wayland::compositor::CompositorHandler for Probe {
            fn compositor_state(&mut self) -> &mut smithay::wayland::compositor::CompositorState {
                unreachable!("keyboard probes have no surfaces")
            }

            fn client_compositor_state<'a>(
                &self, _client: &'a smithay::reexports::wayland_server::Client,
            ) -> &'a smithay::wayland::compositor::CompositorClientState {
                unreachable!("keyboard probes have no clients")
            }

            fn commit(&mut self, _surface: &WlSurface) {}
        }

        fn keyboard() -> (Probe, Seat<Probe>, KeyboardHandle<Probe>) {
            let mut probe = Probe { seat_state: SeatState::new() };
            let mut seat = probe.seat_state.new_seat("probe");
            let config = XkbConfig { layout: "us,ua", ..XkbConfig::default() };
            let keyboard = seat.add_keyboard(config, 200, 25).expect("the us,ua keymap compiles");
            (probe, seat, keyboard)
        }

        fn key(probe: &mut Probe, keyboard: &KeyboardHandle<Probe>, code: u32, state: KeyState) -> Option<Keysym> {
            let serial = smithay::utils::SERIAL_COUNTER.next_serial();
            keyboard.input(probe, Keycode::new(code), state, serial, InputTime::now(), |_, _, handle| {
                FilterResult::Intercept(handle.modified_sym())
            })
        }

        #[test]
        fn a_shift_whose_release_was_lost_shifts_every_key_until_the_keyboard_is_released() {
            let (mut probe, _seat, keyboard) = keyboard();
            key(&mut probe, &keyboard, SHIFT, KeyState::Pressed);
            assert_eq!(key(&mut probe, &keyboard, A, KeyState::Pressed), Some(Keysym::A));
            key(&mut probe, &keyboard, A, KeyState::Released);
            assert_eq!(release_keyboard(&keyboard, &mut probe), Some((1, true)));
            assert_eq!(key(&mut probe, &keyboard, A, KeyState::Pressed), Some(Keysym::a));
            key(&mut probe, &keyboard, A, KeyState::Released);
            assert_eq!(release_keyboard(&keyboard, &mut probe), None, "nothing is held any more");
        }

        #[test]
        fn a_key_whose_release_was_lost_swallows_its_next_press_until_the_keyboard_is_released() {
            let (mut probe, _seat, keyboard) = keyboard();
            key(&mut probe, &keyboard, A, KeyState::Pressed);
            assert_eq!(key(&mut probe, &keyboard, A, KeyState::Pressed), None, "smithay absorbs a press of a held key");
            assert_eq!(release_keyboard(&keyboard, &mut probe), Some((1, false)));
            assert_eq!(key(&mut probe, &keyboard, A, KeyState::Pressed), Some(Keysym::a));
        }

        #[test]
        fn replacing_modifiers_keeps_the_layout_that_smithay_resets() {
            let (mut probe, _seat, keyboard) = keyboard();
            keyboard.with_xkb_state(&mut probe, |mut context| context.set_layout(Layout(1)));
            let num_lock = ModifiersState { num_lock: true, ..keyboard.modifier_state() };
            replace_modifiers(&keyboard, &mut probe, num_lock, true);
            assert_eq!(layout_of(&keyboard, &mut probe), 1);
            assert!(keyboard.modifier_state().num_lock);
            let off = ModifiersState { num_lock: false, ..keyboard.modifier_state() };
            replace_modifiers(&keyboard, &mut probe, off, false);
            assert_eq!(layout_of(&keyboard, &mut probe), 0, "set_modifier_state drops the locked layout");
        }
    }

    #[test]
    fn a_half_of_the_work_area_is_tiled_on_its_outer_edges_only() {
        let rect = glide_layout::Rectangle::from_loc_and_size;
        let area = rect(0, 32, 1920, 1048);
        assert_eq!(tiled_edges(rect(0, 32, 960, 1048), area), [true, false, true, true]);
        assert_eq!(tiled_edges(rect(960, 32, 960, 1048), area), [false, true, true, true]);
        assert_eq!(tiled_edges(area, area), [false; 4], "the full area is maximized, not tiled");
        assert_eq!(tiled_edges(rect(0, 100, 960, 900), area), [false; 4], "a window that only touches an edge floats");
    }

    #[test]
    fn auto_scale_picks_hidpi_for_dense_laptop_panels() {
        assert!((auto_output_scale((286, 179), (2560, 1600)) - 1.5).abs() < f64::EPSILON);
        assert!((auto_output_scale((302, 189), (2880, 1800)) - 2.0).abs() < f64::EPSILON);
        assert!((auto_output_scale((344, 194), (3840, 2160)) - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn auto_scale_keeps_ordinary_desktop_monitors_at_one() {
        assert!((auto_output_scale((531, 299), (1920, 1080)) - 1.0).abs() < f64::EPSILON);
        assert!((auto_output_scale((597, 336), (2560, 1440)) - 1.0).abs() < f64::EPSILON);
        assert!((auto_output_scale((294, 165), (1920, 1080)) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn auto_scale_uses_fractional_for_27_inch_4k() {
        assert!((auto_output_scale((597, 336), (3840, 2160)) - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn auto_scale_ignores_unknown_and_bogus_physical_sizes() {
        assert!((auto_output_scale((0, 0), (3840, 2160)) - 1.0).abs() < f64::EPSILON);
        assert!((auto_output_scale((160, 90), (3840, 2160)) - 1.0).abs() < f64::EPSILON);
        assert!((auto_output_scale((16, 10), (2560, 1600)) - 1.0).abs() < f64::EPSILON);
        assert!((auto_output_scale((1600, 900), (1920, 1080)) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn auto_scale_never_shrinks_the_logical_width_below_1024() {
        assert!((auto_output_scale((120, 75), (1920, 1200)) - 1.5).abs() < f64::EPSILON);
        assert!((auto_output_scale((100, 60), (1280, 800)) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn configure_bounds_leave_room_for_the_titlebar() {
        let work_area = glide_layout::Rectangle::from_loc_and_size(0, 34, 1920, 1046);
        let insets = glide_layout::Insets { top: 48, ..glide_layout::Insets::NONE };
        let bounds = content_bounds(work_area, insets);
        assert_eq!((bounds.w, bounds.h), (1920, 998));
        assert_eq!(content_bounds(work_area, glide_layout::Insets::NONE).h, 1046);
    }
}

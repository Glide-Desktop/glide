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
        pointer_constraints::PointerConstraintsState,
        presentation::PresentationState,
        relative_pointer::RelativePointerManagerState,
        security_context::SecurityContextState,
        selection::{
            data_device::DataDeviceState,
            ext_data_control::DataControlState as ExtDataControlState,
            primary_selection::PrimarySelectionState,
            wlr_data_control::DataControlState as WlrDataControlState,
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
    pub foreign_toplevel_list_state:
        smithay::wayland::foreign_toplevel_list::ForeignToplevelListState,
    pub idle_notifier_state: smithay::wayland::idle_notify::IdleNotifierState<GlideState>,
    pub idle_inhibit_state: smithay::wayland::idle_inhibit::IdleInhibitManagerState,
    pub wlr_output_manager_state: crate::handlers::wlr_output_management::WlrOutputManagerState,
    pub ext_workspace_state: crate::handlers::ext_workspace::ExtWorkspaceManagerState,
    pub xdg_activation_state: XdgActivationState,
    pub security_context_state: SecurityContextState,
    pub relative_pointer_manager_state: RelativePointerManagerState,
    pub presentation_state: PresentationState,
    pub pointer_constraints_state: PointerConstraintsState,
}

impl Protocols {
    fn new(
        dh: &DisplayHandle,
        loop_handle: &LoopHandle<'static, GlideState>,
        seat_state: SeatState<GlideState>,
        clock_id: u32,
    ) -> Self {
        let compositor_state = CompositorState::new::<GlideState>(dh);
        let xdg_shell_state = XdgShellState::new::<GlideState>(dh);
        let xdg_dialog_state = XdgDialogState::new::<GlideState>(dh);
        let cursor_shape_state = CursorShapeManagerState::new::<GlideState>(dh);
        let xdg_decoration_state = XdgDecorationState::new::<GlideState>(dh);
        let kde_decoration_state = KdeDecorationState::new::<GlideState>(
            dh,
            smithay::reexports::wayland_protocols_misc::server_decoration::server
                ::org_kde_kwin_server_decoration_manager::Mode::Server,
        );
        let layer_shell_state = WlrLayerShellState::new::<GlideState>(dh);
        let fractional_scale_state = FractionalScaleManagerState::new::<GlideState>(dh);
        let viewporter_state = smithay::wayland::viewporter::ViewporterState::new::<GlideState>(dh);
        let shm_state = ShmState::new::<GlideState>(dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<GlideState>(dh);

        let data_device_state = DataDeviceState::new::<GlideState>(dh);
        let primary_selection_state = PrimarySelectionState::new::<GlideState>(dh);
        let wlr_data_control_state =
            WlrDataControlState::new::<GlideState, _>(dh, Some(&primary_selection_state), |_| true);
        let ext_data_control_state =
            ExtDataControlState::new::<GlideState, _>(dh, Some(&primary_selection_state), |_| true);

        let foreign_toplevel_list_state =
            smithay::wayland::foreign_toplevel_list::ForeignToplevelListState::new::<GlideState>(
                dh,
            );
        let idle_notifier_state =
            smithay::wayland::idle_notify::IdleNotifierState::new(dh, loop_handle.clone());
        let idle_inhibit_state =
            smithay::wayland::idle_inhibit::IdleInhibitManagerState::new::<GlideState>(dh);
        let wlr_output_manager_state =
            crate::handlers::wlr_output_management::WlrOutputManagerState::new::<GlideState>(dh);
        let ext_workspace_state =
            crate::handlers::ext_workspace::ExtWorkspaceManagerState::new::<GlideState>(dh);
        let xdg_activation_state = XdgActivationState::new::<GlideState>(dh);
        let security_context_state = SecurityContextState::new::<GlideState, _>(dh, |client| {
            client
                .get_data::<ClientState>()
                .is_none_or(|state| state.security_context.is_none())
        });
        let relative_pointer_manager_state = RelativePointerManagerState::new::<GlideState>(dh);
        let presentation_state = PresentationState::new::<GlideState>(dh, clock_id);
        let pointer_constraints_state = PointerConstraintsState::new::<GlideState>(dh);

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
            primary_selection_state,
            wlr_data_control_state,
            ext_data_control_state,
            foreign_toplevel_list_state,
            idle_notifier_state,
            idle_inhibit_state,
            wlr_output_manager_state,
            ext_workspace_state,
            xdg_activation_state,
            security_context_state,
            relative_pointer_manager_state,
            presentation_state,
            pointer_constraints_state,
        }
    }
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

    pub cursor_status: CursorImageStatus,
    pub cursor_position_hint: Option<(WlSurface, Point<f64, Logical>)>,

    pub layer_focus: Option<LayerSurface>,

    pub overview: Option<crate::overview::OverviewUi>,

    pub suppressed_keys: Vec<Keycode>,

    pub foreign_toplevel_handles: HashMap<
        glide_layout::WindowId,
        smithay::wayland::foreign_toplevel_list::ForeignToplevelHandle,
    >,
    pub idle_inhibited_surfaces: HashSet<WlSurface>,
    pub ipc_subscribers: crate::ipc::IpcSubscribers,
}

impl GlideState {
    #[must_use]
    pub fn new(
        backend_data: TtyData,
        loop_handle: LoopHandle<'static, Self>,
        loop_signal: LoopSignal,
        display: Display<Self>,
    ) -> Self {
        let start_time = std::time::Instant::now();
        let clock = smithay::utils::Clock::new();

        let dh = display.handle();
        let popups = PopupManager::default();

        let seat_name = backend_data.seat_name();
        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, seat_name);

        seat.add_keyboard(XkbConfig::default(), 200, 25)
            .expect("the default XKB keymap should always compile");

        seat.add_pointer();

        let space = Space::default();

        let socket_name = Self::init_wayland_listener(display, &loop_handle);

        #[allow(clippy::cast_sign_loss)]
        let protocols = Protocols::new(&dh, &loop_handle, seat_state, clock.id() as u32);
        let foreign_toplevel_handles = HashMap::new();

        Self {
            backend_data,

            start_time,
            clock,
            display_handle: dh,

            space,
            loop_signal,
            loop_handle,
            socket_name,

            protocols,
            popups,
            seat,
            layout: glide_layout::Layout::new(),
            window_map: HashMap::new(),
            window_to_id: HashMap::new(),
            output_ids: HashMap::new(),
            keybinds: Vec::new(),
            decoration_theme: crate::config::DecorationTheme::default(),
            overview_theme: crate::config::OverviewTheme::default(),
            background_color: crate::config::GeneralConfig::default().background,
            cursor_status: CursorImageStatus::default_named(),
            cursor_position_hint: None,
            layer_focus: None,
            overview: None,
            suppressed_keys: Vec::new(),

            foreign_toplevel_handles,
            idle_inhibited_surfaces: HashSet::new(),
            ipc_subscribers: crate::ipc::IpcSubscribers::default(),
        }
    }

    fn init_wayland_listener(
        display: Display<Self>,
        loop_handle: &LoopHandle<'static, Self>,
    ) -> OsString {
        let listening_socket = ListeningSocketSource::new_auto()
            .expect("failed to create a wayland-N socket in XDG_RUNTIME_DIR");

        let socket_name = listening_socket.socket_name().to_os_string();

        loop_handle
            .insert_source(listening_socket, move |client_stream, (), state| {
                state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .expect("a freshly accepted client stream is always a valid new client");
            })
            .expect("failed to register the wayland listening socket with the event loop");

        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
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

    pub fn reconcile_space(&mut self) {
        self.cleanup_dead_windows();

        self.refresh_overview();

        let visible: Vec<glide_layout::WindowState> = self.layout.visible_windows().collect();
        let visible_ids: HashSet<glide_layout::WindowId> =
            visible.iter().map(|state| state.id).collect();

        let focused = self.layout.focused_window();

        let scale = self
            .space
            .outputs()
            .next()
            .map_or(1, |output| output.current_scale().integer_scale());

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

        for state in &visible {
            let Some(window) = self.window_map.get(&state.id).cloned() else {
                continue;
            };

            let is_focused = focused == Some(state.id);

            let content = state.content_geometry();
            let location = Point::from((content.loc.x, content.loc.y));

            self.space.map_element(window.clone(), location, false);
            Self::apply_window_state(&window, state, is_focused);

            let decoration_height = if state.fullscreen {
                0
            } else {
                state.insets.top
            };
            window.sync_decoration(
                &self.decoration_theme,
                decoration_height,
                is_focused,
                state.maximized,
                scale,
            );
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
            self.space.unmap_elem(&window);
            self.window_to_id.remove(&window);
            self.window_map.remove(&id);
            self.layout.remove_window(id);
            if let Some(handle) = self.foreign_toplevel_handles.remove(&id) {
                handle.send_closed();
                self.protocols
                    .foreign_toplevel_list_state
                    .remove_toplevel(&handle);
            }
            self.ipc_subscribers
                .broadcast(&glide_ipc::Event::WindowClosed { id: id.as_u64() });
        }

        self.idle_inhibited_surfaces
            .retain(smithay::utils::IsAlive::alive);
        if self.idle_inhibited_surfaces.is_empty() {
            self.protocols.idle_notifier_state.set_is_inhibited(false);
        }

        self.suppressed_keys.clear();
        self.protocols
            .ext_workspace_state
            .sync_workspaces(&self.display_handle, &self.layout);
    }

    fn apply_window_state(window: &GlideWindow, state: &glide_layout::WindowState, focused: bool) {
        let Some(toplevel) = window.toplevel() else {
            return;
        };

        let mut changed = false;
        toplevel.with_pending_state(|pending| {
            changed |= set_state_flag(pending, XdgState::Activated, focused);
            changed |= set_state_flag(pending, XdgState::Maximized, state.maximized);
            changed |= set_state_flag(pending, XdgState::Fullscreen, state.fullscreen);

            let content = state.content_geometry();
            let size = state
                .is_placed
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

    fn apply_keyboard_focus(&mut self) {
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

        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let keyboard = self
            .seat
            .get_keyboard()
            .expect("a keyboard was added in GlideState::new");

        let current_focus = keyboard.current_focus();
        if current_focus != target {
            self.suppressed_keys.clear();
            let focused_window_id = self
                .layer_focus
                .is_none()
                .then(|| self.layout.focused_window())
                .flatten();
            self.ipc_subscribers
                .broadcast(&glide_ipc::Event::FocusChanged {
                    id: focused_window_id.map(glide_layout::WindowId::as_u64),
                });
        }

        keyboard.set_focus(self, target, serial);
    }

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

        self.layout
            .set_window_insets(id, glide_layout::Insets::NONE);

        self.window_map.insert(id, window.clone());
        self.window_to_id.insert(window.clone(), id);
        self.ipc_subscribers
            .broadcast(&glide_ipc::Event::WindowOpened { id: id.as_u64() });

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
            .protocols
            .foreign_toplevel_list_state
            .new_toplevel::<Self>(title, app_id);
        self.foreign_toplevel_handles.insert(id, handle);

        self.reconcile_space();
        self.protocols
            .ext_workspace_state
            .sync_workspaces(&self.display_handle, &self.layout);
    }

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
                    self.protocols
                        .foreign_toplevel_list_state
                        .remove_toplevel(&handle);
                }
                self.ipc_subscribers
                    .broadcast(&glide_ipc::Event::WindowClosed { id: id.as_u64() });
            }
        }

        self.cleanup_dead_windows();
        self.reconcile_space();
        self.protocols
            .ext_workspace_state
            .sync_workspaces(&self.display_handle, &self.layout);
    }

    #[must_use]
    pub fn window_id(&self, window: &GlideWindow) -> Option<glide_layout::WindowId> {
        self.window_to_id.get(window).copied()
    }

    #[must_use]
    pub fn window_id_for_surface(&self, surface: &WlSurface) -> Option<glide_layout::WindowId> {
        self.window_map
            .iter()
            .find(|(_, window)| window_surface(window) == surface)
            .map(|(&id, _)| id)
    }

    #[must_use]
    pub fn layer_for_surface(&self, surface: &WlSurface) -> Option<LayerSurface> {
        self.space.outputs().find_map(|output| {
            layer_map_for_output(output)
                .layer_for_surface(surface, WindowSurfaceType::ALL)
                .cloned()
        })
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

    pub fn apply_keyboard_focus_public(&mut self) {
        self.apply_keyboard_focus();
    }

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

    fn overview_area(&self) -> Rectangle<i32, Logical> {
        let area = self.layout.primary_output_work_area();
        Rectangle::new(
            Point::from((area.loc.x, area.loc.y)),
            smithay::utils::Size::from((area.size.width, area.size.height)),
        )
    }

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

    pub fn begin_close_overview(&mut self, select: Option<glide_layout::WindowId>) -> bool {
        if !self.layout.overview_active() {
            return false;
        }
        if let Some(ui) = self.overview.as_mut() {
            ui.begin_close(select);
            true
        } else {
            self.close_overview()
        }
    }

    pub fn close_overview(&mut self) -> bool {
        self.overview = None;
        self.layout.close_overview()
    }

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

    #[must_use]
    pub fn any_button_hover_animating(&self) -> bool {
        self.window_map.values().any(|window| {
            window
                .try_decoration_state()
                .is_some_and(|state| state.header.is_hover_animating())
        })
    }

    pub fn refresh_painter_font(&self) {
        #[allow(clippy::cast_possible_truncation)]
        let font_px = (f64::from(self.decoration_theme.height).max(14.0) * 0.45) as f32;
        crate::paint::init_or_replace(font_px.clamp(10.0, 18.0));
    }

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

    pub fn focus_window(&mut self, id: glide_layout::WindowId) {
        self.layer_focus = None;
        let closed = self.close_overview();
        if self.layout.focus_window(id) || closed {
            self.reconcile_space();
        }
    }

    pub fn sync_layout_outputs(&mut self) {
        let outputs: Vec<Output> = self.space.outputs().cloned().collect();

        self.output_ids.retain(|output, _| outputs.contains(output));

        let infos: Vec<glide_layout::OutputInfo> = outputs
            .into_iter()
            .map(|output| {
                let geometry = self.space.output_geometry(&output).unwrap_or_default();

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
        self.protocols
            .wlr_output_manager_state
            .sync_outputs(&self.display_handle, &self.space);
    }

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

    pub fn record_window_move(
        &mut self,
        window: &GlideWindow,
        content_location: Point<i32, Logical>,
    ) {
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

    pub fn record_window_resize(&mut self, window: &GlideWindow) {
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

    pub fn clear_focus(&mut self) {
        self.layer_focus = None;
        self.layout.clear_focus();
        self.reconcile_space();
    }

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

pub(crate) fn window_surface(window: &GlideWindow) -> &WlSurface {
    window
        .toplevel()
        .expect("only Window::new_wayland_window is used, so toplevel() is always Some")
        .wl_surface()
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
    pub security_context: Option<smithay::wayland::security_context::SecurityContext>,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

impl FractionalScaleHandler for GlideState {
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

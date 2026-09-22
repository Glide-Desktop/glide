use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
        TouchEvent as BackendTouchEvent,
    },
    input::{
        keyboard::{FilterResult, Keysym, ModifiersState, keysyms, xkb},
        pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent},
        touch::{DownEvent, MotionEvent as TouchMotionEvent, UpEvent},
    },
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER},
    wayland::{
        pointer_constraints::{PointerConstraint, with_pointer_constraint},
        seat::WaylandFocus,
    },
};

use crate::{
    overview::{Direction, OverviewUi},
    state::GlideState,
};

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    Quit,
    VtSwitch(i32),
    SwitchWorkspace(usize),
    MoveWindowToWorkspace(usize),
    NextWorkspace,
    PrevWorkspace,
    Maximize,
    Fullscreen,
    Minimize,
    CloseFocused,
    OpenWindowMenu,
    ToggleOverview,
    OverviewCancel,
    OverviewSelect,
    OverviewNav(Direction),
    Exec(String),
    Screenshot,
    ToggleLauncher,
    LauncherPage(bool),
    LauncherNav(i32, i32),
    LauncherActivate,
    LauncherBackspace,
    LauncherClear,
    LauncherType(char),
    ReplySend,
    ReplyBackspace,
    ReplyCancel,
    ReplyType(char),
    TopbarClosePopup,
    TopbarNav(i32, i32),
    TopbarActivate,
    None,
}

fn topbar_popup_shortcut(
    keybinds: &[ResolvedKeybind],
    modifiers: &ModifiersState,
    raw_syms: &[Keysym],
    modified_sym: Keysym,
) -> Option<KeyAction> {
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);
    if matches(Keysym::Escape) {
        return Some(KeyAction::TopbarClosePopup);
    }
    if matches(Keysym::Left) {
        return Some(KeyAction::TopbarNav(-1, 0));
    }
    if matches(Keysym::Right) {
        return Some(KeyAction::TopbarNav(1, 0));
    }
    if matches(Keysym::Up) {
        return Some(KeyAction::TopbarNav(0, -1));
    }
    if matches(Keysym::Down) {
        return Some(KeyAction::TopbarNav(0, 1));
    }
    if matches(Keysym::Tab) {
        let step = if modifiers.shift { -1 } else { 1 };
        return Some(KeyAction::TopbarNav(step, 0));
    }
    if matches(Keysym::Return) || matches(Keysym::KP_Enter) || matches(Keysym::space) {
        return Some(KeyAction::TopbarActivate);
    }
    process_keyboard_shortcut(keybinds, modifiers, raw_syms, modified_sym)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedKeybind {
    pub modifiers: ModifiersState,
    pub keysym: Keysym,
    pub action: KeyAction,
}

fn process_keyboard_shortcut(
    keybinds: &[ResolvedKeybind],
    modifiers: &ModifiersState,
    raw_syms: &[Keysym],
    modified_sym: Keysym,
) -> Option<KeyAction> {
    let matches_key =
        |entry: &ResolvedKeybind| entry.keysym == modified_sym || raw_syms.contains(&entry.keysym);

    let binding = keybinds.iter().find(|entry| {
        entry.modifiers.logo == modifiers.logo
            && entry.modifiers.shift == modifiers.shift
            && entry.modifiers.ctrl == modifiers.ctrl
            && entry.modifiers.alt == modifiers.alt
            && matches_key(entry)
    });

    if let Some(entry) = binding {
        return Some(entry.action.clone());
    }

    vt_switch_shortcut(raw_syms, modified_sym)
}

fn vt_switch_shortcut(raw_syms: &[Keysym], modified_sym: Keysym) -> Option<KeyAction> {
    raw_syms
        .iter()
        .chain(std::iter::once(&modified_sym))
        .map(|sym| sym.raw())
        .find(|raw| (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12).contains(raw))
        .map(|raw| KeyAction::VtSwitch((raw - keysyms::KEY_XF86Switch_VT_1 + 1).cast_signed()))
}

fn is_global_shortcut(action: &KeyAction) -> bool {
    matches!(action, KeyAction::Screenshot)
}

fn launcher_active_shortcut(
    keybinds: &[ResolvedKeybind],
    modifiers: &ModifiersState,
    raw_syms: &[Keysym],
    modified_sym: Keysym,
) -> KeyAction {
    launcher_shortcut(modifiers, raw_syms, modified_sym)
        .or_else(|| vt_switch_shortcut(raw_syms, modified_sym))
        .or_else(|| {
            process_keyboard_shortcut(keybinds, modifiers, raw_syms, modified_sym).filter(
                |action| is_global_shortcut(action) || matches!(action, KeyAction::ToggleLauncher),
            )
        })
        .or_else(|| launcher_text_input(modifiers, modified_sym))
        .unwrap_or(KeyAction::None)
}

fn reply_active_shortcut(
    keybinds: &[ResolvedKeybind],
    modifiers: &ModifiersState,
    raw_syms: &[Keysym],
    modified_sym: Keysym,
) -> KeyAction {
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);

    if matches(Keysym::Escape) {
        return KeyAction::ReplyCancel;
    }
    if matches(Keysym::BackSpace) {
        return KeyAction::ReplyBackspace;
    }
    if matches(Keysym::Return) || matches(Keysym::KP_Enter) {
        return KeyAction::ReplySend;
    }

    vt_switch_shortcut(raw_syms, modified_sym)
        .or_else(|| {
            process_keyboard_shortcut(keybinds, modifiers, raw_syms, modified_sym)
                .filter(is_global_shortcut)
        })
        .or_else(|| reply_text_input(modifiers, modified_sym))
        .unwrap_or(KeyAction::None)
}

fn reply_text_input(modifiers: &ModifiersState, modified_sym: Keysym) -> Option<KeyAction> {
    if modifiers.ctrl || modifiers.alt || modifiers.logo {
        return None;
    }
    let ch = modified_sym.key_char()?;
    (!ch.is_control()).then_some(KeyAction::ReplyType(ch))
}

fn launcher_shortcut(
    modifiers: &ModifiersState,
    raw_syms: &[Keysym],
    modified_sym: Keysym,
) -> Option<KeyAction> {
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);

    if matches(Keysym::Escape) {
        return Some(KeyAction::LauncherClear);
    }
    if matches(Keysym::BackSpace) {
        return Some(KeyAction::LauncherBackspace);
    }
    if matches(Keysym::Return) || matches(Keysym::KP_Enter) {
        return Some(KeyAction::LauncherActivate);
    }
    if matches(Keysym::Page_Up) {
        return Some(KeyAction::LauncherPage(false));
    }
    if matches(Keysym::Page_Down) {
        return Some(KeyAction::LauncherPage(true));
    }
    if matches(Keysym::Left) {
        return Some(KeyAction::LauncherNav(-1, 0));
    }
    if matches(Keysym::Right) {
        return Some(KeyAction::LauncherNav(1, 0));
    }
    if matches(Keysym::Up) {
        return Some(KeyAction::LauncherNav(0, -1));
    }
    if matches(Keysym::Down) {
        return Some(KeyAction::LauncherNav(0, 1));
    }
    if matches(Keysym::Tab) {
        let step = if modifiers.shift { -1 } else { 1 };
        return Some(KeyAction::LauncherNav(step, 0));
    }
    None
}

fn launcher_text_input(modifiers: &ModifiersState, modified_sym: Keysym) -> Option<KeyAction> {
    if modifiers.ctrl || modifiers.alt || modifiers.logo {
        return None;
    }
    let ch = modified_sym.key_char()?;
    (!ch.is_control()).then_some(KeyAction::LauncherType(ch))
}

fn overview_active_shortcut(
    keybinds: &[ResolvedKeybind],
    modifiers: &ModifiersState,
    raw_syms: &[Keysym],
    modified_sym: Keysym,
) -> KeyAction {
    overview_shortcut(modifiers, raw_syms, modified_sym)
        .or_else(|| vt_switch_shortcut(raw_syms, modified_sym))
        .or_else(|| {
            process_keyboard_shortcut(keybinds, modifiers, raw_syms, modified_sym)
                .filter(is_global_shortcut)
        })
        .unwrap_or(KeyAction::None)
}

fn overview_shortcut(
    modifiers: &ModifiersState,
    raw_syms: &[Keysym],
    modified_sym: Keysym,
) -> Option<KeyAction> {
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);

    // Space selects, but not while a modifier is held: the default layout
    // switch is Super+Space, and it must not also pick a window.
    let plain_space =
        matches(Keysym::space) && !modifiers.logo && !modifiers.ctrl && !modifiers.alt;

    if matches(Keysym::Escape) {
        Some(KeyAction::OverviewCancel)
    } else if matches(Keysym::Return) || matches(Keysym::KP_Enter) || plain_space {
        Some(KeyAction::OverviewSelect)
    } else if matches(Keysym::Left) || matches(Keysym::h) {
        Some(KeyAction::OverviewNav(Direction::Left))
    } else if matches(Keysym::Right) || matches(Keysym::l) {
        Some(KeyAction::OverviewNav(Direction::Right))
    } else if matches(Keysym::Up) || matches(Keysym::k) {
        Some(KeyAction::OverviewNav(Direction::Up))
    } else if matches(Keysym::Down) || matches(Keysym::j) {
        Some(KeyAction::OverviewNav(Direction::Down))
    } else {
        None
    }
}

fn clamp_to_outputs(
    pos: Point<f64, Logical>,
    outputs: &[Rectangle<i32, Logical>],
) -> Point<f64, Logical> {
    if outputs.is_empty() {
        return pos;
    }

    let (pos_x, pos_y) = pos.into();

    let mut best: Option<(f64, Point<f64, Logical>)> = None;
    for rect in outputs {
        let left = f64::from(rect.loc.x);
        let top = f64::from(rect.loc.y);
        let right = f64::from(rect.loc.x + rect.size.w);
        let bottom = f64::from(rect.loc.y + rect.size.h);

        let clamped: Point<f64, Logical> =
            (pos_x.clamp(left, right), pos_y.clamp(top, bottom)).into();
        let (dx, dy) = (clamped.x - pos_x, clamped.y - pos_y);
        let distance = dx * dx + dy * dy;

        if distance == 0.0 {
            return pos;
        }
        if best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, clamped));
        }
    }

    best.map_or(pos, |(_, clamped)| clamped)
}

impl GlideState {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        self.protocols
            .idle_notifier_state
            .notify_activity(&self.seat);
        self.last_activity = std::time::Instant::now();
        if self.outputs_blanked {
            self.set_outputs_blanked(false);
        }
        match event {
            InputEvent::Keyboard { event, .. } => self.handle_keyboard_event::<I>(&event),
            InputEvent::PointerMotion { event, .. } => self.handle_pointer_motion::<I>(&event),
            InputEvent::PointerMotionAbsolute { event, .. } => {
                self.handle_pointer_motion_absolute::<I>(&event);
            }
            InputEvent::PointerButton { event, .. } => self.handle_pointer_button::<I>(&event),
            InputEvent::PointerAxis { event, .. } => self.handle_pointer_axis::<I>(&event),
            InputEvent::TouchDown { event } => self.handle_touch_down::<I>(&event),
            InputEvent::TouchUp { event } => self.handle_touch_up::<I>(&event),
            InputEvent::TouchMotion { event } => self.handle_touch_motion::<I>(&event),
            InputEvent::TouchFrame { .. } => self.handle_touch_frame(),
            InputEvent::TouchCancel { .. } => self.handle_touch_cancel(),
            _ => {}
        }
    }

    fn touch_position<I: InputBackend>(
        &self,
        event: &impl AbsolutePositionEvent<I>,
    ) -> Option<Point<f64, Logical>> {
        let output = self.space.outputs().next()?;
        let geometry = self.space.output_geometry(output)?;
        Some(event.position_transformed(geometry.size) + geometry.loc.to_f64())
    }

    fn handle_touch_down<I: InputBackend>(&mut self, event: &I::TouchDownEvent) {
        let Some(touch) = self.seat.get_touch() else {
            return;
        };
        let Some(position) = self.touch_position::<I>(event) else {
            return;
        };
        self.queue_redraw();
        let serial = SERIAL_COUNTER.next_serial();
        let under = self
            .surface_under(position)
            .and_then(|(target, loc)| Some((target.wl_surface()?.into_owned(), loc.to_f64())));
        touch.down(
            self,
            under,
            &DownEvent {
                slot: event.slot(),
                location: position,
                serial,
                time: event.time(),
            },
        );
    }

    fn handle_touch_up<I: InputBackend>(&mut self, event: &I::TouchUpEvent) {
        let Some(touch) = self.seat.get_touch() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        touch.up(
            self,
            &UpEvent {
                slot: event.slot(),
                serial,
                time: event.time(),
            },
        );
    }

    fn handle_touch_motion<I: InputBackend>(&mut self, event: &I::TouchMotionEvent) {
        let Some(touch) = self.seat.get_touch() else {
            return;
        };
        let Some(position) = self.touch_position::<I>(event) else {
            return;
        };
        self.queue_redraw();
        let under = self
            .surface_under(position)
            .and_then(|(target, loc)| Some((target.wl_surface()?.into_owned(), loc.to_f64())));
        touch.motion(
            self,
            under,
            &TouchMotionEvent {
                slot: event.slot(),
                location: position,
                time: event.time(),
            },
        );
    }

    fn handle_touch_frame(&mut self) {
        if let Some(touch) = self.seat.get_touch() {
            touch.frame(self);
        }
    }

    fn handle_touch_cancel(&mut self) {
        if let Some(touch) = self.seat.get_touch() {
            touch.cancel(self);
        }
    }

    #[allow(clippy::too_many_lines)]
    fn handle_keyboard_event<I: InputBackend>(&mut self, event: &I::KeyboardKeyEvent) {
        let serial = SERIAL_COUNTER.next_serial();
        let time = event.time();
        let key_state = event.state();
        let keycode = event.key_code();
        let mut suppressed_keys = std::mem::take(&mut self.suppressed_keys);
        let keybinds = std::mem::take(&mut self.keybinds);

        let overview_active = self.layout.overview_active();
        let launcher_open = self
            .launcher
            .as_ref()
            .is_some_and(|launcher| !launcher.is_closing());
        let replying = self.notifications.reply_target().is_some();
        let topbar_popup = self.topbar.has_open_popup() || self.dock.menu_rect().is_some();
        let locked = self.session_lock.is_some();
        if locked {
            self.focus_lock_surface();
        }

        let exclusive_layer = self.exclusive_layer_surface();
        if let Some(layer) = exclusive_layer {
            self.layer_focus = Some(layer);
            self.apply_keyboard_focus_public();
        }
        let exclusive = self.layer_focus.is_some()
            && self
                .layer_focus
                .as_ref()
                .is_some_and(smithay::desktop::LayerSurface::can_receive_keyboard_focus);

        let shortcuts_inhibited = {
            use smithay::wayland::keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitorSeat;
            self.seat.keyboard_shortcuts_inhibited()
        };

        let keyboard = self
            .seat
            .get_keyboard()
            .expect("a keyboard was added in GlideState::new");
        let action: Option<KeyAction> = keyboard.input(
            self,
            keycode,
            key_state,
            serial,
            time,
            |_, modifiers, handle| {
                let keysym = handle.modified_sym();
                let mut binding_syms = handle.raw_syms();
                if let Some(latin) = handle.raw_latin_sym_or_raw_current_sym()
                    && !binding_syms.contains(&latin)
                {
                    binding_syms.push(latin);
                }

                tracing::debug!(
                    ?keycode,
                    ?key_state,
                    keysym = xkb::keysym_get_name(keysym),
                    mods = ?modifiers,
                    "key"
                );

                if let KeyState::Pressed = key_state {
                    if locked {
                        let action = vt_switch_shortcut(&binding_syms, keysym);
                        if let Some(action) = action {
                            suppressed_keys.push(keycode);
                            return FilterResult::Intercept(action);
                        }
                        return FilterResult::Forward;
                    }
                    let action = if replying && !exclusive {
                        Some(reply_active_shortcut(
                            &keybinds,
                            modifiers,
                            &binding_syms,
                            keysym,
                        ))
                    } else if launcher_open && !exclusive {
                        Some(launcher_active_shortcut(
                            &keybinds,
                            modifiers,
                            &binding_syms,
                            keysym,
                        ))
                    } else if overview_active {
                        Some(overview_active_shortcut(
                            &keybinds,
                            modifiers,
                            &binding_syms,
                            keysym,
                        ))
                    } else if topbar_popup && !exclusive {
                        topbar_popup_shortcut(&keybinds, modifiers, &binding_syms, keysym)
                    } else {
                        process_keyboard_shortcut(&keybinds, modifiers, &binding_syms, keysym)
                    };

                    let action = if exclusive || shortcuts_inhibited {
                        action.filter(|a| matches!(a, KeyAction::VtSwitch(_)))
                    } else {
                        action
                    };

                    if action.is_some() {
                        suppressed_keys.push(keycode);
                    }
                    action.map_or(FilterResult::Forward, FilterResult::Intercept)
                } else if suppressed_keys.contains(&keycode) {
                    suppressed_keys.retain(|k| *k != keycode);
                    FilterResult::Intercept(KeyAction::None)
                } else {
                    FilterResult::Forward
                }
            },
        );

        self.suppressed_keys = suppressed_keys;
        self.keybinds = keybinds;

        if let Some(action) = action {
            self.run_key_action(action);
        }
    }

    #[allow(clippy::too_many_lines)]
    fn run_key_action(&mut self, action: KeyAction) {
        match action {
            KeyAction::Quit => self.loop_signal.stop(),
            KeyAction::VtSwitch(vt) => {
                self.suppressed_keys.clear();
                self.backend_data.change_vt(vt);
            }
            KeyAction::SwitchWorkspace(idx) => {
                tracing::info!(workspace_index = idx, "switching workspace");
                self.suppressed_keys.clear();
                self.switch_workspace_animated(glide_layout::WorkspaceTarget::Index(idx));
                self.protocols
                    .ext_workspace_state
                    .sync_workspaces(&self.display_handle, &self.layout);
            }
            KeyAction::MoveWindowToWorkspace(idx) => {
                if let Some(focused) = self.layout.focused_window() {
                    tracing::info!(focused = ?focused, workspace_index = idx, "moving window to workspace");
                    self.suppressed_keys.clear();
                    self.layout.move_window_to_workspace(
                        focused,
                        glide_layout::WorkspaceTarget::Index(idx),
                    );
                    self.reconcile_space();
                    self.protocols
                        .ext_workspace_state
                        .sync_workspaces(&self.display_handle, &self.layout);
                }
            }
            KeyAction::NextWorkspace => {
                tracing::info!("switching to next workspace");
                self.suppressed_keys.clear();
                self.switch_workspace_animated(glide_layout::WorkspaceTarget::Next);
                self.protocols
                    .ext_workspace_state
                    .sync_workspaces(&self.display_handle, &self.layout);
            }
            KeyAction::PrevWorkspace => {
                tracing::info!("switching to prev workspace");
                self.suppressed_keys.clear();
                self.switch_workspace_animated(glide_layout::WorkspaceTarget::Previous);
                self.protocols
                    .ext_workspace_state
                    .sync_workspaces(&self.display_handle, &self.layout);
            }
            KeyAction::Maximize => {
                if let Some(focused) = self.layout.focused_window() {
                    tracing::info!(focused = ?focused, "toggling maximize");
                    self.morph_geometry(focused, |state| {
                        state.layout.toggle_maximized(focused);
                    });
                }
            }
            KeyAction::Fullscreen => {
                if let Some(focused) = self.layout.focused_window() {
                    tracing::info!(focused = ?focused, "toggling fullscreen");
                    self.morph_geometry(focused, |state| {
                        state.layout.toggle_fullscreen(focused);
                    });
                }
            }
            KeyAction::Minimize => {
                if let Some(focused) = self.layout.focused_window() {
                    tracing::info!(focused = ?focused, "minimizing");
                    self.minimize_with_flight(focused);
                }
            }
            KeyAction::CloseFocused => {
                if let Some(focused) = self.layout.focused_window()
                    && let Some(window) = self.window_map.get(&focused)
                    && let Some(toplevel) = window.toplevel()
                {
                    tracing::info!(focused = ?focused, "closing via keybind");
                    toplevel.send_close();
                }
            }
            KeyAction::OpenWindowMenu => {
                if let Some(focused) = self.layout.focused_window()
                    && let Some(window) = self.window_map.get(&focused)
                    && let Some(mut deco) = window.try_decoration_state_mut()
                {
                    tracing::info!(focused = ?focused, "opening titlebar menu via keybind");
                    deco.header.open_menu_at_default_position();
                }
                self.queue_redraw();
            }
            KeyAction::OverviewCancel => {
                self.begin_close_overview(None);
            }
            KeyAction::OverviewSelect => {
                let select = self.overview.as_ref().and_then(OverviewUi::hovered);
                self.begin_close_overview(select);
            }
            KeyAction::OverviewNav(dir) => self.navigate_overview(dir),
            KeyAction::ToggleOverview => {
                if self.layout.overview_active() {
                    self.begin_close_overview(None);
                } else {
                    self.open_overview();
                }
            }
            KeyAction::Exec(cmd) => {
                tracing::info!(command = %cmd, "executing keybind command");
                if let Err(err) = crate::spawn_shell_command(&cmd) {
                    tracing::error!(command = %cmd, ?err, "failed to execute keybind command");
                }
            }
            KeyAction::ToggleLauncher => self.toggle_launcher(),
            KeyAction::LauncherPage(forward) => {
                if let Some(launcher) = self.launcher.as_mut()
                    && launcher.turn_page(forward)
                {
                    self.queue_redraw();
                }
            }
            KeyAction::LauncherNav(dx, dy) => {
                if let Some(launcher) = self.launcher.as_mut()
                    && launcher.move_selection(dx, dy)
                {
                    self.queue_redraw();
                }
            }
            KeyAction::LauncherActivate => {
                if let Some(selected) = self
                    .launcher
                    .as_ref()
                    .and_then(crate::launcher::Launcher::selected)
                {
                    self.activate_launcher_app(selected);
                }
            }
            KeyAction::ReplyType(ch) => {
                if self.notifications.type_reply(ch) {
                    self.queue_redraw();
                }
            }
            KeyAction::ReplyBackspace => {
                if self.notifications.backspace_reply() {
                    self.queue_redraw();
                }
            }
            KeyAction::ReplyCancel => {
                if self.notifications.cancel_reply() {
                    self.queue_redraw();
                }
            }
            KeyAction::ReplySend => {
                if self.notifications.send_reply() {
                    self.refresh_topbar();
                    self.queue_redraw();
                }
            }
            KeyAction::LauncherType(ch) => {
                if let Some(launcher) = self.launcher.as_mut()
                    && launcher.push_query(ch)
                {
                    self.queue_redraw();
                }
            }
            KeyAction::LauncherBackspace => {
                if let Some(launcher) = self.launcher.as_mut()
                    && launcher.pop_query()
                {
                    self.queue_redraw();
                }
            }
            KeyAction::LauncherClear => {
                let menu_closed = self
                    .launcher
                    .as_mut()
                    .is_some_and(crate::launcher::Launcher::close_menu);
                let cleared = menu_closed
                    || self
                        .launcher
                        .as_mut()
                        .is_some_and(crate::launcher::Launcher::clear_query);
                if cleared {
                    self.queue_redraw();
                } else {
                    self.toggle_launcher();
                }
            }
            KeyAction::Screenshot => {
                tracing::info!("screenshot requested");
                self.pending_screenshot = true;
                self.queue_redraw();
            }
            KeyAction::TopbarClosePopup => {
                let topbar_closed = self.topbar.close_popup();
                let dock_closed = self.dock.close_menu();
                if topbar_closed || dock_closed {
                    self.queue_redraw();
                }
            }
            KeyAction::TopbarNav(dx, dy) => {
                let adjusted = if dx != 0 && self.topbar.has_focus() {
                    let delta = i8::try_from(dx * 5).unwrap_or(0);
                    self.topbar.focus_adjust(delta)
                } else {
                    None
                };
                if let Some(action) = adjusted {
                    self.run_topbar_action(action);
                    self.queue_redraw();
                } else if self.topbar.focus_nav(dx, dy) {
                    self.queue_redraw();
                }
            }
            KeyAction::TopbarActivate => {
                if let Some(action) = self.topbar.focused_action() {
                    self.run_topbar_action(action);
                    self.queue_redraw();
                }
            }
            KeyAction::None => {}
        }
    }

    fn navigate_overview(&mut self, dir: Direction) {
        if self.overview.as_mut().is_some_and(|ui| ui.navigate(dir)) {
            self.queue_redraw();
        }
    }

    fn launcher_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        let open = self
            .launcher
            .as_ref()
            .is_some_and(|launcher| !launcher.is_closing());
        if !open {
            return false;
        }
        let area = self.launcher_area();
        let hit = self
            .launcher
            .as_ref()
            .and_then(|launcher| launcher.app_at(area, pos));
        let Some(launcher) = self.launcher.as_mut() else {
            return true;
        };
        let menu_changed = launcher.set_menu_hover(pos);
        let hover_changed = launcher.set_hover(hit);
        if menu_changed || hover_changed {
            self.queue_redraw();
        }
        true
    }

    fn topbar_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        if self.session_lock.is_some() || !self.topbar.is_enabled() {
            return false;
        }
        if let Some(drag_slot) = self.active_slider_drag
            && let Some(rect) = self.topbar.slot_rect(drag_slot)
        {
            let ratio = ((pos.x - f64::from(rect.loc.x)) / f64::from(rect.size.w)).clamp(0.0, 1.0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // Ratio is clamped to 0.0..=1.0
            let pct = (ratio * 100.0).round() as u8;
            match drag_slot {
                crate::topbar::Slot::QsVolumeTrack => {
                    self.handle_qs_action(crate::topbar::TopbarAction::QsVolumeAdjust(pct));
                }
                crate::topbar::Slot::QsBrightnessTrack => {
                    self.handle_qs_action(crate::topbar::TopbarAction::QsBrightnessAdjust(pct));
                }
                _ => {}
            }
            self.queue_redraw();
            return true;
        }
        let output = self.output_rect();
        let over = self.topbar.contains(output, pos);
        if self.topbar.set_hover(over.then_some(pos)) {
            self.queue_redraw();
        }
        over
    }

    fn notifications_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        if self.session_lock.is_some() || !self.notifications.is_enabled() {
            return false;
        }
        let output = self.output_rect();
        let over = self.notifications.contains(output, pos);
        if self.notifications.set_hover(over.then_some(pos)) {
            self.queue_redraw();
        }
        over
    }

    fn dock_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        if self.session_lock.is_some() {
            return false;
        }
        if !self.dock.is_enabled() || self.layout.overview_active() {
            if self.dock.set_hover(None) {
                self.queue_redraw();
            }
            return false;
        }
        let area = self.dock_area();
        if self.dock.drag_motion(pos) {
            self.queue_redraw();
            return true;
        }
        let over = self.dock.contains(area, pos);
        let item = if over {
            self.dock.item_at(area, pos)
        } else {
            None
        };
        let tile_changed = self.dock.set_hover(item);
        let menu_changed = self.dock.set_menu_hover(over.then_some(pos));
        if tile_changed || menu_changed {
            self.queue_redraw();
        }
        over
    }

    fn overview_motion(
        &mut self,
        pos: Point<f64, Logical>,
    ) -> Option<(crate::focus::PointerFocusTarget, Point<f64, Logical>)> {
        let over_launcher = self.launcher_motion(pos);
        let over_topbar = self.topbar_motion(pos);
        let over_notifications = self.notifications_motion(pos);
        let over_dock = self.dock_motion(pos);
        if over_dock || over_launcher || over_topbar || over_notifications {
            return None;
        }
        if !self.layout.overview_active() {
            return self.surface_under(pos);
        }

        let under = self.surface_under(pos);
        let on_layer = matches!(
            under,
            Some((crate::focus::PointerFocusTarget::Surface(ref surface), _))
                if self.layer_for_surface(surface).is_some()
        );

        let on_close = self
            .overview
            .as_ref()
            .and_then(|ui| ui.close_button_at(pos));
        let hovered = self
            .overview
            .as_ref()
            .and_then(|ui| ui.preview_at(pos))
            .or(on_close);

        let changed = self.overview.as_mut().is_some_and(|ui| {
            let ring = ui.set_hover(hovered);
            let button = ui.set_close_hover(on_close);
            ring || button
        });
        if changed {
            self.queue_redraw();
        }

        if on_layer { under } else { None }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "ported closely from anvil's pointer-constraints reference implementation to minimize the risk of a novel bug in the pointer-motion hot path"
    )]
    fn handle_pointer_motion<I: InputBackend>(&mut self, event: &I::PointerMotionEvent) {
        self.queue_redraw();
        let pointer = self
            .seat
            .get_pointer()
            .expect("a pointer was added in GlideState::new");
        let mut pointer_location = pointer.current_location();
        let serial = SERIAL_COUNTER.next_serial();

        let under = self.overview_motion(pointer_location);

        let mut pointer_locked = false;
        let mut pointer_confined = false;
        let mut confine_region = None;
        if let Some((surface, surface_loc)) = under
            .as_ref()
            .and_then(|(target, loc)| Some((target.wl_surface()?.into_owned(), *loc)))
        {
            with_pointer_constraint(&surface, &pointer, |constraint| {
                let Some(constraint) = constraint.filter(|c| c.is_active()) else {
                    return;
                };
                let in_region = constraint.region().is_none_or(|region| {
                    region.contains((pointer_location - surface_loc).to_i32_round())
                });
                if !in_region {
                    return;
                }
                match &*constraint {
                    PointerConstraint::Locked(_) => pointer_locked = true,
                    PointerConstraint::Confined(confine) => {
                        pointer_confined = true;
                        confine_region = confine.region().cloned();
                    }
                }
            });
        }

        pointer.relative_motion(
            self,
            under.clone(),
            &RelativeMotionEvent {
                delta: event.delta(),
                delta_unaccel: event.delta_unaccel(),
                time: event.time(),
            },
        );

        if pointer_locked {
            pointer.frame(self);
            return;
        }

        let mut delta = self.clamp_coords(pointer_location + event.delta()) - pointer_location;

        if pointer_confined
            && let (Some((_, surface_loc)), Some(region)) = (&under, &confine_region)
        {
            if !region.contains(
                (pointer_location + Point::from((delta.x, 0.0)) - *surface_loc).to_i32_round(),
            ) {
                delta.x = 0.0;
            }
            if !region.contains(
                (pointer_location + Point::from((0.0, delta.y)) - *surface_loc).to_i32_round(),
            ) {
                delta.y = 0.0;
            }
        }

        pointer_location += delta;

        let new_under = self.overview_motion(pointer_location);

        if pointer_confined
            && let Some((surface, _)) = &under
            && new_under
                .as_ref()
                .and_then(|(target, _)| target.wl_surface())
                != surface.wl_surface()
        {
            pointer.frame(self);
            return;
        }

        tracing::trace!(
            delta = ?event.delta(),
            location = ?pointer_location,
            "pointer motion (relative)"
        );

        pointer.motion(
            self,
            new_under.clone(),
            &MotionEvent {
                location: pointer_location,
                serial,
                time: event.time(),
            },
        );
        pointer.frame(self);

        if let Some((surface, surface_location)) =
            new_under.and_then(|(target, loc)| Some((target.wl_surface()?.into_owned(), loc)))
        {
            with_pointer_constraint(&surface, &pointer, |constraint| {
                let Some(constraint) = constraint.filter(|c| !c.is_active()) else {
                    return;
                };
                let point = (pointer_location - surface_location).to_i32_round();
                if constraint
                    .region()
                    .is_none_or(|region| region.contains(point))
                {
                    constraint.activate();
                }
            });
        }
    }

    fn handle_pointer_motion_absolute<I: InputBackend>(
        &mut self,
        event: &I::PointerMotionAbsoluteEvent,
    ) {
        self.queue_redraw();
        let Some(output) = self.space.outputs().next() else {
            return;
        };
        let output_geo = self
            .space
            .output_geometry(output)
            .expect("output came from space.outputs(), so it has a geometry");

        let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

        let serial = SERIAL_COUNTER.next_serial();

        let pointer = self
            .seat
            .get_pointer()
            .expect("a pointer was added in GlideState::new");

        let under = self.overview_motion(pos);

        pointer.motion(
            self,
            under,
            &MotionEvent {
                location: pos,
                serial,
                time: event.time(),
            },
        );
        pointer.frame(self);
    }

    #[allow(clippy::too_many_lines)]
    fn handle_pointer_button<I: InputBackend>(&mut self, event: &I::PointerButtonEvent) {
        let pointer = self
            .seat
            .get_pointer()
            .expect("a pointer was added in GlideState::new");
        let serial = SERIAL_COUNTER.next_serial();

        let button = event.button_code();

        let button_state = event.state();

        if button_state == ButtonState::Released {
            self.active_slider_drag = None;
            if button == BTN_LEFT {
                let area = self.dock_area();
                let pos = pointer.current_location();
                match self.dock.end_press(area, pos) {
                    crate::dock::PressOutcome::Click(index) => self.activate_dock_item(index),
                    crate::dock::PressOutcome::Drop { index, target } => {
                        self.drop_dock_tile(index, target);
                    }
                    crate::dock::PressOutcome::Nothing => {}
                }
            }
        }

        if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
            let pos = pointer.current_location();

            if self.session_lock.is_some() {
                self.focus_lock_surface();
            } else if self.shell_button(pos, button) {
                pointer.frame(self);
                return;
            } else {
                self.focus_click_target(pos);
            }
        }

        pointer.button(
            self,
            &ButtonEvent {
                button,
                state: button_state,
                serial,
                time: event.time(),
            },
        );
        pointer.frame(self);
    }

    fn shell_button(&mut self, pos: Point<f64, Logical>, button: u32) -> bool {
        if self.topbar_button(pos) {
            return true;
        }
        if self.notifications_button(pos) {
            return true;
        }
        if self.launcher_button(pos, button) {
            return true;
        }
        if self.dock_button(pos, button) {
            return true;
        }
        self.overview_button(pos, button)
    }

    fn topbar_button(&mut self, pos: Point<f64, Logical>) -> bool {
        if self.topbar.contains(self.output_rect(), pos) {
            if let Some(action) = self.topbar.action_at(pos) {
                if let Some(slot) = self.topbar.slot_at(pos)
                    && matches!(
                        slot,
                        crate::topbar::Slot::QsVolumeTrack | crate::topbar::Slot::QsBrightnessTrack
                    )
                {
                    self.active_slider_drag = Some(slot);
                }
                self.run_topbar_action(action);
            }
            return true;
        }
        if self.topbar.close_popup() {
            self.queue_redraw();
            return true;
        }
        false
    }

    fn notifications_button(&mut self, pos: Point<f64, Logical>) -> bool {
        if !self.notifications.contains(self.output_rect(), pos) {
            return false;
        }
        if let Some(action) = self.notifications.action_at(pos) {
            self.run_notification_action(action);
        }
        true
    }

    fn launcher_button(&mut self, pos: Point<f64, Logical>, button: u32) -> bool {
        let area = self.launcher_area();
        let Some(launcher) = self.launcher.as_mut().filter(|l| !l.is_closing()) else {
            return false;
        };
        if let (Some(item), Some(result)) = (launcher.menu_item_at(pos), launcher.menu_result()) {
            launcher.close_menu();
            self.run_launcher_menu_item(result, item);
            self.queue_redraw();
            return true;
        }
        if launcher.close_menu() {
            self.queue_redraw();
            return true;
        }

        let hit = launcher.app_at(area, pos);
        if button == BTN_RIGHT {
            if let Some(result) = hit {
                let pinned = self.launcher_app_is_pinned(result);
                let opened = self
                    .launcher
                    .as_mut()
                    .is_some_and(|launcher| launcher.open_menu(result, area, pinned));
                if opened {
                    self.queue_redraw();
                }
            }
            return true;
        }

        let dot = launcher.dot_at(area, pos);
        let on_search = launcher.search_contains(area, pos);
        let chip = launcher.category_at(pos);
        let changed = if let Some(result) = hit {
            self.activate_launcher_app(result);
            false
        } else if let Some(category) = chip {
            launcher.toggle_category(category)
        } else if let Some(page) = dot {
            launcher.go_to_page(page)
        } else if on_search {
            false
        } else {
            launcher.begin_close();
            true
        };
        if changed {
            self.queue_redraw();
        }
        true
    }

    fn dock_button(&mut self, pos: Point<f64, Logical>, button: u32) -> bool {
        if !self.dock.is_enabled() || self.layout.overview_active() {
            return false;
        }
        let area = self.dock_area();
        if let Some(item) = self.dock.menu_item_at(pos) {
            let index = self.dock.menu_index();
            self.dock.close_menu();
            if let Some(index) = index {
                self.run_dock_menu_item(index, item);
            }
            self.queue_redraw();
            return true;
        }
        let menu_closed = self.dock.close_menu();
        if menu_closed {
            self.queue_redraw();
        }
        if !self.dock.contains(area, pos) {
            return menu_closed;
        }
        if let Some(index) = self.dock.item_at(area, pos) {
            if button == BTN_RIGHT {
                if self.dock.open_menu(index, area) {
                    self.queue_redraw();
                }
            } else if button == BTN_MIDDLE {
                self.run_dock_menu_item(index, crate::dock::DockMenuItem::NewWindow);
            } else if self.dock.action(index) == Some(crate::dock::DockAction::OpenLauncher) {
                self.toggle_launcher();
            } else {
                self.dock.begin_press(index, pos, area);
            }
        }
        true
    }

    fn overview_button(&mut self, pos: Point<f64, Logical>, button: u32) -> bool {
        if !self.layout.overview_active() {
            return false;
        }
        let on_layer = matches!(
            self.surface_under(pos),
            Some((crate::focus::PointerFocusTarget::Surface(ref surface), _))
                if self.layer_for_surface(surface).is_some()
        );
        if on_layer {
            return false;
        }

        let close = self
            .overview
            .as_ref()
            .and_then(|ui| ui.close_button_at(pos));
        if let Some(id) = close {
            tracing::info!(window = ?id, "closing from the overview");
            self.send_close(id);
            return true;
        }

        match self.overview.as_ref().and_then(|ui| ui.preview_at(pos)) {
            Some(id) if button == BTN_MIDDLE => self.send_close(id),
            hit => {
                self.begin_close_overview(hit);
            }
        }
        true
    }

    fn send_close(&self, id: glide_layout::WindowId) {
        if let Some(toplevel) = self.window_map.get(&id).and_then(|w| w.toplevel()) {
            toplevel.send_close();
        }
    }

    fn focus_click_target(&mut self, pos: Point<f64, Logical>) {
        let target = self.surface_under(pos).map(|(target, _)| target);
        match target {
            Some(crate::focus::PointerFocusTarget::Surface(surface))
                if self.layer_for_surface(&surface).is_some() =>
            {
                let layer = self
                    .layer_for_surface(&surface)
                    .expect("matched Some in the guard above");
                if layer.can_receive_keyboard_focus() {
                    self.layer_focus = Some(layer);
                    self.apply_keyboard_focus_public();
                }
            }
            Some(crate::focus::PointerFocusTarget::Decoration(deco)) => {
                if let Some(id) = self.window_id(&deco.0) {
                    self.focus_window(id);
                }
            }
            Some(crate::focus::PointerFocusTarget::Surface(_)) => {
                let clicked = self
                    .space
                    .element_under(pos)
                    .map(|(window, _)| window.clone());
                if let Some(id) = clicked.as_ref().and_then(|window| self.window_id(window)) {
                    self.focus_window(id);
                }
            }
            None => self.clear_focus(),
        }
    }

    fn topbar_scroll<I: InputBackend>(&mut self, event: &I::PointerAxisEvent) -> bool {
        if self.session_lock.is_some() || !self.topbar.is_enabled() {
            return false;
        }
        let pos = self
            .seat
            .get_pointer()
            .expect("a pointer was added in GlideState::new")
            .current_location();
        let Some(slot) = self.topbar.slot_at(pos) else {
            return self.topbar.contains(self.output_rect(), pos);
        };
        let vertical = event
            .amount(Axis::Vertical)
            .unwrap_or_else(|| event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.0);
        if vertical == 0.0 {
            return true;
        }
        let step: i16 = if vertical < 0.0 { 5 } else { -5 };
        let status = self.topbar.system_status();
        let action = match slot {
            crate::topbar::Slot::Status
            | crate::topbar::Slot::QsVolumeMute
            | crate::topbar::Slot::QsVolumeTrack => {
                let pct =
                    (i16::from(status.volume_pct) + step).clamp(0, i16::from(self.max_volume_pct));
                Some(crate::topbar::TopbarAction::QsVolumeAdjust(
                    u8::try_from(pct).unwrap_or(0),
                ))
            }
            crate::topbar::Slot::QsBrightnessTrack => {
                let pct = (i16::from(status.brightness_pct) + step).clamp(1, 100);
                Some(crate::topbar::TopbarAction::QsBrightnessAdjust(
                    u8::try_from(pct).unwrap_or(1),
                ))
            }
            _ => None,
        };
        if let Some(action) = action {
            self.handle_qs_action(action);
            self.queue_redraw();
        }
        true
    }

    fn handle_pointer_axis<I: InputBackend>(&mut self, event: &I::PointerAxisEvent) {
        if self.topbar_scroll::<I>(event) {
            return;
        }
        if self
            .launcher
            .as_ref()
            .is_some_and(|launcher| !launcher.is_closing())
        {
            let vertical = event
                .amount(Axis::Vertical)
                .unwrap_or_else(|| event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.0);
            let horizontal = event.amount(Axis::Horizontal).unwrap_or_else(|| {
                event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.0
            });
            let amount = if vertical == 0.0 {
                horizontal
            } else {
                vertical
            };
            if amount != 0.0
                && let Some(launcher) = self.launcher.as_mut()
                && launcher.scroll_by(amount)
            {
                self.queue_redraw();
            }
            return;
        }

        let source = event.source();

        let horizontal_amount = event
            .amount(Axis::Horizontal)
            .unwrap_or_else(|| event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.);
        let vertical_amount = event
            .amount(Axis::Vertical)
            .unwrap_or_else(|| event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.);
        let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
        let vertical_amount_discrete = event.amount_v120(Axis::Vertical);

        let mut frame = AxisFrame::new(event.time()).source(source);
        if horizontal_amount != 0.0 {
            frame = frame.value(Axis::Horizontal, horizontal_amount);
            if let Some(discrete) = horizontal_amount_discrete {
                #[allow(clippy::cast_possible_truncation)]
                {
                    frame = frame.v120(Axis::Horizontal, discrete as i32);
                }
            }
        }
        if vertical_amount != 0.0 {
            frame = frame.value(Axis::Vertical, vertical_amount);
            if let Some(discrete) = vertical_amount_discrete {
                #[allow(clippy::cast_possible_truncation)]
                {
                    frame = frame.v120(Axis::Vertical, discrete as i32);
                }
            }
        }

        if source == AxisSource::Finger {
            if event.amount(Axis::Horizontal) == Some(0.0) {
                frame = frame.stop(Axis::Horizontal);
            }
            if event.amount(Axis::Vertical) == Some(0.0) {
                frame = frame.stop(Axis::Vertical);
            }
        }

        let pointer = self
            .seat
            .get_pointer()
            .expect("a pointer was added in GlideState::new");
        pointer.axis(self, frame);
        pointer.frame(self);
    }

    fn clamp_coords(&self, pos: Point<f64, Logical>) -> Point<f64, Logical> {
        let output_rects: Vec<Rectangle<i32, Logical>> = self
            .space
            .outputs()
            .map(|o| {
                self.space
                    .output_geometry(o)
                    .expect("output came from space.outputs(), so it has a geometry")
            })
            .collect();
        clamp_to_outputs(pos, &output_rects)
    }
}

#[cfg(test)]
mod tests {
    use smithay::{
        input::keyboard::ModifiersState,
        utils::{Point, Size},
    };

    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((x, y)), Size::from((w, h)))
    }

    fn no_modifiers() -> ModifiersState {
        ModifiersState::default()
    }

    mod launcher_keys {
        use super::*;

        fn typed(sym: Keysym) -> KeyAction {
            launcher_active_shortcut(&[], &no_modifiers(), &[sym], sym)
        }

        fn with(mods: ModifiersState, sym: Keysym) -> KeyAction {
            launcher_active_shortcut(&[], &mods, &[sym], sym)
        }

        #[test]
        fn a_plain_letter_goes_into_the_search_field() {
            assert_eq!(typed(Keysym::a), KeyAction::LauncherType('a'));
            assert_eq!(typed(Keysym::z), KeyAction::LauncherType('z'));
        }

        #[test]
        fn digits_and_punctuation_are_typed_too() {
            assert_eq!(typed(Keysym::_7), KeyAction::LauncherType('7'));
            assert_eq!(typed(Keysym::minus), KeyAction::LauncherType('-'));
        }

        #[test]
        fn space_is_typed_rather_than_swallowed() {
            assert_eq!(typed(Keysym::space), KeyAction::LauncherType(' '));
        }

        #[test]
        fn shift_still_types_so_capitals_reach_the_query() {
            let mods = ModifiersState {
                shift: true,
                ..ModifiersState::default()
            };
            assert_eq!(with(mods, Keysym::A), KeyAction::LauncherType('A'));
        }

        #[test]
        fn a_modifier_combination_is_left_for_the_global_bindings() {
            for mods in [
                ModifiersState {
                    ctrl: true,
                    ..ModifiersState::default()
                },
                ModifiersState {
                    alt: true,
                    ..ModifiersState::default()
                },
                ModifiersState {
                    logo: true,
                    ..ModifiersState::default()
                },
            ] {
                assert_eq!(
                    with(mods, Keysym::a),
                    KeyAction::None,
                    "a bound combination must not leak into the search field"
                );
            }
        }

        #[test]
        fn editing_and_navigation_keys_keep_their_meaning() {
            assert_eq!(typed(Keysym::BackSpace), KeyAction::LauncherBackspace);
            assert_eq!(typed(Keysym::Escape), KeyAction::LauncherClear);
            assert_eq!(typed(Keysym::Return), KeyAction::LauncherActivate);
            assert_eq!(typed(Keysym::Right), KeyAction::LauncherNav(1, 0));
            assert_eq!(typed(Keysym::Down), KeyAction::LauncherNav(0, 1));
        }

        #[test]
        fn shift_tab_walks_the_selection_backwards() {
            let mods = ModifiersState {
                shift: true,
                ..ModifiersState::default()
            };
            assert_eq!(with(mods, Keysym::Tab), KeyAction::LauncherNav(-1, 0));
            assert_eq!(typed(Keysym::Tab), KeyAction::LauncherNav(1, 0));
        }
    }

    mod clamp_to_outputs {
        use super::*;

        #[test]
        fn passes_position_through_unchanged_when_no_outputs_are_mapped() {
            let pos = Point::from((-100.0, 500.0));

            let result = clamp_to_outputs(pos, &[]);

            assert_eq!(result, pos);
        }

        #[test]
        fn leaves_position_unchanged_when_inside_a_single_output() {
            let outputs = [rect(0, 0, 1920, 1080)];
            let pos = Point::from((500.0, 500.0));

            let result = clamp_to_outputs(pos, &outputs);

            assert_eq!(result, pos);
        }

        #[test]
        #[allow(clippy::float_cmp)]
        fn clamps_x_to_the_left_edge() {
            let outputs = [rect(0, 0, 1920, 1080)];

            let result = clamp_to_outputs(Point::from((-50.0, 500.0)), &outputs);

            assert_eq!(result.x, 0.0);
        }

        #[test]
        #[allow(clippy::float_cmp)]
        fn clamps_x_to_the_right_edge_of_the_combined_outputs() {
            let outputs = [rect(0, 0, 1920, 1080)];

            let result = clamp_to_outputs(Point::from((5000.0, 500.0)), &outputs);

            assert_eq!(result.x, 1920.0);
        }

        #[test]
        fn snaps_to_the_nearest_output_point_below_mixed_heights() {
            let outputs = [rect(0, 0, 1920, 1080), rect(1920, 0, 1280, 720)];

            let result = clamp_to_outputs(Point::from((2500.0, 5000.0)), &outputs);

            assert!(
                (result.x - 1920.0).abs() < f64::EPSILON
                    && (result.y - 1080.0).abs() < f64::EPSILON,
                "expected the taller output's corner, got ({}, {})",
                result.x,
                result.y
            );
        }

        #[test]
        fn a_position_in_a_gap_is_pulled_into_an_output() {
            let outputs = [rect(0, 0, 1000, 1080), rect(2000, 0, 1000, 1080)];

            let result = clamp_to_outputs(Point::from((1500.0, 5000.0)), &outputs);

            let nudged: Point<f64, Logical> = Point::from((result.x - 0.5, result.y - 0.5));
            let inside = outputs
                .iter()
                .any(|r| r.to_f64().contains(result) || r.to_f64().contains(nudged));
            assert!(
                inside,
                "({}, {}) is still outside every output",
                result.x, result.y
            );
        }

        #[test]
        fn respects_output_origins_when_stacked_vertically() {
            let outputs = [rect(0, 0, 1920, 1080), rect(0, 1080, 1920, 1080)];

            let result = clamp_to_outputs(Point::from((500.0, 1500.0)), &outputs);

            assert!(
                (result.x - 500.0).abs() < f64::EPSILON && (result.y - 1500.0).abs() < f64::EPSILON,
                "a position inside the lower output moved to ({}, {})",
                result.x,
                result.y
            );
        }
    }

    mod process_keyboard_shortcut {
        use super::*;

        fn default_binds() -> Vec<ResolvedKeybind> {
            crate::config::Config::with_defaults().resolve_keybinds()
        }

        #[test]
        fn super_q_quits() {
            let binds = default_binds();
            let mut modifiers = no_modifiers();
            modifiers.logo = true;

            let action = process_keyboard_shortcut(&binds, &modifiers, &[], Keysym::q);

            assert_eq!(action, Some(KeyAction::Quit));
        }

        #[test]
        fn q_alone_without_super_does_not_quit() {
            let binds = default_binds();
            let action = process_keyboard_shortcut(&binds, &no_modifiers(), &[], Keysym::q);

            assert_eq!(action, None);
        }

        #[test]
        fn alt_f4_closes_the_focused_window() {
            let binds = default_binds();
            let mut modifiers = no_modifiers();
            modifiers.alt = true;

            let action = process_keyboard_shortcut(&binds, &modifiers, &[], Keysym::F4);

            assert_eq!(action, Some(KeyAction::CloseFocused));
        }

        #[test]
        fn f4_alone_without_alt_does_not_close() {
            let binds = default_binds();
            let action = process_keyboard_shortcut(&binds, &no_modifiers(), &[], Keysym::F4);

            assert_eq!(action, None);
        }

        #[test]
        fn xf86_switch_vt_2_switches_to_vt_2() {
            let binds = default_binds();
            let keysym = Keysym::from(keysyms::KEY_XF86Switch_VT_2);

            let action = process_keyboard_shortcut(&binds, &no_modifiers(), &[], keysym);

            assert_eq!(action, Some(KeyAction::VtSwitch(2)));
        }

        #[test]
        fn xf86_switch_vt_12_switches_to_vt_12() {
            let binds = default_binds();
            let keysym = Keysym::from(keysyms::KEY_XF86Switch_VT_12);

            let action = process_keyboard_shortcut(&binds, &no_modifiers(), &[], keysym);

            assert_eq!(action, Some(KeyAction::VtSwitch(12)));
        }

        #[test]
        fn unrelated_key_has_no_shortcut() {
            let binds = default_binds();
            let action = process_keyboard_shortcut(&binds, &no_modifiers(), &[], Keysym::a);

            assert_eq!(action, None);
        }

        #[test]
        fn super_digits_switches_workspace() {
            let binds = default_binds();
            let mut modifiers = no_modifiers();
            modifiers.logo = true;

            let action =
                process_keyboard_shortcut(&binds, &modifiers, &[], Keysym::from(keysyms::KEY_1));
            assert_eq!(action, Some(KeyAction::SwitchWorkspace(0)));

            let action =
                process_keyboard_shortcut(&binds, &modifiers, &[], Keysym::from(keysyms::KEY_9));
            assert_eq!(action, Some(KeyAction::SwitchWorkspace(8)));
        }

        #[test]
        fn super_shift_digits_moves_window_to_workspace() {
            let binds = default_binds();
            let mut modifiers = no_modifiers();
            modifiers.logo = true;
            modifiers.shift = true;

            let action =
                process_keyboard_shortcut(&binds, &modifiers, &[], Keysym::from(keysyms::KEY_3));
            assert_eq!(action, Some(KeyAction::MoveWindowToWorkspace(2)));
        }

        #[test]
        fn shifted_keys_match_on_the_raw_keysym() {
            let binds = default_binds();
            let mut modifiers = no_modifiers();
            modifiers.logo = true;
            modifiers.shift = true;

            let action = process_keyboard_shortcut(
                &binds,
                &modifiers,
                &[Keysym::from(keysyms::KEY_3)],
                Keysym::numbersign,
            );
            assert_eq!(action, Some(KeyAction::MoveWindowToWorkspace(2)));

            let action =
                process_keyboard_shortcut(&binds, &modifiers, &[Keysym::Tab], Keysym::ISO_Left_Tab);
            assert_eq!(action, Some(KeyAction::PrevWorkspace));
        }

        #[test]
        fn vt_switch_works_even_with_no_keybinds_configured() {
            let action = process_keyboard_shortcut(
                &[],
                &no_modifiers(),
                &[],
                Keysym::from(keysyms::KEY_XF86Switch_VT_3),
            );

            assert_eq!(action, Some(KeyAction::VtSwitch(3)));
        }

        #[test]
        fn super_tab_switches_to_next_or_prev_workspace() {
            let binds = default_binds();
            let mut modifiers = no_modifiers();
            modifiers.logo = true;

            let action = process_keyboard_shortcut(&binds, &modifiers, &[], Keysym::Tab);
            assert_eq!(action, Some(KeyAction::NextWorkspace));

            modifiers.shift = true;
            let action = process_keyboard_shortcut(&binds, &modifiers, &[], Keysym::Tab);
            assert_eq!(action, Some(KeyAction::PrevWorkspace));
        }
    }

    mod overview_active_shortcut_tests {
        use super::*;

        fn default_binds() -> Vec<ResolvedKeybind> {
            crate::config::Config::with_defaults().resolve_keybinds()
        }

        #[test]
        fn overviews_own_navigation_keys_still_work() {
            let action = overview_active_shortcut(&[], &no_modifiers(), &[], Keysym::Escape);
            assert_eq!(action, KeyAction::OverviewCancel);
        }

        #[test]
        fn vt_switch_still_works_during_overview() {
            let action = overview_active_shortcut(
                &[],
                &no_modifiers(),
                &[],
                Keysym::from(keysyms::KEY_XF86Switch_VT_4),
            );
            assert_eq!(action, KeyAction::VtSwitch(4));
        }

        #[test]
        fn the_configured_screenshot_keybind_still_fires_during_overview() {
            let binds = default_binds();
            let action = overview_active_shortcut(&binds, &no_modifiers(), &[], Keysym::Print);
            assert_eq!(action, KeyAction::Screenshot);
        }

        #[test]
        fn an_ordinary_configured_keybind_does_not_leak_through_during_overview() {
            let binds = default_binds();
            let mut modifiers = no_modifiers();
            modifiers.logo = true;

            let action = overview_active_shortcut(&binds, &modifiers, &[], Keysym::q);
            assert_eq!(action, KeyAction::None);
        }

        #[test]
        fn plain_space_selects_the_hovered_preview() {
            let action = overview_active_shortcut(&[], &no_modifiers(), &[], Keysym::space);
            assert_eq!(action, KeyAction::OverviewSelect);
        }

        /// The default layout switch is Super+Space. Inside the overview it
        /// must only change the keyboard layout, not also pick a window.
        #[test]
        fn super_space_does_not_select_a_preview() {
            let mut modifiers = no_modifiers();
            modifiers.logo = true;

            let action = overview_active_shortcut(&[], &modifiers, &[], Keysym::space);
            assert_eq!(action, KeyAction::None);
        }

        #[test]
        fn an_unrelated_key_resolves_to_a_no_op_not_a_missing_action() {
            let action = overview_active_shortcut(&[], &no_modifiers(), &[], Keysym::a);
            assert_eq!(action, KeyAction::None);
        }
    }

    mod keycode_suppression {
        use smithay::backend::input::Keycode;

        #[test]
        fn suppressed_keys_tracking_and_release() {
            let mut suppressed_keys: Vec<Keycode> = Vec::new();
            let keycode1 = Keycode::from(16_u32);
            let keycode2 = Keycode::from(24_u32);

            suppressed_keys.push(keycode1);
            suppressed_keys.push(keycode2);

            assert!(suppressed_keys.contains(&keycode1));
            assert!(suppressed_keys.contains(&keycode2));

            suppressed_keys.retain(|k| *k != keycode1);
            assert!(!suppressed_keys.contains(&keycode1));
            assert!(suppressed_keys.contains(&keycode2));

            suppressed_keys.retain(|k| *k != keycode2);
            assert!(suppressed_keys.is_empty());
        }

        #[test]
        fn keycode_suppression_resets_on_workspace_switch_or_focus_change() {
            let mut suppressed_keys: Vec<Keycode> =
                vec![Keycode::from(10_u32), Keycode::from(20_u32)];
            assert!(!suppressed_keys.is_empty());

            suppressed_keys.clear();
            assert!(
                suppressed_keys.is_empty(),
                "suppressed_keys must be empty after reset"
            );
        }
    }
}

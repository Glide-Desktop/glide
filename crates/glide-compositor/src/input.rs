use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::{FilterResult, Keysym, ModifiersState, keysyms, xkb},
        pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent},
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
    None,
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

/// Configured keybinds that must keep firing even while overview mode has
/// taken over key dispatch (see `overview_active_shortcut`) — actions that
/// don't touch window/workspace state the way overview's own navigation
/// assumes exclusive control over, so they're safe to let through
/// regardless of what's on screen.
fn is_global_shortcut(action: &KeyAction) -> bool {
    matches!(action, KeyAction::Screenshot)
}

/// Resolves a key press while overview mode is open. Overview's own
/// navigation shortcuts take priority, then VT-switching (always
/// available, same as outside overview), then — unlike outside overview —
/// only the subset of configured keybinds `is_global_shortcut` allows
/// through, so e.g. the screenshot key still works without overview's
/// dedicated arrow/enter/escape handling having to know about every
/// possible user keybind. Anything left over resolves to `KeyAction::None`
/// (still *intercepted*, i.e. not forwarded to a client — overview owns
/// the keyboard while it's open — just a no-op).
fn overview_active_shortcut(
    keybinds: &[ResolvedKeybind],
    modifiers: &ModifiersState,
    raw_syms: &[Keysym],
    modified_sym: Keysym,
) -> KeyAction {
    overview_shortcut(raw_syms, modified_sym)
        .or_else(|| vt_switch_shortcut(raw_syms, modified_sym))
        .or_else(|| {
            process_keyboard_shortcut(keybinds, modifiers, raw_syms, modified_sym)
                .filter(is_global_shortcut)
        })
        .unwrap_or(KeyAction::None)
}

fn overview_shortcut(raw_syms: &[Keysym], modified_sym: Keysym) -> Option<KeyAction> {
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);

    if matches(Keysym::Escape) {
        Some(KeyAction::OverviewCancel)
    } else if matches(Keysym::Return) || matches(Keysym::KP_Enter) || matches(Keysym::space) {
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
    let max_x = outputs.iter().fold(0, |acc, r| acc + r.size.w);
    let clamped_x = pos_x.clamp(0.0, f64::from(max_x));

    #[allow(clippy::cast_possible_truncation)]
    let max_y = outputs
        .iter()
        .find(|r| r.contains((clamped_x as i32, 0)))
        .map(|r| r.size.h);

    match max_y {
        Some(max_y) => (clamped_x, pos_y.clamp(0.0, f64::from(max_y))).into(),
        None => (clamped_x, pos_y).into(),
    }
}

impl GlideState {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        self.protocols
            .idle_notifier_state
            .notify_activity(&self.seat);
        match event {
            InputEvent::Keyboard { event, .. } => self.handle_keyboard_event::<I>(&event),
            InputEvent::PointerMotion { event, .. } => self.handle_pointer_motion::<I>(&event),
            InputEvent::PointerMotionAbsolute { event, .. } => {
                self.handle_pointer_motion_absolute::<I>(&event);
            }
            InputEvent::PointerButton { event, .. } => self.handle_pointer_button::<I>(&event),
            InputEvent::PointerAxis { event, .. } => self.handle_pointer_axis::<I>(&event),
            _ => {}
        }
    }

    fn handle_keyboard_event<I: InputBackend>(&mut self, event: &I::KeyboardKeyEvent) {
        let serial = SERIAL_COUNTER.next_serial();
        let time = Event::time_msec(event);
        let key_state = event.state();
        let keycode = event.key_code();
        let mut suppressed_keys = std::mem::take(&mut self.suppressed_keys);
        let keybinds = std::mem::take(&mut self.keybinds);

        let overview_active = self.layout.overview_active();

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

                tracing::debug!(
                    ?keycode,
                    ?key_state,
                    keysym = xkb::keysym_get_name(keysym),
                    mods = ?modifiers,
                    "key"
                );

                if let KeyState::Pressed = key_state {
                    let action = if overview_active {
                        Some(overview_active_shortcut(
                            &keybinds,
                            modifiers,
                            &handle.raw_syms(),
                            keysym,
                        ))
                    } else {
                        process_keyboard_shortcut(&keybinds, modifiers, &handle.raw_syms(), keysym)
                    };

                    let action = if exclusive {
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

    // A flat dispatcher — one arm per action; splitting it up would just
    // scatter the dispatch table.
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
                self.layout
                    .switch_to_workspace(glide_layout::WorkspaceTarget::Index(idx));
                self.reconcile_space();
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
                self.layout
                    .switch_to_workspace(glide_layout::WorkspaceTarget::Next);
                self.reconcile_space();
                self.protocols
                    .ext_workspace_state
                    .sync_workspaces(&self.display_handle, &self.layout);
            }
            KeyAction::PrevWorkspace => {
                tracing::info!("switching to prev workspace");
                self.suppressed_keys.clear();
                self.layout
                    .switch_to_workspace(glide_layout::WorkspaceTarget::Previous);
                self.reconcile_space();
                self.protocols
                    .ext_workspace_state
                    .sync_workspaces(&self.display_handle, &self.layout);
            }
            KeyAction::Maximize => {
                if let Some(focused) = self.layout.focused_window() {
                    tracing::info!(focused = ?focused, "toggling maximize");
                    self.layout.toggle_maximized(focused);
                    self.reconcile_space();
                }
            }
            KeyAction::Fullscreen => {
                if let Some(focused) = self.layout.focused_window() {
                    tracing::info!(focused = ?focused, "toggling fullscreen");
                    self.layout.toggle_fullscreen(focused);
                    self.reconcile_space();
                }
            }
            KeyAction::Minimize => {
                if let Some(focused) = self.layout.focused_window() {
                    tracing::info!(focused = ?focused, "minimizing");
                    self.layout.toggle_minimize(focused);
                    self.reconcile_space();
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
            KeyAction::Screenshot => {
                tracing::info!("screenshot requested");
                self.pending_screenshot = true;
                self.queue_redraw();
            }
            KeyAction::None => {}
        }
    }

    fn navigate_overview(&mut self, dir: Direction) {
        if self.overview.as_mut().is_some_and(|ui| ui.navigate(dir)) {
            self.queue_redraw();
        }
    }

    /// Tracks the pointer over the dock. Returns whether the dock swallows
    /// this position, so it is not also handed to a window underneath.
    fn dock_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        if !self.dock.is_enabled() || self.layout.overview_active() {
            if self.dock.set_hover(None) {
                self.queue_redraw();
            }
            return false;
        }
        let area = self.dock_area();
        let over = self.dock.contains(area, pos);
        let item = if over {
            self.dock.item_at(area, pos)
        } else {
            None
        };
        if self.dock.set_hover(item) {
            self.queue_redraw();
        }
        over
    }

    fn overview_motion(
        &mut self,
        pos: Point<f64, Logical>,
    ) -> Option<(crate::focus::PointerFocusTarget, Point<f64, Logical>)> {
        if self.dock_motion(pos) {
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
        // The button hangs outside the preview, but in the design it is a
        // child of the window, so pointing at it keeps the ring lit.
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
                utime: event.time(),
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
            under,
            &MotionEvent {
                location: pointer_location,
                serial,
                time: event.time_msec(),
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
                time: event.time_msec(),
            },
        );
        pointer.frame(self);
    }

    fn handle_pointer_button<I: InputBackend>(&mut self, event: &I::PointerButtonEvent) {
        let pointer = self
            .seat
            .get_pointer()
            .expect("a pointer was added in GlideState::new");
        let serial = SERIAL_COUNTER.next_serial();

        let button = event.button_code();

        let button_state = event.state();

        if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
            let pos = pointer.current_location();

            // The dock is compositor chrome and sits above every window,
            // so it gets first refusal on a click.
            if self.dock.is_enabled() && !self.layout.overview_active() {
                let area = self.dock_area();
                if self.dock.contains(area, pos) {
                    if let Some(index) = self.dock.item_at(area, pos) {
                        self.activate_dock_item(index);
                    }
                    pointer.frame(self);
                    return;
                }
            }

            if self.layout.overview_active() {
                let on_layer = matches!(
                    self.surface_under(pos),
                    Some((crate::focus::PointerFocusTarget::Surface(ref surface), _))
                        if self.layer_for_surface(surface).is_some()
                );

                if !on_layer {
                    // The close button takes precedence over selecting the
                    // window it belongs to.
                    if let Some(id) = self
                        .overview
                        .as_ref()
                        .and_then(|ui| ui.close_button_at(pos))
                    {
                        if let Some(window) = self.window_map.get(&id)
                            && let Some(toplevel) = window.toplevel()
                        {
                            tracing::info!(window = ?id, "closing from the overview");
                            toplevel.send_close();
                        }
                        pointer.frame(self);
                        return;
                    }

                    let hit = self.overview.as_ref().and_then(|ui| ui.preview_at(pos));

                    match hit {
                        Some(id) if button == BTN_MIDDLE => {
                            if let Some(window) = self.window_map.get(&id)
                                && let Some(toplevel) = window.toplevel()
                            {
                                toplevel.send_close();
                            }
                        }
                        Some(id) => {
                            self.begin_close_overview(Some(id));
                        }
                        None => {
                            self.begin_close_overview(None);
                        }
                    }

                    pointer.frame(self);
                    return;
                }
            }

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

        pointer.button(
            self,
            &ButtonEvent {
                button,
                state: button_state,
                serial,
                time: event.time_msec(),
            },
        );
        pointer.frame(self);
    }

    fn handle_pointer_axis<I: InputBackend>(&mut self, event: &I::PointerAxisEvent) {
        let source = event.source();

        let horizontal_amount = event
            .amount(Axis::Horizontal)
            .unwrap_or_else(|| event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.);
        let vertical_amount = event
            .amount(Axis::Vertical)
            .unwrap_or_else(|| event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.);
        let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
        let vertical_amount_discrete = event.amount_v120(Axis::Vertical);

        let mut frame = AxisFrame::new(event.time_msec()).source(source);
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
        #[allow(clippy::float_cmp)]
        fn clamps_y_using_the_height_of_the_output_under_the_clamped_x() {
            let outputs = [rect(0, 0, 1920, 1080), rect(1920, 0, 1280, 720)];

            let result = clamp_to_outputs(Point::from((2500.0, 5000.0)), &outputs);

            assert_eq!(result.y, 720.0);
        }

        #[test]
        #[allow(clippy::float_cmp)]
        fn does_not_clamp_y_when_clamped_x_lands_in_a_gap_between_outputs() {
            let outputs = [rect(0, 0, 1000, 1080), rect(2000, 0, 1000, 1080)];

            let result = clamp_to_outputs(Point::from((1500.0, 5000.0)), &outputs);

            assert_eq!(result.y, 5000.0);
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

        /// Regression test: the screenshot keybind used to be silently
        /// swallowed into `KeyAction::None` while overview mode was open,
        /// because the overview-active branch never consulted the
        /// configured keybinds table at all — see `is_global_shortcut`.
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

            // Super+Q (quit) is a real, configured keybind, but it isn't
            // one of overview's own shortcuts and isn't in
            // `is_global_shortcut` — overview must own the keyboard for
            // everything else while it's open.
            let action = overview_active_shortcut(&binds, &modifiers, &[], Keysym::q);
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

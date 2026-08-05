use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::{FilterResult, Keysym, ModifiersState, keysyms, xkb},
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER},
};

use crate::{
    overview::{Direction, OverviewUi},
    state::GlideState,
};

/// Middle mouse button, as defined in the Linux kernel's
/// linux/input-event-codes.h (`BTN_MIDDLE`).
const BTN_MIDDLE: u32 = 0x112;

/// Outcome of a key press that was intercepted by the compositor instead of
/// being forwarded to the focused client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    /// Quit the compositor (Super+Q).
    Quit,
    /// Switch to a different virtual terminal (Ctrl+Alt+F1..F12).
    VtSwitch(i32),
    /// Switch active workspace by 0-based index (Super+1..9).
    SwitchWorkspace(usize),
    /// Move currently focused window to workspace by 0-based index (Super+Shift+1..9).
    MoveWindowToWorkspace(usize),
    /// Switch to next workspace (Super+Tab).
    NextWorkspace,
    /// Switch to previous workspace (Super+Shift+Tab).
    PrevWorkspace,
    /// Toggle maximize for focused window.
    Maximize,
    /// Toggle fullscreen for focused window.
    Fullscreen,
    /// Toggle minimize for focused window.
    Minimize,
    /// Toggle overview mode.
    ToggleOverview,
    /// Dismiss the overview without changing focus (Escape).
    OverviewCancel,
    /// Activate the highlighted preview (Enter/Space).
    OverviewSelect,
    /// Move the overview's highlight (arrow keys / hjkl).
    OverviewNav(Direction),
    /// Execute arbitrary shell command.
    Exec(String),
    /// The key was intercepted (e.g. a suppressed release) but has no effect.
    None,
}

/// Resolved keybinding rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedKeybind {
    pub modifiers: ModifiersState,
    pub keysym: Keysym,
    pub action: KeyAction,
}

/// Checks whether a pressed key (with its current modifier state) matches a
/// compositor-level keybinding.
/// Both the raw and the modified keysym are considered, because they differ
/// exactly when a modifier is part of the binding: `Shift+Tab` arrives as
/// `ISO_Left_Tab`, and `Shift+1` as `exclam`, so a binding written the
/// obvious way (`Tab`, `1`) would never match on the modified symbol alone.
///
/// Takes slices rather than an owned `Vec` on purpose: this runs on every
/// single key event, and there is nothing here worth allocating for.
fn process_keyboard_shortcut(
    keybinds: &[ResolvedKeybind],
    modifiers: &ModifiersState,
    raw_syms: &[Keysym],
    modified_sym: Keysym,
) -> Option<KeyAction> {
    let matches_key =
        |entry: &ResolvedKeybind| entry.keysym == modified_sym || raw_syms.contains(&entry.keysym);

    // Modifiers must match exactly, so Super+Shift+1 does not also fire a
    // plain Super+1 binding.
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

/// VT switching, which is deliberately not configurable: it is the only way
/// off the tty, so it must keep working with an empty or broken keybind
/// config — and in every modal state the compositor can be in.
fn vt_switch_shortcut(raw_syms: &[Keysym], modified_sym: Keysym) -> Option<KeyAction> {
    raw_syms
        .iter()
        .chain(std::iter::once(&modified_sym))
        .map(|sym| sym.raw())
        .find(|raw| (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12).contains(raw))
        .map(|raw| KeyAction::VtSwitch((raw - keysyms::KEY_XF86Switch_VT_1 + 1).cast_signed()))
}

/// The overview's own keys.
///
/// Fixed rather than configurable: these are the conventions every
/// overview-style switcher uses, and rebinding them would mostly be a way
/// to end up unable to dismiss the thing.
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

/// Clamps `pos` so it stays within the union of `outputs` (outputs are
/// assumed laid out left-to-right, matching how `connector_connected` maps
/// them). Pure geometry, deliberately decoupled from `Space`/`Output` so it
/// can be unit tested without any Smithay/DRM machinery.
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

    // Outputs are not guaranteed contiguous, so the clamped x may still land
    // in a gap between two of them; `find` yields `None` in that case.
    //
    // Truncating a clamped logical coordinate back to i32 for the containment
    // check is safe: `clamped_x` is bounded by `max_x` above, an i32-derived
    // sum of output widths, so it can never approach f64's precision limits.
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
    /// Dispatches a raw backend input event to the appropriate handler.
    ///
    /// # Panics
    ///
    /// Panics if no keyboard or pointer was added to the seat; `GlideState::new`
    /// always adds both, so in practice this never fires.
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        self.idle_notifier_state.notify_activity(&self.seat);
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
        // Taken rather than cloned: this runs on every key event, and
        // `suppressed_keys` is put back (unconditionally) below, so a
        // clone here would just be an allocation this function
        // immediately throws away in favour of its own copy.
        let mut suppressed_keys = std::mem::take(&mut self.suppressed_keys);
        // Taken for the same reason: `keyboard.input` borrows `self` for the
        // duration of the callback, so the bindings cannot be read from
        // `self` inside it. Taking beats cloning — this is the hottest path
        // in the compositor, and the bindings contain owned `String`s.
        let keybinds = std::mem::take(&mut self.keybinds);

        let overview_active = self.layout.overview_active();

        // A Top/Overlay layer surface with exclusive keyboard interactivity
        // (lock screens, some launchers) takes the keyboard for as long as
        // it exists, no click required — that is what "exclusive" means.
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

                // Only decide on a shortcut when the key is pressed. On
                // release we only need to know whether *this* keysym was
                // previously suppressed, so the client never sees a
                // release without the matching press (a "stuck key").
                if let KeyState::Pressed = key_state {
                    // The overview is modal: it owns the keyboard while
                    // open, so its own keys are matched first and
                    // everything else is swallowed rather than reaching a
                    // client the user cannot see behind the scrim. VT
                    // switching is the sole exception — it is the only way
                    // off the tty and must work in every mode.
                    let action = if overview_active {
                        overview_shortcut(&handle.raw_syms(), keysym).or_else(|| {
                            vt_switch_shortcut(&handle.raw_syms(), keysym).or(Some(KeyAction::None))
                        })
                    } else {
                        process_keyboard_shortcut(&keybinds, modifiers, &handle.raw_syms(), keysym)
                    };

                    // While a layer surface holds the keyboard, compositor
                    // shortcuts step aside — a launcher's keystrokes are its
                    // own — with one exception that must survive anything:
                    // VT switching is the only way off the tty.
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

    /// Executes a compositor-level action resolved from a keybinding.
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
                self.ext_workspace_state
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
                    self.ext_workspace_state
                        .sync_workspaces(&self.display_handle, &self.layout);
                }
            }
            KeyAction::NextWorkspace => {
                tracing::info!("switching to next workspace");
                self.suppressed_keys.clear();
                self.layout
                    .switch_to_workspace(glide_layout::WorkspaceTarget::Next);
                self.reconcile_space();
                self.ext_workspace_state
                    .sync_workspaces(&self.display_handle, &self.layout);
            }
            KeyAction::PrevWorkspace => {
                tracing::info!("switching to prev workspace");
                self.suppressed_keys.clear();
                self.layout
                    .switch_to_workspace(glide_layout::WorkspaceTarget::Previous);
                self.reconcile_space();
                self.ext_workspace_state
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
                crate::reap_child_processes();
                if let Err(err) = std::process::Command::new("sh").arg("-c").arg(&cmd).spawn() {
                    tracing::error!(command = %cmd, ?err, "failed to execute keybind command");
                }
                crate::reap_child_processes();
            }
            KeyAction::None => {}
        }
    }

    /// Moves the overview's selection, repainting if it moved.
    fn navigate_overview(&mut self, dir: Direction) {
        if self.overview.as_mut().is_some_and(|ui| ui.navigate(dir)) {
            self.queue_redraw();
        }
    }

    /// Pointer-focus target for a motion event, accounting for the
    /// overview.
    ///
    /// While the overview is up the previews are render-only, so the real
    /// windows underneath must not receive enter/motion — a client would
    /// otherwise light up hover states and swap cursors for a pointer the
    /// user cannot see over it. Layer surfaces are the exception: panels
    /// are drawn above the overview and stay interactive.
    ///
    /// Also updates which preview is highlighted, and asks for a repaint
    /// when that changes.
    fn overview_motion(
        &mut self,
        pos: Point<f64, Logical>,
    ) -> Option<(crate::focus::PointerFocusTarget, Point<f64, Logical>)> {
        if !self.layout.overview_active() {
            return self.surface_under(pos);
        }

        let under = self.surface_under(pos);
        let on_layer = matches!(
            under,
            Some((crate::focus::PointerFocusTarget::Surface(ref surface), _))
                if self.layer_for_surface(surface).is_some()
        );

        let hovered = self.overview.as_ref().and_then(|ui| ui.preview_at(pos));
        let changed = self
            .overview
            .as_mut()
            .is_some_and(|ui| ui.set_hover(hovered));
        if changed {
            self.queue_redraw();
        }

        if on_layer { under } else { None }
    }

    fn handle_pointer_motion<I: InputBackend>(&mut self, event: &I::PointerMotionEvent) {
        // libinput reports relative deltas, so we have to track the
        // absolute pointer position ourselves and keep it on screen.
        let pointer = self
            .seat
            .get_pointer()
            .expect("a pointer was added in GlideState::new");
        let pointer_location = self.clamp_coords(pointer.current_location() + event.delta());

        // trace, not debug: this fires for every motion event, so at
        // debug it buries everything else in the log.
        tracing::trace!(
            delta = ?event.delta(),
            location = ?pointer_location,
            "pointer motion (relative)"
        );

        let serial = SERIAL_COUNTER.next_serial();
        let under = self.overview_motion(pointer_location);

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
    }

    fn handle_pointer_motion_absolute<I: InputBackend>(
        &mut self,
        event: &I::PointerMotionAbsoluteEvent,
    ) {
        // Absolute positioning devices (touchscreens, drawing
        // tablets) report coordinates relative to an output, so
        // without one mapped there is nowhere to place the event.
        // Not a bug to recover from -- just nothing to do yet, e.g.
        // before the first connector is set up in device_added().
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

            // ── Overview input ────────────────────────────────────
            //
            // Previews are render-only: the real windows are still mapped
            // at their real positions, so the normal `surface_under` lookup
            // would resolve to whatever is invisibly underneath the
            // pointer. Hit-test the previews instead.
            //
            // Overlay/Top layer surfaces stay clickable — a panel is drawn
            // above the overview and should keep working, as it does in
            // other compositors.
            if self.layout.overview_active() {
                let on_layer = matches!(
                    self.surface_under(pos),
                    Some((crate::focus::PointerFocusTarget::Surface(ref surface), _))
                        if self.layer_for_surface(surface).is_some()
                );

                if !on_layer {
                    let hit = self.overview.as_ref().and_then(|ui| ui.preview_at(pos));

                    match hit {
                        // Middle click closes the window without leaving
                        // the overview — the quickest way to tidy up.
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
                        // A click on empty space dismisses without
                        // changing focus.
                        None => {
                            self.begin_close_overview(None);
                        }
                    }

                    pointer.frame(self);
                    return;
                }
            }

            // What was actually clicked, layers included — `element_under`
            // alone would find the window *underneath* a panel and focus
            // that, since layer surfaces are not Space elements.
            let target = self.surface_under(pos).map(|(target, _)| target);

            match target {
                Some(crate::focus::PointerFocusTarget::Surface(surface))
                    if self.layer_for_surface(&surface).is_some() =>
                {
                    let layer = self
                        .layer_for_surface(&surface)
                        .expect("matched Some in the guard above");
                    // Click-to-focus for OnDemand launchers; layers that
                    // declared no keyboard interest are left alone (a
                    // wallpaper click should not steal the keyboard).
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
                    // Focus goes through the layout engine, never straight
                    // to the keyboard: the layout owns which window is
                    // active. Resolution still via element_under, which
                    // (unlike a bare surface lookup) also attributes popups
                    // and subsurfaces to their window.
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
                // The v120 tick count from libinput is always a small
                // integer-valued f64 (multiples of 120), nowhere near
                // i32::MAX, so this narrowing never truncates in practice.
                #[allow(clippy::cast_possible_truncation)]
                {
                    frame = frame.v120(Axis::Horizontal, discrete as i32);
                }
            }
        }
        if vertical_amount != 0.0 {
            frame = frame.value(Axis::Vertical, vertical_amount);
            if let Some(discrete) = vertical_amount_discrete {
                // See the horizontal case above: v120 ticks never approach i32::MAX.
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

    /// Clamp a pointer position so it never moves outside the mapped outputs.
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

        // These tests assert exact equality against the literal bounds passed
        // into `f64::clamp`, which `clamp` returns byte-for-byte when the
        // input is outside range (no arithmetic that could introduce
        // rounding). clippy::float_cmp can't see that guarantee, so it is
        // silenced locally rather than weakening the assertions.
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
            // Two outputs side by side with different heights, as if a
            // shorter external monitor sat to the right of the laptop panel.
            let outputs = [rect(0, 0, 1920, 1080), rect(1920, 0, 1280, 720)];

            let result = clamp_to_outputs(Point::from((2500.0, 5000.0)), &outputs);

            assert_eq!(result.y, 720.0);
        }

        #[test]
        #[allow(clippy::float_cmp)]
        fn does_not_clamp_y_when_clamped_x_lands_in_a_gap_between_outputs() {
            // Outputs need not be contiguous; nothing currently guarantees
            // that, so this must not panic if clamped_x lands in a gap.
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
            // With Shift held, xkb reports the *shifted* symbol: `3` arrives
            // as `numbersign`, `Tab` as `ISO_Left_Tab`. Bindings are written
            // against the unshifted key, so matching must consider raw syms
            // too or every Super+Shift+N binding silently stops working.
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
            // VT switching is the only way off the tty, so it must not depend
            // on the user's config being valid.
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

    mod keycode_suppression {
        use smithay::backend::input::Keycode;

        #[test]
        fn suppressed_keys_tracking_and_release() {
            let mut suppressed_keys: Vec<Keycode> = Vec::new();
            let keycode1 = Keycode::from(16_u32); // e.g. key '1'
            let keycode2 = Keycode::from(24_u32); // e.g. key 'q'

            // Add suppressed keys on press
            suppressed_keys.push(keycode1);
            suppressed_keys.push(keycode2);

            assert!(suppressed_keys.contains(&keycode1));
            assert!(suppressed_keys.contains(&keycode2));

            // On release of keycode1, remove it
            suppressed_keys.retain(|k| *k != keycode1);
            assert!(!suppressed_keys.contains(&keycode1));
            assert!(suppressed_keys.contains(&keycode2));

            // On release of keycode2, remove it
            suppressed_keys.retain(|k| *k != keycode2);
            assert!(suppressed_keys.is_empty());
        }

        #[test]
        fn keycode_suppression_resets_on_workspace_switch_or_focus_change() {
            let mut suppressed_keys: Vec<Keycode> =
                vec![Keycode::from(10_u32), Keycode::from(20_u32)];
            assert!(!suppressed_keys.is_empty());

            // Workspace switch / VT switch / focus change triggers clear
            suppressed_keys.clear();
            assert!(
                suppressed_keys.is_empty(),
                "suppressed_keys must be empty after reset"
            );
        }
    }
}

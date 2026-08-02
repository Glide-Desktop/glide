use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::{FilterResult, Keysym, ModifiersState, keysyms, xkb},
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER},
};

use crate::state::{GlideState, window_surface};

/// Outcome of a key press that was intercepted by the compositor instead of
/// being forwarded to the focused client.
#[derive(Debug, PartialEq, Eq)]
enum KeyAction {
    /// Quit the compositor (Super+Q).
    Quit,
    /// Switch to a different virtual terminal (Ctrl+Alt+F1..F12).
    VtSwitch(i32),
    /// The key was intercepted (e.g. a suppressed release) but has no effect.
    None,
}

/// Checks whether a pressed key (with its current modifier state) matches a
/// compositor-level keybinding.
fn process_keyboard_shortcut(modifiers: &ModifiersState, keysym: Keysym) -> Option<KeyAction> {
    if modifiers.logo && keysym == Keysym::q {
        Some(KeyAction::Quit)
    } else if (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12).contains(&keysym.raw())
    {
        Some(KeyAction::VtSwitch(
            (keysym.raw() - keysyms::KEY_XF86Switch_VT_1 + 1).cast_signed(),
        ))
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
                    let action = process_keyboard_shortcut(modifiers, keysym);
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

        match action {
            Some(KeyAction::Quit) => self.loop_signal.stop(),
            Some(KeyAction::VtSwitch(vt)) => self.backend_data.change_vt(vt),
            Some(KeyAction::None) | None => {}
        }
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
        let under = self.surface_under(pointer_location);

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

        let under = self.surface_under(pos);

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
        let keyboard = self
            .seat
            .get_keyboard()
            .expect("a keyboard was added in GlideState::new");

        let serial = SERIAL_COUNTER.next_serial();

        let button = event.button_code();

        let button_state = event.state();

        if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
            if let Some((window, _loc)) = self
                .space
                .element_under(pointer.current_location())
                .map(|(w, l)| (w.clone(), l))
            {
                self.space.raise_element(&window, true);
                keyboard.set_focus(self, Some(window_surface(&window).clone()), serial);
                self.space.elements().for_each(|window| {
                    window
                        .toplevel()
                        .expect(
                            "only Window::new_wayland_window is used, so toplevel() is always Some",
                        )
                        .send_pending_configure();
                });
            } else {
                self.space.elements().for_each(|window| {
                    window.set_activated(false);
                    window
                        .toplevel()
                        .expect(
                            "only Window::new_wayland_window is used, so toplevel() is always Some",
                        )
                        .send_pending_configure();
                });
                keyboard.set_focus(self, Option::<WlSurface>::None, serial);
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

        #[test]
        fn super_q_quits() {
            let mut modifiers = no_modifiers();
            modifiers.logo = true;

            let action = process_keyboard_shortcut(&modifiers, Keysym::q);

            assert_eq!(action, Some(KeyAction::Quit));
        }

        #[test]
        fn q_alone_without_super_does_not_quit() {
            let action = process_keyboard_shortcut(&no_modifiers(), Keysym::q);

            assert_eq!(action, None);
        }

        #[test]
        fn xf86_switch_vt_2_switches_to_vt_2() {
            let keysym = Keysym::from(keysyms::KEY_XF86Switch_VT_2);

            let action = process_keyboard_shortcut(&no_modifiers(), keysym);

            assert_eq!(action, Some(KeyAction::VtSwitch(2)));
        }

        #[test]
        fn xf86_switch_vt_12_switches_to_vt_12() {
            let keysym = Keysym::from(keysyms::KEY_XF86Switch_VT_12);

            let action = process_keyboard_shortcut(&no_modifiers(), keysym);

            assert_eq!(action, Some(KeyAction::VtSwitch(12)));
        }

        #[test]
        fn unrelated_key_has_no_shortcut() {
            let action = process_keyboard_shortcut(&no_modifiers(), Keysym::a);

            assert_eq!(action, None);
        }
    }
}

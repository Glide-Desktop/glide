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
    utils::{Logical, Point, SERIAL_COUNTER},
};

use crate::state::{Backend, GlideState};

/// Outcome of a key press that was intercepted by the compositor instead of
/// being forwarded to the focused client.
#[derive(Debug)]
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
            (keysym.raw() - keysyms::KEY_XF86Switch_VT_1 + 1) as i32,
        ))
    } else {
        None
    }
}

impl<BackendData: Backend + 'static> GlideState<BackendData> {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);
                let key_state = event.state();
                let keycode = event.key_code();
                let mut suppressed_keys = self.suppressed_keys.clone();

                let keyboard = self.seat.get_keyboard().unwrap();
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
                            action
                                .map(FilterResult::Intercept)
                                .unwrap_or(FilterResult::Forward)
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
            InputEvent::PointerMotion { event, .. } => {
                // libinput reports relative deltas, so we have to track the
                // absolute pointer position ourselves and keep it on screen.
                let pointer = self.seat.get_pointer().unwrap();
                let pointer_location =
                    self.clamp_coords(pointer.current_location() + event.delta());

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
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = self.space.outputs().next().unwrap();

                let output_geo = self.space.output_geometry(output).unwrap();

                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.seat.get_pointer().unwrap();

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
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let keyboard = self.seat.get_keyboard().unwrap();

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
                        keyboard.set_focus(
                            self,
                            Some(window.toplevel().unwrap().wl_surface().clone()),
                            serial,
                        );
                        self.space.elements().for_each(|window| {
                            window.toplevel().unwrap().send_pending_configure();
                        });
                    } else {
                        self.space.elements().for_each(|window| {
                            window.set_activated(false);
                            window.toplevel().unwrap().send_pending_configure();
                        });
                        keyboard.set_focus(self, Option::<WlSurface>::None, serial);
                    }
                };

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
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();

                let horizontal_amount = event.amount(Axis::Horizontal).unwrap_or_else(|| {
                    event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.
                });
                let vertical_amount = event.amount(Axis::Vertical).unwrap_or_else(|| {
                    event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.
                });
                let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_v120(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time_msec()).source(source);
                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.v120(Axis::Horizontal, discrete as i32);
                    }
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.v120(Axis::Vertical, discrete as i32);
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

                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }

    /// Clamp a pointer position so it never moves outside the mapped outputs.
    fn clamp_coords(&self, pos: Point<f64, Logical>) -> Point<f64, Logical> {
        if self.space.outputs().next().is_none() {
            return pos;
        }

        let (pos_x, pos_y) = pos.into();
        let max_x = self.space.outputs().fold(0, |acc, o| {
            acc + self.space.output_geometry(o).unwrap().size.w
        });
        let clamped_x = pos_x.clamp(0.0, max_x as f64);
        let max_y = self
            .space
            .outputs()
            .find(|o| {
                let geo = self.space.output_geometry(o).unwrap();
                geo.contains((clamped_x as i32, 0))
            })
            .map(|o| self.space.output_geometry(o).unwrap().size.h);

        if let Some(max_y) = max_y {
            let clamped_y = pos_y.clamp(0.0, max_y as f64);
            (clamped_x, clamped_y).into()
        } else {
            (clamped_x, pos_y).into()
        }
    }
}

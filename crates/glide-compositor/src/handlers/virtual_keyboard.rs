use smithay::{
    backend::input::{Event, InputEvent, KeyboardKeyEvent},
    input::keyboard::{FilterResult, KeyboardSource, ModifiersState},
    utils::SERIAL_COUNTER,
    wayland::{
        input_method::InputMethodSeat,
        virtual_keyboard::{
            VirtualKeyboardBackend, VirtualKeyboardDevice, VirtualKeyboardHandler, VirtualKeyboardSpecialEvent,
        },
    },
};

use crate::state::GlideState;

#[derive(Debug, Clone, Copy)]
pub struct Lent {
    source: KeyboardSource,
    layout: usize,
    mods: ModifiersState,
}

impl VirtualKeyboardHandler for GlideState {
    fn process_virtual_keyboard_event(&mut self, event: InputEvent<VirtualKeyboardBackend>) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        match event {
            InputEvent::Keyboard { event } => {
                let device = event.device();
                self.use_keymap_of(&device);
                let serial = SERIAL_COUNTER.next_serial();
                let grab_client = self.seat.input_method().keyboard_grab_client();
                if grab_client.is_some() && grab_client == device.client() {
                    keyboard.input_forward_bypassing_grab(
                        self,
                        event.key_code(),
                        event.state(),
                        serial,
                        event.time(),
                        true,
                    );
                } else {
                    keyboard.input_from_source(
                        device.source(),
                        self,
                        event.key_code(),
                        event.state(),
                        serial,
                        event.time(),
                        |_, _, _| FilterResult::<()>::Forward,
                    );
                }
            }
            InputEvent::Special(VirtualKeyboardSpecialEvent::Modifiers {
                device,
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
            }) => {
                self.use_keymap_of(&device);
                keyboard.with_xkb_state(self, |mut context| {
                    context.set_modifier_mask(mods_depressed, mods_latched, mods_locked, group);
                });
            }
            InputEvent::Special(VirtualKeyboardSpecialEvent::KeymapChanged { device }) => {
                if self.lent_to(&device) {
                    self.restore_seat_keymap();
                }
            }
            InputEvent::DeviceRemoved { device } => {
                keyboard.release_source(self, device.source());
                if self.lent_to(&device) {
                    self.restore_seat_keymap();
                }
            }
            _ => {}
        }
    }
}

impl GlideState {
    fn lent_to(&self, device: &VirtualKeyboardDevice) -> bool {
        self.session.lent_keymap.is_some_and(|lent| lent.source == device.source())
    }

    fn use_keymap_of(&mut self, device: &VirtualKeyboardDevice) {
        let Some(keymap) = device.keymap() else {
            self.restore_seat_keymap();
            return;
        };
        if self.lent_to(device) {
            return;
        }
        self.restore_seat_keymap();
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let layout = self.active_layout_index().unwrap_or(0);
        let mods = keyboard.modifier_state();
        if let Err(err) = keyboard.set_keymap_from_string(self, keymap.to_string()) {
            tracing::warn!(?err, "a virtual keyboard sent a keymap that does not compile");
            return;
        }
        self.session.lent_keymap = Some(Lent { source: device.source(), layout, mods });
    }

    pub fn restore_seat_keymap(&mut self) {
        let Some(lent) = self.session.lent_keymap.take() else {
            return;
        };
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        if let Err(err) = keyboard.set_xkb_config(self, self.input_config.clone().xkb()) {
            tracing::error!(?err, "failed to restore the xkb config after a virtual keyboard");
            return;
        }
        crate::state::replace_modifiers(&keyboard, self, crate::state::locks_only(lent.mods), false);
        self.set_keyboard_layout(lent.layout);
    }
}

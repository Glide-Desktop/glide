use std::collections::HashMap;

use smithay::{
    backend::input::{InputTime, KeyState},
    input::keyboard::{FilterResult, Keycode, xkb},
    reexports::{
        wayland_protocols::wp::text_input::zv3::server::zwp_text_input_v3::{ContentHint, ContentPurpose},
        wayland_server::protocol::wl_surface::WlSurface,
    },
    utils::SERIAL_COUNTER,
    wayland::text_input::TextInputSeat,
};

use crate::state::GlideState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Password,
    Pin,
    Code,
    Email,
    Url,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub kind: Kind,
    pub sensitive: bool,
}

impl Field {
    #[must_use]
    pub fn from_content(content: Option<(ContentHint, ContentPurpose)>) -> Self {
        let Some((hint, purpose)) = content else {
            return Self { kind: Kind::Text, sensitive: false };
        };
        let kind = match purpose {
            ContentPurpose::Password => Kind::Password,
            ContentPurpose::Pin => Kind::Pin,
            ContentPurpose::Digits | ContentPurpose::Number => Kind::Code,
            ContentPurpose::Email => Kind::Email,
            ContentPurpose::Url => Kind::Url,
            ContentPurpose::Terminal => Kind::Terminal,
            _ => Kind::Text,
        };
        let sensitive = matches!(kind, Kind::Password | Kind::Pin)
            || hint.intersects(ContentHint::SensitiveData | ContentHint::HiddenText);
        Self { kind, sensitive }
    }

    #[must_use]
    pub const fn takes_secret(self) -> bool {
        matches!(self.kind, Kind::Password | Kind::Pin) || (self.sensitive && !matches!(self.kind, Kind::Terminal))
    }

    #[must_use]
    pub const fn takes_code(self) -> bool {
        matches!(self.kind, Kind::Code | Kind::Pin)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stroke {
    pub keycode: u32,
    pub shift: bool,
}

#[must_use]
pub fn plan(text: &str, caps_lock: bool, find: impl Fn(char) -> Option<(u32, bool)>) -> Option<Vec<Stroke>> {
    text.chars()
        .map(|ch| {
            let (keycode, shift) = find(ch)?;
            let flipped = caps_lock && ch.is_alphabetic();
            Some(Stroke { keycode, shift: shift != flipped })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Committed,
    Typed,
    Refused,
}

struct KeyTable {
    keys: HashMap<char, (u32, bool)>,
    shift: Option<u32>,
}

impl GlideState {
    #[must_use]
    pub fn focused_surface(&self) -> Option<WlSurface> {
        self.seat.get_keyboard().and_then(|keyboard| keyboard.current_focus())
    }

    pub fn deliver_text(&mut self, target: &WlSurface, text: &str) -> Delivery {
        if self.screen_locked() || self.focused_surface().as_ref() != Some(target) {
            return Delivery::Refused;
        }
        let text_input = self.seat.text_input().clone();
        let mut committed = false;
        text_input.with_active_text_input(|input, surface| {
            if surface == target {
                input.commit_string(Some(text.to_string()));
                committed = true;
            }
        });
        if committed {
            text_input.done(false);
            return Delivery::Committed;
        }
        self.type_text(text)
    }

    pub fn type_into(&mut self, target: &WlSurface, text: &str) -> Delivery {
        if self.screen_locked() || self.focused_surface().as_ref() != Some(target) {
            return Delivery::Refused;
        }
        self.type_text(text)
    }

    fn type_text(&mut self, text: &str) -> Delivery {
        self.restore_seat_keymap();
        let Some(keyboard) = self.seat.get_keyboard() else {
            return Delivery::Refused;
        };
        let mods = keyboard.modifier_state();
        if crate::state::holds_modifiers(&mods) {
            return Delivery::Refused;
        }
        let table = self.key_table();
        let Some(strokes) = plan(text, mods.caps_lock, |ch| table.keys.get(&ch).copied()) else {
            return Delivery::Refused;
        };
        if strokes.iter().any(|stroke| stroke.shift) && table.shift.is_none() {
            return Delivery::Refused;
        }
        for stroke in strokes {
            let shift = table.shift.filter(|_| stroke.shift);
            if let Some(shift) = shift {
                self.synthesize(shift, KeyState::Pressed);
            }
            self.synthesize(stroke.keycode, KeyState::Pressed);
            self.synthesize(stroke.keycode, KeyState::Released);
            if let Some(shift) = shift {
                self.synthesize(shift, KeyState::Released);
            }
        }
        Delivery::Typed
    }

    fn synthesize(&mut self, keycode: u32, state: KeyState) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        keyboard.input::<(), _>(self, Keycode::new(keycode), state, serial, InputTime::now(), |_, _, _| {
            FilterResult::Forward
        });
    }

    fn key_table(&mut self) -> KeyTable {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return KeyTable { keys: HashMap::new(), shift: None };
        };
        keyboard.with_xkb_state(self, |context| {
            let xkb = context.xkb().lock().expect("single-threaded event loop, this lock is never poisoned");
            let layout = xkb.active_layout().0;
            // SAFETY: the keymap is borrowed only inside this closure, while the Xkb lock is held, and no ref-count is taken.
            let keymap = unsafe { xkb.keymap() };
            let shift_mask = 1 << keymap.mod_get_index(xkb::MOD_NAME_SHIFT);
            let mut keys = HashMap::new();
            let mut shift = None;
            for level in 0..2 {
                keymap.key_for_each(|keymap, key| {
                    if level >= keymap.num_levels_for_key(key, layout) {
                        return;
                    }
                    let mut masks = [0; 8];
                    let count = keymap.key_get_mods_for_level(key, layout, level, &mut masks);
                    let wanted = if level == 0 { 0 } else { shift_mask };
                    if !masks[..count.min(masks.len())].contains(&wanted) {
                        return;
                    }
                    for &sym in keymap.key_get_syms_by_level(key, layout, level) {
                        if level == 0 && sym == xkb::Keysym::Shift_L {
                            shift.get_or_insert(key.raw());
                        }
                        let Some(ch) = char::from_u32(xkb::keysym_to_utf32(sym)).filter(|ch| *ch != '\0') else {
                            continue;
                        };
                        let ch = if ch == '\r' { '\n' } else { ch };
                        keys.entry(ch).or_insert((key.raw(), level == 1));
                    }
                });
            }
            KeyTable { keys, shift }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_without_a_content_type_is_plain_text() {
        assert_eq!(Field::from_content(None), Field { kind: Kind::Text, sensitive: false });
    }

    #[test]
    fn password_and_pin_purposes_take_secrets() {
        for purpose in [ContentPurpose::Password, ContentPurpose::Pin] {
            let field = Field::from_content(Some((ContentHint::None, purpose)));
            assert!(field.sensitive && field.takes_secret(), "{purpose:?}");
        }
    }

    #[test]
    fn a_hidden_text_hint_marks_a_normal_field_as_secret() {
        let field = Field::from_content(Some((ContentHint::HiddenText, ContentPurpose::Normal)));
        assert_eq!(field.kind, Kind::Text);
        assert!(field.takes_secret());
    }

    #[test]
    fn digit_fields_are_one_time_code_candidates_not_secrets() {
        let field = Field::from_content(Some((ContentHint::None, ContentPurpose::Digits)));
        assert_eq!(field.kind, Kind::Code);
        assert!(!field.takes_secret());
    }

    fn qwerty(ch: char) -> Option<(u32, bool)> {
        match ch {
            'a' | 'ф' => Some((38, false)),
            'A' => Some((38, true)),
            '1' => Some((10, false)),
            '!' => Some((10, true)),
            _ => None,
        }
    }

    #[test]
    fn a_plan_presses_shift_only_for_upper_level_characters() {
        let strokes = plan("aA1!", false, qwerty).unwrap();
        let shifts: Vec<bool> = strokes.iter().map(|stroke| stroke.shift).collect();
        assert_eq!(shifts, [false, true, false, true]);
        assert_eq!(strokes[0].keycode, strokes[1].keycode);
    }

    #[test]
    fn caps_lock_flips_shift_for_letters_only() {
        let strokes = plan("aA1!ф", true, qwerty).unwrap();
        let shifts: Vec<bool> = strokes.iter().map(|stroke| stroke.shift).collect();
        assert_eq!(shifts, [true, false, false, true, true]);
    }

    #[test]
    fn a_character_the_layout_cannot_type_refuses_the_whole_text() {
        assert_eq!(plan("a€", false, qwerty), None, "typing half a password is worse than typing none");
    }

    #[test]
    fn only_code_and_pin_fields_are_offered_one_time_codes() {
        let field = |purpose| Field::from_content(Some((ContentHint::None, purpose)));
        assert!(field(ContentPurpose::Digits).takes_code());
        assert!(field(ContentPurpose::Pin).takes_code());
        assert!(!field(ContentPurpose::Password).takes_code());
        assert!(!Field::from_content(None).takes_code());
    }

    #[test]
    fn a_terminal_never_takes_a_secret_even_when_marked_sensitive() {
        let field = Field::from_content(Some((ContentHint::SensitiveData, ContentPurpose::Terminal)));
        assert!(!field.takes_secret());
    }
}

use std::collections::HashMap;

use zbus::zvariant::{OwnedValue, Value};

use crate::model::{Sound, Urgency};

#[must_use]
pub fn string(hints: &HashMap<String, OwnedValue>, key: &str) -> String {
    hints.get(key).and_then(|value| String::try_from(value.clone()).ok()).unwrap_or_default()
}

#[must_use]
pub fn boolean(hints: &HashMap<String, OwnedValue>, key: &str) -> bool {
    hints.get(key).and_then(|value| bool::try_from(value.clone()).ok()).unwrap_or(false)
}

#[must_use]
pub fn integer(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<i64> {
    let value = hints.get(key)?;
    match &**value {
        Value::U8(inner) => Some(i64::from(*inner)),
        Value::I16(inner) => Some(i64::from(*inner)),
        Value::U16(inner) => Some(i64::from(*inner)),
        Value::I32(inner) => Some(i64::from(*inner)),
        Value::U32(inner) => Some(i64::from(*inner)),
        Value::I64(inner) => Some(*inner),
        Value::U64(inner) => i64::try_from(*inner).ok(),
        Value::F64(inner) => Some(inner.round() as i64),
        _ => None,
    }
}

#[must_use]
pub fn urgency(hints: &HashMap<String, OwnedValue>) -> Urgency {
    integer(hints, "urgency").and_then(|value| u8::try_from(value).ok()).map_or(Urgency::Normal, Urgency::from_hint)
}

#[must_use]
pub fn sound(hints: &HashMap<String, OwnedValue>) -> Sound {
    if boolean(hints, "suppress-sound") {
        return Sound::None;
    }
    let file = string(hints, "sound-file");
    let file = file.trim();
    if !file.is_empty() {
        return Sound::File(file.strip_prefix("file://").unwrap_or(file).to_string());
    }
    let name = string(hints, "sound-name");
    let name = name.trim();
    if name.is_empty() { Sound::None } else { Sound::Named(name.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint(key: &str, value: Value<'static>) -> HashMap<String, OwnedValue> {
        let mut map = HashMap::new();
        map.insert(key.to_string(), OwnedValue::try_from(value).expect("value converts"));
        map
    }

    #[test]
    fn a_sound_file_beats_a_sound_name_and_suppress_sound_beats_both() {
        assert_eq!(
            sound(&hint("sound-name", Value::from("message-new-instant"))),
            Sound::Named("message-new-instant".into())
        );
        let mut both = hint("sound-name", Value::from("bell"));
        both.extend(hint("sound-file", Value::from("file:///usr/share/sounds/ping.oga")));
        assert_eq!(sound(&both), Sound::File("/usr/share/sounds/ping.oga".into()));
        both.extend(hint("suppress-sound", Value::Bool(true)));
        assert_eq!(sound(&both), Sound::None);
        assert_eq!(sound(&hint("sound-name", Value::from("  "))), Sound::None);
        assert_eq!(sound(&HashMap::new()), Sound::None, "silent unless the app asks");
    }

    #[test]
    fn urgency_is_read_from_the_byte_the_spec_prescribes() {
        assert_eq!(urgency(&hint("urgency", Value::U8(2))), Urgency::Critical);
        assert_eq!(urgency(&hint("urgency", Value::U8(0))), Urgency::Low);
    }

    #[test]
    fn urgency_tolerates_senders_that_use_the_wrong_integer_width() {
        assert_eq!(urgency(&hint("urgency", Value::I32(2))), Urgency::Critical);
        assert_eq!(urgency(&hint("urgency", Value::U32(0))), Urgency::Low);
    }

    #[test]
    fn a_missing_urgency_hint_means_normal() {
        assert_eq!(urgency(&HashMap::new()), Urgency::Normal);
    }

    #[test]
    fn strings_and_flags_default_rather_than_failing() {
        let hints = HashMap::new();
        assert_eq!(string(&hints, "desktop-entry"), "");
        assert!(!boolean(&hints, "transient"));
    }

    #[test]
    fn a_string_hint_of_the_wrong_type_is_ignored() {
        let hints = hint("desktop-entry", Value::I32(7));
        assert_eq!(string(&hints, "desktop-entry"), "");
    }
}

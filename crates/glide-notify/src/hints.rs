use std::collections::HashMap;

use zbus::zvariant::{OwnedValue, Value};

use crate::model::{Image, MAX_IMAGE_BYTES, Sound, Urgency};

#[must_use]
pub fn string(hints: &HashMap<String, OwnedValue>, key: &str) -> String {
    hints.get(key).and_then(|value| String::try_from(value.clone()).ok()).unwrap_or_default()
}

#[must_use]
pub fn boolean(hints: &HashMap<String, OwnedValue>, key: &str) -> bool {
    match hints.get(key).map(|value| &**value) {
        Some(Value::Bool(flag)) => *flag,
        Some(_) => integer(hints, key).is_some_and(|value| value != 0),
        None => false,
    }
}

#[must_use]
pub fn image(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<Image> {
    let Value::Structure(structure) = &**hints.get(key)? else {
        return None;
    };
    let int = |value: &Value<'_>| match value {
        Value::I32(inner) => Some(*inner),
        _ => None,
    };
    let [width, height, rowstride, Value::Bool(has_alpha), bits, channels, Value::Array(data)] = structure.fields()
    else {
        return None;
    };
    if data.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let bytes: Vec<u8> = data
        .inner()
        .iter()
        .map(|value| match value {
            Value::U8(byte) => Some(*byte),
            _ => None,
        })
        .collect::<Option<_>>()?;
    Image::from_raw(int(width)?, int(height)?, int(rowstride)?, *has_alpha, int(bits)?, int(channels)?, &bytes)
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
        if !file.starts_with("file://") {
            return Sound::File(file.to_string());
        }
        if let Some(path) = glide_config::uri::local_path(file) {
            return Sound::File(path.to_string_lossy().into_owned());
        }
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
    fn flags_sent_as_integers_count_as_set_when_non_zero() {
        assert!(boolean(&hint("transient", Value::I32(1)), "transient"));
        assert!(boolean(&hint("transient", Value::U8(1)), "transient"));
        assert!(!boolean(&hint("transient", Value::I32(0)), "transient"));
        assert!(boolean(&hint("transient", Value::Bool(true)), "transient"));
        assert!(!boolean(&hint("transient", Value::from("yes")), "transient"));
    }

    #[test]
    fn sound_file_uris_are_percent_decoded_and_may_name_localhost() {
        assert_eq!(
            sound(&hint("sound-file", Value::from("file:///home/me/My%20Sounds/ping.oga"))),
            Sound::File("/home/me/My Sounds/ping.oga".into())
        );
        assert_eq!(
            sound(&hint("sound-file", Value::from("file://localhost/usr/share/ping.oga"))),
            Sound::File("/usr/share/ping.oga".into())
        );
        assert_eq!(sound(&hint("sound-file", Value::from("/plain/ping.oga"))), Sound::File("/plain/ping.oga".into()));
        assert_eq!(sound(&hint("sound-file", Value::from("file://server/ping.oga"))), Sound::None);
    }

    fn raw(width: i32, height: i32, rowstride: i32, alpha: bool, channels: i32, data: Vec<u8>) -> Value<'static> {
        Value::from((width, height, rowstride, alpha, 8i32, channels, data))
    }

    #[test]
    fn image_data_is_unpacked_honouring_the_rowstride_and_alpha() {
        let rgb = vec![1, 2, 3, 4, 5, 6, 0, 0, 7, 8, 9, 10, 11, 12, 0, 0];
        let unpacked = image(&hint("image-data", raw(2, 2, 8, false, 3, rgb)), "image-data");
        assert_eq!(
            unpacked,
            Some(Image::Rgba {
                width: 2,
                height: 2,
                pixels: vec![1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
            })
        );
        let rgba = vec![1, 2, 3, 4];
        let unpacked = image(&hint("image-data", raw(1, 1, 4, true, 4, rgba.clone())), "image-data");
        assert_eq!(unpacked, Some(Image::Rgba { width: 1, height: 1, pixels: rgba }));
    }

    #[test]
    fn malformed_or_oversized_image_data_is_refused() {
        let refused = |value: Value<'static>| image(&hint("image-data", value), "image-data").is_none();
        assert!(refused(raw(2, 2, 6, false, 3, vec![0; 11])), "too short for its rowstride");
        assert!(refused(raw(2, 1, 4, false, 3, vec![0; 6])), "rowstride narrower than a row");
        assert!(refused(raw(1, 1, 3, true, 3, vec![0; 3])), "alpha needs four channels");
        assert!(refused(raw(-1, 1, 4, true, 4, vec![0; 4])));
        assert!(refused(raw(0, 1, 4, true, 4, vec![0; 4])));
        assert!(refused(raw(4096, 1, 4 * 4096, true, 4, vec![0; 4 * 4096])), "wider than the cap");
        assert!(refused(Value::from((1i32, 1i32, 4i32, true, 16i32, 4i32, vec![0u8; 8]))), "16-bit samples");
        assert!(refused(Value::from("not an image")));
    }

    #[test]
    fn a_string_hint_of_the_wrong_type_is_ignored() {
        let hints = hint("desktop-entry", Value::I32(7));
        assert_eq!(string(&hints, "desktop-entry"), "");
    }
}

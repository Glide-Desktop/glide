use std::collections::HashMap;

use zbus::zvariant::{OwnedValue, Value};

use crate::model::{Image, Urgency};

#[must_use]
pub fn string(hints: &HashMap<String, OwnedValue>, key: &str) -> String {
    hints
        .get(key)
        .and_then(|value| String::try_from(value.clone()).ok())
        .unwrap_or_default()
}

#[must_use]
pub fn boolean(hints: &HashMap<String, OwnedValue>, key: &str) -> bool {
    hints
        .get(key)
        .and_then(|value| bool::try_from(value.clone()).ok())
        .unwrap_or(false)
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
        Value::F64(inner) =>
        {
            #[allow(clippy::cast_possible_truncation)]
            Some(inner.round() as i64)
        }
        _ => None,
    }
}

#[must_use]
pub fn urgency(hints: &HashMap<String, OwnedValue>) -> Urgency {
    integer(hints, "urgency")
        .and_then(|value| u8::try_from(value).ok())
        .map_or(Urgency::Normal, Urgency::from_hint)
}

#[must_use]
pub fn progress(hints: &HashMap<String, OwnedValue>) -> Option<f32> {
    for key in ["value", "x-glide-progress", "x-kde-progress"] {
        if let Some(raw) = integer(hints, key) {
            #[allow(clippy::cast_precision_loss)]
            return Some((raw as f32 / 100.0).clamp(0.0, 1.0));
        }
    }
    None
}

#[must_use]
pub fn image(hints: &HashMap<String, OwnedValue>) -> Option<Image> {
    ["image-data", "image_data", "icon_data"]
        .into_iter()
        .filter_map(|key| hints.get(key))
        .find_map(decode_image)
        .map(crate::model::scale_image)
}

fn decode_image(value: &OwnedValue) -> Option<Image> {
    let Value::Structure(structure) = &**value else {
        return None;
    };
    let fields = structure.fields();
    if fields.len() < 7 {
        return None;
    }

    let width = i32::try_from(fields[0].try_clone().ok()?).ok()?;
    let height = i32::try_from(fields[1].try_clone().ok()?).ok()?;
    let stride = i32::try_from(fields[2].try_clone().ok()?).ok()?;
    let has_alpha = bool::try_from(fields[3].try_clone().ok()?).ok()?;
    let bits = i32::try_from(fields[4].try_clone().ok()?).ok()?;
    let channels = i32::try_from(fields[5].try_clone().ok()?).ok()?;
    let pixels: Vec<u8> = Vec::<u8>::try_from(fields[6].try_clone().ok()?).ok()?;

    normalise(width, height, stride, has_alpha, bits, channels, &pixels)
}

fn normalise(
    width: i32,
    height: i32,
    stride: i32,
    has_alpha: bool,
    bits: i32,
    channels: i32,
    pixels: &[u8],
) -> Option<Image> {
    if width <= 0 || height <= 0 || width > 4096 || height > 4096 || stride <= 0 {
        return None;
    }
    if bits != 8 {
        return None;
    }
    let expected_channels = if has_alpha { 4 } else { 3 };
    if channels != expected_channels {
        return None;
    }

    let columns = usize::try_from(width).ok()?;
    let rows = usize::try_from(height).ok()?;
    let row_stride = usize::try_from(stride).ok()?;
    let step = usize::try_from(channels).ok()?;

    let mut rgba = Vec::with_capacity(columns * rows * 4);
    for y in 0..rows {
        let row = y * row_stride;
        for x in 0..columns {
            let offset = row + x * step;
            if offset + step > pixels.len() {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let r = pixels[offset];
            let g = pixels[offset + 1];
            let b = pixels[offset + 2];
            let a = if has_alpha { pixels[offset + 3] } else { 255 };
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }

    Some(Image {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint(key: &str, value: Value<'static>) -> HashMap<String, OwnedValue> {
        let mut map = HashMap::new();
        map.insert(
            key.to_string(),
            OwnedValue::try_from(value).expect("value converts"),
        );
        map
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
    fn progress_is_normalised_into_a_fraction() {
        let value = progress(&hint("value", Value::I32(42))).expect("progress present");
        assert!((value - 0.42).abs() < 1e-6);
    }

    #[test]
    fn progress_is_clamped_to_the_unit_range() {
        assert_eq!(progress(&hint("value", Value::I32(400))), Some(1.0));
        assert_eq!(progress(&hint("value", Value::I32(-20))), Some(0.0));
    }

    #[test]
    fn a_notification_without_a_progress_hint_reports_none() {
        assert_eq!(progress(&HashMap::new()), None);
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

    #[test]
    fn rgb_pixels_gain_an_opaque_alpha_channel() {
        let image = normalise(2, 1, 6, false, 8, 3, &[1, 2, 3, 4, 5, 6]).expect("image decodes");
        assert_eq!(image.rgba, vec![1, 2, 3, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn row_padding_in_the_stride_is_skipped() {
        let pixels = vec![1, 2, 3, 255, 9, 9, 9, 9, 4, 5, 6, 255, 8, 8, 8, 8];
        let image = normalise(1, 2, 8, true, 8, 4, &pixels).expect("image decodes");
        assert_eq!(image.rgba, vec![1, 2, 3, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn a_short_pixel_buffer_is_padded_rather_than_panicking() {
        let image = normalise(4, 4, 16, true, 8, 4, &[0; 8]).expect("image decodes");
        assert_eq!(image.rgba.len(), 4 * 4 * 4);
    }

    #[test]
    fn implausible_geometry_is_rejected_outright() {
        assert!(normalise(0, 4, 16, true, 8, 4, &[0; 64]).is_none());
        assert!(normalise(9000, 4, 16, true, 8, 4, &[0; 64]).is_none());
        assert!(normalise(4, 4, 0, true, 8, 4, &[0; 64]).is_none());
    }

    #[test]
    fn unsupported_sample_depths_and_channel_counts_are_rejected() {
        assert!(normalise(2, 2, 8, true, 16, 4, &[0; 32]).is_none());
        assert!(normalise(2, 2, 8, true, 8, 3, &[0; 32]).is_none());
    }
}

use std::io::Write;

use base64::Engine;
use glide_config::facekey::{Event, Reason};

use crate::biometrics::RgbaImage;
use crate::camera::CameraError;

pub const PREVIEW_SIDE: usize = 256;

#[must_use]
pub fn quality_reason(cause: &str) -> Reason {
    let cause = cause.to_ascii_lowercase();
    if cause.contains("too dark") || cause.contains("contrast") || cause.contains("brightness") {
        Reason::TooDark
    } else if cause.contains("too bright") || cause.contains("overexposed") {
        Reason::TooBright
    } else if cause.contains("blurry") {
        Reason::Blurry
    } else if cause.contains("too small") || cause.contains("too far") {
        Reason::TooFar
    } else if cause.contains("out of camera frame") {
        Reason::OffFrame
    } else if cause.contains("hold still") || cause.contains("variance") {
        Reason::Moving
    } else {
        Reason::HeadTurned
    }
}

#[must_use]
pub fn reason_from_code(code: &str) -> Reason {
    let (head, detail) = code.split_once(':').unwrap_or((code, ""));
    match head.trim() {
        "PASSED" => Reason::Passed,
        "NO_FACE_DETECTED" | "SEARCHING" | "" => Reason::NoFace,
        "QUALITY_CHECK_FAILED" => quality_reason(detail),
        "SPOOF_SUSPECTED" => Reason::Spoof,
        "IDENTITY_MISMATCH" => Reason::Mismatch,
        "VERIFICATION_TIMEOUT" => Reason::Timeout,
        _ => Reason::Error,
    }
}

#[must_use]
pub fn enrollment_reason(message: &str) -> Reason {
    let message = message.to_ascii_lowercase();
    if message.contains("identity changed") {
        Reason::OtherPerson
    } else if message.contains("timed out") {
        Reason::Timeout
    } else {
        Reason::Error
    }
}

#[must_use]
pub fn camera_reason(error: &CameraError) -> Reason {
    match error {
        CameraError::Untrusted(_) => Reason::CameraUntrusted,
        CameraError::Device(message) if message.contains("busy") => Reason::CameraBusy,
        CameraError::Device(message) if message.contains("No such file") => Reason::NoCamera,
        _ => Reason::Error,
    }
}

#[must_use]
pub fn preview(image: &RgbaImage, side: usize) -> Option<Event> {
    let crop = image.width.min(image.height);
    if crop == 0 || side == 0 {
        return None;
    }
    let (left, top) = ((image.width - crop) / 2, (image.height - crop) / 2);
    let mut rgb = Vec::with_capacity(side * side * 3);
    for y in 0..side {
        let sy = top + y * crop / side;
        for x in 0..side {
            let sx = left + crop - 1 - x * crop / side;
            let i = sy * image.stride() + sx * 4;
            rgb.extend_from_slice(&image.pix[i..i + 3]);
        }
    }
    let mut jpeg = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 72);
    encoder.encode(&rgb, side as u32, side as u32, image::ExtendedColorType::Rgb8).ok()?;
    Some(Event::Frame {
        width: side as u32,
        height: side as u32,
        jpeg: base64::engine::general_purpose::STANDARD.encode(jpeg),
    })
}

pub fn emit(event: &Event) {
    if let Ok(line) = serde_json::to_string(event) {
        let mut out = std::io::stdout().lock();
        if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crown_reason_codes_map_to_stable_reasons() {
        assert_eq!(reason_from_code("PASSED"), Reason::Passed);
        assert_eq!(reason_from_code("QUALITY_CHECK_FAILED: Lighting too dark"), Reason::TooDark);
        assert_eq!(reason_from_code("QUALITY_CHECK_FAILED: Face image is blurry (hold still)"), Reason::Blurry);
        assert_eq!(reason_from_code("QUALITY_CHECK_FAILED: Hold still (embedding variance too high)"), Reason::Moving);
        assert_eq!(reason_from_code("SPOOF_SUSPECTED: Liveness 0.7% < 70.0%"), Reason::Spoof);
        assert_eq!(reason_from_code("IDENTITY_MISMATCH: Similarity 0.31 < 0.50"), Reason::Mismatch);
        assert_eq!(reason_from_code("VERIFICATION_TIMEOUT"), Reason::Timeout);
        assert_eq!(
            enrollment_reason("Identity changed during enrollment (similarity 0.20 < 0.35)"),
            Reason::OtherPerson
        );
    }

    #[test]
    fn previews_are_square_mirrored_jpegs() {
        let mut image = RgbaImage::new(8, 4);
        image.pix[0] = 255;
        let Some(Event::Frame { width, height, jpeg }) = preview(&image, 4) else { panic!("no frame") };
        assert_eq!((width, height), (4, 4));
        let bytes = base64::engine::general_purpose::STANDARD.decode(jpeg).unwrap();
        assert_eq!(&bytes[..2], &[0xff, 0xd8]);
    }
}

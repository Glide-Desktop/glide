use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use super::image::RgbaImage;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Point2D {
    pub x: f32,
    pub y: f32,
}

impl Point2D {
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Landmarks5 {
    pub right_eye: Point2D,
    pub left_eye: Point2D,
    pub nose_tip: Point2D,
    pub right_mouth: Point2D,
    pub left_mouth: Point2D,
}

impl Landmarks5 {
    #[must_use]
    pub fn as_array(&self) -> [Point2D; 5] {
        [self.right_eye, self.left_eye, self.nose_tip, self.right_mouth, self.left_mouth]
    }

    #[must_use]
    pub fn from_array(p: [Point2D; 5]) -> Self {
        Self { right_eye: p[0], left_eye: p[1], nose_tip: p[2], right_mouth: p[3], left_mouth: p[4] }
    }
}

pub const LM_RIGHT_CHEEK: usize = 2;
pub const LM_LEFT_CHEEK: usize = 14;
pub const LM_NOSE_TIP: usize = 30;
pub const LM_RIGHT_MOUTH_CORNER: usize = 48;
pub const LM_LEFT_MOUTH_CORNER: usize = 54;
pub const DENSE_LANDMARK_COUNT: usize = 68;

#[allow(clippy::trivially_copy_pass_by_ref)] // serde hands skip predicates a reference
fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde hands skip predicates a reference
fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedFace {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub score: f32,
    pub landmarks: Landmarks5,
    #[serde(skip)]
    pub dense: Vec<Point2D>,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub track_id: i32,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub track_age: i32,
    pub stability: f64,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub ear: f64,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub motion_score: f64,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub blink_count: i32,
    pub color: String,
}

impl DetectedFace {
    #[must_use]
    pub fn has_dense(&self) -> bool {
        self.dense.len() >= DENSE_LANDMARK_COUNT
    }

    #[must_use]
    pub fn anchor(&self) -> (f32, f32) {
        let l = &self.landmarks;
        ((l.right_eye.x + l.left_eye.x + l.nose_tip.x) / 3.0, (l.right_eye.y + l.left_eye.y + l.nose_tip.y) / 3.0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityMetrics {
    pub blur_score: f64,
    pub brightness: f64,
    pub contrast: f64,
    pub face_size: f64,
    pub pose_yaw: f64,
    pub pose_pitch: f64,
    pub pose_roll: f64,
    pub eye_openness: f64,
    pub symmetry: f64,
    pub stability: f64,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reject_cause: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionResult {
    #[serde(skip)]
    pub embedding: Vec<f32>,
    pub similarity: f64,
    pub threshold: f64,
    pub matched: bool,
}

#[allow(clippy::struct_excessive_bools)] // Each veto is reported on its own in the diagnostic report
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntiSpoofResult {
    pub score: f64,
    pub neural_score: f64,
    pub texture_score: f64,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub motion_score: f64,
    pub raw_logits: [f32; 3],
    pub class_probs: [f64; 3],
    pub threshold: f64,
    pub is_live: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub screen_spoof: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub static_spoof: bool,
    pub uncertain: bool,
}

mod duration_ns {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(d.as_nanos() as u64)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        Ok(Duration::from_nanos(u64::deserialize(d)?))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct LatencyBreakdown {
    #[serde(rename = "captureMs", with = "duration_ns")]
    pub capture: Duration,
    #[serde(rename = "detectionMs", with = "duration_ns")]
    pub detection: Duration,
    #[serde(rename = "qualityMs", with = "duration_ns")]
    pub quality: Duration,
    #[serde(rename = "alignmentMs", with = "duration_ns")]
    pub alignment: Duration,
    #[serde(rename = "recognitionMs", with = "duration_ns")]
    pub recognition: Duration,
    #[serde(rename = "antiSpoofMs", with = "duration_ns")]
    pub anti_spoof: Duration,
    #[serde(rename = "totalMs", with = "duration_ns")]
    pub total: Duration,
}

#[must_use]
pub fn duration_ms(d: Duration) -> f64 {
    d.as_micros() as f64 / 1000.0
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiometricEvaluation {
    #[serde(with = "go_time")]
    pub timestamp: DateTime<Local>,
    pub face_found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face: Option<DetectedFace>,
    #[serde(skip)]
    pub all_faces: Vec<DetectedFace>,
    pub quality: QualityMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recognition: Option<RecognitionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anti_spoof: Option<AntiSpoofResult>,
    pub latency: LatencyBreakdown,
    pub accepted: bool,
    pub reason_code: String,
    #[serde(skip)]
    pub frozen_frame: Option<Arc<RgbaImage>>,
    #[serde(skip)]
    pub aligned_face: Option<Arc<RgbaImage>>,
}

impl BiometricEvaluation {
    #[must_use]
    pub fn new() -> Self {
        Self {
            timestamp: Local::now(),
            face_found: false,
            face: None,
            all_faces: Vec::new(),
            quality: QualityMetrics::default(),
            recognition: None,
            anti_spoof: None,
            latency: LatencyBreakdown::default(),
            accepted: false,
            reason_code: String::new(),
            frozen_frame: None,
            aligned_face: None,
        }
    }

    #[must_use]
    pub fn is_live(&self) -> bool {
        self.anti_spoof.as_ref().is_some_and(|a| a.is_live)
    }
}

impl Default for BiometricEvaluation {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
pub(crate) fn clamp_f64(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

pub mod go_time {
    use chrono::{DateTime, Local, Timelike};
    use serde::{Deserialize, Deserializer, Serializer};

    #[must_use]
    pub fn format(t: &DateTime<Local>) -> String {
        let mut s = t.format("%Y-%m-%dT%H:%M:%S").to_string();
        let ns = t.nanosecond() % 1_000_000_000;
        if ns > 0 {
            let frac = format!("{ns:09}");
            s.push('.');
            s.push_str(frac.trim_end_matches('0'));
        }
        let off = t.format("%:z").to_string();
        s.push_str(if off == "+00:00" { "Z" } else { &off });
        s
    }

    pub fn serialize<S: Serializer>(t: &DateTime<Local>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format(t))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<DateTime<Local>, D::Error> {
        let s = String::deserialize(d)?;
        DateTime::parse_from_rfc3339(&s).map(|t| t.with_timezone(&Local)).map_err(serde::de::Error::custom)
    }

    pub mod option {
        use chrono::{DateTime, Local};
        use serde::{Deserialize, Deserializer, Serializer};

        #[allow(clippy::ref_option)] // serde hands `with` serializers a reference to the field
        pub fn serialize<S: Serializer>(t: &Option<DateTime<Local>>, s: S) -> Result<S::Ok, S::Error> {
            match t {
                Some(t) => s.serialize_str(&super::format(t)),
                None => s.serialize_none(),
            }
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<DateTime<Local>>, D::Error> {
            let s: Option<String> = Option::deserialize(d)?;
            s.map(|s| {
                DateTime::parse_from_rfc3339(&s).map(|t| t.with_timezone(&Local)).map_err(serde::de::Error::custom)
            })
            .transpose()
        }
    }
}

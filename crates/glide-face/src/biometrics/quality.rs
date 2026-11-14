use super::image::RgbaImage;
use super::landmarks::compute_ear68;
use super::pose3d::estimate_head_pose_3d;
use super::types::{DetectedFace, LM_LEFT_CHEEK, LM_NOSE_TIP, LM_RIGHT_CHEEK, QualityMetrics};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityThresholds {
    pub min_face_size: f64,
    pub min_blur_score: f64,
    pub min_brightness: f64,
    pub max_brightness: f64,
    pub min_contrast: f64,
    pub max_yaw_angle: f64,
    pub max_pitch_angle: f64,
    pub max_roll_angle: f64,
}

pub const DEFAULT_QUALITY_THRESHOLDS: QualityThresholds = QualityThresholds {
    min_face_size: 80.0,
    min_blur_score: 30.0,
    min_brightness: 40.0,
    max_brightness: 225.0,
    min_contrast: 15.0,
    max_yaw_angle: 40.0,
    max_pitch_angle: 35.0,
    max_roll_angle: 25.0,
};

impl Default for QualityThresholds {
    fn default() -> Self {
        DEFAULT_QUALITY_THRESHOLDS
    }
}

fn reject(mut qm: QualityMetrics, cause: &str) -> QualityMetrics {
    qm.passed = false;
    qm.reject_cause = cause.to_string();
    qm
}

#[must_use]
pub fn compute_quality(src: &RgbaImage, face: &DetectedFace, th: &QualityThresholds) -> QualityMetrics {
    let mut qm = QualityMetrics { passed: true, ..Default::default() };

    let min_dim = (face.width as f64).min(face.height as f64);
    qm.face_size = min_dim;
    if min_dim < th.min_face_size {
        return reject(qm, "Face too small or too far from camera");
    }

    let pts = &face.landmarks;
    let dist_nose_right = (pts.nose_tip.x - pts.right_eye.x).abs() as f64;
    let dist_nose_left = (pts.left_eye.x - pts.nose_tip.x).abs() as f64;
    let eye_asym = if dist_nose_right + dist_nose_left > 1e-3 {
        (dist_nose_right - dist_nose_left) / (dist_nose_right + dist_nose_left)
    } else {
        0.0
    };
    let (yaw, pitch, roll, reproj_err) = estimate_head_pose_3d(face);
    qm.pose_yaw = yaw;
    qm.pose_pitch = pitch;
    qm.pose_roll = roll;

    if face.has_dense() {
        let d = &face.dense;
        qm.eye_openness = compute_ear68(d);
        let dcr = (d[LM_NOSE_TIP].x - d[LM_RIGHT_CHEEK].x).abs() as f64;
        let dcl = (d[LM_LEFT_CHEEK].x - d[LM_NOSE_TIP].x).abs() as f64;
        let sum = dcr + dcl;
        qm.symmetry = if sum > 1e-3 {
            let cheek_asym = (dcr - dcl) / sum;
            (1.0 - 0.5 * (eye_asym.abs() + cheek_asym.abs()) - 0.5 * reproj_err.min(0.20)).max(0.0)
        } else {
            (1.0 - eye_asym.abs()).max(0.0)
        };
    } else {
        qm.eye_openness = 0.30;
        qm.symmetry = (1.0 - eye_asym.abs()).max(0.0);
    }
    qm.stability = if face.track_age > 0 || face.stability > 0.0 { face.stability } else { 1.0 };

    let x0 = (face.x as i64).max(0);
    let y0 = (face.y as i64).max(0);
    let x1 = ((face.x + face.width) as i64).min(src.width as i64);
    let y1 = ((face.y + face.height) as i64).min(src.height as i64);
    let crop_w = x1 - x0;
    let crop_h = y1 - y0;
    if crop_w < 20 || crop_h < 20 {
        return reject(qm, "Face region out of camera frame");
    }
    let (crop_w, crop_h, x0, y0) = (crop_w as usize, crop_h as usize, x0 as usize, y0 as usize);
    let mut gray = vec![0f64; crop_w * crop_h];
    let (mut sum, mut sum_sq) = (0.0, 0.0);
    for cy in 0..crop_h {
        for cx in 0..crop_w {
            let v = src.lum(x0 + cx, y0 + cy);
            gray[cy * crop_w + cx] = v;
            sum += v;
            sum_sq += v * v;
        }
    }
    let n = (crop_w * crop_h) as f64;
    let mean = sum / n;
    let variance = sum_sq / n - mean * mean;
    qm.brightness = mean;
    qm.contrast = if variance > 0.0 { variance.sqrt() } else { 0.0 };

    let (mut lap_sum, mut lap_sq, mut lap_n) = (0.0, 0.0, 0.0);
    for cy in 1..crop_h - 1 {
        for cx in 1..crop_w - 1 {
            let i = cy * crop_w + cx;
            let lap = gray[i - crop_w] + gray[i + crop_w] + gray[i - 1] + gray[i + 1] - 4.0 * gray[i];
            lap_sum += lap;
            lap_sq += lap * lap;
            lap_n += 1.0;
        }
    }
    if lap_n > 0.0 {
        let lap_mean = lap_sum / lap_n;
        qm.blur_score = lap_sq / lap_n - lap_mean * lap_mean;
    }

    if qm.blur_score < th.min_blur_score {
        return reject(qm, "Face image is blurry (hold still)");
    }
    if qm.brightness < th.min_brightness {
        return reject(qm, "Lighting too dark");
    }
    if qm.brightness > th.max_brightness {
        return reject(qm, "Lighting too bright (overexposed)");
    }
    if qm.contrast < th.min_contrast {
        return reject(qm, "Low image contrast");
    }
    if qm.pose_roll.abs() > th.max_roll_angle {
        return reject(qm, "Keep your head upright");
    }
    if qm.pose_yaw.abs() > th.max_yaw_angle {
        return reject(qm, "Head turned too far - face the camera more");
    }
    if qm.pose_pitch.abs() > th.max_pitch_angle {
        return reject(qm, "Head tilted too far - level your head");
    }
    qm
}

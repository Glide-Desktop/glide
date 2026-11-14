use std::sync::Mutex;

use ort::session::Session;

use super::image::RgbaImage;
use super::ort_init::{build_session, run_session};
use super::types::{DetectedFace, Landmarks5, Point2D};
use super::{BiometricError, models};

pub const DEFAULT_COLORS: [&str; 6] = ["#00FF66", "#3399FF", "#FFCC00", "#FF3366", "#9933FF", "#00FFFF"];

const INPUT: usize = 640;
const OUTPUTS: [&str; 12] = [
    "cls_8", "cls_16", "cls_32", "obj_8", "obj_16", "obj_32", "bbox_8", "bbox_16", "bbox_32", "kps_8", "kps_16",
    "kps_32",
];
const PLANE: usize = INPUT * INPUT;

pub struct YuNetDetector {
    session: Mutex<Session>,
    pub score_threshold: f32,
    pub nms_threshold: f32,
}

impl YuNetDetector {
    pub fn new() -> Result<Self, BiometricError> {
        Ok(Self {
            session: Mutex::new(build_session(&models::load(models::YUNET)?, "YuNet")?),
            score_threshold: 0.60,
            nms_threshold: 0.40,
        })
    }

    #[allow(clippy::too_many_lines, clippy::many_single_char_names)] // One pass of letterbox, decoding and NMS fusion, kept bit-identical with the reference detector
    pub fn detect(&self, img: &RgbaImage) -> Result<Vec<DetectedFace>, BiometricError> {
        let (orig_w, orig_h) = (img.width, img.height);
        if orig_w == 0 || orig_h == 0 {
            return Ok(Vec::new());
        }

        let mut scale = orig_w as f32 / INPUT as f32;
        let s = orig_h as f32 / INPUT as f32;
        if s > scale {
            scale = s;
        }
        if scale < 1.0 {
            scale = 1.0;
        }
        let used_w = ((orig_w as f32 / scale) as usize).min(INPUT);
        let used_h = ((orig_h as f32 / scale) as usize).min(INPUT);

        let mut input = vec![0f32; 3 * PLANE];
        let stride = img.stride();
        for y in 0..used_h {
            let sy = ((y as f32 * scale) as usize).min(orig_h - 1);
            let row = sy * stride;
            for x in 0..used_w {
                let sx = ((x as f32 * scale) as usize).min(orig_w - 1);
                let p = row + sx * 4;
                let idx = y * INPUT + x;
                input[idx] = f32::from(img.pix[p + 2]);
                input[PLANE + idx] = f32::from(img.pix[p + 1]);
                input[2 * PLANE + idx] = f32::from(img.pix[p]);
            }
        }

        let out = {
            let session = self.session.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            run_session(&session, "input", [1, 3, INPUT, INPUT], input, &OUTPUTS, "YuNet")?
        };

        let mut candidates: Vec<DetectedFace> = Vec::new();
        for (si, (stride_px, grid)) in [(8.0f32, 80usize), (16.0, 40), (32.0, 20)].into_iter().enumerate() {
            let (cls, obj, bbox, kps) = (&out[si], &out[3 + si], &out[6 + si], &out[9 + si]);
            for r in 0..grid {
                for c in 0..grid {
                    let idx = r * grid + c;
                    let cls_v = cls[idx].clamp(0.0, 1.0);
                    let obj_v = obj[idx].clamp(0.0, 1.0);
                    let score = ((cls_v * obj_v) as f64).sqrt() as f32;
                    if score < self.score_threshold {
                        continue;
                    }
                    let b = idx * 4;
                    let cx = (c as f32 + bbox[b]) * stride_px;
                    let cy = (r as f32 + bbox[b + 1]) * stride_px;
                    let w = (bbox[b + 2] as f64).exp() as f32 * stride_px;
                    let h = (bbox[b + 3] as f64).exp() as f32 * stride_px;
                    let k = idx * 10;
                    let kp = |i: usize| {
                        Point2D::new(
                            (kps[k + i * 2] + c as f32) * stride_px * scale,
                            (kps[k + i * 2 + 1] + r as f32) * stride_px * scale,
                        )
                    };
                    candidates.push(DetectedFace {
                        x: (cx - w / 2.0) * scale,
                        y: (cy - h / 2.0) * scale,
                        width: w * scale,
                        height: h * scale,
                        score,
                        landmarks: Landmarks5::from_array([kp(0), kp(1), kp(2), kp(3), kp(4)]),
                        ..Default::default()
                    });
                }
            }
        }

        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        let mut faces = Vec::new();
        let mut suppressed = vec![false; candidates.len()];
        for i in 0..candidates.len() {
            if suppressed[i] {
                continue;
            }
            let seed = &candidates[i];
            let w_seed = (seed.score as f64).powi(4);
            let mut sum_w = w_seed;
            let mut sx = seed.x as f64 * w_seed;
            let mut sy = seed.y as f64 * w_seed;
            let mut sbw = seed.width as f64 * w_seed;
            let mut sbh = seed.height as f64 * w_seed;
            let mut skp = [[0f64; 2]; 5];
            for (k, p) in seed.landmarks.as_array().iter().enumerate() {
                skp[k] = [p.x as f64 * w_seed, p.y as f64 * w_seed];
            }
            for j in i + 1..candidates.len() {
                if suppressed[j] {
                    continue;
                }
                let b = &candidates[j];
                let iou = compute_iou(seed, b);
                if iou > self.nms_threshold {
                    suppressed[j] = true;
                    if iou > 0.50 && seed.score - b.score <= 0.08 {
                        let w = (b.score as f64).powi(4);
                        sum_w += w;
                        sx += b.x as f64 * w;
                        sy += b.y as f64 * w;
                        sbw += b.width as f64 * w;
                        sbh += b.height as f64 * w;
                        for (k, p) in b.landmarks.as_array().iter().enumerate() {
                            skp[k][0] += p.x as f64 * w;
                            skp[k][1] += p.y as f64 * w;
                        }
                    }
                }
            }
            let inv = 1.0 / sum_w;
            let kp = |k: usize| Point2D::new((skp[k][0] * inv) as f32, (skp[k][1] * inv) as f32);
            faces.push(DetectedFace {
                x: (sx * inv) as f32,
                y: (sy * inv) as f32,
                width: (sbw * inv) as f32,
                height: (sbh * inv) as f32,
                score: seed.score,
                landmarks: Landmarks5::from_array([kp(0), kp(1), kp(2), kp(3), kp(4)]),
                stability: 1.0,
                ..Default::default()
            });
        }
        for (i, f) in faces.iter_mut().enumerate() {
            f.color = DEFAULT_COLORS[i % DEFAULT_COLORS.len()].to_string();
        }
        Ok(faces)
    }
}

#[must_use]
pub fn compute_iou(a: &DetectedFace, b: &DetectedFace) -> f32 {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.width).min(b.x + b.width);
    let y2 = (a.y + a.height).min(b.y + b.height);
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let union = a.width * a.height + b.width * b.height - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

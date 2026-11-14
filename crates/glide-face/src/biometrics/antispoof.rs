use std::sync::Mutex;

use ort::session::Session;

use super::image::RgbaImage;
use super::ort_init::{build_session, run_session};
use super::types::{AntiSpoofResult, DetectedFace, clamp_f64};
use super::{BiometricError, models};

pub const DEFAULT_LIVENESS_THRESHOLD: f64 = 0.70;
pub const MINIFASNET_CROP_SCALE: f64 = 2.7;
const SIDE: usize = 80;
const PLANE: usize = SIDE * SIDE;

pub struct MiniFasNetAntiSpoofer {
    session: Mutex<Session>,
    pub threshold: f64,
}

impl MiniFasNetAntiSpoofer {
    pub fn new() -> Result<Self, BiometricError> {
        Ok(Self {
            session: Mutex::new(build_session(&models::load(models::MINIFASNET)?, "MiniFASNet")?),
            threshold: DEFAULT_LIVENESS_THRESHOLD,
        })
    }

    #[allow(clippy::similar_names, clippy::too_many_lines)] // Crop, resample and score in one pass, kept identical to the reference preprocessing
    pub fn evaluate_liveness(&self, src: &RgbaImage, face: &DetectedFace) -> Result<AntiSpoofResult, BiometricError> {
        let (src_w, src_h) = (src.width, src.height);
        if src_w < 2 || src_h < 2 || face.width <= 1.0 || face.height <= 1.0 {
            return Err(BiometricError::Inference("invalid image or face box".into()));
        }

        let (w_img, h_img) = (src_w as f64, src_h as f64);
        let (bw, bh) = (f64::from(face.width), f64::from(face.height));
        let scale = MINIFASNET_CROP_SCALE.min(((h_img - 1.0) / bh).min((w_img - 1.0) / bw));
        let (nw, nh) = (bw * scale, bh * scale);
        let (cx, cy) = (f64::from(face.x) + bw / 2.0, f64::from(face.y) + bh / 2.0);
        let (mut x0, mut y0, mut x1, mut y1) = (cx - nw / 2.0, cy - nh / 2.0, cx + nw / 2.0, cy + nh / 2.0);
        if x0 < 0.0 {
            x1 -= x0;
            x0 = 0.0;
        }
        if y0 < 0.0 {
            y1 -= y0;
            y0 = 0.0;
        }
        if x1 > w_img - 1.0 {
            x0 -= x1 - w_img + 1.0;
            x1 = w_img - 1.0;
        }
        if y1 > h_img - 1.0 {
            y0 -= y1 - h_img + 1.0;
            y1 = h_img - 1.0;
        }
        let (ix0, iy0, ix1, iy1) = (x0 as i64, y0 as i64, x1 as i64, y1 as i64);
        let crop_w = (ix1 - ix0 + 1).max(1) as usize;
        let crop_h = (iy1 - iy0 + 1).max(1) as usize;
        let (ix0, iy0) = (ix0.max(0) as usize, iy0.max(0) as usize);

        let mut input = vec![0f32; 3 * PLANE];
        let stride = src.stride();
        let pix = &src.pix;
        let sx_scale = crop_w as f64 / SIDE as f64;
        let sy_scale = crop_h as f64 / SIDE as f64;
        let max_row = src_h - 1;
        let max_col = src_w - 1;
        for y in 0..SIDE {
            let fy = ((y as f64 + 0.5) * sy_scale - 0.5).max(0.0);
            let py0 = fy as usize;
            let py1 = (py0 + 1).min(crop_h - 1);
            let wy = fy - py0 as f64;
            let row0 = (iy0 + py0).min(max_row) * stride;
            let row1 = (iy0 + py1).min(max_row) * stride;
            for x in 0..SIDE {
                let fx = ((x as f64 + 0.5) * sx_scale - 0.5).max(0.0);
                let px0 = fx as usize;
                let px1 = (px0 + 1).min(crop_w - 1);
                let wx = fx - px0 as f64;
                let c0 = (ix0 + px0).min(max_col) * 4;
                let c1 = (ix0 + px1).min(max_col) * 4;
                let (p00, p01, p10, p11) = (row0 + c0, row0 + c1, row1 + c0, row1 + c1);
                let w00 = (1.0 - wx) * (1.0 - wy);
                let w01 = wx * (1.0 - wy);
                let w10 = (1.0 - wx) * wy;
                let w11 = wx * wy;
                let idx = y * SIDE + x;
                for ch in 0..3 {
                    let v = w00 * f64::from(pix[p00 + ch])
                        + w01 * f64::from(pix[p01 + ch])
                        + w10 * f64::from(pix[p10 + ch])
                        + w11 * f64::from(pix[p11 + ch]);
                    input[(2 - ch) * PLANE + idx] = v as f32;
                }
            }
        }

        let out = {
            let session = self.session.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            run_session(&session, "input", [1, 3, SIDE, SIDE], input, &["output"], "MiniFASNet")?
        };
        let o = &out[0];
        if o.len() < 3 {
            return Err(BiometricError::Inference("MiniFASNet: short output".into()));
        }
        let logits = [o[0], o[1], o[2]];
        let max_logit = logits.iter().map(|&v| f64::from(v)).fold(f64::MIN, f64::max);
        let e: Vec<f64> = logits.iter().map(|&v| (f64::from(v) - max_logit).exp()).collect();
        let sum: f64 = e.iter().sum();
        let probs = [e[0] / sum, e[1] / sum, e[2] / sum];

        let neural_score = probs[1];
        let (texture_score, screen_spoof) = analyze_texture_and_screen_artifacts(src, face);
        let motion_score = if face.motion_score <= 0.0 { 0.92 } else { face.motion_score };
        let static_spoof = face.track_age >= 8 && motion_score < 0.40;

        let mut live_score = neural_score;
        if screen_spoof {
            live_score = live_score.min(0.42);
        }
        if static_spoof {
            live_score = live_score.min(0.45);
        }
        let is_live = live_score >= self.threshold && !screen_spoof && !static_spoof;
        let uncertain = live_score >= 0.50 && !is_live;

        Ok(AntiSpoofResult {
            score: live_score,
            neural_score,
            texture_score,
            motion_score,
            raw_logits: logits,
            class_probs: probs,
            threshold: self.threshold,
            is_live,
            screen_spoof,
            static_spoof,
            uncertain,
        })
    }
}

#[allow(clippy::many_single_char_names)] // Pixel channels r, g, b and coordinates x, y, w, h
#[must_use]
pub fn analyze_texture_and_screen_artifacts(src: &RgbaImage, face: &DetectedFace) -> (f64, bool) {
    let mut x0 = (face.x + face.width * 0.18) as i64;
    let mut y0 = (face.y + face.height * 0.18) as i64;
    let mut x1 = (face.x + face.width * 0.82) as i64;
    let mut y1 = (face.y + face.height * 0.82) as i64;
    let (w, h) = (src.width as i64, src.height as i64);
    if x0 < 1 {
        x0 = 1;
    }
    if y0 < 1 {
        y0 = 1;
    }
    if x1 >= w - 1 {
        x1 = w - 2;
    }
    if y1 >= h - 1 {
        y1 = h - 2;
    }
    if x1 - x0 < 16 || y1 - y0 < 16 {
        return (0.85, false);
    }

    let stride = src.stride();
    let pix = &src.pix;
    let lum_at = |i: usize| 0.299 * f64::from(pix[i]) + 0.587 * f64::from(pix[i + 1]) + 0.114 * f64::from(pix[i + 2]);
    let (mut total, mut specular, mut moire, mut high_freq, mut grad) = (0.0, 0.0, 0.0, 0.0, 0.0);

    let mut y = y0;
    while y < y1 {
        let row = y as usize * stride;
        let row_up = (y - 1) as usize * stride;
        let row_dn = (y + 1) as usize * stride;
        let mut x = x0;
        while x < x1 {
            let xc = x as usize * 4;
            let idx = row + xc;
            let (r, g, b) = (f64::from(pix[idx]), f64::from(pix[idx + 1]), f64::from(pix[idx + 2]));
            let lum_c = 0.299 * r + 0.587 * g + 0.114 * b;
            let lum_l = lum_at(row + xc - 4);
            let lum_r = lum_at(row + xc + 4);
            let lum_u = lum_at(row_up + xc);
            let lum_d = lum_at(row_dn + xc);

            total += 1.0;
            if r > 250.0 && g > 250.0 && b > 250.0 {
                specular += 1.0;
            }
            let dx1 = lum_c - lum_l;
            let dx2 = lum_r - lum_c;
            let dy1 = lum_c - lum_u;
            let dy2 = lum_d - lum_c;
            if dx1 * dx2 < -64.0 || dy1 * dy2 < -64.0 {
                moire += 1.0;
            }
            high_freq += (dx2 - dx1).abs() + (dy2 - dy1).abs();
            grad += (lum_r - lum_l).abs() + (lum_d - lum_u).abs() + 1.0;
            x += 2;
        }
        y += 2;
    }
    if total < 10.0 {
        return (0.85, false);
    }
    let specular_ratio = specular / total;
    let moire_ratio = moire / total;
    let freq_ratio = high_freq / grad;
    if moire_ratio > 0.28 && freq_ratio > 1.55 {
        return (0.20, true);
    }
    if specular_ratio > 0.18 {
        return (0.30, true);
    }
    (clamp_f64(0.96 - 1.2 * moire_ratio - 1.5 * specular_ratio, 0.10, 1.00), false)
}

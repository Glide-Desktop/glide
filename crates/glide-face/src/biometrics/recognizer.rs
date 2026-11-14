use std::sync::Mutex;

use ort::session::Session;

use super::image::RgbaImage;
use super::ort_init::{build_session, run_session};
use super::types::QualityMetrics;
use super::{BiometricError, models};

pub const EMBEDDING_DIM: usize = 512;
pub const RECOGNIZER_ID: &str = "GhostFaceNetV1-1.3-1-MS1MV3-512d";
const SIDE: usize = 112;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecognizerSpec {
    pub name: &'static str,
    pub input: &'static str,
    pub output: &'static str,
    pub mean: f32,
    pub scale: f32,
    pub dim: usize,
}

pub const GHOSTFACENET: RecognizerSpec = RecognizerSpec {
    name: "GhostFaceNetV1-1.3-1 MS1MV3",
    input: "input",
    output: "embedding",
    mean: 127.5,
    scale: 0.007_812_5,
    dim: 512,
};

pub const SFACE: RecognizerSpec =
    RecognizerSpec { name: "SFace 2021Dec", input: "data", output: "fc1", mean: 0.0, scale: 1.0, dim: 128 };

pub struct FaceRecognizer {
    session: Mutex<Session>,
    pub spec: RecognizerSpec,
    pub enable_flip_tta: bool,
}

fn l2_normalized(v: &[f32]) -> Vec<f64> {
    let mut out: Vec<f64> = v.iter().map(|&x| x as f64).collect();
    let norm = out.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm > 1e-9 {
        for o in &mut out {
            *o /= norm;
        }
    }
    out
}

impl FaceRecognizer {
    pub fn new() -> Result<Self, BiometricError> {
        Self::from_model(GHOSTFACENET, &models::load(models::RECOGNIZER)?)
    }

    pub fn from_model(spec: RecognizerSpec, model: &[u8]) -> Result<Self, BiometricError> {
        Ok(Self { session: Mutex::new(build_session(model, spec.name)?), spec, enable_flip_tta: true })
    }

    fn run(&self, aligned: &RgbaImage, flip: bool) -> Result<Vec<f32>, BiometricError> {
        const PLANE: usize = SIDE * SIDE;
        let (mean, scale) = (self.spec.mean, self.spec.scale);
        let mut input = vec![0f32; 3 * PLANE];
        let stride = aligned.stride();
        for y in 0..SIDE {
            for x in 0..SIDE {
                let sx = if flip { SIDE - 1 - x } else { x };
                let p = y * stride + sx * 4;
                let i = y * SIDE + x;
                input[i] = (aligned.pix[p] as f32 - mean) * scale;
                input[PLANE + i] = (aligned.pix[p + 1] as f32 - mean) * scale;
                input[2 * PLANE + i] = (aligned.pix[p + 2] as f32 - mean) * scale;
            }
        }
        let session = self.session.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut out =
            run_session(&session, self.spec.input, [1, 3, SIDE, SIDE], input, &[self.spec.output], self.spec.name)?;
        let v = out.swap_remove(0);
        if v.len() < self.spec.dim {
            return Err(BiometricError::Inference(format!("{}: short output", self.spec.name)));
        }
        Ok(v)
    }

    pub fn extract_embedding(&self, aligned: &RgbaImage) -> Result<Vec<f32>, BiometricError> {
        if aligned.width != SIDE || aligned.height != SIDE {
            return Err(BiometricError::Inference(format!(
                "expected 112x112 aligned image, got {}x{}",
                aligned.width, aligned.height
            )));
        }
        let dim = self.spec.dim;
        let orig = l2_normalized(&self.run(aligned, false)?[..dim]);
        if !self.enable_flip_tta {
            return Ok(orig.iter().map(|&v| v as f32).collect());
        }
        let flip = l2_normalized(&self.run(aligned, true)?[..dim]);

        let mut emb = vec![0f32; dim];
        let mut norm_sq = 0.0;
        for i in 0..dim {
            let v = 0.50 * orig[i] + 0.50 * flip[i];
            emb[i] = v as f32;
            norm_sq += v * v;
        }
        let norm = norm_sq.sqrt();
        if norm > 1e-9 {
            let inv = (1.0 / norm) as f32;
            for v in &mut emb {
                *v *= inv;
            }
        }
        Ok(emb)
    }
}

#[must_use]
pub fn compute_adaptive_threshold(base: f64, q: &QualityMetrics) -> f64 {
    let mut th = base;
    let max_angle = q.pose_yaw.abs().max(q.pose_pitch.abs());
    if max_angle > 14.0 {
        th += 0.04f64.min((max_angle - 14.0) * 0.0025);
    }
    if q.blur_score > 0.0 && q.blur_score < 55.0 {
        th += (55.0 - q.blur_score) * 0.0008;
    }
    th
}

#[must_use]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    a.iter().zip(b).map(|(&x, &y)| f64::from(x * y)).sum()
}

#[must_use]
pub fn best_match_similarity(live: &[f32], centroid: &[f32], multi: &[Vec<f32>]) -> f64 {
    super::calibration::verifier_score(live, centroid, multi, super::calibration::thresholds().centroid_floor)
}

#[must_use]
pub fn average_embeddings(embeddings: &[Vec<f32>]) -> Vec<f32> {
    weighted_average_embeddings(embeddings, None)
}

#[must_use]
pub fn weighted_average_embeddings(embeddings: &[Vec<f32>], weights: Option<&[f64]>) -> Vec<f32> {
    match embeddings.len() {
        0 => return Vec::new(),
        1 => return embeddings[0].clone(),
        _ => {}
    }
    let dim = embeddings[0].len();
    let mut sum = vec![0f64; dim];
    for (k, emb) in embeddings.iter().enumerate() {
        let w = weights.and_then(|w| w.get(k).copied()).unwrap_or(1.0);
        for (s, &v) in sum.iter_mut().zip(emb.iter()) {
            *s += w * v as f64;
        }
    }
    let norm = sum.iter().map(|v| v * v).sum::<f64>().sqrt();
    if norm > 1e-9 { sum.iter().map(|v| (v / norm) as f32).collect() } else { vec![0f32; dim] }
}

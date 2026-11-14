use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use chrono::Local;

use super::BiometricError;
use super::align::align_face_mesh;
use super::antispoof::{DEFAULT_LIVENESS_THRESHOLD, MiniFasNetAntiSpoofer};
use super::calibration::default_match_threshold;
use super::image::RgbaImage;
use super::landmark_onnx::Landmark68Aligner;
use super::landmarks::extract_dense_landmarks;
use super::quality::{DEFAULT_QUALITY_THRESHOLDS, QualityThresholds, compute_quality};
use super::recognizer::{FaceRecognizer, best_match_similarity, compute_adaptive_threshold};
use super::tracker::{FaceTracker, OneEuroFilter};
use super::types::{
    AntiSpoofResult, BiometricEvaluation, DENSE_LANDMARK_COUNT, DetectedFace, Point2D, RecognitionResult,
};
use super::yunet::YuNetDetector;

const NEURAL_FACES: usize = 2;

struct LiveSlot {
    filters: Vec<OneEuroFilter>,
    last: Option<Instant>,
}

impl LiveSlot {
    fn new() -> Self {
        Self { filters: vec![OneEuroFilter::new(2.0, 1.6, 1.0); DENSE_LANDMARK_COUNT * 2], last: None }
    }

    fn reset(&mut self) {
        self.filters.iter_mut().for_each(OneEuroFilter::reset);
        self.last = None;
    }
}

struct LiveState {
    aligner: Landmark68Aligner,
    slots: [LiveSlot; NEURAL_FACES],
}

pub struct Pipeline {
    detector: YuNetDetector,
    aligner68: Landmark68Aligner,
    recognizer: FaceRecognizer,
    spoofer: MiniFasNetAntiSpoofer,
    tracker: Mutex<FaceTracker>,
    live: Mutex<LiveState>,

    pub recognition_threshold: f64,
    pub liveness_threshold: f64,
    pub quality_thresholds: QualityThresholds,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Pipeline {
    pub fn new() -> Result<Self, BiometricError> {
        fn ctx(stage: &'static str) -> impl Fn(BiometricError) -> BiometricError {
            move |e| BiometricError::Init(format!("failed to init {stage}: {e}"))
        }
        Ok(Self {
            detector: YuNetDetector::new().map_err(ctx("YuNet"))?,
            aligner68: Landmark68Aligner::new().map_err(ctx("Landmark68Aligner"))?,
            recognizer: FaceRecognizer::new().map_err(ctx("FaceRecognizer"))?,
            spoofer: MiniFasNetAntiSpoofer::new().map_err(ctx("MiniFASNet"))?,
            tracker: Mutex::new(FaceTracker::new()),
            live: Mutex::new(LiveState {
                aligner: Landmark68Aligner::new().map_err(ctx("live Landmark68Aligner"))?,
                slots: [LiveSlot::new(), LiveSlot::new()],
            }),
            recognition_threshold: default_match_threshold(),
            liveness_threshold: DEFAULT_LIVENESS_THRESHOLD,
            quality_thresholds: DEFAULT_QUALITY_THRESHOLDS,
        })
    }

    pub fn reset_tracker(&self) {
        locked(&self.tracker).reset();
        locked(&self.live).slots.iter_mut().for_each(LiveSlot::reset);
    }

    fn detect_with_landmarks(&self, frame: &RgbaImage) -> Result<Vec<DetectedFace>, BiometricError> {
        let mut faces = self.detector.detect(frame)?;
        for (i, face) in faces.iter_mut().enumerate() {
            let neural = if i < NEURAL_FACES { self.aligner68.predict68(frame, face).ok() } else { None };
            face.dense = match neural {
                Some(d) if d.len() >= DENSE_LANDMARK_COUNT => d,
                _ => extract_dense_landmarks(frame, face),
            };
        }
        Ok(faces)
    }

    pub fn evaluate(&self, frame: &Arc<RgbaImage>, enrolled: &[f32]) -> Result<BiometricEvaluation, BiometricError> {
        self.evaluate_with_multi(frame, enrolled, &[])
    }

    pub fn evaluate_with_multi(
        &self, frame: &Arc<RgbaImage>, enrolled: &[f32], multi: &[Vec<f32>],
    ) -> Result<BiometricEvaluation, BiometricError> {
        let mut tracker = locked(&self.tracker);
        let start = Instant::now();
        let mut eval = BiometricEvaluation { timestamp: Local::now(), ..Default::default() };

        let t_det = Instant::now();
        let detected = self.detect_with_landmarks(frame);
        let faces = match detected {
            Ok(f) => tracker.select_and_track(Some(frame), f, frame.width, frame.height, start),
            Err(e) => return Err(BiometricError::Inference(format!("detection error: {e}"))),
        };
        eval.latency.detection = t_det.elapsed();

        if faces.is_empty() {
            eval.reason_code = "NO_FACE_DETECTED".into();
            eval.latency.total = start.elapsed();
            return Ok(eval);
        }
        let primary = faces[0].clone();
        eval.face_found = true;
        eval.face = Some(primary.clone());
        eval.all_faces = faces;

        let t_q = Instant::now();
        eval.quality = compute_quality(frame, &primary, &self.quality_thresholds);
        eval.latency.quality = t_q.elapsed();
        if !eval.quality.passed {
            eval.reason_code = format!("QUALITY_CHECK_FAILED: {}", eval.quality.reject_cause);
            eval.latency.total = start.elapsed();
            return Ok(eval);
        }

        let t_s = Instant::now();
        let spoof = self
            .spoofer
            .evaluate_liveness(frame, &primary)
            .map_err(|e| BiometricError::Inference(format!("anti-spoof error: {e}")))?;
        eval.latency.anti_spoof = t_s.elapsed();
        if !spoof.is_live {
            eval.reason_code = spoof_reason(&spoof);
            eval.anti_spoof = Some(spoof);
            eval.latency.total = start.elapsed();
            return Ok(eval);
        }
        eval.anti_spoof = Some(spoof);

        let t_a = Instant::now();
        let aligned = Arc::new(align_face_mesh(frame, &primary));
        eval.latency.alignment = t_a.elapsed();
        let t_r = Instant::now();
        let embedding = self
            .recognizer
            .extract_embedding(&aligned)
            .map_err(|e| BiometricError::Inference(format!("embedding extraction error: {e}")))?;
        eval.latency.recognition = t_r.elapsed();
        eval.aligned_face = Some(aligned);

        let threshold = compute_adaptive_threshold(self.recognition_threshold, &eval.quality);
        let mut rec = RecognitionResult { embedding, threshold, ..Default::default() };

        if !enrolled.is_empty() || !multi.is_empty() {
            let sim = best_match_similarity(&rec.embedding, enrolled, multi);
            rec.similarity = sim;
            rec.matched = sim >= threshold;
            if !rec.matched {
                eval.reason_code = format!("IDENTITY_MISMATCH: Similarity {sim:.2} < {threshold:.2}");
                eval.recognition = Some(rec);
                eval.latency.total = start.elapsed();
                return Ok(eval);
            }
        } else {
            rec.matched = true;
        }
        eval.recognition = Some(rec);
        eval.accepted = true;
        eval.reason_code = "PASSED".into();
        eval.frozen_frame = Some(Arc::clone(frame));
        eval.latency.total = start.elapsed();
        Ok(eval)
    }

    pub fn detect_faces(&self, frame: &RgbaImage) -> Result<Vec<DetectedFace>, BiometricError> {
        let mut tracker = locked(&self.tracker);
        let faces = self.detect_with_landmarks(frame)?;
        Ok(tracker.select_and_track(Some(frame), faces, frame.width, frame.height, Instant::now()))
    }

    pub fn track_live_frame(&self, frame: &RgbaImage, faces: &mut [DetectedFace]) {
        if faces.is_empty() || frame.is_empty() {
            return;
        }
        let mut live = locked(&self.live);
        let now = Instant::now();
        let LiveState { aligner, slots } = &mut *live;
        for (face, slot) in faces.iter_mut().zip(slots.iter_mut()) {
            for pass in 0..2 {
                let (ox, oy) = face.anchor();
                let old_span = landmark_span(face);
                let Ok(dense) = aligner.predict68(frame, face) else { break };
                if dense.len() < DENSE_LANDMARK_COUNT {
                    break;
                }
                face.dense = dense;
                let (nx, ny) = face.anchor();
                let (dx, dy) = (nx - ox, ny - oy);
                rescale_box(face, (ox, oy), (nx, ny), old_span, landmark_span(face));
                if pass == 1 || f64::from(dx).hypot(f64::from(dy)) <= f64::from(face.width) * 0.04 {
                    break;
                }
            }
            if face.has_dense() {
                smooth_live(slot, face, now);
            }
        }
    }

    pub fn extract_aligned_embedding(
        &self, frame: &RgbaImage, face: &DetectedFace,
    ) -> Result<(Vec<f32>, RgbaImage), BiometricError> {
        let _guard = locked(&self.tracker);
        let aligned = align_face_mesh(frame, face);
        let emb = self.recognizer.extract_embedding(&aligned)?;
        Ok((emb, aligned))
    }

    pub fn evaluate_liveness(&self, frame: &RgbaImage, face: &DetectedFace) -> Result<AntiSpoofResult, BiometricError> {
        let _guard = locked(&self.tracker);
        self.spoofer.evaluate_liveness(frame, face)
    }
}

fn spoof_reason(s: &AntiSpoofResult) -> String {
    if s.screen_spoof {
        format!("SPOOF_SUSPECTED: Screen Moire / glare artifact detected ({:.0}%)", s.texture_score * 100.0)
    } else if s.static_spoof {
        "SPOOF_SUSPECTED: Flat 2D photo translation detected (no 3D parallax)".into()
    } else {
        format!("SPOOF_SUSPECTED: Liveness {:.1}% < {:.1}%", s.score * 100.0, s.threshold * 100.0)
    }
}

fn landmark_span(face: &DetectedFace) -> f64 {
    let l = &face.landmarks;
    let eye = f64::from(l.left_eye.x - l.right_eye.x).hypot(f64::from(l.left_eye.y - l.right_eye.y));
    let em_x = 0.5 * f64::from(l.right_mouth.x + l.left_mouth.x - l.right_eye.x - l.left_eye.x);
    let em_y = 0.5 * f64::from(l.right_mouth.y + l.left_mouth.y - l.right_eye.y - l.left_eye.y);
    eye + em_x.hypot(em_y)
}

fn rescale_box(face: &mut DetectedFace, old_anchor: (f32, f32), new_anchor: (f32, f32), old_span: f64, new_span: f64) {
    let ratio = if old_span > 4.0 && new_span > 4.0 { (new_span / old_span).clamp(0.85, 1.18) as f32 } else { 1.0 };
    let rel_x = (face.x - old_anchor.0) * ratio;
    let rel_y = (face.y - old_anchor.1) * ratio;
    face.x = new_anchor.0 + rel_x;
    face.y = new_anchor.1 + rel_y;
    face.width *= ratio;
    face.height *= ratio;
}

fn smooth_live(slot: &mut LiveSlot, face: &mut DetectedFace, now: Instant) {
    let dt = slot.last.map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f64());
    slot.last = Some(now);
    let scale = f64::from(face.width).max(f64::from(face.height)).max(32.0);
    for (i, p) in face.dense.iter_mut().take(DENSE_LANDMARK_COUNT).enumerate() {
        *p = Point2D::new(
            slot.filters[i * 2].filter(f64::from(p.x), dt, scale) as f32,
            slot.filters[i * 2 + 1].filter(f64::from(p.y), dt, scale) as f32,
        );
    }
}

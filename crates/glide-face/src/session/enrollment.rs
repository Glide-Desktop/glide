use std::sync::Arc;

use crate::biometrics::calibration::{SELF_CHECK_HEADROOM, thresholds, verifier_score};
use crate::biometrics::{
    BiometricEvaluation, Pipeline, QualityMetrics, RgbaImage, cosine_similarity, weighted_average_embeddings,
};
use crate::profile::{Profile, Store};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EnrollmentStep {
    Frontal1 = 0,
    TurnLeft = 1,
    TurnRight = 2,
    Tilt = 3,
    Frontal2 = 4,
    Complete = 5,
    Failed = 6,
}

impl EnrollmentStep {
    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }

    fn next(self) -> Self {
        match self {
            Self::Frontal1 => Self::TurnLeft,
            Self::TurnLeft => Self::TurnRight,
            Self::TurnRight => Self::Tilt,
            Self::Tilt => Self::Frontal2,
            Self::Frontal2 | Self::Complete => Self::Complete,
            Self::Failed => Self::Failed,
        }
    }
}

const IDENTITY_GUARD_STRIKES: u32 = 3;

pub struct EnrollmentState {
    pub current_step: EnrollmentStep,
    pub instruction: String,
    pub hold_frames: i32,
    pub required_hold: i32,
    pub progress: f64,
    pub status_message: String,
    pub error_message: String,
    pub is_success: bool,
    pub is_done: bool,

    has_baseline: bool,
    base_yaw: f64,
    base_pitch: f64,
    first_turn: f64,

    step_candidates: Vec<Vec<f32>>,
    step_weights: Vec<f64>,
    best_step_face: Option<Arc<RgbaImage>>,
    best_step_score: f64,
    hold_track_id: i32,
    identity_strikes: u32,

    pub captured_embeddings: Vec<Vec<f32>>,
    pub captured_weights: Vec<f64>,
    pub captured_faces: Vec<Arc<RgbaImage>>,
    pub last_evaluation: Option<BiometricEvaluation>,
    pub template_hash: String,
    pub self_check_score: f64,

    store: Option<Store>,
}

impl EnrollmentState {
    #[must_use]
    pub fn new(store: Option<Store>) -> Self {
        Self {
            current_step: EnrollmentStep::Frontal1,
            instruction: "Step 1 of 5: Look directly at the camera".into(),
            hold_frames: 0,
            required_hold: 4,
            progress: 0.0,
            status_message: "Position your face in the center".into(),
            error_message: String::new(),
            is_success: false,
            is_done: false,
            has_baseline: false,
            base_yaw: 0.0,
            base_pitch: 0.0,
            first_turn: 0.0,
            step_candidates: Vec::new(),
            step_weights: Vec::new(),
            best_step_face: None,
            best_step_score: 0.0,
            hold_track_id: 0,
            identity_strikes: 0,
            captured_embeddings: Vec::with_capacity(5),
            captured_weights: Vec::with_capacity(5),
            captured_faces: Vec::with_capacity(5),
            last_evaluation: None,
            template_hash: String::new(),
            self_check_score: 0.0,
            store,
        }
    }

    pub fn process_frame(
        &mut self, frame: &Arc<RgbaImage>, pipe: &Pipeline,
    ) -> Result<(), crate::biometrics::BiometricError> {
        if self.is_done {
            return Ok(());
        }
        let eval = pipe.evaluate(frame, &[])?;
        self.process_evaluation(eval);
        Ok(())
    }

    pub fn process_evaluation(&mut self, eval: BiometricEvaluation) {
        if self.is_done {
            return;
        }
        let verdict = self.check_frame(&eval);
        if let Err(msg) = verdict {
            self.reset_step_hold(&msg);
            self.last_evaluation = Some(eval);
            return;
        }

        let embedding = eval.recognition.as_ref().map(|r| r.embedding.clone()).unwrap_or_default();
        if let (Some(anchor), false) = (self.captured_embeddings.first(), embedding.is_empty()) {
            let sim = cosine_similarity(&embedding, anchor);
            let guard = thresholds().identity_guard;
            if sim < guard {
                self.identity_strikes += 1;
                if self.identity_strikes >= IDENTITY_GUARD_STRIKES {
                    self.fail(format!(
                        "Identity changed during enrollment (similarity {sim:.2} < {guard:.2}). Please retry."
                    ));
                } else {
                    self.reset_step_hold("Different face detected - keep the same person in view");
                }
                self.last_evaluation = Some(eval);
                return;
            }
        }
        self.identity_strikes = 0;

        let track_id = eval.face.as_ref().map_or(0, |f| f.track_id);
        if self.hold_frames > 0 && track_id != self.hold_track_id {
            self.reset_step_hold("Tracking lost - hold still");
        }
        self.hold_track_id = track_id;

        self.hold_frames += 1;
        let weight = frame_weight(&eval);
        if !embedding.is_empty() {
            self.step_candidates.push(embedding.clone());
            self.step_weights.push(weight);
        }
        let frame_quality = eval.quality.blur_score * eval.quality.stability.max(0.5);
        if let Some(aligned) = &eval.aligned_face
            && (self.best_step_face.is_none() || frame_quality >= self.best_step_score)
        {
            self.best_step_face = Some(Arc::clone(aligned));
            self.best_step_score = frame_quality;
        }

        let step = self.current_step.index() as f64;
        self.progress = step / 5.0 + f64::from(self.hold_frames) / f64::from(self.required_hold) / 5.0;
        self.status_message = format!("Hold position... ({}/{})", self.hold_frames, self.required_hold);

        if self.hold_frames >= self.required_hold {
            if self.current_step == EnrollmentStep::Frontal1 && !self.has_baseline {
                self.has_baseline = true;
                self.base_yaw = eval.quality.pose_yaw;
                self.base_pitch = eval.quality.pose_pitch;
            } else if self.current_step == EnrollmentStep::TurnLeft {
                self.first_turn = eval.quality.pose_yaw - self.base_yaw;
            }
            let (emb, w) = self.aggregate_step_candidates(&embedding);
            self.captured_embeddings.push(emb);
            self.captured_weights.push(w);
            if let Some(face) = self.best_step_face.take().or_else(|| eval.aligned_face.clone()) {
                self.captured_faces.push(face);
            }
            self.reset_step_hold("");
            self.last_evaluation = Some(eval);
            self.advance_step();
            return;
        }
        self.last_evaluation = Some(eval);
    }

    fn check_frame(&self, eval: &BiometricEvaluation) -> Result<(), String> {
        if !eval.face_found {
            return Err("No face detected in camera view".into());
        }
        if !eval.quality.passed {
            return Err(eval.quality.reject_cause.clone());
        }
        if !eval.is_live() {
            return Err("Liveness check failed: real live face required".into());
        }
        let (ok, hint) = self.validate_pose_for_step(self.current_step, &eval.quality);
        if !ok {
            return Err(hint.into());
        }
        Ok(())
    }

    fn fail(&mut self, msg: String) {
        self.current_step = EnrollmentStep::Failed;
        self.is_done = true;
        self.error_message = msg;
    }

    fn reset_step_hold(&mut self, msg: &str) {
        self.hold_frames = 0;
        self.step_candidates.clear();
        self.step_weights.clear();
        self.best_step_face = None;
        self.best_step_score = 0.0;
        if !msg.is_empty() {
            self.status_message = msg.to_string();
        }
    }

    fn aggregate_step_candidates(&self, fallback: &[f32]) -> (Vec<f32>, f64) {
        if self.step_candidates.is_empty() {
            return (fallback.to_vec(), 1.0);
        }
        let centroid = weighted_average_embeddings(&self.step_candidates, Some(&self.step_weights));
        let (kept, kept_w): (Vec<Vec<f32>>, Vec<f64>) = self
            .step_candidates
            .iter()
            .zip(&self.step_weights)
            .filter(|(c, _)| cosine_similarity(c, &centroid) >= 0.70)
            .map(|(c, w)| (c.clone(), *w))
            .unzip();
        let mean_w = |w: &[f64]| if w.is_empty() { 1.0 } else { w.iter().sum::<f64>() / w.len() as f64 };
        if kept.is_empty() {
            return (centroid, mean_w(&self.step_weights));
        }
        (weighted_average_embeddings(&kept, Some(&kept_w)), mean_w(&kept_w))
    }

    fn validate_pose_for_step(&self, step: EnrollmentStep, q: &QualityMetrics) -> (bool, &'static str) {
        let rel_yaw = q.pose_yaw - self.base_yaw;
        let rel_pitch = q.pose_pitch - self.base_pitch;
        match step {
            EnrollmentStep::Frontal1 => {
                if q.pose_yaw.abs() > 16.0 || q.pose_pitch.abs() > 20.0 {
                    return (false, "Look straight at the camera");
                }
                (true, "")
            }
            EnrollmentStep::TurnLeft => {
                if rel_yaw.abs() < 6.0 {
                    return (false, "Turn your head slightly to the LEFT");
                }
                if q.pose_yaw.abs() > 38.0 {
                    return (false, "Turn back slightly (too far)");
                }
                (true, "")
            }
            EnrollmentStep::TurnRight => {
                if rel_yaw.abs() < 6.0 {
                    return (false, "Turn your head slightly to the RIGHT");
                }
                if self.first_turn != 0.0 && rel_yaw * self.first_turn > 0.0 {
                    return (false, "Turn your head to the OTHER side (RIGHT)");
                }
                if q.pose_yaw.abs() > 38.0 {
                    return (false, "Turn back slightly (too far)");
                }
                (true, "")
            }
            EnrollmentStep::Tilt => {
                if rel_pitch.abs() < 4.5 && q.pose_pitch.abs() < 8.0 {
                    return (false, "Tilt your head slightly UP or DOWN");
                }
                if q.pose_pitch.abs() > 34.0 {
                    return (false, "Too much tilt, adjust slightly");
                }
                (true, "")
            }
            EnrollmentStep::Frontal2 => {
                if rel_yaw.abs() > 14.0 || rel_pitch.abs() > 16.0 {
                    return (false, "Look straight forward again to confirm");
                }
                (true, "")
            }
            _ => (false, ""),
        }
    }

    #[must_use]
    pub fn target_pose_angles(&self) -> (f64, f64, f64, f64) {
        let (by, bp) = (self.base_yaw, self.base_pitch);
        match self.current_step {
            EnrollmentStep::Frontal1 => (0.0, 0.0, 14.0, 16.0),
            EnrollmentStep::TurnLeft => (by + 16.0, bp, 12.0, 18.0),
            EnrollmentStep::TurnRight if self.first_turn < 0.0 => (by + 16.0, bp, 12.0, 18.0),
            EnrollmentStep::TurnRight => (by - 16.0, bp, 12.0, 18.0),
            EnrollmentStep::Tilt => (by, bp - 12.0, 16.0, 12.0),
            EnrollmentStep::Frontal2 => (by, bp, 12.0, 14.0),
            _ => (0.0, 0.0, 14.0, 14.0),
        }
    }

    fn advance_step(&mut self) {
        self.current_step = self.current_step.next();
        self.progress = self.current_step.index() as f64 / 5.0;
        match self.current_step {
            EnrollmentStep::TurnLeft => self.instruction = "Step 2 of 5: Turn head slightly to the LEFT".into(),
            EnrollmentStep::TurnRight => self.instruction = "Step 3 of 5: Turn head slightly to the RIGHT".into(),
            EnrollmentStep::Tilt => self.instruction = "Step 4 of 5: Tilt head slightly UP or DOWN".into(),
            EnrollmentStep::Frontal2 => self.instruction = "Step 5 of 5: Look straight forward again to confirm".into(),
            EnrollmentStep::Complete => self.finalize(),
            _ => {}
        }
    }

    fn finalize(&mut self) {
        if self.captured_embeddings.len() < 5 {
            return self.fail("Insufficient biometric samples captured".into());
        }
        let caps = &self.captured_embeddings;

        let sim = cosine_similarity(&caps[0], &caps[4]);
        if sim < 0.60 {
            return self.fail(format!("Consistency check failed (frontal similarity {sim:.2} < 0.60). Please retry."));
        }

        let master = weighted_average_embeddings(caps, Some(&self.captured_weights));

        for (i, step) in caps.iter().enumerate() {
            let sim_centroid = cosine_similarity(step, &master);
            let sim_frontal = cosine_similarity(step, &caps[0]).max(cosine_similarity(step, &caps[4]));
            if sim_centroid < 0.65 || sim_frontal < thresholds().step_frontal {
                return self.fail(format!(
                    "Step {} failed identity consistency check (centroid {sim_centroid:.2}, frontal {sim_frontal:.2}). Please retry.",
                    i + 1
                ));
            }
        }

        let self_check = frontal_self_check(caps, &self.captured_weights);
        let required = thresholds().match_threshold + SELF_CHECK_HEADROOM;
        if self_check < required {
            return self.fail(format!(
                "Enrollment self-check failed (frontal score {self_check:.2} < required {required:.2}). Improve lighting and retry."
            ));
        }
        self.self_check_score = self_check;

        let mut prof = Profile {
            version: 2,
            self_check_score: Some(self_check),
            created_at: Some(chrono::Local::now()),
            embedding: master,
            multi_embeddings: caps.clone(),
            num_samples: caps.len() as i32,
            sample_weights: self.captured_weights.clone(),
            ..Default::default()
        };
        let Some(store) = &self.store else {
            return self.fail("No profile store configured".into());
        };
        if let Err(e) = store.save(&mut prof) {
            return self.fail(format!("Failed to save encrypted profile: {e}"));
        }
        self.is_done = true;
        self.is_success = true;
        self.progress = 1.0;
        self.instruction = "Profile successfully enrolled and securely saved!".into();
        self.status_message = format!("Template ID: {}", prof.template_hash);
        self.template_hash = prof.template_hash;
    }
}

#[must_use]
pub fn frontal_self_check(captures: &[Vec<f32>], weights: &[f64]) -> f64 {
    let floor = thresholds().centroid_floor;
    [0usize, captures.len().saturating_sub(1)]
        .into_iter()
        .map(|held_out| {
            let (rest, rest_w): (Vec<Vec<f32>>, Vec<f64>) = captures
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != held_out)
                .map(|(i, c)| (c.clone(), weights.get(i).copied().unwrap_or(1.0)))
                .unzip();
            let centroid = weighted_average_embeddings(&rest, Some(&rest_w));
            verifier_score(&captures[held_out], &centroid, &rest, floor)
        })
        .fold(f64::MAX, f64::min)
}

fn frame_weight(eval: &BiometricEvaluation) -> f64 {
    let sharp = (eval.quality.blur_score / 150.0).clamp(0.35, 1.0);
    let stable = eval.quality.stability.clamp(0.5, 1.0);
    let live = eval.anti_spoof.as_ref().map_or(1.0, |a| a.neural_score.clamp(0.5, 1.0));
    sharp * stable * live
}

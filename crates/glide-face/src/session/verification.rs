use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::RngCore;

use super::report::{DiagnosticReport, build_report};
use crate::biometrics::{
    BiometricEvaluation, Pipeline, RgbaImage, average_embeddings, best_match_similarity, cosine_similarity,
};
use crate::profile::Profile;

pub struct VerificationSession {
    pub session_id: String,
    pub profile: Profile,
    pub consensus_frames: i32,
    pub required_consensus: i32,
    pub start_time: Instant,
    pub timeout: Duration,

    pub is_done: bool,
    pub is_success: bool,
    pub reason_code: String,
    pub status_text: String,

    pub last_evaluation: Option<BiometricEvaluation>,
    pub winning_evaluation: Option<BiometricEvaluation>,
    pub frozen_snapshot: Option<Arc<RgbaImage>>,
    pub report: Option<DiagnosticReport>,

    last_accepted_embedding: Vec<f32>,
    consensus_embeddings: Vec<Vec<f32>>,
    consensus_thresholds: Vec<f64>,
    consensus_track_id: i32,
    last_frame: Option<Arc<RgbaImage>>,
}

impl VerificationSession {
    #[must_use]
    pub fn new(profile: Profile) -> Self {
        let mut id = [0u8; 8];
        rand::rngs::OsRng.fill_bytes(&mut id);
        Self {
            session_id: hex::encode(id),
            profile,
            consensus_frames: 0,
            required_consensus: 5,
            start_time: Instant::now(),
            timeout: Duration::from_secs(10),
            is_done: false,
            is_success: false,
            reason_code: String::new(),
            status_text: "Align your face with the camera...".into(),
            last_evaluation: None,
            winning_evaluation: None,
            frozen_snapshot: None,
            report: None,
            last_accepted_embedding: Vec::new(),
            consensus_embeddings: Vec::new(),
            consensus_thresholds: Vec::new(),
            consensus_track_id: 0,
            last_frame: None,
        }
    }

    pub fn process_frame(
        &mut self, frame: &Arc<RgbaImage>, pipe: &Pipeline,
    ) -> Result<(), crate::biometrics::BiometricError> {
        if self.is_done {
            return Ok(());
        }
        let eval = pipe.evaluate_with_multi(frame, &self.profile.embedding, &self.profile.multi_embeddings)?;
        self.process_evaluation(Some(frame), eval);
        Ok(())
    }

    pub fn check_timeout(&mut self) -> bool {
        if self.is_done || self.start_time.elapsed() <= self.timeout {
            return false;
        }
        self.is_done = true;
        self.is_success = false;
        self.reason_code = match &self.last_evaluation {
            Some(e) if e.face_found && !e.reason_code.is_empty() && e.reason_code != "PASSED" => e.reason_code.clone(),
            _ => "VERIFICATION_TIMEOUT".into(),
        };
        self.status_text = "Verification timed out. Face not matched.".into();
        if self.frozen_snapshot.is_none() {
            self.frozen_snapshot.clone_from(&self.last_frame);
        }
        self.finalize_report();
        true
    }

    fn reset_consensus(&mut self) {
        self.consensus_frames = 0;
        self.consensus_embeddings.clear();
        self.consensus_thresholds.clear();
    }

    pub fn process_evaluation(&mut self, frame: Option<&Arc<RgbaImage>>, mut eval: BiometricEvaluation) {
        if self.is_done {
            return;
        }
        if let Some(f) = frame {
            self.last_frame = Some(Arc::clone(f));
        }

        if !self.profile.multi_embeddings.is_empty() {
            let live = eval.is_live();
            let quality_ok = eval.quality.passed;
            if let Some(rec) = eval.recognition.as_mut().filter(|r| !r.embedding.is_empty()) {
                let sim =
                    best_match_similarity(&rec.embedding, &self.profile.embedding, &self.profile.multi_embeddings);
                if sim > rec.similarity {
                    rec.similarity = sim;
                    if sim >= rec.threshold && quality_ok && live {
                        rec.matched = true;
                        eval.accepted = true;
                        eval.reason_code = "PASSED".into();
                    }
                }
            }
        }

        if eval.accepted && (eval.quality.pose_yaw.abs() > 25.0 || eval.quality.pose_pitch.abs() > 25.0) {
            eval.accepted = false;
            eval.reason_code = "QUALITY_CHECK_FAILED: Look straight at the camera".into();
        }

        if self.start_time.elapsed() > self.timeout {
            self.is_done = true;
            self.is_success = false;
            self.reason_code = if eval.face_found && !eval.reason_code.is_empty() && eval.reason_code != "PASSED" {
                eval.reason_code.clone()
            } else {
                "VERIFICATION_TIMEOUT".into()
            };
            self.status_text = "Verification timed out. Face not matched.".into();
            self.frozen_snapshot = frame.cloned();
            self.last_evaluation = Some(eval);
            self.finalize_report();
            return;
        }

        if eval.accepted
            && !self.last_accepted_embedding.is_empty()
            && let Some(rec) = &eval.recognition
            && cosine_similarity(&rec.embedding, &self.last_accepted_embedding) < 0.72
        {
            eval.accepted = false;
            eval.reason_code = "QUALITY_CHECK_FAILED: Hold still (embedding variance too high)".into();
        }

        if eval.accepted {
            let track_id = eval.face.as_ref().map_or(0, |f| f.track_id);
            if self.consensus_frames > 0 && track_id != self.consensus_track_id {
                self.reset_consensus();
            }
            self.consensus_track_id = track_id;

            if let Some(rec) = &eval.recognition
                && !rec.embedding.is_empty()
            {
                self.last_accepted_embedding.clone_from(&rec.embedding);
                self.consensus_embeddings.push(rec.embedding.clone());
                self.consensus_thresholds.push(rec.threshold);
            }
            self.consensus_frames += 1;
            self.status_text =
                format!("Verifying identity... ({}/{} frames)", self.consensus_frames, self.required_consensus);

            if self.consensus_frames >= self.required_consensus {
                if let Some((fused_sim, th)) = self.fused_similarity()
                    && fused_sim < th
                {
                    self.reset_consensus();
                    self.status_text = format!("IDENTITY_MISMATCH: Fused similarity {fused_sim:.2} < {th:.2}");
                    self.last_evaluation = Some(eval);
                    return;
                }
                self.is_done = true;
                self.is_success = true;
                self.reason_code = "PASSED".into();
                self.status_text = "Identity verified successfully!".into();
                self.frozen_snapshot = frame.cloned();
                self.winning_evaluation = Some(eval.clone());
                self.last_evaluation = Some(eval);
                self.finalize_report();
                return;
            }
        } else {
            let spoof = eval.anti_spoof.as_ref().is_some_and(|a| !a.is_live);
            let mismatch = eval.recognition.as_ref().is_some_and(|r| !r.matched);
            if spoof || mismatch {
                self.reset_consensus();
            } else if self.consensus_frames > 0 {
                self.consensus_frames -= 1;
                self.consensus_embeddings.pop();
                self.consensus_thresholds.pop();
            }
            self.status_text = if eval.face_found {
                eval.reason_code.clone()
            } else {
                "Position your face inside the camera view".into()
            };
        }
        self.last_evaluation = Some(eval);
    }

    fn fused_similarity(&self) -> Option<(f64, f64)> {
        if self.consensus_embeddings.len() < 2 {
            return None;
        }
        let fused = average_embeddings(&self.consensus_embeddings);
        let sim = best_match_similarity(&fused, &self.profile.embedding, &self.profile.multi_embeddings);
        let th = self.consensus_thresholds.iter().sum::<f64>() / self.consensus_thresholds.len() as f64;
        Some((sim, th))
    }

    fn finalize_report(&mut self) {
        let eval = self.winning_evaluation.as_ref().or(self.last_evaluation.as_ref());
        self.report = Some(build_report(
            &self.session_id,
            self.is_success,
            &self.reason_code,
            self.consensus_frames,
            self.required_consensus,
            eval,
            &self.profile.template_hash,
        ));
    }
}

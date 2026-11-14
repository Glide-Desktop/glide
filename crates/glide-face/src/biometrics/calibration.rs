use std::sync::LazyLock;

use super::recognizer::cosine_similarity;

pub mod data {
    include!("calibration_data.rs");
}

pub const TARGET_FAR_MATCH: f64 = 5e-7;
pub const TARGET_FAR_CENTROID_FLOOR: f64 = 1e-5;
pub const TARGET_FAR_IDENTITY_GUARD: f64 = 1e-4;
pub const TARGET_FAR_STEP_FRONTAL: f64 = 1e-6;
pub const SELF_CHECK_HEADROOM: f64 = 0.06;

#[must_use]
pub fn score_threshold_for_far(far: f64) -> f64 {
    (far.log10() - data::SCORE_TAIL_INTERCEPT) / data::SCORE_TAIL_SLOPE
}

#[must_use]
pub fn pair_threshold_for_far(far: f64) -> f64 {
    (far.log10() - data::PAIR_TAIL_INTERCEPT) / data::PAIR_TAIL_SLOPE
}

#[must_use]
pub fn far_at_score_threshold(threshold: f64) -> f64 {
    10f64.powf(data::SCORE_TAIL_INTERCEPT + data::SCORE_TAIL_SLOPE * threshold)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub match_threshold: f64,
    pub centroid_floor: f64,
    pub identity_guard: f64,
    pub step_frontal: f64,
}

static THRESHOLDS: LazyLock<Thresholds> = LazyLock::new(|| Thresholds {
    match_threshold: score_threshold_for_far(TARGET_FAR_MATCH),
    centroid_floor: pair_threshold_for_far(TARGET_FAR_CENTROID_FLOOR),
    identity_guard: pair_threshold_for_far(TARGET_FAR_IDENTITY_GUARD),
    step_frontal: pair_threshold_for_far(TARGET_FAR_STEP_FRONTAL),
});

#[must_use]
pub fn thresholds() -> &'static Thresholds {
    &THRESHOLDS
}

#[must_use]
pub fn default_match_threshold() -> f64 {
    thresholds().match_threshold
}

#[must_use]
pub fn verifier_score(live: &[f32], centroid: &[f32], multi: &[Vec<f32>], floor: f64) -> f64 {
    let sim_centroid = cosine_similarity(live, centroid);
    if multi.is_empty() {
        return sim_centroid;
    }
    if !centroid.is_empty() && sim_centroid < floor {
        return sim_centroid;
    }
    let best_pose =
        multi.iter().filter(|m| m.len() == live.len()).map(|m| cosine_similarity(live, m)).fold(-1.0f64, f64::max);
    if best_pose < 0.0 {
        return sim_centroid;
    }
    if centroid.is_empty() {
        return best_pose;
    }
    (0.50 * best_pose + 0.50 * sim_centroid).min(sim_centroid + 0.05).max(sim_centroid)
}

use glide_face::biometrics::calibration::{
    SELF_CHECK_HEADROOM, TARGET_FAR_MATCH, data, far_at_score_threshold, pair_threshold_for_far,
    score_threshold_for_far, thresholds,
};
use glide_face::biometrics::{EMBEDDING_DIM, RECOGNIZER_ID, cosine_similarity};
use glide_face::session::enrollment::frontal_self_check;

const _: () = assert!(TARGET_FAR_MATCH < data::SCORE_TAIL_MEASURED_FAR, "the policy target is an extrapolation");

#[test]
fn thresholds_follow_from_the_far_policy() {
    let t = thresholds();
    assert_eq!(data::MEASURED_RECOGNIZER, RECOGNIZER_ID, "re-run the calibration stand after changing the model");
    assert!((far_at_score_threshold(t.match_threshold) / TARGET_FAR_MATCH - 1.0).abs() < 1e-9);
    assert!((score_threshold_for_far(1e-4) - 0.343).abs() < 0.005, "fit must reproduce the measured 1e-4 point");
    assert!((pair_threshold_for_far(1e-4) - 0.335).abs() < 0.005);
    assert!(score_threshold_for_far(1e-7) > t.match_threshold && t.match_threshold > score_threshold_for_far(1e-5));
    assert!(t.step_frontal > t.centroid_floor && t.centroid_floor > t.identity_guard);
    assert!(
        (t.match_threshold - 0.50).abs() < 0.02,
        "moving the match threshold {} is a security change",
        t.match_threshold
    );
}

fn captures(noise: f32, seed: u64) -> Vec<Vec<f32>> {
    let mut s = seed;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        ((s >> 11) as f64 / (1u64 << 53) as f64) as f32 - 0.5
    };
    let base: Vec<f32> = (0..EMBEDDING_DIM).map(|_| next()).collect();
    (0..5)
        .map(|_| {
            let v: Vec<f32> = base.iter().map(|b| b + noise * next()).collect();
            let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            v.iter().map(|x| x / n).collect()
        })
        .collect()
}

#[test]
fn enrollment_self_check_is_leave_one_out() {
    let required = thresholds().match_threshold + SELF_CHECK_HEADROOM;
    let w = [1.0; 5];

    let good = captures(0.7, 11);
    assert!(cosine_similarity(&good[0], &good[4]) < 0.95, "captures must be distinct");
    let score = frontal_self_check(&good, &w);
    assert!(score >= required && score < 0.999, "score {score}");

    let mut bad = good.clone();
    bad[4] = captures(0.7, 99)[0].clone();
    assert!(frontal_self_check(&bad, &w) < required, "a frontal capture unlike the rest must fail");

    let mut other = good.clone();
    other[0] = good[1].clone();
    assert!(frontal_self_check(&other, &w) >= required, "the held-out capture never scores itself");
}

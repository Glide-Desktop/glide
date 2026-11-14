use std::sync::Arc;
use std::time::Duration;

use glide_face::biometrics::*;
use glide_face::profile::{Profile, Store};
use glide_face::session::*;

use crate::pipeline;

fn sample() -> Arc<RgbaImage> {
    Arc::new(crate::sample())
}

const STEP_POSES: [(f64, f64); 5] = [(0.0, 5.0), (-14.0, 5.0), (14.0, 5.0), (0.0, 15.0), (1.0, 5.0)];

fn with_pose(base: &BiometricEvaluation, yaw: f64, pitch: f64) -> BiometricEvaluation {
    let mut ev = base.clone();
    ev.quality.pose_yaw = yaw;
    ev.quality.pose_pitch = pitch;
    ev
}

#[test]
fn enrollment_and_verification() {
    let Some(pipe) = pipeline() else { return };
    let img = sample();
    let faces = pipe.detect_faces(&img).unwrap();
    let (emb, _) = pipe.extract_aligned_embedding(&img, &faces[0]).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let store = Store::new_test(dir.path()).unwrap();
    let mut prof = Profile { embedding: emb.clone(), num_samples: 1, ..Default::default() };
    store.save(&mut prof).unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(loaded.template_hash, prof.template_hash);
    assert!(cosine_similarity(&emb, &loaded.embedding) >= 0.999);

    let base = pipe.evaluate(&img, &[]).unwrap();
    assert!(base.accepted, "{}", base.reason_code);

    let mut wiz = EnrollmentState::new(Some(store.clone()));
    for (step, (yaw, pitch)) in STEP_POSES.iter().enumerate() {
        for _ in 0..wiz.required_hold {
            wiz.process_evaluation(with_pose(&base, *yaw, *pitch));
        }
        if step < 4 {
            assert_eq!(wiz.current_step.index(), step + 1, "step {} did not advance: {}", step + 1, wiz.status_message);
        }
    }
    assert!(wiz.is_done && wiz.is_success, "wizard failed: {}", wiz.error_message);
    assert_eq!(wiz.captured_faces.len(), 5);
    let enrolled = store.load().unwrap();
    assert_eq!(enrolled.multi_embeddings.len(), 5);
    assert_eq!(enrolled.sample_weights.len(), 5);

    let mut ver = VerificationSession::new(enrolled);
    for _ in 0..ver.required_consensus {
        ver.process_frame(&img, &pipe).unwrap();
    }
    assert!(ver.is_done && ver.is_success, "verification failed: {}", ver.reason_code);
    let rep = ver.report.as_ref().unwrap();
    assert_eq!(rep.verdict, "PASS");
    assert!(ver.frozen_snapshot.is_some());

    let mut bad = EnrollmentState::new(Some(store.clone()));
    let impostor: Vec<f32> = emb.iter().map(|v| -v).collect();
    for (step, (yaw, pitch)) in STEP_POSES.iter().enumerate() {
        for _ in 0..bad.required_hold {
            let mut ev = with_pose(&base, *yaw, *pitch);
            if step == 1 {
                ev.recognition.as_mut().unwrap().embedding = impostor.clone();
            }
            bad.process_evaluation(ev);
        }
    }
    assert!(bad.is_done && !bad.is_success, "impostor enrollment must fail");
}

#[test]
fn verification_rejects_track_swap_mid_consensus() {
    let Some(pipe) = pipeline() else { return };
    let img = sample();
    let base = pipe.evaluate(&img, &[]).unwrap();
    let emb = base.recognition.as_ref().unwrap().embedding.clone();
    let profile = Profile { embedding: emb, template_hash: "h".into(), ..Default::default() };

    let matched = |track: i32| {
        let mut ev = base.clone();
        let r = ev.recognition.as_mut().unwrap();
        r.similarity = 0.95;
        r.matched = true;
        ev.face.as_mut().unwrap().track_id = track;
        ev
    };
    let mut ver = VerificationSession::new(profile);
    for _ in 0..3 {
        ver.process_evaluation(None, matched(1));
    }
    assert_eq!(ver.consensus_frames, 3);
    ver.process_evaluation(None, matched(2));
    assert_eq!(ver.consensus_frames, 1, "consensus must restart on a track change");
    for _ in 0..4 {
        ver.process_evaluation(None, matched(2));
    }
    assert!(ver.is_done && ver.is_success);
}

#[test]
fn verification_timeout_without_frames() {
    let profile =
        Profile { version: 1, embedding: vec![0.0; 128], template_hash: "mock-hash".into(), ..Default::default() };
    let mut sess = VerificationSession::new(profile);
    sess.timeout = Duration::from_millis(10);
    std::thread::sleep(Duration::from_millis(15));
    assert!(!sess.is_done, "not done before processing");
    assert!(sess.check_timeout());
    assert!(sess.is_done && !sess.is_success);
    assert_eq!(sess.reason_code, "VERIFICATION_TIMEOUT");
    assert_eq!(sess.report.as_ref().unwrap().verdict, "FAIL");
}

#[test]
fn report_generation() {
    let mut eval =
        BiometricEvaluation { face_found: true, accepted: true, reason_code: "PASSED".into(), ..Default::default() };
    eval.quality = QualityMetrics {
        blur_score: 120.5,
        brightness: 110.0,
        contrast: 45.2,
        face_size: 180.0,
        pose_yaw: -2.5,
        pose_pitch: 1.2,
        pose_roll: 0.8,
        passed: true,
        ..Default::default()
    };
    eval.anti_spoof = Some(AntiSpoofResult {
        score: 0.96,
        class_probs: [0.02, 0.96, 0.02],
        threshold: 0.70,
        is_live: true,
        ..Default::default()
    });
    eval.recognition =
        Some(RecognitionResult { similarity: 0.85, threshold: 0.60, matched: true, ..Default::default() });
    eval.latency.detection = Duration::from_millis(18);
    eval.latency.total = Duration::from_millis(41);

    let rep = build_report("test-sess-123", true, "PASSED", 5, 5, Some(&eval), "abc123hash");
    let json = rep.to_json().unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["verdict"], "PASS");
    assert_eq!(v["latencies"]["detectionMs"], 18.0);
    assert_eq!(v["system"]["arch"], if cfg!(target_arch = "x86_64") { "amd64" } else { std::env::consts::ARCH });
    assert!(v["metrics"].get("faceBox").is_none());

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rep.json");
    let saved = rep.save_to_file(path.to_str().unwrap()).unwrap();
    assert_eq!(saved, path);
    let mode = std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&path).unwrap().permissions()) & 0o777;
    assert_eq!(mode, 0o600);

    let rep = build_report("x", false, "IDENTITY_MISMATCH: Similarity 0.19 < 0.60", 1, 1, None, "");
    assert!(rep.to_json().unwrap().contains("\\u003c"));
}

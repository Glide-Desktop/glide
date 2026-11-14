use std::sync::Arc;

use base64::Engine;
use glide_face::biometrics::align::align_face;
use glide_face::biometrics::landmark_onnx::Landmark68Aligner;
use glide_face::biometrics::yunet::YuNetDetector;
use glide_face::biometrics::*;
use serde_json::Value;

use crate::{SAMPLE_GOLDEN, SAMPLE_RGBA, fixtures_present};

fn golden() -> Value {
    serde_json::from_str(&std::fs::read_to_string(SAMPLE_GOLDEN).unwrap()).unwrap()
}

fn go_pixels() -> RgbaImage {
    RgbaImage::from_raw(320, 400, std::fs::read(SAMPLE_RGBA).unwrap())
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn assert_close(name: &str, got: f64, want: f64, tol: f64) {
    assert!((got - want).abs() <= tol, "{name}: got {got}, want {want} (tol {tol})");
}

fn assert_points(name: &str, got: &[Point2D], want: &Value, tol: f64) {
    let want = want.as_array().unwrap();
    assert_eq!(got.len(), want.len(), "{name}: length");
    let max = got
        .iter()
        .zip(want)
        .map(|(g, w)| (f64::from(g.x) - f(&w["x"])).abs().max((f64::from(g.y) - f(&w["y"])).abs()))
        .fold(0.0, f64::max);
    assert!(max <= tol, "{name}: max point deviation {max} > {tol}");
}

fn assert_face(name: &str, got: &DetectedFace, want: &Value, tol: f64) {
    assert_close(&format!("{name}.x"), f64::from(got.x), f(&want["x"]), tol);
    assert_close(&format!("{name}.y"), f64::from(got.y), f(&want["y"]), tol);
    assert_close(&format!("{name}.w"), f64::from(got.width), f(&want["width"]), tol);
    assert_close(&format!("{name}.h"), f64::from(got.height), f(&want["height"]), tol);
    assert_close(&format!("{name}.score"), f64::from(got.score), f(&want["score"]), 1e-4);
    let l = &want["landmarks"];
    let pts = got.landmarks.as_array();
    for (i, k) in ["rightEye", "leftEye", "noseTip", "rightMouth", "leftMouth"].iter().enumerate() {
        assert_close(&format!("{name}.{k}.x"), f64::from(pts[i].x), f(&l[k]["x"]), tol);
        assert_close(&format!("{name}.{k}.y"), f64::from(pts[i].y), f(&l[k]["y"]), tol);
    }
}

#[test]
fn detector_and_landmarks_match_go() {
    if !fixtures_present() {
        return;
    }
    let g = golden();
    let img = go_pixels();
    let det = YuNetDetector::new().unwrap();
    let mut raw = det.detect(&img).unwrap();
    assert_eq!(raw.len(), g["detectRaw"].as_array().unwrap().len());

    let mut heuristic = raw[0].clone();
    let dense_h = extract_dense_landmarks(&img, &mut heuristic);
    assert_face("detect[0]", &heuristic, &g["detectRaw"][0], 1e-3);
    assert_points("heuristic", &dense_h, &g["heuristicDense"], 1e-3);

    let al = Landmark68Aligner::new().unwrap();
    let dense = al.predict68(&img, &mut raw[0]).unwrap();
    assert_points("neural68", &dense, &g["denseRaw"][0], 1e-3);
}

#[test]
fn pipeline_matches_go() {
    if !fixtures_present() {
        return;
    }
    let g = golden();
    let img = go_pixels();
    let pipe = Pipeline::new().unwrap();
    let faces = pipe.detect_faces(&img).unwrap();
    let face = &faces[0];
    assert_face("faces[0]", face, &g["faces"][0], 1e-3);
    assert_points("dense0", &face.dense, &g["dense0"], 1e-3);
    assert_eq!(i64::from(face.track_age), g["faces"][0]["trackAge"].as_i64().unwrap());

    let q = compute_quality(&img, face, &DEFAULT_QUALITY_THRESHOLDS);
    let gq = &g["quality"];
    for (name, got) in [
        ("blurScore", q.blur_score),
        ("brightness", q.brightness),
        ("contrast", q.contrast),
        ("faceSize", q.face_size),
        ("eyeOpenness", q.eye_openness),
        ("stability", q.stability),
    ] {
        assert_close(name, got, f(&gq[name]), 1e-4);
    }
    for (name, got) in [("poseYaw", q.pose_yaw), ("posePitch", q.pose_pitch), ("poseRoll", q.pose_roll)] {
        assert_close(name, got, f(&gq[name]), 2.5);
    }
    assert_close("symmetry", q.symmetry, f(&gq["symmetry"]), 0.02);
    assert_eq!(q.passed, gq["passed"].as_bool().unwrap());

    let anti = pipe.evaluate_liveness(&img, face).unwrap();
    for i in 0..3 {
        assert_close("classProbs", anti.class_probs[i], f(&g["anti"]["classProbs"][i]), 1e-5);
    }
    assert_close("textureScore", anti.texture_score, f(&g["anti"]["textureScore"]), 1e-9);

    let aligned = align_face(&img, &face.landmarks);
    let go_aligned = base64::engine::general_purpose::STANDARD.decode(g["alignedPix"].as_str().unwrap()).unwrap();
    let max_px = aligned.pix.iter().zip(&go_aligned).map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs()).max().unwrap();
    assert!(max_px <= 1, "aligned pixels differ by {max_px}");
    let (emb, _) = pipe.extract_aligned_embedding(&img, face).unwrap();
    assert_eq!(emb.len(), EMBEDDING_DIM);

    let pipe2 = Pipeline::new().unwrap();
    let ev = pipe2.evaluate(&Arc::new(img.clone()), &[]).unwrap();
    assert_eq!(ev.reason_code, g["eval"]["reasonCode"].as_str().unwrap());
    assert_eq!(ev.accepted, g["eval"]["accepted"].as_bool().unwrap());
    assert_eq!(ev.recognition.as_ref().unwrap().embedding.len(), EMBEDDING_DIM);

    let again = glide_face::biometrics::align::align_face_mesh(&img, ev.face.as_ref().unwrap());
    assert_eq!(again.pix, ev.aligned_face.as_ref().unwrap().pix);
}

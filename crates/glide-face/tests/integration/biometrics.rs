use std::sync::Arc;

use glide_face::biometrics::klt::{extract_gray_buffer, track_point_klt};
use glide_face::biometrics::*;

use crate::{pipeline, sample};

#[test]
fn biometrics_pipeline() {
    let Some(pipe) = pipeline() else { return };
    let img = sample();
    let (w, h) = (img.width as f32, img.height as f32);

    let faces = pipe.detect_faces(&img).unwrap();
    assert!(!faces.is_empty(), "expected a face in sample.jpg");
    let face = faces[0].clone();
    assert!(face.score >= 0.80, "score {}", face.score);

    let (emb, aligned) = pipe.extract_aligned_embedding(&img, &face).unwrap();
    assert_eq!(emb.len(), EMBEDDING_DIM);
    assert_eq!((aligned.width, aligned.height), (112, 112));
    assert!(cosine_similarity(&emb, &emb) >= 0.99);

    pipe.evaluate_liveness(&img, &face).unwrap();

    let avg = average_embeddings(&[emb.clone(), emb.clone()]);
    assert_eq!(avg.len(), EMBEDDING_DIM);
    assert!(cosine_similarity(&avg, &emb) >= 0.99);
    assert!(best_match_similarity(&emb, &avg, std::slice::from_ref(&emb)) >= 0.99);

    assert_eq!(DENSE_LANDMARK_COUNT, 68);
    assert_eq!(face.dense.len(), 68);
    for (i, p) in face.dense.iter().enumerate() {
        assert!(p.x > 0.0 && p.y > 0.0 && p.x < w && p.y < h, "landmark {i} out of bounds: {p:?}");
    }
    assert!((0.15..=0.55).contains(&face.ear), "EAR {}", face.ear);
    let faces2 = pipe.detect_faces(&img).unwrap();
    assert!(faces2[0].track_age >= 2);
    assert!(faces2[0].stability >= 0.95, "stability {}", faces2[0].stability);

    let mut dark = img.clone();
    for px in dark.pix.chunks_exact_mut(4) {
        for c in &mut px[..3] {
            *c = (f64::from(*c) * 0.60) as u8;
        }
        px[3] = 255;
    }
    let (dark_emb, _) = pipe.extract_aligned_embedding(&dark, &face).unwrap();
    let dark_sim = cosine_similarity(&emb, &dark_emb);
    assert!(dark_sim >= 0.92, "dark similarity {dark_sim}");

    let mut moire = img.clone();
    let stride = moire.stride();
    for y in 0..moire.height {
        for x in 0..moire.width {
            let delta: i32 = if (x + y) % 2 == 0 { -22 } else { 22 };
            for c in 0..3 {
                let i = y * stride + x * 4 + c;
                moire.pix[i] = (i32::from(moire.pix[i]) + delta).clamp(0, 255) as u8;
            }
        }
    }
    assert!(analyze_texture_and_screen_artifacts(&moire, &face).1, "moiré grid must be flagged");

    let (yaw, pitch, roll, err) = estimate_head_pose_3d(&face);
    assert!(err <= 0.25, "reprojection error {err}");
    for (n, v) in [("yaw", yaw), ("pitch", pitch), ("roll", roll)] {
        assert!((-10.0..=10.0).contains(&v), "{n} {v}");
    }

    let mut gray = Vec::new();
    let (gw, gh) = extract_gray_buffer(&img, &mut gray);
    let nose = face.landmarks.nose_tip;
    let (pt, conf) = track_point_klt(&gray, &gray, gw, gh, nose, nose, f64::from(face.width));
    assert!(conf > 0.1, "KLT confidence {conf}");
    assert!((pt.x - nose.x).abs() <= 1.0, "KLT drift");

    let (sx, sy) = (24usize, 16usize);
    let mut shifted = RgbaImage::new(img.width, img.height);
    for y in 0..img.height - sy {
        let src = &img.pix[y * stride..y * stride + (img.width - sx) * 4];
        let dst = (y + sy) * stride + sx * 4;
        shifted.pix[dst..dst + src.len()].copy_from_slice(src);
    }
    let mut live = vec![face.clone()];
    pipe.track_live_frame(&shifted, &mut live);
    let dx = live[0].dense[LM_NOSE_TIP].x - face.dense[LM_NOSE_TIP].x;
    let dy = live[0].dense[LM_NOSE_TIP].y - face.dense[LM_NOSE_TIP].y;
    assert!((dx - sx as f32).abs() <= 2.5 && (dy - sy as f32).abs() <= 2.5, "live lock moved by ({dx:.2}, {dy:.2})");
    assert!((live[0].x - face.x - sx as f32).abs() <= 3.0 && (live[0].y - face.y - sy as f32).abs() <= 3.0);
}

#[test]
fn live_lock_follows_scale_changes() {
    let Some(pipe) = pipeline() else { return };
    let img = sample();
    let face = pipe.detect_faces(&img).unwrap().remove(0);
    let s = 1.15f32;
    let (cx, cy) = (img.width as f32 / 2.0, img.height as f32 / 2.0);
    let mut zoomed = RgbaImage::new(img.width, img.height);
    for y in 0..img.height {
        for x in 0..img.width {
            let sx = ((x as f32 - cx) / s + cx) as usize;
            let sy = ((y as f32 - cy) / s + cy) as usize;
            let (d, o) = (y * img.stride() + x * 4, sy.min(img.height - 1) * img.stride() + sx.min(img.width - 1) * 4);
            zoomed.pix[d..d + 4].copy_from_slice(&img.pix[o..o + 4]);
        }
    }
    let mut live = vec![face.clone()];
    pipe.track_live_frame(&zoomed, &mut live);
    let ratio = live[0].width / face.width;
    assert!((ratio - s).abs() < 0.06, "box scale ratio {ratio}");
}

#[test]
fn full_evaluation_accepts_sample() {
    let Some(pipe) = pipeline() else { return };
    let ev = pipe.evaluate(&Arc::new(sample()), &[]).unwrap();
    assert!(ev.accepted, "{}", ev.reason_code);
    assert_eq!(ev.reason_code, "PASSED");
    assert!(ev.aligned_face.is_some() && ev.frozen_frame.is_some());
}

fn jitter(rng: &mut u64, sigma: f32) -> (f32, f32) {
    let mut next = || {
        *rng ^= *rng << 13;
        *rng ^= *rng >> 7;
        *rng ^= *rng << 17;
        ((*rng >> 11) as f64 / (1u64 << 53) as f64) as f32
    };
    let (u1, u2) = (next().max(1e-6), next());
    let r = (-2.0 * u1.ln()).sqrt() * sigma;
    (r * (std::f32::consts::TAU * u2).cos(), r * (std::f32::consts::TAU * u2).sin())
}

#[test]
fn mesh_assisted_alignment_is_more_stable_than_pupils_alone() {
    use glide_face::biometrics::align::{align_face, alignment_keypoints};
    use glide_face::biometrics::recognizer::FaceRecognizer;
    let Some(pipe) = pipeline() else { return };
    let rec = FaceRecognizer::new().unwrap();
    let img = sample();
    let face = pipe.detect_faces(&img).unwrap().remove(0);
    let base_old = rec.extract_embedding(&align_face(&img, &face.landmarks)).unwrap();
    let base_new = rec.extract_embedding(&align_face(&img, &alignment_keypoints(&face))).unwrap();
    assert!(cosine_similarity(&base_old, &base_new) > 0.97);

    let (mut s_old, mut s_new, mut rng) = (0.0, 0.0, 0x9E37_79B9_7F4A_7C15_u64);
    for _ in 0..20 {
        let mut f = face.clone();
        for p in [&mut f.landmarks.right_eye, &mut f.landmarks.left_eye].into_iter().chain(f.dense.iter_mut()) {
            let (dx, dy) = jitter(&mut rng, 2.0);
            p.x += dx;
            p.y += dy;
        }
        s_old += cosine_similarity(&rec.extract_embedding(&align_face(&img, &f.landmarks)).unwrap(), &base_old);
        s_new +=
            cosine_similarity(&rec.extract_embedding(&align_face(&img, &alignment_keypoints(&f))).unwrap(), &base_new);
    }
    assert!(s_new > s_old, "7-point eye centers must be steadier: {s_new} vs {s_old}");
}

#[test]
fn head_pose_uses_many_points_and_stays_steady() {
    let Some(pipe) = pipeline() else { return };
    let img = sample();
    let face = pipe.detect_faces(&img).unwrap().remove(0);
    let (mut ys, mut ps, mut rng) = (Vec::new(), Vec::new(), 0xD1B5_4A32_D192_ED03_u64);
    for _ in 0..200 {
        let mut f = face.clone();
        for p in &mut f.dense {
            let (dx, dy) = jitter(&mut rng, 1.0);
            p.x += dx;
            p.y += dy;
        }
        let (y, p, _, _) = estimate_head_pose_3d(&f);
        ys.push(y);
        ps.push(p);
    }
    let std = |v: &[f64]| {
        let m = v.iter().sum::<f64>() / v.len() as f64;
        (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
    };
    assert!(std(&ys) < 1.15, "yaw std {}", std(&ys));
    assert!(std(&ps) < 0.30, "pitch std {}", std(&ps));
}

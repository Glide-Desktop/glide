use super::types::{DetectedFace, clamp_f64};

struct ModelPoint {
    idx: usize,
    x: f64,
    y: f64,
    z: f64,
    weight: f64,
}

const fn mp(idx: usize, x: f64, y: f64, z: f64, weight: f64) -> ModelPoint {
    ModelPoint { idx, x, y, z, weight }
}

const MODEL: [ModelPoint; 50] = [
    mp(27, 0.0, -22.0, 28.0, 1.2),
    mp(30, 0.0, 0.0, 44.0, 1.5),
    mp(31, -15.5, 6.0, 29.0, 1.0),
    mp(33, 0.0, 8.0, 32.0, 1.0),
    mp(35, 15.5, 6.0, 29.0, 1.0),
    mp(36, -43.0, -26.0, 12.0, 1.3),
    mp(39, -17.5, -25.0, 20.0, 1.3),
    mp(42, 17.5, -25.0, 20.0, 1.3),
    mp(45, 43.0, -26.0, 12.0, 1.3),
    mp(48, -26.5, 27.0, 22.0, 1.1),
    mp(54, 26.5, 27.0, 22.0, 1.1),
    mp(8, 0.0, 56.0, 20.0, 0.9),
    mp(3, -55.0, 10.0, -14.0, 0.8),
    mp(13, 55.0, 10.0, -14.0, 0.8),
    mp(28, 0.0, -14.5, 33.0, 0.8),
    mp(29, 0.0, -7.0, 38.5, 0.8),
    mp(32, -8.0, 7.5, 31.0, 0.7),
    mp(34, 8.0, 7.5, 31.0, 0.7),
    mp(37, -35.0, -30.0, 16.0, 0.7),
    mp(38, -25.5, -30.0, 19.0, 0.7),
    mp(40, -25.5, -22.0, 18.0, 0.7),
    mp(41, -35.0, -22.0, 15.5, 0.7),
    mp(43, 25.5, -30.0, 19.0, 0.7),
    mp(44, 35.0, -30.0, 16.0, 0.7),
    mp(46, 35.0, -22.0, 15.5, 0.7),
    mp(47, 25.5, -22.0, 18.0, 0.7),
    mp(17, -52.0, -36.0, 6.0, 0.5),
    mp(18, -44.0, -43.0, 14.0, 0.5),
    mp(19, -34.0, -46.0, 20.0, 0.5),
    mp(20, -24.0, -45.0, 24.0, 0.5),
    mp(21, -13.0, -41.0, 27.0, 0.5),
    mp(22, 13.0, -41.0, 27.0, 0.5),
    mp(23, 24.0, -45.0, 24.0, 0.5),
    mp(24, 34.0, -46.0, 20.0, 0.5),
    mp(25, 44.0, -43.0, 14.0, 0.5),
    mp(26, 52.0, -36.0, 6.0, 0.5),
    mp(49, -17.0, 23.0, 27.0, 0.5),
    mp(50, -7.0, 21.0, 30.5, 0.5),
    mp(51, 0.0, 22.0, 31.0, 0.5),
    mp(52, 7.0, 21.0, 30.5, 0.5),
    mp(53, 17.0, 23.0, 27.0, 0.5),
    mp(55, 17.0, 32.0, 25.0, 0.5),
    mp(56, 8.0, 34.0, 27.5, 0.5),
    mp(57, 0.0, 35.0, 28.0, 0.5),
    mp(58, -8.0, 34.0, 27.5, 0.5),
    mp(59, -17.0, 32.0, 25.0, 0.5),
    mp(5, -42.0, 40.0, -2.0, 0.5),
    mp(7, -17.0, 52.0, 14.0, 0.6),
    mp(9, 17.0, 52.0, 14.0, 0.6),
    mp(11, 42.0, 40.0, -2.0, 0.5),
];

const N: usize = MODEL.len();

const PAIRS: [(usize, usize, f64); 9] = [
    (36, 45, 1.3),
    (39, 42, 1.3),
    (37, 44, 0.7),
    (38, 43, 0.7),
    (41, 46, 0.7),
    (40, 47, 0.7),
    (31, 35, 0.6),
    (48, 54, 0.8),
    (17, 26, 0.5),
];

#[allow(clippy::too_many_lines, clippy::similar_names)] // One weak-perspective fit; the stages share the observed points
#[must_use]
pub fn estimate_head_pose_3d(face: &DetectedFace) -> (f64, f64, f64, f64) {
    let p5 = &face.landmarks;
    let dx_eyes = f64::from(p5.left_eye.x - p5.right_eye.x);
    let dy_eyes = f64::from(p5.left_eye.y - p5.right_eye.y);
    let mut roll_rad = dy_eyes.atan2(dx_eyes);
    let mut roll_deg = roll_rad.to_degrees();
    let (mut yaw_deg, mut pitch_deg) = (0.0, 0.0);

    if !face.has_dense() {
        let dist_r = f64::from((p5.nose_tip.x - p5.right_eye.x).abs());
        let dist_l = f64::from((p5.left_eye.x - p5.nose_tip.x).abs());
        if dist_r + dist_l > 1e-3 {
            yaw_deg = ((dist_r - dist_l) / (dist_r + dist_l)) * 85.0;
        }
        let eye_y = 0.5 * f64::from(p5.right_eye.y + p5.left_eye.y);
        let mouth_y = 0.5 * f64::from(p5.right_mouth.y + p5.left_mouth.y);
        if mouth_y - eye_y > 1e-3 {
            pitch_deg = ((f64::from(p5.nose_tip.y) - eye_y) / (mouth_y - eye_y) - 0.50) * 90.0;
        }
        return (yaw_deg, pitch_deg, roll_deg, 0.02);
    }

    let d = &face.dense;
    let dx_outer = f64::from(d[45].x - d[36].x);
    let dy_outer = f64::from(d[45].y - d[36].y);
    let centroid = |r: std::ops::Range<usize>| {
        let n = r.len() as f64;
        let (sx, sy) = d[r].iter().fold((0.0, 0.0), |(x, y), p| (x + f64::from(p.x), y + f64::from(p.y)));
        (sx / n, sy / n)
    };
    let (rcx, rcy) = centroid(36..42);
    let (lcx, lcy) = centroid(42..48);
    roll_rad = (0.7 * (lcy - rcy) + 0.3 * dy_eyes).atan2(0.7 * (lcx - rcx) + 0.3 * dx_eyes);
    roll_deg = roll_rad.to_degrees();

    let (cos_r, sin_r) = ((-roll_rad).cos(), (-roll_rad).sin());
    let (nose_x, nose_y) = (f64::from(d[30].x), f64::from(d[30].y));
    let mut u_obs = [0f64; N];
    let mut v_obs = [0f64; N];
    for (k, m) in MODEL.iter().enumerate() {
        let rx = f64::from(d[m.idx].x) - nose_x;
        let ry = f64::from(d[m.idx].y) - nose_y;
        u_obs[k] = rx * cos_r - ry * sin_r;
        v_obs[k] = rx * sin_r + ry * cos_r;
    }

    let mut eye_span_px = dx_outer.hypot(dy_outer);
    if eye_span_px < 8.0 {
        eye_span_px = f64::from(face.width) * 0.65;
    }

    let pos = |idx: usize| MODEL.iter().position(|m| m.idx == idx).expect("model index");
    let pair_ratio = |a: usize, b: usize| -> Option<f64> {
        let (ia, ib) = (pos(a), pos(b));
        let dr = (u_obs[1] - u_obs[ia]).abs();
        let dl = (u_obs[ib] - u_obs[1]).abs();
        if dr + dl < 1e-3 {
            return None;
        }
        let m = &MODEL[ib];
        let scale = (m.x / (44.0 - m.z)) / (43.0 / 32.0);
        Some((dr - dl) / (dr + dl) * scale)
    };
    let (mut rsum, mut wsum) = (0.0, 0.0);
    for (a, b, w) in PAIRS {
        if let Some(r) = pair_ratio(a, b) {
            rsum += w * r;
            wsum += w;
        }
    }
    let yaw_ratio = if wsum > 0.0 { rsum / wsum } else { 0.0 };
    let has_ratio = wsum > 0.0;
    let mut yaw = yaw_ratio * 1.15;
    let mean_v = |ids: &[usize]| ids.iter().map(|&i| v_obs[pos(i)]).sum::<f64>() / ids.len() as f64;
    let eye_mid_v = mean_v(&[36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47]);
    let mouth_mid_v = mean_v(&[48, 54]);
    let mut pitch = if mouth_mid_v - eye_mid_v > 1e-3 {
        ((0.0 - eye_mid_v) / (mouth_mid_v - eye_mid_v) - 0.50) * 1.25
    } else {
        0.0
    };

    let w_total: f64 = MODEL.iter().map(|m| m.weight).sum();
    let obs_cu = MODEL.iter().zip(&u_obs).map(|(m, u)| m.weight * u).sum::<f64>() / w_total;
    let obs_cv = MODEL.iter().zip(&v_obs).map(|(m, v)| m.weight * v).sum::<f64>() / w_total;
    let eval_cost = |y_rad: f64, p_rad: f64| -> f64 {
        let (cy, sy) = (y_rad.cos(), y_rad.sin());
        let (cp, sp) = (p_rad.cos(), p_rad.sin());
        let mut up = [0f64; N];
        let mut vp = [0f64; N];
        let (mut cu, mut cv) = (0.0, 0.0);
        for (k, m) in MODEL.iter().enumerate() {
            let x1 = m.x * cy + m.z * sy;
            let z1 = -m.x * sy + m.z * cy;
            up[k] = x1;
            vp[k] = m.y * cp - z1 * sp;
            cu += m.weight * up[k];
            cv += m.weight * vp[k];
        }
        cu /= w_total;
        cv /= w_total;
        let (mut num, mut den) = (0.0, 0.0);
        for (k, m) in MODEL.iter().enumerate() {
            up[k] -= cu;
            vp[k] -= cv;
            num += m.weight * (up[k] * (u_obs[k] - obs_cu) + vp[k] * (v_obs[k] - obs_cv));
            den += m.weight * (up[k] * up[k] + vp[k] * vp[k]);
        }
        if den < 1e-6 {
            return 1e9;
        }
        let mut s = num / den;
        if s < 0.1 {
            s = eye_span_px / 86.0;
        }
        let mut err_sq = 0.0;
        for (k, m) in MODEL.iter().enumerate() {
            let du = s * up[k] - (u_obs[k] - obs_cu);
            let dv = s * vp[k] - (v_obs[k] - obs_cv);
            err_sq += m.weight * (du * du + dv * dv);
        }
        err_sq / w_total
    };

    let mut step = 0.08;
    let mut best_cost = eval_cost(yaw, pitch);
    for _ in 0..20 {
        let mut improved = false;
        for dy in [-step, 0.0, step] {
            for dp in [-step, 0.0, step] {
                if dy == 0.0 && dp == 0.0 {
                    continue;
                }
                let cand_y = clamp_f64(yaw + dy, -0.85, 0.85);
                let cand_p = clamp_f64(pitch + dp, -0.75, 0.75);
                let c = eval_cost(cand_y, cand_p);
                if c < best_cost {
                    best_cost = c;
                    yaw = cand_y;
                    pitch = cand_p;
                    improved = true;
                }
            }
        }
        step *= if improved { 0.70 } else { 0.45 };
    }
    let reproj_err = best_cost.sqrt() / eye_span_px.max(1.0);

    let geom_yaw = if has_ratio { yaw_ratio * 85.0 } else { 0.0 };
    let geom_pitch = if mouth_mid_v - eye_mid_v > 1e-3 {
        ((0.0 - eye_mid_v) / (mouth_mid_v - eye_mid_v) - 0.50) * 90.0
    } else {
        0.0
    };
    yaw_deg = 0.80 * yaw.to_degrees() + 0.20 * geom_yaw;
    pitch_deg = 0.80 * pitch.to_degrees() + 0.20 * geom_pitch;
    (yaw_deg, pitch_deg, roll_deg, reproj_err)
}

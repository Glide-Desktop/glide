use std::f64::consts::PI;

use super::image::RgbaImage;
use super::types::{DENSE_LANDMARK_COUNT, DetectedFace, LM_NOSE_TIP, Point2D, clamp_f64};

#[rustfmt::skip]
pub const FACE_MESH_CONTOUR_EDGES: &[[usize; 2]] = &[
    [0, 1], [1, 2], [2, 3], [3, 4], [4, 5], [5, 6], [6, 7], [7, 8],
    [8, 9], [9, 10], [10, 11], [11, 12], [12, 13], [13, 14], [14, 15], [15, 16],
    [17, 18], [18, 19], [19, 20], [20, 21],
    [22, 23], [23, 24], [24, 25], [25, 26],
    [27, 28], [28, 29], [29, 30],
    [31, 32], [32, 33], [33, 34], [34, 35],
    [30, 31], [30, 33], [30, 35],
    [36, 37], [37, 38], [38, 39], [39, 40], [40, 41], [41, 36],
    [42, 43], [43, 44], [44, 45], [45, 46], [46, 47], [47, 42],
    [48, 49], [49, 50], [50, 51], [51, 52], [52, 53], [53, 54],
    [54, 55], [55, 56], [56, 57], [57, 58], [58, 59], [59, 48],
    [60, 61], [61, 62], [62, 63], [63, 64], [64, 65], [65, 66], [66, 67], [67, 60],
];

#[rustfmt::skip]
pub const FACE_MESH_TRIANGULATION_EDGES: &[[usize; 2]] = &[
    [0, 17], [17, 36], [18, 37], [19, 37], [20, 38], [21, 39],
    [21, 27], [22, 27], [21, 22],
    [22, 42], [23, 43], [24, 44], [25, 44], [26, 45], [16, 26],
    [39, 27], [39, 28], [42, 27], [42, 28],
    [36, 1], [36, 2], [41, 2], [41, 29], [40, 29], [40, 31],
    [45, 15], [45, 14], [46, 14], [46, 29], [47, 29], [47, 35],
    [27, 31], [27, 35],
    [31, 3], [31, 48], [32, 49], [33, 51], [34, 53], [35, 54], [35, 13],
    [2, 31], [3, 48], [4, 48], [5, 59], [6, 58], [7, 57], [8, 57],
    [9, 57], [10, 56], [11, 55], [12, 54], [13, 54], [14, 35],
];

const BROW_FRACS: [[f64; 2]; 5] = [[-1.26, -0.76], [-0.72, -0.98], [-0.16, -1.08], [0.38, -0.98], [0.88, -0.82]];

#[inline]
fn dist(a: Point2D, b: Point2D) -> f64 {
    ((a.x - b.x) as f64).hypot((a.y - b.y) as f64)
}

#[must_use]
pub fn compute_ear68(d: &[Point2D]) -> f64 {
    if d.len() < DENSE_LANDMARK_COUNT {
        return 0.30;
    }
    let ratio = |outer: usize, u1: usize, u2: usize, inner: usize, l2: usize, l1: usize| {
        let horiz = 2.0 * dist(d[outer], d[inner]);
        let vert = dist(d[u1], d[l1]) + dist(d[u2], d[l2]);
        if horiz > 1e-3 { vert / horiz } else { 0.0 }
    };
    let r = ratio(36, 37, 38, 39, 40, 41);
    let l = ratio(42, 43, 44, 45, 46, 47);
    0.5 * (r + l)
}

#[inline]
fn sample_lum(src: &RgbaImage, fx: f64, fy: f64) -> Option<f64> {
    let ix = (fx + 0.5) as i64;
    let iy = (fy + 0.5) as i64;
    if ix < 0 || iy < 0 || ix >= src.width as i64 || iy >= src.height as i64 {
        return None;
    }
    Some(src.lum(ix as usize, iy as usize))
}

fn offset(base: Point2D, du: f64, dv: f64, u: (f64, f64), v: (f64, f64)) -> Point2D {
    Point2D::new(base.x + (du * u.0 + dv * v.0) as f32, base.y + (du * u.1 + dv * v.1) as f32)
}

fn lerp_pt(a: Point2D, b: Point2D, t: f64) -> Point2D {
    Point2D::new(a.x + (t * (b.x - a.x) as f64) as f32, a.y + (t * (b.y - a.y) as f64) as f32)
}

#[allow(clippy::too_many_lines, clippy::similar_names)] // The 68 points are placed region by region in one geometric frame
pub fn extract_dense_landmarks(src: &RgbaImage, face: &mut DetectedFace) -> Vec<Point2D> {
    let mut pts = vec![Point2D::default(); DENSE_LANDMARK_COUNT];
    let k5 = face.landmarks;

    let r_eye = refine_pupil_center(src, k5.right_eye, face.width * 0.065);
    let l_eye = refine_pupil_center(src, k5.left_eye, face.width * 0.065);
    face.landmarks.right_eye = r_eye;
    face.landmarks.left_eye = l_eye;

    let nose = k5.nose_tip;
    let r_mouth = k5.right_mouth;
    let l_mouth = k5.left_mouth;

    let mut dx_e = (l_eye.x - r_eye.x) as f64;
    let mut dy_e = (l_eye.y - r_eye.y) as f64;
    let mut d_eyes = dx_e.hypot(dy_e);
    if d_eyes < 4.0 {
        d_eyes = face.width as f64 * 0.42;
        dx_e = d_eyes;
        dy_e = 0.0;
    }
    let u = (dx_e / d_eyes, dy_e / d_eyes);
    let v = (-u.1, u.0);

    let eye_mid_x = 0.5 * (r_eye.x + l_eye.x) as f64;
    let eye_mid_y = 0.5 * (r_eye.y + l_eye.y) as f64;
    let mouth_mid_x = 0.5 * (r_mouth.x + l_mouth.x) as f64;
    let mouth_mid_y = 0.5 * (r_mouth.y + l_mouth.y) as f64;

    let mut h_em = (mouth_mid_x - eye_mid_x) * v.0 + (mouth_mid_y - eye_mid_y) * v.1;
    if h_em < d_eyes * 0.55 {
        h_em = d_eyes * 0.85;
    }

    let nose_proj_u = (nose.x as f64 - eye_mid_x) * u.0 + (nose.y as f64 - eye_mid_y) * u.1;
    let yaw_ratio = clamp_f64(nose_proj_u / (0.5 * d_eyes), -0.65, 0.65);
    let scale_right = (1.0 + 0.34 * yaw_ratio).max(0.35);
    let scale_left = (1.0 - 0.34 * yaw_ratio).max(0.35);

    let off = |base: Point2D, du: f64, dv: f64| offset(base, du, dv, u, v);

    let eye_center = Point2D::new(eye_mid_x as f32, eye_mid_y as f32);
    let mouth_mid = Point2D::new(mouth_mid_x as f32, mouth_mid_y as f32);
    let chin_center =
        snap_gradient_along_axis(src, off(mouth_mid, -0.05 * yaw_ratio * d_eyes, 0.50 * h_em), v.0, v.1, 5.5);

    let jaw_base_span_u = 0.80 * d_eyes;
    for i in 0..=16usize {
        if i == 8 {
            pts[8] = chin_center;
            continue;
        }
        let t = (i as f64 - 8.0) / 8.0;
        let abs_t = t.abs();
        let side_scale = if t > 0.0 { scale_left } else { scale_right };
        let u_norm = (t * (PI / 2.0)).sin();
        let du = u_norm * jaw_base_span_u * side_scale;
        let mut chin_drop_v = (chin_center.x as f64 - eye_mid_x) * v.0 + (chin_center.y as f64 - eye_mid_y) * v.1;
        if chin_drop_v < 1.1 * h_em {
            chin_drop_v = 1.45 * h_em;
        }
        let dv = (1.0 - abs_t.powf(1.65)) * chin_drop_v;
        let mut p = off(eye_center, du, dv);
        if (2..=14).contains(&i) {
            let rad_x = u_norm * u.0 + (1.0 - abs_t) * v.0;
            let rad_y = u_norm * u.1 + (1.0 - abs_t) * v.1;
            let r_norm = rad_x.hypot(rad_y);
            if r_norm > 1e-3 {
                p = snap_gradient_along_axis(src, p, rad_x / r_norm, rad_y / r_norm, 4.0);
            }
        }
        pts[i] = p;
    }

    let r_eye_radius_u = 0.25 * d_eyes * scale_right.sqrt();
    let l_eye_radius_u = 0.25 * d_eyes * scale_left.sqrt();
    let brow_lift = 0.28 * d_eyes;
    for k in 0..5 {
        let ru = BROW_FRACS[k][0] * r_eye_radius_u;
        let rv = BROW_FRACS[k][1] * brow_lift;
        pts[17 + k] = snap_dark_ridge_along_axis(src, off(r_eye, ru, rv), v.0, v.1, 4.0);
        let m = 4 - k;
        let lu = -BROW_FRACS[m][0] * l_eye_radius_u;
        let lv = BROW_FRACS[m][1] * brow_lift;
        pts[22 + k] = snap_dark_ridge_along_axis(src, off(l_eye, lu, lv), v.0, v.1, 4.0);
    }

    for (k, f) in [0.16, 0.44, 0.72, 1.00].iter().enumerate() {
        pts[27 + k] = Point2D::new(
            (eye_mid_x + f * (nose.x as f64 - eye_mid_x)) as f32,
            (eye_mid_y + f * (nose.y as f64 - eye_mid_y)) as f32,
        );
    }
    let nostril_w = 0.23 * d_eyes;
    pts[31] = off(nose, -nostril_w * scale_right, 0.055 * h_em);
    pts[32] = off(nose, -0.50 * nostril_w * scale_right, 0.075 * h_em);
    pts[33] = off(nose, 0.0, 0.085 * h_em);
    pts[34] = off(nose, 0.50 * nostril_w * scale_left, 0.075 * h_em);
    pts[35] = off(nose, nostril_w * scale_left, 0.055 * h_em);

    let eye_radius_v = 0.112 * d_eyes;
    let sg = |p: Point2D, m: f64| snap_gradient_along_axis(src, p, v.0, v.1, m);
    pts[36] = off(r_eye, -r_eye_radius_u, 0.01 * d_eyes);
    pts[37] = sg(off(r_eye, -0.42 * r_eye_radius_u, -eye_radius_v * 0.95), 3.2);
    pts[38] = sg(off(r_eye, 0.38 * r_eye_radius_u, -eye_radius_v * 0.95), 3.2);
    pts[39] = off(r_eye, 0.94 * r_eye_radius_u, 0.015 * d_eyes);
    pts[40] = sg(off(r_eye, 0.38 * r_eye_radius_u, eye_radius_v * 0.90), 3.0);
    pts[41] = sg(off(r_eye, -0.42 * r_eye_radius_u, eye_radius_v * 0.90), 3.0);

    pts[42] = off(l_eye, -0.94 * l_eye_radius_u, 0.015 * d_eyes);
    pts[43] = sg(off(l_eye, -0.38 * l_eye_radius_u, -eye_radius_v * 0.95), 3.2);
    pts[44] = sg(off(l_eye, 0.42 * l_eye_radius_u, -eye_radius_v * 0.95), 3.2);
    pts[45] = off(l_eye, l_eye_radius_u, 0.01 * d_eyes);
    pts[46] = sg(off(l_eye, 0.42 * l_eye_radius_u, eye_radius_v * 0.90), 3.0);
    pts[47] = sg(off(l_eye, -0.38 * l_eye_radius_u, eye_radius_v * 0.90), 3.0);

    let dx_m = (l_mouth.x - r_mouth.x) as f64;
    let dy_m = (l_mouth.y - r_mouth.y) as f64;
    let mut w_mouth = dx_m.hypot(dy_m);
    if w_mouth < 4.0 {
        w_mouth = 0.75 * d_eyes;
    }
    let lip_half_h = (0.125 * h_em).max(0.19 * w_mouth);
    let lm = |t: f64| lerp_pt(r_mouth, l_mouth, t);

    pts[48] = r_mouth;
    pts[49] = sg(off(lm(0.20), 0.0, -0.68 * lip_half_h), 3.2);
    pts[50] = sg(off(lm(0.37), 0.0, -1.04 * lip_half_h), 3.5);
    pts[51] = sg(off(mouth_mid, 0.0, -0.94 * lip_half_h), 3.5);
    pts[52] = sg(off(lm(0.63), 0.0, -1.04 * lip_half_h), 3.5);
    pts[53] = sg(off(lm(0.80), 0.0, -0.68 * lip_half_h), 3.2);
    pts[54] = l_mouth;
    pts[55] = sg(off(lm(0.80), 0.0, 0.74 * lip_half_h), 3.2);
    pts[56] = sg(off(lm(0.64), 0.0, 1.08 * lip_half_h), 3.5);
    pts[57] = sg(off(mouth_mid, 0.0, 1.15 * lip_half_h), 3.8);
    pts[58] = sg(off(lm(0.36), 0.0, 1.08 * lip_half_h), 3.5);
    pts[59] = sg(off(lm(0.20), 0.0, 0.74 * lip_half_h), 3.2);

    let inner_h = 0.36 * lip_half_h;
    pts[60] = lm(0.08);
    pts[61] = off(lm(0.32), 0.0, -inner_h);
    pts[62] = off(mouth_mid, 0.0, -inner_h * 0.95);
    pts[63] = off(lm(0.68), 0.0, -inner_h);
    pts[64] = lm(0.92);
    pts[65] = off(lm(0.68), 0.0, inner_h * 1.05);
    pts[66] = off(mouth_mid, 0.0, inner_h * 1.10);
    pts[67] = off(lm(0.32), 0.0, inner_h * 1.05);

    refine_68_landmarks_active(src, face, &mut pts);
    face.ear = compute_ear68(&pts);
    pts
}

#[allow(clippy::too_many_lines, clippy::many_single_char_names, clippy::similar_names)] // Colour-space math in its usual notation; the three refinement passes share the skin reference
pub fn refine_68_landmarks_active(src: &RgbaImage, face: &DetectedFace, pts: &mut [Point2D]) {
    if pts.len() < DENSE_LANDMARK_COUNT || src.width < 16 || src.height < 16 {
        return;
    }
    let sample = |fx: f64, fy: f64| -> Option<(f64, f64, f64, f64)> {
        let ix = (fx + 0.5) as i64;
        let iy = (fy + 0.5) as i64;
        if ix < 0 || iy < 0 || ix >= src.width as i64 || iy >= src.height as i64 {
            return None;
        }
        let i = iy as usize * src.stride() + ix as usize * 4;
        let r = src.pix[i] as f64;
        let g = src.pix[i + 1] as f64;
        let b = src.pix[i + 2] as f64;
        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        let cb = 128.0 - 0.168_736 * r - 0.331_264 * g + 0.5 * b;
        let cr = 128.0 + 0.5 * r - 0.418_688 * g - 0.081_312 * b;
        let red = 1.5 * r - g - 0.5 * b;
        Some((y, cb, cr, red))
    };

    let cheeks = [
        (0.5 * (pts[36].x + pts[48].x) as f64, 0.5 * (pts[36].y + pts[48].y) as f64),
        (0.5 * (pts[45].x + pts[54].x) as f64, 0.5 * (pts[45].y + pts[54].y) as f64),
    ];
    let (mut skin_y, mut skin_cb, mut skin_cr, mut skin_count) = (0.0, 0.0, 0.0, 0.0);
    for (cx, cy) in cheeks {
        for dy in [-2.0, 0.0, 2.0] {
            for dx in [-2.0, 0.0, 2.0] {
                if let Some((y, cb, cr, _)) = sample(cx + dx, cy + dy) {
                    skin_y += y;
                    skin_cb += cb;
                    skin_cr += cr;
                    skin_count += 1.0;
                }
            }
        }
    }
    if skin_count > 0.0 {
        skin_y /= skin_count;
        skin_cb /= skin_count;
        skin_cr /= skin_count;
    }

    let mut jaw = [Point2D::default(); 17];
    jaw[0] = pts[0];
    jaw[16] = pts[16];
    let max_ray = clamp_f64(face.width as f64 * 0.045, 4.0, 12.0);
    for i in 1..=15 {
        let tx = (pts[i + 1].x - pts[i - 1].x) as f64;
        let ty = (pts[i + 1].y - pts[i - 1].y) as f64;
        let t_len = tx.hypot(ty);
        if t_len < 1e-3 {
            jaw[i] = pts[i];
            continue;
        }
        let (mut nx, mut ny) = (-ty / t_len, tx / t_len);
        let to_out_x = (pts[i].x - pts[LM_NOSE_TIP].x) as f64;
        let to_out_y = (pts[i].y - pts[LM_NOSE_TIP].y) as f64;
        if nx * to_out_x + ny * to_out_y < 0.0 {
            nx = -nx;
            ny = -ny;
        }
        let mut best_t = 0.0;
        let mut best_score = 0.0;
        let mut t = -max_ray;
        while t <= max_ray {
            let px = pts[i].x as f64 + t * nx;
            let py = pts[i].y as f64 + t * ny;
            if let (Some((y_in, cb_in, cr_in, _)), Some((y_out, cb_out, cr_out, _))) =
                (sample(px - 1.5 * nx, py - 1.5 * ny), sample(px + 1.5 * nx, py + 1.5 * ny))
            {
                let d_lum = (y_out - y_in).abs();
                let d_chroma = (cb_out - cb_in).hypot(cr_out - cr_in);
                let dist_in = (cb_in - skin_cb).hypot(cr_in - skin_cr) + 0.35 * (y_in - skin_y).abs();
                let dist_out = (cb_out - skin_cb).hypot(cr_out - skin_cr) + 0.35 * (y_out - skin_y).abs();
                let skin_step = (dist_out - dist_in).max(0.0);
                let prior = 1.0 - 0.45 * (t.abs() / max_ray);
                let score = (d_lum + 1.8 * d_chroma + 1.4 * skin_step) * prior;
                if score > best_score {
                    best_score = score;
                    best_t = t;
                }
            }
            t += 1.0;
        }
        jaw[i] = if best_score > 14.0 {
            Point2D::new(pts[i].x + (0.45 * best_t * nx) as f32, pts[i].y + (0.45 * best_t * ny) as f32)
        } else {
            pts[i]
        };
    }
    for i in 1..=15 {
        let (a, b) = if i == 8 { (0.15f32, 0.70f32) } else { (0.20, 0.60) };
        pts[i] = Point2D::new(
            a * jaw[i - 1].x + b * jaw[i].x + a * jaw[i + 1].x,
            a * jaw[i - 1].y + b * jaw[i].y + a * jaw[i + 1].y,
        );
    }

    let dx_m = (pts[54].x - pts[48].x) as f64;
    let dy_m = (pts[54].y - pts[48].y) as f64;
    let m_len = dx_m.hypot(dy_m);
    if m_len > 4.0 {
        let (vx, vy) = (-dy_m / m_len, dx_m / m_len);
        for idx in [49usize, 50, 51, 52, 53, 55, 56, 57, 58, 59] {
            let mut best_t = 0.0;
            let mut best_grad = 0.0;
            let mut t = -3.5f64;
            while t <= 3.5 {
                let px = pts[idx].x as f64 + t * vx;
                let py = pts[idx].y as f64 + t * vy;
                if let (Some((y1, _, _, r1)), Some((y2, _, _, r2))) =
                    (sample(px - vx, py - vy), sample(px + vx, py + vy))
                {
                    let grad = ((y2 - y1).abs() + 1.6 * (r2 - r1).abs()) * (1.0 - 0.35 * t.abs() / 3.5);
                    if grad > best_grad {
                        best_grad = grad;
                        best_t = t;
                    }
                }
                t += 0.7;
            }
            if best_grad > 10.0 {
                pts[idx].x += (0.40 * best_t * vx) as f32;
                pts[idx].y += (0.40 * best_t * vy) as f32;
            }
        }
    }
}

#[must_use]
pub fn refine_pupil_center(src: &RgbaImage, p: Point2D, radius: f32) -> Point2D {
    let r = clamp_f64(radius as f64, 3.0, 9.0) as i64;
    let cx = p.x as i64;
    let cy = p.y as i64;
    if cx - r < 0 || cx + r >= src.width as i64 || cy - r < 0 || cy + r >= src.height as i64 {
        return p;
    }
    let mut sum = 0.0;
    let mut count = 0usize;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy > r * r {
                continue;
            }
            sum += src.lum((cx + dx) as usize, (cy + dy) as usize);
            count += 1;
        }
    }
    if count == 0 {
        return p;
    }
    let mean = sum / count as f64;
    let (mut wx, mut wy, mut w_sum) = (0.0, 0.0, 0.0);
    let sigma2 = (r * r) as f64 * 0.65;
    for dy in -r..=r {
        for dx in -r..=r {
            let d2 = (dx * dx + dy * dy) as f64;
            if d2 > (r * r) as f64 {
                continue;
            }
            let darkness = mean - src.lum((cx + dx) as usize, (cy + dy) as usize);
            if darkness <= 2.0 {
                continue;
            }
            let w = darkness * darkness * (-d2 / (2.0 * sigma2)).exp();
            wx += dx as f64 * w;
            wy += dy as f64 * w;
            w_sum += w;
        }
    }
    if w_sum <= 1e-3 {
        return p;
    }
    Point2D::new(p.x + (0.40 * (wx / w_sum)) as f32, p.y + (0.40 * (wy / w_sum)) as f32)
}

#[must_use]
pub fn snap_gradient_along_axis(src: &RgbaImage, p: Point2D, ax: f64, ay: f64, max_step: f64) -> Point2D {
    let steps = ((max_step * 2.0) as i64).max(4);
    let mut best_t = 0.0;
    let mut best_grad = 0.0;
    for i in -steps..=steps {
        let t = (i as f64 / steps as f64) * max_step;
        let x = p.x as f64 + t * ax;
        let y = p.y as f64 + t * ay;
        if let (Some(prev), Some(next)) = (sample_lum(src, x - ax, y - ay), sample_lum(src, x + ax, y + ay)) {
            let grad = (next - prev).abs() * (1.0 - 0.35 * t.abs() / max_step);
            if grad > best_grad {
                best_grad = grad;
                best_t = t;
            }
        }
    }
    if best_grad < 8.0 {
        return p;
    }
    Point2D::new(p.x + (0.45 * best_t * ax) as f32, p.y + (0.45 * best_t * ay) as f32)
}

#[must_use]
pub fn snap_dark_ridge_along_axis(src: &RgbaImage, p: Point2D, ax: f64, ay: f64, max_step: f64) -> Point2D {
    let mut best_t = 0.0;
    let mut best_score = -1e9;
    let mut t = -max_step;
    while t <= max_step {
        let ix = (p.x as f64 + t * ax + 0.5) as i64;
        let iy = (p.y as f64 + t * ay + 0.5) as i64;
        if ix >= 0 && iy >= 0 && ix < src.width as i64 && iy < src.height as i64 {
            let lum = src.lum(ix as usize, iy as usize);
            let score = (255.0 - lum) * (1.0 - 0.25 * t.abs() / max_step);
            if score > best_score {
                best_score = score;
                best_t = t;
            }
        }
        t += 1.0;
    }
    Point2D::new(p.x + (0.35 * best_t * ax) as f32, p.y + (0.35 * best_t * ay) as f32)
}

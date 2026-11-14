use super::image::RgbaImage;
use super::types::{DetectedFace, Landmarks5, Point2D, clamp_f64};

const CANONICAL_LANDMARKS: [[f64; 2]; 5] =
    [[38.2946, 51.6963], [73.5318, 51.5014], [56.0252, 71.7366], [41.5493, 92.3655], [70.7299, 92.2041]];

pub type TransformMatrix = [[f64; 3]; 2];

#[must_use]
pub fn alignment_keypoints(face: &DetectedFace) -> Landmarks5 {
    let mut k = face.landmarks;
    if face.has_dense() {
        let d = &face.dense;
        let centroid = |r: std::ops::Range<usize>| {
            let n = r.len() as f32;
            let (sx, sy) = d[r].iter().fold((0.0, 0.0), |(x, y), p| (x + p.x, y + p.y));
            Point2D::new(sx / n, sy / n)
        };
        let blend = |pupil: Point2D, c: Point2D| Point2D::new(0.5 * pupil.x + 0.5 * c.x, 0.5 * pupil.y + 0.5 * c.y);
        k.right_eye = blend(k.right_eye, centroid(36..42));
        k.left_eye = blend(k.left_eye, centroid(42..48));
    }
    k
}

#[must_use]
pub fn align_face_mesh(src: &RgbaImage, face: &DetectedFace) -> RgbaImage {
    align_face(src, &alignment_keypoints(face))
}

#[must_use]
pub fn align_face(src: &RgbaImage, landmarks: &Landmarks5) -> RgbaImage {
    let mut src_pts = [[0f64; 2]; 5];
    for (i, p) in landmarks.as_array().iter().enumerate() {
        src_pts[i] = [f64::from(p.x), f64::from(p.y)];
    }
    let m = similarity_transform(&src_pts, &CANONICAL_LANDMARKS);
    let mut aligned = RgbaImage::new(112, 112);
    warp_affine_bilinear(src, &mut aligned, &m);
    normalize_aligned_illumination(&mut aligned);
    aligned
}

fn normalize_aligned_illumination(img: &mut RgbaImage) {
    let n = img.width * img.height;
    let (mut sum, mut sum_sq) = (0.0, 0.0);
    for px in img.pix.chunks_exact(4) {
        let lum = 0.299 * px[0] as f64 + 0.587 * px[1] as f64 + 0.114 * px[2] as f64;
        sum += lum;
        sum_sq += lum * lum;
    }
    let mean = sum / n as f64;
    let variance = sum_sq / n as f64 - mean * mean;
    let std = variance.max(1.0).sqrt();
    if (95.0..=165.0).contains(&mean) && std >= 36.0 {
        return;
    }
    let target_mean = clamp_f64(mean, 105.0, 150.0);
    let target_std = std.max(40.0);
    let gain = clamp_f64(target_std / std, 0.80, 1.45);
    for px in img.pix.chunks_exact_mut(4) {
        for c in &mut px[..3] {
            let v = *c as f64;
            let norm = (v - mean) * gain + target_mean;
            *c = clamp_f64(0.50 * v + 0.50 * norm + 0.5, 0.0, 255.0) as u8;
        }
    }
}

#[must_use]
pub fn similarity_transform(src: &[[f64; 2]; 5], dst: &[[f64; 2]; 5]) -> TransformMatrix {
    let mut src_mean = [0f64; 2];
    let mut dst_mean = [0f64; 2];
    for i in 0..5 {
        src_mean[0] += src[i][0];
        src_mean[1] += src[i][1];
        dst_mean[0] += dst[i][0];
        dst_mean[1] += dst[i][1];
    }
    for v in src_mean.iter_mut().chain(dst_mean.iter_mut()) {
        *v /= 5.0;
    }
    let (mut a00, mut a01, mut a10, mut a11, mut var_src) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for i in 0..5 {
        let s = [src[i][0] - src_mean[0], src[i][1] - src_mean[1]];
        let d = [dst[i][0] - dst_mean[0], dst[i][1] - dst_mean[1]];
        a00 += d[0] * s[0];
        a01 += d[0] * s[1];
        a10 += d[1] * s[0];
        a11 += d[1] * s[1];
        var_src += s[0] * s[0] + s[1] * s[1];
    }
    a00 /= 5.0;
    a01 /= 5.0;
    a10 /= 5.0;
    a11 /= 5.0;
    var_src /= 5.0;

    let (u, s, vt) = svd2x2(a00, a01, a10, a11);
    let d1 = if a00 * a11 - a01 * a10 < 0.0 { -1.0 } else { 1.0 };

    let mut r00 = u[0][0] * vt[0][0] + u[0][1] * d1 * vt[1][0];
    let mut r01 = u[0][0] * vt[0][1] + u[0][1] * d1 * vt[1][1];
    let mut r10 = u[1][0] * vt[0][0] + u[1][1] * d1 * vt[1][0];
    let mut r11 = u[1][0] * vt[0][1] + u[1][1] * d1 * vt[1][1];

    let scale = if var_src > 1e-6 { (s[0] + s[1] * d1) / var_src } else { 1.0 };
    r00 *= scale;
    r01 *= scale;
    r10 *= scale;
    r11 *= scale;

    let tx = dst_mean[0] - (r00 * src_mean[0] + r01 * src_mean[1]);
    let ty = dst_mean[1] - (r10 * src_mean[0] + r11 * src_mean[1]);
    [[r00, r01, tx], [r10, r11, ty]]
}

fn svd2x2(a00: f64, a01: f64, a10: f64, a11: f64) -> ([[f64; 2]; 2], [f64; 2], [[f64; 2]; 2]) {
    let ata00 = a00 * a00 + a10 * a10;
    let ata01 = a00 * a01 + a10 * a11;
    let ata11 = a01 * a01 + a11 * a11;
    let tr = ata00 + ata11;
    let diff = ata00 - ata11;
    let disc = (diff * diff + 4.0 * ata01 * ata01).max(0.0).sqrt();
    let val1 = f64::midpoint(tr, disc).max(0.0);
    let val2 = ((tr - disc) / 2.0).max(0.0);
    let s = [val1.sqrt(), val2.sqrt()];

    let theta = if ata01.abs() > 1e-9 {
        0.5 * (2.0 * ata01).atan2(diff)
    } else if ata00 >= ata11 {
        0.0
    } else {
        std::f64::consts::PI / 2.0
    };
    let (c, sn) = (theta.cos(), theta.sin());
    let vt = [[c, sn], [-sn, c]];

    let mut u = [[0f64; 2]; 2];
    if s[0] > 1e-7 {
        u[0][0] = (a00 * c + a01 * sn) / s[0];
        u[1][0] = (a10 * c + a11 * sn) / s[0];
    } else {
        u[0][0] = 1.0;
        u[1][0] = 0.0;
    }
    if s[1] > 1e-7 {
        u[0][1] = (-a00 * sn + a01 * c) / s[1];
        u[1][1] = (-a10 * sn + a11 * c) / s[1];
    } else {
        u[0][1] = -u[1][0];
        u[1][1] = u[0][0];
    }
    (u, s, vt)
}

#[allow(clippy::many_single_char_names)] // The affine matrix entries keep their algebraic names
pub fn warp_affine_bilinear(src: &RgbaImage, dst: &mut RgbaImage, m: &TransformMatrix) {
    let (a, b, tx) = (m[0][0], m[0][1], m[0][2]);
    let (c, d, ty) = (m[1][0], m[1][1], m[1][2]);
    let det = a * d - b * c;
    if det.abs() < 1e-7 || src.is_empty() {
        return;
    }
    let inv = 1.0 / det;
    let (ia, ib, ic, id) = (d * inv, -b * inv, -c * inv, a * inv);
    let itx = (b * ty - d * tx) * inv;
    let ity = (c * tx - a * ty) * inv;

    let (sw, sh) = (src.width, src.height);
    let ss = src.stride();
    let ds = dst.stride();
    for dy in 0..dst.height {
        let fdy = dy as f64;
        for dx in 0..dst.width {
            let fdx = dx as f64;
            let sx = (ia * fdx + ib * fdy + itx).clamp(0.0, (sw - 1) as f64);
            let sy = (ic * fdx + id * fdy + ity).clamp(0.0, (sh - 1) as f64);
            let x0 = sx as usize;
            let y0 = sy as usize;
            let x1 = (x0 + 1).min(sw - 1);
            let y1 = (y0 + 1).min(sh - 1);
            let fx = sx - x0 as f64;
            let fy = sy - y0 as f64;
            let (p00, p10, p01, p11) = (y0 * ss + x0 * 4, y0 * ss + x1 * 4, y1 * ss + x0 * 4, y1 * ss + x1 * 4);
            let w00 = (1.0 - fx) * (1.0 - fy);
            let w10 = fx * (1.0 - fy);
            let w01 = (1.0 - fx) * fy;
            let w11 = fx * fy;
            let o = dy * ds + dx * 4;
            for ch in 0..3 {
                dst.pix[o + ch] = (w00 * f64::from(src.pix[p00 + ch])
                    + w10 * f64::from(src.pix[p10 + ch])
                    + w01 * f64::from(src.pix[p01 + ch])
                    + w11 * f64::from(src.pix[p11 + ch])) as u8;
            }
            dst.pix[o + 3] = 255;
        }
    }
}

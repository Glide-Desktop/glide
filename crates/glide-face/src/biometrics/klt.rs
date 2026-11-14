use super::image::RgbaImage;
use super::types::Point2D;

const WIN_R: i64 = 4;
const SIDE: usize = (2 * WIN_R + 1) as usize;

pub fn extract_gray_buffer(src: &RgbaImage, buf: &mut Vec<u8>) -> (usize, usize) {
    let (w, h) = (src.width, src.height);
    buf.resize(w * h, 0);
    for (o, px) in buf.iter_mut().zip(src.pix.chunks_exact(4)) {
        *o = ((77 * u32::from(px[0]) + 150 * u32::from(px[1]) + 29 * u32::from(px[2])) >> 8) as u8;
    }
    (w, h)
}

#[allow(clippy::many_single_char_names, clippy::similar_names)] // Lucas-Kanade in its textbook notation
#[must_use]
pub fn track_point_klt(
    prev: &[u8], curr: &[u8], w: usize, h: usize, prev_pt: Point2D, det_pt: Point2D, face_scale: f64,
) -> (Point2D, f64) {
    if prev.len() < w * h || curr.len() < w * h || w < 24 || h < 24 {
        return (det_pt, 0.0);
    }
    let (px, py) = (prev_pt.x as f64, prev_pt.y as f64);
    let dx_det = det_pt.x as f64 - px;
    let dy_det = det_pt.y as f64 - py;

    if dx_det.hypot(dy_det) > 1.8f64.max(0.015 * face_scale) {
        return (det_pt, 0.0);
    }
    let (ipx, ipy) = (px as i64, py as i64);
    if ipx - WIN_R - 1 < 0 || ipx + WIN_R + 2 >= w as i64 || ipy - WIN_R - 1 < 0 || ipy + WIN_R + 2 >= h as i64 {
        return (det_pt, 0.0);
    }

    let sample = |img: &[u8], fx: f64, fy: f64| -> f64 {
        let fx = fx.clamp(0.0, (w - 2) as f64);
        let fy = fy.clamp(0.0, (h - 2) as f64);
        let (x0, y0) = (fx as usize, fy as usize);
        let (ax, ay) = (fx - x0 as f64, fy - y0 as f64);
        let (r0, r1) = (y0 * w, (y0 + 1) * w);
        let v00 = img[r0 + x0] as f64;
        let v10 = img[r0 + x0 + 1] as f64;
        let v01 = img[r1 + x0] as f64;
        let v11 = img[r1 + x0 + 1] as f64;
        (1.0 - ax) * (1.0 - ay) * v00 + ax * (1.0 - ay) * v10 + (1.0 - ax) * ay * v01 + ax * ay * v11
    };

    let mut t = [0f64; SIDE * SIDE];
    let mut ix = [0f64; SIDE * SIDE];
    let mut iy = [0f64; SIDE * SIDE];
    let mut wt = [0f64; SIDE * SIDE];
    let (mut gxx, mut gxy, mut gyy) = (0.0, 0.0, 0.0);
    let mut k = 0;
    for wy in -WIN_R..=WIN_R {
        for wx in -WIN_R..=WIN_R {
            let sx = px + wx as f64;
            let sy = py + wy as f64;
            let weight = (-((wx * wx + wy * wy) as f64) / 10.0).exp();
            wt[k] = weight;
            t[k] = sample(prev, sx, sy);
            let gx = 0.5 * (sample(prev, sx + 1.0, sy) - sample(prev, sx - 1.0, sy));
            let gy = 0.5 * (sample(prev, sx, sy + 1.0) - sample(prev, sx, sy - 1.0));
            ix[k] = gx;
            iy[k] = gy;
            gxx += weight * gx * gx;
            gxy += weight * gx * gy;
            gyy += weight * gy * gy;
            k += 1;
        }
    }

    let trace = gxx + gyy;
    let det = gxx * gyy - gxy * gxy;
    if det < 1e-3 {
        return (det_pt, 0.0);
    }
    let lambda_min = 0.5 * (trace - (trace * trace - 4.0 * det).max(0.0).sqrt());
    if lambda_min < 18.0 {
        return (det_pt, 0.0);
    }

    let inv_det = 1.0 / det;
    let (mut u, mut v) = (dx_det, dy_det);
    let (mut err_sum, mut w_sum) = (0.0, 0.0);
    for _ in 0..5 {
        let (mut bx, mut by) = (0.0, 0.0);
        err_sum = 0.0;
        w_sum = 0.0;
        let mut idx = 0;
        for wy in -WIN_R..=WIN_R {
            for wx in -WIN_R..=WIN_R {
                let diff = t[idx] - sample(curr, px + wx as f64 + u, py + wy as f64 + v);
                let wv = wt[idx];
                bx += wv * ix[idx] * diff;
                by += wv * iy[idx] * diff;
                err_sum += wv * diff.abs();
                w_sum += wv;
                idx += 1;
            }
        }
        let du = (gyy * bx - gxy * by) * inv_det;
        let dv = (-gxy * bx + gxx * by) * inv_det;
        u += du;
        v += dv;
        if du * du + dv * dv < 1e-3 {
            break;
        }
    }

    let (klt_x, klt_y) = (px + u, py + v);
    let drift = (klt_x - det_pt.x as f64).hypot(klt_y - det_pt.y as f64);
    if drift > 4.0f64.max(0.045 * face_scale) {
        return (det_pt, 0.0);
    }
    let photo_err = err_sum / w_sum.max(1.0);
    if photo_err > 24.0 {
        return (det_pt, 0.0);
    }
    let tex_conf = ((lambda_min - 18.0) / 120.0).min(1.0);
    let photo_conf = (1.0 - photo_err / 26.0).max(0.0);
    (Point2D::new(klt_x as f32, klt_y as f32), 0.25 * tex_conf * photo_conf)
}

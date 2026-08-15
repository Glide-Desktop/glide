pub fn box_blur_rgba(buf: &mut [u8], width: i32, height: i32, radius: i32) {
    if radius <= 0 || width <= 0 || height <= 0 {
        return;
    }
    let Ok(len) = usize::try_from(width * height) else {
        return;
    };
    if buf.len() < len * 4 {
        return;
    }

    let mut plane = vec![0u8; len];
    for channel in 0..4 {
        for (i, px) in plane.iter_mut().enumerate() {
            *px = buf[i * 4 + channel];
        }
        box_blur_alpha(&mut plane, width, height, radius);
        for (i, px) in plane.iter().enumerate() {
            buf[i * 4 + channel] = *px;
        }
    }
}

pub(crate) fn box_blur_alpha(alpha: &mut [u8], width: i32, height: i32, radius: i32) {
    if radius <= 0 || width <= 0 || height <= 0 {
        return;
    }
    let Ok(len) = usize::try_from(width * height) else {
        return;
    };
    if alpha.len() < len {
        return;
    }

    let mut scratch = vec![0u8; len];
    for _ in 0..3 {
        box_blur_pass(alpha, &mut scratch, width, height, radius, true);
        box_blur_pass(&scratch, alpha, width, height, radius, false);
    }
}

fn box_blur_pass(
    src: &[u8],
    dst: &mut [u8],
    width: i32,
    height: i32,
    radius: i32,
    horizontal: bool,
) {
    if horizontal {
        for y in 0..height {
            let row = (y * width) as usize;
            let mut sum: i64 = 0;
            let mut count: i64 = 0;
            for dx in 0..=radius.min(width - 1) {
                sum += i64::from(src[row + dx as usize]);
                count += 1;
            }
            for x in 0..width {
                dst[row + x as usize] = (sum / count.max(1)) as u8;
                let leaving = x - radius;
                let entering = x + radius + 1;
                if leaving >= 0 {
                    sum -= i64::from(src[row + leaving as usize]);
                    count -= 1;
                }
                if entering < width {
                    sum += i64::from(src[row + entering as usize]);
                    count += 1;
                }
            }
        }
    } else {
        for x in 0..width {
            let mut sum: i64 = 0;
            let mut count: i64 = 0;
            for dy in 0..=radius.min(height - 1) {
                sum += i64::from(src[(dy * width + x) as usize]);
                count += 1;
            }
            for y in 0..height {
                dst[(y * width + x) as usize] = (sum / count.max(1)) as u8;
                let leaving = y - radius;
                let entering = y + radius + 1;
                if leaving >= 0 {
                    sum -= i64::from(src[(leaving * width + x) as usize]);
                    count -= 1;
                }
                if entering < height {
                    sum += i64::from(src[(entering * width + x) as usize]);
                    count += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_radius_is_a_no_op() {
        let mut alpha = vec![10u8, 20, 30, 40];
        let before = alpha.clone();
        box_blur_alpha(&mut alpha, 2, 2, 0);
        assert_eq!(alpha, before);
    }

    #[test]
    fn interior_of_a_uniform_field_stays_uniform() {
        let (w, h) = (40, 40);
        let mut alpha = vec![255u8; (w * h) as usize];
        box_blur_alpha(&mut alpha, w, h, 6);
        let center = alpha[(h / 2 * w + w / 2) as usize];
        assert_eq!(
            center, 255,
            "a uniform field must blur to itself away from any edge"
        );
    }

    #[test]
    fn a_hard_edge_becomes_a_soft_gradient() {
        let (w, h) = (60, 60);
        let mut alpha = vec![0u8; (w * h) as usize];
        for y in 0..h {
            for x in (w / 2)..w {
                alpha[(y * w + x) as usize] = 255;
            }
        }
        box_blur_alpha(&mut alpha, w, h, 8);

        let row = h / 2 * w;
        let boundary = alpha[(row + w / 2) as usize];
        assert!(
            boundary > 0 && boundary < 255,
            "the pixel right at the old hard edge must now be a mid value, got {boundary}"
        );

        let far_left = alpha[(row) as usize];
        let far_right = alpha[(row + w - 1) as usize];
        assert!(
            far_left < boundary,
            "far from the edge on the 0 side must stay darker"
        );
        assert!(
            far_right > boundary,
            "far from the edge on the 255 side must stay brighter"
        );
    }

    #[test]
    fn blur_is_symmetric_for_a_symmetric_shape() {
        let (w, h) = (41, 41);
        let mut alpha = vec![0u8; (w * h) as usize];
        alpha[(h / 2 * w + w / 2) as usize] = 255;
        box_blur_alpha(&mut alpha, w, h, 10);

        let center_row = h / 2 * w;
        let left = alpha[(center_row + w / 2 - 5) as usize];
        let right = alpha[(center_row + w / 2 + 5) as usize];
        assert_eq!(left, right, "a centred point must blur symmetrically");
    }

    #[test]
    fn larger_radius_spreads_further() {
        let (w, h) = (120, 120);
        let half_plane = |radius: i32| -> u8 {
            let mut alpha = vec![0u8; (w * h) as usize];
            for y in 0..h {
                for x in (w / 2)..w {
                    alpha[(y * w + x) as usize] = 255;
                }
            }
            box_blur_alpha(&mut alpha, w, h, radius);
            let row = h / 2 * w;
            alpha[(row + w / 2 + 15) as usize]
        };

        let small = half_plane(2);
        let large = half_plane(12);
        assert!(
            large < small,
            "15px past the edge, a larger blur radius must have bled the dark side through further \
             (a lower value here), a small radius should have already fully recovered to 255: \
             small={small} large={large}"
        );
    }

    #[test]
    fn does_not_panic_on_a_buffer_smaller_than_declared() {
        let mut alpha = vec![1u8; 4];
        box_blur_alpha(&mut alpha, 100, 100, 5);
    }
}

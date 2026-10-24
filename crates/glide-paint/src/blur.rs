pub fn box_blur_rgba(buf: &mut [u8], width: i32, height: i32, radius: i32) {
    blur::<4>(buf, width, height, radius);
}

pub(crate) fn box_blur_alpha(alpha: &mut [u8], width: i32, height: i32, radius: i32) {
    blur::<1>(alpha, width, height, radius);
}

fn blur<const C: usize>(buf: &mut [u8], width: i32, height: i32, radius: i32) {
    let (Ok(w), Ok(h), Ok(r)) = (usize::try_from(width), usize::try_from(height), usize::try_from(radius)) else {
        return;
    };
    let row = w * C;
    if w == 0 || h == 0 || r == 0 || buf.len() < row * h {
        return;
    }
    let band_rows = h.div_ceil(bands(row * h).min(h));
    blur_banded::<C>(&mut buf[..row * h], w, r, band_rows);
}

fn blur_banded<const C: usize>(buf: &mut [u8], w: usize, r: usize, band_rows: usize) {
    let row = w * C;
    let h = buf.len() / row;
    let means: Vec<Mean> = (0..=(2 * r + 1).min(w.max(h))).map(Mean::of_count).collect();
    let mut scratch = vec![0u8; buf.len()];
    let band = row * band_rows;
    for _ in 0..3 {
        let source = &*buf;
        in_bands(&mut scratch, band, |start, out| {
            let lines = source[start..start + out.len()].chunks_exact(row);
            lines.zip(out.chunks_exact_mut(row)).for_each(|(src, dst)| blur_row::<C>(src, dst, r, &means));
        });
        in_bands(buf, band, |start, out| blur_columns(&scratch, out, row, r, &means, start / row));
    }
}

const BYTES_PER_BAND: usize = 1 << 18;

pub(crate) fn bands(bytes: usize) -> usize {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get).min(8);
    if bytes < 2 * BYTES_PER_BAND { 1 } else { cores.min(bytes / BYTES_PER_BAND) }
}

pub(crate) fn in_bands(buf: &mut [u8], band: usize, work: impl Fn(usize, &mut [u8]) + Sync) {
    if buf.len() <= band {
        work(0, buf);
        return;
    }
    std::thread::scope(|scope| {
        for (index, chunk) in buf.chunks_mut(band).enumerate() {
            let work = &work;
            scope.spawn(move || work(index * band, chunk));
        }
    });
}

#[derive(Debug, Clone, Copy)]
struct Mean {
    half: u32,
    reciprocal: u64,
}

impl Mean {
    fn of_count(count: usize) -> Self {
        let count = count.max(1) as u64;
        Self { half: (count / 2) as u32, reciprocal: (1 << 48) / count + 1 }
    }

    fn apply(self, sum: u32) -> u8 {
        ((u64::from(sum + self.half) * self.reciprocal) >> 48).min(255) as u8
    }
}

fn blur_row<const C: usize>(src: &[u8], dst: &mut [u8], r: usize, means: &[Mean]) {
    let len = src.len() / C;
    let pixel = |i: usize| -> [u32; C] { std::array::from_fn(|c| u32::from(src[i * C + c])) };
    let mut count = r.min(len - 1) + 1;
    let mut sums = [0u32; C];
    for i in 0..count {
        sums.iter_mut().zip(pixel(i)).for_each(|(sum, v)| *sum += v);
    }
    for (i, out) in dst.chunks_exact_mut(C).enumerate() {
        let mean = means[count];
        out.iter_mut().zip(sums).for_each(|(px, sum)| *px = mean.apply(sum));
        if i >= r {
            sums.iter_mut().zip(pixel(i - r)).for_each(|(sum, v)| *sum -= v);
            count -= 1;
        }
        if i + r + 1 < len {
            sums.iter_mut().zip(pixel(i + r + 1)).for_each(|(sum, v)| *sum += v);
            count += 1;
        }
    }
}

fn blur_columns(src: &[u8], dst: &mut [u8], row: usize, r: usize, means: &[Mean], first: usize) {
    let rows = src.len() / row;
    let line = |y: usize| &src[y * row..(y + 1) * row];
    let add = |sums: &mut [u32], y: usize| sums.iter_mut().zip(line(y)).for_each(|(sum, &v)| *sum += u32::from(v));
    let remove = |sums: &mut [u32], y: usize| sums.iter_mut().zip(line(y)).for_each(|(sum, &v)| *sum -= u32::from(v));
    let mut sums = vec![0u32; row];
    let window = first.saturating_sub(r)..=(first + r).min(rows - 1);
    let mut count = window.clone().count();
    window.for_each(|y| add(&mut sums, y));
    for (y, out) in dst.chunks_exact_mut(row).enumerate().map(|(offset, out)| (first + offset, out)) {
        let mean = means[count];
        out.iter_mut().zip(sums.iter()).for_each(|(px, &sum)| *px = mean.apply(sum));
        if y >= r {
            remove(&mut sums, y - r);
            count -= 1;
        }
        if y + r + 1 < rows {
            add(&mut sums, y + r + 1);
            count += 1;
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
        assert_eq!(center, 255, "a uniform field must blur to itself away from any edge");
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
        assert!(far_left < boundary, "far from the edge on the 0 side must stay darker");
        assert!(far_right > boundary, "far from the edge on the 255 side must stay brighter");
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
    fn the_window_mean_rounds_to_nearest_instead_of_truncating() {
        let of = |sum: usize, count: usize| usize::from(Mean::of_count(count).apply(sum as u32));
        assert_eq!(of(3, 2), 2);
        assert_eq!(of(5, 3), 2);
        assert_eq!(of(4, 3), 1);
        assert_eq!(of(255 * 7, 7), 255);
        for count in (1..=5000).chain([65_536, 1_000_000]) {
            for sum in [0, 1, count / 2, count, 127 * count + count / 2, 255 * count - 1, 255 * count] {
                assert_eq!(of(sum, count), (sum + count / 2) / count, "{sum} / {count}");
            }
        }
    }

    fn reference(buf: &mut [u8], width: usize, height: usize, radius: usize, channels: usize) {
        let pass = |src: &[u8], dst: &mut [u8], horizontal: bool| {
            let (len, lines) = if horizontal { (width, height) } else { (height, width) };
            for channel in 0..channels {
                for line in 0..lines {
                    let at = |i: usize| {
                        let (x, y) = if horizontal { (i, line) } else { (line, i) };
                        (y * width + x) * channels + channel
                    };
                    for i in 0..len {
                        let window = i.saturating_sub(radius)..=(i + radius).min(len - 1);
                        let count = window.clone().count();
                        let sum: usize = window.map(|j| usize::from(src[at(j)])).sum();
                        dst[at(i)] = ((sum + count / 2) / count) as u8;
                    }
                }
            }
        };
        let mut scratch = vec![0u8; buf.len()];
        for _ in 0..3 {
            pass(buf, &mut scratch, true);
            pass(&scratch, buf, false);
        }
    }

    #[test]
    fn the_blur_matches_a_plain_window_mean_to_the_bit() {
        let mut seed = 0x2545_f491_u32;
        for (w, h, r, channels) in [(17, 9, 3, 4), (5, 23, 7, 1), (31, 31, 40, 4), (1, 12, 2, 1), (12, 1, 5, 4)] {
            let mut buf: Vec<u8> = (0..w * h * channels)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    (seed >> 24) as u8
                })
                .collect();
            let mut expected = buf.clone();
            reference(&mut expected, w, h, r, channels);
            for band_rows in [h, 1, 2, 4] {
                let mut banded = buf.clone();
                if channels == 4 {
                    blur_banded::<4>(&mut banded, w, r, band_rows);
                } else {
                    blur_banded::<1>(&mut banded, w, r, band_rows);
                }
                assert_eq!(banded, expected, "{w}x{h} r={r} channels={channels} bands of {band_rows} rows");
            }
            let blur = if channels == 4 { box_blur_rgba } else { box_blur_alpha };
            blur(&mut buf, w as i32, h as i32, r as i32);
            assert_eq!(buf, expected, "{w}x{h} r={r} channels={channels}");
        }
    }

    #[test]
    fn a_non_uniform_field_keeps_its_average_brightness() {
        let (w, h) = (64, 64);
        let mut alpha: Vec<u8> = (0..w * h).map(|i| if (i / w + i % w) % 2 == 0 { 101 } else { 102 }).collect();
        let before: u64 = alpha.iter().map(|&v| u64::from(v)).sum();
        box_blur_alpha(&mut alpha, w, h, 4);
        let after: u64 = alpha.iter().map(|&v| u64::from(v)).sum();
        let pixels = u64::try_from(w * h).expect("positive");
        assert!(
            before.abs_diff(after) < pixels / 4,
            "blurring must not systematically darken: before={before} after={after}"
        );
    }

    #[test]
    fn does_not_panic_on_a_buffer_smaller_than_declared() {
        let mut alpha = vec![1u8; 4];
        box_blur_alpha(&mut alpha, 100, 100, 5);
    }
}

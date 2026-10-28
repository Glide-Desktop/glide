use tiny_skia::{Pixmap, Stroke};

use crate::{Color, bgr, copy_pixmap_into, fill_solid, over, rounded_rect, stroke_solid};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowLayerSpec {
    pub dy: f32,
    pub blur: f32,
    pub color: Color,
}

pub const SHADOW_DY_RATIO: f32 = 0.43;

#[must_use]
pub fn css_blur_box_radius(blur: f32) -> i32 {
    let sigma = blur.max(0.0) / 2.0;
    ((sigma * sigma + 0.25).sqrt() - 0.5).round() as i32
}

#[must_use]
pub fn shadow_fade_margin(size: i32) -> i32 {
    if size <= 0 {
        return 0;
    }
    (size as f32 * (1.0 + SHADOW_DY_RATIO)).ceil() as i32
}

pub fn draw_inset_border(buf: &mut [u8], width: i32, height: i32, border_width: f32, radius: f32, color: Color) {
    if border_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = over(buf, width, height, 1) else {
        return;
    };
    let inset = border_width / 2.0;
    let inner_radius = (radius - inset).max(0.0);
    let fw = width as f32;
    let fh = height as f32;

    let path = rounded_rect(inset, inset, fw - border_width, fh - border_width, inner_radius);
    stroke_solid(&mut pixmap, path, bgr(color), &plain_stroke(border_width));
}

fn plain_stroke(width: f32) -> Stroke {
    Stroke { width, ..Stroke::default() }
}

fn rasterize_rounded_rect_alpha(
    canvas_w: i32, canvas_h: i32, rect_x: f32, rect_y: f32, rect_w: f32, rect_h: f32, radius: f32,
) -> Option<Vec<u8>> {
    let (Ok(w), Ok(h)) = (u32::try_from(canvas_w), u32::try_from(canvas_h)) else {
        return None;
    };
    let mut pixmap = Pixmap::new(w, h)?;
    let path = rounded_rect(rect_x, rect_y, rect_w, rect_h, radius)?;
    fill_solid(&mut pixmap.as_mut(), Some(path), Color([0.0, 0.0, 0.0, 1.0]));
    Some(pixmap.data().chunks_exact(4).map(|px| px[3]).collect())
}

fn composite_solid_mask_over(data: &mut [u8], mask: &[u8], straight_rgb: [f32; 3], base_alpha: f32) {
    for (px, &m) in data.chunks_exact_mut(4).zip(mask.iter()) {
        let src_a = (f32::from(m) / 255.0) * base_alpha;
        if src_a <= 0.0 {
            continue;
        }
        let inv = 1.0 - src_a;
        for (channel, &straight) in px.iter_mut().take(3).zip(straight_rgb.iter()) {
            let src_premul = straight * src_a * 255.0;
            *channel = (src_premul + f32::from(*channel) * inv).round().clamp(0.0, 255.0) as u8;
        }
        px[3] = (src_a * 255.0 + f32::from(px[3]) * inv).round().clamp(0.0, 255.0) as u8;
    }
}

pub fn blit_rounded_backdrop(
    dst: &mut [u8], dst_w: i32, dst_h: i32, radius: f32, src: &[u8], src_w: i32, src_h: i32,
    src_rect: (i32, i32, i32, i32),
) -> bool {
    let (Ok(dw), Ok(dh)) = (usize::try_from(dst_w), usize::try_from(dst_h)) else {
        return false;
    };
    if dw == 0 || dh == 0 || dst.len() < dw * dh * 4 {
        return false;
    }
    let (Ok(sw), Ok(sh)) = (usize::try_from(src_w), usize::try_from(src_h)) else {
        return false;
    };
    if sw == 0 || sh == 0 || src.len() < sw * sh * 4 {
        return false;
    }

    let (rx, ry, rw, rh) = src_rect;
    if rw <= 0 || rh <= 0 {
        return false;
    }

    let Some(mask) = rasterize_rounded_rect_alpha(dst_w, dst_h, 0.0, 0.0, dst_w as f32, dst_h as f32, radius) else {
        return false;
    };

    for y in 0..dh {
        let sy = (ry + (y as i32 * rh) / dst_h).clamp(0, src_h - 1) as usize;
        for x in 0..dw {
            let m = mask[y * dw + x];
            if m == 0 {
                continue;
            }
            let sx = (rx + (x as i32 * rw) / dst_w).clamp(0, src_w - 1) as usize;
            let s = (sy * sw + sx) * 4;
            let d = (y * dw + x) * 4;
            let coverage = f32::from(m) / 255.0;
            for channel in 0..4 {
                dst[d + channel] = (f32::from(src[s + channel]) * coverage).round() as u8;
            }
        }
    }
    true
}

fn punch_out_window(
    data: &mut [u8], buf_width: i32, buf_height: i32, rect_x: f32, rect_y: f32, window_width: f32, window_height: f32,
    punch_radius: f32,
) {
    let Some(mask) =
        rasterize_rounded_rect_alpha(buf_width, buf_height, rect_x, rect_y, window_width, window_height, punch_radius)
    else {
        return;
    };
    for (px, &m) in data.chunks_exact_mut(4).zip(mask.iter()) {
        if m == 0 {
            continue;
        }
        let keep = f32::from(255 - m) / 255.0;
        for channel in px.iter_mut() {
            *channel = (f32::from(*channel) * keep).round() as u8;
        }
    }
}

#[allow(clippy::many_single_char_names)]
pub fn draw_outset_shadow_and_border(
    buf: &mut [u8], buf_width: i32, buf_height: i32, window_width: f32, window_height: f32,
    shadow_layers: &[ShadowLayerSpec], border_width: f32, border_color: Color, window_radius: f32,
    punch_radius: Option<f32>,
) {
    let (Ok(w), Ok(h)) = (u32::try_from(buf_width), u32::try_from(buf_height)) else {
        return;
    };
    if w == 0 || h == 0 {
        return;
    }
    let Some(mut pixmap) = Pixmap::new(w, h) else {
        return;
    };

    let rect_x = (buf_width as f32 - window_width) / 2.0;
    let rect_y = (buf_height as f32 - window_height) / 2.0;

    for layer in shadow_layers {
        if layer.color.0[3] <= 0.0 || layer.blur < 0.0 {
            continue;
        }
        let Some(mut alpha) = rasterize_rounded_rect_alpha(
            buf_width,
            buf_height,
            rect_x,
            rect_y + layer.dy,
            window_width,
            window_height,
            window_radius,
        ) else {
            continue;
        };
        let box_radius = css_blur_box_radius(layer.blur);
        crate::blur::box_blur_alpha(&mut alpha, buf_width, buf_height, box_radius);

        let [r, g, b, a] = layer.color.0;
        composite_solid_mask_over(pixmap.data_mut(), &alpha, [r, g, b], a);
    }

    if let Some(punch_radius) = punch_radius {
        punch_out_window(
            pixmap.data_mut(),
            buf_width,
            buf_height,
            rect_x,
            rect_y,
            window_width,
            window_height,
            punch_radius,
        );
    }

    if border_width > 0.0 {
        let half_stroke = border_width / 2.0;
        let (ring_w, ring_h) = (window_width + half_stroke * 2.0, window_height + half_stroke * 2.0);
        let path = rounded_rect(
            buf_width as f32 / 2.0 - ring_w / 2.0,
            buf_height as f32 / 2.0 - ring_h / 2.0,
            ring_w,
            ring_h,
            window_radius + half_stroke,
        );
        stroke_solid(&mut pixmap.as_mut(), path, border_color, &plain_stroke(border_width));
    }
    copy_pixmap_into(buf, pixmap);
}

pub struct OutsetShadow {
    pixels: Vec<u8>,
    side: i32,
    split: i32,
}

impl std::fmt::Debug for OutsetShadow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutsetShadow").field("side", &self.side).field("split", &self.split).finish_non_exhaustive()
    }
}

impl OutsetShadow {
    #[must_use]
    pub fn new(
        margin: i32, shadow_layers: &[ShadowLayerSpec], border_width: f32, border_color: Color, window_radius: f32,
        punch_radius: Option<f32>,
    ) -> Self {
        let reach = shadow_layers
            .iter()
            .map(|layer| 3 * css_blur_box_radius(layer.blur) + layer.dy.abs().ceil() as i32)
            .max()
            .unwrap_or(0);
        let radius = window_radius.max(punch_radius.unwrap_or(0.0)).max(0.0);
        let inside = reach + (radius + border_width.max(0.0)).ceil() as i32 + 2;
        let window = inside * 2 + 1;
        let side = window + margin.max(0) * 2;
        let mut pixels = vec![0u8; usize::try_from(side * side * 4).unwrap_or_default()];
        draw_outset_shadow_and_border(
            &mut pixels,
            side,
            side,
            window as f32,
            window as f32,
            shadow_layers,
            border_width,
            border_color,
            window_radius,
            punch_radius,
        );
        Self { pixels, side, split: margin.max(0) + inside }
    }

    #[must_use]
    pub const fn fits(&self, buf_width: i32, buf_height: i32) -> bool {
        buf_width >= self.side && buf_height >= self.side
    }

    pub fn compose(&self, buf: &mut [u8], buf_width: i32, buf_height: i32) -> bool {
        let (Ok(width), Ok(height)) = (usize::try_from(buf_width), usize::try_from(buf_height)) else {
            return false;
        };
        if !self.fits(buf_width, buf_height) || buf.len() < width * height * 4 {
            return false;
        }
        let (side, split) = (self.side as usize, self.split as usize);
        let tail = side - split - 1;
        let stride = width * 4;

        let line = |source: usize, out: &mut [u8]| {
            let row = &self.pixels[source * side * 4..(source + 1) * side * 4];
            out[..split * 4].copy_from_slice(&row[..split * 4]);
            let middle = &row[split * 4..split * 4 + 4];
            for pixel in out[split * 4..stride - tail * 4].chunks_exact_mut(4) {
                pixel.copy_from_slice(middle);
            }
            out[stride - tail * 4..stride].copy_from_slice(&row[(split + 1) * 4..]);
        };

        let rows = &mut buf[..stride * height];
        let (top, rest) = rows.split_at_mut(stride * split);
        let (middle, bottom) = rest.split_at_mut(stride * (height - split - tail));
        for (y, out) in top.chunks_exact_mut(stride).enumerate() {
            line(y, out);
        }
        for (y, out) in bottom.chunks_exact_mut(stride).enumerate() {
            line(split + 1 + y, out);
        }
        let mut lines = middle.chunks_exact_mut(stride);
        if let Some(first) = lines.next() {
            line(split, first);
            let first: &[u8] = first;
            for out in lines {
                out.copy_from_slice(first);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLEAR: Color = Color([0.0; 4]);
    const RED: Color = Color([1.0, 0.0, 0.0, 1.0]);

    fn black(dy: f32, blur: f32, alpha: f32) -> ShadowLayerSpec {
        ShadowLayerSpec { dy, blur, color: Color([0.0, 0.0, 0.0, alpha]) }
    }

    fn pixel(buf: &[u8], buf_w: i32, x: i32, y: i32) -> [u8; 4] {
        let off = ((y * buf_w + x) * 4) as usize;
        buf[off..off + 4].try_into().unwrap()
    }

    fn render(
        side: i32, window: (f32, f32), layers: &[ShadowLayerSpec], border: (f32, Color), radius: f32,
        punch: Option<f32>,
    ) -> Vec<u8> {
        let mut buf = vec![0u8; (side * side * 4) as usize];
        let (w, h) = window;
        draw_outset_shadow_and_border(&mut buf, side, side, w, h, layers, border.0, border.1, radius, punch);
        buf
    }

    #[test]
    fn rounded_backdrops_crop_clamp_and_keep_their_corners_clear() {
        let source = |value| vec![value; 64 * 64 * 4];
        let alpha = |buf: &[u8], x: usize, y: usize| buf[(y * 32 + x) * 4 + 3];
        let blit = |src: &[u8], radius, rect| {
            let mut dst = vec![0u8; 32 * 32 * 4];
            assert!(blit_rounded_backdrop(&mut dst, 32, 32, radius, src, 64, 64, rect));
            dst
        };
        assert_eq!(alpha(&blit(&source(200), 8.0, (16, 16, 32, 32)), 16, 16), 200);
        let rounded = blit(&source(255), 12.0, (0, 0, 64, 64));
        for (x, y) in [(0, 0), (31, 0), (0, 31), (31, 31)] {
            assert_eq!(alpha(&rounded, x, y), 0);
        }
        assert_eq!(alpha(&blit(&source(255), 0.0, (0, 0, 64, 64)), 0, 0), 255);
        assert_eq!(alpha(&blit(&source(128), 0.0, (48, 48, 64, 64)), 31, 31), 128);

        let (src, mut dst) = (vec![255u8; 8 * 8 * 4], vec![0u8; 8 * 8 * 4]);
        assert!(!blit_rounded_backdrop(&mut dst, 8, 8, 2.0, &src, 8, 8, (0, 0, 0, 8)));
        assert!(!blit_rounded_backdrop(&mut dst, 0, 8, 2.0, &src, 8, 8, (0, 0, 8, 8)));
        assert!(!blit_rounded_backdrop(&mut dst, 8, 8, 2.0, &[0; 4], 8, 8, (0, 0, 8, 8)));
    }

    #[test]
    fn degenerate_and_invisible_shadows_paint_nothing() {
        let mut tiny = vec![0u8; 4];
        for side in [0, -5] {
            draw_outset_shadow_and_border(&mut tiny, side, side, 10.0, 10.0, &[], 1.0, CLEAR, 4.0, None);
        }
        render(60, (0.0, 0.0), &[black(5.0, 20.0, 0.5)], (0.0, CLEAR), 4.0, None);
        render(40, (20.0, 20.0), &[black(0.0, -5.0, 1.0)], (0.0, CLEAR), 4.0, None);
        let invisible = render(60, (20.0, 20.0), &[black(6.0, 20.0, 0.0)], (0.0, CLEAR), 4.0, None);
        assert!(invisible.iter().all(|&b| b == 0));
    }

    #[test]
    fn the_border_ring_is_drawn_opaque_in_its_colour_and_survives_a_rounded_punch() {
        let px = pixel(&render(60, (40.0, 40.0), &[], (2.0, RED), 6.0, None), 60, 9, 30);
        assert!(px[3] == 255 && px[2] > px[0], "{px:?}");
        let punched = render(100, (60.0, 60.0), &[], (2.0, RED), 8.0, Some(8.0));
        assert!(pixel(&punched, 100, 19, 50)[3] > 200);
    }

    #[test]
    fn shadows_fall_downwards_and_stack_beyond_the_window() {
        let offset = render(120, (40.0, 40.0), &[black(15.0, 24.0, 1.0)], (0.0, CLEAR), 6.0, None);
        assert!(pixel(&offset, 120, 60, 100)[3] > pixel(&offset, 120, 60, 20)[3]);
        let layers = [black(3.0, 8.0, 1.0), black(12.0, 40.0, 1.0)];
        let stacked = render(140, (40.0, 40.0), &layers, (0.0, CLEAR), 6.0, None);
        assert!(pixel(&stacked, 140, 70, 120)[3] > 0);
    }

    #[test]
    fn punching_clears_the_interior_but_keeps_the_outside_ring() {
        let layers = [black(0.0, 16.0, 1.0)];
        let shadow = |punch| render(120, (60.0, 60.0), &layers, (0.0, CLEAR), 8.0, punch);
        assert!(pixel(&shadow(None), 120, 60, 60)[3] > 0);
        let punched = shadow(Some(0.0));
        assert_eq!(pixel(&punched, 120, 60, 60)[3], 0);
        assert_eq!(pixel(&punched, 120, 31, 31)[3], 0);
        assert!(pixel(&punched, 120, 26, 60)[3] > 0);
    }

    #[test]
    fn inset_borders_stroke_the_edge_and_leave_the_inside_alone() {
        let (w, h) = (60, 30);
        let bg = [40u8, 30, 20, 255];
        let mut buf: Vec<u8> = bg.iter().copied().cycle().take((w * h * 4) as usize).collect();
        draw_inset_border(&mut buf, w, h, 2.0, 6.0, RED);
        assert_eq!(pixel(&buf, w, w / 2, h / 2), bg);
        assert!(buf.chunks_exact(4).all(|px| px[3] == 255));
        assert!(pixel(&buf, w, 1, h / 2)[2] > bg[2]);

        let mut empty = vec![0u8; 40 * 20 * 4];
        draw_inset_border(&mut empty, 40, 20, 2.0, 4.0, Color([0.0, 1.0, 0.0, 1.0]));
        assert_eq!(pixel(&empty, 40, 20, 10)[3], 0);
        assert!(pixel(&empty, 40, 1, 10)[3] > 0);
    }

    #[test]
    fn a_composed_shadow_is_the_direct_raster_up_to_corner_antialiasing_noise() {
        let ring = Color([1.0, 1.0, 1.0, 0.12]);
        for scale in [1, 2] {
            let layers: Vec<ShadowLayerSpec> =
                glide_theme::elevation::layers(glide_theme::elevation::Level::ElWin, glide_theme::Appearance::Dark)
                    .into_iter()
                    .filter(|layer| layer.blur > 0.0)
                    .map(|layer| ShadowLayerSpec {
                        dy: layer.dy * scale as f32,
                        blur: layer.blur * scale as f32,
                        color: layer.color,
                    })
                    .collect();
            let margin = shadow_fade_margin(68) * scale + scale;
            for (radius, punch) in [(12.0 * scale as f32, Some(12.0 * scale as f32)), (0.0, Some(0.0))] {
                let shadow = OutsetShadow::new(margin, &layers, scale as f32, ring, radius, punch);
                for (w, h) in [(shadow.side - margin * 2, shadow.side - margin * 2), (720 * scale, 430 * scale)] {
                    let (buf_w, buf_h) = (w + margin * 2, h + margin * 2);
                    let mut direct = vec![0u8; (buf_w * buf_h * 4) as usize];
                    draw_outset_shadow_and_border(
                        &mut direct,
                        buf_w,
                        buf_h,
                        w as f32,
                        h as f32,
                        &layers,
                        scale as f32,
                        ring,
                        radius,
                        punch,
                    );
                    let mut composed = vec![7u8; direct.len()];
                    assert!(shadow.compose(&mut composed, buf_w, buf_h));
                    let differs = direct.iter().zip(&composed).filter(|(a, b)| a != b).count();
                    let worst = direct.iter().zip(&composed).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
                    assert!(
                        worst <= 2 && differs <= 16,
                        "scale {scale}, radius {radius}, {w}x{h}: {differs} bytes differ, by up to {worst}"
                    );
                }
                assert!(!shadow.fits(shadow.side - 1, shadow.side), "a smaller window is rastered directly");
            }
        }
    }

    #[test]
    fn three_box_passes_match_the_css_blur_sigma() {
        for blur in [4.0_f32, 16.0, 24.0, 52.0, 68.0] {
            let r = css_blur_box_radius(blur);
            let sigma = ((r * (r + 1)) as f32).sqrt();
            assert!((sigma - blur / 2.0).abs() <= 0.75, "blur {blur}");
        }
        assert_eq!(css_blur_box_radius(0.0), 0);
    }

    #[test]
    fn shadow_fade_margin_fades_an_offset_shadow_that_a_plain_margin_clips() {
        assert_eq!((shadow_fade_margin(0), shadow_fade_margin(-5)), (0, 0));
        assert!(shadow_fade_margin(68) > 68);
        let layer = black(68.0 * SHADOW_DY_RATIO, 68.0, 1.0);
        let bottom_alpha = |margin: i32| {
            let (buf_w, buf_h) = (200 + margin * 2, 120 + margin * 2);
            let mut buf = vec![0u8; (buf_w * buf_h * 4) as usize];
            draw_outset_shadow_and_border(&mut buf, buf_w, buf_h, 200.0, 120.0, &[layer], 0.0, CLEAR, 0.0, None);
            pixel(&buf, buf_w, buf_w / 2, buf_h - 1)[3]
        };
        let naive = bottom_alpha(68);
        assert!(naive > 20);
        assert!(bottom_alpha(shadow_fade_margin(68)) < naive);
    }
}

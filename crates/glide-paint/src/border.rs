use tiny_skia::{FillRule, Paint, Pixmap, Shader, Stroke, Transform};

use crate::{Color, rounded_rect, skia_pixmap_into_buffer_bytes, to_skia_color};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowLayerSpec {
    pub dy: f32,
    pub blur: f32,
    pub color: Color,
}

pub const SHADOW_DY_RATIO: f32 = 0.43;

#[must_use]
pub fn shadow_fade_margin(size: i32) -> i32 {
    if size <= 0 {
        return 0;
    }
    #[allow(clippy::cast_precision_loss)]
    let margin = size as f32 * (1.0 + SHADOW_DY_RATIO);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        margin.ceil() as i32
    }
}

pub(crate) fn composite_premul_over(dst: &mut [u8], src: &[u8]) {
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        let src_a = s[3];
        if src_a == 255 {
            d.copy_from_slice(s);
            continue;
        }
        if src_a == 0 {
            continue;
        }
        let inv = 1.0 - f32::from(src_a) / 255.0;
        for (dc, &sc) in d.iter_mut().zip(s.iter()) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                *dc = (f32::from(sc) + f32::from(*dc) * inv).round().min(255.0) as u8;
            }
        }
    }
}

pub fn draw_inset_border(
    buf: &mut [u8],
    width: i32,
    height: i32,
    border_width: f32,
    radius: f32,
    color: Color,
) {
    let (Ok(w), Ok(h)) = (u32::try_from(width), u32::try_from(height)) else {
        return;
    };
    if w == 0 || h == 0 || border_width <= 0.0 {
        return;
    }
    let Some(mut pixmap) = Pixmap::new(w, h) else {
        return;
    };

    let inset = border_width / 2.0;
    let inner_radius = (radius - inset).max(0.0);
    let fw = width as f32;
    let fh = height as f32;

    if let Some(path) = rounded_rect(
        inset,
        inset,
        fw - border_width,
        fh - border_width,
        inner_radius,
    ) {
        let paint = Paint {
            shader: Shader::SolidColor(to_skia_color(color)),
            anti_alias: true,
            ..Paint::default()
        };
        let stroke = Stroke {
            width: border_width,
            ..Stroke::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    composite_premul_over(&mut buf[..n], &bytes[..n]);
}

fn stroke_ring(
    pixmap: &mut Pixmap,
    center_x: f32,
    center_y: f32,
    width: f32,
    height: f32,
    radius: f32,
    stroke_width: f32,
    color: Color,
) {
    let Some(path) = rounded_rect(
        center_x - width / 2.0,
        center_y - height / 2.0,
        width,
        height,
        radius,
    ) else {
        return;
    };
    let paint = Paint {
        shader: Shader::SolidColor(to_skia_color(color)),
        anti_alias: true,
        ..Paint::default()
    };
    let stroke = Stroke {
        width: stroke_width,
        ..Stroke::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}

fn rasterize_rounded_rect_alpha(
    canvas_w: i32,
    canvas_h: i32,
    rect_x: f32,
    rect_y: f32,
    rect_w: f32,
    rect_h: f32,
    radius: f32,
) -> Option<Vec<u8>> {
    let (Ok(w), Ok(h)) = (u32::try_from(canvas_w), u32::try_from(canvas_h)) else {
        return None;
    };
    let mut pixmap = Pixmap::new(w, h)?;
    let path = rounded_rect(rect_x, rect_y, rect_w, rect_h, radius)?;
    let paint = Paint {
        shader: Shader::SolidColor(tiny_skia::Color::BLACK),
        anti_alias: true,
        ..Paint::default()
    };
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
    Some(pixmap.data().chunks_exact(4).map(|px| px[3]).collect())
}

fn composite_solid_mask_over(
    data: &mut [u8],
    mask: &[u8],
    straight_rgb: [f32; 3],
    base_alpha: f32,
) {
    for (px, &m) in data.chunks_exact_mut(4).zip(mask.iter()) {
        let src_a = (f32::from(m) / 255.0) * base_alpha;
        if src_a <= 0.0 {
            continue;
        }
        let inv = 1.0 - src_a;
        for (channel, &straight) in px.iter_mut().take(3).zip(straight_rgb.iter()) {
            let src_premul = straight * src_a * 255.0;
            *channel = (src_premul + f32::from(*channel) * inv)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
        px[3] = (src_a * 255.0 + f32::from(px[3]) * inv)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
}

#[allow(clippy::too_many_arguments)]
pub fn blit_rounded_backdrop(
    dst: &mut [u8],
    dst_w: i32,
    dst_h: i32,
    radius: f32,
    src: &[u8],
    src_w: i32,
    src_h: i32,
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

    #[allow(clippy::cast_precision_loss)]
    let Some(mask) =
        rasterize_rounded_rect_alpha(dst_w, dst_h, 0.0, 0.0, dst_w as f32, dst_h as f32, radius)
    else {
        return false;
    };

    for y in 0..dh {
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let sy = (ry + (y as i32 * rh) / dst_h).clamp(0, src_h - 1) as usize;
        for x in 0..dw {
            let m = mask[y * dw + x];
            if m == 0 {
                continue;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let sx = (rx + (x as i32 * rw) / dst_w).clamp(0, src_w - 1) as usize;

            let s = (sy * sw + sx) * 4;
            let d = (y * dw + x) * 4;
            let coverage = f32::from(m) / 255.0;
            for channel in 0..4 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    dst[d + channel] = (f32::from(src[s + channel]) * coverage).round() as u8;
                }
            }
        }
    }
    true
}

fn punch_out_window(
    data: &mut [u8],
    buf_width: i32,
    buf_height: i32,
    rect_x: f32,
    rect_y: f32,
    window_width: f32,
    window_height: f32,
    punch_radius: f32,
) {
    let Some(mask) = rasterize_rounded_rect_alpha(
        buf_width,
        buf_height,
        rect_x,
        rect_y,
        window_width,
        window_height,
        punch_radius,
    ) else {
        return;
    };
    for (px, &m) in data.chunks_exact_mut(4).zip(mask.iter()) {
        if m == 0 {
            continue;
        }
        let keep = f32::from(255 - m) / 255.0;
        for channel in px.iter_mut() {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                *channel = (f32::from(*channel) * keep).round() as u8;
            }
        }
    }
}

#[allow(clippy::many_single_char_names, clippy::too_many_arguments)]
pub fn draw_outset_shadow_and_border(
    buf: &mut [u8],
    buf_width: i32,
    buf_height: i32,
    window_width: f32,
    window_height: f32,
    shadow_layers: &[ShadowLayerSpec],
    border_width: f32,
    border_color: Color,
    window_radius: f32,
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
        let box_radius = (layer.blur / 3.0).round() as i32;
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
        stroke_ring(
            &mut pixmap,
            buf_width as f32 / 2.0,
            buf_height as f32 / 2.0,
            window_width + half_stroke * 2.0,
            window_height + half_stroke * 2.0,
            window_radius + half_stroke,
            border_width,
            border_color,
        );
    }

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    buf[..n].copy_from_slice(&bytes[..n]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque(color: Color) -> Color {
        Color([color.0[0], color.0[1], color.0[2], 1.0])
    }

    mod rounded_backdrop {
        use super::*;

        fn source(w: i32, h: i32, value: u8) -> Vec<u8> {
            vec![value; (w * h * 4) as usize]
        }

        fn alpha_at(buf: &[u8], w: i32, x: i32, y: i32) -> u8 {
            buf[((y * w + x) * 4 + 3) as usize]
        }

        #[test]
        fn the_interior_is_filled_from_the_requested_crop() {
            let src = source(64, 64, 200);
            let mut dst = vec![0u8; 32 * 32 * 4];
            assert!(blit_rounded_backdrop(
                &mut dst,
                32,
                32,
                8.0,
                &src,
                64,
                64,
                (16, 16, 32, 32)
            ));
            assert_eq!(alpha_at(&dst, 32, 16, 16), 200, "centre must be opaque");
        }

        #[test]
        fn the_corners_stay_transparent() {
            let src = source(64, 64, 255);
            let mut dst = vec![0u8; 32 * 32 * 4];
            blit_rounded_backdrop(&mut dst, 32, 32, 12.0, &src, 64, 64, (0, 0, 64, 64));

            for (x, y) in [(0, 0), (31, 0), (0, 31), (31, 31)] {
                assert_eq!(
                    alpha_at(&dst, 32, x, y),
                    0,
                    "corner ({x},{y}) must not be painted"
                );
            }
        }

        #[test]
        fn a_zero_radius_keeps_the_corners() {
            let src = source(64, 64, 255);
            let mut dst = vec![0u8; 32 * 32 * 4];
            blit_rounded_backdrop(&mut dst, 32, 32, 0.0, &src, 64, 64, (0, 0, 64, 64));
            assert_eq!(alpha_at(&dst, 32, 0, 0), 255);
        }

        #[test]
        fn a_crop_reaching_past_the_source_is_clamped_not_wrapped() {
            let src = source(64, 64, 128);
            let mut dst = vec![0u8; 32 * 32 * 4];
            assert!(blit_rounded_backdrop(
                &mut dst,
                32,
                32,
                0.0,
                &src,
                64,
                64,
                (48, 48, 64, 64)
            ));
            assert_eq!(alpha_at(&dst, 32, 31, 31), 128);
        }

        #[test]
        fn malformed_inputs_are_refused() {
            let src = source(8, 8, 255);
            let mut dst = vec![0u8; 8 * 8 * 4];
            assert!(!blit_rounded_backdrop(
                &mut dst,
                8,
                8,
                2.0,
                &src,
                8,
                8,
                (0, 0, 0, 8)
            ));
            assert!(!blit_rounded_backdrop(
                &mut dst,
                0,
                8,
                2.0,
                &src,
                8,
                8,
                (0, 0, 8, 8)
            ));
            let short = vec![0u8; 4];
            assert!(!blit_rounded_backdrop(
                &mut dst,
                8,
                8,
                2.0,
                &short,
                8,
                8,
                (0, 0, 8, 8)
            ));
        }
    }

    fn pixel(buf: &[u8], buf_w: i32, x: i32, y: i32) -> [u8; 4] {
        let off = ((y * buf_w + x) * 4) as usize;
        buf[off..off + 4].try_into().unwrap()
    }

    #[test]
    fn does_not_panic_on_degenerate_input() {
        let mut buf = vec![0u8; 4];
        draw_outset_shadow_and_border(
            &mut buf,
            0,
            0,
            10.0,
            10.0,
            &[],
            1.0,
            Color([0.0; 4]),
            4.0,
            None,
        );
        draw_outset_shadow_and_border(
            &mut buf,
            -5,
            -5,
            10.0,
            10.0,
            &[],
            1.0,
            Color([0.0; 4]),
            4.0,
            None,
        );

        let layers = [ShadowLayerSpec {
            dy: 5.0,
            blur: 20.0,
            color: Color([0.0, 0.0, 0.0, 0.5]),
        }];
        let mut buf2 = vec![0u8; (60 * 60 * 4) as usize];
        draw_outset_shadow_and_border(
            &mut buf2,
            60,
            60,
            0.0,
            0.0,
            &layers,
            0.0,
            Color([0.0; 4]),
            4.0,
            None,
        );
    }

    #[test]
    fn border_ring_is_visible_at_the_configured_color() {
        let (buf_w, buf_h) = (60, 60);
        let border_color = Color([1.0, 0.0, 0.0, 1.0]);
        let mut buf = vec![0u8; (buf_w * buf_h * 4) as usize];
        draw_outset_shadow_and_border(
            &mut buf,
            buf_w,
            buf_h,
            40.0,
            40.0,
            &[],
            2.0,
            border_color,
            6.0,
            None,
        );

        let px = pixel(&buf, buf_w, 9, 30);
        assert_eq!(px[3], 255, "border pixel must be opaque");
        assert!(
            px[2] > px[0],
            "expected red-dominant BGRA pixel, got {px:?}"
        );
    }

    #[test]
    fn zero_alpha_shadow_layers_leave_no_visible_trace() {
        let (buf_w, buf_h) = (60, 60);
        let layers = [ShadowLayerSpec {
            dy: 6.0,
            blur: 20.0,
            color: Color([0.0, 0.0, 0.0, 0.0]),
        }];
        let mut buf = vec![0u8; (buf_w * buf_h * 4) as usize];
        draw_outset_shadow_and_border(
            &mut buf,
            buf_w,
            buf_h,
            20.0,
            20.0,
            &layers,
            0.0,
            Color([0.0; 4]),
            4.0,
            None,
        );
        assert!(
            buf.iter().all(|&b| b == 0),
            "a zero-alpha layer must paint nothing"
        );
    }

    #[test]
    fn positive_dy_makes_the_shadow_extend_further_below_than_above() {
        let (buf_w, buf_h) = (120, 120);
        let layers = [ShadowLayerSpec {
            dy: 15.0,
            blur: 24.0,
            color: opaque(Color([0.0, 0.0, 0.0, 1.0])),
        }];
        let mut buf = vec![0u8; (buf_w * buf_h * 4) as usize];
        draw_outset_shadow_and_border(
            &mut buf,
            buf_w,
            buf_h,
            40.0,
            40.0,
            &layers,
            0.0,
            Color([0.0; 4]),
            6.0,
            None,
        );

        let cx = buf_w / 2;
        let window_top = (buf_h - 40) / 2;
        let window_bottom = window_top + 40;

        let above = pixel(&buf, buf_w, cx, window_top - 20)[3];
        let below = pixel(&buf, buf_w, cx, window_bottom + 20)[3];
        assert!(
            below > above,
            "a downward-offset shadow must be more visible below the window than above it: \
             above={above} below={below}"
        );
    }

    #[test]
    fn combining_two_shadow_layers_paints_beyond_the_windows_own_footprint() {
        let (buf_w, buf_h) = (140, 140);
        let tight = ShadowLayerSpec {
            dy: 3.0,
            blur: 8.0,
            color: Color([0.0, 0.0, 0.0, 1.0]),
        };
        let soft = ShadowLayerSpec {
            dy: 12.0,
            blur: 40.0,
            color: Color([0.0, 0.0, 0.0, 1.0]),
        };
        let mut buf = vec![0u8; (buf_w * buf_h * 4) as usize];
        draw_outset_shadow_and_border(
            &mut buf,
            buf_w,
            buf_h,
            40.0,
            40.0,
            &[tight, soft],
            0.0,
            Color([0.0; 4]),
            6.0,
            None,
        );
        let cx = buf_w / 2;
        let window_bottom = (buf_h - 40) / 2 + 40;
        assert!(pixel(&buf, buf_w, cx, window_bottom + 30)[3] > 0);
    }

    #[test]
    fn negative_blur_layers_are_skipped_without_panicking() {
        let layers = [ShadowLayerSpec {
            dy: 0.0,
            blur: -5.0,
            color: Color([0.0, 0.0, 0.0, 1.0]),
        }];
        let mut buf = vec![0u8; (40 * 40 * 4) as usize];
        draw_outset_shadow_and_border(
            &mut buf,
            40,
            40,
            20.0,
            20.0,
            &layers,
            0.0,
            Color([0.0; 4]),
            4.0,
            None,
        );
    }

    #[test]
    fn punching_clears_the_interior_but_keeps_the_outside_ring() {
        let (buf_w, buf_h) = (120, 120);
        let (win_w, win_h) = (60.0, 60.0);
        let layers = [ShadowLayerSpec {
            dy: 0.0,
            blur: 16.0,
            color: Color([0.0, 0.0, 0.0, 1.0]),
        }];

        let render = |punch: Option<f32>| {
            let mut buf = vec![0u8; (buf_w * buf_h * 4) as usize];
            draw_outset_shadow_and_border(
                &mut buf,
                buf_w,
                buf_h,
                win_w,
                win_h,
                &layers,
                0.0,
                Color([0.0; 4]),
                8.0,
                punch,
            );
            buf
        };

        let unpunched = render(None);
        let punched = render(Some(0.0));

        let centre = (buf_w / 2, buf_h / 2);
        assert!(
            pixel(&unpunched, buf_w, centre.0, centre.1)[3] > 0,
            "sanity: without a punch the fill covers the window interior"
        );
        assert_eq!(
            pixel(&punched, buf_w, centre.0, centre.1)[3],
            0,
            "the punched interior must be fully transparent"
        );

        let win_left = (buf_w - 60) / 2;
        let win_top = (buf_h - 60) / 2;
        assert_eq!(
            pixel(&punched, buf_w, win_left + 1, win_top + 1)[3],
            0,
            "window-rect corners must be cleared by a square punch"
        );

        let outside = pixel(&punched, buf_w, win_left - 4, buf_h / 2)[3];
        assert!(
            outside > 0,
            "the shadow ring outside the window must not be erased"
        );
    }

    #[test]
    fn a_rounded_punch_leaves_the_border_ring_intact() {
        let (buf_w, buf_h) = (100, 100);
        let (win_w, win_h) = (60.0_f32, 60.0_f32);
        let radius = 8.0;
        let mut buf = vec![0u8; (buf_w * buf_h * 4) as usize];
        draw_outset_shadow_and_border(
            &mut buf,
            buf_w,
            buf_h,
            win_w,
            win_h,
            &[],
            2.0,
            Color([1.0, 0.0, 0.0, 1.0]),
            radius,
            Some(radius),
        );

        let win_left = (buf_w - 60) / 2;
        let px = pixel(&buf, buf_w, win_left - 1, buf_h / 2);
        assert!(
            px[3] > 200,
            "the border ring must survive a rounded punch, got {px:?}"
        );
    }

    #[test]
    fn drawing_a_border_preserves_the_background_underneath() {
        let (w, h) = (60, 30);
        let bg = [40u8, 30, 20, 255];
        let mut buf: Vec<u8> = bg
            .iter()
            .copied()
            .cycle()
            .take((w * h * 4) as usize)
            .collect();

        draw_inset_border(&mut buf, w, h, 2.0, 6.0, Color([1.0, 0.0, 0.0, 1.0]));

        assert_eq!(
            pixel(&buf, w, w / 2, h / 2),
            bg,
            "the border wiped out the panel background"
        );
        for (i, px) in buf.chunks_exact(4).enumerate() {
            assert_eq!(
                px[3],
                255,
                "pixel {i} lost its opacity (x={}, y={})",
                i as i32 % w,
                i as i32 / w
            );
        }
        let edge = pixel(&buf, w, 1, h / 2);
        assert!(
            edge[2] > bg[2],
            "expected a red-dominant BGRA stroke on the edge, got {edge:?}"
        );
    }

    #[test]
    fn a_border_over_an_empty_buffer_still_just_draws_the_stroke() {
        let (w, h) = (40, 20);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        draw_inset_border(&mut buf, w, h, 2.0, 4.0, Color([0.0, 1.0, 0.0, 1.0]));

        assert_eq!(
            pixel(&buf, w, w / 2, h / 2)[3],
            0,
            "compositing over nothing must leave the interior transparent"
        );
        assert!(pixel(&buf, w, 1, h / 2)[3] > 0, "stroke missing");
    }

    #[test]
    fn shadow_fade_margin_grows_with_size_and_is_zero_for_no_shadow() {
        assert_eq!(shadow_fade_margin(0), 0);
        assert_eq!(shadow_fade_margin(-5), 0);
        assert!(shadow_fade_margin(68) > shadow_fade_margin(32));
        assert!(shadow_fade_margin(68) > 68);
    }

    #[test]
    fn a_dy_offset_shadow_layer_fully_fades_within_shadow_fade_margin_but_not_within_plain_size() {
        let size = 68.0_f32;
        let layer = ShadowLayerSpec {
            dy: size * SHADOW_DY_RATIO,
            blur: size,
            color: Color([0.0, 0.0, 0.0, 1.0]),
        };
        let (window_w, window_h) = (200.0_f32, 120.0_f32);

        let render_with_margin = |margin: i32| -> u8 {
            let buf_w = window_w as i32 + margin * 2;
            let buf_h = window_h as i32 + margin * 2;
            let mut buf = vec![0u8; (buf_w * buf_h * 4) as usize];
            draw_outset_shadow_and_border(
                &mut buf,
                buf_w,
                buf_h,
                window_w,
                window_h,
                &[layer],
                0.0,
                Color([0.0; 4]),
                0.0,
                None,
            );
            let cx = buf_w / 2;
            pixel(&buf, buf_w, cx, buf_h - 1)[3]
        };

        let naive_alpha = render_with_margin(size as i32);
        let fixed_alpha = render_with_margin(shadow_fade_margin(size as i32));

        assert!(
            naive_alpha > 20,
            "sanity check: a plain `size` margin must still be visibly \
             hard-clipped at the offset edge, got alpha={naive_alpha}"
        );
        assert!(
            fixed_alpha < naive_alpha,
            "shadow_fade_margin must fade further than a plain size margin: \
             naive={naive_alpha} fixed={fixed_alpha}"
        );
    }
}

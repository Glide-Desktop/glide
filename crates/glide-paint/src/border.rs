use tiny_skia::{FillRule, Paint, Pixmap, Shader, Stroke, Transform};

use crate::{Color, rounded_rect, skia_pixmap_into_buffer_bytes, to_skia_color};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowLayerSpec {
    pub dy: f32,
    pub blur: f32,
    pub color: Color,
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
    buf[..n].copy_from_slice(&bytes[..n]);
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

#[allow(clippy::many_single_char_names)]
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

    fn pixel(buf: &[u8], buf_w: i32, x: i32, y: i32) -> [u8; 4] {
        let off = ((y * buf_w + x) * 4) as usize;
        buf[off..off + 4].try_into().unwrap()
    }

    #[test]
    fn does_not_panic_on_degenerate_input() {
        let mut buf = vec![0u8; 4];
        draw_outset_shadow_and_border(&mut buf, 0, 0, 10.0, 10.0, &[], 1.0, Color([0.0; 4]), 4.0);
        draw_outset_shadow_and_border(&mut buf, -5, -5, 10.0, 10.0, &[], 1.0, Color([0.0; 4]), 4.0);

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
        );

        // The window's left edge sits at (buf_w - 40) / 2 = 10; the 2px
        // border straddles it, so a point just left of that edge must show
        // the (BGRA) border color.
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
        );
    }
}

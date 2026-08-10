use tiny_skia::{FillRule, Paint, Pixmap, Shader, Stroke, Transform};

use crate::{Color, rounded_rect, skia_pixmap_into_buffer_bytes, to_skia_color};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowLayerSpec {
    pub dy: f32,
    pub blur: f32,
    pub color: Color,
}

/// Ratio between a shadow layer's downward offset (`dy`) and its blur
/// radius, mirroring the relationship baked into every
/// `glide_theme::elevation` entry (its `ShadowLayer::dy`/`blur` pairs all
/// sit close to this ratio). Callers that derive a `dy` from a single
/// configured size — instead of a full elevation-token layer list — use
/// this so the shadow reads as a downward-cast drop shadow rather than a
/// halo that hugs the window symmetrically on every side.
pub const SHADOW_DY_RATIO: f32 = 0.43;

/// How much padding a buffer needs on every side so a shadow layer of the
/// given (unscaled) `size` can fully fade out before `box_blur_alpha`'s
/// support region reaches the buffer edge.
///
/// A `size`-blur, 3-pass box blur needs roughly `size` px of room to fade
/// to (near-)zero — that's why the old, non-offset renderer used
/// `margin = shadow_size` and it worked. Once the shadow is shifted
/// downward by `SHADOW_DY_RATIO * size` (see [`SHADOW_DY_RATIO`]), the
/// *bottom* edge needs that much extra room too, or the blur gets
/// hard-clipped by the buffer boundary instead of fading out — which
/// reads as a dense, sharp-edged halo hugging the window rather than a
/// soft shadow, and can visibly square off what should be a rounded
/// corner right where the clip happens. Sizing the (symmetric) margin for
/// the worst case keeps every side generous enough.
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

/// Source-over blend of `src` onto `dst`, both premultiplied BGRA8.
///
/// `dst = src + dst * (1 - src_alpha)`
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

/// Strokes a rounded-rect border just inside the `width × height` canvas,
/// **blended over** whatever `buf` already holds.
///
/// Compositing (rather than replacing) matters: callers routinely paint a
/// panel background first and then outline it — overwriting would erase
/// that background and leave a hollow, see-through panel with nothing but
/// an outline.
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

/// Erases (multiplies out) everything inside the window's own footprint —
/// a rounded rect of `punch_radius` at `(rect_x, rect_y)` — from the
/// already-composited shadow pixels.
///
/// The shadow layers are *filled* blurred shapes, so without this the
/// area behind the window is painted too. That's invisible behind opaque
/// content, but shows straight through anything the client leaves
/// transparent — most notably the rounded-off corners of CSD windows
/// (GTK etc.), which then read as dark, square corners instead of showing
/// whatever actually sits behind the window.
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
        // Premultiplied pixels — scaling all four channels uniformly is a
        // correct alpha-fade.
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

    /// The shadow layers are filled shapes — without a punch the area
    /// behind the window is painted too, and that fill shows through any
    /// transparent client pixels (CSD rounded corners, translucent
    /// terminals). Punching must clear the interior while leaving the
    /// outside ring intact.
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

        // A corner of the window rect (inside the square punch, outside
        // the shadow shape's rounded outline) must be cleared too — this
        // is exactly where CSD clients leave their corners transparent.
        let win_left = (buf_w - 60) / 2;
        let win_top = (buf_h - 60) / 2;
        assert_eq!(
            pixel(&punched, buf_w, win_left + 1, win_top + 1)[3],
            0,
            "window-rect corners must be cleared by a square punch"
        );

        // Just outside the window rect the blurred ring must survive.
        let outside = pixel(&punched, buf_w, win_left - 4, buf_h / 2)[3];
        assert!(
            outside > 0,
            "the shadow ring outside the window must not be erased"
        );
    }

    /// The border stroke is drawn *after* the punch, so a rounded punch
    /// must never eat the border's corner arcs.
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

        // Mid-edge border pixel, just outside the window's left edge.
        let win_left = (buf_w - 60) / 2;
        let px = pixel(&buf, buf_w, win_left - 1, buf_h / 2);
        assert!(
            px[3] > 200,
            "the border ring must survive a rounded punch, got {px:?}"
        );
    }

    /// Regression test: `draw_inset_border` used to `copy_from_slice` a
    /// fresh, mostly-transparent pixmap over the whole buffer, wiping any
    /// background painted beforehand. Every caller that outlines a filled
    /// panel (the overview label pill, the titlebar context menu) came out
    /// hollow and see-through as a result.
    #[test]
    fn drawing_a_border_preserves_the_background_underneath() {
        let (w, h) = (60, 30);
        // Pre-fill the whole buffer with an opaque background.
        let bg = [40u8, 30, 20, 255];
        let mut buf: Vec<u8> = bg
            .iter()
            .copied()
            .cycle()
            .take((w * h * 4) as usize)
            .collect();

        draw_inset_border(&mut buf, w, h, 2.0, 6.0, Color([1.0, 0.0, 0.0, 1.0]));

        // Dead centre is far from the stroke and must be untouched.
        assert_eq!(
            pixel(&buf, w, w / 2, h / 2),
            bg,
            "the border wiped out the panel background"
        );
        // Every pixel must still be opaque — no holes anywhere.
        for (i, px) in buf.chunks_exact(4).enumerate() {
            assert_eq!(
                px[3],
                255,
                "pixel {i} lost its opacity (x={}, y={})",
                i as i32 % w,
                i as i32 / w
            );
        }
        // And the stroke itself must actually be there, on the edge.
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
        // Bigger than the plain size — that's the whole point (room for
        // the `dy` offset too).
        assert!(shadow_fade_margin(68) > 68);
    }

    /// Reproduces the "glowing halo" / "square corner" bug: a `dy`-offset
    /// shadow layer sized to fit in a plain `size`-px margin gets
    /// hard-clipped by the buffer edge on the side it's offset towards,
    /// instead of fading to (near-)zero alpha like it does on every other
    /// side. `shadow_fade_margin` must give it enough room to actually
    /// finish fading on the offset side too.
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
            // The bottom-center row of the buffer, right at its edge — the
            // side the shadow is offset towards.
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

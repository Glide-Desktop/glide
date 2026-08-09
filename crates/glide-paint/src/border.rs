use tiny_skia::{Paint, Pixmap, Shader, Stroke, Transform};

use crate::{Color, rounded_rect, skia_pixmap_into_buffer_bytes, to_skia_color};

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

#[allow(clippy::too_many_arguments)]
pub fn draw_outset_shadow_and_border(
    buf: &mut [u8],
    buf_width: i32,
    buf_height: i32,
    window_width: f32,
    window_height: f32,
    shadow_size: f32,
    bands: usize,
    base_shadow_color: Color,
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

    let center_x = buf_width as f32 / 2.0;
    let center_y = buf_height as f32 / 2.0;

    if bands > 0 && shadow_size > 0.0 {
        let step = shadow_size / bands as f32;
        let mut outset = 0.0;

        for i in 0..bands {
            let t = (i as f32 + 0.5) / bands as f32;
            let falloff = 1.0 - t;
            let alpha = falloff * falloff;

            let mut color = base_shadow_color;
            color.0[3] *= alpha;

            let current_outset = outset + step / 2.0;
            stroke_ring(
                &mut pixmap,
                center_x,
                center_y,
                window_width + current_outset * 2.0,
                window_height + current_outset * 2.0,
                window_radius + current_outset,
                step,
                color,
            );

            outset += step;
        }
    }

    if border_width > 0.0 {
        let half_stroke = border_width / 2.0;
        stroke_ring(
            &mut pixmap,
            center_x,
            center_y,
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

use tiny_skia::{
    BlendMode, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point, RadialGradient,
    Rect, Shader, SpreadMode, Stroke, Transform,
};

use crate::{Color, skia_pixmap_into_buffer_bytes, to_skia_color};

fn draw_radial(
    pixmap: &mut Pixmap,
    fw: f32,
    fh: f32,
    center_x: f32,
    center_y: f32,
    radius_x: f32,
    radius_y: f32,
    stop_frac: f32,
    color: Color,
) {
    let mut color_transparent = color;
    color_transparent.0[3] = 0.0;

    let scale_y = radius_y / radius_x;

    let shader = RadialGradient::new(
        Point::from_xy(center_x, center_y / scale_y),
        0.0,
        Point::from_xy(center_x, center_y / scale_y),
        radius_x,
        vec![
            GradientStop::new(0.0, to_skia_color(color)),
            GradientStop::new(stop_frac, to_skia_color(color_transparent)),
        ],
        SpreadMode::Pad,
        Transform::from_scale(1.0, scale_y),
    )
    .unwrap_or_else(|| Shader::SolidColor(to_skia_color(color_transparent)));

    let paint = Paint {
        shader,
        blend_mode: BlendMode::SourceOver,
        ..Paint::default()
    };
    if let Some(rect) = Rect::from_xywh(0.0, 0.0, fw, fh) {
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

fn draw_ring(pixmap: &mut Pixmap, paint: &Paint, stroke: &Stroke, cx: f32, cy: f32, r: f32) {
    if let Some(path) = PathBuilder::from_circle(cx, cy, r) {
        pixmap.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

pub fn draw_wallpaper(buf: &mut [u8], width: i32, height: i32) {
    let (Ok(w), Ok(h)) = (u32::try_from(width), u32::try_from(height)) else {
        return;
    };
    let Some(mut pixmap) = Pixmap::new(w, h) else {
        return;
    };
    let fw = width as f32;
    let fh = height as f32;

    let wall1 = Color([34.0 / 255.0, 44.0 / 255.0, 60.0 / 255.0, 1.0]);
    let wall2 = Color([44.0 / 255.0, 36.0 / 255.0, 54.0 / 255.0, 1.0]);
    let wall3 = Color([24.0 / 255.0, 32.0 / 255.0, 44.0 / 255.0, 1.0]);
    let wall4 = Color([36.0 / 255.0, 29.0 / 255.0, 44.0 / 255.0, 1.0]);
    let wall_line = Color([1.0, 1.0, 1.0, 0.14]);

    let angle = (160.0 - 90.0) * std::f32::consts::PI / 180.0;
    let diag = (fw * fw + fh * fh).sqrt();
    let start = Point::from_xy(fw / 2.0 - diag * angle.cos(), fh / 2.0 - diag * angle.sin());
    let end = Point::from_xy(fw / 2.0 + diag * angle.cos(), fh / 2.0 + diag * angle.sin());

    if let Some(shader) = LinearGradient::new(
        start,
        end,
        vec![
            GradientStop::new(0.0, to_skia_color(wall1)),
            GradientStop::new(1.0, to_skia_color(wall3)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    ) {
        let paint = Paint {
            shader,
            ..Paint::default()
        };
        if let Some(rect) = Rect::from_xywh(0.0, 0.0, fw, fh) {
            pixmap.fill_rect(rect, &paint, Transform::identity(), None);
        }
    }

    draw_radial(
        &mut pixmap,
        fw,
        fh,
        fw * 0.50,
        fh * 0.42,
        fw * 0.88,
        fh * 0.68,
        0.70,
        wall3,
    );
    draw_radial(
        &mut pixmap,
        fw,
        fh,
        fw * 0.12,
        fh * 0.88,
        fw * 0.66,
        fh * 0.52,
        0.60,
        wall4,
    );
    draw_radial(
        &mut pixmap,
        fw,
        fh,
        fw * 0.78,
        fh * 0.14,
        fw * 0.56,
        fh * 0.44,
        0.62,
        wall2,
    );

    let ring_paint = Paint {
        shader: Shader::SolidColor(to_skia_color(wall_line)),
        anti_alias: true,
        ..Paint::default()
    };
    let ring_stroke = Stroke {
        width: 1.5,
        ..Stroke::default()
    };
    draw_ring(
        &mut pixmap,
        &ring_paint,
        &ring_stroke,
        -130.0 + 220.0,
        fh - (-180.0 + 220.0),
        220.0,
    );
    draw_ring(
        &mut pixmap,
        &ring_paint,
        &ring_stroke,
        -230.0 + 320.0,
        fh - (-300.0 + 320.0),
        320.0,
    );
    draw_ring(
        &mut pixmap,
        &ring_paint,
        &ring_stroke,
        fw - (-70.0 + 130.0),
        -90.0 + 130.0,
        130.0,
    );

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    buf[..n].copy_from_slice(&bytes[..n]);
}

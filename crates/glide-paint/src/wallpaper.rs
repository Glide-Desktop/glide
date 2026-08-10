use std::sync::{Arc, RwLock};

use tiny_skia::{
    BlendMode, FilterQuality, GradientStop, LinearGradient, Paint, PathBuilder, Pattern, Pixmap,
    Point, RadialGradient, Rect, Shader, SpreadMode, Stroke, Transform,
};

use glide_theme::WallpaperFit;

use crate::{Color, skia_pixmap_into_buffer_bytes, to_skia_color};

static EMBEDDED_WALLPAPER_BYTES: &[u8] = include_bytes!("../assets/wallpaper.png");

#[must_use]
pub fn embedded_wallpaper_bytes() -> &'static [u8] {
    EMBEDDED_WALLPAPER_BYTES
}

#[must_use]
pub fn decode_wallpaper(png_bytes: &[u8]) -> Option<Pixmap> {
    Pixmap::decode_png(png_bytes).ok()
}

static WALLPAPER_SOURCE: RwLock<Option<Arc<Pixmap>>> = RwLock::new(None);

pub fn init_wallpaper(override_bytes: Option<&[u8]>) {
    let decoded = override_bytes
        .and_then(decode_wallpaper)
        .or_else(|| decode_wallpaper(embedded_wallpaper_bytes()));
    let mut slot = WALLPAPER_SOURCE
        .write()
        .expect("single-threaded event loop, this lock is never poisoned");
    *slot = decoded.map(Arc::new);
}

/// Scales every pixel's saturation towards grey by `factor`, in place.
///
/// `1.0` is a no-op; the overview backdrop uses `0.9`, matching the
/// design's `filter: saturate(.9)`. Operates on premultiplied BGRA, so
/// the luma mix is applied to the premultiplied channels directly — that
/// is equivalent to desaturating the straight colour and re-multiplying,
/// because the mix is linear and alpha is untouched.
pub fn desaturate_in_place(buf: &mut [u8], factor: f32) {
    // Rec. 601 luma weights, in buffer (BGRA) channel order.
    const W_B: f32 = 0.114;
    const W_G: f32 = 0.587;
    const W_R: f32 = 0.299;

    if (factor - 1.0).abs() < f32::EPSILON {
        return;
    }
    let factor = factor.clamp(0.0, 1.0);

    for px in buf.chunks_exact_mut(4) {
        let (b, g, r) = (f32::from(px[0]), f32::from(px[1]), f32::from(px[2]));
        let luma = r * W_R + g * W_G + b * W_B;
        for (channel, value) in px.iter_mut().take(3).zip([b, g, r]) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                *channel = (luma + (value - luma) * factor).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

pub fn draw_wallpaper_into(buf: &mut [u8], out_w: i32, out_h: i32, fit: WallpaperFit) {
    let source = WALLPAPER_SOURCE
        .read()
        .expect("single-threaded event loop, this lock is never poisoned")
        .clone();
    if let Some(source) = source {
        compose_wallpaper(buf, out_w, out_h, &source, fit);
    } else {
        draw_wallpaper(buf, out_w, out_h);
    }
}

#[must_use]
pub fn fit_transform(
    fit: WallpaperFit,
    src_w: f32,
    src_h: f32,
    out_w: f32,
    out_h: f32,
) -> (f32, f32, f32) {
    if src_w <= 0.0 || src_h <= 0.0 || out_w <= 0.0 || out_h <= 0.0 {
        return (1.0, 0.0, 0.0);
    }

    let scale = match fit {
        WallpaperFit::Cover => (out_w / src_w).max(out_h / src_h),
        WallpaperFit::Contain => (out_w / src_w).min(out_h / src_h),
        WallpaperFit::Center | WallpaperFit::Tile => 1.0,
    };

    let offset_x = (out_w - src_w * scale) / 2.0;
    let offset_y = (out_h - src_h * scale) / 2.0;
    (scale, offset_x, offset_y)
}

pub fn compose_wallpaper(
    buf: &mut [u8],
    out_w: i32,
    out_h: i32,
    source: &Pixmap,
    fit: WallpaperFit,
) {
    let (Ok(w), Ok(h)) = (u32::try_from(out_w), u32::try_from(out_h)) else {
        return;
    };
    let Some(mut pixmap) = Pixmap::new(w, h) else {
        return;
    };

    let output_rect = Rect::from_xywh(0.0, 0.0, out_w as f32, out_h as f32);

    let (fill_rect, transform, spread) = match fit {
        WallpaperFit::Tile => (output_rect, Transform::identity(), SpreadMode::Repeat),
        WallpaperFit::Cover => {
            let (scale, offset_x, offset_y) = fit_transform(
                fit,
                source.width() as f32,
                source.height() as f32,
                out_w as f32,
                out_h as f32,
            );
            (
                output_rect,
                Transform::from_translate(offset_x, offset_y).pre_scale(scale, scale),
                SpreadMode::Pad,
            )
        }
        WallpaperFit::Contain | WallpaperFit::Center => {
            let (scale, offset_x, offset_y) = fit_transform(
                fit,
                source.width() as f32,
                source.height() as f32,
                out_w as f32,
                out_h as f32,
            );
            let scaled_w = source.width() as f32 * scale;
            let scaled_h = source.height() as f32 * scale;
            (
                Rect::from_xywh(offset_x, offset_y, scaled_w, scaled_h),
                Transform::from_translate(offset_x, offset_y).pre_scale(scale, scale),
                SpreadMode::Pad,
            )
        }
    };
    let Some(fill_rect) = fill_rect else {
        return;
    };

    let paint = Paint {
        shader: Pattern::new(
            source.as_ref(),
            spread,
            FilterQuality::Bilinear,
            1.0,
            transform,
        ),
        ..Paint::default()
    };
    pixmap.fill_rect(fill_rect, &paint, Transform::identity(), None);

    let bytes = skia_pixmap_into_buffer_bytes(pixmap);
    let n = bytes.len().min(buf.len());
    buf[..n].copy_from_slice(&bytes[..n]);
}

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

#[allow(clippy::too_many_lines)]
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

#[cfg(test)]
mod tests {
    use super::*;

    mod desaturation {
        use super::*;

        /// Spread between the most and least intense colour channel — a
        /// direct proxy for saturation.
        fn spread(px: &[u8]) -> i32 {
            let channels = [i32::from(px[0]), i32::from(px[1]), i32::from(px[2])];
            channels.iter().max().unwrap() - channels.iter().min().unwrap()
        }

        #[test]
        fn a_factor_of_one_changes_nothing() {
            let original = vec![200u8, 40, 90, 255, 10, 220, 60, 255];
            let mut buf = original.clone();
            desaturate_in_place(&mut buf, 1.0);
            assert_eq!(buf, original);
        }

        #[test]
        fn a_partial_factor_pulls_channels_towards_each_other() {
            let mut buf = vec![200u8, 40, 90, 255];
            let before = spread(&buf);
            desaturate_in_place(&mut buf, 0.9);
            let after = spread(&buf);
            assert!(
                after < before,
                "saturation did not drop: {before} -> {after}"
            );
            // 10% is a subtle nudge, not a wash-out.
            assert!(
                after > before / 2,
                "0.9 should barely mute the colour, got {before} -> {after}"
            );
        }

        #[test]
        fn a_factor_of_zero_produces_pure_grey() {
            let mut buf = vec![200u8, 40, 90, 255];
            desaturate_in_place(&mut buf, 0.0);
            assert_eq!(spread(&buf), 0, "expected all channels equal: {buf:?}");
        }

        #[test]
        fn alpha_is_never_touched() {
            let mut buf = vec![200u8, 40, 90, 123, 1, 2, 3, 0];
            desaturate_in_place(&mut buf, 0.5);
            assert_eq!(buf[3], 123);
            assert_eq!(buf[7], 0);
        }

        #[test]
        fn grey_input_survives_untouched_at_any_factor() {
            for factor in [0.0, 0.5, 0.9] {
                let mut buf = vec![128u8, 128, 128, 255];
                desaturate_in_place(&mut buf, factor);
                assert_eq!(spread(&buf), 0);
                // Luma of a grey pixel is that grey, so it must not drift.
                assert!(
                    buf[..3].iter().all(|&c| c.abs_diff(128) <= 1),
                    "grey drifted at factor {factor}: {buf:?}"
                );
            }
        }

        #[test]
        fn a_ragged_buffer_tail_is_ignored_without_panicking() {
            let mut buf = vec![200u8, 40, 90, 255, 7, 7];
            desaturate_in_place(&mut buf, 0.5);
            assert_eq!(&buf[4..], &[7, 7], "the partial pixel must be left alone");
        }
    }

    #[test]
    fn embedded_wallpaper_decodes() {
        let pixmap = decode_wallpaper(embedded_wallpaper_bytes()).expect("must decode");
        assert!(pixmap.width() > 0);
        assert!(pixmap.height() > 0);
    }

    #[test]
    fn decode_wallpaper_rejects_garbage() {
        assert!(decode_wallpaper(b"not a png").is_none());
    }

    #[test]
    fn cover_never_leaves_the_output_uncovered() {
        for (src_w, src_h, out_w, out_h) in [
            (2560.0, 1503.0, 1920.0, 1080.0),
            (2560.0, 1503.0, 3440.0, 1440.0),
            (2560.0, 1503.0, 1080.0, 1920.0),
            (100.0, 100.0, 4000.0, 200.0),
        ] {
            let (scale, ox, oy) = fit_transform(WallpaperFit::Cover, src_w, src_h, out_w, out_h);
            assert!(
                src_w * scale >= out_w - 0.01,
                "scaled width {} must cover output width {out_w}",
                src_w * scale
            );
            assert!(
                src_h * scale >= out_h - 0.01,
                "scaled height {} must cover output height {out_h}",
                src_h * scale
            );
            assert!(
                ox <= 0.01,
                "cover crops horizontally, offset must be <= 0, got {ox}"
            );
            assert!(
                oy <= 0.01,
                "cover crops vertically, offset must be <= 0, got {oy}"
            );
        }
    }

    #[test]
    fn contain_never_crops_the_source() {
        for (src_w, src_h, out_w, out_h) in [
            (2560.0, 1503.0, 1920.0, 1080.0),
            (2560.0, 1503.0, 3440.0, 1440.0),
            (2560.0, 1503.0, 1080.0, 1920.0),
        ] {
            let (scale, ox, oy) = fit_transform(WallpaperFit::Contain, src_w, src_h, out_w, out_h);
            assert!(src_w * scale <= out_w + 0.01);
            assert!(src_h * scale <= out_h + 0.01);
            assert!(
                ox >= -0.01,
                "contain never crops, offset must be >= 0, got {ox}"
            );
            assert!(
                oy >= -0.01,
                "contain never crops, offset must be >= 0, got {oy}"
            );
        }
    }

    #[test]
    fn center_and_tile_use_native_scale() {
        for fit in [WallpaperFit::Center, WallpaperFit::Tile] {
            let (scale, _, _) = fit_transform(fit, 2560.0, 1503.0, 1920.0, 1080.0);
            assert!((scale - 1.0).abs() < f32::EPSILON, "{fit:?}");
        }
    }

    #[test]
    fn cover_centers_the_crop() {
        let (scale, ox, oy) = fit_transform(WallpaperFit::Cover, 2560.0, 1503.0, 1920.0, 1080.0);
        let scaled_w = 2560.0 * scale;
        let scaled_h = 1503.0 * scale;
        assert!((ox - (1920.0 - scaled_w) / 2.0).abs() < 0.01);
        assert!((oy - (1080.0 - scaled_h) / 2.0).abs() < 0.01);
    }

    #[test]
    fn degenerate_dimensions_never_panic_and_return_identity() {
        for (src_w, src_h, out_w, out_h) in [
            (0.0, 100.0, 100.0, 100.0),
            (100.0, 0.0, 100.0, 100.0),
            (100.0, 100.0, 0.0, 100.0),
            (100.0, 100.0, 100.0, 0.0),
            (-1.0, 100.0, 100.0, 100.0),
        ] {
            let (scale, ox, oy) = fit_transform(WallpaperFit::Cover, src_w, src_h, out_w, out_h);
            assert!((scale - 1.0).abs() < f32::EPSILON);
            assert!(ox.abs() < f32::EPSILON);
            assert!(oy.abs() < f32::EPSILON);
        }
    }

    #[test]
    fn compose_wallpaper_fills_the_whole_buffer_for_every_fit_mode() {
        let mut source = Pixmap::new(4, 4).unwrap();
        source.fill(tiny_skia::Color::from_rgba8(200, 100, 50, 255));

        for fit in [
            WallpaperFit::Cover,
            WallpaperFit::Contain,
            WallpaperFit::Center,
            WallpaperFit::Tile,
        ] {
            let (out_w, out_h) = (16, 12);
            let mut buf = vec![0u8; (out_w * out_h * 4) as usize];
            compose_wallpaper(&mut buf, out_w, out_h, &source, fit);
            assert_eq!(buf.len(), (out_w * out_h * 4) as usize, "{fit:?}");
        }
    }

    #[test]
    fn compose_wallpaper_cover_paints_every_pixel_opaque() {
        let mut source = Pixmap::new(4, 4).unwrap();
        source.fill(tiny_skia::Color::from_rgba8(200, 100, 50, 255));

        let (out_w, out_h) = (16, 12);
        let mut buf = vec![0u8; (out_w * out_h * 4) as usize];
        compose_wallpaper(&mut buf, out_w, out_h, &source, WallpaperFit::Cover);

        for px in buf.chunks_exact(4) {
            assert_eq!(px[3], 255, "cover must leave no transparent pixel");
        }
    }

    #[test]
    fn compose_wallpaper_center_can_leave_transparent_letterboxing() {
        let mut source = Pixmap::new(4, 4).unwrap();
        source.fill(tiny_skia::Color::from_rgba8(200, 100, 50, 255));

        let (out_w, out_h) = (16, 12);
        let mut buf = vec![0u8; (out_w * out_h * 4) as usize];
        compose_wallpaper(&mut buf, out_w, out_h, &source, WallpaperFit::Center);

        let top_left_alpha = buf[3];
        assert_eq!(
            top_left_alpha, 0,
            "a 4x4 source centered in a 16x12 output must leave the corners transparent"
        );
    }

    #[test]
    fn init_wallpaper_with_no_override_falls_back_to_the_embedded_image() {
        init_wallpaper(None);
        let mut buf = vec![0u8; 8 * 8 * 4];
        draw_wallpaper_into(&mut buf, 8, 8, WallpaperFit::Cover);
        for px in buf.chunks_exact(4) {
            assert_eq!(px[3], 255, "cover must leave no transparent pixel");
        }
    }

    #[test]
    fn init_wallpaper_with_a_garbage_override_still_falls_back_to_the_embedded_image() {
        init_wallpaper(Some(b"not a png"));
        let mut buf = vec![0u8; 8 * 8 * 4];
        draw_wallpaper_into(&mut buf, 8, 8, WallpaperFit::Cover);
        for px in buf.chunks_exact(4) {
            assert_eq!(
                px[3], 255,
                "must still fall back to a fully opaque wallpaper"
            );
        }
    }
}

use std::sync::{Arc, RwLock};

use tiny_skia::{
    BlendMode, FilterQuality, GradientStop, LinearGradient, Paint, PathBuilder, Pattern, Pixmap, Point, RadialGradient,
    Rect, Shader, SpreadMode, Stroke, Transform,
};

use glide_theme::WallpaperFit;

use crate::{Color, copy_pixmap_into, stroke_solid, to_skia_color};

static EMBEDDED_WALLPAPER_BYTES: &[u8] = include_bytes!("../assets/wallpaper.png");

#[must_use]
pub fn embedded_wallpaper_bytes() -> &'static [u8] {
    EMBEDDED_WALLPAPER_BYTES
}

#[must_use]
pub fn decode_wallpaper(bytes: &[u8]) -> Option<Pixmap> {
    if bytes.starts_with(&[0xff, 0xd8]) {
        return decode_jpeg(bytes);
    }
    if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
        return decode_webp(bytes);
    }
    Pixmap::decode_png(bytes).ok().or_else(|| decode_jpeg(bytes)).or_else(|| decode_webp(bytes))
}

pub(crate) fn decode_webp(bytes: &[u8]) -> Option<Pixmap> {
    let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    let channels = if decoder.has_alpha() { 4 } else { 3 };
    let mut pixels = vec![0u8; decoder.output_buffer_size()?];
    decoder.read_image(&mut pixels).ok()?;

    let mut pixmap = Pixmap::new(width, height)?;
    for (chunk, rgb) in pixmap.pixels_mut().iter_mut().zip(pixels.chunks_exact(channels)) {
        let alpha = if channels == 4 { rgb[3] } else { 255 };
        let premultiply = |value: u8| u8::try_from(u16::from(value) * u16::from(alpha) / 255).unwrap_or(alpha);
        *chunk = tiny_skia::PremultipliedColorU8::from_rgba(
            premultiply(rgb[0]),
            premultiply(rgb[1]),
            premultiply(rgb[2]),
            alpha,
        )?;
    }
    Some(pixmap)
}

pub(crate) fn decode_jpeg(bytes: &[u8]) -> Option<Pixmap> {
    let mut decoder = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(bytes));
    decoder.decode_headers().ok()?;
    let (width, height) = decoder.dimensions()?;
    let pixels = decoder.decode().ok()?;
    let width = u32::try_from(width).ok()?;
    let height = u32::try_from(height).ok()?;
    let area = usize::try_from(u64::from(width) * u64::from(height)).ok()?;
    if area == 0 {
        return None;
    }
    let channels = pixels.len() / area;
    if channels != 3 && channels != 1 {
        return None;
    }
    let mut pixmap = Pixmap::new(width, height)?;
    for (index, pixel) in pixmap.pixels_mut().iter_mut().enumerate() {
        let offset = index * channels;
        let rgb = pixels.get(offset..offset + channels)?;
        let (r, g, b) = if channels == 1 { (rgb[0], rgb[0], rgb[0]) } else { (rgb[0], rgb[1], rgb[2]) };
        *pixel = tiny_skia::PremultipliedColorU8::from_rgba(r, g, b, 255)?;
    }
    Some(pixmap)
}

static WALLPAPER_SOURCE: RwLock<Option<Arc<Pixmap>>> = RwLock::new(None);
static WALLPAPER_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn init_wallpaper(override_bytes: Option<&[u8]>) {
    let decoded = override_bytes.and_then(decode_wallpaper).or_else(|| decode_wallpaper(embedded_wallpaper_bytes()));
    let mut slot = WALLPAPER_SOURCE.write().expect(crate::NEVER_POISONED);
    *slot = decoded.map(Arc::new);
    WALLPAPER_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[must_use]
pub fn wallpaper_generation() -> u64 {
    WALLPAPER_GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn desaturate_in_place(buf: &mut [u8], factor: f32) {
    saturate_in_place(buf, factor.clamp(0.0, 1.0));
}

pub fn saturate_in_place(buf: &mut [u8], factor: f32) {
    const W_B: f32 = 0.114;
    const W_G: f32 = 0.587;
    const W_R: f32 = 0.299;

    if (factor - 1.0).abs() < f32::EPSILON {
        return;
    }
    let factor = factor.clamp(0.0, 4.0);

    for px in buf.chunks_exact_mut(4) {
        let (b, g, r) = (f32::from(px[0]), f32::from(px[1]), f32::from(px[2]));
        let luma = r * W_R + g * W_G + b * W_B;
        for (channel, value) in px.iter_mut().take(3).zip([b, g, r]) {
            *channel = (luma + (value - luma) * factor).round().clamp(0.0, 255.0) as u8;
        }
    }
}

pub fn draw_wallpaper_into(buf: &mut [u8], out_w: i32, out_h: i32, fit: WallpaperFit) {
    let source = WALLPAPER_SOURCE.read().expect(crate::NEVER_POISONED).clone();
    if let Some(source) = source {
        compose_wallpaper(buf, out_w, out_h, &source, fit);
    } else {
        draw_wallpaper(buf, out_w, out_h);
    }
}

#[must_use]
pub fn fit_transform(fit: WallpaperFit, src_w: f32, src_h: f32, out_w: f32, out_h: f32) -> (f32, f32, f32) {
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

pub fn compose_wallpaper(buf: &mut [u8], out_w: i32, out_h: i32, source: &Pixmap, fit: WallpaperFit) {
    let (Ok(w), Ok(h)) = (u32::try_from(out_w), u32::try_from(out_h)) else {
        return;
    };
    let Some(mut pixmap) = Pixmap::new(w, h) else {
        return;
    };

    let output_rect = Rect::from_xywh(0.0, 0.0, out_w as f32, out_h as f32);

    let (src_w, src_h) = (source.width() as f32, source.height() as f32);
    let (fill_rect, transform, spread) = if matches!(fit, WallpaperFit::Tile) {
        (output_rect, Transform::identity(), SpreadMode::Repeat)
    } else {
        let (scale, offset_x, offset_y) = fit_transform(fit, src_w, src_h, out_w as f32, out_h as f32);
        let fill_rect = if matches!(fit, WallpaperFit::Cover) {
            output_rect
        } else {
            Rect::from_xywh(offset_x, offset_y, src_w * scale, src_h * scale)
        };
        (fill_rect, Transform::from_translate(offset_x, offset_y).pre_scale(scale, scale), SpreadMode::Pad)
    };
    let Some(fill_rect) = fill_rect else {
        return;
    };

    let paint = Paint {
        shader: Pattern::new(source.as_ref(), spread, FilterQuality::Bilinear, 1.0, transform),
        ..Paint::default()
    };
    pixmap.fill_rect(fill_rect, &paint, Transform::identity(), None);
    copy_pixmap_into(buf, pixmap);
}

fn draw_radial(
    pixmap: &mut Pixmap, fw: f32, fh: f32, center_x: f32, center_y: f32, radius_x: f32, radius_y: f32, stop_frac: f32,
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

    let paint = Paint { shader, blend_mode: BlendMode::SourceOver, ..Paint::default() };
    if let Some(rect) = Rect::from_xywh(0.0, 0.0, fw, fh) {
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
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
        vec![GradientStop::new(0.0, to_skia_color(wall1)), GradientStop::new(1.0, to_skia_color(wall3))],
        SpreadMode::Pad,
        Transform::identity(),
    ) {
        let paint = Paint { shader, ..Paint::default() };
        if let Some(rect) = Rect::from_xywh(0.0, 0.0, fw, fh) {
            pixmap.fill_rect(rect, &paint, Transform::identity(), None);
        }
    }

    for (cx, cy, rx, ry, stop, colour) in [
        (0.50, 0.42, 0.88, 0.68, 0.70, wall3),
        (0.12, 0.88, 0.66, 0.52, 0.60, wall4),
        (0.78, 0.14, 0.56, 0.44, 0.62, wall2),
    ] {
        draw_radial(&mut pixmap, fw, fh, fw * cx, fh * cy, fw * rx, fh * ry, stop, colour);
    }

    let ring_stroke = Stroke { width: 1.5, ..Stroke::default() };
    for (cx, cy, r) in [
        (-130.0 + 220.0, fh - (-180.0 + 220.0), 220.0),
        (-230.0 + 320.0, fh - (-300.0 + 320.0), 320.0),
        (fw - (-70.0 + 130.0), -90.0 + 130.0, 130.0),
    ] {
        let ring = PathBuilder::from_circle(cx, cy, r);
        stroke_solid(&mut pixmap.as_mut(), ring, wall_line, &ring_stroke);
    }
    copy_pixmap_into(buf, pixmap);
}

#[cfg(test)]
mod tests {
    use super::*;

    mod decoding {
        use super::*;

        #[test]
        fn jpeg_wallpapers_decode_at_full_size() {
            let pixmap = decode_wallpaper(include_bytes!("../testdata/fixtures/red_8x4.jpg"))
                .expect("a JPEG wallpaper must decode");
            assert_eq!((pixmap.width(), pixmap.height()), (8, 4));
            let px = pixmap.pixels()[0];
            assert!(px.red() > 240 && px.green() < 20 && px.blue() < 20);
            assert_eq!(px.alpha(), 255);
        }

        #[test]
        fn greyscale_jpeg_expands_to_equal_channels() {
            let pixmap = decode_wallpaper(include_bytes!("../testdata/fixtures/gray_8x4.jpg"))
                .expect("a greyscale JPEG must decode");
            let px = pixmap.pixels()[5];
            assert_eq!(px.red(), px.green());
            assert_eq!(px.green(), px.blue());
        }

        #[test]
        fn the_embedded_png_still_decodes_and_garbage_does_not() {
            assert!(decode_wallpaper(embedded_wallpaper_bytes()).is_some());
            assert!(decode_wallpaper(b"not an image").is_none());
            assert!(decode_wallpaper(&[0xff, 0xd8, 0x00]).is_none());
        }

        #[test]
        fn webp_wallpapers_decode_to_pixmap() {
            let rgba: Vec<u8> = vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
            let mut encoded = Vec::new();
            image_webp::WebPEncoder::new(&mut encoded)
                .encode(&rgba, 2, 2, image_webp::ColorType::Rgba8)
                .expect("encodes webp");
            let pixmap = decode_wallpaper(&encoded).expect("a WebP wallpaper must decode");
            assert_eq!((pixmap.width(), pixmap.height()), (2, 2));
            assert_eq!(pixmap.pixels()[0].red(), 255);
        }
    }

    fn spread(px: &[u8]) -> i32 {
        let channels = [i32::from(px[0]), i32::from(px[1]), i32::from(px[2])];
        channels.iter().max().unwrap() - channels.iter().min().unwrap()
    }

    #[test]
    fn desaturation_mutes_colour_but_never_alpha_grey_or_a_ragged_tail() {
        let original = vec![200u8, 40, 90, 123, 10, 220, 60, 0, 7, 7];
        let mut buf = original.clone();
        desaturate_in_place(&mut buf, 1.0);
        assert_eq!(buf, original);
        desaturate_in_place(&mut buf, 1.6);
        assert_eq!(buf, original);

        desaturate_in_place(&mut buf, 0.9);
        let (before, after) = (spread(&original), spread(&buf));
        assert!(after < before && after > before / 2);
        assert_eq!((buf[3], buf[7], &buf[8..]), (123, 0, &[7u8, 7][..]));
        desaturate_in_place(&mut buf, 0.0);
        assert_eq!(spread(&buf), 0);

        for factor in [0.0, 0.5, 0.9] {
            let mut grey = vec![128u8, 128, 128, 255];
            desaturate_in_place(&mut grey, factor);
            assert!(grey[..3].iter().all(|&c| c.abs_diff(128) <= 1));
        }

        let mut vivid = vec![100u8, 120, 140, 255];
        saturate_in_place(&mut vivid, 1.6);
        assert!(vivid[2] > 140 && vivid[0] < 100 && vivid[3] == 255);
    }

    #[test]
    fn fit_transforms_cover_contain_and_centre_as_named() {
        for (src_w, src_h, out_w, out_h) in [
            (2560.0, 1503.0, 1920.0, 1080.0),
            (2560.0, 1503.0, 3440.0, 1440.0),
            (2560.0, 1503.0, 1080.0, 1920.0),
            (100.0, 100.0, 4000.0, 200.0),
        ] {
            let (scale, ox, oy) = fit_transform(WallpaperFit::Cover, src_w, src_h, out_w, out_h);
            assert!(src_w * scale >= out_w - 0.01 && src_h * scale >= out_h - 0.01);
            assert!(ox <= 0.01 && oy <= 0.01);
            assert!((ox - (out_w - src_w * scale) / 2.0).abs() < 0.01);
            assert!((oy - (out_h - src_h * scale) / 2.0).abs() < 0.01);

            let (scale, ox, oy) = fit_transform(WallpaperFit::Contain, src_w, src_h, out_w, out_h);
            assert!(src_w * scale <= out_w + 0.01 && src_h * scale <= out_h + 0.01);
            assert!(ox >= -0.01 && oy >= -0.01);
        }
        for fit in [WallpaperFit::Center, WallpaperFit::Tile] {
            assert!((fit_transform(fit, 2560.0, 1503.0, 1920.0, 1080.0).0 - 1.0).abs() < f32::EPSILON);
        }
        for (src_w, src_h, out_w, out_h) in [
            (0.0, 100.0, 100.0, 100.0),
            (100.0, 0.0, 100.0, 100.0),
            (100.0, 100.0, 0.0, 100.0),
            (100.0, 100.0, 100.0, 0.0),
            (-1.0, 100.0, 100.0, 100.0),
        ] {
            assert_eq!(fit_transform(WallpaperFit::Cover, src_w, src_h, out_w, out_h), (1.0, 0.0, 0.0));
        }
    }

    #[test]
    fn cover_paints_every_pixel_and_centre_letterboxes() {
        let mut source = Pixmap::new(4, 4).unwrap();
        source.fill(tiny_skia::Color::from_rgba8(200, 100, 50, 255));
        let compose = |fit| {
            let mut buf = vec![0u8; 16 * 12 * 4];
            compose_wallpaper(&mut buf, 16, 12, &source, fit);
            buf
        };
        assert!(compose(WallpaperFit::Cover).chunks_exact(4).all(|px| px[3] == 255));
        assert_eq!(compose(WallpaperFit::Center)[3], 0);
        compose(WallpaperFit::Contain);
        compose(WallpaperFit::Tile);
    }

    #[test]
    fn a_missing_or_garbage_override_falls_back_to_the_embedded_image() {
        for override_bytes in [None, Some(&b"not a png"[..])] {
            init_wallpaper(override_bytes);
            let mut buf = vec![0u8; 8 * 8 * 4];
            draw_wallpaper_into(&mut buf, 8, 8, WallpaperFit::Cover);
            assert!(buf.chunks_exact(4).all(|px| px[3] == 255));
        }
    }
}

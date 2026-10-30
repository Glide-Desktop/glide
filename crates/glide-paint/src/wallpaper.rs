use std::sync::{Arc, Mutex, RwLock};

use tiny_skia::{FilterQuality, Paint, Pattern, Pixmap, PixmapMut, Rect, SpreadMode, Transform};

use glide_theme::WallpaperFit;

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
    decode_png(bytes).or_else(|| decode_jpeg(bytes)).or_else(|| decode_webp(bytes))
}

const MAX_IMAGE_SIDE: u32 = 16_384;
const MAX_IMAGE_PIXELS: u64 = 64 * 1024 * 1024;

pub(crate) fn fits_in_memory(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= MAX_IMAGE_SIDE
        && height <= MAX_IMAGE_SIDE
        && u64::from(width) * u64::from(height) <= MAX_IMAGE_PIXELS
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']) || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
    Some((width, height))
}

pub(crate) fn decode_png(bytes: &[u8]) -> Option<Pixmap> {
    let (width, height) = png_dimensions(bytes)?;
    if !fits_in_memory(width, height) {
        return None;
    }
    Pixmap::decode_png(bytes).ok()
}

pub(crate) fn decode_webp(bytes: &[u8]) -> Option<Pixmap> {
    let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let (width, height) = decoder.dimensions();
    if !fits_in_memory(width, height) {
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
    let width = u32::try_from(width).ok()?;
    let height = u32::try_from(height).ok()?;
    if !fits_in_memory(width, height) {
        return None;
    }
    let orientation = decoder.exif().and_then(|exif| exif_orientation(exif)).unwrap_or(1);
    let pixels = decoder.decode().ok()?;
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
    drop(pixels);
    Some(orient(pixmap, orientation))
}

const EXIF_ORIENTATION_TAG: u16 = 0x0112;
const EXIF_MAX_ENTRIES: usize = 512;

pub(crate) fn exif_orientation(tiff: &[u8]) -> Option<u8> {
    let little = match tiff.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_at = |offset: usize| -> Option<u16> {
        let bytes: [u8; 2] = tiff.get(offset..offset.checked_add(2)?)?.try_into().ok()?;
        Some(if little { u16::from_le_bytes(bytes) } else { u16::from_be_bytes(bytes) })
    };
    let u32_at = |offset: usize| -> Option<u32> {
        let bytes: [u8; 4] = tiff.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
        Some(if little { u32::from_le_bytes(bytes) } else { u32::from_be_bytes(bytes) })
    };
    if u16_at(2)? != 42 {
        return None;
    }
    let ifd = usize::try_from(u32_at(4)?).ok()?;
    let entries = usize::from(u16_at(ifd)?).min(EXIF_MAX_ENTRIES);
    (0..entries).find_map(|index| {
        let entry = ifd.checked_add(2)?.checked_add(index.checked_mul(12)?)?;
        if u16_at(entry)? != EXIF_ORIENTATION_TAG {
            return None;
        }
        let kind = u16_at(entry + 2)?;
        let count = u32_at(entry + 4)?;
        if kind != 3 || count != 1 {
            return None;
        }
        u8::try_from(u16_at(entry + 8)?).ok().filter(|value| (1..=8).contains(value))
    })
}

pub(crate) fn orient(source: Pixmap, orientation: u8) -> Pixmap {
    if !(2..=8).contains(&orientation) {
        return source;
    }
    let (w, h) = (source.width(), source.height());
    let swapped = orientation >= 5;
    let (out_w, out_h) = if swapped { (h, w) } else { (w, h) };
    let Some(mut out) = Pixmap::new(out_w, out_h) else {
        return source;
    };
    let src = source.pixels();
    for (index, pixel) in out.pixels_mut().iter_mut().enumerate() {
        let (x, y) = (index as u32 % out_w, index as u32 / out_w);
        let (sx, sy) = match orientation {
            2 => (w - 1 - x, y),
            3 => (w - 1 - x, h - 1 - y),
            4 => (x, h - 1 - y),
            5 => (y, x),
            6 => (y, h - 1 - x),
            7 => (w - 1 - y, h - 1 - x),
            _ => (w - 1 - y, x),
        };
        *pixel = src[(sy * w + sx) as usize];
    }
    out
}

static WALLPAPER_SOURCE: RwLock<Option<Arc<Pixmap>>> = RwLock::new(None);
static WALLPAPER_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

static WALLPAPER_REQUESTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn init_wallpaper(override_bytes: Option<&[u8]>) {
    install(WALLPAPER_REQUESTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1, override_bytes);
}

fn install(request: u64, override_bytes: Option<&[u8]>) {
    let decoded = override_bytes.and_then(decode_wallpaper).or_else(|| decode_wallpaper(embedded_wallpaper_bytes()));
    let mut slot = WALLPAPER_SOURCE.write().expect(crate::NEVER_POISONED);
    if WALLPAPER_REQUESTS.load(std::sync::atomic::Ordering::Relaxed) == request {
        *slot = decoded.map(Arc::new);
        WALLPAPER_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

pub fn load_wallpaper(path: Option<std::path::PathBuf>, done: impl FnOnce(Option<std::io::Error>) + Send + 'static) {
    let request = WALLPAPER_REQUESTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let load = move || {
        let read = path.map(std::fs::read).transpose();
        let (bytes, error) = match read {
            Ok(bytes) => (bytes, None),
            Err(error) => (None, Some(error)),
        };
        install(request, bytes.as_deref());
        done(error);
    };
    let first = WALLPAPER_SOURCE.read().expect(crate::NEVER_POISONED).is_none();
    dispatch(first, load, |job| std::thread::Builder::new().name("glide-wallpaper".into()).spawn(job).map(|_| ()));
}

fn dispatch(
    inline: bool, load: impl FnOnce() + Send + 'static,
    spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<()>,
) {
    if inline {
        load();
        return;
    }
    let shared = Arc::new(Mutex::new(Some(load)));
    let worker = Arc::clone(&shared);
    let job = move || {
        if let Some(load) = worker.lock().ok().and_then(|mut slot| slot.take()) {
            load();
        }
    };
    if spawn(Box::new(job)).is_err()
        && let Some(load) = shared.lock().ok().and_then(|mut slot| slot.take())
    {
        load();
    }
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
    let band = (buf.len() / 4).div_ceil(crate::blur::bands(buf.len())).max(1) * 4;
    crate::blur::in_bands(buf, band, |_, pixels| {
        for px in pixels.chunks_exact_mut(4) {
            let (b, g, r) = (f32::from(px[0]), f32::from(px[1]), f32::from(px[2]));
            let ceiling = f32::from(px[3]);
            let luma = r * W_R + g * W_G + b * W_B;
            for (channel, value) in px.iter_mut().take(3).zip([b, g, r]) {
                *channel = (luma + (value - luma) * factor + 0.5).clamp(0.0, ceiling) as u8;
            }
        }
    });
}

pub fn draw_wallpaper_into(buf: &mut [u8], out_w: i32, out_h: i32, fit: WallpaperFit) {
    let source = WALLPAPER_SOURCE.read().expect(crate::NEVER_POISONED).clone();
    if let Some(source) = source.or_else(|| decode_wallpaper(embedded_wallpaper_bytes()).map(Arc::new)) {
        compose_wallpaper(buf, out_w, out_h, &source, fit);
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
    let rows = usize::try_from(out_h).unwrap_or(0);
    let bands = match fit {
        WallpaperFit::Cover | WallpaperFit::Tile => crate::blur::bands(usize::try_from(out_w).unwrap_or(0) * rows * 4),
        WallpaperFit::Contain | WallpaperFit::Center => 1,
    };
    compose_banded(buf, out_w, out_h, source, fit, rows.div_ceil(bands).max(1));
}

fn compose_banded(buf: &mut [u8], out_w: i32, out_h: i32, source: &Pixmap, fit: WallpaperFit, band_rows: usize) {
    let (Ok(w), Ok(h)) = (u32::try_from(out_w), u32::try_from(out_h)) else {
        return;
    };
    let row = w as usize * 4;
    let Some(buf) = buf.get_mut(..row * h as usize).filter(|buf| !buf.is_empty()) else {
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

    crate::blur::in_bands(buf, row * band_rows, |start, pixels| {
        let rows = u32::try_from(pixels.len() / row).unwrap_or(0);
        if let Some(mut pixmap) = PixmapMut::from_bytes(pixels, w, rows) {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            let paint = Paint {
                shader: Pattern::new(source.as_ref(), spread, FilterQuality::Bilinear, 1.0, transform),
                ..Paint::default()
            };
            let top = (start / row) as f32;
            pixmap.fill_rect(fill_rect, &paint, Transform::from_translate(0.0, -top), None);
        }
        pixels.chunks_exact_mut(4).for_each(|px| px.swap(0, 2));
    });
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
        let original = vec![200u8, 40, 90, 223, 0, 0, 0, 0, 7, 7];
        let mut buf = original.clone();
        desaturate_in_place(&mut buf, 1.0);
        assert_eq!(buf, original);
        desaturate_in_place(&mut buf, 1.6);
        assert_eq!(buf, original);

        desaturate_in_place(&mut buf, 0.9);
        let (before, after) = (spread(&original), spread(&buf));
        assert!(after < before && after > before / 2);
        assert_eq!((buf[3], buf[7], &buf[8..]), (223, 0, &[7u8, 7][..]));
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
    fn composing_in_bands_matches_one_pass_over_the_whole_output() {
        let mut source = Pixmap::new(37, 23).unwrap();
        for (index, px) in source.pixels_mut().iter_mut().enumerate() {
            let value = (index * 37 % 251) as u8;
            *px = tiny_skia::ColorU8::from_rgba(value, value.wrapping_mul(3), 255 - value, 255).premultiply();
        }
        for fit in [WallpaperFit::Cover, WallpaperFit::Tile] {
            let render = |band_rows| {
                let mut buf = vec![7u8; 61 * 43 * 4];
                compose_banded(&mut buf, 61, 43, &source, fit, band_rows);
                buf
            };
            let whole = render(43);
            for band_rows in [1, 5, 16] {
                assert!(render(band_rows) == whole, "{fit:?} in bands of {band_rows} rows");
            }
        }
    }

    fn tiff(little: bool, entries: &[(u16, u16, u32, u16)]) -> Vec<u8> {
        let u16b = |value: u16| if little { value.to_le_bytes() } else { value.to_be_bytes() };
        let u32b = |value: u32| if little { value.to_le_bytes() } else { value.to_be_bytes() };
        let mut out = if little { b"II".to_vec() } else { b"MM".to_vec() };
        out.extend(u16b(42));
        out.extend(u32b(8));
        out.extend(u16b(u16::try_from(entries.len()).unwrap()));
        for &(tag, kind, count, value) in entries {
            out.extend(u16b(tag));
            out.extend(u16b(kind));
            out.extend(u32b(count));
            out.extend(u16b(value));
            out.extend([0, 0]);
        }
        out.extend(u32b(0));
        out
    }

    #[test]
    fn exif_orientation_reads_both_byte_orders_and_refuses_malformed_blocks() {
        for little in [true, false] {
            for value in 1..=8u16 {
                let block = tiff(little, &[(0x010f, 2, 4, 0), (EXIF_ORIENTATION_TAG, 3, 1, value)]);
                assert_eq!(exif_orientation(&block), Some(value as u8), "little={little} value={value}");
            }
            assert_eq!(exif_orientation(&tiff(little, &[(0x010f, 2, 4, 0)])), None);
            assert_eq!(exif_orientation(&tiff(little, &[(EXIF_ORIENTATION_TAG, 3, 1, 0)])), None);
            assert_eq!(exif_orientation(&tiff(little, &[(EXIF_ORIENTATION_TAG, 3, 1, 9)])), None);
            assert_eq!(exif_orientation(&tiff(little, &[(EXIF_ORIENTATION_TAG, 4, 1, 6)])), None);
            assert_eq!(exif_orientation(&tiff(little, &[(EXIF_ORIENTATION_TAG, 3, 2, 6)])), None);
        }

        let valid = tiff(true, &[(EXIF_ORIENTATION_TAG, 3, 1, 6)]);
        let value_end = 8 + 2 + 8 + 2;
        for cut in 0..value_end {
            assert_eq!(exif_orientation(&valid[..cut]), None, "truncated to {cut} bytes");
        }
        assert_eq!(exif_orientation(&valid[..value_end]), Some(6));
        let mut wrong_magic = valid.clone();
        wrong_magic[2] = 43;
        assert_eq!(exif_orientation(&wrong_magic), None);
        let mut far_ifd = valid;
        far_ifd[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(exif_orientation(&far_ifd), None);
        let mut endless = tiff(true, &[(0x010f, 2, 4, 0)]);
        endless[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
        assert_eq!(exif_orientation(&endless), None);
        assert_eq!(exif_orientation(b""), None);
        assert_eq!(exif_orientation(b"XX*\0"), None);
    }

    fn labelled(width: u32, height: u32) -> Pixmap {
        let mut pixmap = Pixmap::new(width, height).unwrap();
        for (index, px) in pixmap.pixels_mut().iter_mut().enumerate() {
            *px = tiny_skia::PremultipliedColorU8::from_rgba(index as u8 * 10 + 10, 0, 0, 255).unwrap();
        }
        pixmap
    }

    fn rows(pixmap: &Pixmap) -> Vec<Vec<u8>> {
        pixmap
            .pixels()
            .chunks(pixmap.width() as usize)
            .map(|row| row.iter().map(|px| px.red() / 10).collect())
            .collect()
    }

    #[test]
    fn all_eight_orientations_turn_a_3_by_2_image_as_the_exif_table_says() {
        let upright = [vec![1, 2, 3], vec![4, 5, 6]];
        assert_eq!(rows(&labelled(3, 2)), upright);
        let expected: [(u8, Vec<Vec<u8>>); 8] = [
            (1, vec![vec![1, 2, 3], vec![4, 5, 6]]),
            (2, vec![vec![3, 2, 1], vec![6, 5, 4]]),
            (3, vec![vec![6, 5, 4], vec![3, 2, 1]]),
            (4, vec![vec![4, 5, 6], vec![1, 2, 3]]),
            (5, vec![vec![1, 4], vec![2, 5], vec![3, 6]]),
            (6, vec![vec![4, 1], vec![5, 2], vec![6, 3]]),
            (7, vec![vec![6, 3], vec![5, 2], vec![4, 1]]),
            (8, vec![vec![3, 6], vec![2, 5], vec![1, 4]]),
        ];
        for (orientation, want) in expected {
            let turned = orient(labelled(3, 2), orientation);
            assert_eq!(rows(&turned), want, "orientation {orientation}");
        }
        for unknown in [0, 9, 255] {
            assert_eq!(rows(&orient(labelled(3, 2), unknown)), upright, "orientation {unknown}");
        }
    }

    fn with_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        let block = tiff(false, &[(EXIF_ORIENTATION_TAG, 3, 1, orientation)]);
        let mut segment = b"Exif\0\0".to_vec();
        segment.extend(block);
        let length = u16::try_from(segment.len() + 2).unwrap();
        let mut out = jpeg[..2].to_vec();
        out.extend([0xff, 0xe1]);
        out.extend(length.to_be_bytes());
        out.extend(segment);
        out.extend(&jpeg[2..]);
        out
    }

    #[test]
    fn a_jpeg_tagged_as_rotated_decodes_upright() {
        let plain = include_bytes!("../testdata/fixtures/red_8x4.jpg");
        let dimensions = |bytes: &[u8]| {
            let pixmap = decode_wallpaper(bytes).expect("the tagged JPEG must still decode");
            (pixmap.width(), pixmap.height())
        };
        assert_eq!(dimensions(plain), (8, 4));
        for orientation in [1, 2, 3, 4] {
            assert_eq!(dimensions(&with_exif_orientation(plain, orientation)), (8, 4), "orientation {orientation}");
        }
        for orientation in [5, 6, 7, 8] {
            assert_eq!(dimensions(&with_exif_orientation(plain, orientation)), (4, 8), "orientation {orientation}");
        }
        let turned = decode_wallpaper(&with_exif_orientation(plain, 6)).unwrap();
        let px = turned.pixels()[0];
        assert!(px.red() > 240 && px.green() < 20 && px.blue() < 20);
    }

    #[test]
    fn saturating_a_premultiplied_pixel_never_pushes_a_channel_above_its_alpha() {
        let mut buf = vec![10u8, 10, 100, 100];
        saturate_in_place(&mut buf, 1.6);
        assert!(buf[..3].iter().all(|&channel| channel <= buf[3]), "channels {:?} exceed alpha {}", &buf[..3], buf[3]);
        assert_eq!(buf[3], 100);
        assert_eq!(buf[2], 100);

        let mut opaque = vec![100u8, 120, 140, 255];
        saturate_in_place(&mut opaque, 1.6);
        assert!(opaque[2] > 140 && opaque[0] < 100);

        let mut clear = vec![0u8; 4];
        saturate_in_place(&mut clear, 4.0);
        assert_eq!(clear, vec![0u8; 4]);
    }

    #[test]
    fn only_the_first_wallpaper_loads_inline_and_a_failed_spawn_still_loads() {
        let ran = |inline: bool, spawn_works: bool| {
            let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let spawned = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let (counted, tried) = (Arc::clone(&count), Arc::clone(&spawned));
            dispatch(
                inline,
                move || {
                    counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                },
                |job| {
                    tried.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if spawn_works {
                        job();
                        Ok(())
                    } else {
                        Err(std::io::Error::other("no threads left"))
                    }
                },
            );
            (count.load(std::sync::atomic::Ordering::SeqCst), spawned.load(std::sync::atomic::Ordering::SeqCst))
        };
        assert_eq!(ran(true, true), (1, 0), "the first wallpaper must be in place before the first frame");
        assert_eq!(ran(false, true), (1, 1), "later wallpapers load on the worker, once");
        assert_eq!(ran(false, false), (1, 1), "a failed spawn must run the load on the caller, or `done` never fires");
    }

    #[test]
    fn a_wallpaper_that_cannot_be_read_reports_why_and_keeps_an_image() {
        let (sender, receiver) = std::sync::mpsc::channel();
        load_wallpaper(Some("/nonexistent/glide-wallpaper.png".into()), move |error| {
            let _ = sender.send(error.map(|error| error.kind()));
        });
        let reported = receiver.recv_timeout(std::time::Duration::from_secs(10)).expect("the load finishes");
        assert_eq!(reported, Some(std::io::ErrorKind::NotFound));
        let mut buf = vec![0u8; 4 * 4 * 4];
        draw_wallpaper_into(&mut buf, 4, 4, WallpaperFit::Cover);
        assert!(buf.chunks_exact(4).all(|px| px[3] == 255));
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

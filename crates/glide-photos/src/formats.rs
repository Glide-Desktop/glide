use std::io::{BufReader, Cursor, Read};
use std::path::Path;

use image::{AnimationDecoder, DynamicImage, ImageFormat, ImageReader, Rgba32FImage, RgbaImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    Raster,
    Raw,
    Vector,
    Hdr,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    pub family: Family,
    pub name: &'static str,
}

const RASTER: &[(&str, &str)] = &[
    ("jpg", "JPEG"),
    ("jpeg", "JPEG"),
    ("jpe", "JPEG"),
    ("jfif", "JPEG"),
    ("png", "PNG"),
    ("apng", "PNG"),
    ("gif", "GIF"),
    ("webp", "WebP"),
    ("bmp", "BMP"),
    ("ico", "ICO"),
    ("tif", "TIFF"),
    ("tiff", "TIFF"),
    ("tga", "TGA"),
    ("pbm", "PNM"),
    ("pgm", "PNM"),
    ("ppm", "PNM"),
    ("pnm", "PNM"),
    ("pam", "PNM"),
    ("qoi", "QOI"),
    ("dds", "DDS"),
];

const RAW: &[&str] = &[
    "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "raf", "orf", "rw2", "pef", "dng", "3fr", "iiq", "x3f",
    "srw", "erf", "kdc", "mrw", "mos", "rwl", "dcr",
];

const HDR: &[(&str, &str)] = &[("hdr", "HDR"), ("exr", "OpenEXR")];

const VECTOR: &[(&str, &str)] = &[("svg", "SVG"), ("svgz", "SVGZ")];

const UNSUPPORTED: &[(&str, &str)] = &[
    ("heic", "HEIC"),
    ("heif", "HEIF"),
    ("hif", "HEIF"),
    ("avif", "AVIF"),
    ("jxl", "JPEG XL"),
    ("jp2", "JPEG 2000"),
    ("j2k", "JPEG 2000"),
    ("jxr", "JPEG XR"),
    ("psd", "PSD"),
    ("psb", "PSB"),
    ("xcf", "XCF"),
    ("kra", "KRA"),
    ("ora", "ORA"),
    ("pdf", "PDF"),
    ("eps", "EPS"),
    ("ai", "AI"),
    ("wmf", "WMF"),
    ("emf", "EMF"),
    ("icns", "ICNS"),
    ("mng", "MNG"),
    ("cbz", "CBZ"),
    ("mpo", "MPO"),
];

#[must_use]
pub fn of(path: &Path) -> Option<Format> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let find = |list: &[(&str, &'static str)]| list.iter().find(|(e, _)| *e == ext).map(|(_, name)| *name);
    if let Some(name) = find(RASTER) {
        return Some(Format { family: Family::Raster, name });
    }
    if RAW.contains(&ext.as_str()) {
        return Some(Format { family: Family::Raw, name: "RAW" });
    }
    if let Some(name) = find(HDR) {
        return Some(Format { family: Family::Hdr, name });
    }
    if let Some(name) = find(VECTOR) {
        return Some(Format { family: Family::Vector, name });
    }
    find(UNSUPPORTED).map(|name| Format { family: Family::Unsupported, name })
}

#[must_use]
pub fn sniffed(bytes: &[u8]) -> Option<&'static str> {
    let format = image::guess_format(bytes).ok()?;
    Some(match format {
        ImageFormat::Jpeg => "JPEG",
        ImageFormat::Png => "PNG",
        ImageFormat::Gif => "GIF",
        ImageFormat::WebP => "WebP",
        ImageFormat::Bmp => "BMP",
        ImageFormat::Ico => "ICO",
        ImageFormat::Tiff => "TIFF",
        ImageFormat::Qoi => "QOI",
        ImageFormat::Avif => "AVIF",
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Traits {
    pub frames: u32,
    pub pages: u32,
}

#[must_use]
pub fn traits(path: &Path, format: Format) -> Traits {
    let mut traits = Traits { frames: 1, pages: 1 };
    if format.family != Family::Raster {
        return traits;
    }
    let Ok(file) = std::fs::File::open(path) else {
        return traits;
    };
    let reader = BufReader::new(file);
    match format.name {
        "GIF" => {
            if let Ok(decoder) = image::codecs::gif::GifDecoder::new(reader) {
                traits.frames = decoder.into_frames().take(2).flatten().count().max(1) as u32;
            }
        }
        "WebP" => {
            if let Ok(decoder) = image::codecs::webp::WebPDecoder::new(reader)
                && decoder.has_animation()
            {
                traits.frames = 2;
            }
        }
        "PNG" => {
            if let Ok(decoder) = image::codecs::png::PngDecoder::new(reader)
                && decoder.is_apng().unwrap_or(false)
            {
                traits.frames = 2;
            }
        }
        "TIFF" => traits.pages = tiff_pages(path),
        _ => {}
    }
    traits
}

fn tiff_pages(path: &Path) -> u32 {
    let Ok(file) = std::fs::File::open(path) else {
        return 1;
    };
    let Ok(mut decoder) = tiff::decoder::Decoder::new(BufReader::new(file)) else {
        return 1;
    };
    let mut pages = 1;
    while decoder.more_images() && pages < 999 {
        if decoder.next_image().is_err() {
            break;
        }
        pages += 1;
    }
    pages
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Note {
    EmbeddedPreview,
    ToneMapped,
    Converted,
    Partial,
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub image: RgbaImage,
    pub delay_ms: u32,
}

#[derive(Debug, Clone)]
pub struct Decoded {
    pub image: RgbaImage,
    pub frames: Vec<Frame>,
    pub pages: u32,
    pub hdr: Option<Rgba32FImage>,
    pub note: Option<Note>,
    pub size: (u32, u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Unsupported(&'static str),
    Unreadable(String),
}

pub struct Request<'a> {
    pub path: &'a Path,
    pub format: Format,
    pub max_side: Option<u32>,
    pub page: u32,
    pub animate: bool,
}

const EXTERNAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

fn run_converter(program: &str, args: &[&std::ffi::OsStr]) -> bool {
    let Ok(mut child) = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if started.elapsed() < EXTERNAL_TIMEOUT => {
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

#[must_use]
pub fn converters(name: &str) -> &'static [&'static str] {
    match name {
        "HEIC" | "HEIF" => &["heif-convert", "magick"],
        "JPEG XL" => &["djxl", "magick"],
        "CBZ" => &[],
        _ => &["magick"],
    }
}

fn external(path: &Path, name: &str, page: u32) -> Option<DynamicImage> {
    let dir = std::env::temp_dir().join(format!("glide-photos-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let output = dir.join(format!("{stamp}.png"));
    let mut source = path.as_os_str().to_os_string();
    let decoded = converters(name).iter().any(|&program| {
        let converted = match program {
            "magick" => {
                source.push(format!("[{page}]"));
                let density = std::ffi::OsStr::new("-density");
                run_converter(program, &[density, std::ffi::OsStr::new("150"), &source, output.as_os_str()])
            }
            _ => run_converter(program, &[path.as_os_str(), output.as_os_str()]),
        };
        converted && output.is_file()
    });
    let image = decoded.then(|| image::open(&output).ok()).flatten();
    let _ = std::fs::remove_file(&output);
    image
}

#[must_use]
pub fn external_available(name: &str) -> bool {
    converters(name).iter().any(|program| glide_config::on_path(program))
}

pub fn decode(request: &Request<'_>) -> Result<Decoded, Failure> {
    match request.format.family {
        Family::Unsupported => match external(request.path, request.format.name, request.page) {
            Some(image) => Ok(finish(image, request, Some(Note::Converted), 1)),
            None => Err(Failure::Unsupported(request.format.name)),
        },
        Family::Vector => vector(request),
        Family::Raw => raw(request),
        Family::Hdr => hdr(request),
        Family::Raster => raster(request),
    }
}

fn unreadable(error: impl std::fmt::Display) -> Failure {
    Failure::Unreadable(error.to_string())
}

fn fit(image: DynamicImage, max_side: Option<u32>) -> DynamicImage {
    match max_side {
        Some(side) if image.width() > side || image.height() > side => image.thumbnail(side, side),
        _ => image,
    }
}

fn finish(image: DynamicImage, request: &Request<'_>, note: Option<Note>, pages: u32) -> Decoded {
    let size = (image.width(), image.height());
    let image = oriented(fit(image, request.max_side), orientation(request.path)).into_rgba8();
    Decoded { image, frames: Vec::new(), pages, hdr: None, note, size }
}

fn raster(request: &Request<'_>) -> Result<Decoded, Failure> {
    if request.animate
        && matches!(request.format.name, "GIF" | "WebP" | "PNG")
        && let Some(decoded) = animation(request)
    {
        return Ok(decoded);
    }
    if request.format.name == "TIFF" && request.page > 0 {
        return tiff_page(request);
    }
    let reader = ImageReader::open(request.path).map_err(unreadable)?.with_guessed_format().map_err(unreadable)?;
    match reader.decode() {
        Ok(image) => {
            let pages = if request.format.name == "TIFF" { tiff_pages(request.path) } else { 1 };
            Ok(finish(image, request, None, pages))
        }
        Err(error) => Err(unreadable(error)),
    }
}

fn animation(request: &Request<'_>) -> Option<Decoded> {
    let reader = BufReader::new(std::fs::File::open(request.path).ok()?);
    let frames = match request.format.name {
        "GIF" => image::codecs::gif::GifDecoder::new(reader).ok()?.into_frames().collect_frames().ok()?,
        "WebP" => {
            let decoder = image::codecs::webp::WebPDecoder::new(reader).ok()?;
            if !decoder.has_animation() {
                return None;
            }
            decoder.into_frames().collect_frames().ok()?
        }
        _ => {
            let decoder = image::codecs::png::PngDecoder::new(reader).ok()?;
            if !decoder.is_apng().ok()? {
                return None;
            }
            decoder.apng().ok()?.into_frames().collect_frames().ok()?
        }
    };
    if frames.len() < 2 {
        return None;
    }
    let frames: Vec<Frame> = frames
        .into_iter()
        .map(|frame| {
            let (numer, denom) = frame.delay().numer_denom_ms();
            let delay_ms = numer.checked_div(denom).map_or(100, |delay| delay.max(20));
            let image = fit(DynamicImage::ImageRgba8(frame.into_buffer()), request.max_side).into_rgba8();
            Frame { image, delay_ms }
        })
        .collect();
    let first = frames.first()?.image.clone();
    let size = (first.width(), first.height());
    Some(Decoded { image: first, frames, pages: 1, hdr: None, note: None, size })
}

fn tiff_page(request: &Request<'_>) -> Result<Decoded, Failure> {
    let file = std::fs::File::open(request.path).map_err(unreadable)?;
    let mut decoder = tiff::decoder::Decoder::new(BufReader::new(file)).map_err(unreadable)?;
    for _ in 0..request.page {
        decoder.next_image().map_err(unreadable)?;
    }
    let (width, height) = decoder.dimensions().map_err(unreadable)?;
    let color = decoder.colortype().map_err(unreadable)?;
    let data = decoder.read_image().map_err(unreadable)?;
    let tiff::decoder::DecodingResult::U8(bytes) = data else {
        return Err(Failure::Unreadable("16-bit page".into()));
    };
    let image = match color {
        tiff::ColorType::RGBA(8) => RgbaImage::from_raw(width, height, bytes).map(DynamicImage::ImageRgba8),
        tiff::ColorType::RGB(8) => image::RgbImage::from_raw(width, height, bytes).map(DynamicImage::ImageRgb8),
        tiff::ColorType::Gray(8) => image::GrayImage::from_raw(width, height, bytes).map(DynamicImage::ImageLuma8),
        _ => None,
    }
    .ok_or_else(|| Failure::Unreadable("page colour type".into()))?;
    Ok(finish(image, request, None, tiff_pages(request.path)))
}

fn hdr(request: &Request<'_>) -> Result<Decoded, Failure> {
    let reader = ImageReader::open(request.path).map_err(unreadable)?.with_guessed_format().map_err(unreadable)?;
    let image = fit(reader.decode().map_err(unreadable)?, request.max_side.or(Some(2048)));
    let size = (image.width(), image.height());
    let float = image.into_rgba32f();
    let mapped = tone_map(&float, 0.0);
    Ok(Decoded { image: mapped, frames: Vec::new(), pages: 1, hdr: Some(float), note: Some(Note::ToneMapped), size })
}

#[must_use]
pub fn tone_map(image: &Rgba32FImage, exposure: f32) -> RgbaImage {
    let gain = 2f32.powf(exposure);
    let channel = |value: f32| {
        let v = (value * gain).max(0.0);
        let mapped = v / (1.0 + v);
        (mapped.powf(1.0 / 2.2) * 255.0).round().clamp(0.0, 255.0) as u8
    };
    RgbaImage::from_fn(image.width(), image.height(), |x, y| {
        let p = image.get_pixel(x, y).0;
        image::Rgba([channel(p[0]), channel(p[1]), channel(p[2]), (p[3].clamp(0.0, 1.0) * 255.0) as u8])
    })
}

fn vector(request: &Request<'_>) -> Result<Decoded, Failure> {
    let bytes = std::fs::read(request.path).map_err(unreadable)?;
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(&bytes, &options).map_err(unreadable)?;
    let size = tree.size();
    let target = request.max_side.unwrap_or(2048) as f32;
    let scale = (target / size.width().max(size.height())).max(0.01);
    let width = (size.width() * scale).ceil().max(1.0) as u32;
    let height = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).ok_or_else(|| unreadable("empty canvas"))?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let mut data = pixmap.take();
    unpremultiply(&mut data);
    let image = RgbaImage::from_raw(width, height, data).ok_or_else(|| unreadable("canvas size"))?;
    Ok(Decoded {
        image,
        frames: Vec::new(),
        pages: 1,
        hdr: None,
        note: None,
        size: (size.width().ceil() as u32, size.height().ceil() as u32),
    })
}

fn unpremultiply(data: &mut [u8]) {
    for pixel in data.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        if alpha > 0 && alpha < 255 {
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
}

fn raw(request: &Request<'_>) -> Result<Decoded, Failure> {
    let bytes = std::fs::read(request.path).map_err(unreadable)?;
    if let Some(image) = embedded_preview(&bytes) {
        return Ok(finish(image, request, Some(Note::EmbeddedPreview), 1));
    }
    external(request.path, "RAW", 0)
        .map(|image| finish(image, request, Some(Note::Converted), 1))
        .ok_or(Failure::Unsupported("RAW"))
}

#[must_use]
pub fn embedded_preview(bytes: &[u8]) -> Option<DynamicImage> {
    let starts: Vec<usize> = bytes
        .windows(3)
        .enumerate()
        .filter(|(_, window)| window[0] == 0xFF && window[1] == 0xD8 && window[2] == 0xFF)
        .map(|(at, _)| at)
        .collect();
    let mut spans: Vec<(usize, usize)> = starts
        .iter()
        .enumerate()
        .map(|(index, &start)| (start, starts.get(index + 1).copied().unwrap_or(bytes.len()) - start))
        .collect();
    spans.sort_by_key(|&(_, length)| std::cmp::Reverse(length));
    spans.into_iter().take(4).find_map(|(start, _)| {
        image::load_from_memory_with_format(&bytes[start..], ImageFormat::Jpeg)
            .ok()
            .filter(|image| image.width() >= 160 && image.height() >= 120)
    })
}

#[must_use]
pub fn orientation(path: &Path) -> u32 {
    exif(path)
        .and_then(|exif| exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?.value.get_uint(0))
        .unwrap_or(1)
}

#[must_use]
pub fn oriented(image: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        2 => image.fliph(),
        3 => image.rotate180(),
        4 => image.flipv(),
        5 => image.rotate90().fliph(),
        6 => image.rotate90(),
        7 => image.rotate270().fliph(),
        8 => image.rotate270(),
        _ => image,
    }
}

#[must_use]
pub fn exif(path: &Path) -> Option<exif::Exif> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    exif::Reader::new().read_from_container(&mut reader).ok()
}

#[must_use]
pub fn exif_from_bytes(bytes: &[u8]) -> Option<exif::Exif> {
    exif::Reader::new().read_from_container(&mut Cursor::new(bytes)).ok()
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Camera {
    pub model: Option<String>,
    pub aperture: Option<String>,
    pub exposure: Option<String>,
    pub iso: Option<String>,
    pub focal: Option<String>,
    pub taken: Option<(i32, u32, u32, u32, u32)>,
    pub gps: Option<(f64, f64)>,
}

#[must_use]
pub fn camera(exif: &exif::Exif) -> Camera {
    let field = |tag| exif.get_field(tag, exif::In::PRIMARY);
    let rational = |tag| match field(tag).map(|field| &field.value) {
        Some(exif::Value::Rational(values)) => values.first().map(|value| (value.num, value.denom)),
        _ => None,
    };
    let text = |tag| field(tag).map(|field| field.display_value().to_string().trim_matches('"').trim().to_string());
    let model = text(exif::Tag::Model).filter(|value| !value.is_empty());
    let aperture = rational(exif::Tag::FNumber).filter(|(_, d)| *d != 0).map(|(n, d)| {
        let value = f64::from(n) / f64::from(d);
        format!("ƒ/{}", trim(value))
    });
    let exposure = rational(exif::Tag::ExposureTime).filter(|(_, d)| *d != 0).map(|(n, d)| {
        if n == 0 {
            "0".into()
        } else if n >= d {
            format!("{} с", trim(f64::from(n) / f64::from(d)))
        } else {
            format!("1/{}", (f64::from(d) / f64::from(n)).round())
        }
    });
    let iso =
        field(exif::Tag::PhotographicSensitivity).and_then(|field| field.value.get_uint(0)).map(|v| format!("ISO {v}"));
    let focal = rational(exif::Tag::FocalLengthIn35mmFilm)
        .or_else(|| rational(exif::Tag::FocalLength))
        .filter(|(_, d)| *d != 0)
        .map(|(n, d)| format!("{} мм", (f64::from(n) / f64::from(d)).round()))
        .or_else(|| {
            field(exif::Tag::FocalLengthIn35mmFilm).and_then(|field| field.value.get_uint(0)).map(|v| format!("{v} мм"))
        });
    let taken =
        text(exif::Tag::DateTimeOriginal).or_else(|| text(exif::Tag::DateTime)).and_then(|value| parse_stamp(&value));
    let gps = gps(exif);
    Camera { model, aperture, exposure, iso, focal, taken, gps }
}

fn trim(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if (rounded - rounded.trunc()).abs() < f64::EPSILON {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded:.1}")
    }
}

#[must_use]
pub fn parse_stamp(value: &str) -> Option<(i32, u32, u32, u32, u32)> {
    let digits: Vec<u32> = value
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .take(5)
        .map(|part| part.parse().ok())
        .collect::<Option<_>>()?;
    let [year, month, day, hour, minute] = digits[..] else {
        return None;
    };
    ((1..=12).contains(&month) && (1..=31).contains(&day) && year > 1800 && hour < 24 && minute < 60).then_some((
        year as i32,
        month,
        day,
        hour,
        minute,
    ))
}

fn gps(exif: &exif::Exif) -> Option<(f64, f64)> {
    let degrees = |tag| match &exif.get_field(tag, exif::In::PRIMARY)?.value {
        exif::Value::Rational(values) if values.len() >= 3 => {
            let part = |index: usize| values[index].to_f64();
            Some(part(0) + part(1) / 60.0 + part(2) / 3600.0)
        }
        _ => None,
    };
    let reference = |tag| exif.get_field(tag, exif::In::PRIMARY).map(|field| field.display_value().to_string());
    let mut lat = degrees(exif::Tag::GPSLatitude)?;
    let mut lon = degrees(exif::Tag::GPSLongitude)?;
    if reference(exif::Tag::GPSLatitudeRef).is_some_and(|value| value.contains('S')) {
        lat = -lat;
    }
    if reference(exif::Tag::GPSLongitudeRef).is_some_and(|value| value.contains('W')) {
        lon = -lon;
    }
    Some((lat, lon))
}

#[must_use]
pub fn read_head(path: &Path, limit: usize) -> Vec<u8> {
    let mut head = Vec::new();
    if let Ok(file) = std::fs::File::open(path) {
        let _ = file.take(limit as u64).read_to_end(&mut head);
    }
    head
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_map_to_families_case_insensitively() {
        assert_eq!(of(Path::new("a.JPG")).map(|f| f.family), Some(Family::Raster));
        assert_eq!(of(Path::new("a.nef")).map(|f| f.family), Some(Family::Raw));
        assert_eq!(of(Path::new("a.svg")).map(|f| f.family), Some(Family::Vector));
        assert_eq!(of(Path::new("a.exr")).map(|f| f.family), Some(Family::Hdr));
        assert_eq!(of(Path::new("a.heic")).map(|f| f.family), Some(Family::Unsupported));
        assert_eq!(of(Path::new("a.txt")), None);
    }

    #[test]
    fn exif_stamps_parse_with_any_separators_and_reject_nonsense() {
        assert_eq!(parse_stamp("2026:11:02 14:32:07"), Some((2026, 11, 2, 14, 32)));
        assert_eq!(parse_stamp("2026-11-02T14:32"), Some((2026, 11, 2, 14, 32)));
        assert_eq!(parse_stamp("0000:00:00 00:00:00"), None);
    }

    #[test]
    fn the_largest_embedded_jpeg_is_the_raw_preview() {
        let small = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(32, 24, image::Rgb([200, 10, 10])));
        let large = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(320, 240, image::Rgb([10, 200, 10])));
        let jpeg = |image: &DynamicImage| {
            let mut out = Cursor::new(Vec::new());
            image.write_to(&mut out, ImageFormat::Jpeg).unwrap();
            out.into_inner()
        };
        let mut file = b"II*\0fake raw header".to_vec();
        file.extend(jpeg(&small));
        file.extend([0u8; 64]);
        file.extend(jpeg(&large));
        file.extend([0u8; 16]);
        let preview = embedded_preview(&file).unwrap();
        assert_eq!((preview.width(), preview.height()), (320, 240));
        assert!(embedded_preview(b"no jpeg here").is_none());
    }

    #[test]
    fn tone_mapping_is_monotonic_and_exposure_brightens() {
        let mut image = Rgba32FImage::new(2, 1);
        image.put_pixel(0, 0, image::Rgba([0.2, 0.2, 0.2, 1.0]));
        image.put_pixel(1, 0, image::Rgba([4.0, 4.0, 4.0, 1.0]));
        let normal = tone_map(&image, 0.0);
        let bright = tone_map(&image, 1.0);
        assert!(normal.get_pixel(0, 0).0[0] < normal.get_pixel(1, 0).0[0]);
        assert!(bright.get_pixel(0, 0).0[0] > normal.get_pixel(0, 0).0[0]);
    }
}

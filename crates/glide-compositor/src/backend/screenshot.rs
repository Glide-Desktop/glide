use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use smithay::{
    backend::allocator::Fourcc,
    backend::renderer::{
        Bind, ExportMem, Offscreen,
        damage::OutputDamageTracker,
        gles::{GlesRenderer, GlesTexture},
    },
    utils::{Buffer, Rectangle, Transform},
};

use super::tty::GlideRenderElements;

pub struct Saved {
    pub path: Option<PathBuf>,
    pub png: Vec<u8>,
}

pub fn save(mut pixels: Vec<u8>, width: i32, height: i32) -> anyhow::Result<Saved> {
    bgra_to_rgba_in_place(&mut pixels);
    let (width, height) = (u32::try_from(width)?, u32::try_from(height)?);
    save_rgba(&pixels, width, height, true)
}

pub fn save_rgba(rgba: &[u8], width: u32, height: u32, write: bool) -> anyhow::Result<Saved> {
    let png = encode_png(i32::try_from(width)?, i32::try_from(height)?, rgba)?;
    if !write {
        return Ok(Saved { path: None, png });
    }
    let path = screenshot_path().context("failed to determine where to save the screenshot")?;
    std::fs::write(&path, &png).with_context(|| format!("failed to write the screenshot to {}", path.display()))?;
    Ok(Saved { path: Some(path), png })
}

pub(super) fn capture_frame(
    renderer: &mut GlesRenderer, elements: &[GlideRenderElements], size: (i32, i32), scale: f64, transform: Transform,
) -> anyhow::Result<Vec<u8>> {
    capture_elements(renderer, elements, size, scale, transform, [0.0, 0.0, 0.0, 1.0])
}

pub(super) fn capture_elements<E>(
    renderer: &mut GlesRenderer, elements: &[E], size: (i32, i32), scale: f64, transform: Transform, clear: [f32; 4],
) -> anyhow::Result<Vec<u8>>
where
    E: smithay::backend::renderer::element::RenderElement<GlesRenderer>,
{
    let (width, height) = size;
    if width <= 0 || height <= 0 {
        return Err(anyhow!("output has no usable size ({width}x{height}), nothing to capture"));
    }

    let mut texture: GlesTexture = renderer
        .create_buffer(Fourcc::Abgr8888, (width, height).into())
        .context("failed to allocate an offscreen capture buffer")?;
    let mut target = renderer.bind(&mut texture).context("failed to bind the offscreen capture buffer")?;

    let mut tracker = OutputDamageTracker::new((width, height), scale, transform);
    tracker
        .render_output(renderer, &mut target, 0, elements, clear)
        .map_err(|err| anyhow!("failed to render the frame: {err:?}"))?;

    let region = Rectangle::<i32, Buffer>::from_size((width, height).into());
    let mapping = renderer
        .copy_framebuffer(&target, region, Fourcc::Argb8888)
        .context("failed to read back the offscreen capture buffer")?;
    let data = renderer.map_texture(&mapping).context("failed to map the offscreen capture buffer")?;

    Ok(data.to_vec())
}

fn bgra_to_rgba_in_place(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}

fn pick_screenshot_dir(home: &Path, pictures_override: Option<&Path>) -> PathBuf {
    let pictures = pictures_override.map_or_else(|| home.join("Pictures"), Path::to_path_buf);
    if pictures.is_dir() { pictures } else { home.to_path_buf() }
}

fn parse_user_dirs_pictures(text: &str, home: &Path) -> Option<PathBuf> {
    let value = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("XDG_PICTURES_DIR=").map(|value| value.trim().trim_matches('"')))?;
    if let Some(rest) = value.strip_prefix("$HOME") {
        let rest = rest.trim_start_matches('/');
        return Some(if rest.is_empty() { home.to_path_buf() } else { home.join(rest) });
    }
    value.starts_with('/').then(|| PathBuf::from(value))
}

fn screenshot_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| anyhow!("HOME is not set"))?;
    let pictures_override = std::env::var_os("XDG_PICTURES_DIR").map(PathBuf::from).or_else(|| {
        let user_dirs = glide_config::xdg_path("", "user-dirs.dirs")?;
        parse_user_dirs_pictures(&std::fs::read_to_string(user_dirs).ok()?, &home)
    });
    Ok(pick_screenshot_dir(&home, pictures_override.as_deref()))
}

fn screenshot_path() -> anyhow::Result<PathBuf> {
    let dir = screenshot_dir()?;
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    Ok(dir.join(format!("glide-screenshot-{timestamp}.png")))
}

fn encode_png(width: i32, height: i32, rgba: &[u8]) -> anyhow::Result<Vec<u8>> {
    let width = u32::try_from(width).context("screenshot width does not fit in a u32")?;
    let height = u32::try_from(height).context("screenshot height does not fit in a u32")?;

    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().context("failed to write PNG header")?;
        writer.write_image_data(rgba).context("failed to write PNG image data")?;
        writer.finish().context("failed to finish the PNG stream")?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localised_pictures_dir_is_read_from_user_dirs() {
        let home = Path::new("/home/u");
        let text = "# comment\nXDG_DESKTOP_DIR=\"$HOME/Стільниця\"\nXDG_PICTURES_DIR=\"$HOME/Зображення\"\n";
        assert_eq!(parse_user_dirs_pictures(text, home), Some(PathBuf::from("/home/u/Зображення")));
        assert_eq!(
            parse_user_dirs_pictures("XDG_PICTURES_DIR=\"/data/pics\"", home),
            Some(PathBuf::from("/data/pics"))
        );
        assert_eq!(parse_user_dirs_pictures("XDG_PICTURES_DIR=\"$HOME/\"", home), Some(PathBuf::from("/home/u")));
        assert_eq!(parse_user_dirs_pictures("XDG_MUSIC_DIR=\"$HOME/M\"", home), None);
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glide-screenshot-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn argb8888_readback_is_swizzled_to_png_rgba_order() {
        let mut pixels = vec![10, 20, 30, 40, 1, 2, 3, 4];
        bgra_to_rgba_in_place(&mut pixels);
        assert_eq!(pixels, vec![30, 20, 10, 40, 3, 2, 1, 4]);
    }

    #[test]
    fn prefers_an_existing_pictures_override_over_home() {
        let home = test_dir("override-exists");
        let pictures = home.join("Pics");
        std::fs::create_dir_all(&pictures).unwrap();
        assert_eq!(pick_screenshot_dir(&home, Some(&pictures)), pictures, "an existing override must win");
    }

    #[test]
    fn falls_back_to_home_pictures_when_no_override_is_given() {
        let home = test_dir("default-pictures");
        let pictures = home.join("Pictures");
        std::fs::create_dir_all(&pictures).unwrap();
        assert_eq!(pick_screenshot_dir(&home, None), pictures);
    }

    #[test]
    fn falls_back_to_home_when_neither_pictures_dir_exists() {
        let home = test_dir("no-pictures-anywhere");
        assert_eq!(pick_screenshot_dir(&home, None), home, "must never point at a directory that doesn't exist");
        let bogus_override = home.join("does-not-exist");
        assert_eq!(pick_screenshot_dir(&home, Some(&bogus_override)), home);
    }

    #[test]
    fn encode_png_round_trips_exact_pixel_data() {
        let (w, h) = (3_i32, 2_i32);
        let rgba: Vec<u8> =
            (0..(w * h * 4)).map(|i| u8::try_from((i * 17) % 256).expect("modulo 256 always fits in a u8")).collect();

        let png = encode_png(w, h, &rgba).expect("encode_png must succeed");

        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().expect("must be a valid PNG");
        assert_eq!(reader.info().width, w.cast_unsigned());
        assert_eq!(reader.info().height, h.cast_unsigned());
        assert_eq!(reader.info().color_type, png::ColorType::Rgba);

        let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
        let frame = reader.next_frame(&mut buf).expect("must decode a frame");
        assert_eq!(&buf[..frame.buffer_size()], rgba.as_slice());
    }

    #[test]
    fn encode_png_rejects_a_width_that_does_not_fit_in_a_u32() {
        assert!(encode_png(-1, 10, &[]).is_err());
    }
}

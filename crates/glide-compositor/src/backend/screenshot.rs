//! A minimal, self-contained "press a key, get a PNG" screenshot capture.
//!
//! This deliberately does **not** implement `ext-image-copy-capture-v1`
//! (see `docs/ROADMAP.md`, phase B4) — that's the real, client-facing
//! protocol `grim`/`grimshot` and friends speak, and wiring it up properly
//! (session negotiation, buffer constraints, cursor toggling, etc.) is a
//! whole phase of work on its own. This is a much smaller escape hatch: a
//! single internal keybind (`KeyAction::Screenshot`) that re-renders the
//! output's already-built frame elements into an offscreen GL texture,
//! reads it back, and writes it straight to disk.

use std::io::BufWriter;
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

/// Renders `elements` (already built for this output, in front-to-back
/// order, in physical pixels at `scale`) into a throwaway offscreen
/// buffer, reads it back, and writes it to a timestamped PNG file.
///
/// Returns the path written to.
pub(super) fn capture_screenshot(
    renderer: &mut GlesRenderer,
    elements: &[GlideRenderElements],
    size: (i32, i32),
    scale: f64,
    transform: Transform,
) -> anyhow::Result<PathBuf> {
    let (width, height) = size;
    if width <= 0 || height <= 0 {
        return Err(anyhow!(
            "output has no usable size ({width}x{height}), nothing to capture"
        ));
    }

    let mut texture: GlesTexture = renderer
        .create_buffer(Fourcc::Abgr8888, (width, height).into())
        .context("failed to allocate an offscreen capture buffer")?;
    let mut target = renderer
        .bind(&mut texture)
        .context("failed to bind the offscreen capture buffer")?;

    let mut tracker = OutputDamageTracker::new((width, height), scale, transform);
    // `age: 0` — a fresh tracker every call, so this always forces full
    // (not incremental) damage; there's no previous frame to diff against.
    tracker
        .render_output(renderer, &mut target, 0, elements, [0.0, 0.0, 0.0, 1.0])
        .map_err(|err| anyhow!("failed to render the screenshot: {err:?}"))?;

    let region = Rectangle::<i32, Buffer>::from_size((width, height).into());
    let mapping = renderer
        .copy_framebuffer(&target, region, Fourcc::Abgr8888)
        .context("failed to read back the offscreen capture buffer")?;
    let data = renderer
        .map_texture(&mapping)
        .context("failed to map the offscreen capture buffer")?;

    let path = screenshot_path().context("failed to determine where to save the screenshot")?;
    write_png(&path, width, height, data).context("failed to write the screenshot to disk")?;
    Ok(path)
}

/// Picks a directory to save into, preferring an explicit
/// `$XDG_PICTURES_DIR` (or plain `~/Pictures`) if it actually exists, and
/// falling back to `home` otherwise (e.g. a fresh account with no
/// `xdg-user-dirs` setup yet still gets a screenshot saved *somewhere*
/// instead of the request silently failing).
fn pick_screenshot_dir(home: &Path, pictures_override: Option<&Path>) -> PathBuf {
    let pictures = pictures_override.map_or_else(|| home.join("Pictures"), Path::to_path_buf);
    if pictures.is_dir() {
        pictures
    } else {
        home.to_path_buf()
    }
}

fn screenshot_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set"))?;
    let pictures_override = std::env::var_os("XDG_PICTURES_DIR").map(PathBuf::from);
    Ok(pick_screenshot_dir(&home, pictures_override.as_deref()))
}

fn screenshot_path() -> anyhow::Result<PathBuf> {
    let dir = screenshot_dir()?;
    // Millisecond precision so two captures within the same second (easy
    // to do by pressing Print twice) don't overwrite each other.
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    Ok(dir.join(format!("glide-screenshot-{timestamp}.png")))
}

/// `rgba` must be exactly `width * height * 4` bytes of straight
/// (non-premultiplied is fine — every scanned-out pixel is opaque)
/// RGBA8, row-major, top-to-bottom.
fn write_png(path: &Path, width: i32, height: i32, rgba: &[u8]) -> anyhow::Result<()> {
    let width = u32::try_from(width).context("screenshot width does not fit in a u32")?;
    let height = u32::try_from(height).context("screenshot height does not fit in a u32")?;

    let file = std::fs::File::create(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .context("failed to write PNG header")?;
    writer
        .write_image_data(rgba)
        .context("failed to write PNG image data")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "glide-screenshot-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn prefers_an_existing_pictures_override_over_home() {
        let home = test_dir("override-exists");
        let pictures = home.join("Pics");
        std::fs::create_dir_all(&pictures).unwrap();
        assert_eq!(
            pick_screenshot_dir(&home, Some(&pictures)),
            pictures,
            "an existing override must win"
        );
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
        assert_eq!(
            pick_screenshot_dir(&home, None),
            home,
            "must never point at a directory that doesn't exist"
        );
        let bogus_override = home.join("does-not-exist");
        assert_eq!(pick_screenshot_dir(&home, Some(&bogus_override)), home);
    }

    #[test]
    fn write_png_round_trips_exact_pixel_data() {
        let dir = test_dir("write-png");
        let path = dir.join("shot.png");
        let (w, h) = (3_i32, 2_i32);
        let rgba: Vec<u8> = (0..(w * h * 4))
            .map(|i| u8::try_from((i * 17) % 256).expect("modulo 256 always fits in a u8"))
            .collect();

        write_png(&path, w, h, &rgba).expect("write_png must succeed");

        let file =
            std::io::BufReader::new(std::fs::File::open(&path).expect("the file must exist"));
        let decoder = png::Decoder::new(file);
        let mut reader = decoder.read_info().expect("must be a valid PNG");
        assert_eq!(reader.info().width, w.cast_unsigned());
        assert_eq!(reader.info().height, h.cast_unsigned());
        assert_eq!(reader.info().color_type, png::ColorType::Rgba);

        let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
        let frame = reader.next_frame(&mut buf).expect("must decode a frame");
        assert_eq!(&buf[..frame.buffer_size()], rgba.as_slice());
    }

    #[test]
    fn write_png_rejects_a_width_that_does_not_fit_in_a_u32() {
        let dir = test_dir("negative-size");
        let path = dir.join("shot.png");
        assert!(write_png(&path, -1, 10, &[]).is_err());
    }
}

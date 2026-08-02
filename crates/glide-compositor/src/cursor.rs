//! Pointer cursor: theme loading and rendering.
//!
//! There are two sources for what the cursor looks like at any moment:
//!
//! - the client under the pointer can supply its own cursor surface (this is
//!   how a terminal shows an I-beam over its text area), and
//! - otherwise we draw the default cursor from the system XCursor theme.
//!
//! Both are exposed as render elements via [`PointerElement`], which is fed
//! the current [`CursorImageStatus`] each frame.

use std::{io::Read, time::Duration};

use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer, Texture,
        element::{
            AsRenderElements, Kind,
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            surface::WaylandSurfaceRenderElement,
        },
    },
    input::pointer::CursorImageStatus,
    render_elements,
    utils::{Physical, Point, Scale},
};
use xcursor::{
    CursorTheme,
    parser::{Image, parse_xcursor},
};

/// Cursor drawn when the theme cannot be loaded at all, as a 12x19 classic
/// arrow. `#` is the black outline, `.` the white fill, space transparent.
///
/// Having *some* cursor matters more than having the right one: an invisible
/// pointer on a bare tty leaves no way to tell whether input is working.
const FALLBACK_ARROW: &[&str] = &[
    "#           ",
    "##          ",
    "#.#         ",
    "#..#        ",
    "#...#       ",
    "#....#      ",
    "#.....#     ",
    "#......#    ",
    "#.......#   ",
    "#........#  ",
    "#.....##### ",
    "#..#..#     ",
    "#.# #..#    ",
    "##  #..#    ",
    "#    #..#   ",
    "     #..#   ",
    "      #..#  ",
    "      #..#  ",
    "       ##   ",
];

/// The default cursor, loaded from the system XCursor theme.
pub struct Cursor {
    images: Vec<Image>,
    size: u32,
}

impl Cursor {
    pub fn load() -> Self {
        let theme_name = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".into());
        let size = std::env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(24);

        let images = load_theme_images(&theme_name).unwrap_or_else(|err| {
            tracing::warn!(
                theme = theme_name,
                "could not load xcursor theme ({err}), using built-in fallback cursor"
            );
            vec![fallback_image()]
        });

        Self { images, size }
    }

    /// Picks the image to draw for the given output scale and elapsed time,
    /// advancing through the frames of an animated cursor.
    pub fn image(&self, scale: u32, time: Duration) -> Image {
        frame(time.as_millis() as u32, self.size * scale, &self.images)
    }
}

#[derive(thiserror::Error, Debug)]
enum ThemeError {
    #[error("theme has no default cursor")]
    NoDefaultCursor,
    #[error("could not open cursor file: {0}")]
    File(#[from] std::io::Error),
    #[error("could not parse cursor file")]
    Parse,
}

fn load_theme_images(theme_name: &str) -> Result<Vec<Image>, ThemeError> {
    let theme = CursorTheme::load(theme_name);
    let path = theme
        .load_icon("default")
        .ok_or(ThemeError::NoDefaultCursor)?;
    let mut data = Vec::new();
    std::fs::File::open(path)?.read_to_end(&mut data)?;
    parse_xcursor(&data).ok_or(ThemeError::Parse)
}

/// Rasterizes [`FALLBACK_ARROW`].
///
/// Pixels are emitted in the byte order XCursor files use, which is also
/// what `Fourcc::Argb8888` expects in memory on a little-endian machine:
/// B, G, R, A, with alpha premultiplied. Since every pixel here is either
/// fully opaque or fully transparent, premultiplying is a no-op.
fn fallback_image() -> Image {
    let height = FALLBACK_ARROW.len();
    let width = FALLBACK_ARROW
        .iter()
        .map(|row| row.len())
        .max()
        .unwrap_or(0);

    let mut pixels = Vec::with_capacity(width * height * 4);
    for row in FALLBACK_ARROW {
        let row = row.as_bytes();
        for x in 0..width {
            let pixel = match row.get(x) {
                Some(b'#') => [0, 0, 0, 255],       // black, opaque
                Some(b'.') => [255, 255, 255, 255], // white, opaque
                _ => [0, 0, 0, 0],                  // transparent
            };
            pixels.extend_from_slice(&pixel);
        }
    }

    Image {
        size: height as u32,
        width: width as u32,
        height: height as u32,
        xhot: 0,
        yhot: 0,
        delay: 0,
        pixels_rgba: pixels,
        pixels_argb: Vec::new(), // unused by us
    }
}

/// All images matching the size closest to `size`. An XCursor file holds
/// several resolutions, and animated cursors hold several frames per
/// resolution; this narrows to the frames of one resolution.
fn nearest_images(size: u32, images: &[Image]) -> impl Iterator<Item = &Image> {
    let nearest = images
        .iter()
        .min_by_key(|image| (size as i32 - image.size as i32).abs())
        .expect("cursor always has at least the fallback image");

    images
        .iter()
        .filter(move |image| image.width == nearest.width && image.height == nearest.height)
}

/// Selects the animation frame for `millis`, looping over the total duration.
fn frame(mut millis: u32, size: u32, images: &[Image]) -> Image {
    let total: u32 = nearest_images(size, images).map(|image| image.delay).sum();
    if total == 0 {
        // Not animated.
        return nearest_images(size, images)
            .next()
            .expect("cursor always has at least the fallback image")
            .clone();
    }

    millis %= total;
    for image in nearest_images(size, images) {
        if millis < image.delay {
            return image.clone();
        }
        millis -= image.delay;
    }

    unreachable!("millis was reduced below the total delay above")
}

render_elements! {
    pub PointerRenderElement<R> where R: ImportAll + ImportMem;
    Surface=WaylandSurfaceRenderElement<R>,
    Memory=MemoryRenderBufferRenderElement<R>,
}

impl<R: Renderer> std::fmt::Debug for PointerRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface(arg) => f.debug_tuple("Surface").field(arg).finish(),
            Self::Memory(arg) => f.debug_tuple("Memory").field(arg).finish(),
            Self::_GenericCatcher(arg) => f.debug_tuple("_GenericCatcher").field(arg).finish(),
        }
    }
}

/// Renders whatever the cursor currently is: a client-provided surface, the
/// themed default, or nothing at all when hidden.
pub struct PointerElement {
    buffer: Option<MemoryRenderBuffer>,
    status: CursorImageStatus,
}

impl Default for PointerElement {
    fn default() -> Self {
        Self {
            buffer: None,
            status: CursorImageStatus::default_named(),
        }
    }
}

impl PointerElement {
    pub fn set_status(&mut self, status: CursorImageStatus) {
        self.status = status;
    }

    /// Sets the buffer holding the themed default cursor, used for every
    /// status other than a client-provided surface.
    pub fn set_buffer(&mut self, buffer: MemoryRenderBuffer) {
        self.buffer = Some(buffer);
    }
}

impl<T: Texture + Clone + Send + 'static, R> AsRenderElements<R> for PointerElement
where
    R: Renderer<TextureId = T> + ImportAll + ImportMem,
{
    type RenderElement = PointerRenderElement<R>;

    fn render_elements<E: From<PointerRenderElement<R>>>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Vec<E> {
        match &self.status {
            CursorImageStatus::Hidden => Vec::new(),
            // Named shapes beyond the default aren't distinguished yet: all
            // of them draw the theme's default cursor.
            CursorImageStatus::Named(_) => match self.buffer.as_ref() {
                Some(buffer) => MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    location.to_f64(),
                    buffer,
                    None,
                    None,
                    None,
                    Kind::Cursor,
                )
                .map(|element| vec![E::from(PointerRenderElement::Memory(element))])
                .unwrap_or_default(),
                None => Vec::new(),
            },
            CursorImageStatus::Surface(surface) => {
                let elements: Vec<PointerRenderElement<R>> =
                    smithay::backend::renderer::element::surface::render_elements_from_surface_tree(
                        renderer,
                        surface,
                        location,
                        scale,
                        alpha,
                        Kind::Cursor,
                    );
                elements.into_iter().map(E::from).collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_image_is_well_formed() {
        let image = fallback_image();

        let expected_width = FALLBACK_ARROW.iter().map(|r| r.len()).max().unwrap() as u32;
        assert_eq!(image.width, expected_width);
        assert_eq!(image.height, FALLBACK_ARROW.len() as u32);

        // Four bytes per pixel; a mismatch here would be read as a shifted
        // image (or out of bounds) by the renderer.
        assert_eq!(
            image.pixels_rgba.len(),
            (image.width * image.height * 4) as usize
        );
    }

    #[test]
    fn fallback_image_uses_argb8888_byte_order() {
        let image = fallback_image();
        let pixels: Vec<&[u8]> = image.pixels_rgba.chunks_exact(4).collect();

        // The very first cell of the arrow is '#': opaque black. In the
        // byte order Fourcc::Argb8888 expects on a little-endian machine
        // that is B=0, G=0, R=0, A=255 — the alpha byte last. Getting this
        // backwards yields an invisible (alpha=0) cursor, which is exactly
        // the failure this cursor work is meant to fix.
        assert_eq!(pixels[0], &[0, 0, 0, 255]);

        // Trailing padding on the first row is transparent.
        assert_eq!(pixels[(image.width - 1) as usize], &[0, 0, 0, 0]);

        // Row 2 is "#.#": black, white, black.
        let row2 = (image.width * 2) as usize;
        assert_eq!(pixels[row2], &[0, 0, 0, 255]);
        assert_eq!(pixels[row2 + 1], &[255, 255, 255, 255]);
        assert_eq!(pixels[row2 + 2], &[0, 0, 0, 255]);
    }

    #[test]
    fn fallback_image_is_not_fully_transparent() {
        // A cursor that is entirely transparent is indistinguishable from
        // having no cursor at all.
        let image = fallback_image();
        assert!(
            image.pixels_rgba.chunks_exact(4).any(|px| px[3] != 0),
            "fallback cursor has no visible pixels"
        );
    }

    #[test]
    fn cursor_loads_and_yields_an_image() {
        // Exercises the real theme lookup on this machine, falling back to
        // the built-in arrow if no theme is installed. Either way a cursor
        // must come out, and picking a frame must not panic.
        let cursor = Cursor::load();
        let image = cursor.image(1, Duration::ZERO);
        assert!(image.width > 0 && image.height > 0);
        assert_eq!(
            image.pixels_rgba.len(),
            (image.width * image.height * 4) as usize
        );
    }
}

//! Pointer cursor: theme loading and rendering.
//!
//! There are two sources for what the cursor looks like at any moment:
//!
//! - the client under the pointer can supply its own cursor surface (this is
//!   how a terminal shows an I-beam over its text area), and
//! - otherwise we draw the default cursor from the system `XCursor` theme.
//!
//! Both are exposed as render elements via [`PointerElement`], which is fed
//! the current [`CursorImageStatus`] each frame.

use std::{collections::HashMap, io::Read, time::Duration};

use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer, Texture,
        element::{
            AsRenderElements, Kind,
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            surface::WaylandSurfaceRenderElement,
        },
    },
    input::pointer::{CursorIcon, CursorImageStatus},
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

/// Themed cursors, loaded from the system `XCursor` theme per icon shape.
///
/// Shapes are loaded lazily and cached: most sessions only ever touch a
/// handful (default plus the resize arrows), and loading all ~35 up front
/// would mostly parse files nothing ever displays. A shape the theme is
/// missing falls back to whatever [`CursorIcon::Default`] resolved to, so
/// the pointer is never invisible just because a theme lacks `sw-resize`.
pub struct Cursor {
    theme: CursorTheme,
    size: u32,
    cache: HashMap<CursorIcon, Vec<Image>>,
}

impl Cursor {
    pub fn load() -> Self {
        let theme_name = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".into());
        let size = std::env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(24);

        let theme = CursorTheme::load(&theme_name);

        let mut cursor = Self {
            theme,
            size,
            cache: HashMap::new(),
        };

        // Resolve the default eagerly: it doubles as the fallback for every
        // other shape, and if even it is missing we want the warning at
        // startup, not on first pointer motion.
        let default_images = cursor
            .load_icon_images(CursorIcon::Default)
            .unwrap_or_else(|| {
                tracing::warn!(
                    theme = theme_name,
                    "could not load the xcursor theme's default cursor, \
                     using the built-in fallback"
                );
                vec![fallback_image()]
            });
        cursor.cache.insert(CursorIcon::Default, default_images);

        cursor
    }

    /// Picks the image to draw for `icon` at the given output scale and
    /// elapsed time, advancing through the frames of an animated cursor.
    pub fn image(&mut self, icon: CursorIcon, scale: u32, time: Duration) -> Image {
        // `frame` only ever uses `millis` modulo the animation's total delay
        // (typically well under a minute), so wrapping every ~49 days of
        // continuous uptime has no observable effect on which frame is
        // picked.
        #[allow(clippy::cast_possible_truncation)]
        let millis = time.as_millis() as u32;
        let size = self.size * scale;
        frame(millis, size, self.images_for(icon))
    }

    /// Cached frames for `icon`, loading them on first use.
    fn images_for(&mut self, icon: CursorIcon) -> &[Image] {
        if !self.cache.contains_key(&icon) {
            let images = self.load_icon_images(icon).unwrap_or_else(|| {
                tracing::debug!(
                    icon = icon.name(),
                    "xcursor theme lacks this shape; substituting the default cursor"
                );
                self.cache
                    .get(&CursorIcon::Default)
                    .expect("Default is inserted unconditionally in load()")
                    .clone()
            });
            self.cache.insert(icon, images);
        }

        self.cache
            .get(&icon)
            .expect("inserted above if it was missing")
    }

    /// Loads `icon` from the theme, trying its CSS name first and then the
    /// legacy X11 aliases — themes predating the CSS naming ship files like
    /// `top_left_corner` rather than `nw-resize`, and `alt_names` is
    /// exactly that mapping.
    fn load_icon_images(&self, icon: CursorIcon) -> Option<Vec<Image>> {
        std::iter::once(icon.name())
            .chain(icon.alt_names().iter().copied())
            .find_map(|name| {
                let path = self.theme.load_icon(name)?;
                let mut data = Vec::new();
                std::fs::File::open(path)
                    .ok()?
                    .read_to_end(&mut data)
                    .ok()?;
                parse_xcursor(&data)
            })
    }
}

/// Rasterizes [`FALLBACK_ARROW`].
///
/// Pixels are emitted in the byte order `XCursor` files use, which is also
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

    // FALLBACK_ARROW is a small fixed array (19x13), nowhere near u32::MAX,
    // so these conversions can never actually fail.
    let width = u32::try_from(width).expect("FALLBACK_ARROW is a small fixed-size array");
    let height = u32::try_from(height).expect("FALLBACK_ARROW is a small fixed-size array");

    Image {
        size: height,
        width,
        height,
        xhot: 0,
        yhot: 0,
        delay: 0,
        pixels_rgba: pixels,
        pixels_argb: Vec::new(), // unused by us
    }
}

/// All images matching the size closest to `size`. An `XCursor` file holds
/// several resolutions, and animated cursors hold several frames per
/// resolution; this narrows to the frames of one resolution.
fn nearest_images(size: u32, images: &[Image]) -> impl Iterator<Item = &Image> {
    let nearest = images
        .iter()
        .min_by_key(|image| (size.cast_signed() - image.size.cast_signed()).abs())
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

        let expected_width =
            u32::try_from(FALLBACK_ARROW.iter().map(|r| r.len()).max().unwrap()).unwrap();
        assert_eq!(image.width, expected_width);
        assert_eq!(image.height, u32::try_from(FALLBACK_ARROW.len()).unwrap());

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
        let mut cursor = Cursor::load();
        let image = cursor.image(CursorIcon::Default, 1, Duration::ZERO);
        assert!(image.width > 0 && image.height > 0);
        assert_eq!(
            image.pixels_rgba.len(),
            (image.width * image.height * 4) as usize
        );
    }
}

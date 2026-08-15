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

    pub fn image_with_index(
        &mut self,
        icon: CursorIcon,
        scale: u32,
        time: Duration,
    ) -> (Image, usize) {
        #[allow(clippy::cast_possible_truncation)]
        let millis = time.as_millis() as u32;
        let size = self.size * scale;
        frame_with_index(millis, size, self.images_for(icon))
    }

    #[allow(dead_code)]
    pub fn image(&mut self, icon: CursorIcon, scale: u32, time: Duration) -> Image {
        self.image_with_index(icon, scale, time).0
    }

    pub fn time_to_next_frame(
        &mut self,
        icon: CursorIcon,
        scale: u32,
        time: Duration,
    ) -> Option<Duration> {
        #[allow(clippy::cast_possible_truncation)]
        let millis = time.as_millis() as u32;
        let size = self.size * scale;
        time_to_next_frame(millis, size, self.images_for(icon))
    }

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
                Some(b'#') => [0, 0, 0, 255],
                Some(b'.') => [255, 255, 255, 255],
                _ => [0, 0, 0, 0],
            };
            pixels.extend_from_slice(&pixel);
        }
    }

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
        pixels_argb: Vec::new(),
    }
}

fn nearest_images(size: u32, images: &[Image]) -> impl Iterator<Item = &Image> {
    let nearest = images
        .iter()
        .min_by_key(|image| (size.cast_signed() - image.size.cast_signed()).abs())
        .expect("cursor always has at least the fallback image");

    images
        .iter()
        .filter(move |image| image.width == nearest.width && image.height == nearest.height)
}

fn time_to_next_frame(mut millis: u32, size: u32, images: &[Image]) -> Option<Duration> {
    let total: u32 = nearest_images(size, images).map(|image| image.delay).sum();
    if total == 0 || nearest_images(size, images).count() < 2 {
        return None;
    }

    millis %= total;
    for image in nearest_images(size, images) {
        if millis < image.delay {
            return Some(Duration::from_millis(u64::from(image.delay - millis)));
        }
        millis -= image.delay;
    }

    unreachable!("millis was reduced below the total delay above")
}

fn frame_with_index(mut millis: u32, size: u32, images: &[Image]) -> (Image, usize) {
    let total: u32 = nearest_images(size, images).map(|image| image.delay).sum();
    if total == 0 {
        let img = nearest_images(size, images)
            .next()
            .expect("cursor always has at least the fallback image")
            .clone();
        return (img, 0);
    }

    millis %= total;
    for (idx, image) in nearest_images(size, images).enumerate() {
        if millis < image.delay {
            return (image.clone(), idx);
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

        assert_eq!(
            image.pixels_rgba.len(),
            (image.width * image.height * 4) as usize
        );
    }

    #[test]
    fn fallback_image_uses_argb8888_byte_order() {
        let image = fallback_image();
        let pixels: Vec<&[u8]> = image.pixels_rgba.chunks_exact(4).collect();

        assert_eq!(pixels[0], &[0, 0, 0, 255]);

        assert_eq!(pixels[(image.width - 1) as usize], &[0, 0, 0, 0]);

        let row2 = (image.width * 2) as usize;
        assert_eq!(pixels[row2], &[0, 0, 0, 255]);
        assert_eq!(pixels[row2 + 1], &[255, 255, 255, 255]);
        assert_eq!(pixels[row2 + 2], &[0, 0, 0, 255]);
    }

    #[test]
    fn fallback_image_is_not_fully_transparent() {
        let image = fallback_image();
        assert!(
            image.pixels_rgba.chunks_exact(4).any(|px| px[3] != 0),
            "fallback cursor has no visible pixels"
        );
    }

    #[test]
    fn cursor_loads_and_yields_an_image() {
        let mut cursor = Cursor::load();
        let image = cursor.image(CursorIcon::Default, 1, Duration::ZERO);
        assert!(image.width > 0 && image.height > 0);
        assert_eq!(
            image.pixels_rgba.len(),
            (image.width * image.height * 4) as usize
        );
    }

    #[test]
    fn cursor_cache_caches_icons_and_missing_shapes() {
        let mut cursor = Cursor::load();

        assert!(cursor.cache.contains_key(&CursorIcon::Default));
        let initial_cache_count = cursor.cache.len();

        let _ = cursor.image_with_index(CursorIcon::Pointer, 1, Duration::ZERO);
        assert!(cursor.cache.contains_key(&CursorIcon::Pointer));

        let _ = cursor.image_with_index(CursorIcon::ContextMenu, 1, Duration::ZERO);
        assert!(cursor.cache.contains_key(&CursorIcon::ContextMenu));

        let count_after_queries = cursor.cache.len();
        assert!(count_after_queries >= initial_cache_count);

        let _ = cursor.image_with_index(CursorIcon::Pointer, 1, Duration::from_millis(100));
        let _ = cursor.image_with_index(CursorIcon::ContextMenu, 2, Duration::from_millis(200));

        assert_eq!(cursor.cache.len(), count_after_queries);
    }

    #[test]
    fn cursor_cache_performance_stress() {
        let mut cursor = Cursor::load();
        let icons = [
            CursorIcon::Default,
            CursorIcon::Pointer,
            CursorIcon::Grab,
            CursorIcon::Progress,
            CursorIcon::Wait,
            CursorIcon::Crosshair,
            CursorIcon::Text,
            CursorIcon::NotAllowed,
            CursorIcon::NwResize,
            CursorIcon::SeResize,
        ];

        let start = std::time::Instant::now();
        for i in 0..50_000 {
            let icon = icons[i % icons.len()];
            let dur = Duration::from_millis((i as u64) * 10);
            let (img, _idx) = cursor.image_with_index(icon, 1, dur);
            assert!(img.width > 0 && img.height > 0);
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(1),
            "50,000 cache lookups took too long: {elapsed:?}"
        );
    }
}

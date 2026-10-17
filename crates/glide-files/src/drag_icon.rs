use std::path::Path;

use resvg::tiny_skia::{Pixmap, PixmapPaint, Transform};
use resvg::usvg;

pub const SIZE: u32 = 48;
pub const SCALE: u32 = 2;
pub const OFFSET: (i32, i32) = (10, 8);
const STACKED: f32 = 0.55;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragImage {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

#[must_use]
pub fn render(art: &Path, count: usize) -> Option<DragImage> {
    let pixels = SIZE * SCALE;
    let icon = rasterize(art, pixels)?;
    let offset = if count > 1 { pixels / 8 } else { 0 };
    let side = pixels + offset;
    let mut canvas = Pixmap::new(side, side)?;
    if count > 1 {
        let behind = PixmapPaint { opacity: STACKED, ..PixmapPaint::default() };
        canvas.draw_pixmap(
            offset.cast_signed(),
            offset.cast_signed(),
            icon.as_ref(),
            &behind,
            Transform::identity(),
            None,
        );
    }
    canvas.draw_pixmap(0, 0, icon.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    let mut bgra = canvas.take();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(DragImage { width: side, height: side, bgra })
}

fn rasterize(art: &Path, size: u32) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(size, size)?;
    if art.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("svg")) {
        let data = std::fs::read(art).ok()?;
        let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).ok()?;
        let natural = tree.size();
        let scale = (size as f32 / natural.width()).min(size as f32 / natural.height());
        let (dx, dy) = ((size as f32 - natural.width() * scale) / 2.0, (size as f32 - natural.height() * scale) / 2.0);
        resvg::render(&tree, Transform::from_scale(scale, scale).post_translate(dx, dy), &mut pixmap.as_mut());
        return Some(pixmap);
    }
    let image = image::open(art).ok()?.resize(size, size, image::imageops::FilterType::Triangle).to_rgba8();
    let (dx, dy) = ((size - image.width()) / 2, (size - image.height()) / 2);
    let pixels = pixmap.pixels_mut();
    for (x, y, pixel) in image.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        let premultiply = |channel: u8| (u16::from(channel) * u16::from(a) / 255) as u8;
        let index = ((y + dy) * size + x + dx) as usize;
        if let Some(color) =
            resvg::tiny_skia::PremultipliedColorU8::from_rgba(premultiply(r), premultiply(g), premultiply(b), a)
        {
            pixels[index] = color;
        }
    }
    Some(pixmap)
}

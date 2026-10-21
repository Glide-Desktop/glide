use tiny_skia::{
    BlendMode, Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Rect, Stroke,
    Transform,
};

use super::model::{Mark, Shape};

pub const PEN_WIDTH: f32 = 4.0;
pub const RECT_WIDTH: f32 = 3.0;
pub const ARROW_WIDTH: f32 = 4.0;
pub const MARK_RADIUS: f32 = glide_theme::shape::R1;
pub const HIGHLIGHT_ALPHA: f32 = 0.38;
pub const STEP_RADIUS: f32 = 14.0;
pub const BLUR_BLOCK: f64 = 10.0;

#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

impl Image {
    #[must_use]
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let len = usize::try_from(u64::from(width) * u64::from(height) * 4).ok()?;
        (width > 0 && height > 0).then(|| Self { width, height, data: vec![0; len] })
    }

    #[must_use]
    pub fn from_bgra(width: u32, height: u32, mut bgra: Vec<u8>) -> Option<Self> {
        let len = usize::try_from(u64::from(width) * u64::from(height) * 4).ok()?;
        if bgra.len() < len || width == 0 || height == 0 {
            return None;
        }
        bgra.truncate(len);
        swap_red_blue(&mut bgra);
        Some(Self { width, height, data: bgra })
    }

    #[must_use]
    pub fn to_bgra(&self) -> Vec<u8> {
        let mut out = self.data.clone();
        swap_red_blue(&mut out);
        out
    }

    #[must_use]
    pub fn crop(&self, (x, y, w, h): (i32, i32, i32, i32)) -> Option<Self> {
        let x0 = x.clamp(0, self.width as i32);
        let y0 = y.clamp(0, self.height as i32);
        let x1 = (x + w).clamp(0, self.width as i32);
        let y1 = (y + h).clamp(0, self.height as i32);
        let mut out = Self::new((x1 - x0).max(0) as u32, (y1 - y0).max(0) as u32)?;
        let stride = self.width as usize * 4;
        let row = out.width as usize * 4;
        for (index, line) in out.data.chunks_exact_mut(row).enumerate() {
            let start = (y0 as usize + index) * stride + x0 as usize * 4;
            line.copy_from_slice(&self.data[start..start + row]);
        }
        Some(out)
    }

    pub fn pixmap(&mut self) -> Option<PixmapMut<'_>> {
        PixmapMut::from_bytes(&mut self.data, self.width, self.height)
    }

    pub fn draw(&mut self, src: &Self, x: i32, y: i32) {
        let Some(source) = tiny_skia::PixmapRef::from_bytes(&src.data, src.width, src.height) else {
            return;
        };
        if let Some(mut target) = self.pixmap() {
            target.draw_pixmap(x, y, source, &PixmapPaint::default(), Transform::identity(), None);
        }
    }

    #[must_use]
    pub fn into_straight(mut self) -> Vec<u8> {
        for pixel in self.data.chunks_exact_mut(4) {
            let alpha = u32::from(pixel[3]);
            match alpha {
                0 => pixel.copy_from_slice(&[0, 0, 0, 0]),
                255 => {}
                _ => {
                    for channel in &mut pixel[..3] {
                        *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
                    }
                }
            }
        }
        self.data
    }
}

fn swap_red_blue(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sprite {
    pub image: Image,
    pub offset: (f32, f32),
    pub scale: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
}

impl Frame {
    #[must_use]
    pub fn map(&self, point: super::model::Pos) -> (f32, f32) {
        ((point.x * self.scale - self.x) as f32, (point.y * self.scale - self.y) as f32)
    }

    #[must_use]
    pub fn rect(&self, a: super::model::Pos, b: super::model::Pos) -> Option<Rect> {
        let (ax, ay) = self.map(a);
        let (bx, by) = self.map(b);
        Rect::from_ltrb(ax.min(bx), ay.min(by), ax.max(bx), ay.max(by))
    }

    #[must_use]
    pub fn pixels(&self, a: super::model::Pos, b: super::model::Pos) -> (i32, i32, i32, i32) {
        let (ax, ay) = self.map(a);
        let (bx, by) = self.map(b);
        let (x0, y0) = (ax.min(bx).floor() as i32, ay.min(by).floor() as i32);
        let (x1, y1) = (ax.max(bx).ceil() as i32, ay.max(by).ceil() as i32);
        (x0, y0, x1 - x0, y1 - y0)
    }
}

fn paint(rgb: [u8; 3], alpha: f32) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(rgb[0], rgb[1], rgb[2], (alpha.clamp(0.0, 1.0) * 255.0).round() as u8));
    paint.anti_alias = true;
    paint
}

fn line_stroke(width: f32) -> Stroke {
    Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() }
}

fn rounded(rect: Rect, radius: f32) -> Option<tiny_skia::Path> {
    let radius = radius.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    if radius <= 0.0 {
        return Some(PathBuilder::from_rect(rect));
    }
    let (left, top, right, bottom) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    let handle = radius * 0.552_284_8;
    let mut path = PathBuilder::new();
    path.move_to(left + radius, top);
    path.line_to(right - radius, top);
    path.cubic_to(right - radius + handle, top, right, top + radius - handle, right, top + radius);
    path.line_to(right, bottom - radius);
    path.cubic_to(right, bottom - radius + handle, right - radius + handle, bottom, right - radius, bottom);
    path.line_to(left + radius, bottom);
    path.cubic_to(left + radius - handle, bottom, left, bottom - radius + handle, left, bottom - radius);
    path.line_to(left, top + radius);
    path.cubic_to(left, top + radius - handle, left + radius - handle, top, left + radius, top);
    path.close();
    path.finish()
}

#[must_use]
pub fn arrow_head(from: (f32, f32), to: (f32, f32), width: f32) -> Option<[(f32, f32); 3]> {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx.hypot(dy);
    if length < f32::EPSILON {
        return None;
    }
    let (ux, uy) = (dx / length, dy / length);
    let head = (width * 4.0).min(length * 0.6);
    let half = head * 0.55;
    let base = (to.0 - ux * head, to.1 - uy * head);
    Some([to, (base.0 - uy * half, base.1 + ux * half), (base.0 + uy * half, base.1 - ux * half)])
}

pub fn draw_mark(pixmap: &mut PixmapMut<'_>, mark: &Mark, frame: &Frame) {
    let scale = frame.scale as f32;
    let color = mark.color();
    match &mark.shape {
        Shape::Pen(points) => {
            let mut path = PathBuilder::new();
            let mut mapped = points.iter().map(|point| frame.map(*point));
            let Some(first) = mapped.next() else {
                return;
            };
            path.move_to(first.0, first.1);
            let mut drawn = false;
            for (x, y) in mapped {
                path.line_to(x, y);
                drawn = true;
            }
            if !drawn {
                path.line_to(first.0 + 0.01, first.1);
            }
            if let Some(path) = path.finish() {
                pixmap.stroke_path(
                    &path,
                    &paint(color, 1.0),
                    &line_stroke(PEN_WIDTH * scale),
                    Transform::identity(),
                    None,
                );
            }
        }
        Shape::Arrow(a, b) => {
            let (from, to) = (frame.map(*a), frame.map(*b));
            let width = ARROW_WIDTH * scale;
            let Some(head) = arrow_head(from, to, width) else {
                return;
            };
            let shaft_end = (f32::midpoint(head[1].0, head[2].0), f32::midpoint(head[1].1, head[2].1));
            let mut shaft = PathBuilder::new();
            shaft.move_to(from.0, from.1);
            shaft.line_to(shaft_end.0, shaft_end.1);
            let mut tip = PathBuilder::new();
            tip.move_to(head[0].0, head[0].1);
            tip.line_to(head[1].0, head[1].1);
            tip.line_to(head[2].0, head[2].1);
            tip.close();
            let paint = paint(color, 1.0);
            if let Some(path) = shaft.finish() {
                pixmap.stroke_path(&path, &paint, &line_stroke(width), Transform::identity(), None);
            }
            if let Some(path) = tip.finish() {
                pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
                let edge = Stroke { width: width * 0.5, line_join: LineJoin::Round, ..Stroke::default() };
                pixmap.stroke_path(&path, &paint, &edge, Transform::identity(), None);
            }
        }
        Shape::Rect(a, b) => {
            if let Some(path) = frame.rect(*a, *b).and_then(|rect| rounded(rect, MARK_RADIUS * scale)) {
                let stroke = Stroke { width: RECT_WIDTH * scale, line_join: LineJoin::Round, ..Stroke::default() };
                pixmap.stroke_path(&path, &paint(color, 1.0), &stroke, Transform::identity(), None);
            }
        }
        Shape::Highlight(a, b) => {
            if let Some(path) = frame.rect(*a, *b).and_then(|rect| rounded(rect, MARK_RADIUS * scale)) {
                let mut paint = paint(color, HIGHLIGHT_ALPHA);
                paint.blend_mode = BlendMode::SourceOver;
                pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
            }
        }
        Shape::Blur(..) => {}
        Shape::Text { at, .. } | Shape::Step { at, .. } => {
            let Some(sprite) = mark.sprite.as_deref() else {
                return;
            };
            let Some(source) =
                tiny_skia::PixmapRef::from_bytes(&sprite.image.data, sprite.image.width, sprite.image.height)
            else {
                return;
            };
            let (x, y) = frame.map(*at);
            let factor = (frame.scale / sprite.scale) as f32;
            let transform = Transform::from_row(
                factor,
                0.0,
                0.0,
                factor,
                x + sprite.offset.0 * factor,
                y + sprite.offset.1 * factor,
            );
            let quality = if (factor - 1.0).abs() < f32::EPSILON {
                tiny_skia::FilterQuality::Nearest
            } else {
                tiny_skia::FilterQuality::Bilinear
            };
            let paint = PixmapPaint { quality, ..PixmapPaint::default() };
            pixmap.draw_pixmap(0, 0, source, &paint, transform, None);
        }
    }
}

#[must_use]
pub fn obscured(source: &Image, region: (i32, i32, i32, i32), scale: f64) -> Option<Image> {
    let mut patch = source.crop(region)?;
    let block = ((BLUR_BLOCK * scale).round() as u32).max(4);
    let (w, h) = (patch.width, patch.height);
    let (sw, sh) = (w.div_ceil(block).max(1), h.div_ceil(block).max(1));
    let mut small = vec![0u32; (sw * sh * 4) as usize];
    let mut counts = vec![0u32; (sw * sh) as usize];
    for y in 0..h {
        for x in 0..w {
            let cell = ((y / block) * sw + x / block) as usize;
            let index = ((y * w + x) * 4) as usize;
            for channel in 0..4 {
                small[cell * 4 + channel] += u32::from(patch.data[index + channel]);
            }
            counts[cell] += 1;
        }
    }
    let averaged: Vec<f32> = small
        .chunks_exact(4)
        .zip(&counts)
        .flat_map(|(sum, count)| sum.iter().map(move |value| *value as f32 / (*count).max(1) as f32))
        .collect();
    let sample = |cx: i64, cy: i64, channel: usize| -> f32 {
        let cx = cx.clamp(0, i64::from(sw) - 1) as usize;
        let cy = cy.clamp(0, i64::from(sh) - 1) as usize;
        averaged[(cy * sw as usize + cx) * 4 + channel]
    };
    let half = block as f32 / 2.0;
    for y in 0..h {
        let fy = (y as f32 + 0.5 - half) / block as f32;
        let (y0, ty) = (fy.floor() as i64, fy - fy.floor());
        for x in 0..w {
            let fx = (x as f32 + 0.5 - half) / block as f32;
            let (x0, tx) = (fx.floor() as i64, fx - fx.floor());
            let index = ((y * w + x) * 4) as usize;
            for channel in 0..4 {
                let top = sample(x0, y0, channel) * (1.0 - tx) + sample(x0 + 1, y0, channel) * tx;
                let bottom = sample(x0, y0 + 1, channel) * (1.0 - tx) + sample(x0 + 1, y0 + 1, channel) * tx;
                patch.data[index + channel] = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    glide_paint::box_blur_rgba(&mut patch.data, w as i32, h as i32, (block / 2) as i32);
    Some(patch)
}

pub fn apply_marks(image: &mut Image, marks: &[Mark], frame: &Frame) {
    let source = image.clone();
    for mark in marks {
        if let Shape::Blur(a, b) = mark.shape {
            let (x, y, w, h) = frame.pixels(a, b);
            if let Some(patch) = obscured(&source, (x, y, w, h), frame.scale) {
                image.draw_replace(&patch, x.max(0), y.max(0));
            }
        }
    }
    let Some(mut pixmap) = image.pixmap() else {
        return;
    };
    for mark in marks {
        draw_mark(&mut pixmap, mark, frame);
    }
}

impl Image {
    fn draw_replace(&mut self, src: &Self, x: i32, y: i32) {
        let stride = self.width as usize * 4;
        for row in 0..src.height as usize {
            let ty = y as usize + row;
            if ty >= self.height as usize {
                break;
            }
            let columns = (src.width as usize).min(self.width as usize - (x as usize).min(self.width as usize));
            let from = row * src.width as usize * 4;
            let to = ty * stride + x as usize * 4;
            self.data[to..to + columns * 4].copy_from_slice(&src.data[from..from + columns * 4]);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub radius: f32,
    pub scale: f64,
}

const SHADOW_LAYERS: [(f32, f32, f32); 3] = [(1.0, 3.0, 0.10), (6.0, 18.0, 0.09), (18.0, 48.0, 0.13)];
const SHADOW_RING: f32 = 0.10;
const TOP_TRIM: f32 = 0.4;

#[must_use]
pub fn shadow_margins(scale: f64) -> (u32, u32) {
    let deepest = SHADOW_LAYERS.iter().map(|(_, blur, _)| *blur).fold(0.0_f32, f32::max);
    let margin = glide_paint::shadow_fade_margin((deepest * scale as f32).ceil() as i32).max(0) as u32;
    (margin, (margin as f32 * (1.0 - TOP_TRIM)).round() as u32)
}

#[must_use]
pub fn with_shadow(content: &Image, shadow: Shadow) -> Option<Image> {
    let scale = shadow.scale as f32;
    let (margin, top) = shadow_margins(shadow.scale);
    let (width, height) = (content.width + margin * 2, content.height + margin * 2);
    let mut canvas = Image::new(width, height)?;
    let layers: Vec<glide_paint::ShadowLayerSpec> = SHADOW_LAYERS
        .iter()
        .map(|(dy, blur, alpha)| glide_paint::ShadowLayerSpec {
            dy: dy * scale,
            blur: blur * scale,
            color: glide_theme::Color([0.0, 0.0, 0.0, *alpha]),
        })
        .collect();
    glide_paint::draw_outset_shadow_and_border(
        &mut canvas.data,
        width as i32,
        height as i32,
        content.width as f32,
        content.height as f32,
        &layers,
        scale.max(1.0).round(),
        glide_theme::Color([0.0, 0.0, 0.0, SHADOW_RING]),
        shadow.radius,
        Some(shadow.radius),
    );
    canvas.draw(content, margin as i32, margin as i32);
    canvas.crop(((0), (margin - top) as i32, width as i32, (height - (margin - top)) as i32))
}

#[must_use]
pub fn rounded_corners(mut content: Image, radius: f32) -> Image {
    if radius <= 0.0 {
        return content;
    }
    let Some(mask_path) =
        Rect::from_xywh(0.0, 0.0, content.width as f32, content.height as f32).and_then(|rect| rounded(rect, radius))
    else {
        return content;
    };
    let Some(mut mask) = Pixmap::new(content.width, content.height) else {
        return content;
    };
    mask.fill_path(&mask_path, &paint([255, 255, 255], 1.0), FillRule::Winding, Transform::identity(), None);
    for (pixel, cover) in content.data.chunks_exact_mut(4).zip(mask.data().chunks_exact(4)) {
        let keep = u16::from(cover[3]);
        if keep < 255 {
            for channel in pixel.iter_mut() {
                *channel = ((u16::from(*channel) * keep + 127) / 255) as u8;
            }
        }
    }
    content
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shot::model::{Mark, Shape};
    use smithay::utils::Point;

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Image {
        let mut image = Image::new(width, height).expect("image");
        for pixel in image.data.chunks_exact_mut(4) {
            pixel.copy_from_slice(&rgba);
        }
        image
    }

    fn at(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * image.width + x) * 4) as usize;
        image.data[index..index + 4].try_into().expect("pixel")
    }

    #[test]
    fn readback_bytes_become_rgba_and_back() {
        let image = Image::from_bgra(1, 1, vec![10, 20, 30, 255]).expect("image");
        assert_eq!(image.data, [30, 20, 10, 255]);
        assert_eq!(image.to_bgra(), [10, 20, 30, 255]);
        assert!(Image::from_bgra(2, 2, vec![0; 4]).is_none(), "too few bytes");
    }

    #[test]
    fn crops_are_clamped_to_the_image() {
        let mut image = solid(4, 4, [0, 0, 0, 255]);
        image.data[(4 + 2) * 4] = 99;
        let cropped = image.crop((2, 1, 10, 10)).expect("crop");
        assert_eq!((cropped.width, cropped.height), (2, 3));
        assert_eq!(at(&cropped, 0, 0)[0], 99);
        assert!(image.crop((9, 9, 2, 2)).is_none());
    }

    #[test]
    fn half_transparent_shadow_pixels_reach_the_png_as_straight_colour() {
        let mut image = solid(1, 1, [64, 32, 0, 128]);
        image.data.extend_from_slice(&[0, 0, 0, 0]);
        image.width = 2;
        let straight = image.into_straight();
        assert_eq!(&straight[..4], &[128, 64, 0, 128]);
        assert_eq!(&straight[4..], &[0, 0, 0, 0]);
    }

    #[test]
    fn the_shadow_pads_the_capture_and_fades_out_before_the_edges() {
        let content = solid(100, 60, [255, 255, 255, 255]);
        let shadowed = with_shadow(&content, Shadow { radius: 0.0, scale: 1.0 }).expect("shadow");
        let (margin, top) = shadow_margins(1.0);
        assert_eq!((shadowed.width, shadowed.height), (100 + margin * 2, 60 + margin + top));
        assert_eq!(at(&shadowed, margin + 50, top + 30), [255, 255, 255, 255], "the capture itself is untouched");
        assert_eq!(at(&shadowed, 0, 0)[3], 0, "the corners are clear");
        assert!(at(&shadowed, margin + 50, shadowed.height - 1)[3] <= 2, "the shadow is gone before the edge");
        let below = at(&shadowed, margin + 50, top + 60 + 12)[3];
        let above = at(&shadowed, margin + 50, top - 12)[3];
        assert!(below > above, "the light comes from above: {below} vs {above}");
        assert!(below < 80, "barely there: {below}");
    }

    #[test]
    fn rounded_corners_clear_the_corner_but_not_the_middle() {
        let rounded = rounded_corners(solid(40, 40, [200, 100, 50, 255]), 12.0);
        assert_eq!(at(&rounded, 0, 0)[3], 0);
        assert_eq!(at(&rounded, 20, 20), [200, 100, 50, 255]);
        assert_eq!(at(&rounded, 20, 0)[3], 255);
    }

    #[test]
    fn obscuring_hides_detail_but_keeps_the_average() {
        let mut image = solid(64, 64, [0, 0, 0, 255]);
        for y in 0..64 {
            for x in 0..64 {
                if (x + y) % 2 == 0 {
                    let index = ((y * 64 + x) * 4) as usize;
                    image.data[index..index + 3].copy_from_slice(&[255, 255, 255]);
                }
            }
        }
        let patch = obscured(&image, (8, 8, 40, 40), 1.0).expect("patch");
        assert_eq!((patch.width, patch.height), (40, 40));
        for pixel in patch.data.chunks_exact(4) {
            assert!((100..=155).contains(&pixel[0]), "{pixel:?}");
            assert_eq!(pixel[3], 255);
        }
    }

    #[test]
    fn marks_land_where_the_frame_maps_them() {
        let mut image = solid(200, 100, [0, 0, 0, 255]);
        let frame = Frame { x: 100.0, y: 50.0, scale: 2.0 };
        let marks = [
            Mark::new(Shape::Rect(Point::from((60.0, 30.0)), Point::from((100.0, 50.0))), 0),
            Mark::new(Shape::Highlight(Point::from((110.0, 30.0)), Point::from((140.0, 70.0))), 1),
        ];
        apply_marks(&mut image, &marks, &frame);
        assert!(at(&image, 20, 30)[0] > 200, "the rect's left edge sits at x 20");
        assert_eq!(at(&image, 60, 40), [0, 0, 0, 255], "the rect stays hollow");
        let highlight = at(&image, 150, 60);
        assert!(highlight[0] > 60 && highlight[0] < 140 && highlight[3] == 255, "{highlight:?}");
    }

    #[test]
    fn blur_marks_change_only_their_own_region() {
        let mut image = solid(100, 100, [0, 0, 0, 255]);
        for x in 0..100 {
            let index = ((50 * 100 + x) * 4) as usize;
            image.data[index..index + 3].copy_from_slice(&[255, 255, 255]);
        }
        let frame = Frame { x: 0.0, y: 0.0, scale: 1.0 };
        let marks = [Mark::new(Shape::Blur(Point::from((10.0, 30.0)), Point::from((50.0, 70.0))), 0)];
        apply_marks(&mut image, &marks, &frame);
        assert!(at(&image, 30, 50)[0] < 200, "the line is smeared inside");
        assert_eq!(at(&image, 80, 50), [255, 255, 255, 255], "and sharp outside");
        assert_eq!(at(&image, 80, 10), [0, 0, 0, 255]);
    }

    #[test]
    fn the_arrow_head_points_at_the_end() {
        let head = arrow_head((0.0, 0.0), (100.0, 0.0), 4.0).expect("head");
        assert_eq!(head[0], (100.0, 0.0));
        assert!(head[1].0 < 100.0 && head[2].0 < 100.0);
        assert!((head[1].1 + head[2].1).abs() < 1e-4, "symmetric about the shaft");
        assert!(arrow_head((5.0, 5.0), (5.0, 5.0), 4.0).is_none());
    }
}

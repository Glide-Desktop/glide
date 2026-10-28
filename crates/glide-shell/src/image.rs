use tiny_skia::{PixmapMut, PixmapPaint, Transform};

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

    pub fn draw_replace(&mut self, src: &Self, x: i32, y: i32) {
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

    pub fn png(self) -> Result<Vec<u8>, png::EncodingError> {
        let (width, height) = (self.width, self.height);
        encode_png(width, height, &self.into_straight())
    }
}

fn swap_red_blue(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}

pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, png::EncodingError> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(rgba)?;
        writer.finish()?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readback_bytes_become_rgba_and_back() {
        let image = Image::from_bgra(1, 1, vec![10, 20, 30, 255]).expect("image");
        assert_eq!(image.data, [30, 20, 10, 255]);
        assert_eq!(image.to_bgra(), [10, 20, 30, 255]);
        assert!(Image::from_bgra(2, 2, vec![0; 4]).is_none(), "too few bytes");
    }

    #[test]
    fn encode_png_round_trips_exact_pixel_data() {
        let (w, h) = (3_u32, 2_u32);
        let rgba: Vec<u8> = (0..(w * h * 4)).map(|i| u8::try_from((i * 17) % 256).expect("fits in a u8")).collect();

        let png = encode_png(w, h, &rgba).expect("encode_png must succeed");

        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().expect("must be a valid PNG");
        assert_eq!(reader.info().width, w);
        assert_eq!(reader.info().height, h);
        assert_eq!(reader.info().color_type, png::ColorType::Rgba);

        let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
        let frame = reader.next_frame(&mut buf).expect("must decode a frame");
        assert_eq!(&buf[..frame.buffer_size()], rgba.as_slice());
    }
}

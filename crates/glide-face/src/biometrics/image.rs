use std::path::Path;

#[derive(Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: usize,
    pub height: usize,
    pub pix: Vec<u8>,
}

impl std::fmt::Debug for RgbaImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RgbaImage({}x{})", self.width, self.height)
    }
}

impl RgbaImage {
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, pix: vec![0; width * height * 4] }
    }

    #[must_use]
    pub fn from_raw(width: usize, height: usize, pix: Vec<u8>) -> Self {
        assert_eq!(pix.len(), width * height * 4, "RGBA buffer size mismatch");
        Self { width, height, pix }
    }

    #[inline]
    #[must_use]
    pub fn stride(&self) -> usize {
        self.width * 4
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, image::ImageError> {
        let img = image::open(path)?.to_rgba8();
        let (w, h) = img.dimensions();
        Ok(Self::from_raw(w as usize, h as usize, img.into_raw()))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, image::ImageError> {
        let img = image::load_from_memory(bytes)?.to_rgba8();
        let (w, h) = img.dimensions();
        Ok(Self::from_raw(w as usize, h as usize, img.into_raw()))
    }

    pub fn save_png(&self, path: impl AsRef<Path>) -> Result<(), image::ImageError> {
        image::save_buffer(path, &self.pix, self.width as u32, self.height as u32, image::ExtendedColorType::Rgba8)
    }

    #[inline]
    #[must_use]
    pub fn lum(&self, x: usize, y: usize) -> f64 {
        let i = y * self.stride() + x * 4;
        0.299 * f64::from(self.pix[i]) + 0.587 * f64::from(self.pix[i + 1]) + 0.114 * f64::from(self.pix[i + 2])
    }

    #[must_use]
    pub fn mirrored(&self) -> Self {
        let mut out = Self::new(self.width, self.height);
        let s = self.stride();
        for y in 0..self.height {
            let row = &self.pix[y * s..(y + 1) * s];
            let dst = &mut out.pix[y * s..(y + 1) * s];
            for x in 0..self.width {
                let sx = self.width - 1 - x;
                dst[x * 4..x * 4 + 4].copy_from_slice(&row[sx * 4..sx * 4 + 4]);
            }
        }
        out
    }
}

use std::path::{Path, PathBuf};

use image::{DynamicImage, Rgba, RgbaImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slider {
    Exposure,
    Contrast,
    Shadows,
    Saturation,
    Warmth,
    Vignette,
}

impl Slider {
    pub const LIGHT: [Self; 3] = [Self::Exposure, Self::Contrast, Self::Shadows];
    pub const COLOR: [Self; 2] = [Self::Saturation, Self::Warmth];
    pub const EFFECTS: [Self; 1] = [Self::Vignette];

    #[must_use]
    pub const fn min(self) -> i32 {
        if matches!(self, Self::Vignette) { 0 } else { -100 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Filter {
    #[default]
    None,
    Vivid,
    Warm,
    Cool,
    Mono,
    Noir,
    Film,
    Drama,
    Fade,
}

impl Filter {
    pub const ALL: [Self; 9] =
        [Self::None, Self::Vivid, Self::Warm, Self::Cool, Self::Mono, Self::Noir, Self::Film, Self::Drama, Self::Fade];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Ratio {
    #[default]
    Free,
    Original,
    Square,
    R3x2,
    R4x3,
    R5x4,
    R16x9,
    R9x16,
}

impl Ratio {
    pub const ALL: [Self; 8] =
        [Self::Free, Self::Original, Self::Square, Self::R3x2, Self::R4x3, Self::R5x4, Self::R16x9, Self::R9x16];

    #[must_use]
    pub fn value(self, original: f32, portrait: bool) -> Option<f32> {
        let ratio = match self {
            Self::Free => return None,
            Self::Original => original,
            Self::Square => 1.0,
            Self::R3x2 => 1.5,
            Self::R4x3 => 4.0 / 3.0,
            Self::R5x4 => 1.25,
            Self::R16x9 => 16.0 / 9.0,
            Self::R9x16 => return Some(9.0 / 16.0),
        };
        Some(if portrait { 1.0 / ratio } else { ratio })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crop {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Default for Crop {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }
    }
}

impl Crop {
    #[must_use]
    pub fn fitted(ratio: f32, image_ratio: f32) -> Self {
        let relative = ratio / image_ratio;
        if relative >= 1.0 {
            let h = 1.0 / relative;
            Self { x: 0.0, y: (1.0 - h) / 2.0, w: 1.0, h }
        } else {
            Self { x: (1.0 - relative) / 2.0, y: 0.0, w: relative, h: 1.0 }
        }
    }

    #[must_use]
    pub fn is_full(self) -> bool {
        self.x <= 0.0 && self.y <= 0.0 && self.w >= 1.0 && self.h >= 1.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edit {
    pub exposure: i32,
    pub contrast: i32,
    pub shadows: i32,
    pub saturation: i32,
    pub warmth: i32,
    pub vignette: i32,
    pub filter: Filter,
    pub strength: i32,
    pub turns: u8,
    pub flip_h: bool,
    pub flip_v: bool,
    pub tilt: f32,
    pub ratio: Ratio,
    pub portrait: bool,
    pub crop: Crop,
}

impl Default for Edit {
    fn default() -> Self {
        Self {
            exposure: 0,
            contrast: 0,
            shadows: 0,
            saturation: 0,
            warmth: 0,
            vignette: 0,
            filter: Filter::None,
            strength: 100,
            turns: 0,
            flip_h: false,
            flip_v: false,
            tilt: 0.0,
            ratio: Ratio::Free,
            portrait: false,
            crop: Crop::default(),
        }
    }
}

impl Edit {
    #[must_use]
    pub const fn get(&self, slider: Slider) -> i32 {
        match slider {
            Slider::Exposure => self.exposure,
            Slider::Contrast => self.contrast,
            Slider::Shadows => self.shadows,
            Slider::Saturation => self.saturation,
            Slider::Warmth => self.warmth,
            Slider::Vignette => self.vignette,
        }
    }

    pub fn set(&mut self, slider: Slider, value: i32) {
        let value = value.clamp(slider.min(), 100);
        match slider {
            Slider::Exposure => self.exposure = value,
            Slider::Contrast => self.contrast = value,
            Slider::Shadows => self.shadows = value,
            Slider::Saturation => self.saturation = value,
            Slider::Warmth => self.warmth = value,
            Slider::Vignette => self.vignette = value,
        }
    }

    #[must_use]
    pub fn auto() -> Self {
        Self { exposure: 14, contrast: 18, shadows: 22, saturation: 12, warmth: 6, ..Self::default() }
    }

    #[must_use]
    pub fn tone_changed(&self) -> bool {
        let base = Self::default();
        self.exposure != 0
            || self.contrast != 0
            || self.shadows != 0
            || self.saturation != 0
            || self.warmth != 0
            || self.vignette != 0
            || (self.filter != base.filter && self.strength > 0)
    }

    #[must_use]
    pub fn geometry_changed(&self) -> bool {
        !self.turns.is_multiple_of(4) || self.flip_h || self.flip_v || self.tilt.abs() > 0.01 || !self.crop.is_full()
    }

    #[must_use]
    pub fn is_identity(&self) -> bool {
        !self.tone_changed() && !self.geometry_changed()
    }
}

#[must_use]
pub fn geometry(image: &RgbaImage, edit: &Edit) -> RgbaImage {
    let mut out = DynamicImage::ImageRgba8(image.clone());
    out = match edit.turns % 4 {
        1 => out.rotate90(),
        2 => out.rotate180(),
        3 => out.rotate270(),
        _ => out,
    };
    if edit.flip_h {
        out = out.fliph();
    }
    if edit.flip_v {
        out = out.flipv();
    }
    let mut out = out.into_rgba8();
    if edit.tilt.abs() > 0.01 {
        out = straighten(&out, edit.tilt);
    }
    crop(&out, edit.crop)
}

#[must_use]
pub fn crop(image: &RgbaImage, crop: Crop) -> RgbaImage {
    if crop.is_full() {
        return image.clone();
    }
    let (w, h) = (image.width() as f32, image.height() as f32);
    let x = (crop.x.clamp(0.0, 1.0) * w).round() as u32;
    let y = (crop.y.clamp(0.0, 1.0) * h).round() as u32;
    let cw = ((crop.w.clamp(0.0, 1.0) * w).round() as u32).clamp(1, image.width() - x.min(image.width() - 1));
    let ch = ((crop.h.clamp(0.0, 1.0) * h).round() as u32).clamp(1, image.height() - y.min(image.height() - 1));
    image::imageops::crop_imm(image, x, y, cw, ch).to_image()
}

#[must_use]
pub fn inscribed(width: f32, height: f32, degrees: f32) -> (f32, f32) {
    let angle = degrees.abs().to_radians();
    let (sin, cos) = angle.sin_cos();
    let scale = (width / (width * cos + height * sin)).min(height / (width * sin + height * cos));
    (width * scale, height * scale)
}

#[must_use]
pub fn straighten(image: &RgbaImage, degrees: f32) -> RgbaImage {
    let (width, height) = (image.width() as f32, image.height() as f32);
    let (inner_w, inner_h) = inscribed(width, height, degrees);
    let (out_w, out_h) = (inner_w.floor().max(1.0) as u32, inner_h.floor().max(1.0) as u32);
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (cx, cy) = (width / 2.0, height / 2.0);
    RgbaImage::from_fn(out_w, out_h, |col, row| {
        let dx = col as f32 + 0.5 - out_w as f32 / 2.0;
        let dy = row as f32 + 0.5 - out_h as f32 / 2.0;
        let sx = cos * dx + sin * dy + cx - 0.5;
        let sy = -sin * dx + cos * dy + cy - 0.5;
        bilinear(image, sx, sy)
    })
}

fn bilinear(image: &RgbaImage, left: f32, top: f32) -> Rgba<u8> {
    let max_x = image.width() as i64 - 1;
    let max_y = image.height() as i64 - 1;
    let (col, row) = (left.floor(), top.floor());
    let (fx, fy) = (left - col, top - row);
    let at = |xx: i64, yy: i64| image.get_pixel(xx.clamp(0, max_x) as u32, yy.clamp(0, max_y) as u32).0;
    let (col, row) = (col as i64, row as i64);
    let corners = [at(col, row), at(col + 1, row), at(col, row + 1), at(col + 1, row + 1)];
    let mut out = [0u8; 4];
    for (channel, value) in out.iter_mut().enumerate() {
        let [nw, ne, sw, se] = corners.map(|pixel| f32::from(pixel[channel]));
        let upper = nw * (1.0 - fx) + ne * fx;
        let lower = sw * (1.0 - fx) + se * fx;
        *value = (upper * (1.0 - fy) + lower * fy).round().clamp(0.0, 255.0) as u8;
    }
    Rgba(out)
}

fn luma(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn filtered(filter: Filter, [r, g, b]: [f32; 3]) -> [f32; 3] {
    let sat = |[r, g, b]: [f32; 3], k: f32| {
        let l = luma(r, g, b);
        [l + (r - l) * k, l + (g - l) * k, l + (b - l) * k]
    };
    let contrast = |[r, g, b]: [f32; 3], k: f32| [(r - 0.5) * k + 0.5, (g - 0.5) * k + 0.5, (b - 0.5) * k + 0.5];
    let bright = |[r, g, b]: [f32; 3], k: f32| [r * k, g * k, b * k];
    let sepia = |[r, g, b]: [f32; 3], k: f32| {
        let s =
            [0.393 * r + 0.769 * g + 0.189 * b, 0.349 * r + 0.686 * g + 0.168 * b, 0.272 * r + 0.534 * g + 0.131 * b];
        [r + (s[0] - r) * k, g + (s[1] - g) * k, b + (s[2] - b) * k]
    };
    let rgb = [r, g, b];
    match filter {
        Filter::None => rgb,
        Filter::Vivid => contrast(sat(rgb, 1.45), 1.08),
        Filter::Warm => sat(sepia(rgb, 0.28), 1.2),
        Filter::Cool => {
            let [r, g, b] = bright(sat(rgb, 1.1), 1.03);
            [r * 0.94, g, b * 1.08]
        }
        Filter::Mono => contrast(sat(rgb, 0.0), 1.1),
        Filter::Noir => bright(contrast(sat(rgb, 0.0), 1.45), 0.9),
        Filter::Film => bright(sat(contrast(sepia(rgb, 0.18), 0.92), 0.85), 1.04),
        Filter::Drama => bright(sat(contrast(rgb, 1.35), 0.8), 0.94),
        Filter::Fade => sat(bright(contrast(rgb, 0.78), 1.1), 0.7),
    }
}

#[must_use]
pub fn tone(image: &RgbaImage, edit: &Edit) -> RgbaImage {
    if !edit.tone_changed() {
        return image.clone();
    }
    let gain = 2f32.powf(edit.exposure as f32 / 100.0);
    let contrast = 1.0 + edit.contrast as f32 / 100.0 * 0.6;
    let shadows = edit.shadows as f32 / 100.0 * 0.35;
    let saturation = 1.0 + edit.saturation as f32 / 100.0;
    let warmth = edit.warmth as f32 / 100.0 * 0.08;
    let strength = (edit.strength as f32 / 100.0).clamp(0.0, 1.0);
    let vignette = edit.vignette as f32 / 100.0;
    let (width, height) = (image.width() as f32, image.height() as f32);
    let mut out = image.clone();
    for (col, row, pixel) in out.enumerate_pixels_mut() {
        let [red, green, blue, alpha] = pixel.0;
        let mut rgb = [f32::from(red) / 255.0, f32::from(green) / 255.0, f32::from(blue) / 255.0];
        for value in &mut rgb {
            *value *= gain;
        }
        let light = luma(rgb[0], rgb[1], rgb[2]).clamp(0.0, 1.0);
        let lift = shadows * (1.0 - light) * (1.0 - light);
        for value in &mut rgb {
            *value = (*value - 0.5) * contrast + 0.5 + lift;
        }
        let light = luma(rgb[0], rgb[1], rgb[2]);
        for value in &mut rgb {
            *value = light + (*value - light) * saturation;
        }
        rgb[0] += warmth;
        rgb[2] -= warmth;
        if edit.filter != Filter::None && strength > 0.0 {
            let target = filtered(edit.filter, rgb);
            for (value, goal) in rgb.iter_mut().zip(target) {
                *value += (goal - *value) * strength;
            }
        }
        if vignette > 0.0 {
            let across = (col as f32 + 0.5) / width - 0.5;
            let down = (row as f32 + 0.5) / height - 0.5;
            let distance = ((across * across + down * down).sqrt() / std::f32::consts::FRAC_1_SQRT_2).clamp(0.0, 1.0);
            let ramp = ((distance - 0.45) / 0.55).clamp(0.0, 1.0);
            let keep = 1.0 - vignette * 0.85 * ramp * ramp * (3.0 - 2.0 * ramp);
            for value in &mut rgb {
                *value *= keep;
            }
        }
        let byte = |value: f32| (value * 255.0).round().clamp(0.0, 255.0) as u8;
        pixel.0 = [byte(rgb[0]), byte(rgb[1]), byte(rgb[2]), alpha];
    }
    out
}

#[must_use]
pub fn apply(image: &RgbaImage, edit: &Edit) -> RgbaImage {
    tone(&geometry(image, edit), edit)
}

pub const BINS: usize = 64;

#[must_use]
pub fn histogram(image: &RgbaImage) -> [[u32; BINS]; 3] {
    let mut bins = [[0u32; BINS]; 3];
    let step = ((image.width() as usize * image.height() as usize) / 200_000).max(1);
    for pixel in image.pixels().step_by(step) {
        for channel in 0..3 {
            bins[channel][usize::from(pixel.0[channel]) * BINS / 256] += 1;
        }
    }
    bins
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Output {
    #[default]
    Jpeg,
    Png,
    Webp,
}

impl Output {
    pub const ALL: [Self; 3] = [Self::Jpeg, Self::Png, Self::Webp];

    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Jpeg => "JPEG",
            Self::Png => "PNG",
            Self::Webp => "WebP",
        }
    }

    #[must_use]
    pub fn of(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" | "jpe" | "jfif" => Some(Self::Jpeg),
            "png" => Some(Self::Png),
            "webp" => Some(Self::Webp),
            _ => None,
        }
    }
}

#[must_use]
pub fn copy_path(original: &Path, stem: &str, output: Output) -> PathBuf {
    let dir = original.parent().unwrap_or_else(|| Path::new("."));
    let mut candidate = dir.join(format!("{stem}.{}", output.extension()));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem} {n}.{}", output.extension()));
        n += 1;
    }
    candidate
}

pub fn write(image: &RgbaImage, path: &Path, output: Output) -> Result<(), String> {
    let partial = path.with_extension(format!("{}.{}.part", output.extension(), std::process::id()));
    let result = match output {
        Output::Jpeg => {
            let rgb = DynamicImage::ImageRgba8(image.clone()).into_rgb8();
            std::fs::File::create(&partial).map_err(|e| e.to_string()).and_then(|file| {
                let mut writer = std::io::BufWriter::new(file);
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 92)
                    .encode_image(&rgb)
                    .map_err(|e| e.to_string())
            })
        }
        Output::Png => image.save_with_format(&partial, image::ImageFormat::Png).map_err(|e| e.to_string()),
        Output::Webp => image.save_with_format(&partial, image::ImageFormat::WebP).map_err(|e| e.to_string()),
    };
    match result.and_then(|()| std::fs::rename(&partial, path).map_err(|e| e.to_string())) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&partial);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_edits_leave_pixels_alone() {
        let image = RgbaImage::from_fn(8, 6, |x, y| Rgba([x as u8 * 30, y as u8 * 40, 90, 255]));
        assert!(Edit::default().is_identity());
        assert_eq!(apply(&image, &Edit::default()), image);
    }

    #[test]
    fn exposure_brightens_mono_drops_colour_and_vignette_darkens_corners() {
        let image = RgbaImage::from_pixel(20, 20, Rgba([100, 60, 30, 255]));
        let bright = tone(&image, &Edit { exposure: 80, ..Edit::default() });
        assert!(bright.get_pixel(10, 10).0[0] > 100);
        let mono = tone(&image, &Edit { filter: Filter::Mono, ..Edit::default() });
        let p = mono.get_pixel(3, 3).0;
        assert!(p[0].abs_diff(p[1]) <= 1 && p[1].abs_diff(p[2]) <= 1);
        let half = tone(&image, &Edit { filter: Filter::Mono, strength: 0, ..Edit::default() });
        assert_eq!(half, image);
        let vignette = tone(&image, &Edit { vignette: 100, ..Edit::default() });
        assert!(vignette.get_pixel(0, 0).0[0] < vignette.get_pixel(10, 10).0[0]);
    }

    #[test]
    fn quarter_turns_flips_and_crops_change_the_geometry() {
        let image = RgbaImage::from_fn(40, 20, |x, _| Rgba([x as u8, 0, 0, 255]));
        let turned = geometry(&image, &Edit { turns: 1, ..Edit::default() });
        assert_eq!(turned.dimensions(), (20, 40));
        let flipped = geometry(&image, &Edit { flip_h: true, ..Edit::default() });
        assert_eq!(flipped.get_pixel(0, 0).0[0], 39);
        let cropped = geometry(&image, &Edit { crop: Crop { x: 0.5, y: 0.0, w: 0.5, h: 1.0 }, ..Edit::default() });
        assert_eq!(cropped.dimensions(), (20, 20));
        assert_eq!(cropped.get_pixel(0, 0).0[0], 20);
    }

    #[test]
    fn straightening_keeps_the_largest_upright_rectangle_inside_the_frame() {
        let (w, h) = inscribed(300.0, 200.0, 0.0);
        assert!((w - 300.0).abs() < 0.01 && (h - 200.0).abs() < 0.01);
        let (w, h) = inscribed(300.0, 200.0, 10.0);
        assert!(w < 300.0 && h < 200.0 && (w / h - 1.5).abs() < 0.01);
        let image = RgbaImage::from_pixel(60, 40, Rgba([10, 20, 30, 255]));
        let out = straighten(&image, -5.0);
        assert!(out.width() < 60 && out.get_pixel(0, 0).0 == [10, 20, 30, 255]);
    }

    #[test]
    fn ratios_fit_inside_the_image_and_flip_for_portrait() {
        let crop = Crop::fitted(1.0, 1.5);
        assert!((crop.w - 2.0 / 3.0).abs() < 1e-4 && (crop.h - 1.0).abs() < 1e-6);
        assert!((Ratio::R3x2.value(1.0, true).unwrap() - 2.0 / 3.0).abs() < 1e-6);
        assert_eq!(Ratio::Free.value(1.5, false), None);
    }

    #[test]
    fn copies_never_overwrite_and_histograms_count_samples() {
        let dir = std::env::temp_dir().join(format!("glide-photos-edit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let original = dir.join("a.heic");
        std::fs::write(dir.join("a (edited).jpg"), b"x").unwrap();
        let path = copy_path(&original, "a (edited)", Output::Jpeg);
        assert_eq!(path, dir.join("a (edited) 2.jpg"));
        let image = RgbaImage::from_pixel(4, 4, Rgba([255, 0, 128, 255]));
        write(&image, &path, Output::Jpeg).unwrap();
        assert_eq!(image::image_dimensions(&path).unwrap(), (4, 4));
        let bins = histogram(&image);
        assert_eq!(bins[0][BINS - 1], 16);
        assert_eq!(bins[1][0], 16);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

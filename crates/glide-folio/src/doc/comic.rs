use std::io::Read;
use std::path::{Path, PathBuf};

use image::RgbaImage;
use image::imageops::FilterType;

use super::{Engine, Format, Info, Link, OpenError, Opened, PageText, Size};

const POINTS_PER_PIXEL: f32 = 0.5;
const IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "webp", "gif", "bmp"];

enum Source {
    Zip(PathBuf, Vec<String>),
    Folder(Vec<PathBuf>),
    Tiff(Vec<RgbaImage>),
}

pub struct ImageEngine {
    source: Source,
    opened: Opened,
    cache: Option<(usize, RgbaImage)>,
}

#[must_use]
pub fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    let key = |s: &str| {
        let mut parts: Vec<(String, u64)> = Vec::new();
        let mut digits = String::new();
        let mut letters = String::new();
        for c in s.to_lowercase().chars() {
            if c.is_ascii_digit() {
                if !letters.is_empty() {
                    parts.push((std::mem::take(&mut letters), 0));
                }
                digits.push(c);
            } else {
                if !digits.is_empty() {
                    parts.push((String::new(), digits.parse().unwrap_or(0)));
                    digits.clear();
                }
                letters.push(c);
            }
        }
        if !digits.is_empty() {
            parts.push((String::new(), digits.parse().unwrap_or(0)));
        }
        if !letters.is_empty() {
            parts.push((letters, 0));
        }
        parts
    };
    key(a).cmp(&key(b))
}

fn is_image(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        && !name.split('/').any(|part| part.starts_with('.') || part == "__MACOSX")
}

fn tiff_pages(path: &Path) -> Result<Vec<RgbaImage>, OpenError> {
    let file = std::fs::File::open(path).map_err(|e| OpenError::Io(e.to_string()))?;
    let mut decoder = tiff::decoder::Decoder::new(std::io::BufReader::new(file)).map_err(|_| OpenError::Damaged)?;
    let mut pages = Vec::new();
    loop {
        let (w, h) = decoder.dimensions().map_err(|_| OpenError::Damaged)?;
        let colour = decoder.colortype().map_err(|_| OpenError::Damaged)?;
        let data = decoder.read_image().map_err(|_| OpenError::Damaged)?;
        let pixels: Vec<u8> = match data {
            tiff::decoder::DecodingResult::U8(bytes) => bytes,
            tiff::decoder::DecodingResult::U16(words) => words.iter().map(|w| (w >> 8) as u8).collect(),
            _ => return Err(OpenError::Damaged),
        };
        let image = match colour {
            tiff::ColorType::Gray(_) => {
                image::GrayImage::from_raw(w, h, pixels).map(|g| image::DynamicImage::ImageLuma8(g).to_rgba8())
            }
            tiff::ColorType::RGB(_) => {
                image::RgbImage::from_raw(w, h, pixels).map(|g| image::DynamicImage::ImageRgb8(g).to_rgba8())
            }
            tiff::ColorType::RGBA(_) => RgbaImage::from_raw(w, h, pixels),
            _ => None,
        };
        pages.push(image.ok_or(OpenError::Damaged)?);
        if !decoder.more_images() || decoder.next_image().is_err() {
            break;
        }
    }
    Ok(pages)
}

fn extract(path: &Path) -> Result<Vec<PathBuf>, OpenError> {
    use super::convert::{TOOL_TIMEOUT, cache_dir, cache_key, find, prune, run, scratch, touch};
    let root = cache_dir();
    let target = root.join(cache_key(path));
    if target.is_dir() {
        touch(&target);
    } else {
        let tool = find(&["bsdtar", "7z", "unrar"])
            .ok_or_else(|| OpenError::Missing { tool: "bsdtar".into(), package: "libarchive".into() })?;
        std::fs::create_dir_all(&root).map_err(|e| OpenError::Io(e.to_string()))?;
        prune(&root);
        let work = scratch(&target);
        std::fs::create_dir_all(&work).map_err(|e| OpenError::Io(e.to_string()))?;
        let mut command = std::process::Command::new(&tool);
        match tool.file_name().and_then(|n| n.to_str()) {
            Some("7z") => command.arg("x").arg("-y").arg(format!("-o{}", work.display())).arg("--").arg(path),
            Some("unrar") => command.arg("x").arg("-o+").arg("--").arg(path).arg(&work),
            _ => command.arg("-xf").arg(path).arg("-C").arg(&work),
        };
        if !run(&mut command, TOOL_TIMEOUT) {
            let _ = std::fs::remove_dir_all(&work);
            return Err(OpenError::Damaged);
        }
        if std::fs::rename(&work, &target).is_err() {
            let _ = std::fs::remove_dir_all(&work);
            if !target.is_dir() {
                return Err(OpenError::Damaged);
            }
        }
    }
    let mut found = Vec::new();
    let mut stack = vec![target];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| OpenError::Io(e.to_string()))?.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                stack.push(path);
            } else if path.to_str().is_some_and(is_image) {
                found.push(path);
            }
        }
    }
    found.sort_by(|a, b| natural(&a.to_string_lossy(), &b.to_string_lossy()));
    Ok(found)
}

impl ImageEngine {
    pub fn open(path: &Path, format: Format) -> Result<Self, OpenError> {
        let source = if format == Format::Tiff {
            Source::Tiff(tiff_pages(path)?)
        } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("cbz")) {
            let file = std::fs::File::open(path).map_err(|e| OpenError::Io(e.to_string()))?;
            let archive = zip::ZipArchive::new(file).map_err(|_| OpenError::Damaged)?;
            let mut names: Vec<String> = archive.file_names().filter(|n| is_image(n)).map(str::to_string).collect();
            names.sort_by(|a, b| natural(a, b));
            Source::Zip(path.to_path_buf(), names)
        } else {
            Source::Folder(extract(path)?)
        };
        let mut engine = Self {
            source,
            opened: Opened {
                format,
                pages: Vec::new(),
                labels: Vec::new(),
                info: Info { print: true, copy: true, ..Info::default() },
                outline: Vec::new(),
                fields: Vec::new(),
                notes: Vec::new(),
                bookmarks: Vec::new(),
                broken: Vec::new(),
                reflow: None,
                view_only: None,
                right_to_left: false,
            },
            cache: None,
        };
        let count = engine.count();
        if count == 0 {
            return Err(OpenError::Damaged);
        }
        for index in 0..count {
            let size = engine.dimensions(index);
            if size.is_none() {
                engine.opened.broken.push(index);
            }
            let (w, h) = size.unwrap_or((1240, 1754));
            engine.opened.pages.push(Size { w: w as f32 * POINTS_PER_PIXEL, h: h as f32 * POINTS_PER_PIXEL });
            engine.opened.labels.push((index + 1).to_string());
        }
        Ok(engine)
    }

    fn count(&self) -> usize {
        match &self.source {
            Source::Zip(_, names) => names.len(),
            Source::Folder(files) => files.len(),
            Source::Tiff(pages) => pages.len(),
        }
    }

    fn bytes(&self, index: usize) -> Option<Vec<u8>> {
        match &self.source {
            Source::Zip(path, names) => {
                let file = std::fs::File::open(path).ok()?;
                let mut archive = zip::ZipArchive::new(file).ok()?;
                let mut entry = archive.by_name(names.get(index)?).ok()?;
                let mut out = Vec::new();
                entry.read_to_end(&mut out).ok()?;
                Some(out)
            }
            Source::Folder(files) => std::fs::read(files.get(index)?).ok(),
            Source::Tiff(_) => None,
        }
    }

    fn dimensions(&self, index: usize) -> Option<(u32, u32)> {
        if let Source::Tiff(pages) = &self.source {
            return pages.get(index).map(RgbaImage::dimensions);
        }
        let bytes = self.bytes(index)?;
        image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?.into_dimensions().ok()
    }

    fn decode(&mut self, index: usize) -> Option<RgbaImage> {
        if let Some((at, image)) = &self.cache
            && *at == index
        {
            return Some(image.clone());
        }
        let image = if let Source::Tiff(pages) = &self.source {
            pages.get(index)?.clone()
        } else {
            image::load_from_memory(&self.bytes(index)?).ok()?.to_rgba8()
        };
        self.cache = Some((index, image.clone()));
        Some(image)
    }
}

impl Engine for ImageEngine {
    fn opened(&self) -> &Opened {
        &self.opened
    }

    fn render(&mut self, page: usize, scale: f32) -> Option<RgbaImage> {
        let size = *self.opened.pages.get(page)?;
        let image = self.decode(page)?;
        let w = (size.w * scale).round().max(1.0) as u32;
        let h = (size.h * scale).round().max(1.0) as u32;
        if (w, h) == image.dimensions() {
            return Some(image);
        }
        Some(image::imageops::resize(&image, w, h, FilterType::Triangle))
    }

    fn text(&mut self, _: usize) -> PageText {
        PageText::default()
    }

    fn links(&mut self, _: usize) -> Vec<Link> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn pages_sort_naturally() {
        let mut names = vec!["p10.png", "p2.png", "p1.png"];
        names.sort_by(|a, b| natural(a, b));
        assert_eq!(names, ["p1.png", "p2.png", "p10.png"]);
    }

    #[test]
    fn a_cbz_opens_its_images_as_pages() {
        let dir = std::env::temp_dir().join(format!("glide-folio-cbz-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("book.cbz");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (name, shade) in [("10.png", 10u8), ("2.png", 200u8), ("notes.txt", 0)] {
            writer.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            if shade > 0 {
                let mut bytes = Vec::new();
                image::RgbaImage::from_pixel(40, 60, image::Rgba([shade, shade, shade, 255]))
                    .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
                    .unwrap();
                writer.write_all(&bytes).unwrap();
            } else {
                writer.write_all(b"x").unwrap();
            }
        }
        writer.finish().unwrap();
        let mut engine = ImageEngine::open(&path, Format::Comic).expect("opens");
        assert_eq!(engine.opened().pages.len(), 2);
        assert_eq!(engine.opened().pages[0], Size { w: 20.0, h: 30.0 });
        let first = engine.render(0, 2.0).unwrap();
        assert_eq!(first.dimensions(), (40, 60));
        assert_eq!(first.get_pixel(0, 0).0[0], 200);
        let _ = std::fs::remove_dir_all(dir);
    }
}

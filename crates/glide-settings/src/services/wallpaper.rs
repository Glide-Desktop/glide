use std::path::{Path, PathBuf};

pub(crate) const THUMB_WIDTH: u32 = 320;
pub(crate) const MAX_PER_SOURCE: usize = 256;
pub(crate) const MAX_PER_ROOT: usize = 64;
pub(crate) const SOLID_SIZE: u32 = 64;

static BUILTIN: &[u8] = include_bytes!("../../../glide-paint/assets/wallpaper.png");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Builtin,
    Yours,
    System,
}

#[derive(Debug, Clone)]
pub struct Thumbnail {
    pub path: PathBuf,
    pub handle: iced::widget::image::Handle,
    pub source: Source,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Gallery {
    pub builtin: Option<Thumbnail>,
    pub items: Vec<Thumbnail>,
}

impl Gallery {
    #[must_use]
    pub fn from(&self, source: Source) -> Vec<&Thumbnail> {
        self.items.iter().filter(|thumb| thumb.source == source).collect()
    }
}

#[must_use]
pub(crate) fn is_screenshot(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        let name = name.to_string_lossy().to_lowercase();
        ["screenshot", "screen shot", "знімок", "снимок", "grim"].iter().any(|marker| name.contains(marker))
    })
}

#[must_use]
pub(crate) fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "jfif" | "png" | "webp"))
}

use glide_config::home;

fn user_pictures_dir(home: &Path) -> PathBuf {
    if let Some(val) = std::env::var_os("XDG_PICTURES_DIR") {
        let p = PathBuf::from(val);
        if p.is_absolute() {
            return p;
        }
    }
    let config_dirs = home.join(".config/user-dirs.dirs");
    if let Ok(content) = std::fs::read_to_string(config_dirs) {
        for line in content.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("XDG_PICTURES_DIR=") {
                let value = rest.trim().trim_matches('"');
                if let Some(rel) = value.strip_prefix("$HOME/") {
                    return home.join(rel);
                } else if value.starts_with('/') {
                    return PathBuf::from(value);
                }
            }
        }
    }
    home.join("Pictures")
}

#[must_use]
pub(crate) fn user_folder() -> Option<PathBuf> {
    let home = home()?;
    let pictures = user_pictures_dir(&home);
    let dedicated = pictures.join("Wallpapers");
    if dedicated.is_dir() {
        return Some(dedicated);
    }
    let fallback = home.join("Pictures/Wallpapers");
    if fallback.is_dir() {
        return Some(fallback);
    }
    Some(dedicated)
}

#[must_use]
pub(crate) fn user_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let Some(home) = home() else {
        return roots;
    };
    let pictures = user_pictures_dir(&home);
    let mut add = |p: PathBuf| {
        if !roots.contains(&p) {
            roots.push(p);
        }
    };

    add(pictures.join("Wallpapers"));
    add(pictures.join("wallpapers"));
    add(pictures.join("Backgrounds"));
    add(pictures.join("backgrounds"));

    if pictures != home.join("Pictures") {
        add(home.join("Pictures/Wallpapers"));
        add(home.join("Pictures/wallpapers"));
        add(home.join("Pictures/Backgrounds"));
        add(home.join("Pictures/backgrounds"));
    }

    let data_home = super::privacy::data_home().unwrap_or_else(|| home.join(".local/share"));
    add(data_home.join("wallpapers"));
    add(data_home.join("backgrounds"));
    add(data_home.join("plasma/wallpapers"));
    add(data_home.join("xfce4/backdrops"));
    add(data_home.join("lxqt/wallpapers"));
    add(data_home.join("flatpak/exports/share/wallpapers"));
    add(data_home.join("flatpak/exports/share/backgrounds"));

    add(home.join(".local/share/wallpapers"));
    add(home.join(".local/share/backgrounds"));
    add(home.join(".wallpapers"));
    add(home.join(".backgrounds"));

    add(pictures);
    add(home.join("Pictures"));

    roots
}

#[must_use]
pub(crate) fn system_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut add = |p: PathBuf| {
        if !roots.contains(&p) {
            roots.push(p);
        }
    };

    let subdirs = [
        "backgrounds",
        "wallpapers",
        "plasma/wallpapers",
        "xfce4/backdrops",
        "lxqt/wallpapers",
        "lxde/wallpapers",
        "desktop-base",
        "images/desktop-base",
        "pixmaps/backgrounds",
    ];

    let xdg_dirs = std::env::var_os("XDG_DATA_DIRS")
        .and_then(|val| val.into_string().ok())
        .unwrap_or_else(|| String::from("/usr/local/share:/usr/share"));

    for base in xdg_dirs.split(':').filter(|s| !s.is_empty()) {
        let base_path = Path::new(base);
        for sub in subdirs {
            add(base_path.join(sub));
        }
    }

    let extra = [
        "/usr/share/backgrounds",
        "/usr/share/wallpapers",
        "/usr/share/plasma/wallpapers",
        "/usr/share/xfce4/backdrops",
        "/usr/share/lxqt/wallpapers",
        "/usr/share/lxde/wallpapers",
        "/usr/share/desktop-base",
        "/usr/share/images/desktop-base",
        "/usr/share/images",
        "/usr/share/pixmaps/backgrounds",
        "/usr/local/share/backgrounds",
        "/usr/local/share/wallpapers",
        "/usr/local/share/plasma/wallpapers",
        "/usr/local/share/xfce4/backdrops",
        "/usr/local/share/lxqt/wallpapers",
        "/opt/share/backgrounds",
        "/opt/share/wallpapers",
        "/run/current-system/sw/share/backgrounds",
        "/run/current-system/sw/share/wallpapers",
        "/var/lib/flatpak/exports/share/backgrounds",
        "/var/lib/flatpak/exports/share/wallpapers",
        "/snap/current/share/backgrounds",
    ];

    for path in extra {
        add(PathBuf::from(path));
    }

    roots
}

fn resolution_score(path: &Path) -> u64 {
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        && let Some((w, h)) = stem.split_once('x')
        && let (Ok(w), Ok(h)) = (w.parse::<u64>(), h.parse::<u64>())
    {
        return w.saturating_mul(h);
    }
    path.metadata().map_or(0, |m| m.len())
}

fn best_image_in(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut images: Vec<PathBuf> =
        entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| is_image(p) && !is_screenshot(p)).collect();
    images.sort_by_key(|p| resolution_score(p));
    images.pop()
}

fn kde_package_images(dir: &Path) -> Option<Vec<PathBuf>> {
    let images_dir = dir.join("contents/images");
    if !images_dir.is_dir() {
        return None;
    }
    let mut results = Vec::new();
    if let Some(best) = best_image_in(&images_dir) {
        results.push(best);
    }
    let dark_dir = dir.join("contents/images_dark");
    if dark_dir.is_dir()
        && let Some(best) = best_image_in(&dark_dir)
    {
        results.push(best);
    }
    if results.is_empty() { None } else { Some(results) }
}

fn walk(dir: &Path, depth: usize, limit: usize, found: &mut Vec<PathBuf>) {
    if found.len() >= limit {
        return;
    }
    if let Some(package_images) = kde_package_images(dir) {
        for img in package_images {
            if !found.contains(&img) && solid_color(&img).is_none() {
                found.push(img);
                if found.len() >= limit {
                    return;
                }
            }
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if found.len() >= limit {
            return;
        }
        if is_screenshot(&path) {
            continue;
        }
        if path.is_dir() {
            if let Some(package_images) = kde_package_images(&path) {
                for img in package_images {
                    if !found.contains(&img) && solid_color(&img).is_none() {
                        found.push(img);
                        if found.len() >= limit {
                            return;
                        }
                    }
                }
            } else if depth > 0 && !path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')) {
                walk(&path, depth - 1, limit, found);
            }
        } else if is_image(&path) && !found.contains(&path) && solid_color(&path).is_none() {
            found.push(path);
        }
    }
}

fn collect(roots: &[PathBuf], skip: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let home = home();
    for root in roots {
        if found.len() >= MAX_PER_SOURCE {
            break;
        }
        let is_general_pictures =
            home.as_ref().is_some_and(|h| root == &h.join("Pictures") || root == &user_pictures_dir(h));
        let depth = if is_general_pictures { 1 } else { 4 };
        let per_root_limit = found.len() + MAX_PER_ROOT;
        let limit = per_root_limit.min(MAX_PER_SOURCE);
        walk(root, depth, limit, &mut found);
    }
    found.retain(|path| !skip.contains(path));
    found
}

#[must_use]
pub fn discover(current: Option<&Path>) -> Vec<(PathBuf, Source)> {
    let mut yours = collect(&user_roots(), &[]);
    let system = collect(&system_roots(), &yours);
    if let Some(current) = current.filter(|path| path.is_file() && solid_color(path).is_none())
        && !yours.iter().chain(&system).any(|path| path == current)
    {
        yours.insert(0, current.to_path_buf());
    }
    yours
        .into_iter()
        .map(|path| (path, Source::Yours))
        .chain(system.into_iter().map(|path| (path, Source::System)))
        .collect()
}

fn thumbnail_of(image: &image::DynamicImage, path: PathBuf, source: Source) -> Option<Thumbnail> {
    let height = (u64::from(THUMB_WIDTH) * u64::from(image.height()) / u64::from(image.width().max(1)))
        .clamp(1, u64::from(THUMB_WIDTH));
    let height = u32::try_from(height).ok()?;
    let small = image.resize_to_fill(THUMB_WIDTH, height, image::imageops::FilterType::Triangle).into_rgba8();
    let (width, height) = small.dimensions();
    Some(Thumbnail {
        path,
        handle: iced::widget::image::Handle::from_rgba(width, height, small.into_raw()),
        source,
        width: image.width(),
        height: image.height(),
    })
}

#[must_use]
pub(crate) fn thumbnail_from(path: &Path, source: Source) -> Option<Thumbnail> {
    thumbnail_of(&decode_limited(path)?, path.to_path_buf(), source)
}

fn decode_limited(path: &Path) -> Option<image::DynamicImage> {
    let mut reader = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().ok()
}

#[must_use]
pub fn thumbnail(path: &Path) -> Option<Thumbnail> {
    thumbnail_from(path, Source::Yours)
}

#[must_use]
pub fn builtin() -> Option<Thumbnail> {
    thumbnail_of(&image::load_from_memory(BUILTIN).ok()?, PathBuf::new(), Source::Builtin)
}

#[must_use]
pub(crate) fn gallery(current: Option<&Path>) -> Gallery {
    Gallery { builtin: builtin(), items: load(current) }
}

#[must_use]
pub fn load(current: Option<&Path>) -> Vec<Thumbnail> {
    let paths = discover(current);
    let mut thumbnails = Vec::with_capacity(paths.len());
    for chunk in paths.chunks(4) {
        std::thread::scope(|scope| {
            let jobs: Vec<_> =
                chunk.iter().map(|(path, source)| scope.spawn(move || thumbnail_from(path, *source))).collect();
            thumbnails.extend(jobs.into_iter().filter_map(|job| job.join().ok().flatten()));
        });
    }
    thumbnails
}

#[must_use]
pub(crate) fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.trim().trim_start_matches('#');
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

#[must_use]
pub fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

#[must_use]
pub(crate) fn solid_file_name(rgb: [u8; 3]) -> String {
    format!("solid-{}.png", hex(rgb).trim_start_matches('#'))
}

#[must_use]
pub(crate) fn solid_color(path: &Path) -> Option<[u8; 3]> {
    let name = path.file_name()?.to_str()?;
    let digits = name.strip_prefix("solid-")?.strip_suffix(".png")?;
    parse_hex(digits)
}

#[must_use]
pub(crate) fn solid_dir() -> Option<PathBuf> {
    super::privacy::data_home().map(|data| data.join("glide/solid"))
}

pub(crate) fn write_solid_in(dir: &Path, rgb: [u8; 3]) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    let path = dir.join(solid_file_name(rgb));
    if path.is_file() {
        return Ok(path);
    }
    let [r, g, b] = rgb;
    let pixels = image::RgbImage::from_pixel(SOLID_SIZE, SOLID_SIZE, image::Rgb([r, g, b]));
    let mut tmp = path.clone().into_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    pixels.save_with_format(&tmp, image::ImageFormat::Png).map_err(|err| err.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|err| {
        let _ = std::fs::remove_file(&tmp);
        err.to_string()
    })?;
    Ok(path)
}

pub(crate) fn write_solid(rgb: [u8; 3]) -> Result<PathBuf, String> {
    let dir = solid_dir().ok_or_else(|| String::from("no home directory"))?;
    write_solid_in(&dir, rgb)
}

pub(crate) fn open_user_folder() -> Result<(), String> {
    let dir = user_folder().ok_or_else(|| String::from("no home directory"))?;
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    super::run::status("xdg-open", &[&dir.to_string_lossy()]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colours_parse_with_or_without_a_hash() {
        assert_eq!(parse_hex("#1d2433"), Some([0x1d, 0x24, 0x33]));
        assert_eq!(parse_hex(" 1D2433 "), Some([0x1d, 0x24, 0x33]));
        assert_eq!(parse_hex("#1d243"), None);
        assert_eq!(parse_hex("#1d24zz"), None);
        assert_eq!(hex([0x1d, 0x24, 0x33]), "#1d2433");
    }

    #[test]
    fn a_solid_colour_file_names_its_colour() {
        let name = solid_file_name([0xab, 0x01, 0xff]);
        assert_eq!(name, "solid-ab01ff.png");
        assert_eq!(solid_color(Path::new(&name)), Some([0xab, 0x01, 0xff]));
        assert_eq!(solid_color(Path::new("/tmp/beach.png")), None);
    }

    #[test]
    fn a_solid_colour_is_written_as_a_png_the_compositor_can_decode() {
        let dir = std::env::temp_dir().join(format!("glide-solid-{}", std::process::id()));
        let path = write_solid_in(&dir, [10, 20, 30]).expect("writable");
        assert!(is_image(&path));
        let decoded = image::open(&path).expect("decodes").into_rgb8();
        assert_eq!(decoded.dimensions(), (SOLID_SIZE, SOLID_SIZE));
        assert_eq!(decoded.get_pixel(3, 3).0, [10, 20, 30]);
        assert_eq!(write_solid_in(&dir, [10, 20, 30]).expect("reused"), path);
        let mut found = Vec::new();
        walk(&dir, 0, 10, &mut found);
        assert!(found.is_empty(), "generated colours never crowd the gallery");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn system_roots_include_kde_gnome_xfce_and_lxqt() {
        let roots = system_roots();
        assert!(roots.contains(&PathBuf::from("/usr/share/backgrounds")));
        assert!(roots.contains(&PathBuf::from("/usr/share/wallpapers")));
        assert!(roots.contains(&PathBuf::from("/usr/share/plasma/wallpapers")));
        assert!(roots.contains(&PathBuf::from("/usr/share/xfce4/backdrops")));
        assert!(roots.contains(&PathBuf::from("/usr/share/lxqt/wallpapers")));
    }

    #[test]
    fn user_roots_include_kde_and_pictures_wallpapers() {
        if let Some(home) = home() {
            let roots = user_roots();
            assert!(roots.contains(&home.join(".local/share/wallpapers")));
            assert!(roots.contains(&home.join(".local/share/backgrounds")));
            assert!(roots.iter().any(|r| r.ends_with("Wallpapers")));
        }
    }

    #[test]
    fn is_image_accepts_common_wallpaper_formats() {
        assert!(is_image(Path::new("wallpaper.png")));
        assert!(is_image(Path::new("wallpaper.jpg")));
        assert!(is_image(Path::new("wallpaper.jpeg")));
        assert!(is_image(Path::new("wallpaper.jfif")));
        assert!(is_image(Path::new("wallpaper.webp")));
        assert!(is_image(Path::new("WALLPAPER.PNG")));
        assert!(is_image(Path::new("WALLPAPER.WEBP")));
        assert!(!is_image(Path::new("wallpaper.txt")));
        assert!(!is_image(Path::new("wallpaper.xml")));
        assert!(!is_image(Path::new("wallpaper")));
    }

    #[test]
    fn kde_packages_pick_highest_resolution_and_dark_variant() {
        let dir = std::env::temp_dir().join(format!("glide-kde-test-{}", std::process::id()));
        let pkg = dir.join("Wallpapers/Next");
        let images = pkg.join("contents/images");
        let images_dark = pkg.join("contents/images_dark");
        std::fs::create_dir_all(&images).expect("dir created");
        std::fs::create_dir_all(&images_dark).expect("dir created");

        let write_stub = |p: &Path| {
            std::fs::write(p, [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]).expect("written");
        };

        write_stub(&pkg.join("contents/screenshot.png"));
        write_stub(&images.join("1920x1080.png"));
        write_stub(&images.join("5120x2880.png"));
        write_stub(&images_dark.join("1920x1080.png"));
        write_stub(&images_dark.join("5120x2880.png"));

        let mut found = Vec::new();
        walk(&dir, 4, 100, &mut found);

        assert_eq!(found.len(), 2);
        assert!(found.contains(&images.join("5120x2880.png")));
        assert!(found.contains(&images_dark.join("5120x2880.png")));
        assert!(!found.contains(&images.join("1920x1080.png")));
        assert!(!found.contains(&pkg.join("contents/screenshot.png")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fair_quota_across_multiple_roots() {
        let dir = std::env::temp_dir().join(format!("glide-fair-test-{}", std::process::id()));
        let root1 = dir.join("root1");
        let root2 = dir.join("root2");
        std::fs::create_dir_all(&root1).expect("root1");
        std::fs::create_dir_all(&root2).expect("root2");

        for i in 0..80 {
            std::fs::write(root1.join(format!("r1_{i}.png")), [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a])
                .expect("written");
        }
        for i in 0..10 {
            std::fs::write(root2.join(format!("r2_{i}.png")), [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a])
                .expect("written");
        }

        let collected = collect(&[root1, root2], &[]);
        let root2_count = collected.iter().filter(|p| p.to_string_lossy().contains("r2_")).count();
        assert_eq!(root2_count, 10);
        let root1_count = collected.iter().filter(|p| p.to_string_lossy().contains("r1_")).count();
        assert_eq!(root1_count, MAX_PER_ROOT);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

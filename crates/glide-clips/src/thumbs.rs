use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

pub const SIDE: u32 = 320;

fn fnv(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

#[must_use]
pub fn cache_name(path: &Path, modified_nanos: u128, len: u64) -> String {
    let key =
        fnv(path.as_os_str().as_bytes().iter().copied().chain(modified_nanos.to_le_bytes()).chain(len.to_le_bytes()));
    format!("{key:016x}-{SIDE}.png")
}

#[must_use]
pub fn cache_dir() -> Option<PathBuf> {
    Some(glide_config::cache_home()?.join("glide/thumbnails"))
}

#[must_use]
pub fn cached(path: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(path).ok().filter(std::fs::Metadata::is_file)?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(cache_dir()?.join(cache_name(path, modified.as_nanos(), meta.len())))
}

#[must_use]
pub fn make(path: &Path) -> Option<PathBuf> {
    let output = cached(path)?;
    if output.is_file() {
        return Some(output);
    }
    let failed = output.with_extension("fail");
    if failed.exists() {
        return None;
    }
    std::fs::create_dir_all(output.parent()?).ok()?;
    let uri = glide_video::gstreamer::glib::filename_to_uri(path, None).ok()?.to_string();
    let info = glide_video::info::discover(&uri, Duration::from_secs(10)).ok();
    let size = info.as_ref().and_then(|info| info.video.as_ref()).map(|video| (video.width, video.height));
    let Some((width, height)) = size.filter(|(width, height)| *width > 0 && *height > 0) else {
        let _ = std::fs::write(&failed, b"");
        return None;
    };
    let narrow = if width >= height { SIDE } else { (SIDE * width / height).max(2) & !1 };
    let at = info.and_then(|info| info.duration).map_or(Duration::from_secs(1), |length| length.mul_f32(0.1));
    let Some(frame) = glide_video::frames::still(&uri, at, narrow) else {
        let _ = std::fs::write(&failed, b"");
        return None;
    };
    let image = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba)?;
    let partial = output.with_extension(format!("{}.part.png", std::process::id()));
    if image.save_with_format(&partial, image::ImageFormat::Png).is_ok() && std::fs::rename(&partial, &output).is_ok() {
        return Some(output);
    }
    let _ = std::fs::remove_file(&partial);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_key_matches_files_and_photos() {
        let name = cache_name(Path::new("/home/u/Відео/a.mkv"), 1_700_000_000_000_000_000, 4096);
        assert!(name.ends_with("-320.png"));
        assert_eq!(name.len(), 16 + "-320.png".len());
        assert_ne!(name, cache_name(Path::new("/home/u/Відео/a.mkv"), 1_700_000_000_000_000_001, 4096));
    }
}

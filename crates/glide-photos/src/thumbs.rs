use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::formats::{self, Format, Request};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Thumb {
    Ready(PathBuf),
    Failed,
}

#[must_use]
pub fn make(input: &Path, format: Format) -> Thumb {
    cache_dir().map_or(Thumb::Failed, |dir| make_in(&dir, input, format))
}

#[must_use]
pub fn make_in(dir: &Path, input: &Path, format: Format) -> Thumb {
    let Some(meta) = std::fs::metadata(input).ok().filter(std::fs::Metadata::is_file) else {
        return Thumb::Failed;
    };
    let Some(modified) = meta.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()) else {
        return Thumb::Failed;
    };
    let output = dir.join(cache_name(input, modified.as_nanos(), meta.len()));
    if output.is_file() {
        return Thumb::Ready(output);
    }
    let failed = output.with_extension("fail");
    if failed.exists() || std::fs::create_dir_all(dir).is_err() {
        return Thumb::Failed;
    }
    let request = Request { path: input, format, max_side: Some(SIDE), page: 0, animate: false };
    let Ok(decoded) = formats::decode(&request) else {
        let _ = std::fs::write(&failed, b"");
        return Thumb::Failed;
    };
    let partial = output.with_extension(format!("{}.part.png", std::process::id()));
    if decoded.image.save_with_format(&partial, image::ImageFormat::Png).is_ok()
        && std::fs::rename(&partial, &output).is_ok()
    {
        return Thumb::Ready(output);
    }
    let _ = std::fs::remove_file(&partial);
    Thumb::Failed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnails_are_cached_under_a_key_that_follows_the_file() {
        let dir = std::env::temp_dir().join(format!("glide-photos-thumbs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("wide.png");
        image::RgbImage::from_pixel(800, 400, image::Rgb([1, 2, 3])).save(&input).unwrap();
        let format = formats::of(&input).unwrap();
        let cache = dir.join("cache");
        let Thumb::Ready(first) = make_in(&cache, &input, format) else { panic!("no thumbnail") };
        assert_eq!(image::image_dimensions(&first).unwrap(), (SIDE, SIDE / 2));
        assert_eq!(make_in(&cache, &input, format), Thumb::Ready(first));
        let broken = dir.join("broken.jpg");
        std::fs::write(&broken, b"not a jpeg").unwrap();
        assert_eq!(make_in(&cache, &broken, formats::of(&broken).unwrap()), Thumb::Failed);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

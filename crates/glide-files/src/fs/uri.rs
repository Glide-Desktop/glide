use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

pub use glide_config::uri::{encode as encode_path, file_uri};

#[must_use]
pub fn decode_path(encoded: &str) -> PathBuf {
    PathBuf::from(OsString::from_vec(glide_config::uri::decode(encoded)))
}

#[must_use]
pub fn path_from_arg(arg: &str) -> Option<PathBuf> {
    match arg.strip_prefix("file://") {
        Some(rest) => {
            let path = rest.find('/').map_or(rest, |slash| &rest[slash..]);
            Some(decode_path(path))
        }
        None if arg.contains("://") => None,
        None if Path::new(arg).is_absolute() => Some(PathBuf::from(arg)),
        None => std::fs::canonicalize(arg).or_else(|_| std::path::absolute(arg)).ok(),
    }
}

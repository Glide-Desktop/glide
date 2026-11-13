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
    if Path::new(arg).is_absolute() {
        return Some(PathBuf::from(arg));
    }
    if arg.get(..5).is_some_and(|scheme| scheme.eq_ignore_ascii_case("file:")) {
        return glide_config::uri::local_path(arg);
    }
    if arg.contains("://") {
        return None;
    }
    std::fs::canonicalize(arg).or_else(|_| std::path::absolute(arg)).ok()
}

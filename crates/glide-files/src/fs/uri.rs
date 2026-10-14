use std::ffi::OsString;
use std::fmt::Write;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

#[must_use]
pub fn encode_path(path: &Path) -> String {
    let mut out = String::new();
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[must_use]
pub fn decode_path(encoded: &str) -> PathBuf {
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%')
            .then(|| bytes.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        if let Some(byte) = escaped {
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    PathBuf::from(OsString::from_vec(out))
}

#[must_use]
pub fn path_from_arg(arg: &str) -> Option<PathBuf> {
    match arg.strip_prefix("file://") {
        Some(rest) => {
            let path = rest.find('/').map_or(rest, |slash| &rest[slash..]);
            Some(decode_path(path))
        }
        None if arg.contains("://") => None,
        None => Some(PathBuf::from(arg)),
    }
}

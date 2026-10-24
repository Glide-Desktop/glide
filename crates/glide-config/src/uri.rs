use std::ffi::OsString;
use std::fmt::Write;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

#[must_use]
pub fn encode(path: &Path) -> String {
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
pub fn file_uri(path: &Path) -> String {
    format!("file://{}", encode(path))
}

#[must_use]
pub fn decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%')
            .then(|| text.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        if let Some(byte) = escaped {
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    out
}

#[must_use]
pub fn local_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.trim().strip_prefix("file://")?;
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    if !path.starts_with('/') {
        return None;
    }
    let bytes = decode(path);
    (!bytes.contains(&0)).then(|| PathBuf::from(OsString::from_vec(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_round_trip_through_file_uris() {
        let path = Path::new("/home/me/My Docs/ü%.txt");
        assert_eq!(file_uri(path), "file:///home/me/My%20Docs/%C3%BC%25.txt");
        assert_eq!(local_path(&file_uri(path)).as_deref(), Some(path));
    }

    #[test]
    fn only_local_file_uris_name_a_path() {
        assert_eq!(local_path("file:///a%20b"), Some(PathBuf::from("/a b")));
        assert_eq!(local_path("file://localhost/etc/hosts"), Some(PathBuf::from("/etc/hosts")));
        assert_eq!(local_path("file:///bad%zz%2"), Some(PathBuf::from("/bad%zz%2")));
        assert_eq!(local_path("file://server/share"), None);
        assert_eq!(local_path("https://example.com/a"), None);
        assert_eq!(local_path("/plain/path"), None);
        assert_eq!(local_path("file:///a%00b"), None);
    }
}

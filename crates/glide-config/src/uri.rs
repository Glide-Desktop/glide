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
            .filter(|hex| hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
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

fn host_name() -> Option<String> {
    let mut buffer = [0u8; 256];
    // SAFETY: the buffer is writable for its whole length, which is what gethostname is told.
    if unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) } != 0 {
        return None;
    }
    let end = buffer.iter().position(|byte| *byte == 0).unwrap_or(buffer.len());
    std::str::from_utf8(&buffer[..end]).ok().filter(|name| !name.is_empty()).map(str::to_string)
}

#[must_use]
pub fn local_path(uri: &str) -> Option<PathBuf> {
    local_path_on(uri, host_name().as_deref())
}

fn local_path_on(uri: &str, host: Option<&str>) -> Option<PathBuf> {
    let uri = uri.trim();
    if !uri.get(..5).is_some_and(|scheme| scheme.eq_ignore_ascii_case("file:")) {
        return None;
    }
    let rest = uri.get(5..)?.split(['?', '#']).next().unwrap_or_default();
    let path = match rest.strip_prefix("//") {
        Some(authority) => {
            let (name, path) = authority.split_at(authority.find('/').unwrap_or(authority.len()));
            let local = name.is_empty()
                || name.eq_ignore_ascii_case("localhost")
                || host.is_some_and(|host| host.eq_ignore_ascii_case(name));
            if !local {
                return None;
            }
            path
        }
        None => rest,
    };
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

    #[test]
    fn the_short_form_and_this_machines_own_host_name_name_local_files() {
        let on = |uri: &str| local_path_on(uri, Some("laptop"));
        assert_eq!(on("file:/etc/hosts"), Some(PathBuf::from("/etc/hosts")));
        assert_eq!(on("file://laptop/etc/hosts"), Some(PathBuf::from("/etc/hosts")));
        assert_eq!(on("FILE://LAPTOP/etc/hosts"), Some(PathBuf::from("/etc/hosts")));
        assert_eq!(on("file://server/etc/hosts"), None);
        assert_eq!(on("file:relative"), None);
        assert_eq!(local_path_on("file://laptop/etc/hosts", None), None);
    }

    #[test]
    fn a_query_or_fragment_is_not_part_of_the_path() {
        let on = |uri: &str| local_path_on(uri, None);
        assert_eq!(on("file:///tmp/a.pdf#page=2"), Some(PathBuf::from("/tmp/a.pdf")));
        assert_eq!(on("file:///tmp/a.txt?line=4"), Some(PathBuf::from("/tmp/a.txt")));
        assert_eq!(on("file:///tmp/a%23b%3Fc"), Some(PathBuf::from("/tmp/a#b?c")), "escaped ones stay in the name");
    }
}

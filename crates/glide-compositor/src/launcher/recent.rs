use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

pub const LIMIT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentFile {
    pub path: PathBuf,
    pub name: String,
    pub folder: String,
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

#[must_use]
pub fn unescape_xml(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

#[must_use]
pub fn percent_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    Some(decoded)
}

#[must_use]
pub fn path_from_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    Some(PathBuf::from(OsString::from_vec(percent_decode(rest)?)))
}

#[must_use]
pub fn parse(text: &str) -> Vec<(PathBuf, String)> {
    text.split("<bookmark ")
        .skip(1)
        .filter_map(|chunk| {
            let tag = format!(" {}", &chunk[..chunk.find('>')?]);
            let href = unescape_xml(attribute(&tag, "href")?);
            let modified = attribute(&tag, "modified").unwrap_or_default().to_string();
            Some((path_from_uri(&href)?, modified))
        })
        .collect()
}

fn folder_label(path: &Path, home: Option<&Path>) -> String {
    let Some(parent) = path.parent() else {
        return String::new();
    };
    match home.and_then(|home| parent.strip_prefix(home).ok()) {
        Some(inside) if inside.as_os_str().is_empty() => "~".to_string(),
        Some(inside) => format!("~/{}", inside.display()),
        None => parent.display().to_string(),
    }
}

#[must_use]
pub fn recent_files(
    mut entries: Vec<(PathBuf, String)>, home: Option<&Path>, exists: impl Fn(&Path) -> bool,
) -> Vec<RecentFile> {
    entries.sort_by(|a, b| b.1.cmp(&a.1));
    let mut seen = std::collections::HashSet::new();
    entries
        .into_iter()
        .filter(|(path, _)| seen.insert(path.clone()))
        .filter(|(path, _)| exists(path))
        .take(LIMIT)
        .filter_map(|(path, _)| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            Some(RecentFile { folder: folder_label(&path, home), name, path })
        })
        .collect()
}

#[must_use]
pub fn load() -> Vec<RecentFile> {
    let home = glide_config::home();
    let Some(file) = glide_config::data_home().map(|dir| dir.join("recently-used.xbel")) else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&file) else {
        return Vec::new();
    };
    recent_files(parse(&text), home.as_deref(), Path::exists)
}

#[cfg(test)]
mod tests {
    use super::*;

    const XBEL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xbel version="1.0" xmlns:bookmark="http://www.freedesktop.org/standards/desktop-bookmarks">
  <bookmark href="file:///home/user/Documents/Tax%20return%202025.pdf" added="2026-01-02T10:00:00Z" modified="2026-01-02T10:00:00Z" visited="2026-01-02T10:00:00Z">
    <info><metadata owner="http://freedesktop.org">
      <bookmark:applications><bookmark:application name="evince" exec="&apos;evince %u&apos;" modified="2026-01-02T10:00:00Z" count="1"/></bookmark:applications>
    </metadata></info>
  </bookmark>
  <bookmark href="file:///home/user/%D0%97%D0%B2%D1%96%D1%82.odt" added="2026-03-01T09:00:00Z" modified="2026-03-01T09:00:00Z" visited="2026-03-01T09:00:00Z"/>
  <bookmark href="https://example.com/page" modified="2026-04-01T00:00:00Z"/>
  <bookmark href="file:///tmp/a&amp;b.txt" modified="2025-12-31T23:00:00Z"/>
</xbel>"#;

    #[test]
    fn bookmarks_yield_local_paths_newest_first() {
        let home = Path::new("/home/user");
        let files = recent_files(parse(XBEL), Some(home), |_| true);
        let summary: Vec<(&str, &str)> = files.iter().map(|file| (file.name.as_str(), file.folder.as_str())).collect();
        assert_eq!(summary, [("Звіт.odt", "~"), ("Tax return 2025.pdf", "~/Documents"), ("a&b.txt", "/tmp")]);
    }

    #[test]
    fn missing_files_and_duplicates_are_dropped() {
        let entries = vec![
            (PathBuf::from("/a/x.txt"), "2026-01-02".to_string()),
            (PathBuf::from("/a/x.txt"), "2026-01-01".to_string()),
            (PathBuf::from("/a/gone.txt"), "2026-01-03".to_string()),
        ];
        let files = recent_files(entries, None, |path| !path.ends_with("gone.txt"));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].folder, "/a");
    }

    #[test]
    fn uris_are_decoded_strictly() {
        assert_eq!(path_from_uri("file:///a%20b"), Some(PathBuf::from("/a b")));
        assert_eq!(path_from_uri("file://localhost/etc/hosts"), Some(PathBuf::from("/etc/hosts")));
        assert_eq!(path_from_uri("file:///broken%2"), None);
        assert_eq!(path_from_uri("file://server/share"), None);
        assert_eq!(path_from_uri("/plain/path"), None);
        assert_eq!(unescape_xml("&lt;&amp;lt;&gt;"), "<&lt;>");
    }
}

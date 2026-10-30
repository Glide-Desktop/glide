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

const LONGEST_REFERENCE: usize = 12;

fn reference(body: &str) -> Option<char> {
    match body {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "amp" => Some('&'),
        _ => {
            let digits = body.strip_prefix('#')?;
            let (digits, radix) = match digits.strip_prefix('x') {
                Some(hex) => (hex, 16),
                None => (digits, 10),
            };
            if digits.is_empty() || !digits.chars().all(|ch| ch.is_digit(radix)) {
                return None;
            }
            char::from_u32(u32::from_str_radix(digits, radix).ok()?).filter(|&ch| ch != '\0')
        }
    }
}

#[must_use]
pub fn unescape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let window = tail.char_indices().take(LONGEST_REFERENCE + 1).find(|&(_, ch)| ch == ';');
        if let Some((end, ch)) = window.and_then(|(end, _)| Some((end, reference(&tail[..end])?))) {
            out.push(ch);
            rest = &tail[end + 1..];
        } else {
            out.push('&');
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

use glide_config::uri::local_path as path_from_uri;

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
    fn xml_entities_are_unescaped_once() {
        assert_eq!(unescape_xml("&lt;&amp;lt;&gt;"), "<&lt;>");
        assert_eq!(unescape_xml("&amp;#39;"), "&#39;");
        assert_eq!(unescape_xml("&amp;amp;"), "&amp;");
    }

    #[test]
    fn numeric_character_references_are_decoded() {
        assert_eq!(unescape_xml("Tom&#39;s &#x27;notes&#X27;"), "Tom's 'notes&#X27;");
        assert_eq!(unescape_xml("&#1047;&#x432;&#x1F600;"), "Зв\u{1F600}");
    }

    #[test]
    fn invalid_character_references_are_left_as_they_are() {
        for text in [
            "&#99999999999999999999;",
            "&#xD800;",
            "&#x110000;",
            "&#0;",
            "&#;",
            "&#x;",
            "&#+39;",
            "&#-39;",
            "&#3 9;",
            "&#39",
            "&unknown;",
            "&",
            "a & b",
            "&#0000000000000000039;",
        ] {
            assert_eq!(unescape_xml(text), text, "{text:?}");
        }
    }

    #[test]
    fn a_recent_file_with_an_apostrophe_in_its_name_is_listed() {
        let xbel = r#"<xbel><bookmark href="file:///home/user/Tom&#39;s%20notes.pdf" modified="2026-05-01T00:00:00Z"/>
<bookmark href="file:///home/user/Ann&apos;s.txt" modified="2026-04-01T00:00:00Z"/></xbel>"#;
        let files = recent_files(parse(xbel), Some(Path::new("/home/user")), |_| true);
        let names: Vec<&str> = files.iter().map(|file| file.name.as_str()).collect();
        assert_eq!(names, ["Tom's notes.pdf", "Ann's.txt"]);
    }
}

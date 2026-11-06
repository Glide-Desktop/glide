use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{Engine, Format, Heading, Info, Link, OpenError, Opened, PageText, Size};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Heading(u8),
    Para,
    Quote,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub kind: Kind,
    pub text: String,
    pub chapter: usize,
}

pub struct EpubEngine {
    opened: Opened,
}

#[must_use]
pub fn entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let Some(end) = tail.find(';').filter(|end| *end < 12) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let name = &tail[1..end];
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "hellip" => Some('…'),
            "laquo" => Some('«'),
            "raquo" => Some('»'),
            _ => name
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let at = tag.find(&format!("{name}="))?;
    let rest = &tag[at + name.len() + 1..];
    let quote = rest.chars().next()?;
    let rest = &rest[1..];
    let end = rest.find(quote)?;
    Some(entities(&rest[..end]))
}

#[must_use]
pub fn blocks(markup: &str, chapter: usize) -> Vec<Block> {
    let mut out = Vec::new();
    let mut current: Option<(Kind, String)> = None;
    let mut skip = 0u32;
    let mut rest = markup;
    while let Some(open) = rest.find('<') {
        let chunk = &rest[..open];
        if skip == 0
            && let Some((_, text)) = current.as_mut()
        {
            text.push_str(&entities(chunk));
        }
        let Some(close) = rest[open..].find('>') else {
            break;
        };
        let tag = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        let name = name.rsplit(':').next().unwrap_or("").to_string();
        match name.as_str() {
            "script" | "style" | "head" => {
                if closing {
                    skip = skip.saturating_sub(1);
                } else if !tag.ends_with('/') {
                    skip += 1;
                }
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "li" | "blockquote" | "title" | "div" => {
                if name == "div" && !closing && current.is_some() {
                    continue;
                }
                if let Some((kind, text)) = current.take() {
                    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !text.is_empty() {
                        out.push(Block { kind, text, chapter });
                    }
                }
                if !closing && name != "div" {
                    let kind = match name.as_str() {
                        "blockquote" => Kind::Quote,
                        "title" => Kind::Heading(2),
                        "p" | "li" => Kind::Para,
                        heading => Kind::Heading(heading[1..].parse().unwrap_or(2)),
                    };
                    current = Some((kind, String::new()));
                }
            }
            "br" => {
                if let Some((_, text)) = current.as_mut() {
                    text.push(' ');
                }
            }
            _ => {}
        }
    }
    if let Some((kind, text)) = current {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !text.is_empty() {
            out.push(Block { kind, text, chapter });
        }
    }
    out
}

fn read_entry(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<String> {
    let mut entry = archive.by_name(name).ok()?;
    let mut text = String::new();
    entry.read_to_string(&mut text).ok()?;
    Some(text)
}

fn spine(archive: &mut zip::ZipArchive<std::fs::File>) -> Result<(Vec<String>, Info), OpenError> {
    let container = read_entry(archive, "META-INF/container.xml").ok_or(OpenError::Damaged)?;
    let root = attribute(&container, "full-path").ok_or(OpenError::Damaged)?;
    let opf = read_entry(archive, &root).ok_or(OpenError::Damaged)?;
    let base = root.rsplit_once('/').map_or(String::new(), |(dir, _)| format!("{dir}/"));
    let mut manifest = std::collections::HashMap::new();
    for item in opf.split("<item ").skip(1) {
        let tag = item.split('>').next().unwrap_or("");
        if let (Some(id), Some(href)) = (attribute(tag, "id"), attribute(tag, "href")) {
            manifest.insert(id, format!("{base}{href}"));
        }
    }
    let order: Vec<String> = opf
        .split("<itemref ")
        .skip(1)
        .filter_map(|item| attribute(item.split('>').next().unwrap_or(""), "idref"))
        .filter_map(|id| manifest.get(&id).cloned())
        .collect();
    let between = |open: &str| {
        let at = opf.find(open)?;
        let start = opf[at..].find('>')? + at + 1;
        let end = opf[start..].find("</")? + start;
        Some(entities(opf[start..end].trim()))
    };
    let info = Info {
        title: between("<dc:title").unwrap_or_default(),
        author: between("<dc:creator").unwrap_or_default(),
        print: true,
        copy: true,
        ..Info::default()
    };
    Ok((order, info))
}

impl EpubEngine {
    pub fn open(path: &Path) -> Result<Self, OpenError> {
        let is_fb2 = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("fb2"));
        let (mut all, info, chapters) = if is_fb2 {
            let text = std::fs::read_to_string(path).map_err(|e| OpenError::Io(e.to_string()))?;
            let mut all = Vec::new();
            for (index, section) in text.split("<section").skip(1).enumerate() {
                all.extend(blocks(&format!("<section{section}"), index));
            }
            let title = text.find("<book-title>").and_then(|at| {
                let rest = &text[at + 12..];
                rest.find("</book-title>").map(|end| entities(&rest[..end]))
            });
            let chapters = all.last().map_or(1, |b| b.chapter + 1);
            (all, Info { title: title.unwrap_or_default(), print: true, copy: true, ..Info::default() }, chapters)
        } else {
            let file = std::fs::File::open(path).map_err(|e| OpenError::Io(e.to_string()))?;
            let mut archive = zip::ZipArchive::new(file).map_err(|_| OpenError::Damaged)?;
            let (order, info) = spine(&mut archive)?;
            let mut all = Vec::new();
            for (index, name) in order.iter().enumerate() {
                if let Some(markup) = read_entry(&mut archive, name) {
                    all.extend(blocks(&markup, index));
                }
            }
            (all, info, order.len().max(1))
        };
        all.retain(|block| !block.text.is_empty());
        if all.is_empty() {
            return Err(OpenError::Damaged);
        }
        let outline = all
            .iter()
            .filter_map(|block| match block.kind {
                Kind::Heading(level) if level <= 3 => {
                    Some(Heading { title: block.text.clone(), level: level.max(1) - 1, page: Some(block.chapter) })
                }
                _ => None,
            })
            .collect();
        Ok(Self {
            opened: Opened {
                format: Format::Epub,
                pages: vec![Size { w: 420.0, h: 595.0 }; chapters],
                labels: (1..=chapters).map(|n| n.to_string()).collect(),
                info,
                outline,
                fields: Vec::new(),
                notes: Vec::new(),
                bookmarks: Vec::new(),
                broken: Vec::new(),
                reflow: Some(all),
                view_only: None,
                right_to_left: false,
            },
        })
    }
}

impl Engine for EpubEngine {
    fn opened(&self) -> &Opened {
        &self.opened
    }

    fn render(&mut self, _: usize, _: f32) -> Option<image::RgbaImage> {
        None
    }

    fn text(&mut self, chapter: usize) -> PageText {
        let mut text = PageText::default();
        for (line, block) in self.opened.reflow.iter().flatten().filter(|b| b.chapter == chapter).enumerate() {
            for word in block.text.split_whitespace() {
                text.words.push(super::Word {
                    text: word.to_string(),
                    rect: super::Rect::default(),
                    line: line as u32,
                });
            }
        }
        text
    }

    fn links(&mut self, _: usize) -> Vec<Link> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn markup_becomes_blocks() {
        let found = blocks(
            "<html><head><title>x</title></head><body><h1>Дія друга</h1><p>Галява &amp; ліс<br/>далі</p><script>no</script><blockquote>Цитата</blockquote></body></html>",
            3,
        );
        assert_eq!(
            found,
            vec![
                Block { kind: Kind::Heading(1), text: "Дія друга".into(), chapter: 3 },
                Block { kind: Kind::Para, text: "Галява & ліс далі".into(), chapter: 3 },
                Block { kind: Kind::Quote, text: "Цитата".into(), chapter: 3 },
            ]
        );
    }

    #[test]
    fn an_epub_reads_its_spine_in_order() {
        let dir = std::env::temp_dir().join(format!("glide-folio-epub-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("book.epub");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let files = [
            (
                "META-INF/container.xml",
                r#"<container><rootfiles><rootfile full-path="OEBPS/book.opf"/></rootfiles></container>"#,
            ),
            (
                "OEBPS/book.opf",
                r#"<package><metadata><dc:title>Лісова пісня</dc:title><dc:creator>Леся Українка</dc:creator></metadata><manifest><item id="a" href="one.xhtml"/><item id="b" href="two.xhtml"/></manifest><spine><itemref idref="b"/><itemref idref="a"/></spine></package>"#,
            ),
            ("OEBPS/one.xhtml", "<html><body><h2>Дія перша</h2><p>Один</p></body></html>"),
            ("OEBPS/two.xhtml", "<html><body><h2>Пролог</h2><p>Два</p></body></html>"),
        ];
        for (name, body) in files {
            zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let engine = EpubEngine::open(&path).expect("opens");
        let opened = engine.opened();
        assert_eq!(opened.info.title, "Лісова пісня");
        assert_eq!(opened.outline[0].title, "Пролог");
        assert_eq!(opened.outline[1].page, Some(1));
        assert_eq!(opened.reflow.as_ref().map(Vec::len), Some(4));
        let _ = std::fs::remove_dir_all(dir);
    }
}

use std::io::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use iced::widget::text_editor::{Action, Content, Cursor, Edit, Position};

use crate::detect;
use crate::history::{Diff, History, Kind};
use crate::syntax::{self, Language, Lines};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    #[default]
    Utf8,
    Utf16Le,
    Utf16Be,
    Latin1,
}

impl Encoding {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
            Self::Latin1 => "ISO-8859-1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ending {
    #[default]
    Lf,
    CrLf,
}

impl Ending {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::CrLf => "CRLF",
        }
    }

    const fn bytes(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub encoding: Encoding,
    pub bom: bool,
    pub ending: Ending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    Binary,
    Io(String),
}

fn utf16(bytes: &[u8], little: bool) -> String {
    let units = bytes.chunks_exact(2).map(|pair| {
        if little { u16::from_le_bytes([pair[0], pair[1]]) } else { u16::from_be_bytes([pair[0], pair[1]]) }
    });
    char::decode_utf16(units).map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
}

fn normalise(text: &str) -> (String, Ending) {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    let ending = if crlf > lf { Ending::CrLf } else { Ending::Lf };
    let text = if text.contains('\r') { text.replace("\r\n", "\n").replace('\r', "\n") } else { text.to_string() };
    (text, ending)
}

pub fn decode(bytes: &[u8]) -> Result<Decoded, LoadError> {
    let (text, encoding, bom) = if let Some(rest) = bytes.strip_prefix(b"\xef\xbb\xbf") {
        (String::from_utf8_lossy(rest).into_owned(), Encoding::Utf8, true)
    } else if let Some(rest) = bytes.strip_prefix(b"\xff\xfe") {
        (utf16(rest, true), Encoding::Utf16Le, true)
    } else if let Some(rest) = bytes.strip_prefix(b"\xfe\xff") {
        (utf16(rest, false), Encoding::Utf16Be, true)
    } else if bytes[..bytes.len().min(8192)].contains(&0) {
        return Err(LoadError::Binary);
    } else {
        match std::str::from_utf8(bytes) {
            Ok(text) => (text.to_string(), Encoding::Utf8, false),
            Err(_) => (bytes.iter().map(|&byte| char::from(byte)).collect(), Encoding::Latin1, false),
        }
    };
    let (text, ending) = normalise(&text);
    Ok(Decoded { text, encoding, bom, ending })
}

#[must_use]
pub fn encode(text: &str, encoding: Encoding, bom: bool, ending: Ending) -> Vec<u8> {
    let text = if ending == Ending::Lf { text.to_string() } else { text.replace('\n', ending.bytes()) };
    match encoding {
        Encoding::Utf8 => {
            let mut bytes = if bom { b"\xef\xbb\xbf".to_vec() } else { Vec::new() };
            bytes.extend_from_slice(text.as_bytes());
            bytes
        }
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let little = encoding == Encoding::Utf16Le;
            let mut bytes = if little { vec![0xff, 0xfe] } else { vec![0xfe, 0xff] };
            for unit in text.encode_utf16() {
                bytes.extend_from_slice(&if little { unit.to_le_bytes() } else { unit.to_be_bytes() });
            }
            bytes
        }
        Encoding::Latin1 => text.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect(),
    }
}

#[must_use]
pub fn fits(text: &str, encoding: Encoding) -> bool {
    encoding != Encoding::Latin1 || text.chars().all(|c| u32::from(c) < 256)
}

pub fn read(path: &Path) -> Result<Decoded, LoadError> {
    let bytes = std::fs::read(path).map_err(|error| LoadError::Io(error.to_string()))?;
    decode(&bytes)
}

pub fn write(path: &Path, bytes: &[u8]) -> std::io::Result<SystemTime> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let folder = target.parent().filter(|folder| !folder.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = target.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let mode = std::fs::metadata(&target).map_or(0o644, |meta| meta.permissions().mode() & 0o7777);
    let temporary = folder.join(format!(".{name}.glide-text-{}", std::process::id()));
    let written = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(mode).open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(mode))?;
        std::fs::rename(&temporary, &target)
    })();
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temporary);
        if error.kind() != std::io::ErrorKind::PermissionDenied && error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(error);
        }
        std::fs::write(&target, bytes)?;
    }
    modified(&target).ok_or_else(|| std::io::Error::other("the saved file vanished"))
}

#[must_use]
pub fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

#[must_use]
pub fn mirror(content: &Content) -> String {
    let mut text = String::new();
    for (index, line) in content.lines().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        text.push_str(&line.text);
    }
    text
}

#[must_use]
pub fn position(text: &str, offset: usize) -> Position {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let column = offset - before.rfind('\n').map_or(0, |at| at + 1);
    Position { line, column }
}

#[must_use]
pub fn offset(text: &str, position: Position) -> usize {
    let mut start = 0;
    for _ in 0..position.line {
        match text[start..].find('\n') {
            Some(at) => start += at + 1,
            None => return text.len(),
        }
    }
    let end = text[start..].find('\n').map_or(text.len(), |at| start + at);
    let mut at = (start + position.column).min(end);
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[must_use]
pub fn selection(cursor: Cursor, text: &str) -> Range<usize> {
    let caret = offset(text, cursor.position);
    let anchor = cursor.selection.map_or(caret, |anchor| offset(text, anchor));
    caret.min(anchor)..caret.max(anchor)
}

pub struct Document {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub untitled: usize,
    pub content: Content,
    pub text: String,
    pub detected: Language,
    pub pinned: Option<Language>,
    pub encoding: Encoding,
    pub bom: bool,
    pub ending: Ending,
    pub history: History,
    pub saved: u64,
    pub disk: Option<SystemTime>,
    pub changed_on_disk: bool,
    pub lines: Arc<Lines>,
    pub paint: u64,
    pub revision: u64,
    pub highlighted: u64,
    pub detect_pending: bool,
    pub draft: Option<PathBuf>,
    counted: std::cell::Cell<(u64, usize)>,
}

impl Document {
    #[must_use]
    pub fn new(id: u64, untitled: usize) -> Self {
        Self {
            id,
            path: None,
            untitled,
            content: Content::new(),
            text: String::new(),
            detected: Language::Plain,
            pinned: None,
            encoding: Encoding::Utf8,
            bom: false,
            ending: Ending::Lf,
            history: History::default(),
            saved: 0,
            disk: None,
            changed_on_disk: false,
            lines: Arc::new(vec![Vec::new()]),
            paint: 0,
            revision: 0,
            highlighted: u64::MAX,
            detect_pending: false,
            draft: None,
            counted: std::cell::Cell::new((u64::MAX, 0)),
        }
    }

    #[must_use]
    pub fn open(id: u64, path: PathBuf, decoded: Decoded) -> Self {
        let mut document = Self::new(id, 0);
        document.disk = modified(&path);
        document.detected = detect::detect(Some(&path), &decoded.text);
        document.path = Some(path);
        document.set_text(decoded.text);
        document.encoding = decoded.encoding;
        document.bom = decoded.bom;
        document.ending = decoded.ending;
        document
    }

    pub fn set_text(&mut self, text: String) {
        self.content = Content::with_text(&text);
        self.lines = Arc::new(vec![Vec::new(); text.matches('\n').count() + 1]);
        self.text = text;
        self.paint += 1;
        self.revision += 1;
    }

    #[must_use]
    pub fn language(&self) -> Language {
        self.pinned.unwrap_or(self.detected)
    }

    #[must_use]
    pub fn dirty(&self) -> bool {
        self.history.generation() != self.saved
    }

    #[must_use]
    pub fn name(&self, untitled: &str) -> String {
        match &self.path {
            Some(path) => {
                path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
            }
            None if self.untitled > 1 => format!("{untitled} {}", self.untitled),
            None => untitled.to_string(),
        }
    }

    #[must_use]
    pub fn cursor(&self) -> Cursor {
        self.content.cursor()
    }

    #[must_use]
    pub fn selected(&self) -> Range<usize> {
        selection(self.cursor(), &self.text)
    }

    pub fn perform(&mut self, action: Action) -> bool {
        let kind = match &action {
            Action::Edit(Edit::Insert(c)) if !c.is_whitespace() || *c == ' ' => Kind::Typing,
            Action::Edit(Edit::Backspace | Edit::Delete) if self.cursor().selection.is_none() => Kind::Deleting,
            Action::Edit(_) => Kind::Other,
            _ => {
                if matches!(action, Action::Click(_) | Action::Move(_) | Action::Select(_)) {
                    self.history.seal();
                }
                self.content.perform(action);
                return false;
            }
        };
        let before = self.cursor();
        self.content.perform(action);
        self.sync(before, kind)
    }

    fn sync(&mut self, before: Cursor, kind: Kind) -> bool {
        let text = mirror(&self.content);
        let after = self.cursor();
        let Some(diff) = self.history.record(&self.text, &text, before, after, kind) else { return false };
        self.shift(&diff, &text);
        self.text = text;
        self.revision += 1;
        self.detect_pending = true;
        true
    }

    fn shift(&mut self, diff: &Diff, new: &str) {
        let line = self.text[..diff.start].matches('\n').count();
        let removed = self.text[diff.start..diff.old_end].matches('\n').count();
        let added = new[diff.start..diff.new_end].matches('\n').count();
        if removed != added {
            self.lines = Arc::new(syntax::shift(&self.lines, line + 1, removed, added));
            self.paint += 1;
        }
    }

    fn select(&mut self, range: Range<usize>) {
        let start = position(&self.text, range.start);
        let end = position(&self.text, range.end);
        self.content.move_to(Cursor { position: end, selection: (range.start != range.end).then_some(start) });
    }

    pub fn place(&mut self, range: Range<usize>) {
        self.history.seal();
        self.select(range);
    }

    fn splice(&mut self, range: Range<usize>, with: &str) {
        self.select(range.clone());
        if with.is_empty() {
            if !range.is_empty() {
                self.content.perform(Action::Edit(Edit::Delete));
            }
        } else {
            self.content.perform(Action::Edit(Edit::Paste(Arc::new(with.to_string()))));
        }
    }

    pub fn replace(&mut self, range: Range<usize>, with: &str, kind: Kind) -> bool {
        let before = self.cursor();
        self.splice(range, with);
        self.sync(before, kind)
    }

    pub fn undo(&mut self) -> bool {
        let Some(step) = self.history.undo() else { return false };
        let (range, text, cursor) = (step.range, step.text.to_string(), step.cursor);
        self.restore(range, &text, cursor)
    }

    pub fn redo(&mut self) -> bool {
        let Some(step) = self.history.redo() else { return false };
        let (range, text, cursor) = (step.range, step.text.to_string(), step.cursor);
        self.restore(range, &text, cursor)
    }

    fn restore(&mut self, range: Range<usize>, with: &str, cursor: Cursor) -> bool {
        self.splice(range, with);
        let text = mirror(&self.content);
        if let Some(diff) = crate::history::diff(&self.text, &text) {
            self.shift(&diff, &text);
        }
        self.text = text;
        self.content.move_to(cursor);
        self.revision += 1;
        self.detect_pending = true;
        true
    }

    pub fn reload(&mut self, decoded: &Decoded) {
        let before = self.cursor();
        let whole = 0..self.text.len();
        self.splice(whole, &decoded.text);
        self.sync(before, Kind::Other);
        let at = offset(&self.text, before.position);
        self.select(at..at);
        self.encoding = decoded.encoding;
        self.bom = decoded.bom;
        self.ending = decoded.ending;
        self.history.seal();
        self.saved = self.history.generation();
        self.disk = self.path.as_deref().and_then(modified);
        self.changed_on_disk = false;
    }

    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        encode(&self.text, self.encoding, self.bom, self.ending)
    }

    pub fn mark_saved(&mut self, path: PathBuf, disk: SystemTime) {
        if self.path.as_ref() != Some(&path) && self.pinned.is_none() {
            self.detected = detect::detect(Some(&path), &self.text);
            self.highlighted = u64::MAX;
        }
        self.path = Some(path);
        self.history.seal();
        self.saved = self.history.generation();
        self.disk = Some(disk);
        self.changed_on_disk = false;
    }

    pub(crate) fn redetect(&mut self) -> bool {
        self.detect_pending = false;
        if self.pinned.is_some() {
            return false;
        }
        let language = detect::detect(self.path.as_deref(), &self.text);
        if language == self.detected {
            return false;
        }
        self.detected = language;
        self.highlighted = u64::MAX;
        true
    }

    pub fn install(&mut self, revision: u64, lines: Lines) {
        if revision < self.highlighted && self.highlighted != u64::MAX {
            return;
        }
        self.highlighted = revision;
        self.lines = Arc::new(lines);
        self.paint += 1;
    }

    #[must_use]
    pub fn stale(&self) -> bool {
        self.highlighted != self.revision
    }

    #[must_use]
    pub fn words(&self) -> usize {
        let (revision, words) = self.counted.get();
        if revision == self.revision {
            return words;
        }
        let words = self.text.split_whitespace().count();
        self.counted.set((self.revision, words));
        words
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoding_keeps_the_encoding_bom_and_dominant_line_ending() {
        let decoded = decode(b"one\r\ntwo\r\nthree").unwrap();
        assert_eq!(decoded.text, "one\ntwo\nthree");
        assert_eq!(decoded.ending, Ending::CrLf);
        let bom = decode("\u{feff}привіт".as_bytes()).unwrap();
        assert!(bom.bom);
        assert_eq!(bom.text, "привіт");
        let utf16 = decode(&[0xff, 0xfe, b'h', 0, b'i', 0]).unwrap();
        assert_eq!((utf16.text.as_str(), utf16.encoding), ("hi", Encoding::Utf16Le));
        let latin = decode(&[b'c', b'a', b'f', 0xe9]).unwrap();
        assert_eq!((latin.text.as_str(), latin.encoding), ("café", Encoding::Latin1));
        assert_eq!(decode(b"\x7fELF\x00\x00"), Err(LoadError::Binary));
    }

    #[test]
    fn encoding_round_trips_every_format() {
        for (encoding, bom) in
            [(Encoding::Utf8, false), (Encoding::Utf8, true), (Encoding::Utf16Le, true), (Encoding::Utf16Be, true)]
        {
            for ending in [Ending::Lf, Ending::CrLf] {
                let bytes = encode("рядок\nдругий", encoding, bom, ending);
                let back = decode(&bytes).unwrap();
                assert_eq!(back.text, "рядок\nдругий");
                assert_eq!((back.encoding, back.bom), (encoding, bom));
                assert_eq!(back.ending, ending);
            }
        }
        assert!(fits("café", Encoding::Latin1));
        assert!(!fits("кафе", Encoding::Latin1));
    }

    #[test]
    fn offsets_and_positions_agree_on_multibyte_lines() {
        let text = "аб\ncd\nїж";
        for at in [0, 2, 4, 5, 7, 8, 10, 12] {
            assert_eq!(offset(text, position(text, at)), at);
        }
        assert_eq!(position(text, 8), Position { line: 2, column: 0 });
        assert_eq!(offset(text, Position { line: 9, column: 0 }), text.len());
        assert_eq!(offset(text, Position { line: 0, column: 99 }), 4);
    }

    #[test]
    fn edits_undo_and_redo_through_the_editor_content() {
        let mut document = Document::new(1, 1);
        for c in "привіт світ".chars() {
            document.perform(Action::Edit(Edit::Insert(c)));
        }
        assert_eq!(document.text, "привіт світ");
        assert!(document.dirty());
        document.perform(Action::Edit(Edit::Enter));
        document.perform(Action::Edit(Edit::Insert('x')));
        assert_eq!(document.text, "привіт світ\nx");
        assert!(document.undo());
        assert!(document.undo());
        assert_eq!(document.text, "привіт світ");
        assert!(document.undo());
        assert_eq!(document.text, "привіт");
        assert!(document.redo());
        assert_eq!(document.text, "привіт світ");
        assert_eq!(mirror(&document.content), document.text);
    }

    #[test]
    fn replacing_a_range_and_saving_marks_it_clean() {
        let mut document = Document::new(1, 1);
        document.set_text("one two three".into());
        document.replace(4..7, "2", Kind::Other);
        document.select(4..5);
        assert_eq!(document.text, "one 2 three");
        assert_eq!(document.selected(), 4..5);
        assert!(document.dirty());
        document.mark_saved(PathBuf::from("/tmp/x.txt"), SystemTime::now());
        assert!(!document.dirty());
        document.undo();
        assert!(document.dirty());
    }

    #[test]
    fn saving_writes_through_a_temporary_file_and_keeps_the_mode() {
        use std::os::unix::fs::PermissionsExt;
        let folder = std::env::temp_dir().join(format!("glide-text-save-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("script.sh");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        write(&path, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o755);
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1, "no temporary file is left behind");
        std::fs::remove_dir_all(&folder).unwrap();
    }
}

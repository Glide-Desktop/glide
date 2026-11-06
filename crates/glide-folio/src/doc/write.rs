use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

use super::pdf::{annotation_ids, encode, note_id};
use super::{Mark, Note, Rect};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    Unreadable,
    Encrypted,
    Io(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub x0: f32,
    pub y0: f32,
    pub w: f32,
    pub h: f32,
    pub rotate: i64,
}

impl Geometry {
    #[must_use]
    pub fn to_pdf(&self, u: f32, v: f32) -> (f32, f32) {
        let (u, v) = match self.rotate.rem_euclid(360) {
            90 => (v, self.h - u),
            180 => (self.w - u, self.h - v),
            270 => (self.w - v, u),
            _ => (u, v),
        };
        (self.x0 + u, self.y0 + self.h - v)
    }

    #[must_use]
    pub fn rect(&self, rect: &Rect) -> [f32; 4] {
        let a = self.to_pdf(rect.x, rect.y);
        let b = self.to_pdf(rect.right(), rect.bottom());
        [a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)]
    }
}

fn inherited<'a>(doc: &'a Document, page: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut current = doc.get_dictionary(page).ok()?;
    for _ in 0..32 {
        if let Ok(value) = current.get(key) {
            return Some(value);
        }
        let parent = current.get(b"Parent").ok()?.as_reference().ok()?;
        current = doc.get_dictionary(parent).ok()?;
    }
    None
}

fn boxed(doc: &Document, object: Option<&Object>) -> Option<[f32; 4]> {
    let object = match object? {
        Object::Reference(id) => doc.get_object(*id).ok()?,
        other => other,
    };
    let values: Vec<f32> = object.as_array().ok()?.iter().filter_map(|v| v.as_float().ok()).collect();
    (values.len() == 4).then(|| {
        [values[0].min(values[2]), values[1].min(values[3]), values[0].max(values[2]), values[1].max(values[3])]
    })
}

#[must_use]
pub fn geometry(doc: &Document, page: ObjectId) -> Geometry {
    let media = boxed(doc, inherited(doc, page, b"MediaBox")).unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let crop = boxed(doc, inherited(doc, page, b"CropBox"))
        .map_or(media, |c| [c[0].max(media[0]), c[1].max(media[1]), c[2].min(media[2]), c[3].min(media[3])]);
    let rotate = inherited(doc, page, b"Rotate").and_then(|r| r.as_i64().ok()).unwrap_or(0);
    Geometry { x0: crop[0], y0: crop[1], w: crop[2] - crop[0], h: crop[3] - crop[1], rotate }
}

fn load(path: &Path, password: Option<&str>) -> Result<Document, WriteError> {
    let bytes = std::fs::read(path).map_err(|e| WriteError::Io(e.to_string()))?;
    let mut doc = Document::load_mem(&bytes).map_err(|_| WriteError::Unreadable)?;
    if doc.is_encrypted() || doc.trailer.get(b"Encrypt").is_ok() {
        if let Some(password) = password {
            doc.decrypt(password).map_err(|_| WriteError::Unreadable)?;
        }
        return Err(WriteError::Encrypted);
    }
    Ok(doc)
}

fn store(doc: &mut Document, target: &Path) -> Result<(), WriteError> {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).map_err(|e| WriteError::Io(e.to_string()))?;
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let partial = dir.join(format!(".{name}.folio-{}", std::process::id()));
    std::fs::write(&partial, &bytes).map_err(|e| WriteError::Io(e.to_string()))?;
    if let Ok(meta) = std::fs::metadata(target) {
        let _ = std::fs::set_permissions(&partial, meta.permissions());
    }
    std::fs::rename(&partial, target).map_err(|e| {
        let _ = std::fs::remove_file(&partial);
        WriteError::Io(e.to_string())
    })
}

fn now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil(days);
    format!("D:{y:04}{m:02}{d:02}{:02}{:02}{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn reals(values: impl IntoIterator<Item = f32>) -> Object {
    Object::Array(values.into_iter().map(Object::Real).collect())
}

fn annotation(note: &Note, geometry: &Geometry) -> Dictionary {
    let rgb = note.hue.rgb().map(|c| f32::from(c) / 255.0);
    let bounds = note
        .rects
        .iter()
        .copied()
        .reduce(|a, b| a.union(&b))
        .or_else(|| {
            let xs = note.stroke.iter().map(|p| p.0);
            let ys = note.stroke.iter().map(|p| p.1);
            let (x0, x1) = (xs.clone().fold(f32::MAX, f32::min), xs.fold(f32::MIN, f32::max));
            let (y0, y1) = (ys.clone().fold(f32::MAX, f32::min), ys.fold(f32::MIN, f32::max));
            (x0 <= x1).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
        })
        .unwrap_or_default();
    let subtype = match note.mark {
        Mark::Highlight => "Highlight",
        Mark::Underline => "Underline",
        Mark::Strike => "StrikeOut",
        Mark::Comment => "Text",
        Mark::Ink => "Ink",
        Mark::Text => "FreeText",
    };
    let mut dict = dictionary! {
        "Type" => "Annot",
        "Subtype" => subtype,
        "Rect" => reals(geometry.rect(&bounds)),
        "C" => reals(rgb),
        "F" => 4,
        "M" => Object::string_literal(now()),
        "NM" => Object::string_literal(format!("folio-{}", note.id)),
    };
    if !note.comment.is_empty() {
        dict.set("Contents", Object::String(encode(&note.comment), lopdf::StringFormat::Literal));
    }
    if !note.author.is_empty() {
        dict.set("T", Object::String(encode(&note.author), lopdf::StringFormat::Literal));
    }
    if !note.quote.is_empty() {
        dict.set("Subj", Object::String(encode(&note.quote), lopdf::StringFormat::Literal));
    }
    match note.mark {
        Mark::Highlight | Mark::Underline | Mark::Strike => {
            let quads: Vec<f32> = note
                .rects
                .iter()
                .flat_map(|rect| {
                    let [x0, y0, x1, y1] = geometry.rect(rect);
                    [x0, y1, x1, y1, x0, y0, x1, y0]
                })
                .collect();
            dict.set("QuadPoints", reals(quads));
            if note.mark == Mark::Highlight {
                dict.set("CA", Object::Real(0.45));
            }
        }
        Mark::Comment => {
            dict.set("Name", Object::Name(b"Comment".to_vec()));
            dict.set("Open", Object::Boolean(false));
        }
        Mark::Ink => {
            let points: Vec<f32> = note
                .stroke
                .iter()
                .flat_map(|(u, v)| {
                    let (x, y) = geometry.to_pdf(*u, *v);
                    [x, y]
                })
                .collect();
            dict.set("InkList", Object::Array(vec![reals(points)]));
            dict.set("BS", dictionary! { "W" => 2 });
        }
        Mark::Text => {
            dict.set("DA", Object::string_literal("/Helv 12 Tf 0 g"));
        }
    }
    dict
}

pub fn save_notes(path: &Path, target: &Path, notes: &[Note]) -> Result<(), WriteError> {
    let mut doc = load(path, None)?;
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for (index, page) in pages.iter().enumerate() {
        let geometry = geometry(&doc, *page);
        let existing = annotation_ids(&doc, *page);
        let mut keep: Vec<Object> = Vec::new();
        for id in &existing {
            let managed = doc
                .get_dictionary(*id)
                .ok()
                .and_then(|d| d.get(b"Subtype").ok())
                .and_then(|s| s.as_name().ok())
                .is_some_and(|name| super::pdf::MANAGED.contains(&name) && name != b"Popup");
            if !managed {
                keep.push(Object::Reference(*id));
                continue;
            }
            if let Some(note) = notes.iter().find(|n| n.id == note_id(*id) && n.page == index) {
                if let Ok(dict) = doc.get_dictionary_mut(*id) {
                    let rgb = note.hue.rgb().map(|c| f32::from(c) / 255.0);
                    dict.set("C", reals(rgb));
                    dict.set("Contents", Object::String(encode(&note.comment), lopdf::StringFormat::Literal));
                    dict.set("M", Object::string_literal(now()));
                }
                keep.push(Object::Reference(*id));
            }
        }
        let existing_ids: Vec<u64> = existing.iter().map(|id| note_id(*id)).collect();
        for note in notes.iter().filter(|n| n.page == index && !existing_ids.contains(&n.id)) {
            let mut dict = annotation(note, &geometry);
            dict.set("P", Object::Reference(*page));
            let id = doc.add_object(dict);
            keep.push(Object::Reference(id));
        }
        if let Ok(dict) = doc.get_dictionary_mut(*page) {
            if keep.is_empty() {
                dict.remove(b"Annots");
            } else {
                dict.set("Annots", Object::Array(keep));
            }
        }
    }
    store(&mut doc, target)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageSource {
    Here(usize),
    Other(PathBuf, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageOp {
    pub source: PageSource,
    pub rotate: i64,
}

const INHERITABLE: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

fn absorb(doc: &mut Document, other: Document) -> Vec<ObjectId> {
    let mut other = other;
    other.renumber_objects_with(doc.max_id + 1);
    let pages: Vec<ObjectId> = other.get_pages().values().copied().collect();
    flatten(&mut other, &pages);
    doc.max_id = doc.max_id.max(other.max_id);
    for (id, object) in other.objects {
        doc.objects.entry(id).or_insert(object);
    }
    pages
}

fn flatten(doc: &mut Document, pages: &[ObjectId]) {
    for page in pages {
        let mut copied: Vec<(Vec<u8>, Object)> = Vec::new();
        for key in INHERITABLE {
            let has = doc.get_dictionary(*page).is_ok_and(|d| d.has(key));
            if !has && let Some(value) = inherited(doc, *page, key) {
                copied.push((key.to_vec(), value.clone()));
            }
        }
        if let Ok(dict) = doc.get_dictionary_mut(*page) {
            for (key, value) in copied {
                dict.set(key, value);
            }
        }
    }
}

pub fn organize(path: &Path, ops: &[PageOp], target: &Path) -> Result<(), WriteError> {
    let mut doc = load(path, None)?;
    let here: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    flatten(&mut doc, &here);
    let mut others: BTreeMap<PathBuf, Vec<ObjectId>> = BTreeMap::new();
    for op in ops {
        if let PageSource::Other(other, _) = &op.source
            && !others.contains_key(other)
        {
            let loaded = load(other, None)?;
            let ids = absorb(&mut doc, loaded);
            others.insert(other.clone(), ids);
        }
    }
    let root_pages = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Pages").ok())
        .and_then(|p| p.as_reference().ok())
        .ok_or(WriteError::Unreadable)?;
    let mut kids = Vec::new();
    for op in ops {
        let id = match &op.source {
            PageSource::Here(index) => here.get(*index).copied(),
            PageSource::Other(other, index) => others.get(other).and_then(|ids| ids.get(*index)).copied(),
        };
        let Some(id) = id else {
            continue;
        };
        if let Ok(dict) = doc.get_dictionary_mut(id) {
            let current = dict.get(b"Rotate").and_then(Object::as_i64).unwrap_or(0);
            dict.set("Rotate", (current + op.rotate).rem_euclid(360));
            dict.set("Parent", Object::Reference(root_pages));
        }
        kids.push(Object::Reference(id));
    }
    if kids.is_empty() {
        return Err(WriteError::Unreadable);
    }
    let count = kids.len() as i64;
    let dict = doc.get_dictionary_mut(root_pages).map_err(|_| WriteError::Unreadable)?;
    dict.set("Kids", Object::Array(kids));
    dict.set("Count", count);
    for key in INHERITABLE {
        dict.remove(key);
    }
    doc.prune_objects();
    store(&mut doc, target)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signature {
    pub page: usize,
    pub rect: Rect,
    pub strokes: Vec<Vec<(f32, f32)>>,
}

pub fn fill(
    path: &Path, values: &[(String, String)], signature: Option<&Signature>, target: &Path,
) -> Result<(), WriteError> {
    let mut doc = load(path, None)?;
    let form = doc.catalog().ok().and_then(|c| c.get(b"AcroForm").ok()).and_then(|f| f.as_reference().ok());
    let mut fields: Vec<ObjectId> = Vec::new();
    let root_fields = match form {
        Some(id) => doc.get_dictionary(id).ok().and_then(|f| f.get(b"Fields").ok()).cloned(),
        None => doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"AcroForm").ok())
            .and_then(|f| f.as_dict().ok())
            .and_then(|f| f.get(b"Fields").ok())
            .cloned(),
    };
    let mut stack: Vec<(ObjectId, String)> = root_fields
        .and_then(|f| f.as_array().ok().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|f| f.as_reference().ok())
        .map(|id| (id, String::new()))
        .collect();
    let mut named: Vec<(ObjectId, String)> = Vec::new();
    while let Some((id, prefix)) = stack.pop() {
        let Ok(dict) = doc.get_dictionary(id) else {
            continue;
        };
        let part = match dict.get(b"T") {
            Ok(Object::String(bytes, _)) => super::pdf::decode(bytes),
            _ => String::new(),
        };
        let name = if prefix.is_empty() {
            part.clone()
        } else if part.is_empty() {
            prefix.clone()
        } else {
            format!("{prefix}.{part}")
        };
        let kids: Vec<ObjectId> = dict
            .get(b"Kids")
            .ok()
            .and_then(|k| k.as_array().ok())
            .map(|k| k.iter().filter_map(|k| k.as_reference().ok()).collect())
            .unwrap_or_default();
        let has_named_kids = kids.iter().any(|k| doc.get_dictionary(*k).is_ok_and(|d| d.has(b"T")));
        if has_named_kids {
            stack.extend(kids.into_iter().map(|k| (k, name.clone())));
        } else {
            named.push((id, name));
            fields.push(id);
        }
    }
    for (id, name) in named {
        let Some((_, value)) = values.iter().find(|(n, _)| *n == name) else {
            continue;
        };
        let kind = doc
            .get_dictionary(id)
            .ok()
            .and_then(|d| d.get(b"FT").ok())
            .and_then(|f| f.as_name().ok())
            .map(<[u8]>::to_vec);
        let kids: Vec<ObjectId> = doc
            .get_dictionary(id)
            .ok()
            .and_then(|d| d.get(b"Kids").ok())
            .and_then(|k| k.as_array().ok())
            .map(|k| k.iter().filter_map(|k| k.as_reference().ok()).collect())
            .unwrap_or_default();
        if kind.as_deref() == Some(b"Btn") {
            let on = value == "true";
            let state = if on { b"Yes".to_vec() } else { b"Off".to_vec() };
            if let Ok(dict) = doc.get_dictionary_mut(id) {
                dict.set("V", Object::Name(state.clone()));
                dict.set("AS", Object::Name(state.clone()));
            }
            for kid in kids {
                if let Ok(dict) = doc.get_dictionary_mut(kid) {
                    dict.set("AS", Object::Name(state.clone()));
                }
            }
        } else if let Ok(dict) = doc.get_dictionary_mut(id) {
            dict.set("V", Object::String(encode(value), lopdf::StringFormat::Literal));
            dict.remove(b"AP");
        }
    }
    let set_need = |dict: &mut Dictionary| dict.set("NeedAppearances", Object::Boolean(true));
    match form {
        Some(id) => {
            if let Ok(dict) = doc.get_dictionary_mut(id) {
                set_need(dict);
            }
        }
        None => {
            if let Ok(catalog) = doc.catalog_mut()
                && let Ok(Object::Dictionary(form)) = catalog.get_mut(b"AcroForm")
            {
                set_need(form);
            }
        }
    }
    if let Some(signature) = signature {
        stamp(&mut doc, signature)?;
    }
    store(&mut doc, target)
}

fn stamp(doc: &mut Document, signature: &Signature) -> Result<(), WriteError> {
    let page = *doc.get_pages().values().nth(signature.page).ok_or(WriteError::Unreadable)?;
    let geometry = geometry(doc, page);
    let bounds = geometry.rect(&signature.rect);
    let (w, h) = (bounds[2] - bounds[0], bounds[3] - bounds[1]);
    let mut ops = String::from("q 0.106 0.102 0.09 RG 1.6 w 1 J 1 j\n");
    for stroke in &signature.strokes {
        for (index, (u, v)) in stroke.iter().enumerate() {
            let (x, y) = geometry.to_pdf(*u, *v);
            let (x, y) = (x - bounds[0], y - bounds[1]);
            let _ = writeln!(ops, "{x:.2} {y:.2} {}", if index == 0 { "m" } else { "l" });
        }
        ops.push_str("S\n");
    }
    ops.push_str("Q\n");
    let form = Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => reals([0.0, 0.0, w, h]) },
        ops.into_bytes(),
    );
    let form = doc.add_object(form);
    let annot = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Stamp",
        "Rect" => reals(bounds),
        "F" => 4,
        "Name" => "Signature",
        "M" => Object::string_literal(now()),
        "AP" => dictionary! { "N" => form },
        "P" => page,
    });
    let mut annots: Vec<Object> = annotation_ids(doc, page).into_iter().map(Object::Reference).collect();
    annots.push(Object::Reference(annot));
    doc.get_dictionary_mut(page).map_err(|_| WriteError::Unreadable)?.set("Annots", Object::Array(annots));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::pdf::{PdfEngine, tests::sample};
    use super::super::{Engine, Format, Hue};
    use super::*;

    fn temp(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glide-folio-write-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.pdf");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn rotated_geometry_round_trips_through_the_renderer() {
        for rotate in [0, 90, 180, 270] {
            let geometry = Geometry { x0: 10.0, y0: 20.0, w: 600.0, h: 800.0, rotate };
            let (x, y) = geometry.to_pdf(0.0, 0.0);
            assert!(x >= 10.0 && y >= 20.0 && x <= 610.0 && y <= 820.0, "{rotate}: {x} {y}");
        }
        let top_left = Geometry { x0: 0.0, y0: 0.0, w: 612.0, h: 792.0, rotate: 0 }.to_pdf(0.0, 0.0);
        assert_eq!(top_left, (0.0, 792.0));
    }

    #[test]
    fn new_notes_are_written_as_standard_annotations_and_read_back() {
        let path = temp("notes", &sample(2));
        let mut engine = PdfEngine::open(&path, None, Format::Pdf, None).unwrap();
        let text = engine.text(1);
        let rect = text.words[0].rect;
        let mut notes = engine.opened().notes.clone();
        notes[0].comment = "changed".into();
        notes.push(Note {
            id: 7,
            page: 1,
            mark: Mark::Underline,
            hue: Hue::Blue,
            rects: vec![rect],
            quote: "Workspaces".into(),
            comment: String::new(),
            author: "Ви".into(),
            modified: String::new(),
            stroke: Vec::new(),
        });
        save_notes(&path, &path, &notes).unwrap();
        let reopened = PdfEngine::open(&path, None, Format::Pdf, None).unwrap();
        let read = &reopened.opened().notes;
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].comment, "changed");
        let underline = read.iter().find(|n| n.mark == Mark::Underline).unwrap();
        assert_eq!(underline.page, 1);
        assert_eq!(underline.hue, Hue::Blue);
        assert_eq!(underline.author, "Ви");
        assert!((underline.rects[0].x - rect.x).abs() < 0.5 && (underline.rects[0].y - rect.y).abs() < 0.5);
        save_notes(&path, &path, &[]).unwrap();
        assert!(PdfEngine::open(&path, None, Format::Pdf, None).unwrap().opened().notes.is_empty());
        assert_eq!(PdfEngine::open(&path, None, Format::Pdf, None).unwrap().links(0).len(), 1);
    }

    #[test]
    fn the_organiser_reorders_rotates_drops_and_inserts_pages() {
        let path = temp("organise", &sample(3));
        let other = temp("organise-other", &sample(2));
        let target = path.with_file_name("copy.pdf");
        let ops = vec![
            PageOp { source: PageSource::Here(2), rotate: 90 },
            PageOp { source: PageSource::Other(other, 1), rotate: 0 },
            PageOp { source: PageSource::Here(0), rotate: 0 },
        ];
        organize(&path, &ops, &target).unwrap();
        let mut copy = PdfEngine::open(&target, None, Format::Pdf, None).unwrap();
        assert_eq!(copy.opened().pages.len(), 3);
        assert_eq!(copy.opened().pages[0], super::super::Size { w: 792.0, h: 612.0 });
        assert_eq!(copy.text(0).plain(), "Workspaces page 3");
        assert_eq!(copy.text(1).plain(), "Workspaces page 2");
        assert_eq!(copy.text(2).plain(), "Workspaces page 1");
        assert_eq!(PdfEngine::open(&path, None, Format::Pdf, None).unwrap().opened().pages.len(), 3);
    }

    #[test]
    fn a_drawn_signature_becomes_a_stamp_with_its_own_appearance() {
        let path = temp("sign", &sample(1));
        let signature = Signature {
            page: 0,
            rect: Rect::new(100.0, 600.0, 200.0, 60.0),
            strokes: vec![vec![(110.0, 640.0), (200.0, 620.0), (280.0, 650.0)]],
        };
        fill(&path, &[], Some(&signature), &path).unwrap();
        let doc = Document::load(&path).unwrap();
        let page = *doc.get_pages().values().next().unwrap();
        let stamps = annotation_ids(&doc, page)
            .into_iter()
            .filter(|id| {
                doc.get_dictionary(*id).ok().and_then(|d| d.get(b"Subtype").ok()).and_then(|s| s.as_name().ok())
                    == Some(b"Stamp")
            })
            .count();
        assert_eq!(stamps, 1);
        let mut engine = PdfEngine::open(&path, None, Format::Pdf, None).unwrap();
        let image = engine.render(0, 1.0).unwrap();
        assert!(
            image.get_pixel(200, 630).0[0] < 250
                || image.get_pixel(201, 628).0[0] < 250
                || image.pixels().filter(|p| p.0[0] < 200).count() > 50
        );
    }
}

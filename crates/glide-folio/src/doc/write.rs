use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;
use std::path::{Path, PathBuf};

use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream, dictionary};

use super::pdf::{SIGNATURE_STAMP, encode, note_id};
use super::{Hue, Mark, Note, Rect};

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

fn load_bytes(path: &Path) -> Result<(Vec<u8>, Document), WriteError> {
    let bytes = std::fs::read(path).map_err(|e| WriteError::Io(e.to_string()))?;
    let doc = Document::load_mem(&bytes).map_err(|_| WriteError::Unreadable)?;
    if doc.is_encrypted() || doc.trailer.get(b"Encrypt").is_ok() {
        return Err(WriteError::Encrypted);
    }
    Ok((bytes, doc))
}

fn load(path: &Path) -> Result<Document, WriteError> {
    load_bytes(path).map(|(_, doc)| doc)
}

fn store(doc: &mut Document, target: &Path) -> Result<(), WriteError> {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).map_err(|e| WriteError::Io(e.to_string()))?;
    write_file(&bytes, target)
}

fn write_file(bytes: &[u8], target: &Path) -> Result<(), WriteError> {
    use std::io::Write as _;
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let partial = dir.join(format!(".{name}.folio-{}", std::process::id()));
    let written = std::fs::File::create(&partial).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    if let Err(error) = written {
        let _ = std::fs::remove_file(&partial);
        return Err(WriteError::Io(error.to_string()));
    }
    if let Ok(meta) = std::fs::metadata(target) {
        let _ = std::fs::set_permissions(&partial, meta.permissions());
    }
    std::fs::rename(&partial, target).map_err(|e| {
        let _ = std::fs::remove_file(&partial);
        WriteError::Io(e.to_string())
    })
}

fn update(
    path: &Path, target: &Path, edit: impl FnOnce(&mut Document) -> Result<(), WriteError>,
) -> Result<(), WriteError> {
    let (bytes, previous) = load_bytes(path)?;
    let mut doc = previous.clone();
    edit(&mut doc)?;
    if previous.xref_start == 0 {
        return store(&mut doc, target);
    }
    let mut revision = IncrementalDocument::create_from(bytes, previous);
    let changed: Vec<(ObjectId, Object)> = doc
        .objects
        .into_iter()
        .filter(|(id, object)| revision.get_prev_documents().objects.get(id) != Some(object))
        .collect();
    for (id, object) in changed {
        revision.new_document.set_object(id, object);
    }
    revision.new_document.max_id = revision.new_document.max_id.max(doc.max_id);
    let mut out = Vec::new();
    revision.save_to(&mut out).map_err(|e| WriteError::Io(e.to_string()))?;
    write_file(&out, target)
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
            let xs = note.strokes.iter().flatten().map(|p| p.0);
            let ys = note.strokes.iter().flatten().map(|p| p.1);
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
            let strokes: Vec<Object> = note
                .strokes
                .iter()
                .map(|stroke| {
                    reals(stroke.iter().flat_map(|(u, v)| {
                        let (x, y) = geometry.to_pdf(*u, *v);
                        [x, y]
                    }))
                })
                .collect();
            dict.set("InkList", Object::Array(strokes));
            dict.set("BS", dictionary! { "W" => 2 });
        }
        Mark::Text => {
            dict.set("DA", Object::string_literal("/Helv 12 Tf 0 g"));
        }
    }
    dict
}

pub fn save_notes(path: &Path, target: &Path, notes: &[Note]) -> Result<(), WriteError> {
    update(path, target, |doc| {
        write_notes(doc, notes);
        Ok(())
    })
}

fn write_notes(doc: &mut Document, notes: &[Note]) {
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for (index, page) in pages.iter().enumerate() {
        let geometry = geometry(doc, *page);
        let (holder, raw) = annotations_of(doc, *page);
        let existing: Vec<ObjectId> = raw.iter().filter_map(|item| item.as_reference().ok()).collect();
        let mut keep: Vec<Object> = raw.iter().filter(|item| item.as_reference().is_err()).cloned().collect();
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
                    let rgb = reals(note.hue.rgb().map(|c| f32::from(c) / 255.0));
                    let contents = Object::String(encode(&note.comment), lopdf::StringFormat::Literal);
                    let same_colour = match dict.get(b"C").and_then(Object::as_array) {
                        Ok(c) => {
                            let c: Vec<f32> = c.iter().filter_map(|v| v.as_float().ok()).collect();
                            c.len() == 3 && Hue::nearest([c[0], c[1], c[2]]) == note.hue
                        }
                        Err(_) => note.hue == Hue::Yellow,
                    };
                    let same_text = match dict.get(b"Contents") {
                        Ok(Object::String(bytes, _)) => super::pdf::decode(bytes) == note.comment,
                        _ => note.comment.is_empty(),
                    };
                    if !same_colour || !same_text {
                        dict.set("C", rgb);
                        dict.set("Contents", contents);
                        dict.set("M", Object::string_literal(now()));
                    }
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
        let unchanged = keep.len() == raw.len() && keep.iter().all(|item| raw.contains(item));
        if unchanged {
            continue;
        }
        match holder {
            Some(array) => {
                doc.objects.insert(array, Object::Array(keep));
            }
            None => {
                if let Ok(dict) = doc.get_dictionary_mut(*page) {
                    if keep.is_empty() {
                        dict.remove(b"Annots");
                    } else {
                        dict.set("Annots", Object::Array(keep));
                    }
                }
            }
        }
    }
}

fn annotations_of(doc: &Document, page: ObjectId) -> (Option<ObjectId>, Vec<Object>) {
    let Ok(dict) = doc.get_dictionary(page) else {
        return (None, Vec::new());
    };
    match dict.get(b"Annots") {
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Array(items)) => (Some(*id), items.clone()),
            _ => (None, Vec::new()),
        },
        Ok(Object::Array(items)) => (None, items.clone()),
        _ => (None, Vec::new()),
    }
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

struct Imported {
    ids: std::ops::RangeInclusive<u32>,
    resources: Option<Dictionary>,
    appearance: Option<Object>,
}

fn resolved_dict(doc: &Document, object: Option<&Object>) -> Option<Dictionary> {
    match object? {
        Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
        Object::Dictionary(dict) => Some(dict.clone()),
        _ => None,
    }
}

fn absorb(doc: &mut Document, other: Document) -> (Vec<ObjectId>, Imported) {
    let mut other = other;
    let first = doc.max_id + 1;
    other.renumber_objects_with(first);
    let pages: Vec<ObjectId> = other.get_pages().values().copied().collect();
    flatten(&mut other, &pages);
    let form = resolved_dict(&other, other.catalog().ok().and_then(|c| c.get(b"AcroForm").ok()));
    let imported = Imported {
        ids: first..=other.max_id.max(first),
        resources: form.as_ref().and_then(|f| resolved_dict(&other, f.get(b"DR").ok())),
        appearance: form.as_ref().and_then(|f| f.get(b"DA").ok()).cloned(),
    };
    doc.max_id = doc.max_id.max(other.max_id);
    for (id, object) in other.objects {
        doc.objects.entry(id).or_insert(object);
    }
    (pages, imported)
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
    let mut doc = load(path)?;
    let here: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    flatten(&mut doc, &here);
    let mut others: BTreeMap<PathBuf, Vec<ObjectId>> = BTreeMap::new();
    let mut imported: Vec<Imported> = Vec::new();
    for op in ops {
        if let PageSource::Other(other, _) = &op.source
            && !others.contains_key(other)
        {
            let loaded = load(other)?;
            let (ids, form) = absorb(&mut doc, loaded);
            others.insert(other.clone(), ids);
            imported.push(form);
        }
    }
    let root_pages = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Pages").ok())
        .and_then(|p| p.as_reference().ok())
        .ok_or(WriteError::Unreadable)?;
    let mut kids = Vec::new();
    let mut used = HashSet::new();
    for op in ops {
        let id = match &op.source {
            PageSource::Here(index) => here.get(*index).copied(),
            PageSource::Other(other, index) => others.get(other).and_then(|ids| ids.get(*index)).copied(),
        };
        let Some(mut id) = id else {
            continue;
        };
        if !used.insert(id) {
            let Ok(copy) = doc.get_dictionary(id).cloned() else {
                continue;
            };
            id = doc.add_object(copy);
        }
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
    let kept: Vec<ObjectId> = kids.iter().filter_map(|kid| kid.as_reference().ok()).collect();
    let dict = doc.get_dictionary_mut(root_pages).map_err(|_| WriteError::Unreadable)?;
    dict.set("Kids", Object::Array(kids));
    dict.set("Count", count);
    for key in INHERITABLE {
        dict.remove(key);
    }
    let gone: HashSet<ObjectId> =
        here.iter().chain(others.values().flatten()).filter(|page| !kept.contains(page)).copied().collect();
    forget_pages(&mut doc, &gone);
    rebuild_fields(&mut doc, &kept, &imported);
    doc.prune_objects();
    store(&mut doc, target)
}

fn forget_pages(doc: &mut Document, gone: &HashSet<ObjectId>) {
    fn scrub(object: &mut Object, gone: &HashSet<ObjectId>) {
        match object {
            Object::Reference(id) if gone.contains(id) => *object = Object::Null,
            Object::Array(items) => items.iter_mut().for_each(|item| scrub(item, gone)),
            Object::Dictionary(dict) => dict.iter_mut().for_each(|(_, value)| scrub(value, gone)),
            Object::Stream(stream) => stream.dict.iter_mut().for_each(|(_, value)| scrub(value, gone)),
            _ => {}
        }
    }
    if gone.is_empty() {
        return;
    }
    for (id, object) in &mut doc.objects {
        if !gone.contains(id) {
            scrub(object, gone);
        }
    }
    let mut trailer = std::mem::take(&mut doc.trailer);
    trailer.iter_mut().for_each(|(_, value)| scrub(value, gone));
    doc.trailer = trailer;
}

fn is_widget(dict: &Dictionary) -> bool {
    dict.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Widget")
}

fn kid_ids(dict: &Dictionary) -> Option<Vec<ObjectId>> {
    kid_ids_of(dict, b"Kids")
}

fn prune_field(
    doc: &mut Document, id: ObjectId, placed: &HashSet<ObjectId>, visited: &mut HashSet<ObjectId>, depth: u8,
) -> bool {
    if depth > 32 || !visited.insert(id) {
        return false;
    }
    let Ok(dict) = doc.get_dictionary(id) else {
        return false;
    };
    let widget = is_widget(dict);
    let Some(kids) = kid_ids(dict).filter(|kids| !kids.is_empty()) else {
        return !widget || placed.contains(&id);
    };
    let kept: Vec<ObjectId> =
        kids.iter().copied().filter(|kid| prune_field(doc, *kid, placed, visited, depth + 1)).collect();
    if kept.is_empty() {
        return false;
    }
    if kept.len() != kids.len()
        && let Ok(dict) = doc.get_dictionary_mut(id)
    {
        dict.set("Kids", Object::Array(kept.into_iter().map(Object::Reference).collect()));
    }
    true
}

fn field_root(doc: &Document, widget: ObjectId) -> ObjectId {
    let mut current = widget;
    for _ in 0..32 {
        let parent =
            doc.get_dictionary(current).ok().and_then(|d| d.get(b"Parent").ok()).and_then(|p| p.as_reference().ok());
        match parent {
            Some(parent) if doc.get_dictionary(parent).is_ok() => current = parent,
            _ => break,
        }
    }
    current
}

fn field_name(doc: &Document, field: ObjectId) -> String {
    match doc.get_dictionary(field).ok().and_then(|d| d.get(b"T").ok()) {
        Some(Object::String(bytes, _)) => super::pdf::decode(bytes),
        _ => String::new(),
    }
}

fn adopt(doc: &mut Document, root: ObjectId, source: &Imported, names: &mut HashSet<String>) {
    let name = field_name(doc, root);
    let Ok(dict) = doc.get_dictionary_mut(root) else {
        return;
    };
    if !dict.has(b"DA")
        && let Some(appearance) = &source.appearance
    {
        dict.set("DA", appearance.clone());
    }
    if name.is_empty() {
        return;
    }
    let mut unique = name.clone();
    let mut n = 2;
    while names.contains(&unique) {
        unique = format!("{name} ({n})");
        n += 1;
    }
    if unique != name {
        dict.set("T", Object::String(encode(&unique), lopdf::StringFormat::Literal));
    }
    names.insert(unique);
}

fn merge_resources(doc: &Document, form: &mut Dictionary, imported: &[Imported]) {
    if imported.iter().all(|source| source.resources.is_none()) {
        return;
    }
    let mut resources = resolved_dict(doc, form.get(b"DR").ok()).unwrap_or_default();
    for extra in imported.iter().filter_map(|source| source.resources.as_ref()) {
        for (category, value) in extra {
            let Some(entries) = resolved_dict(doc, Some(value)) else {
                continue;
            };
            let mut current = resolved_dict(doc, resources.get(category).ok()).unwrap_or_default();
            for (name, entry) in &entries {
                if !current.has(name) {
                    current.set(name.clone(), entry.clone());
                }
            }
            resources.set(category.clone(), Object::Dictionary(current));
        }
    }
    form.set("DR", Object::Dictionary(resources));
}

fn rebuild_fields(doc: &mut Document, kept: &[ObjectId], imported: &[Imported]) {
    let placed: HashSet<ObjectId> = kept
        .iter()
        .flat_map(|page| annotations_of(doc, *page).1)
        .filter_map(|item| item.as_reference().ok())
        .filter(|id| doc.get_dictionary(*id).is_ok_and(is_widget))
        .collect();
    let form = doc.catalog().ok().and_then(|c| c.get(b"AcroForm").ok()).cloned();
    let form_id = form.as_ref().and_then(|f| f.as_reference().ok());
    let form_dict = match (&form, form_id) {
        (_, Some(id)) => doc.get_dictionary(id).ok().cloned(),
        (Some(Object::Dictionary(dict)), None) => Some(dict.clone()),
        _ => None,
    };
    if form_dict.is_none() && placed.is_empty() {
        return;
    }
    let roots: Vec<ObjectId> = match form_dict.as_ref().and_then(|f| f.get(b"Fields").ok()) {
        Some(Object::Reference(id)) => doc
            .get_object(*id)
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|items| items.iter().filter_map(|item| item.as_reference().ok()).collect())
            .unwrap_or_default(),
        Some(Object::Array(items)) => items.iter().filter_map(|item| item.as_reference().ok()).collect(),
        _ => Vec::new(),
    };
    let mut visited = HashSet::new();
    let mut fields: Vec<ObjectId> = Vec::new();
    for root in roots {
        if prune_field(doc, root, &placed, &mut visited, 0) {
            fields.push(root);
        }
    }
    let mut names: HashSet<String> = fields.iter().map(|field| field_name(doc, *field)).collect();
    let mut extra: Vec<ObjectId> = placed.iter().map(|widget| field_root(doc, *widget)).collect();
    extra.sort_unstable();
    extra.dedup();
    for root in extra {
        if !fields.contains(&root) && prune_field(doc, root, &placed, &mut visited, 0) {
            if let Some(source) = imported.iter().find(|source| source.ids.contains(&root.0)) {
                adopt(doc, root, source, &mut names);
            }
            fields.push(root);
        }
    }
    let mut dict = form_dict.unwrap_or_default();
    merge_resources(doc, &mut dict, imported);
    dict.set("Fields", Object::Array(fields.into_iter().map(Object::Reference).collect()));
    match form_id {
        Some(id) => {
            doc.objects.insert(id, Object::Dictionary(dict));
        }
        None => {
            if let Ok(catalog) = doc.catalog_mut() {
                catalog.set("AcroForm", Object::Dictionary(dict));
            }
        }
    }
}

fn kid_ids_of(dict: &Dictionary, key: &[u8]) -> Option<Vec<ObjectId>> {
    dict.get(key).ok().and_then(|k| k.as_array().ok()).map(|k| k.iter().filter_map(|k| k.as_reference().ok()).collect())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signature {
    pub field: Option<String>,
    pub page: usize,
    pub rect: Rect,
    pub strokes: Vec<Vec<(f32, f32)>>,
}

pub fn fill(
    path: &Path, values: &[(String, String)], signatures: &[Signature], notes: Option<&[Note]>, target: &Path,
) -> Result<(), WriteError> {
    update(path, target, |doc| {
        if let Some(notes) = notes {
            write_notes(doc, notes);
        }
        let (form, named) = named_fields(doc);
        if set_values(doc, &named, values) {
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
        }
        for signature in signatures {
            stamp(doc, signature)?;
        }
        Ok(())
    })
}

fn named_fields(doc: &Document) -> (Option<ObjectId>, Vec<(ObjectId, String)>) {
    let form = doc.catalog().ok().and_then(|c| c.get(b"AcroForm").ok()).and_then(|f| f.as_reference().ok());
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
        }
    }
    (form, named)
}

fn on_states(doc: &Document, widget: ObjectId) -> Vec<Vec<u8>> {
    let resolve = |object: &Object| match object {
        Object::Reference(id) => doc.get_object(*id).ok().cloned(),
        other => Some(other.clone()),
    };
    doc.get_dictionary(widget)
        .ok()
        .and_then(|w| w.get(b"AP").ok())
        .and_then(resolve)
        .and_then(|ap| ap.as_dict().ok().and_then(|ap| ap.get(b"N").ok()).and_then(resolve))
        .and_then(|n| n.as_dict().ok().map(|n| n.iter().map(|(state, _)| state.clone()).collect::<Vec<_>>()))
        .unwrap_or_default()
        .into_iter()
        .filter(|state| state != b"Off")
        .collect()
}

type WidgetState = (ObjectId, Vec<u8>);

fn button_states(
    doc: &Document, widgets: &[ObjectId], radio: bool, value: &str,
) -> Option<(Vec<u8>, Vec<WidgetState>)> {
    let off = b"Off".to_vec();
    if radio {
        let chosen = value.as_bytes().to_vec();
        let picked = !value.is_empty() && value != "Off";
        let offered: Vec<Vec<Vec<u8>>> = widgets.iter().map(|w| on_states(doc, *w)).collect();
        if picked && !offered.iter().any(|states| states.contains(&chosen)) {
            return None;
        }
        let states = widgets
            .iter()
            .zip(&offered)
            .map(|(w, states)| (*w, if picked && states.contains(&chosen) { chosen.clone() } else { off.clone() }))
            .collect();
        return Some((if picked { chosen } else { off }, states));
    }
    let on = value == "true";
    let state_of = |w: ObjectId| {
        if on { on_states(doc, w).into_iter().next().unwrap_or_else(|| b"Yes".to_vec()) } else { off.clone() }
    };
    let states: Vec<(ObjectId, Vec<u8>)> = widgets.iter().map(|w| (*w, state_of(*w))).collect();
    let field = states.first().map_or_else(|| if on { b"Yes".to_vec() } else { off.clone() }, |(_, s)| s.clone());
    Some((field, states))
}

fn set_values(doc: &mut Document, named: &[(ObjectId, String)], values: &[(String, String)]) -> bool {
    let mut changed = false;
    for (id, name) in named {
        let id = *id;
        let Some((_, value)) = values.iter().find(|(n, _)| n == name) else {
            continue;
        };
        let kind = inherited(doc, id, b"FT").and_then(|f| f.as_name().ok()).map(<[u8]>::to_vec);
        let flags = inherited(doc, id, b"Ff").and_then(|f| f.as_i64().ok()).unwrap_or(0);
        let current = doc.get_dictionary(id).ok().and_then(|d| d.get(b"V").ok()).cloned();
        let kids: Vec<ObjectId> = doc
            .get_dictionary(id)
            .ok()
            .and_then(|d| d.get(b"Kids").ok())
            .and_then(|k| k.as_array().ok())
            .map(|k| k.iter().filter_map(|k| k.as_reference().ok()).collect())
            .unwrap_or_default();
        if kind.as_deref() == Some(b"Btn") {
            let widgets = if kids.is_empty() { vec![id] } else { kids };
            let Some((state, states)) = button_states(doc, &widgets, flags & (1 << 15) != 0, value) else {
                continue;
            };
            let was = match &current {
                Some(Object::Name(name)) => name.clone(),
                _ => b"Off".to_vec(),
            };
            if was == state {
                continue;
            }
            changed = true;
            if let Ok(dict) = doc.get_dictionary_mut(id) {
                dict.set("V", Object::Name(state));
            }
            for (widget, state) in states {
                if let Ok(dict) = doc.get_dictionary_mut(widget) {
                    dict.set("AS", Object::Name(state));
                }
            }
        } else {
            let was = match &current {
                Some(Object::String(bytes, _)) => super::pdf::decode(bytes),
                _ => String::new(),
            };
            if was == *value {
                continue;
            }
            changed = true;
            if let Ok(dict) = doc.get_dictionary_mut(id) {
                dict.set("V", Object::String(encode(value), lopdf::StringFormat::Literal));
                dict.remove(b"AP");
            }
            for kid in kids {
                if let Ok(dict) = doc.get_dictionary_mut(kid) {
                    dict.remove(b"AP");
                }
            }
        }
    }
    changed
}

fn stamp(doc: &mut Document, signature: &Signature) -> Result<(), WriteError> {
    let page = *doc.get_pages().values().nth(signature.page).ok_or(WriteError::Unreadable)?;
    let marker = signature.field.as_ref().map(|field| format!("{SIGNATURE_STAMP}{field}"));
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
    if let (Some(marker), Ok(dict)) = (&marker, doc.get_dictionary_mut(annot)) {
        dict.set("NM", Object::String(encode(marker), lopdf::StringFormat::Literal));
    }
    let earlier = |item: &Object| {
        let Ok(id) = item.as_reference() else {
            return false;
        };
        doc.get_dictionary(id).ok().and_then(|d| d.get(b"NM").ok()).is_some_and(|nm| match nm {
            Object::String(bytes, _) => marker.as_ref().is_some_and(|m| super::pdf::decode(bytes) == *m),
            _ => false,
        })
    };
    let (holder, raw) = annotations_of(doc, page);
    let mut annots: Vec<Object> = raw.into_iter().filter(|item| !earlier(item)).collect();
    annots.push(Object::Reference(annot));
    match holder {
        Some(array) => {
            doc.objects.insert(array, Object::Array(annots));
        }
        None => {
            doc.get_dictionary_mut(page).map_err(|_| WriteError::Unreadable)?.set("Annots", Object::Array(annots));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::pdf::{PdfEngine, annotation_ids, tests::sample};
    use super::super::{Engine, Format, Hue};
    use super::*;

    fn temp(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glide-folio-write-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.pdf");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn appearance(doc: &mut Document, states: &[&str]) -> Object {
        let form = doc.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 10.into(), 10.into()] },
            Vec::new(),
        ));
        let mut normal = Dictionary::new();
        for state in states {
            normal.set(*state, form);
        }
        Object::Dictionary(dictionary! { "N" => normal })
    }

    fn form_sample() -> Vec<u8> {
        let mut doc = Document::load_mem(&sample(2)).unwrap();
        let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
        let check_ap = appearance(&mut doc, &["On", "Off"]);
        let check = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "T" => Object::string_literal("agree"),
            "Rect" => vec![72.into(), 100.into(), 92.into(), 120.into()],
            "V" => "Off", "AS" => "Off", "AP" => check_ap,
        });
        let radio = doc.new_object_id();
        let mut kids = Vec::new();
        for (state, x) in [("A", 72), ("B", 120)] {
            let ap = appearance(&mut doc, &[state, "Off"]);
            kids.push(Object::Reference(doc.add_object(dictionary! {
                "Type" => "Annot", "Subtype" => "Widget", "Parent" => radio, "P" => pages[0],
                "Rect" => vec![x.into(), 200.into(), (x + 20).into(), 220.into()],
                "AS" => "Off", "AP" => ap,
            })));
        }
        doc.objects.insert(
            radio,
            Object::Dictionary(dictionary! {
                "FT" => "Btn", "Ff" => 1i64 << 15, "T" => Object::string_literal("size"), "V" => "Off", "Kids" => kids.clone(),
            }),
        );
        let sign = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Sig", "T" => Object::string_literal("sign"),
            "Rect" => vec![300.into(), 100.into(), 500.into(), 160.into()],
        });
        let fruit = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Ch", "T" => Object::string_literal("fruit"),
            "Rect" => vec![72.into(), 300.into(), 200.into(), 320.into()],
            "Opt" => vec![
                Object::Array(vec![Object::string_literal("a"), Object::string_literal("Apple")]),
                Object::string_literal("Pear"),
            ],
            "V" => Object::string_literal("a"),
        });
        doc.get_dictionary_mut(pages[0]).unwrap().set("Annots", kids);
        doc.get_dictionary_mut(pages[1]).unwrap().set("Annots", vec![check.into(), sign.into(), fruit.into()]);
        let form = doc.add_object(dictionary! {
            "Fields" => vec![check.into(), radio.into(), sign.into(), fruit.into()],
            "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
        });
        doc.catalog_mut().unwrap().set("AcroForm", form);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn name(doc: &Document, id: ObjectId, key: &[u8]) -> Vec<u8> {
        doc.get_dictionary(id).unwrap().get(key).unwrap().as_name().unwrap().to_vec()
    }

    #[test]
    fn fields_without_a_page_link_land_on_the_page_that_lists_them_and_radios_offer_their_states() {
        let path = temp("fields", &form_sample());
        let engine = PdfEngine::open(&path, None, Format::Pdf, None).unwrap();
        let fields = &engine.opened().fields;
        let find = |name: &str| fields.iter().find(|f| f.name == name).unwrap();
        assert_eq!(find("agree").page, 1);
        assert_eq!(find("sign").page, 1);
        let super::super::FieldKind::Radio(states) = &find("size").kind else { panic!("a radio group") };
        let names: Vec<&str> = states.iter().map(|(state, _)| state.as_str()).collect();
        assert_eq!(names, ["A", "B"]);
        assert!(states[1].1.x > states[0].1.x + 30.0, "each state sits over its own button");
        assert_eq!(find("size").value, "");
        let super::super::FieldKind::Choice(options) = &find("fruit").kind else { panic!("a choice") };
        assert_eq!((options[0].value.as_str(), options[0].label.as_str()), ("a", "Apple"));
        assert_eq!((options[1].value.as_str(), options[1].label.as_str()), ("Pear", "Pear"));
        assert_eq!(find("fruit").value, "a", "the stored value is the export value");
    }

    #[test]
    fn filling_keeps_the_original_bytes_and_uses_each_widgets_own_states() {
        let original = form_sample();
        let path = temp("fill", &original);
        let values = vec![("agree".to_string(), "true".to_string()), ("size".to_string(), "B".to_string())];
        fill(&path, &values, &[], None, &path).unwrap();
        let saved = std::fs::read(&path).unwrap();
        assert!(saved.starts_with(&original), "an incremental update appends to the signed bytes");
        let doc = Document::load(&path).unwrap();
        let (_, named) = named_fields(&doc);
        let id = |n: &str| named.iter().find(|(_, name)| name == n).unwrap().0;
        assert_eq!(name(&doc, id("agree"), b"V"), b"On");
        assert_eq!(name(&doc, id("agree"), b"AS"), b"On");
        assert_eq!(name(&doc, id("size"), b"V"), b"B");
        let kids: Vec<ObjectId> = doc
            .get_dictionary(id("size"))
            .unwrap()
            .get(b"Kids")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_reference().unwrap())
            .collect();
        assert_eq!(name(&doc, kids[0], b"AS"), b"Off");
        assert_eq!(name(&doc, kids[1], b"AS"), b"B");
        let mut again = doc;
        assert!(!set_values(&mut again, &named, &values), "unchanged values leave the fields alone");
    }

    #[test]
    fn dropping_a_page_drops_its_fields_and_the_page_itself() {
        let path = temp("organise-fields", &form_sample());
        let original = Document::load(&path).unwrap();
        let first = *original.get_pages().values().next().unwrap();
        let target = path.with_file_name("kept.pdf");
        organize(&path, &[PageOp { source: PageSource::Here(1), rotate: 0 }], &target).unwrap();
        let doc = Document::load(&target).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
        assert!(doc.get_object(first).is_err(), "the dropped page is gone from the file");
        let (_, fields) = named_fields(&doc);
        let names: Vec<&str> = fields.iter().map(|(_, name)| name.as_str()).collect();
        assert!(names.contains(&"agree") && names.contains(&"sign"), "{names:?}");
        assert!(!names.contains(&"size"), "the radio group lived on the dropped page: {names:?}");
        let engine = PdfEngine::open(&target, None, Format::Pdf, None).unwrap();
        assert!(engine.opened().fields.iter().all(|field| field.page == 0));
    }

    #[test]
    fn fields_of_an_inserted_page_join_the_form_under_their_own_names() {
        let path = temp("organise-join", &form_sample());
        let other = temp("organise-join-other", &form_sample());
        let target = path.with_file_name("joined.pdf");
        let ops = [
            PageOp { source: PageSource::Here(1), rotate: 0 },
            PageOp { source: PageSource::Other(other, 1), rotate: 0 },
        ];
        organize(&path, &ops, &target).unwrap();
        let doc = Document::load(&target).unwrap();
        let (_, fields) = named_fields(&doc);
        let names: Vec<&str> = fields.iter().map(|(_, name)| name.as_str()).collect();
        assert!(names.contains(&"agree") && names.contains(&"agree (2)"), "{names:?}");
        let (id, _) = fields.iter().find(|(_, name)| name == "agree (2)").unwrap();
        assert!(doc.get_dictionary(*id).unwrap().has(b"DA"), "the inserted field keeps its default appearance");
    }

    #[test]
    fn signatures_placed_anywhere_add_up_instead_of_replacing_each_other() {
        let path = temp("free-sign", &sample(1));
        let signature = |x: f32| Signature {
            field: None,
            page: 0,
            rect: Rect::new(x, 600.0, 120.0, 40.0),
            strokes: vec![vec![(x + 10.0, 610.0), (x + 100.0, 630.0)]],
        };
        fill(&path, &[], &[signature(100.0)], None, &path).unwrap();
        fill(&path, &[], &[signature(300.0)], None, &path).unwrap();
        let doc = Document::load(&path).unwrap();
        let page = *doc.get_pages().values().next().unwrap();
        let stamps = annotation_ids(&doc, page)
            .into_iter()
            .filter(|id| {
                doc.get_dictionary(*id).ok().and_then(|d| d.get(b"Subtype").ok()).and_then(|s| s.as_name().ok())
                    == Some(b"Stamp")
            })
            .count();
        assert_eq!(stamps, 2);
    }

    #[test]
    fn a_placed_signature_is_known_as_stamped_after_reopening() {
        let path = temp("stamped", &form_sample());
        let signature = Signature {
            field: Some("sign".into()),
            page: 1,
            rect: Rect::new(300.0, 632.0, 200.0, 60.0),
            strokes: vec![vec![(310.0, 650.0), (400.0, 640.0)]],
        };
        fill(&path, &[], &[signature], None, &path).unwrap();
        let engine = PdfEngine::open(&path, None, Format::Pdf, None).unwrap();
        let sign = engine.opened().fields.iter().find(|f| f.name == "sign").unwrap();
        assert!(matches!(sign.kind, super::super::FieldKind::Signature { signed: false, stamped: true, .. }));
    }

    #[test]
    fn every_stroke_of_an_ink_note_survives_a_save() {
        let path = temp("ink", &sample(1));
        let ink = Note {
            id: 1 << 40,
            page: 0,
            mark: Mark::Ink,
            hue: Hue::Red,
            rects: Vec::new(),
            quote: String::new(),
            comment: String::new(),
            author: String::new(),
            modified: String::new(),
            strokes: vec![vec![(10.0, 10.0), (40.0, 20.0)], vec![(100.0, 100.0), (120.0, 140.0), (150.0, 130.0)]],
        };
        save_notes(&path, &path, &[ink]).unwrap();
        let reopened = PdfEngine::open(&path, None, Format::Pdf, None).unwrap();
        let read = &reopened.opened().notes[0];
        assert_eq!(read.strokes.len(), 2);
        assert_eq!(read.strokes[1].len(), 3);
        assert!((read.strokes[1][2].0 - 150.0).abs() < 0.5 && (read.strokes[1][2].1 - 130.0).abs() < 0.5);
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
            strokes: Vec::new(),
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
            PageOp { source: PageSource::Other(other.clone(), 1), rotate: 0 },
            PageOp { source: PageSource::Here(0), rotate: 0 },
            PageOp { source: PageSource::Other(other, 1), rotate: 90 },
        ];
        organize(&path, &ops, &target).unwrap();
        let mut copy = PdfEngine::open(&target, None, Format::Pdf, None).unwrap();
        assert_eq!(copy.opened().pages.len(), 4);
        assert_eq!(copy.opened().pages[1], super::super::Size { w: 612.0, h: 792.0 });
        assert_eq!(copy.opened().pages[3], super::super::Size { w: 792.0, h: 612.0 }, "a page used twice turns alone");
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
            field: Some("sign".into()),
            page: 0,
            rect: Rect::new(100.0, 600.0, 200.0, 60.0),
            strokes: vec![vec![(110.0, 640.0), (200.0, 620.0), (280.0, 650.0)]],
        };
        fill(&path, &[], std::slice::from_ref(&signature), None, &path).unwrap();
        fill(&path, &[], std::slice::from_ref(&signature), None, &path).unwrap();
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
        let inked = (100..300)
            .flat_map(|x| (600..660).map(move |y| (x, y)))
            .filter(|(x, y)| image.get_pixel(*x, *y).0[0] < 200)
            .count();
        assert!(inked > 60, "the strokes are drawn inside the signature box, {inked} dark pixels");
        let outside = (100..300).filter(|x| image.get_pixel(*x, 680).0[0] < 200).count();
        assert_eq!(outside, 0, "nothing is drawn below the box");
    }
}

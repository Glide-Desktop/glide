use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use hayro::hayro_interpret::font::GlyphRun;
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::hayro_interpret::hayro_syntax::{LoadPdfError, Pdf};
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache,
    InterpreterSettings, SoftMask, interpret_page,
};
use hayro::kurbo::{Affine, BezPath, Point};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use lopdf::{Document, Object, ObjectId};

use super::text::{self, Glyph};
use super::{
    Choice, Engine, Field, FieldKind, FontInfo, Format, Heading, Hue, Info, Link, Mark, Note, OpenError, Opened,
    PageText, Rect, Size, Target,
};

pub const MANAGED: [&[u8]; 8] =
    [b"Highlight", b"Underline", b"StrikeOut", b"Squiggly", b"Text", b"Ink", b"FreeText", b"Popup"];

pub struct PdfEngine {
    pdf: Pdf,
    opened: Opened,
    transforms: Vec<Affine>,
    links: Vec<Vec<Link>>,
}

impl PdfEngine {
    pub fn open(
        path: &Path, password: Option<&str>, format: Format, view_only: Option<PathBuf>,
    ) -> Result<Self, OpenError> {
        let bytes = std::fs::read(path).map_err(|error| OpenError::Io(error.to_string()))?;
        Self::from_bytes(bytes, password, format, view_only)
    }

    pub fn from_bytes(
        bytes: Vec<u8>, password: Option<&str>, format: Format, view_only: Option<PathBuf>,
    ) -> Result<Self, OpenError> {
        let lo = load_structure(&bytes, password);
        let content_end = bytes.iter().rposition(|b| !b.is_ascii_whitespace()).map_or(0, |at| at + 1);
        let original = match Pdf::new_with_password(bytes, password.unwrap_or("")) {
            Ok(pdf) => pdf,
            Err(LoadPdfError::Decryption(_)) => return Err(OpenError::Locked { wrong: password.is_some() }),
            Err(LoadPdfError::Invalid) => return Err(OpenError::Damaged),
        };
        let encrypted = lo.as_ref().is_some_and(|doc| doc.trailer.get(b"Encrypt").is_ok()) || password.is_some();
        let pdf = lo
            .as_ref()
            .and_then(render_copy)
            .and_then(|copy| Pdf::new(copy).ok())
            .filter(|copy| copy.pages().len() == original.pages().len())
            .unwrap_or(original);
        let pages = pdf.pages();
        if pages.is_empty() {
            return Err(OpenError::Damaged);
        }
        let sizes: Vec<Size> = pages
            .iter()
            .map(|page| {
                let (w, h) = page.render_dimensions();
                Size { w, h }
            })
            .collect();
        let transforms: Vec<Affine> = pages.iter().map(|page| page.initial_transform(true).to_kurbo()).collect();
        let mut info = metadata(&pdf);
        info.encrypted = encrypted;
        let mut opened = Opened {
            format,
            labels: (1..=sizes.len()).map(|n| n.to_string()).collect(),
            pages: sizes,
            info,
            outline: Vec::new(),
            fields: Vec::new(),
            notes: Vec::new(),
            bookmarks: Vec::new(),
            broken: Vec::new(),
            reflow: None,
            view_only,
            right_to_left: false,
        };
        let mut links = vec![Vec::new(); opened.pages.len()];
        if let Some(doc) = lo.as_ref() {
            structure(doc, content_end, &transforms, &mut opened, &mut links);
        }
        Ok(Self { pdf, opened, transforms, links })
    }
}

impl Engine for PdfEngine {
    fn opened(&self) -> &Opened {
        &self.opened
    }

    fn render(&mut self, page: usize, scale: f32) -> Option<image::RgbaImage> {
        let pages = self.pdf.pages();
        let page = pages.get(page)?;
        let result = catch_unwind(AssertUnwindSafe(|| {
            let cache = RenderCache::new();
            let pixmap = render(
                page,
                &cache,
                &InterpreterSettings::default(),
                &RenderSettings::default(),
                &PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE },
            );
            let (w, h) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
            image::RgbaImage::from_raw(w, h, pixmap.data_as_u8_slice().to_vec())
        }));
        result.ok().flatten()
    }

    fn text(&mut self, index: usize) -> PageText {
        let pages = self.pdf.pages();
        let Some(page) = pages.get(index) else {
            return PageText::default();
        };
        let transform = self.transforms.get(index).copied().unwrap_or(Affine::IDENTITY);
        let crop = page.intersected_crop_box();
        let (w, h) = page.base_dimensions();
        let upright = Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, f64::from(h)]) * Affine::translate((-crop.x0, -crop.y0));
        let rotation = transform * upright.inverse();
        let result = catch_unwind(AssertUnwindSafe(|| {
            let cache = InterpreterCache::new();
            let mut context = Context::new(
                upright,
                hayro::kurbo::Rect::new(0.0, 0.0, f64::from(w), f64::from(h)),
                &cache,
                page.xref(),
                InterpreterSettings::default(),
            );
            let mut device = Collector::default();
            interpret_page(page, &mut context, &mut device);
            let mut found = text::words(&device.glyphs, device.images);
            if rotation != Affine::IDENTITY {
                for word in &mut found.words {
                    word.rect = rotate(rotation, &word.rect);
                }
                for image in &mut found.images {
                    *image = rotate(rotation, image);
                }
            }
            found
        }));
        result.unwrap_or_default()
    }

    fn links(&mut self, page: usize) -> Vec<Link> {
        self.links.get(page).cloned().unwrap_or_default()
    }
}

#[derive(Default)]
struct Collector {
    glyphs: Vec<Glyph>,
    images: Vec<Rect>,
}

impl Device<'_> for Collector {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'_>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, '_>, props: DrawProps<'_>, _: &DrawMode) {
        for glyph in run.glyphs() {
            let transform = props.transform * glyph.transform();
            let origin = transform * Point::ZERO;
            let up = transform * Point::new(0.0, 1000.0);
            let size = ((up.x - origin.x).hypot(up.y - origin.y)) as f32;
            let text = match glyph.as_unicode() {
                Some(BfString::Char(c)) => c.to_string(),
                Some(BfString::String(s)) => s,
                None => continue,
            };
            self.glyphs.push(Glyph { text, x: origin.x as f32, baseline: origin.y as f32, size, advance: None });
        }
    }

    fn draw_image(&mut self, _: Image<'_, '_>, props: ImageDrawProps<'_>) {
        let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)].map(|(x, y)| props.transform * Point::new(x, y));
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in corners {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        if x1 - x0 > 24.0 && y1 - y0 > 24.0 {
            self.images.push(Rect::new(x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32));
        }
    }
}

fn load_structure(bytes: &[u8], password: Option<&str>) -> Option<Document> {
    let mut doc = Document::load_mem(bytes).ok()?;
    if doc.is_encrypted()
        && let Some(password) = password
    {
        doc.decrypt(password).ok()?;
    }
    Some(doc)
}

fn render_copy(doc: &Document) -> Option<Vec<u8>> {
    let mut copy = doc.clone();
    let pages: Vec<ObjectId> = copy.get_pages().values().copied().collect();
    let mut stripped = false;
    for page in pages {
        let annots = annotation_ids(&copy, page);
        let keep: Vec<Object> = annots
            .iter()
            .filter(|id| {
                !copy
                    .get_dictionary(**id)
                    .ok()
                    .and_then(|d| d.get(b"Subtype").ok())
                    .and_then(|s| s.as_name().ok())
                    .is_some_and(|name| MANAGED.contains(&name))
            })
            .map(|id| Object::Reference(*id))
            .collect();
        if keep.len() != annots.len()
            && let Ok(dict) = copy.get_dictionary_mut(page)
        {
            dict.set("Annots", Object::Array(keep));
            stripped = true;
        }
    }
    if !stripped {
        return None;
    }
    copy.trailer.remove(b"Encrypt");
    let mut out = Vec::new();
    copy.save_to(&mut out).ok()?;
    Some(out)
}

#[must_use]
pub fn annotation_ids(doc: &Document, page: ObjectId) -> Vec<ObjectId> {
    let Ok(dict) = doc.get_dictionary(page) else {
        return Vec::new();
    };
    let annots = match dict.get(b"Annots") {
        Ok(Object::Reference(id)) => doc.get_object(*id).ok().and_then(|o| o.as_array().ok()).cloned(),
        Ok(Object::Array(items)) => Some(items.clone()),
        _ => None,
    };
    annots.unwrap_or_default().iter().filter_map(|item| item.as_reference().ok()).collect()
}

#[must_use]
pub fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        let units: Vec<u16> = rest.chunks_exact(2).map(|pair| u16::from_be_bytes([pair[0], pair[1]])).collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

#[must_use]
pub fn encode(text: &str) -> Vec<u8> {
    if text.is_ascii() {
        return text.as_bytes().to_vec();
    }
    let mut out = vec![0xfe, 0xff];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_be_bytes());
    }
    out
}

#[must_use]
pub fn date(raw: &str) -> String {
    let digits: String = raw.trim_start_matches("D:").chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 8 {
        return String::new();
    }
    let part = |range: std::ops::Range<usize>| digits.get(range).unwrap_or("00").to_string();
    format!("{}-{}-{}T{}:{}", part(0..4), part(4..6), part(6..8), part(8..10), part(10..12))
}

fn metadata(pdf: &Pdf) -> Info {
    let meta = pdf.metadata();
    let text = |value: &Option<Box<[u8]>>| value.as_deref().map(decode).unwrap_or_default();
    let when = |value: &Option<hayro::hayro_interpret::hayro_syntax::object::DateTime>| {
        value
            .as_ref()
            .map(|d| format!("{:04}-{:02}-{:02}T{:02}:{:02}", d.year, d.month, d.day, d.hour, d.minute))
            .unwrap_or_default()
    };
    Info {
        title: text(&meta.title),
        author: text(&meta.author),
        subject: text(&meta.subject),
        creator: text(&meta.creator),
        producer: text(&meta.producer),
        created: when(&meta.creation_date),
        modified: when(&meta.modification_date),
        version: version_name(&format!("{:?}", pdf.version())),
        print: true,
        copy: true,
        ..Info::default()
    }
}

fn rotate(rotation: Affine, rect: &Rect) -> Rect {
    let corners = [(rect.x, rect.y), (rect.right(), rect.bottom())]
        .map(|(x, y)| rotation * Point::new(f64::from(x), f64::from(y)));
    let (x0, x1) = (corners[0].x.min(corners[1].x), corners[0].x.max(corners[1].x));
    let (y0, y1) = (corners[0].y.min(corners[1].y), corners[0].y.max(corners[1].y));
    Rect::new(x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32)
}

fn version_name(debug: &str) -> String {
    let digits: String = debug.chars().filter(char::is_ascii_digit).collect();
    match digits.len() {
        2 => format!("{}.{}", &digits[..1], &digits[1..]),
        _ => digits,
    }
}

fn map_rect(transform: Affine, rect: [f32; 4]) -> Rect {
    let a = transform * Point::new(f64::from(rect[0]), f64::from(rect[1]));
    let b = transform * Point::new(f64::from(rect[2]), f64::from(rect[3]));
    let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
    let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
    Rect::new(x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32)
}

fn numbers(doc: &Document, object: &Object) -> Vec<f32> {
    let object = match object {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(object),
        other => other,
    };
    object.as_array().map(|items| items.iter().filter_map(|item| item.as_float().ok()).collect()).unwrap_or_default()
}

fn rect_of(doc: &Document, dict: &lopdf::Dictionary) -> Option<[f32; 4]> {
    let values = numbers(doc, dict.get(b"Rect").ok()?);
    (values.len() == 4).then(|| [values[0], values[1], values[2], values[3]])
}

fn string(doc: &Document, dict: &lopdf::Dictionary, key: &[u8]) -> String {
    match dict.get(key) {
        Ok(Object::String(bytes, _)) => decode(bytes),
        Ok(Object::Name(name)) => String::from_utf8_lossy(name).into_owned(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::String(bytes, _)) => decode(bytes),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

fn resolve<'a>(doc: &'a Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(object),
        other => other,
    }
}

#[must_use]
pub fn note_id(id: ObjectId) -> u64 {
    (u64::from(id.0) << 16) | u64::from(id.1)
}

fn structure(doc: &Document, content_end: usize, transforms: &[Affine], opened: &mut Opened, links: &mut [Vec<Link>]) {
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    if let Ok(toc) = doc.get_toc() {
        opened.outline = toc
            .toc
            .into_iter()
            .map(|entry| Heading {
                title: entry.title,
                level: entry.level.saturating_sub(1).min(6) as u8,
                page: entry.page.checked_sub(1),
            })
            .collect();
    }
    read_annotations(doc, &pages, transforms, opened, links);
    read_catalog(doc, &pages, content_end, transforms, opened);
    let stamped = signature_stamps(doc, &pages);
    for field in &mut opened.fields {
        if let FieldKind::Signature { stamped: on, .. } = &mut field.kind {
            *on = stamped.contains(&field.name);
        }
    }
    opened.info.fonts = read_fonts(doc, &pages);
}

fn read_annotations(
    doc: &Document, pages: &[ObjectId], transforms: &[Affine], opened: &mut Opened, links: &mut [Vec<Link>],
) {
    let page_of = |id: ObjectId| pages.iter().position(|page| *page == id);
    for (index, page) in pages.iter().enumerate() {
        let Some(transform) = transforms.get(index).copied() else {
            continue;
        };
        for id in annotation_ids(doc, *page) {
            let Ok(dict) = doc.get_dictionary(id) else {
                continue;
            };
            let Some(subtype) = dict.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) else {
                continue;
            };
            let Some(rect) = rect_of(doc, dict) else {
                continue;
            };
            let area = map_rect(transform, rect);
            if subtype == b"Link" {
                if let Some(target) = link_target(doc, dict, &page_of)
                    && let Some(list) = links.get_mut(index)
                {
                    list.push(Link { rect: area, target });
                }
                continue;
            }
            let mark = match subtype {
                b"Highlight" => Mark::Highlight,
                b"Underline" | b"Squiggly" => Mark::Underline,
                b"StrikeOut" => Mark::Strike,
                b"Text" => Mark::Comment,
                b"Ink" => Mark::Ink,
                b"FreeText" => Mark::Text,
                _ => continue,
            };
            let quads = dict.get(b"QuadPoints").map(|q| numbers(doc, q)).unwrap_or_default();
            let rects: Vec<Rect> = if quads.len() >= 8 {
                quads
                    .chunks_exact(8)
                    .map(|q| {
                        let xs = [q[0], q[2], q[4], q[6]];
                        let ys = [q[1], q[3], q[5], q[7]];
                        let fold = |v: [f32; 4], f: fn(f32, f32) -> f32, s: f32| v.into_iter().fold(s, f);
                        map_rect(
                            transform,
                            [
                                fold(xs, f32::min, f32::MAX),
                                fold(ys, f32::min, f32::MAX),
                                fold(xs, f32::max, f32::MIN),
                                fold(ys, f32::max, f32::MIN),
                            ],
                        )
                    })
                    .collect()
            } else {
                vec![area]
            };
            let color = dict.get(b"C").map(|c| numbers(doc, c)).unwrap_or_default();
            let hue = if color.len() == 3 { Hue::nearest([color[0], color[1], color[2]]) } else { Hue::Yellow };
            let strokes: Vec<Vec<(f32, f32)>> = dict
                .get(b"InkList")
                .ok()
                .and_then(|list| resolve(doc, list).as_array().ok())
                .map(|list| {
                    list.iter()
                        .map(|stroke| {
                            numbers(doc, stroke)
                                .chunks_exact(2)
                                .map(|p| {
                                    let point = transform * Point::new(f64::from(p[0]), f64::from(p[1]));
                                    (point.x as f32, point.y as f32)
                                })
                                .collect::<Vec<_>>()
                        })
                        .filter(|stroke| !stroke.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            opened.notes.push(Note {
                id: note_id(id),
                page: index,
                mark,
                hue,
                rects,
                quote: string(doc, dict, b"Subj"),
                comment: string(doc, dict, b"Contents"),
                author: string(doc, dict, b"T"),
                modified: date(&string(doc, dict, b"M")),
                strokes,
            });
        }
    }
}

fn read_catalog(doc: &Document, pages: &[ObjectId], content_end: usize, transforms: &[Affine], opened: &mut Opened) {
    let page_of = |id: ObjectId| pages.iter().position(|page| *page == id);
    let catalog = doc.catalog().ok();
    if let Some(catalog) = catalog {
        if let Some(form) = catalog.get(b"AcroForm").ok().map(|f| resolve(doc, f)).and_then(|f| f.as_dict().ok())
            && let Some(fields) = form.get(b"Fields").ok().map(|f| resolve(doc, f)).and_then(|f| f.as_array().ok())
        {
            let widget_pages: HashMap<ObjectId, usize> = pages
                .iter()
                .enumerate()
                .flat_map(|(index, page)| annotation_ids(doc, *page).into_iter().map(move |id| (id, index)))
                .collect();
            let scan = Scan { doc, page_of: &page_of, widget_pages: &widget_pages, transforms, content_end };
            for field in fields {
                if let Ok(id) = field.as_reference() {
                    collect_field(&scan, id, "", None, &mut opened.fields, 0);
                }
            }
        }
        let names = catalog.get(b"Names").ok().map(|n| resolve(doc, n)).and_then(|n| n.as_dict().ok());
        opened.info.javascript = names.is_some_and(|n| n.has(b"JavaScript"))
            || catalog
                .get(b"OpenAction")
                .ok()
                .map(|a| resolve(doc, a))
                .and_then(|a| a.as_dict().ok())
                .is_some_and(|a| a.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"JavaScript"));
        opened.info.attachments = names
            .and_then(|n| n.get(b"EmbeddedFiles").ok())
            .map(|e| resolve(doc, e))
            .and_then(|e| e.as_dict().ok())
            .and_then(|e| e.get(b"Names").ok())
            .map(|n| resolve(doc, n))
            .and_then(|n| n.as_array().ok())
            .map_or(0, |n| n.len() / 2);
    }
    if let Ok(encrypt) = doc.trailer.get(b"Encrypt").map(|e| resolve(doc, e))
        && let Ok(encrypt) = encrypt.as_dict()
        && let Ok(bits) = encrypt.get(b"P").and_then(Object::as_i64)
    {
        opened.info.print = bits & 4 != 0;
        opened.info.copy = bits & 16 != 0;
    }
}

fn read_fonts(doc: &Document, pages: &[ObjectId]) -> Vec<FontInfo> {
    let mut fonts: Vec<FontInfo> = Vec::new();
    for page in pages.iter().take(60) {
        let Ok(found) = doc.get_page_fonts(*page) else {
            continue;
        };
        for font in found.values() {
            let name = font.get(b"BaseFont").ok().and_then(|n| n.as_name().ok()).map(|n| {
                let name = String::from_utf8_lossy(n).into_owned();
                name.split_once('+').map_or(name.clone(), |(_, rest)| rest.to_string())
            });
            let Some(name) = name else {
                continue;
            };
            if fonts.iter().any(|f| f.name == name) {
                continue;
            }
            let kind = font
                .get(b"Subtype")
                .ok()
                .and_then(|n| n.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).into_owned())
                .unwrap_or_default();
            let descriptor =
                font.get(b"FontDescriptor").ok().map(|d| resolve(doc, d)).and_then(|d| d.as_dict().ok()).or_else(
                    || {
                        font.get(b"DescendantFonts")
                            .ok()
                            .map(|d| resolve(doc, d))
                            .and_then(|d| d.as_array().ok())
                            .and_then(|d| d.first())
                            .map(|d| resolve(doc, d))
                            .and_then(|d| d.as_dict().ok())
                            .and_then(|d| d.get(b"FontDescriptor").ok())
                            .map(|d| resolve(doc, d))
                            .and_then(|d| d.as_dict().ok())
                    },
                );
            let embedded = descriptor.is_some_and(|d| d.has(b"FontFile") || d.has(b"FontFile2") || d.has(b"FontFile3"));
            fonts.push(FontInfo { name, embedded, kind });
        }
    }
    fonts
}

struct Scan<'a> {
    doc: &'a Document,
    page_of: &'a dyn Fn(ObjectId) -> Option<usize>,
    widget_pages: &'a HashMap<ObjectId, usize>,
    transforms: &'a [Affine],
    content_end: usize,
}

fn appearance_states(doc: &Document, widget: &lopdf::Dictionary) -> Vec<String> {
    widget
        .get(b"AP")
        .ok()
        .map(|ap| resolve(doc, ap))
        .and_then(|ap| ap.as_dict().ok())
        .and_then(|ap| ap.get(b"N").ok())
        .map(|n| resolve(doc, n))
        .and_then(|n| n.as_dict().ok())
        .map(|n| {
            n.iter()
                .map(|(state, _)| String::from_utf8_lossy(state).into_owned())
                .filter(|state| state != "Off")
                .collect()
        })
        .unwrap_or_default()
}

fn signature_changed(doc: &Document, signature: &lopdf::Dictionary, content_end: usize) -> bool {
    let range: Vec<i64> = signature
        .get(b"ByteRange")
        .ok()
        .map(|r| resolve(doc, r))
        .and_then(|r| r.as_array().ok())
        .map(|r| r.iter().filter_map(|v| resolve(doc, v).as_i64().ok()).collect())
        .unwrap_or_default();
    let [_, _, from, length] = range[..] else {
        return true;
    };
    usize::try_from(from.saturating_add(length)).map_or(true, |end| end < content_end)
}

pub const SIGNATURE_STAMP: &str = "folio-signature:";

fn signature_stamps(doc: &Document, pages: &[ObjectId]) -> Vec<String> {
    pages
        .iter()
        .flat_map(|page| annotation_ids(doc, *page))
        .filter_map(|id| {
            let dict = doc.get_dictionary(id).ok()?;
            let stamp = dict.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Stamp");
            stamp.then(|| string(doc, dict, b"NM").strip_prefix(SIGNATURE_STAMP).map(str::to_string)).flatten()
        })
        .collect()
}

fn field_flags(doc: &Document, field: &lopdf::Dictionary) -> i64 {
    let mut current = field;
    for _ in 0..32 {
        if let Ok(flags) = current.get(b"Ff").map(|f| resolve(doc, f)).and_then(Object::as_i64) {
            return flags;
        }
        let Some(parent) =
            current.get(b"Parent").ok().and_then(|p| p.as_reference().ok()).and_then(|p| doc.get_dictionary(p).ok())
        else {
            return 0;
        };
        current = parent;
    }
    0
}

fn widget_page(scan: &Scan<'_>, id: ObjectId, widget: &lopdf::Dictionary) -> Option<usize> {
    widget
        .get(b"P")
        .ok()
        .and_then(|p| p.as_reference().ok())
        .and_then(scan.page_of)
        .or_else(|| scan.widget_pages.get(&id).copied())
}

fn button_kind(
    scan: &Scan<'_>, widgets: &[ObjectId], page: usize, flags: i64, value: &mut String,
) -> Option<FieldKind> {
    if flags & (1 << 16) != 0 {
        return None;
    }
    if flags & (1 << 15) == 0 {
        return Some(FieldKind::Check(!value.is_empty() && value != "Off"));
    }
    let doc = scan.doc;
    let transform = scan.transforms.get(page).copied()?;
    let mut states: Vec<(String, Rect)> = Vec::new();
    for id in widgets {
        let Ok(widget) = doc.get_dictionary(*id) else {
            continue;
        };
        if widget_page(scan, *id, widget).is_some_and(|on| on != page) {
            continue;
        }
        let Some(rect) = rect_of(doc, widget).map(|rect| map_rect(transform, rect)) else {
            continue;
        };
        for state in appearance_states(doc, widget) {
            if !states.iter().any(|(known, _)| *known == state) {
                states.push((state, rect));
            }
        }
    }
    if value == "Off" {
        value.clear();
    }
    Some(FieldKind::Radio(states))
}

fn choices(doc: &Document, field: &lopdf::Dictionary) -> Vec<Choice> {
    let Some(items) = field.get(b"Opt").ok().map(|o| resolve(doc, o)).and_then(|o| o.as_array().ok()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| match resolve(doc, item) {
            Object::String(bytes, _) => {
                let text = decode(bytes);
                Some(Choice { value: text.clone(), label: text })
            }
            Object::Array(pair) => {
                let part = |at: usize| pair.get(at).map(|p| resolve(doc, p)).and_then(|p| p.as_str().ok()).map(decode);
                let value = part(0)?;
                let label = part(1).unwrap_or_else(|| value.clone());
                Some(Choice { value, label })
            }
            _ => None,
        })
        .collect()
}

fn collect_field(
    scan: &Scan<'_>, id: ObjectId, prefix: &str, inherited: Option<&[u8]>, out: &mut Vec<Field>, depth: u8,
) {
    let doc = scan.doc;
    let Ok(dict) = doc.get_dictionary(id) else {
        return;
    };
    if depth > 12 {
        return;
    }
    let part = string(doc, dict, b"T");
    let name = if prefix.is_empty() {
        part
    } else if part.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}.{part}")
    };
    let kind_name: Option<Vec<u8>> = dict
        .get(b"FT")
        .ok()
        .and_then(|f| f.as_name().ok())
        .map(<[u8]>::to_vec)
        .or_else(|| inherited.map(<[u8]>::to_vec));
    if let Ok(kids) = dict.get(b"Kids").map(|k| resolve(doc, k)).and_then(Object::as_array) {
        let fields_below =
            kids.iter().filter_map(|k| k.as_reference().ok()).any(|k| doc.get_dictionary(k).is_ok_and(|d| d.has(b"T")));
        if fields_below {
            for kid in kids.iter().filter_map(|k| k.as_reference().ok()) {
                collect_field(scan, kid, &name, kind_name.as_deref(), out, depth + 1);
            }
            return;
        }
    }
    let Some(kind_name) = kind_name else {
        return;
    };
    let widgets: Vec<ObjectId> = dict
        .get(b"Kids")
        .map(|k| resolve(doc, k))
        .and_then(Object::as_array)
        .map(|kids| kids.iter().filter_map(|k| k.as_reference().ok()).collect())
        .unwrap_or_default();
    let (widget_id, widget) =
        widgets.first().and_then(|k| doc.get_dictionary(*k).ok().map(|d| (*k, d))).unwrap_or((id, dict));
    let page = widget_page(scan, widget_id, widget).unwrap_or(0);
    let rect = rect_of(doc, widget)
        .zip(scan.transforms.get(page))
        .map(|(rect, transform)| map_rect(*transform, rect))
        .unwrap_or_default();
    let flags = field_flags(doc, dict);
    let mut value = string(doc, dict, b"V");
    let kind = match kind_name.as_slice() {
        b"Btn" => {
            let Some(kind) = button_kind(scan, &widgets, page, flags, &mut value) else {
                return;
            };
            kind
        }
        b"Ch" => FieldKind::Choice(choices(doc, dict)),
        b"Sig" => {
            let signature = dict.get(b"V").ok().map(|v| resolve(doc, v)).and_then(|v| v.as_dict().ok());
            FieldKind::Signature {
                signed: signature.is_some(),
                signer: signature.map(|s| string(doc, s, b"Name")).unwrap_or_default(),
                when: signature.map(|s| date(&string(doc, s, b"M"))).unwrap_or_default(),
                changed: signature.is_some_and(|s| signature_changed(doc, s, scan.content_end)),
                stamped: false,
            }
        }
        _ => FieldKind::Text,
    };
    let label = string(doc, dict, b"TU");
    out.push(Field {
        label: if label.is_empty() { name.clone() } else { label },
        name,
        kind,
        value,
        page,
        rect,
        required: flags & 2 != 0,
    });
}

fn link_target(
    doc: &Document, dict: &lopdf::Dictionary, page_of: &dyn Fn(ObjectId) -> Option<usize>,
) -> Option<Target> {
    let dest_page = |dest: &Object| -> Option<Target> {
        let dest = resolve(doc, dest);
        let array = match dest {
            Object::Array(items) => items.clone(),
            Object::Dictionary(d) => {
                d.get(b"D").ok().map(|d| resolve(doc, d)).and_then(|d| d.as_array().ok()).cloned()?
            }
            _ => return None,
        };
        let first = array.first()?;
        match first {
            Object::Reference(id) => page_of(*id).map(Target::Page),
            Object::Integer(n) => usize::try_from(*n).ok().map(Target::Page),
            _ => None,
        }
    };
    if let Ok(action) = dict.get(b"A").map(|a| resolve(doc, a)).and_then(Object::as_dict) {
        let kind = action.get(b"S").ok().and_then(|s| s.as_name().ok());
        return match kind {
            Some(b"URI") => {
                let uri = string(doc, action, b"URI");
                (!uri.is_empty()).then_some(Target::Uri(uri))
            }
            Some(b"GoTo") => action.get(b"D").ok().and_then(dest_page),
            _ => None,
        };
    }
    dict.get(b"Dest").ok().and_then(dest_page)
}

#[cfg(test)]
pub mod tests {
    use lopdf::content::{Content, Operation};
    use lopdf::{Document, Object, Stream, dictionary};

    use super::*;

    #[must_use]
    pub fn sample(pages: usize) -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
        let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });
        let mut kids = Vec::new();
        let mut ids = Vec::new();
        for n in 0..pages {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    Operation::new("Td", vec![72.into(), 700.into()]),
                    Operation::new("Tj", vec![Object::string_literal(format!("Workspaces page {}", n + 1))]),
                    Operation::new("ET", vec![]),
                ],
            };
            let stream = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap_or_default()));
            let page = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => stream,
                "Resources" => resources,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            });
            kids.push(page.into());
            ids.push(page);
        }
        if pages > 1 {
            let link = doc.add_object(dictionary! {
                "Type" => "Annot", "Subtype" => "Link",
                "Rect" => vec![72.into(), 690.into(), 300.into(), 730.into()],
                "Dest" => vec![ids[1].into(), "Fit".into()],
            });
            let highlight = doc.add_object(dictionary! {
                "Type" => "Annot", "Subtype" => "Highlight",
                "Rect" => vec![72.into(), 690.into(), 300.into(), 730.into()],
                "QuadPoints" => vec![72.into(), 730.into(), 300.into(), 730.into(), 72.into(), 690.into(), 300.into(), 690.into()],
                "C" => vec![0.4.into(), 0.73.into(), 0.38.into()],
                "Contents" => Object::string_literal("check"),
            });
            if let Ok(dict) = doc.get_dictionary_mut(ids[0]) {
                dict.set("Annots", vec![link.into(), highlight.into()]);
            }
        }
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => pages as i64 }),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap_or_default();
        out
    }

    #[test]
    fn a_pdf_opens_with_sizes_text_links_and_notes() {
        let mut engine = PdfEngine::from_bytes(sample(3), None, Format::Pdf, None).expect("opens");
        let opened = engine.opened().clone();
        assert_eq!(opened.pages.len(), 3);
        assert_eq!(opened.pages[0], Size { w: 612.0, h: 792.0 });
        let text = engine.text(1);
        assert_eq!(text.plain(), "Workspaces page 2");
        let first = &text.words[0].rect;
        assert!(first.y > 50.0 && first.y < 100.0, "top-left origin: {first:?}");
        let links = engine.links(0);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, Target::Page(1));
        assert_eq!(opened.notes.len(), 1);
        assert_eq!(opened.notes[0].mark, Mark::Highlight);
        assert_eq!(opened.notes[0].hue, Hue::Green);
        assert_eq!(opened.notes[0].comment, "check");
        let image = engine.render(0, 0.5).expect("renders");
        assert_eq!((image.width(), image.height()), (306, 396));
        assert!(image.pixels().any(|p| p.0[0] < 128), "some ink on the page");
    }

    #[test]
    fn garbage_is_damaged() {
        assert!(matches!(
            PdfEngine::from_bytes(b"not a pdf".to_vec(), None, Format::Pdf, None),
            Err(OpenError::Damaged)
        ));
    }

    #[test]
    fn strings_decode_from_utf16_and_dates_read() {
        assert_eq!(decode(&encode("Простір")), "Простір");
        assert_eq!(decode(b"plain"), "plain");
        assert_eq!(date("D:20261106104200+02'00'"), "2026-11-06T10:42");
    }
}

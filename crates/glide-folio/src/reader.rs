use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use iced::widget::image::Handle;

use crate::doc::text::{self, Hit};
use crate::doc::write::PageSource;
use crate::doc::{Field, FieldKind, Format, Hue, Link, Mark, Note, Opened, PageText, Rect, Size};
use crate::store::{Layout, Memory, Paper, Zoom};
use crate::worker::Session;

pub const MARGIN: f32 = 24.0;
pub const GAP: f32 = 16.0;
pub const STEPS: [f32; 11] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0];
pub const ZOOM_LEVELS: [u16; 8] = [50, 75, 100, 125, 150, 200, 300, 400];
pub const TILE_LIMIT: usize = 18;
pub const THUMB_SCALE_WIDTH: f32 = 192.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Highlight,
    Underline,
    Strike,
    Comment,
    Ink,
    Text,
}

impl Tool {
    #[must_use]
    pub const fn mark(self) -> Option<Mark> {
        match self {
            Self::Highlight => Some(Mark::Highlight),
            Self::Underline => Some(Mark::Underline),
            Self::Strike => Some(Mark::Strike),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Text,
    Notes,
}

#[derive(Debug, Clone)]
pub struct Find {
    pub query: String,
    pub case: bool,
    pub whole: bool,
    pub scope: Scope,
    pub hits: Vec<Hit>,
    pub current: usize,
    pub scanned: usize,
    pub generation: u64,
    pub open: bool,
}

impl Find {
    #[must_use]
    pub fn new() -> Self {
        Self {
            query: String::new(),
            case: false,
            whole: false,
            scope: Scope::Text,
            hits: Vec::new(),
            current: 0,
            scanned: 0,
            generation: 0,
            open: true,
        }
    }
}

impl Default for Find {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub page: usize,
    pub words: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Hover {
    Link(usize, usize),
    Pin(usize, u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgPage {
    pub source: PageSource,
    pub rotate: i64,
    pub removed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Organizer {
    pub pages: Vec<OrgPage>,
    pub selected: BTreeSet<usize>,
    pub anchor: Option<usize>,
    pub extra: HashMap<(PathBuf, usize), Handle>,
}

impl Organizer {
    #[must_use]
    pub fn new(count: usize) -> Self {
        Self {
            pages: (0..count).map(|i| OrgPage { source: PageSource::Here(i), rotate: 0, removed: false }).collect(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn kept(&self) -> usize {
        self.pages.iter().filter(|p| !p.removed).count()
    }

    pub fn rotate(&mut self, by: i64) {
        for index in &self.selected {
            if let Some(page) = self.pages.get_mut(*index) {
                page.rotate = (page.rotate + by).rem_euclid(360);
            }
        }
    }

    pub fn remove(&mut self) {
        for index in &self.selected {
            if let Some(page) = self.pages.get_mut(*index) {
                page.removed = true;
            }
        }
        self.selected.clear();
    }

    pub fn pick(&mut self, index: usize, range: bool, toggle: bool) {
        if range && let Some(anchor) = self.anchor {
            let (a, b) = (anchor.min(index), anchor.max(index));
            self.selected = (a..=b).collect();
            return;
        }
        if toggle {
            if !self.selected.remove(&index) {
                self.selected.insert(index);
            }
        } else if self.selected.len() == 1 && self.selected.contains(&index) {
            self.selected.clear();
        } else {
            self.selected = BTreeSet::from([index]);
        }
        self.anchor = Some(index);
    }

    #[must_use]
    pub fn ops(&self, only_selected: bool) -> Vec<crate::doc::write::PageOp> {
        self.pages
            .iter()
            .enumerate()
            .filter(|(i, p)| !p.removed && (!only_selected || self.selected.contains(i)))
            .map(|(_, p)| crate::doc::write::PageOp { source: p.source.clone(), rotate: p.rotate })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Present {
    pub started: Instant,
    pub paused: Option<Instant>,
    pub moved: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ocr {
    Hidden,
    Offer,
    Running { done: usize, total: usize },
    Done,
}

pub type Stamp = (usize, Vec<Vec<(f32, f32)>>);

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Signing {
    pub field: usize,
    pub strokes: Vec<Vec<(f32, f32)>>,
}

pub struct Doc {
    pub path: PathBuf,
    pub name: String,
    pub opened: Opened,
    pub raster: Arc<Mutex<Session>>,
    pub index: Arc<Mutex<Session>>,
    pub generation: u64,
    pub memory: Memory,
    pub tiles: HashMap<(usize, usize, Paper), Handle>,
    pub order: VecDeque<(usize, usize, Paper)>,
    pub pending: HashSet<(usize, usize, Paper)>,
    pub thumbs: HashMap<usize, Handle>,
    pub thumbs_pending: HashSet<usize>,
    pub texts: HashMap<usize, PageText>,
    pub texts_pending: HashSet<usize>,
    pub links: HashMap<usize, Vec<Link>>,
    pub notes: Vec<Note>,
    pub saved: Vec<Note>,
    pub find: Option<Find>,
    pub tool: Option<Tool>,
    pub hue: Hue,
    pub selection: Option<Selection>,
    pub focus: Option<u64>,
    pub draft: Option<(u64, String)>,
    pub history: Vec<usize>,
    pub future: Vec<usize>,
    pub back_chip: Option<(usize, Instant)>,
    pub hover: Option<Hover>,
    pub popover: bool,
    pub viewport: iced::Size,
    pub scroll: f32,
    pub organizer: Option<Organizer>,
    pub present: Option<Present>,
    pub bare: bool,
    pub values: HashMap<String, String>,
    pub signing: Option<Signing>,
    pub stamps: Vec<Stamp>,
    pub form: bool,
    pub ocr: Ocr,
    pub step: i8,
    pub dpr: f32,
    pub undo: Vec<Vec<Note>>,
}

impl Doc {
    #[must_use]
    pub fn new(
        path: PathBuf, opened: Opened, raster: Session, index: Session, memory: Memory, generation: u64,
    ) -> Self {
        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let notes = opened.notes.clone();
        let form = opened
            .fields
            .iter()
            .any(|f| f.required && f.value.is_empty() || matches!(f.kind, FieldKind::Signature { signed: false, .. }));
        let values = opened
            .fields
            .iter()
            .map(|f| {
                (
                    f.name.clone(),
                    match f.kind {
                        FieldKind::Check(on) => on.to_string(),
                        _ => f.value.clone(),
                    },
                )
            })
            .collect();
        let mut memory = memory;
        memory.pages = opened.pages.len();
        memory.page = memory.page.min(opened.pages.len().saturating_sub(1));
        Self {
            path,
            name,
            raster: Arc::new(Mutex::new(raster)),
            index: Arc::new(Mutex::new(index)),
            generation,
            memory,
            tiles: HashMap::new(),
            order: VecDeque::new(),
            pending: HashSet::new(),
            thumbs: HashMap::new(),
            thumbs_pending: HashSet::new(),
            texts: HashMap::new(),
            texts_pending: HashSet::new(),
            links: HashMap::new(),
            saved: notes.clone(),
            notes,
            find: None,
            tool: None,
            hue: Hue::Yellow,
            selection: None,
            focus: None,
            draft: None,
            history: Vec::new(),
            future: Vec::new(),
            back_chip: None,
            hover: None,
            popover: false,
            viewport: iced::Size::new(1000.0, 680.0),
            scroll: 0.0,
            organizer: None,
            present: None,
            bare: false,
            values,
            signing: None,
            stamps: Vec::new(),
            form,
            ocr: Ocr::Hidden,
            step: 0,
            dpr: 1.0,
            undo: Vec::new(),
            opened,
        }
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.opened.pages.len()
    }

    #[must_use]
    pub fn page(&self) -> usize {
        self.memory.page
    }

    #[must_use]
    pub fn size(&self, page: usize) -> Size {
        let size = self.opened.pages.get(page).copied().unwrap_or(Size { w: 612.0, h: 792.0 });
        if self.memory.crop { Size { w: size.w * 0.86, h: size.h * 0.9 } } else { size }
    }

    #[must_use]
    pub fn widest(&self) -> Size {
        self.opened.pages.iter().fold(Size { w: 1.0, h: 1.0 }, |acc, s| Size { w: acc.w.max(s.w), h: acc.h.max(s.h) })
    }

    #[must_use]
    pub fn zoom(&self) -> f32 {
        let widest = self.widest();
        let columns = if self.memory.layout == Layout::Spread { 2.0 } else { 1.0 };
        let width = (self.viewport.width - 2.0 * MARGIN - (columns - 1.0) * 4.0).max(100.0);
        let height = (self.viewport.height - 2.0 * MARGIN).max(100.0);
        match self.memory.zoom {
            Zoom::FitWidth => (width / (widest.w * columns)).clamp(0.1, 8.0),
            Zoom::FitPage => (width / (widest.w * columns)).min(height / widest.h).clamp(0.1, 8.0),
            Zoom::FitHeight => (height / widest.h).clamp(0.1, 8.0),
            Zoom::Percent(percent) => f32::from(percent) / 100.0,
        }
    }

    #[must_use]
    pub fn percent(&self) -> u16 {
        (self.zoom() * 100.0).round() as u16
    }

    #[must_use]
    pub fn step_for(zoom: f32, dpr: f32) -> usize {
        let wanted = zoom * dpr;
        STEPS.iter().position(|step| *step >= wanted - 0.01).unwrap_or(STEPS.len() - 1)
    }

    #[must_use]
    pub fn rows(&self) -> Vec<Vec<usize>> {
        let count = self.count();
        match self.memory.layout {
            Layout::Single => vec![vec![self.page()]],
            Layout::Continuous => (0..count).map(|p| vec![p]).collect(),
            Layout::Spread => {
                let mut rows = Vec::new();
                let mut page = 0;
                if self.memory.cover_alone && count > 0 {
                    rows.push(vec![0]);
                    page = 1;
                }
                while page < count {
                    let mut row = vec![page];
                    if page + 1 < count {
                        row.push(page + 1);
                    }
                    if self.memory.rtl {
                        row.reverse();
                    }
                    rows.push(row);
                    page += 2;
                }
                rows
            }
        }
    }

    #[must_use]
    pub fn row_heights(&self) -> Vec<f32> {
        let zoom = self.zoom();
        self.rows().iter().map(|row| row.iter().map(|p| self.size(*p).h * zoom).fold(0.0, f32::max)).collect()
    }

    #[must_use]
    pub fn row_tops(&self) -> Vec<f32> {
        let mut y = MARGIN;
        self.row_heights()
            .into_iter()
            .map(|h| {
                let top = y;
                y += h + GAP;
                top
            })
            .collect()
    }

    #[must_use]
    pub fn content_height(&self) -> f32 {
        let heights = self.row_heights();
        MARGIN * 2.0 + heights.iter().sum::<f32>() + GAP * heights.len().saturating_sub(1) as f32
    }

    #[must_use]
    pub fn row_of(&self, page: usize) -> usize {
        self.rows().iter().position(|row| row.contains(&page)).unwrap_or(0)
    }

    #[must_use]
    pub fn top_of(&self, page: usize) -> f32 {
        self.row_tops().get(self.row_of(page)).copied().unwrap_or(0.0)
    }

    #[must_use]
    pub fn page_at_centre(&self) -> usize {
        if self.memory.layout == Layout::Single {
            return self.page();
        }
        let centre = self.scroll + self.viewport.height / 2.0;
        let tops = self.row_tops();
        let heights = self.row_heights();
        let rows = self.rows();
        let row = tops
            .iter()
            .zip(&heights)
            .position(|(top, h)| centre < top + h + GAP / 2.0)
            .unwrap_or(rows.len().saturating_sub(1));
        rows.get(row).and_then(|r| r.iter().min().copied()).unwrap_or(0)
    }

    #[must_use]
    pub fn visible(&self) -> Vec<usize> {
        if self.memory.layout == Layout::Single {
            return vec![self.page()];
        }
        let (from, to) = (self.scroll - self.viewport.height * 0.5, self.scroll + self.viewport.height * 1.5);
        let tops = self.row_tops();
        let heights = self.row_heights();
        self.rows()
            .into_iter()
            .zip(tops.iter().zip(&heights))
            .filter(|(_, (top, h))| **top + **h >= from && **top <= to)
            .flat_map(|(row, _)| row)
            .collect()
    }

    #[must_use]
    pub fn tile(&self, page: usize) -> Option<Handle> {
        let paper = self.memory.paper;
        let wanted = Self::step_for(self.zoom(), self.dpr);
        if let Some(handle) = self.tiles.get(&(page, wanted, paper)) {
            return Some(handle.clone());
        }
        self.tiles
            .iter()
            .filter(|((p, _, pp), _)| *p == page && *pp == paper)
            .min_by_key(|((_, step, _), _)| step.abs_diff(wanted))
            .map(|(_, handle)| handle.clone())
            .or_else(|| self.tiles.iter().find(|((p, _, _), _)| *p == page).map(|(_, h)| h.clone()))
    }

    pub fn store_tile(&mut self, key: (usize, usize, Paper), handle: Handle) {
        self.pending.remove(&key);
        self.tiles.insert(key, handle);
        self.order.retain(|k| *k != key);
        self.order.push_back(key);
        while self.order.len() > TILE_LIMIT {
            if let Some(old) = self.order.pop_front() {
                self.tiles.remove(&old);
            }
        }
    }

    #[must_use]
    pub fn missing_tiles(&self) -> Vec<(usize, usize, Paper)> {
        let step = Self::step_for(self.zoom(), self.dpr);
        let paper = self.memory.paper;
        self.visible()
            .into_iter()
            .map(|p| (p, step, paper))
            .filter(|key| !self.tiles.contains_key(key) && !self.pending.contains(key))
            .collect()
    }

    #[must_use]
    pub fn dirty(&self) -> bool {
        self.notes != self.saved
    }

    #[must_use]
    pub fn new_notes(&self) -> usize {
        self.notes.iter().filter(|n| !self.saved.iter().any(|s| s.id == n.id)).count()
    }

    pub fn remember(&mut self) {
        self.undo.push(self.notes.clone());
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
    }

    pub fn add_note(&mut self, note: Note) {
        self.remember();
        self.focus = Some(note.id);
        self.notes.push(note);
    }

    #[must_use]
    pub fn fresh_id(&self) -> u64 {
        let base = self.notes.iter().map(|n| n.id).max().unwrap_or(0);
        (base + 1).max(1 << 40)
    }

    #[must_use]
    pub fn note_from_selection(&self, mark: Mark, author: &str, now: &str) -> Option<Note> {
        let selection = self.selection.as_ref()?;
        let text = self.texts.get(&selection.page)?;
        let rects = text::line_rects(text, &selection.words);
        if rects.is_empty() {
            return None;
        }
        Some(Note {
            id: self.fresh_id(),
            page: selection.page,
            mark,
            hue: self.hue,
            rects,
            quote: text::quote(text, &selection.words),
            comment: String::new(),
            author: author.to_string(),
            modified: now.to_string(),
            stroke: Vec::new(),
        })
    }

    pub fn go(&mut self, page: usize, remember: bool) -> usize {
        let page = page.min(self.count().saturating_sub(1));
        if remember && page != self.page() {
            self.history.push(self.page());
            self.future.clear();
        }
        self.step = match page.cmp(&self.page()) {
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
        };
        self.memory.page = page;
        page
    }

    #[must_use]
    pub fn hit_pages(&self) -> Vec<(usize, Rect, bool)> {
        let Some(find) = &self.find else {
            return Vec::new();
        };
        find.hits
            .iter()
            .enumerate()
            .flat_map(|(i, hit)| hit.rects.iter().map(move |r| (hit.page, *r, i == find.current)))
            .collect()
    }

    #[must_use]
    pub fn required_empty(&self) -> usize {
        self.opened
            .fields
            .iter()
            .filter(|f| f.required && !matches!(f.kind, FieldKind::Signature { .. }))
            .filter(|f| self.values.get(&f.name).is_none_or(|v| v.is_empty() || v == "false"))
            .count()
    }

    #[must_use]
    pub fn filled(&self) -> (usize, usize) {
        let fields: Vec<&Field> =
            self.opened.fields.iter().filter(|f| !matches!(f.kind, FieldKind::Signature { .. })).collect();
        let done =
            fields.iter().filter(|f| self.values.get(&f.name).is_some_and(|v| !v.is_empty() && v != "false")).count();
        (done, fields.len())
    }

    #[must_use]
    pub fn form_dirty(&self) -> bool {
        !self.stamps.is_empty()
            || self.opened.fields.iter().any(|f| {
                let original = match f.kind {
                    FieldKind::Check(on) => on.to_string(),
                    _ => f.value.clone(),
                };
                self.values.get(&f.name).is_some_and(|v| *v != original)
            })
    }

    #[must_use]
    pub fn editable(&self) -> bool {
        self.opened.format.annotates_in_place() && !self.opened.info.encrypted && self.opened.view_only.is_none()
    }

    #[must_use]
    pub fn reflow(&self) -> bool {
        self.opened.format == Format::Epub
    }

    #[must_use]
    pub fn has_text(&self) -> bool {
        self.texts.values().any(|t| !t.words.is_empty())
    }
}

pub fn tint(image: &mut image::RgbaImage, paper: Paper, keep: &[(u32, u32, u32, u32)]) {
    let (face, ink): ([f32; 3], [f32; 3]) = match paper {
        Paper::White => return,
        Paper::Sepia => ([246.0, 239.0, 223.0], [59.0, 48.0, 34.0]),
        Paper::Night => ([29.0, 28.0, 32.0], [222.0, 219.0, 213.0]),
    };
    let width = image.width();
    for (index, pixel) in image.pixels_mut().enumerate() {
        let (column, line) = (index as u32 % width, index as u32 / width);
        if keep.iter().any(|(left, top, w, h)| column >= *left && column < left + w && line >= *top && line < top + h) {
            continue;
        }
        let [red, green, blue, alpha] = pixel.0;
        let lum = (0.299 * f32::from(red) + 0.587 * f32::from(green) + 0.114 * f32::from(blue)) / 255.0;
        let dark = 1.0 - lum;
        let mix = |channel: usize| (face[channel] * (1.0 - dark) + ink[channel] * dark).round().clamp(0.0, 255.0) as u8;
        pixel.0 = [mix(0), mix(1), mix(2), alpha];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Info, Opened};

    pub fn opened(pages: usize) -> Opened {
        Opened {
            format: Format::Pdf,
            pages: vec![Size { w: 612.0, h: 792.0 }; pages],
            labels: (1..=pages).map(|n| n.to_string()).collect(),
            info: Info::default(),
            outline: Vec::new(),
            fields: Vec::new(),
            notes: Vec::new(),
            bookmarks: Vec::new(),
            broken: Vec::new(),
            reflow: None,
            view_only: None,
            right_to_left: false,
        }
    }

    fn doc(pages: usize) -> Doc {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("glide-folio-reader-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.pdf");
        std::fs::write(&path, crate::doc::pdf::tests::sample(pages)).unwrap();
        let raster = Session::local(&path, None).unwrap();
        let index = Session::local(&path, None).unwrap();
        Doc::new(path, opened(pages), raster, index, Memory::default(), 1)
    }

    #[test]
    fn fit_width_fills_the_viewport_and_steps_round_up() {
        let mut doc = doc(3);
        doc.viewport = iced::Size::new(660.0, 600.0);
        assert!((doc.zoom() - 1.0).abs() < 0.01, "{}", doc.zoom());
        assert_eq!(STEPS[Doc::step_for(1.0, 1.0)], 1.0);
        assert_eq!(STEPS[Doc::step_for(1.1, 2.0)], 2.5);
        doc.memory.zoom = Zoom::Percent(125);
        assert!((doc.zoom() - 1.25).abs() < f32::EPSILON);
    }

    #[test]
    fn fit_height_shows_the_whole_page_height_whatever_the_width() {
        let mut doc = doc(3);
        let tall = doc.widest().h;
        doc.memory.zoom = Zoom::FitHeight;
        for width in [400.0, 660.0, 1600.0] {
            doc.viewport = iced::Size::new(width, 600.0);
            assert!((doc.zoom() * tall - (600.0 - 2.0 * MARGIN)).abs() < 0.5, "width {width}: {}", doc.zoom());
        }
    }

    #[test]
    fn the_page_at_the_viewport_centre_is_current() {
        let mut doc = doc(3);
        doc.viewport = iced::Size::new(660.0, 600.0);
        doc.memory.zoom = Zoom::Percent(100);
        assert_eq!(doc.page_at_centre(), 0);
        doc.scroll = doc.top_of(1) - 200.0;
        assert_eq!(doc.page_at_centre(), 1);
        assert_eq!(doc.visible(), vec![0, 1]);
        doc.scroll = doc.top_of(2);
        assert_eq!(doc.visible(), vec![1, 2]);
    }

    #[test]
    fn spreads_keep_the_cover_alone_and_flip_for_right_to_left() {
        let mut doc = doc(3);
        doc.memory.layout = Layout::Spread;
        assert_eq!(doc.rows(), vec![vec![0], vec![1, 2]]);
        doc.memory.rtl = true;
        assert_eq!(doc.rows(), vec![vec![0], vec![2, 1]]);
        doc.memory.cover_alone = false;
        assert_eq!(doc.rows(), vec![vec![1, 0], vec![2]]);
    }

    #[test]
    fn the_tile_cache_keeps_only_the_newest() {
        let mut doc = doc(1);
        for page in 0..TILE_LIMIT + 4 {
            doc.store_tile((page, 2, Paper::White), Handle::from_rgba(1, 1, vec![0, 0, 0, 255]));
        }
        assert_eq!(doc.tiles.len(), TILE_LIMIT);
        assert!(!doc.tiles.contains_key(&(0, 2, Paper::White)));
    }

    #[test]
    fn history_records_jumps_and_undo_restores_notes() {
        let mut doc = doc(3);
        doc.go(2, true);
        assert_eq!(doc.history, vec![0]);
        doc.go(1, false);
        assert_eq!(doc.history, vec![0]);
        let note = Note {
            id: 1,
            page: 0,
            mark: Mark::Comment,
            hue: Hue::Yellow,
            rects: vec![],
            quote: String::new(),
            comment: "x".into(),
            author: String::new(),
            modified: String::new(),
            stroke: vec![],
        };
        doc.add_note(note);
        assert!(doc.dirty());
        doc.notes = doc.undo.pop().unwrap();
        assert!(!doc.dirty());
    }

    #[test]
    fn the_organiser_selects_ranges_and_rotates_only_the_selection() {
        let mut org = Organizer::new(6);
        org.pick(1, false, false);
        org.pick(3, true, false);
        assert_eq!(org.selected.iter().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
        org.rotate(90);
        assert_eq!(org.pages[2].rotate, 90);
        assert_eq!(org.pages[0].rotate, 0);
        org.remove();
        assert_eq!(org.kept(), 3);
        assert_eq!(org.ops(false).len(), 3);
    }

    #[test]
    fn night_paper_inverts_ink_but_keeps_images() {
        let mut image = image::RgbaImage::from_pixel(4, 2, image::Rgba([255, 255, 255, 255]));
        image.put_pixel(3, 1, image::Rgba([200, 30, 30, 255]));
        tint(&mut image, Paper::Night, &[(3, 0, 1, 2)]);
        assert_eq!(image.get_pixel(0, 0).0, [29, 28, 32, 255]);
        assert_eq!(image.get_pixel(3, 1).0, [200, 30, 30, 255]);
    }
}

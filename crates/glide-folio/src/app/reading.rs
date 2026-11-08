use glide_sdk::Shell;
use glide_sdk::widget::PageEvent;
use iced::Task;
use iced::widget::image::Handle;

use super::{App, Message, NoteMsg, later};
use crate::doc::text;
use crate::doc::{Link, PageText, Target};
use crate::i18n::Key;
use crate::reader::{Doc, Find, Hover, STEPS, Scope, ZOOM_LEVELS, tint};
use crate::store::{Layout, Paper, Tab, Zoom};

pub const THUMB_ROW: f32 = 158.0;
const FIND_CHUNK: usize = 6;

#[must_use]
pub fn rendered_scale(session: &crate::worker::Session, page: usize, width: u32, asked: f32) -> f32 {
    session.opened().pages.get(page).filter(|size| size.w > 0.0).map_or(asked, |size| width as f32 / size.w)
}

#[must_use]
pub fn openable(uri: &str) -> bool {
    let scheme = uri.split_once(':').map(|(scheme, _)| scheme.to_ascii_lowercase());
    matches!(scheme.as_deref(), Some("http" | "https" | "mailto"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_web_and_mail_links_leave_the_document() {
        assert!(super::openable("https://glide.example/doc"));
        assert!(super::openable("HTTP://glide.example"));
        assert!(super::openable("mailto:someone@example.com"));
        assert!(!super::openable("file:///etc/passwd"));
        assert!(!super::openable("smb://server/share"));
        assert!(!super::openable("steam://run/1"));
        assert!(!super::openable("javascript:alert(1)"));
    }
}

#[derive(Debug, Clone)]
pub enum Read {
    Library,
    Tile(u64, bool, (usize, usize, Paper), Option<Handle>),
    Thumb(u64, usize, Option<Handle>),
    Text(u64, usize, PageText, Vec<Link>),
    Scrolled(iced::Rectangle, iced::widget::scrollable::AbsoluteOffset),
    Reflowed(f32),
    Scroll(i32),
    ThumbsScrolled(f32),
    Go(usize),
    Step(i32),
    PageInput(String),
    PageSubmit,
    Back,
    Forward,
    Tab(Tab),
    ToggleSide,
    Layout(Layout),
    Zoom(Zoom),
    ZoomBy(i32),
    ZoomLevel(usize),
    ZoomWheel(i8),
    Paper(Paper),
    KeepImages(bool),
    CoverAlone(bool),
    Crop(bool),
    Rtl(bool),
    Popover(bool),
    Page(usize, PageEvent),
    Follow(Target),
    Context(iced::Point),
}

#[derive(Debug, Clone)]
pub enum FindMsg {
    Open,
    Query(String),
    Next,
    Previous,
    Case(bool),
    Whole(bool),
    Scope(Scope),
    Close,
    Chunk(u64, usize, Vec<(usize, PageText)>),
    Pick(usize),
}

impl App {
    pub fn request_visible(&mut self) -> Task<Message> {
        let Some(doc) = self.doc.as_mut() else {
            return Task::none();
        };
        if doc.reflow() {
            return Task::none();
        }
        let mut tasks = Vec::new();
        for key in doc.missing_tiles() {
            doc.pending.insert(key);
            let session = doc.raster.clone();
            let generation = doc.generation;
            let keep = doc.memory.keep_images;
            tasks.push(later(
                move || {
                    let mut session = session.lock().ok()?;
                    let (page, step, paper) = key;
                    let scale = STEPS[step];
                    let images = if paper != Paper::White && keep { session.text(page).images } else { Vec::new() };
                    let mut image = session.render(page, scale)?;
                    let scale = rendered_scale(&session, page, image.width(), scale);
                    let keep: Vec<(u32, u32, u32, u32)> = images
                        .iter()
                        .map(|r| {
                            (
                                (r.x * scale) as u32,
                                (r.y * scale) as u32,
                                (r.w * scale).ceil() as u32,
                                (r.h * scale).ceil() as u32,
                            )
                        })
                        .collect();
                    tint(&mut image, paper, &keep);
                    let (w, h) = image.dimensions();
                    Some(Handle::from_rgba(w, h, image.into_raw()))
                },
                move |handle| Message::Read(Read::Tile(generation, keep, key, handle)),
            ));
        }
        for page in doc.visible() {
            if doc.texts.contains_key(&page) || !doc.texts_pending.insert(page) {
                continue;
            }
            let session = doc.index.clone();
            let generation = doc.generation;
            tasks.push(later(
                move || {
                    let mut session = session.lock().ok()?;
                    Some((session.text(page), session.links(page)))
                },
                move |found| match found {
                    Some((text, links)) => Message::Read(Read::Text(generation, page, text, links)),
                    None => Message::Ignore,
                },
            ));
        }
        Task::batch(tasks)
    }

    pub fn request_thumbs(&mut self) -> Task<Message> {
        let Some(doc) = self.doc.as_ref() else {
            return Task::none();
        };
        let around = doc.page();
        self.thumbs_between(around.saturating_sub(4), around + 10)
    }

    pub fn thumbs_range(&mut self, from: usize, to: usize) -> Task<Message> {
        self.thumbs_between(from, to)
    }

    fn thumbs_between(&mut self, from: usize, to: usize) -> Task<Message> {
        let Some(doc) = self.doc.as_mut() else {
            return Task::none();
        };
        if doc.reflow() {
            return Task::none();
        }
        let mut tasks = Vec::new();
        for page in from..to.min(doc.count()) {
            if doc.thumbs.contains_key(&page) || !doc.thumbs_pending.insert(page) {
                continue;
            }
            let session = doc.index.clone();
            let generation = doc.generation;
            let width = doc.size(page).w;
            tasks.push(later(
                move || {
                    let mut session = session.lock().ok()?;
                    let image = session.render(page, crate::reader::THUMB_SCALE_WIDTH / width.max(1.0))?;
                    let (w, h) = image.dimensions();
                    Some(Handle::from_rgba(w, h, image.into_raw()))
                },
                move |handle| Message::Read(Read::Thumb(generation, page, handle)),
            ));
        }
        Task::batch(tasks)
    }

    pub fn fit_viewport(&mut self) -> bool {
        let window = self.window.size;
        let Some(doc) = self.doc.as_mut() else {
            return false;
        };
        let panel = if doc.memory.side { crate::view::SIDEBAR + 1.0 } else { 0.0 };
        let size = iced::Size::new(
            (window.width - panel).max(320.0),
            (window.height - glide_sdk::window::HEADER - glide_sdk::widget::STATUS_BAR).max(240.0),
        );
        let changed = (doc.viewport.width - size.width).abs() > 0.5 || (doc.viewport.height - size.height).abs() > 0.5;
        doc.viewport = size;
        changed
    }

    pub fn scroll_to_page(&self) -> Task<Message> {
        let Some(doc) = self.doc.as_ref() else {
            return Task::none();
        };
        if doc.reflow() {
            return iced::widget::operation::snap_to(
                crate::view::PAGES_ID,
                iced::widget::scrollable::RelativeOffset { x: 0.0, y: doc.memory.offset.clamp(0.0, 1.0) },
            );
        }
        if doc.memory.layout == Layout::Single {
            return Task::none();
        }
        let y = (doc.top_of(doc.page()) - crate::reader::MARGIN).max(0.0);
        iced::widget::operation::scroll_to(
            crate::view::PAGES_ID,
            iced::widget::scrollable::AbsoluteOffset { x: doc.scroll_x, y },
        )
    }

    fn relayout(&mut self) -> Task<Message> {
        let jump = self.scroll_to_page();
        Task::batch([jump, self.request_visible()])
    }

    fn jump(&mut self, page: usize, remember: bool) -> Task<Message> {
        let Some(doc) = self.doc.as_mut() else {
            return Task::none();
        };
        let page = doc.go(page, remember);
        if remember {
            doc.back_chip = doc.history.last().map(|from| (*from, std::time::Instant::now()));
        }
        let near = self.thumbs_between(page.saturating_sub(4), page + 10);
        Task::batch([self.scroll_to_page(), self.request_visible(), near])
    }

    #[allow(clippy::too_many_lines)] // One arm per reader message; each delegates or sets one field
    pub fn read(&mut self, read: Read, shell: &mut Shell<Message>) -> Task<Message> {
        let generation = self.doc.as_ref().map_or(0, |d| d.generation);
        match read {
            Read::Library => self.leave(super::After::Library, shell),
            Read::Tile(at, keep, key, handle) => {
                if let Some(doc) = self.doc.as_mut().filter(|_| at == generation) {
                    if keep != doc.memory.keep_images && key.2 != Paper::White {
                        doc.pending.remove(&key);
                        return self.request_visible();
                    }
                    if let Some(handle) = handle {
                        doc.store_tile(key, handle);
                    } else {
                        doc.pending.remove(&key);
                        doc.failed.insert((key.0, key.1));
                        if !doc.opened.broken.contains(&key.0) {
                            doc.opened.broken.push(key.0);
                        }
                    }
                }
                Task::none()
            }
            Read::Thumb(at, page, handle) => {
                if let Some(doc) = self.doc.as_mut().filter(|_| at == generation) {
                    doc.thumbs_pending.remove(&page);
                    if let Some(handle) = handle {
                        doc.thumbs.insert(page, handle);
                    }
                }
                Task::none()
            }
            Read::Text(at, page, text, links) => {
                if let Some(doc) = self.doc.as_mut().filter(|_| at == generation) {
                    doc.texts_pending.remove(&page);
                    if page == 0 && text.words.is_empty() && !doc.reflow() && doc.ocr == crate::reader::Ocr::Hidden {
                        doc.ocr = crate::reader::Ocr::Offer;
                    }
                    if !text.words.is_empty() && doc.ocr == crate::reader::Ocr::Offer {
                        doc.ocr = crate::reader::Ocr::Hidden;
                    }
                    doc.texts.insert(page, text);
                    doc.links.insert(page, links);
                }
                Task::none()
            }
            Read::Reflowed(offset) => {
                if let Some(doc) = self.doc.as_mut()
                    && offset.is_finite()
                {
                    doc.memory.offset = offset.clamp(0.0, 1.0);
                    doc.overflows = Some(doc.page());
                }
                Task::none()
            }
            Read::Scroll(by) => {
                let Some(doc) = self.doc.as_mut() else {
                    return Task::none();
                };
                if !doc.reflow() {
                    return self.read(Read::Step(by), shell);
                }
                if doc.overflows != Some(doc.page()) {
                    if doc.nudged == Some(doc.page()) {
                        doc.nudged = None;
                        return self.read(Read::Step(by), shell);
                    }
                    doc.nudged = Some(doc.page());
                }
                if doc.memory.offset >= 0.999 && by > 0 {
                    return self.read(Read::Step(1), shell);
                }
                if doc.memory.offset <= 0.001 && by < 0 {
                    if doc.page() == 0 {
                        return Task::none();
                    }
                    let page = doc.page() - 1;
                    doc.go(page, false);
                    doc.memory.offset = 1.0;
                    return self.scroll_to_page();
                }
                let distance = doc.viewport.height * 0.9 * by as f32;
                iced::widget::operation::scroll_by(
                    crate::view::PAGES_ID,
                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: distance },
                )
            }
            Read::Scrolled(bounds, offset) => {
                let Some(doc) = self.doc.as_mut() else {
                    return Task::none();
                };
                let resized = (doc.viewport.width - bounds.width).abs() > 1.0
                    || (doc.viewport.height - bounds.height).abs() > 1.0;
                doc.viewport = bounds.size();
                doc.scroll = offset.y;
                doc.scroll_x = offset.x;
                let current = doc.page_at_centre();
                if current != doc.memory.page && doc.memory.layout != Layout::Single {
                    doc.memory.page = current;
                }
                let thumbs = if resized { Task::none() } else { self.request_thumbs() };
                Task::batch([self.request_visible(), thumbs])
            }
            Read::ThumbsScrolled(offset) => {
                let first = (offset / THUMB_ROW).floor().max(0.0) as usize;
                self.thumbs_between(first.saturating_sub(2), first + 10)
            }
            Read::Go(page) => self.jump(page, false),
            Read::Step(by) => {
                let Some(doc) = self.doc.as_ref() else {
                    return Task::none();
                };
                let stride = if doc.memory.layout == Layout::Spread { 2 } else { 1 };
                let page = (doc.page() as i64 + i64::from(by) * stride).clamp(0, doc.count() as i64 - 1) as usize;
                self.jump(page, false)
            }
            Read::PageInput(text) => {
                self.page_input = Some(text.chars().filter(char::is_ascii_digit).take(5).collect());
                Task::none()
            }
            Read::PageSubmit => {
                let wanted = self.page_input.take().and_then(|t| t.parse::<usize>().ok());
                wanted.map_or_else(Task::none, |n| self.jump(n.saturating_sub(1), true))
            }
            Read::Back => {
                let Some(doc) = self.doc.as_mut() else {
                    return Task::none();
                };
                let Some(page) = doc.history.pop() else {
                    return Task::none();
                };
                doc.future.push(doc.page());
                doc.back_chip = None;
                self.jump(page, false)
            }
            Read::Forward => {
                let Some(doc) = self.doc.as_mut() else {
                    return Task::none();
                };
                let Some(page) = doc.future.pop() else {
                    return Task::none();
                };
                doc.history.push(doc.page());
                self.jump(page, false)
            }
            Read::Tab(tab) => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.memory.tab = tab;
                    doc.form = false;
                }
                self.request_thumbs()
            }
            Read::ToggleSide => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.memory.side = !doc.memory.side;
                }
                self.fit_viewport();
                Task::batch([self.request_visible(), self.request_thumbs()])
            }
            Read::Layout(layout) => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.memory.layout = layout;
                }
                self.relayout()
            }
            Read::Zoom(zoom) => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.memory.zoom = zoom;
                }
                self.relayout()
            }
            Read::ZoomBy(by) => self.zoom_by(by),
            Read::ZoomWheel(by) => self.zoom_by(i32::from(by)),
            Read::ZoomLevel(level) => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.memory.zoom = Zoom::Percent(ZOOM_LEVELS[level.min(ZOOM_LEVELS.len() - 1)]);
                }
                self.relayout()
            }
            Read::Paper(paper) => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.memory.paper = paper;
                }
                self.request_visible()
            }
            Read::KeepImages(on) => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.memory.keep_images = on;
                    doc.tiles.retain(|(_, _, paper), _| *paper == Paper::White);
                    doc.order.retain(|(_, _, paper)| *paper == Paper::White);
                }
                self.request_visible()
            }
            Read::CoverAlone(on) => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.memory.cover_alone = on;
                }
                self.relayout()
            }
            Read::Crop(on) => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.memory.crop = on;
                }
                self.relayout()
            }
            Read::Rtl(on) => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.memory.rtl = on;
                }
                Task::none()
            }
            Read::Popover(open) => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.popover = open;
                }
                Task::none()
            }
            Read::Page(page, event) => self.page_event(page, event, shell),
            Read::Follow(target) => match target {
                Target::Page(page) => self.jump(page, true),
                Target::Uri(uri) => {
                    if openable(&uri) {
                        let _ = glide_sdk::system::open(&uri, crate::APP_ID);
                    } else {
                        shell.toast(self.t(Key::LinkBlocked));
                    }
                    Task::none()
                }
            },
            Read::Context(point) => {
                let items = self.page_menu();
                shell.context_menu_at(point, items);
                Task::none()
            }
        }
    }

    fn zoom_by(&mut self, by: i32) -> Task<Message> {
        let Some(doc) = self.doc.as_mut() else {
            return Task::none();
        };
        let current = doc.percent();
        let next = if by > 0 {
            ZOOM_LEVELS.iter().copied().find(|level| *level > current + 2).unwrap_or(ZOOM_LEVELS[ZOOM_LEVELS.len() - 1])
        } else {
            ZOOM_LEVELS.iter().rev().copied().find(|level| *level + 2 < current).unwrap_or(ZOOM_LEVELS[0])
        };
        doc.memory.zoom = Zoom::Percent(next);
        self.relayout()
    }

    fn page_event(&mut self, page: usize, event: PageEvent, shell: &mut Shell<Message>) -> Task<Message> {
        match event {
            PageEvent::Hover(zone) => {
                if let Some(doc) = self.doc.as_mut() {
                    let links = doc.links.get(&page).map_or(0, Vec::len);
                    doc.hover = zone.map(|zone| {
                        if zone < links {
                            Hover::Link(page, zone)
                        } else {
                            let pins: Vec<u64> = doc
                                .notes
                                .iter()
                                .filter(|n| n.page == page && n.mark == crate::doc::Mark::Comment)
                                .map(|n| n.id)
                                .collect();
                            Hover::Pin(page, pins.get(zone - links).copied().unwrap_or(0))
                        }
                    });
                }
                Task::none()
            }
            PageEvent::Click(x, y) => {
                let Some(doc) = self.doc.as_ref() else {
                    return Task::none();
                };
                if doc.placing.is_some() {
                    return self.form(super::Form::Drop(page, x, y), shell);
                }
                let link = doc
                    .links
                    .get(&page)
                    .and_then(|links| links.iter().find(|l| l.rect.contains(x, y)))
                    .map(|l| l.target.clone());
                if let Some(target) = link
                    && doc.tool.is_none_or(|t| t == crate::reader::Tool::Select)
                {
                    if let Target::Uri(_) = target {
                        return Task::none();
                    }
                    return self.read(Read::Follow(target), shell);
                }
                self.note(NoteMsg::Click(page, x, y), shell)
            }
            PageEvent::Select(words) => self.note(NoteMsg::Selected(page, words), shell),
            PageEvent::Stroke(points) => self.note(NoteMsg::Stroke(page, points), shell),
            PageEvent::Clear => self.note(NoteMsg::Clear, shell),
        }
    }

    pub fn find(&mut self, message: FindMsg) -> Task<Message> {
        let Some(doc) = self.doc.as_mut() else {
            return Task::none();
        };
        match message {
            FindMsg::Open => {
                let find = doc.find.get_or_insert_with(Find::new);
                find.open = true;
                if let Some(selection) = &doc.selection
                    && let Some(text) = doc.texts.get(&selection.page)
                {
                    find.query = text::quote(text, &selection.words);
                    return Task::batch([self.restart_find(), iced::widget::operation::focus(crate::view::FIND_ID)]);
                }
                iced::widget::operation::focus(crate::view::FIND_ID)
            }
            FindMsg::Query(query) => {
                doc.find.get_or_insert_with(Find::new).query = query;
                self.restart_find()
            }
            FindMsg::Case(on) => {
                doc.find.get_or_insert_with(Find::new).case = on;
                self.restart_find()
            }
            FindMsg::Whole(on) => {
                doc.find.get_or_insert_with(Find::new).whole = on;
                self.restart_find()
            }
            FindMsg::Scope(scope) => {
                doc.find.get_or_insert_with(Find::new).scope = scope;
                self.restart_find()
            }
            FindMsg::Close => {
                if let Some(find) = doc.find.as_mut() {
                    find.open = false;
                }
                Task::none()
            }
            FindMsg::Next | FindMsg::Previous => {
                let Some(find) = doc.find.as_mut() else {
                    return Task::none();
                };
                if find.hits.is_empty() {
                    return Task::none();
                }
                let len = find.hits.len();
                find.current = if matches!(message, FindMsg::Next) {
                    (find.current + 1) % len
                } else {
                    (find.current + len - 1) % len
                };
                let page = find.hits[find.current].page;
                self.jump(page, false)
            }
            FindMsg::Pick(index) => {
                let Some(find) = doc.find.as_mut() else {
                    return Task::none();
                };
                find.current = index;
                find.open = true;
                let page = find.hits.get(index).map_or(0, |h| h.page);
                self.jump(page, false)
            }
            FindMsg::Chunk(generation, next, pages) => {
                let Some(find) = doc.find.as_mut().filter(|f| f.generation == generation) else {
                    return Task::none();
                };
                for (page, text) in pages {
                    find.hits.extend(text::find(page, &text, &find.query, find.whole, find.case));
                    doc.texts.entry(page).or_insert(text);
                }
                find.scanned = next;
                let first_hit_page = (find.hits.len() == 1).then(|| find.hits[0].page);
                let more = self.continue_find(generation, next);
                let jump = first_hit_page.map_or_else(Task::none, |page| self.jump(page, false));
                Task::batch([more, jump])
            }
        }
    }

    fn restart_find(&mut self) -> Task<Message> {
        let Some(doc) = self.doc.as_mut() else {
            return Task::none();
        };
        let count = doc.count();
        let Some(find) = doc.find.as_mut() else {
            return Task::none();
        };
        find.generation += 1;
        find.hits.clear();
        find.current = 0;
        find.scanned = 0;
        if find.query.trim().chars().count() < 2 {
            return Task::none();
        }
        if find.scope == Scope::Notes {
            for note in &doc.notes {
                let hay = format!("{} {}", note.quote, note.comment);
                if text::mentions(&hay, &find.query, find.whole, find.case) {
                    find.hits.push(text::Hit {
                        page: note.page,
                        rects: note.rects.clone(),
                        before: String::new(),
                        matched: hay,
                        after: String::new(),
                    });
                }
            }
            find.scanned = count;
            return Task::none();
        }
        let generation = find.generation;
        self.continue_find(generation, 0)
    }

    fn continue_find(&mut self, generation: u64, from: usize) -> Task<Message> {
        let Some(doc) = self.doc.as_ref() else {
            return Task::none();
        };
        if from >= doc.count() {
            return Task::none();
        }
        let to = (from + FIND_CHUNK).min(doc.count());
        let known: Vec<(usize, PageText)> =
            (from..to).filter_map(|p| doc.texts.get(&p).map(|t| (p, t.clone()))).collect();
        let session = doc.index.clone();
        later(
            move || {
                let mut out = Vec::new();
                let Ok(mut session) = session.lock() else {
                    return out;
                };
                for page in from..to {
                    match known.iter().find(|(p, _)| *p == page) {
                        Some((_, text)) => out.push((page, text.clone())),
                        None => out.push((page, session.text(page))),
                    }
                }
                out
            },
            move |pages| Message::Find(FindMsg::Chunk(generation, to, pages)),
        )
    }

    #[must_use]
    pub fn doc_mut(&mut self) -> Option<&mut Doc> {
        self.doc.as_mut()
    }
}

mod extra;
mod library;
mod notes;
mod reading;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use glide_sdk::window::{Action, Window};
use glide_sdk::{Item, Lang, Menu, Paint, Shell};
use iced::widget::image::Handle;
use iced::{Subscription, Task, keyboard};

pub use self::extra::{Form, Org, PropsTab, Show};
pub use self::library::{Chip, Lib, Place};
pub use self::notes::NoteMsg;
pub use self::reading::{FindMsg, Read};
use crate::doc::{Hue, OpenError};
use crate::i18n::Key;
use crate::reader::Doc;
use crate::store::{self, Entry, Memory, Prefs};
use crate::worker::Session;

pub struct Slot<T>(pub Arc<Mutex<Option<T>>>);

impl<T> Slot<T> {
    pub fn new(value: T) -> Self {
        Self(Arc::new(Mutex::new(Some(value))))
    }

    #[must_use]
    pub fn take(&self) -> Option<T> {
        self.0.lock().ok().and_then(|mut slot| slot.take())
    }
}

impl<T> Clone for Slot<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> std::fmt::Debug for Slot<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Slot")
    }
}

pub type Sessions = Result<(Session, Session, u64), OpenError>;

#[derive(Debug, Clone)]
pub enum Screen {
    Library,
    Loading(PathBuf),
    Locked { path: PathBuf, text: String, wrong: bool, tries: u32, reveal: bool },
    Failed { path: PathBuf, error: OpenError },
    Reading,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum After {
    Stay,
    Quit,
    Library,
    Open(PathBuf),
}

#[derive(Debug, Clone)]
pub enum Message {
    Window(Action),
    Close,
    Copy(String),
    Ignore,
    Tick,
    Frame(Instant),
    ScaleFactor(f32),
    Library(Lib),
    OpenPath(PathBuf),
    Opened(u64, PathBuf, Slot<Sessions>),
    Read(Read),
    Find(FindMsg),
    Note(NoteMsg),
    Org(Org),
    Form(Form),
    Show(Show),
    Toast(String),
}

pub struct App {
    pub lang: Lang,
    pub paint: Paint,
    pub window: Window,
    pub screen: Screen,
    pub prefs: Prefs,
    pub memories: BTreeMap<String, Memory>,
    pub entries: Vec<Entry>,
    pub covers: BTreeMap<PathBuf, Option<Handle>>,
    pub covers_pending: Vec<PathBuf>,
    pub covering: bool,
    pub place: Place,
    pub chip: Chip,
    pub query: String,
    pub doc: Option<Doc>,
    pub generation: u64,
    pub scans: u64,
    pub password: Option<String>,
    pub props: Option<PropsTab>,
    pub page_input: Option<String>,
    pub modifiers: keyboard::Modifiers,
    pub dpr: f32,
    pub saved_memory: Option<(String, Memory)>,
    pub now: Instant,
    pub commented: bool,
    pub pointer: iced::Point,
}

pub async fn work<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    let spawned = std::thread::Builder::new().name("glide-folio-work".into()).spawn(move || {
        let _ = sender.send(job());
    });
    drop(spawned);
    receiver.await.ok()
}

pub fn later<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static, done: impl Fn(T) -> Message + Send + 'static,
) -> Task<Message> {
    Task::perform(work(job), move |result| result.map_or(Message::Ignore, &done))
}

#[must_use]
pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[must_use]
pub fn iso_date(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[must_use]
pub fn iso_now() -> String {
    glide_config::local_time().map_or_else(String::new, |t| {
        format!("{:04}-{:02}-{:02}T{:02}:{:02}", t.year, t.month, t.day, t.hour, t.minute)
    })
}

impl App {
    #[must_use]
    pub fn new(shell: &Shell<Message>) -> Self {
        Self {
            lang: shell.lang(),
            paint: *shell.paint(),
            window: *shell.window(),
            screen: Screen::Library,
            prefs: store::load_prefs(),
            memories: store::load_memories(),
            entries: Vec::new(),
            covers: BTreeMap::new(),
            covers_pending: Vec::new(),
            covering: false,
            place: Place::Recent,
            chip: Chip::All,
            query: String::new(),
            doc: None,
            generation: 0,
            scans: 0,
            password: None,
            props: None,
            page_input: None,
            modifiers: keyboard::Modifiers::default(),
            dpr: 1.0,
            saved_memory: None,
            now: Instant::now(),
            commented: false,
            pointer: iced::Point::ORIGIN,
        }
    }

    #[must_use]
    pub fn t(&self, key: Key) -> &'static str {
        key.text(self.lang)
    }

    pub fn update(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::Window(action) => {
                shell.window_action(action);
                Task::none()
            }
            Message::Close => self.leave(After::Quit, shell),
            Message::Copy(text) => {
                shell.toast(self.t(Key::Copied));
                iced::clipboard::write(text)
            }
            Message::Ignore => Task::none(),
            Message::Tick => self.tick(),
            Message::Frame(now) => {
                self.now = now;
                let pointer = shell.pointer();
                if pointer != self.pointer {
                    self.pointer = pointer;
                    if let Some(present) = self.doc.as_mut().and_then(|d| d.present.as_mut()) {
                        present.moved = now;
                    }
                }
                Task::none()
            }
            Message::ScaleFactor(factor) => {
                self.dpr = factor.max(1.0);
                if let Some(doc) = self.doc.as_mut() {
                    doc.dpr = self.dpr;
                }
                self.request_visible()
            }
            Message::Library(lib) => self.library(lib, shell),
            Message::OpenPath(path) => self.leave(After::Open(path), shell),
            Message::Opened(generation, path, slot) => self.opened(generation, path, &slot),
            Message::Read(read) => self.read(read, shell),
            Message::Find(find) => self.find(find),
            Message::Note(note) => self.note(note, shell),
            Message::Org(org) => self.org(org, shell),
            Message::Form(form) => self.form(form, shell),
            Message::Show(show) => self.show(show, shell),
            Message::Toast(text) => {
                shell.toast(text);
                Task::none()
            }
        }
    }

    pub fn leave(&mut self, after: After, shell: &mut Shell<Message>) -> Task<Message> {
        if let Some(doc) = &self.doc
            && (doc.dirty() || doc.form_dirty())
        {
            let mut dialog = glide_sdk::Dialog::new(
                crate::i18n::fill(self.t(Key::UnsavedTitle), &[("name", &doc.name)]),
                self.t(Key::UnsavedBody),
            )
            .choice(self.t(Key::DontSave), glide_sdk::Tone::Danger, Message::Note(NoteMsg::Discard(after.clone())))
            .cancel(self.t(Key::Cancel));
            if doc.editable() {
                dialog =
                    dialog.choice(self.t(Key::Save), glide_sdk::Tone::Accent, Message::Note(NoteMsg::SaveThen(after)));
            }
            shell.dialog(dialog);
            return Task::none();
        }
        self.proceed(after, shell)
    }

    pub fn proceed(&mut self, after: After, shell: &mut Shell<Message>) -> Task<Message> {
        match after {
            After::Stay => Task::none(),
            After::Quit => {
                self.remember_doc();
                shell.exit();
                Task::none()
            }
            After::Library => {
                self.remember_doc();
                self.doc = None;
                self.screen = Screen::Library;
                self.rescan()
            }
            After::Open(path) => self.load(path, None),
        }
    }

    pub fn remember_doc(&mut self) {
        self.note_doc(true);
    }

    fn note_doc(&mut self, eager: bool) {
        let Some(doc) = &self.doc else {
            return;
        };
        let mut memory = doc.lasting_memory();
        memory.opened = unix_now();
        memory.notes = doc.notes.len();
        memory.unsigned = doc.opened.fields.iter().enumerate().any(|(index, f)| {
            matches!(f.kind, crate::doc::FieldKind::Signature { signed: false, stamped: false, .. })
                && !doc.stamped(index)
        });
        memory.filled = doc.filled();
        let key = store::key(&doc.path);
        let unchanged = self.saved_memory.as_ref().is_some_and(|(known, saved)| {
            *known == key
                && if eager { *saved == memory } else { Memory { opened: saved.opened, ..memory.clone() } == *saved }
        });
        self.memories.insert(key.clone(), memory.clone());
        if !unchanged {
            self.saved_memory = Some((key, memory));
            drop(store::save_memories_later(self.memories.clone()));
        }
    }

    fn tick(&mut self) -> Task<Message> {
        self.now = Instant::now();
        if let Some(doc) = self.doc.as_mut()
            && let Some((_, since)) = doc.back_chip
            && since.elapsed() > Duration::from_secs(8)
        {
            doc.back_chip = None;
        }
        if self.doc.is_some() {
            self.note_doc(false);
        }
        self.pump_covers()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![iced::time::every(Duration::from_secs(2)).map(|_| Message::Tick)];
        if self.doc.as_ref().is_some_and(|doc| doc.present.is_some()) {
            subs.push(iced::time::every(Duration::from_millis(500)).map(|_| Message::Frame(Instant::now())));
        }
        Subscription::batch(subs)
    }

    #[must_use]
    pub fn hue_name(&self, hue: Hue) -> &'static str {
        self.t(match hue {
            Hue::Yellow => Key::Yellow,
            Hue::Green => Key::Green,
            Hue::Blue => Key::Blue,
            Hue::Red => Key::Red,
            Hue::Purple => Key::Purple,
        })
    }

    pub fn key(&mut self, event: &keyboard::Event, captured: bool, shell: &mut Shell<Message>) -> Task<Message> {
        use keyboard::key::Named;
        if let keyboard::Event::ModifiersChanged(modifiers) = event {
            self.modifiers = *modifiers;
            return Task::none();
        }
        let keyboard::Event::KeyPressed { key, modifiers, physical_key, .. } = event else {
            return Task::none();
        };
        if !captured
            && !shell.has_dialog()
            && let Some(message) = glide_sdk::menu::shortcut(&self.menus(), key, *physical_key, *modifiers)
        {
            return self.update(message, shell);
        }
        if captured || shell.has_dialog() {
            if matches!(key, keyboard::Key::Named(Named::Escape)) && self.doc.as_ref().is_some_and(|d| d.find.is_some())
            {
                return self.update(Message::Find(FindMsg::Close), shell);
            }
            return Task::none();
        }
        let reading = self.doc.is_some() && !modifiers.control() && !modifiers.alt() && !modifiers.logo();
        let mirrored = self.doc.as_ref().is_some_and(|d| d.memory.rtl);
        let step = |by: i32| Some(Message::Read(Read::Step(by)));
        let message = match key.as_ref() {
            keyboard::Key::Named(Named::Escape) => self.escape(),
            keyboard::Key::Named(Named::PageDown) if reading => Some(Message::Read(Read::Scroll(1))),
            keyboard::Key::Named(Named::PageUp) if reading => Some(Message::Read(Read::Scroll(-1))),
            keyboard::Key::Named(Named::ArrowRight) if reading => step(if mirrored { -1 } else { 1 }),
            keyboard::Key::Named(Named::ArrowLeft) if reading => step(if mirrored { 1 } else { -1 }),
            keyboard::Key::Named(Named::Space) if self.doc.as_ref().is_some_and(|d| d.present.is_some()) => {
                Some(Message::Show(Show::Slide(1)))
            }
            keyboard::Key::Named(Named::Home) if reading => Some(Message::Read(Read::Go(0))),
            keyboard::Key::Named(Named::End) if reading => Some(Message::Read(Read::Go(usize::MAX))),
            keyboard::Key::Named(Named::F3) if self.doc.is_some() => {
                Some(Message::Find(if modifiers.shift() { FindMsg::Previous } else { FindMsg::Next }))
            }
            _ => None,
        };
        message.map_or_else(Task::none, |message| self.update(message, shell))
    }

    fn escape(&self) -> Option<Message> {
        let doc = self.doc.as_ref()?;
        if doc.present.is_some() {
            return Some(Message::Show(Show::Exit));
        }
        if doc.bare {
            return Some(Message::Show(Show::Bare(false)));
        }
        if doc.signing.is_some() || doc.placing.is_some() {
            return Some(Message::Form(Form::SignCancel));
        }
        if doc.popover {
            return Some(Message::Read(Read::Popover(false)));
        }
        if doc.find.as_ref().is_some_and(|f| f.open) {
            return Some(Message::Find(FindMsg::Close));
        }
        if doc.organizer.is_some() {
            return Some(Message::Org(Org::Exit));
        }
        if doc.selection.is_some() || doc.focus.is_some() || doc.hover.is_some() {
            return Some(Message::Note(NoteMsg::Clear));
        }
        if doc.tool.is_some() {
            return Some(Message::Note(NoteMsg::Tool(None)));
        }
        if matches!(doc.ocr, crate::reader::Ocr::Running { .. } | crate::reader::Ocr::Starting) {
            return Some(Message::Show(Show::OcrDismiss));
        }
        None
    }

    #[must_use]
    #[allow(clippy::too_many_lines)] // The six application menus listed in order; each row is one builder chain
    pub fn menus(&self) -> Vec<Menu<Message>> {
        let doc = self.doc.as_ref();
        let reading = doc.is_some() && matches!(self.screen, Screen::Reading);
        let editable = doc.is_some_and(Doc::editable);
        vec![
            Menu::new(
                self.t(Key::FileMenu),
                vec![
                    Item::action(self.t(Key::OpenFile), Message::Library(Lib::OpenDialog)).shortcut("Ctrl+O"),
                    Item::action(self.t(Key::Save), Message::Note(NoteMsg::Save))
                        .shortcut("Ctrl+S")
                        .enabled(doc.is_some_and(|d| (d.dirty() || d.form_dirty()) && d.editable())),
                    Item::action(self.t(Key::SaveCopy), Message::Show(Show::SaveCopy))
                        .shortcut("Ctrl+Shift+S")
                        .enabled(reading),
                    Item::action(self.t(Key::Print), Message::Show(Show::Print)).shortcut("Ctrl+P").enabled(reading),
                    Item::separator(),
                    Item::action(self.t(Key::Properties), Message::Show(Show::Properties(true)))
                        .shortcut("Alt+Enter")
                        .enabled(reading),
                    Item::action(self.t(Key::ShowInFiles), Message::Show(Show::Reveal)).enabled(reading),
                    Item::separator(),
                    Item::action(self.t(Key::BackToLibrary), Message::Read(Read::Library))
                        .shortcut("Ctrl+W")
                        .enabled(reading),
                    Item::action(self.t(Key::Quit), Message::Close).shortcut("Ctrl+Q"),
                ],
            ),
            Menu::new(
                self.t(Key::EditMenu),
                vec![
                    Item::action(self.t(Key::Undo), Message::Note(NoteMsg::Undo))
                        .shortcut("Ctrl+Z")
                        .enabled(doc.is_some_and(|d| !d.undo.is_empty())),
                    Item::separator(),
                    Item::action(self.t(Key::Copy), Message::Note(NoteMsg::CopySelection))
                        .shortcut("Ctrl+C")
                        .enabled(doc.is_some_and(|d| d.selection.is_some())),
                    Item::action(self.t(Key::Find), Message::Find(FindMsg::Open)).shortcut("Ctrl+F").enabled(reading),
                    Item::action(self.t(Key::NextMatch), Message::Find(FindMsg::Next)).enabled(reading),
                    Item::action(self.t(Key::PreviousMatch), Message::Find(FindMsg::Previous)).enabled(reading),
                ],
            ),
            Menu::new(
                self.t(Key::ViewMenu),
                vec![
                    Item::action(self.t(Key::SidePanel), Message::Read(Read::ToggleSide))
                        .shortcut("F9")
                        .enabled(reading)
                        .checked(doc.is_some_and(|d| d.memory.side)),
                    Item::separator(),
                    Item::action(self.t(Key::ZoomIn), Message::Read(Read::ZoomBy(1)))
                        .shortcut("Ctrl+=")
                        .enabled(reading),
                    Item::action(self.t(Key::ZoomOut), Message::Read(Read::ZoomBy(-1)))
                        .shortcut("Ctrl+-")
                        .enabled(reading),
                    Item::action(self.t(Key::FitWidth), Message::Read(Read::Zoom(store::Zoom::FitWidth)))
                        .shortcut("Ctrl+2")
                        .enabled(reading),
                    Item::action(self.t(Key::FitHeight), Message::Read(Read::Zoom(store::Zoom::FitHeight)))
                        .shortcut("Ctrl+3")
                        .enabled(reading),
                    Item::action(self.t(Key::FitPage), Message::Read(Read::Zoom(store::Zoom::FitPage)))
                        .shortcut("Ctrl+0")
                        .enabled(reading),
                    Item::action(self.t(Key::ActualSize), Message::Read(Read::Zoom(store::Zoom::Percent(100))))
                        .shortcut("Ctrl+1")
                        .enabled(reading),
                    Item::separator(),
                    Item::action(self.t(Key::LayoutSingle), Message::Read(Read::Layout(store::Layout::Single)))
                        .enabled(reading),
                    Item::action(self.t(Key::LayoutContinuous), Message::Read(Read::Layout(store::Layout::Continuous)))
                        .enabled(reading),
                    Item::action(self.t(Key::LayoutSpread), Message::Read(Read::Layout(store::Layout::Spread)))
                        .enabled(reading),
                    Item::separator(),
                    Item::action(self.t(Key::Presentation), Message::Show(Show::Present))
                        .shortcut("F5")
                        .enabled(reading),
                    Item::action(self.t(Key::Fullscreen), Message::Show(Show::Bare(true)))
                        .shortcut("F11")
                        .enabled(reading),
                ],
            ),
            Menu::new(
                self.t(Key::GoMenu),
                vec![
                    Item::action(self.t(Key::FirstPage), Message::Read(Read::Go(0))).enabled(reading),
                    Item::action(self.t(Key::PreviousPage), Message::Read(Read::Step(-1))).enabled(reading),
                    Item::action(self.t(Key::NextPage), Message::Read(Read::Step(1))).enabled(reading),
                    Item::action(self.t(Key::LastPage), Message::Read(Read::Go(usize::MAX))).enabled(reading),
                    Item::separator(),
                    Item::action(self.t(Key::Back), Message::Read(Read::Back))
                        .shortcut("Alt+Left")
                        .enabled(doc.is_some_and(|d| !d.history.is_empty())),
                    Item::action(self.t(Key::Forward), Message::Read(Read::Forward))
                        .shortcut("Alt+Right")
                        .enabled(doc.is_some_and(|d| !d.future.is_empty())),
                    Item::separator(),
                    Item::action(self.t(Key::AddBookmark), Message::Note(NoteMsg::Bookmark))
                        .shortcut("Ctrl+D")
                        .enabled(reading),
                ],
            ),
            Menu::new(
                self.t(Key::NotesMenu),
                vec![
                    Item::action(
                        self.t(Key::ToolHighlight),
                        Message::Note(NoteMsg::Tool(Some(crate::reader::Tool::Highlight))),
                    )
                    .enabled(reading),
                    Item::action(
                        self.t(Key::ToolUnderline),
                        Message::Note(NoteMsg::Tool(Some(crate::reader::Tool::Underline))),
                    )
                    .enabled(reading),
                    Item::action(
                        self.t(Key::ToolComment),
                        Message::Note(NoteMsg::Tool(Some(crate::reader::Tool::Comment))),
                    )
                    .enabled(reading),
                    Item::separator(),
                    Item::action(self.t(Key::ExportMarkdown), Message::Note(NoteMsg::Export))
                        .enabled(doc.is_some_and(|d| !d.notes.is_empty())),
                    Item::action(self.t(Key::Organise), Message::Org(Org::Start)).enabled(reading && editable),
                    Item::action(self.t(Key::FillSign), Message::Form(Form::Mode(true))).enabled(reading && editable),
                ],
            ),
            Menu::new(
                self.t(Key::HelpMenu),
                vec![Item::action(self.t(Key::Shortcuts), Message::Show(Show::Shortcuts)).shortcut("F1")],
            ),
        ]
    }

    #[must_use]
    pub fn subtitle(&self) -> String {
        match &self.doc {
            Some(doc) if matches!(self.screen, Screen::Reading) => doc.name.clone(),
            _ => String::new(),
        }
    }
}

impl glide_sdk::App for App {
    type Message = Message;

    fn new(shell: &mut Shell<Message>) -> (Self, Task<Message>) {
        let mut app = Self::new(shell);
        let scan = app.rescan();
        let args: Vec<String> = std::env::args().skip(1).collect();
        let open = app.reopened(&args, shell);
        let scale = iced::window::latest().and_then(iced::window::scale_factor).map(Message::ScaleFactor);
        (app, Task::batch([scan, open, scale]))
    }

    fn title(&self, _: &Shell<Message>) -> String {
        match (&self.doc, &self.screen) {
            (Some(doc), Screen::Reading) => format!("{} — {}", doc.name, self.t(Key::AppName)),
            _ => self.t(Key::AppName).to_string(),
        }
    }

    fn update(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        Self::update(self, message, shell)
    }

    fn view<'a>(&'a self, _: &'a Shell<Message>) -> iced::Element<'a, Message> {
        crate::view::view(self)
    }

    fn subscription(&self) -> Subscription<Message> {
        Self::subscription(self)
    }

    fn menus(&self, _: &Shell<Message>) -> Vec<Menu<Message>> {
        Self::menus(self)
    }

    fn window_changed(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        self.window = *shell.window();
        if self.fit_viewport() { Task::batch([self.request_visible(), self.request_thumbs()]) } else { Task::none() }
    }

    fn key(&mut self, event: &keyboard::Event, captured: bool, shell: &mut Shell<Message>) -> Task<Message> {
        Self::key(self, event, captured, shell)
    }

    fn paint(&self, _: &Shell<Message>) -> Paint {
        self.paint
    }

    fn look_changed(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        self.lang = shell.lang();
        self.paint = *shell.paint();
        Task::none()
    }

    fn close_requested(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        self.leave(After::Quit, shell)
    }

    fn reopened(&mut self, args: Vec<String>, shell: &mut Shell<Message>) -> Task<Message> {
        Self::reopened(self, &args, shell)
    }
}

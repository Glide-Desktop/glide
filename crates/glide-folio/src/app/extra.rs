use std::path::{Path, PathBuf};
use std::time::Instant;

use glide_sdk::{Item, Shell};
use iced::Task;
use iced::widget::image::Handle;

use super::{After, App, FindMsg, Message, NoteMsg, Read, later};
use crate::doc::write::{self, PageSource, Signature};
use crate::doc::{FieldKind, Mark, Rect};
use crate::i18n::{Key, fill};
use crate::icons::Icon;
use crate::reader::{Ocr, Organizer, Present, Signing, Stamp, ZOOM_LEVELS, fit_strokes, free_rect};
use crate::store::{Layout, Paper, Zoom};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropsTab {
    General,
    Fonts,
    Security,
}

#[derive(Debug, Clone)]
pub enum Org {
    Start,
    Exit,
    Pick(usize),
    Rotate(i64),
    Remove,
    Clear,
    Reset,
    Insert,
    Chosen(PathBuf),
    Inserted(u64, Option<(PathBuf, Vec<Option<Handle>>)>),
    Scrolled(f32),
    Save,
    Extract,
    Target(bool, PathBuf),
    Done(Result<PathBuf, String>),
}

#[derive(Debug, Clone)]
pub enum Form {
    Mode(bool),
    Value(String, String),
    Toggle(String),
    Sign(Option<usize>),
    Stroke(Vec<(f32, f32)>),
    Erase,
    SignCancel,
    Place,
    Drop(usize, f32, f32),
    Clear,
    Save,
    Saved(u64, Result<(), String>, After),
}

#[derive(Debug, Clone)]
pub enum Show {
    Present,
    Slide(i32),
    Pause,
    Exit,
    Bare(bool),
    Properties(bool),
    Tab(PropsTab),
    Print,
    Printed(bool),
    SaveCopy,
    CopyTarget(PathBuf),
    Reveal,
    OpenWith,
    Shortcuts,
    More,
    ZoomMenu,
    Ocr,
    OcrPage(u64, usize, Option<crate::doc::PageText>),
    OcrReady(u64, PathBuf, Option<String>),
    OcrDismiss,
    Recover,
    Moved,
}

fn insertable(path: &Path) -> Option<(PathBuf, Vec<Option<Handle>>)> {
    if crate::doc::Format::of(path) != Some(crate::doc::Format::Pdf) {
        return None;
    }
    let mut session = crate::worker::Session::open(path, None).ok()?;
    if session.opened().info.encrypted {
        return None;
    }
    let widths: Vec<f32> = session.opened().pages.iter().map(|size| size.w).collect();
    let thumbs = widths
        .iter()
        .enumerate()
        .map(|(page, width)| {
            session.render(page, crate::reader::THUMB_SCALE_WIDTH / width.max(1.0)).map(|image| {
                let (w, h) = image.dimensions();
                Handle::from_rgba(w, h, image.into_raw())
            })
        })
        .collect();
    Some((path.to_path_buf(), thumbs))
}

fn recognise(
    session: &std::sync::Mutex<crate::worker::Session>, tool: &Path, langs: &str, page: usize,
) -> Option<crate::doc::PageText> {
    use std::io::Write as _;
    let (image, scale) = {
        let mut session = session.lock().ok()?;
        let image = session.render(page, 2.0)?;
        let scale = super::reading::rendered_scale(&session, page, image.width(), 2.0);
        (image, scale)
    };
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).ok()?;
    let mut child = std::process::Command::new(tool)
        .args(["stdin", "stdout", "-l", langs, "tsv"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let fed = child.stdin.take().is_some_and(|mut stdin| stdin.write_all(png.get_ref()).is_ok());
    let output = child.wait_with_output().ok()?;
    (fed && output.status.success()).then(|| crate::ocr::parse_tsv(&String::from_utf8_lossy(&output.stdout), scale))
}

fn ocr_page(
    session: std::sync::Arc<std::sync::Mutex<crate::worker::Session>>, tool: PathBuf, langs: String, generation: u64,
    page: usize,
) -> Task<Message> {
    later(
        move || recognise(&session, &tool, &langs, page),
        move |text| Message::Show(Show::OcrPage(generation, page, text)),
    )
}

fn copy_with_notes(source: &Path, target: &Path, notes: Option<&[crate::doc::Note]>) -> bool {
    if let Some(notes) = notes {
        return write::save_notes(source, target, notes).is_ok();
    }
    let same = std::fs::canonicalize(source).ok().zip(std::fs::canonicalize(target).ok()).is_some_and(|(a, b)| a == b);
    same || std::fs::copy(source, target).is_ok()
}

fn glyph(item: Item<Message>, icon: Icon) -> Item<Message> {
    item.glyph(icon.handle())
}

fn save_dialog(
    title: &str, accept: &str, name: String, folder: Option<PathBuf>,
    done: impl Fn(PathBuf) -> Message + Send + 'static,
) -> Task<Message> {
    let ask = glide_sdk::chooser::Ask {
        purpose: glide_sdk::chooser::Purpose::Save,
        title: title.into(),
        accept: accept.into(),
        name: Some(name),
        folder,
    };
    let receiver = glide_sdk::chooser::spawn(ask);
    Task::perform(async move { receiver.await.ok() }, move |chosen| match chosen {
        Some(glide_sdk::chooser::Chosen::Paths(paths)) => paths.into_iter().next().map_or(Message::Ignore, &done),
        _ => Message::Ignore,
    })
}

impl App {
    #[must_use]
    pub fn more_menu(&self) -> Vec<Item<Message>> {
        let Some(doc) = self.doc.as_ref() else {
            return Vec::new();
        };
        let paper = doc.memory.paper;
        let editable = doc.editable();
        let has_text = doc.has_text();
        let mut recognise = glyph(Item::action(self.t(Key::Recognise), Message::Show(Show::Ocr)), Icon::Scan)
            .enabled(!has_text && !doc.reflow());
        if has_text {
            recognise = recognise.shortcut(self.t(Key::HasText));
        }
        vec![
            Item::heading(self.t(Key::ViewHeading)),
            glyph(Item::action(self.t(Key::PaperWhite), Message::Read(Read::Paper(Paper::White))), Icon::Paper)
                .checked(paper == Paper::White),
            glyph(Item::action(self.t(Key::PaperSepia), Message::Read(Read::Paper(Paper::Sepia))), Icon::Paper)
                .checked(paper == Paper::Sepia),
            glyph(Item::action(self.t(Key::PaperNight), Message::Read(Read::Paper(Paper::Night))), Icon::Paper)
                .checked(paper == Paper::Night),
            glyph(Item::action(self.t(Key::Presentation), Message::Show(Show::Present)), Icon::Screen).shortcut("F5"),
            glyph(Item::action(self.t(Key::Fullscreen), Message::Show(Show::Bare(true))), Icon::Fullscreen)
                .shortcut("F11"),
            Item::separator(),
            Item::heading(self.t(Key::DocumentHeading)),
            glyph(Item::action(self.t(Key::Print), Message::Show(Show::Print)), Icon::Print).shortcut("Ctrl+P"),
            glyph(Item::action(self.t(Key::SaveCopy), Message::Show(Show::SaveCopy)), Icon::Save)
                .shortcut("Ctrl+Shift+S"),
            glyph(Item::action(self.t(Key::Organise), Message::Org(Org::Start)), Icon::Pages).enabled(editable),
            glyph(Item::action(self.t(Key::FillSign), Message::Form(Form::Mode(true))), Icon::Sign).enabled(editable),
            recognise,
            glyph(Item::action(self.t(Key::Properties), Message::Show(Show::Properties(true))), Icon::Info)
                .shortcut("Alt+Enter"),
            Item::separator(),
            glyph(Item::action(self.t(Key::ShowInFiles), Message::Show(Show::Reveal)), Icon::Folder),
            glyph(Item::action(self.t(Key::OpenWith), Message::Show(Show::OpenWith)), Icon::External),
            glyph(Item::action(self.t(Key::Shortcuts), Message::Show(Show::Shortcuts)), Icon::Keyboard).shortcut("F1"),
        ]
    }

    #[must_use]
    pub fn zoom_menu(&self) -> Vec<Item<Message>> {
        let Some(doc) = self.doc.as_ref() else {
            return Vec::new();
        };
        let zoom = doc.memory.zoom;
        let mut items = vec![
            glyph(Item::action(self.t(Key::FitWidth), Message::Read(Read::Zoom(Zoom::FitWidth))), Icon::FitWidth)
                .checked(zoom == Zoom::FitWidth)
                .shortcut("Ctrl+2"),
            glyph(Item::action(self.t(Key::FitHeight), Message::Read(Read::Zoom(Zoom::FitHeight))), Icon::FitHeight)
                .checked(zoom == Zoom::FitHeight)
                .shortcut("Ctrl+3"),
            glyph(Item::action(self.t(Key::FitPage), Message::Read(Read::Zoom(Zoom::FitPage))), Icon::FitPage)
                .checked(zoom == Zoom::FitPage)
                .shortcut("Ctrl+0"),
            glyph(Item::action(self.t(Key::ActualSize), Message::Read(Read::Zoom(Zoom::Percent(100)))), Icon::Standard)
                .checked(zoom == Zoom::Percent(100))
                .shortcut("Ctrl+1"),
            Item::separator(),
        ];
        for (index, level) in ZOOM_LEVELS.iter().enumerate() {
            items.push(
                glyph(Item::action(format!("{level} %"), Message::Read(Read::ZoomLevel(index))), Icon::Actual)
                    .checked(zoom == Zoom::Percent(*level)),
            );
        }
        items
    }

    #[must_use]
    pub fn page_menu(&self) -> Vec<Item<Message>> {
        let Some(doc) = self.doc.as_ref() else {
            return Vec::new();
        };
        let page = doc.page() + 1;
        let bookmark = glyph(
            Item::action(
                fill(self.t(Key::BookmarkPage), &[("n", &page.to_string())]),
                Message::Note(NoteMsg::Bookmark),
            ),
            Icon::Bookmark,
        )
        .shortcut("Ctrl+D");
        let Some(selection) = doc.selection.as_ref() else {
            return vec![
                glyph(Item::action(self.t(Key::Find), Message::Find(FindMsg::Open)), Icon::Search).shortcut("Ctrl+F"),
                bookmark,
                Item::separator(),
                glyph(Item::action(self.t(Key::Properties), Message::Show(Show::Properties(true))), Icon::Info)
                    .shortcut("Alt+Enter"),
            ];
        };
        let quote =
            doc.texts.get(&selection.page).map(|t| crate::doc::text::quote(t, &selection.words)).unwrap_or_default();
        let short: String = quote.chars().take(24).collect();
        let swatches = crate::doc::Hue::ALL
            .iter()
            .map(|hue| glide_sdk::menu::Swatch {
                label: self.hue_name(*hue).to_string(),
                color: crate::view::hue_color(*hue),
                on: *hue == doc.hue,
                message: Message::Note(NoteMsg::Hue(*hue)),
            })
            .collect();
        vec![
            glyph(Item::action(self.t(Key::Copy), Message::Note(NoteMsg::CopySelection)), Icon::Copy)
                .shortcut("Ctrl+C"),
            Item::submenu(
                self.t(Key::ToolHighlight),
                vec![
                    Item::heading(self.t(Key::Colour)),
                    Item::swatches(swatches),
                    Item::separator(),
                    glyph(
                        Item::action(self.t(Key::ToolHighlight), Message::Note(NoteMsg::Mark(Mark::Highlight))),
                        Icon::Highlight,
                    ),
                    glyph(
                        Item::action(self.t(Key::HighlightAndComment), Message::Note(NoteMsg::Comment)),
                        Icon::Comment,
                    ),
                ],
            )
            .glyph(Icon::Highlight.handle()),
            glyph(
                Item::action(self.t(Key::ToolUnderline), Message::Note(NoteMsg::Mark(Mark::Underline))),
                Icon::Underline,
            ),
            glyph(Item::action(self.t(Key::ToolStrike), Message::Note(NoteMsg::Mark(Mark::Strike))), Icon::Strike),
            glyph(
                Item::action(format!("{}…", self.t(Key::ToolComment)), Message::Note(NoteMsg::Comment)),
                Icon::Comment,
            ),
            Item::separator(),
            glyph(
                Item::action(
                    fill(self.t(Key::FindSelection), &[("text", &short)]),
                    Message::Note(NoteMsg::FindSelection),
                ),
                Icon::Search,
            ),
            glyph(Item::action(self.t(Key::SearchWeb), Message::Note(NoteMsg::WebSearch)), Icon::Globe),
            Item::separator(),
            bookmark,
        ]
    }

    #[allow(clippy::too_many_lines)] // One arm per organiser message
    pub fn org(&mut self, message: Org, shell: &mut Shell<Message>) -> Task<Message> {
        let lang = self.lang;
        let t = move |key: Key| key.text(lang);
        let Some(doc) = self.doc.as_mut() else {
            return Task::none();
        };
        match message {
            Org::Start => {
                doc.organizer = Some(Organizer::new(doc.count()));
                doc.find = None;
                self.thumbs_range(0, 96)
            }
            Org::Exit => {
                doc.organizer = None;
                Task::none()
            }
            Org::Pick(index) => {
                let (range, toggle) = (self.modifiers.shift(), self.modifiers.control());
                if let Some(org) = doc.organizer.as_mut() {
                    org.pick(index, range, toggle);
                }
                Task::none()
            }
            Org::Rotate(by) => {
                if let Some(org) = doc.organizer.as_mut() {
                    org.rotate(by);
                }
                Task::none()
            }
            Org::Remove => {
                if let Some(org) = doc.organizer.as_mut() {
                    org.remove();
                }
                Task::none()
            }
            Org::Clear => {
                if let Some(org) = doc.organizer.as_mut() {
                    org.selected.clear();
                }
                Task::none()
            }
            Org::Reset => {
                doc.organizer = Some(Organizer::new(doc.count()));
                Task::none()
            }
            Org::Scrolled(fraction) => {
                let first = (fraction.clamp(0.0, 1.0) * doc.count() as f32) as usize;
                self.thumbs_range(first.saturating_sub(24), first + 72)
            }
            Org::Insert => {
                let ask = glide_sdk::chooser::Ask {
                    purpose: glide_sdk::chooser::Purpose::Open,
                    title: t(Key::InsertFromFile).trim_end_matches('…').into(),
                    accept: t(Key::Open).into(),
                    name: None,
                    folder: doc.path.parent().map(std::path::Path::to_path_buf),
                };
                let receiver = glide_sdk::chooser::spawn(ask);
                let generation = doc.generation;
                Task::perform(async move { receiver.await.ok() }, |chosen| match chosen {
                    Some(glide_sdk::chooser::Chosen::Paths(paths)) => {
                        paths.into_iter().next().map_or(Message::Ignore, |path| Message::Org(Org::Chosen(path)))
                    }
                    _ => Message::Ignore,
                })
                .then(move |message| match message {
                    Message::Org(Org::Chosen(path)) => {
                        later(move || insertable(&path), move |found| Message::Org(Org::Inserted(generation, found)))
                    }
                    _ => Task::none(),
                })
            }
            Org::Chosen(_) => Task::none(),
            Org::Inserted(generation, found) => {
                if doc.generation != generation {
                    return Task::none();
                }
                let Some((path, thumbs)) = found else {
                    shell.toast(self.t(Key::Unsupported));
                    return Task::none();
                };
                if let Some(org) = doc.organizer.as_mut() {
                    let at = org.selected.iter().next_back().map_or(org.pages.len(), |last| last + 1);
                    let count = thumbs.len();
                    for (page, thumb) in thumbs.into_iter().enumerate().rev() {
                        if let Some(thumb) = thumb {
                            org.extra.insert((path.clone(), page), thumb);
                        }
                        org.pages.insert(
                            at,
                            crate::reader::OrgPage {
                                source: PageSource::Other(path.clone(), page),
                                rotate: 0,
                                removed: false,
                            },
                        );
                    }
                    org.selected = (at..at + count).collect();
                    org.anchor = Some(at);
                }
                Task::none()
            }
            Org::Save | Org::Extract => {
                let extract = matches!(message, Org::Extract);
                let stem = doc.path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
                let name = format!(
                    "{stem} ({}).pdf",
                    if extract {
                        t(Key::Pages).to_lowercase()
                    } else {
                        t(Key::SaveCopy).trim_end_matches('…').to_lowercase()
                    }
                );
                save_dialog(
                    t(Key::SaveCopy).trim_end_matches('…'),
                    t(Key::Save),
                    name,
                    doc.path.parent().map(std::path::Path::to_path_buf),
                    move |path| Message::Org(Org::Target(extract, path)),
                )
            }
            Org::Target(extract, target) => {
                let Some(org) = doc.organizer.as_ref() else {
                    return Task::none();
                };
                let ops = org.ops(extract);
                let source = doc.path.clone();
                later(
                    move || {
                        write::organize(&source, &ops, &target).map(|()| target.clone()).map_err(|e| format!("{e:?}"))
                    },
                    |result| Message::Org(Org::Done(result)),
                )
            }
            Org::Done(result) => {
                match result {
                    Ok(path) => {
                        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                        shell.toast_with(
                            fill(self.t(Key::SavedCopy), &[("name", &name)]),
                            self.t(Key::Open),
                            Message::OpenPath(path),
                        );
                    }
                    Err(_) => shell.toast(self.t(Key::SaveFailed)),
                }
                Task::none()
            }
        }
    }

    #[allow(clippy::too_many_lines)] // One arm per form message
    pub fn form(&mut self, message: Form, shell: &mut Shell<Message>) -> Task<Message> {
        let Some(doc) = self.doc.as_mut() else {
            return Task::none();
        };
        match message {
            Form::Mode(on) => {
                doc.form = on;
                doc.memory.side = true;
                Task::none()
            }
            Form::Value(name, value) => {
                doc.values.insert(name, value);
                Task::none()
            }
            Form::Toggle(name) => {
                let on = doc.values.get(&name).is_some_and(|v| v == "true");
                doc.values.insert(name, (!on).to_string());
                Task::none()
            }
            Form::Sign(field) => {
                if !doc.editable() {
                    shell.toast(self.t(Key::EncryptedSave));
                    return Task::none();
                }
                doc.placing = None;
                doc.tool = None;
                doc.signing = Some(Signing { field, strokes: Vec::new() });
                Task::none()
            }
            Form::Stroke(points) => {
                if let Some(signing) = doc.signing.as_mut() {
                    signing.strokes.push(points);
                }
                Task::none()
            }
            Form::Erase => {
                if let Some(signing) = doc.signing.as_mut() {
                    signing.strokes.clear();
                }
                Task::none()
            }
            Form::SignCancel => {
                doc.signing = None;
                doc.placing = None;
                Task::none()
            }
            Form::Place => {
                let Some(signing) = doc.signing.take() else {
                    return Task::none();
                };
                if signing.strokes.iter().all(|stroke| stroke.len() < 2) {
                    return Task::none();
                }
                let Some(index) = signing.field else {
                    doc.placing = Some(signing.strokes);
                    return Task::none();
                };
                let Some(field) = doc.opened.fields.get(index) else {
                    return Task::none();
                };
                let stamp = Stamp {
                    field: Some(index),
                    page: field.page,
                    rect: field.rect,
                    strokes: fit_strokes(&signing.strokes, field.rect),
                };
                doc.stamps.retain(|stamp| stamp.field != Some(index));
                doc.stamps.push(stamp);
                Task::none()
            }
            Form::Drop(page, x, y) => {
                let Some(strokes) = doc.placing.take() else {
                    return Task::none();
                };
                let rect = free_rect(&strokes, doc.size(page), x, y);
                doc.stamps.push(Stamp { field: None, page, rect, strokes: fit_strokes(&strokes, rect) });
                doc.form = true;
                Task::none()
            }
            Form::Clear => {
                for field in &doc.opened.fields {
                    let empty =
                        if matches!(field.kind, FieldKind::Check(_)) { "false".to_string() } else { String::new() };
                    doc.values.insert(field.name.clone(), empty);
                }
                doc.stamps.clear();
                Task::none()
            }
            Form::Save => self.save_form(After::Stay, shell),
            Form::Saved(generation, result, after) => {
                if doc.generation != generation {
                    return Task::none();
                }
                if result.is_err() {
                    shell.toast(self.t(Key::SaveFailed));
                    return Task::none();
                }
                if after == After::Stay {
                    let path = doc.path.clone();
                    return self.load(path, self.password.clone());
                }
                self.proceed(after, shell)
            }
        }
    }

    fn pending_signatures(&self) -> Vec<Signature> {
        let Some(doc) = self.doc.as_ref() else {
            return Vec::new();
        };
        doc.stamps
            .iter()
            .map(|stamp| Signature {
                field: stamp.field.and_then(|f| doc.opened.fields.get(f)).map(|f| f.name.clone()),
                page: stamp.page,
                rect: stamp.rect,
                strokes: stamp.strokes.clone(),
            })
            .collect()
    }

    pub(super) fn save_form(&mut self, after: After, shell: &mut Shell<Message>) -> Task<Message> {
        let Some(doc) = self.doc.as_ref() else {
            return Task::none();
        };
        if !doc.editable() {
            shell.toast(self.t(Key::EncryptedSave));
            return Task::none();
        }
        let values: Vec<(String, String)> = doc.values.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let signatures = self.pending_signatures();
        let notes = doc.dirty().then(|| doc.notes.clone());
        let (path, generation) = (doc.path.clone(), doc.generation);
        later(
            move || write::fill(&path, &values, &signatures, notes.as_deref(), &path).map_err(|e| format!("{e:?}")),
            move |result| Message::Form(Form::Saved(generation, result, after.clone())),
        )
    }

    #[allow(clippy::too_many_lines)] // One arm per message of the presentation, dialogs and document actions
    pub fn show(&mut self, message: Show, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Show::Present => {
                let Some(doc) = self.doc.as_mut() else {
                    return Task::none();
                };
                if doc.present.is_some() {
                    return Task::none();
                }
                let now = Instant::now();
                doc.present = Some(Present { started: now, paused: None, moved: now });
                doc.restore.get_or_insert((doc.memory.layout, doc.memory.zoom));
                doc.memory.zoom = Zoom::FitPage;
                doc.memory.layout = Layout::Single;
                shell.window_action(glide_sdk::window::Action::ToggleFullscreen);
                self.request_visible()
            }
            Show::Slide(by) => self.read(Read::Step(by), shell),
            Show::Pause => {
                if let Some(present) = self.doc.as_mut().and_then(|d| d.present.as_mut()) {
                    present.paused = match present.paused {
                        Some(paused) => {
                            present.started += paused.elapsed();
                            None
                        }
                        None => Some(Instant::now()),
                    };
                }
                Task::none()
            }
            Show::Exit => {
                if let Some(doc) = self.doc.as_mut()
                    && doc.present.take().is_some()
                {
                    doc.leave_full_screen_mode();
                    shell.window_action(glide_sdk::window::Action::ToggleFullscreen);
                }
                self.request_visible()
            }
            Show::Bare(on) => {
                if let Some(doc) = self.doc.as_mut()
                    && doc.bare != on
                {
                    shell.window_action(glide_sdk::window::Action::ToggleFullscreen);
                    doc.bare = on;
                    if on {
                        doc.restore.get_or_insert((doc.memory.layout, doc.memory.zoom));
                        doc.memory.zoom = Zoom::FitPage;
                    } else {
                        doc.leave_full_screen_mode();
                    }
                }
                self.request_visible()
            }
            Show::Properties(open) => {
                self.props = open.then_some(PropsTab::General);
                Task::none()
            }
            Show::Tab(tab) => {
                self.props = Some(tab);
                Task::none()
            }
            Show::Print => {
                let Some(doc) = self.doc.as_ref() else {
                    return Task::none();
                };
                let format = doc.opened.format;
                let converted = crate::doc::convert::tool_for(format).is_some();
                if format != crate::doc::Format::Pdf && !converted {
                    shell.toast(self.t(Key::PrintUnsupported));
                    return Task::none();
                }
                let source = doc.path.clone();
                let title = doc.name.clone();
                let writable = !doc.opened.info.encrypted;
                let notes =
                    (writable && (doc.dirty() || converted && !doc.notes.is_empty())).then(|| doc.notes.clone());
                let values: Vec<(String, String)> = if writable && doc.form_dirty() && !converted {
                    doc.values.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                } else {
                    Vec::new()
                };
                let signatures = if writable && !converted { self.pending_signatures() } else { Vec::new() };
                later(
                    move || {
                        let pdf =
                            if converted { crate::doc::convert::to_pdf(&source, format).ok() } else { Some(source) };
                        let Some(pdf) = pdf else {
                            return false;
                        };
                        if notes.is_none() && values.is_empty() && signatures.is_empty() {
                            return crate::print::print(&pdf, &title);
                        }
                        let private = std::env::var_os("XDG_RUNTIME_DIR")
                            .map(PathBuf::from)
                            .filter(|dir| dir.is_dir())
                            .unwrap_or_else(crate::doc::convert::cache_dir);
                        let _ = std::fs::create_dir_all(&private);
                        let copy = crate::doc::convert::scratch(&private.join("glide-folio-print.pdf"));
                        let ok = write::fill(&pdf, &values, &signatures, notes.as_deref(), &copy).is_ok()
                            && crate::print::print(&copy, &title);
                        let _ = std::fs::remove_file(&copy);
                        ok
                    },
                    |ok| Message::Show(Show::Printed(ok)),
                )
            }
            Show::Printed(ok) => {
                if !ok {
                    shell.toast(self.t(Key::PrintFailed));
                }
                Task::none()
            }
            Show::SaveCopy => {
                let Some(doc) = self.doc.as_ref() else {
                    return Task::none();
                };
                let name = if crate::doc::convert::tool_for(doc.opened.format).is_some() {
                    let stem = doc.path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
                    format!("{stem}.pdf")
                } else {
                    doc.name.clone()
                };
                save_dialog(
                    self.t(Key::SaveCopy).trim_end_matches('…'),
                    self.t(Key::Save),
                    name,
                    doc.path.parent().map(std::path::Path::to_path_buf),
                    |path| Message::Show(Show::CopyTarget(path)),
                )
            }
            Show::CopyTarget(target) => {
                let Some(doc) = self.doc.as_ref() else {
                    return Task::none();
                };
                let (source, format) = (doc.path.clone(), doc.opened.format);
                let converted = crate::doc::convert::tool_for(format).is_some();
                let as_pdf = converted && target.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
                let writable = (format.annotates_in_place() || as_pdf) && !doc.opened.info.encrypted;
                let changed = if as_pdf { !doc.notes.is_empty() } else { doc.dirty() };
                let notes = (writable && changed).then(|| doc.notes.clone());
                let name = target.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                let done = fill(self.t(Key::SavedCopy), &[("name", &name)]);
                let failed = self.t(Key::SaveFailed).to_string();
                later(
                    move || {
                        let source = if as_pdf {
                            match crate::doc::convert::to_pdf(&source, format) {
                                Ok(pdf) => pdf,
                                Err(_) => return false,
                            }
                        } else {
                            source
                        };
                        copy_with_notes(&source, &target, notes.as_deref())
                    },
                    move |ok| Message::Toast(if ok { done.clone() } else { failed.clone() }),
                )
            }
            Show::Reveal => {
                if let Some(doc) = self.doc.as_ref() {
                    let _ = glide_sdk::system::reveal(&doc.path, crate::APP_ID);
                }
                Task::none()
            }
            Show::OpenWith => {
                if let Some(doc) = self.doc.as_ref() {
                    let target = doc.opened.view_only.clone().unwrap_or_else(|| doc.path.clone());
                    let _ = glide_sdk::system::open(&target.to_string_lossy(), crate::APP_ID);
                }
                Task::none()
            }
            Show::Shortcuts => {
                let rows = [
                    ("Ctrl+F", Key::Find),
                    ("F3", Key::NextMatch),
                    ("Ctrl+D", Key::AddBookmark),
                    ("F9", Key::SidePanel),
                    ("F5", Key::Presentation),
                    ("F11", Key::Fullscreen),
                    ("Ctrl+= / Ctrl+-", Key::ZoomIn),
                    ("Alt+←", Key::Back),
                    ("Alt+Enter", Key::Properties),
                ];
                let body =
                    rows.iter().map(|(keys, key)| format!("{keys} — {}", self.t(*key))).collect::<Vec<_>>().join("\n");
                shell.dialog(glide_sdk::Dialog::new(self.t(Key::Shortcuts), body).cancel(self.t(Key::Done)));
                Task::none()
            }
            Show::More => {
                let items = self.more_menu();
                shell.context_menu(items);
                Task::none()
            }
            Show::ZoomMenu => {
                let items = self.zoom_menu();
                shell.context_menu(items);
                Task::none()
            }
            Show::Ocr => self.start_ocr(shell),
            Show::OcrPage(generation, page, text) => {
                let Some(doc) = self.doc.as_mut().filter(|d| d.generation == generation) else {
                    return Task::none();
                };
                if let Some(text) = text {
                    doc.texts.insert(page, text);
                }
                let Ocr::Running { done, total, tool, langs } = &mut doc.ocr else {
                    return Task::none();
                };
                *done += 1;
                if *done >= *total {
                    doc.ocr = Ocr::Done;
                    shell.toast(self.t(Key::Recognised));
                    return Task::none();
                }
                let (next, tool, langs) = (*done, tool.clone(), langs.clone());
                ocr_page(doc.index.clone(), tool, langs, generation, next)
            }
            Show::OcrReady(generation, tool, langs) => {
                let Some(doc) = self.doc.as_mut().filter(|d| d.generation == generation && d.ocr == Ocr::Starting)
                else {
                    return Task::none();
                };
                let Some(langs) = langs else {
                    doc.ocr = Ocr::Offer;
                    shell.toast(self.t(Key::NoOcrLanguage));
                    return Task::none();
                };
                doc.ocr = Ocr::Running { done: 0, total: doc.count(), tool: tool.clone(), langs: langs.clone() };
                ocr_page(doc.index.clone(), tool, langs, generation, 0)
            }
            Show::OcrDismiss => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.ocr = Ocr::Hidden;
                }
                Task::none()
            }
            Show::Recover => {
                let Some(doc) = self.doc.as_ref() else {
                    return Task::none();
                };
                let broken = doc.opened.broken.clone();
                let mut org = Organizer::new(doc.count());
                for page in broken {
                    if let Some(entry) = org.pages.get_mut(page) {
                        entry.removed = true;
                    }
                }
                if let Some(doc) = self.doc.as_mut() {
                    doc.organizer = Some(org);
                }
                self.org(Org::Save, shell)
            }
            Show::Moved => {
                if let Some(present) = self.doc.as_mut().and_then(|d| d.present.as_mut()) {
                    present.moved = Instant::now();
                }
                Task::none()
            }
        }
    }

    fn start_ocr(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        let Some(doc) = self.doc.as_mut() else {
            return Task::none();
        };
        if matches!(doc.ocr, Ocr::Running { .. } | Ocr::Starting) || doc.count() == 0 {
            return Task::none();
        }
        let Some(tool) = crate::doc::convert::find(&["tesseract"]) else {
            shell.toast(self.t(Key::NoTesseract));
            return Task::none();
        };
        doc.ocr = Ocr::Starting;
        let generation = doc.generation;
        later(
            move || {
                let langs = crate::ocr::languages(&tool);
                (tool, langs)
            },
            move |(tool, langs)| Message::Show(Show::OcrReady(generation, tool, langs)),
        )
    }

    #[must_use]
    pub fn signature_rect(&self, field: usize) -> Option<Rect> {
        self.doc.as_ref()?.opened.fields.get(field).map(|f| f.rect)
    }
}

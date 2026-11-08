use std::path::{Path, PathBuf};

use glide_sdk::Shell;
use iced::Task;
use iced::widget::image::Handle;

use super::{App, Message, Screen, Sessions, Slot, later};
use crate::doc::{FieldKind, Format, OpenError};
use crate::i18n::Key;
use crate::reader::Doc;
use crate::store::{self, Entry, Memory};
use crate::worker::Session;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    Recent,
    Reading,
    Notes,
    Favourites,
    Unsigned,
    Folder(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chip {
    All,
    Pdf,
    Books,
    Comics,
    Scans,
}

impl Chip {
    #[must_use]
    pub fn takes(self, format: Format) -> bool {
        match self {
            Self::All => true,
            Self::Pdf => matches!(format, Format::Pdf | Format::PostScript | Format::Xps | Format::Office),
            Self::Books => format == Format::Epub,
            Self::Comics => format == Format::Comic,
            Self::Scans => matches!(format, Format::Djvu | Format::Tiff),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Lib {
    Scanned(u64, Vec<Entry>),
    Cover(PathBuf, Option<PathBuf>),
    Place(Place),
    Chip(Chip),
    Query(String),
    ClearQuery,
    OpenDialog,
    Chosen(Vec<PathBuf>),
    Watch,
    Watched(PathBuf),
    Favourite(PathBuf),
    Password(String),
    Reveal,
    TryPassword,
    Back,
}

impl App {
    pub fn rescan(&mut self) -> Task<Message> {
        self.scans += 1;
        let scan = self.scans;
        let prefs = self.prefs.clone();
        let memories = self.memories.clone();
        later(move || store::entries(&prefs, &memories), move |entries| Message::Library(Lib::Scanned(scan, entries)))
    }

    pub fn pump_covers(&mut self) -> Task<Message> {
        if self.covering {
            return Task::none();
        }
        let Some(path) = self.covers_pending.pop() else {
            return Task::none();
        };
        self.covering = true;
        let key = path.clone();
        later(move || store::cover(&path), move |made| Message::Library(Lib::Cover(key.clone(), made)))
    }

    #[must_use]
    pub fn memory(&self, path: &Path) -> Memory {
        self.memories.get(&store::key(path)).cloned().unwrap_or_default()
    }

    #[must_use]
    pub fn shown_entries(&self) -> Vec<&Entry> {
        let query = crate::doc::text::fold(self.query.trim());
        self.entries
            .iter()
            .filter(|entry| self.chip.takes(entry.format))
            .filter(|entry| {
                let memory = self.memories.get(&store::key(&entry.path));
                match &self.place {
                    Place::Recent => true,
                    Place::Reading => memory.is_some_and(|m| m.page > 0 && m.progress() < 0.98),
                    Place::Notes => memory.is_some_and(|m| m.notes > 0),
                    Place::Favourites => memory.is_some_and(|m| m.favourite),
                    Place::Unsigned => memory.is_some_and(|m| m.unsigned),
                    Place::Folder(folder) => entry.path.starts_with(folder),
                }
            })
            .filter(|entry| {
                query.is_empty()
                    || crate::doc::text::fold(
                        &entry.path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    )
                    .contains(&query)
            })
            .collect()
    }

    #[must_use]
    pub fn continuing(&self) -> Vec<&Entry> {
        let mut found: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|entry| {
                self.memories
                    .get(&store::key(&entry.path))
                    .is_some_and(|m| (m.page > 0 && m.progress() < 0.98) || m.unsigned)
            })
            .collect();
        found.sort_by_key(|entry| {
            std::cmp::Reverse(self.memories.get(&store::key(&entry.path)).map_or(0, |m| m.opened))
        });
        found.truncate(3);
        found
    }

    pub fn library(&mut self, lib: Lib, shell: &mut Shell<Message>) -> Task<Message> {
        match lib {
            Lib::Scanned(scan, _) if scan != self.scans => Task::none(),
            Lib::Scanned(_, entries) => {
                self.covers_pending = entries
                    .iter()
                    .rev()
                    .filter(|e| !self.covers.contains_key(&e.path))
                    .map(|e| e.path.clone())
                    .collect();
                self.entries = entries;
                self.pump_covers()
            }
            Lib::Cover(path, made) => {
                self.covering = false;
                self.covers.insert(path, made.map(Handle::from_path));
                self.pump_covers()
            }
            Lib::Place(place) => {
                self.place = place;
                Task::none()
            }
            Lib::Chip(chip) => {
                self.chip = chip;
                Task::none()
            }
            Lib::Query(query) => {
                self.query = query;
                Task::none()
            }
            Lib::ClearQuery => {
                self.query.clear();
                Task::none()
            }
            Lib::OpenDialog => {
                let ask = glide_sdk::chooser::Ask {
                    purpose: glide_sdk::chooser::Purpose::Open,
                    title: self.t(Key::OpenTitle).into(),
                    accept: self.t(Key::Open).into(),
                    name: None,
                    folder: glide_config::home()
                        .map(|home| home.join("Documents"))
                        .filter(|p| p.is_dir())
                        .or_else(glide_config::home),
                };
                let receiver = glide_sdk::chooser::spawn(ask);
                Task::perform(async move { receiver.await.ok() }, |chosen| match chosen {
                    Some(glide_sdk::chooser::Chosen::Paths(paths)) => Message::Library(Lib::Chosen(paths)),
                    _ => Message::Ignore,
                })
            }
            Lib::Chosen(paths) => {
                paths.into_iter().next().map_or_else(Task::none, |path| self.leave(super::After::Open(path), shell))
            }
            Lib::Watch => {
                let ask = glide_sdk::chooser::Ask {
                    purpose: glide_sdk::chooser::Purpose::Folder,
                    title: self.t(Key::WatchFolder).trim_end_matches('…').into(),
                    accept: self.t(Key::ChooseFolder).into(),
                    name: None,
                    folder: glide_config::home(),
                };
                let receiver = glide_sdk::chooser::spawn(ask);
                Task::perform(async move { receiver.await.ok() }, |chosen| match chosen {
                    Some(glide_sdk::chooser::Chosen::Paths(paths)) => {
                        paths.into_iter().next().map_or(Message::Ignore, |p| Message::Library(Lib::Watched(p)))
                    }
                    _ => Message::Ignore,
                })
            }
            Lib::Watched(folder) => {
                if !self.prefs.folders.contains(&folder) {
                    self.prefs.folders.push(folder.clone());
                    let prefs = self.prefs.clone();
                    std::thread::spawn(move || store::save_prefs(&prefs));
                }
                self.place = Place::Folder(folder);
                self.rescan()
            }
            Lib::Favourite(path) => {
                let memory = self.memories.entry(store::key(&path)).or_default();
                memory.favourite = !memory.favourite;
                drop(store::save_memories_later(self.memories.clone()));
                Task::none()
            }
            Lib::Password(_) | Lib::Reveal | Lib::TryPassword => self.password_step(lib),
            Lib::Back => {
                shell.close_dialog();
                self.screen = Screen::Library;
                self.rescan()
            }
        }
    }

    fn password_step(&mut self, lib: Lib) -> Task<Message> {
        match lib {
            Lib::Password(text) => {
                if let Screen::Locked { text: current, wrong, .. } = &mut self.screen {
                    *current = text;
                    *wrong = false;
                }
                Task::none()
            }
            Lib::Reveal => {
                if let Screen::Locked { reveal, .. } = &mut self.screen {
                    *reveal = !*reveal;
                }
                Task::none()
            }
            _ => {
                let Screen::Locked { path, text, .. } = &self.screen else {
                    return Task::none();
                };
                let (path, text) = (path.clone(), text.clone());
                self.load(path, Some(text))
            }
        }
    }

    pub fn load(&mut self, path: PathBuf, password: Option<String>) -> Task<Message> {
        self.remember_doc();
        self.doc = None;
        self.generation += 1;
        let generation = self.generation;
        if !matches!(self.screen, Screen::Locked { .. }) {
            self.screen = Screen::Loading(path.clone());
        }
        self.password.clone_from(&password);
        let target = path.clone();
        later(
            move || -> Sessions {
                let raster = Session::open(&target, password.as_deref())?;
                let index = Session::open(&target, password.as_deref())?;
                let bytes = std::fs::metadata(&target).map_or(0, |meta| meta.len());
                Ok((raster, index, bytes))
            },
            move |sessions| Message::Opened(generation, path.clone(), Slot::new(sessions)),
        )
    }

    pub fn opened(&mut self, generation: u64, path: PathBuf, slot: &Slot<Sessions>) -> Task<Message> {
        if generation != self.generation {
            return Task::none();
        }
        let Some(result) = slot.take() else {
            return Task::none();
        };
        match result {
            Ok((raster, index, bytes)) => {
                let opened = raster.opened().clone();
                let memory = self.memory(&path);
                let mut doc = Doc::new(path, opened, raster, index, memory, generation);
                doc.dpr = self.dpr;
                doc.bytes = bytes;
                if doc.opened.format == Format::Djvu || doc.opened.format == Format::Tiff {
                    doc.ocr = crate::reader::Ocr::Offer;
                }
                let unsigned = doc
                    .opened
                    .fields
                    .iter()
                    .any(|f| matches!(f.kind, FieldKind::Signature { signed: false, stamped: false, .. }));
                if unsigned && doc.editable() {
                    doc.form = true;
                }
                self.doc = Some(doc);
                self.fit_viewport();
                self.screen = Screen::Reading;
                self.remember_doc();
                let jump = self.scroll_to_page();
                Task::batch([jump, self.request_visible(), self.request_thumbs()])
            }
            Err(OpenError::Locked { wrong }) => {
                let tries = match &self.screen {
                    Screen::Locked { tries, .. } => tries + 1,
                    _ => 0,
                };
                self.screen = Screen::Locked { path, text: String::new(), wrong, tries, reveal: false };
                iced::widget::operation::focus(crate::view::PASSWORD_ID)
            }
            Err(error) => {
                self.screen = Screen::Failed { path, error };
                Task::none()
            }
        }
    }

    pub fn reopened(&mut self, args: &[String], shell: &mut Shell<Message>) -> Task<Message> {
        let Some(path) = args.iter().filter(|a| !a.starts_with("--")).map(PathBuf::from).find(|p| p.is_file()) else {
            return Task::none();
        };
        let path = std::path::absolute(&path).unwrap_or(path);
        self.leave(super::After::Open(path), shell)
    }
}

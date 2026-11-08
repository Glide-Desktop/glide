use std::fmt::Write as _;
use std::path::PathBuf;

use glide_sdk::Shell;
use iced::Task;

use super::{After, App, Message, iso_now, later};
use crate::doc::text;
use crate::doc::write::{self, WriteError};
use crate::doc::{Hue, Mark, Note, Rect};
use crate::i18n::Key;
use crate::reader::{Selection, Tool};

#[derive(Debug, Clone)]
pub enum NoteMsg {
    Tool(Option<Tool>),
    Hue(Hue),
    Click(usize, f32, f32),
    Selected(usize, Vec<usize>),
    Stroke(usize, Vec<(f32, f32)>),
    Clear,
    Mark(Mark),
    Comment,
    CopySelection,
    FindSelection,
    WebSearch,
    Draft(String),
    DraftSave,
    DraftCancel,
    Focus(u64),
    Edit(u64),
    Remove(u64),
    Undo,
    Save,
    SaveThen(After),
    Saved(u64, Result<(Vec<Note>, Vec<Note>), String>, After),
    Discard(After),
    Export,
    Exported(PathBuf),
    Bookmark,
    BookmarkRemove(usize),
    BookmarkGo(usize),
    Commented(bool),
}

fn search_url(query: &str) -> String {
    let mut out = String::from("https://duckduckgo.com/?q=");
    for byte in query.bytes() {
        if byte.is_ascii_alphanumeric() {
            out.push(char::from(byte));
        } else if byte == b' ' {
            out.push('+');
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[must_use]
pub fn markdown(name: &str, notes: &[Note], labels: &[String]) -> String {
    let mut out = format!("# {name}\n");
    let mut sorted: Vec<&Note> = notes.iter().collect();
    sorted.sort_by(|a, b| {
        a.page.cmp(&b.page).then(a.rects.first().map_or(0.0, |r| r.y).total_cmp(&b.rects.first().map_or(0.0, |r| r.y)))
    });
    let mut page = None;
    for note in sorted {
        if page != Some(note.page) {
            let label = labels.get(note.page).cloned().unwrap_or_else(|| (note.page + 1).to_string());
            let _ = write!(out, "\n## {label}\n\n");
            page = Some(note.page);
        }
        if !note.quote.is_empty() {
            let _ = writeln!(out, "> {}", note.quote);
        }
        if !note.comment.is_empty() {
            if !note.quote.is_empty() {
                out.push('\n');
            }
            let _ = writeln!(out, "{}", note.comment);
        }
        out.push('\n');
    }
    out
}

impl App {
    #[allow(clippy::too_many_lines)] // One arm per note message; the arms are short and independent
    pub fn note(&mut self, message: NoteMsg, shell: &mut Shell<Message>) -> Task<Message> {
        let lang = self.lang;
        let t = move |key: Key| key.text(lang);
        let author = t(Key::You).to_string();
        let Some(doc) = self.doc.as_mut() else {
            return Task::none();
        };
        let adds = match &message {
            NoteMsg::Tool(Some(tool)) => *tool != Tool::Select,
            NoteMsg::Mark(_) | NoteMsg::Comment => true,
            _ => false,
        };
        if adds && !doc.keeps_notes() {
            doc.tool = None;
            shell.toast(t(Key::NotesUnsupported));
            return Task::none();
        }
        match message {
            NoteMsg::Tool(tool) => {
                doc.tool = tool;
                doc.selection = None;
                Task::none()
            }
            NoteMsg::Hue(hue) => {
                doc.hue = hue;
                if let Some(id) = doc.focus {
                    doc.remember();
                    if let Some(note) = doc.notes.iter_mut().find(|n| n.id == id) {
                        note.hue = hue;
                    }
                }
                Task::none()
            }
            NoteMsg::Click(page, x, y) => {
                let pin = 22.0 / doc.zoom().max(0.01);
                let hit = doc.notes.iter().rev().find(|n| {
                    n.page == page
                        && (n.rects.iter().any(|r| r.contains(x, y))
                            || (n.mark == Mark::Comment
                                && n.rects
                                    .first()
                                    .is_some_and(|r| x >= r.x && x <= r.x + pin && y >= r.y && y <= r.y + pin)))
                });
                if let Some(id) = hit.map(|n| n.id) {
                    doc.focus = Some(id);
                    doc.selection = None;
                    return Task::none();
                }
                if let Some(tool @ (Tool::Comment | Tool::Text)) = doc.tool {
                    let mark = if tool == Tool::Comment { Mark::Comment } else { Mark::Text };
                    let note = Note {
                        id: doc.fresh_id(),
                        page,
                        mark,
                        hue: doc.hue,
                        rects: vec![Rect::new(
                            x,
                            y,
                            if mark == Mark::Text { 180.0 } else { 22.0 / doc.zoom().max(0.01) },
                            if mark == Mark::Text { 20.0 } else { 22.0 / doc.zoom().max(0.01) },
                        )],
                        quote: String::new(),
                        comment: String::new(),
                        author,
                        modified: iso_now(),
                        strokes: Vec::new(),
                    };
                    doc.draft = Some((note.id, String::new()));
                    doc.add_note(note);
                    return iced::widget::operation::focus(crate::view::DRAFT_ID);
                }
                doc.selection = None;
                doc.focus = None;
                Task::none()
            }
            NoteMsg::Selected(page, words) => {
                doc.selection = Some(Selection { page, words });
                doc.focus = None;
                if let Some(mark) = doc.tool.and_then(Tool::mark)
                    && let Some(note) = doc.note_from_selection(mark, &author, &iso_now())
                {
                    doc.add_note(note);
                    doc.selection = None;
                }
                Task::none()
            }
            NoteMsg::Stroke(page, points) => {
                if doc.tool == Some(Tool::Ink) {
                    let note = Note {
                        id: doc.fresh_id(),
                        page,
                        mark: Mark::Ink,
                        hue: doc.hue,
                        rects: Vec::new(),
                        quote: String::new(),
                        comment: String::new(),
                        author,
                        modified: iso_now(),
                        strokes: vec![points],
                    };
                    doc.add_note(note);
                }
                Task::none()
            }
            NoteMsg::Clear => {
                doc.selection = None;
                doc.focus = None;
                doc.hover = None;
                Task::none()
            }
            NoteMsg::Mark(mark) => {
                if let Some(note) = doc.note_from_selection(mark, &author, &iso_now()) {
                    doc.add_note(note);
                }
                doc.selection = None;
                Task::none()
            }
            NoteMsg::Comment => {
                let Some(note) = doc.note_from_selection(Mark::Highlight, &author, &iso_now()) else {
                    return Task::none();
                };
                doc.draft = Some((note.id, String::new()));
                doc.add_note(note);
                doc.selection = None;
                iced::widget::operation::focus(crate::view::DRAFT_ID)
            }
            NoteMsg::CopySelection => {
                let Some(selection) = &doc.selection else {
                    return Task::none();
                };
                let quote =
                    doc.texts.get(&selection.page).map(|t| text::quote(t, &selection.words)).unwrap_or_default();
                shell.toast(t(Key::Copied));
                iced::clipboard::write(quote)
            }
            NoteMsg::FindSelection => self.find(super::FindMsg::Open),
            NoteMsg::WebSearch => {
                if let Some(selection) = &doc.selection
                    && let Some(text) = doc.texts.get(&selection.page)
                {
                    let _ = glide_sdk::system::open(&search_url(&text::quote(text, &selection.words)), crate::APP_ID);
                }
                Task::none()
            }
            NoteMsg::Draft(text) => {
                if let Some((_, draft)) = doc.draft.as_mut() {
                    *draft = text;
                }
                Task::none()
            }
            NoteMsg::DraftSave => {
                let Some((id, text)) = doc.draft.take() else {
                    return Task::none();
                };
                let edits = doc.notes.iter().find(|n| n.id == id).is_some_and(|n| n.comment != text);
                let fresh = doc.notes.iter().find(|n| n.id == id).is_some_and(|n| n.comment.is_empty())
                    && doc.undo.last().is_some_and(|before| !before.iter().any(|n| n.id == id));
                if edits && !fresh {
                    doc.remember();
                }
                if let Some(note) = doc.notes.iter_mut().find(|n| n.id == id) {
                    note.comment = text;
                    note.modified = iso_now();
                }
                Task::none()
            }
            NoteMsg::DraftCancel => {
                if let Some((id, _)) = doc.draft.take() {
                    let empty = doc
                        .notes
                        .iter()
                        .any(|n| n.id == id && n.comment.is_empty() && matches!(n.mark, Mark::Comment | Mark::Text));
                    if empty {
                        doc.notes.retain(|n| n.id != id);
                    }
                }
                Task::none()
            }
            NoteMsg::Focus(id) => {
                doc.focus = Some(id);
                let page = doc.notes.iter().find(|n| n.id == id).map(|n| n.page);
                page.map_or_else(Task::none, |page| self.read(super::Read::Go(page), shell))
            }
            NoteMsg::Edit(id) => {
                let current = doc.notes.iter().find(|n| n.id == id).map(|n| n.comment.clone()).unwrap_or_default();
                doc.draft = Some((id, current));
                iced::widget::operation::focus(crate::view::DRAFT_ID)
            }
            NoteMsg::Remove(id) => {
                doc.remember();
                doc.notes.retain(|n| n.id != id);
                doc.focus = None;
                doc.draft = None;
                Task::none()
            }
            NoteMsg::Undo => {
                if let Some(previous) = doc.undo.pop() {
                    doc.notes = previous;
                }
                Task::none()
            }
            NoteMsg::Save => self.save_notes(After::Stay, shell),
            NoteMsg::SaveThen(after) => self.save_notes(after, shell),
            NoteMsg::Saved(generation, result, after) => {
                let Some(doc) = self.doc.as_mut().filter(|d| d.generation == generation) else {
                    return Task::none();
                };
                match result {
                    Ok((sent, written)) => {
                        if doc.notes == sent && doc.draft.is_none() {
                            doc.notes.clone_from(&written);
                            doc.saved = written;
                            doc.undo.clear();
                            doc.focus = None;
                        } else {
                            doc.saved = sent;
                        }
                        self.proceed(after, shell)
                    }
                    Err(reason) => {
                        shell.toast(reason);
                        Task::none()
                    }
                }
            }
            NoteMsg::Discard(after) => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.notes = doc.saved.clone();
                }
                self.proceed(after, shell)
            }
            NoteMsg::Export => {
                let stem = doc.path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
                let ask = glide_sdk::chooser::Ask {
                    purpose: glide_sdk::chooser::Purpose::Save,
                    title: t(Key::ExportMarkdown).trim_end_matches('…').into(),
                    accept: t(Key::Save).into(),
                    name: Some(format!("{stem} — {}.md", t(Key::Notes).to_lowercase())),
                    folder: doc.path.parent().map(std::path::Path::to_path_buf),
                };
                let receiver = glide_sdk::chooser::spawn(ask);
                Task::perform(async move { receiver.await.ok() }, |chosen| match chosen {
                    Some(glide_sdk::chooser::Chosen::Paths(paths)) => {
                        paths.into_iter().next().map_or(Message::Ignore, |p| Message::Note(NoteMsg::Exported(p)))
                    }
                    _ => Message::Ignore,
                })
            }
            NoteMsg::Exported(target) => {
                let text = markdown(&doc.name, &doc.notes, &doc.opened.labels);
                let done = crate::i18n::fill(
                    t(Key::SavedCopy),
                    &[("name", &target.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()))],
                );
                let failed = t(Key::SaveFailed).to_string();
                later(
                    move || std::fs::write(&target, text).is_ok(),
                    move |ok| Message::Toast(if ok { done.clone() } else { failed.clone() }),
                )
            }
            NoteMsg::Bookmark => {
                let page = doc.page();
                if !doc.memory.bookmarks.iter().any(|(_, p)| *p == page) {
                    let name = doc.opened.outline.iter().rfind(|h| h.page.is_some_and(|p| p <= page)).map_or_else(
                        || crate::i18n::fill(t(Key::BookmarkName), &[("n", &(page + 1).to_string())]),
                        |h| h.title.clone(),
                    );
                    doc.memory.bookmarks.push((name, page));
                    doc.memory.bookmarks.sort_by_key(|(_, p)| *p);
                }
                Task::none()
            }
            NoteMsg::BookmarkRemove(index) => {
                if index < doc.memory.bookmarks.len() {
                    doc.memory.bookmarks.remove(index);
                }
                Task::none()
            }
            NoteMsg::BookmarkGo(index) => {
                let page = doc.memory.bookmarks.get(index).map(|(_, p)| *p);
                page.map_or_else(Task::none, |page| self.read(super::Read::Go(page), shell))
            }
            NoteMsg::Commented(on) => {
                self.commented = on;
                Task::none()
            }
        }
    }

    fn save_notes(&mut self, after: After, shell: &mut Shell<Message>) -> Task<Message> {
        let Some(doc) = self.doc.as_ref() else {
            return Task::none();
        };
        if doc.form_dirty() {
            return self.save_form(after, shell);
        }
        if !doc.editable() {
            shell.toast(self.t(if doc.opened.info.encrypted { Key::EncryptedSave } else { Key::NotesInCopy }));
            return Task::none();
        }
        let (path, notes, generation) = (doc.path.clone(), doc.notes.clone(), doc.generation);
        let failed = self.t(Key::SaveFailed).to_string();
        later(
            move || -> Result<(Vec<Note>, Vec<Note>), String> {
                write::save_notes(&path, &path, &notes).map_err(|error| match error {
                    WriteError::Io(reason) => reason,
                    _ => String::new(),
                })?;
                let written = crate::worker::Session::open(&path, None)
                    .map(|session| session.opened().notes.clone())
                    .map_err(|_| String::new())?;
                Ok((notes, written))
            },
            move |result| Message::Note(NoteMsg::Saved(generation, result.map_err(|_| failed.clone()), after.clone())),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_export_to_markdown_grouped_by_page() {
        let note = |page, quote: &str, comment: &str, y| Note {
            id: 1,
            page,
            mark: Mark::Highlight,
            hue: Hue::Yellow,
            rects: vec![Rect::new(0.0, y, 1.0, 1.0)],
            quote: quote.into(),
            comment: comment.into(),
            author: String::new(),
            modified: String::new(),
            strokes: Vec::new(),
        };
        let notes = vec![note(6, "друга", "", 50.0), note(6, "перша", "Перевірити", 10.0), note(10, "інша", "", 0.0)];
        let labels: Vec<String> = (1..=12).map(|n| n.to_string()).collect();
        let text = markdown("Посібник", &notes, &labels);
        assert_eq!(text, "# Посібник\n\n## 7\n\n> перша\n\nПеревірити\n\n> друга\n\n\n## 11\n\n> інша\n\n");
    }

    #[test]
    fn web_searches_escape_the_query() {
        assert_eq!(
            search_url("робочі простори"),
            "https://duckduckgo.com/?q=%D1%80%D0%BE%D0%B1%D0%BE%D1%87%D1%96+%D0%BF%D1%80%D0%BE%D1%81%D1%82%D0%BE%D1%80%D0%B8"
        );
    }
}

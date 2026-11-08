use glide_sdk::theme::{SP1, SP2, SP3, SP5, TextStyle};
use glide_sdk::widget::{self as ui, Size, ThumbMarks, Verdict};
use glide_sdk::window::{self, Side, side_corners};
use iced::widget::{Column, Space, column, container, mouse_area, row, scrollable};
use iced::{Alignment, Element, Fill, Length, Padding};

use super::{SIDEBAR, THUMBS_ID, hue_color};
use crate::app::{App, FindMsg, Form, Message, NoteMsg, Read};
use crate::doc::{FieldKind, Mark};
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::reader::Doc;
use crate::store::Tab;

fn top(app: &App) -> Element<'_, Message> {
    mouse_area(
        container(window::maximize_button(
            &app.paint,
            &app.window,
            app.t(Key::Maximize),
            Message::Window(window::Action::ToggleMaximize),
        ))
        .padding([0.0, SP3])
        .height(window::HEADER)
        .width(Fill)
        .align_y(Alignment::Center),
    )
    .on_press(Message::Window(window::Action::Drag))
    .on_double_click(Message::Window(window::Action::ToggleMaximize))
    .into()
}

fn tabs<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let options = [
        (Tab::Thumbs, Icon::Thumbs, app.t(Key::Thumbnails)),
        (Tab::Outline, Icon::Outline, app.t(Key::Outline)),
        (Tab::Notes, Icon::Pen, app.t(Key::Notes)),
        (Tab::Marks, Icon::Bookmark, app.t(Key::Bookmarks)),
    ];
    container(ui::icon_segmented(&app.paint, &options, doc.memory.tab, |tab| Message::Read(Read::Tab(tab))))
        .center_x(Fill)
        .padding(Padding { top: 0.0, right: SP3, bottom: SP2, left: SP3 })
        .into()
}

fn thumbs<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let current = doc.page();
    let mut list =
        Column::new().spacing(SP3 + 2.0).align_x(Alignment::Center).width(Fill).padding(Padding::from([6.0, 0.0]));
    for page in 0..doc.count() {
        let size = doc.size(page);
        let marks = ThumbMarks {
            bookmark: doc.memory.bookmarks.iter().any(|(_, p)| *p == page),
            note: doc.notes.iter().any(|n| n.page == page),
            broken: doc.opened.broken.contains(&page),
        };
        let label = doc.opened.labels.get(page).cloned().unwrap_or_else(|| (page + 1).to_string());
        list = list.push(ui::page_thumb(
            &app.paint,
            doc.thumbs.get(&page).cloned(),
            size.h / size.w.max(1.0),
            label,
            page == current,
            marks,
            Message::Read(Read::Go(page)),
        ));
    }
    scrollable(list)
        .id(THUMBS_ID)
        .on_scroll(|viewport| Message::Read(Read::ThumbsScrolled(viewport.absolute_offset().y)))
        .height(Fill)
        .into()
}

fn outline<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    if doc.opened.outline.is_empty() {
        return ui::empty_note(&app.paint, Icon::Outline, app.t(Key::NoOutline), app.t(Key::NoOutlineBody));
    }
    let current = doc.page();
    let active = doc.opened.outline.iter().rposition(|h| h.page.is_some_and(|p| p <= current));
    let mut list = Column::new().spacing(1).padding(Padding::from([0.0, SP2]));
    for (index, heading) in doc.opened.outline.iter().enumerate() {
        let indent = f32::from(heading.level) * 20.0;
        let mut item = ui::SidebarItem::new(&app.paint, heading.title.clone())
            .leading(Space::new().width(indent))
            .current(active == Some(index))
            .selected(active == Some(index))
            .on_press(heading.page.map(|p| Message::Read(Read::Go(p))));
        if let Some(page) = heading.page {
            item = item.count(page + 1);
        }
        list = list.push(item);
    }
    ui::overlay_scrollable(&app.paint, list).height(Fill).into()
}

fn chapter_of(doc: &Doc, page: usize) -> String {
    doc.opened
        .outline
        .iter()
        .rfind(|h| h.level == 0 && h.page.is_some_and(|p| p <= page))
        .map_or_else(|| (page + 1).to_string(), |h| h.title.clone())
}

fn notes<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let paint = app.paint;
    let total = doc.notes.iter().filter(|n| n.mark != Mark::Ink).count();
    let chips = row![
        ui::chip(
            &paint,
            i18n::fill(app.t(Key::NotesAll), &[("n", &total.to_string())]),
            !app.commented,
            Some(Message::Note(NoteMsg::Commented(false)))
        ),
        ui::chip(&paint, app.t(Key::NotesCommented), app.commented, Some(Message::Note(NoteMsg::Commented(true)))),
    ]
    .spacing(6);
    let mut sorted: Vec<&crate::doc::Note> =
        doc.notes.iter().filter(|n| !app.commented || !n.comment.is_empty()).collect();
    sorted.sort_by(|a, b| {
        a.page.cmp(&b.page).then(a.rects.first().map_or(0.0, |r| r.y).total_cmp(&b.rects.first().map_or(0.0, |r| r.y)))
    });
    let mut list = Column::new().spacing(2).padding(Padding::from([0.0, SP2]));
    if sorted.is_empty() {
        return column![
            container(chips).padding(Padding { top: 0.0, right: SP3, bottom: SP2, left: SP3 }),
            ui::empty_note(&paint, Icon::Pen, app.t(Key::NoNotes), app.t(Key::NoNotesBody))
        ]
        .into();
    }
    let mut chapter = String::new();
    for note in sorted {
        let here = chapter_of(doc, note.page);
        if here != chapter {
            list = list.push(ui::section_title(&paint, &here, false));
            chapter = here;
        }
        let mut meta = vec![i18n::fill(app.t(Key::PageShort), &[("n", &(note.page + 1).to_string())])];
        let fresh = !doc.saved.iter().any(|s| s.id == note.id);
        meta.push(if fresh { app.t(Key::JustNow).to_string() } else { i18n::date(app.lang, &note.modified, false) });
        if fresh {
            meta.push(app.t(Key::New).to_string());
        }
        let quote = if note.quote.is_empty() {
            app.t(match note.mark {
                Mark::Ink => Key::ToolInk,
                Mark::Text => Key::ToolText,
                _ => Key::ToolComment,
            })
        } else {
            note.quote.as_str()
        };
        list = list.push(ui::quote_row(
            &paint,
            hue_color(note.hue),
            quote,
            Some(note.comment.as_str()),
            &meta,
            doc.focus == Some(note.id),
            Message::Note(NoteMsg::Focus(note.id)),
        ));
    }
    let export = container(ui::full_button(
        &paint,
        app.t(Key::ExportMarkdown),
        ui::Tone::Outline,
        Some(Message::Note(NoteMsg::Export)),
    ))
    .padding(Padding { top: SP2, right: SP3 + 4.0, bottom: SP3, left: SP3 + 4.0 });
    column![
        container(chips).padding(Padding { top: 0.0, right: SP3, bottom: SP2, left: SP3 }),
        ui::overlay_scrollable(&paint, list).height(Fill),
        export
    ]
    .into()
}

fn marks<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let paint = app.paint;
    let current = doc.page();
    let mut list = Column::new().spacing(2).padding(Padding::from([0.0, SP2]));
    for (index, (name, page)) in doc.memory.bookmarks.iter().enumerate() {
        let on = *page == current;
        let item: Element<'a, Message> = ui::SidebarItem::new(&paint, name.clone())
            .glyph(Icon::Bookmark)
            .count(page + 1)
            .current(on)
            .selected(on)
            .on_press(Some(Message::Note(NoteMsg::BookmarkGo(index))))
            .into();
        list = list.push(mouse_area(item).on_right_press(Message::Note(NoteMsg::BookmarkRemove(index))));
    }
    list = list.push(
        container(ui::sized_button(
            &paint,
            app.t(Key::AddBookmark),
            ui::Tone::Ghost,
            Size::Small,
            Some(Message::Note(NoteMsg::Bookmark)),
        ))
        .padding(Padding { top: SP2, right: 0.0, bottom: 0.0, left: SP2 }),
    );
    list = list.push(
        container(ui::text(app.t(Key::BookmarksNote), TextStyle::CAP).color(paint.fg_2())).padding(Padding {
            top: SP2,
            right: SP3,
            bottom: 0.0,
            left: SP3,
        }),
    );
    ui::overlay_scrollable(&paint, list).height(Fill).into()
}

fn hits<'a>(app: &'a App, doc: &'a Doc) -> Option<Element<'a, Message>> {
    let find = doc.find.as_ref().filter(|f| f.open && f.query.trim().chars().count() >= 2)?;
    let paint = app.paint;
    let sections: std::collections::BTreeSet<String> = find.hits.iter().map(|h| chapter_of(doc, h.page)).collect();
    let summary = if find.hits.is_empty() && find.scanned >= doc.count() {
        app.t(Key::NoMatches).to_string()
    } else {
        i18n::matches(app.lang, find.hits.len())
    };
    let sub = if find.scanned < doc.count() {
        app.t(Key::Searching).to_string()
    } else {
        i18n::fill(app.t(Key::InSections), &[("n", &sections.len().to_string())])
    };
    let head = row![ui::text(summary, TextStyle::HEAD), ui::text(sub, TextStyle::CAP).color(paint.fg_2())]
        .spacing(6)
        .align_y(Alignment::End)
        .padding(Padding { top: 0.0, right: SP5 - 4.0, bottom: SP2, left: SP5 - 4.0 });
    let mut list = Column::new().spacing(1).padding(Padding::from([0.0, SP2]));
    let mut chapter = String::new();
    for (index, hit) in find.hits.iter().enumerate() {
        let here = chapter_of(doc, hit.page);
        if here != chapter {
            list = list.push(ui::section_title(&paint, &here, false));
            chapter = here;
        }
        list = list.push(ui::hit_row(
            &paint,
            &(hit.page + 1).to_string(),
            &hit.before,
            &hit.matched,
            &hit.after,
            index == find.current,
            Message::Find(FindMsg::Pick(index)),
        ));
    }
    Some(column![head, ui::overlay_scrollable(&paint, list).height(Fill)].into())
}

fn form<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let paint = app.paint;
    let (done, total) = doc.filled();
    let head = row![
        ui::text(app.t(Key::Fields), TextStyle { ..TextStyle::MICRO }).color(paint.fg_2()).width(Fill),
        ui::text(format!("{done} / {total}"), TextStyle::CAP).color(paint.fg_2()),
    ]
    .padding(Padding { top: 0.0, right: SP5 - 4.0, bottom: SP1, left: SP5 - 4.0 });
    let mut list = Column::new().spacing(1).padding(Padding::from([0.0, SP2]));
    for field in doc.opened.fields.iter().filter(|f| !matches!(f.kind, FieldKind::Signature { .. })) {
        let filled = doc.values.get(&field.name).is_some_and(|v| !v.is_empty() && v != "false");
        let mut item = ui::SidebarItem::new(&paint, field.label.clone())
            .glyph(if filled { Icon::Check } else { Icon::TextBox })
            .on_press(Some(Message::Read(Read::Go(field.page))));
        if !filled && field.required {
            item = item
                .ink(Some(paint.warn()))
                .accessory(ui::text(app.t(Key::Required), TextStyle::CAP).color(paint.fg_2()));
        }
        list = list.push(item);
    }
    let mut cards = Column::new().spacing(SP2).padding(Padding::from([0.0, SP3]));
    for (index, field) in doc.opened.fields.iter().enumerate() {
        let FieldKind::Signature { signed, signer, when, changed, stamped } = &field.kind else {
            continue;
        };
        let stamped = *stamped || doc.stamped(index);
        let (verdict, lines) = if *signed {
            let (verdict, said) =
                if *changed { (Verdict::Changed, Key::SignedChanged) } else { (Verdict::Intact, Key::SignedIntact) };
            let mut lines = vec![app.t(said).to_string(), app.t(Key::CertificateNotChecked).to_string()];
            if !when.is_empty() {
                lines.push(i18n::date(app.lang, when, true));
            }
            (verdict, lines)
        } else if stamped {
            (Verdict::Intact, vec![app.t(Key::NotDigital).to_string()])
        } else {
            (Verdict::Waiting, vec![app.t(Key::AwaitingYou).to_string()])
        };
        let who = if signer.is_empty() { field.label.clone() } else { signer.clone() };
        cards = cards.push(ui::signature_status(
            &paint,
            verdict,
            &who,
            lines,
            (!*signed && !stamped && doc.editable())
                .then(|| (app.t(Key::SignHere), Message::Form(Form::Sign(Some(index))))),
        ));
    }
    column![
        head,
        list,
        container(ui::section_title(&paint, app.t(Key::Signatures), false)).padding(Padding::from([0.0, SP2])),
        ui::overlay_scrollable(&paint, cards).height(Fill)
    ]
    .into()
}

pub fn view<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let paint = app.paint;
    let body: Element<'a, Message> = if let Some(found) = hits(app, doc) {
        found
    } else if doc.form && !doc.opened.fields.is_empty() {
        form(app, doc)
    } else {
        let content = match doc.memory.tab {
            Tab::Thumbs => thumbs(app, doc),
            Tab::Outline => outline(app, doc),
            Tab::Notes => notes(app, doc),
            Tab::Marks => marks(app, doc),
        };
        column![tabs(app, doc), content].into()
    };
    ui::pane(
        paint.sidebar_bg(),
        side_corners(Side::Left, app.window.corner()),
        column![top(app), container(body).height(Fill)],
    )
    .width(Length::Fixed(SIDEBAR))
    .into()
}

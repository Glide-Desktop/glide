use glide_sdk::theme::{SP1, SP2, SP3, TextStyle};
use glide_sdk::widget::{self as ui, Area, Layer, MarkStyle, Marked, PageTool, Size};
use glide_sdk::window::{self, Header, Side, side_corners};
use iced::widget::{Column, Row, Space, column, container, row, scrollable, stack, text_input};
use iced::{Alignment, Color, Element, Fill, Length, Padding, Vector};

use super::{DRAFT_ID, FIND_ID, PAGE_FIELD_ID, PAGES_ID, hue_color, ink, paper};
use crate::app::{App, FindMsg, Form, Message, NoteMsg, Read, Show};
use crate::doc::{FieldKind, Hue, Mark};
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::reader::{Doc, GAP, Hover, MARGIN, Ocr, Scope, Tool, ZOOM_LEVELS};
use crate::store::{Layout, Paper};

fn area(rect: &crate::doc::Rect) -> Area {
    Area::new(rect.x, rect.y, rect.w, rect.h)
}

fn page_field<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let paint = app.paint;
    let shown = app
        .page_input
        .clone()
        .unwrap_or_else(|| doc.opened.labels.get(doc.page()).cloned().unwrap_or_else(|| (doc.page() + 1).to_string()));
    let field = text_input(app.t(Key::PageField), &shown)
        .id(PAGE_FIELD_ID)
        .on_input(|text| Message::Read(Read::PageInput(text)))
        .on_submit(Message::Read(Read::PageSubmit))
        .width(48)
        .align_x(Alignment::Center);
    row![
        container(ui::field(&paint, field)).width(48),
        ui::text(i18n::fill(app.t(Key::Of), &[("total", &doc.count().to_string())]), TextStyle::CAP)
            .color(paint.fg_2())
            .wrapping(ui::Wrapping::None)
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn subtitle(app: &App, doc: &Doc) -> String {
    if doc.dirty() {
        return i18n::fill(app.t(Key::Changed), &[("n", &doc.new_notes().to_string())]);
    }
    if doc.form && !doc.opened.fields.is_empty() {
        let empty = doc.required_empty();
        return if empty > 0 {
            i18n::fill(app.t(Key::FormEmpty), &[("n", &empty.to_string())])
        } else {
            app.t(Key::FormReady).to_string()
        };
    }
    doc.opened
        .outline
        .iter()
        .rfind(|h| h.page.is_some_and(|p| p <= doc.page()))
        .map(|h| h.title.clone())
        .unwrap_or_default()
}

fn header<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let paint = app.paint;
    let back = ui::sized_button(
        &paint,
        app.t(Key::BackToLibrary),
        ui::Tone::Ghost,
        Size::Small,
        Some(Message::Read(Read::Library)),
    );
    let back = row![ui::icon(Icon::Back, 16.0, paint.fg_2()), back].spacing(0).align_y(Alignment::Center);
    let side = ui::sized_icon_button(
        &paint,
        Icon::Sidebar,
        app.t(Key::SidePanel),
        Some(Message::Read(Read::ToggleSide)),
        doc.memory.side,
        Size::Medium,
    );
    let field = page_field(app, doc);
    let subtitle = subtitle(app, doc);
    let mut bar = Header::new(doc.name.clone())
        .subtitle((!subtitle.is_empty()).then_some(subtitle))
        .labels(app.t(Key::Maximize), app.t(Key::CloseWindow))
        .corners(side_corners(if doc.memory.side { Side::TopRight } else { Side::Top }, app.window.corner()))
        .leading(back)
        .leading(side)
        .trailing(field)
        .trailing(ui::tool_divider(&paint));
    if doc.memory.side {
        bar = bar.without_maximize();
    }
    let finding = doc.find.as_ref().is_some_and(|f| f.open);
    bar = bar
        .trailing(ui::sized_icon_button(
            &paint,
            Icon::Letters,
            app.t(Key::ReadingSettings),
            Some(Message::Read(Read::Popover(!doc.popover))),
            doc.popover,
            Size::Medium,
        ))
        .trailing(ui::sized_icon_button(
            &paint,
            Icon::Search,
            app.t(Key::Find),
            Some(Message::Find(if finding { FindMsg::Close } else { FindMsg::Open })),
            finding,
            Size::Medium,
        ))
        .trailing(ui::sized_icon_button(
            &paint,
            Icon::Pen,
            app.t(Key::Annotate),
            Some(Message::Note(NoteMsg::Tool(if doc.tool.is_some() { None } else { Some(Tool::Select) }))),
            doc.tool.is_some(),
            Size::Medium,
        ));
    if doc.form && doc.form_dirty() {
        bar = bar.trailing(ui::sized_button(
            &paint,
            app.t(Key::ClearForm),
            ui::Tone::Neutral,
            Size::Small,
            Some(Message::Form(Form::Clear)),
        ));
    }
    if (doc.dirty() || doc.form_dirty()) && doc.editable() {
        bar = bar.trailing(ui::sized_button(
            &paint,
            app.t(Key::Save),
            ui::Tone::Accent,
            Size::Small,
            Some(Message::Note(NoteMsg::Save)),
        ));
    }
    bar = bar.trailing(ui::sized_icon_button(
        &paint,
        Icon::More,
        app.t(Key::MoreActions),
        Some(Message::Show(Show::More)),
        false,
        Size::Medium,
    ));
    window::header(&paint, &app.window, bar, Message::Window, Message::Close, 120.0)
}

fn layer(app: &App, doc: &Doc, page: usize, zoom: f32) -> Layer {
    let paint = app.paint;
    let text = doc.texts.get(&page);
    let marks = doc
        .notes
        .iter()
        .filter(|n| n.page == page && matches!(n.mark, Mark::Highlight | Mark::Underline | Mark::Strike))
        .map(|n| Marked {
            areas: n.rects.iter().map(area).collect(),
            color: if n.mark == Mark::Underline && n.hue == Hue::Yellow {
                Color::from_rgb8(0xcf, 0x65, 0x24)
            } else {
                hue_color(n.hue)
            },
            style: match n.mark {
                Mark::Underline => MarkStyle::Under,
                Mark::Strike => MarkStyle::Strike,
                _ => MarkStyle::Fill,
            },
            focused: doc.focus == Some(n.id),
        })
        .collect();
    let selection = doc
        .selection
        .as_ref()
        .filter(|s| s.page == page)
        .and_then(|s| text.map(|t| crate::doc::text::line_rects(t, &s.words)))
        .unwrap_or_default()
        .iter()
        .map(area)
        .collect();
    let tool = match doc.tool {
        Some(Tool::Highlight | Tool::Underline | Tool::Strike) => PageTool::Mark,
        Some(Tool::Ink) => PageTool::Ink,
        Some(Tool::Comment | Tool::Text) => PageTool::Pin,
        _ => PageTool::Select,
    };
    Layer {
        scale: zoom,
        words: text.map(|t| t.words.iter().map(|w| area(&w.rect)).collect()).unwrap_or_default(),
        marks,
        hits: doc
            .hit_pages()
            .into_iter()
            .filter(|(p, ..)| *p == page)
            .map(|(_, r, current)| (area(&r), current))
            .collect(),
        selection,
        links: doc.links.get(&page).map(|l| l.iter().map(|l| area(&l.rect)).collect()).unwrap_or_default(),
        pins: doc
            .notes
            .iter()
            .filter(|n| n.page == page && n.mark == Mark::Comment)
            .filter_map(|n| n.rects.first().map(|r| ((r.x, r.y), hue_color(n.hue))))
            .collect(),
        strokes: doc
            .notes
            .iter()
            .filter(|n| n.page == page && n.mark == Mark::Ink)
            .map(|n| (n.stroke.clone(), hue_color(n.hue)))
            .collect(),
        tool,
        ink: hue_color(doc.hue),
        ring: paint.acc_bg(),
    }
}

fn form_overlay<'a>(app: &'a App, doc: &'a Doc, page: usize, zoom: f32) -> Option<Element<'a, Message>> {
    if !doc.form {
        return None;
    }
    let paint = app.paint;
    let mut layers: Vec<Element<'a, Message>> = Vec::new();
    for (index, field) in doc.opened.fields.iter().enumerate().filter(|(_, f)| f.page == page) {
        let (x, y, w, h) = (field.rect.x * zoom, field.rect.y * zoom, field.rect.w * zoom, field.rect.h * zoom);
        let control: Element<'a, Message> = match &field.kind {
            FieldKind::Text | FieldKind::Choice(_) => {
                let value = doc.values.get(&field.name).cloned().unwrap_or_default();
                let name = field.name.clone();
                let input = text_input("", &value)
                    .on_input(move |text| Message::Form(Form::Value(name.clone(), text)))
                    .size((h * 0.62).clamp(9.0, 16.0))
                    .padding(Padding::from([0.0, 6.0]))
                    .style(move |_, status| ui::form_input_style(&paint, status, field.required && value.is_empty()));
                container(input).width(w).height(h).center_y(h).into()
            }
            FieldKind::Check(_) => {
                let on = doc.values.get(&field.name).is_some_and(|v| v == "true");
                container(ui::check_box(
                    &paint,
                    if on { ui::Check::On } else { ui::Check::Off },
                    Some(Message::Form(Form::Toggle(field.name.clone()))),
                ))
                .width(w)
                .height(h)
                .center(Length::Shrink)
                .into()
            }
            FieldKind::Signature { signed: false, .. } if !doc.stamps.iter().any(|(f, _)| *f == index) => {
                ui::sign_here(&paint, app.t(Key::SignHere), w, h, Message::Form(Form::Sign(index)))
            }
            FieldKind::Signature { .. } => {
                let strokes: Vec<(Vec<(f32, f32)>, Color)> = doc
                    .stamps
                    .iter()
                    .filter(|(f, _)| *f == index)
                    .flat_map(|(_, s)| {
                        s.iter().map(|stroke| {
                            (
                                stroke.iter().map(|(sx, sy)| (sx - field.rect.x, sy - field.rect.y)).collect(),
                                Color::from_rgb8(0x1b, 0x1a, 0x17),
                            )
                        })
                    })
                    .collect();
                if strokes.is_empty() {
                    continue;
                }
                ui::page_layer(Layer { scale: zoom, strokes, ..Layer::default() }, w, h, |_| Message::Ignore)
            }
        };
        layers.push(
            container(control)
                .padding(Padding { top: y, left: x, right: 0.0, bottom: 0.0 })
                .width(Fill)
                .height(Fill)
                .into(),
        );
    }
    (!layers.is_empty()).then(|| stack(layers).into())
}

fn page<'a>(app: &'a App, doc: &'a Doc, index: usize, zoom: f32) -> Element<'a, Message> {
    let size = doc.size(index);
    let (w, h) = ((size.w * zoom).round(), (size.h * zoom).round());
    let mut over: Vec<Element<'a, Message>> =
        vec![ui::page_layer(layer(app, doc, index, zoom), w, h, move |event| Message::Read(Read::Page(index, event)))];
    if let Some(form) = form_overlay(app, doc, index, zoom) {
        over.push(form);
    }
    if doc.opened.broken.contains(&index) && doc.tile(index).is_none() {
        over.push(
            container(ui::empty_note(
                &app.paint,
                Icon::Broken,
                &i18n::fill(app.t(Key::PageBroken), &[("n", &(index + 1).to_string())]),
                "",
            ))
            .center(Fill)
            .into(),
        );
    }
    ui::page_sheet(&app.paint, doc.tile(index), paper(doc.memory.paper).face, w, h, Some(stack(over).into()))
}

#[must_use]
pub fn row_width(doc: &Doc, row: &[usize], zoom: f32) -> f32 {
    row.iter().map(|p| (doc.size(*p).w * zoom).round()).sum::<f32>() + 4.0 * row.len().saturating_sub(1) as f32
}

#[must_use]
pub fn content_width(viewport: f32, widest: f32) -> f32 {
    viewport.max(widest + 2.0 * MARGIN).floor()
}

#[must_use]
pub fn page_origin(doc: &Doc, index: usize) -> (f32, f32) {
    let zoom = doc.zoom();
    let rows = doc.rows();
    let at = doc.row_of(index);
    let row = rows.get(at).cloned().unwrap_or_default();
    let width = row_width(doc, &row, zoom);
    let mut x = ((doc.viewport.width - width) / 2.0).max(MARGIN);
    for p in &row {
        if *p == index {
            break;
        }
        x += (doc.size(*p).w * zoom).round() + 4.0;
    }
    if doc.memory.layout == Layout::Single {
        let h = (doc.size(index).h * zoom).round();
        return (x, ((doc.viewport.height - h) / 2.0).max(MARGIN));
    }
    (x, doc.top_of(index) - doc.scroll)
}

fn pages<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let paint = app.paint;
    let zoom = doc.zoom();
    if doc.memory.layout == Layout::Single {
        let current = doc.page();
        let step = f32::from(doc.step);
        let sheet =
            glide_sdk::anim::animate(1.0, glide_sdk::anim::Speed::EXPAND, move |_| page(app, doc, current, zoom))
                .from(0.0)
                .keyed(current as u64)
                .offset(move |value| Vector::new(48.0 * step * (1.0 - value), 0.0));
        return container(sheet)
            .center(Fill)
            .style(move |_| glide_sdk::glide_iced::style::rounded(paint.sunken_bg(), 0.0))
            .into();
    }
    let rows = doc.rows();
    let tops = doc.row_tops();
    let heights = doc.row_heights();
    let (from, to) = (doc.scroll - doc.viewport.height, doc.scroll + doc.viewport.height * 2.0);
    let widest = rows.iter().map(|row| row_width(doc, row, zoom)).fold(0.0, f32::max);
    let mut column = Column::new()
        .spacing(GAP)
        .align_x(Alignment::Center)
        .width(content_width(doc.viewport.width, widest))
        .padding(Padding::from([MARGIN, MARGIN]));
    for (row, (top, height)) in rows.iter().zip(tops.iter().zip(&heights)) {
        if top + height < from || *top > to {
            column = column.push(Space::new().width(1).height(*height));
            continue;
        }
        let items = row.iter().map(|p| page(app, doc, *p, zoom));
        column = column.push(Row::with_children(items).spacing(4).align_y(Alignment::Start));
    }
    let list = scrollable(column)
        .id(PAGES_ID)
        .direction(scrollable::Direction::Both {
            vertical: scrollable::Scrollbar::new(),
            horizontal: scrollable::Scrollbar::new(),
        })
        .on_scroll(|viewport| Message::Read(Read::Scrolled(viewport.bounds(), viewport.absolute_offset().y)))
        .width(Fill)
        .height(Fill)
        .style(move |theme, status| {
            let mut style = scrollable::default(theme, status);
            style.container.background = Some(iced::Background::Color(paint.sunken_bg()));
            style
        });
    ui::context_area(list, |point| Message::Read(Read::Context(point)))
        .on_zoom(|by| Message::Read(Read::ZoomWheel(by)))
        .into()
}

fn reflow<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let page = paper(doc.memory.paper);
    let blocks = doc.opened.reflow.as_deref().unwrap_or_default();
    let size = f32::from(doc.percent().clamp(60, 240)) / 100.0 * 17.0;
    let mut column =
        Column::new().spacing(size * 0.8).max_width(640.0 * size / 17.0).padding(Padding::from([40.0, 48.0]));
    for block in blocks.iter().filter(|b| b.chapter == doc.page()) {
        let style = match block.kind {
            crate::doc::epub::Kind::Heading(level) => TextStyle {
                size_px: size * if level <= 1 { 1.6 } else { 1.25 },
                line_height_px: size * if level <= 1 { 2.0 } else { 1.6 },
                weight: 650,
                ..TextStyle::T2
            },
            _ => TextStyle { size_px: size, line_height_px: size * 1.6, ..TextStyle::BODY },
        };
        let mut words = ui::text(block.text.clone(), style).color(page.ink);
        if matches!(block.kind, crate::doc::epub::Kind::Quote) {
            words = words.color(Color { a: 0.8, ..page.ink });
        }
        column = column.push(words);
    }
    let _ = app;
    let body = scrollable(container(column).center_x(Fill)).id(PAGES_ID).height(Fill);
    container(body)
        .width(Fill)
        .height(Fill)
        .style(move |_| glide_sdk::glide_iced::style::rounded(page.face, 0.0))
        .into()
}

fn place<'a>(content: impl Into<Element<'a, Message>>, x: f32, y: f32) -> Element<'a, Message> {
    container(content)
        .padding(Padding { top: y.max(0.0), left: x.max(0.0), right: 0.0, bottom: 0.0 })
        .width(Fill)
        .height(Fill)
        .into()
}

fn find_float<'a>(app: &'a App, doc: &'a Doc) -> Option<Element<'a, Message>> {
    let find = doc.find.as_ref().filter(|f| f.open)?;
    let paint = app.paint;
    let input = text_input(app.t(Key::Search), &find.query)
        .id(FIND_ID)
        .on_input(|q| Message::Find(FindMsg::Query(q)))
        .on_submit(Message::Find(FindMsg::Next));
    let status = if find.hits.is_empty() {
        if find.query.trim().chars().count() >= 2 && find.scanned >= doc.count() {
            app.t(Key::NoMatches).to_string()
        } else {
            String::new()
        }
    } else {
        i18n::fill(
            app.t(Key::MatchOf),
            &[("n", &(find.current + 1).to_string()), ("total", &find.hits.len().to_string())],
        )
    };
    let failed = find.hits.is_empty() && !status.is_empty();
    let bar = ui::find_bar(&paint, input)
        .status(status, failed)
        .toggle("Aa", app.t(Key::CaseSensitive), find.case, Message::Find(FindMsg::Case(!find.case)))
        .toggle("W", app.t(Key::WholeWord), find.whole, Message::Find(FindMsg::Whole(!find.whole)))
        .steps(
            (Message::Find(FindMsg::Previous), app.t(Key::PreviousMatch)),
            (Message::Find(FindMsg::Next), app.t(Key::NextMatch)),
        )
        .done(app.t(Key::CloseFind), Message::Find(FindMsg::Close))
        .extra(
            row![
                ui::chip(
                    &paint,
                    app.t(Key::ScopeText),
                    find.scope == Scope::Text,
                    Some(Message::Find(FindMsg::Scope(Scope::Text)))
                ),
                ui::chip(
                    &paint,
                    app.t(Key::ScopeNotes),
                    find.scope == Scope::Notes,
                    Some(Message::Find(FindMsg::Scope(Scope::Notes)))
                ),
            ]
            .spacing(6),
        )
        .floating(700.0);
    Some(container(bar).align_right(Fill).padding(Padding { top: 8.0, right: 16.0, bottom: 0.0, left: 0.0 }).into())
}

fn tools_float<'a>(app: &'a App, doc: &'a Doc) -> Option<Element<'a, Message>> {
    let tool = doc.tool?;
    let paint = app.paint;
    let tools = [
        (Tool::Select, Icon::Select, Key::ToolSelect),
        (Tool::Highlight, Icon::Highlight, Key::ToolHighlight),
        (Tool::Underline, Icon::Underline, Key::ToolUnderline),
        (Tool::Strike, Icon::Strike, Key::ToolStrike),
        (Tool::Comment, Icon::Comment, Key::ToolComment),
        (Tool::Ink, Icon::Ink, Key::ToolInk),
        (Tool::Text, Icon::TextBox, Key::ToolText),
    ];
    let mut bar = Row::new().spacing(2).align_y(Alignment::Center);
    for (value, glyph, key) in tools {
        bar = bar.push(ui::tool_button(
            &paint,
            glyph,
            app.t(key),
            tool == value,
            Some(Message::Note(NoteMsg::Tool(Some(value)))),
        ));
    }
    bar = bar.push(ui::tool_divider(&paint));
    let colors: Vec<(Hue, Color)> = Hue::ALL.iter().map(|h| (*h, hue_color(*h))).collect();
    bar = bar.push(
        container(ui::swatches(&paint, &colors, Some(doc.hue), |h| Message::Note(NoteMsg::Hue(h))))
            .padding(Padding::from([0.0, SP1])),
    );
    bar = bar.push(ui::tool_divider(&paint));
    bar = bar.push(ui::tool_button(
        &paint,
        Icon::Undo,
        app.t(Key::Undo),
        false,
        (!doc.undo.is_empty()).then_some(Message::Note(NoteMsg::Undo)),
    ));
    bar = bar.push(ui::tool_label(&paint, None, app.t(Key::Done), false, Some(Message::Note(NoteMsg::Tool(None)))));
    Some(container(ui::floating(&paint, bar, 4)).center_x(Fill).padding(Padding { top: 12.0, ..Padding::ZERO }).into())
}

fn popover<'a>(app: &'a App, doc: &'a Doc) -> Option<Element<'a, Message>> {
    if !doc.popover {
        return None;
    }
    let paint = app.paint;
    let papers = [
        (Paper::White, app.t(Key::PaperWhite), paper(Paper::White)),
        (Paper::Sepia, app.t(Key::PaperSepia), paper(Paper::Sepia)),
        (Paper::Night, app.t(Key::PaperNight), paper(Paper::Night)),
    ];
    let micro = |key: Key| ui::text(app.t(key), TextStyle::MICRO).color(paint.fg_2());
    let switch = |label: Key, hint: Option<Key>, on: bool, message: fn(bool) -> Read| -> Element<'a, Message> {
        let mut words = column![ui::text(app.t(label), TextStyle::CALL)];
        if let Some(hint) = hint {
            words = words.push(ui::text(app.t(hint), TextStyle::CAP).color(paint.fg_2()));
        }
        row![
            words.width(Fill),
            ui::sized_switch(&paint, on, ui::SwitchSize::Bar, Some(move |v| Message::Read(message(v))))
        ]
        .align_y(Alignment::Center)
        .spacing(SP3)
        .into()
    };
    let layouts = [
        (Layout::Single, app.t(Key::One)),
        (Layout::Continuous, app.t(Key::Continuous)),
        (Layout::Spread, app.t(Key::Spread)),
    ];
    let content = column![
        micro(Key::PageColour),
        ui::paper_picker(&paint, &papers, doc.memory.paper, |p| Message::Read(Read::Paper(p))),
        switch(Key::KeepImages, Some(Key::KeepImagesHint), doc.memory.keep_images, Read::KeepImages),
        ui::separator(&paint),
        micro(Key::LayoutHeading),
        ui::segmented_fill(&paint, &layouts, doc.memory.layout, |l| Message::Read(Read::Layout(l))),
        switch(Key::CoverAlone, None, doc.memory.cover_alone, Read::CoverAlone),
        switch(Key::CropMargins, Some(Key::CropMarginsHint), doc.memory.crop, Read::Crop),
        switch(Key::RightToLeft, None, doc.memory.rtl, Read::Rtl),
        ui::text(app.t(Key::RememberedHere), TextStyle::CAP).color(paint.fg_2()),
    ]
    .spacing(SP3)
    .width(288);
    Some(
        container(ui::floating(&paint, content, 16))
            .align_right(Fill)
            .padding(Padding { top: 6.0, right: 132.0, bottom: 0.0, left: 0.0 })
            .into(),
    )
}

fn selection_float<'a>(app: &'a App, doc: &'a Doc) -> Option<Element<'a, Message>> {
    if doc.tool.is_some_and(|t| t != Tool::Select) {
        return None;
    }
    let selection = doc.selection.as_ref()?;
    let text = doc.texts.get(&selection.page)?;
    let first = crate::doc::text::line_rects(text, &selection.words).into_iter().next()?;
    let paint = app.paint;
    let zoom = doc.zoom();
    let (x, y) = page_origin(doc, selection.page);
    let dot = container(Space::new())
        .width(12)
        .height(12)
        .style(move |_| glide_sdk::glide_iced::style::rounded(hue_color(doc.hue), 6.0));
    let bar = row![
        ui::tool_label(
            &paint,
            Some(dot.into()),
            app.t(Key::ToolHighlight),
            false,
            Some(Message::Note(NoteMsg::Mark(Mark::Highlight)))
        ),
        ui::tool_label(
            &paint,
            Some(ui::icon(Icon::Underline, 16.0, paint.fg_2())),
            app.t(Key::ToolUnderline),
            false,
            Some(Message::Note(NoteMsg::Mark(Mark::Underline)))
        ),
        ui::tool_label(
            &paint,
            Some(ui::icon(Icon::Comment, 16.0, paint.fg_2())),
            app.t(Key::ToolComment),
            false,
            Some(Message::Note(NoteMsg::Comment))
        ),
        ui::tool_divider(&paint),
        ui::tool_button(&paint, Icon::Copy, app.t(Key::Copy), false, Some(Message::Note(NoteMsg::CopySelection))),
        ui::tool_button(&paint, Icon::Search, app.t(Key::Find), false, Some(Message::Note(NoteMsg::FindSelection))),
    ]
    .spacing(2)
    .align_y(Alignment::Center);
    let float = glide_sdk::anim::animate(1.0, glide_sdk::anim::Speed::SLIDE, move |_| ui::floating(&paint, bar, 4))
        .from(0.0)
        .offset(|v| Vector::new(0.0, 6.0 * (1.0 - v)));
    let left = (x + first.x * zoom).min(doc.viewport.width - 470.0).max(8.0);
    Some(place(float, left, y + first.y * zoom - 46.0))
}

fn note_float<'a>(app: &'a App, doc: &'a Doc) -> Option<Element<'a, Message>> {
    let id = doc.draft.as_ref().map(|(id, _)| *id).or(doc.focus)?;
    let note = doc.notes.iter().find(|n| n.id == id)?;
    let paint = app.paint;
    let zoom = doc.zoom();
    let (x, y) = page_origin(doc, note.page);
    let anchor = note.rects.first().copied().unwrap_or_default();
    let when = if doc.saved.iter().any(|s| s.id == id) {
        i18n::date(app.lang, &note.modified, true)
    } else {
        app.t(Key::JustNow).to_string()
    };
    let author = if note.author.is_empty() { app.t(Key::You).to_string() } else { note.author.clone() };
    let (body, actions): (Element<'a, Message>, Vec<Element<'a, Message>>) = match &doc.draft {
        Some((_, draft)) => (
            text_input(app.t(Key::CommentPlaceholder), draft)
                .id(DRAFT_ID)
                .on_input(|t| Message::Note(NoteMsg::Draft(t)))
                .on_submit(Message::Note(NoteMsg::DraftSave))
                .into(),
            vec![
                ui::sized_button(
                    &paint,
                    app.t(Key::Save),
                    ui::Tone::Accent,
                    Size::Small,
                    Some(Message::Note(NoteMsg::DraftSave)),
                ),
                ui::sized_button(
                    &paint,
                    app.t(Key::Cancel),
                    ui::Tone::Ghost,
                    Size::Small,
                    Some(Message::Note(NoteMsg::DraftCancel)),
                ),
            ],
        ),
        None => (
            ui::text(if note.comment.is_empty() { note.quote.clone() } else { note.comment.clone() }, TextStyle::CALL)
                .into(),
            vec![
                ui::sized_button(
                    &paint,
                    app.t(Key::Edit),
                    ui::Tone::Neutral,
                    Size::Small,
                    Some(Message::Note(NoteMsg::Edit(id))),
                ),
                ui::sized_button(
                    &paint,
                    app.t(Key::Remove),
                    ui::Tone::Ghost,
                    Size::Small,
                    Some(Message::Note(NoteMsg::Remove(id))),
                ),
            ],
        ),
    };
    if doc.draft.is_none() && note.comment.is_empty() && note.mark != Mark::Comment {
        return None;
    }
    let card = ui::note_card(&paint, &author, &when, body, actions);
    let left = (x + (anchor.x + anchor.w) * zoom + 12.0).min(doc.viewport.width - 280.0);
    Some(place(card, left, y + anchor.y * zoom - 8.0))
}

fn link_float<'a>(app: &'a App, doc: &'a Doc) -> Option<Element<'a, Message>> {
    let Some(Hover::Link(page, index)) = doc.hover.clone() else {
        return None;
    };
    let link = doc.links.get(&page)?.get(index)?;
    let paint = app.paint;
    let zoom = doc.zoom();
    let (x, y) = page_origin(doc, page);
    let (left, top) = (x + link.rect.x * zoom, y + (link.rect.y + link.rect.h) * zoom + 6.0);
    let card: Element<'a, Message> = match &link.target {
        crate::doc::Target::Page(to) => {
            let title = doc
                .opened
                .outline
                .iter()
                .rfind(|h| h.page.is_some_and(|p| p <= *to))
                .map(|h| h.title.clone())
                .unwrap_or_default();
            let size = doc.size(*to);
            ui::link_preview(
                &paint,
                doc.thumbs.get(to).cloned(),
                size.h / size.w.max(1.0),
                title,
                i18n::fill(app.t(Key::PageNumber), &[("n", &(to + 1).to_string())]),
                app.t(Key::LinkHint),
            )
        }
        crate::doc::Target::Uri(uri) => {
            let address = uri.trim_start_matches("https://").trim_start_matches("http://").to_string();
            ui::external_link(
                &paint,
                &address,
                app.t(Key::ExternalLink),
                (app.t(Key::Open), Message::Read(Read::Follow(link.target.clone()))),
                (app.t(Key::CopyAddress), Message::Toast(uri.clone())),
            )
        }
    };
    Some(place(card, left, top))
}

fn chip_float<'a>(app: &'a App, doc: &'a Doc) -> Option<Element<'a, Message>> {
    let (page, _) = doc.back_chip?;
    let paint = app.paint;
    let chip = ui::floating(
        &paint,
        ui::tool_label(
            &paint,
            Some(ui::icon(Icon::Back, 16.0, paint.fg_2())),
            i18n::fill(app.t(Key::BackTo), &[("n", &(page + 1).to_string())]),
            false,
            Some(Message::Read(Read::Back)),
        ),
        2,
    );
    Some(container(chip).align_bottom(Fill).padding(Padding { top: 0.0, right: 0.0, bottom: 16.0, left: 16.0 }).into())
}

fn ocr_float<'a>(app: &'a App, doc: &'a Doc) -> Option<Element<'a, Message>> {
    let paint = app.paint;
    let card: Element<'a, Message> = match &doc.ocr {
        Ocr::Offer => {
            let body = column![
                ui::text(app.t(Key::NoText), TextStyle::HEAD),
                ui::text(i18n::fill(app.t(Key::NoTextBody), &[("n", &doc.count().to_string())]), TextStyle::CALL)
                    .color(paint.fg_2()),
                row![
                    ui::sized_button(
                        &paint,
                        app.t(Key::Recognise),
                        ui::Tone::Accent,
                        Size::Small,
                        Some(Message::Show(Show::Ocr))
                    ),
                    ui::text(app.t(Key::Languages), TextStyle::CAP).color(paint.fg_2()),
                    Space::new().width(Fill),
                    ui::sized_button(
                        &paint,
                        app.t(Key::NotNow),
                        ui::Tone::Ghost,
                        Size::Small,
                        Some(Message::Show(Show::OcrDismiss))
                    ),
                ]
                .spacing(SP2)
                .align_y(Alignment::Center),
            ]
            .spacing(SP2);
            row![ui::inked_tile(Icon::Scan, paint.acc_soft(), paint.acc_text(), 30.0), body]
                .spacing(SP3)
                .width(560)
                .into()
        }
        Ocr::Running { done, total } => column![
            row![
                ui::text(app.t(Key::Recognising), TextStyle::HEAD).width(Fill),
                ui::text(format!("{done} / {total}"), TextStyle::NUM).color(paint.fg_2())
            ],
            ui::progress(&paint, Some(*done as f32 / (*total).max(1) as f32)),
            ui::text(app.t(Key::KeepReading), TextStyle::CAP).color(paint.fg_2()),
        ]
        .spacing(SP2)
        .width(420)
        .into(),
        _ => return None,
    };
    Some(
        container(ui::floating(&paint, card, 14)).center_x(Fill).padding(Padding { top: 16.0, ..Padding::ZERO }).into(),
    )
}

fn damaged_float<'a>(app: &'a App, doc: &'a Doc) -> Option<Element<'a, Message>> {
    if doc.opened.broken.is_empty() || doc.opened.format != crate::doc::Format::Pdf {
        return None;
    }
    let paint = app.paint;
    let read = doc.count() - doc.opened.broken.len();
    let body = column![
        ui::text(app.t(Key::Damaged), TextStyle::HEAD),
        ui::text(
            i18n::fill(app.t(Key::DamagedBody), &[("n", &read.to_string()), ("total", &doc.count().to_string())]),
            TextStyle::CALL
        )
        .color(paint.fg_2()),
    ]
    .spacing(2)
    .width(Fill);
    let card = row![
        ui::inked_tile(Icon::Warning, paint.warn_soft(), paint.warn(), 30.0),
        body,
        ui::sized_button(
            &paint,
            format!("{}…", app.t(Key::SaveCopy).trim_end_matches('…')),
            ui::Tone::Neutral,
            Size::Small,
            Some(Message::Show(Show::Recover))
        )
    ]
    .spacing(SP3)
    .align_y(Alignment::Center)
    .width(600);
    Some(
        container(ui::floating(&paint, card, Padding::from([12.0, 14.0])))
            .center_x(Fill)
            .padding(Padding { top: 16.0, ..Padding::ZERO })
            .into(),
    )
}

fn signing_float<'a>(app: &'a App, doc: &'a Doc) -> Option<Element<'a, Message>> {
    let signing = doc.signing.as_ref()?;
    let field = doc.opened.fields.get(signing.field)?;
    let paint = app.paint;
    let zoom = doc.zoom();
    let (x, y) = page_origin(doc, field.page);
    let modes = [(0u8, app.t(Key::Draw)), (1u8, app.t(Key::Type)), (2u8, app.t(Key::Picture))];
    let content = column![
        ui::text(app.t(Key::YourSignature), TextStyle::HEAD),
        ui::segmented_fill(&paint, &modes, 0u8, |_| Message::Ignore),
        stack![
            ui::signature_pad(&paint, signing.strokes.clone(), 120.0, |stroke| Message::Form(Form::Stroke(stroke))),
            container(ui::sized_button(
                &paint,
                app.t(Key::Erase),
                ui::Tone::Ghost,
                Size::Small,
                Some(Message::Form(Form::Erase))
            ))
            .align_right(Fill)
            .padding(6),
        ],
        row![
            Space::new().width(Fill),
            ui::sized_button(
                &paint,
                app.t(Key::Cancel),
                ui::Tone::Neutral,
                Size::Small,
                Some(Message::Form(Form::SignCancel))
            ),
            ui::sized_button(
                &paint,
                app.t(Key::PlaceSignature),
                ui::Tone::Accent,
                Size::Small,
                (!signing.strokes.is_empty()).then_some(Message::Form(Form::Place))
            ),
        ]
        .spacing(SP2),
    ]
    .spacing(SP3)
    .width(crate::view::SIGNATURE_PAD.0);
    let card = crate::view::SIGNATURE_PAD.0 + 28.0 + 16.0;
    let right = x + (field.rect.x + field.rect.w) * zoom + 16.0;
    let left = if right + card <= doc.viewport.width { right } else { (x + field.rect.x * zoom - card).max(8.0) };
    Some(place(ui::floating(&paint, content, 14), left, (y + field.rect.y * zoom - 120.0).max(8.0)))
}

fn status<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let paint = app.paint;
    let fraction = (doc.page() + 1) as f32 / doc.count().max(1) as f32;
    let percent = (fraction * 100.0).round() as u32;
    let left: Vec<Element<'a, Message>> = if doc.tool.is_some() {
        let hint = match doc.tool {
            Some(Tool::Highlight) => Key::HintHighlight,
            Some(Tool::Underline) => Key::HintUnderline,
            Some(Tool::Strike) => Key::HintStrike,
            Some(Tool::Comment) => Key::HintComment,
            Some(Tool::Ink) => Key::HintInk,
            Some(Tool::Text) => Key::HintText,
            _ => Key::HintSelect,
        };
        vec![ui::text(app.t(hint), TextStyle::CAP).color(paint.fg_2()).into()]
    } else if doc.find.as_ref().is_some_and(|f| f.open) {
        vec![
            ui::reading_progress(&paint, fraction, i18n::fill(app.t(Key::Progress), &[("n", &percent.to_string())])),
            Space::new().width(SP3).into(),
            ui::text(app.t(Key::FindHint), TextStyle::CAP).color(paint.fg_2()).into(),
        ]
    } else if doc.form && !doc.opened.fields.is_empty() {
        vec![ui::text(app.t(Key::TabHint), TextStyle::CAP).color(paint.fg_2()).into()]
    } else {
        vec![ui::reading_progress(&paint, fraction, i18n::fill(app.t(Key::Progress), &[("n", &percent.to_string())]))]
    };
    let mut right: Vec<Element<'a, Message>> = Vec::new();
    if doc.tool.is_some() {
        right.push(
            ui::text(app.t(if doc.editable() { Key::NotesStored } else { Key::NotesInCopy }), TextStyle::CAP)
                .color(paint.fg_2())
                .into(),
        );
    } else if doc.form && !doc.opened.fields.is_empty() {
        right.push(ui::text(app.t(Key::NotDigital), TextStyle::CAP).color(paint.fg_2()).into());
    } else if !doc.reflow() {
        let layouts = [
            (Layout::Single, Icon::Single, app.t(Key::LayoutSingle)),
            (Layout::Continuous, Icon::Continuous, app.t(Key::LayoutContinuous)),
            (Layout::Spread, Icon::Spread, app.t(Key::LayoutSpread)),
        ];
        right.push(ui::icon_segmented(&paint, &layouts, doc.memory.layout, |l| Message::Read(Read::Layout(l))));
        right.push(ui::tool_divider(&paint));
    }
    let level = ZOOM_LEVELS.iter().enumerate().min_by_key(|(_, l)| l.abs_diff(doc.percent())).map_or(2, |(i, _)| i);
    right.push(ui::zoom_control(
        &paint,
        level,
        ZOOM_LEVELS.len(),
        |l| Message::Read(Read::ZoomLevel(l)),
        (app.t(Key::ZoomOut), app.t(Key::ZoomIn)),
    ));
    right.push(ui::tool_label(
        &paint,
        None,
        format!("{} %", doc.percent()),
        false,
        Some(Message::Show(Show::ZoomMenu)),
    ));
    let corners = side_corners(if doc.memory.side { Side::BottomRight } else { Side::Bottom }, app.window.corner());
    ui::status_bar(&paint, left, right, corners)
}

pub fn view<'a>(app: &'a App, doc: &'a Doc) -> Element<'a, Message> {
    let paint = app.paint;
    let corner = app.window.corner();
    let canvas = if doc.reflow() { reflow(app, doc) } else { pages(app, doc) };
    let mut layers = vec![canvas];
    for float in [
        find_float(app, doc),
        tools_float(app, doc),
        ocr_float(app, doc),
        damaged_float(app, doc),
        selection_float(app, doc),
        note_float(app, doc),
        link_float(app, doc),
        signing_float(app, doc),
        popover(app, doc),
        chip_float(app, doc),
    ]
    .into_iter()
    .flatten()
    {
        layers.push(float);
    }
    let body = container(stack(layers)).width(Fill).height(Fill);
    let main = column![header(app, doc), body, status(app, doc)];
    let main_corners = side_corners(if doc.memory.side { Side::Right } else { Side::Top }, corner);
    let main = ui::pane(paint.view_bg(), main_corners, main);
    let _ = (SP1, ink(app));
    if doc.memory.side {
        ui::pane(paint.win_bg(), corner, row![super::side::view(app, doc), ui::vertical_separator(&paint), main]).into()
    } else {
        ui::pane(paint.win_bg(), corner, main).into()
    }
}

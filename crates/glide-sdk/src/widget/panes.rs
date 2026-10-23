use std::borrow::Cow;

use glide_iced::Paint;
use glide_iced::style::{self, R2, SP1, SP2, SP3};
use glide_theme::TextStyle;
use iced::border::Radius;
use iced::widget::{Column, Container, Row, Space, column, container, row};
use iced::{Alignment, Background, Border, Color, Element, Fill, Padding};

use super::{Wrapping, scrollable_x, text};

#[must_use]
pub fn pane<'a, M: 'a>(
    background: Color, corners: impl Into<Radius>, content: impl Into<Element<'a, M>>,
) -> Container<'a, M> {
    let corners = corners.into();
    container(content).width(Fill).height(Fill).style(move |_| style::rounded(background, corners))
}

#[must_use]
pub fn heading_row<'a, M: 'a>(
    paint: &Paint, title: impl Into<Cow<'a, str>>, actions: Vec<Element<'a, M>>,
) -> Element<'a, M> {
    let title = text(title, TextStyle::MICRO).color(paint.fg_3()).wrapping(Wrapping::None);
    actions
        .into_iter()
        .fold(row![title, Space::new().width(Fill)], Row::push)
        .spacing(SP1)
        .align_y(Alignment::Center)
        .padding(Padding::from([SP1, SP2]).left(SP3))
        .into()
}

#[must_use]
pub fn stat_rows<'a, M: 'a>(paint: &Paint, rows: Vec<(String, String)>) -> Element<'a, M> {
    let paint = *paint;
    Column::with_children(rows.into_iter().map(|(label, value)| {
        row![text(label, TextStyle::CALL).color(paint.fg_2()), Space::new().width(Fill), text(value, TextStyle::NUM)]
            .align_y(Alignment::Center)
            .into()
    }))
    .spacing(SP2)
    .into()
}

#[must_use]
pub fn code_block<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>, padding: f32) -> Element<'a, M> {
    let paint = *paint;
    container(scrollable_x(&paint, container(content).padding(padding)))
        .width(Fill)
        .style(move |_| container::Style {
            text_color: Some(paint.fg_1()),
            background: Some(Background::Color(paint.fill_1())),
            border: Border { color: paint.line(), width: 1.0, radius: R2.into() },
            ..container::Style::default()
        })
        .into()
}

#[must_use]
pub fn shortcut_list<'a, M: 'a>(paint: &Paint, sections: Vec<(String, Vec<(String, String)>)>) -> Element<'a, M> {
    let paint = *paint;
    let mut rows: Vec<Element<'a, M>> = Vec::new();
    for (heading, items) in sections.into_iter().filter(|(_, items)| !items.is_empty()) {
        rows.push(text(heading, TextStyle::MICRO).color(paint.fg_3()).into());
        rows.extend(items.into_iter().map(|(label, chord)| {
            row![
                text(label, TextStyle::CALL),
                Space::new().width(Fill),
                text(chord, TextStyle::CAP).color(paint.fg_2())
            ]
            .align_y(Alignment::Center)
            .into()
        }));
    }
    column(rows).spacing(SP1 + 2.0).into()
}

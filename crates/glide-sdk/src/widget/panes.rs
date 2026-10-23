use std::borrow::Cow;

use glide_iced::Paint;
use glide_iced::style::{self, R2, SP1, SP2, SP3};
use glide_theme::TextStyle;
use iced::border::Radius;
use iced::widget::{Column, Container, Row, Space, column, container, pane_grid, row};
use iced::{Alignment, Background, Border, Color, Element, Fill, Font, Padding, Size};

use super::{Wrapping, dot, scrollable_x, text};

#[must_use]
pub fn split_view<'a, T, M: 'a>(
    paint: &Paint, state: &'a pane_grid::State<T>,
    view: impl Fn(pane_grid::Pane, &'a T, bool) -> pane_grid::Content<'a, M>,
) -> pane_grid::PaneGrid<'a, M> {
    let paint = *paint;
    pane_grid(state, view).spacing(1).style(move |_| {
        let split = pane_grid::Line { color: paint.acc_bg(), width: 2.0 };
        pane_grid::Style {
            hovered_region: pane_grid::Highlight {
                background: Background::Color(paint.acc_soft()),
                border: Border { color: paint.acc_bg(), width: 1.0, radius: R2.into() },
            },
            picked_split: split,
            hovered_split: split,
        }
    })
}

#[must_use]
pub fn palette_swatch<'a, M: 'a>(
    paint: &Paint, background: Color, foreground: Color, accents: &[Color], font: Font, size: Size,
) -> Element<'a, M> {
    let line = paint.line();
    let dots = Row::with_children(accents.iter().map(|tint| dot(*tint, 6.0))).spacing(SP1);
    container(
        row![text("Aa", TextStyle::BODY).font(font).color(foreground), Space::new().width(Fill), dots]
            .align_y(Alignment::Center),
    )
    .padding(Padding::from([0.0, SP3]))
    .width(size.width)
    .height(size.height)
    .align_y(Alignment::Center)
    .style(move |_| container::Style {
        border: Border { color: line, width: 1.0, radius: R2.into() },
        ..style::rounded(background, R2)
    })
    .into()
}

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

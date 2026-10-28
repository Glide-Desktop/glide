use glide_iced::Paint;
use glide_iced::style::{self, R2, R3, SP1, SP2, SP3, SP4, SP5};
use glide_iced::widgets;
use glide_theme::elevation::Level;
use glide_theme::shape::R1;
use glide_theme::{TextStyle, component};
use iced::widget::{Column, Row, Space, button as control, column, container, row};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding};

use super::controls::radio_indicator;
use super::feedback::{dot, icon_tile};
use super::focus::activatable;
use super::layout::card_style;
use super::{Glyph, Wrapping, mix, text};

#[must_use]
pub fn compare_card<'a, M: 'a>(
    paint: &Paint, lead: impl Into<Element<'a, M>>, label: impl Into<String>, badge: Option<String>,
    lines: Vec<String>, emphasized: bool,
) -> Element<'a, M> {
    let paint = *paint;
    let mut heading = row![text(label.into(), TextStyle::CAP).color(paint.fg_2())].spacing(SP1 + 2.0);
    if let Some(badge) = badge {
        heading = heading.push(super::feedback::badge(&paint, badge, super::Severity::Accent));
    }
    let facts = lines.into_iter().enumerate().map(|(index, line)| {
        let figures = TextStyle { size_px: 12.5, line_height_px: 19.0, ..TextStyle::NUM };
        let line = text(line, figures).wrapping(Wrapping::None);
        if index == 0 { line.into() } else { line.color(paint.fg_2()).into() }
    });
    let content = row![
        container(lead.into()).center(40),
        Column::with_children(std::iter::once(heading.align_y(Alignment::Center).into()).chain(facts))
            .spacing(2)
            .width(Fill)
            .clip(true),
    ]
    .spacing(SP3)
    .align_y(Alignment::Center);
    container(content)
        .padding(SP3)
        .width(Fill)
        .style(move |_| {
            if emphasized {
                style::rounded(paint.acc_soft(), R3)
            } else {
                container::Style {
                    border: Border { color: paint.line(), width: 1.0, radius: R3.into() },
                    ..style::rounded(paint.win_bg(), R3)
                }
            }
        })
        .into()
}

#[must_use]
pub fn grid<'a, M: 'a>(cells: Vec<Element<'a, M>>, per_line: usize, spacing: f32) -> Element<'a, M> {
    let per_line = per_line.max(1);
    let mut lines = Column::new().spacing(spacing);
    let mut cells = cells.into_iter().peekable();
    while cells.peek().is_some() {
        let line: Vec<Element<'a, M>> = cells.by_ref().take(per_line).collect();
        let missing = per_line - line.len();
        lines = lines.push(
            Row::with_children(line).extend((0..missing).map(|_| Space::new().width(Fill).into())).spacing(spacing),
        );
    }
    lines.into()
}

#[must_use]
pub fn stat_tile<'a, M: 'a>(
    paint: &Paint, glyph: impl Glyph, color: Color, value: impl Into<std::borrow::Cow<'a, str>>,
    label: impl Into<std::borrow::Cow<'a, str>>,
) -> Element<'a, M> {
    let paint = *paint;
    let figures = column![text(value, TextStyle::T1), text(label, TextStyle::CAP).color(paint.fg_2())];
    container(column![icon_tile(glyph, color, 28.0), figures].spacing(SP3))
        .padding(SP3 + 2.0)
        .width(Fill)
        .style(move |_| card_style(&paint))
        .into()
}

fn portion(fraction: f32) -> u16 {
    (fraction.clamp(0.0, 1.0) * 1000.0).round() as u16
}

#[must_use]
pub fn usage_bar<'a, M: 'a>(paint: &Paint, segments: &[(f32, Color)], height: f32) -> Element<'a, M> {
    let track = paint.fill_2();
    let mut bar = Row::new().spacing(2).width(Fill).height(height);
    let mut used: u16 = 0;
    for &(fraction, color) in segments {
        let part = portion(fraction).min(1000 - used);
        if part == 0 {
            continue;
        }
        used += part;
        bar = bar.push(
            container(Space::new())
                .width(Length::FillPortion(part))
                .height(Fill)
                .style(move |_| style::rounded(color, R1)),
        );
    }
    if used < 1000 {
        bar = bar.push(
            container(Space::new())
                .width(Length::FillPortion(1000 - used))
                .height(Fill)
                .style(move |_| style::rounded(track, R1)),
        );
    }
    bar.into()
}

#[must_use]
pub fn legend<'a, M: 'a>(paint: &Paint, items: Vec<(Color, String, String)>) -> Element<'a, M> {
    let mut line = Row::new().spacing(SP4).align_y(Alignment::Center);
    for (color, label, value) in items {
        let mut entry =
            row![dot(color, 8.0), text(label, TextStyle::CAP)].spacing(SP1 + 2.0).align_y(Alignment::Center);
        if !value.is_empty() {
            entry = entry.push(text(value, TextStyle::CAP).color(paint.fg_3()));
        }
        line = line.push(entry);
    }
    line.wrap().vertical_spacing(SP1 + 2.0).into()
}

#[must_use]
pub fn usage_card<'a, M: 'a>(
    paint: &Paint, heading: impl Into<std::borrow::Cow<'a, str>>, summary: impl Into<std::borrow::Cow<'a, str>>,
    segments: Vec<(f32, Color, String, String)>,
) -> Element<'a, M> {
    let paint = *paint;
    let bars: Vec<(f32, Color)> = segments.iter().map(|(fraction, color, _, _)| (*fraction, *color)).collect();
    let items = segments.into_iter().map(|(_, color, label, value)| (color, label, value)).collect();
    let head = row![text(heading, TextStyle::HEAD).width(Fill), text(summary, TextStyle::CALL).color(paint.fg_2())]
        .spacing(SP3)
        .align_y(Alignment::Center);
    container(column![head, usage_bar(&paint, &bars, 10.0), legend(&paint, items)].spacing(SP3))
        .padding(SP4)
        .width(Fill)
        .style(move |_| card_style(&paint))
        .into()
}

#[must_use]
pub fn tinted_card_style(paint: &Paint, tint: Color) -> container::Style {
    container::Style {
        background: Some(Background::Gradient(iced::Gradient::Linear(
            iced::gradient::Linear::new(std::f32::consts::PI * 0.75)
                .add_stop(0.0, mix(paint.card_bg(), tint, glide_theme::motion::ST_PRESS))
                .add_stop(1.0, paint.card_bg()),
        ))),
        ..card_style(paint)
    }
}

#[must_use]
pub fn tinted_card<'a, M: 'a>(
    paint: &Paint, tint: Color, padding: impl Into<Padding>, content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let paint = *paint;
    container(content).padding(padding).width(Fill).style(move |_| tinted_card_style(&paint, tint)).into()
}

#[must_use]
pub fn preview_ring_style(paint: &Paint, selected: bool, status: control::Status) -> control::Style {
    let color = if selected {
        paint.acc_bg()
    } else if matches!(status, control::Status::Hovered | control::Status::Pressed) {
        paint.line_2()
    } else {
        Color::TRANSPARENT
    };
    control::Style {
        background: None,
        text_color: paint.fg_1(),
        border: Border { color, width: component::FOCUS_RING, radius: R3.into() },
        ..control::Style::default()
    }
}

#[must_use]
pub fn preview_card<'a, M: Clone + 'a>(
    paint: &Paint, content: impl Into<Element<'a, M>>, label: impl Into<std::borrow::Cow<'a, str>>, selected: bool,
    message: Option<M>,
) -> Element<'a, M> {
    let paint = *paint;
    let framed = control(content.into()).on_press_maybe(message.clone()).padding(SP1);
    let framed = widgets::button(framed, move |status| preview_ring_style(&paint, selected, status));
    let style = if selected { TextStyle { weight: 600, ..TextStyle::CAP } } else { TextStyle::CAP };
    column![
        activatable(&paint, framed, R3, message),
        text(label, style).color(if selected { paint.acc_text() } else { paint.fg_2() }),
    ]
    .spacing(SP1 + 2.0)
    .align_x(Alignment::Center)
    .into()
}

#[must_use]
pub fn choice_card<'a, M: Clone + 'a>(
    paint: &Paint, illustration: impl Into<Element<'a, M>>, heading: impl Into<std::borrow::Cow<'a, str>>,
    subtitle: Option<String>, selected: bool, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let mut labels = column![text(heading, TextStyle::HEAD)].spacing(1).width(Fill);
    if let Some(subtitle) = subtitle.filter(|text| !text.is_empty()) {
        labels = labels.push(text(subtitle, TextStyle::CAP).color(paint.fg_2()));
    }
    let body = column![
        container(illustration.into()).center_x(Fill),
        row![radio_indicator(&paint, selected), labels].spacing(SP2).align_y(Alignment::Center),
    ]
    .spacing(SP2 + 2.0);
    let card = control(body).on_press(message.clone()).padding(SP2 + 2.0).width(Fill);
    let card = widgets::button(card, move |status| control::Style {
        background: Some(Background::Color(paint.layered(paint.card_bg(), status))),
        text_color: paint.fg_1(),
        border: Border {
            color: if selected { paint.acc_bg() } else { paint.line() },
            width: if selected { component::FOCUS_RING } else { 1.0 },
            radius: R3.into(),
        },
        ..control::Style::default()
    });
    activatable(&paint, widgets::surface(card, &paint, Level::El1, R3), R3, Some(message))
}

#[must_use]
pub fn hero_card<'a, M: 'a>(
    paint: &Paint, glyph: impl Glyph, color: Color, heading: impl Into<std::borrow::Cow<'a, str>>,
    description: impl Into<std::borrow::Cow<'a, str>>, trailing: Option<Element<'a, M>>, stacked: bool,
) -> Element<'a, M> {
    let paint = *paint;
    let mut labels = column![
        text(heading, TextStyle::T2).wrapping(Wrapping::WordOrGlyph),
        text(description, TextStyle::CALL).color(paint.fg_2()),
    ]
    .spacing(SP1)
    .width(Fill);
    let mut end = None;
    match trailing {
        Some(trailing) if stacked => labels = labels.push(container(trailing).padding(Padding::ZERO.top(SP1))),
        other => end = other,
    }
    let mut line = row![icon_tile(glyph, color, 64.0), labels].spacing(SP4 + 2.0).align_y(Alignment::Center);
    if let Some(trailing) = end {
        line = line.push(trailing);
    }
    container(line).padding(SP5 - 4.0).width(Fill).style(move |_| tinted_card_style(&paint, color)).into()
}

#[must_use]
pub fn page_header<'a, M: 'a>(
    paint: &Paint, glyph: impl Glyph, color: Color, eyebrow: Option<&str>,
    heading: impl Into<std::borrow::Cow<'a, str>>, description: Option<&'a str>, trailing: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let mut labels = Column::new().spacing(2).width(Fill);
    if let Some(eyebrow) = eyebrow.filter(|text| !text.is_empty()) {
        labels = labels.push(text(eyebrow.to_string(), TextStyle::MICRO).color(paint.fg_3()));
    }
    labels = labels.push(text(heading, TextStyle::T1));
    if let Some(description) = description.filter(|text| !text.is_empty()) {
        labels = labels.push(text(description, TextStyle::BODY).color(paint.fg_2()));
    }
    let mut line = row![icon_tile(glyph, color, 52.0), labels].spacing(SP4).align_y(Alignment::Center);
    if let Some(trailing) = trailing {
        line = line.push(trailing);
    }
    line.into()
}

#[must_use]
pub fn sample_well<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let background = paint.sunken_bg();
    container(content)
        .padding(Padding::from([SP3, SP4]))
        .width(Fill)
        .style(move |_| style::rounded(background, R2))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glide_theme::{Accent, Appearance};

    #[test]
    fn selecting_a_preview_keeps_its_ring_width_so_neighbours_never_shift() {
        let paint = Paint::new(Appearance::Light, Accent::Blue);
        let on = preview_ring_style(&paint, true, control::Status::Active);
        let off = preview_ring_style(&paint, false, control::Status::Active);
        assert!((on.border.width - off.border.width).abs() < f32::EPSILON);
        assert_eq!(on.border.color, paint.acc_bg());
        assert_eq!(off.border.color, Color::TRANSPARENT);
    }
}

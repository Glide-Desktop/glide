use glide_iced::Paint;
use glide_iced::style::{self, R2, R3, SP2, SP3, SP4};
use glide_iced::widgets;
use glide_theme::elevation::Level;
use glide_theme::{TextStyle, component};
use iced::widget::{Column, Space, button as control, column, container, mouse_area, row, stack};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding, Vector, mouse};

use super::controls::chevron;
use super::focus::{activatable, focusable};
use super::{Icon, Wrapping, icon, mix, separator, text};
use crate::anim::{Speed, animate, collapse, hover};

#[must_use]
pub fn card_style(paint: &Paint) -> container::Style {
    container::Style {
        text_color: Some(paint.fg_1()),
        background: Some(Background::Color(paint.card_bg())),
        border: Border { color: paint.line(), width: 1.0, radius: R3.into() },
        ..container::Style::default()
    }
}

#[must_use]
pub fn card<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let paint = *paint;
    widgets::surface(
        container(content).padding(SP4).width(Fill).style(move |_| card_style(&paint)),
        &paint,
        Level::El1,
        R3,
    )
}

#[must_use]
pub fn action_card<'a, M: Clone + 'a>(paint: &Paint, content: impl Into<Element<'a, M>>, message: M) -> Element<'a, M> {
    let paint = *paint;
    let content = content.into();
    let lifted = hover(Speed::SPRING, move |value| {
        let value = value.clone();
        widgets::surface(
            container(content).padding(SP4).width(Fill).style(move |_| {
                let amount = value.get().clamp(0.0, 1.0);
                container::Style {
                    border: Border { color: mix(paint.line(), paint.line_2(), amount), width: 1.0, radius: R3.into() },
                    ..card_style(&paint)
                }
            }),
            &paint,
            Level::El1,
            R3,
        )
    })
    .offset(|value| Vector::new(0.0, -3.0 * value));
    let control = mouse_area(lifted).on_release(message.clone()).interaction(mouse::Interaction::Pointer);
    activatable(&paint, control, R3, Some(message))
}

#[must_use]
pub fn list<'a, M: 'a>(paint: &Paint, rows: Vec<Element<'a, M>>) -> Element<'a, M> {
    let paint = *paint;
    let mut items = column![];
    let count = rows.len();
    for (index, item) in rows.into_iter().enumerate() {
        items = items.push(item);
        if index + 1 < count {
            items = items.push(container(separator(&paint)).padding(Padding::from([0.0, SP4])));
        }
    }
    widgets::surface(container(items).width(Fill).clip(true).style(move |_| card_style(&paint)), &paint, Level::El1, R3)
}

fn row_label<'a, M: 'a>(paint: &Paint, title: &'a str, subtitle: Option<&'a str>) -> Element<'a, M> {
    let mut label = column![text(title, TextStyle::CALL)].spacing(2);
    if let Some(subtitle) = subtitle {
        label = label.push(text(subtitle, TextStyle::CAP).color(paint.fg_3()));
    }
    container(label).width(Fill).into()
}

#[must_use]
pub fn list_row<'a, M: 'a>(
    paint: &Paint, title: &'a str, subtitle: Option<&'a str>, trailing: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    container(row![row_label(paint, title, subtitle), trailing.into()].spacing(SP3).align_y(Alignment::Center))
        .padding(Padding::from([SP2, SP4]))
        .width(Fill)
        .into()
}

#[must_use]
pub fn action_row<'a, M: Clone + 'a>(
    paint: &Paint, title: &'a str, subtitle: Option<&'a str>, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let content = row![row_label(&paint, title, subtitle), icon(Icon::Right, 12.0, paint.fg_3())]
        .spacing(SP3)
        .align_y(Alignment::Center);
    let control = control(content).on_press(message.clone()).width(Fill).padding(Padding::from([SP2 + 2.0, SP4]));
    let control = widgets::button(control, move |status| {
        style::flat(paint.layered(Color::TRANSPARENT, status), paint.fg_1(), 0.0)
    });
    focusable(&paint, control, R2).inset().on_activate(Some(message)).into()
}

#[must_use]
pub fn section<'a, M: 'a>(paint: &Paint, title: &'a str, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    column![
        container(text(title, TextStyle::MICRO).color(paint.fg_3())).padding(Padding::from([0.0, SP2])),
        content.into()
    ]
    .spacing(SP2)
    .into()
}

#[must_use]
pub fn expander<'a, M: Clone + 'a>(
    paint: &Paint, title: &'a str, open: bool, on_toggle: M, content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let paint = *paint;
    let header = control(
        row![text(title, TextStyle::HEAD), Space::new().width(Fill), chevron::<M>(&paint, open)]
            .align_y(Alignment::Center),
    )
    .on_press(on_toggle.clone())
    .width(Fill)
    .padding(Padding::from([SP3, SP4]));
    let header = widgets::button(header, move |status| {
        style::flat(paint.layered(Color::TRANSPARENT, status), paint.fg_1(), 0.0)
    });
    let header = focusable(&paint, header, R3).inset().on_activate(Some(on_toggle));
    let body = collapse(open, container(content).padding(Padding::from([0.0, SP4]).bottom(SP4)).width(Fill));
    widgets::surface(
        container(column![header, body]).width(Fill).clip(true).style(move |_| card_style(&paint)),
        &paint,
        Level::El1,
        R3,
    )
}

#[must_use]
pub fn sidebar_item<'a, M: Clone + 'a>(
    paint: &Paint, glyph: Option<Icon>, label: &'a str, selected: bool, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    animate(if selected { 1.0 } else { 0.0 }, Speed::COLOR, move |value| {
        let tint = if selected { paint.acc_soft_fg() } else { paint.fg_3() };
        let mut content = row![].spacing(SP2 + 2.0).align_y(Alignment::Center);
        if let Some(glyph) = glyph {
            content = content.push(icon(glyph, 16.0, tint));
        }
        let style = if selected { TextStyle { weight: 600, ..TextStyle::CALL } } else { TextStyle::CALL };
        content = content.push(text(label, style).wrapping(Wrapping::None));
        let control = control(container(content).center_y(Fill).clip(true))
            .on_press(message.clone())
            .width(Fill)
            .height(component::CONTROL)
            .padding(Padding::from([0.0, SP3]));
        let value = value.clone();
        let item = widgets::button(control, move |status| {
            let amount = value.get().clamp(0.0, 1.0);
            let background = mix(paint.layered(Color::TRANSPARENT, status), paint.acc_soft(), amount);
            style::flat(background, mix(paint.fg_2(), paint.acc_soft_fg(), amount), R2)
        });
        let item = focusable(&paint, item, R2).inset().on_activate(Some(message));
        let bar = animate(if selected { 1.0 } else { 0.0 }, Speed::GROW, |_| {
            container(Space::new().width(3).height(18)).style(move |_| style::rounded(paint.acc_bg(), 2.0))
        })
        .from(0.0)
        .scale(|value| value.max(0.0));
        stack![item, container(bar).height(component::CONTROL).center_y(component::CONTROL)]
    })
    .into()
}

#[must_use]
pub fn sidebar<'a, M: 'a>(paint: &Paint, width: f32, items: Vec<Element<'a, M>>) -> Element<'a, M> {
    let background = paint.sidebar_bg();
    container(super::scrollable(paint, Column::with_children(items).spacing(2).padding(SP2)))
        .width(width)
        .height(Fill)
        .style(move |_| style::rounded(background, 0.0))
        .into()
}

#[must_use]
pub fn surface<'a, M: 'a>(background: Color, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content).width(Fill).height(Length::Fill).style(move |_| style::rounded(background, 0.0)).into()
}

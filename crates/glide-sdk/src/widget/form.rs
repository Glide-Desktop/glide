use glide_iced::Paint;
use glide_iced::style::{self, R2, SP1, SP2, SP3};
use glide_theme::TextStyle;
use iced::widget::{Row, TextInput, column, container, row, text_input};
use iced::{Alignment, Element, Fill, Length, Padding};

use super::inputs::{Validity, inline_style};
use super::{Glyph, icon, text};

const LINE_Y: f32 = 5.0;

#[must_use]
pub fn sheet<'a, M: 'a>(paint: &Paint, rows: Vec<Element<'a, M>>) -> Element<'a, M> {
    let well = paint.fill_1();
    let sheet = container(
        container(column(rows).spacing(2))
            .padding(Padding::from([SP2, SP1]))
            .width(Fill)
            .style(move |_| style::rounded(well, R2)),
    )
    .padding(Padding::from([SP1, SP2]));
    super::rising(sheet)
}

#[must_use]
pub fn key_values<'a, M: 'a>(paint: &Paint, rows: Vec<(String, String)>) -> Element<'a, M> {
    let paint = *paint;
    column(rows.into_iter().map(|(key, value)| {
        container(
            row![
                text(key, TextStyle::CALL).color(paint.fg_2()).width(Length::FillPortion(2)),
                text(value, TextStyle::CALL).color(paint.fg_1()).width(Length::FillPortion(3)).align_x(Alignment::End),
            ]
            .spacing(SP3)
            .align_y(Alignment::Center),
        )
        .padding(Padding::from([LINE_Y, SP3]))
        .into()
    }))
    .spacing(2)
    .into()
}

#[must_use]
pub fn form_row<'a, M: 'a>(paint: &Paint, label: &str, hint: Option<&str>, control: Element<'a, M>) -> Element<'a, M> {
    let mut labels = column![text(label.to_string(), TextStyle::BODY).color(paint.fg_1())].spacing(1);
    if let Some(hint) = hint.filter(|hint| !hint.is_empty()) {
        labels = labels.push(text(hint.to_string(), TextStyle::CAP).color(paint.fg_2()));
    }
    container(
        row![container(labels).width(Length::FillPortion(2)), container(control).width(Length::FillPortion(3))]
            .spacing(SP3)
            .align_y(Alignment::Center),
    )
    .padding(Padding::from([LINE_Y, SP3]))
    .into()
}

#[must_use]
pub fn hint_line<'a, M: 'a>(paint: &Paint, glyph: impl Glyph, message: &str) -> Element<'a, M> {
    container(
        row![
            icon(glyph, 14.0, paint.fg_3()),
            text(message.to_string(), TextStyle::CAP).color(paint.fg_2()).width(Fill)
        ]
        .spacing(SP2)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([SP1, SP3]))
    .into()
}

#[must_use]
pub fn action_bar<'a, M: 'a>(buttons: Vec<Element<'a, M>>) -> Element<'a, M> {
    container(Row::with_children(buttons).spacing(SP2).align_y(Alignment::Center))
        .align_right(Fill)
        .padding(Padding::from([SP1, SP3]))
        .into()
}

#[must_use]
pub fn boxed_input<'a, M: Clone + 'a>(
    paint: &Paint, placeholder: &str, value: &str, validity: Validity,
) -> TextInput<'a, M> {
    let paint = *paint;
    text_input(placeholder, value).padding(Padding::from([SP2, SP3 - 1.0])).size(TextStyle::CALL.size_px).style(
        move |_, status| {
            let mut style = inline_style(&paint, status);
            match validity {
                Validity::Neutral => {}
                Validity::Valid => style.border.color = paint.ok(),
                Validity::Invalid => {
                    style.border.color = paint.danger();
                    style.border.width = 1.5;
                }
            }
            style
        },
    )
}

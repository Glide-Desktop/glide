use std::fmt::Display;

use glide_iced::Paint;
use glide_iced::style::{self, R2, SP1, SP2, SP3};
use glide_iced::widgets;
use glide_theme::{TextStyle, component};
use iced::widget::text_editor;
use iced::widget::{Space, button as control, container, row, text_input};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding};

use super::buttons::{Tone, button_style};
use super::{Icon, icon, text};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Validity {
    #[default]
    Neutral,
    Valid,
    Invalid,
}

#[must_use]
pub fn input_style(paint: &Paint, status: text_input::Status) -> text_input::Style {
    let mut style = style::field(paint, status);
    style.background = Background::Color(Color::TRANSPARENT);
    style.border = Border::default();
    if status == text_input::Status::Disabled {
        style.value = paint.fg_4();
    }
    style
}

fn styled<'a, M: Clone + 'a>(paint: &Paint, input: text_input::TextInput<'a, M>) -> text_input::TextInput<'a, M> {
    let paint = *paint;
    input
        .size(TextStyle::BODY.size_px)
        .padding(Padding::from([SP1, 0.0]))
        .style(move |_, status| input_style(&paint, status))
}

fn well<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>, validity: Validity) -> Element<'a, M> {
    let inner =
        container(content).height(component::CONTROL).padding(Padding::from([0.0, SP3])).center_y(component::CONTROL);
    match validity {
        Validity::Neutral => widgets::field(inner, paint),
        Validity::Valid => widgets::tinted_field(inner, paint, paint.ok(), paint.ok_soft()),
        Validity::Invalid => widgets::tinted_field(inner, paint, paint.danger(), paint.danger_soft()),
    }
}

#[must_use]
pub fn field<'a, M: Clone + 'a>(paint: &Paint, input: text_input::TextInput<'a, M>) -> Element<'a, M> {
    well(paint, styled(paint, input), Validity::Neutral)
}

#[must_use]
pub fn validated_field<'a, M: Clone + 'a>(
    paint: &Paint, input: text_input::TextInput<'a, M>, validity: Validity,
) -> Element<'a, M> {
    well(paint, styled(paint, input), validity)
}

#[must_use]
pub fn text_field<'a, M: Clone + 'a>(
    paint: &Paint, placeholder: &str, value: &str, on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    field(paint, text_input(placeholder, value).on_input(on_input))
}

fn trailing<'a, M: Clone + 'a>(paint: &Paint, glyph: Icon, message: M) -> Element<'a, M> {
    let paint = *paint;
    let control = control(container(icon(glyph, 12.0, paint.fg_3())).center(Fill))
        .on_press(message)
        .width(22)
        .height(22)
        .padding(0);
    widgets::button(control, move |status| {
        let mut style = button_style(&paint, Tone::Ghost, status);
        style.border.radius = glide_theme::shape::R_ROUND.into();
        style
    })
}

#[must_use]
pub fn search_field<'a, M: Clone + 'a>(
    paint: &Paint, placeholder: &str, value: &str, on_input: impl Fn(String) -> M + 'a, on_clear: M,
) -> Element<'a, M> {
    let clear = (!value.is_empty()).then_some(on_clear);
    search_input(paint, text_input(placeholder, value).on_input(on_input), clear, "")
}

#[must_use]
pub fn search_input<'a, M: Clone + 'a>(
    paint: &Paint, input: text_input::TextInput<'a, M>, on_clear: Option<M>, clear_label: &'a str,
) -> Element<'a, M> {
    let lead = icon(Icon::Search, 14.0, paint.fg_3());
    let mut content = row![lead, styled(paint, input)].spacing(SP2).align_y(Alignment::Center);
    if let Some(on_clear) = on_clear {
        content = content.push(super::tooltip(paint, trailing(paint, Icon::Close, on_clear), clear_label));
    }
    well(paint, content, Validity::Neutral)
}

#[must_use]
pub fn password_field<'a, M: Clone + 'a>(
    paint: &Paint, placeholder: &str, value: &str, revealed: bool, on_input: impl Fn(String) -> M + 'a, on_reveal: M,
) -> Element<'a, M> {
    let input = styled(paint, text_input(placeholder, value).secure(!revealed).on_input(on_input));
    let content = row![input, trailing(paint, Icon::Eye, on_reveal)].spacing(SP2).align_y(Alignment::Center);
    well(paint, content, Validity::Neutral)
}

#[must_use]
pub fn text_area_style(paint: &Paint, status: text_editor::Status) -> text_editor::Style {
    let border = match status {
        text_editor::Status::Focused { .. } => paint.acc_bg(),
        text_editor::Status::Hovered => paint.fill_3(),
        _ => paint.line_2(),
    };
    text_editor::Style {
        background: Background::Color(paint.view_bg()),
        border: Border { color: border, width: 1.0, radius: R2.into() },
        placeholder: paint.fg_3(),
        value: if status == text_editor::Status::Disabled { paint.fg_4() } else { paint.fg_1() },
        selection: paint.acc_soft(),
    }
}

#[must_use]
pub fn bare_text_area_style(paint: &Paint, status: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        placeholder: paint.fg_4(),
        value: if status == text_editor::Status::Disabled { paint.fg_4() } else { paint.fg_1() },
        selection: paint.acc_soft(),
    }
}

#[must_use]
pub fn stepper<'a, T: Copy + PartialOrd + Display + 'a, M: Clone + 'a>(
    paint: &Paint, value: T, previous: Option<T>, next: Option<T>, on_change: impl Fn(T) -> M,
) -> Element<'a, M> {
    let paint = *paint;
    let step = |glyph: &'a str, target: Option<T>| {
        let control = control(container(text(glyph, TextStyle::HEAD)).center(Fill))
            .on_press_maybe(target.map(&on_change))
            .width(component::SMALL_CONTROL)
            .height(component::SMALL_CONTROL)
            .padding(0);
        widgets::button(control, move |status| button_style(&paint, Tone::Ghost, status))
    };
    container(
        row![
            step("−", previous),
            container(text(value.to_string(), TextStyle::NUM)).center_x(Length::Fixed(44.0)),
            step("+", next),
            Space::new().width(0)
        ]
        .align_y(Alignment::Center),
    )
    .padding(3)
    .style(move |_| container::Style {
        border: Border { color: paint.line(), width: 1.0, radius: R2.into() },
        ..style::rounded(paint.fill_1(), R2)
    })
    .into()
}

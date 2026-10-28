use std::borrow::Cow;
use std::time::Duration;

use glide_iced::Paint;
use glide_iced::style::{self, R2, SP1, SP2, SP3, SP4, SP5};
use glide_theme::shape::{R_ROUND, R1};
use glide_theme::{TextStyle, motion};
use iced::gradient::Linear;
use iced::widget::{Row, Space, column, container, row};
use iced::{Alignment, Background, Border, Color, Degrees, Element, Fill, Gradient, Length, Padding};

use super::{Icon, Wrapping, icon, text};
use crate::anim::{Speed, animate, spin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Severity {
    #[default]
    Neutral,
    Accent,
    Ok,
    Warn,
    Danger,
}

impl Severity {
    pub(crate) fn colors(self, paint: &Paint) -> (Color, Color) {
        match self {
            Self::Neutral => (paint.fill_2(), paint.fg_2()),
            Self::Accent => (paint.acc_soft(), paint.acc_soft_fg()),
            Self::Ok => (paint.ok_soft(), paint.ok()),
            Self::Warn => (paint.warn_soft(), paint.warn()),
            Self::Danger => (paint.danger_soft(), paint.danger()),
        }
    }
}

#[must_use]
pub fn badge<'a, M: 'a>(paint: &Paint, label: impl Into<Cow<'a, str>>, severity: Severity) -> Element<'a, M> {
    let (background, color) = severity.colors(paint);
    container(text(label, TextStyle::MICRO).color(color).wrapping(Wrapping::None))
        .padding(Padding::from([0.0, 7.0]))
        .height(20)
        .center_y(20)
        .style(move |_| style::rounded(background, R_ROUND))
        .into()
}

#[must_use]
pub fn status_badge<'a, M: 'a>(paint: &Paint, label: impl Into<Cow<'a, str>>, severity: Severity) -> Element<'a, M> {
    let (background, color) = severity.colors(paint);
    let content = row![dot(color, 6.0), text(label, TextStyle::CAP).color(color).wrapping(Wrapping::None)]
        .spacing(SP1 + 1.0)
        .align_y(Alignment::Center);
    container(content).padding(Padding::from([2.0, SP2])).style(move |_| style::rounded(background, R_ROUND)).into()
}

#[must_use]
pub fn dot<'a, M: 'a>(color: Color, side: f32) -> Element<'a, M> {
    container(Space::new().width(side).height(side)).style(move |_| style::rounded(color, R_ROUND)).into()
}

#[must_use]
pub fn busy_label<'a, M: 'a>(paint: &Paint, label: impl Into<Cow<'a, str>>) -> Element<'a, M> {
    row![super::controls::sized_spinner(paint, 14.0), text(label, TextStyle::CAP).color(paint.fg_3())]
        .spacing(SP1 + 2.0)
        .align_y(Alignment::Center)
        .into()
}

#[must_use]
pub fn key_combo<'a, M: 'a>(paint: &Paint, combo: &str, symbol: impl Fn(&str) -> String) -> Element<'a, M> {
    let keys = combo.split('+').filter(|part| !part.trim().is_empty()).map(|part| keycap(paint, symbol(part)));
    Row::with_children(keys).spacing(SP1).align_y(Alignment::Center).into()
}

#[must_use]
pub fn count_badge<'a, M: 'a>(paint: &Paint, count: u32) -> Element<'a, M> {
    let paint = *paint;
    let shown = count > 0;
    let label = if count > 99 { "99+".to_string() } else { count.to_string() };
    animate(if shown { 1.0 } else { 0.0 }, if shown { Speed::OVERSHOOT } else { Speed::STATE }, move |_| {
        container(
            text(label, TextStyle { tabular: true, ..TextStyle::MICRO }).color(paint.acc_fg()).wrapping(Wrapping::None),
        )
        .padding(Padding::from([1.0, SP1 + 2.0]))
        .center_x(iced::Length::Shrink)
        .style(move |_| style::rounded(paint.acc_bg(), R_ROUND))
    })
    .from(0.0)
    .scale(|value| 0.6 + 0.4 * value.clamp(0.0, 1.2))
    .into()
}

#[must_use]
pub fn banner<'a, M: 'a>(
    paint: &Paint, severity: Severity, title: &'a str, body: &'a str, actions: Vec<Element<'a, M>>,
) -> Element<'a, M> {
    let glyph = match severity {
        Severity::Ok => Icon::Check,
        Severity::Danger | Severity::Warn => Icon::Bell,
        _ => Icon::More,
    };
    noted_banner(paint, severity, glyph, title, body, actions)
}

#[must_use]
pub fn noted_banner<'a, M: 'a>(
    paint: &Paint, severity: Severity, glyph: impl super::Glyph, title: &str, body: &str, actions: Vec<Element<'a, M>>,
) -> Element<'a, M> {
    let (background, color) = match severity {
        Severity::Neutral => (paint.fill_1(), paint.fg_3()),
        other => other.colors(paint),
    };
    let mut lines = column![].spacing(2).width(Fill);
    if !title.is_empty() {
        lines = lines.push(text(title.to_string(), TextStyle::HEAD));
    }
    if !body.is_empty() {
        lines = lines.push(text(body.to_string(), TextStyle::CALL).color(paint.fg_2()));
    }
    let content =
        row![icon(glyph, 16.0, color), lines, Row::with_children(actions).spacing(SP2).align_y(Alignment::Center)]
            .spacing(SP3)
            .align_y(Alignment::Center);
    let strip_colour = if severity == Severity::Neutral { paint.fg_4() } else { color };
    let radius = iced::border::Radius::new(0).top_left(R2).bottom_left(R2);
    let strip: Element<'a, M> = container(Space::new().width(3).height(Fill))
        .height(Fill)
        .style(move |_| container::Style {
            border: Border { radius, ..Border::default() },
            ..style::rounded(strip_colour, 0.0)
        })
        .into();
    let body = container(content).padding(Padding::from([SP3, SP4]).left(SP4 + 3.0)).width(Fill);
    let banner = container(iced::widget::stack![body, container(strip).height(Fill)]).width(Fill).style(move |_| {
        container::Style { border: Border { color, width: 0.0, radius: R2.into() }, ..style::rounded(background, R2) }
    });
    super::rising(banner)
}

#[must_use]
pub fn empty_state<'a, M: 'a>(
    paint: &Paint, glyph: impl super::Glyph, title: impl Into<Cow<'a, str>>, body: impl Into<Cow<'a, str>>,
    action: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let (title, body) = (title.into(), body.into());
    let paint = *paint;
    let well =
        container(icon(glyph, 28.0, paint.fg_3())).center(64).style(move |_| style::rounded(paint.fill_1(), R_ROUND));
    let mut content = column![well].spacing(SP3).align_x(Alignment::Center).max_width(360);
    if !title.is_empty() {
        content = content.push(text(title, TextStyle::HEAD));
    }
    if !body.is_empty() {
        content = content.push(text(body, TextStyle::CALL).color(paint.fg_3()).align_x(Alignment::Center).width(Fill));
    }
    if let Some(action) = action {
        content = content.push(action);
    }
    super::rising(container(content).center(Fill).padding(SP5))
}

#[must_use]
pub fn empty_note<'a, M: 'a>(paint: &Paint, glyph: impl super::Glyph, title: &str, body: &str) -> Element<'a, M> {
    let paint = *paint;
    let well =
        container(icon(glyph, 22.0, paint.fg_3())).center(44).style(move |_| style::rounded(paint.fill_1(), R_ROUND));
    let mut content = column![well].spacing(SP2).align_x(Alignment::Center).max_width(420);
    if !title.is_empty() {
        content = content.push(text(title.to_string(), TextStyle::HEAD));
    }
    if !body.is_empty() {
        content = content
            .push(text(body.to_string(), TextStyle::CAP).color(paint.fg_2()).align_x(Alignment::Center).width(Fill));
    }
    super::rising(container(content).padding(Padding::from([SP5, SP4])).center_x(Fill))
}

#[must_use]
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let letters: String = match words.as_slice() {
        [] => return String::from("?"),
        [one] => one.chars().take(1).collect(),
        [first, .., last] => first.chars().take(1).chain(last.chars().take(1)).collect(),
    };
    letters.to_uppercase()
}

#[must_use]
pub fn avatar<'a, M: 'a>(paint: &Paint, name: &str, side: f32) -> Element<'a, M> {
    let initial = initials(name);
    let ramp = paint.tokens.ramp;
    let from = style::to_iced(ramp[1]);
    let to = style::to_iced(ramp[4]);
    let glyph = (side * 0.42).max(11.0);
    container(
        text(initial, TextStyle { size_px: glyph, line_height_px: glyph * 1.2, weight: 650, ..TextStyle::HEAD })
            .color(Color::WHITE),
    )
    .center(side)
    .style(move |_| container::Style {
        background: Some(Background::Gradient(Gradient::Linear(
            Linear::new(Degrees(135.0)).add_stop(0.0, from).add_stop(1.0, to),
        ))),
        border: Border { radius: R_ROUND.into(), ..Border::default() },
        ..container::Style::default()
    })
    .into()
}

#[must_use]
pub fn letter_tile<'a, M: 'a>(paint: &Paint, name: &str, side: f32) -> Element<'a, M> {
    let letter: String =
        name.chars().find(|c| c.is_alphanumeric()).map_or_else(|| String::from("?"), |c| c.to_uppercase().collect());
    let glyph = (side * 0.46).round();
    let well = paint.fill_2();
    container(
        text(letter, TextStyle { size_px: glyph, line_height_px: glyph * 1.2, weight: 600, ..TextStyle::HEAD })
            .color(paint.fg_2()),
    )
    .center(side)
    .style(move |_| style::rounded(well, R2))
    .into()
}

#[must_use]
pub fn icon_tile<'a, M: 'a>(glyph: impl super::Glyph, color: Color, side: f32) -> Element<'a, M> {
    use glide_theme::shape::{R3, R4};
    let radius = if side <= 32.0 {
        R2
    } else if side <= 56.0 {
        R3
    } else {
        R4
    };
    let top = super::mix(color, Color::WHITE, motion::ST_DRAG);
    container(icon(glyph, (side * 0.66).round(), Color::WHITE))
        .center(side)
        .style(move |_| container::Style {
            background: Some(Background::Gradient(Gradient::Linear(
                Linear::new(std::f32::consts::PI).add_stop(0.0, top).add_stop(1.0, color),
            ))),
            border: Border { color: Color { a: 0.12, ..Color::BLACK }, width: 0.5, radius: radius.into() },
            shadow: iced::Shadow {
                color: Color { a: 0.18, ..Color::BLACK },
                offset: iced::Vector::new(0.0, 1.0),
                blur_radius: 2.0,
            },
            ..container::Style::default()
        })
        .into()
}

pub struct Task<'a, M> {
    pub title: String,
    pub current: Option<String>,
    pub fraction: Option<f32>,
    pub paused: bool,
    pub detail: String,
    pub actions: Vec<Element<'a, M>>,
    pub footer: Option<Element<'a, M>>,
}

const TASK_FIGURES: TextStyle = TextStyle { size_px: 11.5, line_height_px: 16.0, ..TextStyle::NUM };

#[must_use]
pub fn task_card<'a, M: 'a>(paint: &Paint, glyph: impl super::Glyph, task: Task<'a, M>) -> Element<'a, M> {
    let paint = *paint;
    let title =
        text(task.title, TextStyle { weight: 630, ..TextStyle::CALL }).wrapping(Wrapping::WordOrGlyph).width(Fill);
    let head = row![icon(glyph, 16.0, paint.acc_text()), title].spacing(SP2).align_y(Alignment::Start);
    let mut card = column![head].spacing(SP2);
    if let Some(current) = task.current {
        card = card.push(text(current, TextStyle::CAP).color(paint.fg_2()).wrapping(Wrapping::None));
    }
    let detail = text(task.detail, TASK_FIGURES).color(paint.fg_2()).wrapping(Wrapping::WordOrGlyph).width(Fill);
    let detail = task.actions.into_iter().fold(row![detail].align_y(Alignment::Center), Row::push);
    card = card.push(super::controls::paused_progress(&paint, task.fraction, task.paused)).push(detail);
    if let Some(footer) = task.footer {
        card = card.push(footer);
    }
    let card = container(card)
        .padding(Padding::from([SP2 + 2.0, SP3]).right(SP2))
        .width(Fill)
        .style(move |_| super::layout::card_style(&paint));
    glide_iced::widgets::surface(card, &paint, glide_theme::elevation::Level::El1, style::R3)
}

#[must_use]
pub fn task_row<'a, M: 'a>(
    paint: &Paint, lead: impl Into<Element<'a, M>>, task: Task<'a, M>, width: f32,
) -> Element<'a, M> {
    let paint = *paint;
    let lines = column![
        text(task.title, TextStyle { weight: 630, ..TextStyle::CALL }).wrapping(Wrapping::None),
        text(task.detail, TASK_FIGURES).color(paint.fg_2()).wrapping(Wrapping::None),
    ]
    .push(task.fraction.map(|fraction| {
        container(super::controls::paused_progress(&paint, Some(fraction), task.paused)).padding(Padding::ZERO.top(2))
    }))
    .spacing(2)
    .width(Fill)
    .clip(true);
    let row = task.actions.into_iter().fold(
        row![container(lead.into()).center_x(SP4 + SP1), lines].spacing(SP2).align_y(Alignment::Center),
        Row::push,
    );
    container(row).padding(Padding::from([SP2 - 2.0, SP3])).width(width).into()
}

const PULSE: Duration = Duration::from_millis(1400);

fn pulse(phase: f32) -> f32 {
    let rise = 2.0 * phase.min(1.0 - phase);
    motion::ease_io(f64::from(rise.clamp(0.0, 1.0))) as f32
}

#[must_use]
pub fn skeleton<'a, M: 'a>(paint: &Paint, width: impl Into<Length>, height: f32) -> Element<'a, M> {
    let paint = *paint;
    let width = width.into();
    spin(PULSE, move |phase| {
        let phase = phase.clone();
        container(Space::new().width(width).height(height)).style(move |_| {
            let amount = 1.0 - 0.45 * pulse(phase.get());
            style::rounded(paint.fill_2().scale_alpha(amount), R1)
        })
    })
}

#[must_use]
pub fn keycap<'a, M: 'a>(paint: &Paint, label: impl Into<std::borrow::Cow<'a, str>>) -> Element<'a, M> {
    let paint = *paint;
    container(text(label, TextStyle::CAP).color(paint.fg_2()).wrapping(Wrapping::None))
        .padding(Padding::from([1.0, SP1 + 2.0]))
        .style(move |_| container::Style {
            border: Border { color: paint.line_2(), width: 1.0, radius: R1.into() },
            ..style::rounded(paint.fill_1(), R1)
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glide_theme::{Accent, Appearance};

    #[test]
    fn initials_take_the_first_and_last_words() {
        assert_eq!(initials("Артем Шевченко"), "АШ");
        assert_eq!(initials("artem"), "A");
        assert_eq!(initials("anna maria tr"), "AT");
        assert_eq!(initials(""), "?");
    }

    #[test]
    fn every_severity_pairs_a_soft_fill_with_its_strong_ink() {
        let paint = Paint::new(Appearance::Light, Accent::Blue);
        assert_eq!(Severity::Danger.colors(&paint), (paint.danger_soft(), paint.danger()));
        assert_eq!(Severity::Ok.colors(&paint), (paint.ok_soft(), paint.ok()));
        assert_ne!(Severity::Warn.colors(&paint).0, Severity::Neutral.colors(&paint).0);
    }
}

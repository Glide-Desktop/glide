use glide_iced::Paint;
use glide_iced::style::{self, R2, SP1, SP2, SP3, SP4, SP5};
use glide_theme::TextStyle;
use glide_theme::shape::{R_ROUND, R1};
use iced::gradient::Linear;
use iced::widget::{Row, column, container, row};
use iced::{Alignment, Background, Border, Color, Degrees, Element, Fill, Gradient, Padding};

use super::{Icon, Wrapping, icon, text};
use crate::anim::{Speed, animate};

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
    fn colors(self, paint: &Paint) -> (Color, Color) {
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
pub fn badge<'a, M: 'a>(paint: &Paint, label: &'a str, severity: Severity) -> Element<'a, M> {
    let (background, color) = severity.colors(paint);
    container(text(label, TextStyle::CAP).color(color).wrapping(Wrapping::None))
        .padding(Padding::from([2.0, SP2]))
        .style(move |_| style::rounded(background, R_ROUND))
        .into()
}

#[must_use]
pub fn count_badge<'a, M: 'a>(paint: &Paint, count: u32) -> Element<'a, M> {
    let paint = *paint;
    let shown = count > 0;
    let label = if count > 99 { "99+".to_string() } else { count.to_string() };
    animate(if shown { 1.0 } else { 0.0 }, if shown { Speed::SPRING } else { Speed::STATE }, move |_| {
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
    let (background, color) = match severity {
        Severity::Neutral => (paint.fill_1(), paint.fg_3()),
        other => other.colors(paint),
    };
    let glyph = match severity {
        Severity::Ok => Icon::Check,
        Severity::Danger | Severity::Warn => Icon::Bell,
        _ => Icon::More,
    };
    let mut lines = column![text(title, TextStyle::HEAD)].spacing(2).width(Fill);
    if !body.is_empty() {
        lines = lines.push(text(body, TextStyle::CALL).color(paint.fg_2()));
    }
    let content =
        row![icon(glyph, 16.0, color), lines, Row::with_children(actions).spacing(SP2).align_y(Alignment::Center)]
            .spacing(SP3)
            .align_y(Alignment::Center);
    container(content)
        .padding(Padding::from([SP3, SP4]).left(SP4 + 3.0))
        .width(Fill)
        .style(move |_| container::Style {
            border: Border { color, width: 0.0, radius: R2.into() },
            ..style::rounded(background, R2)
        })
        .into()
}

#[must_use]
pub fn empty_state<'a, M: 'a>(
    paint: &Paint, glyph: Icon, title: &'a str, body: &'a str, action: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let paint = *paint;
    let well =
        container(icon(glyph, 28.0, paint.fg_3())).center(64).style(move |_| style::rounded(paint.fill_1(), R_ROUND));
    let mut content = column![
        well,
        text(title, TextStyle::HEAD),
        text(body, TextStyle::CALL).color(paint.fg_3()).align_x(Alignment::Center)
    ]
    .spacing(SP3)
    .align_x(Alignment::Center)
    .max_width(360);
    if let Some(action) = action {
        content = content.push(action);
    }
    container(content).center(Fill).padding(SP5).into()
}

#[must_use]
pub fn avatar<'a, M: 'a>(paint: &Paint, name: &str, side: f32) -> Element<'a, M> {
    let initial: String = name.trim().chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
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
pub fn keycap<'a, M: 'a>(paint: &Paint, label: &'a str) -> Element<'a, M> {
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
    fn every_severity_pairs_a_soft_fill_with_its_strong_ink() {
        let paint = Paint::new(Appearance::Light, Accent::Blue);
        assert_eq!(Severity::Danger.colors(&paint), (paint.danger_soft(), paint.danger()));
        assert_eq!(Severity::Ok.colors(&paint), (paint.ok_soft(), paint.ok()));
        assert_ne!(Severity::Warn.colors(&paint).0, Severity::Neutral.colors(&paint).0);
    }
}

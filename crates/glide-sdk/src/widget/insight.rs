use std::borrow::Cow;

use glide_iced::Paint;
use glide_iced::style::{self, R2, R3, SP1, SP2, SP3, SP4};
use glide_iced::widgets;
use glide_theme::TextStyle;
use glide_theme::elevation::Level;
use glide_theme::shape::{R_ROUND, R1};
use iced::widget::{Column, Row, Space, button as control, column, container, mouse_area, row, stack};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding, mouse};

use super::buttons::{Tone, link, sized_button};
use super::cards::grid;
use super::feedback::{Severity, dot};
use super::layout::{action_card, card, card_style};
use super::{Icon, Wrapping, icon, separator, text};

pub const INFO_BUTTON: f32 = 22.0;

#[must_use]
pub fn applies_chip<'a, M: 'a>(paint: &Paint, label: impl Into<Cow<'a, str>>, notable: bool) -> Element<'a, M> {
    let (background, line, ink) = if notable {
        (paint.warn_soft(), paint.warn_soft(), paint.warn())
    } else {
        (Color::TRANSPARENT, paint.line(), paint.fg_3())
    };
    container(text(label, TextStyle::CAP).color(ink).wrapping(Wrapping::None))
        .padding(Padding::from([1.0, SP1 + 2.0]))
        .style(move |_| container::Style {
            background: Some(Background::Color(background)),
            border: Border { color: line, width: 1.0, radius: R1.into() },
            ..container::Style::default()
        })
        .into()
}

#[must_use]
pub fn info_button<'a, M: Clone + 'a>(paint: &Paint, open: bool, message: M) -> Element<'a, M> {
    let paint = *paint;
    let ink = if open { paint.acc_soft_fg() } else { paint.fg_2() };
    let face = container(text("i", TextStyle { weight: 650, ..TextStyle::CAP }).color(ink)).center(INFO_BUTTON);
    let button = control(face).on_press(message).padding(0);
    widgets::button(button, move |status| {
        let background = if open { paint.acc_soft() } else { paint.layered(Color::TRANSPARENT, status) };
        let border = if open { Color::TRANSPARENT } else { paint.line_2() };
        control::Style {
            background: Some(Background::Color(background)),
            text_color: ink,
            border: Border { color: border, width: 1.0, radius: R_ROUND.into() },
            ..control::Style::default()
        }
    })
}

#[must_use]
pub fn reset_action<'a, M: Clone + 'a>(
    paint: &Paint, note: impl Into<Cow<'a, str>>, label: impl Into<Cow<'a, str>>, message: Option<M>,
) -> Element<'a, M> {
    row![
        text(note, TextStyle::CAP).color(paint.fg_3()).wrapping(Wrapping::None),
        sized_button(paint, label.into(), Tone::Neutral, super::buttons::Size::Small, message),
    ]
    .spacing(SP2)
    .align_y(Alignment::Center)
    .into()
}

#[must_use]
pub fn status_chips<'a, M: 'a>(paint: &Paint, items: Vec<(Color, String)>) -> Element<'a, M> {
    let paint = *paint;
    let chips = items.into_iter().map(move |(color, label)| {
        let content = row![dot(color, 7.0), text(label, TextStyle::CAP).color(paint.fg_1()).wrapping(Wrapping::None)]
            .spacing(SP1 + 2.0)
            .align_y(Alignment::Center);
        container(content)
            .padding(Padding::from([SP1 + 2.0, SP2 + 2.0]))
            .style(move |_| style::rounded(paint.fill_1(), R2))
            .into()
    });
    Row::with_children(chips).spacing(SP2).align_y(Alignment::Center).wrap().into()
}

#[must_use]
pub fn fact_grid<'a, M: 'a>(paint: &Paint, facts: Vec<(String, String)>, per_line: usize) -> Element<'a, M> {
    let paint = *paint;
    let per_line = per_line.max(1);
    let cell = move |(label, value): (String, String)| -> Element<'a, M> {
        container(
            column![
                text(label, TextStyle::CAP).color(paint.fg_3()),
                text(value, TextStyle::BODY).wrapping(Wrapping::WordOrGlyph),
            ]
            .spacing(2),
        )
        .padding(Padding::from([SP2 + 2.0, SP3 + 2.0]))
        .width(Fill)
        .into()
    };
    let mut lines = Column::new();
    let mut facts = facts.into_iter().peekable();
    let mut first = true;
    while facts.peek().is_some() {
        if !first {
            lines = lines.push(separator(&paint));
        }
        first = false;
        let line: Vec<Element<'a, M>> = facts.by_ref().take(per_line).map(cell).collect();
        let missing = per_line - line.len();
        lines = lines.push(Row::with_children(line).extend((0..missing).map(|_| Space::new().width(Fill).into())));
    }
    let boxed = container(lines).width(Fill).style(move |_| card_style(&paint));
    widgets::surface(boxed, &paint, Level::El1, R3)
}

#[must_use]
pub fn signal_bars<'a, M: 'a>(paint: &Paint, level: u8, steps: u8) -> Element<'a, M> {
    let steps = steps.max(1);
    let (lit, unlit) = (paint.fg_2(), paint.fill_3());
    let bars = (1..=steps).map(|step| {
        let height = 4.0 + 10.0 * f32::from(step - 1) / f32::from(steps.max(2) - 1);
        let color = if step <= level { lit } else { unlit };
        container(Space::new().width(3).height(height)).style(move |_| style::rounded(color, 1.0)).into()
    });
    container(Row::with_children(bars).spacing(2).align_y(Alignment::End)).height(14).align_y(Alignment::End).into()
}

pub struct Issue<'a, M> {
    paint: Paint,
    title: String,
    summary: Option<String>,
    details: Vec<String>,
    open: bool,
    toggle: Option<(M, String, String)>,
    action: Option<Element<'a, M>>,
    severity: Severity,
}

impl<'a, M: Clone + 'a> Issue<'a, M> {
    #[must_use]
    pub fn new(paint: &Paint, title: impl Into<String>) -> Self {
        Self {
            paint: *paint,
            title: title.into(),
            summary: None,
            details: Vec::new(),
            open: false,
            toggle: None,
            action: None,
            severity: Severity::Warn,
        }
    }

    #[must_use]
    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    #[must_use]
    pub fn details(mut self, details: impl IntoIterator<Item = String>, open: bool) -> Self {
        self.details = details.into_iter().collect();
        self.open = open;
        self
    }

    #[must_use]
    pub fn toggle(mut self, message: M, more: impl Into<String>, less: impl Into<String>) -> Self {
        self.toggle = Some((message, more.into(), less.into()));
        self
    }

    #[must_use]
    pub fn action(mut self, action: impl Into<Element<'a, M>>) -> Self {
        self.action = Some(action.into());
        self
    }

    #[must_use]
    pub const fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    #[must_use]
    pub fn build(self) -> Element<'a, M> {
        self.into()
    }
}

impl<'a, M: Clone + 'a> From<Issue<'a, M>> for Element<'a, M> {
    fn from(issue: Issue<'a, M>) -> Self {
        let Issue { paint, title, summary, details, open, toggle, action, severity } = issue;
        let (well, ink) = match severity {
            Severity::Danger => (paint.danger_soft(), paint.danger()),
            Severity::Ok => (paint.ok_soft(), paint.ok()),
            Severity::Accent => (paint.acc_soft(), paint.acc_soft_fg()),
            Severity::Neutral => (paint.fill_1(), paint.fg_2()),
            Severity::Warn => (paint.warn_soft(), paint.warn()),
        };
        let mark = container(icon(Icon::Warning, 16.0, ink)).center(28).style(move |_| style::rounded(well, R2));
        let mut body = Column::new().spacing(1).width(Fill).push(text(title, super::ROW_TITLE));
        if let Some(summary) = summary {
            body = body.push(text(summary, super::ROW_NOTE).color(paint.fg_2()));
        }
        if !details.is_empty()
            && let Some((message, more, less)) = toggle
        {
            body = body.push(
                container(link(&paint, if open { less } else { more }, Some(message))).padding(Padding::ZERO.top(4)),
            );
        }
        if open && !details.is_empty() {
            let note = TextStyle { line_height_px: 18.0, ..super::ROW_NOTE };
            let lines = details.into_iter().map(|line| text(line, note).color(paint.fg_2()).into());
            body = body.push(
                container(
                    container(Column::with_children(lines).spacing(SP1))
                        .padding(Padding::from([SP2, SP2 + 2.0]))
                        .width(Fill)
                        .style(move |_| style::rounded(paint.fill_1(), R2)),
                )
                .padding(Padding::ZERO.top(6)),
            );
        }
        let mut line = row![mark, body].spacing(SP3).align_y(Alignment::Start);
        if let Some(action) = action {
            line = line.push(action);
        }
        container(line).padding(Padding::from([SP2, 14.0])).width(Fill).into()
    }
}

pub const GLANCE_HEIGHT: f32 = 150.0;
pub const GLANCE_HEADLINE: TextStyle =
    TextStyle { size_px: 19.0, line_height_px: 24.0, weight: 640, tracking_em: -0.018, ..TextStyle::T2 };

pub struct Glance<'a, M> {
    paint: Paint,
    tile: Element<'a, M>,
    title: String,
    aside: Option<Element<'a, M>>,
    headline: String,
    detail: Option<String>,
    meter: Option<(f32, Color)>,
    message: Option<M>,
}

impl<'a, M: Clone + 'a> Glance<'a, M> {
    #[must_use]
    pub fn new(paint: &Paint, lead: impl Into<Element<'a, M>>, title: impl Into<String>) -> Self {
        Self {
            paint: *paint,
            tile: lead.into(),
            title: title.into(),
            aside: None,
            headline: String::new(),
            detail: None,
            meter: None,
            message: None,
        }
    }

    #[must_use]
    pub fn headline(mut self, headline: impl Into<String>) -> Self {
        self.headline = headline.into();
        self
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        if !detail.is_empty() {
            self.detail = Some(detail);
        }
        self
    }

    #[must_use]
    pub fn meter(mut self, fraction: f32, color: Color) -> Self {
        self.meter = Some((fraction.clamp(0.0, 1.0), color));
        self
    }

    #[must_use]
    pub fn aside(mut self, aside: impl Into<Element<'a, M>>) -> Self {
        self.aside = Some(aside.into());
        self
    }

    #[must_use]
    pub fn on_press(mut self, message: M) -> Self {
        self.message = Some(message);
        self
    }

    #[must_use]
    pub fn build(self) -> Element<'a, M> {
        self.into()
    }
}

impl<'a, M: Clone + 'a> From<Glance<'a, M>> for Element<'a, M> {
    fn from(glance: Glance<'a, M>) -> Self {
        let Glance { paint, tile, title, aside, headline, detail, meter, message } = glance;
        let mut top = row![tile, text(title, TextStyle { weight: 600, ..super::ROW_TITLE }).width(Fill)]
            .spacing(SP2)
            .align_y(Alignment::Center);
        if let Some(aside) = aside {
            top = top.push(aside);
        }
        let mut content = Column::new().spacing(10).push(top);
        if !headline.is_empty() {
            content = content.push(text(headline, GLANCE_HEADLINE).wrapping(Wrapping::WordOrGlyph));
        }
        if let Some((fraction, color)) = meter {
            content = content.push(super::cards::usage_bar(&paint, &[(fraction, color)], 6.0));
        }
        if let Some(detail) = detail {
            content = content.push(text(detail, super::SECTION_NOTE).color(paint.fg_2()));
        }
        let content = container(content).height(Length::Fixed(GLANCE_HEIGHT - 2.0 * SP4 - 2.0));
        match message {
            Some(message) => action_card(&paint, content, message),
            None => card(&paint, content),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage {
    pub at: f32,
    pub label: String,
    pub color: Color,
}

impl Stage {
    #[must_use]
    pub fn new(at: f32, label: impl Into<String>, color: Color) -> Self {
        Self { at: at.clamp(0.0, 1.0), label: label.into(), color }
    }
}

fn portion(fraction: f32) -> u16 {
    (fraction.clamp(0.0, 1.0) * 1000.0).round() as u16
}

#[must_use]
pub fn stage_track<'a, M: 'a>(paint: &Paint, stages: &[Stage]) -> Element<'a, M> {
    let paint = *paint;
    let mut sorted: Vec<&Stage> = stages.iter().collect();
    sorted.sort_by(|a, b| a.at.total_cmp(&b.at));
    let mut marks = Row::new().width(Fill).align_y(Alignment::Start);
    let mut previous = 0.0;
    for stage in sorted {
        let gap = portion(stage.at - previous);
        if gap > 0 {
            marks = marks.push(Space::new().width(Length::FillPortion(gap)));
        }
        previous = stage.at;
        let color = stage.color;
        let ring = paint.card_bg();
        let knob = container(Space::new().width(12).height(12)).style(move |_| container::Style {
            background: Some(Background::Color(color)),
            border: Border { color: ring, width: 2.0, radius: R_ROUND.into() },
            ..container::Style::default()
        });
        marks = marks.push(
            column![knob, text(stage.label.clone(), TextStyle::CAP).color(color).wrapping(Wrapping::None)]
                .spacing(SP1)
                .align_x(Alignment::Start),
        );
    }
    let rest = portion(1.0 - previous);
    if rest > 0 {
        marks = marks.push(Space::new().width(Length::FillPortion(rest)));
    }
    let line = container(Space::new().width(Fill).height(2)).style(move |_| style::rounded(paint.line_2(), 1.0));
    let height = 12.0 + SP1 + TextStyle::CAP.line_height_px;
    let base = container(line).padding(Padding::ZERO.top(5)).width(Fill).height(height);
    stack![base, marks].into()
}

#[must_use]
pub fn choice_tiles<'a, M: Clone + 'a, T: Copy + PartialEq + 'a>(
    paint: &Paint, options: Vec<(T, String, String)>, selected: T, on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    let paint = *paint;
    let tiles: Vec<Element<'a, M>> = options
        .into_iter()
        .map(|(value, title, hint)| {
            let chosen = value == selected;
            let body = column![
                text(title, TextStyle { weight: 600, ..TextStyle::BODY }),
                text(hint, TextStyle::CAP).color(paint.fg_2()).wrapping(Wrapping::WordOrGlyph),
            ]
            .spacing(SP1);
            let face = container(body).padding(Padding::from([SP3, SP3 + 2.0])).width(Fill).style(move |_| {
                let (border, width) = if chosen { (paint.acc_bg(), 2.0) } else { (paint.line(), 1.0) };
                container::Style {
                    background: Some(Background::Color(paint.card_bg())),
                    border: Border { color: border, width, radius: R3.into() },
                    ..container::Style::default()
                }
            });
            mouse_area(face).on_release(on_select(value)).interaction(mouse::Interaction::Pointer).into()
        })
        .collect();
    let count = tiles.len();
    grid(tiles, count, SP3)
}

#[must_use]
pub fn labelled_switch<'a, M: Clone + 'a>(
    paint: &Paint, label: impl Into<Cow<'a, str>>, on: bool, on_toggle: Option<impl Fn(bool) -> M + 'a>,
) -> Element<'a, M> {
    row![
        text(label, TextStyle::CAP).color(paint.fg_2()).wrapping(Wrapping::None),
        super::sized_switch(paint, on, super::SwitchSize::Bar, on_toggle),
    ]
    .spacing(SP1 + 2.0)
    .align_y(Alignment::Center)
    .into()
}

#[must_use]
pub fn section_heading<'a, M: 'a>(paint: &Paint, title: impl Into<Cow<'a, str>>) -> Element<'a, M> {
    container(text(title, TextStyle::HEAD).color(paint.fg_1())).padding(Padding::from([0, 2])).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_are_laid_out_in_order_whatever_order_they_arrive_in() {
        let paint = Paint::new(glide_theme::Appearance::Light, glide_theme::Accent::Blue);
        let stages = [Stage::new(0.8, "сон", paint.ok()), Stage::new(0.3, "гасне", paint.acc_bg())];
        let _: Element<'_, ()> = stage_track(&paint, &stages);
        assert_eq!(portion(0.3), 300);
        assert_eq!(portion(1.4), 1000, "a stage past the end stays on the track");
    }
}

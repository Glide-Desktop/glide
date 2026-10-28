use glide_iced::Paint;
use glide_iced::style::{self, R3, SP2, SP3, SP4};
use glide_theme::TextStyle;
use iced::widget::{Column, Row, Space, column, container, row};
use iced::{Border, Element, Fill, Length, Padding};

use super::{Wrapping, text};

pub const INSPECTOR_WIDTH: f32 = 288.0;
pub const INSPECTOR_PREVIEW: f32 = 168.0;
const FACT_LABEL: f32 = 96.0;
const FACT_VALUE: f32 = INSPECTOR_WIDTH - 2.0 * SP4 - FACT_LABEL - SP3;

pub struct Inspector<'a, M> {
    paint: Paint,
    preview: Option<Element<'a, M>>,
    title: String,
    subtitle: Option<String>,
    facts: Vec<(String, String, bool)>,
    sections: Vec<(String, Element<'a, M>)>,
    actions: Vec<Element<'a, M>>,
}

impl<'a, M: 'a> Inspector<'a, M> {
    #[must_use]
    pub fn new(paint: &Paint, title: impl Into<String>) -> Self {
        Self {
            paint: *paint,
            preview: None,
            title: title.into(),
            subtitle: None,
            facts: Vec::new(),
            sections: Vec::new(),
            actions: Vec::new(),
        }
    }

    #[must_use]
    pub fn preview(mut self, preview: impl Into<Element<'a, M>>) -> Self {
        self.preview = Some(preview.into());
        self
    }

    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    #[must_use]
    pub fn fact(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        self.facts.push((label.into(), value.into(), false));
        self
    }

    #[must_use]
    pub fn figure(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        self.facts.push((label.into(), value.into(), true));
        self
    }

    #[must_use]
    pub fn section(mut self, title: impl Into<String>, content: impl Into<Element<'a, M>>) -> Self {
        self.sections.push((title.into(), content.into()));
        self
    }

    #[must_use]
    pub fn action(mut self, action: impl Into<Element<'a, M>>) -> Self {
        self.actions.push(action.into());
        self
    }
}

impl<'a, M: 'a> From<Inspector<'a, M>> for Element<'a, M> {
    fn from(inspector: Inspector<'a, M>) -> Self {
        let Inspector { paint, preview, title, subtitle, facts, sections, actions } = inspector;
        let mut body = Column::new().spacing(SP4).width(Fill).height(Fill);
        if let Some(preview) = preview {
            body =
                body.push(container(preview).center_x(Fill).center_y(INSPECTOR_PREVIEW).clip(true).style(move |_| {
                    container::Style {
                        border: Border { color: paint.line(), width: 1.0, radius: R3.into() },
                        ..style::rounded(paint.view_bg(), R3)
                    }
                }));
        }
        let mut heading = column![text(title, TextStyle::T3).wrapping(Wrapping::WordOrGlyph)].spacing(2);
        if let Some(subtitle) = subtitle {
            heading = heading.push(text(subtitle, TextStyle::CALL).color(paint.fg_2()).wrapping(Wrapping::WordOrGlyph));
        }
        body = body.push(heading);
        if !facts.is_empty() {
            let number = TextStyle { size_px: 12.5, line_height_px: 19.0, ..TextStyle::NUM };
            body = body.push(
                Column::with_children(facts.into_iter().map(|(label, value, mono)| {
                    row![
                        text(label, TextStyle::CALL).color(paint.fg_2()).width(FACT_LABEL),
                        text(value, if mono { number } else { TextStyle::CALL })
                            .width(Length::Fixed(FACT_VALUE))
                            .wrapping(Wrapping::WordOrGlyph),
                    ]
                    .spacing(SP3)
                    .into()
                }))
                .spacing(SP2),
            );
        }
        for (title, content) in sections {
            body = body.push(column![text(title, TextStyle::MICRO).color(paint.fg_2()), content].spacing(SP2));
        }
        if !actions.is_empty() {
            body = body.push(Space::new().height(Fill));
            body = body.push(Row::with_children(actions).spacing(SP2).width(Fill));
        }
        container(body)
            .padding(Padding { top: 24.0, right: SP4, bottom: SP4, left: SP4 })
            .width(INSPECTOR_WIDTH)
            .height(Fill)
            .style(move |_| style::rounded(paint.win_bg(), 0.0))
            .into()
    }
}

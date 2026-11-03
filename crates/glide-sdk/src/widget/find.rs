use glide_iced::Paint;
use glide_iced::style::{self, SP1, SP2, SP3, SP4};
use glide_theme::TextStyle;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use iced::widget::{Space, column, container, row, text_input::TextInput};
use iced::{Alignment, Color, Element, Fill, Length, Padding, Vector};

use crate::anim::{Speed, Transform, animate};

use super::buttons::{Size, Tone, compact_button, flat_toggle_button, icon_button};
use super::feedback::Severity;
use super::inputs::{field, search_input};
use super::{Icon, Wrapping, text, tooltip};

pub const FIND_FIELD: f32 = 260.0;

type Toggle<'a, M> = (&'a str, &'a str, bool, M);
type Labelled<'a, M> = (M, &'a str);
type Replace<'a, M> = (TextInput<'a, M>, Vec<(&'a str, M)>);

pub struct FindBar<'a, M> {
    paint: Paint,
    input: TextInput<'a, M>,
    status: String,
    failed: bool,
    steps: Option<(Labelled<'a, M>, Labelled<'a, M>)>,
    toggles: Vec<Toggle<'a, M>>,
    extra: Vec<Element<'a, M>>,
    done: Option<(&'a str, M)>,
    replace: Option<Replace<'a, M>>,
    narrow: bool,
    height: Option<f32>,
    floating: Option<f32>,
}

#[must_use]
pub fn find_bar<'a, M>(paint: &Paint, input: TextInput<'a, M>) -> FindBar<'a, M> {
    FindBar {
        paint: *paint,
        input,
        status: String::new(),
        failed: false,
        steps: None,
        toggles: Vec::new(),
        extra: Vec::new(),
        done: None,
        replace: None,
        narrow: false,
        height: None,
        floating: None,
    }
}

impl<'a, M> FindBar<'a, M> {
    #[must_use]
    pub fn status(mut self, status: impl Into<String>, failed: bool) -> Self {
        self.status = status.into();
        self.failed = failed;
        self
    }

    #[must_use]
    pub fn steps(mut self, previous: (M, &'a str), next: (M, &'a str)) -> Self {
        self.steps = Some((previous, next));
        self
    }

    #[must_use]
    pub fn toggle(mut self, label: &'a str, hint: &'a str, on: bool, message: M) -> Self {
        self.toggles.push((label, hint, on, message));
        self
    }

    #[must_use]
    pub fn extra(mut self, element: impl Into<Element<'a, M>>) -> Self {
        self.extra.push(element.into());
        self
    }

    #[must_use]
    pub fn done(mut self, label: &'a str, message: M) -> Self {
        self.done = Some((label, message));
        self
    }

    #[must_use]
    pub fn replace(mut self, input: TextInput<'a, M>, actions: Vec<(&'a str, M)>) -> Self {
        self.replace = Some((input, actions));
        self
    }

    #[must_use]
    pub fn narrow(mut self, narrow: bool) -> Self {
        self.narrow = narrow;
        self
    }

    #[must_use]
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    #[must_use]
    pub fn floating(mut self, width: f32) -> Self {
        self.floating = Some(width);
        self
    }
}

impl<'a, M: Clone + 'a> From<FindBar<'a, M>> for Element<'a, M> {
    fn from(bar: FindBar<'a, M>) -> Self {
        let paint = bar.paint;
        let width = if bar.narrow { Length::Fill } else { Length::Fixed(FIND_FIELD) };
        let search = container(search_input(&paint, bar.input, None, "")).width(width);
        let mut controls = row![].spacing(SP1).align_y(Alignment::Center).width(Fill);
        if let Some(((previous, before), (next, after))) = bar.steps {
            controls = controls.push(icon_button(&paint, Icon::Up, before, Some(previous), false)).push(icon_button(
                &paint,
                Icon::Down,
                after,
                Some(next),
                false,
            ));
        }
        for (label, hint, on, message) in bar.toggles {
            let control = flat_toggle_button(&paint, label, on, Size::Small, Some(move |_| message.clone()));
            controls = controls.push(tooltip(&paint, control, hint));
        }
        if !bar.status.is_empty() {
            let ink = if bar.failed { paint.danger() } else { paint.fg_3() };
            controls = controls.push(
                container(text(bar.status, TextStyle::NUM).color(ink).wrapping(Wrapping::None))
                    .padding(Padding::from([0.0, SP1])),
            );
        }
        controls = controls.push(Space::new().width(Fill));
        for element in bar.extra {
            controls = controls.push(element);
        }
        if let Some((label, message)) = bar.done {
            controls = controls.push(compact_button(&paint, label, Tone::Neutral, Some(message)));
        }
        let first: Element<'a, M> = if bar.narrow {
            column![search, controls].spacing(SP1).into()
        } else {
            row![search, controls].spacing(SP2).align_y(Alignment::Center).into()
        };
        let mut body = column![first].spacing(SP1);
        if let Some((input, actions)) = bar.replace {
            let mut replace =
                row![container(field(&paint, input)).width(width)].spacing(SP2).align_y(Alignment::Center);
            for (label, message) in actions {
                replace = replace.push(compact_button(&paint, label, Tone::Neutral, Some(message)));
            }
            body = body.push(replace);
        }
        let shown: Element<'a, M> = if let Some(width) = bar.floating {
            super::popover(&paint, container(body).padding(Padding::from([0.0, SP1])).width(width))
        } else {
            match bar.height {
                Some(height) => {
                    let background = paint.bar_bg();
                    container(body)
                        .padding(Padding::from([0.0, SP3]))
                        .width(Fill)
                        .height(height)
                        .center_y(height)
                        .style(move |_| style::rounded(background, 0.0))
                        .into()
                }
                None => bar_strip(&paint, None, body),
            }
        };
        arriving(shown)
    }
}

#[must_use]
pub fn arriving<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let content = content.into();
    animate(1.0, Speed::SLIDE, move |_| content)
        .from(0.0)
        .transform(|value| Transform {
            scale: ARRIVE_SCALE + (1.0 - ARRIVE_SCALE) * value,
            offset: Vector::new(0.0, -ARRIVE_RISE * (1.0 - value)),
        })
        .into()
}

#[must_use]
pub fn popping<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    animate(1.0, Speed::OVERSHOOT, |_| content.into()).from(0.0).scale(|value| value.max(0.001)).into()
}

#[must_use]
pub fn rising<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let content = content.into();
    animate(1.0, Speed::SLIDE, move |_| content)
        .from(0.0)
        .transform(|value| Transform { scale: 1.0, offset: Vector::new(0.0, ARRIVE_RISE * (1.0 - value)) })
        .into()
}

#[must_use]
pub fn arriving_over<'a, M: 'a>(backdrop: Color, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let content = content.into();
    animate(1.0, Speed::SLIDE, move |_| content)
        .from(0.0)
        .veil(backdrop)
        .transform(|value| Transform { scale: 1.0, offset: Vector::new(0.0, ARRIVE_RISE * (1.0 - value)) })
        .into()
}

#[must_use]
pub fn swapped<'a, M: 'a>(
    key: impl Hash, backdrop: Option<Color>, content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let content = content.into();
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    let swap = animate(1.0, Speed::SLIDE, move |_| content)
        .from(0.0)
        .keyed(hasher.finish())
        .transform(|value| Transform { scale: 1.0, offset: Vector::new(0.0, ARRIVE_RISE * (1.0 - value)) });
    match backdrop {
        Some(backdrop) => swap.veil(backdrop).into(),
        None => swap.into(),
    }
}

const ARRIVE_SCALE: f32 = 0.97;
const ARRIVE_RISE: f32 = 6.0;

#[must_use]
pub fn bar_strip<'a, M: 'a>(
    paint: &Paint, severity: Option<Severity>, content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    strip(paint, severity, Padding::from([SP2, SP3]), content)
}

fn strip<'a, M: 'a>(
    paint: &Paint, severity: Option<Severity>, padding: Padding, content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let background = match severity {
        None => paint.bar_bg(),
        Some(Severity::Warn) => paint.warn_soft(),
        Some(Severity::Danger) => paint.danger_soft(),
        Some(Severity::Ok) => paint.ok_soft(),
        Some(Severity::Accent) => paint.acc_soft(),
        Some(Severity::Neutral) => paint.fill_1(),
    };
    container(content).padding(padding).width(Fill).style(move |_| style::rounded(background, 0.0)).into()
}

pub const INFO_BAR: f32 = glide_theme::component::SMALL_CONTROL + 2.0 * SP2;
const BANNER_STRIP: f32 = 3.0;

#[must_use]
pub fn info_bar<'a, M: 'a>(
    paint: &Paint, severity: Option<Severity>, title: &str, detail: &str, actions: Vec<Element<'a, M>>,
) -> Element<'a, M> {
    let weight = if detail.is_empty() { TextStyle::CALL.weight } else { 630 };
    let mut content = row![text(title.to_string(), TextStyle { weight, ..TextStyle::CALL }).wrapping(Wrapping::None)]
        .spacing(SP3)
        .align_y(Alignment::Center);
    let blurb = text(detail.to_string(), TextStyle::CALL).color(paint.fg_2()).wrapping(Wrapping::None);
    content = content.push(container(blurb).width(Fill).clip(true));
    let content = actions.into_iter().fold(content, iced::widget::Row::push);
    let content = container(content).height(INFO_BAR).align_y(Alignment::Center);
    let edge = match severity {
        Some(Severity::Warn) => Some(paint.warn()),
        Some(Severity::Danger) => Some(paint.danger()),
        Some(Severity::Ok) => Some(paint.ok()),
        Some(Severity::Accent) => Some(paint.acc_bg()),
        Some(Severity::Neutral) => Some(paint.fg_4()),
        None => None,
    };
    let Some(edge) = edge else {
        return strip(paint, severity, Padding::from([0.0, SP3]), content);
    };
    let mark = container(Space::new().width(BANNER_STRIP).height(INFO_BAR)).style(move |_| style::rounded(edge, 0.0));
    let body = container(content).padding(Padding::from([0.0, SP3]).left(SP4 - BANNER_STRIP)).width(Fill);
    strip(paint, severity, Padding::ZERO, row![mark, body])
}

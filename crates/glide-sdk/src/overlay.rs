use std::time::{Duration, Instant};

use glide_iced::Paint;
use glide_iced::style::{self, R2, R3, SP1, SP2, SP3, SP4, SP5, SP6};
use glide_iced::widgets;
use glide_theme::elevation::Level;
use glide_theme::{TextStyle, component, motion};
use glide_ui::Reveal;
use iced::keyboard::{Key, key::Named};
use iced::widget::{Space, column, container, mouse_area, opaque, row, stack};
use iced::{Alignment, Color, Element, Fill, Padding, Size};

use crate::widget::{Tone, Wrapping, button, text};

pub const TOAST: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, PartialEq)]
pub struct Choice<M> {
    pub label: String,
    pub tone: Tone,
    pub message: Option<M>,
}

#[derive(Debug, Clone)]
pub struct Dialog<M> {
    pub title: String,
    pub body: String,
    pub choices: Vec<Choice<M>>,
    pub width: f32,
    pub reveal: Reveal,
}

impl<M> Dialog<M> {
    #[must_use]
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        let mut reveal = Reveal::closed();
        reveal.open(motion::t2());
        Self { title: title.into(), body: body.into(), choices: Vec::new(), width: 420.0, reveal }
    }

    #[must_use]
    pub fn choice(mut self, label: impl Into<String>, tone: Tone, message: M) -> Self {
        self.choices.push(Choice { label: label.into(), tone, message: Some(message) });
        self
    }

    #[must_use]
    pub fn cancel(mut self, label: impl Into<String>) -> Self {
        self.choices.push(Choice { label: label.into(), tone: Tone::Neutral, message: None });
        self
    }

    #[must_use]
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    #[must_use]
    pub fn default_choice(&self) -> Option<usize> {
        self.choices.iter().position(|choice| choice.tone == Tone::Accent)
    }

    #[must_use]
    pub fn cancel_choice(&self) -> Option<usize> {
        self.choices.iter().position(|choice| choice.message.is_none())
    }

    #[must_use]
    pub fn key(&self, key: &Key) -> Option<Answer> {
        match key {
            Key::Named(Named::Escape) => Some(Answer::Dismiss),
            Key::Named(Named::Enter) => self.default_choice().map(Answer::Chose),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Chose(usize),
    Dismiss,
}

#[must_use]
pub fn dialog<'a, M>(paint: &Paint, dialog: &'a Dialog<M>, window: Size, corner: f32) -> Element<'a, Answer> {
    let progress = dialog.reveal.value(motion::t2()) as f32;
    let paint = paint.faded(progress);
    let choices: Vec<(&str, Tone, Option<Answer>)> = dialog
        .choices
        .iter()
        .enumerate()
        .map(|(index, choice)| (choice.label.as_str(), choice.tone, Some(Answer::Chose(index))))
        .collect();
    let body: Option<Element<'a, Answer>> =
        (!dialog.body.is_empty()).then(|| text(&dialog.body, TextStyle::CALL).color(paint.fg_2()).into());
    let card = dialog_card(&paint, &dialog.title, body, dialog_buttons(&paint, choices, None), dialog.width, window);
    modal(&paint, progress, card, Some(Answer::Dismiss), corner)
}

#[must_use]
pub fn modal<'a, M: Clone + 'a>(
    paint: &Paint, progress: f32, card: impl Into<Element<'a, M>>, dismiss: Option<M>, corner: f32,
) -> Element<'a, M> {
    let scrim = Color { a: 0.32 * progress, ..Color::BLACK };
    let scale = component::DIALOG_CLOSED_SCALE + (1.0 - component::DIALOG_CLOSED_SCALE) * progress;
    let backdrop = container(Space::new().width(Fill).height(Fill)).style(move |_| style::rounded(scrim, corner));
    let backdrop: Element<'a, M> = match dismiss {
        Some(message) => mouse_area(backdrop).on_press(message).into(),
        None => backdrop.into(),
    };
    stack![
        opaque(backdrop),
        container(widgets::scale(opaque(widgets::surface(card, paint, Level::El3, R3)), scale)).center(Fill),
    ]
    .into()
}

#[must_use]
pub fn dialog_card<'a, M: 'a>(
    paint: &Paint, title: &str, body: Option<Element<'a, M>>, buttons: Element<'a, M>, width: f32, window: Size,
) -> Element<'a, M> {
    let paint = *paint;
    let mut content = column![].spacing(SP3);
    if !title.is_empty() {
        content = content.push(text(title.to_string(), TextStyle::T3).wrapping(Wrapping::WordOrGlyph));
    }
    if let Some(body) = body {
        content = content.push(body);
    }
    let content = crate::widget::scrollable(&paint, content.push(Space::new().height(SP1)).push(buttons));
    container(content)
        .padding(SP4)
        .width(width)
        .max_width((window.width - SP6).max(1.0))
        .max_height((window.height - 2.0 * SP4).max(1.0))
        .style(move |_| style::popover(&paint))
        .into()
}

#[must_use]
pub fn dialog_buttons<'a, M: Clone + 'a>(
    paint: &Paint, choices: Vec<(&str, Tone, Option<M>)>, focus: Option<usize>,
) -> Element<'a, M> {
    let paint = *paint;
    let control = |index: usize, (label, tone, message): (&str, Tone, Option<M>)| {
        ringed(&paint, button(&paint, label.to_string(), tone, message), R2, focus == Some(index))
    };
    let (left, right): (Vec<_>, Vec<_>) =
        choices.into_iter().enumerate().partition(|(_, (_, tone, _))| matches!(tone, Tone::Danger | Tone::Ghost));
    left.into_iter()
        .map(|(index, choice)| control(index, choice))
        .chain(std::iter::once(Space::new().width(Fill).into()))
        .chain(right.into_iter().map(|(index, choice)| control(index, choice)))
        .fold(row![].spacing(SP2).align_y(Alignment::Center), iced::widget::Row::push)
        .into()
}

pub const FINDER_ROWS: usize = 8;
pub const FINDER_WIDTH: f32 = 580.0;
pub const FINDER_ROW: f32 = 36.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinderRow {
    pub label: String,
    pub detail: String,
}

#[must_use]
pub fn finder_window(count: usize, selected: usize) -> std::ops::Range<usize> {
    let start = (selected + 1).saturating_sub(FINDER_ROWS);
    start..(start + FINDER_ROWS).min(count)
}

#[must_use]
#[allow(clippy::too_many_arguments)] // One call draws one finder; every argument is a distinct input
pub fn finder<'a, M: Clone + 'a>(
    paint: &Paint, progress: f32, input: Element<'a, M>, rows: &[FinderRow], selected: usize, empty: &'a str,
    on_pick: impl Fn(usize) -> M, dismiss: M, window: Size, corner: f32,
) -> Element<'a, M> {
    let paint = paint.faded(progress);
    let shown = finder_window(rows.len(), selected);
    let list: Element<'a, M> = if rows.is_empty() {
        container(text(empty, TextStyle::CALL).color(paint.fg_3())).padding(SP4).into()
    } else {
        let items = shown.map(|index| {
            let active = index == selected;
            let row = &rows[index];
            let color = if active { paint.acc_soft_fg() } else { paint.fg_1() };
            let content = row![
                text(row.label.clone(), TextStyle::CALL).color(color).wrapping(Wrapping::None),
                Space::new().width(Fill),
                text(row.detail.clone(), TextStyle::CAP).color(paint.fg_3()).wrapping(Wrapping::None),
            ]
            .spacing(SP3)
            .align_y(Alignment::Center);
            let control = iced::widget::button(container(content).center_y(Fill).clip(true))
                .on_press(on_pick(index))
                .width(Fill)
                .height(FINDER_ROW)
                .padding(Padding::from([0.0, SP3]));
            widgets::button(control, move |status| {
                let background = if active { paint.acc_soft() } else { paint.layered(Color::TRANSPARENT, status) };
                style::flat(background, color, R2)
            })
        });
        column(items).spacing(2).padding(SP1).into()
    };
    let card = container(column![container(input).padding(SP2), crate::widget::separator(&paint), list])
        .width(FINDER_WIDTH)
        .max_width((window.width - SP6).max(1.0));
    let scale = component::DIALOG_CLOSED_SCALE + (1.0 - component::DIALOG_CLOSED_SCALE) * progress;
    let scrim = Color { a: 0.32 * progress, ..Color::BLACK };
    let backdrop = container(Space::new().width(Fill).height(Fill)).style(move |_| style::rounded(scrim, corner));
    stack![
        opaque(mouse_area(backdrop).on_press(dismiss)),
        container(widgets::scale(opaque(widgets::surface(card, &paint, Level::El3, R3)), scale))
            .width(Fill)
            .height(Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .padding(Padding::ZERO.top(window.height * 0.14)),
    ]
    .into()
}

#[must_use]
pub fn ringed<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>, radius: f32, on: bool) -> Element<'a, M> {
    let color = if on { paint.acc_ring() } else { Color::TRANSPARENT };
    container(content)
        .padding(crate::widget::RING)
        .style(move |_| container::Style {
            border: iced::Border { color, width: crate::widget::RING, radius: (radius + crate::widget::RING).into() },
            ..container::Style::default()
        })
        .into()
}

#[derive(Debug, Clone)]
pub struct Toast<M> {
    pub text: String,
    pub action: Option<(String, M)>,
    pub at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastAction;

impl<M> Toast<M> {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), action: None, at: Instant::now() }
    }

    #[must_use]
    pub fn action(mut self, label: impl Into<String>, message: M) -> Self {
        self.action = Some((label.into(), message));
        self
    }

    #[must_use]
    pub fn progress(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.at);
        let duration = motion::t2();
        if duration.is_zero() {
            return if elapsed < TOAST { 1.0 } else { 0.0 };
        }
        let rise = (elapsed.as_secs_f32() / duration.as_secs_f32()).min(1.0);
        let fall = (TOAST.saturating_sub(elapsed).as_secs_f32() / duration.as_secs_f32()).min(1.0);
        motion::ease_out(f64::from(rise.min(fall))) as f32
    }

    #[must_use]
    pub fn expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.at) >= TOAST
    }

    #[must_use]
    pub fn animating(&self, now: Instant) -> bool {
        let progress = self.progress(now);
        !self.expired(now) && progress < 1.0
    }
}

#[must_use]
pub fn toast<'a, M>(paint: &Paint, toast: &'a Toast<M>, window: Size) -> Element<'a, ToastAction> {
    let progress = toast.progress(Instant::now());
    let action = toast.action.as_ref().map(|(label, _)| (label.as_str(), ToastAction));
    toast_pill(paint, progress, &toast.text, None, action, window)
}

#[must_use]
pub fn toast_pill<'a, M: Clone + 'a>(
    paint: &Paint, progress: f32, message: &str, ink: Option<Color>, action: Option<(&str, M)>, window: Size,
) -> Element<'a, M> {
    let paint = paint.faded(progress);
    let card = toast_card(&paint, message, ink, action, None, (window.width - 48.0).max(1.0));
    container(card)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::End)
        .padding(Padding::from([0.0, SP5]).bottom(SP5 + (1.0 - progress) * component::POPOVER_RISE))
        .into()
}

#[must_use]
pub fn toast_motion<'a, M: 'a>(card: impl Into<Element<'a, M>>, progress: f32, entering: bool) -> Element<'a, M> {
    const RISE: f32 = 16.0;
    const SHRINK: f32 = 0.04;
    let (drop, scale) = if entering { ((1.0 - progress) * RISE, 1.0) } else { (0.0, 1.0 - (1.0 - progress) * SHRINK) };
    glide_iced::widgets::lift(card, scale, drop)
}

#[must_use]
pub fn toast_card<'a, M: Clone + 'a>(
    paint: &Paint, message: &str, ink: Option<Color>, action: Option<(&str, M)>, close: Option<(M, &'a str)>,
    max_width: f32,
) -> Element<'a, M> {
    let paint = *paint;
    let mut label = text(message.to_string(), TextStyle::CALL).wrapping(Wrapping::WordOrGlyph);
    if let Some(ink) = ink {
        label = label.color(Color { a: ink.a * paint.alpha, ..ink });
    }
    let mut content = row![label].spacing(SP3).align_y(Alignment::Center);
    let compact = action.is_some() || close.is_some();
    if let Some((label, message)) = action {
        content = content.push(crate::widget::compact_button(&paint, label.to_string(), Tone::Tonal, Some(message)));
    }
    if let Some((message, hint)) = close {
        content =
            content.push(crate::widget::icon_button(&paint, crate::widget::Icon::Close, hint, Some(message), false));
    }
    let padding = if compact { Padding::from([SP1, SP1]).left(SP4) } else { Padding::from([SP2, SP4]) };
    let pill = container(content).padding(padding).max_width(max_width).style(move |_| style::popover(&paint));
    widgets::surface(pill, &paint, Level::El3, R3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_finder_list_scrolls_with_the_selection_and_never_runs_past_the_rows() {
        assert_eq!(finder_window(0, 0), 0..0);
        assert_eq!(finder_window(5, 4), 0..5);
        assert_eq!(finder_window(30, 0), 0..FINDER_ROWS);
        assert_eq!(finder_window(30, 7), 0..FINDER_ROWS);
        assert_eq!(finder_window(30, 8), 1..FINDER_ROWS + 1);
        assert_eq!(finder_window(30, 29), 22..30);
    }

    #[test]
    fn enter_takes_the_accent_choice_and_escape_dismisses() {
        let dialog = Dialog::new("Save?", "").choice("Don't Save", Tone::Danger, 1).cancel("Cancel").choice(
            "Save",
            Tone::Accent,
            2,
        );
        assert_eq!(dialog.key(&Key::Named(Named::Enter)), Some(Answer::Chose(2)));
        assert_eq!(dialog.key(&Key::Named(Named::Escape)), Some(Answer::Dismiss));
        assert_eq!(dialog.cancel_choice(), Some(1));
    }

    #[test]
    fn a_toast_fades_in_holds_and_leaves() {
        let toast = Toast::new("Saved").action("Undo", ());
        assert!(toast.action.is_some());
        assert!(toast.progress(toast.at) < 0.01);
        assert!((toast.progress(toast.at + Duration::from_secs(2)) - 1.0).abs() < 1e-6);
        assert!(toast.expired(toast.at + TOAST));
    }
}

use std::time::{Duration, Instant};

use glide_iced::Paint;
use glide_iced::style::{self, R3, SP1, SP2, SP3, SP4, SP5, SP6};
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
    let choice = |(index, choice): (usize, &'a Choice<M>)| {
        button(&paint, &choice.label, choice.tone, Some(Answer::Chose(index)))
    };
    let (left, right): (Vec<_>, Vec<_>) =
        dialog.choices.iter().enumerate().partition(|(_, choice)| choice.tone == Tone::Danger);
    let buttons = left
        .into_iter()
        .map(choice)
        .chain(std::iter::once(Space::new().width(Fill).into()))
        .chain(right.into_iter().map(choice))
        .fold(row![].spacing(SP2).align_y(Alignment::Center), iced::widget::Row::push);
    let mut content = column![text(&dialog.title, TextStyle::T3)].spacing(SP3);
    if !dialog.body.is_empty() {
        content = content.push(text(&dialog.body, TextStyle::CALL).color(paint.fg_2()));
    }
    let card = container(content.push(buttons))
        .padding(SP4)
        .width(dialog.width)
        .max_width((window.width - SP6).max(1.0))
        .style(move |_| style::popover(&paint));
    let scrim = Color { a: 0.32 * progress, ..Color::BLACK };
    let scale = component::DIALOG_CLOSED_SCALE + (1.0 - component::DIALOG_CLOSED_SCALE) * progress;
    stack![
        opaque(
            mouse_area(container(Space::new().width(Fill).height(Fill)).style(move |_| style::rounded(scrim, corner)))
                .on_press(Answer::Dismiss)
        ),
        container(widgets::scale(opaque(widgets::surface(card, &paint, Level::El3, R3)), scale)).center(Fill),
    ]
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
    let paint = paint.faded(progress);
    let mut content =
        row![text(&toast.text, TextStyle::CALL).wrapping(Wrapping::Word)].spacing(SP3).align_y(Alignment::Center);
    if let Some((label, _)) = &toast.action {
        content = content.push(crate::widget::compact_button(&paint, label, Tone::Tonal, Some(ToastAction)));
    }
    let padding = if toast.action.is_some() { Padding::from([SP1, SP1]).left(SP4) } else { Padding::from([SP2, SP4]) };
    let pill = container(content)
        .padding(padding)
        .max_width((window.width - 48.0).max(1.0))
        .style(move |_| style::popover(&paint));
    container(widgets::surface(pill, &paint, Level::El3, R3))
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::End)
        .padding(Padding::from([0.0, SP5]).bottom(SP5 + (1.0 - progress) * component::POPOVER_RISE))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

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

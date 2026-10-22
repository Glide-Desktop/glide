use iced::Element;

use super::Ctx;
use crate::icons::Icon;
use crate::theme::ThemeState;
use crate::widgets::{self, Row};

#[must_use]
pub fn error_banner<'a, M: Clone + 'a>(t: ThemeState, error: Option<&'a str>) -> Option<Element<'a, M>> {
    error.map(|error| widgets::banner(t, t.danger(), Icon::Warning, error, None))
}

pub fn gated<'a, M: 'a>(c: Ctx, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    widgets::inert(c.theme, c.writable, content)
}

pub fn toggle<'a, M: Clone + 'a>(c: Ctx, value: bool, on_toggle: impl Fn(bool) -> M + 'a) -> Element<'a, M> {
    gated(c, widgets::switch(c.theme, value, on_toggle))
}

pub fn switch_row<'a, M: Clone + 'a>(
    c: Ctx, title: &str, hint: &str, value: bool, on_toggle: impl Fn(bool) -> M + 'a,
) -> Element<'a, M> {
    Row::new(c.theme, title).subtitle(hint).suffix(toggle(c, value, on_toggle)).build()
}

pub fn choice<'a, M: Clone + 'a, T: Copy + PartialEq + 'a>(
    c: Ctx, options: impl IntoIterator<Item = (T, &'a str)>, selected: T, on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    let options = options.into_iter().map(|(value, label)| (value, label.to_string())).collect();
    gated(c, widgets::segmented(c.theme, options, selected, on_select))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub value: i64,
    label: String,
}

impl Choice {
    #[must_use]
    pub fn new(value: i64, label: &str) -> Self {
        Self { value, label: label.to_string() }
    }
}

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

pub fn presets(values: &[i64], current: i64, label: impl Fn(i64) -> String) -> (Vec<Choice>, Option<Choice>) {
    let options = values
        .iter()
        .copied()
        .chain((!values.contains(&current)).then_some(current))
        .map(|value| Choice { value, label: label(value) })
        .collect();
    let selected = Choice { value: current, label: label(current) };
    (options, Some(selected))
}

use std::borrow::Cow;

use glide_iced::Paint;
use glide_iced::style::{self, R2, SP3, SP4};
use glide_iced::widgets;
use glide_theme::elevation::{self, Level};
use glide_theme::shape::R1;
use glide_theme::{TextStyle, component};
use iced::border::Radius;
use iced::widget::{Row, button as control, container};
use iced::{Border, Color, Element, Fill, Length, Padding, Shadow, Vector};

use super::focus::activatable;
use super::popover::tooltip;
use super::{Glyph, Wrapping, icon, mix, text};
use crate::anim::{Speed, animate};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Tone {
    Accent,
    Tonal,
    #[default]
    Neutral,
    Outline,
    Ghost,
    Danger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
}

impl Size {
    const fn height(self) -> f32 {
        match self {
            Self::Small => component::SMALL_CONTROL,
            Self::Medium => component::CONTROL,
            Self::Large => component::LARGE_CONTROL,
        }
    }

    const fn padding(self) -> f32 {
        match self {
            Self::Small => SP3 - 1.0,
            Self::Medium => SP4,
            Self::Large => 22.0,
        }
    }

    const fn label(self) -> TextStyle {
        let size = match self {
            Self::Small => 12.5,
            Self::Medium => 13.5,
            Self::Large => 14.5,
        };
        TextStyle { size_px: size, weight: 500, ..TextStyle::MENU_ITEM }
    }

    #[must_use]
    pub fn width(self, label: &str) -> f32 {
        super::measure(label, self.label()) + 2.0 * self.padding()
    }
}

fn shadow(paint: &Paint, level: Level) -> Shadow {
    elevation::layers(level, paint.tokens.appearance).last().map_or_else(Shadow::default, |layer| Shadow {
        color: style::to_iced(layer.color).scale_alpha(paint.alpha),
        offset: Vector::new(layer.dx, layer.dy),
        blur_radius: layer.blur,
    })
}

#[must_use]
pub fn button_style(paint: &Paint, tone: Tone, status: control::Status) -> control::Style {
    let active = matches!(status, control::Status::Hovered | control::Status::Pressed);
    if status == control::Status::Disabled {
        return match tone {
            Tone::Ghost | Tone::Outline => style::flat(Color::TRANSPARENT, paint.fg_4(), R2),
            _ => style::flat(paint.fill_1(), paint.fg_4(), R2),
        };
    }
    match tone {
        Tone::Accent => {
            let background = if active { paint.acc_bg_hover() } else { paint.acc_bg() };
            let level = if active { Level::El2 } else { Level::El1 };
            control::Style { shadow: shadow(paint, level), ..style::flat(background, paint.acc_fg(), R2) }
        }
        Tone::Tonal => style::flat(paint.layered(paint.acc_soft(), status), paint.acc_soft_fg(), R2),
        Tone::Neutral => style::flat(paint.layered(paint.fill_1(), status), paint.fg_1(), R2),
        Tone::Outline => control::Style {
            border: Border {
                color: if active { paint.fill_3() } else { paint.line_2() },
                width: 1.0,
                radius: R2.into(),
            },
            ..style::flat(paint.layered(Color::TRANSPARENT, status), paint.fg_1(), R2)
        },
        Tone::Ghost => {
            style::flat(paint.layered(Color::TRANSPARENT, status), if active { paint.fg_1() } else { paint.fg_2() }, R2)
        }
        Tone::Danger if active => style::flat(paint.danger(), paint.view_bg(), R2),
        Tone::Danger => style::flat(paint.danger_soft(), paint.danger(), R2),
    }
}

#[must_use]
pub fn sized_button<'a, M: Clone + 'a>(
    paint: &Paint, label: impl Into<Cow<'a, str>>, tone: Tone, size: Size, message: Option<M>,
) -> Element<'a, M> {
    let paint = *paint;
    let control = control(container(text(label, size.label()).wrapping(Wrapping::None)).center_y(Fill))
        .on_press_maybe(message.clone())
        .height(size.height())
        .padding(Padding::from([0.0, size.padding()]));
    activatable(&paint, widgets::button(control, move |status| button_style(&paint, tone, status)), R2, message)
}

#[must_use]
pub fn button<'a, M: Clone + 'a>(
    paint: &Paint, label: impl Into<Cow<'a, str>>, tone: Tone, message: Option<M>,
) -> Element<'a, M> {
    sized_button(paint, label, tone, Size::Medium, message)
}

#[must_use]
pub fn compact_button<'a, M: Clone + 'a>(
    paint: &Paint, label: impl Into<Cow<'a, str>>, tone: Tone, message: Option<M>,
) -> Element<'a, M> {
    sized_button(paint, label, tone, Size::Small, message)
}

#[must_use]
pub fn link<'a, M: Clone + 'a>(paint: &Paint, label: impl Into<Cow<'a, str>>, message: Option<M>) -> Element<'a, M> {
    let paint = *paint;
    let enabled = message.is_some();
    let control = control(text(label, TextStyle { weight: 600, ..TextStyle::CALL }))
        .on_press_maybe(message.clone())
        .padding(Padding::from([style::SP1, style::SP2]));
    let control = widgets::button(control, move |status| {
        let mut style = button_style(&paint, Tone::Ghost, status);
        if enabled {
            style.text_color = paint.acc_text();
        }
        style
    });
    activatable(&paint, control, R2, message)
}

#[must_use]
pub fn primary<'a, M: Clone + 'a>(paint: &Paint, label: impl Into<Cow<'a, str>>, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Accent, Some(message))
}

#[must_use]
pub fn secondary<'a, M: Clone + 'a>(paint: &Paint, label: impl Into<Cow<'a, str>>, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Neutral, Some(message))
}

#[must_use]
pub fn tonal<'a, M: Clone + 'a>(paint: &Paint, label: impl Into<Cow<'a, str>>, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Tonal, Some(message))
}

#[must_use]
pub fn outline<'a, M: Clone + 'a>(paint: &Paint, label: impl Into<Cow<'a, str>>, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Outline, Some(message))
}

#[must_use]
pub fn ghost<'a, M: Clone + 'a>(paint: &Paint, label: impl Into<Cow<'a, str>>, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Ghost, Some(message))
}

#[must_use]
pub fn danger<'a, M: Clone + 'a>(paint: &Paint, label: impl Into<Cow<'a, str>>, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Danger, Some(message))
}

#[must_use]
pub fn icon_button<'a, M: Clone + 'a>(
    paint: &Paint, glyph: impl Glyph, label: &'a str, message: Option<M>, on: bool,
) -> Element<'a, M> {
    sized_icon_button(paint, glyph, label, message, on, Size::Small)
}

#[must_use]
pub fn sized_icon_button<'a, M: Clone + 'a>(
    paint: &Paint, glyph: impl Glyph, label: &'a str, message: Option<M>, on: bool, size: Size,
) -> Element<'a, M> {
    let extent = size.height();
    let paint = *paint;
    let tint = if message.is_none() {
        paint.fg_4()
    } else if on {
        paint.fg_1()
    } else {
        paint.fg_2()
    };
    let control = control(container(icon(glyph, 16.0, tint)).center(Fill))
        .on_press_maybe(message.clone())
        .width(extent)
        .height(extent)
        .padding(0);
    let control = widgets::button(control, move |status| {
        if on {
            style::flat(paint.layered(paint.fill_2(), status), paint.fg_1(), R2)
        } else {
            button_style(&paint, Tone::Ghost, status)
        }
    });
    tooltip(&paint, activatable(&paint, control, R2, message), label)
}

#[must_use]
pub fn toggle_button<'a, M: Clone + 'a>(
    paint: &Paint, label: impl Into<Cow<'a, str>>, on: bool, on_toggle: Option<impl Fn(bool) -> M + 'a>,
) -> Element<'a, M> {
    sized_toggle_button(paint, label, on, Size::Medium, on_toggle)
}

#[must_use]
pub fn sized_toggle_button<'a, M: Clone + 'a>(
    paint: &Paint, label: impl Into<Cow<'a, str>>, on: bool, size: Size, on_toggle: Option<impl Fn(bool) -> M + 'a>,
) -> Element<'a, M> {
    toggle(paint, label.into(), on, size, Tone::Neutral, on_toggle)
}

#[must_use]
pub fn flat_toggle_button<'a, M: Clone + 'a>(
    paint: &Paint, label: impl Into<Cow<'a, str>>, on: bool, size: Size, on_toggle: Option<impl Fn(bool) -> M + 'a>,
) -> Element<'a, M> {
    toggle(paint, label.into(), on, size, Tone::Ghost, on_toggle)
}

fn toggle<'a, M: Clone + 'a>(
    paint: &Paint, label: Cow<'a, str>, on: bool, size: Size, rest: Tone, on_toggle: Option<impl Fn(bool) -> M + 'a>,
) -> Element<'a, M> {
    let paint = *paint;
    let message = on_toggle.map(|toggle| toggle(!on));
    let square = label.chars().count() <= 2;
    animate(if on { 1.0 } else { 0.0 }, Speed::COLOR, move |value| {
        let value = value.clone();
        let control = control(container(text(label, size.label()).wrapping(Wrapping::None)).center(Fill))
            .on_press_maybe(message.clone())
            .width(if square { Length::Fixed(size.height()) } else { Length::Shrink })
            .height(size.height())
            .padding(Padding::from([0.0, if square { 0.0 } else { size.padding() }]));
        let control = widgets::button(control, move |status| {
            let amount = value.get().clamp(0.0, 1.0);
            let idle = button_style(&paint, rest, status);
            let lit = button_style(&paint, Tone::Tonal, status);
            let pick = |style: &control::Style| match style.background {
                Some(iced::Background::Color(color)) => color,
                _ => Color::TRANSPARENT,
            };
            control::Style {
                background: Some(iced::Background::Color(mix(pick(&idle), pick(&lit), amount))),
                text_color: mix(idle.text_color, lit.text_color, amount),
                ..lit
            }
        });
        activatable(&paint, control, R2, message)
    })
    .into()
}

#[must_use]
pub fn linked<'a, M: Clone + 'a>(paint: &Paint, items: &[(&'a str, Option<M>)]) -> Element<'a, M> {
    let paint = *paint;
    let last = items.len().saturating_sub(1);
    let buttons = items.iter().enumerate().map(|(index, (label, message))| {
        let left = if index == 0 { R2 } else { R1 };
        let right = if index == last { R2 } else { R1 };
        let radius = Radius::default().top_left(left).bottom_left(left).top_right(right).bottom_right(right);
        let control = control(container(text(*label, Size::Medium.label()).wrapping(Wrapping::None)).center_y(Fill))
            .on_press_maybe(message.clone())
            .height(component::CONTROL)
            .padding(Padding::from([0.0, SP4]));
        let control = widgets::button(control, move |status| {
            let mut style = button_style(&paint, Tone::Neutral, status);
            style.border.radius = radius;
            style
        });
        activatable(&paint, control, R1, message.clone())
    });
    Row::with_children(buttons).spacing(1).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glide_theme::{Accent, Appearance};
    use iced::Background;

    #[test]
    fn disabled_buttons_read_as_disabled_in_every_tone() {
        let paint = Paint::new(Appearance::Dark, Accent::Blue);
        for tone in [Tone::Accent, Tone::Tonal, Tone::Neutral, Tone::Outline, Tone::Ghost, Tone::Danger] {
            assert_eq!(button_style(&paint, tone, control::Status::Disabled).text_color, paint.fg_4());
        }
    }

    #[test]
    fn the_primary_button_lifts_and_danger_fills_on_hover() {
        let paint = Paint::new(Appearance::Light, Accent::Teal);
        let active = button_style(&paint, Tone::Accent, control::Status::Active);
        let hovered = button_style(&paint, Tone::Accent, control::Status::Hovered);
        assert_eq!(active.background, Some(Background::Color(paint.acc_bg())));
        assert_eq!(hovered.background, Some(Background::Color(paint.acc_bg_hover())));
        assert!(hovered.shadow.blur_radius > active.shadow.blur_radius, "El1 rises to El2");
        let danger = button_style(&paint, Tone::Danger, control::Status::Hovered);
        assert_eq!(danger.background, Some(Background::Color(paint.danger())));
        assert_eq!(danger.text_color, paint.view_bg());
    }

    #[test]
    fn sizes_follow_the_spec_table() {
        assert_eq!((Size::Small.height(), Size::Medium.height(), Size::Large.height()), (28.0, 34.0, 42.0));
        assert_eq!((Size::Small.padding(), Size::Medium.padding(), Size::Large.padding()), (11.0, 16.0, 22.0));
        assert_eq!(Size::Small.label().size_px, 12.5);
    }
}

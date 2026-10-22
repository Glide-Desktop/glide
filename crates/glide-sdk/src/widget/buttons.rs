use glide_iced::Paint;
use glide_iced::style::{self, R2, SP3, SP4};
use glide_iced::widgets;
use glide_theme::elevation::{self, Level};
use glide_theme::shape::R1;
use glide_theme::{TextStyle, component};
use iced::border::Radius;
use iced::widget::{Row, button as control, container};
use iced::{Border, Color, Element, Fill, Padding, Shadow, Vector};

use super::popover::tooltip;
use super::{Icon, Wrapping, icon, text};

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
    paint: &Paint, label: &'a str, tone: Tone, size: Size, message: Option<M>,
) -> Element<'a, M> {
    let paint = *paint;
    let control = control(container(text(label, size.label()).wrapping(Wrapping::None)).center_y(Fill))
        .on_press_maybe(message)
        .height(size.height())
        .padding(Padding::from([0.0, size.padding()]));
    widgets::button(control, move |status| button_style(&paint, tone, status))
}

#[must_use]
pub fn button<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, tone: Tone, message: Option<M>) -> Element<'a, M> {
    sized_button(paint, label, tone, Size::Medium, message)
}

#[must_use]
pub fn compact_button<'a, M: Clone + 'a>(
    paint: &Paint, label: &'a str, tone: Tone, message: Option<M>,
) -> Element<'a, M> {
    sized_button(paint, label, tone, Size::Small, message)
}

#[must_use]
pub fn primary<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Accent, Some(message))
}

#[must_use]
pub fn secondary<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Neutral, Some(message))
}

#[must_use]
pub fn tonal<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Tonal, Some(message))
}

#[must_use]
pub fn outline<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Outline, Some(message))
}

#[must_use]
pub fn ghost<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Ghost, Some(message))
}

#[must_use]
pub fn danger<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, message: M) -> Element<'a, M> {
    button(paint, label, Tone::Danger, Some(message))
}

#[must_use]
pub fn icon_button<'a, M: Clone + 'a>(
    paint: &Paint, glyph: Icon, label: &'a str, message: Option<M>, on: bool,
) -> Element<'a, M> {
    let paint = *paint;
    let tint = if message.is_none() {
        paint.fg_4()
    } else if on {
        paint.fg_1()
    } else {
        paint.fg_2()
    };
    let control = control(container(icon(glyph, 16.0, tint)).center(Fill))
        .on_press_maybe(message)
        .width(component::SMALL_CONTROL)
        .height(component::SMALL_CONTROL)
        .padding(0);
    tooltip(
        &paint,
        widgets::button(control, move |status| {
            if on {
                style::flat(paint.layered(paint.fill_2(), status), paint.fg_1(), R2)
            } else {
                button_style(&paint, Tone::Ghost, status)
            }
        }),
        label,
    )
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
        widgets::button(control, move |status| {
            let mut style = button_style(&paint, Tone::Neutral, status);
            style.border.radius = radius;
            style
        })
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

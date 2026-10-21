use glide_theme::motion::{ST_HOVER, ST_PRESS};
use glide_theme::{Accent, Appearance, Tokens};
use iced::border::Radius;
use iced::widget::{button, container, svg, text_input};
use iced::{Background, Border, Color, Shadow};

pub use glide_theme::shape::{R2, R3};
pub const SP1: f32 = glide_theme::spacing::SP1 as f32;
pub const SP2: f32 = glide_theme::spacing::SP2 as f32;
pub const SP3: f32 = glide_theme::spacing::SP3 as f32;
pub const SP4: f32 = glide_theme::spacing::SP4 as f32;
pub const SP5: f32 = glide_theme::spacing::SP5 as f32;
pub const SP6: f32 = glide_theme::spacing::SP6 as f32;

pub const SEMIBOLD: iced::Font = iced::Font { weight: iced::font::Weight::Semibold, ..iced::Font::DEFAULT };

pub use crate::font::use_ui_family;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    pub tokens: Tokens,
    pub alpha: f32,
}

macro_rules! colors {
    ($($name:ident),* $(,)?) => {
        impl Paint {
            $(
                #[must_use]
                pub fn $name(&self) -> Color {
                    to_iced(self.tokens.$name).scale_alpha(self.alpha)
                }
            )*
        }
    };
}

colors!(
    win_bg,
    bar_bg,
    bar_bg_idle,
    popover_bg,
    fg_1,
    fg_2,
    fg_3,
    fg_4,
    line,
    line_2,
    fill_1,
    fill_2,
    fill_3,
    acc_bg,
    acc_bg_hover,
    acc_fg,
    acc_soft,
    danger,
    danger_soft,
    warn,
);

#[must_use]
pub fn over(top: Color, base: Color) -> Color {
    to_iced(
        glide_theme::Color([base.r, base.g, base.b, base.a])
            .state_layer(glide_theme::Color([top.r, top.g, top.b, top.a]), 1.0),
    )
}

#[must_use]
pub fn to_iced(color: glide_theme::Color) -> Color {
    let [r, g, b, a] = color.0;
    Color { r, g, b, a }
}

impl Paint {
    #[must_use]
    pub fn new(appearance: Appearance, accent: Accent) -> Self {
        Self { tokens: Tokens::new(appearance, accent), alpha: 1.0 }
    }

    #[must_use]
    pub fn hairline(&self) -> Color {
        over(self.line(), self.bar_bg())
    }

    #[must_use]
    pub fn divider(&self) -> Color {
        over(self.line_2(), self.bar_bg())
    }

    #[must_use]
    pub fn faded(self, alpha: f32) -> Self {
        Self { alpha: self.alpha * alpha.clamp(0.0, 1.0), ..self }
    }

    #[must_use]
    pub fn layer(&self, base: Color, opacity: f32) -> Color {
        if opacity <= 0.0 {
            return base;
        }
        let state = self.tokens.state.with_alpha(self.tokens.state.0[3] * self.alpha);
        to_iced(glide_theme::Color([base.r, base.g, base.b, base.a]).state_layer(state, opacity))
    }

    #[must_use]
    pub fn layered(&self, base: Color, status: button::Status) -> Color {
        match status {
            button::Status::Hovered => self.layer(base, ST_HOVER),
            button::Status::Pressed => self.layer(base, ST_PRESS),
            button::Status::Active | button::Status::Disabled => base,
        }
    }

    #[must_use]
    pub fn iced_theme(&self) -> iced::Theme {
        iced::Theme::custom(
            "Meridian",
            iced::theme::Palette {
                background: self.win_bg(),
                text: self.fg_1(),
                primary: self.acc_bg(),
                success: to_iced(self.tokens.ok),
                warning: self.warn(),
                danger: self.danger(),
            },
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Ghost,
    Danger,
}

#[must_use]
pub fn flat(background: Color, text_color: Color, radius: impl Into<Radius>) -> button::Style {
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border { radius: radius.into(), ..Border::default() },
        shadow: Shadow::default(),
        snap: true,
    }
}

#[must_use]
pub fn action(paint: &Paint, tone: Tone, status: button::Status) -> button::Style {
    let active = matches!(status, button::Status::Hovered | button::Status::Pressed);
    match tone {
        _ if status == button::Status::Disabled => flat(Color::TRANSPARENT, paint.fg_4(), R2),
        Tone::Neutral => flat(paint.layered(paint.fill_1(), status), paint.fg_1(), R2),
        Tone::Ghost => {
            flat(paint.layered(Color::TRANSPARENT, status), if active { paint.fg_1() } else { paint.fg_2() }, R2)
        }
        Tone::Danger if active => flat(paint.danger(), Color::WHITE, R2),
        Tone::Danger => flat(paint.danger_soft(), paint.danger(), R2),
    }
}

#[must_use]
pub fn toggle(paint: &Paint, on: bool, status: button::Status) -> button::Style {
    if on {
        flat(paint.layered(paint.acc_soft(), status), paint.fg_1(), R2)
    } else {
        action(paint, Tone::Ghost, status)
    }
}

#[must_use]
pub fn menu_item(paint: &Paint, status: button::Status) -> button::Style {
    match status {
        button::Status::Disabled => flat(Color::TRANSPARENT, paint.fg_4(), R2),
        button::Status::Hovered | button::Status::Pressed => flat(paint.fill_2(), paint.fg_1(), R2),
        button::Status::Active => flat(Color::TRANSPARENT, paint.fg_1(), R2),
    }
}

#[must_use]
pub fn window_control(paint: &Paint, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => flat(paint.danger(), Color::WHITE, R2),
        _ => flat(paint.fill_1(), paint.fg_2(), R2),
    }
}

#[must_use]
pub fn maximize_control(paint: &Paint, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => flat(paint.acc_bg(), paint.acc_fg(), R2),
        _ => flat(paint.fill_1(), paint.fg_2(), R2),
    }
}

#[must_use]
pub fn rounded(background: Color, radius: impl Into<Radius>) -> container::Style {
    container::Style {
        background: Some(Background::Color(background)),
        border: Border { radius: radius.into(), ..Border::default() },
        ..container::Style::default()
    }
}

#[must_use]
pub fn popover(paint: &Paint) -> container::Style {
    container::Style {
        text_color: Some(paint.fg_1()),
        background: Some(Background::Color(paint.popover_bg())),
        border: Border { color: paint.line(), width: 1.0, radius: Radius::from(R3) },
        shadow: Shadow::default(),
        snap: true,
    }
}

#[must_use]
pub fn frame(paint: &Paint) -> container::Style {
    container::Style {
        border: Border { color: paint.line_2(), width: 1.0, radius: Radius::from(R3) },
        ..container::Style::default()
    }
}

#[must_use]
pub fn field(paint: &Paint, status: text_input::Status) -> text_input::Style {
    let border = match status {
        text_input::Status::Focused { .. } => paint.acc_bg(),
        text_input::Status::Hovered => paint.fill_3(),
        text_input::Status::Active | text_input::Status::Disabled => Color::TRANSPARENT,
    };
    text_input::Style {
        background: Background::Color(paint.fill_1()),
        border: Border { color: border, width: 1.0, radius: Radius::from(R2) },
        icon: paint.fg_3(),
        placeholder: paint.fg_3(),
        value: paint.fg_1(),
        selection: paint.acc_soft(),
    }
}

#[must_use]
pub fn glyph(color: Color) -> svg::Style {
    svg::Style { color: Some(color) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hairlines_are_opaque_so_the_desktop_never_shows_through_the_chrome() {
        for appearance in [Appearance::Dark, Appearance::Light] {
            let paint = Paint::new(appearance, Accent::Blue);
            assert!(paint.line().a < 1.0, "the token itself is translucent");
            for line in [paint.hairline(), paint.divider()] {
                assert!((line.a - 1.0).abs() < f32::EPSILON);
                assert_ne!(line, paint.bar_bg());
            }
        }
    }
}

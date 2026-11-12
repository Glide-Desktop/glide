use glide_theme::motion::{ST_HOVER, ST_PRESS};
use glide_theme::{Accent, Appearance, Tokens};
use iced::border::Radius;
use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Shadow};

pub use glide_theme::shape::{R_ROUND, R2, R3};
pub const SP1: f32 = glide_theme::spacing::SP1 as f32;
pub const SP2: f32 = glide_theme::spacing::SP2 as f32;
pub const SP3: f32 = glide_theme::spacing::SP3 as f32;
pub const SP4: f32 = glide_theme::spacing::SP4 as f32;
pub const SP5: f32 = glide_theme::spacing::SP5 as f32;
pub const SP6: f32 = glide_theme::spacing::SP6 as f32;

pub const SEMIBOLD: iced::Font = iced::Font { weight: iced::font::Weight::Semibold, ..iced::Font::DEFAULT };

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    pub tokens: Tokens,
    pub alpha: f32,
}

impl From<&Paint> for Paint {
    fn from(paint: &Paint) -> Self {
        *paint
    }
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
    view_bg,
    sidebar_bg,
    card_bg,
    sunken_bg,
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
    acc_text,
    acc_soft,
    acc_soft_fg,
    acc_ring,
    ok,
    ok_soft,
    danger,
    danger_soft,
    warn,
    warn_soft,
);

#[must_use]
pub fn over(top: Color, base: Color) -> Color {
    to_iced(from_iced(base).state_layer(from_iced(top), 1.0))
}

#[must_use]
pub fn to_iced(color: glide_theme::Color) -> Color {
    let [r, g, b, a] = color.0;
    Color { r, g, b, a }
}

#[must_use]
pub fn from_iced(color: Color) -> glide_theme::Color {
    glide_theme::Color([color.r, color.g, color.b, color.a])
}

impl Default for Paint {
    fn default() -> Self {
        Self::new(Appearance::Dark, Accent::Blue)
    }
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
        to_iced(from_iced(base).state_layer(state, opacity))
    }

    #[must_use]
    pub fn hover(&self, base: Color, progress: f32) -> Color {
        self.layer(base, ST_HOVER * progress.clamp(0.0, 1.0))
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
                success: self.ok(),
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
        Tone::Neutral if status == button::Status::Disabled => flat(paint.fill_1(), paint.fg_4(), R2),
        _ if status == button::Status::Disabled => flat(Color::TRANSPARENT, paint.fg_4(), R2),
        Tone::Neutral => flat(paint.layered(paint.fill_1(), status), paint.fg_1(), R2),
        Tone::Ghost => {
            flat(paint.layered(Color::TRANSPARENT, status), if active { paint.fg_1() } else { paint.fg_2() }, R2)
        }
    }
}

#[must_use]
pub fn menu_item(paint: &Paint, status: button::Status) -> button::Style {
    match status {
        button::Status::Disabled => flat(Color::TRANSPARENT, paint.fg_3(), R2),
        _ => flat(paint.layered(Color::TRANSPARENT, status), paint.fg_1(), R2),
    }
}

#[must_use]
pub fn window_control(paint: &Paint, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => flat(paint.danger(), Color::WHITE, R_ROUND),
        button::Status::Pressed => flat(paint.layer(paint.danger(), ST_PRESS), Color::WHITE, R_ROUND),
        _ => flat(paint.fill_1(), paint.fg_2(), R_ROUND),
    }
}

#[must_use]
pub fn maximize_control(paint: &Paint, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => flat(paint.acc_bg(), paint.acc_fg(), R_ROUND),
        button::Status::Pressed => flat(paint.layer(paint.acc_bg(), ST_PRESS), paint.acc_fg(), R_ROUND),
        _ => flat(paint.fill_1(), paint.fg_2(), R_ROUND),
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

    #[test]
    fn window_controls_darken_further_when_pressed_and_disabled_neutral_buttons_keep_their_fill() {
        let paint = Paint::default();
        for control in [window_control, maximize_control] {
            assert_ne!(control(&paint, button::Status::Hovered), control(&paint, button::Status::Pressed));
        }
        let disabled = action(&paint, Tone::Neutral, button::Status::Disabled);
        assert_eq!(disabled.background, Some(Background::Color(paint.fill_1())));
        assert_eq!(disabled.text_color, paint.fg_4());
        let ghost = action(&paint, Tone::Ghost, button::Status::Disabled);
        assert_eq!(ghost.background, Some(Background::Color(Color::TRANSPARENT)));
    }

    #[test]
    fn menu_items_use_distinct_hover_and_press_state_layers() {
        for appearance in [Appearance::Dark, Appearance::Light] {
            let paint = Paint::new(appearance, Accent::Blue);
            for (status, opacity) in [(button::Status::Hovered, ST_HOVER), (button::Status::Pressed, ST_PRESS)] {
                let Some(Background::Color(background)) = menu_item(&paint, status).background else {
                    panic!("menu state must have a solid layer");
                };
                assert!((background.a - opacity).abs() < 1e-6);
            }
        }
    }
}

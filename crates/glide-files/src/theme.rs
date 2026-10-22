use glide_theme::elevation::{self, Level};
use glide_theme::motion::{ST_HOVER, ST_PRESS};
use glide_theme::{Accent, Appearance, TextStyle, Tokens};
use iced::border::Radius;
use iced::widget::{button, container, rule, svg};
use iced::{Background, Border, Color, Shadow};

pub use glide_theme::shape::{R_ROUND, R1, R2, R3};

pub const SEMIBOLD: iced::Font = iced::Font { weight: iced::font::Weight::Semibold, ..iced::Font::DEFAULT };

pub use glide_iced::font::use_ui_family;

pub const MICRO: f32 = TextStyle::MICRO.size_px;
pub const CAP: f32 = TextStyle::CAP.size_px;
pub const CALL: f32 = TextStyle::CALL.size_px;
pub const TITLE: f32 = TextStyle::WIN_TITLE.size_px;
pub const T3: f32 = TextStyle::T3.size_px;

pub const CONTROL: f32 = glide_theme::component::CONTROL;
pub const RING: f32 = glide_theme::component::FIELD_HALO;
pub const SMALL_CONTROL: f32 = glide_theme::component::SMALL_CONTROL;

pub const SP1: f32 = glide_theme::spacing::SP1 as f32;
pub const SP2: f32 = glide_theme::spacing::SP2 as f32;
pub const SP3: f32 = glide_theme::spacing::SP3 as f32;
pub const SP4: f32 = glide_theme::spacing::SP4 as f32;
pub const SP5: f32 = glide_theme::spacing::SP5 as f32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeState {
    pub tokens: Tokens,
    pub alpha: f32,
}

macro_rules! colors {
    ($($name:ident),* $(,)?) => {
        impl ThemeState {
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
    danger,
    danger_soft,
);

impl Default for ThemeState {
    fn default() -> Self {
        Self::new(Appearance::Dark, Accent::Blue)
    }
}

impl ThemeState {
    #[must_use]
    pub fn new(appearance: Appearance, accent: Accent) -> Self {
        Self { tokens: Tokens::new(appearance, accent), alpha: 1.0 }
    }

    #[must_use]
    pub fn faded(&self, alpha: f32) -> Self {
        Self { alpha: self.alpha * alpha.clamp(0.0, 1.0), ..*self }
    }

    #[must_use]
    pub fn hover(&self, base: Color, progress: f32) -> Color {
        self.layer(base, ST_HOVER * progress.clamp(0.0, 1.0))
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
    pub fn iced_theme(&self) -> iced::Theme {
        iced::Theme::custom(
            "Meridian",
            iced::theme::Palette {
                background: self.win_bg(),
                text: self.fg_1(),
                primary: self.acc_bg(),
                success: to_iced(self.tokens.ok),
                warning: to_iced(self.tokens.warn),
                danger: self.danger(),
            },
        )
    }

    #[must_use]
    pub fn layered(&self, base: Color, status: button::Status) -> Color {
        let opacity = match status {
            button::Status::Hovered => ST_HOVER,
            button::Status::Pressed => ST_PRESS,
            button::Status::Active | button::Status::Disabled => return base,
        };
        self.layer(base, opacity)
    }

    #[must_use]
    pub fn shadow(&self, level: Level) -> Shadow {
        elevation::layers(level, self.tokens.appearance).into_iter().rev().find(|layer| layer.blur > 0.0).map_or_else(
            Shadow::default,
            |layer| Shadow {
                color: to_iced(layer.color).scale_alpha(self.alpha),
                offset: iced::Vector::new(layer.dx, layer.dy),
                blur_radius: layer.blur,
            },
        )
    }
}

#[must_use]
pub fn to_iced(color: glide_theme::Color) -> Color {
    let [r, g, b, a] = color.0;
    Color { r, g, b, a }
}

fn from_iced(color: Color) -> glide_theme::Color {
    glide_theme::Color([color.r, color.g, color.b, color.a])
}

fn flat(background: Color, text_color: Color, radius: f32) -> button::Style {
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border { radius: Radius::from(radius), ..Border::default() },
        shadow: Shadow::default(),
        snap: true,
    }
}

#[must_use]
pub fn plain(text: Color) -> button::Style {
    flat(Color::TRANSPARENT, text, R2)
}

fn interactive(theme: &ThemeState, base: Color, hover: f32, status: button::Status) -> Color {
    let hovered = theme.hover(base, hover);
    if status == button::Status::Pressed { theme.layer(hovered, ST_PRESS - ST_HOVER) } else { hovered }
}

#[must_use]
pub fn list_row(theme: &ThemeState, selected: bool, stripe: bool, hover: f32, status: button::Status) -> button::Style {
    let (base, text) = match (selected, stripe) {
        (true, _) => (theme.acc_soft(), theme.acc_soft_fg()),
        (false, true) => (theme.fill_1(), theme.fg_1()),
        (false, false) => (Color::TRANSPARENT, theme.fg_1()),
    };
    flat(interactive(theme, base, hover, status), text, R2)
}

#[must_use]
pub fn tile_plate(theme: &ThemeState, selected: bool, hover: f32) -> container::Style {
    let base = if selected { theme.acc_soft() } else { Color::TRANSPARENT };
    rounded(theme.hover(base, hover), R2)
}

#[must_use]
pub fn surface(background: Color) -> container::Style {
    container::Style { background: Some(Background::Color(background)), ..container::Style::default() }
}

#[must_use]
pub fn rounded(background: Color, radius: impl Into<Radius>) -> container::Style {
    container::Style {
        background: Some(Background::Color(background)),
        border: Border { radius: radius.into(), ..Border::default() },
        ..container::Style::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    TopRight,
    BottomRight,
    Top,
    BottomLeft,
}

#[must_use]
pub fn window_corners(side: Side, corner: f32) -> Radius {
    let none = Radius::default();
    match side {
        Side::Left => none.top_left(corner).bottom_left(corner),
        Side::Right => none.top_right(corner).bottom_right(corner),
        Side::TopRight => none.top_right(corner),
        Side::BottomRight => none.bottom_right(corner),
        Side::Top => none.top_left(corner).top_right(corner),
        Side::BottomLeft => none.bottom_left(corner),
    }
}

#[must_use]
pub fn line(theme: &ThemeState) -> rule::Style {
    rule::Style { color: theme.line(), radius: Radius::default(), fill_mode: rule::FillMode::Full, snap: true }
}

#[must_use]
pub fn glyph(color: Color) -> svg::Style {
    svg::Style { color: Some(color) }
}

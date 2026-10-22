use glide_theme::elevation::{self, Level};
use glide_theme::motion::{ST_HOVER, ST_PRESS};
use glide_theme::{Accent, Appearance, TextStyle, Tokens};
use iced::border::Radius;
use iced::widget::{button, container, rule, scrollable, svg, text_input};
use iced::{Background, Border, Color, Shadow};

pub use glide_theme::shape::{R_ROUND, R1, R2, R3};

pub const SEMIBOLD: iced::Font = iced::Font { weight: iced::font::Weight::Semibold, ..iced::Font::DEFAULT };

pub use glide_iced::font::use_ui_family;

pub const MICRO: f32 = TextStyle::MICRO.size_px;
pub const CAP: f32 = TextStyle::CAP.size_px;
pub const CALL: f32 = TextStyle::CALL.size_px;
pub const MENU_ITEM: f32 = TextStyle::MENU_ITEM.size_px;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Primary,
    Neutral,
    Ghost,
    Open,
    Danger,
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
pub fn action(theme: &ThemeState, tone: Tone, status: button::Status) -> button::Style {
    let active = matches!(status, button::Status::Hovered | button::Status::Pressed);
    if status == button::Status::Disabled {
        let background = match tone {
            Tone::Ghost => Color::TRANSPARENT,
            _ => theme.fill_1(),
        };
        return flat(background, theme.fg_4(), R2);
    }
    match tone {
        Tone::Primary => {
            let (background, level) =
                if active { (theme.acc_bg_hover(), Level::El2) } else { (theme.acc_bg(), Level::El1) };
            button::Style { shadow: theme.shadow(level), ..flat(background, theme.acc_fg(), R2) }
        }
        Tone::Neutral => flat(theme.layered(theme.fill_1(), status), theme.fg_1(), R2),
        Tone::Open => flat(theme.layered(theme.fill_2(), status), theme.fg_1(), R2),
        Tone::Ghost => {
            let text = if active { theme.fg_1() } else { theme.fg_2() };
            flat(theme.layered(Color::TRANSPARENT, status), text, R2)
        }
        Tone::Danger if active => flat(theme.danger(), theme.view_bg(), R2),
        Tone::Danger => flat(theme.danger_soft(), theme.danger(), R2),
    }
}

#[must_use]
pub fn segment(theme: &ThemeState, active: bool, status: button::Status) -> button::Style {
    if active {
        flat(Color::TRANSPARENT, theme.fg_1(), R1)
    } else {
        let text = match status {
            button::Status::Hovered | button::Status::Pressed => theme.fg_1(),
            _ => theme.fg_2(),
        };
        flat(theme.layered(Color::TRANSPARENT, status), text, R1)
    }
}

#[must_use]
pub fn segment_thumb(theme: &ThemeState) -> container::Style {
    match theme.tokens.appearance {
        Appearance::Light => container::Style { shadow: theme.shadow(Level::El1), ..rounded(theme.view_bg(), R1) },
        Appearance::Dark => rounded(theme.fill_3(), R1),
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
pub fn sidebar_row(theme: &ThemeState, selected: bool, hover: f32, status: button::Status) -> button::Style {
    let (base, text) = match (selected, hover > 0.5 || status == button::Status::Pressed) {
        (true, _) => (theme.acc_soft(), theme.acc_soft_fg()),
        (false, true) => (Color::TRANSPARENT, theme.fg_1()),
        (false, false) => (Color::TRANSPARENT, theme.fg_2()),
    };
    flat(interactive(theme, base, hover, status), text, R2)
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
pub fn crumb(theme: &ThemeState, current: bool, hover: f32, status: button::Status) -> button::Style {
    let text = if current || hover > 0.5 { theme.fg_1() } else { theme.fg_2() };
    flat(interactive(theme, Color::TRANSPARENT, hover, status), text, R1)
}

#[must_use]
pub fn highlighted(status: button::Status) -> bool {
    matches!(status, button::Status::Hovered | button::Status::Pressed)
}

#[must_use]
pub fn menu_item(theme: &ThemeState, danger: bool, status: button::Status) -> button::Style {
    let text = if danger { theme.danger() } else { theme.fg_1() };
    match status {
        button::Status::Disabled => plain(theme.fg_4()),
        button::Status::Pressed => flat(theme.layer(theme.fill_2(), ST_PRESS - ST_HOVER), text, R2),
        button::Status::Hovered => flat(theme.fill_2(), text, R2),
        button::Status::Active => plain(text),
    }
}

#[must_use]
pub fn window_control(theme: &ThemeState, danger: bool, status: button::Status) -> button::Style {
    match (highlighted(status), danger) {
        (true, true) => flat(theme.danger(), Color::WHITE, R2),
        (true, false) => flat(theme.acc_bg(), theme.acc_fg(), R2),
        (false, _) => flat(theme.fill_1(), theme.fg_2(), R2),
    }
}

#[must_use]
pub fn search_field(theme: &ThemeState, status: text_input::Status) -> text_input::Style {
    let (background, border) = match status {
        text_input::Status::Focused { .. } => (theme.view_bg(), theme.acc_bg()),
        text_input::Status::Hovered => (theme.fill_2(), Color::TRANSPARENT),
        text_input::Status::Active | text_input::Status::Disabled => (theme.fill_1(), Color::TRANSPARENT),
    };
    text_input::Style {
        background: Background::Color(background),
        border: Border { color: border, width: 1.0, radius: Radius::from(R2) },
        ..field(theme, status)
    }
}

#[must_use]
pub fn clear_button(theme: &ThemeState, status: button::Status) -> button::Style {
    let background = if highlighted(status) { theme.fill_2() } else { Color::TRANSPARENT };
    flat(background, theme.fg_3(), R1)
}

#[must_use]
pub fn tooltip(theme: &ThemeState) -> container::Style {
    let base = popover(theme);
    container::Style {
        border: Border { radius: Radius::from(R2), ..base.border },
        shadow: theme.shadow(Level::El2),
        ..base
    }
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
pub fn window_frame(theme: &ThemeState, corner: f32) -> container::Style {
    container::Style {
        border: Border { color: theme.line(), width: 1.0, radius: Radius::from(corner) },
        ..container::Style::default()
    }
}

#[must_use]
pub fn popover(theme: &ThemeState) -> container::Style {
    container::Style {
        text_color: Some(theme.fg_1()),
        background: Some(Background::Color(theme.popover_bg())),
        border: Border { color: theme.line(), width: 1.0, radius: Radius::from(R3) },
        shadow: theme.shadow(Level::El3),
        snap: true,
    }
}

#[must_use]
pub fn scrim(progress: f32) -> container::Style {
    surface(Color::from_rgba8(16, 15, 14, 0.4 * progress.clamp(0.0, 1.0)))
}

#[must_use]
pub fn ring(color: Color) -> container::Style {
    container::Style {
        border: Border { color, width: RING, radius: Radius::from(R2 + RING) },
        ..container::Style::default()
    }
}

#[must_use]
pub fn field(theme: &ThemeState, status: text_input::Status) -> text_input::Style {
    let border = match status {
        text_input::Status::Focused { .. } => theme.acc_bg(),
        text_input::Status::Hovered => theme.fill_3(),
        text_input::Status::Active | text_input::Status::Disabled => theme.line_2(),
    };
    text_input::Style {
        background: Background::Color(theme.view_bg()),
        border: Border { color: border, width: 1.0, radius: Radius::from(R2) },
        icon: theme.fg_3(),
        placeholder: theme.fg_3(),
        value: theme.fg_1(),
        selection: theme.acc_soft(),
    }
}

#[must_use]
pub fn bare_field(theme: &ThemeState, status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        ..field(theme, status)
    }
}

#[must_use]
pub fn scroller(theme: &ThemeState, status: scrollable::Status) -> scrollable::Style {
    let active = matches!(
        status,
        scrollable::Status::Hovered { is_vertical_scrollbar_hovered: true, .. } | scrollable::Status::Dragged { .. }
    );
    let rail = scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller {
            background: Background::Color(if active { theme.fg_3() } else { theme.fill_3() }),
            border: Border { radius: Radius::from(glide_theme::shape::R_ROUND), ..Border::default() },
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: Background::Color(theme.popover_bg()),
            border: Border { color: theme.line(), width: 1.0, radius: Radius::from(glide_theme::shape::R_ROUND) },
            shadow: Shadow::default(),
            icon: theme.fg_2(),
        },
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

#[must_use]
pub fn flatten(top: Color, bottom: Color) -> Color {
    let mix = |over: f32, under: f32| under + (over - under) * top.a;
    Color { r: mix(top.r, bottom.r), g: mix(top.g, bottom.g), b: mix(top.b, bottom.b), a: bottom.a }
}

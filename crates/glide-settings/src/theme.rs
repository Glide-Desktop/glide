use iced::widget::{button, container};
use serde::{Deserialize, Serialize};

pub const R1: f32 = glide_theme::shape::R1;
pub const R2: f32 = glide_theme::shape::R2;
pub const R3: f32 = glide_theme::shape::R3;
pub const R4: f32 = glide_theme::shape::R4;
pub const R_ROUND: f32 = glide_theme::shape::R_ROUND;

pub const SCALE: [f32; 5] = [R1, R2, R3, R4, R_ROUND];

pub const ST_HOVER: f32 = glide_theme::motion::ST_HOVER;
pub const ST_PRESS: f32 = glide_theme::motion::ST_PRESS;
pub const ST_DRAG: f32 = glide_theme::motion::ST_DRAG;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Appearance {
    Light,
    #[default]
    Dark,
}

impl From<Appearance> for glide_theme::Appearance {
    fn from(appearance: Appearance) -> Self {
        match appearance {
            Appearance::Light => Self::Light,
            Appearance::Dark => Self::Dark,
        }
    }
}

impl From<glide_theme::Appearance> for Appearance {
    fn from(appearance: glide_theme::Appearance) -> Self {
        match appearance {
            glide_theme::Appearance::Light => Self::Light,
            glide_theme::Appearance::Dark => Self::Dark,
        }
    }
}

impl PartialEq<glide_theme::Appearance> for Appearance {
    fn eq(&self, other: &glide_theme::Appearance) -> bool {
        glide_theme::Appearance::from(*self) == *other
    }
}

impl PartialEq<Appearance> for glide_theme::Appearance {
    fn eq(&self, other: &Appearance) -> bool {
        *self == glide_theme::Appearance::from(*other)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Accent {
    #[default]
    Azurit,
    Malachite,
    Conifer,
    Amber,
    Terracotta,
    Cinnabar,
    Amethyst,
    Graphite,
}

impl Accent {
    pub const ALL: &'static [Self] = &[
        Self::Azurit,
        Self::Malachite,
        Self::Conifer,
        Self::Amber,
        Self::Terracotta,
        Self::Cinnabar,
        Self::Amethyst,
        Self::Graphite,
    ];

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Azurit => "Azurit",
            Self::Malachite => "Malachite",
            Self::Conifer => "Conifer",
            Self::Amber => "Amber",
            Self::Terracotta => "Terracotta",
            Self::Cinnabar => "Cinnabar",
            Self::Amethyst => "Amethyst",
            Self::Graphite => "Graphite",
        }
    }

    #[must_use]
    pub fn name_uk(self) -> &'static str {
        match self {
            Self::Azurit => "Азурит",
            Self::Malachite => "Малахіт",
            Self::Conifer => "Хвоя",
            Self::Amber => "Бурштин",
            Self::Terracotta => "Теракота",
            Self::Cinnabar => "Цинобра",
            Self::Amethyst => "Аметист",
            Self::Graphite => "Графіт",
        }
    }

    #[must_use]
    pub fn primary_hex(self) -> &'static str {
        match self {
            Self::Azurit => "#2f6fd0",
            Self::Malachite => "#128b7e",
            Self::Conifer => "#3f9639",
            Self::Amber => "#b57e10",
            Self::Terracotta => "#cf6524",
            Self::Cinnabar => "#c8483c",
            Self::Amethyst => "#7a4dbb",
            Self::Graphite => "#6a6459",
        }
    }

    #[must_use]
    pub fn sample_color(self) -> iced::Color {
        let t = glide_theme::Color::from_hex(self.primary_hex()).expect("canonical accent hex literals are valid");
        to_iced_color(t)
    }

    #[must_use]
    pub fn ramp(self) -> [glide_theme::Color; 6] {
        let g: glide_theme::Accent = self.into();
        g.ramp()
    }
}

impl From<Accent> for glide_theme::Accent {
    fn from(accent: Accent) -> Self {
        match accent {
            Accent::Azurit => Self::Blue,
            Accent::Malachite => Self::Teal,
            Accent::Conifer => Self::Green,
            Accent::Amber => Self::Amber,
            Accent::Terracotta => Self::Orange,
            Accent::Cinnabar => Self::Red,
            Accent::Amethyst => Self::Purple,
            Accent::Graphite => Self::Graphite,
        }
    }
}

impl From<glide_theme::Accent> for Accent {
    fn from(accent: glide_theme::Accent) -> Self {
        match accent {
            glide_theme::Accent::Blue => Self::Azurit,
            glide_theme::Accent::Teal => Self::Malachite,
            glide_theme::Accent::Green => Self::Conifer,
            glide_theme::Accent::Amber => Self::Amber,
            glide_theme::Accent::Orange => Self::Terracotta,
            glide_theme::Accent::Red => Self::Cinnabar,
            glide_theme::Accent::Purple => Self::Amethyst,
            glide_theme::Accent::Graphite => Self::Graphite,
        }
    }
}

impl PartialEq<glide_theme::Accent> for Accent {
    fn eq(&self, other: &glide_theme::Accent) -> bool {
        glide_theme::Accent::from(*self) == *other
    }
}

impl PartialEq<Accent> for glide_theme::Accent {
    fn eq(&self, other: &Accent) -> bool {
        *self == glide_theme::Accent::from(*other)
    }
}

#[must_use]
pub fn to_iced_color(color: glide_theme::Color) -> iced::Color {
    iced::Color { r: color.0[0], g: color.0[1], b: color.0[2], a: color.0[3] }
}

#[must_use]
pub fn blend_state_layer(base: iced::Color, current: iced::Color, opacity: f32) -> iced::Color {
    let p = opacity.clamp(0.0, 1.0) * current.a;
    let base_a = base.a;
    let out_a = p + base_a * (1.0 - p);
    if out_a <= 0.0 {
        return iced::Color::TRANSPARENT;
    }
    let r = (current.r * p + base.r * base_a * (1.0 - p)) / out_a;
    let g = (current.g * p + base.g * base_a * (1.0 - p)) / out_a;
    let b = (current.b * p + base.b * base_a * (1.0 - p)) / out_a;
    iced::Color { r: r.clamp(0.0, 1.0), g: g.clamp(0.0, 1.0), b: b.clamp(0.0, 1.0), a: out_a.clamp(0.0, 1.0) }
}

#[must_use]
pub fn hover_layer(base: iced::Color, current: iced::Color) -> iced::Color {
    blend_state_layer(base, current, ST_HOVER)
}

#[must_use]
pub fn press_layer(base: iced::Color, current: iced::Color) -> iced::Color {
    blend_state_layer(base, current, ST_PRESS)
}

#[must_use]
pub fn radius_r1() -> iced::border::Radius {
    iced::border::Radius::from(R1)
}

#[must_use]
pub fn radius_r2() -> iced::border::Radius {
    iced::border::Radius::from(R2)
}

#[must_use]
pub fn radius_r3() -> iced::border::Radius {
    iced::border::Radius::from(R3)
}

#[must_use]
pub fn radius_r4() -> iced::border::Radius {
    iced::border::Radius::from(R4)
}

#[must_use]
pub fn radius_round() -> iced::border::Radius {
    iced::border::Radius::from(R_ROUND)
}

pub use glide_iced::font::use_ui_family;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeState {
    pub appearance: Appearance,
    pub accent: Accent,
    pub tokens: glide_theme::Tokens,
    pub compact: bool,
}

impl ThemeState {
    #[must_use]
    pub fn new(appearance: impl Into<Appearance>, accent: impl Into<Accent>) -> Self {
        let appearance = appearance.into();
        let accent = accent.into();
        let tokens = glide_theme::Tokens::new(appearance.into(), accent.into());
        Self { appearance, accent, tokens, compact: false }
    }

    #[must_use]
    pub const fn compact(self, compact: bool) -> Self {
        Self { compact, ..self }
    }

    #[must_use]
    pub const fn appearance(&self) -> Appearance {
        self.appearance
    }

    #[must_use]
    pub const fn accent(&self) -> Accent {
        self.accent
    }

    #[must_use]
    pub const fn tokens(&self) -> &glide_theme::Tokens {
        &self.tokens
    }

    #[must_use]
    pub fn iced_theme(&self) -> iced::Theme {
        match self.appearance {
            Appearance::Light => iced::Theme::Light,
            Appearance::Dark => iced::Theme::Dark,
        }
    }

    pub fn set_appearance(&mut self, appearance: impl Into<Appearance>) {
        let appearance = appearance.into();
        if self.appearance != appearance {
            self.appearance = appearance;
            self.tokens = glide_theme::Tokens::new(self.appearance.into(), self.accent.into());
        }
    }

    pub fn set_accent(&mut self, accent: impl Into<Accent>) {
        let accent = accent.into();
        if self.accent != accent {
            self.accent = accent;
            self.tokens = glide_theme::Tokens::new(self.appearance.into(), self.accent.into());
        }
    }
}

macro_rules! token_colors {
    ($($name:ident),* $(,)?) => {
        impl ThemeState {
            $(
                #[must_use]
                pub fn $name(&self) -> iced::Color {
                    to_iced_color(self.tokens.$name)
                }
            )*
        }
    };
}

token_colors!(
    win_bg,
    view_bg,
    sidebar_bg,
    bar_bg,
    card_bg,
    popover_bg,
    sunken_bg,
    fg_1,
    fg_2,
    fg_3,
    fg_4,
    line,
    line_2,
    fill_1,
    fill_2,
    fill_3,
    state,
    acc_bg,
    acc_bg_hover,
    acc_fg,
    acc_text,
    acc_soft,
    acc_soft_fg,
    acc_ring,
    ok,
    ok_soft,
    warn,
    warn_soft,
    danger,
    danger_soft,
);

impl Default for ThemeState {
    fn default() -> Self {
        Self::new(Appearance::Dark, Accent::Azurit)
    }
}

#[must_use]
pub fn elevation(theme: &ThemeState, level: glide_theme::elevation::Level) -> iced::Shadow {
    glide_theme::elevation::layers(level, theme.appearance.into())
        .into_iter()
        .find(|layer| layer.blur > 0.0)
        .map_or_else(iced::Shadow::default, |layer| iced::Shadow {
            color: to_iced_color(layer.color),
            offset: iced::Vector::new(layer.dx, layer.dy),
            blur_radius: layer.blur,
        })
}

#[must_use]
pub fn card_style(theme: &ThemeState) -> container::Style {
    container::Style {
        text_color: Some(theme.fg_1()),
        background: Some(iced::Background::Color(theme.card_bg())),
        border: iced::Border { color: theme.line(), width: 1.0, radius: radius_r3() },
        shadow: elevation(theme, glide_theme::elevation::Level::El1),
        snap: true,
    }
}

#[must_use]
pub fn window_container_style(theme: &ThemeState) -> container::Style {
    container::Style {
        text_color: Some(theme.fg_1()),
        background: Some(iced::Background::Color(theme.win_bg())),
        border: iced::Border::default(),
        shadow: iced::Shadow::default(),
        snap: true,
    }
}

#[must_use]
pub fn content_container_style(theme: &ThemeState) -> container::Style {
    window_container_style(theme)
}

pub const SEMIBOLD: iced::Font = iced::Font { weight: iced::font::Weight::Semibold, ..iced::Font::DEFAULT };

pub const CONTROL_HEIGHT: f32 = glide_theme::component::CONTROL;
pub const SMALL_CONTROL_HEIGHT: f32 = glide_theme::component::SMALL_CONTROL;
pub const SIDEBAR_WIDTH: f32 = 256.0;
pub const CONTENT_MAX_WIDTH: f32 = 680.0;
pub const ROW_PADDING_Y: f32 = 5.0;
pub const ROW_PADDING_X: f32 = 12.0;
pub const SECTION_GAP: f32 = 24.0;
pub const PAGE_PADDING_X: f32 = 32.0;
pub const PAGE_PADDING_TOP: f32 = 24.0;
pub const PAGE_TITLE_SIZE: f32 = 26.0;
pub const RING_GAP: f32 = 4.0;
pub const RING_WIDTH: f32 = glide_theme::component::FOCUS_RING;

#[must_use]
pub fn meridian_theme(theme: &ThemeState) -> iced::Theme {
    iced::Theme::custom(
        "Meridian",
        iced::theme::Palette {
            background: theme.win_bg(),
            text: theme.fg_1(),
            primary: theme.acc_bg(),
            success: theme.ok(),
            warning: theme.warn(),
            danger: theme.danger(),
        },
    )
}

#[must_use]
pub fn ramp_color(accent: Accent, step: usize) -> iced::Color {
    to_iced_color(accent.ramp()[step.min(5)])
}

#[must_use]
pub fn solid(color: iced::Color, radius: iced::border::Radius) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(color)),
        border: iced::Border { color: iced::Color::TRANSPARENT, width: 0.0, radius },
        ..container::Style::default()
    }
}

#[must_use]
pub fn rule_style(theme: &ThemeState) -> iced::widget::rule::Style {
    iced::widget::rule::Style {
        color: theme.line(),
        radius: iced::border::Radius::default(),
        fill_mode: iced::widget::rule::FillMode::Full,
        snap: true,
    }
}

#[must_use]
pub fn preview_frame_style(theme: &ThemeState, selected: bool) -> container::Style {
    container::Style {
        border: iced::Border {
            color: if selected { theme.acc_bg() } else { theme.line_2() },
            width: if selected { RING_WIDTH } else { 1.0 },
            radius: radius_r3(),
        },
        ..container::Style::default()
    }
}

#[must_use]
pub fn monitor_style(theme: &ThemeState, selected: bool) -> container::Style {
    let ramp = theme.accent.ramp();
    let (top, bottom) =
        if selected { (to_iced_color(ramp[2]), to_iced_color(ramp[4])) } else { (theme.fill_2(), theme.fill_1()) };
    container::Style {
        text_color: Some(if selected { theme.acc_fg() } else { theme.fg_1() }),
        background: Some(iced::Background::Gradient(iced::Gradient::Linear(
            iced::gradient::Linear::new(std::f32::consts::PI).add_stop(0.0, top).add_stop(1.0, bottom),
        ))),
        border: iced::Border {
            color: if selected { theme.fg_1() } else { theme.line_2() },
            width: if selected { RING_WIDTH } else { 1.0 },
            radius: radius_r1(),
        },
        shadow: elevation(theme, glide_theme::elevation::Level::El1),
        snap: true,
    }
}

#[must_use]
pub fn plain_button_style(theme: &ThemeState, _status: button::Status) -> button::Style {
    button::Style {
        background: None,
        text_color: theme.fg_1(),
        border: iced::Border::default(),
        shadow: iced::Shadow::default(),
        snap: true,
    }
}

pub use glide_sdk::widget::Severity as Tone;

#[cfg(test)]
mod tests {
    use super::*;

    fn luma(c: glide_theme::Color) -> f32 {
        c.0[0] * 0.299 + c.0[1] * 0.587 + c.0[2] * 0.114
    }

    #[test]
    fn radii_tokens_match_meridian_spec_exact_constants() {
        assert!((R1 - 4.0).abs() < f32::EPSILON);
        assert!((R2 - 8.0).abs() < f32::EPSILON);
        assert!((R3 - 12.0).abs() < f32::EPSILON);
        assert!((R4 - 18.0).abs() < f32::EPSILON);
        assert!((R_ROUND - 999.0).abs() < f32::EPSILON);
    }

    #[test]
    fn radii_scale_strictly_increases_monotonically() {
        for pair in SCALE.windows(2) {
            assert!(pair[0] < pair[1]);
        }
    }

    #[test]
    fn state_layer_opacities_match_normative_constants() {
        assert!((ST_HOVER - 0.08).abs() < f32::EPSILON);
        assert!((ST_PRESS - 0.13).abs() < f32::EPSILON);
        assert!((ST_DRAG - 0.18).abs() < f32::EPSILON);
        let opacities = [ST_HOVER, ST_PRESS, ST_DRAG];
        for pair in opacities.windows(2) {
            assert!(pair[0] < pair[1]);
        }
    }

    #[test]
    fn state_layer_darkens_light_surface_without_overflow() {
        let base = glide_theme::Color([1.0, 1.0, 1.0, 1.0]);
        let state = glide_theme::Color::from_rgb8(27, 26, 23, 1.0);
        let hovered = base.state_layer(state, ST_HOVER);
        let pressed = base.state_layer(state, ST_PRESS);

        assert!(luma(hovered) < luma(base));
        assert!(luma(pressed) < luma(hovered));

        for channel in hovered.0 {
            assert!((0.0..=1.0).contains(&channel));
        }
        for channel in pressed.0 {
            assert!((0.0..=1.0).contains(&channel));
        }
    }

    #[test]
    fn state_layer_illuminates_dark_surface_without_overflow() {
        let base = glide_theme::Color::from_hex("#161518").expect("valid hex");
        let state = glide_theme::Color([1.0, 1.0, 1.0, 1.0]);
        let hovered = base.state_layer(state, ST_HOVER);
        let pressed = base.state_layer(state, ST_PRESS);

        assert!(luma(hovered) > luma(base));
        assert!(luma(pressed) > luma(hovered));

        for channel in hovered.0 {
            assert!((0.0..=1.0).contains(&channel));
        }
        for channel in pressed.0 {
            assert!((0.0..=1.0).contains(&channel));
        }
    }

    #[test]
    fn state_layer_on_transparent_base_accumulates_pure_alpha() {
        let transparent = glide_theme::Color([0.0, 0.0, 0.0, 0.0]);
        let state = glide_theme::Color::from_rgb8(27, 26, 23, 1.0);
        let hovered = transparent.state_layer(state, ST_HOVER);
        let pressed = transparent.state_layer(state, ST_PRESS);

        assert!((hovered.0[3] - ST_HOVER).abs() < 1e-5);
        assert!((pressed.0[3] - ST_PRESS).abs() < 1e-5);

        for i in 0..3 {
            assert!((hovered.0[i] - state.0[i]).abs() < 1e-5);
            assert!((pressed.0[i] - state.0[i]).abs() < 1e-5);
        }
    }

    #[test]
    fn state_layer_achromatic_preservation() {
        let gray_base = glide_theme::Color([0.5, 0.5, 0.5, 1.0]);
        let gray_state = glide_theme::Color([0.2, 0.2, 0.2, 1.0]);
        let blended = gray_base.state_layer(gray_state, ST_HOVER);

        assert!((blended.0[0] - blended.0[1]).abs() < 1e-6);
        assert!((blended.0[1] - blended.0[2]).abs() < 1e-6);
    }

    #[test]
    fn state_layer_grid_never_overflows_channels() {
        let steps = [0.0, 0.25, 0.5, 0.75, 1.0];
        let opacities = [ST_HOVER, ST_PRESS, ST_DRAG];

        for &br in &steps {
            for &bg in &steps {
                for &bb in &steps {
                    for &ba in &steps {
                        let base = glide_theme::Color([br, bg, bb, ba]);
                        for &sr in &steps {
                            for &sg in &steps {
                                for &sb in &steps {
                                    let state = glide_theme::Color([sr, sg, sb, 1.0]);
                                    for &op in &opacities {
                                        let out = base.state_layer(state, op);
                                        for &c in &out.0 {
                                            assert!(!c.is_nan());
                                            assert!(!c.is_infinite());
                                            assert!((-1e-6..=1.0 + 1e-6).contains(&c));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn theme_state_converts_glide_color_to_iced_color() {
        let glide_color = glide_theme::Color([0.2, 0.4, 0.6, 0.8]);
        let iced_color = to_iced_color(glide_color);

        assert!((iced_color.r - 0.2).abs() < 1e-5);
        assert!((iced_color.g - 0.4).abs() < 1e-5);
        assert!((iced_color.b - 0.6).abs() < 1e-5);
        assert!((iced_color.a - 0.8).abs() < 1e-5);
    }

    #[test]
    fn theme_state_mutation_updates_tokens_immediately() {
        let mut state = ThemeState::new(Appearance::Light, Accent::Azurit);
        assert_eq!(state.appearance(), Appearance::Light);
        assert_eq!(state.accent(), Accent::Azurit);

        let initial_win = state.tokens().win_bg;
        state.set_appearance(Appearance::Dark);
        assert_eq!(state.appearance(), Appearance::Dark);
        assert_ne!(state.tokens().win_bg, initial_win);

        let initial_acc = state.tokens().acc_bg;
        state.set_accent(Accent::Terracotta);
        assert_eq!(state.accent(), Accent::Terracotta);
        assert_ne!(state.tokens().acc_bg, initial_acc);
    }
}

use std::ops::Deref;

use glide_sdk::Paint;
use serde::{Deserialize, Serialize};

pub use glide_sdk::theme::{R_ROUND, R1, R2, R3, R4, to_iced as to_iced_color, use_ui_family};
pub use glide_sdk::widget::Severity as Tone;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Appearance {
    Light,
    #[default]
    Dark,
}

impl From<Appearance> for glide_sdk::theme::Appearance {
    fn from(appearance: Appearance) -> Self {
        match appearance {
            Appearance::Light => Self::Light,
            Appearance::Dark => Self::Dark,
        }
    }
}

impl From<glide_sdk::theme::Appearance> for Appearance {
    fn from(appearance: glide_sdk::theme::Appearance) -> Self {
        match appearance {
            glide_sdk::theme::Appearance::Light => Self::Light,
            glide_sdk::theme::Appearance::Dark => Self::Dark,
        }
    }
}

impl PartialEq<glide_sdk::theme::Appearance> for Appearance {
    fn eq(&self, other: &glide_sdk::theme::Appearance) -> bool {
        glide_sdk::theme::Appearance::from(*self) == *other
    }
}

impl PartialEq<Appearance> for glide_sdk::theme::Appearance {
    fn eq(&self, other: &Appearance) -> bool {
        *self == glide_sdk::theme::Appearance::from(*other)
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
        let color = glide_sdk::theme::TokenColor::from_hex(self.primary_hex())
            .expect("canonical accent hex literals are valid");
        to_iced_color(color)
    }

    #[must_use]
    pub fn ramp(self) -> [glide_sdk::theme::TokenColor; 6] {
        glide_sdk::theme::Accent::from(self).ramp()
    }
}

impl From<Accent> for glide_sdk::theme::Accent {
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

impl From<glide_sdk::theme::Accent> for Accent {
    fn from(accent: glide_sdk::theme::Accent) -> Self {
        match accent {
            glide_sdk::theme::Accent::Blue => Self::Azurit,
            glide_sdk::theme::Accent::Teal => Self::Malachite,
            glide_sdk::theme::Accent::Green => Self::Conifer,
            glide_sdk::theme::Accent::Amber => Self::Amber,
            glide_sdk::theme::Accent::Orange => Self::Terracotta,
            glide_sdk::theme::Accent::Red => Self::Cinnabar,
            glide_sdk::theme::Accent::Purple => Self::Amethyst,
            glide_sdk::theme::Accent::Graphite => Self::Graphite,
        }
    }
}

impl PartialEq<glide_sdk::theme::Accent> for Accent {
    fn eq(&self, other: &glide_sdk::theme::Accent) -> bool {
        glide_sdk::theme::Accent::from(*self) == *other
    }
}

impl PartialEq<Accent> for glide_sdk::theme::Accent {
    fn eq(&self, other: &Accent) -> bool {
        *self == glide_sdk::theme::Accent::from(*other)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeState {
    pub paint: Paint,
    pub compact: bool,
}

impl Deref for ThemeState {
    type Target = Paint;

    fn deref(&self) -> &Paint {
        &self.paint
    }
}

impl ThemeState {
    #[must_use]
    pub fn new(appearance: impl Into<Appearance>, accent: impl Into<Accent>) -> Self {
        Self { paint: Paint::new(appearance.into().into(), accent.into().into()), compact: false }
    }

    #[must_use]
    pub const fn compact(self, compact: bool) -> Self {
        Self { compact, ..self }
    }

    #[must_use]
    pub fn appearance(&self) -> Appearance {
        self.paint.tokens.appearance.into()
    }

    #[must_use]
    pub fn accent(&self) -> Accent {
        self.paint.tokens.accent.into()
    }

    #[must_use]
    pub const fn tokens(&self) -> &glide_sdk::theme::Tokens {
        &self.paint.tokens
    }

    pub fn set_appearance(&mut self, appearance: impl Into<Appearance>) {
        let appearance = appearance.into();
        if self.appearance() != appearance {
            self.paint = Paint::new(appearance.into(), self.accent().into());
        }
    }

    pub fn set_accent(&mut self, accent: impl Into<Accent>) {
        let accent = accent.into();
        if self.accent() != accent {
            self.paint = Paint::new(self.appearance().into(), accent.into());
        }
    }
}

impl Default for ThemeState {
    fn default() -> Self {
        Self::new(Appearance::Dark, Accent::Azurit)
    }
}

pub const CONTROL_HEIGHT: f32 = glide_sdk::theme::component::CONTROL;
pub const SMALL_CONTROL_HEIGHT: f32 = glide_sdk::theme::component::SMALL_CONTROL;
pub const SIDEBAR_WIDTH: f32 = 264.0;
pub(crate) const CONTENT_MAX_WIDTH: f32 = 880.0;
pub(crate) const ROW_PADDING_Y: f32 = 8.0;
pub const ROW_PADDING_X: f32 = 14.0;
pub const SECTION_GAP: f32 = 24.0;
pub const PAGE_PADDING_X: f32 = 40.0;
pub const PAGE_PADDING_TOP: f32 = 24.0;
pub const RING_GAP: f32 = 4.0;

#[must_use]
pub(crate) fn ramp_color(accent: Accent, step: usize) -> iced::Color {
    to_iced_color(accent.ramp()[step.min(5)])
}

#[cfg(test)]
mod tests {
    use super::*;

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

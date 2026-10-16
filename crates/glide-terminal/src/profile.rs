#![allow(clippy::unreadable_literal)] // Colours are 0xRRGGBB, the same six digits as #rrggbb in the design files

use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use glide_theme::{Appearance, Tokens};

const fn rgb(hex: u32) -> Rgb {
    Rgb { r: (hex >> 16) as u8, g: (hex >> 8) as u8, b: hex as u8 }
}

const fn ansi(hex: [u32; 16]) -> [Rgb; 16] {
    let mut out = [Rgb { r: 0, g: 0, b: 0 }; 16];
    let mut index = 0;
    while index < 16 {
        out[index] = rgb(hex[index]);
        index += 1;
    }
    out
}

const TERMINAL_APP: [Rgb; 16] = ansi([
    0x000000, 0xc23621, 0x25bc24, 0xadad27, 0x492ee1, 0xd338d3, 0x33bbc8, 0xcbcccd, 0x818383, 0xfc391f, 0x31e722,
    0xeaec23, 0x5833ff, 0xf935f8, 0x14f0f0, 0xe9ebeb,
]);

const MERIDIAN_DARK: [Rgb; 16] = ansi([
    0x131218, 0xf0796f, 0x8fe08a, 0xe6c07b, 0x8fb8ff, 0xd797e8, 0x76d6d5, 0xcfe4ca, 0x535158, 0xf4948c, 0xa6e7a2,
    0xebd097, 0xa9cbff, 0xe1aef0, 0x95dedd, 0xffffff,
]);

const MERIDIAN_LIGHT: [Rgb; 16] = ansi([
    0x1e241b, 0xbf372c, 0x1c7a4c, 0x8d600b, 0x1e5bb8, 0x8a3fb8, 0x167a82, 0xd8d6cf, 0x6b6960, 0xd14f43, 0x289460,
    0xa87615, 0x2d72db, 0xa356d4, 0x21949d, 0xf5f4f0,
]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    #[default]
    Meridian,
    Basic,
    Pro,
    Homebrew,
    Ocean,
    Grass,
    RedSands,
    ManPage,
    Novel,
    SilverAerogel,
    ClearDark,
    ClearLight,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub ansi: [Rgb; 16],
    pub foreground: Rgb,
    pub background: Rgb,
    pub bold: Option<Rgb>,
    pub cursor: Rgb,
    pub cursor_text: Rgb,
    pub selection: Rgb,
    pub opacity: f32,
}

impl Palette {
    const fn apple(background: u32, foreground: u32, bold: Option<u32>, cursor: u32, selection: u32) -> Self {
        Self {
            ansi: TERMINAL_APP,
            foreground: rgb(foreground),
            background: rgb(background),
            bold: match bold {
                Some(hex) => Some(rgb(hex)),
                None => None,
            },
            cursor: rgb(cursor),
            cursor_text: rgb(background),
            selection: rgb(selection),
            opacity: 1.0,
        }
    }

    const fn translucent(self, opacity: f32) -> Self {
        Self { opacity, ..self }
    }

    #[must_use]
    pub fn is_dark(&self) -> bool {
        luma(self.background) < 0.5
    }

    #[must_use]
    pub fn indexed(&self, colors: &Colors, index: usize) -> Rgb {
        colors[index].unwrap_or_else(|| match index {
            0..16 => self.ansi[index],
            16..232 => {
                let level = |step: usize| if step == 0 { 0 } else { (55 + step * 40) as u8 };
                let offset = index - 16;
                Rgb { r: level(offset / 36), g: level(offset / 6 % 6), b: level(offset % 6) }
            }
            232..256 => {
                let grey = (8 + 10 * (index - 232)) as u8;
                Rgb { r: grey, g: grey, b: grey }
            }
            _ => self.foreground,
        })
    }

    #[must_use]
    pub fn resolve(&self, colors: &Colors, color: Color) -> Rgb {
        match color {
            Color::Spec(rgb) => rgb,
            Color::Indexed(index) => self.indexed(colors, usize::from(index)),
            Color::Named(named) => self.named(colors, named),
        }
    }

    #[must_use]
    pub fn query(&self, colors: &Colors, index: usize) -> Rgb {
        match index {
            256 => self.named(colors, NamedColor::Foreground),
            257 => self.named(colors, NamedColor::Background),
            258 => self.named(colors, NamedColor::Cursor),
            _ => self.indexed(colors, index),
        }
    }

    fn named(&self, colors: &Colors, named: NamedColor) -> Rgb {
        let index = named as usize;
        if index < 16 {
            return self.indexed(colors, index);
        }
        colors[index].unwrap_or_else(|| match named {
            NamedColor::Background => self.background,
            NamedColor::Cursor => self.cursor,
            NamedColor::BrightForeground => self.bold.unwrap_or(self.foreground),
            NamedColor::DimForeground => self.dim(self.foreground),
            NamedColor::Foreground => self.foreground,
            dim => self.dim(self.indexed(colors, dim.to_bright() as usize)),
        })
    }

    fn dim(&self, color: Rgb) -> Rgb {
        mix(color, self.background, 0.34)
    }

    #[must_use]
    pub fn cell(&self, colors: &Colors, cell: &Cell, bold_bright: bool) -> (Rgb, Rgb) {
        let bold = cell.flags.contains(Flags::BOLD) && bold_bright;
        let dim = cell.flags.contains(Flags::DIM);
        let mut fg = match cell.fg {
            Color::Named(named) if bold && !dim => self.named(colors, named.to_bright()),
            Color::Named(named) if dim && !bold => self.named(colors, named.to_dim()),
            Color::Indexed(index) if bold && index < 8 => self.indexed(colors, usize::from(index) + 8),
            other => self.resolve(colors, other),
        };
        if dim && !matches!(cell.fg, Color::Named(_)) {
            fg = self.dim(fg);
        }
        let mut bg = self.resolve(colors, cell.bg);
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        if cell.flags.contains(Flags::HIDDEN) {
            fg = bg;
        }
        (fg, bg)
    }
}

#[must_use]
pub fn mix(from: Rgb, to: Rgb, amount: f32) -> Rgb {
    let blend = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8;
    Rgb { r: blend(from.r, to.r), g: blend(from.g, to.g), b: blend(from.b, to.b) }
}

fn luma(color: Rgb) -> f32 {
    (0.2126 * f32::from(color.r) + 0.7152 * f32::from(color.g) + 0.0722 * f32::from(color.b)) / 255.0
}

#[must_use]
pub fn color(rgb: Rgb, alpha: f32) -> iced::Color {
    iced::Color::from_rgba8(rgb.r, rgb.g, rgb.b, alpha)
}

fn token(color: glide_theme::Color) -> Rgb {
    let [r, g, b, _] = color.0;
    let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb { r: byte(r), g: byte(g), b: byte(b) }
}

impl Profile {
    pub const ALL: [Self; 12] = [
        Self::Meridian,
        Self::Basic,
        Self::Pro,
        Self::Homebrew,
        Self::Ocean,
        Self::Grass,
        Self::RedSands,
        Self::ManPage,
        Self::Novel,
        Self::SilverAerogel,
        Self::ClearDark,
        Self::ClearLight,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Meridian => "Meridian",
            Self::Basic => "Basic",
            Self::Pro => "Pro",
            Self::Homebrew => "Homebrew",
            Self::Ocean => "Ocean",
            Self::Grass => "Grass",
            Self::RedSands => "Red Sands",
            Self::ManPage => "Man Page",
            Self::Novel => "Novel",
            Self::SilverAerogel => "Silver Aerogel",
            Self::ClearDark => "Clear Dark",
            Self::ClearLight => "Clear Light",
        }
    }

    #[must_use]
    pub fn palette(self, tokens: &Tokens) -> Palette {
        let dark = tokens.appearance == Appearance::Dark;
        match self {
            Self::Meridian => {
                let background = token(tokens.term_bg);
                Palette {
                    ansi: if dark { MERIDIAN_DARK } else { MERIDIAN_LIGHT },
                    foreground: token(tokens.term_fg),
                    background,
                    bold: None,
                    cursor: token(tokens.term_accent),
                    cursor_text: background,
                    selection: if dark { rgb(0x2d3a54) } else { rgb(0xc7daf8) },
                    opacity: 1.0,
                }
            }
            Self::Basic if dark => Palette::apple(0x1e1e1e, 0xf2f2f2, Some(0xffffff), 0x8c8c8c, 0x3f638b),
            Self::Basic => Palette::apple(0xffffff, 0x000000, None, 0x7f7f7f, 0xb3d7ff),
            Self::Pro => Palette::apple(0x000000, 0xf2f2f2, Some(0xffffff), 0x4d4d4d, 0x414141).translucent(0.92),
            Self::Homebrew => Palette::apple(0x000000, 0x28fe14, Some(0x28fe14), 0x38fe27, 0x083905),
            Self::Ocean => Palette::apple(0x224fbc, 0xffffff, Some(0xffffff), 0x7f7f7f, 0x216dff),
            Self::Grass => Palette::apple(0x13773d, 0xfff0a5, Some(0xffb03b), 0x8c2800, 0xb64926),
            Self::RedSands => Palette::apple(0x7a251e, 0xd7c9a7, Some(0xdfbd22), 0xffffff, 0xa4a390),
            Self::ManPage => Palette::apple(0xfef49c, 0x000000, None, 0x7f7f7f, 0xa4c9cd),
            Self::Novel => Palette::apple(0xdfdbc3, 0x3b2322, Some(0x8b2e1d), 0x3a2322, 0xa4a390),
            Self::SilverAerogel => {
                Palette::apple(0x929292, 0x000000, Some(0xffffff), 0x2f2f2f, 0xb0b0b0).translucent(0.9)
            }
            Self::ClearDark => Palette::apple(0x000000, 0xf2f2f2, Some(0xffffff), 0x8c8c8c, 0x4d4d4d).translucent(0.78),
            Self::ClearLight => Palette::apple(0xffffff, 0x000000, None, 0x7f7f7f, 0xb3d7ff).translucent(0.78),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glide_theme::Accent;

    fn tokens(appearance: Appearance) -> Tokens {
        Tokens::new(appearance, Accent::Blue)
    }

    #[test]
    fn meridian_follows_the_terminal_tokens_of_each_appearance() {
        for appearance in [Appearance::Dark, Appearance::Light] {
            let tokens = tokens(appearance);
            let palette = Profile::Meridian.palette(&tokens);
            assert_eq!(palette.background, token(tokens.term_bg));
            assert_eq!(palette.foreground, token(tokens.term_fg));
            assert_eq!(palette.is_dark(), appearance == Appearance::Dark);
        }
    }

    #[test]
    fn the_256_colour_table_is_the_xterm_cube_and_ramp() {
        let palette = Profile::Pro.palette(&tokens(Appearance::Dark));
        let colors = Colors::default();
        assert_eq!(palette.indexed(&colors, 1), rgb(0xc23621));
        assert_eq!(palette.indexed(&colors, 16), rgb(0x000000));
        assert_eq!(palette.indexed(&colors, 21), rgb(0x0000ff));
        assert_eq!(palette.indexed(&colors, 196), rgb(0xff0000));
        assert_eq!(palette.indexed(&colors, 232), rgb(0x080808));
        assert_eq!(palette.indexed(&colors, 255), rgb(0xeeeeee));
    }

    #[test]
    fn application_overrides_win_over_the_profile() {
        let palette = Profile::Basic.palette(&tokens(Appearance::Light));
        let mut colors = Colors::default();
        colors[NamedColor::Background] = Some(rgb(0x123456));
        colors[2] = Some(rgb(0x010203));
        assert_eq!(palette.resolve(&colors, Color::Named(NamedColor::Background)), rgb(0x123456));
        assert_eq!(palette.resolve(&colors, Color::Indexed(2)), rgb(0x010203));
    }

    #[test]
    fn bold_brightens_inverse_swaps_and_hidden_hides() {
        let palette = Profile::Basic.palette(&tokens(Appearance::Light));
        let colors = Colors::default();
        let mut cell = Cell { fg: Color::Named(NamedColor::Red), flags: Flags::BOLD, ..Cell::default() };
        assert_eq!(palette.cell(&colors, &cell, true).0, TERMINAL_APP[9]);
        assert_eq!(palette.cell(&colors, &cell, false).0, TERMINAL_APP[1]);
        cell.flags = Flags::INVERSE;
        assert_eq!(palette.cell(&colors, &cell, true), (palette.background, TERMINAL_APP[1]));
        cell.flags = Flags::HIDDEN;
        let (fg, bg) = palette.cell(&colors, &cell, true);
        assert_eq!(fg, bg);
    }
}

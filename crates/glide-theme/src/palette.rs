#![allow(clippy::unreadable_literal)] // Colours are 0xRRGGBB, the same six digits as #rrggbb in the design files

use serde::Deserialize;

use crate::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Accent {
    #[default]
    Blue,
    Teal,
    Green,
    Amber,
    Orange,
    Red,
    Purple,
    Graphite,
}

impl Accent {
    pub const ALL: &'static [Self] =
        &[Self::Blue, Self::Teal, Self::Green, Self::Amber, Self::Orange, Self::Red, Self::Purple, Self::Graphite];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Teal => "teal",
            Self::Green => "green",
            Self::Amber => "amber",
            Self::Orange => "orange",
            Self::Red => "red",
            Self::Purple => "purple",
            Self::Graphite => "graphite",
        }
    }

    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|accent| accent.key() == key)
    }

    #[must_use]
    pub fn ramp(self) -> [Color; 6] {
        let hex: [u32; 6] = match self {
            Self::Blue => [0xd9e6fb, 0xa7c8f6, 0x5b9bec, 0x2f6fd0, 0x1f56a8, 0x163e79],
            Self::Teal => [0xd3ebe7, 0x9ad4cd, 0x46b3a6, 0x128b7e, 0x0d6a61, 0x084b45],
            Self::Green => [0xdcefd9, 0xb0dbaa, 0x6cbb62, 0x3f9639, 0x2d7229, 0x1e4f1c],
            Self::Amber => [0xfaecd2, 0xf2d296, 0xe0a83f, 0xb57e10, 0x8d600b, 0x63430a],
            Self::Orange => [0xfce4d6, 0xf6bf9c, 0xec8b4f, 0xcf6524, 0xa44b17, 0x74340f],
            Self::Red => [0xfbe3e0, 0xf3b8b1, 0xe3736a, 0xc8483c, 0x9f3529, 0x71241b],
            Self::Purple => [0xeae0f8, 0xcdb4ee, 0xa077d8, 0x7a4dbb, 0x5d3893, 0x402566],
            Self::Graphite => [0xe5e2db, 0xc6c0b5, 0x918a7d, 0x6a6459, 0x4c4740, 0x35312c],
        };
        hex.map(Color::rgb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_read_their_lowercase_names_from_toml() {
        #[derive(Deserialize)]
        struct Wrapper {
            accent: Accent,
        }
        for accent in Accent::ALL {
            let name = format!("{accent:?}").to_lowercase();
            let parsed: Wrapper = toml::from_str(&format!("accent = \"{name}\"\n")).unwrap();
            assert_eq!(parsed.accent, *accent);
        }
        assert_eq!(Accent::default(), Accent::Blue);
    }
}

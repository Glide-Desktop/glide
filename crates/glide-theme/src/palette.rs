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
        let hex: [&str; 6] = match self {
            Self::Blue => ["#d9e6fb", "#a7c8f6", "#5b9bec", "#2f6fd0", "#1f56a8", "#163e79"],
            Self::Teal => ["#d3ebe7", "#9ad4cd", "#46b3a6", "#128b7e", "#0d6a61", "#084b45"],
            Self::Green => ["#dcefd9", "#b0dbaa", "#6cbb62", "#3f9639", "#2d7229", "#1e4f1c"],
            Self::Amber => ["#faecd2", "#f2d296", "#e0a83f", "#b57e10", "#8d600b", "#63430a"],
            Self::Orange => ["#fce4d6", "#f6bf9c", "#ec8b4f", "#cf6524", "#a44b17", "#74340f"],
            Self::Red => ["#fbe3e0", "#f3b8b1", "#e3736a", "#c8483c", "#9f3529", "#71241b"],
            Self::Purple => ["#eae0f8", "#cdb4ee", "#a077d8", "#7a4dbb", "#5d3893", "#402566"],
            Self::Graphite => ["#e5e2db", "#c6c0b5", "#918a7d", "#6a6459", "#4c4740", "#35312c"],
        };
        hex.map(|h| Color::from_hex(h).expect("palette hex literals are well-formed"))
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

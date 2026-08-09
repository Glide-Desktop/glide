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
    pub const ALL: &'static [Self] = &[
        Self::Blue,
        Self::Teal,
        Self::Green,
        Self::Amber,
        Self::Orange,
        Self::Red,
        Self::Purple,
        Self::Graphite,
    ];

    #[must_use]
    pub fn ramp(self) -> [Color; 6] {
        let hex: [&str; 6] = match self {
            Self::Blue => [
                "#d9e6fb", "#a7c8f6", "#5b9bec", "#2f6fd0", "#1f56a8", "#163e79",
            ],
            Self::Teal => [
                "#d3ebe7", "#9ad4cd", "#46b3a6", "#128b7e", "#0d6a61", "#084b45",
            ],
            Self::Green => [
                "#dcefd9", "#b0dbaa", "#6cbb62", "#3f9639", "#2d7229", "#1e4f1c",
            ],
            Self::Amber => [
                "#faecd2", "#f2d296", "#e0a83f", "#b57e10", "#8d600b", "#63430a",
            ],
            Self::Orange => [
                "#fce4d6", "#f6bf9c", "#ec8b4f", "#cf6524", "#a44b17", "#74340f",
            ],
            Self::Red => [
                "#fbe3e0", "#f3b8b1", "#e3736a", "#c8483c", "#9f3529", "#71241b",
            ],
            Self::Purple => [
                "#eae0f8", "#cdb4ee", "#a077d8", "#7a4dbb", "#5d3893", "#402566",
            ],
            Self::Graphite => [
                "#e5e2db", "#c6c0b5", "#918a7d", "#6a6459", "#4c4740", "#35312c",
            ],
        };
        hex.map(|h| Color::from_hex(h).expect("palette hex literals are well-formed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_accent_ramp_parses_and_has_six_steps() {
        for accent in Accent::ALL {
            let ramp = accent.ramp();
            assert_eq!(ramp.len(), 6, "{accent:?} must have six steps");
        }
    }

    #[test]
    fn ramps_get_darker_from_step_zero_to_step_five() {
        for accent in Accent::ALL {
            let ramp = accent.ramp();
            let luma = |c: Color| c.0[0] * 0.299 + c.0[1] * 0.587 + c.0[2] * 0.114;
            for pair in ramp.windows(2) {
                assert!(
                    luma(pair[0]) > luma(pair[1]),
                    "{accent:?}: step must get darker, got {:?} -> {:?}",
                    pair[0].0,
                    pair[1].0
                );
            }
        }
    }

    #[test]
    fn blue_matches_the_design_spec_table_exactly() {
        let ramp = Accent::Blue.ramp();
        assert_eq!(ramp[3], Color::from_hex("#2f6fd0").unwrap());
        assert_eq!(ramp[5], Color::from_hex("#163e79").unwrap());
    }

    #[test]
    fn default_accent_is_blue() {
        assert_eq!(Accent::default(), Accent::Blue);
    }

    #[test]
    fn accent_round_trips_through_toml() {
        for accent in Accent::ALL {
            let name = match accent {
                Accent::Blue => "blue",
                Accent::Teal => "teal",
                Accent::Green => "green",
                Accent::Amber => "amber",
                Accent::Orange => "orange",
                Accent::Red => "red",
                Accent::Purple => "purple",
                Accent::Graphite => "graphite",
            };
            let parsed: Accent = toml::from_str(&format!("accent = \"{name}\"\n"))
                .map_or_else(|e| panic!("{name}: {e}"), |w: AccentWrapper| w.accent);
            assert_eq!(parsed, *accent);
        }
    }

    #[derive(Deserialize)]
    struct AccentWrapper {
        accent: Accent,
    }
}

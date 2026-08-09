use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WallpaperFit {
    #[default]
    Cover,
    Contain,
    Center,
    Tile,
}

impl WallpaperFit {
    #[must_use]
    pub const fn always_covers_the_output(self) -> bool {
        matches!(self, Self::Cover | Self::Tile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_fit_is_cover() {
        assert_eq!(WallpaperFit::default(), WallpaperFit::Cover);
    }

    #[test]
    fn cover_and_tile_always_cover_the_output() {
        assert!(WallpaperFit::Cover.always_covers_the_output());
        assert!(WallpaperFit::Tile.always_covers_the_output());
        assert!(!WallpaperFit::Contain.always_covers_the_output());
        assert!(!WallpaperFit::Center.always_covers_the_output());
    }

    #[test]
    fn round_trips_through_toml() {
        #[derive(Deserialize)]
        struct Wrapper {
            fit: WallpaperFit,
        }
        for (text, expected) in [
            ("cover", WallpaperFit::Cover),
            ("contain", WallpaperFit::Contain),
            ("center", WallpaperFit::Center),
            ("tile", WallpaperFit::Tile),
        ] {
            let parsed: Wrapper = toml::from_str(&format!("fit = \"{text}\"\n")).unwrap();
            assert_eq!(parsed.fit, expected);
        }
    }
}

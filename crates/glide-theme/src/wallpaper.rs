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
    fn fits_read_from_toml_and_only_cover_and_tile_fill_the_output() {
        #[derive(Deserialize)]
        struct Wrapper {
            fit: WallpaperFit,
        }
        for (text, expected, covers) in [
            ("cover", WallpaperFit::Cover, true),
            ("contain", WallpaperFit::Contain, false),
            ("center", WallpaperFit::Center, false),
            ("tile", WallpaperFit::Tile, true),
        ] {
            let parsed: Wrapper = toml::from_str(&format!("fit = \"{text}\"\n")).unwrap();
            assert_eq!(parsed.fit, expected);
            assert_eq!(expected.always_covers_the_output(), covers);
        }
    }
}

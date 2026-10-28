use std::path::PathBuf;

use crate::{Accent, Appearance};

pub const FAMILY: &str = "Colloid";

pub const FALLBACK: &str = "Papirus";

const fn tint(accent: Accent) -> &'static str {
    match accent {
        Accent::Blue => "",
        Accent::Teal => "-Teal",
        Accent::Green => "-Green",
        Accent::Amber => "-Yellow",
        Accent::Orange => "-Orange",
        Accent::Red => "-Red",
        Accent::Purple => "-Purple",
        Accent::Graphite => "-Grey",
    }
}

const fn shade(appearance: Appearance) -> &'static str {
    match appearance {
        Appearance::Light => "-Light",
        Appearance::Dark => "-Dark",
    }
}

#[must_use]
pub fn candidates(appearance: Appearance, accent: Accent) -> Vec<String> {
    let (tint, shade) = (tint(accent), shade(appearance));
    let mut names = vec![format!("{FAMILY}{tint}{shade}")];
    if !tint.is_empty() {
        names.push(format!("{FAMILY}{shade}"));
    }
    names.push(FAMILY.to_string());
    names.push(FALLBACK.to_string());
    names
}

#[must_use]
pub fn default_theme(appearance: Appearance, accent: Accent, installed: impl Fn(&str) -> bool) -> String {
    candidates(appearance, accent).into_iter().find(|name| installed(name)).unwrap_or_else(|| FALLBACK.to_string())
}

#[must_use]
pub fn installed_in(roots: &[PathBuf], name: &str) -> bool {
    roots.iter().any(|root| root.join(name).join("index.theme").is_file())
}

#[must_use]
pub fn resolve(configured: Option<&str>, appearance: Appearance, accent: Accent, roots: &[PathBuf]) -> String {
    match configured.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => name.to_string(),
        None => default_theme(appearance, accent, |name| installed_in(roots, name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_accent_and_appearance_names_a_colloid_variant_first_and_papirus_last() {
        for &accent in Accent::ALL {
            for appearance in [Appearance::Light, Appearance::Dark] {
                let names = candidates(appearance, accent);
                assert!(names[0].starts_with(FAMILY), "{names:?}");
                assert!(names[0].ends_with(shade(appearance)), "{names:?}");
                assert_eq!(names.last().map(String::as_str), Some(FALLBACK));
                assert!(names.contains(&FAMILY.to_string()));
            }
        }
    }

    #[test]
    fn accents_whose_names_differ_from_colloid_map_to_its_folder_colours() {
        assert_eq!(candidates(Appearance::Dark, Accent::Amber)[0], "Colloid-Yellow-Dark");
        assert_eq!(candidates(Appearance::Light, Accent::Graphite)[0], "Colloid-Grey-Light");
        assert_eq!(candidates(Appearance::Light, Accent::Blue), ["Colloid-Light", "Colloid", "Papirus"]);
        assert_eq!(
            candidates(Appearance::Dark, Accent::Teal),
            ["Colloid-Teal-Dark", "Colloid-Dark", "Colloid", "Papirus"]
        );
    }

    #[test]
    fn a_missing_accent_variant_falls_back_to_the_plain_shade_then_papirus() {
        let only = |names: &'static [&'static str]| move |name: &str| names.contains(&name);
        assert_eq!(default_theme(Appearance::Dark, Accent::Red, only(&["Colloid-Dark"])), "Colloid-Dark");
        assert_eq!(default_theme(Appearance::Dark, Accent::Red, only(&[])), "Papirus");
    }

    #[test]
    fn a_configured_theme_wins_and_blank_counts_as_unset() {
        let roots = Vec::new();
        assert_eq!(resolve(Some("Adwaita"), Appearance::Dark, Accent::Blue, &roots), "Adwaita");
        assert_eq!(resolve(Some("  "), Appearance::Dark, Accent::Blue, &roots), "Papirus");
    }

    #[test]
    fn a_theme_counts_as_installed_only_with_an_index() {
        let root = std::env::temp_dir().join(format!("glide-theme-icons-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Colloid-Dark")).expect("temp dir");
        std::fs::create_dir_all(root.join("Colloid")).expect("temp dir");
        std::fs::write(root.join("Colloid").join("index.theme"), "[Icon Theme]\n").expect("index");
        let roots = vec![root.clone()];
        assert_eq!(resolve(None, Appearance::Dark, Accent::Blue, &roots), "Colloid");
        std::fs::remove_dir_all(root).ok();
    }
}

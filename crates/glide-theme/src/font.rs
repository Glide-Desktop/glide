pub const DEFAULT_FAMILY: &str = "Adwaita Sans";

pub const FALLBACK_FAMILIES: &[&str] =
    &[DEFAULT_FAMILY, "Cantarell", "Inter", "Noto Sans", "Ubuntu", "DejaVu Sans", "Liberation Sans"];

#[must_use]
pub fn ui_family<'a>(requested: Option<&str>, available: impl IntoIterator<Item = &'a str>) -> String {
    let available: Vec<&str> = available.into_iter().map(str::trim).collect();
    let installed =
        |name: &str| available.iter().copied().find(|family| !name.is_empty() && family.eq_ignore_ascii_case(name));
    requested
        .map(str::trim)
        .and_then(installed)
        .into_iter()
        .chain(FALLBACK_FAMILIES.iter().copied().filter_map(installed))
        .next()
        .unwrap_or(DEFAULT_FAMILY)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ui_family_falls_back_to_an_installed_face() {
        assert_eq!(ui_family(None, ["Noto Sans", "Adwaita Sans"]), DEFAULT_FAMILY);
        assert_eq!(ui_family(None, ["DejaVu Sans", "Noto Sans"]), "Noto Sans");
        assert_eq!(ui_family(Some("Inter"), ["Inter", "Adwaita Sans"]), "Inter");
        assert_eq!(ui_family(Some("Missing Font"), ["Cantarell", "Liberation Sans"]), "Cantarell");
        assert_eq!(ui_family(Some("  "), std::iter::empty()), DEFAULT_FAMILY);
    }

    #[test]
    fn the_ui_family_uses_the_installed_spelling() {
        assert_eq!(ui_family(Some("inter"), ["INTER", "Adwaita Sans"]), "INTER");
        assert_eq!(ui_family(Some(" noto sans "), ["Noto Sans"]), "Noto Sans");
        assert_eq!(ui_family(None, ["adwaita sans"]), "adwaita sans");
        assert_eq!(ui_family(None, std::iter::empty()), DEFAULT_FAMILY);
    }
}

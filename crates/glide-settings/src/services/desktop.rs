use std::process::{Command, Stdio};

use crate::theme::{Accent, Appearance};

#[must_use]
pub const fn color_scheme(appearance: Appearance) -> &'static str {
    match appearance {
        Appearance::Light => "prefer-light",
        Appearance::Dark => "prefer-dark",
    }
}

#[must_use]
pub const fn gnome_accent(accent: Accent) -> &'static str {
    match accent {
        Accent::Azurit => "blue",
        Accent::Malachite => "teal",
        Accent::Conifer => "green",
        Accent::Amber => "yellow",
        Accent::Terracotta => "orange",
        Accent::Cinnabar => "red",
        Accent::Amethyst => "purple",
        Accent::Graphite => "slate",
    }
}

#[must_use]
pub fn gtk_theme_for(current: &str, appearance: Appearance) -> Option<&'static str> {
    let current = current.trim().trim_matches('\'');
    if !matches!(current, "" | "Adwaita" | "Adwaita-dark" | "adwaita" | "adwaita-dark") {
        return None;
    }
    Some(match appearance {
        Appearance::Light => "Adwaita",
        Appearance::Dark => "Adwaita-dark",
    })
}

fn set(key: &str, value: &str) {
    let _ = Command::new("gsettings")
        .args(["set", "org.gnome.desktop.interface", key, value])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub fn sync(appearance: Appearance, accent: Accent, icon_theme: Option<&str>) {
    set("color-scheme", color_scheme(appearance));
    set("accent-color", gnome_accent(accent));
    let current =
        super::run::output("gsettings", &["get", "org.gnome.desktop.interface", "gtk-theme"]).unwrap_or_default();
    if let Some(theme) = gtk_theme_for(&current, appearance) {
        set("gtk-theme", theme);
    }
    if let Some(icons) = icon_theme.filter(|name| !name.is_empty()) {
        set("icon-theme", icons);
    }
}

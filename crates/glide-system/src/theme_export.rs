use std::path::Path;
use std::sync::{Mutex, OnceLock, PoisonError};

use glide_config::toolkits::Support;
use glide_theme::{Appearance, Tokens, export};

use crate::system_info::DesktopAppearance;

const GTK_IMPORT: &str = "@import url(\"glide.css\");";
const QT_SCHEME: &str = "Glide.conf";
const KDE_SCHEME: &str = "Glide";
const QT_FONT_POINTS: u32 = 11;

fn named(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub fn export(look: &DesktopAppearance, gtk_theme: Option<&str>) {
    remember(look.toolkits);
    let (Some(config), Some(data)) = (glide_config::config_home(), glide_config::data_home()) else {
        return;
    };
    let tokens = Tokens::new(look.appearance, look.accent);
    let icon_theme = named(look.icon_theme.as_deref());
    write_gtk(&config.join("gtk-4.0"), &export::gtk4_css(&tokens));
    write_gtk(&config.join("gtk-3.0"), &export::gtk3_css(&tokens));
    for (dir, gtk3) in [("gtk-3.0", true), ("gtk-4.0", false)] {
        write_settings_ini(&config.join(dir), &gtk_settings(look, gtk_theme, gtk3));
    }
    let family = named(look.font_family.as_deref()).unwrap_or(glide_theme::font::DEFAULT_FAMILY);
    for (name, roles) in [("qt6ct", 22), ("qt5ct", 21)] {
        let colors = export::qtct_colors(&tokens, roles);
        write_qtct(&config.join(name), name, &colors, family, icon_theme, look.toolkits.kvantum);
    }
    write_kde(&config, &data, &tokens, family, icon_theme);
    write_kvantum(&config.join("Kvantum"), &tokens);
    write_sourceview(&data, tokens.accent);
    share_with_flatpak(&data);
}

#[must_use]
pub fn gtk_settings(look: &DesktopAppearance, gtk_theme: Option<&str>, gtk3: bool) -> Vec<(&'static str, String)> {
    let mut keys = Vec::new();
    if gtk3 {
        if let Some(theme) = named(gtk_theme) {
            keys.push(("gtk-theme-name", theme.to_string()));
        }
        keys.push(("gtk-application-prefer-dark-theme", (look.appearance == Appearance::Dark).to_string()));
    }
    if let Some(icons) = named(look.icon_theme.as_deref()) {
        keys.push(("gtk-icon-theme-name", icons.to_string()));
    }
    if let Some(family) = named(look.font_family.as_deref()) {
        keys.push(("gtk-font-name", format!("{family} {}", crate::system_info::INTERFACE_FONT_POINTS)));
    }
    if let Some(cursor) = named(look.cursor_theme.as_deref()) {
        keys.push(("gtk-cursor-theme-name", cursor.to_string()));
    }
    if let Some(size) = look.cursor_size {
        keys.push(("gtk-cursor-theme-size", size.to_string()));
    }
    keys
}

fn write_settings_ini(dir: &Path, keys: &[(&str, String)]) {
    let path = dir.join("settings.ini");
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    for (key, value) in keys {
        text = set_ini(&text, "Settings", key, value);
    }
    write(&path, &text);
}

const SCHEME_KEYS: [(&str, &str); 2] =
    [("org.gnome.TextEditor", "style-scheme"), ("org.gnome.gedit.preferences.editor", "scheme")];

fn write_sourceview(data: &Path, accent: glide_theme::Accent) {
    let light = Tokens::new(Appearance::Light, accent);
    let dark = Tokens::new(Appearance::Dark, accent);
    for dir in ["gtksourceview-5", "gtksourceview-4"] {
        let styles = data.join(dir).join("styles");
        write(&styles.join("glide.xml"), &export::sourceview_scheme(&light, "glide", "Glide", "glide-dark"));
        write(&styles.join("glide-dark.xml"), &export::sourceview_scheme(&dark, "glide-dark", "Glide Dark", "glide"));
    }
    for (schema, key) in SCHEME_KEYS {
        adopt_scheme(schema, key);
    }
}

#[must_use]
pub fn replaces_scheme(current: &str) -> bool {
    let current = current.trim().trim_matches('\'');
    matches!(current, "" | "Adwaita" | "Adwaita-dark" | "classic" | "classic-dark" | "kate" | "tango")
}

fn adopt_scheme(schema: &str, key: &str) {
    let Ok(output) = std::process::Command::new("gsettings")
        .args(["get", schema, key])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return;
    };
    if !output.status.success() || !replaces_scheme(&String::from_utf8_lossy(&output.stdout)) {
        return;
    }
    let _ = std::process::Command::new("gsettings")
        .args(["set", schema, key, "'glide'"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

fn write(path: &Path, contents: &str) {
    if std::fs::read_to_string(path).is_ok_and(|current| current == contents) {
        return;
    }
    if let Some(dir) = path.parent()
        && let Err(err) = std::fs::create_dir_all(dir)
    {
        tracing::warn!(path = %dir.display(), "cannot create a theme directory: {err}");
        return;
    }
    if let Err(err) = glide_config::write_atomic(path, contents.as_bytes(), 0o644) {
        tracing::warn!(path = %path.display(), "cannot write an exported theme: {err}");
    }
}

fn write_gtk(dir: &Path, css: &str) {
    write(&dir.join("glide.css"), css);
    let main = dir.join("gtk.css");
    let current = std::fs::read_to_string(&main).unwrap_or_default();
    if !current.lines().any(|line| line.trim() == GTK_IMPORT) {
        write(&main, &format!("{GTK_IMPORT}\n{current}"));
    }
}

#[must_use]
pub fn set_ini(text: &str, section: &str, key: &str, value: &str) -> String {
    let header = format!("[{section}]");
    let mut out: Vec<String> = Vec::new();
    let mut in_section = false;
    let mut seen_section = false;
    let mut written = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_section && !written {
                out.push(format!("{key}={value}"));
                written = true;
            }
            in_section = trimmed == header;
            seen_section |= in_section;
        } else if in_section && trimmed.split_once('=').is_some_and(|(name, _)| name.trim() == key) {
            if !written {
                out.push(format!("{key}={value}"));
                written = true;
            }
            continue;
        }
        out.push(line.to_string());
    }
    if !written {
        if !seen_section {
            if out.last().is_some_and(|line| !line.trim().is_empty()) {
                out.push(String::new());
            }
            out.push(header);
        }
        out.push(format!("{key}={value}"));
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

fn ini_value<'a>(text: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let header = format!("[{section}]");
    let mut in_section = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == header;
        } else if in_section
            && let Some((name, value)) = trimmed.split_once('=')
            && name.trim() == key
        {
            return Some(value.trim());
        }
    }
    None
}

fn write_qtct(dir: &Path, name: &str, colors: &str, family: &str, icon_theme: Option<&str>, kvantum: bool) {
    let scheme = dir.join("colors").join(QT_SCHEME);
    write(&scheme, colors);
    let conf = dir.join(format!("{name}.conf"));
    let text = std::fs::read_to_string(&conf).unwrap_or_default();
    write(&conf, &qtct_conf(&text, &scheme.to_string_lossy(), family, icon_theme, kvantum));
}

const QT_GNOME_BUTTON_LAYOUT: &str = "3";

#[must_use]
pub fn qtct_conf(current: &str, scheme: &str, family: &str, icon_theme: Option<&str>, kvantum: bool) -> String {
    let mut text = set_ini(current, "Appearance", "color_scheme_path", scheme);
    text = set_ini(&text, "Appearance", "custom_palette", "true");
    let style = ini_value(&text, "Appearance", "style").unwrap_or_default().to_string();
    let wanted = if kvantum { "kvantum" } else { "Fusion" };
    if matches!(style.as_str(), "" | "Fusion" | "kvantum") && style != wanted {
        text = set_ini(&text, "Appearance", "style", wanted);
    }
    if matches!(ini_value(&text, "Appearance", "standard_dialogs"), None | Some("" | "default")) {
        text = set_ini(&text, "Appearance", "standard_dialogs", "xdgdesktopportal");
    }
    if let Some(icons) = icon_theme {
        text = set_ini(&text, "Appearance", "icon_theme", icons);
    }
    if ini_value(&text, "Interface", "buttonbox_layout").is_none() {
        text = set_ini(&text, "Interface", "buttonbox_layout", QT_GNOME_BUTTON_LAYOUT);
    }
    set_ini(&text, "Fonts", "general", &export::qt_font(family, QT_FONT_POINTS))
}

fn write_kde(config: &Path, data: &Path, tokens: &Tokens, family: &str, icon_theme: Option<&str>) {
    let scheme = export::kde_color_scheme(tokens, KDE_SCHEME);
    write(&data.join("color-schemes").join(format!("{KDE_SCHEME}.colors")), &scheme);
    let globals_path = config.join("kdeglobals");
    let mut globals = std::fs::read_to_string(&globals_path).unwrap_or_default();
    let mut section = String::new();
    for line in scheme.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed[1..trimmed.len() - 1].to_string();
        } else if let Some((key, value)) = trimmed.split_once('=') {
            globals = set_ini(&globals, &section, key, value);
        }
    }
    globals = set_ini(&globals, "General", "font", &export::qt_font(family, QT_FONT_POINTS).replace('"', ""));
    if let Some(icons) = icon_theme {
        globals = set_ini(&globals, "Icons", "Theme", icons);
    }
    write(&globals_path, &globals);
}

fn share_with_flatpak(data: &Path) {
    static DONE: OnceLock<()> = OnceLock::new();
    if DONE.set(()).is_err() {
        return;
    }
    let overrides = std::fs::read_to_string(data.join("flatpak/overrides/global")).unwrap_or_default();
    if overrides.contains("xdg-config/gtk-4.0") && overrides.contains("xdg-config/gtk-3.0") {
        return;
    }
    let status = std::process::Command::new("flatpak")
        .args(["override", "--user", "--filesystem=xdg-config/gtk-4.0:ro", "--filesystem=xdg-config/gtk-3.0:ro"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if let Ok(status) = status
        && status.success()
    {
        tracing::info!("shared the exported GTK theme with Flatpak apps");
    }
}

#[must_use]
pub fn gtk3_theme(current: &str, appearance: Appearance, toolkits: Support) -> Option<String> {
    let current = current.trim().trim_matches('\'').to_ascii_lowercase();
    if !matches!(current.as_str(), "" | "adwaita" | "adwaita-dark" | "adw-gtk3" | "adw-gtk3-dark") {
        return None;
    }
    if !toolkits.adw_gtk3 {
        return None;
    }
    Some(match appearance {
        Appearance::Light => "adw-gtk3".to_string(),
        Appearance::Dark => "adw-gtk3-dark".to_string(),
    })
}

fn write_kvantum(dir: &Path, tokens: &Tokens) {
    let theme = dir.join(glide_theme::kvantum::THEME);
    write(&theme.join(format!("{}.kvconfig", glide_theme::kvantum::THEME)), &glide_theme::kvantum::config(tokens));
    write(&theme.join(format!("{}.svg", glide_theme::kvantum::THEME)), &glide_theme::kvantum::svg(tokens));
    let manager = dir.join("kvantum.kvconfig");
    let current = std::fs::read_to_string(&manager).unwrap_or_default();
    if glide_theme::kvantum::replaces_theme(&current) {
        write(&manager, &glide_theme::kvantum::manager_config(&current));
    }
}

static TOOLKITS: Mutex<Option<Support>> = Mutex::new(None);

fn remember(toolkits: Support) {
    *TOOLKITS.lock().unwrap_or_else(PoisonError::into_inner) = Some(toolkits);
}

fn toolkits() -> Support {
    let known = *TOOLKITS.lock().unwrap_or_else(PoisonError::into_inner);
    known.unwrap_or_else(|| {
        let detected = Support::detect();
        remember(detected);
        detected
    })
}

#[must_use]
pub fn qt_style_override() -> Option<&'static str> {
    std::env::var_os("QT_STYLE_OVERRIDE").is_none().then(|| toolkits().qt_style_override()).flatten()
}

#[must_use]
pub fn qt_platform_theme() -> Option<&'static str> {
    toolkits().qt_platform_theme()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_keys_are_replaced_in_place_and_others_are_kept() {
        let text = "[Appearance]\nstyle=Breeze\ncustom_palette=false\n\n[Fonts]\nfixed=Mono\n";
        let text = set_ini(text, "Appearance", "custom_palette", "true");
        let text = set_ini(&text, "Fonts", "general", "Sans");
        let text = set_ini(&text, "Interface", "menus_have_icons", "true");
        assert_eq!(ini_value(&text, "Appearance", "custom_palette"), Some("true"));
        assert_eq!(ini_value(&text, "Appearance", "style"), Some("Breeze"));
        assert_eq!(ini_value(&text, "Fonts", "fixed"), Some("Mono"));
        assert_eq!(ini_value(&text, "Fonts", "general"), Some("Sans"));
        assert_eq!(ini_value(&text, "Interface", "menus_have_icons"), Some("true"));
        assert_eq!(text.matches("custom_palette").count(), 1);
    }

    #[test]
    fn only_stock_editor_schemes_are_replaced() {
        assert!(replaces_scheme("'Adwaita'\n"));
        assert!(replaces_scheme("'classic-dark'"));
        assert!(!replaces_scheme("'solarized-dark'"));
        assert!(!replaces_scheme("'glide'"));
    }

    fn look() -> DesktopAppearance {
        DesktopAppearance {
            appearance: Appearance::Dark,
            accent: glide_theme::Accent::Blue,
            icon_theme: Some("Papirus-Dark".into()),
            cursor_theme: Some("  ".into()),
            cursor_size: Some(32),
            font_family: Some("Inter".into()),
            wallpaper: None,
            toolkits: Support::default(),
        }
    }

    #[test]
    fn x11_gtk_apps_without_xsettings_read_the_look_from_settings_ini() {
        let gtk3 = gtk_settings(&look(), Some("adw-gtk3-dark"), true);
        for (key, value) in [
            ("gtk-theme-name", "adw-gtk3-dark"),
            ("gtk-application-prefer-dark-theme", "true"),
            ("gtk-icon-theme-name", "Papirus-Dark"),
            ("gtk-font-name", "Inter 11"),
            ("gtk-cursor-theme-size", "32"),
        ] {
            assert!(gtk3.contains(&(key, value.to_string())), "{key}");
        }
        assert!(gtk3.iter().all(|(key, _)| *key != "gtk-cursor-theme-name"), "a blank cursor theme stays unset");
        let gtk4 = gtk_settings(&look(), Some("adw-gtk3-dark"), false);
        assert!(gtk4.iter().all(|(key, _)| !matches!(*key, "gtk-theme-name" | "gtk-application-prefer-dark-theme")));
        assert!(gtk_settings(&look(), None, true).iter().all(|(key, _)| *key != "gtk-theme-name"));
    }

    #[test]
    fn qt_dialogs_go_to_the_portal_only_while_the_user_has_not_picked_others() {
        let fresh = qtct_conf("", "/s/Glide.conf", "Inter", Some("Papirus"), false);
        assert_eq!(ini_value(&fresh, "Appearance", "standard_dialogs"), Some("xdgdesktopportal"));
        assert_eq!(ini_value(&fresh, "Interface", "buttonbox_layout"), Some(QT_GNOME_BUTTON_LAYOUT));
        assert_eq!(ini_value(&fresh, "Appearance", "style"), Some("Fusion"));
        let own = "[Appearance]\nstandard_dialogs=kde\nstyle=Breeze\n\n[Interface]\nbuttonbox_layout=2\n";
        let kept = qtct_conf(own, "/s/Glide.conf", "Inter", None, true);
        assert_eq!(ini_value(&kept, "Appearance", "standard_dialogs"), Some("kde"));
        assert_eq!(ini_value(&kept, "Appearance", "style"), Some("Breeze"));
        assert_eq!(ini_value(&kept, "Interface", "buttonbox_layout"), Some("2"));
        assert_eq!(ini_value(&kept, "Appearance", "icon_theme"), None);
    }

    #[test]
    fn a_users_own_gtk_theme_is_left_alone() {
        let installed = Support { adw_gtk3: true, ..Support::default() };
        assert_eq!(gtk3_theme("'Orchis-Dark'", Appearance::Dark, installed), None);
        assert_eq!(gtk3_theme("'Adwaita-dark'", Appearance::Dark, installed).as_deref(), Some("adw-gtk3-dark"));
        assert_eq!(gtk3_theme("'Adwaita-dark'", Appearance::Dark, Support::default()), None);
    }
}

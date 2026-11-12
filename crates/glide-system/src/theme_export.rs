use std::path::Path;
use std::sync::{Mutex, OnceLock, PoisonError};

use glide_config::toolkits::Support;
use glide_theme::export::{self, Qt};
use glide_theme::{Appearance, Tokens};

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
    for (name, roles, qt) in [("qt6ct", 22, Qt::Six), ("qt5ct", 21, Qt::Five)] {
        let colors = export::qtct_colors(&tokens, roles);
        write_qtct(&config.join(name), name, &colors, (family, qt), icon_theme, look.toolkits.kvantum);
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

fn user_file(path: &Path) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Some(String::new()),
        Err(err) => {
            tracing::warn!(path = %path.display(), "leaving a settings file alone because it cannot be read: {err}");
            None
        }
    }
}

fn write_settings_ini(dir: &Path, keys: &[(&str, String)]) {
    let path = dir.join("settings.ini");
    let Some(mut text) = user_file(&path) else { return };
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
    let run = |args: &[&str]| crate::spawn::run_with_timeout("gsettings", args, crate::system_info::APPEARANCE_TIMEOUT);
    let Some(output) = run(&["get", schema, key]) else {
        return;
    };
    if !output.status.success() || !replaces_scheme(&String::from_utf8_lossy(&output.stdout)) {
        return;
    }
    let _ = run(&["set", schema, key, "'glide'"]);
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
    let Some(current) = user_file(&main) else { return };
    if !current.lines().any(|line| line.trim() == GTK_IMPORT) {
        write(&main, &format!("{GTK_IMPORT}\n{current}"));
    }
}

#[must_use]
pub fn set_ini(text: &str, section: &str, key: &str, value: &str) -> String {
    fn append(out: &mut Vec<String>, entry: String) {
        let end = out.iter().rposition(|line| !line.trim().is_empty()).map_or(0, |last| last + 1);
        out.insert(end, entry);
    }
    let header = format!("[{section}]");
    let mut out: Vec<String> = Vec::new();
    let mut in_section = false;
    let mut seen_section = false;
    let mut written = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_section && !written {
                append(&mut out, format!("{key}={value}"));
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
    if !written && seen_section {
        append(&mut out, format!("{key}={value}"));
    } else if !written {
        if out.last().is_some_and(|line| !line.trim().is_empty()) {
            out.push(String::new());
        }
        out.push(header);
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

fn write_qtct(dir: &Path, name: &str, colors: &str, font: (&str, Qt), icon_theme: Option<&str>, kvantum: bool) {
    let scheme = dir.join("colors").join(QT_SCHEME);
    write(&scheme, colors);
    let conf = dir.join(format!("{name}.conf"));
    let Some(text) = user_file(&conf) else { return };
    write(&conf, &qtct_conf(&text, &scheme.to_string_lossy(), font, icon_theme, kvantum));
}

const QT_GNOME_BUTTON_LAYOUT: &str = "3";

fn unset_or(text: &str, section: &str, key: &str, ours: impl Fn(&str) -> bool) -> bool {
    ini_value(text, section, key).is_none_or(|value| value.is_empty() || ours(value))
}

#[must_use]
pub fn is_glide_qt_font(value: &str, points: u32) -> bool {
    let value = value.trim_matches('"');
    [Qt::Five, Qt::Six].into_iter().any(|qt| {
        let sample = export::qt_font("", points, qt);
        let tail = sample.trim_matches('"');
        value.ends_with(tail) && value.len() > tail.len()
    })
}

#[must_use]
pub fn qtct_conf(current: &str, scheme: &str, font: (&str, Qt), icon_theme: Option<&str>, kvantum: bool) -> String {
    let (family, qt) = font;
    let file_name = Path::new(scheme).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let palette_in_use = ini_value(current, "Appearance", "custom_palette") == Some("true");
    let owned = !palette_in_use
        || unset_or(current, "Appearance", "color_scheme_path", |path| {
            path == scheme || Path::new(path).file_name().is_some_and(|name| name.to_string_lossy() == file_name)
        });
    let mut text = current.to_string();
    if owned {
        text = set_ini(&text, "Appearance", "color_scheme_path", scheme);
        text = set_ini(&text, "Appearance", "custom_palette", "true");
    }
    let style = ini_value(&text, "Appearance", "style").unwrap_or_default().to_string();
    let wanted = if kvantum { "kvantum" } else { "Fusion" };
    if matches!(style.as_str(), "" | "Fusion" | "kvantum") && style != wanted {
        text = set_ini(&text, "Appearance", "style", wanted);
    }
    if matches!(ini_value(&text, "Appearance", "standard_dialogs"), None | Some("" | "default")) {
        text = set_ini(&text, "Appearance", "standard_dialogs", "xdgdesktopportal");
    }
    if let Some(icons) = icon_theme
        && (owned || unset_or(&text, "Appearance", "icon_theme", |_| false))
    {
        text = set_ini(&text, "Appearance", "icon_theme", icons);
    }
    if ini_value(&text, "Interface", "buttonbox_layout").is_none() {
        text = set_ini(&text, "Interface", "buttonbox_layout", QT_GNOME_BUTTON_LAYOUT);
    }
    if owned || unset_or(&text, "Fonts", "general", |font| is_glide_qt_font(font, QT_FONT_POINTS)) {
        text = set_ini(&text, "Fonts", "general", &export::qt_font(family, QT_FONT_POINTS, qt));
    }
    text
}

#[must_use]
pub fn replaces_kde_scheme(globals: &str) -> bool {
    unset_or(globals, "General", "ColorScheme", |scheme| scheme == KDE_SCHEME || scheme.starts_with("Breeze"))
}

#[must_use]
pub fn kde_globals(current: &str, scheme: &str, family: &str, icon_theme: Option<&str>) -> Option<String> {
    if !replaces_kde_scheme(current) {
        return None;
    }
    let mut globals = current.to_string();
    let mut section = String::new();
    for line in scheme.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed[1..trimmed.len() - 1].to_string();
        } else if let Some((key, value)) = trimmed.split_once('=')
            && !(section == "General" && key == "Name")
        {
            globals = set_ini(&globals, &section, key, value);
        }
    }
    globals = set_ini(&globals, "General", "font", &export::qt_font(family, QT_FONT_POINTS, Qt::Five).replace('"', ""));
    if let Some(icons) = icon_theme {
        globals = set_ini(&globals, "Icons", "Theme", icons);
    }
    Some(globals)
}

fn write_kde(config: &Path, data: &Path, tokens: &Tokens, family: &str, icon_theme: Option<&str>) {
    let scheme = export::kde_color_scheme(tokens, KDE_SCHEME);
    write(&data.join("color-schemes").join(format!("{KDE_SCHEME}.colors")), &scheme);
    let globals_path = config.join("kdeglobals");
    let Some(current) = user_file(&globals_path) else { return };
    if let Some(globals) = kde_globals(&current, &scheme, family, icon_theme) {
        write(&globals_path, &globals);
    } else {
        tracing::debug!("kdeglobals names another colour scheme; leaving it alone");
    }
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
    let shared = crate::spawn::run_with_timeout(
        "flatpak",
        &["override", "--user", "--filesystem=xdg-config/gtk-4.0:ro", "--filesystem=xdg-config/gtk-3.0:ro"],
        crate::system_info::APPEARANCE_TIMEOUT,
    );
    if shared.is_some_and(|output| output.status.success()) {
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
    let Some(current) = user_file(&manager) else { return };
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
    fn a_gtk_css_that_cannot_be_read_as_text_is_left_untouched() {
        let dir = std::env::temp_dir().join(format!("glide-theme-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir is writable");
        let main = dir.join("gtk.css");
        let latin1 = b"/* caf\xe9 */\nwindow { color: red; }\n";
        std::fs::write(&main, latin1).expect("seed");
        write_gtk(&dir, "window {}");
        assert_eq!(std::fs::read(&main).expect("still there"), latin1, "the user's CSS survives");
        assert!(user_file(&dir.join("missing.css")).is_some_and(|text| text.is_empty()));
        let _ = std::fs::remove_dir_all(&dir);
    }

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
    fn a_new_key_joins_its_section_above_the_blank_line_that_ends_it() {
        let text = "[Appearance]\nstyle=Breeze\n\n[Fonts]\nfixed=Mono\n\n";
        let text = set_ini(text, "Appearance", "icon_theme", "Papirus");
        assert_eq!(text, "[Appearance]\nstyle=Breeze\nicon_theme=Papirus\n\n[Fonts]\nfixed=Mono\n\n");
        let text = set_ini(&text, "Fonts", "general", "Sans");
        assert_eq!(text, "[Appearance]\nstyle=Breeze\nicon_theme=Papirus\n\n[Fonts]\nfixed=Mono\ngeneral=Sans\n\n");
        assert_eq!(set_ini("", "Settings", "a", "1"), "[Settings]\na=1\n");
        assert_eq!(set_ini("[Settings]\n", "Settings", "a", "1"), "[Settings]\na=1\n");
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
        let fresh = qtct_conf("", "/s/Glide.conf", ("Inter", Qt::Six), Some("Papirus"), false);
        assert_eq!(ini_value(&fresh, "Appearance", "standard_dialogs"), Some("xdgdesktopportal"));
        assert_eq!(ini_value(&fresh, "Interface", "buttonbox_layout"), Some(QT_GNOME_BUTTON_LAYOUT));
        assert_eq!(ini_value(&fresh, "Appearance", "style"), Some("Fusion"));
        let own = "[Appearance]\nstandard_dialogs=kde\nstyle=Breeze\n\n[Interface]\nbuttonbox_layout=2\n";
        let kept = qtct_conf(own, "/s/Glide.conf", ("Inter", Qt::Six), None, true);
        assert_eq!(ini_value(&kept, "Appearance", "standard_dialogs"), Some("kde"));
        assert_eq!(ini_value(&kept, "Appearance", "style"), Some("Breeze"));
        assert_eq!(ini_value(&kept, "Interface", "buttonbox_layout"), Some("2"));
        assert_eq!(ini_value(&kept, "Appearance", "icon_theme"), None);
    }

    #[test]
    fn a_qtct_palette_font_and_icons_the_user_chose_survive_an_apply() {
        let scheme = "/home/me/.config/qt6ct/colors/Glide.conf";
        let font = export::qt_font("Inter", QT_FONT_POINTS, Qt::Six);
        let fresh = qtct_conf("", scheme, ("Inter", Qt::Six), Some("Papirus"), false);
        assert_eq!(ini_value(&fresh, "Appearance", "color_scheme_path"), Some(scheme));
        assert_eq!(ini_value(&fresh, "Appearance", "custom_palette"), Some("true"));
        assert_eq!(ini_value(&fresh, "Appearance", "icon_theme"), Some("Papirus"));
        assert_eq!(ini_value(&fresh, "Fonts", "general"), Some(font.as_str()));
        let again = qtct_conf(&fresh, scheme, ("Noto Sans", Qt::Six), Some("Colloid"), false);
        assert_eq!(ini_value(&again, "Appearance", "icon_theme"), Some("Colloid"), "ours, so it follows Glide");
        assert_eq!(ini_value(&again, "Fonts", "general"), Some(export::qt_font("Noto Sans", 11, Qt::Six).as_str()));

        let theirs = "[Appearance]\ncolor_scheme_path=/usr/share/qt6ct/colors/darker.conf\ncustom_palette=true\n\
                      icon_theme=breeze\n\n[Fonts]\ngeneral=\"Fira Sans,10,-1,5,50,0,0,0,0,0\"\n";
        let kept = qtct_conf(theirs, scheme, ("Inter", Qt::Six), Some("Papirus"), false);
        assert_eq!(ini_value(&kept, "Appearance", "color_scheme_path"), Some("/usr/share/qt6ct/colors/darker.conf"));
        assert_eq!(ini_value(&kept, "Appearance", "icon_theme"), Some("breeze"));
        assert_eq!(ini_value(&kept, "Fonts", "general"), Some("\"Fira Sans,10,-1,5,50,0,0,0,0,0\""));

        let inactive = theirs.replace("custom_palette=true", "custom_palette=false");
        let adopted = qtct_conf(&inactive, scheme, ("Inter", Qt::Six), None, false);
        assert_eq!(ini_value(&adopted, "Appearance", "color_scheme_path"), Some(scheme), "an unused palette is free");
        assert!(is_glide_qt_font(&font, QT_FONT_POINTS) && !is_glide_qt_font("\"Fira Sans,10\"", QT_FONT_POINTS));
    }

    #[test]
    fn qt5ct_gets_a_font_string_qt5_parses_and_a_qt6_one_written_before_is_still_ours() {
        let qt6 = qtct_conf("", "/s/Glide.conf", ("Inter", Qt::Six), None, false);
        let qt5 = qtct_conf(&qt6, "/s/Glide.conf", ("Inter", Qt::Five), None, false);
        assert_eq!(ini_value(&qt5, "Fonts", "general"), Some("\"Inter,11,-1,5,50,0,0,0,0,0\""));
        let globals = kde_globals("", "[General]\nColorScheme=Glide\n", "Inter", None).expect("nothing chosen yet");
        assert_eq!(
            ini_value(&globals, "General", "font"),
            Some("Inter,11,-1,5,50,0,0,0,0,0"),
            "Plasma 5 and 6 read it"
        );
    }

    #[test]
    fn a_plasma_users_colour_scheme_keeps_kdeglobals_untouched() {
        let scheme = "[Colors:Window]\nBackgroundNormal=1,2,3\n\n[General]\nColorScheme=Glide\nName=Glide\n";
        let fresh = kde_globals("", scheme, "Inter", Some("Papirus")).expect("nothing chosen yet");
        assert_eq!(ini_value(&fresh, "General", "ColorScheme"), Some("Glide"));
        assert_eq!(ini_value(&fresh, "General", "Name"), None, "the scheme's own name is not a kdeglobals key");
        assert_eq!(ini_value(&fresh, "Icons", "Theme"), Some("Papirus"));
        assert!(kde_globals(&fresh, scheme, "Inter", None).is_some(), "Glide's own scheme is refreshed");
        for stock in ["BreezeLight", "BreezeDark", "BreezeClassic"] {
            let globals = format!("[General]\nColorScheme={stock}\n");
            assert!(kde_globals(&globals, scheme, "Inter", None).is_some(), "{stock}");
        }
        let plasma = "[General]\nColorScheme=Nordic\nfont=Noto Sans,10\n\n[Icons]\nTheme=breeze-dark\n";
        assert_eq!(kde_globals(plasma, scheme, "Inter", Some("Papirus")), None);
    }

    #[test]
    fn a_users_own_gtk_theme_is_left_alone() {
        let installed = Support { adw_gtk3: true, ..Support::default() };
        assert_eq!(gtk3_theme("'Orchis-Dark'", Appearance::Dark, installed), None);
        assert_eq!(gtk3_theme("'Adwaita-dark'", Appearance::Dark, installed).as_deref(), Some("adw-gtk3-dark"));
        assert_eq!(gtk3_theme("'Adwaita-dark'", Appearance::Dark, Support::default()), None);
    }
}

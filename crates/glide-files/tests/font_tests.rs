use iced::advanced::graphics::text::cosmic_text::fontdb::Family;
use iced::advanced::graphics::text::font_system;

#[test]
fn files_text_uses_the_shell_family() {
    let chosen = glide_files::theme::use_ui_family(Some("No Such Family 42"));
    let mut system = font_system().write().expect("font system");
    let raw = system.raw();
    let installed: Vec<String> =
        raw.db().faces().flat_map(|face| face.families.iter().map(|(name, _)| name.clone())).collect();
    let shell = glide_theme::font::ui_family(None, installed.iter().map(String::as_str));
    assert_eq!(chosen, shell, "an unknown family falls back the way the shell does");
    assert_eq!(raw.db().family_name(&Family::SansSerif), chosen, "default and semibold text resolve to it");
}

#[test]
fn semibold_text_stays_in_the_ui_family() {
    use iced::advanced::graphics::text::cosmic_text::{Attrs, Buffer, Family as Generic, Metrics, Shaping, Weight};
    let chosen = glide_files::theme::use_ui_family(None);
    let mut system = font_system().write().expect("font system");
    let raw = system.raw();
    let installed = raw.db().faces().any(|face| face.families.iter().any(|(name, _)| *name == chosen));
    if !installed {
        return;
    }
    for weight in [Weight::NORMAL, Weight::SEMIBOLD] {
        let mut buffer = Buffer::new(raw, Metrics::new(14.0, 18.0));
        buffer.set_text(raw, "Reports", &Attrs::new().family(Generic::SansSerif).weight(weight), Shaping::Basic, None);
        let ids: Vec<_> = buffer.layout_runs().flat_map(|run| run.glyphs.iter().map(|glyph| glyph.font_id)).collect();
        for id in ids {
            let face = raw.db().face(id).expect("face");
            assert!(
                face.families.iter().any(|(name, _)| *name == chosen),
                "weight {} drew with {} instead of {chosen}",
                weight.0,
                face.post_script_name
            );
        }
    }
}

#[test]
fn the_font_family_is_read_from_the_general_section() {
    let look = glide_files::config::Look::parse_with("[general]\nfont-family = \"Inter\"\n", false);
    assert_eq!(look.font_family.as_deref(), Some("Inter"));
    let blank = glide_files::config::Look::parse_with("[general]\nfont-family = \"  \"\n", false);
    assert_eq!(blank.font_family, None);
}

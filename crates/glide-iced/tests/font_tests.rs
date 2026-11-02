use glide_iced::font::{revision, use_ui_family};
use iced::advanced::graphics::text::font_system;

#[test]
fn the_same_family_keeps_every_shaping_cache_and_a_live_change_reshapes_iced_paragraphs() {
    let family = use_ui_family(None);
    let settled = revision();
    assert_eq!(use_ui_family(None), family);
    assert_eq!(use_ui_family(Some(&family)), family);
    assert_eq!(revision(), settled);
    let other = font_system()
        .write()
        .expect("font system")
        .raw()
        .db()
        .faces()
        .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
        .find(|name| *name != family);
    let Some(other) = other else { return };
    let before = font_system().read().expect("font system").version();
    assert_eq!(use_ui_family(Some(&other)), other);
    assert!(revision() > settled);
    assert!(
        font_system().read().expect("font system").version() > before,
        "iced's own paragraphs compare this version before reusing their shaping"
    );
}

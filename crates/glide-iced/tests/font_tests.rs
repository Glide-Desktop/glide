use glide_iced::font::{revision, use_ui_family};

#[test]
fn choosing_the_family_already_in_use_keeps_every_shaping_cache() {
    let family = use_ui_family(None);
    let settled = revision();
    assert_eq!(use_ui_family(None), family);
    assert_eq!(use_ui_family(Some(&family)), family);
    assert_eq!(revision(), settled);
}

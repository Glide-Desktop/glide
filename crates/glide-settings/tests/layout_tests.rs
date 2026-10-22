use glide_settings::icons::Icon;
use glide_settings::panels::network::expands;
use glide_settings::panels::sound::device_icon;
use glide_settings::services::network::AccessPoint;
use glide_settings::theme::{
    self, Accent, Appearance, CONTROL_HEIGHT, PAGE_PADDING_TOP, PAGE_PADDING_X, R2, R3, RING_GAP, ROW_PADDING_X,
    SECTION_GAP, SMALL_CONTROL_HEIGHT, ThemeState,
};
use glide_settings::{Lang, Panel};

fn luma(c: iced::Color) -> f32 {
    c.r * 0.299 + c.g * 0.587 + c.b * 0.114
}

fn over(base: iced::Color, top: iced::Color) -> iced::Color {
    theme::blend_state_layer(base, top, 1.0)
}

fn both() -> [ThemeState; 2] {
    [ThemeState::new(Appearance::Light, Accent::Azurit), ThemeState::new(Appearance::Dark, Accent::Azurit)]
}

#[test]
fn card_shadow_is_the_spec_el_1_and_never_glows_lighter_than_the_card() {
    for t in both() {
        let card = theme::card_style(&t);
        let shadow = card.shadow;
        let spec = glide_theme::elevation::layers(glide_theme::elevation::Level::El1, t.appearance().into())[0];
        assert!((shadow.blur_radius - spec.blur).abs() < f32::EPSILON);
        assert!((shadow.offset.y - spec.dy).abs() < f32::EPSILON);
        assert!(
            luma(over(t.card_bg(), shadow.color)) <= luma(t.card_bg()),
            "{:?}: a card shadow must darken, not draw a light rim under the card",
            t.appearance()
        );
    }
}

#[test]
fn every_card_is_a_hairline_r3_card() {
    for t in both() {
        let card = theme::card_style(&t);
        assert_eq!(card.border.color, t.line());
        assert!((card.border.width - 1.0).abs() < f32::EPSILON);
        assert_eq!(card.border.radius, theme::radius_r3());
    }
}

#[test]
fn inner_radii_are_concentric_with_the_card() {
    assert!((R3 - RING_GAP - R2).abs() < f32::EPSILON, "a selected thumbnail sits RING_GAP inside its ring");
}

#[test]
fn page_rhythm_uses_only_spec_spacing_steps() {
    for value in [SECTION_GAP, PAGE_PADDING_X, PAGE_PADDING_TOP, ROW_PADDING_X, RING_GAP] {
        assert!(glide_theme::spacing::STEPS.contains(&(value as i32)), "{value} is not a Meridian spacing step");
    }
    assert!(
        ((CONTROL_HEIGHT - SMALL_CONTROL_HEIGHT) / 2.0 - 3.0).abs() < f32::EPSILON,
        "the segmented track pads a small control up to a full control"
    );
}

#[test]
fn the_active_segment_stands_out_from_its_track_in_both_themes() {
    for t in both() {
        let track = over(t.card_bg(), t.fill_1());
        let thumb = over(track, theme::to_iced_color(t.tokens().segment_thumb()));
        assert!((luma(thumb) - luma(track)).abs() > 0.02, "{:?}: the selected segment is invisible", t.appearance());
    }
}

#[test]
fn every_panel_has_a_page_description_in_both_languages() {
    for panel in Panel::ALL {
        for lang in [Lang::Uk, Lang::En] {
            assert!(!panel.description(lang).trim().is_empty(), "{panel:?}");
            assert_ne!(panel.description(lang), panel.label(lang));
        }
    }
}

#[test]
fn only_networks_that_need_input_show_a_disclosure_chevron() {
    let point = |in_use, secured, known| AccessPoint {
        ssid: String::from("home"),
        signal: 80,
        secured,
        in_use,
        known,
        profile: None,
    };
    assert!(expands(&point(true, false, true)));
    assert!(expands(&point(false, true, false)));
    assert!(!expands(&point(false, true, true)));
    assert!(!expands(&point(false, false, false)));
}

#[test]
fn sidebar_tiles_are_twenty_four_pixels_on_the_spacing_scale() {
    use glide_settings::sidebar::{EDGE, GROUP_GAP, TILE_SIZE};
    assert!((TILE_SIZE - 24.0).abs() < f32::EPSILON);
    for value in [GROUP_GAP, EDGE] {
        assert!(glide_theme::spacing::STEPS.contains(&(value as i32)), "{value} is not a Meridian spacing step");
    }
}

#[test]
fn every_sidebar_tile_has_its_own_colour() {
    for (index, panel) in Panel::ALL.iter().enumerate() {
        for other in &Panel::ALL[index + 1..] {
            assert_ne!(panel.color(), other.color(), "{panel:?} and {other:?}");
        }
    }
}

#[test]
fn key_names_render_as_keycap_symbols() {
    use glide_settings::widgets::key_symbol;
    assert_eq!(key_symbol("Super"), "⌘");
    assert_eq!(key_symbol("shift"), "⇧");
    assert_eq!(key_symbol("Ctrl"), "Ctrl");
    assert_eq!(key_symbol("Return"), "↵");
    assert_eq!(key_symbol("q"), "Q");
    assert_eq!(key_symbol("XF86AudioMute"), "Mute");
    assert_eq!(key_symbol(" Left "), "←");
    assert_eq!(key_symbol("Tab"), "⇥", "the shortcuts list and multitasking draw Tab the same way");
    assert_eq!(key_symbol("Page_Down"), "PgDn");
    assert_eq!(key_symbol("Next"), "PgDn");
}

#[test]
fn search_highlights_the_matched_span_in_either_script() {
    use glide_settings::sidebar::highlight;
    assert_eq!(highlight("Displays", "play"), Some((3, 7)));
    let label = "Миша й тачпад";
    let (start, end) = highlight(label, "ТАЧ").expect("cyrillic match");
    assert_eq!(&label[start..end], "тач");
    assert_eq!(highlight("Sound", "  "), None);
    assert_eq!(highlight("Sound", "wallpaper"), None);
}

#[test]
fn result_counts_use_ukrainian_plural_forms() {
    use glide_settings::sidebar::results_label;
    assert_eq!(results_label(Lang::En, 1), "1 result");
    assert_eq!(results_label(Lang::En, 3), "3 results");
    assert_eq!(results_label(Lang::Uk, 1), "1 результат");
    assert_eq!(results_label(Lang::Uk, 3), "3 результати");
    assert_eq!(results_label(Lang::Uk, 5), "5 результатів");
    assert_eq!(results_label(Lang::Uk, 12), "12 результатів");
    assert_eq!(results_label(Lang::Uk, 21), "21 результат");
}

#[test]
fn audio_devices_get_a_glyph_that_matches_their_kind() {
    assert_eq!(device_icon("WH-1000XM4 Headphones", Icon::Volume), Icon::Headphones);
    assert_eq!(device_icon("Rembrandt HDMI / DisplayPort 1 Output", Icon::Volume), Icon::Display);
    assert_eq!(device_icon("Ryzen HD Audio Controller Speaker", Icon::Volume), Icon::Volume);
    assert_eq!(device_icon("Digital Microphone", Icon::Mic), Icon::Mic);
}

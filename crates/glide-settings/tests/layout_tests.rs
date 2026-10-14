use glide_settings::icons::Icon;
use glide_settings::panels::network::expands;
use glide_settings::panels::sound::device_icon;
use glide_settings::services::network::AccessPoint;
use glide_settings::theme::{
    self, Accent, Appearance, CONTROL_HEIGHT, GROUP_INSET, PAGE_PADDING_TOP, PAGE_PADDING_X, R2, R3, RING_GAP,
    ROW_MIN_HEIGHT, ROW_PADDING_X, SECTION_GAP, SMALL_CONTROL_HEIGHT, SWITCH_PADDING_RATIO, SWITCH_SIZE, ThemeState,
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
fn every_boxed_list_uses_one_card_style() {
    for t in both() {
        let card = theme::card_style(&t);
        assert_eq!(theme::group_style(&t), card);
        assert_eq!(card.border.color, t.line());
        assert!((card.border.width - 1.0).abs() < f32::EPSILON);
        assert_eq!(card.border.radius, theme::radius_r3());
    }
}

#[test]
fn a_row_fits_one_control_exactly_and_is_forty_four_pixels_tall() {
    assert!((ROW_MIN_HEIGHT - 44.0).abs() < f32::EPSILON);
    assert!((theme::row_content_height() - CONTROL_HEIGHT).abs() < f32::EPSILON);
}

#[test]
fn inner_radii_are_concentric_with_the_card() {
    assert!((R3 - GROUP_INSET - R2).abs() < f32::EPSILON, "a row's hover layer sits GROUP_INSET inside the card");
    assert!((R3 - RING_GAP - R2).abs() < f32::EPSILON, "a selected thumbnail sits RING_GAP inside its ring");
}

#[test]
fn page_rhythm_uses_only_spec_spacing_steps() {
    for value in [SECTION_GAP, PAGE_PADDING_X, PAGE_PADDING_TOP, GROUP_INSET, ROW_PADDING_X, RING_GAP] {
        assert!(glide_theme::spacing::STEPS.contains(&(value as i32)), "{value} is not a Meridian spacing step");
    }
    assert!(
        ((CONTROL_HEIGHT - SMALL_CONTROL_HEIGHT) / 2.0 - 3.0).abs() < f32::EPSILON,
        "the segmented track pads a small control up to a full control"
    );
    assert!(
        (SWITCH_PADDING_RATIO * SWITCH_SIZE).fract().abs() < f32::EPSILON,
        "the switch thumb keeps a whole-pixel margin inside its track"
    );
}

#[test]
fn the_active_segment_stands_out_from_its_track_in_both_themes() {
    for t in both() {
        let track = over(t.card_bg(), t.fill_1());
        let thumb = over(track, theme::segment_thumb(&t));
        assert!((luma(thumb) - luma(track)).abs() > 0.02, "{:?}: the selected segment is invisible", t.appearance());
    }
}

#[test]
fn the_search_field_is_a_filled_well_that_gains_an_accent_border_on_focus() {
    use iced::widget::text_input::Status;
    for t in both() {
        let idle = theme::search_field_style(&t, Status::Active);
        assert_eq!(idle.background, iced::Background::Color(t.fill_1()));
        assert_eq!(idle.border.color, iced::Color::TRANSPARENT);
        let focused = theme::search_field_style(&t, Status::Focused { is_hovered: false });
        assert_eq!(focused.border.color, t.acc_bg());
        assert_eq!(focused.background, iced::Background::Color(t.view_bg()));
    }
}

#[test]
fn selection_rings_follow_the_spec() {
    for t in both() {
        let on = theme::swatch_ring_style(&t, true);
        let off = theme::swatch_ring_style(&t, false);
        assert_eq!(on.border.color, t.fg_1());
        assert_eq!(off.border.color, iced::Color::TRANSPARENT);
        assert!(
            (on.border.width - off.border.width).abs() < f32::EPSILON,
            "selecting a swatch must not shift its neighbours"
        );
        let frame = theme::preview_frame_style(&t, true);
        assert_eq!(frame.border.color, t.acc_bg());
        assert_eq!(frame.border.radius, theme::radius_r3());
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
fn sidebar_rows_are_thirty_two_pixels_with_centred_twenty_four_pixel_tiles() {
    use glide_settings::sidebar::{EDGE, GROUP_GAP, ITEM_HEIGHT, TILE_SIZE};
    assert!((ITEM_HEIGHT - 32.0).abs() < f32::EPSILON);
    assert!((TILE_SIZE - 24.0).abs() < f32::EPSILON);
    for value in [(ITEM_HEIGHT - TILE_SIZE) / 2.0, GROUP_GAP, EDGE] {
        assert!(glide_theme::spacing::STEPS.contains(&(value as i32)), "{value} is not a Meridian spacing step");
    }
    const {
        assert!(
            theme::SIDEBAR_INDICATOR_HEIGHT <= ITEM_HEIGHT - 2.0 * R2,
            "the indicator must sit between the item's rounded corners"
        );
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
fn the_selected_sidebar_item_is_acc_soft_with_an_accent_indicator() {
    use iced::widget::button::Status;
    for t in both() {
        let selected = theme::sidebar_item_style(&t, true, Status::Active);
        assert_eq!(selected.background, Some(iced::Background::Color(t.acc_soft())));
        assert_eq!(selected.text_color, t.acc_soft_fg());
        let idle = theme::sidebar_item_style(&t, false, Status::Active);
        assert_eq!(idle.text_color, t.fg_1());
        let hovered = theme::sidebar_item_style(&t, false, Status::Hovered);
        assert_eq!(
            hovered.background,
            Some(iced::Background::Color(theme::hover_layer(iced::Color::TRANSPARENT, t.state())))
        );
        assert_eq!(theme::sidebar_indicator_style(&t, true).background, Some(iced::Background::Color(t.acc_bg())));
        assert_eq!(
            theme::sidebar_indicator_style(&t, false).background,
            Some(iced::Background::Color(iced::Color::TRANSPARENT))
        );
    }
}

#[test]
fn the_account_card_is_a_hairline_card_that_turns_acc_soft_when_selected() {
    use iced::widget::button::Status;
    for t in both() {
        let idle = theme::account_card_style(&t, false, Status::Active);
        assert_eq!(idle.border.color, t.line());
        assert_eq!(idle.border.radius, theme::radius_r3());
        assert_eq!(idle.background, Some(iced::Background::Color(t.card_bg())));
        let selected = theme::account_card_style(&t, true, Status::Active);
        assert_eq!(selected.background, Some(iced::Background::Color(t.acc_soft())));
    }
}

#[test]
fn tile_corners_grow_with_the_tile_and_stay_on_the_radius_scale() {
    assert_eq!(theme::tile_radius(24.0), theme::radius_r2());
    assert_eq!(theme::tile_radius(theme::PAGE_TILE_SIZE), theme::radius_r3());
    assert_eq!(theme::tile_radius(72.0), theme::radius_r4());
}

#[test]
fn selectable_cards_keep_their_ring_on_the_card_radius() {
    use iced::widget::button::Status;
    for t in both() {
        let on = theme::selectable_card_style(&t, true, Status::Active);
        let off = theme::selectable_card_style(&t, false, Status::Active);
        assert_eq!(on.border.color, t.acc_bg());
        assert_eq!(off.border.color, t.line());
        assert_eq!(on.border.radius, theme::radius_r3());
        let ring = theme::ring_button_style(&t, true, Status::Active);
        assert!((ring.border.width - theme::RING_WIDTH).abs() < f32::EPSILON);
        assert!(
            (theme::ring_button_style(&t, false, Status::Active).border.width - ring.border.width).abs() < f32::EPSILON,
            "selecting a preview must not shift its neighbours"
        );
    }
}

#[test]
fn tones_pair_a_soft_background_with_a_strong_foreground() {
    use glide_settings::theme::Tone;
    for t in both() {
        assert_eq!(theme::tone_colors(&t, Tone::Ok), (t.ok_soft(), t.ok()));
        assert_eq!(theme::tone_colors(&t, Tone::Accent), (t.acc_soft(), t.acc_soft_fg()));
        assert_eq!(theme::tone_colors(&t, Tone::Danger), (t.danger_soft(), t.danger()));
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
    assert_eq!(key_symbol("XF86AudioMute"), "AudioMute");
    assert_eq!(key_symbol(" Left "), "←");
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

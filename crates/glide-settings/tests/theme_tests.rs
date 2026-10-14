use glide_settings::theme::{
    Accent as SettingsAccent, R_ROUND, R1, R2, R3, ThemeState, button_ghost, button_primary, button_secondary,
    card_style, content_container_style, radius_r1, radius_r2, radius_r3, radius_round, sidebar_item_style,
    slider_style, text_input_style, toggle_style, window_container_style,
};
use glide_theme::{Accent, Appearance, Color, Tokens};
use iced::widget::{button, slider, text_input, toggler};

fn luminance(c: Color) -> f32 {
    let lin = |v: f32| {
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c.0[0]) + 0.7152 * lin(c.0[1]) + 0.0722 * lin(c.0[2])
}

fn contrast(a: Color, b: Color) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn luma(c: Color) -> f32 {
    c.0[0] * 0.299 + c.0[1] * 0.587 + c.0[2] * 0.114
}

#[test]
fn widget_styles_use_only_meridian_radii() {
    const { assert!(R1 < R2 && R2 < R3 && R3 < R_ROUND) };
    let theme = ThemeState::new(Appearance::Dark, Accent::Blue);
    let active = button::Status::Active;
    assert_eq!(card_style(&theme).border.radius, radius_r3());
    for style in [button_primary, button_secondary, button_ghost] {
        assert_eq!(style(&theme, active).border.radius, radius_r2());
    }
    assert_eq!(text_input_style(&theme, text_input::Status::Active).border.radius, radius_r2());
    let on = toggler::Status::Active { is_toggled: true };
    assert_eq!(toggle_style(&theme, on).border_radius, Some(radius_round()));
    assert_eq!(slider_style(&theme, slider::Status::Active).rail.border.radius, radius_r1());
    assert_eq!(sidebar_item_style(&theme, false, active).border.radius, radius_r2());
    for style in [window_container_style, content_container_style] {
        assert_eq!(style(&theme).border.radius, iced::border::Radius::default());
    }
}

#[test]
fn accents_are_distinct_with_monotonic_ramps_and_readable_text() {
    for (index, &accent) in Accent::ALL.iter().enumerate() {
        let ramp = accent.ramp();
        assert!(ramp.windows(2).all(|pair| luma(pair[0]) > luma(pair[1])));
        let light = Tokens::new(Appearance::Light, accent);
        let dark = Tokens::new(Appearance::Dark, accent);
        assert!(contrast(light.acc_fg, light.acc_bg) >= 3.0, "{accent:?} fill");
        assert!(contrast(light.acc_text, light.view_bg) >= 4.5, "{accent:?} light text");
        assert!(contrast(dark.acc_text, dark.view_bg) >= 4.5, "{accent:?} dark text");
        assert!(contrast(light.acc_soft_fg, light.acc_soft) >= 4.5, "{accent:?} pill");
        for &other in &Accent::ALL[index + 1..] {
            assert_ne!(dark.acc_bg, Tokens::new(Appearance::Dark, other).acc_bg);
        }
    }
}

#[test]
fn surfaces_and_text_form_a_readable_hierarchy() {
    let light = Tokens::new(Appearance::Light, Accent::Blue);
    let [view, card, win, sidebar, sunken] =
        [light.view_bg, light.card_bg, light.win_bg, light.sidebar_bg, light.sunken_bg].map(luma);
    assert!((view - card).abs() < 1e-4 && card > win && win - sidebar >= 0.02 && sidebar > sunken);

    let dark = Tokens::new(Appearance::Dark, Accent::Blue);
    let ladder = [dark.sunken_bg, dark.view_bg, dark.win_bg, dark.sidebar_bg, dark.card_bg, dark.popover_bg].map(luma);
    assert!(ladder[0] < ladder[1]);
    assert!(ladder[1..].windows(2).all(|pair| pair[1] - pair[0] >= 0.015));

    for t in [light, dark] {
        assert!(t.fg_1.0[3] >= t.fg_2.0[3] && t.fg_2.0[3] > t.fg_3.0[3] && t.fg_3.0[3] > t.fg_4.0[3]);
        assert!(contrast(t.fg_1, t.card_bg) >= 7.0);
        assert!(contrast(t.fg_2, t.card_bg) >= 4.5);
        assert!(t.line_2.0[3] > t.line.0[3] && t.fill_1.0[3] < t.fill_2.0[3] && t.fill_2.0[3] < t.fill_3.0[3]);
    }
}

#[test]
fn switching_theme_changes_accent_tokens_only_and_round_trips() {
    let mut theme = ThemeState::new(Appearance::Light, Accent::Blue);
    let initial = *theme.tokens();
    assert_eq!(initial.win_bg, Color::from_hex("#f3f2ef").unwrap());
    theme.set_appearance(Appearance::Dark);
    assert_eq!(theme.tokens().win_bg, Color::from_hex("#1c1b1e").unwrap());
    assert_eq!(theme.tokens().acc_bg_hover, theme.tokens().ramp[2]);
    let neutral = (theme.tokens().win_bg, theme.tokens().card_bg, theme.tokens().fg_1);
    for &accent in Accent::ALL {
        theme.set_accent(accent);
        assert_eq!(theme.tokens().acc_bg, accent.ramp()[3]);
        assert_eq!((theme.tokens().win_bg, theme.tokens().card_bg, theme.tokens().fg_1), neutral);
    }
    theme.set_accent(SettingsAccent::Amethyst);
    theme.set_appearance(Appearance::Light);
    theme.set_accent(Accent::Blue);
    assert_eq!(*theme.tokens(), initial);
}

#[test]
fn every_settings_accent_has_names_and_an_opaque_sample() {
    for accent in SettingsAccent::ALL {
        assert!(!accent.name().is_empty() && !accent.name_uk().is_empty());
        assert!(accent.primary_hex().starts_with('#'));
        assert!(accent.sample_color().a > 0.99);
    }
}

use glide_settings::app::Message;
use glide_settings::panels::keyboard::shortcut_label;
use glide_settings::panels::{appearance, displays, keyboard, mouse, region};
use glide_settings::services::config::ConfigStore;
use glide_settings::services::input::SWITCH_OPTIONS;
use glide_settings::services::ipc::ReloadError;
use glide_settings::{Accent, App, Appearance, Category, Key, Lang, Panel};

fn app(config: &str) -> App {
    App::with_config(ConfigStore::parse(config))
}

#[test]
fn every_key_is_translated_in_both_languages() {
    for key in Key::ALL {
        assert!(!key.en().trim().is_empty(), "{key:?} has no English text");
        assert!(!key.uk().trim().is_empty(), "{key:?} has no Ukrainian text");
    }
}

#[test]
fn only_micro_labels_may_be_uppercase() {
    for key in Key::ALL {
        for text in [key.en(), key.uk()] {
            let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
            let shouting = letters.len() > 3 && letters.iter().all(|c| c.is_uppercase());
            assert!(!shouting, "{key:?} is written in capitals: {text}");
        }
    }
}

#[test]
fn vpn_and_netbird_share_one_page_while_other_merged_entries_keep_tabs() {
    let app = app("").with_netbird(true);
    assert!(Category::Vpn.stacked());
    assert_eq!(app.category_panels(Category::Vpn), [Panel::Vpn, Panel::Netbird]);
    for category in Category::ALL.iter().filter(|category| **category != Category::Vpn) {
        assert!(!category.stacked(), "{category:?} keeps its tabs");
    }
    let without = self::app("").with_netbird(false);
    assert_eq!(without.category_panels(Category::Vpn), [Panel::Vpn], "no NetBird section without NetBird");
}

#[test]
fn every_panel_has_a_distinct_label_and_sits_in_exactly_one_group() {
    for lang in [Lang::Uk, Lang::En] {
        let mut labels: Vec<&str> = Panel::ALL.iter().map(|p| p.label(lang)).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), Panel::ALL.len());
    }
    for panel in Panel::ALL {
        let hits = Category::ALL.iter().filter(|category| category.panels().contains(panel)).count();
        assert_eq!(hits, 1, "{panel:?} belongs to exactly one sidebar entry");
    }
    for category in Category::ALL {
        let hits = Category::GROUPS.iter().filter(|group| group.categories.contains(category)).count();
        assert_eq!(hits, 1, "{category:?}");
    }
    for lang in [Lang::Uk, Lang::En] {
        let mut labels: Vec<&str> = Category::ALL.iter().map(|category| category.label(lang)).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), Category::ALL.len());
    }
}

#[test]
fn search_matches_labels_in_either_language_and_keywords() {
    assert!(Panel::Network.matches("wi-fi"));
    assert!(Panel::Network.matches("МЕРЕЖ"));
    assert!(Panel::Wallpaper.matches("wallpaper"));
    assert!(Panel::Wallpaper.matches("шпалери"));
    assert!(Panel::Appearance.matches("accent"));
    assert!(Panel::NightLight.matches("night"));
    assert!(Panel::Displays.matches("brightness"));
    assert!(Panel::Power.matches("кришка"));
    assert!(Panel::Sound.matches("гучн"));
    assert!(Panel::Mouse.matches("touchpad"));
    assert!(!Panel::Sound.matches("wallpaper"));
    assert!(Panel::About.matches("   "));
}

#[test]
fn command_line_slugs_open_panels() {
    for panel in Panel::ALL {
        assert_eq!(Panel::from_slug(panel.slug()), Some(*panel));
    }
    assert_eq!(Panel::from_slug("Mouse"), Some(Panel::Mouse));
    assert_eq!(Panel::from_slug("touchpad"), Some(Panel::Mouse));
    assert_eq!(Panel::from_slug("wifi"), Some(Panel::Network));
    assert_eq!(Panel::from_slug("display"), Some(Panel::Displays));
    assert_eq!(Panel::from_slug("blue"), Some(Panel::Bluetooth));
    assert_eq!(Panel::from_slug("x"), None);
}

#[test]
fn search_filters_visible_categories_and_enter_opens_the_first_hit() {
    let mut app = app("");
    let _ = app.update(Message::SearchChanged(String::from("resolution")));
    assert_eq!(app.visible_categories(), vec![Category::Displays]);
    let _ = app.update(Message::SearchSubmitted);
    assert_eq!(app.active_panel(), Panel::Displays);
    let _ = app.update(Message::ClearSearch);
    assert_eq!(app.visible_categories().len(), Category::ALL.len());
}

#[test]
fn a_hit_inside_a_merged_entry_opens_its_tab() {
    let mut app = app("");
    let _ = app.update(Message::SearchChanged(String::from("шпалери")));
    assert_eq!(app.visible_categories(), vec![Category::Appearance]);
    let _ = app.update(Message::SearchSubmitted);
    assert_eq!(app.active_panel(), Panel::Wallpaper);
    let _ = app.update(Message::ClearSearch);
    let _ = app.update(Message::CategorySelected(Category::Desktop));
    assert_eq!(app.active_panel(), Panel::Desktop, "a sidebar click opens the first tab");
}

#[test]
fn the_netbird_tab_is_listed_only_when_netbird_is_installed() {
    let hidden = app("");
    assert_eq!(hidden.category_panels(Category::Vpn), vec![Panel::Vpn]);
    let mut shown = app("").with_netbird(true);
    assert_eq!(shown.category_panels(Category::Vpn), vec![Panel::Vpn, Panel::Netbird]);
    let _ = shown.update(Message::SearchChanged(String::from("exit node")));
    assert_eq!(shown.visible_categories(), vec![Category::Vpn]);
    let _ = shown.update(Message::SearchSubmitted);
    assert_eq!(shown.active_panel(), Panel::Netbird);
    assert_eq!(Panel::from_slug("netbird"), Some(Panel::Netbird), "the quick settings link resolves either way");
}

#[test]
fn search_with_no_hits_leaves_the_active_panel_alone() {
    let mut app = app("");
    let _ = app.update(Message::PanelSelected(Panel::Keyboard));
    let _ = app.update(Message::SearchChanged(String::from("zzzz")));
    assert!(app.visible_categories().is_empty());
    let _ = app.update(Message::SearchSubmitted);
    assert_eq!(app.active_panel(), Panel::Keyboard);
}

#[test]
fn the_overview_opens_first_and_flags_a_settings_file_it_cannot_save() {
    let mut app = app("");
    assert_eq!(app.active_panel(), Panel::Home);
    let page = glide_settings::panels::home::Page::default();
    assert!(page.problems(&ConfigStore::parse("")).is_empty(), "nothing to report before anything was read");
    let broken = ConfigStore::parse("[topbar\n");
    assert_eq!(page.problems(&broken), vec![glide_settings::panels::home::Problem::ReadOnly]);
    let _ = app.update(Message::Home(glide_settings::panels::home::Msg::Open(Panel::Storage)));
    assert_eq!(app.active_panel(), Panel::Storage, "a tile opens its page");
}

#[test]
fn a_second_launch_opens_the_panel_it_names() {
    let mut app = app("");
    let _ = app.open_args(&[String::from("power")]);
    assert_eq!(app.active_panel(), Panel::Power);
    let _ = app.open_args(&[]);
    assert_eq!(app.active_panel(), Panel::Power, "no argument keeps the current panel");
}

#[test]
fn theme_follows_the_config_file() {
    let app = app("[topbar]\nappearance = \"light\"\naccent = \"purple\"\n[general]\nlanguage = \"en\"\n");
    assert_eq!(app.theme.appearance(), Appearance::Light);
    assert_eq!(app.theme.accent(), Accent::Amethyst);
    assert_eq!(app.lang(), Lang::En);
    assert_eq!(app.title(), "Overview — Settings");
}

#[test]
fn the_window_title_names_the_page_on_screen() {
    let mut app = app("[general]\nlanguage = \"uk\"\n");
    let _ = app.open_args(&["sound".to_string()]);
    assert_eq!(app.title(), "Звук — Налаштування");
    let _ = app.open_args(&["netbird".to_string()]);
    assert_eq!(app.title(), "VPN — Налаштування", "a stacked page is titled by its sidebar entry");
}

#[test]
fn appearance_changes_retheme_the_app_immediately() {
    let mut app = app("");
    let _ = app.update(Message::Appearance(appearance::Msg::SetAppearance(Appearance::Light)));
    let _ = app.update(Message::Appearance(appearance::Msg::SetAccent(Accent::Conifer)));
    assert_eq!(app.theme.appearance(), Appearance::Light);
    assert_eq!(app.theme.accent(), Accent::Conifer);
    assert_eq!(app.config().accent(), Accent::Conifer);
}

#[test]
fn a_look_from_the_host_rethemes_the_app_and_falls_back_to_the_stored_language() {
    let mut app = App::with_config(ConfigStore::parse("").with_system_language(Lang::Uk));
    let look = glide_sdk::look::Look::parse("[topbar]\nappearance = \"light\"\n", false);
    app.follow_look(&look, look.paint());
    assert_eq!(app.theme.appearance(), Appearance::Light);
    assert_eq!(app.lang(), Lang::Uk, "no language in the look keeps the system language");
    let english = glide_sdk::look::Look::parse("[general]\nlanguage = \"en\"\n", false);
    app.follow_look(&english, english.paint());
    assert_eq!(app.lang(), Lang::En);
}

#[test]
fn language_switch_changes_the_interface_language() {
    let mut app = App::with_config(ConfigStore::parse("").with_system_language(Lang::Uk));
    assert_eq!(app.lang(), Lang::Uk);
    let _ = app.update(Message::Region(region::Msg::Interface(region::LangChoice::Explicit(Lang::En))));
    assert_eq!(app.lang(), Lang::En);
    assert_eq!(app.config().language(), Lang::En);
    assert_eq!(app.config().language_choice(), Some(Lang::En));
}

#[test]
fn the_layout_scope_is_written_only_when_it_is_not_global() {
    let mut app = app("");
    let (choices, current) = keyboard::input_choices(Lang::En, app.config(), keyboard::LAYOUT_SCOPE, &keyboard::SCOPES);
    assert_eq!(current.value, "global");
    let window = choices.iter().find(|choice| choice.value == "window").cloned().expect("window is one of the scopes");
    let _ = app.update(Message::Keyboard(keyboard::Msg::Pick(window)));
    assert_eq!(app.config().get_str("input", "layout-scope"), Some("window"));
    let _ = app.update(Message::Keyboard(keyboard::Msg::Pick(choices[0].clone())));
    assert!(!app.config().to_text().contains("layout-scope"));
}

#[test]
fn an_unset_language_follows_the_system_locale() {
    for system in [Lang::Uk, Lang::En] {
        let app = App::with_config(ConfigStore::parse("").with_system_language(system));
        assert_eq!(app.lang(), system);
        assert_eq!(app.config().language_choice(), None);
    }
}

#[test]
fn picking_system_removes_the_language_key() {
    let mut app = App::with_config(ConfigStore::parse("[general]\nlanguage = \"uk\"\n").with_system_language(Lang::En));
    assert_eq!(app.lang(), Lang::Uk);
    let (_, current) = region::LangChoice::options(app.config());
    assert_eq!(current, region::LangChoice::Explicit(Lang::Uk));
    let _ = app.update(Message::Region(region::Msg::Interface(region::LangChoice::System {
        detected: Lang::En,
        ui: Lang::Uk,
    })));
    assert_eq!(app.config().language_choice(), None);
    assert!(!app.config().to_text().contains("language"));
    assert_eq!(app.lang(), Lang::En);
}

#[test]
fn the_system_choice_names_the_detected_language() {
    let config = ConfigStore::parse("").with_system_language(Lang::Uk);
    let (options, current) = region::LangChoice::options(&config);
    assert_eq!(options.len(), 3);
    assert_eq!(current, options[0]);
    assert_eq!(current.lang(), None);
    assert_eq!(current.to_string(), "Системна (Українська)");
    let english = ConfigStore::parse("").with_system_language(Lang::En);
    let (_, current) = region::LangChoice::options(&english);
    assert_eq!(current.to_string(), "System (English)");
}

#[test]
fn keyboard_sources_can_be_added_reordered_and_removed_but_never_emptied() {
    let mut app = app("[input]\nxkb-layout = \"us,ua\"\n");
    let _ = app.update(Message::Keyboard(keyboard::Msg::Add(glide_settings::services::input::Layout {
        code: String::from("pl"),
        name: String::from("Polish"),
    })));
    assert_eq!(app.config().xkb_layout(), "us,ua,pl");
    let _ = app.update(Message::Keyboard(keyboard::Msg::MoveUp(2)));
    assert_eq!(app.config().xkb_layout(), "us,pl,ua");
    let _ = app.update(Message::Keyboard(keyboard::Msg::MoveDown(0)));
    assert_eq!(app.config().xkb_layout(), "pl,us,ua");
    for _ in 0..5 {
        let _ = app.update(Message::Keyboard(keyboard::Msg::Remove(0)));
    }
    assert_eq!(app.config().xkb_layout(), "ua");
}

#[test]
fn repeat_sliders_only_write_on_release() {
    let mut app = app("");
    let _ = app.update(Message::Keyboard(keyboard::Msg::Delay(450.0)));
    assert_eq!(app.config().repeat_delay(), 200);
    let _ = app.update(Message::Keyboard(keyboard::Msg::CommitRepeat));
    assert_eq!(app.config().repeat_delay(), 450);
    let _ = app.update(Message::Keyboard(keyboard::Msg::ResetRepeat));
    assert_eq!(app.config().repeat_delay(), 200);
}

#[test]
fn switch_shortcut_labels_cover_every_offered_option() {
    for option in SWITCH_OPTIONS {
        assert_ne!(shortcut_label(option), "—", "{option}");
    }
}

#[test]
fn the_layout_switch_shows_off_and_keeps_an_unknown_option_when_picked_again() {
    let mut app = app("[input]\nxkb-layout = \"us,ua\"\nxkb-options = \"grp:lwin_toggle,compose:ralt\"\n");
    let ctx =
        |app: &App| glide_settings::panels::Ctx { theme: app.theme, lang: Lang::En, writable: true, online: false };
    let (options, current) = keyboard::shortcut_choices(ctx(&app), &app.config().xkb_options());
    assert_eq!(current.label, "grp:lwin_toggle");
    assert_eq!(options[0].label, "Off");
    let _ = app.update(Message::Keyboard(keyboard::Msg::Shortcut(current)));
    assert_eq!(app.config().xkb_options(), "grp:lwin_toggle,compose:ralt");
    let _ = app.update(Message::Keyboard(keyboard::Msg::Shortcut(options[0].clone())));
    assert_eq!(app.config().xkb_options(), "compose:ralt");
    let (_, current) = keyboard::shortcut_choices(ctx(&app), &app.config().xkb_options());
    assert_eq!(current, options[0], "no grp: option reads as Off instead of an empty box");
}

#[test]
fn the_window_turns_compact_below_the_breakpoint() {
    let mut app = app("");
    assert!(!app.compact());
    let sized = |width| glide_sdk::window::Window { size: iced::Size::new(width, 700.0), ..Default::default() };
    app.follow_window(sized(glide_settings::app::COMPACT_BELOW - 1.0));
    assert!(app.compact());
    app.follow_window(sized(1000.0));
    assert!(!app.compact());
}

#[test]
fn a_scheduled_appearance_follows_the_clock_like_the_shell() {
    let scheduled = ConfigStore::parse("[topbar]\nappearance = \"light\"\nappearance-schedule = true\n");
    assert_eq!(scheduled.appearance_at(Some(20 * 60)), Appearance::Dark);
    assert_eq!(scheduled.appearance_at(Some(6 * 60 + 59)), Appearance::Dark);
    assert_eq!(scheduled.appearance_at(Some(12 * 60)), Appearance::Light);
    assert_eq!(scheduled.appearance_at(None), Appearance::Light);
    let fixed = ConfigStore::parse("[topbar]\nappearance = \"light\"\n");
    assert_eq!(fixed.appearance_at(Some(20 * 60)), Appearance::Light);
}

#[test]
fn mouse_settings_write_input_keys() {
    let mut app = app("");
    let _ = app.update(Message::Mouse(mouse::Msg::Natural(true)));
    let _ = app.update(Message::Mouse(mouse::Msg::Tap(false)));
    let _ = app.update(Message::Mouse(mouse::Msg::Speed(0.4)));
    let _ = app.update(Message::Mouse(mouse::Msg::CommitSpeed));
    assert!(app.config().natural_scroll());
    assert!(!app.config().tap_to_click());
    assert!((app.config().accel_speed() - 0.4).abs() < 1e-9);
}

#[test]
fn night_light_times_are_only_stored_when_valid() {
    use glide_settings::panels::night_light::Msg;
    let mut app = app("");
    let _ = app.update(Message::NightLight(Msg::NightSchedule(true)));
    let _ = app.update(Message::NightLight(Msg::NightFrom(String::from("21:3"))));
    assert_eq!(app.config().night_light_from(), "20:00");
    let _ = app.update(Message::NightLight(Msg::NightFrom(String::from("21:30"))));
    assert_eq!(app.config().night_light_from(), "21:30");
    assert!(app.config().night_light_schedule());
}

#[test]
fn night_light_temperature_is_written_rounded_clamped_and_reset() {
    use glide_settings::panels::night_light::Msg;
    let mut app = app("");
    assert_eq!(app.config().night_light_temperature(), 4000);
    let _ = app.update(Message::NightLight(Msg::Temperature(3333.0)));
    assert_eq!(app.config().get_int("night-light", "temperature"), None);
    let _ = app.update(Message::NightLight(Msg::CommitTemperature));
    assert_eq!(app.config().get_int("night-light", "temperature"), Some(3300));
    let _ = app.update(Message::NightLight(Msg::Temperature(90_000.0)));
    let _ = app.update(Message::NightLight(Msg::CommitTemperature));
    assert_eq!(app.config().get_int("night-light", "temperature"), Some(6000));
    let _ = app.update(Message::NightLight(Msg::ResetTemperature));
    assert_eq!(app.config().get_int("night-light", "temperature"), None);
}

#[test]
fn night_light_presets_turn_the_schedule_on_with_both_times() {
    use glide_settings::panels::night_light::{Msg, PRESETS};
    let mut app = app("");
    let _ = app.update(Message::NightLight(Msg::Preset(1)));
    assert!(app.config().night_light_schedule());
    assert_eq!(app.config().night_light_from(), PRESETS[1].0);
    assert_eq!(app.config().night_light_to(), PRESETS[1].1);
    let _ = app.update(Message::NightLight(Msg::Preset(99)));
    assert_eq!(app.config().night_light_from(), PRESETS[1].0);
}

#[test]
fn a_night_light_toggle_is_optimistic_and_a_failure_is_reported() {
    use glide_settings::panels::night_light::Msg;
    let mut app = app("");
    let _ = app.update(Message::PanelSelected(Panel::NightLight));
    let _ = app.update(Message::NightLight(Msg::Toggle(true)));
    let _ = app.update(Message::NightLight(Msg::Toggled(Some(Err(String::from("glide ipc socket not found"))))));
    let _ = app.view();
    let _ = app.update(Message::NightLight(Msg::Toggled(Some(Ok(false)))));
    let _ = app.view();
}

fn two_outputs() -> glide_settings::services::display::DisplayState {
    use glide_ipc::{ModeInfo, OutputInfo};
    let mode = |width, height, refresh_mhz, preferred| ModeInfo { width, height, refresh_mhz, preferred };
    glide_settings::services::display::DisplayState {
        live: true,
        compositor_running: true,
        backlight: None,
        outputs: vec![
            OutputInfo {
                name: String::from("eDP-1"),
                width: 2560,
                height: 1600,
                refresh_mhz: 60_000,
                scale: 2.0,
                modes: vec![
                    mode(2560, 1600, 60_000, true),
                    mode(2560, 1600, 120_000, false),
                    mode(1920, 1200, 60_000, false),
                ],
                physical_mm: (300, 190),
                ..OutputInfo::default()
            },
            OutputInfo {
                name: String::from("DP-2"),
                width: 1920,
                height: 1080,
                refresh_mhz: 60_000,
                scale: 1.0,
                x: 1280,
                ..OutputInfo::default()
            },
        ],
    }
}

#[test]
fn choosing_a_display_mode_writes_and_clears_the_output_mode_key() {
    use glide_settings::panels::displays::{ModeChoice, Msg};
    let mut app = app("");
    let _ = app.update(Message::Displays(Msg::Loaded(two_outputs())));
    let _ = app.update(Message::Displays(Msg::Mode(ModeChoice {
        key: Some(String::from("2560x1600@120")),
        label: String::from("120 Hz"),
    })));
    assert_eq!(app.config().output_mode("eDP-1").as_deref(), Some("2560x1600@120"));
    let _ = app.view();
    let _ = app.update(Message::Displays(Msg::Mode(ModeChoice { key: None, label: String::new() })));
    assert_eq!(app.config().output_mode("eDP-1"), None);
    assert!(app.config().to_text().contains("name = \"eDP-1\""));
}

#[test]
fn moving_a_display_rewrites_every_position_left_to_right() {
    use glide_settings::panels::displays::Msg;
    let mut app = app("");
    let _ = app.update(Message::PanelSelected(Panel::Displays));
    let _ = app.update(Message::Displays(Msg::Loaded(two_outputs())));
    let _ = app.update(Message::Displays(Msg::Move(false)));
    assert_eq!(app.config().output_position("eDP-1"), None, "already leftmost");
    let _ = app.update(Message::Displays(Msg::Move(true)));
    assert_eq!(app.config().output_position("DP-2"), Some((0, 0)));
    assert_eq!(app.config().output_position("eDP-1"), Some((1920, 0)));
    let _ = app.view();
}

#[test]
fn scale_ids_round_trip_common_fractions() {
    for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
        let id = displays::scale_id(scale);
        assert!((f64::from(id) / 100.0 - scale).abs() < 1e-9);
    }
}

#[test]
fn dismissing_a_notice_clears_it() {
    let mut app = app("");
    let _ = app.update(Message::Reloaded(Some(Err(ReloadError::Offline))));
    assert!(app.notice().is_some_and(|notice| !notice.error));
    let _ = app.update(Message::DismissNotice);
    assert!(app.notice().is_none());
}

#[test]
fn a_rejected_reload_shows_an_error_and_keeps_the_app_online() {
    let mut app = app("");
    let _ = app.update(Message::Reloaded(Some(Err(ReloadError::Rejected(String::from("bad key"))))));
    let notice = app.notice().expect("a notice is shown");
    assert!(notice.error);
    assert!(notice.text.contains("bad key"));
    assert!(notice.text.starts_with(Key::ConfigRejected.text(app.lang())));
}

#[test]
fn animation_speed_writes_the_inverse_multiplier() {
    let mut app = app("");
    let _ = app.update(Message::Appearance(appearance::Msg::Speed(2.0)));
    let _ = app.update(Message::Appearance(appearance::Msg::CommitSpeed));
    assert_eq!(app.config().get_float("animations", "speed"), Some(0.5));
}

#[test]
fn a_settled_slider_commits_without_a_release() {
    let mut app = app("");
    let _ = app.update(Message::Mouse(mouse::Msg::Speed(0.3)));
    let _ = app.update(Message::Mouse(mouse::Msg::Speed(0.5)));
    let _ = app.update(Message::Mouse(mouse::Msg::SettleSpeed(1)));
    assert!(app.config().get_float("input", "accel-speed").is_none());
    let _ = app.update(Message::Mouse(mouse::Msg::SettleSpeed(2)));
    assert!((app.config().accel_speed() - 0.5).abs() < 1e-9);
}

#[test]
fn a_successful_reload_clears_the_offline_notice() {
    let mut app = app("");
    let _ = app.update(Message::Reloaded(None));
    assert!(app.notice().is_some());
    let _ = app.update(Message::Reloaded(Some(Ok(()))));
    assert!(app.notice().is_none());
}

#[test]
fn every_panel_renders_in_both_themes() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let mut app = app("");
        let _ = app.update(Message::Appearance(appearance::Msg::SetAppearance(appearance)));
        for panel in Panel::ALL {
            let _ = app.update(Message::PanelSelected(*panel));
            let _ = app.view();
        }
    }
}

#[test]
fn every_panel_renders_read_only() {
    let mut app = app("[topbar\n");
    assert!(!app.config().writable());
    for panel in Panel::ALL {
        let _ = app.update(Message::PanelSelected(*panel));
        let _ = app.view();
    }
}

#[test]
fn dock_panel_writes_dock_keys() {
    use glide_settings::panels::desktop::{Autohide, Knob, Msg};
    let mut app = app("");
    let _ = app.update(Message::Desktop(Msg::Slide(Knob::Size, 64.0)));
    let _ = app.update(Message::Desktop(Msg::Commit));
    let _ = app.update(Message::Desktop(Msg::Slide(Knob::Magnification, 1.3)));
    let _ = app.update(Message::Desktop(Msg::Slide(Knob::Lift, 7.0)));
    let _ = app.update(Message::Desktop(Msg::Slide(Knob::Margin, 20.0)));
    let _ = app.update(Message::Desktop(Msg::Commit));
    let _ = app.update(Message::Desktop(Msg::Autohide(Autohide::Always)));
    let _ = app.update(Message::Desktop(Msg::Dock(false)));
    let config = app.config();
    assert_eq!(config.get_str("dock", "autohide"), Some("always"));
    assert_eq!(config.get_int("dock", "item-size"), Some(64));
    assert_eq!(config.get_int("dock", "icon-size"), Some(50));
    assert!(
        config.get_float("dock", "hover-scale").is_some_and(|scale| (scale - 1.3).abs() < 1e-9),
        "moving a second slider must commit the first one"
    );
    assert_eq!(config.get_int("dock", "hover-lift"), Some(7));
    assert_eq!(config.get_int("dock", "bottom-margin"), Some(20));
    assert_eq!(config.get_bool("dock", "enabled"), Some(false));
}

#[test]
fn slider_values_are_clamped_to_what_the_compositor_draws_well() {
    use glide_settings::panels::desktop::{Knob, Msg};
    let mut app = app("");
    let _ = app.update(Message::Desktop(Msg::Slide(Knob::Magnification, 9.0)));
    let _ = app.update(Message::Desktop(Msg::Commit));
    assert!(app.config().get_float("dock", "hover-scale").is_some_and(|scale| (scale - 1.5).abs() < 1e-9));
}

#[test]
fn top_bar_panel_writes_topbar_keys() {
    use glide_settings::panels::topbar::Msg;
    let mut app = app("[topbar]\nclock-24h = true\n");
    let _ = app.update(Message::TopBar(Msg::GlobalMenu(false)));
    let _ = app.update(Message::TopBar(Msg::Registrar(false)));
    let _ = app.update(Message::TopBar(Msg::Battery(false)));
    let _ = app.update(Message::TopBar(Msg::Tray(false)));
    let config = app.config();
    assert_eq!(config.get_bool("topbar", "global-menu"), Some(false));
    assert_eq!(config.get_bool("topbar", "appmenu-registrar"), Some(false));
    assert_eq!(config.get_bool("topbar", "battery-percentage"), Some(false));
    assert_eq!(config.get_bool("topbar", "tray"), Some(false));
    let _ = app.update(Message::TopBar(Msg::Open(Panel::DateTime)));
    assert_eq!(app.active_panel(), Panel::DateTime, "the clock row leads to its owner");
}

#[test]
fn the_top_bar_preview_clock_follows_the_format() {
    use glide_settings::panels::topbar::{TimeFormat, sample_clock};
    assert_eq!(sample_clock(TimeFormat::Hours24, Lang::En, false), "14:05");
    assert_eq!(sample_clock(TimeFormat::Hours12, Lang::En, false), "2:05 PM");
    assert!(sample_clock(TimeFormat::Hours24, Lang::Uk, true).starts_with("чт"));
}

#[test]
fn multitasking_panel_writes_workspace_overview_and_snapping_keys() {
    use glide_settings::panels::desktop::NewWindow;
    use glide_settings::panels::multitasking::Msg;
    let mut app = app("");
    let _ = app.update(Message::Multitasking(Msg::NewWindow(NewWindow::ActiveWorkspace)));
    let _ = app.update(Message::Multitasking(Msg::OverviewLabels(false)));
    let _ = app.update(Message::Multitasking(Msg::HotCorner(true)));
    let _ = app.update(Message::Multitasking(Msg::EdgeSnapping(false)));
    let config = app.config();
    assert_eq!(config.get_str("workspaces", "new-window"), Some("active-workspace"));
    assert_eq!(config.get_bool("overview", "label"), Some(false));
    assert_eq!(config.get_bool("overview", "hot-corner"), Some(true));
    assert_eq!(config.get_bool("windows", "edge-snapping"), Some(false));
}

#[test]
fn windows_panel_writes_decoration_keys() {
    use glide_settings::panels::windows::{ButtonLayout, COMPACT_HEIGHT, Density, DoubleClick, Msg, TitleAlign};
    let mut app = app("");
    let _ = app.update(Message::Windows(Msg::Align(TitleAlign::Center)));
    let _ = app.update(Message::Windows(Msg::Density(Density::Compact)));
    let _ = app.update(Message::Windows(Msg::Shadow(false)));
    let _ = app.update(Message::Windows(Msg::Border(false)));
    let _ = app.update(Message::Windows(Msg::Buttons(ButtonLayout::CloseLeft)));
    let _ = app.update(Message::Windows(Msg::DoubleClick(DoubleClick::Nothing)));
    let config = app.config();
    assert_eq!(config.get_str("decorations", "title-align"), Some("center"));
    assert_eq!(config.get_int("decorations", "height"), Some(COMPACT_HEIGHT));
    assert_eq!(config.get_bool("decorations", "shadow"), Some(false));
    assert_eq!(config.get_int("decorations", "border-width"), Some(0));
    assert_eq!(config.get_str("decorations", "button-layout"), Some("close-maximize"));
    assert_eq!(config.get_str("decorations", "double-click"), Some("none"));
    let _ = app.update(Message::Windows(Msg::Border(true)));
    assert_eq!(
        app.config().get_int("decorations", "border-width"),
        None,
        "turning the border back on restores the default width"
    );
}

#[test]
fn turning_the_border_on_keeps_a_custom_width() {
    use glide_settings::panels::windows::Msg;
    let mut app = app("[decorations]\nborder-width = 3\n");
    let _ = app.update(Message::Windows(Msg::Border(true)));
    assert_eq!(app.config().get_int("decorations", "border-width"), Some(3));
}

#[test]
fn wallpaper_panel_writes_general_keys() {
    use glide_settings::panels::wallpaper::Msg;
    use glide_settings::services::config::WallpaperFit;
    let mut app = app("[general]\nwallpaper = \"/tmp/old.png\"\n");
    let _ = app.update(Message::Wallpaper(Msg::SetFit(WallpaperFit::Contain)));
    let _ = app.update(Message::Wallpaper(Msg::Backdrop([0x10, 0x20, 0x30])));
    let _ = app.update(Message::Wallpaper(Msg::Pick("/tmp/new.jpg".into())));
    let config = app.config();
    assert_eq!(config.get_str("general", "wallpaper-fit"), Some("contain"));
    assert_eq!(config.get_str("general", "background"), Some("#102030"));
    assert_eq!(config.get_str("general", "wallpaper"), Some("/tmp/new.jpg"));
    let _ = app.update(Message::Wallpaper(Msg::SolidReady([1, 2, 3], Ok("/tmp/solid-010203.png".into()))));
    assert_eq!(app.config().get_str("general", "wallpaper"), Some("/tmp/solid-010203.png"));
    assert_eq!(
        app.config().get_str("general", "background"),
        Some("#010203"),
        "a solid colour also paints the letterbox so the two never disagree"
    );
    let _ = app.update(Message::Wallpaper(Msg::UseBuiltin));
    assert_eq!(app.config().get_str("general", "wallpaper"), None);
}

#[test]
fn an_invalid_solid_colour_is_flagged_instead_of_written() {
    use glide_settings::panels::wallpaper::Msg;
    let mut app = app("");
    let _ = app.update(Message::Wallpaper(Msg::SolidHex("#12zz45".into())));
    let _ = app.update(Message::Wallpaper(Msg::ApplySolidHex));
    assert_eq!(app.config().get_str("general", "wallpaper"), None);
    assert_eq!(app.config().get_str("general", "background"), None);
}

#[test]
fn personalisation_panels_render_in_both_styles() {
    for style in ["dark", "light"] {
        let mut app = app(&format!("[topbar]\nappearance = \"{style}\"\n[general]\nwallpaper-fit = \"tile\"\n"));
        for panel in
            [Panel::Appearance, Panel::Wallpaper, Panel::Desktop, Panel::TopBar, Panel::Windows, Panel::Multitasking]
        {
            let _ = app.update(Message::PanelSelected(panel));
            let _ = app.view();
        }
    }
}

#[test]
fn power_panel_writes_battery_keys_and_links_to_the_lock_screen() {
    use glide_settings::panels::power::Msg;
    let mut app = app("");
    let _ = app.update(Message::Power(Msg::BatteryLow(12.0)));
    let _ = app.update(Message::Power(Msg::CommitBatteryLow));
    assert_eq!(app.config().get_int("topbar", "battery-low-pct"), Some(12));
    let _ = app.update(Message::Power(Msg::Open(Panel::LockScreen)));
    assert_eq!(app.active_panel(), Panel::LockScreen);
}

#[test]
fn startup_commands_are_added_once_and_removed() {
    use glide_settings::panels::startup::Msg;
    let mut app = app("");
    let _ = app.update(Message::Startup(Msg::Command(String::from("  nm-applet  "))));
    let _ = app.update(Message::Startup(Msg::AddCommand));
    let _ = app.update(Message::Startup(Msg::Command(String::from("nm-applet"))));
    let _ = app.update(Message::Startup(Msg::AddCommand));
    assert_eq!(app.config().autostart(), vec!["nm-applet"]);
    let _ = app.update(Message::Startup(Msg::RemoveStartup(0)));
    assert!(app.config().autostart().is_empty());
}

#[test]
fn startup_commands_reorder_and_xdg_entries_toggle_the_config_key() {
    use glide_settings::panels::startup::Msg;
    let mut app = app("[general]\nautostart = [\"a\", \"b\", \"c\"]\n");
    let _ = app.update(Message::Startup(Msg::MoveUp(2)));
    assert_eq!(app.config().autostart(), vec!["a", "c", "b"]);
    let _ = app.update(Message::Startup(Msg::MoveUp(0)));
    assert_eq!(app.config().autostart(), vec!["a", "c", "b"]);
    let _ = app.update(Message::Startup(Msg::Launch(true)));
    assert_eq!(app.config().get_bool("general", "xdg-autostart"), Some(true));
    let _ = app.update(Message::Startup(Msg::PolkitAgent(false)));
    assert_eq!(app.config().get_bool("general", "polkit-agent"), Some(false));
}

#[test]
fn date_time_clock_format_writes_and_clears_the_topbar_key() {
    use glide_settings::panels::date_time::{ClockFormat, Msg};
    let mut app = app("");
    let _ = app.update(Message::DateTime(Msg::Format(ClockFormat::H12)));
    assert_eq!(app.config().get_bool("topbar", "clock-24h"), Some(false));
    assert_eq!(ClockFormat::from_config(app.config()), ClockFormat::H12);
    let _ = app.update(Message::DateTime(Msg::Format(ClockFormat::Auto)));
    assert_eq!(app.config().get_bool("topbar", "clock-24h"), None);
    let _ = app.update(Message::DateTime(Msg::ShowDate(false)));
    assert_eq!(app.config().get_bool("topbar", "clock-date"), Some(false));
}

#[test]
fn date_time_helpers_format_both_languages() {
    use glide_settings::panels::date_time::{clock_text, long_date, offset_delta};
    use glide_settings::services::tz;
    let when = tz::civil(tz::from_civil(2026, 9, 24, 14, 5, 0));
    assert_eq!(clock_text(&when, false), "14:05");
    assert_eq!(clock_text(&when, true), "2:05 PM");
    assert_eq!(long_date(&when, Lang::En), "Thursday, 24 September 2026");
    assert_eq!(long_date(&when, Lang::Uk), "четвер, 24 вересня 2026 р.");
    assert_eq!(offset_delta(3600, Lang::En), "+1 h");
    assert_eq!(offset_delta(-19_800, Lang::Uk), "−5:30 год");
}

#[test]
fn region_language_row_writes_the_interface_language() {
    use glide_settings::panels::region::Msg;
    let mut app = App::with_config(ConfigStore::parse("").with_system_language(Lang::Uk));
    let _ = app.update(Message::Region(Msg::Interface(region::LangChoice::Explicit(Lang::En))));
    assert_eq!(app.config().language_choice(), Some(Lang::En));
    assert_eq!(app.lang(), Lang::En);
}

#[test]
fn region_choice_sets_every_format_category_or_clears_it() {
    use glide_settings::panels::region::with_region;
    use glide_settings::services::locale::{FORMAT_CATEGORIES, parse_assignments};
    let system = parse_assignments(["LANG=en_US.UTF-8", "LC_TIME=en_GB.UTF-8"]);
    let next = with_region(&system, "uk_UA.UTF-8");
    for category in FORMAT_CATEGORIES {
        assert_eq!(next.get(category), Some("uk_UA.UTF-8"));
    }
    let back = with_region(&next, "en_US.UTF-8");
    assert_eq!(back.assignments(), vec!["LANG=en_US.UTF-8"]);
}

#[test]
fn about_rename_needs_a_valid_name() {
    use glide_settings::panels::about::Msg;
    let mut app = app("");
    let _ = app.update(Message::PanelSelected(Panel::About));
    let _ = app.update(Message::About(Msg::Name(String::from("ignored"))));
    let _ = app.update(Message::About(Msg::CancelRename));
    let _ = app.update(Message::About(Msg::SaveName));
    let _ = app.view();
}

#[test]
fn storage_confirmation_can_be_cancelled() {
    use glide_settings::panels::storage::{Action, Msg};
    let mut app = app("");
    let _ = app.update(Message::Storage(Msg::Ask(Action::EmptyTrash)));
    let _ = app.update(Message::Storage(Msg::Cancel));
    let _ = app.view();
}

#[test]
fn default_apps_filter_and_search_render() {
    use glide_settings::panels::apps::Msg;
    use glide_settings::services::apps::Kind;
    let mut app = app("");
    let _ = app.update(Message::PanelSelected(Panel::Apps));
    let _ = app.update(Message::Apps(Msg::Filter(Some(Kind::Video))));
    let _ = app.update(Message::Apps(Msg::Query(String::from(".mkv"))));
    let _ = app.update(Message::Apps(Msg::AskReset));
    let _ = app.update(Message::Apps(Msg::CancelReset));
    let _ = app.view();
}

#[test]
fn over_amplification_toggles_max_volume() {
    use glide_settings::panels::sound::{Msg, max_volume};
    let mut app = app("");
    assert!((max_volume(app.config()) - 1.0).abs() < f32::EPSILON);
    let _ = app.update(Message::Sound(Msg::OverAmplify(true)));
    assert_eq!(app.config().get_int("topbar", "max-volume-pct"), Some(150));
    assert!((max_volume(app.config()) - 1.5).abs() < f32::EPSILON);
}

#[test]
fn icon_theme_and_font_choices_are_written_and_cleared() {
    use glide_settings::panels::appearance::{Choice, Msg};
    let mut app = app("");
    let _ = app.update(Message::Appearance(Msg::IconTheme(Choice {
        value: Some(String::from("Papirus")),
        label: String::from("Papirus"),
    })));
    let _ = app.update(Message::Appearance(Msg::Font(Choice {
        value: Some(String::from("Inter")),
        label: String::from("Inter"),
    })));
    assert_eq!(app.config().get_str("dock", "icon-theme"), Some("Papirus"));
    assert_eq!(app.config().get_str("general", "font-family"), Some("Inter"));
    let _ = app.update(Message::Appearance(Msg::Font(Choice { value: None, label: String::from("Default") })));
    assert_eq!(app.config().get_str("general", "font-family"), None);
}

fn key_press(key: iced::keyboard::Key, modifiers: iced::keyboard::Modifiers) -> iced::keyboard::Event {
    use iced::keyboard::key::{NativeCode, Physical};
    iced::keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key: Physical::Unidentified(NativeCode::Unidentified),
        location: iced::keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    }
}

#[test]
fn sidebar_shortcuts_step_through_panels() {
    use glide_settings::app::shortcut;
    use iced::keyboard::{Key, Modifiers, key::Named};
    let alt = Modifiers::ALT;
    let ctrl = Modifiers::CTRL;
    assert!(matches!(shortcut(key_press(Key::Named(Named::ArrowDown), alt)), Some(Message::SelectNext)));
    assert!(matches!(shortcut(key_press(Key::Named(Named::ArrowUp), alt)), Some(Message::SelectPrevious)));
    assert!(matches!(shortcut(key_press(Key::Named(Named::PageDown), ctrl)), Some(Message::SelectNext)));
    assert!(matches!(
        shortcut(key_press(Key::Named(Named::Tab), ctrl | Modifiers::SHIFT)),
        Some(Message::SelectPrevious)
    ));
    assert!(matches!(shortcut(key_press(Key::Character("f".into()), ctrl)), Some(Message::FocusSearch)));
    assert!(shortcut(key_press(Key::Named(Named::ArrowDown), Modifiers::empty())).is_none());
}

#[test]
fn stepping_walks_the_sidebar_in_group_order_and_wraps() {
    let order: Vec<Category> = Category::GROUPS.iter().flat_map(|group| group.categories.iter().copied()).collect();
    let mut app = app("");
    let _ = app.update(Message::CategorySelected(order[0]));
    for expected in order.iter().skip(1) {
        let _ = app.update(Message::SelectNext);
        assert_eq!(app.active_category(), *expected);
        assert_eq!(app.active_panel(), expected.first());
    }
    let _ = app.update(Message::SelectNext);
    assert_eq!(app.active_category(), order[0]);
    let _ = app.update(Message::SelectPrevious);
    assert_eq!(app.active_category(), order[order.len() - 1]);
}

#[test]
fn every_category_knows_its_sidebar_group() {
    for group in Category::GROUPS {
        for category in group.categories {
            assert_eq!(category.group().title, group.title, "{category:?}");
        }
    }
}

#[test]
fn a_keyword_hit_names_the_keyword_but_a_label_hit_does_not() {
    assert_eq!(Panel::Displays.matched_keyword("resolution"), Some("resolution"));
    assert_eq!(Panel::Displays.matched_keyword("displ"), None);
    assert_eq!(Panel::Displays.matched_keyword(""), None);
}

#[test]
fn the_sidebar_renders_while_searching_and_with_no_hits() {
    let mut app = app("");
    for query in ["res", "мереж", "zzzz", ""] {
        let _ = app.update(Message::SearchChanged(String::from(query)));
        let _ = app.view();
    }
}

#[test]
fn mouse_panel_writes_every_pointer_key() {
    use glide_settings::panels::mouse::{ClickMethod, Msg, ScrollMethod, Slider};
    let mut app = app("");
    let send = |app: &mut App, msg: Msg| {
        let _ = app.update(Message::Mouse(msg));
    };
    send(&mut app, Msg::LeftHanded(true));
    send(&mut app, Msg::MiddleEmulation(true));
    send(&mut app, Msg::SmoothWheel(false));
    send(&mut app, Msg::TapAndDrag(false));
    send(&mut app, Msg::DisableWhileTyping(false));
    send(&mut app, Msg::Scroll(ScrollMethod::Edge));
    send(&mut app, Msg::Click(ClickMethod::Clickfinger));
    send(&mut app, Msg::Slide(Slider::MouseScroll, 1.5));
    send(&mut app, Msg::Commit(Slider::MouseScroll));
    let config = app.config();
    assert_eq!(config.get_bool("input", "left-handed"), Some(true));
    assert_eq!(config.get_bool("input", "middle-emulation"), Some(true));
    assert_eq!(config.get_bool("input", "smooth-wheel"), Some(false));
    assert_eq!(config.get_bool("input", "tap-and-drag"), Some(false));
    assert_eq!(config.get_bool("input", "disable-while-typing"), Some(false));
    assert_eq!(config.get_str("input", "scroll-method"), Some("edge"));
    assert_eq!(config.get_str("input", "click-method"), Some("clickfinger"));
    assert_eq!(config.get_float("input", "scroll-factor"), Some(1.5));
    assert_eq!(
        config.get_float("input", "touchpad-scroll-factor"),
        Some(1.0),
        "changing the mouse keeps the touchpad where it was"
    );
    send(&mut app, Msg::Click(ClickMethod::Default));
    assert_eq!(app.config().get_str("input", "click-method"), None);
}

#[test]
fn touchpad_speed_follows_the_mouse_until_it_gets_its_own() {
    use glide_settings::panels::mouse::{Msg, Slider, touchpad_speed};
    let mut app = app("[input]\naccel-speed = 0.3\n");
    assert_eq!(touchpad_speed(app.config()), None);
    let _ = app.update(Message::Mouse(Msg::TouchpadOwnSpeed(true)));
    assert_eq!(touchpad_speed(app.config()), Some(0.3));
    let _ = app.update(Message::Mouse(Msg::Slide(Slider::TouchpadSpeed, -0.4)));
    let _ = app.update(Message::Mouse(Msg::Commit(Slider::TouchpadSpeed)));
    assert_eq!(touchpad_speed(app.config()), Some(-0.4));
    assert!((app.config().accel_speed() - 0.3).abs() < 1e-9);
    let _ = app.update(Message::Mouse(Msg::TouchpadOwnSpeed(false)));
    assert_eq!(touchpad_speed(app.config()), None);
}

#[test]
fn mouse_scrolling_changes_leave_the_touchpad_alone() {
    use glide_settings::panels::mouse::{Msg, touchpad_natural_scroll, touchpad_profile};
    use glide_settings::services::config::AccelProfile;
    let mut app = app("");
    let _ = app.update(Message::Mouse(Msg::Natural(true)));
    let _ = app.update(Message::Mouse(Msg::Profile(AccelProfile::Flat)));
    assert!(app.config().natural_scroll());
    assert!(!touchpad_natural_scroll(app.config()));
    assert_eq!(app.config().accel_profile(), AccelProfile::Flat);
    assert_eq!(touchpad_profile(app.config()), AccelProfile::Adaptive);
    let _ = app.update(Message::Mouse(Msg::TouchpadNatural(true)));
    assert!(touchpad_natural_scroll(app.config()));
}

#[test]
fn the_click_test_area_renders_and_writes_nothing() {
    use glide_settings::panels::mouse::Msg;
    let mut app = app("");
    let _ = app.update(Message::PanelSelected(Panel::Mouse));
    let _ = app.update(Message::Mouse(Msg::TestClick));
    let _ = app.update(Message::Mouse(Msg::TestClick));
    let _ = app.view();
    assert_eq!(app.config().to_text(), "", "the test area writes nothing");
}

#[test]
fn keyboard_special_keys_edit_only_their_xkb_option_family() {
    use glide_settings::panels::keyboard::{Msg, OptionChoice};
    let mut app = app("[input]\nxkb-options = \"grp:alt_shift_toggle,caps:escape\"\n");
    let choice = |option: &str| OptionChoice { option: Some(option.to_string()), label: option.to_string() };
    let _ = app.update(Message::Keyboard(Msg::Compose(choice("compose:ralt"))));
    let _ = app.update(Message::Keyboard(Msg::Caps(choice("ctrl:nocaps"))));
    let _ = app.update(Message::Keyboard(Msg::SwapAltSuper(true)));
    let _ = app.update(Message::Keyboard(Msg::NumLock(true)));
    assert_eq!(app.config().xkb_options(), "grp:alt_shift_toggle,compose:ralt,ctrl:nocaps,altwin:swap_alt_win");
    assert_eq!(app.config().get_bool("input", "numlock"), Some(true));
    let _ = app.update(Message::Keyboard(Msg::Caps(OptionChoice { option: None, label: String::new() })));
    let _ = app.update(Message::Keyboard(Msg::SwapAltSuper(false)));
    assert_eq!(app.config().xkb_options(), "grp:alt_shift_toggle,compose:ralt");
}

#[test]
fn keyboard_option_choices_show_an_unknown_current_value() {
    use glide_settings::panels::keyboard::caps_choices;
    let app = app("");
    let c = glide_settings::panels::Ctx { theme: app.theme, lang: Lang::En, writable: true, online: false };
    let (choices, current) = caps_choices(c, "caps:ctrl_modifier");
    assert_eq!(current.option.as_deref(), Some("caps:ctrl_modifier"));
    assert!(choices.contains(&current));
    let (_, default) = caps_choices(c, "grp:caps_toggle");
    assert_eq!(default.option, None, "a layout switch is not a caps option");
}

fn shortcuts_edit(app: &mut App, index: usize, mods: &[&str], key: &str) {
    use glide_settings::panels::shortcuts::Msg;
    use glide_settings::services::keybinds::Mod;
    let _ = app.update(Message::Shortcuts(Msg::Edit(index)));
    let current: Vec<Mod> = app.config().root_item("keybinds").map_or_else(
        || glide_settings::services::keybinds::defaults().to_vec(),
        |item| glide_settings::services::keybinds::from_item(item).expect("parses"),
    )[index]
        .mods
        .clone();
    for m in Mod::ALL {
        let wanted = mods.contains(&m.key());
        if current.contains(&m) != wanted {
            let _ = app.update(Message::Shortcuts(Msg::ToggleMod(m)));
        }
    }
    let _ = app.update(Message::Shortcuts(Msg::Key(key.to_string())));
    let _ = app.update(Message::Shortcuts(Msg::Save));
}

#[test]
fn editing_one_shortcut_writes_the_whole_list_and_round_trips() {
    use glide_settings::services::keybinds::{self, Action, Mod};
    let mut app = app("[general]\nlanguage = \"en\"\n");
    assert!(app.config().root_item("keybinds").is_none());
    let defaults = keybinds::defaults();
    assert!(defaults.len() > 40, "the mirror of the compositor defaults loads");
    shortcuts_edit(&mut app, 0, &["super", "ctrl"], "w");
    let text = app.config().to_text();
    let parsed = keybinds::parse_document(&text).expect("the written list parses");
    assert_eq!(parsed.len(), defaults.len());
    assert_eq!(parsed[0].mods, vec![Mod::Super, Mod::Ctrl]);
    assert_eq!(parsed[0].key, "w");
    assert_eq!(parsed[0].action, Action::Named(String::from("close-app")));
    assert_eq!(&parsed[1..], &defaults[1..]);
    assert!(text.contains("[[keybinds]]"));
    assert!(text.contains("action = { exec = \"brightnessctl set +5%\" }"), "{text}");
    assert!(text.contains("key = \"XF86AudioNext\"\naction = \"media-next\""), "{text}");
    let reparsed = ConfigStore::parse(&text);
    assert_eq!(keybinds::load(&reparsed).expect("loads").binds, parsed, "writing and reading back is lossless");
}

#[test]
fn page_down_shortcuts_from_the_defaults_can_be_edited_and_match_their_alias() {
    use glide_settings::panels::shortcuts::{Draft, validate};
    use glide_settings::services::keybinds::{self, Action};
    let defaults = keybinds::defaults();
    let index = defaults
        .iter()
        .position(|bind| bind.action == Action::Named(String::from("move-window-to-next-workspace")))
        .expect("the default exists");
    let bind = &defaults[index];
    assert_eq!(bind.key, "Page_Down");
    let draft =
        Draft { index: Some(index), mods: bind.mods.clone(), key: bind.key.clone(), action: bind.action.clone() };
    let saved = validate(&draft).expect("an unchanged default validates");
    assert!(saved.same_combo(bind), "Page_Down and Next are one key");
}

#[test]
fn restoring_the_default_combo_removes_the_keybinds_list() {
    let mut app = app("");
    shortcuts_edit(&mut app, 2, &["super"], "F9");
    assert!(app.config().root_item("keybinds").is_some());
    shortcuts_edit(&mut app, 2, &["super"], "Tab");
    assert!(app.config().root_item("keybinds").is_none(), "a list equal to the defaults is not written out");
}

#[test]
fn reset_all_shortcuts_removes_the_key() {
    use glide_settings::panels::shortcuts::Msg;
    let mut app = app("[[keybinds]]\nmods = [\"super\"]\nkey = \"Return\"\naction = { exec = \"alacritty\" }\n");
    let loaded = glide_settings::services::keybinds::load(app.config()).expect("loads");
    assert!(loaded.customised);
    assert_eq!(loaded.binds.len(), 1);
    let _ = app.update(Message::Shortcuts(Msg::ResetAll));
    assert!(app.config().root_item("keybinds").is_none());
}

#[test]
fn a_custom_command_shortcut_is_appended_and_validated() {
    use glide_settings::panels::shortcuts::Msg;
    use glide_settings::services::keybinds::{self, Action, Mod};
    let mut app = app("");
    let _ = app.update(Message::Shortcuts(Msg::AddCommand));
    let _ = app.update(Message::Shortcuts(Msg::Key(String::from("Enter"))));
    let _ = app.update(Message::Shortcuts(Msg::Save));
    assert!(app.config().root_item("keybinds").is_none(), "an empty command is never saved");
    let _ = app.update(Message::Shortcuts(Msg::Command(String::from(" alacritty "))));
    let _ = app.update(Message::Shortcuts(Msg::Save));
    let binds = keybinds::load(app.config()).expect("loads").binds;
    let last = binds.last().expect("not empty");
    assert_eq!(last.key, "Return");
    assert_eq!(last.mods, vec![Mod::Super]);
    assert_eq!(last.action, Action::Exec(String::from("alacritty")));
    assert_eq!(binds.len(), keybinds::defaults().len() + 1);
}

#[test]
fn shortcut_validation_rejects_empty_unknown_and_bare_typing_keys() {
    use glide_settings::panels::shortcuts::{Draft, DraftError, validate};
    use glide_settings::services::keybinds::{Action, Mod};
    let draft = |mods: Vec<Mod>, key: &str| Draft {
        index: None,
        mods,
        key: key.to_string(),
        action: Action::Named(String::from("screenshot")),
    };
    assert_eq!(validate(&draft(vec![], "")), Err(DraftError::NeedKey));
    assert_eq!(validate(&draft(vec![Mod::Super], "Hyper_Nonsense")), Err(DraftError::UnknownKey));
    assert_eq!(validate(&draft(vec![], "a")), Err(DraftError::NeedModifier));
    assert_eq!(validate(&draft(vec![Mod::Shift], "space")), Err(DraftError::NeedModifier));
    assert!(validate(&draft(vec![], "Print")).is_ok());
    assert!(validate(&draft(vec![], "XF86Calculator")).is_ok());
    assert_eq!(validate(&draft(vec![Mod::Alt], "f12")).map(|bind| bind.key), Ok(String::from("F12")));
}

#[test]
fn shortcut_conflicts_are_found_and_keybinds_render() {
    use glide_settings::services::keybinds;
    let mut app = app(
        "[[keybinds]]\nmods = [\"super\"]\nkey = \"q\"\naction = \"close-app\"\n\n[[keybinds]]\nmods = [\"logo\"]\nkey = \"Q\"\naction = \"quit\"\n",
    );
    let binds = keybinds::load(app.config()).expect("loads").binds;
    assert_eq!(keybinds::conflicts(&binds), vec![(0, 1)]);
    assert!(keybinds::conflicts(keybinds::defaults()).is_empty());
    let _ = app.update(Message::PanelSelected(Panel::Shortcuts));
    let _ = app.update(Message::Shortcuts(glide_settings::panels::shortcuts::Msg::Edit(1)));
    let _ = app.view();
}

#[test]
fn an_unreadable_keybinds_list_is_left_alone() {
    use glide_settings::panels::shortcuts::Msg;
    let mut app = app("[[keybinds]]\nkey = \"q\"\naction = { launch = \"x\" }\n");
    assert!(glide_settings::services::keybinds::load(app.config()).is_err());
    let _ = app.update(Message::Shortcuts(Msg::ResetAll));
    let _ = app.update(Message::Shortcuts(Msg::AddCommand));
    assert!(app.config().root_item("keybinds").is_some());
    let _ = app.update(Message::PanelSelected(Panel::Shortcuts));
    let _ = app.view();
}

#[test]
fn search_shortcuts_work_on_non_latin_keyboard_layouts_and_recording_stops_on_panel_switch() {
    use glide_settings::app::shortcut;
    use glide_settings::panels::shortcuts::Msg;
    use iced::keyboard::{
        Event, Key, Location, Modifiers,
        key::{Code, Physical},
    };
    let press = |ch: &str, code: Code, modifiers: Modifiers| Event::KeyPressed {
        key: Key::Character(ch.into()),
        modified_key: Key::Character(ch.into()),
        physical_key: Physical::Code(code),
        location: Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    };
    assert!(matches!(shortcut(press("а", Code::KeyF, Modifiers::CTRL)), Some(Message::FocusSearch)));
    assert!(matches!(shortcut(press("л", Code::KeyK, Modifiers::CTRL)), Some(Message::FocusSearch)));

    let mut app = app("");
    let _ = app.update(Message::PanelSelected(Panel::Shortcuts));
    let _ = app.update(Message::Shortcuts(Msg::Edit(0)));
    let _ = app.update(Message::Shortcuts(Msg::Record));
    assert!(app.is_recording());
    let _ = app.update(Message::PanelSelected(Panel::Appearance));
    assert!(!app.is_recording());
}

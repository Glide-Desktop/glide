use glide_settings::services::config::{
    AccelProfile, ConfigStore, WallpaperFit, accent_key, parse_clock, parse_error_summary, round_to,
};
use glide_settings::services::{audio, display, input, network, system};
use glide_settings::{Accent, Appearance, Lang};

const WPCTL_STATUS: &str = "PipeWire 'pipewire-0' [1.6.8, artem@laptop, cookie:1976157988]
 └─ Clients:
        32. WirePlumber                         [1.6.8, artem@laptop, pid:2474]

Audio
 ├─ Devices:
 │      42. Renoir/Cezanne HDMI/DP Audio Controller [alsa]
 │      43. Ryzen HD Audio Controller           [alsa]
 │
 ├─ Sinks:
 │  *   50. Ryzen HD Audio Controller Speaker   [vol: 0.50]
 │      58. HDMI / DisplayPort 1 Output         [vol: 1.00]
 │
 ├─ Sources:
 │  *   51. Ryzen HD Audio Controller Digital Microphone [vol: 1.00 MUTED]
 │
 ├─ Filters:
 │
 └─ Streams:

Video
 ├─ Devices:
 │      52. ACER HD User Facing                 [v4l2]
 ├─ Sinks:
 │
 ├─ Sources:
 │  *   60. ACER HD User Facing (V4L2)
";

#[test]
fn wpctl_status_yields_audio_sinks_and_sources_but_not_video() {
    let (sinks, sources) = audio::parse_status(WPCTL_STATUS);
    assert_eq!(sinks.len(), 2);
    assert_eq!(sinks[0].id, 50);
    assert_eq!(sinks[0].name, "Ryzen HD Audio Controller Speaker");
    assert!(sinks[0].is_default);
    assert!(!sinks[1].is_default);
    assert_eq!(sinks[1].name, "HDMI / DisplayPort 1 Output");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].id, 51);
    assert!(sources.iter().all(|device| device.id != 60));
}

#[test]
fn wpctl_volume_reports_level_and_mute() {
    let level = audio::parse_volume("Volume: 0.45 [MUTED]\n").expect("parses");
    assert!((level.volume - 0.45).abs() < 1e-6);
    assert!(level.muted);
    let level = audio::parse_volume("Volume: 1.00").expect("parses");
    assert!(!level.muted);
    assert!(audio::parse_volume("garbage").is_none());
}

#[test]
fn nmcli_terse_split_honours_escaped_colons() {
    assert_eq!(network::split_terse(r"*:Cafe\:Guest:72:WPA2"), vec!["*", "Cafe:Guest", "72", "WPA2"]);
    assert_eq!(network::split_terse(r"a\\b:c"), vec![r"a\b", "c"]);
}

#[test]
fn nmcli_networks_are_deduplicated_and_sorted_active_first() {
    let text = " :Neighbour:80:WPA2\n*:Home:40:WPA2\n :Home:65:WPA2\n :Cafe:90:\n ::70:WPA2\n :Saved:20:WPA1\n";
    let known = vec![
        network::KnownNetwork { uuid: String::from("uuid-saved"), ssid: String::from("Saved") },
        network::KnownNetwork { uuid: String::from("uuid-home"), ssid: String::from("Home") },
    ];
    let networks = network::parse_networks(text, &known);
    let names: Vec<&str> = networks.iter().map(|n| n.ssid.as_str()).collect();
    assert_eq!(names, vec!["Home", "Saved", "Cafe", "Neighbour"]);
    assert!(networks[0].in_use);
    assert_eq!(networks[0].signal, 65);
    assert_eq!(networks[0].profile.as_deref(), Some("uuid-home"));
    assert!(!networks[2].secured);
    assert!(!networks[2].known);
    assert!(networks[3].secured);
}

#[test]
fn nmcli_profiles_are_matched_by_ssid_not_by_profile_name() {
    assert_eq!(network::parse_ssid_records("HomeNet\n"), ["HomeNet"]);
    assert_eq!(network::parse_ssid_records("a:b\\c\n\nOffice\n\nCafe 5G\n"), ["a:b\\c", "Office", "Cafe 5G"]);
    assert!(network::parse_ssid_records("").is_empty(), "one SSID per profile, from a single nmcli call");
    let known = vec![network::KnownNetwork { uuid: String::from("1111-aaaa"), ssid: String::from("HomeNet") }];
    let networks = network::parse_networks(" :HomeNet:50:WPA2\n :Home Wi-Fi:40:WPA2\n", &known);
    assert!(networks.iter().any(|n| n.ssid == "HomeNet" && n.known));
    assert!(networks.iter().any(|n| n.ssid == "Home Wi-Fi" && !n.known));
}

#[test]
fn nmcli_connect_uses_the_profile_uuid_when_known() {
    use network::ConnectPlan;
    assert_eq!(
        network::connect_plan("Home", Some("1111"), None),
        ConnectPlan::Nmcli(vec!["connection", "up", "uuid", "1111"])
    );
    assert_eq!(
        network::connect_plan("Home", Some("1111"), Some("")),
        ConnectPlan::Nmcli(vec!["connection", "up", "uuid", "1111"])
    );
    assert_eq!(
        network::connect_plan("Open", None, None),
        ConnectPlan::Nmcli(vec!["device", "wifi", "connect", "Open"])
    );
}

#[test]
fn config_parse_errors_are_one_line_with_a_position() {
    let store = ConfigStore::parse("[general]\nlanguage = \"en\"\n[dock\n");
    let error = store.error().expect("a broken file reports an error");
    assert!(!error.contains('\n'), "banner text must be one line: {error:?}");
    assert!(error.starts_with("line 3, column "), "{error}");
    assert_eq!(parse_error_summary("é\nx", "bad", Some(3)), "line 2, column 1: bad");
    assert_eq!(parse_error_summary("é", "bad", Some(1)), "bad");
    assert_eq!(parse_error_summary("", " bad\n", None), "bad");
}

#[test]
fn wifi_password_never_reaches_argv() {
    use network::ConnectPlan;
    let secret = "hunter2-secret";
    for profile in [None, Some("1111")] {
        for ssid in ["Cafe", secret] {
            let plan = network::connect_plan(ssid, profile, Some(secret));
            assert_eq!(plan, ConnectPlan::AddAndActivate { ssid, psk: secret, replaces: profile });
            assert!(!format!("{plan:?}").contains("psk"));
        }
    }
    for password in [None, Some("")] {
        match network::connect_plan("Cafe", None, password) {
            ConnectPlan::Nmcli(args) => assert!(!args.contains(&"password")),
            ConnectPlan::AddAndActivate { .. } => panic!("an empty password must not use D-Bus"),
        }
    }
}

#[test]
fn wireless_settings_carry_the_psk_and_raw_ssid() {
    use zbus::zvariant::Value;
    let settings = network::wireless_settings("Café", "secret", true);
    assert_eq!(settings["802-11-wireless"]["ssid"], Value::from("Café".as_bytes()));
    let security = &settings["802-11-wireless-security"];
    assert_eq!(security["psk"], Value::from("secret"));
    assert!(!security.contains_key("key-mgmt"));
    let hidden = network::wireless_settings("Hidden", "secret", false);
    assert_eq!(hidden["802-11-wireless-security"]["key-mgmt"], Value::from("wpa-psk"));
}

#[test]
fn activation_failure_reasons_are_readable() {
    assert_eq!(network::failure_reason(7), network::WRONG_PASSWORD);
    assert_eq!(network::failure_reason(8), network::WRONG_PASSWORD);
    assert_eq!(network::failure_reason(11), network::NO_ANSWER);
    assert_ne!(network::failure_reason(53), network::WRONG_PASSWORD);
    assert_eq!(network::failure_reason(0), network::failure_reason(999));
}

#[test]
fn nmcli_devices_pick_wifi_and_usable_ethernet() {
    let text = "wlan0:wifi:connected:Home\nlo:loopback:connected (externally):lo\nenp2s0:ethernet:unavailable:\nenp3s0:ethernet:connected:Wired 1\n";
    let (wifi, wired) = network::parse_devices(text);
    assert_eq!(wifi.as_deref(), Some("wlan0"));
    assert_eq!(wired.len(), 1);
    assert_eq!(wired[0].device, "enp3s0");
    assert!(wired[0].connected);
}

#[test]
fn xkb_rules_layout_section_is_parsed_and_sorted() {
    let text = "! model\n  pc105  Generic\n\n! layout\n  us              English (US)\n  ua              Ukrainian\n  al              Albanian\n\n! variant\n  intl            us: English (intl)\n";
    let layouts = input::parse_layouts(text);
    let codes: Vec<&str> = layouts.iter().map(|l| l.code.as_str()).collect();
    assert_eq!(codes, vec!["al", "us", "ua"]);
}

#[test]
fn xkb_sources_round_trip_with_variants() {
    let sources = input::split_sources("us,ua", ",winkeys");
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[1].variant, "winkeys");
    assert_eq!(input::join_sources(&sources), (String::from("us,ua"), String::from(",winkeys")));
    let plain = input::split_sources("us, de", "");
    assert_eq!(input::join_sources(&plain), (String::from("us,de"), String::new()));
}

#[test]
fn switch_option_is_replaced_without_touching_other_options() {
    let options = "caps:escape,grp:win_space_toggle,compose:ralt";
    assert_eq!(input::switch_option(options), Some("grp:win_space_toggle"));
    assert_eq!(
        input::replace_switch_option(options, "grp:alt_shift_toggle"),
        "grp:alt_shift_toggle,caps:escape,compose:ralt"
    );
    assert_eq!(input::replace_switch_option("", "grp:caps_toggle"), "grp:caps_toggle");
}

#[test]
fn system_parsers_read_proc_formats() {
    assert_eq!(
        system::parse_os_release("NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\n").as_deref(),
        Some("Arch Linux")
    );
    let (cpu, cores) = system::parse_cpuinfo(
        "processor\t: 0\nmodel name\t: AMD Ryzen 7 5700U\n\nprocessor\t: 1\nmodel name\t: AMD Ryzen 7 5700U\n",
    );
    assert_eq!(cpu, "AMD Ryzen 7 5700U");
    assert_eq!(cores, 2);
    assert_eq!(system::parse_meminfo("MemTotal:       16000000 kB\n"), 16_000_000 * 1024);
    assert_eq!(system::parse_df("1B-blocks Avail\n512000 128000\n"), (512_000, 128_000));
    assert_eq!(
        system::parse_passwd_name("artem:x:1000:1000:Artem T,,,:/home/artem:/bin/zsh\n", "artem").as_deref(),
        Some("Artem T")
    );
    assert_eq!(system::parse_passwd_name("root:x:0:0::/root:/bin/sh\n", "root"), None);
}

#[test]
fn byte_formatting() {
    use glide_settings::Lang;
    assert_eq!(system::format_bytes(512, Lang::En), "512 B");
    assert_eq!(system::format_bytes(1_500, Lang::En), "1.5 kB");
    assert_eq!(system::format_bytes(207_300_000_000, Lang::Uk), "207,3 ГБ", "the same units as the Files app");
    assert_eq!(system::format_bytes(999_960, Lang::En), "1.0 MB", "rounding never prints 1000.0 kB");
    assert_eq!(system::format_memory(16 * 1024 * 1024 * 1024, Lang::En), "16.0 GiB");
    assert_eq!(system::format_memory(33_000_000_000, Lang::Uk), "30,7 ГіБ");
}

#[test]
fn display_helpers() {
    assert_eq!(display::parse_mode("2560x1440\n1920x1080\n"), Some((2560, 1440)));
    assert_eq!(display::parse_mode("1920x1080i\n"), Some((1920, 1080)));
    assert_eq!(display::connector_name("card1-eDP-1"), Some("eDP-1"));
    assert_eq!(display::connector_name("card1"), None);
    assert_eq!(display::aspect_label(1920, 1080).as_deref(), Some("16:9"));
    assert_eq!(display::aspect_label(2560, 1600).as_deref(), Some("16:10"));
    assert_eq!(display::aspect_label(3440, 1440).as_deref(), Some("21:9"));
    assert_eq!(display::aspect_label(0, 1080), None);
    assert_eq!(display::scale_label(1.25), "125%");
}

#[test]
fn clock_parsing_matches_the_compositor() {
    assert_eq!(parse_clock("20:00"), Some(1200));
    assert_eq!(parse_clock("07:05"), Some(425));
    assert_eq!(parse_clock("24:00"), None);
    assert_eq!(parse_clock("7:5"), None);
    assert_eq!(parse_clock("nope"), None);
}

#[test]
fn empty_config_reports_compositor_defaults() {
    let config = ConfigStore::parse("").with_system_language(Lang::En);
    assert_eq!(config.appearance(), Appearance::Dark);
    assert_eq!(config.accent(), Accent::Azurit);
    assert_eq!(config.language(), Lang::En);
    assert_eq!(config.language_choice(), None);
    assert_eq!(config.wallpaper_fit(), WallpaperFit::Cover);
    assert!(config.animations_enabled());
    assert!((config.animation_speed() - 1.0).abs() < f64::EPSILON);
    let keyboard = glide_config::SystemKeyboard::detect();
    assert_eq!(config.xkb_layout(), keyboard.layout);
    assert_eq!(config.xkb_options(), keyboard.options);
    assert!(!config.xkb_layout().is_empty());
    assert_eq!(config.repeat_delay(), 200);
    assert_eq!(config.repeat_rate(), 25);
    assert!(config.tap_to_click());
    assert!(!config.natural_scroll());
    assert_eq!(config.accel_profile(), AccelProfile::Adaptive);
    assert_eq!(config.night_light_from(), "20:00");
    assert_eq!(config.night_light_to(), "07:00");
}

#[test]
fn edits_preserve_comments_and_unrelated_keys() {
    let text = "# my config\n[topbar]\nappearance = \"dark\" # keep me\nbattery-low-pct = 15\n\n[[keybinds]]\nmods = [\"super\"]\nkey = \"Return\"\naction = \"launcher\"\n";
    let mut config = ConfigStore::parse(text);
    config.set_appearance(Appearance::Light);
    config.set_accent(Accent::Cinnabar);
    assert!(config.take_dirty());
    let out = config.to_text();
    assert!(out.starts_with("# my config\n"));
    assert!(out.contains("appearance = \"light\" # keep me"));
    assert!(out.contains("accent = \"red\""));
    assert!(out.contains("battery-low-pct = 15"));
    assert!(out.contains("[[keybinds]]"));
    let reparsed = ConfigStore::parse(&out);
    assert_eq!(reparsed.appearance(), Appearance::Light);
    assert_eq!(reparsed.accent(), Accent::Cinnabar);
}

#[test]
fn setting_the_same_value_does_not_mark_dirty() {
    let mut config = ConfigStore::parse("[input]\ntap-to-click = true\n");
    config.set_bool("input", "tap-to-click", true);
    assert!(!config.take_dirty());
    config.set_bool("input", "tap-to-click", false);
    assert!(config.take_dirty());
    assert!(!config.take_dirty());
}

#[test]
fn every_accent_serialises_to_the_compositor_name() {
    let names: Vec<&str> = Accent::ALL.iter().map(|a| accent_key(*a)).collect();
    assert_eq!(names, vec!["blue", "teal", "green", "amber", "orange", "red", "purple", "graphite"]);
    for accent in Accent::ALL {
        let mut config = ConfigStore::parse("");
        config.set_accent(*accent);
        assert_eq!(ConfigStore::parse(&config.to_text()).accent(), *accent);
    }
}

#[test]
fn output_scale_updates_existing_table_or_appends_one() {
    let mut config = ConfigStore::parse("[[output]]\nname = \"eDP-1\"\nscale = 1.5\n");
    assert_eq!(config.output_scale("eDP-1"), Some(1.5));
    config.set_output_scale("eDP-1", 1.25);
    config.set_output_scale("DP-2", 2.0);
    let reparsed = ConfigStore::parse(&config.to_text());
    assert_eq!(reparsed.output_scale("eDP-1"), Some(1.25));
    assert_eq!(reparsed.output_scale("DP-2"), Some(2.0));
    assert_eq!(config.to_text().matches("[[output]]").count(), 2);
}

#[test]
fn integer_speed_is_read_as_float_and_floats_are_rounded() {
    let mut config = ConfigStore::parse("[animations]\nspeed = 2\n");
    assert!((config.animation_speed() - 2.0).abs() < f64::EPSILON);
    config.set_float("animations", "speed", 0.750_000_1);
    assert!(config.to_text().contains("speed = 0.75"));
    assert!((round_to(0.333, 0.05) - 0.35).abs() < 1e-9);
}

#[test]
fn broken_config_is_read_only_and_never_saved() {
    let mut config = ConfigStore::parse("[topbar\naccent = ");
    assert!(!config.writable());
    assert!(config.error().is_some());
    config.set_accent(Accent::Amber);
    assert!(!config.take_dirty());
    assert!(config.save().is_err());
}

#[test]
fn save_writes_atomically_and_creates_the_directory() {
    let dir = std::env::temp_dir().join(format!("glide-settings-test-{}", std::process::id()));
    let path = dir.join("glide").join("glide.toml");
    let _ = std::fs::remove_dir_all(&dir);
    let mut config = ConfigStore::load_from(&path);
    assert!(config.writable());
    config.set_wallpaper_fit(WallpaperFit::Tile);
    config.save().expect("save succeeds");
    let written = std::fs::read_to_string(&path).expect("file exists");
    assert!(written.contains("wallpaper-fit = \"tile\""));
    let leftovers = std::fs::read_dir(path.parent().expect("has parent")).expect("dir exists").count();
    assert_eq!(leftovers, 1);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn removing_a_wallpaper_drops_the_key() {
    let mut config = ConfigStore::parse("[general]\nwallpaper = \"/tmp/a.jpg\"\n");
    assert!(config.wallpaper().is_some());
    config.set_wallpaper(None);
    assert!(config.take_dirty());
    assert!(config.wallpaper().is_none());
    assert!(!config.to_text().contains("wallpaper ="));
}

#[test]
fn string_arrays_round_trip_and_keep_order() {
    let mut config = ConfigStore::parse("[general]\nautostart = [\"a\", \"b\"]\n");
    assert_eq!(config.autostart(), vec!["a", "b"]);
    let next = vec![String::from("b"), String::from("c --flag")];
    config.set_str_array("general", "autostart", &next);
    assert!(config.take_dirty());
    assert_eq!(ConfigStore::parse(&config.to_text()).autostart(), next);
    config.set_str_array("general", "autostart", &next);
    assert!(!config.take_dirty());
}

#[test]
fn optional_strings_are_removed_when_cleared() {
    let mut config = ConfigStore::parse("");
    config.set_optional_str("dock", "icon-theme", Some("Papirus"));
    assert_eq!(config.optional_str("dock", "icon-theme").as_deref(), Some("Papirus"));
    config.set_optional_str("dock", "icon-theme", None);
    assert_eq!(config.optional_str("dock", "icon-theme"), None);
    config.set_optional_str("general", "font-family", Some("  "));
    assert!(!config.to_text().contains("font-family"));
}

#[test]
fn dock_icon_size_scales_with_item_size() {
    use glide_settings::services::config::{DEFAULT_ICON_SIZE, DEFAULT_ITEM_SIZE, icon_size_for};
    assert_eq!(icon_size_for(DEFAULT_ITEM_SIZE), DEFAULT_ICON_SIZE);
    assert_eq!(icon_size_for(64), 50);
    assert_eq!(icon_size_for(40), 32);
    for item in (40..=80).step_by(2) {
        let icon = icon_size_for(item);
        assert_eq!((item - icon) % 2, 0, "uneven margin for item size {item}");
        assert!((icon * DEFAULT_ITEM_SIZE - item * DEFAULT_ICON_SIZE).abs() <= DEFAULT_ITEM_SIZE);
    }
}

#[test]
fn animation_speed_slider_is_the_inverse_of_the_duration_multiplier() {
    use glide_settings::services::config::{SPEED_MAX, SPEED_MIN, multiplier_from_speed, speed_from_multiplier};
    assert!((multiplier_from_speed(2.0) - 0.5).abs() < 1e-9);
    assert!((multiplier_from_speed(0.5) - 2.0).abs() < 1e-9);
    assert!((speed_from_multiplier(0.5) - 2.0).abs() < 1e-9);
    assert!((speed_from_multiplier(1.25) - 0.8).abs() < 1e-9);
    assert!((speed_from_multiplier(0.0) - SPEED_MAX).abs() < 1e-9);
    assert!((speed_from_multiplier(10.0) - SPEED_MIN).abs() < 1e-9);
    let mut step = 10;
    while step <= 40 {
        let speed = f64::from(step) * 0.05;
        let round_trip = speed_from_multiplier(multiplier_from_speed(speed));
        assert!((round_trip - speed).abs() < 1e-9, "{speed} came back as {round_trip}");
        step += 1;
    }
    let mut config = ConfigStore::parse("");
    config.set_float("animations", "speed", multiplier_from_speed(2.0));
    assert!(config.to_text().contains("speed = 0.5"));
}

#[test]
fn peripheral_batteries_are_ignored_and_bat_names_come_first() {
    use glide_settings::services::power;
    assert!(power::is_system_battery("Battery\n", "System\n"));
    assert!(power::is_system_battery("Battery", ""));
    assert!(!power::is_system_battery("Battery", "Device\n"));
    assert!(!power::is_system_battery("Mains", ""));
    let mut names = vec![String::from("hidpp_battery_0"), String::from("BAT1"), String::from("BAT0")];
    power::order_batteries(&mut names);
    assert_eq!(names, vec!["BAT0", "BAT1", "hidpp_battery_0"]);
}

#[test]
fn reload_distinguishes_a_rejected_config_from_success() {
    use glide_ipc::Response;
    use glide_settings::services::ipc::{ReloadError, reload_outcome};
    assert_eq!(reload_outcome(Response::Ok), Ok(()));
    assert_eq!(
        reload_outcome(Response::Error { message: String::from("expected `]`") }),
        Err(ReloadError::Rejected(String::from("expected `]`")))
    );
}

#[test]
fn io_and_parse_faults_are_reported_separately() {
    use glide_settings::services::config::ConfigFault;
    assert_eq!(ConfigStore::parse("[topbar\n").fault(), Some(ConfigFault::Parse));
    assert_eq!(ConfigStore::parse("").fault(), None);
    let dir = std::env::temp_dir().join(format!("glide-settings-fault-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("glide.toml")).expect("create dir");
    let config = ConfigStore::load_from(&dir.join("glide.toml"));
    assert_eq!(config.fault(), Some(ConfigFault::Io));
    assert!(!config.writable());
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn external_edits_are_reloaded_before_the_next_change() {
    let dir = std::env::temp_dir().join(format!("glide-settings-refresh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    let path = dir.join("glide.toml");
    std::fs::write(&path, "[general]\nlanguage = \"en\"\n").expect("seed");
    let mut config = ConfigStore::load_from(&path);
    assert!(!config.refresh_if_changed());
    std::fs::write(&path, "[general]\nlanguage = \"en\"\n\n[dock]\nautohide = \"never\"\n").expect("external edit");
    let stamp = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options().write(true).open(&path).and_then(|file| file.set_modified(stamp)).expect("bump mtime");
    assert!(config.refresh_if_changed());
    config.set_bool("dock", "enabled", false);
    config.save().expect("save");
    let written = std::fs::read_to_string(&path).expect("read back");
    assert!(written.contains("autohide = \"never\""));
    assert!(written.contains("enabled = false"));
    assert!(!config.refresh_if_changed());
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn saving_through_a_symlink_keeps_the_link() {
    let dir = std::env::temp_dir().join(format!("glide-settings-link-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    let target = dir.join("dotfiles.toml");
    let link = dir.join("glide.toml");
    std::fs::write(&target, "[general]\n").expect("seed");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    let mut config = ConfigStore::load_from(&link);
    config.set_bool("dock", "enabled", false);
    config.save().expect("save");
    assert!(std::fs::symlink_metadata(&link).expect("link exists").file_type().is_symlink());
    assert!(std::fs::read_to_string(&target).expect("target").contains("enabled = false"));
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn desktop_entries_parse_localised_names_and_skip_hidden() {
    use glide_settings::services::apps;
    let text = "[Desktop Entry]\nType=Application\nName=Files\nName[uk]=Файли\nExec=nautilus --new-window %U\nMimeType=inode/directory;application/x-7z-compressed;\n\n[Desktop Action new]\nName=New\nExec=nope\n";
    let app = apps::parse_desktop("org.gnome.Nautilus.desktop", text, "uk").expect("parses");
    assert_eq!(app.name, "Файли");
    assert_eq!(app.exec, "nautilus --new-window %U");
    assert_eq!(app.mime_types, vec!["inode/directory", "application/x-7z-compressed"]);
    let english = apps::parse_desktop("x.desktop", text, "en").expect("parses");
    assert_eq!(english.name, "Files");
    assert!(
        apps::parse_desktop("h.desktop", "[Desktop Entry]\nType=Application\nName=H\nExec=h\nHidden=true\n", "en")
            .is_none()
    );
    assert!(apps::parse_desktop("l.desktop", "[Desktop Entry]\nType=Link\nName=L\nExec=l\n", "en").is_none());
    assert_eq!(apps::strip_field_codes("firefox %u --private"), "firefox --private");
}

#[test]
fn role_handlers_match_any_of_the_role_mime_types() {
    use glide_settings::services::apps::{self, DesktopApp, Role};
    let app = |id: &str, mime: &[&str]| DesktopApp {
        id: id.to_string(),
        name: id.to_string(),
        exec: id.to_string(),
        mime_types: mime.iter().map(|m| (*m).to_string()).collect(),
        ..DesktopApp::default()
    };
    let mut hidden_chrome = app("com.google.Chrome.desktop", &["image/png", "application/pdf"]);
    hidden_chrome.no_display = true;
    let list = vec![
        app("firefox", &["text/html", "x-scheme-handler/https"]),
        app("eog", &["image/png"]),
        app("vim", &["text/plain"]),
        hidden_chrome,
    ];
    let web: Vec<&str> = apps::handlers(&list, Role::Web).iter().map(|a| a.id.as_str()).collect();
    assert_eq!(web, vec!["firefox"]);
    let options = |role: Role, current| apps::mime_options(&list, role.mime_types(), current);
    assert_eq!(options(Role::Images, None).len(), 1);
    let images = options(Role::Images, Some("com.google.Chrome.desktop"));
    assert_eq!(images[0].id, "com.google.Chrome.desktop");
    assert_eq!(images.len(), 2);
    let unknown = options(Role::Pdf, Some("okular.desktop"));
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].name, "okular");
    assert!(options(Role::Mail, None).is_empty());
}

#[test]
fn no_display_entries_are_kept_but_hidden_ones_are_dropped() {
    use glide_settings::services::apps;
    let nd =
        apps::parse_desktop("c.desktop", "[Desktop Entry]\nType=Application\nName=C\nExec=c\nNoDisplay=true\n", "en")
            .expect("NoDisplay entries remain valid handlers");
    assert!(nd.no_display);
    assert!(apps::visible(&[nd]).is_empty());
}

#[test]
fn font_families_are_deduplicated_and_sorted() {
    use glide_settings::services::apps;
    let fonts =
        apps::parse_font_families("Noto Sans,Noto Sans Medium\nInter\nnoto sans\n.Hidden\nDejaVu Sans\\,Mono\n");
    assert_eq!(fonts, vec!["DejaVu Sans", "Inter", "Noto Sans"]);
}

#[test]
fn battery_and_power_profiles_parse() {
    use glide_settings::services::power::{self, Profile};
    let battery = power::parse_battery("36\n", "Discharging\n").expect("parses");
    assert_eq!(battery.percent, 36);
    assert!(!battery.charging);
    assert!(power::parse_battery("100", "Full").expect("parses").full);
    assert!(power::parse_battery("", "Full").is_none());
    assert_eq!(Profile::from_key("balanced\n"), Some(Profile::Balanced));
    assert_eq!(Profile::from_key("turbo"), None);
}

#[test]
fn proc_input_devices_are_split_into_mice_and_touchpads() {
    use glide_settings::services::pointer::{self, PointerKind};
    let text = "I: Bus=0011 Vendor=0002 Product=0007 Version=01b1\n\
N: Name=\"SynPS/2 Synaptics TouchPad\"\n\
H: Handlers=mouse1 event7 \n\
\n\
I: Bus=0003 Vendor=046d Product=c52b Version=0111\n\
N: Name=\"Logitech USB Receiver Mouse\"\n\
H: Handlers=mouse0 event4 \n\
\n\
I: Bus=0011 Vendor=0001 Product=0001 Version=ab41\n\
N: Name=\"AT Translated Set 2 keyboard\"\n\
H: Handlers=sysrq kbd leds event3 \n\
\n\
N: Name=\"ELAN Touchscreen\"\n\
H: Handlers=mouse2 event9\n";
    let devices = pointer::parse_devices(text);
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].kind, PointerKind::Touchpad);
    assert_eq!(devices[0].name, "SynPS/2 Synaptics TouchPad");
    assert_eq!(devices[1].kind, PointerKind::Mouse);
    assert!(pointer::parse_devices("").is_empty());
    assert!(pointer::is_touchpad_name("Apple Inc. Magic Trackpad"));
    assert!(!pointer::is_touchpad_name("Razer DeathAdder"));
}

#[test]
fn brightnessctl_machine_output_yields_the_backlight() {
    let text = "intel_backlight,backlight,19200,20%,96000\n";
    let backlight = display::parse_backlight(text).expect("a backlight line");
    assert_eq!(backlight.device, "intel_backlight");
    assert_eq!(backlight.percent, 20);
    assert!(display::parse_backlight("input3::capslock,leds,0,0%,1\n").is_none());
    assert!(display::parse_backlight("").is_none());
}

#[test]
fn display_modes_group_into_resolutions_and_refresh_rates() {
    use glide_ipc::ModeInfo;
    let mode = |width, height, refresh_mhz| ModeInfo { width, height, refresh_mhz, preferred: false };
    let modes = [
        mode(1920, 1080, 60_000),
        mode(2560, 1440, 59_951),
        mode(2560, 1440, 143_912),
        mode(1920, 1080, 59_940),
        mode(1920, 1080, 59_938),
        mode(1280, 720, 60_000),
    ];
    assert_eq!(display::resolutions(&modes), vec![(2560, 1440), (1920, 1080), (1280, 720)]);
    assert_eq!(display::refresh_rates(&modes, (1920, 1080)), vec![60_000, 59_940]);
    assert_eq!(display::mode_key(&modes[2]), "2560x1440@143.912");
    assert_eq!(display::mode_key(&modes[0]), "1920x1080@60");
    assert_eq!(display::mode_key(&mode(800, 600, 0)), "800x600");
    let sysfs = display::parse_modes("1920x1080\n1920x1080\n1280x720i\n");
    assert_eq!(sysfs.len(), 2);
    assert!(sysfs[0].preferred && !sysfs[1].preferred);
}

#[test]
fn displays_line_up_left_to_right_by_logical_width() {
    let positions = display::row_positions(&[
        (String::from("DP-2"), 1920.0),
        (String::from("eDP-1"), 1280.0),
        (String::from("HDMI-A-1"), 1706.67),
    ]);
    assert_eq!(
        positions,
        vec![(String::from("DP-2"), 0, 0), (String::from("eDP-1"), 1920, 0), (String::from("HDMI-A-1"), 3200, 0),]
    );
    assert!(display::is_internal("eDP-1") && !display::is_internal("DP-1"));
    assert_eq!(display::diagonal_inches((300, 190)), Some(14.0));
    assert_eq!(display::diagonal_inches((0, 0)), None);
}

#[test]
fn night_tint_matches_the_compositor_curve_and_the_schedule_wraps() {
    assert_eq!(display::night_tint(6500), (1.0, 1.0, 1.0));
    let (_, green, blue) = display::night_tint(4000);
    assert!((0.75..0.9).contains(&green) && (0.55..0.75).contains(&blue));
    let warm = display::night_tint(2500);
    assert!(warm.1 < green && warm.2 < blue);
    assert_eq!(display::schedule_segments(20 * 60, 7 * 60), vec![(0, 7 * 60), (20 * 60, 24 * 60)]);
    assert_eq!(display::schedule_segments(9 * 60, 17 * 60), vec![(540, 1020)]);
    assert!(display::schedule_segments(600, 600).is_empty());
    assert_eq!(display::schedule_segments(0, 360), vec![(0, 360)]);
}

#[test]
fn battery_uevent_details_cover_energy_and_charge_reporting() {
    use glide_settings::services::power;
    let energy = power::parse_uevent(
        "POWER_SUPPLY_NAME=BAT0\nPOWER_SUPPLY_STATUS=Discharging\nPOWER_SUPPLY_TECHNOLOGY=Li-poly\nPOWER_SUPPLY_CYCLE_COUNT=312\nPOWER_SUPPLY_POWER_NOW=8500000\nPOWER_SUPPLY_ENERGY_FULL_DESIGN=57000000\nPOWER_SUPPLY_ENERGY_FULL=51300000\nPOWER_SUPPLY_ENERGY_NOW=25500000\nPOWER_SUPPLY_MANUFACTURER=SMP\nPOWER_SUPPLY_MODEL_NAME=5B10W13930\n",
    );
    let details = power::battery_details(&energy);
    assert_eq!(details.cycles, Some(312));
    assert_eq!(details.technology, "Li-poly");
    assert!((details.health().expect("health") - 90.0).abs() < 1e-9);
    assert!((details.hours_left().expect("time left") - 3.0).abs() < 1e-9);

    let charge = power::parse_uevent(
        "POWER_SUPPLY_STATUS=Charging\nPOWER_SUPPLY_VOLTAGE_NOW=12000000\nPOWER_SUPPLY_CURRENT_NOW=-2000000\nPOWER_SUPPLY_CHARGE_FULL=4000000\nPOWER_SUPPLY_CHARGE_FULL_DESIGN=5000000\nPOWER_SUPPLY_CHARGE_NOW=2000000\nPOWER_SUPPLY_CYCLE_COUNT=0\n",
    );
    let details = power::battery_details(&charge);
    assert_eq!(details.cycles, None, "a zero cycle count means unknown");
    assert!((details.power_w.expect("power") - 24.0).abs() < 1e-9);
    assert!((details.health().expect("health") - 80.0).abs() < 1e-9);
    assert!((details.hours_left().expect("time to full") - 1.0).abs() < 1e-9);

    let idle = power::battery_details(&power::parse_uevent("POWER_SUPPLY_STATUS=Full\n"));
    assert_eq!(idle.health(), None);
    assert_eq!(idle.hours_left(), None);
}

#[test]
fn logind_policy_reads_login_section_with_drop_in_overrides() {
    use glide_settings::services::power::{LidAction, parse_logind};
    let defaults = parse_logind([]);
    assert_eq!(defaults.lid, LidAction::Suspend);
    assert_eq!(defaults.power_key, LidAction::PowerOff);
    let main = "[Login]\n#HandleLidSwitch=ignore\nHandleLidSwitch=hibernate\nHandlePowerKey=suspend\n";
    let drop_in = "[Login]\nHandleLidSwitchDocked=lock\n[Other]\nHandlePowerKey=ignore\n";
    let policy = parse_logind([main, drop_in]);
    assert_eq!(policy.lid, LidAction::Hibernate);
    assert_eq!(policy.lid_on_ac, LidAction::Hibernate, "follows the lid unless set");
    assert_eq!(policy.lid_docked, LidAction::Lock);
    assert_eq!(policy.power_key, LidAction::Suspend);
    let explicit = parse_logind(["[Login]\nHandleLidSwitchExternalPower=ignore\n"]);
    assert_eq!(explicit.lid_on_ac, LidAction::Ignore);
}

#[test]
fn output_mode_and_positions_are_edited_in_place() {
    let mut config = ConfigStore::parse("# mine\n[[output]]\nname = \"eDP-1\"\nscale = 1.5\n");
    config.set_output_mode("eDP-1", Some("2560x1600@120"));
    config.set_output_positions(&[(String::from("eDP-1"), 0, 0), (String::from("DP-2"), 1706, 0)]);
    assert_eq!(config.output_mode("eDP-1").as_deref(), Some("2560x1600@120"));
    assert_eq!(config.output_position("DP-2"), Some((1706, 0)));
    assert_eq!(config.output_scale("eDP-1"), Some(1.5));
    let text = config.to_text();
    assert!(text.starts_with("# mine\n"));
    assert_eq!(text.matches("[[output]]").count(), 2);
    config.set_output_mode("eDP-1", None);
    assert_eq!(config.output_mode("eDP-1"), None);
}

#[test]
fn a_hung_helper_is_killed_at_its_deadline() {
    use glide_settings::services::run;
    use std::time::{Duration, Instant};
    let started = Instant::now();
    assert_eq!(run::output_within("sleep", &["30"], Duration::from_millis(200)), None);
    assert!(run::status_within("sleep", &["30"], Duration::from_millis(200)).is_err());
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(run::status_within("sh", &["-c", "echo hi"], Duration::from_secs(10)), Ok(String::from("hi")));
    assert_eq!(
        run::status_within("sh", &["-c", "echo nope >&2; exit 3"], Duration::from_secs(10)),
        Err(String::from("nope"))
    );
}

#[test]
fn every_default_keybind_is_its_own_default_even_with_shared_actions() {
    use glide_settings::services::keybinds::{self, Action};
    let binds = keybinds::defaults().to_vec();
    for (index, bind) in binds.iter().enumerate() {
        let default = keybinds::default_for(&binds, Some(index), &bind.action).expect("a default bind has a default");
        assert!(default.same_combo(bind), "{bind:?} is reported as changed");
    }
    let power = Action::Named(String::from("power-dialog"));
    let mut edited = binds;
    let second = edited
        .iter()
        .enumerate()
        .filter(|(_, bind)| bind.action == power)
        .map(|(index, _)| index)
        .nth(1)
        .expect("power-dialog has two defaults");
    edited[second].key = String::from("F12");
    let restored = keybinds::default_for(&edited, Some(second), &power).expect("has a default");
    assert_eq!(restored.key, "XF86PowerOff");
}

#[test]
fn recorded_keys_become_xkb_names_with_their_modifiers() {
    use glide_settings::panels::shortcuts::{Capture, capture, capture_event};
    use glide_settings::services::keybinds::{Mod, is_valid_key, key_label};
    use iced::keyboard::{
        Key, Modifiers,
        key::{Code, Named, Physical},
    };
    let combo = |mods: Vec<Mod>, key: &str| Capture::Combo(mods, key.to_string());
    let super_shift = Modifiers::LOGO | Modifiers::SHIFT;
    assert_eq!(capture(&Key::Character("t".into()), Modifiers::LOGO), combo(vec![Mod::Super], "t"));
    assert_eq!(capture(&Key::Named(Named::ArrowLeft), super_shift), combo(vec![Mod::Super, Mod::Shift], "Left"));
    assert_eq!(capture(&Key::Named(Named::F5), Modifiers::CTRL), combo(vec![Mod::Ctrl], "F5"));
    assert_eq!(capture(&Key::Named(Named::PageDown), Modifiers::ALT), combo(vec![Mod::Alt], "Next"));
    assert_eq!(capture(&Key::Character("-".into()), Modifiers::CTRL), combo(vec![Mod::Ctrl], "minus"));
    assert_eq!(capture(&Key::Named(Named::AudioVolumeUp), Modifiers::empty()), combo(vec![], "XF86AudioRaiseVolume"));
    assert_eq!(capture(&Key::Named(Named::MicrophoneToggle), Modifiers::empty()), combo(vec![], "XF86AudioMicMute"));
    assert_eq!(capture(&Key::Named(Named::PowerOff), Modifiers::empty()), combo(vec![], "XF86PowerOff"));
    assert_eq!(capture(&Key::Named(Named::Shift), Modifiers::SHIFT), Capture::Ignore, "a lone modifier waits");
    assert_eq!(capture(&Key::Named(Named::Escape), Modifiers::empty()), Capture::Stop);
    assert_eq!(capture(&Key::Named(Named::Escape), Modifiers::LOGO), combo(vec![Mod::Super], "Escape"));
    assert_eq!(
        capture_event(&Key::Character("ф".into()), Physical::Code(Code::KeyA), Modifiers::LOGO),
        combo(vec![Mod::Super], "a")
    );
    assert!(is_valid_key("Page_Up") && is_valid_key("Page_Down"));
    assert_eq!(key_label("Page_Up"), "Page Up");
    assert_eq!(key_label("Page_Down"), "Page Down");
}

#[test]
fn new_user_names_follow_the_shadow_utils_rules() {
    use glide_settings::services::accounts::{UserNameProblem, user_name_problem};
    assert_eq!(user_name_problem("olena"), None);
    assert_eq!(user_name_problem("_svc-2"), None);
    assert_eq!(user_name_problem(""), Some(UserNameProblem::Empty));
    assert_eq!(user_name_problem("2fast"), Some(UserNameProblem::BadStart));
    assert_eq!(user_name_problem("Olena"), Some(UserNameProblem::BadStart));
    assert_eq!(user_name_problem("ole.na"), Some(UserNameProblem::BadCharacter));
    assert_eq!(user_name_problem(&"a".repeat(33)), Some(UserNameProblem::TooLong));
}

#[test]
fn the_user_name_is_suggested_from_the_first_word() {
    use glide_settings::services::accounts::suggest_user_name;
    assert_eq!(suggest_user_name("Ada Lovelace"), "ada");
    assert_eq!(suggest_user_name("  Jean-Luc Picard"), "jeanluc");
    assert_eq!(suggest_user_name("Олена Пчілка"), "", "no Latin letters, no guess");
    assert_eq!(suggest_user_name("007 Bond"), "", "a name cannot start with a digit");
}

#[test]
fn passwords_must_match_and_stay_on_one_line() {
    use glide_settings::services::accounts::{PasswordProblem, password_problem};
    assert_eq!(password_problem("s3cret", "s3cret"), None);
    assert_eq!(password_problem("", ""), Some(PasswordProblem::Empty));
    assert_eq!(password_problem("s3cret", "s3cre"), Some(PasswordProblem::Mismatch));
    assert_eq!(password_problem("a\nb", "a\nb"), Some(PasswordProblem::LineBreak));
}

#[test]
fn passwords_are_hashed_with_sha512_crypt_over_stdin() {
    use glide_settings::services::accounts::hash_password;
    if !glide_settings::services::run::available("openssl") {
        eprintln!("no openssl; skipping");
        return;
    }
    let hash = hash_password("correct horse").expect("openssl hashes the password");
    assert!(hash.starts_with("$6$"), "{hash}");
    assert_ne!(hash, hash_password("correct horse").expect("hashed again"), "a fresh salt each time");
    assert!(hash_password("two\nlines").is_err());
}

#[test]
fn the_charge_limit_comes_from_a_supported_system_battery() {
    use glide_settings::services::power::{ChargeLimit, UpowerDevice, pick_charge_limit};
    let device = |path: &str, kind, power_supply, supported| UpowerDevice {
        path: path.to_string(),
        kind,
        power_supply,
        supported,
        enabled: true,
        start: 75,
        end: 80,
    };
    let devices = vec![
        device("/org/freedesktop/UPower/devices/line_power_AC", 1, true, false),
        device("/org/freedesktop/UPower/devices/mouse_hid", 2, false, true),
        device("/org/freedesktop/UPower/devices/battery_BAT0", 2, true, true),
    ];
    assert_eq!(
        pick_charge_limit(devices),
        Some(ChargeLimit {
            path: "/org/freedesktop/UPower/devices/battery_BAT0".into(),
            enabled: true,
            start: 75,
            end: 80
        })
    );
    assert_eq!(pick_charge_limit(vec![device("/b", 2, true, false)]), None, "no threshold support, no switch");
}

#[test]
fn cursor_themes_are_icon_directories_with_cursors() {
    let root = std::env::temp_dir().join(format!("glide-cursor-themes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (user, system) = (root.join("user"), root.join("system"));
    for dir in [
        user.join("Bibata-Modern-Ice/cursors"),
        system.join("Adwaita/cursors"),
        system.join("Adwaita/scalable"),
        system.join("Papirus/64x64"),
        system.join("default/cursors"),
    ] {
        std::fs::create_dir_all(dir).expect("scratch theme");
    }
    let themes = glide_settings::services::apps::cursor_themes_in(&[user, system.clone(), system]);
    assert_eq!(themes, ["Adwaita", "Bibata-Modern-Ice"]);
    std::fs::remove_dir_all(&root).expect("cleanup");
}

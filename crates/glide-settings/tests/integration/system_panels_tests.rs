use std::path::Path;

use glide_settings::services::{apps, autostart, locale, storage, system, timedate, tz};

fn tzif_v2(transitions: &[(i64, u8)], types: &[(i32, bool, &str)], footer: &str) -> Vec<u8> {
    let mut chars = Vec::new();
    let mut indices = Vec::new();
    for (_, _, abbr) in types {
        indices.push(u8::try_from(chars.len()).expect("small"));
        chars.extend_from_slice(abbr.as_bytes());
        chars.push(0);
    }
    let header = |out: &mut Vec<u8>, times: usize| {
        out.extend_from_slice(b"TZif2");
        out.extend_from_slice(&[0; 15]);
        for count in [0, 0, 0, times, types.len(), chars.len()] {
            out.extend_from_slice(&u32::try_from(count).expect("small").to_be_bytes());
        }
    };
    let mut out = Vec::new();
    header(&mut out, 0);
    for (index, (offset, dst, _)) in types.iter().enumerate() {
        out.extend_from_slice(&offset.to_be_bytes());
        out.push(u8::from(*dst));
        out.push(indices[index]);
    }
    out.extend_from_slice(&chars);
    header(&mut out, transitions.len());
    for (time, _) in transitions {
        out.extend_from_slice(&time.to_be_bytes());
    }
    for (_, kind) in transitions {
        out.push(*kind);
    }
    for (index, (offset, dst, _)) in types.iter().enumerate() {
        out.extend_from_slice(&offset.to_be_bytes());
        out.push(u8::from(*dst));
        out.push(indices[index]);
    }
    out.extend_from_slice(&chars);
    out.push(b'\n');
    out.extend_from_slice(footer.as_bytes());
    out.push(b'\n');
    out
}

#[test]
fn civil_conversions_round_trip() {
    assert_eq!(tz::days_from_civil(1970, 1, 1), 0);
    assert_eq!(tz::civil_from_days(0), (1970, 1, 1));
    assert_eq!(tz::civil_from_days(tz::days_from_civil(2024, 2, 29)), (2024, 2, 29));
    let when = tz::civil(tz::from_civil(2026, 9, 24, 14, 5, 9));
    assert_eq!((when.year, when.month, when.day), (2026, 9, 24));
    assert_eq!((when.hour, when.minute, when.second), (14, 5, 9));
    assert_eq!(when.weekday, 4);
    assert_eq!(tz::days_in_month(2100, 2), 28);
    assert_eq!(tz::days_in_month(2000, 2), 29);
    assert_eq!(tz::format_offset(10_800), "UTC+03:00");
    assert_eq!(tz::format_offset(-12_600), "UTC−03:30");
}

#[test]
fn posix_footer_rules_follow_european_daylight_saving() {
    let rule = tz::parse_rule("EET-2EEST,M3.5.0/3,M10.5.0/4").expect("rule");
    let winter = rule.at(tz::from_civil(2026, 1, 15, 12, 0, 0));
    assert_eq!((winter.offset, winter.abbr.as_str()), (7200, "EET"));
    let summer = rule.at(tz::from_civil(2026, 7, 1, 12, 0, 0));
    assert_eq!((summer.offset, summer.dst), (10_800, true));
    let before = rule.at(tz::from_civil(2026, 3, 29, 0, 59, 59));
    let after = rule.at(tz::from_civil(2026, 3, 29, 1, 0, 0));
    assert_eq!((before.offset, after.offset), (7200, 10_800));
    let back = rule.at(tz::from_civil(2026, 10, 25, 1, 0, 0));
    assert_eq!(back.offset, 7200);
    let fixed = tz::parse_rule("<+0530>-5:30").expect("fixed");
    assert_eq!(fixed.at(0).offset, 19_800);
    assert_eq!(fixed.at(0).abbr, "+0530");
    let southern = tz::parse_rule("AEST-10AEDT,M10.1.0,M4.1.0/3").expect("rule");
    assert!(southern.at(tz::from_civil(2026, 1, 10, 0, 0, 0)).dst);
    assert!(!southern.at(tz::from_civil(2026, 7, 10, 0, 0, 0)).dst);
}

#[test]
fn tzif_files_use_transitions_then_the_footer() {
    let bytes = tzif_v2(
        &[(0, 1)],
        &[(3600, false, "OLD"), (7200, false, "EET"), (10_800, true, "EEST")],
        "EET-2EEST,M3.5.0/3,M10.5.0/4",
    );
    let zone = tz::Zone::parse(&bytes).expect("parses");
    assert_eq!(zone.at(-10).abbr, "OLD");
    assert_eq!(zone.at(tz::from_civil(2026, 7, 1, 0, 0, 0)).abbr, "EEST");
    let local = tz::from_civil(2026, 7, 1, 12, 0, 0);
    let utc = zone.to_utc(local);
    assert_eq!(utc, local - 10_800);
    assert_eq!(zone.local(utc).hour, 12);
    assert!(tz::Zone::parse(b"nope").is_none());
    assert!(tz::Zone::load("../etc/passwd").is_none());
}

#[test]
fn zone_names_and_tab_files_parse() {
    assert_eq!(tz::city_name("America/Argentina/Buenos_Aires"), "Buenos Aires");
    assert_eq!(tz::region_name("America/Argentina/Buenos_Aires"), "America · Argentina");
    assert_eq!(tz::region_name("UTC"), "");
    let tab = "# comment\nUA\t+5026+03031\tEurope/Kyiv\nJP\t+353916+1394441\tAsia/Tokyo\tMost\n";
    assert_eq!(tz::parse_zone_tab(tab), vec!["Asia/Tokyo", "Europe/Kyiv"]);
}

#[test]
fn timedatectl_show_and_manual_entry_parse() {
    let state = timedate::parse_show(
        "Timezone=Europe/Kyiv\nLocalRTC=no\nCanNTP=yes\nNTP=yes\nNTPSynchronized=no\nTimeUSec=Thu\n",
    );
    assert!(state.available && state.can_ntp && state.ntp && !state.synchronized);
    assert!(!state.local_rtc);
    assert_eq!(state.timezone, "Europe/Kyiv");
    assert!(!timedate::parse_show("").available);
    assert_eq!(timedate::zone_from_link("../usr/share/zoneinfo/Europe/Kyiv").as_deref(), Some("Europe/Kyiv"));
    assert_eq!(timedate::parse_date("2024-02-29"), Some((2024, 2, 29)));
    assert_eq!(timedate::parse_date("2025-02-29"), None);
    assert_eq!(timedate::parse_time("7:05"), Some((7, 5, 0)));
    assert_eq!(timedate::parse_time("07:5"), None);
    assert_eq!(timedate::parse_time("23:59:59"), Some((23, 59, 59)));
    assert_eq!(timedate::parse_time("24:00"), None);
    assert!(timedate::uses_12_hour(None, Some("en_US.UTF-8"), Some("uk_UA.UTF-8")));
    assert!(!timedate::uses_12_hour(Some("uk_UA.UTF-8"), Some("en_US.UTF-8"), None));
}

#[test]
fn zone_search_matches_city_region_and_offset() {
    let entry = timedate::ZoneEntry {
        name: String::from("America/New_York"),
        city: String::from("New York"),
        region: String::from("America"),
        offset: -14_400,
        abbr: String::from("EDT"),
    };
    assert!(timedate::matches(&entry, "new york"));
    assert!(timedate::matches(&entry, "america/new"));
    assert!(timedate::matches(&entry, "edt"));
    assert!(timedate::matches(&entry, "utc−04"));
    assert!(!timedate::matches(&entry, "kyiv"));
}

#[test]
fn locale_status_and_lists_parse() {
    let status = locale::parse_status(
        "System Locale: LANG=en_US.UTF-8\n                LC_TIME=uk_UA.UTF-8\n    VC Keymap: ua-utf\n",
    );
    assert_eq!(status.lang(), "en_US.UTF-8");
    assert_eq!(status.effective("LC_TIME"), "uk_UA.UTF-8");
    assert_eq!(status.effective("LC_PAPER"), "en_US.UTF-8");
    let cleared = status.with("LC_TIME", None);
    assert_eq!(cleared.assignments(), vec!["LANG=en_US.UTF-8"]);
    assert_eq!(
        locale::parse_locale_list("C.utf8\nen_US.utf8\nPOSIX\nuk_UA.UTF-8\nen_US.UTF-8\n"),
        vec!["C.UTF-8", "en_US.UTF-8", "uk_UA.UTF-8"]
    );
    assert_eq!(locale::normalise("sr_RS.utf8@latin"), "sr_RS.UTF-8@latin");
}

#[test]
fn locale_keywords_become_formats_and_previews() {
    let text = "abday=\"Sun;Mon;Tue;Wed;Thu;Fri;Sat\"\nday=\"Sunday;Monday;Tuesday;Wednesday;Thursday;Friday;Saturday\"\nabmon=\"Jan;Feb;Mar;Apr;May;Jun;Jul;Aug;Sep;Oct;Nov;Dec\"\nmon=\"January;February;March;April;May;June;July;August;September;October;November;December\"\nam_pm=\"AM;PM\"\nd_t_fmt=\"%a %d %b %Y %r %Z\"\nd_fmt=\"%m/%d/%Y\"\nt_fmt=\"%r\"\nweek-1stday=19971130\nfirst_weekday=1\nmeasurement=2\nheight=279\nwidth=216\ndecimal_point=\".\"\nthousands_sep=\",\"\ngrouping=3\nint_curr_symbol=\"USD \"\ncurrency_symbol=\"$\"\nmon_decimal_point=\".\"\nmon_thousands_sep=\",\"\nmon_grouping=3;3\nfrac_digits=2\np_cs_precedes=1\np_sep_by_space=0\nlanguage=\"American English\"\nterritory=\"United States\"\n";
    let formats = locale::formats_from("en_US.UTF-8", &locale::parse_keywords(text));
    assert_eq!(formats.first_weekday, 0);
    assert!(!formats.metric);
    assert_eq!(locale::paper_name(formats.paper.0, formats.paper.1), "Letter");
    assert_eq!(locale::format_number(1_234_567.891, 2, &formats), "1,234,567.89");
    assert_eq!(locale::format_money(1234.5, &formats), "$1,234.50");
    let when = tz::civil(tz::from_civil(2026, 9, 24, 14, 5, 9));
    assert_eq!(locale::strftime(&formats.date, &when, &formats, "EEST"), "09/24/2026");
    assert_eq!(locale::strftime(&formats.time, &when, &formats, "EEST"), "02:05:09 PM");
    assert_eq!(locale::strftime(&formats.date_time, &when, &formats, "EEST"), "Thu 24 Sep 2026 02:05:09 PM EEST");
    assert_eq!(locale::display_name(Some(&formats), "en_US.UTF-8"), "American English (United States)");
    let mut uk = formats;
    uk.thousands_sep = String::from("\u{a0}");
    uk.decimal_point = String::from(",");
    uk.money_thousands = String::from("\u{a0}");
    uk.money_decimal = String::from(",");
    uk.currency = String::from("₴");
    uk.symbol_first = false;
    uk.symbol_space = true;
    assert_eq!(locale::format_number(-1234.5, 1, &uk), "-1\u{a0}234,5");
    assert_eq!(locale::format_money(1234.5, &uk), "1\u{a0}234,50\u{a0}₴");
    assert_eq!(locale::first_weekday("19971130", "2"), 1);
}

#[test]
fn mimeapps_lists_resolve_defaults_in_order() {
    let user = apps::parse_mimeapps(
        "[Default Applications]\ntext/plain=gone.desktop;vim.desktop;\n[Removed Associations]\nimage/png=eog.desktop;\n",
    );
    let system = apps::parse_mimeapps(
        "[Default Applications]\ntext/plain=gedit.desktop\n[Added Associations]\nimage/png=eog.desktop;gimp.desktop;\n",
    );
    let app = |id: &str, mime: &[&str]| apps::DesktopApp {
        id: id.to_string(),
        name: id.to_string(),
        mime_types: mime.iter().map(|m| (*m).to_string()).collect(),
        ..apps::DesktopApp::default()
    };
    let installed = vec![
        app("vim.desktop", &["text/plain"]),
        app("gedit.desktop", &["text/plain"]),
        app("eog.desktop", &["image/png"]),
        app("gimp.desktop", &["image/png"]),
        app("mpv.desktop", &["video/mp4"]),
    ];
    let lists = vec![user, system];
    assert_eq!(apps::resolve_default(&lists, &installed, "text/plain").as_deref(), Some("vim.desktop"));
    assert_eq!(apps::resolve_default(&lists, &installed, "image/png").as_deref(), Some("gimp.desktop"));
    assert_eq!(apps::resolve_default(&lists, &installed, "video/mp4").as_deref(), Some("mpv.desktop"));
    assert!(apps::explicit_default(&lists, "text/plain"));
    assert!(!apps::explicit_default(&lists, "video/mp4"));
}

#[test]
fn resetting_defaults_only_touches_managed_keys() {
    let text = "[Default Applications]\ntext/plain=vim.desktop\nx-scheme-handler/steam=steam.desktop\n\n[Added Associations]\ntext/plain=vim.desktop;\n";
    let (next, removed) = apps::strip_defaults(text, &["text/plain"]);
    assert_eq!(removed, 1);
    assert!(next.contains("x-scheme-handler/steam=steam.desktop"));
    assert!(next.contains("[Added Associations]\ntext/plain=vim.desktop;"));
    assert!(!next.contains("[Default Applications]\ntext/plain"));
    assert!(apps::managed_mime_types().contains(&"text/calendar"));
}

#[test]
fn shared_mime_comments_prefer_the_language() {
    let xml = "<mime-type type=\"application/pdf\">\n  <comment>PDF document</comment>\n  <comment xml:lang=\"uk\">документ PDF</comment>\n  <comment xml:lang=\"de\">PDF-Dokument</comment>\n</mime-type>\n";
    assert_eq!(apps::parse_mime_comment(xml, "uk").as_deref(), Some("документ PDF"));
    assert_eq!(apps::parse_mime_comment(xml, "en").as_deref(), Some("PDF document"));
    assert_eq!(apps::parse_mime_comment("<x/>", "en"), None);
}

#[test]
fn autostart_entries_parse_and_toggle_with_hidden() {
    let text = "[Desktop Entry]\nType=Application\nName=Network\nName[uk]=Мережа\nExec=nm-applet %U\nOnlyShowIn=GNOME;XFCE;\nX-GNOME-Autostart-enabled=true\n";
    let fields = autostart::parse_fields(text);
    let entry = autostart::build_entry(
        "nm-applet.desktop",
        &fields,
        "uk",
        &[String::from("glide")],
        &|_| true,
        autostart::Provided::ALL,
    );
    assert_eq!(entry.name, "Мережа");
    assert_eq!(entry.exec, "nm-applet");
    assert!(entry.enabled && entry.runnable && !entry.desktop_ok && entry.replaced);
    assert_eq!(entry.only_in, vec!["GNOME", "XFCE"]);
    let off = autostart::enabled_text(text, false);
    assert!(off.contains("Hidden=true"));
    assert!(!off.contains("X-GNOME-Autostart-enabled"));
    assert!(autostart::enabled_text(&off, true).find("Hidden").is_none());
    assert_eq!(
        autostart::set_key("[Desktop Entry]\nName=a\n[Desktop Action x]\nName=b\n", "Hidden", Some("true")),
        "[Desktop Entry]\nName=a\nHidden=true\n[Desktop Action x]\nName=b\n"
    );
}

#[test]
fn autostart_entries_for_applets_the_shell_replaces_do_not_count_as_starting() {
    let build = |exec: &str| {
        let text = format!("[Desktop Entry]\nType=Application\nName=Applet\nExec={exec}\nNotShowIn=KDE;GNOME;\n");
        autostart::build_entry(
            "applet.desktop",
            &autostart::parse_fields(&text),
            "en",
            &[String::from("glide")],
            &|_| true,
            autostart::Provided::ALL,
        )
    };
    for exec in ["nm-applet", "/usr/bin/blueman-applet %U", "/usr/bin/netbird-ui"] {
        let entry = build(exec);
        assert!(entry.enabled && entry.desktop_ok && entry.runnable, "{exec}");
        assert!(entry.replaced && !entry.starts(), "{exec}");
    }
    let other = build("syncthing serve");
    assert!(!other.replaced && other.starts());
}

#[test]
fn autostart_scan_prefers_user_overrides() {
    let root = std::env::temp_dir().join(format!("glide-settings-autostart-{}", std::process::id()));
    let user = root.join("user");
    let system = root.join("system");
    std::fs::create_dir_all(&user).expect("user");
    std::fs::create_dir_all(&system).expect("system");
    std::fs::write(system.join("a.desktop"), "[Desktop Entry]\nType=Application\nName=Alpha\nExec=alpha\n").expect("a");
    std::fs::write(
        user.join("a.desktop"),
        "[Desktop Entry]\nType=Application\nName=Alpha\nExec=alpha\nHidden=true\nX-Glide-Override=true\n",
    )
    .expect("override");
    std::fs::write(user.join("mine.desktop"), "[Desktop Entry]\nType=Application\nName=Mine\nExec=missing-tool\n")
        .expect("mine");
    let entries = autostart::scan_dirs(
        Some(&user),
        std::slice::from_ref(&system),
        "en",
        &[String::from("glide")],
        &|program| program == "alpha",
        autostart::Provided::ALL,
    );
    assert_eq!(entries.len(), 2);
    let alpha = &entries[0];
    assert_eq!(alpha.name, "Alpha");
    assert!(!alpha.enabled && !alpha.removable());
    let mine = &entries[1];
    assert!(mine.removable() && !mine.runnable);
    std::fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn df_output_drops_pseudo_file_systems() {
    let text = "Filesystem Type 1B-blocks Used Avail Mounted on\n/dev/nvme0n1p2 ext4 500000 200000 300000 /\ntmpfs tmpfs 100 1 99 /tmp\n/dev/nvme0n1p1 vfat 1000 100 900 /boot\n/dev/sda1 btrfs 9000 3000 6000 /mnt/My Disk\n/dev/nvme0n1p2 ext4 500000 200000 300000 /home\n";
    let volumes = storage::parse_volumes(text);
    let mounts: Vec<&str> = volumes.iter().map(|v| v.mount.as_str()).collect();
    assert_eq!(mounts, vec!["/", "/boot", "/mnt/My Disk"]);
    assert!((volumes[0].fraction() - 0.4).abs() < 1e-6);
}

#[test]
fn user_dirs_and_categories_resolve_inside_home() {
    let home = Path::new("/home/me");
    let dirs = storage::parse_user_dirs(
        "# comment\nXDG_DOCUMENTS_DIR=\"$HOME/Документи\"\nXDG_MUSIC_DIR=\"/srv/music\"\nXDG_DESKTOP_DIR=\"$HOME/\"\n",
        home,
    );
    assert_eq!(dirs["XDG_DOCUMENTS_DIR"], home.join("Документи"));
    let roots = storage::category_roots(home, &dirs);
    assert!(roots.iter().any(|(c, p)| *c == storage::Category::Documents && p == &home.join("Документи")));
    assert!(!roots.iter().any(|(c, _)| *c == storage::Category::Music));
    assert!(!roots.iter().any(|(c, _)| *c == storage::Category::Desktop));
}

#[test]
fn home_measurement_attributes_bytes_to_categories() {
    let root = std::env::temp_dir().join(format!("glide-settings-du-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("Docs/deep")).expect("docs");
    std::fs::create_dir_all(root.join("rest")).expect("rest");
    std::fs::write(root.join("Docs/deep/a.bin"), vec![1u8; 64 * 1024]).expect("a");
    std::fs::write(root.join("rest/b.bin"), vec![1u8; 16 * 1024]).expect("b");
    let usage = storage::measure_tree(&root, &[(storage::Category::Documents, root.join("Docs"))]);
    let docs = usage.size(storage::Category::Documents);
    let other = usage.size(storage::Category::Other);
    assert!(docs >= 64 * 1024, "{docs}");
    assert!(other >= 16 * 1024 && other < docs, "{other}");
    assert_eq!(usage.total, usage.sizes.iter().map(|(_, s)| s).sum::<u64>());
    std::fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn hostnames_battery_and_firmware_helpers() {
    assert_eq!(system::static_hostname_for("Artem's Laptop"), "artems-laptop");
    assert_eq!(system::static_hostname_for("  --Ноутбук--  "), "");
    assert_eq!(
        system::plan_host_names("Artem's Laptop", "laptop"),
        Some((String::from("artems-laptop"), String::from("Artem's Laptop")))
    );
    assert_eq!(system::plan_host_names("Ноутбук", "laptop"), Some((String::from("laptop"), String::from("Ноутбук"))));
    assert_eq!(system::plan_host_names("work-pc", "laptop"), Some((String::from("work-pc"), String::new())));
    assert_eq!(system::plan_host_names("   ", "laptop"), None);
    assert!(!system::valid_static_hostname("-bad"));
    assert_eq!(system::battery_health(Some(4_487_000), Some(4_661_000)), Some(96));
    assert_eq!(system::battery_health(Some(10), Some(0)), None);
    assert_eq!(system::clean_dmi("To Be Filled By O.E.M.\n"), "");
    assert_eq!(system::clean_dmi("Acer\n"), "Acer");
    assert_eq!(system::os_release_value("ID=arch\nLOGO=archlinux-logo\n", "LOGO").as_deref(), Some("archlinux-logo"));
    assert_eq!(system::parse_mem_available("MemAvailable:  2048 kB\n"), 2048 * 1024);
}

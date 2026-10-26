use std::path::Path;

use glide_settings::app::{App, Message};
use glide_settings::services::config::ConfigStore;
use glide_settings::services::{accounts, bluetooth, gsettings, network, privacy, vpn};

fn app(text: &str) -> App {
    App::with_config(ConfigStore::parse(text))
}

#[test]
fn notification_settings_write_the_keys_the_compositor_reads() {
    use glide_settings::panels::notifications::Msg;
    let mut app = app("");
    let banners = |key: &str, name: &str, on| Message::Notifications(Msg::Banners(key.into(), name.into(), on));
    let _ = app.update(Message::Notifications(Msg::Dnd(true)));
    let _ = app.update(Message::Notifications(Msg::Schedule(true)));
    let _ = app.update(Message::Notifications(Msg::From("23:15".to_string())));
    let _ = app.update(Message::Notifications(Msg::To("7:5".to_string())));
    let _ = app.update(Message::Notifications(Msg::AllowCritical(false)));
    let _ = app.update(banners("org.telegram.desktop", "Telegram", false));
    let _ = app.update(banners("Steam", "Steam", false));
    let _ = app.update(banners("steam", "Steam", false));
    let _ = app.update(Message::Notifications(Msg::Sound("firefox".into(), "Firefox".into(), false)));
    let _ = app.update(Message::Notifications(Msg::System("drive-notices", false)));
    let config = app.config();
    assert_eq!(config.get_bool("notifications", "do-not-disturb"), Some(true));
    assert_eq!(config.get_bool("notifications", "schedule"), Some(true));
    assert_eq!(config.get_str("notifications", "from"), Some("23:15"));
    assert_eq!(config.get_str("notifications", "to"), None, "an invalid time is never written");
    assert_eq!(config.get_bool("notifications", "allow-critical"), Some(false));
    assert_eq!(
        config.get_str_array("notifications", "muted-apps"),
        vec!["org.telegram.desktop", "steam"],
        "an app is listed once whatever its case"
    );
    assert_eq!(config.get_str_array("notifications", "silent-apps"), vec!["firefox"]);
    assert_eq!(config.get_bool("notifications", "drive-notices"), Some(false));
    let _ = app.update(banners("steam", "Steam", true));
    let _ = app.update(Message::Notifications(Msg::System("drive-notices", true)));
    assert_eq!(app.config().get_str_array("notifications", "muted-apps"), vec!["org.telegram.desktop"]);
    assert_eq!(app.config().get_bool("notifications", "drive-notices"), None, "the default is not written");
}

#[test]
fn cursor_size_goes_to_the_accessibility_section_and_default_removes_it() {
    use glide_settings::panels::accessibility::Msg;
    let mut app = app("");
    let _ = app.update(Message::Accessibility(Msg::CursorSize(48)));
    assert_eq!(app.config().get_int("accessibility", "cursor-size"), Some(48));
    let _ = app.update(Message::Accessibility(Msg::CursorSize(24)));
    assert_eq!(app.config().get_int("accessibility", "cursor-size"), None);
    let _ = app.update(Message::Accessibility(Msg::FollowSystem));
    assert_eq!(app.config().get_bool("animations", "respect-system"), Some(true));
}

#[test]
fn text_scale_snaps_to_the_nearest_offered_size() {
    use glide_settings::panels::accessibility::nearest_scale;
    assert_eq!(nearest_scale(1.0), 0);
    assert_eq!(nearest_scale(1.3), 1);
    assert_eq!(nearest_scale(3.0), 3);
}

#[test]
fn the_lock_screen_page_owns_the_blank_timeout() {
    use glide_settings::panels::lock_screen::Msg;
    let mut app = app("[general]\nblank-after-seconds = 600\n");
    let choice = privacy_choice(300);
    let _ = app.update(Message::LockScreen(Msg::Blank(choice)));
    assert_eq!(app.config().get_int("general", "blank-after-seconds"), Some(300));
}

#[test]
fn the_idle_lock_and_sleep_timeouts_are_written_to_the_general_table() {
    use glide_settings::panels::{lock_screen, power};
    let mut app = app("[general]\nblank-after-seconds = 600\n");
    let _ = app.update(Message::LockScreen(lock_screen::Msg::LockAfter(privacy_choice(-1))));
    assert_eq!(app.config().get_int("general", "lock-after-seconds"), Some(-1), "never lock");
    let _ = app.update(Message::LockScreen(lock_screen::Msg::LockAfter(privacy_choice(30))));
    assert_eq!(app.config().get_int("general", "lock-after-seconds"), Some(30));
    let hour = power::Blank { seconds: 3600, label: String::new() };
    let _ = app.update(Message::Power(power::Msg::Suspend(hour.clone())));
    let _ = app.update(Message::Power(power::Msg::SuspendOnBattery(power::Blank { seconds: 900, ..hour })));
    assert_eq!(app.config().get_int("general", "suspend-after-seconds"), Some(3600));
    assert_eq!(app.config().get_int("general", "suspend-on-battery-after-seconds"), Some(900));
}

fn privacy_choice(value: i64) -> glide_settings::panels::privacy::Choice {
    glide_settings::panels::privacy::Choice::new(value, "5 min")
}

#[test]
fn hidden_network_and_hotspot_forms_validate_before_connecting() {
    use glide_settings::panels::network::{Msg, Page};
    let mut page = Page::default();
    let _ = page.update(Msg::HiddenSsid("Lab".to_string()));
    assert!(page.hidden_ready(), "an open hidden network needs only a name");
    let _ = page.update(Msg::HiddenSecure(true));
    assert!(!page.hidden_ready());
    let _ = page.update(Msg::HiddenPassword(network::Secret("12345678".to_string())));
    assert!(page.hidden_ready());
    let _ = page.update(Msg::HotspotSsid("Glide".to_string()));
    let _ = page.update(Msg::HotspotPassword(network::Secret("short".to_string())));
    assert!(!page.hotspot_ready());
    let _ = page.update(Msg::GeneratePassword);
    assert!(page.hotspot_ready());
}

#[test]
fn switching_the_proxy_off_does_not_need_a_port() {
    use glide_settings::panels::network::{Msg, Page};
    let mut page = Page::default();
    let _ = page.update(Msg::ProxyMode(network::ProxyMode::Manual));
    let _ = page.update(Msg::ProxyPort(0, "80a80".to_string()));
    assert_eq!(page.proxy_draft().http.port, "8080");
    let _ = page.update(Msg::ProxyMode(network::ProxyMode::None));
    assert_eq!(page.proxy_draft().mode, network::ProxyMode::None);
}

#[test]
fn gsettings_values_parse_like_the_cli_prints_them() {
    assert_eq!(gsettings::parse_bool("true"), Some(true));
    assert_eq!(gsettings::parse_int("uint32 30"), Some(30));
    assert_eq!(gsettings::parse_int("int32 -1"), Some(-1));
    assert_eq!(gsettings::parse_double("1.25"), Some(1.25));
    assert_eq!(gsettings::parse_string(r"'it\'s'").as_deref(), Some("it's"));
    assert_eq!(
        gsettings::parse_string_array("['localhost', '127.0.0.0/8', '::1']"),
        vec!["localhost", "127.0.0.0/8", "::1"]
    );
    assert!(gsettings::parse_string_array("@as []").is_empty());
}

#[test]
fn gsettings_quoting_round_trips() {
    let text = r"a 'quoted' \ value";
    assert_eq!(gsettings::parse_string(&gsettings::quote(text)).as_deref(), Some(text));
    assert_eq!(gsettings::string_array(&[]), "@as []");
    assert_eq!(gsettings::string_array(&["a".to_string(), "b'c".to_string()]), r"['a', 'b\'c']");
}

#[test]
fn a_recursive_listing_splits_schema_key_and_value() {
    let snapshot = gsettings::Snapshot::parse(
        "org.gnome.system.proxy mode 'manual'\n\
         org.gnome.system.proxy ignore-hosts ['localhost']\n\
         org.gnome.system.proxy.http host 'proxy.lan'\n\
         org.gnome.system.proxy.http port 3128\n",
    );
    assert!(snapshot.has_schema("org.gnome.system.proxy"));
    let proxy = network::ProxySettings::from_snapshot(&snapshot);
    assert_eq!(proxy.mode, network::ProxyMode::Manual);
    assert_eq!(proxy.http.host, "proxy.lan");
    assert_eq!(proxy.http.port, "3128");
    assert_eq!(proxy.https.port, "");
    assert_eq!(proxy.ignore_hosts, "localhost");
}

#[test]
fn manual_proxy_writes_every_endpoint_and_rejects_bad_ports() {
    let mut proxy = network::ProxySettings {
        available: true,
        mode: network::ProxyMode::Manual,
        ignore_hosts: "localhost, *.lan".to_string(),
        ..network::ProxySettings::default()
    };
    proxy.endpoint_mut(0).host = "proxy".to_string();
    proxy.endpoint_mut(0).port = "8080".to_string();
    let changes = proxy.changes();
    assert!(changes.contains(&("org.gnome.system.proxy.http".to_string(), "port", "8080".to_string())));
    assert!(changes.contains(&(
        "org.gnome.system.proxy".to_string(),
        "ignore-hosts",
        "['localhost', '*.lan']".to_string()
    )));
    assert_eq!(proxy.invalid_port(), None);
    proxy.endpoint_mut(2).port = "70000".to_string();
    assert_eq!(proxy.invalid_port(), Some(2));
    proxy.mode = network::ProxyMode::None;
    assert_eq!(proxy.changes().len(), 1);
}

#[test]
fn connections_parse_with_activity_and_autoconnect() {
    let profiles = network::parse_connections(
        "Home:1111:802-11-wireless:wlan0:yes:yes\n\
         Glide Hotspot:2222:802-11-wireless::no:no\n\
         Office\\:5G:3333:802-11-wireless::no:yes\n\
         lo:4444:loopback:lo:yes:yes\n",
    );
    assert_eq!(profiles.len(), 4);
    assert!(profiles[0].active);
    assert_eq!(profiles[2].name, "Office:5G");
    let state = network::NetworkState { profiles, ..network::NetworkState::default() };
    let saved: Vec<&str> = state.saved_wifi().map(|p| p.name.as_str()).collect();
    assert_eq!(saved, vec!["Home", "Office:5G"]);
    assert!(state.hotspot().is_none());
    assert_eq!(state.active_profile("wlan0").map(|p| p.uuid.as_str()), Some("1111"));
}

#[test]
fn radios_and_airplane_mode_follow_nmcli_and_rfkill() {
    assert_eq!(
        network::parse_radios("enabled:disabled:missing:enabled\n"),
        Some(network::Radios { wifi_hardware: true, wifi: false })
    );
    let device = |soft, hard| network::RfkillDevice { kind: "wlan".to_string(), soft, hard };
    assert!(!network::airplane_mode(&[]));
    assert!(!network::airplane_mode(&[device(true, false), device(false, false)]));
    assert!(network::airplane_mode(&[device(true, false), device(false, true)]));
}

#[test]
fn device_details_collect_addresses_and_skip_link_local_ipv6() {
    let details = network::parse_device_show(
        "GENERAL.DEVICE:wlan0\n\
         GENERAL.HWADDR:50:FE:0C:12:B7:87\n\
         CAPABILITIES.SPEED:780 Mb/s\n\
         IP4.ADDRESS[1]:192.168.89.233/24\n\
         IP4.GATEWAY:192.168.89.1\n\
         IP4.DNS[1]:192.168.89.1\n\
         IP6.ADDRESS[1]:fe80::1/64\n\
         IP6.ADDRESS[2]:2001:db8::5/64\n\
         IP6.GATEWAY:\n",
    );
    assert_eq!(details.mac, "50:FE:0C:12:B7:87");
    assert_eq!(details.speed, "780 Mb/s");
    assert_eq!(details.ipv4, vec!["192.168.89.233/24"]);
    assert_eq!(details.gateway, "192.168.89.1");
    assert_eq!(details.dns, vec!["192.168.89.1"]);
    assert_eq!(details.ipv6, vec!["2001:db8::5/64"]);
}

#[test]
fn profile_settings_parse_metered_and_custom_dns() {
    let settings = network::parse_profile_settings(
        "connection.metered:yes\n\
         connection.autoconnect:no\n\
         802-11-wireless.hidden:yes\n\
         802-11-wireless-security.key-mgmt:sae\n\
         ipv4.method:auto\n\
         ipv4.dns:1.1.1.1,9.9.9.9\n",
    );
    assert_eq!(settings.metered, network::Metered::Yes);
    assert!(!settings.autoconnect);
    assert!(settings.hidden);
    assert_eq!(settings.key_mgmt, "sae");
    assert_eq!(settings.custom_dns, vec!["1.1.1.1", "9.9.9.9"]);
    assert_eq!(network::Metered::parse("unknown"), network::Metered::Auto);
    assert_eq!(network::Metered::Auto.key(), "unknown");
}

#[test]
fn dns_lists_accept_only_ip_addresses() {
    assert_eq!(
        network::parse_dns_list("1.1.1.1, 2606:4700::1111"),
        Some(vec!["1.1.1.1".to_string(), "2606:4700::1111".to_string()])
    );
    assert_eq!(network::parse_dns_list(""), Some(Vec::new()));
    assert_eq!(network::parse_dns_list("dns.google"), None);
}

#[test]
fn the_active_radio_line_gives_frequency_channel_and_security() {
    assert_eq!(
        network::parse_active_radio(" :2417 MHz:2:WPA2\n*:5180 MHz:36:WPA2 WPA3\n"),
        Some(("5180 MHz".to_string(), "36".to_string(), "WPA2 WPA3".to_string()))
    );
    assert_eq!(network::parse_active_radio(" :2417 MHz:2:WPA2\n"), None);
}

#[test]
fn passwords_are_redacted_and_generated_ones_are_valid_psks() {
    let secret = network::Secret("hunter22".to_string());
    assert!(!format!("{secret:?}").contains("hunter22"));
    assert!(network::valid_psk("12345678"));
    assert!(!network::valid_psk("short"));
    assert!(!network::valid_psk(&"x".repeat(64)));
    let password = network::password_from_bytes(&[0, 1, 2, 255, 128, 64, 7, 9, 11, 13, 17, 19]);
    assert_eq!(password.len(), 12);
    assert!(network::valid_psk(&password));
    assert!(password.chars().all(|c| c.is_ascii_alphanumeric()));
}

#[test]
fn hotspot_and_hidden_settings_carry_the_right_modes() {
    use zbus::zvariant::Value;
    let hotspot = network::hotspot_settings("Glide", "secret123");
    assert_eq!(hotspot["802-11-wireless"]["mode"], Value::from("ap"));
    assert_eq!(hotspot["ipv4"]["method"], Value::from("shared"));
    assert_eq!(hotspot["connection"]["id"], Value::from(network::HOTSPOT_NAME));
    let hidden = network::hidden_settings("Secret", Some("password"));
    assert_eq!(hidden["802-11-wireless"]["hidden"], Value::from(true));
    assert_eq!(hidden["802-11-wireless-security"]["key-mgmt"], Value::from("wpa-psk"));
    assert!(!network::hidden_settings("Open", None).contains_key("802-11-wireless-security"));
}

fn props(pairs: Vec<(&str, zbus::zvariant::Value<'static>)>) -> bluetooth::Properties {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), zbus::zvariant::OwnedValue::try_from(value).expect("owned value")))
        .collect()
}

fn object(interfaces: Vec<(&str, bluetooth::Properties)>) -> std::collections::HashMap<String, bluetooth::Properties> {
    interfaces.into_iter().map(|(name, props)| (name.to_string(), props)).collect()
}

#[test]
fn bluetooth_state_comes_from_one_bluez_object_listing() {
    use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value};
    let path = |text: &str| OwnedObjectPath::try_from(text).expect("path");
    let adapter = || Value::from(ObjectPath::try_from("/org/bluez/hci0").expect("path"));
    let objects: bluetooth::Objects = [
        (
            path("/org/bluez/hci0"),
            object(vec![(
                "org.bluez.Adapter1",
                props(vec![
                    ("Address", Value::from("50:FE:0C:12:B7:86")),
                    ("Name", Value::from("laptop")),
                    ("Alias", Value::from("Artem's laptop")),
                    ("Powered", Value::from(true)),
                    ("PowerState", Value::from("on")),
                    ("Discoverable", Value::from(true)),
                    ("DiscoverableTimeout", Value::from(180u32)),
                    ("Pairable", Value::from(false)),
                ]),
            )]),
        ),
        (
            path("/org/bluez/hci0/dev_AC_3E_B1_98_2D_BF"),
            object(vec![
                (
                    "org.bluez.Device1",
                    props(vec![
                        ("Address", Value::from("AC:3E:B1:98:2D:BF")),
                        ("Alias", Value::from("Pixel Buds")),
                        ("Icon", Value::from("audio-headset")),
                        ("Paired", Value::from(true)),
                        ("Trusted", Value::from(true)),
                        ("Connected", Value::from(true)),
                        ("Adapter", adapter()),
                    ]),
                ),
                ("org.bluez.Battery1", props(vec![("Percentage", Value::from(85u8))])),
            ]),
        ),
        (
            path("/org/bluez/hci0/dev_11_22_33_44_55_66"),
            object(vec![(
                "org.bluez.Device1",
                props(vec![("Address", Value::from("11:22:33:44:55:66")), ("Adapter", adapter())]),
            )]),
        ),
    ]
    .into_iter()
    .collect();
    let state = bluetooth::from_objects(&objects);
    assert!(state.available && state.powered && !state.blocked);
    assert_eq!(state.alias, "Artem's laptop");
    assert!(state.adapter.discoverable && !state.adapter.pairable);
    assert_eq!(state.adapter.discoverable_timeout, 180);
    let buds = state.device("AC:3E:B1:98:2D:BF").expect("the paired headset is listed");
    assert_eq!(buds.battery, Some(85));
    assert!(buds.trusted && !buds.blocked && buds.paired);
    assert_eq!(bluetooth::device_kind(&buds.icon), bluetooth::DeviceKind::Headphones);
    assert_eq!(state.nearby().count(), 0, "a device that only has an address is not offered");
    assert!(!bluetooth::from_objects(&bluetooth::Objects::new()).available);
}

#[test]
fn nameless_nearby_devices_are_hidden_and_aliases_are_cleaned() {
    let anonymous = bluetooth::BtDevice {
        address: "11:22:33:44:55:66".to_string(),
        name: "11-22-33-44-55-66".to_string(),
        ..bluetooth::BtDevice::default()
    };
    assert!(bluetooth::is_anonymous(&anonymous));
    let state = bluetooth::BluetoothState { devices: vec![anonymous], ..bluetooth::BluetoothState::default() };
    assert_eq!(state.nearby().count(), 0);
    assert_eq!(bluetooth::clean_alias("  Kitchen\nspeaker ").as_deref(), Some("Kitchenspeaker"));
    assert_eq!(bluetooth::clean_alias("   "), None);
}

#[test]
fn vpn_connections_are_filtered_and_sorted_active_first() {
    let list = vpn::parse_connections(
        "Home:1111:802-11-wireless:wlan0:yes:activated:yes:/etc/NetworkManager/system-connections/Home.nmconnection\n\
         office:2222:vpn::no::no:/etc/NetworkManager/system-connections/office.nmconnection\n\
         Mullvad:3333:wireguard:wg0:yes:activated:yes:/etc/NetworkManager/system-connections/Mullvad.nmconnection\n\
         wt0:4444:wireguard:wt0:yes:activated:yes:/run/NetworkManager/system-connections/wt0.nmconnection\n",
    );
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].name, "Mullvad");
    assert!(list[0].is_wireguard());
    assert!(!list[0].external);
    assert_eq!(list[0].protocol(), "WireGuard");
    assert!(list[1].external, "a tunnel NetworkManager only mirrors (NetBird's wt0) cannot be toggled or deleted");
    let mut office = list[2].clone();
    office.service = "org.freedesktop.NetworkManager.openvpn".to_string();
    assert_eq!(office.protocol(), "OpenVPN");
}

#[test]
fn vpn_plugins_and_config_files_are_recognised() {
    let plugin = vpn::parse_plugin(
        "[VPN Connection]\nname=openvpn\nservice=org.freedesktop.NetworkManager.openvpn\n\n[libnm]\nplugin=x\n",
    )
    .expect("a plugin file parses");
    assert_eq!(plugin.name, "openvpn");
    assert_eq!(vpn::detect_kind(Path::new("office.ovpn"), ""), Some("openvpn"));
    assert_eq!(
        vpn::detect_kind(Path::new("wg0.conf"), "[Interface]\nPrivateKey = x\n[Peer]\nEndpoint = a:1\n"),
        Some("wireguard")
    );
    assert_eq!(vpn::detect_kind(Path::new("client.conf"), "client\nremote vpn.example 1194\n"), Some("openvpn"));
    assert_eq!(vpn::detect_kind(Path::new("nginx.conf"), "server {}\n"), None);
    assert!(vpn::wireguard_name_ok(Path::new("/tmp/wg-home.conf")));
    assert!(!vpn::wireguard_name_ok(Path::new("/tmp/a very long tunnel name.conf")));
}

#[test]
fn vpn_details_read_the_gateway_from_plugin_data() {
    let details = vpn::parse_details(
        "vpn.data:remote = vpn.example.com:1194, username = artem, dev = tun\n\
         IP4.ADDRESS[1]:10.8.0.2/24\n\
         IP4.DNS[1]:10.8.0.1\n",
    );
    assert_eq!(details.gateway, "vpn.example.com:1194");
    assert_eq!(details.user, "artem");
    assert_eq!(details.addresses, vec!["10.8.0.2/24"]);
    assert_eq!(details.dns, vec!["10.8.0.1"]);
    assert!(!details.only_own_network);
    assert!(details.search.is_empty());
}

#[test]
fn vpn_details_read_split_tunnelling_and_search_domains() {
    let details = vpn::parse_details("ipv4.never-default:yes\nipv4.dns-search:office.lan,corp.example\n");
    assert!(details.only_own_network);
    assert_eq!(details.search, vec!["office.lan", "corp.example"]);
    assert!(!vpn::parse_details("ipv4.never-default:no\nipv4.dns-search:\n").only_own_network);
}

#[test]
fn login_sessions_and_names_parse() {
    assert_eq!(
        accounts::parse_session_ids("3 1000 artem seat0 1977 user tty2 no -\n4 1000 artem - 2155 manager - no -\n"),
        vec!["3", "4"]
    );
    let session = accounts::parse_session(
        "Id=3\nName=artem\nSeat=seat0\nTTY=tty2\nType=wayland\nClass=user\nState=active\nRemote=no\n",
    );
    assert_eq!(session.kind, "wayland");
    assert_eq!(session.tty, "tty2");
    assert!(!session.remote);
    let utc = glide_settings::services::tz::Zone::utc();
    assert_eq!(
        accounts::parse_timestamp("Sat 2026-10-17 08:46:47 EEST", &utc),
        Some(glide_settings::services::tz::from_civil(2026, 10, 17, 8, 46, 47)),
        "logind's local timestamp becomes a time Settings can show as \"2 h ago\""
    );
    assert_eq!(accounts::parse_timestamp("n/a", &utc), None);
    assert_eq!(accounts::clean_real_name("  Artem \t Shevchenko: ").as_deref(), Some("Artem Shevchenko"));
}

#[test]
fn pw_dump_reports_capture_streams_per_kind() {
    let dump = r#"[
        {"id": 1, "type": "PipeWire:Interface:Node", "info": {"state": "running",
            "props": {"media.class": "Stream/Input/Audio", "application.name": "Firefox"}}},
        {"id": 2, "type": "PipeWire:Interface:Node", "info": {"state": "running",
            "props": {"media.class": "Stream/Input/Audio", "application.name": "Meter", "stream.monitor": true}}},
        {"id": 3, "type": "PipeWire:Interface:Node", "info": {"state": "running",
            "props": {"media.class": "Video/Source", "node.name": "v4l2_input.pci"}}},
        {"id": 4, "type": "PipeWire:Interface:Node", "info": {"state": "running",
            "props": {"media.class": "Stream/Input/Video", "application.name": "Zoom"}}},
        {"id": 5, "type": "PipeWire:Interface:Link", "info": {"output-node-id": 3, "input-node-id": 4}}
    ]"#;
    let usage = privacy::parse_pw_dump(dump);
    assert_eq!(usage.microphone, vec!["Firefox"]);
    assert_eq!(usage.camera, vec!["Zoom"]);
    assert!(usage.screen.is_empty());
    assert!(privacy::parse_pw_dump("not json").is_empty());
}

#[test]
fn recent_files_are_counted() {
    assert_eq!(privacy::count_bookmarks("<xbel><bookmark href='a'/><bookmark href='b'/></xbel>"), 2);
}

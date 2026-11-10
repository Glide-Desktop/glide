use glide_phone::config::PhoneConfig;
use glide_phone::crypto::cert::{IdentityCert, compute_fingerprint};
use glide_phone::plugins::battery::{THRESHOLD_BATTERY_LOW, THRESHOLD_NONE, handle_battery_packet};
use glide_phone::plugins::find_device::create_ring_packet;
use glide_phone::plugins::notifications::{
    create_action_packet, create_reply_packet, create_request_packet, extract_auth_code, handle_notification_packet,
};
use glide_phone::protocol::identity::{DeviceType, IdentityBody, PROTOCOL_VERSION};
use glide_phone::protocol::packet::{
    NetworkPacket, PACKET_TYPE_BATTERY, PACKET_TYPE_FINDMYPHONE_REQUEST, PACKET_TYPE_NOTIFICATION, PACKET_TYPE_PAIR,
};
use glide_phone::protocol::pair::{PairBody, PairState};
use glide_phone::state::PhoneManager;
use serde_json::json;
use std::collections::HashMap;

#[test]
fn packet_serialization_roundtrip() {
    let packet = NetworkPacket::new(PACKET_TYPE_PAIR, json!({ "pair": true }));
    let serialized = packet.serialize().expect("serialize packet");
    assert!(serialized.ends_with('\n'));

    let deserialized = NetworkPacket::parse(&serialized).expect("parse packet");
    assert_eq!(deserialized.packet_type, PACKET_TYPE_PAIR);
    assert_eq!(deserialized.body["pair"], true);
}

#[test]
fn identity_packet_roundtrip() {
    let identity = IdentityBody {
        device_id: "test-device-123".to_string(),
        device_name: "Pixel 9".to_string(),
        device_type: DeviceType::Phone,
        protocol_version: PROTOCOL_VERSION,
        tcp_port: Some(1716),
        target_device_id: None,
        target_protocol_version: None,
        incoming_capabilities: vec!["kdeconnect.battery".to_string()],
        outgoing_capabilities: vec!["kdeconnect.battery".to_string()],
    };

    let packet = identity.to_packet().expect("to packet");
    let parsed = IdentityBody::from_packet(&packet).expect("from packet");

    assert_eq!(parsed.device_id, "test-device-123");
    assert_eq!(parsed.device_name, "Pixel 9");
    assert_eq!(parsed.device_type, DeviceType::Phone);
    assert_eq!(parsed.tcp_port, Some(1716));
}

#[test]
fn android_string_target_protocol_version_is_accepted() {
    for version in [json!("8"), json!(8)] {
        let packet = NetworkPacket::new(
            "kdeconnect.identity",
            json!({"deviceId": "phone", "protocolVersion": 8, "targetProtocolVersion": version}),
        );
        assert_eq!(IdentityBody::from_packet(&packet).unwrap().target_protocol_version, Some(8));
    }
    let packet = NetworkPacket::new(
        "kdeconnect.identity",
        json!({"deviceId": "phone", "protocolVersion": 8, "targetProtocolVersion": "invalid"}),
    );
    assert!(IdentityBody::from_packet(&packet).is_err());
}

#[test]
fn advertised_tcp_range_is_accepted_by_kde_connect() {
    assert_eq!(glide_phone::protocol::identity::MIN_TCP_PORT, 1716);
}

#[tokio::test]
async fn malformed_discovery_datagram_does_not_stop_discovery() {
    use glide_phone::discovery::udp::receive_discovery_packet;
    use tokio::net::UdpSocket;

    let receiver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let address = receiver.local_addr().unwrap();
    sender.send_to(&[0xff], address).await.unwrap();
    assert!(receive_discovery_packet(&receiver, "local").await.unwrap().is_none());
    let packet = IdentityBody::new_discovery_packet("phone", "Pixel", 1716).unwrap();
    sender.send_to(packet.serialize().unwrap().as_bytes(), address).await.unwrap();
    assert_eq!(receive_discovery_packet(&receiver, "local").await.unwrap().unwrap().identity.device_id, "phone");
}

#[test]
fn pair_body_packets() {
    let pair_req = PairBody::new(true);
    let pkt = pair_req.to_packet().expect("pair to packet");
    assert_eq!(pkt.packet_type, PACKET_TYPE_PAIR);
    let parsed = PairBody::from_packet(&pkt).expect("parse pair packet");
    assert!(parsed.pair);

    let pair_reject = PairBody::new(false);
    let rej_pkt = pair_reject.to_packet().expect("reject to packet");
    let rej_parsed = PairBody::from_packet(&rej_pkt).expect("parse reject packet");
    assert!(!rej_parsed.pair);
}

#[test]
fn certificate_generation_and_fingerprint() {
    let cert = IdentityCert::generate("Test Device").expect("generate cert");
    assert!(!cert.cert_pem.is_empty());
    assert!(!cert.key_pem.is_empty());
    assert!(!cert.cert_der.is_empty());
    assert!(!cert.fingerprint.is_empty());

    let fp = compute_fingerprint(&cert.cert_der);
    assert_eq!(fp, cert.fingerprint);
    assert_eq!(fp.split(':').count(), 32);
}

#[test]
fn battery_packet_parsing() {
    let packet = NetworkPacket::new(
        PACKET_TYPE_BATTERY,
        json!({
            "currentCharge": 85,
            "isCharging": true,
            "thresholdEvent": THRESHOLD_NONE,
        }),
    );

    let state = handle_battery_packet("Phone", &packet).expect("handle battery");
    assert_eq!(state.charge, 85);
    assert!(state.is_charging);
    assert!(!state.is_low);

    let low_packet = NetworkPacket::new(
        PACKET_TYPE_BATTERY,
        json!({
            "currentCharge": 12,
            "isCharging": false,
            "thresholdEvent": THRESHOLD_BATTERY_LOW,
        }),
    );
    let low_state = handle_battery_packet("Phone", &low_packet).expect("handle low battery");
    assert_eq!(low_state.charge, 12);
    assert!(!low_state.is_charging);
    assert!(low_state.is_low);
}

#[test]
fn notification_auth_code_extraction() {
    assert_eq!(extract_auth_code("Your verification code is 492810."), Some("492810".to_string()));
    assert_eq!(extract_auth_code("Код підтвердження: 8391"), Some("8391".to_string()));
    assert_eq!(extract_auth_code("No digits here"), None);
}

#[test]
fn notification_packet_handling_and_cancellation() {
    let mut active = HashMap::new();
    let packet = NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION,
        json!({
            "id": "org.kde.kdeconnect_tp::notif-42",
            "appName": "Signal",
            "title": "Alice",
            "text": "Hello world!",
            "requestReplyId": "reply-42",
            "actions": ["Mark as read"],
        }),
    );

    let event = handle_notification_packet("Phone", &packet, &mut active).expect("handle notif");
    assert!(event.is_some());
    assert_eq!(active.len(), 1);
    assert!(active.contains_key("notif-42"));

    let cancel_packet = NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION,
        json!({
            "id": "org.kde.kdeconnect_tp::notif-42",
            "isCancel": true,
        }),
    );
    let cancel_event = handle_notification_packet("Phone", &cancel_packet, &mut active).expect("cancel notif");
    assert!(cancel_event.is_some());
    assert!(active.is_empty());
}

#[test]
fn notification_deduplication_prevents_repeated_alerts() {
    use glide_phone::plugins::notifications::handle_notification_packet_with_history;

    let mut active = HashMap::new();
    let mut history = HashMap::new();
    let packet = NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION,
        json!({
            "id": "org.kde.kdeconnect_tp::notif-100",
            "appName": "Telegram",
            "title": "Alice",
            "text": "Hello once",
        }),
    );

    let first = handle_notification_packet_with_history("Phone", &packet, &mut active, &mut history)
        .expect("first notification");
    assert!(first.is_some());
    assert_eq!(active.len(), 1);

    let duplicate = handle_notification_packet_with_history("Phone", &packet, &mut active, &mut history)
        .expect("duplicate notification");
    assert!(duplicate.is_none());
    assert_eq!(active.len(), 1);

    let summary_duplicate = NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION,
        json!({
            "id": "org.kde.kdeconnect_tp::notif-100-summary",
            "appName": "Telegram",
            "title": "Alice",
            "text": "Hello once",
        }),
    );
    let summary = handle_notification_packet_with_history("Phone", &summary_duplicate, &mut active, &mut history)
        .expect("summary duplicate");
    assert!(summary.is_none());
    assert_eq!(active.len(), 1);

    let updated = NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION,
        json!({
            "id": "org.kde.kdeconnect_tp::notif-100",
            "appName": "Telegram",
            "title": "Alice",
            "text": "Hello twice",
        }),
    );
    let second = handle_notification_packet_with_history("Phone", &updated, &mut active, &mut history)
        .expect("updated notification");
    assert!(second.is_some());
    assert_eq!(active["notif-100"].body, "Hello twice");
    assert!(active["notif-100"].icon_path.as_deref().is_some_and(|p| std::path::Path::new(p).is_file()));
}

#[test]
fn badged_notification_icon_renders_96x96_png_with_phone_overlay() {
    use glide_phone::plugins::notifications::build_badged_notification_icon;

    let path = build_badged_notification_icon("Telegram", None, Some("test-hash-1")).expect("icon path");
    let png_bytes = std::fs::read(&path).expect("read png");
    let rendered = glide_paint::render_png(&png_bytes, 96).expect("decode 96x96 png");
    assert_eq!(rendered.len(), 96 * 96 * 4);
    let badge_idx = (73 * 96 + 73) * 4;
    assert_eq!(rendered[badge_idx + 3], 255, "bottom-right phone badge must be opaque");
}

#[test]
fn silhouette_payload_is_not_treated_as_avatar_and_avatar_packet_upgrades_summary() {
    use glide_paint::Painter;
    use glide_phone::plugins::notifications::{
        handle_notification_packet_with_payload, is_silhouette_bgra, payload_is_rich_avatar,
    };
    use glide_theme::Color;

    let mut silhouette_bgra = vec![0u8; 96 * 96 * 4];
    Painter::fill_rounded_rect_over(&mut silhouette_bgra, 96, 96, 30, 30, 36, 36, 1, Color([1.0, 1.0, 1.0, 1.0]), 8.0);
    assert!(is_silhouette_bgra(&silhouette_bgra));
    let silhouette_png = glide_shell::image::Image::from_bgra(96, 96, silhouette_bgra).unwrap().png().unwrap();
    assert!(!payload_is_rich_avatar(Some(&silhouette_png)));

    let mut avatar_bgra = vec![0u8; 96 * 96 * 4];
    Painter::fill_rounded_rect_over(&mut avatar_bgra, 96, 96, 8, 8, 80, 80, 1, Color([0.92, 0.45, 0.28, 1.0]), 40.0);
    Painter::fill_rounded_rect_over(&mut avatar_bgra, 96, 96, 28, 24, 40, 40, 1, Color([0.20, 0.68, 0.88, 1.0]), 20.0);
    assert!(!is_silhouette_bgra(&avatar_bgra));
    let avatar_png = glide_shell::image::Image::from_bgra(96, 96, avatar_bgra).unwrap().png().unwrap();
    assert!(payload_is_rich_avatar(Some(&avatar_png)));

    let mut active = HashMap::new();
    let mut history = HashMap::new();

    let summary_pkt = NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION,
        json!({
            "id": "org.kde.kdeconnect_tp::tg-summary-1",
            "appName": "Telegram",
            "title": "Олена",
            "text": "Привіт з телефону!",
            "silent": true,
        }),
    );
    let first = handle_notification_packet_with_payload(
        "Pixel",
        &summary_pkt,
        Some(&silhouette_png),
        &mut active,
        &mut history,
    )
    .expect("summary packet");
    assert!(first.is_some());
    assert!(!active["tg-summary-1"].has_avatar);
    let summary_icon = active["tg-summary-1"].icon_path.clone().expect("summary icon");
    let summary_icon_bytes = std::fs::read(&summary_icon).expect("read summary icon");
    let summary_decoded = glide_paint::render_png(&summary_icon_bytes, 96).expect("decode summary icon");
    assert!(!is_silhouette_bgra(&summary_decoded), "rendered app fallback icon must not be a bare silhouette");

    let child_pkt = NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION,
        json!({
            "id": "org.kde.kdeconnect_tp::tg-child-1",
            "appName": "Telegram",
            "title": "Олена",
            "text": "Привіт з телефону!",
            "requestReplyId": "reply-uuid-1",
            "silent": true,
        }),
    );
    let upgraded =
        handle_notification_packet_with_payload("Pixel", &child_pkt, Some(&avatar_png), &mut active, &mut history)
            .expect("child avatar packet");
    assert!(upgraded.is_some(), "child notification with rich avatar must upgrade existing summary notification");
    assert_eq!(active.len(), 1);
    assert!(active["tg-child-1"].has_avatar);
    assert_eq!(active["tg-child-1"].request_reply_id.as_deref(), Some("reply-uuid-1"));
    assert_ne!(active["tg-child-1"].icon_path.as_deref(), Some(summary_icon.as_str()));

    let avatar_icon = active["tg-child-1"].icon_path.clone().expect("avatar icon");
    let late_summary = handle_notification_packet_with_payload(
        "Pixel",
        &summary_pkt,
        Some(&silhouette_png),
        &mut active,
        &mut history,
    )
    .expect("late summary");
    assert!(late_summary.is_none());
    assert!(active["tg-child-1"].has_avatar);
    assert_eq!(active["tg-child-1"].icon_path.as_deref(), Some(avatar_icon.as_str()));
}

#[test]
fn avatar_classification_distinguishes_stencils_from_all_avatars() {
    use glide_paint::Painter;
    use glide_phone::plugins::notifications::{is_silhouette_bgra, payload_is_rich_avatar};
    use glide_theme::Color;

    let mut plane_stencil = vec![0u8; 96 * 96 * 4];
    Painter::fill_rounded_rect_over(&mut plane_stencil, 96, 96, 30, 30, 36, 36, 1, Color([1.0, 1.0, 1.0, 1.0]), 8.0);
    assert!(is_silhouette_bgra(&plane_stencil));
    let plane_png = glide_shell::image::Image::from_bgra(96, 96, plane_stencil).unwrap().png().unwrap();
    assert!(!payload_is_rich_avatar(Some(&plane_png)));

    let mut grey_stencil = vec![0u8; 96 * 96 * 4];
    Painter::fill_rounded_rect_over(&mut grey_stencil, 96, 96, 28, 28, 40, 40, 1, Color([0.65, 0.65, 0.65, 1.0]), 6.0);
    assert!(is_silhouette_bgra(&grey_stencil));

    let mut circular_photo = vec![0u8; 96 * 96 * 4];
    Painter::fill_rounded_rect_over(&mut circular_photo, 96, 96, 6, 6, 84, 84, 1, Color([0.22, 0.44, 0.88, 1.0]), 42.0);
    Painter::fill_rounded_rect_over(
        &mut circular_photo,
        96,
        96,
        20,
        20,
        48,
        48,
        1,
        Color([0.94, 0.72, 0.55, 1.0]),
        24.0,
    );
    assert!(!is_silhouette_bgra(&circular_photo));
    let photo_png = glide_shell::image::Image::from_bgra(96, 96, circular_photo).unwrap().png().unwrap();
    assert!(payload_is_rich_avatar(Some(&photo_png)));

    let mut dark_photo = vec![0u8; 96 * 96 * 4];
    Painter::fill_rounded_rect_over(&mut dark_photo, 96, 96, 8, 8, 80, 80, 1, Color([0.10, 0.12, 0.18, 1.0]), 40.0);
    Painter::fill_rounded_rect_over(&mut dark_photo, 96, 96, 30, 24, 36, 36, 1, Color([0.30, 0.24, 0.20, 1.0]), 18.0);
    assert!(!is_silhouette_bgra(&dark_photo));
    let dark_png = glide_shell::image::Image::from_bgra(96, 96, dark_photo).unwrap().png().unwrap();
    assert!(payload_is_rich_avatar(Some(&dark_png)));

    let mut whatsapp_default = vec![0u8; 96 * 96 * 4];
    Painter::fill_rounded_rect_over(
        &mut whatsapp_default,
        96,
        96,
        6,
        6,
        84,
        84,
        1,
        Color([0.38, 0.46, 0.50, 1.0]),
        42.0,
    );
    Painter::fill_rounded_rect_over(
        &mut whatsapp_default,
        96,
        96,
        28,
        24,
        40,
        40,
        1,
        Color([0.81, 0.85, 0.86, 1.0]),
        20.0,
    );
    assert!(!is_silhouette_bgra(&whatsapp_default));
    let wa_png = glide_shell::image::Image::from_bgra(96, 96, whatsapp_default).unwrap().png().unwrap();
    assert!(payload_is_rich_avatar(Some(&wa_png)));

    let mut telegram_monogram = vec![0u8; 96 * 96 * 4];
    Painter::fill_rounded_rect_over(
        &mut telegram_monogram,
        96,
        96,
        6,
        6,
        84,
        84,
        1,
        Color([0.14, 0.51, 0.80, 1.0]),
        42.0,
    );
    Painter::fill_rounded_rect_over(
        &mut telegram_monogram,
        96,
        96,
        34,
        32,
        28,
        30,
        1,
        Color([1.0, 1.0, 1.0, 1.0]),
        4.0,
    );
    assert!(!is_silhouette_bgra(&telegram_monogram));
    let tg_png = glide_shell::image::Image::from_bgra(96, 96, telegram_monogram).unwrap().png().unwrap();
    assert!(payload_is_rich_avatar(Some(&tg_png)));
}

#[test]
fn whatsapp_group_summary_and_child_notification_merging() {
    use glide_paint::Painter;
    use glide_phone::plugins::notifications::handle_notification_packet_with_payload;
    use glide_theme::Color;

    let mut avatar_bgra = vec![0u8; 96 * 96 * 4];
    Painter::fill_rounded_rect_over(&mut avatar_bgra, 96, 96, 6, 6, 84, 84, 1, Color([0.15, 0.83, 0.40, 1.0]), 42.0);
    Painter::fill_rounded_rect_over(&mut avatar_bgra, 96, 96, 24, 24, 48, 48, 1, Color([0.94, 0.72, 0.55, 1.0]), 24.0);
    let avatar_png = glide_shell::image::Image::from_bgra(96, 96, avatar_bgra).unwrap().png().unwrap();

    let mut active = HashMap::new();
    let mut history = HashMap::new();

    let group_summary = NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION,
        json!({
            "id": "org.kde.kdeconnect_tp::0|com.whatsapp|1|null|12345@s.whatsapp.net",
            "appName": "WhatsApp",
            "title": "WhatsApp",
            "text": "2 new messages",
            "silent": true,
        }),
    );

    let summary_ev = handle_notification_packet_with_payload("Pixel", &group_summary, None, &mut active, &mut history)
        .expect("summary event");
    assert!(summary_ev.is_some());
    assert_eq!(active.len(), 1);
    let summary_key = "0|com.whatsapp|1|null|12345@s.whatsapp.net";
    let child_key = "0|com.whatsapp|1|12345@s.whatsapp.net";
    assert!(!active[summary_key].has_avatar);

    let child_msg = NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION,
        json!({
            "id": "org.kde.kdeconnect_tp::0|com.whatsapp|1|12345@s.whatsapp.net",
            "appName": "WhatsApp",
            "title": "Mary",
            "text": "Hello, how are you?",
            "requestReplyId": "reply-wa-1",
            "actions": ["Mark as read"],
            "silent": false,
        }),
    );

    let child_ev =
        handle_notification_packet_with_payload("Pixel", &child_msg, Some(&avatar_png), &mut active, &mut history)
            .expect("child event");
    assert!(child_ev.is_some());
    assert_eq!(active.len(), 1);
    assert!(active[child_key].has_avatar);
    assert_eq!(active[child_key].title, "Mary");
    assert_eq!(active[child_key].body, "Hello, how are you?");
    assert_eq!(active[child_key].request_reply_id.as_deref(), Some("reply-wa-1"));
    assert_eq!(active[child_key].actions, vec!["Mark as read".to_string()]);

    let late_summary =
        handle_notification_packet_with_payload("Pixel", &group_summary, None, &mut active, &mut history)
            .expect("late summary");
    assert!(late_summary.is_none());
    assert_eq!(active.len(), 1);
    assert!(active[child_key].has_avatar);
    assert_eq!(active[child_key].title, "Mary");
}

#[test]
fn payload_hash_caches_and_reuses_raw_payload_when_payload_png_omitted() {
    use glide_paint::Painter;
    use glide_phone::plugins::notifications::handle_notification_packet_with_payload;
    use glide_theme::Color;

    let mut avatar_bgra = vec![0u8; 96 * 96 * 4];
    Painter::fill_rounded_rect_over(&mut avatar_bgra, 96, 96, 6, 6, 84, 84, 1, Color([0.80, 0.20, 0.30, 1.0]), 42.0);
    Painter::fill_rounded_rect_over(&mut avatar_bgra, 96, 96, 26, 26, 44, 44, 1, Color([0.96, 0.75, 0.50, 1.0]), 22.0);
    let avatar_png = glide_shell::image::Image::from_bgra(96, 96, avatar_bgra).unwrap().png().unwrap();

    let mut active = HashMap::new();
    let mut history = HashMap::new();

    let pkt1 = NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION,
        json!({
            "id": "org.kde.kdeconnect_tp::cache-notif-1",
            "appName": "Signal",
            "title": "David",
            "text": "Message one",
            "payloadHash": "testuniquehash12345",
        }),
    );
    let ev1 = handle_notification_packet_with_payload("Pixel", &pkt1, Some(&avatar_png), &mut active, &mut history)
        .expect("packet 1");
    assert!(ev1.is_some());
    assert!(active["cache-notif-1"].has_avatar);
    let cached_icon_path = active["cache-notif-1"].icon_path.clone().expect("icon path");

    let pkt2 = NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION,
        json!({
            "id": "org.kde.kdeconnect_tp::cache-notif-2",
            "appName": "Signal",
            "title": "David",
            "text": "Message two",
            "payloadHash": "testuniquehash12345",
        }),
    );
    let ev2 =
        handle_notification_packet_with_payload("Pixel", &pkt2, None, &mut active, &mut history).expect("packet 2");
    assert!(ev2.is_some());
    assert!(active["cache-notif-2"].has_avatar);
    assert_eq!(active["cache-notif-2"].icon_path.as_deref(), Some(cached_icon_path.as_str()));
}

#[test]
fn notification_outgoing_packets() {
    let reply = create_reply_packet("reply-1", "Thanks!");
    assert_eq!(reply.body["requestReplyId"], "reply-1");
    assert_eq!(reply.body["message"], "Thanks!");

    let action = create_action_packet("act-1", "Archive");
    assert_eq!(action.body["key"], "act-1");
    assert_eq!(action.body["action"], "Archive");

    let req = create_request_packet();
    assert_eq!(req.body["request"], true);
}

#[test]
fn find_my_phone_packet() {
    let pkt = create_ring_packet();
    assert_eq!(pkt.packet_type, PACKET_TYPE_FINDMYPHONE_REQUEST);
}

#[test]
fn a_flood_of_made_up_devices_cannot_grow_the_device_list_without_bound() {
    use glide_phone::config::PairedDevice;
    use glide_phone::state::MAX_KNOWN_DEVICES;
    let cert = IdentityCert::generate("local_1234567890123456789012345678").expect("cert");
    let paired =
        PairedDevice { id: "paired_phone".to_string(), name: "Pixel".to_string(), fingerprint: "ab".to_string() };
    let config = PhoneConfig {
        device_id: "local_1234567890123456789012345678".to_string(),
        device_name: "Laptop".to_string(),
        cert_pem: cert.cert_pem.clone(),
        key_pem: cert.key_pem.clone(),
        cert_der: cert.cert_der.clone(),
        key_der: cert.key_der.clone(),
        paired_devices: HashMap::from([(paired.id.clone(), paired)]),
        config_path: None,
    };
    let mut manager = PhoneManager::new(config, cert);
    let admitted = (0..MAX_KNOWN_DEVICES * 4)
        .filter(|index| manager.register_discovered(format!("fake_{index}"), "Fake".to_string(), DeviceType::Phone))
        .count();
    assert_eq!(manager.devices.len(), MAX_KNOWN_DEVICES);
    assert_eq!(admitted, MAX_KNOWN_DEVICES - 1, "the paired phone holds one place");
    assert!(manager.register_discovered("fake_0".to_string(), "Renamed".to_string(), DeviceType::Phone));
    assert!(manager.register_discovered("paired_phone".to_string(), "Pixel".to_string(), DeviceType::Phone));
}

fn local_config(cert: &IdentityCert, paired: Vec<glide_phone::config::PairedDevice>) -> PhoneConfig {
    PhoneConfig {
        device_id: "local_1234567890123456789012345678".to_string(),
        device_name: "Laptop".to_string(),
        cert_pem: cert.cert_pem.clone(),
        key_pem: cert.key_pem.clone(),
        cert_der: cert.cert_der.clone(),
        key_der: cert.key_der.clone(),
        paired_devices: paired.into_iter().map(|device| (device.id.clone(), device)).collect(),
        config_path: None,
    }
}

#[test]
fn a_phone_saved_as_paired_without_its_certificate_is_not_trusted() {
    use glide_phone::config::PairedDevice;
    let cert = IdentityCert::generate("local_1234567890123456789012345678").expect("cert");
    let paired = |id: &str, fingerprint: &str| PairedDevice {
        id: id.to_string(),
        name: "Pixel".to_string(),
        fingerprint: fingerprint.to_string(),
    };
    let mut manager = PhoneManager::new(local_config(&cert, vec![paired("pinned", "AA"), paired("blank", "")]), cert);
    assert_eq!(manager.devices["pinned"].pair_state, PairState::Paired);
    assert!(!manager.devices.contains_key("blank") && !manager.config.paired_devices.contains_key("blank"));

    manager.register_discovered("blank".to_string(), "Pixel".to_string(), DeviceType::Phone);
    let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
    manager.register_connection("blank".to_string(), sender, Some("EE".to_string()));
    assert_eq!(manager.devices["blank"].pair_state, PairState::Unpaired, "any certificate used to pass as paired");
    let notification = NetworkPacket::new(PACKET_TYPE_NOTIFICATION, json!({"id": "n", "appName": "A", "title": "T"}));
    manager.handle_packet("blank", &notification).expect("handled");
    assert!(manager.devices["blank"].notifications.is_empty());
}

#[test]
fn a_certificate_that_changes_while_pairing_starts_the_pairing_over() {
    let cert = IdentityCert::generate("local_1234567890123456789012345678").expect("cert");
    let mut manager = PhoneManager::new(local_config(&cert, Vec::new()), cert);
    manager.register_discovered("phone_1".to_string(), "Pixel".to_string(), DeviceType::Phone);
    let (shown, _shown_receiver) = tokio::sync::mpsc::unbounded_channel();
    manager.register_connection("phone_1".to_string(), shown, Some("AA".to_string()));
    manager.accept_pair("phone_1", String::new()).expect("requested");
    assert_eq!(manager.devices["phone_1"].pair_state, PairState::RequestedByLocal);
    assert!(manager.presents("phone_1", Some("AA")) && !manager.presents("phone_1", Some("EE")));
    assert!(!manager.presents("phone_1", None) && !manager.presents("unknown", Some("AA")));

    let (impostor, _impostor_receiver) = tokio::sync::mpsc::unbounded_channel();
    manager.register_connection("phone_1".to_string(), impostor, Some("EE".to_string()));
    assert_eq!(manager.devices["phone_1"].pair_state, PairState::Unpaired);
    manager.handle_packet("phone_1", &PairBody::accept().to_packet().unwrap()).expect("handled");
    assert!(manager.config.paired_devices.is_empty(), "the swapped certificate is never pinned by the old request");
}

#[test]
fn phone_manager_pairing_and_state() {
    let cert = IdentityCert::generate("local_1234567890123456789012345678").expect("cert");
    let mut manager = PhoneManager::new(local_config(&cert, Vec::new()), cert);
    manager.register_discovered("phone_1".to_string(), "Pixel".to_string(), DeviceType::Phone);

    let dev = manager.devices.get("phone_1").expect("device exists");
    assert_eq!(dev.pair_state, PairState::Unpaired);
    let unpaired_presenter = NetworkPacket::new("kdeconnect.mousepad.request", json!({"specialKey": 9}));
    manager.handle_packet("phone_1", &unpaired_presenter).unwrap();
    assert_eq!(manager.take_presenter_event(), None);

    let unsolicited = PairBody::accept().to_packet().unwrap();
    manager.handle_packet("phone_1", &unsolicited).unwrap();
    assert_eq!(manager.devices["phone_1"].pair_state, PairState::Unpaired);

    let _req_pkt = manager.accept_pair("phone_1", String::new()).expect("request pair from local");
    assert_eq!(manager.devices["phone_1"].pair_state, PairState::RequestedByLocal);

    let pair_req = NetworkPacket::new(PACKET_TYPE_PAIR, json!({ "pair": true }));
    manager.handle_packet("phone_1", &pair_req).expect("handled");
    assert_eq!(
        manager.devices["phone_1"].pair_state,
        PairState::RequestedByLocal,
        "an acceptance before any certificate was seen pairs with nobody"
    );
    assert!(manager.config.paired_devices.is_empty());

    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let old_sender = sender.downgrade();
    manager.register_connection("phone_1".to_string(), sender, Some("AA:BB:CC".to_string()));
    manager.send_pending_pair_request("phone_1");
    let pending = receiver.try_recv().expect("queued request sent on connection");
    assert!(PairBody::from_packet(&pending).unwrap().pair);

    let (replacement, _replacement_receiver) = tokio::sync::mpsc::unbounded_channel();
    let replacement_sender = replacement.downgrade();
    manager.register_connection("phone_1".to_string(), replacement, Some("AA:BB:CC".to_string()));
    manager.unregister_connection("phone_1", &old_sender);
    assert!(manager.is_connected("phone_1"));
    manager.unregister_connection("phone_1", &replacement_sender);
    assert!(!manager.is_connected("phone_1"));

    let _ = manager.handle_packet("phone_1", &pair_req).expect("handle pair");
    assert_eq!(manager.config.paired_devices["phone_1"].fingerprint, "AA:BB:CC");
    assert_eq!(manager.devices["phone_1"].pair_state, PairState::Paired);
    assert!(manager.config.paired_devices.contains_key("phone_1"));

    let _ = manager.reject_pair("phone_1").expect("reject pair");
    assert_eq!(manager.devices["phone_1"].pair_state, PairState::Unpaired);
    assert!(!manager.config.paired_devices.contains_key("phone_1"));

    let pair_req_from_peer = NetworkPacket::new(PACKET_TYPE_PAIR, json!({ "pair": true, "timestamp": 1_730_000_000 }));
    let _ = manager.handle_packet("phone_1", &pair_req_from_peer).expect("handle peer pair request");
    assert_eq!(manager.devices["phone_1"].pair_state, PairState::RequestedByPeer);

    let _accept_pkt = manager.accept_pair("phone_1", "AA:BB:CC".to_string()).expect("accept pair");
    assert_eq!(manager.devices["phone_1"].pair_state, PairState::Paired);
    assert_eq!(manager.config.paired_devices["phone_1"].fingerprint, "AA:BB:CC");

    manager.register_discovered("phone_2".to_string(), "Second".to_string(), DeviceType::Phone);
    manager.accept_pair("phone_2", "CC:DD".to_string()).unwrap();
    let (first, _first_receiver) = tokio::sync::mpsc::unbounded_channel();
    let first_weak = first.downgrade();
    manager.register_connection("phone_1".to_string(), first, None);
    let (second, _second_receiver) = tokio::sync::mpsc::unbounded_channel();
    manager.register_connection("phone_2".to_string(), second, None);
    let first_move = NetworkPacket::new("kdeconnect.presenter", json!({"dx": 0.1, "dy": 0.0}));
    let second_move = NetworkPacket::new("kdeconnect.presenter", json!({"dx": 0.2, "dy": 0.0}));
    manager.handle_packet("phone_1", &first_move).unwrap();
    let (replacement, _replacement_receiver) = tokio::sync::mpsc::unbounded_channel();
    let replacement_weak = replacement.downgrade();
    manager.register_connection("phone_1".to_string(), replacement, None);
    manager.unregister_connection("phone_1", &first_weak);
    manager.handle_packet("phone_2", &second_move).unwrap();
    manager.handle_packet("phone_2", &second_move).unwrap();
    let next_slide = NetworkPacket::new("kdeconnect.mousepad.request", json!({"specialKey": 9}));
    manager.handle_packet("phone_2", &next_slide).unwrap();
    manager.handle_packet("phone_2", &second_move).unwrap();
    manager.unregister_connection("phone_1", &replacement_weak);
    assert_eq!(manager.take_presenter_event(), Some(glide_phone::PresenterEvent::Move { dx: 0.4, dy: 0.0 }));
    assert_eq!(manager.take_presenter_event(), Some(glide_phone::PresenterEvent::Key { code: 109, ctrl: false }));
    assert_eq!(manager.take_presenter_event(), Some(glide_phone::PresenterEvent::Move { dx: 0.2, dy: 0.0 }));
    assert_eq!(manager.take_presenter_event(), None);
    manager.handle_packet("phone_2", &next_slide).unwrap();
    manager.reject_pair("phone_2").unwrap();
    assert_eq!(manager.take_presenter_event(), None);
    manager.accept_pair("phone_2", "CC:DD".to_string()).unwrap();
    manager.handle_packet("phone_2", &next_slide).unwrap();
    manager.handle_packet("phone_2", &PairBody::reject().to_packet().unwrap()).unwrap();
    assert_eq!(manager.take_presenter_event(), None);

    let notif_pkt = NetworkPacket::new(
        PACKET_TYPE_NOTIFICATION,
        json!({
            "id": "notif-99",
            "appName": "Telegram",
            "title": "Bob",
            "text": "Hi!",
        }),
    );
    let _ = manager.handle_packet("phone_1", &notif_pkt).expect("add notif");
    assert!(manager.devices["phone_1"].notifications.contains_key("notif-99"));

    let dismiss_pkt = manager.dismiss_notification("phone_1", "notif-99").expect("dismiss notif");
    assert_eq!(dismiss_pkt.body["cancel"], "notif-99");
    assert!(!manager.devices["phone_1"].notifications.contains_key("notif-99"));

    let ring_pkt = manager.ring_phone("phone_1").expect("ring phone");
    assert_eq!(ring_pkt.packet_type, PACKET_TYPE_FINDMYPHONE_REQUEST);
}

#[test]
fn config_persists_certificate_and_device_id() {
    let temp_path = std::env::temp_dir().join(format!("glide_phone_test_{}.json", std::process::id()));
    let _ = std::fs::remove_file(&temp_path);

    let (mut cfg1, cert1) = PhoneConfig::load_or_generate(&temp_path).expect("generate config");
    assert_eq!(cfg1.device_id.len(), 32);
    assert!(!cfg1.cert_der.is_empty());
    assert!(!cfg1.key_der.is_empty());
    let expected_name = cfg1.device_name.clone();
    cfg1.device_name = "old name".to_string();
    cfg1.save(&temp_path).expect("save old name");

    let (cfg2, cert2) = PhoneConfig::load_or_generate(&temp_path).expect("reload config");
    assert_eq!(cfg1.device_id, cfg2.device_id);
    assert_eq!(cfg2.device_name, expected_name);
    let persisted: PhoneConfig =
        serde_json::from_str(&std::fs::read_to_string(&temp_path).expect("read config")).expect("parse config");
    assert_eq!(persisted.device_name, expected_name);
    assert_eq!(cert1.fingerprint, cert2.fingerprint);
    assert_eq!(cert1.cert_der, cert2.cert_der);
    assert_eq!(cert1.key_der, cert2.key_der);

    let _ = std::fs::remove_file(&temp_path);
}

#[tokio::test]
async fn phone_daemon_dbus_api() {
    use glide_phone::daemon::PhoneDaemon;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let cert = IdentityCert::generate("local_1234567890123456789012345678").expect("cert");
    let config = PhoneConfig {
        device_id: "local_1234567890123456789012345678".to_string(),
        device_name: "Laptop".to_string(),
        cert_pem: cert.cert_pem.clone(),
        key_pem: cert.key_pem.clone(),
        cert_der: cert.cert_der.clone(),
        key_der: cert.key_der.clone(),
        paired_devices: HashMap::new(),
        config_path: None,
    };

    let mut manager = PhoneManager::new(config, cert);
    manager.register_discovered("phone_1".to_string(), "Pixel".to_string(), DeviceType::Phone);

    let shared = Arc::new(Mutex::new(manager));
    let daemon = PhoneDaemon::new(shared.clone());

    let json_list = daemon.list_devices_json().await;
    assert!(json_list.contains("Pixel"));

    {
        let mut mgr = shared.lock().await;
        let _ = mgr.accept_pair("phone_1", "FINGERPRINT".to_string());
        let notif_pkt = NetworkPacket::new(
            PACKET_TYPE_NOTIFICATION,
            serde_json::json!({
                "id": "notif-1",
                "appName": "Telegram",
                "title": "Alice",
                "text": "Hello",
            }),
        );
        let _ = mgr.handle_packet("phone_1", &notif_pkt);
    }

    assert!(daemon.clear_notifications("phone_1").await);
    assert!(!daemon.clear_notifications("phone_1").await);

    {
        let mut mgr = shared.lock().await;
        let packet = NetworkPacket::new("kdeconnect.mousepad.request", json!({"specialKey": 9}));
        mgr.handle_packet("phone_1", &packet).unwrap();
    }
    let event = daemon.take_presenter_event().await;
    assert_eq!(
        serde_json::from_str::<glide_phone::PresenterEvent>(&event).unwrap(),
        glide_phone::PresenterEvent::Key { code: 109, ctrl: false }
    );
    assert!(daemon.take_presenter_event().await.is_empty());

    if let Some(bus) = glide_services::test_bus::private_bus() {
        let server = zbus::connection::Builder::address(bus.address.as_str())
            .unwrap()
            .name("org.glide.Phone")
            .unwrap()
            .serve_at("/org/glide/Phone", PhoneDaemon::new(shared.clone()))
            .unwrap()
            .build()
            .await
            .unwrap();
        {
            let mut mgr = shared.lock().await;
            mgr.handle_packet("phone_1", &NetworkPacket::new("kdeconnect.presenter", json!({"dx": 0.25, "dy": -0.5})))
                .unwrap();
        }
        let client = zbus::connection::Builder::address(bus.address.as_str()).unwrap().build().await.unwrap();
        let reply = client
            .call_method(
                Some("org.glide.Phone"),
                "/org/glide/Phone",
                Some("org.glide.Phone"),
                "TakePresenterEvent",
                &(),
            )
            .await
            .unwrap();
        let event: String = reply.body().deserialize().unwrap();
        assert_eq!(
            serde_json::from_str::<glide_phone::PresenterEvent>(&event).unwrap(),
            glide_phone::PresenterEvent::Move { dx: 0.25, dy: -0.5 }
        );
        drop(server);
    }

    {
        let mut mgr = shared.lock().await;
        let ring_req = NetworkPacket::new(PACKET_TYPE_FINDMYPHONE_REQUEST, serde_json::json!({}));
        let _ = mgr.handle_packet("phone_1", &ring_req);
    }

    assert!(daemon.is_ringing().await);
    let _ = daemon.stop_alarm("").await;
    assert!(!daemon.is_ringing().await);
}

#[tokio::test]
async fn tcp_tls_v8_role_inversion_handshake() {
    let cert_a = IdentityCert::generate("device_a_0123456789abcdef01234567").expect("cert a");
    let config_a = PhoneConfig {
        device_id: "device_a_0123456789abcdef01234567".to_string(),
        device_name: "Glide Desktop".to_string(),
        cert_pem: cert_a.cert_pem.clone(),
        key_pem: cert_a.key_pem.clone(),
        cert_der: cert_a.cert_der.clone(),
        key_der: cert_a.key_der.clone(),
        paired_devices: HashMap::new(),
        config_path: None,
    };

    let cert_b = IdentityCert::generate("device_b_0123456789abcdef01234567").expect("cert b");
    let fp_b = cert_b.fingerprint.clone();
    let config_b = PhoneConfig {
        device_id: "device_b_0123456789abcdef01234567".to_string(),
        device_name: "Pixel Phone".to_string(),
        cert_pem: cert_b.cert_pem.clone(),
        key_pem: cert_b.key_pem.clone(),
        cert_der: cert_b.cert_der.clone(),
        key_der: cert_b.key_der.clone(),
        paired_devices: HashMap::new(),
        config_path: None,
    };

    let (service_a, listener_a) = glide_phone::service::PhoneService::bind(config_a, cert_a).await.expect("bind a");
    let port_a = service_a.tcp_port();
    assert!(port_a >= 1716);
    let mgr_a = service_a.manager();

    let (service_b, _listener_b) = glide_phone::service::PhoneService::bind(config_b, cert_b).await.expect("bind b");
    let mgr_b = service_b.manager();

    tokio::spawn(async move {
        if let Ok((stream, addr)) = listener_a.accept().await {
            let _ = service_a.handle_incoming_connection(stream, addr).await;
        }
    });

    tokio::spawn(async move {
        let target: std::net::SocketAddr = format!("127.0.0.1:{port_a}").parse().unwrap();
        let _ = service_b.connect_to_peer(target, port_a, "device_a_0123456789abcdef01234567".to_string()).await;
    });

    for _ in 0..40 {
        {
            let guard_a = mgr_a.lock().await;
            let guard_b = mgr_b.lock().await;
            if guard_a.is_connected("device_b_0123456789abcdef01234567")
                && guard_b.is_connected("device_a_0123456789abcdef01234567")
            {
                assert_eq!(
                    guard_a.devices["device_b_0123456789abcdef01234567"].peer_fingerprint.as_deref(),
                    Some(fp_b.as_str())
                );
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    panic!("TCP + inverted TLS handshake did not complete in time");
}

#[tokio::test]
async fn android_style_string_version_completes_incoming_handshake() {
    use glide_phone::link::framed::PacketStream;
    use std::sync::Arc;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpStream;
    use tokio_rustls::TlsAcceptor;

    let local_id = "desktop_0123456789abcdef01234567";
    let phone_id = "phone_0123456789abcdef0123456789";
    let desktop = IdentityCert::generate(local_id).unwrap();
    let phone = IdentityCert::generate(phone_id).unwrap();
    let config = PhoneConfig {
        device_id: local_id.to_string(),
        device_name: "Desktop".to_string(),
        cert_pem: desktop.cert_pem.clone(),
        key_pem: desktop.key_pem.clone(),
        cert_der: desktop.cert_der.clone(),
        key_der: desktop.key_der.clone(),
        paired_devices: HashMap::new(),
        config_path: None,
    };
    let (service, listener) = glide_phone::service::PhoneService::bind(config, desktop).await.unwrap();
    let manager = service.manager();
    let address = format!("127.0.0.1:{}", service.tcp_port());
    let task = tokio::spawn(async move {
        let (stream, addr) = listener.accept().await.unwrap();
        service.handle_incoming_connection(stream, addr).await
    });
    let mut stream = TcpStream::connect(address).await.unwrap();
    let initial = NetworkPacket::new(
        "kdeconnect.identity",
        json!({"deviceId": phone_id, "deviceName": "Pixel", "protocolVersion": 8,
            "targetDeviceId": local_id, "targetProtocolVersion": "8"}),
    );
    stream.write_all(initial.serialize().unwrap().as_bytes()).await.unwrap();
    let server = glide_phone::crypto::tls::make_server_config(&phone, None).unwrap();
    let tls = TlsAcceptor::from(Arc::new(server)).accept(stream).await.unwrap();
    let mut packets = PacketStream::new(tls);
    let identity = IdentityBody {
        device_id: phone_id.to_string(),
        device_name: "Pixel".to_string(),
        device_type: DeviceType::Phone,
        protocol_version: PROTOCOL_VERSION,
        tcp_port: None,
        target_device_id: None,
        target_protocol_version: None,
        incoming_capabilities: vec![],
        outgoing_capabilities: vec![],
    };
    packets.send_packet(&identity.to_packet().unwrap()).await.unwrap();
    assert_eq!(packets.next_packet().await.unwrap().unwrap().packet_type, "kdeconnect.identity");
    for _ in 0..40 {
        if manager.lock().await.is_connected(phone_id) {
            drop(packets);
            task.await.unwrap().unwrap();
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("Android-style handshake did not complete");
}

#[tokio::test]
async fn tls_rejects_certificate_without_matching_private_key() {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
    use rustls::server::{ClientHello, ResolvesServerCert};
    use rustls::sign::CertifiedKey;
    use std::sync::Arc;
    use tokio::net::{TcpListener, TcpStream};
    use tokio_rustls::{TlsAcceptor, TlsConnector};

    #[derive(Debug)]
    struct Resolver(Arc<CertifiedKey>);
    impl ResolvesServerCert for Resolver {
        fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
            Some(self.0.clone())
        }
    }

    let attacker = IdentityCert::generate("attacker_0123456789abcdef0123456").unwrap();
    let victim = IdentityCert::generate("victim_0123456789abcdef012345678").unwrap();
    let key = PrivateKeyDer::Pkcs8(attacker.key_der.clone().into());
    let signing_key = rustls::crypto::ring::sign::any_supported_type(&key).unwrap();
    let forged = CertifiedKey::new(vec![CertificateDer::from(victim.cert_der.clone())], signing_key);
    let server = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(Resolver(Arc::new(forged))));
    let client = glide_phone::crypto::tls::make_client_config(&attacker, Some(victim.fingerprint)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server_task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let _ = TlsAcceptor::from(Arc::new(server)).accept(socket).await;
    });
    let stream = TcpStream::connect(address).await.unwrap();
    let result =
        TlsConnector::from(Arc::new(client)).connect(ServerName::try_from("kdeconnect").unwrap(), stream).await;
    assert!(result.is_err(), "a pinned certificate must not authenticate without its private key");
    server_task.await.unwrap();
}

use glide_settings::app::Message;
use glide_settings::services::audio::{self, MeterEvent, TestSound};
use glide_settings::services::config::ConfigStore;
use glide_settings::services::printers::{self, PrinterState};
use glide_settings::{App, Panel};

const SINK_INPUTS: &str = r#"[
 {"index":71,"sink":52,"mute":false,"channel_map":"front-left,front-right",
  "volume":{"front-left":{"value":65536,"value_percent":"100%"},"front-right":{"value":32768,"value_percent":"50%"}},
  "properties":{"application.name":"Firefox","media.name":"Lo-fi radio","application.process.binary":"firefox"}},
 {"index":72,"mute":true,
  "volume":{"mono":{"value":32768}},
  "properties":{"application.process.binary":"mpv","media.name":"mpv"}},
 {"index":73,"volume":{"mono":{"value":1}},"properties":{}}
]"#;

const SINKS: &str = r#"[
 {"index":52,"name":"alsa_output.speaker","channel_map":"front-left,front-right",
  "volume":{"front-left":{"value":65536},"front-right":{"value":32768}}},
 {"index":60,"name":"alsa_output.mono","channel_map":"mono",
  "volume":{"mono":{"value":65536}}}
]"#;

#[test]
fn sink_inputs_become_named_streams_with_averaged_volume() {
    let streams = audio::parse_sink_inputs(SINK_INPUTS);
    assert_eq!(streams.len(), 2, "a stream with no name is dropped");
    assert_eq!(streams[0].index, 71);
    assert_eq!(streams[0].app, "Firefox");
    assert_eq!(streams[0].title, "Lo-fi radio");
    assert!((streams[0].volume - 0.75).abs() < 1e-6);
    assert!(!streams[0].muted);
    assert_eq!(streams[1].app, "mpv");
    assert_eq!(streams[1].title, "", "a title equal to the app name is not repeated");
    assert!(streams[1].muted);
    assert!(audio::parse_sink_inputs("not json").is_empty());
    assert!(audio::parse_sink_inputs("[]").is_empty());
}

#[test]
fn only_a_stereo_default_sink_offers_balance() {
    let stereo = audio::parse_stereo(SINKS, "alsa_output.speaker\n").expect("stereo sink");
    assert!((stereo.left - 1.0).abs() < 1e-6);
    assert!((stereo.right - 0.5).abs() < 1e-6);
    assert!((stereo.balance() + 0.5).abs() < 1e-6);
    assert!(audio::parse_stereo(SINKS, "alsa_output.mono").is_none());
    assert!(audio::parse_stereo(SINKS, "missing").is_none());
}

#[test]
fn balance_round_trips_through_channel_volumes() {
    for balance in [-1.0_f32, -0.4, 0.0, 0.3, 1.0] {
        let (left, right) = audio::split_balance(0.8, balance);
        assert!(left.max(right) - 0.8 < 1e-6, "the louder side keeps the volume");
        assert!((audio::balance_of(left, right) - balance).abs() < 1e-5);
    }
    assert!(audio::balance_of(0.0, 0.0).abs() < f32::EPSILON);
}

#[test]
fn the_meter_maps_rms_to_a_sixty_decibel_scale() {
    assert!(audio::meter_level(&[]).abs() < f32::EPSILON);
    assert!(audio::meter_level(&[0; 64]).abs() < f32::EPSILON);
    let full: Vec<u8> = (0..64).flat_map(|i| if i % 2 == 0 { i16::MAX } else { i16::MIN + 1 }.to_le_bytes()).collect();
    assert!(audio::meter_level(&full) > 0.99);
    let quiet: Vec<u8> = (0..64).flat_map(|_| 33_i16.to_le_bytes()).collect();
    let level = audio::meter_level(&quiet);
    assert!(level > 0.0 && level < 0.1, "{level}");
}

#[test]
fn test_sounds_fall_back_to_the_next_file() {
    let dir = std::env::temp_dir().join(format!("glide-sounds-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    assert!(audio::test_sound_in(&dir, TestSound::Both).is_none());
    std::fs::write(dir.join("bell.oga"), b"x").expect("write");
    assert_eq!(audio::test_sound_in(&dir, TestSound::Both), Some(dir.join("bell.oga")));
    assert!(audio::test_sound_in(&dir, TestSound::Left).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

const LPSTAT_P: &str = "printer HP_LaserJet is idle.  enabled since Mon 01 Jan 2024 10:00:00 AM
printer Office now printing Office-12.  enabled since Mon 01 Jan 2024 10:00:00 AM
printer Brother_HL is disabled since Mon 01 Jan 2024 10:00:00 AM -
\tPaper jam
system default destination: Office
";

#[test]
fn lpstat_printers_carry_state_job_reason_and_default() {
    let (list, default) = printers::parse_printers(LPSTAT_P);
    assert_eq!(default.as_deref(), Some("Office"));
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].state, PrinterState::Idle);
    assert!(!list[0].is_default);
    assert_eq!(list[1].state, PrinterState::Printing);
    assert_eq!(list[1].job.as_deref(), Some("Office-12"));
    assert!(list[1].is_default);
    assert_eq!(list[2].state, PrinterState::Disabled);
    assert_eq!(list[2].reason.as_deref(), Some("Paper jam"));
    let (none, default) = printers::parse_printers("no system default destination\n");
    assert!(none.is_empty());
    assert!(default.is_none());
}

#[test]
fn lpstat_devices_and_jobs_parse() {
    let devices = printers::parse_devices(
        "device for HP_LaserJet: usb://HP/LaserJet?serial=1\ndevice for Office: ipp://10.0.0.5/ipp/print\n",
    );
    assert_eq!(devices[1].0, "Office");
    assert_eq!(devices[1].1, "ipp://10.0.0.5/ipp/print");
    assert_eq!(printers::connection_label(&devices[0].1), "USB");
    assert_eq!(printers::connection_label(&devices[1].1), "network");

    let (list, _) = printers::parse_printers(LPSTAT_P);
    let jobs = printers::parse_jobs(
        "Office-12               artem          2048   Mon 01 Jan 2024 10:00:00 AM\nHP_LaserJet-3 bob 10\ngarbage\n",
        &list,
    );
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].id, "Office-12");
    assert_eq!(jobs[0].printer, "Office");
    assert_eq!(jobs[0].user, "artem");
    assert_eq!(jobs[0].size, 2048);
    assert_eq!(jobs[0].submitted, "Mon 01 Jan 2024 10:00:00 AM");
    assert_eq!(jobs[1].printer, "HP_LaserJet");
}

#[test]
fn sound_and_printer_messages_render_without_a_backend() {
    use glide_settings::panels::{printers as page, sound};
    let mut app = App::with_config(ConfigStore::parse(""));
    let _ = app.update(Message::PanelSelected(Panel::Sound));
    let _ = app.update(Message::Sound(sound::Msg::Meter(99, MeterEvent::Level(0.5))));
    let _ = app.update(Message::Sound(sound::Msg::Balance(0.02)));
    let _ = app.update(Message::Sound(sound::Msg::CommitBalance));
    let _ = app.view();
    let _ = app.update(Message::PanelSelected(Panel::Printers));
    let _ = app.update(Message::Printers(page::Msg::Select(String::from("Office"))));
    let _ = app.view();
}

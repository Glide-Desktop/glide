use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use iced::futures::channel::mpsc;
use serde_json::Value;

use super::run;

pub const SINK: &str = "@DEFAULT_AUDIO_SINK@";
pub const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";
pub(crate) const PULSE_SINK: &str = "@DEFAULT_SINK@";
pub(crate) const PULSE_SOURCE: &str = "@DEFAULT_SOURCE@";
pub(crate) const SOUNDS_DIR: &str = "/usr/share/sounds/freedesktop/stereo";
pub(crate) const METER_DURATION: Duration = Duration::from_secs(10);
pub(crate) const METER_RATE: usize = 16_000;
pub(crate) const METER_CHUNK: usize = METER_RATE / 10 * 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: u32,
    pub name: String,
    pub is_default: bool,
}

impl std::fmt::Display for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Level {
    pub volume: f32,
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    pub index: u32,
    pub app: String,
    pub title: String,
    pub icon_name: Option<String>,
    pub icon: Option<PathBuf>,
    pub volume: f32,
    pub muted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stereo {
    pub left: f32,
    pub right: f32,
}

impl Stereo {
    #[must_use]
    pub fn balance(self) -> f32 {
        balance_of(self.left, self.right)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestSound {
    Left,
    Right,
    Both,
}

impl TestSound {
    #[must_use]
    pub const fn files(self) -> &'static [&'static str] {
        match self {
            Self::Left => &["audio-channel-front-left.oga"],
            Self::Right => &["audio-channel-front-right.oga"],
            Self::Both => &["audio-test-signal.oga", "bell.oga"],
        }
    }

    #[must_use]
    pub fn path(self) -> Option<PathBuf> {
        test_sound_in(Path::new(SOUNDS_DIR), self)
    }
}

#[must_use]
pub fn test_sound_in(dir: &Path, sound: TestSound) -> Option<PathBuf> {
    sound.files().iter().map(|file| dir.join(file)).find(|path| path.is_file())
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AudioState {
    pub available: bool,
    pub sinks: Vec<Device>,
    pub sources: Vec<Device>,
    pub output: Option<Level>,
    pub input: Option<Level>,
    pub streams: Vec<Stream>,
    pub streams_supported: bool,
    pub stereo: Option<Stereo>,
    pub player: Option<&'static str>,
    pub meter: bool,
}

impl AudioState {
    #[must_use]
    pub(crate) fn default_sink(&self) -> Option<&Device> {
        self.sinks.iter().find(|device| device.is_default)
    }

    #[must_use]
    pub(crate) fn default_source(&self) -> Option<&Device> {
        self.sources.iter().find(|device| device.is_default)
    }
}

#[must_use]
pub fn parse_volume(text: &str) -> Option<Level> {
    let rest = text.trim().strip_prefix("Volume:")?.trim();
    let number = rest.split_whitespace().next()?;
    Some(Level { volume: number.parse().ok()?, muted: rest.contains("[MUTED]") })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Other,
    Sinks,
    Sources,
}

fn parse_entry(line: &str) -> Option<Device> {
    let trimmed = line.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '│' | '├' | '└' | '─'));
    let (is_default, rest) = match trimmed.strip_prefix('*') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, trimmed),
    };
    let (id, rest) = rest.split_once(". ")?;
    let id = id.trim().parse().ok()?;
    let name = rest.split(" [").next().unwrap_or(rest).trim();
    (!name.is_empty()).then(|| Device { id, name: name.to_string(), is_default })
}

#[must_use]
pub fn parse_status(text: &str) -> (Vec<Device>, Vec<Device>) {
    let mut sinks = Vec::new();
    let mut sources = Vec::new();
    let mut in_audio = false;
    let mut section = Section::Other;
    for line in text.lines() {
        let bare = line.trim();
        if !line.starts_with([' ', '│', '├', '└']) && !bare.is_empty() {
            in_audio = bare == "Audio";
            section = Section::Other;
            continue;
        }
        if !in_audio {
            continue;
        }
        let label = bare.trim_start_matches(['│', '├', '└', '─', ' ']);
        if label.ends_with(':') {
            section = match label {
                "Sinks:" => Section::Sinks,
                "Sources:" => Section::Sources,
                _ => Section::Other,
            };
            continue;
        }
        let Some(device) = parse_entry(line) else {
            continue;
        };
        match section {
            Section::Sinks => sinks.push(device),
            Section::Sources => sources.push(device),
            Section::Other => {}
        }
    }
    (sinks, sources)
}

fn channel_value(volume: &Value, channel: &str) -> Option<f32> {
    let raw = volume.get(channel)?.get("value")?.as_f64()?;
    Some((raw / 65536.0) as f32)
}

fn average_volume(volume: &Value) -> Option<f32> {
    let channels = volume.as_object()?;
    let values: Vec<f64> = channels.values().filter_map(|channel| channel.get("value")?.as_f64()).collect();
    if values.is_empty() {
        return None;
    }
    Some((values.iter().sum::<f64>() / values.len() as f64 / 65536.0) as f32)
}

fn property<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    entry.get("properties")?.get(key)?.as_str().map(str::trim).filter(|text| !text.is_empty())
}

#[must_use]
pub fn parse_sink_inputs(json: &str) -> Vec<Stream> {
    let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let index = u32::try_from(entry.get("index")?.as_u64()?).ok()?;
            let volume = entry.get("volume").and_then(average_volume)?;
            let title = property(entry, "media.name").unwrap_or_default();
            let app = property(entry, "application.name")
                .or_else(|| property(entry, "application.process.binary"))
                .or_else(|| property(entry, "node.name"))
                .unwrap_or(title);
            if app.is_empty() {
                return None;
            }
            Some(Stream {
                index,
                app: app.to_string(),
                title: if title == app { String::new() } else { title.to_string() },
                icon_name: property(entry, "application.icon_name")
                    .filter(|name| !name.starts_with('.') && !name.contains('/'))
                    .map(str::to_string),
                icon: None,
                volume,
                muted: entry.get("mute").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

#[must_use]
pub fn parse_stereo(json: &str, default_sink: &str) -> Option<Stereo> {
    let Ok(Value::Array(sinks)) = serde_json::from_str::<Value>(json) else {
        return None;
    };
    let sink = sinks.iter().find(|sink| sink.get("name").and_then(Value::as_str) == Some(default_sink.trim()))?;
    let map = sink.get("channel_map")?.as_str()?;
    if map.trim() != "front-left,front-right" {
        return None;
    }
    let volume = sink.get("volume")?;
    Some(Stereo { left: channel_value(volume, "front-left")?, right: channel_value(volume, "front-right")? })
}

#[must_use]
pub fn balance_of(left: f32, right: f32) -> f32 {
    let loudest = left.max(right);
    if loudest <= f32::EPSILON {
        return 0.0;
    }
    if left >= right { -(1.0 - right / loudest) } else { 1.0 - left / loudest }
}

#[must_use]
pub fn split_balance(volume: f32, balance: f32) -> (f32, f32) {
    let balance = balance.clamp(-1.0, 1.0);
    (volume * (1.0 - balance.max(0.0)), volume * (1.0 + balance.min(0.0)))
}

#[must_use]
pub fn meter_level(samples: &[u8]) -> f32 {
    let count = samples.len() / 2;
    if count == 0 {
        return 0.0;
    }
    let sum: f64 = samples
        .chunks_exact(2)
        .map(|pair| {
            let value = f64::from(i16::from_le_bytes([pair[0], pair[1]])) / 32768.0;
            value * value
        })
        .sum();
    let rms = (sum / count as f64).sqrt();
    if rms <= 1e-6 {
        return 0.0;
    }
    let db = 20.0 * rms.log10();
    ((db + 60.0) / 60.0).clamp(0.0, 1.0) as f32
}

#[must_use]
pub fn read() -> AudioState {
    let Some(status) = run::output("wpctl", &["status"]) else {
        return AudioState::default();
    };
    let (sinks, sources) = parse_status(&status);
    let level = |target: &str| run::output("wpctl", &["get-volume", target]).as_deref().and_then(parse_volume);
    let pactl = run::available("pactl");
    let streams = pactl.then(|| run::output("pactl", &["-f", "json", "list", "sink-inputs"])).flatten();
    let stereo = if pactl {
        let default = run::output("pactl", &["get-default-sink"]);
        let list = run::output("pactl", &["-f", "json", "list", "sinks"]);
        default.zip(list).and_then(|(default, list)| parse_stereo(&list, &default))
    } else {
        None
    };
    let player = ["pw-play", "paplay"].into_iter().find(|tool| run::available(tool));
    let streams_supported = streams.is_some();
    let mut streams = streams.as_deref().map(parse_sink_inputs).unwrap_or_default();
    let icons = super::apps::icon_paths(streams.iter().filter_map(|stream| stream.icon_name.as_deref()), None);
    for stream in &mut streams {
        stream.icon = stream.icon_name.as_ref().and_then(|name| icons.get(name)).cloned();
    }
    AudioState {
        available: true,
        output: level(SINK),
        input: level(SOURCE),
        sinks,
        sources,
        streams_supported,
        streams,
        stereo,
        player,
        meter: run::available("parec"),
    }
}

pub(crate) fn set_volume(target: &str, volume: f32) -> Result<String, String> {
    let volume = format!("{:.2}", volume.clamp(0.0, 1.5));
    run::status("wpctl", &["set-volume", target, &volume])
}

fn percent(volume: f32) -> String {
    format!("{:.1}%", volume.clamp(0.0, 1.5) * 100.0)
}

pub(crate) fn set_output_volume(volume: f32, balance: Option<f32>) -> Result<String, String> {
    match balance.filter(|balance| balance.abs() > 0.005) {
        Some(balance) => set_balance(volume, balance),
        None => set_volume(SINK, volume),
    }
}

pub(crate) fn set_balance(volume: f32, balance: f32) -> Result<String, String> {
    let (left, right) = split_balance(volume, balance);
    run::status("pactl", &["set-sink-volume", PULSE_SINK, &percent(left), &percent(right)])
}

pub(crate) fn set_muted(target: &str, muted: bool) -> Result<String, String> {
    run::status("wpctl", &["set-mute", target, if muted { "1" } else { "0" }])
}

pub fn set_default(id: u32) -> Result<String, String> {
    run::status("wpctl", &["set-default", &id.to_string()])
}

pub(crate) fn set_stream_volume(index: u32, volume: f32) -> Result<String, String> {
    run::status("pactl", &["set-sink-input-volume", &index.to_string(), &percent(volume)])
}

pub(crate) fn set_stream_muted(index: u32, muted: bool) -> Result<String, String> {
    run::status("pactl", &["set-sink-input-mute", &index.to_string(), if muted { "1" } else { "0" }])
}

pub fn play(player: &str, sound: TestSound) -> Result<String, String> {
    let path = sound.path().ok_or_else(|| format!("{SOUNDS_DIR}: no test sound"))?;
    run::status(player, &[&path.to_string_lossy()])
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MeterEvent {
    Level(f32),
    Failed,
    Finished,
}

pub fn meter(stop: Arc<AtomicBool>) -> mpsc::UnboundedReceiver<MeterEvent> {
    let (sender, receiver) = mpsc::unbounded();
    std::thread::spawn(move || {
        let rate = METER_RATE.to_string();
        let spawned = Command::new("parec")
            .args([
                "--raw",
                "--format=s16le",
                "--channels=1",
                &format!("--rate={rate}"),
                "--latency-msec=50",
                "-d",
                PULSE_SOURCE,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = spawned else {
            let _ = sender.unbounded_send(MeterEvent::Failed);
            return;
        };
        if let Some(mut stdout) = child.stdout.take() {
            let reader = sender.clone();
            std::thread::spawn(move || {
                let mut chunk = vec![0u8; METER_CHUNK];
                while stdout.read_exact(&mut chunk).is_ok() {
                    if reader.unbounded_send(MeterEvent::Level(meter_level(&chunk))).is_err() {
                        break;
                    }
                }
            });
        }
        let started = Instant::now();
        let mut exited = false;
        while !stop.load(Ordering::Relaxed) && started.elapsed() < METER_DURATION && !sender.is_closed() {
            if matches!(child.try_wait(), Ok(Some(_))) {
                exited = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if !exited {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = sender.unbounded_send(if exited { MeterEvent::Failed } else { MeterEvent::Finished });
    });
    receiver
}

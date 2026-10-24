use glide_ipc::{ModeInfo, OutputInfo, Request, Response};

use super::{ipc, run};

pub(crate) const SCALES: &[f64] = &[1.0, 1.25, 1.5, 1.75, 2.0];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backlight {
    pub device: String,
    pub percent: u8,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DisplayState {
    pub live: bool,
    pub compositor_running: bool,
    pub outputs: Vec<OutputInfo>,
    pub backlight: Option<Backlight>,
}

#[must_use]
pub fn parse_backlight(text: &str) -> Option<Backlight> {
    text.lines().find_map(|line| {
        let fields: Vec<&str> = line.trim().split(',').collect();
        let [device, class, _, percent, _] = fields.as_slice() else {
            return None;
        };
        if *class != "backlight" || device.is_empty() {
            return None;
        }
        let percent: u8 = percent.trim().trim_end_matches('%').parse().ok()?;
        Some(Backlight { device: (*device).to_string(), percent: percent.min(100) })
    })
}

#[must_use]
pub fn read_backlight() -> Option<Backlight> {
    run::output("brightnessctl", &["-m", "-c", "backlight", "info"]).as_deref().and_then(parse_backlight)
}

pub(crate) fn set_brightness(device: &str, percent: u8) -> Result<String, String> {
    let percent = format!("{}%", percent.clamp(1, 100));
    run::status("brightnessctl", &["-q", "-d", device, "set", &percent])
}

#[must_use]
pub fn is_internal(name: &str) -> bool {
    ["eDP", "LVDS", "DSI"].iter().any(|prefix| name.starts_with(prefix))
}

pub fn night_light(on: Option<bool>) -> Result<bool, String> {
    match ipc::request(&Request::NightLight { on })? {
        Response::NightLight { on } => Ok(on),
        Response::Error { message } => Err(message),
        _ => Err(String::from("unsupported")),
    }
}

#[must_use]
pub fn mode_key(mode: &ModeInfo) -> String {
    if mode.refresh_mhz > 0 {
        format!("{}x{}@{}", mode.width, mode.height, refresh_text(mode.refresh_mhz))
    } else {
        format!("{}x{}", mode.width, mode.height)
    }
}

#[must_use]
pub(crate) const fn hertz(lang: crate::i18n::Lang) -> &'static str {
    match lang {
        crate::i18n::Lang::Uk => "Гц",
        crate::i18n::Lang::En => "Hz",
    }
}

#[must_use]
pub(crate) fn refresh_text(refresh_mhz: i32) -> String {
    let text = format!("{:.3}", f64::from(refresh_mhz) / 1000.0);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[must_use]
pub fn resolutions(modes: &[ModeInfo]) -> Vec<(i32, i32)> {
    let mut sizes: Vec<(i32, i32)> = modes.iter().map(|mode| (mode.width, mode.height)).collect();
    sizes.sort_by_key(|size| std::cmp::Reverse((size.0 * size.1, size.0)));
    sizes.dedup();
    sizes
}

#[must_use]
pub fn refresh_rates(modes: &[ModeInfo], size: (i32, i32)) -> Vec<i32> {
    let mut rates: Vec<i32> = modes
        .iter()
        .filter(|mode| (mode.width, mode.height) == size && mode.refresh_mhz > 0)
        .map(|mode| mode.refresh_mhz)
        .collect();
    rates.sort_unstable_by(|a, b| b.cmp(a));
    rates.dedup_by(|a, b| (*a - *b).abs() < 10);
    rates
}

#[must_use]
pub fn row_positions(outputs: &[(String, f64)]) -> Vec<(String, i64, i64)> {
    let mut x: f64 = 0.0;
    outputs
        .iter()
        .map(|(name, width)| {
            let position = (name.clone(), x.round() as i64, 0);
            x += width.max(1.0);
            position
        })
        .collect()
}

#[must_use]
pub fn diagonal_inches(physical_mm: (i32, i32)) -> Option<f64> {
    let (w, h) = physical_mm;
    (w > 0 && h > 0).then(|| (f64::from(w).hypot(f64::from(h)) / 25.4 * 10.0).round() / 10.0)
}

pub(crate) const NIGHT_MIN_KELVIN: i64 = 2500;
pub(crate) const NIGHT_MAX_KELVIN: i64 = 6000;
const NEUTRAL_KELVIN: f64 = 6500.0;

fn blackbody(kelvin: f64) -> (f64, f64) {
    let k = kelvin / 100.0;
    let green = 99.470_802_586_1 * k.ln() - 161.119_568_166_1;
    let blue = if k <= 19.0 { 0.0 } else { 138.517_731_223_1 * (k - 10.0).ln() - 305.044_792_730_7 };
    (green, blue)
}

#[must_use]
pub fn night_tint(kelvin: i64) -> (f32, f32, f32) {
    let kelvin = (kelvin as f64).clamp(1900.0, NEUTRAL_KELVIN);
    let (green, blue) = blackbody(kelvin);
    let (neutral_green, neutral_blue) = blackbody(NEUTRAL_KELVIN);
    (1.0, (green / neutral_green).clamp(0.0, 1.0) as f32, (blue / neutral_blue).clamp(0.0, 1.0) as f32)
}

#[must_use]
pub fn schedule_segments(from: u16, to: u16) -> Vec<(u16, u16)> {
    const DAY: u16 = 24 * 60;
    let (from, to) = (from.min(DAY), to.min(DAY));
    match from.cmp(&to) {
        std::cmp::Ordering::Equal => Vec::new(),
        std::cmp::Ordering::Less => vec![(from, to)],
        std::cmp::Ordering::Greater => [(0, to), (from, DAY)].into_iter().filter(|(a, b)| b > a).collect(),
    }
}

#[must_use]
pub fn parse_modes(text: &str) -> Vec<ModeInfo> {
    let mut modes: Vec<ModeInfo> = text
        .lines()
        .filter_map(parse_mode)
        .enumerate()
        .map(|(index, (width, height))| ModeInfo { width, height, refresh_mhz: 0, preferred: index == 0 })
        .collect();
    modes.dedup_by_key(|mode| (mode.width, mode.height));
    modes
}

#[must_use]
pub fn parse_mode(text: &str) -> Option<(i32, i32)> {
    let (width, height) = text.lines().next()?.trim().split_once('x')?;
    let height = height.trim_end_matches(|c: char| !c.is_ascii_digit());
    Some((width.parse().ok()?, height.parse().ok()?))
}

#[must_use]
pub fn connector_name(entry: &str) -> Option<&str> {
    let rest = entry.strip_prefix("card")?;
    let (_, name) = rest.split_once('-')?;
    Some(name)
}

fn from_sysfs() -> Vec<OutputInfo> {
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    let mut outputs: Vec<OutputInfo> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            let name = connector_name(&file)?.to_string();
            let path = entry.path();
            let status = std::fs::read_to_string(path.join("status")).ok()?;
            if status.trim() != "connected" {
                return None;
            }
            let modes_text = std::fs::read_to_string(path.join("modes")).unwrap_or_default();
            let (width, height) = parse_mode(&modes_text).unwrap_or((0, 0));
            Some(OutputInfo {
                name,
                width,
                height,
                scale: 1.0,
                modes: parse_modes(&modes_text),
                ..OutputInfo::default()
            })
        })
        .collect();
    outputs.sort_by(|a, b| a.name.cmp(&b.name));
    outputs
}

#[must_use]
pub fn read() -> DisplayState {
    let backlight = read_backlight();
    let (live, compositor_running, outputs) = match ipc::list_outputs() {
        Ok(outputs) => (true, true, outputs),
        Err(_) => (false, ipc::request(&Request::Ping).is_ok(), from_sysfs()),
    };
    DisplayState { live, compositor_running, outputs, backlight }
}

#[must_use]
pub fn scale_label(scale: f64) -> String {
    format!("{:.0}%", scale * 100.0)
}

#[must_use]
pub fn logical_size(output: &OutputInfo) -> (f64, f64) {
    let scale = if output.scale > 0.0 { output.scale } else { 1.0 };
    (f64::from(output.width) / scale, f64::from(output.height) / scale)
}

const EXACT_ASPECTS: [(i32, i32); 5] = [(16, 9), (4, 3), (5, 4), (21, 9), (3, 2)];
const ULTRAWIDE_ASPECTS: [(i32, i32); 3] = [(64, 27), (43, 18), (12, 5)];

const fn gcd(a: i32, b: i32) -> i32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[must_use]
pub fn aspect_label(width: i32, height: i32) -> Option<String> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let divisor = gcd(width, height);
    let (w, h) = (width / divisor, height / divisor);
    if EXACT_ASPECTS.contains(&(w, h)) {
        Some(format!("{w}:{h}"))
    } else if ULTRAWIDE_ASPECTS.contains(&(w, h)) {
        Some(String::from("21:9"))
    } else if (w, h) == (8, 5) {
        Some(String::from("16:10"))
    } else {
        None
    }
}

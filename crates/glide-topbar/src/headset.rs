#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Music,
    Calls,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Profile {
    pub mode: Option<Mode>,
    pub codec: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Headset {
    pub mac: String,
    pub name: String,
    pub icon: String,
    pub battery: Option<u8>,
    pub address: String,
    pub volume: Option<u8>,
    pub profile: Profile,
}

const CODECS: [&str; 12] =
    ["sbc_xq", "sbc", "aac", "aptx_hd", "aptx_ll", "aptx", "ldac", "opus_05", "opus", "lc3", "msbc", "cvsd"];

#[must_use]
pub fn parse_profile(active: &str) -> Profile {
    let active = active.trim().to_ascii_lowercase();
    let mode = if active.starts_with("a2dp") {
        Some(Mode::Music)
    } else if active.starts_with("headset") || active.starts_with("handsfree") || active.contains("head-unit") {
        Some(Mode::Calls)
    } else {
        None
    };
    let codec = CODECS
        .iter()
        .find(|codec| active.ends_with(&format!("-{codec}")) || active.ends_with(&format!("_{codec}")))
        .map(|codec| codec.replace('_', "-").to_ascii_uppercase())
        .or_else(|| (mode == Some(Mode::Music)).then(|| "SBC".to_string()));
    Profile { mode, codec }
}

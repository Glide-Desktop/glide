#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiNetwork {
    pub ssid: String,
    pub security: Security,
    pub signal: u8,
    pub connected: bool,
    pub saved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Security {
    Open,
    Passphrase,
    Enterprise,
}

impl Security {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let text = text.trim();
        if text.is_empty() || text == "--" {
            Self::Open
        } else if text
            .split_whitespace()
            .any(|part| part.eq_ignore_ascii_case("802.1X") || part.eq_ignore_ascii_case("EAP"))
        {
            Self::Enterprise
        } else {
            Self::Passphrase
        }
    }

    #[must_use]
    pub fn is_secured(self) -> bool {
        self != Self::Open
    }
}

impl WifiNetwork {
    #[must_use]
    pub const fn level(&self) -> u8 {
        match self.signal {
            0 => 0,
            1..=24 => 1,
            25..=49 => 2,
            50..=74 => 3,
            _ => 4,
        }
    }

    #[must_use]
    pub const fn icon(&self) -> &'static str {
        match self.level() {
            0 => "network-wireless-signal-none",
            1 => "network-wireless-signal-weak",
            2 => "network-wireless-signal-ok",
            3 => "network-wireless-signal-good",
            _ => "network-wireless-signal-excellent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtKind {
    Headphones,
    Mouse,
    Keyboard,
    Phone,
    Other,
}

impl BtKind {
    #[must_use]
    pub fn from_icon(icon: &str) -> Self {
        let icon = icon.trim();
        if icon.starts_with("audio-") {
            Self::Headphones
        } else if icon == "input-mouse" || icon == "input-tablet" {
            Self::Mouse
        } else if icon == "input-keyboard" {
            Self::Keyboard
        } else if icon == "phone" {
            Self::Phone
        } else {
            Self::Other
        }
    }

    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Headphones => "audio-headphones",
            Self::Mouse => "input-mouse",
            Self::Keyboard => "input-keyboard",
            Self::Phone => "phone",
            Self::Other => "bluetooth",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtDevice {
    pub mac: String,
    pub name: String,
    pub kind: BtKind,
    pub connected: bool,
    pub battery: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    WrongPassword,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioOutput {
    pub name: String,
    pub description: String,
    pub icon: &'static str,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChargeLimit {
    pub path: String,
    pub enabled: bool,
    pub end: u32,
}

pub const CHARGE_LIMIT_KEY: &str = "charge-limit";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetEvent {
    Wifi(Vec<WifiNetwork>),
    WifiResult { ssid: String, result: Result<(), ConnectError> },
    Bluetooth(Vec<BtDevice>),
    BluetoothResult { mac: String, result: Result<(), ConnectError> },
    Outputs(Vec<AudioOutput>),
    OutputResult { name: String, result: Result<(), ConnectError> },
    Inputs(Vec<AudioOutput>),
    InputResult { name: String, result: Result<(), ConnectError> },
    ChargeLimit(Option<ChargeLimit>),
    ChargeLimitResult(Result<(), ConnectError>),
}

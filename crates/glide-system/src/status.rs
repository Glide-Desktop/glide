use glide_shell::battery::{BatteryInfo, PowerProfile};

#[allow(clippy::struct_excessive_bools)] // Independent toggle states across distinct subsystems
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemStatus {
    pub wifi_enabled: bool,
    pub wifi_level: u8,
    pub wifi_ssid: String,
    pub volume_pct: u8,
    pub volume_muted: bool,
    pub brightness_pct: u8,
    pub volume_available: bool,
    pub brightness_available: bool,
    pub mic_pct: u8,
    pub mic_muted: bool,
    pub mic_available: bool,
    pub keyboard_brightness: Option<u8>,
    pub airplane: bool,
    pub power_profile: Option<PowerProfile>,
    pub battery_pct: Option<u8>,
    pub battery_charging: bool,
    pub bluetooth_on: bool,
    pub bluetooth_connected: bool,
    pub vpn_active: bool,
    pub battery: Option<BatteryInfo>,
    pub privacy: Privacy,
    pub night_light_on: bool,
    pub user_name: String,
    pub reduced_motion: bool,
}

impl Default for SystemStatus {
    fn default() -> Self {
        Self {
            wifi_enabled: false,
            wifi_level: 0,
            wifi_ssid: String::new(),
            volume_pct: 50,
            volume_muted: false,
            brightness_pct: 80,
            volume_available: true,
            brightness_available: true,
            mic_pct: 50,
            mic_muted: false,
            mic_available: false,
            keyboard_brightness: None,
            airplane: false,
            power_profile: None,
            battery_pct: None,
            battery_charging: false,
            bluetooth_on: false,
            bluetooth_connected: false,
            vpn_active: false,
            battery: None,
            privacy: Privacy::default(),
            night_light_on: false,
            user_name: String::new(),
            reduced_motion: false,
        }
    }
}

impl SystemStatus {
    #[must_use]
    pub fn looks_like(&self, other: &Self) -> bool {
        let detail_only = Self {
            battery: other.battery,
            reduced_motion: other.reduced_motion,
            keyboard_brightness: other.keyboard_brightness,
            ..self.clone()
        };
        detail_only == *other
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Privacy {
    pub microphone: Vec<String>,
    pub camera: Vec<String>,
    pub screen: Vec<String>,
}

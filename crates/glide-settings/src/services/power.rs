use std::path::Path;

use super::run;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    PowerSaver,
    Balanced,
    Performance,
}

impl Profile {
    pub const ALL: &'static [Self] = &[Self::PowerSaver, Self::Balanced, Self::Performance];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::PowerSaver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }

    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|profile| profile.key() == key.trim())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
    pub full: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BatteryDetails {
    pub status: String,
    pub energy_now_wh: Option<f64>,
    pub energy_full_wh: Option<f64>,
    pub energy_design_wh: Option<f64>,
    pub power_w: Option<f64>,
    pub cycles: Option<u32>,
    pub manufacturer: String,
    pub model: String,
    pub technology: String,
}

impl BatteryDetails {
    #[must_use]
    pub fn health(&self) -> Option<f64> {
        let (full, design) = (self.energy_full_wh?, self.energy_design_wh?);
        (design > 0.0 && full > 0.0).then(|| (full / design * 100.0).min(100.0))
    }

    #[must_use]
    pub fn hours_left(&self) -> Option<f64> {
        let power = self.power_w.filter(|power| *power > 0.05)?;
        let now = self.energy_now_wh?;
        let hours = match self.status.as_str() {
            "Discharging" => now / power,
            "Charging" => (self.energy_full_wh? - now).max(0.0) / power,
            _ => return None,
        };
        (hours.is_finite() && hours > 0.0 && hours < 100.0).then_some(hours)
    }
}

#[must_use]
pub fn parse_uevent(text: &str) -> std::collections::BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.trim().split_once('=')?;
            let key = key.strip_prefix("POWER_SUPPLY_").unwrap_or(key);
            Some((key.to_ascii_uppercase(), value.trim().to_string()))
        })
        .collect()
}

#[must_use]
pub fn battery_details(fields: &std::collections::BTreeMap<String, String>) -> BatteryDetails {
    let micro =
        |key: &str| fields.get(key).and_then(|value| value.parse::<f64>().ok()).map(|value| value / 1_000_000.0);
    let volts = micro("VOLTAGE_NOW").or_else(|| micro("VOLTAGE_MIN_DESIGN"));
    let energy = |energy_key: &str, charge_key: &str| micro(energy_key).or_else(|| Some(micro(charge_key)? * volts?));
    let power = micro("POWER_NOW").or_else(|| Some(micro("CURRENT_NOW")? * volts?)).map(f64::abs);
    let text = |key: &str| fields.get(key).cloned().unwrap_or_default();
    BatteryDetails {
        status: text("STATUS"),
        energy_now_wh: energy("ENERGY_NOW", "CHARGE_NOW"),
        energy_full_wh: energy("ENERGY_FULL", "CHARGE_FULL"),
        energy_design_wh: energy("ENERGY_FULL_DESIGN", "CHARGE_FULL_DESIGN"),
        power_w: power,
        cycles: fields.get("CYCLE_COUNT").and_then(|value| value.parse().ok()).filter(|cycles| *cycles > 0),
        manufacturer: text("MANUFACTURER"),
        model: text("MODEL_NAME"),
        technology: text("TECHNOLOGY"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LidAction {
    Ignore,
    Suspend,
    Hibernate,
    HybridSleep,
    SuspendThenHibernate,
    Lock,
    PowerOff,
    Other,
}

impl LidAction {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "ignore" => Self::Ignore,
            "suspend" => Self::Suspend,
            "hibernate" => Self::Hibernate,
            "hybrid-sleep" => Self::HybridSleep,
            "suspend-then-hibernate" => Self::SuspendThenHibernate,
            "lock" => Self::Lock,
            "poweroff" => Self::PowerOff,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogindPolicy {
    pub lid: LidAction,
    pub lid_on_ac: LidAction,
    pub lid_docked: LidAction,
    pub power_key: LidAction,
}

impl Default for LogindPolicy {
    fn default() -> Self {
        Self {
            lid: LidAction::Suspend,
            lid_on_ac: LidAction::Suspend,
            lid_docked: LidAction::Ignore,
            power_key: LidAction::PowerOff,
        }
    }
}

#[must_use]
pub fn parse_logind<'a>(files: impl IntoIterator<Item = &'a str>) -> LogindPolicy {
    let mut policy = LogindPolicy::default();
    let mut on_ac: Option<LidAction> = None;
    for text in files {
        let mut in_login = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_login = line == "[Login]";
                continue;
            }
            if !in_login || line.starts_with(['#', ';']) {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let action = LidAction::parse(value);
            match key.trim() {
                "HandleLidSwitch" => policy.lid = action,
                "HandleLidSwitchExternalPower" => on_ac = Some(action),
                "HandleLidSwitchDocked" => policy.lid_docked = action,
                "HandlePowerKey" => policy.power_key = action,
                _ => {}
            }
        }
    }
    policy.lid_on_ac = on_ac.unwrap_or(policy.lid);
    policy
}

fn read_logind() -> Option<LogindPolicy> {
    if !Path::new("/run/systemd/system").exists() {
        return None;
    }
    let mut texts = vec![read(Path::new("/etc/systemd/logind.conf"))];
    for dir in ["/usr/lib/systemd/logind.conf.d", "/etc/systemd/logind.conf.d"] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "conf"))
            .collect();
        paths.sort();
        texts.extend(paths.iter().map(|path| read(path)));
    }
    Some(parse_logind(texts.iter().map(String::as_str)))
}

fn has_lid() -> bool {
    std::fs::read_dir("/proc/acpi/button/lid").is_ok_and(|mut entries| entries.next().is_some())
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PowerState {
    pub battery: Option<Battery>,
    pub details: Option<BatteryDetails>,
    pub on_ac: bool,
    pub profile: Option<Profile>,
    pub profiles_available: bool,
    pub logind: Option<LogindPolicy>,
    pub has_lid: bool,
}

#[must_use]
pub fn parse_battery(capacity: &str, status: &str) -> Option<Battery> {
    let percent: u8 = capacity.trim().parse().ok()?;
    let status = status.trim();
    Some(Battery { percent: percent.min(100), charging: status == "Charging", full: status == "Full" })
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[must_use]
pub fn is_system_battery(kind: &str, scope: &str) -> bool {
    kind.trim() == "Battery" && !scope.trim().eq_ignore_ascii_case("device")
}

pub fn order_batteries(names: &mut [String]) {
    names.sort_by(|a, b| (!a.starts_with("BAT")).cmp(&!b.starts_with("BAT")).then_with(|| a.cmp(b)));
}

#[must_use]
pub fn read_state() -> PowerState {
    let mut state = PowerState::default();
    let root = Path::new("/sys/class/power_supply");
    let mut batteries = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let kind = read(&path.join("type"));
            if is_system_battery(&kind, &read(&path.join("scope"))) {
                batteries.push(entry.file_name().to_string_lossy().into_owned());
            } else if kind.trim() == "Mains" {
                state.on_ac |= read(&path.join("online")).trim() == "1";
            }
        }
    }
    order_batteries(&mut batteries);
    let found = batteries.iter().find_map(|name| {
        let path = root.join(name);
        let battery = parse_battery(&read(&path.join("capacity")), &read(&path.join("status")))?;
        Some((battery, battery_details(&parse_uevent(&read(&path.join("uevent"))))))
    });
    (state.battery, state.details) = found.unzip();
    state.logind = read_logind();
    state.has_lid = has_lid();
    if let Some(text) = run::output("powerprofilesctl", &["get"]) {
        state.profiles_available = true;
        state.profile = Profile::from_key(&text);
    }
    state
}

pub fn set_profile(profile: Profile) -> Result<String, String> {
    run::status("powerprofilesctl", &["set", profile.key()])
}

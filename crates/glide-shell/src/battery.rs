use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BatteryState {
    Charging,
    Discharging,
    Full,
    NotCharging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PowerProfile {
    PowerSaver,
    Balanced,
    Performance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BatteryHealth {
    pub wear_pct: Option<u8>,
    pub cycles: Option<u32>,
    pub draw_dw: Option<u32>,
    pub full_dwh: Option<u32>,
    pub design_dwh: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BatteryInfo {
    pub pct: u8,
    pub state: BatteryState,
    pub minutes: Option<u32>,
    pub health: BatteryHealth,
    pub profile: Option<PowerProfile>,
}

fn uevent_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then(|| value.trim())
    })
}

fn uevent_number(text: &str, key: &str) -> Option<f64> {
    uevent_value(text, key)?.parse::<f64>().ok().filter(|value| *value >= 0.0)
}

fn uevent_rate(text: &str, key: &str) -> Option<f64> {
    uevent_value(text, key)?.parse::<f64>().ok().filter(|value| value.is_finite()).map(f64::abs)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BatteryCell {
    pub pct: u8,
    pub state: BatteryState,
    pub energy_units: bool,
    pub now: f64,
    pub full: f64,
    pub rate: f64,
    pub design: f64,
    pub cycles: Option<u32>,
    pub watts: Option<f64>,
    pub watt_hours: Option<(f64, f64)>,
}

const MICRO: f64 = 1e6;
const PICO: f64 = 1e12;

fn deci(value: f64) -> Option<u32> {
    value.is_finite().then(|| (value * 10.0).round().clamp(0.0, f64::from(u32::MAX)) as u32)
}

#[must_use]
pub fn battery_health(cells: &[BatteryCell]) -> BatteryHealth {
    let watts: Option<f64> = cells.iter().map(|cell| cell.watts).sum();
    let watt_hours: Option<(f64, f64)> = cells
        .iter()
        .map(|cell| cell.watt_hours)
        .try_fold((0.0, 0.0), |(full, design), cell| cell.map(|(f, d)| (full + f, design + d)));
    let same_units = cells.windows(2).all(|pair| pair[0].energy_units == pair[1].energy_units);
    let (full, design) = watt_hours.unwrap_or_else(|| {
        if same_units {
            (cells.iter().map(|cell| cell.full).sum(), cells.iter().map(|cell| cell.design).sum())
        } else {
            (0.0, 0.0)
        }
    });
    let wear_pct = (design > 0.0 && full > 0.0).then(|| (full / design * 100.0).round().clamp(0.0, 100.0) as u8);
    BatteryHealth {
        wear_pct,
        cycles: cells.iter().filter_map(|cell| cell.cycles).max(),
        draw_dw: watts.and_then(deci),
        full_dwh: watt_hours.and_then(|(full, _)| deci(full)),
        design_dwh: watt_hours.and_then(|(_, design)| deci(design)).filter(|design| *design > 0),
    }
}

#[must_use]
pub fn parse_battery_cell(text: &str) -> Option<BatteryCell> {
    if uevent_value(text, "POWER_SUPPLY_SCOPE").is_some_and(|scope| scope == "Device") {
        return None;
    }
    let pct = uevent_value(text, "POWER_SUPPLY_CAPACITY")?.parse::<u8>().ok()?.min(100);
    let state = match uevent_value(text, "POWER_SUPPLY_STATUS").unwrap_or_default() {
        "Charging" => BatteryState::Charging,
        "Full" => BatteryState::Full,
        "Not charging" => BatteryState::NotCharging,
        _ => BatteryState::Discharging,
    };
    let (energy_units, now, full, rate) =
        match (uevent_number(text, "POWER_SUPPLY_ENERGY_NOW"), uevent_number(text, "POWER_SUPPLY_ENERGY_FULL")) {
            (Some(now), Some(full)) => (true, now, full, uevent_rate(text, "POWER_SUPPLY_POWER_NOW").unwrap_or(0.0)),
            _ => (
                false,
                uevent_number(text, "POWER_SUPPLY_CHARGE_NOW").unwrap_or(0.0),
                uevent_number(text, "POWER_SUPPLY_CHARGE_FULL").unwrap_or(0.0),
                uevent_rate(text, "POWER_SUPPLY_CURRENT_NOW").unwrap_or(0.0),
            ),
        };
    let design_key = if energy_units { "POWER_SUPPLY_ENERGY_FULL_DESIGN" } else { "POWER_SUPPLY_CHARGE_FULL_DESIGN" };
    let design = uevent_number(text, design_key).unwrap_or(0.0);
    let cycles = uevent_value(text, "POWER_SUPPLY_CYCLE_COUNT").and_then(|n| n.parse::<u32>().ok()).filter(|n| *n > 0);
    let volts_now = uevent_number(text, "POWER_SUPPLY_VOLTAGE_NOW").filter(|volts| *volts > 0.0);
    let volts_design =
        uevent_number(text, "POWER_SUPPLY_VOLTAGE_MIN_DESIGN").filter(|volts| *volts > 0.0).or(volts_now);
    let (watts, watt_hours) = if energy_units {
        (Some(rate / MICRO), Some((full / MICRO, design / MICRO)))
    } else {
        (
            volts_now.map(|volts| rate * volts / PICO),
            volts_design.map(|volts| (full * volts / PICO, design * volts / PICO)),
        )
    };
    Some(BatteryCell { pct, state, energy_units, now, full, rate, design, cycles, watts, watt_hours })
}

#[must_use]
pub fn aggregate_batteries(cells: &[BatteryCell]) -> Option<BatteryInfo> {
    let first = cells.first()?;
    let any = |state: BatteryState| cells.iter().any(|cell| cell.state == state);
    let state = if any(BatteryState::Charging) {
        BatteryState::Charging
    } else if any(BatteryState::Discharging) {
        BatteryState::Discharging
    } else if any(BatteryState::NotCharging) {
        BatteryState::NotCharging
    } else {
        BatteryState::Full
    };
    let health = battery_health(cells);
    if cells.len() == 1 {
        return Some(BatteryInfo {
            pct: first.pct,
            state,
            minutes: battery_minutes(state, first.now, first.full, first.rate),
            health,
            profile: None,
        });
    }
    let same_units = cells.iter().all(|cell| cell.energy_units == first.energy_units);
    let now: f64 = cells.iter().map(|cell| cell.now).sum();
    let full: f64 = cells.iter().map(|cell| cell.full).sum();
    let rate: f64 = cells.iter().map(|cell| cell.rate).sum();
    let pct = if same_units && full > 0.0 {
        (now / full * 100.0).round().clamp(0.0, 100.0) as u8
    } else {
        let total: u32 = cells.iter().map(|cell| u32::from(cell.pct)).sum();
        let count = u32::try_from(cells.len()).unwrap_or(u32::MAX);
        u8::try_from(total / count).unwrap_or(100)
    };
    Some(BatteryInfo {
        pct,
        state,
        minutes: same_units.then(|| battery_minutes(state, now, full, rate)).flatten(),
        health,
        profile: None,
    })
}

#[must_use]
pub fn battery_minutes(state: BatteryState, now: f64, full: f64, rate: f64) -> Option<u32> {
    if rate <= 0.0 {
        return None;
    }
    let hours = match state {
        BatteryState::Charging if full > now => (full - now) / rate,
        BatteryState::Discharging if now > 0.0 => now / rate,
        _ => return None,
    };
    let minutes = (hours * 60.0).round();
    let minutes = minutes.clamp(0.0, 5999.0) as u32;
    (minutes > 0).then_some(minutes)
}

impl PowerProfile {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::PowerSaver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Balanced => Self::PowerSaver,
            Self::PowerSaver => Self::Performance,
            Self::Performance => Self::Balanced,
        }
    }
}

#[must_use]
pub fn parse_power_profile(text: &str) -> Option<PowerProfile> {
    match text.trim() {
        "power-saver" => Some(PowerProfile::PowerSaver),
        "balanced" => Some(PowerProfile::Balanced),
        "performance" => Some(PowerProfile::Performance),
        _ => None,
    }
}

#[must_use]
pub fn read_battery() -> Option<BatteryInfo> {
    let entries = fs::read_dir(Path::new("/sys/class/power_supply")).ok()?;
    let mut cells = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_bat = entry.file_name().to_string_lossy().starts_with("BAT")
            || fs::read_to_string(path.join("type")).is_ok_and(|t| t.trim().eq_ignore_ascii_case("battery"));
        if !is_bat {
            continue;
        }
        if let Some(cell) = fs::read_to_string(path.join("uevent")).ok().as_deref().and_then(parse_battery_cell) {
            cells.push(cell);
        }
    }
    aggregate_batteries(&cells)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_battery_uevent(text: &str) -> Option<BatteryInfo> {
        parse_battery_cell(text).and_then(|cell| aggregate_batteries(&[cell]))
    }

    #[test]
    fn battery_health_reads_wear_cycles_draw_and_capacity_in_either_unit() {
        let charge = "POWER_SUPPLY_STATUS=Discharging\nPOWER_SUPPLY_CYCLE_COUNT=65\n\
                      POWER_SUPPLY_VOLTAGE_MIN_DESIGN=11370000\nPOWER_SUPPLY_VOLTAGE_NOW=12000000\n\
                      POWER_SUPPLY_CURRENT_NOW=700000\nPOWER_SUPPLY_CHARGE_FULL_DESIGN=4661000\n\
                      POWER_SUPPLY_CHARGE_FULL=4615000\nPOWER_SUPPLY_CHARGE_NOW=2300000\nPOWER_SUPPLY_CAPACITY=50\n";
        let health = parse_battery_uevent(charge).expect("battery").health;
        assert_eq!(health.wear_pct, Some(99));
        assert_eq!(health.cycles, Some(65));
        assert_eq!(health.draw_dw, Some(84), "0.7 A at 12 V");
        assert_eq!(health.full_dwh, Some(525), "4.615 Ah at the 11.37 V design voltage");
        assert_eq!(health.design_dwh, Some(530));

        let energy = "POWER_SUPPLY_STATUS=Discharging\nPOWER_SUPPLY_CYCLE_COUNT=0\n\
                      POWER_SUPPLY_ENERGY_FULL_DESIGN=60000000\nPOWER_SUPPLY_ENERGY_FULL=48000000\n\
                      POWER_SUPPLY_ENERGY_NOW=24000000\nPOWER_SUPPLY_POWER_NOW=9500000\nPOWER_SUPPLY_CAPACITY=50\n";
        let health = parse_battery_uevent(energy).expect("battery").health;
        assert_eq!(health.wear_pct, Some(80));
        assert_eq!(health.cycles, None, "a zero count means the firmware does not keep one");
        assert_eq!(health.draw_dw, Some(95));
        assert_eq!((health.full_dwh, health.design_dwh), (Some(480), Some(600)));

        let bare = "POWER_SUPPLY_STATUS=Full\nPOWER_SUPPLY_CAPACITY=100\n";
        assert_eq!(
            parse_battery_uevent(bare).expect("battery").health,
            BatteryHealth { draw_dw: None, full_dwh: None, ..BatteryHealth::default() }
        );
    }

    #[test]
    fn power_profiles_parse_from_powerprofilesctl() {
        for (text, profile) in [
            ("balanced\n", Some(PowerProfile::Balanced)),
            ("power-saver", Some(PowerProfile::PowerSaver)),
            ("performance", Some(PowerProfile::Performance)),
            ("", None),
        ] {
            assert_eq!(parse_power_profile(text), profile);
        }
    }

    const DISCHARGING: &str = "POWER_SUPPLY_NAME=BAT0\nPOWER_SUPPLY_TYPE=Battery\n\
        POWER_SUPPLY_STATUS=Discharging\nPOWER_SUPPLY_PRESENT=1\n\
        POWER_SUPPLY_POWER_NOW=10000000\nPOWER_SUPPLY_ENERGY_FULL=50000000\n\
        POWER_SUPPLY_ENERGY_NOW=25000000\nPOWER_SUPPLY_CAPACITY=50\n";

    #[test]
    fn a_discharging_battery_reports_time_until_empty() {
        let info = parse_battery_uevent(DISCHARGING).unwrap();
        assert_eq!(info.pct, 50);
        assert_eq!(info.state, BatteryState::Discharging);
        assert_eq!(info.minutes, Some(150));
    }

    #[test]
    fn a_charging_battery_reports_time_until_full_from_charge_counters() {
        let text = "POWER_SUPPLY_STATUS=Charging\nPOWER_SUPPLY_CAPACITY=80\n\
            POWER_SUPPLY_CHARGE_NOW=4000000\nPOWER_SUPPLY_CHARGE_FULL=5000000\n\
            POWER_SUPPLY_CURRENT_NOW=2000000\n";
        let info = parse_battery_uevent(text).unwrap();
        assert_eq!(info.state, BatteryState::Charging);
        assert_eq!(info.minutes, Some(30));
    }

    #[test]
    fn a_full_or_idle_battery_has_no_estimate() {
        let full = DISCHARGING.replace("Discharging", "Full");
        assert_eq!(parse_battery_uevent(&full).unwrap().state, BatteryState::Full);
        assert_eq!(parse_battery_uevent(&full).unwrap().minutes, None);
        let idle = DISCHARGING.replace("POWER_NOW=10000000", "POWER_NOW=0");
        assert_eq!(parse_battery_uevent(&idle).unwrap().minutes, None);
        let stalled = DISCHARGING.replace("Discharging", "Not charging");
        assert_eq!(parse_battery_uevent(&stalled).unwrap().state, BatteryState::NotCharging);
    }

    const FULL_ON_AC: &str = "POWER_SUPPLY_NAME=BAT0\nPOWER_SUPPLY_TYPE=Battery\n\
        POWER_SUPPLY_STATUS=Full\nPOWER_SUPPLY_CAPACITY=100\n\
        POWER_SUPPLY_ENERGY_FULL_DESIGN=60000000\nPOWER_SUPPLY_ENERGY_FULL=48000000\n\
        POWER_SUPPLY_ENERGY_NOW=48000000\n";

    #[test]
    fn a_full_battery_that_hides_its_power_draw_is_still_measured_in_energy() {
        let cell = parse_battery_cell(FULL_ON_AC).unwrap();
        assert!(cell.energy_units, "ENERGY_FULL and ENERGY_NOW alone decide the units");
        assert_eq!(cell.rate, 0.0);
        let health = aggregate_batteries(&[cell]).unwrap().health;
        assert_eq!(health.wear_pct, Some(80));
        assert_eq!(health.draw_dw, Some(0));
        assert_eq!((health.full_dwh, health.design_dwh), (Some(480), Some(600)));

        let discharging = parse_battery_cell(DISCHARGING).unwrap();
        let info = aggregate_batteries(&[cell, discharging]).unwrap();
        assert_eq!(info.minutes, Some(438), "both cells are in energy units, so the estimate survives");
        assert_eq!(info.pct, 74);
    }

    #[test]
    fn drivers_that_sign_the_rate_while_discharging_still_give_an_estimate() {
        let signed = DISCHARGING.replace("POWER_NOW=10000000", "POWER_NOW=-10000000");
        let info = parse_battery_uevent(&signed).unwrap();
        assert_eq!(info.minutes, Some(150));
        assert_eq!(info.health.draw_dw, Some(100), "a signed power draw is still 10 W");

        let charge = "POWER_SUPPLY_STATUS=Discharging\nPOWER_SUPPLY_CAPACITY=50\n\
            POWER_SUPPLY_CHARGE_NOW=2500000\nPOWER_SUPPLY_CHARGE_FULL=5000000\n\
            POWER_SUPPLY_CURRENT_NOW=-1000000\n";
        let cell = parse_battery_cell(charge).unwrap();
        assert!(!cell.energy_units);
        assert_eq!(cell.rate, 1_000_000.0);
        assert_eq!(aggregate_batteries(&[cell]).unwrap().minutes, Some(150));

        let negative_capacity = DISCHARGING.replace("ENERGY_NOW=25000000", "ENERGY_NOW=-25000000");
        let cell = parse_battery_cell(&negative_capacity).unwrap();
        assert!(!cell.energy_units, "capacity-like fields stay non-negative");
        assert_eq!(cell.now, 0.0);
    }

    #[test]
    fn two_batteries_combine_into_one_level_and_estimate() {
        let bat1 = "POWER_SUPPLY_STATUS=Not charging\nPOWER_SUPPLY_CAPACITY=50\n\
            POWER_SUPPLY_ENERGY_NOW=10000000\nPOWER_SUPPLY_ENERGY_FULL=20000000\n\
            POWER_SUPPLY_POWER_NOW=0\n";
        let cells = [parse_battery_cell(DISCHARGING).unwrap(), parse_battery_cell(bat1).unwrap()];
        let info = aggregate_batteries(&cells).unwrap();
        assert_eq!(info.pct, 50);
        assert_eq!(info.state, BatteryState::Discharging);
        assert_eq!(info.minutes, Some(210));

        let full = DISCHARGING
            .replace("Discharging", "Full")
            .replace("ENERGY_NOW=25000000", "ENERGY_NOW=50000000")
            .replace("POWER_NOW=10000000", "POWER_NOW=0");
        let charging = "POWER_SUPPLY_STATUS=Charging\nPOWER_SUPPLY_CAPACITY=25\n\
            POWER_SUPPLY_ENERGY_NOW=5000000\nPOWER_SUPPLY_ENERGY_FULL=20000000\n\
            POWER_SUPPLY_POWER_NOW=5000000\n";
        let cells = [parse_battery_cell(&full).unwrap(), parse_battery_cell(charging).unwrap()];
        let info = aggregate_batteries(&cells).unwrap();
        assert_eq!(info.pct, 79);
        assert_eq!(info.state, BatteryState::Charging);
        assert_eq!(info.minutes, Some(180));

        assert_eq!(aggregate_batteries(&[]), None);
    }

    #[test]
    fn batteries_reporting_different_units_average_their_capacity() {
        let charge = "POWER_SUPPLY_STATUS=Discharging\nPOWER_SUPPLY_CAPACITY=90\n\
            POWER_SUPPLY_CHARGE_NOW=4500000\nPOWER_SUPPLY_CHARGE_FULL=5000000\n\
            POWER_SUPPLY_CURRENT_NOW=1000000\n";
        let cells = [parse_battery_cell(DISCHARGING).unwrap(), parse_battery_cell(charge).unwrap()];
        let info = aggregate_batteries(&cells).unwrap();
        assert_eq!(info.pct, 70);
        assert_eq!(info.minutes, None);
    }

    #[test]
    fn peripheral_batteries_and_missing_capacity_are_ignored() {
        let mouse = format!("{DISCHARGING}POWER_SUPPLY_SCOPE=Device\n");
        assert_eq!(parse_battery_uevent(&mouse), None);
        assert_eq!(parse_battery_uevent("POWER_SUPPLY_STATUS=Charging\n"), None);
    }
}

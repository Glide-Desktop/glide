use zbus::blocking::{Connection, Proxy};
use zbus::proxy::MethodFlags;

use super::run;
use super::tz;

const DEST: &str = "org.freedesktop.timedate1";
const PATH: &str = "/org/freedesktop/timedate1";

#[allow(clippy::struct_excessive_bools)] // Mirrors timedated's independent boolean properties
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TimeState {
    pub timezone: String,
    pub local_rtc: bool,
    pub can_ntp: bool,
    pub ntp: bool,
    pub synchronized: bool,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneEntry {
    pub name: String,
    pub city: String,
    pub region: String,
    pub offset: i32,
    pub abbr: String,
}

fn yes(value: &str) -> bool {
    matches!(value.trim(), "yes" | "true" | "1")
}

#[must_use]
pub fn parse_show(text: &str) -> TimeState {
    let mut state = TimeState::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        state.available = true;
        match key.trim() {
            "Timezone" => state.timezone = value.trim().to_string(),
            "LocalRTC" => state.local_rtc = yes(value),
            "CanNTP" => state.can_ntp = yes(value),
            "NTP" => state.ntp = yes(value),
            "NTPSynchronized" => state.synchronized = yes(value),
            _ => {}
        }
    }
    state
}

#[must_use]
pub fn zone_from_link(target: &str) -> Option<String> {
    let (_, zone) = target.split_once("zoneinfo/")?;
    let zone = zone.strip_prefix("posix/").unwrap_or(zone);
    (!zone.is_empty()).then(|| zone.to_string())
}

#[must_use]
pub fn read() -> TimeState {
    if let Some(text) = run::output("timedatectl", &["show"]) {
        let state = parse_show(&text);
        if state.available {
            return state;
        }
    }
    TimeState {
        timezone: std::fs::read_link("/etc/localtime")
            .ok()
            .and_then(|target| zone_from_link(&target.to_string_lossy()))
            .unwrap_or_default(),
        ..TimeState::default()
    }
}

#[must_use]
pub fn zone_names() -> Vec<String> {
    if let Some(text) = run::output("timedatectl", &["list-timezones", "--no-pager"]) {
        let zones: Vec<String> =
            text.lines().map(str::trim).filter(|zone| !zone.is_empty()).map(str::to_string).collect();
        if !zones.is_empty() {
            return zones;
        }
    }
    let tab = tz::zoneinfo_dir().join("zone1970.tab");
    let mut zones = parse_tab_file(&tab);
    if !zones.iter().any(|zone| zone == "UTC") {
        zones.push(String::from("UTC"));
    }
    zones
}

fn parse_tab_file(path: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(path).map(|text| tz::parse_zone_tab(&text)).unwrap_or_default()
}

#[must_use]
pub fn zones() -> Vec<ZoneEntry> {
    let now = tz::now();
    zone_names()
        .into_iter()
        .map(|name| {
            let local = tz::Zone::load(&name).map(|zone| zone.at(now));
            let (offset, abbr) = local.map_or((0, String::new()), |local| (local.offset, local.abbr));
            ZoneEntry { city: tz::city_name(&name), region: tz::region_name(&name), name, offset, abbr }
        })
        .collect()
}

#[must_use]
pub fn matches(entry: &ZoneEntry, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let squashed = query.replace(' ', "_");
    entry.name.to_lowercase().contains(&squashed)
        || entry.city.to_lowercase().contains(&query)
        || entry.abbr.to_lowercase() == query
        || tz::format_offset(entry.offset).to_lowercase().contains(&query)
}

pub fn system_proxy(dest: &'static str, path: &'static str) -> Result<Proxy<'static>, String> {
    Connection::system().and_then(|bus| Proxy::new(&bus, dest, path, dest)).map_err(|err| err.to_string())
}

pub fn authorized_call<B>(proxy: &Proxy<'_>, method: &str, body: &B) -> Result<(), String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    proxy
        .call_with_flags::<_, _, ()>(method, MethodFlags::AllowInteractiveAuth.into(), body)
        .map(|_| ())
        .map_err(|err| describe(&err))
}

fn call<B>(method: &str, body: &B) -> Result<(), String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    authorized_call(&system_proxy(DEST, PATH)?, method, body)
}

#[must_use]
pub fn describe(err: &zbus::Error) -> String {
    match err {
        zbus::Error::MethodError(name, message, _) => {
            let text = message.clone().unwrap_or_default();
            if name.as_str().ends_with("InteractiveAuthorizationRequired")
                || name.as_str().ends_with("AccessDenied")
                || name.as_str().ends_with("NotAuthorized")
            {
                format!("{text} ({name})").trim().to_string()
            } else if text.is_empty() {
                name.to_string()
            } else {
                text
            }
        }
        other => other.to_string(),
    }
}

pub fn set_timezone(zone: &str) -> Result<(), String> {
    call("SetTimezone", &(zone, true))
}

pub fn set_ntp(enabled: bool) -> Result<(), String> {
    call("SetNTP", &(enabled, true))
}

pub fn set_time(utc_seconds: i64) -> Result<(), String> {
    call("SetTime", &(utc_seconds.saturating_mul(1_000_000), false, true))
}

#[must_use]
pub fn parse_date(text: &str) -> Option<(i64, u8, u8)> {
    let mut parts = text.trim().split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day: u8 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1970..=2200).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    (1..=tz::days_in_month(year, month)).contains(&day).then_some((year, month, day))
}

#[must_use]
pub fn parse_time(text: &str) -> Option<(u8, u8, u8)> {
    let mut parts = text.trim().split(':');
    let hour: u8 = parts.next()?.parse().ok()?;
    let minute_text = parts.next()?;
    let second_text = parts.next();
    if parts.next().is_some() || minute_text.len() != 2 {
        return None;
    }
    let minute: u8 = minute_text.parse().ok()?;
    let second: u8 = match second_text {
        Some(text) if text.len() == 2 => text.parse().ok()?,
        Some(_) => return None,
        None => 0,
    };
    (hour < 24 && minute < 60 && second < 60).then_some((hour, minute, second))
}

#[must_use]
pub fn uses_12_hour(lc_all: Option<&str>, lc_time: Option<&str>, lang: Option<&str>) -> bool {
    [lc_all, lc_time, lang]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .is_some_and(|locale| locale.starts_with("en_US"))
}

#[must_use]
pub fn env_uses_12_hour() -> bool {
    let var = |name: &str| std::env::var(name).ok();
    uses_12_hour(var("LC_ALL").as_deref(), var("LC_TIME").as_deref(), var("LANG").as_deref())
}

use std::path::Path;

use super::run;
use super::timedate::{authorized_call, system_proxy};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemInfo {
    pub hostname: String,
    pub user_name: String,
    pub os_name: String,
    pub kernel: String,
    pub cpu: String,
    pub cores: usize,
    pub memory_bytes: u64,
    pub gpus: Vec<String>,
    pub disk_total: u64,
    pub disk_free: u64,
    pub uptime_secs: u64,
    pub session: String,
}

fn read(path: impl AsRef<Path>) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[must_use]
pub fn parse_os_release(text: &str) -> Option<String> {
    let value = |key: &str| {
        text.lines().find_map(|line| {
            line.strip_prefix(key)
                .and_then(|rest| rest.strip_prefix('='))
                .map(|value| value.trim().trim_matches('"').to_string())
        })
    };
    value("PRETTY_NAME").or_else(|| value("NAME")).filter(|name| !name.is_empty())
}

#[must_use]
pub fn parse_cpuinfo(text: &str) -> (String, usize) {
    let model = text
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            matches!(key.trim(), "model name" | "Model" | "Hardware").then(|| value.trim().to_string())
        })
        .unwrap_or_default();
    let cores = text.lines().filter(|line| line.split(':').next().is_some_and(|key| key.trim() == "processor")).count();
    (model, cores)
}

fn meminfo_bytes(text: &str, key: &str) -> u64 {
    text.lines()
        .find_map(|line| {
            let rest = line.strip_prefix(key)?;
            let kib: u64 = rest.split_whitespace().next()?.parse().ok()?;
            Some(kib * 1024)
        })
        .unwrap_or(0)
}

#[must_use]
pub fn parse_meminfo(text: &str) -> u64 {
    meminfo_bytes(text, "MemTotal:")
}

#[must_use]
pub fn parse_df(text: &str) -> (u64, u64) {
    text.lines()
        .nth(1)
        .and_then(|line| {
            let mut fields = line.split_whitespace();
            let total = fields.next()?.parse().ok()?;
            let free = fields.next()?.parse().ok()?;
            Some((total, free))
        })
        .unwrap_or((0, 0))
}

#[must_use]
pub fn parse_passwd_name(passwd: &str, user: &str) -> Option<String> {
    passwd.lines().find_map(|line| {
        let fields: Vec<&str> = line.split(':').collect();
        (fields.first() == Some(&user))
            .then(|| fields.get(4)?.split(',').next())
            .flatten()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
    })
}

#[must_use]
pub fn vendor_name(id: &str) -> &'static str {
    match id.trim().trim_start_matches("0x") {
        "1002" => "AMD",
        "10de" => "NVIDIA",
        "8086" => "Intel",
        "1af4" => "Virtio",
        "15ad" => "VMware",
        _ => "",
    }
}

fn gpus() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    let mut cards: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("card") && !name.contains('-'))
        .collect();
    cards.sort();
    cards
        .into_iter()
        .filter_map(|card| {
            let device = Path::new("/sys/class/drm").join(&card).join("device");
            let uevent = read(device.join("uevent"));
            let slot = uevent.lines().find_map(|line| line.strip_prefix("PCI_SLOT_NAME=")).map(str::to_string);
            let driver = uevent.lines().find_map(|line| line.strip_prefix("DRIVER=")).unwrap_or_default().to_string();
            let named = slot.and_then(|slot| {
                run::output("lspci", &["-mm", "-s", &slot]).and_then(|text| {
                    let fields: Vec<&str> = text.split('"').collect();
                    let vendor = fields.get(3)?;
                    let product = fields.get(5)?;
                    Some(format!("{vendor} {product}"))
                })
            });
            named.or_else(|| {
                let vendor = vendor_name(&read(device.join("vendor")));
                let label = format!("{vendor} {driver}").trim().to_string();
                (!label.is_empty()).then_some(label)
            })
        })
        .collect()
}

#[must_use]
pub fn user_name() -> String {
    let user = std::env::var("USER").unwrap_or_default();
    parse_passwd_name(&read("/etc/passwd"), &user).unwrap_or(user)
}

#[must_use]
pub fn hostname() -> String {
    let name = read("/proc/sys/kernel/hostname");
    let name = name.trim();
    if name.is_empty() { read("/etc/hostname").trim().to_string() } else { name.to_string() }
}

#[must_use]
pub fn collect() -> SystemInfo {
    let (cpu, cores) = parse_cpuinfo(&read("/proc/cpuinfo"));
    let (disk_total, disk_free) =
        run::output("df", &["-B1", "--output=size,avail", "/"]).map_or((0, 0), |text| parse_df(&text));
    let uptime_secs = read("/proc/uptime")
        .split_whitespace()
        .next()
        .and_then(|secs| secs.parse::<f64>().ok())
        .map_or(0, |secs| secs as u64);
    let session = match std::env::var("XDG_SESSION_TYPE").ok().as_deref() {
        Some("x11") => String::from("X11"),
        _ => String::from("Wayland"),
    };
    SystemInfo {
        hostname: hostname(),
        user_name: user_name(),
        os_name: parse_os_release(&read("/etc/os-release")).unwrap_or_else(|| String::from("Linux")),
        kernel: read("/proc/sys/kernel/osrelease").trim().to_string(),
        cpu,
        cores,
        memory_bytes: parse_meminfo(&read("/proc/meminfo")),
        gpus: gpus(),
        disk_total,
        disk_free,
        uptime_secs,
        session,
    }
}

#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 || size >= 100.0 { format!("{size:.0} {}", UNITS[unit]) } else { format!("{size:.1} {}", UNITS[unit]) }
}

#[must_use]
pub fn initials(name: &str) -> String {
    let letters: String =
        name.split_whitespace().filter_map(|word| word.chars().next()).take(2).flat_map(char::to_uppercase).collect();
    if letters.is_empty() { String::from("?") } else { letters }
}

#[must_use]
pub fn os_release_value(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|line| {
            line.strip_prefix(key)
                .and_then(|rest| rest.strip_prefix('='))
                .map(|value| value.trim().trim_matches('"').trim_matches('\'').to_string())
        })
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Firmware {
    pub vendor: String,
    pub product: String,
    pub version: String,
    pub board: String,
    pub bios_vendor: String,
    pub bios_version: String,
    pub bios_date: String,
}

const DMI_PLACEHOLDERS: &[&str] = &[
    "to be filled by o.e.m.",
    "default string",
    "system product name",
    "system manufacturer",
    "not applicable",
    "none",
    "o.e.m.",
];

#[must_use]
pub fn clean_dmi(value: &str) -> String {
    let value = value.trim();
    if DMI_PLACEHOLDERS.contains(&value.to_lowercase().as_str()) { String::new() } else { value.to_string() }
}

#[must_use]
pub fn read_firmware() -> Firmware {
    let dmi = |name: &str| clean_dmi(&read(Path::new("/sys/class/dmi/id").join(name)));
    Firmware {
        vendor: dmi("sys_vendor"),
        product: dmi("product_name"),
        version: dmi("product_version"),
        board: dmi("board_name"),
        bios_vendor: dmi("bios_vendor"),
        bios_version: dmi("bios_version"),
        bios_date: dmi("bios_date"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatteryInfo {
    pub name: String,
    pub manufacturer: String,
    pub model: String,
    pub technology: String,
    pub cycles: Option<u32>,
    pub health: Option<u8>,
    pub percent: Option<u8>,
    pub status: String,
}

#[must_use]
pub fn battery_health(full: Option<u64>, design: Option<u64>) -> Option<u8> {
    let (full, design) = (full?, design?);
    if design == 0 {
        return None;
    }
    u8::try_from((full.saturating_mul(100) / design).min(100)).ok()
}

#[must_use]
pub fn read_battery_dir(name: &str, dir: &Path) -> Option<BatteryInfo> {
    let field = |file: &str| read(dir.join(file)).trim().to_string();
    let number = |file: &str| field(file).parse::<u64>().ok();
    if field("type") != "Battery" || field("scope").eq_ignore_ascii_case("device") {
        return None;
    }
    let health = battery_health(number("energy_full"), number("energy_full_design"))
        .or_else(|| battery_health(number("charge_full"), number("charge_full_design")));
    Some(BatteryInfo {
        name: name.to_string(),
        manufacturer: field("manufacturer"),
        model: field("model_name"),
        technology: field("technology"),
        cycles: number("cycle_count").and_then(|cycles| u32::try_from(cycles).ok()).filter(|cycles| *cycles > 0),
        health,
        percent: number("capacity").and_then(|value| u8::try_from(value.min(100)).ok()),
        status: field("status"),
    })
}

#[must_use]
pub fn batteries() -> Vec<BatteryInfo> {
    let root = Path::new("/sys/class/power_supply");
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut names: Vec<String> =
        entries.filter_map(Result::ok).map(|entry| entry.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names.iter().filter_map(|name| read_battery_dir(name, &root.join(name))).collect()
}

#[must_use]
pub fn parse_mem_available(text: &str) -> u64 {
    meminfo_bytes(text, "MemAvailable:")
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostNames {
    pub hostname: String,
    pub static_name: String,
    pub pretty: String,
    pub chassis: String,
    pub available: bool,
}

const HOSTNAME1: &str = "org.freedesktop.hostname1";
const HOSTNAME1_PATH: &str = "/org/freedesktop/hostname1";

#[must_use]
pub fn read_host_names() -> HostNames {
    let from_bus = (|| {
        let proxy = system_proxy(HOSTNAME1, HOSTNAME1_PATH).ok()?;
        let get = |name: &str| proxy.get_property::<String>(name).unwrap_or_default();
        Some(HostNames {
            hostname: get("Hostname"),
            static_name: get("StaticHostname"),
            pretty: get("PrettyHostname"),
            chassis: get("Chassis"),
            available: true,
        })
    })();
    from_bus.unwrap_or_else(|| HostNames {
        hostname: hostname(),
        static_name: read("/etc/hostname").trim().to_string(),
        ..HostNames::default()
    })
}

#[must_use]
pub fn static_hostname_for(pretty: &str) -> String {
    let mut out = String::new();
    for ch in pretty.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if (ch == '-' || ch == ' ' || ch == '_' || ch == '.') && !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed: String = out.trim_matches('-').chars().take(63).collect();
    trimmed.trim_end_matches('-').to_string()
}

#[must_use]
pub fn valid_static_hostname(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
}

#[must_use]
pub fn plan_host_names(pretty: &str, current_static: &str) -> Option<(String, String)> {
    let pretty = pretty.trim();
    if pretty.is_empty() {
        return None;
    }
    let derived = static_hostname_for(pretty);
    let static_name = if valid_static_hostname(&derived) {
        derived
    } else if valid_static_hostname(current_static) {
        current_static.to_string()
    } else {
        return None;
    };
    let pretty = if pretty == static_name { String::new() } else { pretty.to_string() };
    Some((static_name, pretty))
}

pub fn set_host_names(pretty: &str, current_static: &str) -> Result<(), String> {
    let Some((static_name, pretty)) = plan_host_names(pretty, current_static) else {
        return Err(String::from("invalid hostname"));
    };
    let proxy = system_proxy(HOSTNAME1, HOSTNAME1_PATH)?;
    authorized_call(&proxy, "SetStaticHostname", &(static_name.as_str(), true))?;
    authorized_call(&proxy, "SetPrettyHostname", &(pretty.as_str(), true))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Details {
    pub info: SystemInfo,
    pub names: HostNames,
    pub firmware: Firmware,
    pub batteries: Vec<BatteryInfo>,
    pub memory_available: u64,
    pub os_logo: Option<std::path::PathBuf>,
    pub os_id: String,
    pub os_version: String,
    pub os_url: String,
    pub displays: Vec<glide_ipc::OutputInfo>,
    pub displays_live: bool,
}

#[must_use]
pub fn collect_details() -> Details {
    let release = read("/etc/os-release");
    let logo = os_release_value(&release, "LOGO");
    let display = super::display::read();
    let names = read_host_names();
    let mut info = collect();
    if !names.hostname.is_empty() {
        info.hostname.clone_from(&names.hostname);
    }
    Details {
        info,
        firmware: read_firmware(),
        batteries: batteries(),
        memory_available: parse_mem_available(&read("/proc/meminfo")),
        os_logo: logo.and_then(|name| super::apps::icon_path(&name, None)),
        os_id: os_release_value(&release, "ID").unwrap_or_default(),
        os_version: os_release_value(&release, "VERSION_ID")
            .or_else(|| os_release_value(&release, "BUILD_ID"))
            .unwrap_or_default(),
        os_url: os_release_value(&release, "HOME_URL").unwrap_or_default(),
        displays: display.outputs,
        displays_live: display.live,
        names,
    }
}

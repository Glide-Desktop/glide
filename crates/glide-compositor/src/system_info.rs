use std::fs;
use std::path::Path;
use std::process::Command;

#[allow(clippy::struct_excessive_bools)] // Independent toggle states across distinct subsystems
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemStatus {
    pub wifi_level: u8,
    pub wifi_ssid: String,
    pub volume_pct: u8,
    pub volume_muted: bool,
    pub brightness_pct: u8,
    pub battery_pct: Option<u8>,
    pub battery_charging: bool,
    pub bluetooth_on: bool,
    pub night_light_on: bool,
    pub user_name: String,
    pub reduced_motion: bool,
}

impl Default for SystemStatus {
    fn default() -> Self {
        Self {
            wifi_level: 0,
            wifi_ssid: String::new(),
            volume_pct: 50,
            volume_muted: false,
            brightness_pct: 80,
            battery_pct: None,
            battery_charging: false,
            bluetooth_on: false,
            night_light_on: false,
            user_name: String::new(),
            reduced_motion: false,
        }
    }
}

impl SystemStatus {
    #[must_use]
    pub fn fetch(night_light: bool) -> Self {
        let (battery_pct, battery_charging) = read_battery();
        let (wifi_level, wifi_ssid) = read_wifi();
        let (volume_pct, volume_muted) = read_volume();
        let brightness_pct = read_brightness();
        let bluetooth_on = read_bluetooth();
        let user_name = read_user_name();
        let reduced_motion = reduced_motion();

        Self {
            wifi_level,
            wifi_ssid,
            volume_pct,
            volume_muted,
            brightness_pct,
            battery_pct,
            battery_charging,
            bluetooth_on,
            night_light_on: night_light,
            user_name,
            reduced_motion,
        }
    }
}

fn read_battery() -> (Option<u8>, bool) {
    let power_path = Path::new("/sys/class/power_supply");
    let Ok(entries) = fs::read_dir(power_path) else {
        return (None, false);
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let is_bat = name_str.starts_with("BAT")
            || fs::read_to_string(path.join("type"))
                .is_ok_and(|t| t.trim().eq_ignore_ascii_case("battery"));

        if is_bat {
            let capacity = fs::read_to_string(path.join("capacity"))
                .ok()
                .and_then(|c| c.trim().parse::<u8>().ok());
            let status = fs::read_to_string(path.join("status")).unwrap_or_default();
            let charging = status.trim().eq_ignore_ascii_case("charging");
            if capacity.is_some() {
                return (capacity, charging);
            }
        }
    }

    (None, false)
}

fn read_wifi() -> (u8, String) {
    let mut level = 0u8;
    if let Ok(content) = fs::read_to_string("/proc/net/wireless") {
        for line in content.lines().skip(2) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let link_str = parts[2].trim_end_matches('.');
                if let Ok(quality) = link_str.parse::<f32>() {
                    let ratio = (quality / 70.0).clamp(0.0, 1.0);
                    level = if ratio < 0.1 {
                        1
                    } else if ratio < 0.4 {
                        2
                    } else if ratio < 0.7 {
                        3
                    } else {
                        4
                    };
                    break;
                }
            }
        }
    }

    if level == 0
        && let Ok(entries) = fs::read_dir("/sys/class/net")
    {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("wl") {
                let operstate =
                    fs::read_to_string(entry.path().join("operstate")).unwrap_or_default();
                if operstate.trim().eq_ignore_ascii_case("up") {
                    level = 3;
                    break;
                }
            }
        }
    }

    if is_wifi_radio_off() {
        return (0, crate::i18n::t(crate::i18n::Key::Off).to_string());
    }

    let active_ssid = fetch_active_ssid();
    if let Some(ssid) = active_ssid {
        (level.max(1), ssid)
    } else {
        (0, crate::i18n::t(crate::i18n::Key::Connecting).to_string())
    }
}

fn is_wifi_radio_off() -> bool {
    let Ok(output) = Command::new("nmcli").args(["radio", "wifi"]).output() else {
        return false;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim().eq_ignore_ascii_case("disabled")
}

fn fetch_active_ssid() -> Option<String> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "active,ssid", "dev", "wifi"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(ssid) = line.strip_prefix("yes:")
            && !ssid.is_empty()
        {
            return Some(ssid.to_string());
        }
    }
    None
}

fn read_volume() -> (u8, bool) {
    if let Ok(output) = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        let muted = text.contains("[MUTED]");
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // Clamped to 0.0..=100.0
        let pct = text
            .trim()
            .strip_prefix("Volume:")
            .and_then(|v| v.split_whitespace().next())
            .and_then(|v| v.parse::<f32>().ok())
            .map_or(50, |val| (val * 100.0).round().clamp(0.0, 100.0) as u8);
        return (pct, muted);
    }

    if let Ok(output) = Command::new("pactl")
        .args(["get-sink-volume", "@DEFAULT_SINK@"])
        .output()
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        let pct = text
            .split('/')
            .nth(1)
            .and_then(|p| p.trim().strip_suffix('%'))
            .and_then(|p| p.trim().parse::<u8>().ok())
            .unwrap_or(50);
        let muted = Command::new("pactl")
            .args(["get-sink-mute", "@DEFAULT_SINK@"])
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("yes"));
        return (pct, muted);
    }

    (50, false)
}

fn read_brightness() -> u8 {
    let backlight_path = Path::new("/sys/class/backlight");
    let Ok(entries) = fs::read_dir(backlight_path) else {
        return 80;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let current = fs::read_to_string(path.join("brightness"))
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok());
        let max = fs::read_to_string(path.join("max_brightness"))
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok());

        if let (Some(cur), Some(m)) = (current, max)
            && m > 0.0
        {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // Clamped to 0.0..=100.0
            return ((cur * 100.0) / m).round().clamp(0.0, 100.0) as u8;
        }
    }

    80
}

fn read_bluetooth() -> bool {
    if let Ok(output) = Command::new("bluetoothctl").arg("show").output()
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        if text.contains("Powered: yes") {
            return true;
        }
    }

    let rfkill_path = Path::new("/sys/class/rfkill");
    let Ok(entries) = fs::read_dir(rfkill_path) else {
        return false;
    };

    let mut found_bt = false;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_bt = fs::read_to_string(path.join("type"))
            .is_ok_and(|t| t.trim().eq_ignore_ascii_case("bluetooth"));
        if is_bt {
            found_bt = true;
            let soft = fs::read_to_string(path.join("soft")).is_ok_and(|s| s.trim() == "1");
            let hard = fs::read_to_string(path.join("hard")).is_ok_and(|s| s.trim() == "1");
            if soft || hard {
                return false;
            }
        }
    }

    found_bt
}

pub const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3);

pub fn start_poller(sink: impl Fn(SystemStatus) + Send + 'static) {
    let spawned = std::thread::Builder::new()
        .name("glide-system-status".to_string())
        .spawn(move || {
            loop {
                sink(SystemStatus::fetch(false));
                std::thread::sleep(POLL_INTERVAL);
            }
        });
    if let Err(err) = spawned {
        tracing::warn!("system status poller did not start: {err}");
    }
}

fn read_user_name() -> String {
    let full_name = read_gecos_name().filter(|name| !name.is_empty());
    if let Some(name) = full_name {
        return name;
    }
    let user = std::env::var("USER")
        .unwrap_or_else(|_| crate::i18n::t(crate::i18n::Key::User).to_string());
    let mut chars = user.chars();
    chars.next().map_or_else(String::new, |f| {
        f.to_uppercase().collect::<String>() + chars.as_str()
    })
}

fn read_gecos_name() -> Option<String> {
    // SAFETY: `getpwuid` returns a pointer into libc-owned static storage (or
    // null); the pointer is only dereferenced after the null check, and the
    // GECOS field is copied out before any other libc call can overwrite it.
    let gecos = unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_gecos.is_null() {
            return None;
        }
        std::ffi::CStr::from_ptr((*entry).pw_gecos)
            .to_string_lossy()
            .into_owned()
    };
    let full = gecos.split(',').next().unwrap_or_default().trim();
    (!full.is_empty()).then(|| full.to_string())
}

pub fn toggle_wifi() {
    let current_state = Command::new("nmcli")
        .args(["radio", "wifi"])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default();
    let next = if current_state.eq_ignore_ascii_case("enabled") {
        "off"
    } else {
        "on"
    };
    let _ = Command::new("nmcli").args(["radio", "wifi", next]).spawn();
}

#[must_use]
pub fn is_wifi_on() -> bool {
    Command::new("nmcli")
        .args(["radio", "wifi"])
        .output()
        .is_ok_and(|out| {
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .eq_ignore_ascii_case("enabled")
        })
}

pub fn toggle_bluetooth() {
    let currently_on = read_bluetooth();
    if currently_on {
        let _ = Command::new("rfkill")
            .args(["block", "bluetooth"])
            .spawn()
            .or_else(|_| Command::new("bluetoothctl").args(["power", "off"]).spawn());
    } else {
        let _ = Command::new("rfkill")
            .args(["unblock", "bluetooth"])
            .spawn()
            .or_else(|_| Command::new("bluetoothctl").args(["power", "on"]).spawn());
    }
}

pub fn set_volume_pct(pct: u8) {
    #[allow(clippy::cast_precision_loss)]
    let frac = (f32::from(pct) / 100.0).clamp(0.0, 1.5);
    let _ = Command::new("wpctl")
        .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{frac:.2}")])
        .spawn()
        .or_else(|_| {
            Command::new("pactl")
                .args(["set-sink-volume", "@DEFAULT_SINK@", &format!("{pct}%")])
                .spawn()
        });

    if pct > 0 {
        let _ = Command::new("wpctl")
            .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "0"])
            .spawn()
            .or_else(|_| {
                Command::new("pactl")
                    .args(["set-sink-mute", "@DEFAULT_SINK@", "0"])
                    .spawn()
            });
    } else {
        let _ = Command::new("wpctl")
            .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "1"])
            .spawn()
            .or_else(|_| {
                Command::new("pactl")
                    .args(["set-sink-mute", "@DEFAULT_SINK@", "1"])
                    .spawn()
            });
    }
}

pub fn toggle_volume_mute() {
    let _ = Command::new("wpctl")
        .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
        .spawn()
        .or_else(|_| {
            Command::new("pactl")
                .args(["set-sink-mute", "@DEFAULT_SINK@", "toggle"])
                .spawn()
        });
}

pub fn set_brightness_pct(pct: u8) {
    let backlight_path = Path::new("/sys/class/backlight");
    if let Ok(entries) = fs::read_dir(backlight_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Ok(max_str) = fs::read_to_string(path.join("max_brightness"))
                && let Ok(max) = max_str.trim().parse::<u64>()
                && max > 0
            {
                let val = ((u64::from(pct) * max) / 100).max(1);
                let _ = Command::new("busctl")
                    .args([
                        "call",
                        "org.freedesktop.login1",
                        "/org/freedesktop/login1/session/auto",
                        "org.freedesktop.login1.Session",
                        "SetBrightness",
                        "ssu",
                        "backlight",
                        &name_str,
                        &val.to_string(),
                    ])
                    .spawn();
                return;
            }
        }
    }

    let _ = Command::new("brightnessctl")
        .args(["set", &format!("{pct}%")])
        .spawn();
}

#[must_use]
pub fn reduced_motion() -> bool {
    if std::env::var("GLIDE_REDUCED_MOTION").is_ok_and(|value| value == "1") {
        return true;
    }
    Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "false")
}

pub fn open_settings() {
    let _ = crate::spawn_shell_command(
        "gnome-control-center || systemsettings || xfce4-settings-manager",
    );
}

pub fn lock_session() {
    let _ = Command::new("loginctl").arg("lock-session").spawn();
}

pub fn power_off() {
    let _ = Command::new("systemctl").arg("poweroff").spawn();
}

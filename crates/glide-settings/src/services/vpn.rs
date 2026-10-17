use std::path::{Path, PathBuf};
use std::time::Duration;

use super::network::split_terse;
use super::run;

pub const WIREGUARD: &str = "wireguard";
pub const PLUGIN: &str = "vpn";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Vpn {
    pub name: String,
    pub uuid: String,
    pub kind: String,
    pub device: String,
    pub active: bool,
    pub state: String,
    pub autoconnect: bool,
    pub service: String,
    pub external: bool,
}

impl Vpn {
    #[must_use]
    pub fn is_wireguard(&self) -> bool {
        self.kind == WIREGUARD
    }

    #[must_use]
    pub fn activating(&self) -> bool {
        self.state == "activating"
    }

    #[must_use]
    pub fn protocol(&self) -> String {
        if self.is_wireguard() {
            return "WireGuard".to_string();
        }
        let short = self.service.rsplit('.').next().filter(|name| !name.is_empty()).unwrap_or("VPN");
        protocol_label(short)
    }
}

#[must_use]
pub fn protocol_label(short: &str) -> String {
    match short {
        "openvpn" => "OpenVPN".to_string(),
        "wireguard" => "WireGuard".to_string(),
        "openconnect" => "OpenConnect".to_string(),
        "vpnc" => "Cisco VPN (vpnc)".to_string(),
        "pptp" => "PPTP".to_string(),
        "l2tp" => "L2TP".to_string(),
        "strongswan" => "IPsec (strongSwan)".to_string(),
        "libreswan" => "IPsec (Libreswan)".to_string(),
        "fortisslvpn" => "Fortinet SSL VPN".to_string(),
        "sstp" => "SSTP".to_string(),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    pub name: String,
    pub service: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VpnState {
    pub available: bool,
    pub connections: Vec<Vpn>,
    pub plugins: Vec<Plugin>,
}

impl VpnState {
    #[must_use]
    pub fn find(&self, uuid: &str) -> Option<&Vpn> {
        self.connections.iter().find(|vpn| vpn.uuid == uuid)
    }

    #[must_use]
    pub fn supports(&self, kind: &str) -> bool {
        kind == WIREGUARD || self.plugins.iter().any(|plugin| plugin.name == kind)
    }
}

#[must_use]
pub fn parse_connections(text: &str) -> Vec<Vpn> {
    let mut list: Vec<Vpn> = text
        .lines()
        .filter_map(|line| match split_terse(line).as_slice() {
            [name, uuid, kind, device, active, state, autoconnect, rest @ ..]
                if !uuid.is_empty() && (kind == WIREGUARD || kind == PLUGIN) =>
            {
                Some(Vpn {
                    name: name.clone(),
                    uuid: uuid.clone(),
                    kind: kind.clone(),
                    device: device.clone(),
                    active: active == "yes",
                    state: state.clone(),
                    autoconnect: autoconnect == "yes",
                    service: String::new(),
                    external: rest.first().is_some_and(|file| file.starts_with("/run/")),
                })
            }
            _ => None,
        })
        .collect();
    list.sort_by(|a, b| b.active.cmp(&a.active).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    list
}

#[must_use]
pub fn parse_plugin(text: &str) -> Option<Plugin> {
    let mut section = "";
    let mut name = None;
    let mut service = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(header) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            section = header;
            continue;
        }
        if section != "VPN Connection" {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "name" => name = Some(value.trim().to_string()),
                "service" => service = Some(value.trim().to_string()),
                _ => {}
            }
        }
    }
    Some(Plugin { name: name.filter(|name| !name.is_empty())?, service: service.unwrap_or_default() })
}

#[must_use]
pub fn detect_kind(path: &Path, text: &str) -> Option<&'static str> {
    let extension = path.extension().and_then(|ext| ext.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    let has_interface = text.lines().any(|line| line.trim().eq_ignore_ascii_case("[interface]"));
    let has_peer = text.lines().any(|line| line.trim().eq_ignore_ascii_case("[peer]"));
    let openvpn = text.lines().any(|line| {
        let word = line.split_whitespace().next().unwrap_or_default();
        matches!(word, "remote" | "client" | "<ca>" | "dev")
    });
    match extension.as_str() {
        "ovpn" => Some("openvpn"),
        "pcf" => Some("vpnc"),
        "conf" if has_interface && has_peer => Some(WIREGUARD),
        "conf" if openvpn => Some("openvpn"),
        _ => None,
    }
}

#[must_use]
pub fn wireguard_name_ok(path: &Path) -> bool {
    path.file_stem().and_then(|stem| stem.to_str()).is_some_and(|stem| {
        !stem.is_empty() && stem.len() <= 15 && stem.chars().all(|c| c.is_ascii_alphanumeric() || "_=+.-".contains(c))
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VpnDetails {
    pub gateway: String,
    pub user: String,
    pub addresses: Vec<String>,
    pub dns: Vec<String>,
    pub listen_port: String,
    pub banner: String,
}

#[must_use]
pub fn parse_vpn_data(data: &str) -> Vec<(String, String)> {
    data.split(", ")
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

#[must_use]
pub fn parse_details(text: &str) -> VpnDetails {
    let mut details = VpnDetails::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.split('[').next().unwrap_or(key).trim();
        let value = value.trim();
        match key {
            "vpn.data" => {
                for (name, item) in parse_vpn_data(value) {
                    match name.as_str() {
                        "remote" | "gateway" | "host" if details.gateway.is_empty() => {
                            details.gateway = item;
                        }
                        "username" | "user" if details.user.is_empty() => details.user = item,
                        _ => {}
                    }
                }
            }
            "vpn.user-name" if !value.is_empty() && details.user.is_empty() => {
                details.user = value.to_string();
            }
            "VPN.GATEWAY" if !value.is_empty() => details.gateway = value.to_string(),
            "VPN.BANNER" => details.banner = value.to_string(),
            "wireguard.listen-port" if value != "0" => details.listen_port = value.to_string(),
            "IP4.ADDRESS" | "IP6.ADDRESS" if !value.is_empty() => {
                details.addresses.push(value.to_string());
            }
            "IP4.DNS" | "IP6.DNS" if !value.is_empty() => details.dns.push(value.to_string()),
            _ => {}
        }
    }
    details
}

fn plugin_dirs() -> [&'static str; 2] {
    ["/usr/lib/NetworkManager/VPN", "/etc/NetworkManager/VPN"]
}

fn read_plugins() -> Vec<Plugin> {
    let mut plugins: Vec<Plugin> = plugin_dirs()
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flat_map(Iterator::flatten)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "name"))
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .filter_map(|text| parse_plugin(&text))
        .collect();
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    plugins.dedup_by(|a, b| a.name == b.name);
    plugins
}

const MAX_CANDIDATE_BYTES: u64 = 256 * 1024;

#[must_use]
pub fn find_candidates() -> Vec<Candidate> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for dir in [home.join("Downloads"), home.join("Documents"), home.clone()] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let wanted = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "ovpn" | "conf" | "pcf"));
            if !wanted || !entry.metadata().is_ok_and(|meta| meta.is_file() && meta.len() <= MAX_CANDIDATE_BYTES) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(kind) = detect_kind(&path, &text) {
                found.push(Candidate { path, kind: kind.to_string() });
            }
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found.truncate(12);
    found
}

#[must_use]
pub fn read() -> VpnState {
    let Some(text) = run::output(
        "nmcli",
        &["-t", "-f", "NAME,UUID,TYPE,DEVICE,ACTIVE,STATE,AUTOCONNECT,FILENAME", "connection", "show"],
    ) else {
        return VpnState::default();
    };
    let mut connections = parse_connections(&text);
    let plugins: Vec<&str> =
        connections.iter().filter(|vpn| !vpn.is_wireguard()).map(|vpn| vpn.uuid.as_str()).collect();
    let services = super::network::profile_field("vpn.service-type", &plugins);
    for (vpn, service) in connections.iter_mut().filter(|vpn| !vpn.is_wireguard()).zip(services) {
        vpn.service = service.trim().to_string();
    }
    VpnState { available: true, connections, plugins: read_plugins() }
}

#[must_use]
pub fn read_details(uuid: &str) -> VpnDetails {
    run::output(
        "nmcli",
        &[
            "-t",
            "-f",
            "vpn.data,vpn.user-name,wireguard.listen-port,VPN.GATEWAY,VPN.BANNER,IP4.ADDRESS,IP4.DNS,IP6.ADDRESS,IP6.DNS",
            "connection",
            "show",
            "uuid",
            uuid,
        ],
    )
    .map(|text| parse_details(&text))
    .unwrap_or_default()
}

const CONNECT_WAIT: &str = "40";
const CONNECT_DEADLINE: Duration = Duration::from_secs(50);

pub fn connect(uuid: &str) -> Result<String, String> {
    run::status_within("nmcli", &["--wait", CONNECT_WAIT, "connection", "up", "uuid", uuid], CONNECT_DEADLINE)
}

#[must_use]
pub fn needs_secrets(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    [
        "secrets were required",
        "no secrets",
        "no-secrets",
        "no valid secrets",
        "no valid vpn secrets",
        "no agents were available",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(pub String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl Secret {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[must_use]
pub fn secret_keys(service: &str) -> (&'static str, Option<&'static str>) {
    match service.rsplit('.').next().unwrap_or_default() {
        "vpnc" => ("Xauth password", None),
        "libreswan" => ("xauthpassword", None),
        "fortisslvpn" => ("password", Some("otp")),
        _ => ("password", None),
    }
}

#[must_use]
pub fn passwd_file_text(service: &str, password: &Secret, otp: &Secret) -> String {
    let (password_key, otp_key) = secret_keys(service);
    let mut text = format!("vpn.secrets.{password_key}:{}\n", single_line(&password.0));
    if let Some(otp_key) = otp_key
        && !otp.is_empty()
    {
        text.push_str("vpn.secrets.");
        text.push_str(otp_key);
        text.push(':');
        text.push_str(single_line(&otp.0));
        text.push('\n');
    }
    text
}

fn single_line(value: &str) -> &str {
    value.split(['\n', '\r']).next().unwrap_or_default()
}

struct SecretFile(PathBuf);

impl SecretFile {
    fn create(dir: &Path, text: &str) -> Result<Self, String> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_nanos());
        let path = dir.join(format!("glide-vpn-{}-{stamp}", std::process::id()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|err| format!("{}: {err}", path.display()))?;
        let guard = Self(path);
        file.write_all(text.as_bytes())
            .and_then(|()| file.flush())
            .map_err(|err| format!("{}: {err}", guard.0.display()))?;
        Ok(guard)
    }
}

impl Drop for SecretFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub fn connect_with_secrets(uuid: &str, service: &str, password: &Secret, otp: &Secret) -> Result<String, String> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| "XDG_RUNTIME_DIR is not set".to_string())?;
    let file = SecretFile::create(&dir, &passwd_file_text(service, password, otp))?;
    let path = file.0.to_string_lossy().into_owned();
    let result = run::status_within(
        "nmcli",
        &["--wait", CONNECT_WAIT, "connection", "up", "uuid", uuid, "passwd-file", &path],
        CONNECT_DEADLINE,
    );
    drop(file);
    result
}

pub fn disconnect(uuid: &str) -> Result<String, String> {
    run::status("nmcli", &["connection", "down", "uuid", uuid])
}

pub fn delete(uuid: &str) -> Result<String, String> {
    run::status("nmcli", &["connection", "delete", "uuid", uuid])
}

pub fn set_autoconnect(uuid: &str, on: bool) -> Result<String, String> {
    run::status(
        "nmcli",
        &["connection", "modify", "uuid", uuid, "connection.autoconnect", if on { "yes" } else { "no" }],
    )
}

pub fn rename(uuid: &str, name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a name".to_string());
    }
    run::status("nmcli", &["connection", "modify", "uuid", uuid, "connection.id", name])
}

#[must_use]
pub fn expand_home(path: &str) -> PathBuf {
    let path = path.trim();
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

pub fn inspect(path: &str) -> Result<(PathBuf, String), String> {
    let path = expand_home(path);
    let meta = std::fs::metadata(&path).map_err(|err| format!("{}: {err}", path.display()))?;
    if !meta.is_file() || meta.len() > MAX_CANDIDATE_BYTES {
        return Err(format!("{} is not a VPN configuration file", path.display()));
    }
    let text = std::fs::read_to_string(&path).map_err(|err| format!("{}: {err}", path.display()))?;
    detect_kind(&path, &text)
        .map(|kind| (path.clone(), kind.to_string()))
        .ok_or_else(|| format!("{} is not an OpenVPN, WireGuard or vpnc file", path.display()))
}

pub fn import(path: &Path, kind: &str) -> Result<String, String> {
    if kind == WIREGUARD && !wireguard_name_ok(path) {
        return Err(
            "WireGuard names the tunnel after the file: rename it to at most 15 letters, digits or _=+.- and try again"
                .to_string(),
        );
    }
    let file = path.to_string_lossy();
    run::status("nmcli", &["connection", "import", "type", kind, "file", &file])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(text: &str) -> Secret {
        Secret(text.to_string())
    }

    #[test]
    fn missing_secret_failures_are_recognised() {
        assert!(needs_secrets("Error: Connection activation failed: Secrets were required, but not provided."));
        assert!(needs_secrets("Error: Connection activation failed: No valid secrets"));
        assert!(!needs_secrets("Error: Connection activation failed: The VPN service stopped unexpectedly."));
    }

    #[test]
    fn passwd_file_uses_the_plugin_secret_key() {
        assert_eq!(
            passwd_file_text("org.freedesktop.NetworkManager.openvpn", &secret("hunter2"), &secret("")),
            "vpn.secrets.password:hunter2\n"
        );
        assert_eq!(
            passwd_file_text("org.freedesktop.NetworkManager.vpnc", &secret("pw"), &secret("123456")),
            "vpn.secrets.Xauth password:pw\n"
        );
        assert_eq!(
            passwd_file_text("org.freedesktop.NetworkManager.fortisslvpn", &secret("pw"), &secret("123456")),
            "vpn.secrets.password:pw\nvpn.secrets.otp:123456\n"
        );
    }

    #[test]
    fn a_newline_cannot_inject_another_secret_line() {
        assert_eq!(
            passwd_file_text("org.freedesktop.NetworkManager.openvpn", &secret("pw\nvpn.secrets.other:x"), &secret("")),
            "vpn.secrets.password:pw\n"
        );
    }

    #[test]
    fn the_secret_file_is_private_and_removed_on_drop() {
        use std::os::unix::fs::PermissionsExt;
        let file = SecretFile::create(&std::env::temp_dir(), "vpn.secrets.password:x\n").expect("temp dir is writable");
        let path = file.0.clone();
        let meta = std::fs::metadata(&path).expect("file exists while held");
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(&path).expect("readable"), "vpn.secrets.password:x\n");
        drop(file);
        assert!(!path.exists());
    }

    #[test]
    fn secrets_never_show_in_debug_output() {
        assert_eq!(format!("{:?}", secret("hunter2")), "Secret(***)");
    }
}

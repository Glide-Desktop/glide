use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use crate::crypto::cert::IdentityCert;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneConfig {
    pub device_id: String,
    pub device_name: String,
    pub cert_pem: String,
    pub key_pem: String,
    #[serde(default)]
    pub cert_der: Vec<u8>,
    #[serde(default)]
    pub key_der: Vec<u8>,
    pub paired_devices: HashMap<String, PairedDevice>,
    #[serde(skip)]
    pub config_path: Option<PathBuf>,
}

fn generate_device_id() -> String {
    let rng = SystemRandom::new();
    let mut bytes = [0u8; 16];
    if rng.fill(&mut bytes).is_ok() {
        let mut id = String::with_capacity(32);
        for b in bytes {
            let _ = write!(&mut id, "{b:02x}");
        }
        return id;
    }
    format!("glide_{:026x}", std::process::id())
}

fn device_name_from(user: Option<&str>, hostname: Option<&str>) -> String {
    let nonempty = |value: Option<&str>| value.map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned);
    let user = nonempty(user).unwrap_or_else(|| "user".to_string());
    let hostname = nonempty(hostname).unwrap_or_else(|| "localhost".to_string());
    format!("{user}-{hostname}")
}

fn device_name() -> String {
    let user =
        std::env::var("USER").ok().filter(|value| !value.trim().is_empty()).or_else(|| std::env::var("LOGNAME").ok());
    let hostname = fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| fs::read_to_string("/etc/hostname").ok().filter(|value| !value.trim().is_empty()))
        .or_else(|| std::env::var("HOSTNAME").ok().filter(|value| !value.trim().is_empty()));
    if hostname.is_none() {
        tracing::warn!("Could not determine hostname; using localhost for phone identity");
    }
    device_name_from(user.as_deref(), hostname.as_deref())
}

impl PhoneConfig {
    pub fn load_or_generate(path: &Path) -> Result<(Self, IdentityCert), Box<dyn std::error::Error + Send + Sync>> {
        if path.is_file() {
            let content = fs::read_to_string(path)?;
            if let Ok(mut config) = serde_json::from_str::<PhoneConfig>(&content)
                && !config.cert_der.is_empty()
                && !config.key_der.is_empty()
                && config.device_id.len() >= 32
            {
                config.config_path = Some(path.to_path_buf());
                let name = device_name();
                if config.device_name != name {
                    config.device_name = name;
                    config.save_if_configured();
                }
                let cert = IdentityCert::from_der(
                    config.cert_der.clone(),
                    config.key_der.clone(),
                    config.cert_pem.clone(),
                    config.key_pem.clone(),
                );
                return Ok((config, cert));
            }
        }

        let device_id = generate_device_id();
        let cert = IdentityCert::generate(&device_id)?;

        let config = PhoneConfig {
            device_id,
            device_name: device_name(),
            cert_pem: cert.cert_pem.clone(),
            key_pem: cert.key_pem.clone(),
            cert_der: cert.cert_der.clone(),
            key_der: cert.key_der.clone(),
            paired_devices: HashMap::new(),
            config_path: Some(path.to_path_buf()),
        };

        let _ = config.save(path);

        Ok((config, cert))
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let serialized =
            serde_json::to_string_pretty(self).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(path, serialized)
    }

    pub fn save_if_configured(&self) {
        if let Some(path) = &self.config_path {
            let _ = self.save(path);
        }
    }

    #[must_use]
    pub fn default_config_path() -> PathBuf {
        glide_config::xdg_path("glide", "phone.json").unwrap_or_else(|| PathBuf::from("phone.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::device_name_from;

    #[test]
    fn identity_contains_user_and_hostname() {
        assert_eq!(device_name_from(Some(" alice "), Some(" laptop\n")), "alice-laptop");
    }

    #[test]
    fn identity_never_has_an_empty_component() {
        assert_eq!(device_name_from(Some(" "), Some("\n")), "user-localhost");
        assert_eq!(device_name_from(None, None), "user-localhost");
    }
}

use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::crypto::cert::IdentityCert;

fn restrict_permissions(path: &Path) {
    if let Err(err) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
        tracing::warn!(?err, path = %path.display(), "could not restrict the phone identity file to its owner");
    }
}

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
        match fs::read_to_string(path) {
            Ok(content) => match Self::parse(&content) {
                Ok(mut config) => {
                    config.config_path = Some(path.to_path_buf());
                    restrict_permissions(path);
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
                Err(reason) => {
                    let backup = path.with_extension("json.bad");
                    fs::rename(path, &backup)?;
                    tracing::error!(
                        path = %path.display(),
                        backup = %backup.display(),
                        %reason,
                        "phone identity is unreadable; kept it aside and created a new one, every phone must pair again"
                    );
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(Box::new(err)),
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

        config.save(path)?;

        Ok((config, cert))
    }

    fn parse(content: &str) -> Result<Self, String> {
        let config = serde_json::from_str::<PhoneConfig>(content).map_err(|err| err.to_string())?;
        if config.cert_der.is_empty() || config.key_der.is_empty() {
            return Err("certificate or key missing".into());
        }
        if config.device_id.len() < 32 {
            return Err("device id too short".into());
        }
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty())
            && !parent.exists()
        {
            fs::DirBuilder::new().recursive(true).mode(0o700).create(parent)?;
        }
        let serialized =
            serde_json::to_string_pretty(self).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        glide_config::write_private(path, serialized.as_bytes())
    }

    pub fn save_if_configured(&self) {
        if let Some(path) = &self.config_path
            && let Err(err) = self.save(path)
        {
            tracing::error!(path = %path.display(), %err, "failed to save phone identity and pairings");
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

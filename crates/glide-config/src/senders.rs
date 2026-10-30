use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const KEEP_SECONDS: u64 = 30 * 24 * 60 * 60;
pub const LIMIT: usize = 64;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sender {
    pub name: String,
    pub count: u64,
    pub last: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Senders {
    pub apps: BTreeMap<String, Sender>,
}

#[must_use]
pub fn key(desktop_entry: &str, app_name: &str) -> Option<String> {
    let entry = desktop_entry.trim();
    let entry = entry.strip_suffix(".desktop").filter(|stem| stem.ends_with(".desktop")).unwrap_or(entry);
    let chosen = if entry.is_empty() { app_name.trim() } else { entry };
    (!chosen.is_empty()).then(|| chosen.to_string())
}

impl Senders {
    #[must_use]
    pub fn path() -> Option<PathBuf> {
        crate::state_home().map(|dir| dir.join("glide").join("notification-senders.toml"))
    }

    #[must_use]
    pub fn load() -> Self {
        Self::path().and_then(|path| Self::load_from(&path).ok()).unwrap_or_default()
    }

    pub fn load_from(path: &Path) -> io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err),
        }
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let text = toml::to_string(self).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        crate::write_private(path, text.as_bytes())
    }

    pub fn note(&mut self, key: &str, name: &str, now: u64) {
        let sender = self.apps.entry(key.to_string()).or_default();
        sender.count = sender.count.saturating_add(1);
        sender.last = now;
        let name = name.trim();
        if !name.is_empty() {
            name.clone_into(&mut sender.name);
        }
        self.prune(now);
    }

    pub fn prune(&mut self, now: u64) {
        self.apps.retain(|_, sender| now.saturating_sub(sender.last) <= KEEP_SECONDS);
        while self.apps.len() > LIMIT {
            let Some(oldest) = self.apps.iter().min_by_key(|(_, sender)| sender.last).map(|(key, _)| key.clone())
            else {
                break;
            };
            self.apps.remove(&oldest);
        }
    }

    #[must_use]
    pub fn recent(&self, now: u64) -> Vec<(&str, &Sender)> {
        let mut list: Vec<(&str, &Sender)> = self
            .apps
            .iter()
            .filter(|(_, sender)| now.saturating_sub(sender.last) <= KEEP_SECONDS)
            .map(|(key, sender)| (key.as_str(), sender))
            .collect();
        list.sort_by_key(|(_, sender)| std::cmp::Reverse(sender.last));
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sender_is_keyed_by_its_desktop_entry_else_its_name() {
        assert_eq!(key("org.telegram.desktop.desktop", "Telegram").as_deref(), Some("org.telegram.desktop"));
        assert_eq!(key("", "Firefox").as_deref(), Some("Firefox"));
        assert_eq!(key(" ", " "), None);
    }

    #[test]
    fn a_desktop_entry_hint_that_itself_ends_in_desktop_is_not_cut_short() {
        assert_eq!(key("org.telegram.desktop", "Telegram").as_deref(), Some("org.telegram.desktop"));
        assert_eq!(key(" org.telegram.desktop ", "Telegram").as_deref(), Some("org.telegram.desktop"));
        assert_eq!(key("org.telegram.desktop.desktop", "Telegram").as_deref(), Some("org.telegram.desktop"));
        assert_eq!(key("a.desktop.desktop.desktop", "").as_deref(), Some("a.desktop.desktop"), "one suffix only");
        assert_eq!(key("firefox", "Firefox").as_deref(), Some("firefox"));
        assert_eq!(
            key("firefox.desktop", "Firefox").as_deref(),
            Some("firefox.desktop"),
            "ambiguous with a reverse-DNS id, so kept; consumers match both spellings"
        );
    }

    #[test]
    fn senders_count_up_and_old_ones_fall_away() {
        let mut senders = Senders::default();
        senders.note("firefox", "Firefox", 100);
        senders.note("firefox", "", 200);
        senders.note("org.telegram.desktop", "Telegram", 300);
        assert_eq!(senders.apps["firefox"].count, 2);
        assert_eq!(senders.apps["firefox"].name, "Firefox");
        let recent: Vec<&str> = senders.recent(300).into_iter().map(|(key, _)| key).collect();
        assert_eq!(recent, ["org.telegram.desktop", "firefox"]);
        senders.note("steam", "Steam", 300 + KEEP_SECONDS + 150);
        assert!(!senders.apps.contains_key("firefox"), "a month-old sender is dropped");
    }

    #[test]
    fn the_list_never_grows_past_its_limit() {
        let mut senders = Senders::default();
        for index in 0..(LIMIT as u64 + 5) {
            senders.note(&format!("app{index}"), "", 1000 + index);
        }
        assert_eq!(senders.apps.len(), LIMIT);
        assert!(!senders.apps.contains_key("app0"));
    }

    #[test]
    fn the_file_round_trips() {
        let dir = std::env::temp_dir().join(format!("glide-senders-{}", std::process::id()));
        let path = dir.join("senders.toml");
        let mut senders = Senders::default();
        senders.note("firefox", "Firefox", 42);
        senders.save_to(&path).expect("save");
        assert_eq!(Senders::load_from(&path).expect("load"), senders);
        let _ = std::fs::remove_dir_all(dir);
    }
}

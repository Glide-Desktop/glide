use std::path::{Path, PathBuf};

use crate::i18n::{Key, t};

pub const NOTICE_ID: u32 = u32::MAX - 8;
pub const OPEN_SETTINGS: &str = "open-settings";
pub const OPEN_SHORTCUTS: &str = "open-shortcuts";

#[must_use]
pub fn marker() -> Option<PathBuf> {
    Some(glide_config::state_home()?.join("glide").join("welcomed"))
}

#[must_use]
pub fn first_run(marker: &Path) -> bool {
    !marker.exists()
}

pub fn mark(marker: &Path) -> std::io::Result<()> {
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(marker, b"")
}

#[must_use]
pub fn notification() -> glide_notify::Notification {
    let action = |key: &str, label: Key| glide_notify::Action { key: key.to_string(), label: t(label).to_string() };
    glide_notify::Notification {
        id: NOTICE_ID,
        app_name: "Glide".to_string(),
        app_icon: "preferences-desktop".to_string(),
        summary: t(Key::WelcomeTitle).to_string(),
        body: t(Key::WelcomeBody).to_string(),
        actions: vec![action(OPEN_SHORTCUTS, Key::WelcomeShortcuts), action(OPEN_SETTINGS, Key::WelcomeSettings)],
        timeout: None,
        ..glide_notify::Notification::default()
    }
}

impl crate::GlideState {
    pub fn welcome_if_first_run(&mut self) {
        let Some(marker) = marker() else {
            return;
        };
        if !first_run(&marker) {
            return;
        }
        if let Err(err) = mark(&marker) {
            tracing::warn!(path = %marker.display(), %err, "cannot remember the welcome, so it is not shown");
            return;
        }
        self.notifications.push(&mut self.dock.icons, notification(), true);
        self.refresh_topbar();
        self.queue_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_welcome_is_shown_once_per_account() {
        let dir = std::env::temp_dir().join(format!("glide-welcome-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let marker = dir.join("glide").join("welcomed");
        assert!(first_run(&marker));
        mark(&marker).expect("the marker is written");
        assert!(!first_run(&marker));
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn the_welcome_stays_until_dismissed_and_offers_settings() {
        let notice = notification();
        assert_eq!(notice.timeout, None);
        assert!(crate::notifications::is_local(notice.id));
        let keys: Vec<&str> = notice.actions.iter().map(|action| action.key.as_str()).collect();
        assert_eq!(keys, [OPEN_SHORTCUTS, OPEN_SETTINGS]);
    }
}

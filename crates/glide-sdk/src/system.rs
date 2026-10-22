use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{Sender, channel};

use zbus::zvariant::Value;

pub use glide_app::integration::{file_uri, open_terminal_tab, show_in_files};
pub use glide_app::{quicklist, search_provider};

const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";
const LAUNCHER_ENTRY: &str = "com.canonical.Unity.LauncherEntry";

fn shell_call<R>(method: &str) -> zbus::Result<R>
where
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    let connection = zbus::blocking::Connection::session()?;
    connection
        .call_method(Some(glide_app::BUS_NAME), glide_app::PATH, Some(glide_app::INTERFACE), method, &())?
        .body()
        .deserialize()
}

pub fn frame_windows() -> zbus::Result<()> {
    let connection = zbus::blocking::Connection::session()?;
    connection.call_method(
        Some(glide_app::BUS_NAME),
        glide_app::PATH,
        Some(glide_app::INTERFACE),
        "FrameWindows",
        &(),
    )?;
    Ok(())
}

#[must_use]
pub fn activation_token(app_id: &str) -> Option<String> {
    shell_call::<String>("ActivationToken")
        .ok()
        .filter(|token| !token.is_empty())
        .or_else(|| glide_ipc::client::activation_token(Some(app_id)))
}

#[must_use]
pub fn activation_env(app_id: &str) -> Vec<(&'static str, String)> {
    activation_token(app_id)
        .map(|token| vec![("XDG_ACTIVATION_TOKEN", token.clone()), ("DESKTOP_STARTUP_ID", token)])
        .unwrap_or_default()
}

pub fn spawn_detached(mut command: std::process::Command, app_id: &str) -> std::io::Result<()> {
    command.envs(activation_env(app_id)).stdin(std::process::Stdio::null());
    let mut child = command.spawn()?;
    std::thread::Builder::new().name("glide-sdk-child".into()).spawn(move || {
        let _ = child.wait();
    })?;
    Ok(())
}

pub fn open(target: &str, app_id: &str) -> std::io::Result<()> {
    let mut command = std::process::Command::new("xdg-open");
    command.arg(target);
    spawn_detached(command, app_id)
}

pub fn reveal(path: &Path, app_id: &str) -> std::io::Result<()> {
    let token = activation_token(app_id).unwrap_or_default();
    show_in_files(path, &token).map_err(std::io::Error::other)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Notification {
    pub app_id: String,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub icon: String,
    pub urgency: Urgency,
    pub replaces: u32,
    pub timeout_ms: i32,
    pub actions: Vec<(String, String)>,
}

impl Notification {
    #[must_use]
    pub fn new(app_id: impl Into<String>, summary: impl Into<String>) -> Self {
        Self { app_id: app_id.into(), summary: summary.into(), timeout_ms: -1, ..Self::default() }
    }

    #[must_use]
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    #[must_use]
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    #[must_use]
    pub fn urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
        self
    }

    #[must_use]
    pub fn replaces(mut self, id: u32) -> Self {
        self.replaces = id;
        self
    }

    #[must_use]
    pub fn action(mut self, key: impl Into<String>, label: impl Into<String>) -> Self {
        self.actions.push((key.into(), label.into()));
        self
    }

    fn hints(&self) -> HashMap<&'static str, Value<'_>> {
        let urgency: u8 = match self.urgency {
            Urgency::Low => 0,
            Urgency::Normal => 1,
            Urgency::Critical => 2,
        };
        HashMap::from([("urgency", Value::from(urgency)), ("desktop-entry", Value::from(self.app_id.as_str()))])
    }

    fn flat_actions(&self) -> Vec<&str> {
        self.actions.iter().flat_map(|(key, label)| [key.as_str(), label.as_str()]).collect()
    }
}

pub async fn notify(notification: Notification) -> zbus::Result<u32> {
    let connection = zbus::Connection::session().await?;
    let name = if notification.app_name.is_empty() { &notification.app_id } else { &notification.app_name };
    let reply = connection
        .call_method(
            Some(NOTIFICATIONS),
            NOTIFICATIONS_PATH,
            Some(NOTIFICATIONS),
            "Notify",
            &(
                name.as_str(),
                notification.replaces,
                notification.icon.as_str(),
                notification.summary.as_str(),
                notification.body.as_str(),
                notification.flat_actions(),
                notification.hints(),
                notification.timeout_ms,
            ),
        )
        .await?;
    reply.body().deserialize()
}

pub async fn withdraw(id: u32) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    connection
        .call_method(Some(NOTIFICATIONS), NOTIFICATIONS_PATH, Some(NOTIFICATIONS), "CloseNotification", &(id,))
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Badge {
    pub progress: Option<f64>,
    pub count: Option<i64>,
    pub urgent: bool,
}

impl Badge {
    #[must_use]
    pub fn properties(&self) -> HashMap<&'static str, Value<'static>> {
        HashMap::from([
            ("progress-visible", Value::from(self.progress.is_some())),
            ("progress", Value::from(self.progress.unwrap_or(0.0).clamp(0.0, 1.0))),
            ("count-visible", Value::from(self.count.is_some())),
            ("count", Value::from(self.count.unwrap_or(0))),
            ("urgent", Value::from(self.urgent)),
        ])
    }
}

#[derive(Debug, Clone)]
pub struct Dock {
    updates: Sender<Badge>,
    badge: Badge,
}

impl Dock {
    #[must_use]
    pub fn new(desktop_id: &str) -> Self {
        let (updates, inbox) = channel::<Badge>();
        let uri = format!("application://{}.desktop", desktop_id.trim_end_matches(".desktop"));
        let object = format!("/org/glide/LauncherEntry/{}", object_segment(desktop_id));
        let spawned = std::thread::Builder::new().name("glide-sdk-dock".into()).spawn(move || {
            let Ok(connection) = zbus::blocking::Connection::session() else { return };
            while let Ok(mut badge) = inbox.recv() {
                while let Ok(newer) = inbox.try_recv() {
                    badge = newer;
                }
                let _ = connection.emit_signal(
                    None::<&str>,
                    object.as_str(),
                    LAUNCHER_ENTRY,
                    "Update",
                    &(uri.as_str(), badge.properties()),
                );
            }
        });
        if let Err(err) = spawned {
            tracing::debug!("cannot report to the dock: {err}");
        }
        Self { updates, badge: Badge::default() }
    }

    fn send(&mut self, badge: Badge) {
        if badge != self.badge {
            self.badge = badge;
            let _ = self.updates.send(badge);
        }
    }

    pub fn progress(&mut self, progress: Option<f64>) {
        let progress = progress.map(|value| (value.clamp(0.0, 1.0) * 1000.0).round() / 1000.0);
        self.send(Badge { progress, ..self.badge });
    }

    pub fn count(&mut self, count: Option<i64>) {
        self.send(Badge { count, ..self.badge });
    }

    pub fn urgent(&mut self, urgent: bool) {
        self.send(Badge { urgent, ..self.badge });
    }
}

fn object_segment(desktop_id: &str) -> String {
    desktop_id.trim_end_matches(".desktop").chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_carry_their_app_and_urgency() {
        let notification =
            Notification::new("org.example.Notes", "Saved").urgency(Urgency::Critical).action("open", "Open");
        let hints = notification.hints();
        assert_eq!(hints["urgency"], Value::from(2_u8));
        assert_eq!(hints["desktop-entry"], Value::from("org.example.Notes"));
        assert_eq!(notification.flat_actions(), vec!["open", "Open"]);
        assert_eq!(notification.timeout_ms, -1, "the server picks the timeout by default");
    }

    #[test]
    fn a_hidden_badge_still_says_so() {
        let props = Badge::default().properties();
        assert_eq!(props["progress-visible"], Value::from(false));
        assert_eq!(props["count-visible"], Value::from(false));
        let props = Badge { progress: Some(1.7), count: Some(3), urgent: true }.properties();
        assert_eq!(props["progress"], Value::from(1.0));
        assert_eq!(props["count"], Value::from(3_i64));
    }

    #[test]
    fn launcher_entry_paths_are_valid_object_paths() {
        assert_eq!(object_segment("org.example.Notes.desktop"), "org_example_Notes");
        assert!(
            zbus::zvariant::ObjectPath::try_from(format!("/org/glide/LauncherEntry/{}", object_segment("my-app")))
                .is_ok()
        );
    }
}

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, PoisonError};

use iced::futures::channel::mpsc::{UnboundedReceiver, unbounded};
use zbus::zvariant::Value;

pub use glide_app::integration::{file_uri, open_terminal_tab, show_in_files};
pub use glide_app::{quicklist, search_provider};

const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";
const LAUNCHER_ENTRY: &str = "com.canonical.Unity.LauncherEntry";
const INLINE_REPLY: &str = "inline-reply";

fn bus() -> zbus::Result<zbus::blocking::Connection> {
    static BUS: Mutex<Option<zbus::blocking::Connection>> = Mutex::new(None);
    let mut slot = BUS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(connection) = slot.as_ref() {
        return Ok(connection.clone());
    }
    let connection = zbus::blocking::Connection::session()?;
    *slot = Some(connection.clone());
    Ok(connection)
}

fn shell_call<B, R>(method: &str, body: &B) -> zbus::Result<R>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    bus()?
        .call_method(Some(glide_app::BUS_NAME), glide_app::PATH, Some(glide_app::INTERFACE), method, body)?
        .body()
        .deserialize()
}

pub fn frame_windows() -> zbus::Result<()> {
    shell_call::<_, ()>("FrameWindows", &())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Osd {
    Volume,
    Brightness,
    KeyboardBrightness,
    CapsLock,
}

impl Osd {
    const fn key(self) -> &'static str {
        match self {
            Self::Volume => "volume",
            Self::Brightness => "brightness",
            Self::KeyboardBrightness => "keyboard-brightness",
            Self::CapsLock => "caps-lock",
        }
    }
}

pub fn copy_secret(secret: &str, clear_after: u32) -> zbus::Result<()> {
    shell_call::<_, ()>("CopySecret", &(secret, clear_after))
}

pub fn show_osd(kind: Osd, level: u32, on: bool) -> zbus::Result<()> {
    shell_call::<_, ()>("ShowOsd", &(kind.key(), level.min(100), on))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickSettings {
    Main,
    Wifi,
    Bluetooth,
    Sound,
    Battery,
}

pub fn open_quick_settings(view: QuickSettings) -> zbus::Result<()> {
    let view = match view {
        QuickSettings::Main => "",
        QuickSettings::Wifi => "wifi",
        QuickSettings::Bluetooth => "bluetooth",
        QuickSettings::Sound => "sound",
        QuickSettings::Battery => "battery",
    };
    shell_call::<_, ()>("OpenQuickSettings", &(view,))
}

pub fn scan_screen_qr() -> Result<Vec<glide_ipc::keychain::Hidden>, String> {
    let request = glide_ipc::Request::ScanQr;
    match glide_ipc::client::exchange(&request, std::time::Duration::from_secs(10)).map_err(|err| err.to_string())? {
        glide_ipc::Response::QrCodes { codes } => Ok(codes),
        glide_ipc::Response::Error { message } => Err(message),
        other => Err(format!("unexpected answer: {other:?}")),
    }
}

#[must_use]
pub fn activation_token(app_id: &str) -> Option<String> {
    shell_call::<_, String>("ActivationToken", &())
        .ok()
        .filter(|token| !token.is_empty())
        .or_else(|| glide_ipc::client::activation_token(Some(app_id)))
}

#[must_use]
pub fn activation_env(app_id: &str) -> Vec<(&'static str, String)> {
    token_env(activation_token(app_id))
}

#[must_use]
pub fn launch_token() -> Option<String> {
    shell_call::<_, String>("ActivationToken", &())
        .ok()
        .filter(|token| !token.is_empty())
        .or_else(|| glide_ipc::client::activation_token(None))
}

#[must_use]
pub fn launch_env() -> Vec<(&'static str, String)> {
    token_env(launch_token())
}

fn token_env(token: Option<String>) -> Vec<(&'static str, String)> {
    token.map(|token| vec![("XDG_ACTIVATION_TOKEN", token.clone()), ("DESKTOP_STARTUP_ID", token)]).unwrap_or_default()
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
    pub category: Option<String>,
    pub resident: bool,
    pub transient: bool,
    pub sound: Option<String>,
    pub silent: bool,
    pub progress: Option<u8>,
    pub reply: Option<(String, String)>,
    pub image_path: Option<String>,
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
    pub fn timeout(mut self, milliseconds: i32) -> Self {
        self.timeout_ms = milliseconds;
        self
    }

    #[must_use]
    pub fn action(mut self, key: impl Into<String>, label: impl Into<String>) -> Self {
        self.actions.push((key.into(), label.into()));
        self
    }

    #[must_use]
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    #[must_use]
    pub fn resident(mut self, resident: bool) -> Self {
        self.resident = resident;
        self
    }

    #[must_use]
    pub fn transient(mut self, transient: bool) -> Self {
        self.transient = transient;
        self
    }

    #[must_use]
    pub fn sound(mut self, name: impl Into<String>) -> Self {
        self.sound = Some(name.into());
        self
    }

    #[must_use]
    pub fn silent(mut self, silent: bool) -> Self {
        self.silent = silent;
        self
    }

    #[must_use]
    pub fn progress(mut self, percent: Option<u8>) -> Self {
        self.progress = percent.map(|percent| percent.min(100));
        self
    }

    #[must_use]
    pub fn reply(mut self, label: impl Into<String>, placeholder: impl Into<String>) -> Self {
        self.reply = Some((label.into(), placeholder.into()));
        self
    }

    #[must_use]
    pub fn image(mut self, path: impl Into<String>) -> Self {
        self.image_path = Some(path.into());
        self
    }

    fn hints(&self) -> HashMap<&'static str, Value<'_>> {
        let urgency: u8 = match self.urgency {
            Urgency::Low => 0,
            Urgency::Normal => 1,
            Urgency::Critical => 2,
        };
        let mut hints =
            HashMap::from([("urgency", Value::from(urgency)), ("desktop-entry", Value::from(self.app_id.as_str()))]);
        if let Some(category) = &self.category {
            hints.insert("category", Value::from(category.as_str()));
        }
        if self.resident {
            hints.insert("resident", Value::from(true));
        }
        if self.transient {
            hints.insert("transient", Value::from(true));
        }
        if let Some(sound) = &self.sound {
            hints.insert("sound-name", Value::from(sound.as_str()));
        }
        if self.silent {
            hints.insert("suppress-sound", Value::from(true));
        }
        if let Some(percent) = self.progress {
            hints.insert("value", Value::from(i32::from(percent)));
        }
        if let Some((_, placeholder)) = &self.reply {
            hints.insert("x-kde-reply-placeholder-text", Value::from(placeholder.as_str()));
        }
        if let Some(path) = &self.image_path {
            hints.insert("image-path", Value::from(path.as_str()));
        }
        hints
    }

    fn flat_actions(&self) -> Vec<&str> {
        let mut flat: Vec<&str> = self.actions.iter().flat_map(|(key, label)| [key.as_str(), label.as_str()]).collect();
        if let Some((label, _)) = &self.reply {
            flat.extend([INLINE_REPLY, label.as_str()]);
        }
        flat
    }
}

pub async fn notify(notification: Notification) -> zbus::Result<u32> {
    let connection = bus()?.into_inner();
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
    let connection = bus()?.into_inner();
    connection
        .call_method(Some(NOTIFICATIONS), NOTIFICATIONS_PATH, Some(NOTIFICATIONS), "CloseNotification", &(id,))
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closed {
    Expired,
    Dismissed,
    Withdrawn,
    Other,
}

impl Closed {
    const fn from_reason(reason: u32) -> Self {
        match reason {
            1 => Self::Expired,
            2 => Self::Dismissed,
            3 => Self::Withdrawn,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationEvent {
    Action { id: u32, key: String },
    Replied { id: u32, text: String },
    Closed { id: u32, reason: Closed },
}

fn parse_event(member: &str, message: &zbus::Message) -> Option<NotificationEvent> {
    let body = message.body();
    match member {
        "ActionInvoked" => {
            let (id, key): (u32, String) = body.deserialize().ok()?;
            Some(NotificationEvent::Action { id, key })
        }
        "NotificationReplied" => {
            let (id, text): (u32, String) = body.deserialize().ok()?;
            Some(NotificationEvent::Replied { id, text })
        }
        "NotificationClosed" => {
            let (id, reason): (u32, u32) = body.deserialize().ok()?;
            Some(NotificationEvent::Closed { id, reason: Closed::from_reason(reason) })
        }
        _ => None,
    }
}

#[must_use]
pub fn notification_events() -> UnboundedReceiver<NotificationEvent> {
    let (sender, receiver) = unbounded();
    let spawned = std::thread::Builder::new().name("glide-sdk-notify".into()).spawn(move || {
        let listen = || -> zbus::Result<()> {
            let connection = bus()?;
            let rule = zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .interface(NOTIFICATIONS)?
                .path(NOTIFICATIONS_PATH)?
                .build();
            for message in zbus::blocking::MessageIterator::for_match_rule(rule, &connection, Some(64))? {
                let message = message?;
                let header = message.header();
                let Some(member) = header.member() else { continue };
                if let Some(event) = parse_event(member.as_str(), &message)
                    && sender.unbounded_send(event).is_err()
                {
                    break;
                }
            }
            Ok(())
        };
        if let Err(err) = listen() {
            tracing::debug!("cannot follow notification events: {err}");
        }
    });
    if let Err(err) = spawned {
        tracing::debug!("cannot follow notification events: {err}");
    }
    receiver
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
            let Ok(connection) = bus() else { return };
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

    #[must_use]
    pub fn badge(&self) -> Badge {
        self.badge
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
        assert!(!hints.contains_key("resident"), "unset hints stay out of the map");
    }

    #[test]
    fn a_reply_rides_as_the_inline_reply_action_with_its_placeholder() {
        let notification = Notification::new("org.example.Chat", "Olena")
            .reply("Send", "Type a reply…")
            .progress(Some(140))
            .resident(true)
            .sound("message-new-instant");
        assert_eq!(notification.flat_actions(), vec![INLINE_REPLY, "Send"]);
        let hints = notification.hints();
        assert_eq!(hints["x-kde-reply-placeholder-text"], Value::from("Type a reply…"));
        assert_eq!(hints["value"], Value::from(100_i32), "progress is clamped to a percentage");
        assert_eq!(hints["resident"], Value::from(true));
        assert_eq!(hints["sound-name"], Value::from("message-new-instant"));
    }

    #[test]
    fn close_reasons_follow_the_specification() {
        assert_eq!(Closed::from_reason(1), Closed::Expired);
        assert_eq!(Closed::from_reason(2), Closed::Dismissed);
        assert_eq!(Closed::from_reason(3), Closed::Withdrawn);
        assert_eq!(Closed::from_reason(9), Closed::Other);
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

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use zbus::zvariant::OwnedValue;

use crate::{
    hints,
    model::{
        CloseReason, Notification, NotifyEvent, Reply, capabilities, resolve_timeout,
        sanitize_body, split_actions,
    },
};

pub const BUS_NAME: &str = "org.freedesktop.Notifications";
pub const OBJECT_PATH: &str = "/org/freedesktop/Notifications";

const FLOOD_WINDOW: Duration = Duration::from_secs(1);

pub type Sink = Arc<dyn Fn(NotifyEvent) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inhibitor {
    pub desktop_entry: String,
    pub reason: String,
}

struct Inner {
    next_id: u32,
    next_cookie: u32,
    last: Option<(Notification, Instant)>,
    inhibitors: HashMap<u32, Inhibitor>,
}

impl Inner {
    fn new() -> Self {
        Self {
            next_id: 1,
            next_cookie: 1,
            last: None,
            inhibitors: HashMap::new(),
        }
    }

    fn allocate(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).unwrap_or(1);
        id
    }
}

pub struct Service {
    sink: Sink,
    inner: Mutex<Inner>,
}

impl Service {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn is_inhibited(&self) -> bool {
        !self.lock().inhibitors.is_empty()
    }
}

// Every signature below is dictated by the org.freedesktop.Notifications
// D-Bus interface rather than chosen here: entry points must take `&self`,
// keep the argument order and count the specification fixes, take their
// arguments by value across the bus boundary, and keep a `Result` so zbus can
// map a failure onto a D-Bus error reply.
#[allow(
    clippy::too_many_arguments,
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,
    clippy::unused_self
)]
#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Service {
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hint_map: HashMap<String, OwnedValue>,
        expire_timeout: i32,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<u32> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();

        let urgency = hints::urgency(&hint_map);
        let (buttons, has_default, has_settings, reply_label) = split_actions(&actions);
        let reply = reply_label.map(|label| Reply {
            key: crate::model::ACTION_INLINE_REPLY.to_string(),
            label,
            placeholder: hints::string(&hint_map, "x-kde-reply-placeholder-text"),
        });

        let display_name = hints::string(&hint_map, "x-kde-display-appname");
        let app_name = if display_name.is_empty() {
            app_name
        } else {
            display_name
        };

        let mut notification = Notification {
            id: replaces_id,
            sender,
            app_name,
            desktop_entry: hints::string(&hint_map, "desktop-entry"),
            app_icon,
            summary: sanitize_body(&summary),
            body: sanitize_body(&body),
            actions: buttons,
            has_default_action: has_default,
            has_settings_action: has_settings,
            reply,
            urgency,
            category: hints::string(&hint_map, "category"),
            origin: hints::string(&hint_map, "x-kde-origin-name"),
            image: hints::image(&hint_map),
            image_path: first_non_empty(&hint_map, &["image-path", "image_path"]),
            resident: hints::boolean(&hint_map, "resident"),
            transient: hints::boolean(&hint_map, "transient"),
            timeout: resolve_timeout(expire_timeout, urgency),
            progress: hints::progress(&hint_map),
        };

        let replacing = replaces_id != 0;
        {
            let mut inner = self.lock();
            if !replacing {
                if let Some((previous, at)) = inner.last.as_ref()
                    && at.elapsed() < FLOOD_WINDOW
                    && previous.is_duplicate_of(&notification)
                {
                    return Err(zbus::fdo::Error::Failed(
                        "created too many similar notifications in quick succession".to_string(),
                    ));
                }
                notification.id = inner.allocate();
            }
            inner.last = Some((notification.clone(), Instant::now()));
        }

        let id = notification.id;
        let event = if replacing {
            NotifyEvent::Replaced(Box::new(notification))
        } else {
            NotifyEvent::Added(Box::new(notification))
        };
        (self.sink)(event);

        Ok(id)
    }

    fn close_notification(&self, id: u32) -> zbus::fdo::Result<()> {
        (self.sink)(NotifyEvent::Closed(id, CloseReason::Revoked));
        Ok(())
    }

    fn get_capabilities(&self) -> Vec<String> {
        capabilities()
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "Glide".to_string(),
            "glide".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            "1.3".to_string(),
        )
    }

    fn inhibit(
        &self,
        desktop_entry: String,
        reason: String,
        hints: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<u32> {
        drop(hints);
        let (cookie, became_inhibited) = {
            let mut inner = self.lock();
            let was = !inner.inhibitors.is_empty();
            let cookie = inner.next_cookie;
            inner.next_cookie = inner.next_cookie.checked_add(1).unwrap_or(1);
            inner.inhibitors.insert(
                cookie,
                Inhibitor {
                    desktop_entry,
                    reason,
                },
            );
            (cookie, !was)
        };
        if became_inhibited {
            (self.sink)(NotifyEvent::InhibitedChanged(true));
        }
        Ok(cookie)
    }

    fn un_inhibit(&self, cookie: u32) -> zbus::fdo::Result<()> {
        let released = {
            let mut inner = self.lock();
            inner.inhibitors.remove(&cookie);
            inner.inhibitors.is_empty()
        };
        if released {
            (self.sink)(NotifyEvent::InhibitedChanged(false));
        }
        Ok(())
    }

    #[zbus(property)]
    fn inhibited(&self) -> bool {
        self.is_inhibited()
    }
}

fn first_non_empty(hint_map: &HashMap<String, OwnedValue>, keys: &[&str]) -> String {
    for key in keys {
        let value = hints::string(hint_map, key);
        if !value.is_empty() {
            return value;
        }
    }
    String::new()
}

pub struct NotifyHandle {
    connection: zbus::blocking::Connection,
}

impl std::fmt::Debug for NotifyHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("NotifyHandle")
    }
}

impl NotifyHandle {
    fn emit<B>(&self, member: &str, destination: Option<&str>, body: &B)
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        let built =
            zbus::message::Message::signal(OBJECT_PATH, BUS_NAME, member).and_then(|builder| {
                match destination {
                    Some(destination) => builder.destination(destination)?.build(body),
                    None => builder.build(body),
                }
            });
        match built {
            Ok(message) => {
                if let Err(err) = self.connection.send(&message) {
                    tracing::debug!("failed to emit {member}: {err}");
                }
            }
            Err(err) => tracing::debug!("failed to build {member}: {err}"),
        }
    }

    pub fn closed(&self, id: u32, reason: CloseReason) {
        self.emit("NotificationClosed", None, &(id, reason.as_wire()));
    }

    pub fn invoked(&self, id: u32, action: &str, token: Option<&str>) {
        if let Some(token) = token {
            self.emit("ActivationToken", None, &(id, token));
        }
        self.emit("ActionInvoked", None, &(id, action));
    }

    pub fn replied(&self, id: u32, destination: &str, text: &str) {
        if destination.is_empty() {
            return;
        }
        self.emit("NotificationReplied", Some(destination), &(id, text));
    }
}

pub fn start(sink: Sink) -> anyhow::Result<NotifyHandle> {
    let service = Service {
        sink,
        inner: Mutex::new(Inner::new()),
    };

    let connection = zbus::blocking::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, service)?
        .build()?;

    tracing::info!("serving {BUS_NAME} on the session bus");
    Ok(NotifyHandle { connection })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> (Service, Arc<Mutex<Vec<NotifyEvent>>>) {
        let log: Arc<Mutex<Vec<NotifyEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink_log = Arc::clone(&log);
        let service = Service {
            sink: Arc::new(move |event| {
                sink_log
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(event);
            }),
            inner: Mutex::new(Inner::new()),
        };
        (service, log)
    }

    #[test]
    fn ids_start_at_one_and_never_hand_out_zero() {
        let (service, _) = service();
        let mut inner = service.lock();
        assert_eq!(inner.allocate(), 1);
        assert_eq!(inner.allocate(), 2);

        inner.next_id = u32::MAX;
        assert_eq!(inner.allocate(), u32::MAX);
        assert_eq!(inner.allocate(), 1, "the counter wraps past zero");
    }

    #[test]
    fn an_inhibitor_flips_the_inhibited_property_once_per_transition() {
        let (service, log) = service();
        assert!(!service.inhibited());

        let first = {
            let mut inner = service.lock();
            let cookie = inner.next_cookie;
            inner.next_cookie += 1;
            inner.inhibitors.insert(
                cookie,
                Inhibitor {
                    desktop_entry: "player".to_string(),
                    reason: "playing a film".to_string(),
                },
            );
            cookie
        };
        assert!(service.inhibited());

        service.lock().inhibitors.remove(&first);
        assert!(!service.inhibited());
        assert!(
            log.lock().unwrap().is_empty(),
            "direct state edits do not emit; only the D-Bus entry points do"
        );
    }

    #[test]
    fn the_server_identifies_itself_with_the_spec_version_it_implements() {
        let (service, _) = service();
        let (name, vendor, _, spec) = service.get_server_information();
        assert_eq!(name, "Glide");
        assert_eq!(vendor, "glide");
        assert_eq!(spec, "1.3");
    }

    #[test]
    fn the_first_image_path_hint_that_carries_a_value_wins() {
        let mut hints = HashMap::new();
        hints.insert(
            "image_path".to_string(),
            OwnedValue::try_from(zbus::zvariant::Value::Str("/legacy.png".into()))
                .expect("value converts"),
        );
        assert_eq!(
            first_non_empty(&hints, &["image-path", "image_path"]),
            "/legacy.png"
        );
    }

    #[test]
    fn a_missing_image_path_hint_yields_an_empty_string() {
        assert_eq!(
            first_non_empty(&HashMap::new(), &["image-path", "image_path"]),
            ""
        );
    }
}

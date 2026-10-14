use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use zbus::zvariant::OwnedValue;

use crate::{
    hints,
    model::{
        CloseReason, Notification, NotifyEvent, Reply, capabilities, plain_text, resolve_timeout, sanitize_body,
        split_actions,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Held {
    owner: String,
    inhibitor: Inhibitor,
}

struct Inner {
    next_id: u32,
    next_cookie: u32,
    last: Option<(Notification, Instant)>,
    live: HashSet<u32>,
    inhibitors: HashMap<u32, Held>,
}

impl Inner {
    fn new() -> Self {
        Self { next_id: 1, next_cookie: 1, last: None, live: HashSet::new(), inhibitors: HashMap::new() }
    }

    fn allocate(&mut self) -> u32 {
        let mut id = self.next_id;
        while self.live.contains(&id) {
            id = id.checked_add(1).unwrap_or(1);
        }
        self.next_id = id.checked_add(1).unwrap_or(1);
        id
    }

    fn replaceable(&self, replaces_id: u32) -> bool {
        replaces_id != 0 && self.live.contains(&replaces_id)
    }

    fn forget(&mut self, id: u32) {
        self.live.remove(&id);
    }

    fn hold(&mut self, owner: String, inhibitor: Inhibitor) -> (u32, bool) {
        let was = !self.inhibitors.is_empty();
        let mut cookie = self.next_cookie;
        while cookie == 0 || self.inhibitors.contains_key(&cookie) {
            cookie = cookie.checked_add(1).unwrap_or(1);
        }
        self.next_cookie = cookie.checked_add(1).unwrap_or(1);
        self.inhibitors.insert(cookie, Held { owner, inhibitor });
        (cookie, !was)
    }

    fn release(&mut self, owner: &str, cookie: u32) -> bool {
        let by_caller = self.inhibitors.get(&cookie).is_some_and(|held| held.owner == owner);
        if !by_caller {
            return false;
        }
        self.inhibitors.remove(&cookie);
        self.inhibitors.is_empty()
    }

    fn release_owner(&mut self, owner: &str) -> bool {
        let before = self.inhibitors.len();
        self.inhibitors.retain(|_, held| held.owner != owner);
        before != self.inhibitors.len() && self.inhibitors.is_empty()
    }
}

type Shared = Arc<Mutex<Inner>>;

fn lock_shared(inner: &Shared) -> std::sync::MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub struct Service {
    sink: Sink,
    inner: Shared,
}

impl Service {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        lock_shared(&self.inner)
    }

    fn sender(header: &zbus::message::Header<'_>) -> String {
        header.sender().map(ToString::to_string).unwrap_or_default()
    }
}

// Signatures are fixed by the org.freedesktop.Notifications D-Bus interface.
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value, clippy::unnecessary_wraps, clippy::unused_self)]
#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Service {
    fn notify(
        &self, app_name: String, replaces_id: u32, app_icon: String, summary: String, body: String,
        actions: Vec<String>, hint_map: HashMap<String, OwnedValue>, expire_timeout: i32,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<u32> {
        let urgency = hints::urgency(&hint_map);
        let (buttons, has_default, has_settings, reply_label) = split_actions(&actions);
        let reply = reply_label.map(|label| Reply {
            key: crate::model::ACTION_INLINE_REPLY.to_string(),
            label,
            placeholder: hints::string(&hint_map, "x-kde-reply-placeholder-text"),
            secret: false,
        });

        let display_name = hints::string(&hint_map, "x-kde-display-appname");
        let app_name = if display_name.is_empty() { app_name } else { display_name };

        let mut notification = Notification {
            id: replaces_id,
            sender: Self::sender(&header),
            app_name,
            desktop_entry: hints::string(&hint_map, "desktop-entry"),
            app_icon,
            summary: plain_text(&summary),
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
            sound: hints::sound(&hint_map),
        };

        let replacing = {
            let mut inner = self.lock();
            let replacing = inner.replaceable(replaces_id);
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
            inner.live.insert(notification.id);
            inner.last = Some((notification.clone(), Instant::now()));
            replacing
        };

        let id = notification.id;
        let event = if replacing { NotifyEvent::Replaced } else { NotifyEvent::Added };
        (self.sink)(event(Box::new(notification)));
        Ok(id)
    }

    fn close_notification(&self, id: u32) -> zbus::fdo::Result<()> {
        self.lock().forget(id);
        (self.sink)(NotifyEvent::Closed(id, CloseReason::Revoked));
        Ok(())
    }

    fn get_capabilities(&self) -> Vec<String> {
        capabilities()
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        ("Glide".to_string(), "glide".to_string(), env!("CARGO_PKG_VERSION").to_string(), "1.3".to_string())
    }

    fn inhibit(
        &self, desktop_entry: String, reason: String, hints: HashMap<String, OwnedValue>,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<u32> {
        drop(hints);
        let (cookie, became_inhibited) = self.lock().hold(Self::sender(&header), Inhibitor { desktop_entry, reason });
        if became_inhibited {
            (self.sink)(NotifyEvent::InhibitedChanged(true));
        }
        Ok(cookie)
    }

    fn un_inhibit(&self, cookie: u32, #[zbus(header)] header: zbus::message::Header<'_>) -> zbus::fdo::Result<()> {
        let released = self.lock().release(&Self::sender(&header), cookie);
        if released {
            (self.sink)(NotifyEvent::InhibitedChanged(false));
        }
        Ok(())
    }

    #[zbus(property)]
    fn inhibited(&self) -> bool {
        !self.lock().inhibitors.is_empty()
    }
}

fn first_non_empty(hint_map: &HashMap<String, OwnedValue>, keys: &[&str]) -> String {
    keys.iter().map(|key| hints::string(hint_map, key)).find(|value| !value.is_empty()).unwrap_or_default()
}

pub struct NotifyHandle {
    connection: zbus::blocking::Connection,
    inner: Shared,
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
            zbus::message::Message::signal(OBJECT_PATH, BUS_NAME, member).and_then(|builder| match destination {
                Some(destination) => builder.destination(destination)?.build(body),
                None => builder.build(body),
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
        lock_shared(&self.inner).forget(id);
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
    let inner: Shared = Arc::new(Mutex::new(Inner::new()));
    let service = Service { sink: Arc::clone(&sink), inner: Arc::clone(&inner) };

    let connection =
        zbus::blocking::connection::Builder::session()?.name(BUS_NAME)?.serve_at(OBJECT_PATH, service)?.build()?;

    watch_inhibitor_owners(&connection, Arc::clone(&inner), sink);

    tracing::info!("serving {BUS_NAME} on the session bus");
    Ok(NotifyHandle { connection, inner })
}

fn watch_inhibitor_owners(connection: &zbus::blocking::Connection, inner: Shared, sink: Sink) {
    let signals = zbus::blocking::fdo::DBusProxy::new(connection).and_then(|proxy| proxy.receive_name_owner_changed());
    let signals = match signals {
        Ok(signals) => signals,
        Err(err) => {
            tracing::debug!("inhibitors will not follow vanished clients: {err}");
            return;
        }
    };
    let spawned = std::thread::Builder::new().name("glide-notify-owners".into()).spawn(move || {
        for signal in signals {
            let Ok(args) = signal.args() else {
                continue;
            };
            if args.new_owner().is_some() {
                continue;
            }
            let released = lock_shared(&inner).release_owner(args.name().as_str());
            if released {
                sink(NotifyEvent::InhibitedChanged(false));
            }
        }
    });
    if let Err(err) = spawned {
        tracing::debug!("failed to spawn the inhibitor owner watcher: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> (Service, Arc<Mutex<Vec<NotifyEvent>>>) {
        let log: Arc<Mutex<Vec<NotifyEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink_log = Arc::clone(&log);
        let service = Service {
            sink: Arc::new(move |event| {
                sink_log.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(event);
            }),
            inner: Arc::new(Mutex::new(Inner::new())),
        };
        (service, log)
    }

    fn inhibitor(entry: &str) -> Inhibitor {
        Inhibitor { desktop_entry: entry.to_string(), reason: String::new() }
    }

    #[test]
    fn only_a_live_id_can_be_replaced() {
        let (service, _) = service();
        let mut inner = service.lock();
        assert!(!inner.replaceable(0));
        assert!(!inner.replaceable(7), "an id we never handed out is new");
        let id = inner.allocate();
        inner.live.insert(id);
        assert!(inner.replaceable(id));
        inner.forget(id);
        assert!(!inner.replaceable(id), "a closed id is no longer replaceable");
    }

    #[test]
    fn allocation_skips_ids_that_are_still_live() {
        let (service, _) = service();
        let mut inner = service.lock();
        inner.live.insert(1);
        inner.live.insert(2);
        assert_eq!(inner.allocate(), 3);
    }

    #[test]
    fn an_inhibitor_is_released_only_by_its_owner() {
        let (service, _) = service();
        let mut inner = service.lock();
        let (cookie, became) = inner.hold(":1.5".to_string(), inhibitor("player"));
        assert!(became);
        assert!(!inner.release(":1.9", cookie), "a stranger cannot release it");
        assert_eq!(inner.inhibitors.len(), 1);
        assert!(inner.release(":1.5", cookie));
        assert!(inner.inhibitors.is_empty());
    }

    #[test]
    fn a_vanished_owner_drops_all_of_its_inhibitors() {
        let (service, _) = service();
        let mut inner = service.lock();
        inner.hold(":1.5".to_string(), inhibitor("a"));
        inner.hold(":1.5".to_string(), inhibitor("b"));
        inner.hold(":1.6".to_string(), inhibitor("c"));
        assert!(!inner.release_owner(":1.5"), "another owner still inhibits");
        assert_eq!(inner.inhibitors.len(), 1);
        assert!(inner.release_owner(":1.6"));
        assert!(!inner.release_owner(":1.6"), "nothing left to release");
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
    fn the_legacy_image_path_hint_is_honoured() {
        let hints = HashMap::from([(
            "image_path".to_string(),
            OwnedValue::try_from(zbus::zvariant::Value::Str("/legacy.png".into())).expect("owned"),
        )]);
        assert_eq!(first_non_empty(&hints, &["image-path", "image_path"]), "/legacy.png");
        assert_eq!(first_non_empty(&HashMap::new(), &["image-path"]), "");
    }
}

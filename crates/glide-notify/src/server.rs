use std::{
    collections::HashMap,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Live {
    owner: String,
    app_name: String,
    tag: Option<String>,
    vanished: bool,
}

impl Live {
    fn new(owner: &str, app_name: &str, tag: Option<String>) -> Self {
        Self { owner: owner.to_string(), app_name: app_name.to_string(), tag, vanished: false }
    }

    fn destination(&self) -> Option<&str> {
        (!self.vanished && !self.owner.is_empty()).then_some(self.owner.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Admission {
    New,
    Replacing,
    Repeat(u32),
}

struct Inner {
    next_id: u32,
    next_cookie: u32,
    last: Option<(Notification, Instant)>,
    live: HashMap<u32, Live>,
    inhibitors: HashMap<u32, Held>,
}

impl Inner {
    fn new() -> Self {
        Self { next_id: 1, next_cookie: 1, last: None, live: HashMap::new(), inhibitors: HashMap::new() }
    }

    fn allocate(&mut self) -> u32 {
        let mut id = self.next_id;
        while self.live.contains_key(&id) {
            id = id.checked_add(1).unwrap_or(1);
        }
        self.next_id = id.checked_add(1).unwrap_or(1);
        id
    }

    fn replaceable(&self, replaces_id: u32, sender: &str, app_name: &str) -> bool {
        replaces_id != 0
            && self.live.get(&replaces_id).is_some_and(|live| {
                (!sender.is_empty() && live.owner == sender) || (live.vanished && live.app_name == app_name)
            })
    }

    fn tagged(&self, app_name: &str, tag: &str) -> Option<u32> {
        self.live
            .iter()
            .filter(|(_, live)| live.app_name == app_name && live.tag.as_deref() == Some(tag))
            .map(|(id, _)| *id)
            .min()
    }

    fn target(&self, replaces_id: u32, sender: &str, app_name: &str, tag: Option<&str>) -> Option<u32> {
        if self.replaceable(replaces_id, sender, app_name) {
            return Some(replaces_id);
        }
        tag.and_then(|tag| self.tagged(app_name, tag))
    }

    fn admit(&mut self, notification: &mut Notification, replaces_id: u32, tag: Option<String>) -> Admission {
        let target = self.target(replaces_id, &notification.sender, &notification.app_name, tag.as_deref());
        if let Some(id) = target {
            notification.id = id;
        } else {
            if let Some(previous) = self.repeat_of(notification) {
                return Admission::Repeat(previous);
            }
            notification.id = self.allocate();
        }
        self.live.insert(notification.id, Live::new(&notification.sender, &notification.app_name, tag));
        self.last = Some((notification.clone(), Instant::now()));
        if target.is_some() { Admission::Replacing } else { Admission::New }
    }

    fn repeat_of(&self, notification: &Notification) -> Option<u32> {
        let (previous, at) = self.last.as_ref()?;
        (at.elapsed() < FLOOD_WINDOW && previous.is_duplicate_of(notification) && self.live.contains_key(&previous.id))
            .then_some(previous.id)
    }

    fn forget(&mut self, id: u32) -> Option<Live> {
        self.live.remove(&id)
    }

    fn owner_gone(&mut self, name: &str) {
        for live in self.live.values_mut().filter(|live| live.owner == name) {
            live.vanished = true;
        }
    }

    fn revoke(&mut self, id: u32, sender: &str) -> bool {
        let owned = self.live.get(&id).is_some_and(|live| !sender.is_empty() && live.owner == sender);
        if owned {
            self.live.remove(&id);
        }
        owned
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

    fn hold_inhibitor(&self, owner: String, inhibitor: Inhibitor) -> (u32, bool) {
        let mut inner = self.lock();
        let (cookie, became_inhibited) = inner.hold(owner, inhibitor);
        if became_inhibited {
            (self.sink)(NotifyEvent::InhibitedChanged(true));
        }
        (cookie, became_inhibited)
    }

    fn release_inhibitor(&self, owner: &str, cookie: u32) -> bool {
        let mut inner = self.lock();
        let released = inner.release(owner, cookie);
        if released {
            (self.sink)(NotifyEvent::InhibitedChanged(false));
        }
        released
    }
}

fn first_string(hint_map: &HashMap<String, OwnedValue>, keys: &[&str]) -> String {
    keys.iter().map(|key| hints::string(hint_map, key)).find(|value| !value.is_empty()).unwrap_or_default()
}

fn release_owner(inner: &Shared, sink: &Sink, owner: &str) -> bool {
    let mut inner = lock_shared(inner);
    let released = inner.release_owner(owner);
    if released {
        sink(NotifyEvent::InhibitedChanged(false));
    }
    released
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
        let image_path = first_string(&hint_map, &["image-path", "image_path"]);
        let image_data =
            hints::image(&hint_map, "image-data").or_else(|| hints::image(&hint_map, "image_data")).or_else(|| {
                (image_path.is_empty() && app_icon.trim().is_empty())
                    .then(|| hints::image(&hint_map, "icon_data"))
                    .flatten()
            });
        let tag = Some(first_string(&hint_map, &["x-canonical-private-synchronous", "x-dunst-stack-tag"]))
            .filter(|tag| !tag.trim().is_empty());

        let mut notification = Notification {
            id: replaces_id,
            sender: Self::sender(&header),
            app_name,
            desktop_entry: hints::string(&hint_map, "desktop-entry"),
            origin_device: hints::string(&hint_map, "x-glide-origin-device"),
            app_icon,
            image_path,
            summary: plain_text(&summary),
            body: sanitize_body(&body),
            actions: buttons,
            has_default_action: has_default,
            has_settings_action: has_settings,
            reply,
            urgency,
            category: hints::string(&hint_map, "category"),
            resident: hints::boolean(&hint_map, "resident"),
            transient: hints::boolean(&hint_map, "transient"),
            timeout: resolve_timeout(expire_timeout, urgency),
            sound: hints::sound(&hint_map),
            image_data,
        };

        let replacing = match self.lock().admit(&mut notification, replaces_id, tag) {
            Admission::Repeat(previous) => return Ok(previous),
            Admission::New => false,
            Admission::Replacing => true,
        };

        let id = notification.id;
        let event = if replacing { NotifyEvent::Replaced } else { NotifyEvent::Added };
        (self.sink)(event(Box::new(notification)));
        Ok(id)
    }

    async fn close_notification(
        &self, id: u32, #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = Self::sender(&header);
        let revoked = self.lock().revoke(id, &sender);
        if revoked {
            let reason = CloseReason::Revoked;
            let signal = zbus::message::Message::signal(OBJECT_PATH, BUS_NAME, "NotificationClosed")
                .and_then(|builder| builder.destination(sender.as_str())?.build(&(id, reason.as_wire())));
            match signal {
                Ok(signal) => {
                    if let Err(err) = emitter.connection().send(&signal).await {
                        tracing::debug!("failed to emit NotificationClosed: {err}");
                    }
                }
                Err(err) => tracing::debug!("failed to build NotificationClosed: {err}"),
            }
            (self.sink)(NotifyEvent::Closed(id, reason));
        }
        Ok(())
    }

    fn get_capabilities(&self) -> Vec<String> {
        capabilities()
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        ("Glide".to_string(), "glide".to_string(), env!("CARGO_PKG_VERSION").to_string(), "1.3".to_string())
    }

    async fn inhibit(
        &self, desktop_entry: String, reason: String, hints: HashMap<String, OwnedValue>,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::fdo::Result<u32> {
        drop(hints);
        let (cookie, became_inhibited) =
            self.hold_inhibitor(Self::sender(&header), Inhibitor { desktop_entry, reason });
        if became_inhibited && let Err(err) = self.inhibited_changed(&emitter).await {
            tracing::debug!("failed to emit Inhibited change: {err}");
        }
        Ok(cookie)
    }

    async fn un_inhibit(
        &self, cookie: u32, #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        if self.release_inhibitor(&Self::sender(&header), cookie)
            && let Err(err) = self.inhibited_changed(&emitter).await
        {
            tracing::debug!("failed to emit Inhibited change: {err}");
        }
        Ok(())
    }

    #[zbus(property)]
    fn inhibited(&self) -> bool {
        !self.lock().inhibitors.is_empty()
    }
}

pub struct NotifyHandle {
    outbox: std::sync::mpsc::Sender<zbus::message::Message>,
    inner: Shared,
}

impl std::fmt::Debug for NotifyHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("NotifyHandle")
    }
}

impl NotifyHandle {
    fn emit<B>(&self, member: &str, destination: &str, body: &B)
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        let built = zbus::message::Message::signal(OBJECT_PATH, BUS_NAME, member)
            .and_then(|builder| builder.destination(destination)?.build(body));
        match built {
            Ok(message) => {
                if self.outbox.send(message).is_err() {
                    tracing::debug!("failed to emit {member}: the notification sender has stopped");
                }
            }
            Err(err) => tracing::debug!("failed to build {member}: {err}"),
        }
    }

    pub fn closed(&self, id: u32, reason: CloseReason) {
        let Some(live) = lock_shared(&self.inner).forget(id) else {
            return;
        };
        if let Some(destination) = live.destination() {
            self.emit("NotificationClosed", destination, &(id, reason.as_wire()));
        }
    }

    pub fn invoked(&self, id: u32, destination: &str, action: &str, token: Option<&str>) {
        let destination = self.recipient(id, destination);
        if destination.is_empty() {
            return;
        }
        if let Some(token) = token {
            self.emit("ActivationToken", &destination, &(id, token));
        }
        self.emit("ActionInvoked", &destination, &(id, action));
    }

    pub fn replied(&self, id: u32, destination: &str, text: &str) {
        let destination = self.recipient(id, destination);
        if destination.is_empty() {
            return;
        }
        self.emit("NotificationReplied", &destination, &(id, text));
    }

    fn recipient(&self, id: u32, sender: &str) -> String {
        match lock_shared(&self.inner).live.get(&id) {
            Some(live) => live.destination().unwrap_or_default().to_string(),
            None => sender.to_string(),
        }
    }
}

pub fn start(sink: Sink) -> anyhow::Result<NotifyHandle> {
    let inner: Shared = Arc::new(Mutex::new(Inner::new()));
    let service = Service { sink: Arc::clone(&sink), inner: Arc::clone(&inner) };

    let connection = zbus::blocking::connection::Builder::session()?.serve_at(OBJECT_PATH, service)?.build()?;
    claim_name(&connection)?;

    watch_owners(&connection, Arc::clone(&inner), sink);
    let outbox = start_sender(connection)?;

    tracing::info!("serving {BUS_NAME} on the session bus");
    Ok(NotifyHandle { outbox, inner })
}

fn claim_name(connection: &zbus::blocking::Connection) -> anyhow::Result<()> {
    use zbus::fdo::{RequestNameFlags, RequestNameReply};
    let flags = RequestNameFlags::ReplaceExisting | RequestNameFlags::DoNotQueue;
    match connection.request_name_with_flags(BUS_NAME, flags) {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => Ok(()),
        Ok(_) | Err(zbus::Error::NameTaken) => anyhow::bail!("another notification daemon owns {BUS_NAME}"),
        Err(err) => Err(err.into()),
    }
}

fn start_sender(
    connection: zbus::blocking::Connection,
) -> anyhow::Result<std::sync::mpsc::Sender<zbus::message::Message>> {
    let (outbox, messages) = std::sync::mpsc::channel::<zbus::message::Message>();
    std::thread::Builder::new().name("glide-notify-signals".into()).spawn(move || {
        for message in messages {
            if let Err(err) = connection.send(&message) {
                tracing::debug!("failed to emit a notification signal: {err}");
            }
        }
    })?;
    Ok(outbox)
}

fn watch_owners(connection: &zbus::blocking::Connection, inner: Shared, sink: Sink) {
    let signals = zbus::blocking::fdo::DBusProxy::new(connection).and_then(|proxy| proxy.receive_name_owner_changed());
    let signals = match signals {
        Ok(signals) => signals,
        Err(err) => {
            tracing::debug!("notifications and inhibitors will not follow vanished clients: {err}");
            return;
        }
    };
    let interface = connection.object_server().interface::<_, Service>(OBJECT_PATH);
    let own = connection.unique_name().map(|name| name.as_str().to_string());
    let connection = connection.clone();
    let spawned = std::thread::Builder::new().name("glide-notify-owners".into()).spawn(move || {
        for signal in signals {
            let Ok(args) = signal.args() else {
                continue;
            };
            let new_owner = args.new_owner().as_ref().map(|owner| owner.as_str().to_string());
            if args.name().as_str() == BUS_NAME {
                if new_owner != own {
                    tracing::warn!("lost {BUS_NAME} to {new_owner:?}; asking for it back");
                    if let Err(err) = claim_name(&connection) {
                        tracing::warn!("notifications stay unavailable: {err}");
                    }
                }
                continue;
            }
            if new_owner.is_some() {
                continue;
            }
            lock_shared(&inner).owner_gone(args.name().as_str());
            if release_owner(&inner, &sink, args.name().as_str())
                && let Ok(interface) = interface.as_ref()
            {
                let service = interface.get();
                if let Err(err) = zbus::block_on(service.inhibited_changed(interface.signal_emitter())) {
                    tracing::debug!("failed to emit Inhibited change: {err}");
                }
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

    fn live(owner: &str) -> Live {
        Live::new(owner, "app", None)
    }

    #[test]
    fn only_a_live_id_can_be_replaced() {
        let (service, _) = service();
        let mut inner = service.lock();
        assert!(!inner.replaceable(0, ":1.5", "app"));
        assert!(!inner.replaceable(7, ":1.5", "app"), "an id we never handed out is new");
        let id = inner.allocate();
        inner.live.insert(id, live(":1.5"));
        assert!(inner.replaceable(id, ":1.5", "app"));
        inner.forget(id);
        assert!(!inner.replaceable(id, ":1.5", "app"), "a closed id is no longer replaceable");
    }

    #[test]
    fn another_client_cannot_replace_a_notification() {
        let (service, _) = service();
        let mut inner = service.lock();
        let id = inner.allocate();
        inner.live.insert(id, live(":1.5"));
        assert!(!inner.replaceable(id, ":1.9", "app"));
        assert!(!inner.replaceable(id, "", "app"));
        assert!(inner.replaceable(id, ":1.5", "app"));
    }

    #[test]
    fn a_vanished_owner_hands_its_notification_to_a_later_process_of_the_same_app() {
        let (service, _) = service();
        let mut inner = service.lock();
        let id = inner.allocate();
        inner.live.insert(id, live(":1.5"));
        assert!(!inner.replaceable(id, ":1.9", "app"), "the owner is still on the bus");
        inner.owner_gone(":1.5");
        assert!(inner.replaceable(id, ":1.9", "app"), "notify-send -r from a new process");
        assert!(!inner.replaceable(id, ":1.9", "other"), "another app still cannot take it over");
        assert_eq!(inner.live[&id].destination(), None, "signals for it go nowhere");
    }

    #[test]
    fn a_stack_tag_replaces_the_same_apps_tagged_notification() {
        let (service, _) = service();
        let mut inner = service.lock();
        let id = inner.allocate();
        inner.live.insert(id, Live::new(":1.5", "volume", Some("vol".into())));
        assert_eq!(inner.target(0, ":1.9", "volume", Some("vol")), Some(id));
        assert_eq!(inner.target(0, ":1.9", "volume", Some("brightness")), None);
        assert_eq!(inner.target(0, ":1.9", "other", Some("vol")), None);
        assert_eq!(inner.target(0, ":1.9", "volume", None), None);
        assert_eq!(inner.target(999, ":1.9", "volume", Some("vol")), Some(id), "a stale replaces_id falls back");
    }

    fn sent(service: &Service, sender: &str, summary: &str, replaces_id: u32, tag: Option<&str>) -> u32 {
        let mut notification = Notification {
            sender: sender.to_string(),
            app_name: "app".to_string(),
            summary: summary.to_string(),
            ..Notification::default()
        };
        match service.lock().admit(&mut notification, replaces_id, tag.map(str::to_string)) {
            Admission::Repeat(previous) => previous,
            Admission::New | Admission::Replacing => notification.id,
        }
    }

    #[test]
    fn an_identical_repeat_returns_the_live_id_instead_of_an_error() {
        let (service, _) = service();
        let first = sent(&service, ":1.5", "Build finished", 0, None);
        assert_eq!(sent(&service, ":1.5", "Build finished", 0, None), first);
        assert_ne!(sent(&service, ":1.5", "Build failed", 0, None), first);
        let again = sent(&service, ":1.5", "Same", 0, None);
        service.lock().forget(again);
        assert_ne!(sent(&service, ":1.5", "Same", 0, None), again, "a closed repeat is shown afresh");
    }

    #[test]
    fn tagged_and_vanished_notifications_are_replaced_in_place_across_processes() {
        let (service, _) = service();
        let volume = sent(&service, ":1.5", "Volume 40%", 0, Some("vol"));
        assert_eq!(sent(&service, ":1.6", "Volume 45%", 0, Some("vol")), volume);
        let plain = sent(&service, ":1.7", "Hello", 0, None);
        assert_ne!(sent(&service, ":1.8", "Hello again", plain, None), plain, "the first process is still here");
        service.lock().owner_gone(":1.7");
        assert_eq!(sent(&service, ":1.9", "Hello once more", plain, None), plain);
    }

    #[test]
    fn only_the_owner_can_close_a_notification() {
        let (service, _) = service();
        let mut inner = service.lock();
        let id = inner.allocate();
        inner.live.insert(id, live(":1.5"));
        assert!(!inner.revoke(id, ":1.9"), "a stranger cannot close it");
        assert!(inner.live.contains_key(&id));
        assert!(!inner.revoke(u32::MAX - 3000, ":1.5"), "ids that are not live are never forwarded");
        assert!(inner.revoke(id, ":1.5"));
        assert!(!inner.live.contains_key(&id));
        assert!(!inner.revoke(id, ":1.5"), "a closed id is gone");
    }

    #[test]
    fn allocation_skips_ids_that_are_still_live() {
        let (service, _) = service();
        let mut inner = service.lock();
        inner.live.insert(1, live(":1.5"));
        inner.live.insert(2, live(":1.6"));
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
    fn inhibition_events_follow_the_inhibitor_map() {
        let (service, log) = service();
        let (first, became) = service.hold_inhibitor(":1.5".to_string(), inhibitor("a"));
        assert!(became);
        let (_, became) = service.hold_inhibitor(":1.6".to_string(), inhibitor("b"));
        assert!(!became, "a second inhibitor changes nothing");
        assert!(!service.release_inhibitor(":1.5", first), "another owner still inhibits");
        assert!(release_owner(&service.inner, &service.sink, ":1.6"));
        let (_, became) = service.hold_inhibitor(":1.7".to_string(), inhibitor("c"));
        assert!(became);
        let events = log.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
        assert_eq!(
            events,
            vec![
                NotifyEvent::InhibitedChanged(true),
                NotifyEvent::InhibitedChanged(false),
                NotifyEvent::InhibitedChanged(true)
            ]
        );
    }
}

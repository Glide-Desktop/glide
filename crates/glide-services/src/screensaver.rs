use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

use zbus::{
    blocking::{Connection, MessageIterator, connection::Builder, fdo::DBusProxy},
    fdo::{RequestNameFlags, RequestNameReply},
    message::Header,
    names::BusName,
};

pub const BUS_NAME: &str = "org.freedesktop.ScreenSaver";
const PATHS: [&str; 2] = ["/org/freedesktop/ScreenSaver", "/ScreenSaver"];
const OWNER_CHANGES: &str =
    "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged'";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Inhibited(bool),
    Lock,
    Activity,
}

#[derive(Debug, Default)]
struct Cookies {
    last: u32,
    held: HashMap<u32, (String, String)>,
}

impl Cookies {
    #[cfg(test)]
    fn take(&mut self, owner: &str) -> u32 {
        self.take_for(owner, "")
    }

    fn take_for(&mut self, owner: &str, app: &str) -> u32 {
        loop {
            self.last = self.last.checked_add(1).unwrap_or(1);
            if !self.held.contains_key(&self.last) {
                break;
            }
        }
        self.held.insert(self.last, (owner.to_string(), app.trim().to_string()));
        self.last
    }

    fn release(&mut self, cookie: u32, owner: &str) -> bool {
        let matches = self.held.get(&cookie).is_some_and(|(held, _)| held == owner);
        if matches {
            self.held.remove(&cookie);
        }
        matches
    }

    fn forget(&mut self, owner: &str) -> bool {
        let before = self.held.len();
        self.held.retain(|_, (held, _)| held != owner);
        self.held.len() != before
    }

    fn any(&self) -> bool {
        !self.held.is_empty()
    }

    fn apps(&self) -> Vec<String> {
        let mut apps: Vec<String> =
            self.held.values().map(|(_, app)| app.clone()).filter(|app| !app.is_empty()).collect();
        apps.sort();
        apps.dedup();
        apps
    }
}

struct Shared {
    cookies: Mutex<Cookies>,
    active: AtomicBool,
    sink: Box<dyn Fn(Event) + Send + Sync>,
}

impl Shared {
    fn edit<R>(&self, edit: impl FnOnce(&mut Cookies) -> R) -> R {
        let mut cookies = self.cookies.lock().unwrap_or_else(PoisonError::into_inner);
        let before = cookies.any();
        let result = edit(&mut cookies);
        let after = cookies.any();
        drop(cookies);
        if before != after {
            (self.sink)(Event::Inhibited(after));
        }
        result
    }
}

struct ScreenSaver(Arc<Shared>);

fn sender(header: &Header<'_>) -> String {
    header.sender().map(ToString::to_string).unwrap_or_default()
}

// Signatures are fixed by the org.freedesktop.ScreenSaver D-Bus interface.
#[allow(clippy::needless_pass_by_value, clippy::unused_self)]
#[zbus::interface(name = "org.freedesktop.ScreenSaver")]
impl ScreenSaver {
    fn inhibit(&self, application_name: &str, reason_for_inhibit: &str, #[zbus(header)] header: Header<'_>) -> u32 {
        let owner = sender(&header);
        tracing::info!(app = application_name, reason = reason_for_inhibit, "an app is keeping the screen on");
        self.0.edit(|cookies| cookies.take_for(&owner, application_name))
    }

    fn un_inhibit(&self, cookie: u32, #[zbus(header)] header: Header<'_>) {
        let owner = sender(&header);
        if !self.0.edit(|cookies| cookies.release(cookie, &owner)) {
            tracing::debug!(cookie, "ignoring a screen saver cookie its sender does not hold");
        }
    }

    fn throttle(&self, application_name: &str, reason_for_inhibit: &str) -> u32 {
        tracing::debug!(
            app = application_name,
            reason = reason_for_inhibit,
            "screen saver throttling is not supported"
        );
        0
    }

    fn un_throttle(&self, cookie: u32) {
        tracing::debug!(cookie, "screen saver throttling is not supported");
    }

    fn lock(&self) {
        (self.0.sink)(Event::Lock);
    }

    fn simulate_user_activity(&self) {
        (self.0.sink)(Event::Activity);
    }

    fn get_active(&self) -> bool {
        self.0.active.load(Ordering::Relaxed)
    }

    fn set_active(&self, active: bool) -> bool {
        if active {
            (self.0.sink)(Event::Lock);
        }
        active
    }

    fn get_active_time(&self) -> u32 {
        0
    }

    fn get_session_idle_time(&self) -> u32 {
        0
    }
}

pub struct Handle {
    inhibited: bool,
    active: bool,
    changes: mpsc::Sender<bool>,
    shared: Arc<Shared>,
}

impl Handle {
    #[must_use]
    pub const fn inhibited(&self) -> bool {
        self.inhibited
    }

    #[must_use]
    pub fn apps(&self) -> Vec<String> {
        self.shared.cookies.lock().unwrap_or_else(PoisonError::into_inner).apps()
    }

    pub const fn set_inhibited(&mut self, inhibited: bool) {
        self.inhibited = inhibited;
    }

    pub fn set_active(&mut self, active: bool) {
        if std::mem::replace(&mut self.active, active) != active && self.changes.send(active).is_err() {
            tracing::debug!(active, "the screen saver service is gone; its state was not announced");
        }
    }
}

fn vanished_owner(message: &zbus::Message) -> Option<String> {
    let header = message.header();
    let relevant = header.message_type() == zbus::message::Type::Signal
        && header.sender().is_some_and(|sender| sender.as_str() == "org.freedesktop.DBus")
        && header.interface().is_some_and(|name| name.as_str() == "org.freedesktop.DBus")
        && header.member().is_some_and(|name| name.as_str() == "NameOwnerChanged");
    if !relevant {
        return None;
    }
    let (name, _, new_owner) = message.body().deserialize::<(String, String, String)>().ok()?;
    new_owner.is_empty().then_some(name)
}

fn announce(connection: &Connection, shared: &Shared, changes: mpsc::Receiver<bool>) {
    for active in changes {
        shared.active.store(active, Ordering::Relaxed);
        for path in PATHS {
            if let Err(err) = connection.emit_signal(None::<BusName<'_>>, path, BUS_NAME, "ActiveChanged", &(active,)) {
                tracing::debug!(?err, "failed to announce the screen saver state");
            }
        }
    }
}

pub fn start(sink: impl Fn(Event) + Send + Sync + 'static) -> anyhow::Result<Handle> {
    serve(Builder::session()?, Box::new(sink))
}

fn serve(mut builder: Builder<'_>, sink: Box<dyn Fn(Event) + Send + Sync>) -> anyhow::Result<Handle> {
    let shared = Arc::new(Shared { cookies: Mutex::default(), active: AtomicBool::new(false), sink });
    for path in PATHS {
        builder = builder.serve_at(path, ScreenSaver(shared.clone()))?;
    }
    let connection = builder.build()?;
    match connection.request_name_with_flags(BUS_NAME, RequestNameFlags::DoNotQueue.into())? {
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => {}
        _ => anyhow::bail!("another service already owns the name"),
    }
    DBusProxy::new(&connection)?.add_match_rule(zbus::MatchRule::try_from(OWNER_CHANGES)?)?;

    let watcher = (connection.clone(), shared.clone());
    std::thread::Builder::new().name("glide-screensaver".into()).spawn(move || {
        for message in MessageIterator::from(&watcher.0) {
            if let Some(owner) = message.ok().as_ref().and_then(vanished_owner)
                && watcher.1.edit(|cookies| cookies.forget(&owner))
            {
                tracing::info!(owner, "an app holding the screen on went away");
            }
        }
    })?;

    let (changes, receiver) = mpsc::channel();
    let announcer = shared.clone();
    std::thread::Builder::new()
        .name("glide-screensaver-state".into())
        .spawn(move || announce(&connection, &announcer, receiver))?;
    Ok(Handle { inhibited: false, active: false, changes, shared })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_bus::private_bus;
    use std::time::{Duration, Instant};

    fn call<B: serde::Serialize + zbus::zvariant::DynamicType>(
        connection: &Connection, path: &str, method: &str, body: &B,
    ) -> zbus::Message {
        connection.call_method(Some(BUS_NAME), path, Some(BUS_NAME), method, body).expect("the method is served")
    }

    fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if ready() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("timed out waiting for {what}");
    }

    #[test]
    fn apps_inhibit_over_either_object_path_and_lose_the_inhibitor_when_they_disconnect() {
        let Some(daemon) = private_bus() else { return };
        let (sender, events) = mpsc::channel();
        let sender = Mutex::new(sender);
        let sink = Box::new(move |event| {
            let _ = sender.lock().unwrap_or_else(PoisonError::into_inner).send(event);
        });
        let mut handle = serve(daemon.builder(), sink).expect("served");
        let next = || events.recv_timeout(Duration::from_secs(5)).expect("an event arrives");

        let player = daemon.client();
        let cookie: u32 =
            call(&player, PATHS[0], "Inhibit", &("player", "video")).body().deserialize().expect("a cookie");
        assert_ne!(cookie, 0);
        assert_eq!(next(), Event::Inhibited(true));

        let forger = daemon.client();
        let victim = player.unique_name().expect("the player has a unique name").to_string();
        forger
            .emit_signal(
                Some(BUS_NAME),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "NameOwnerChanged",
                &(victim.as_str(), victim.as_str(), ""),
            )
            .expect("any peer may send a unicast signal");
        assert!(
            events.recv_timeout(Duration::from_millis(300)).is_err(),
            "a NameOwnerChanged forged by another peer must not lift the player's inhibitor"
        );
        drop(forger);

        let browser = daemon.client();
        call(&browser, PATHS[1], "Inhibit", &("browser", "call"));
        call(&browser, PATHS[1], "UnInhibit", &(cookie,));
        call(&player, PATHS[0], "UnInhibit", &(cookie,));
        assert!(events.try_recv().is_err(), "the browser still holds its own inhibitor");
        drop(browser);
        assert_eq!(next(), Event::Inhibited(false), "a vanished client releases what it held");

        call(&player, PATHS[1], "Lock", &());
        assert_eq!(next(), Event::Lock);
        call(&player, PATHS[0], "SimulateUserActivity", &());
        assert_eq!(next(), Event::Activity);

        let active = || call(&player, PATHS[0], "GetActive", &()).body().deserialize::<bool>().expect("a flag");
        assert!(!active());
        handle.set_active(true);
        wait_until("the active state to be published", active);

        let second = serve(daemon.builder(), Box::new(|_| {}));
        assert!(second.is_err(), "a second owner must fail loudly instead of serving paths nobody routes to");
    }

    #[test]
    fn cookies_are_never_zero_and_only_their_owner_can_release_them() {
        let mut cookies = Cookies::default();
        let first = cookies.take(":1.10");
        let second = cookies.take(":1.11");
        assert!(first != 0 && second != 0 && first != second);
        assert!(!cookies.release(first, ":1.11"), "another client cannot lift the inhibitor");
        assert!(!cookies.release(999, ":1.10"));
        assert!(cookies.release(first, ":1.10"));
        assert!(cookies.any());
        assert!(cookies.release(second, ":1.11"));
        assert!(!cookies.any());
    }

    #[test]
    fn inhibitors_are_named_by_the_app_that_asked() {
        let mut cookies = Cookies::default();
        let video = cookies.take_for(":1.10", "Firefox");
        cookies.take_for(":1.11", " Firefox ");
        cookies.take_for(":1.12", "");
        cookies.take_for(":1.13", "mpv");
        assert_eq!(cookies.apps(), ["Firefox", "mpv"], "unnamed ones are left out and names appear once");
        cookies.release(video, ":1.10");
        cookies.forget(":1.11");
        assert_eq!(cookies.apps(), ["mpv"]);
    }

    #[test]
    fn a_wrapped_counter_skips_zero_and_cookies_still_held() {
        let mut cookies = Cookies { last: u32::MAX - 1, ..Cookies::default() };
        assert_eq!(cookies.take("a"), u32::MAX);
        assert_eq!(cookies.take("a"), 1);
        cookies.last = u32::MAX - 1;
        assert_eq!(cookies.take("a"), 2, "u32::MAX and 1 are still held");
    }

    #[test]
    fn a_client_that_disconnects_drops_all_its_inhibitors() {
        let mut cookies = Cookies::default();
        cookies.take(":1.10");
        cookies.take(":1.10");
        let kept = cookies.take(":1.11");
        assert!(cookies.forget(":1.10"));
        assert!(!cookies.forget(":1.10"));
        assert!(cookies.release(kept, ":1.11"));
        assert!(!cookies.any());
    }

    #[test]
    fn the_inhibited_event_fires_only_when_the_first_cookie_comes_or_the_last_goes() {
        let (sender, events) = mpsc::channel();
        let sender = Mutex::new(sender);
        let shared = Shared {
            cookies: Mutex::default(),
            active: AtomicBool::new(false),
            sink: Box::new(move |event| {
                let _ = sender.lock().unwrap_or_else(PoisonError::into_inner).send(event);
            }),
        };
        let first = shared.edit(|cookies| cookies.take("a"));
        let second = shared.edit(|cookies| cookies.take("b"));
        shared.edit(|cookies| cookies.release(first, "a"));
        shared.edit(|cookies| cookies.release(second, "b"));
        assert_eq!(events.try_iter().collect::<Vec<_>>(), [Event::Inhibited(true), Event::Inhibited(false)]);
    }

    #[test]
    fn the_handle_announces_each_state_change_once() {
        let (changes, announced) = mpsc::channel();
        let shared =
            Arc::new(Shared { cookies: Mutex::default(), active: AtomicBool::new(false), sink: Box::new(|_| {}) });
        let mut handle = Handle { inhibited: false, active: false, changes, shared };
        for active in [false, true, true, false] {
            handle.set_active(active);
        }
        assert_eq!(announced.try_iter().collect::<Vec<_>>(), [true, false]);
        handle.set_inhibited(true);
        assert!(handle.inhibited());
    }
}

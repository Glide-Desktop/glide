use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use zbus::{
    fdo::{RequestNameFlags, RequestNameReply},
    names::BusName,
    zvariant::OwnedValue,
};

use crate::{
    model::{
        Address, HOST_PREFIX, ITEM_INTERFACE, ItemProps, Registry, WATCHER_INTERFACE, WATCHER_NAME, WATCHER_PATH,
        parse_props,
    },
    watcher::{Watcher, lock},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayItem {
    pub key: String,
    pub address: Address,
    pub props: ItemProps,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    Item(Box<TrayItem>),
    Removed { key: String },
}

pub type Sink = Arc<dyn Fn(TrayEvent) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Method {
    Activate(i32, i32),
    SecondaryActivate(i32, i32),
    ContextMenu(i32, i32),
    Scroll(i32, bool),
}

#[derive(Debug)]
pub(crate) enum Request {
    Register(Address, Option<String>),
    External { service: String, added: bool },
    Vanished(String),
    WatcherOwner(String),
    Changed { sender: String },
    Call { key: String, method: Method },
    Refresh,
}

#[derive(Debug, Clone)]
pub struct TrayHandle {
    tx: mpsc::Sender<Request>,
}

impl TrayHandle {
    fn send(&self, request: Request) {
        if self.tx.send(request).is_err() {
            tracing::debug!("the tray worker has stopped");
        }
    }

    fn call(&self, key: &str, method: Method) {
        self.send(Request::Call { key: key.to_string(), method });
    }

    pub fn activate(&self, key: &str, x: i32, y: i32) {
        self.call(key, Method::Activate(x, y));
    }

    pub fn secondary_activate(&self, key: &str, x: i32, y: i32) {
        self.call(key, Method::SecondaryActivate(x, y));
    }

    pub fn context_menu(&self, key: &str, x: i32, y: i32) {
        self.call(key, Method::ContextMenu(x, y));
    }

    pub fn scroll(&self, key: &str, delta: i32, vertical: bool) {
        self.call(key, Method::Scroll(delta, vertical));
    }

    pub fn refresh(&self) {
        self.send(Request::Refresh);
    }
}

const SIGNAL_RULES: [&str; 3] = [
    "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged'",
    "type='signal',interface='org.kde.StatusNotifierItem'",
    "type='signal',interface='org.kde.StatusNotifierWatcher'",
];

const METHOD_TIMEOUT: Duration = Duration::from_secs(3);
const FIRST_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_mins(2);
const MAX_PENDING: usize = 64;
const COALESCE: Duration = Duration::from_millis(40);
const MAX_BATCH: Duration = Duration::from_millis(200);
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
const PROPERTY_NAMES: [&str; 11] = [
    "Id",
    "Title",
    "Status",
    "IconName",
    "IconPixmap",
    "AttentionIconName",
    "AttentionIconPixmap",
    "IconThemePath",
    "ToolTip",
    "Menu",
    "ItemIsMenu",
];

pub fn start(sink: Sink) -> anyhow::Result<TrayHandle> {
    let (tx, rx) = mpsc::channel();
    let worker_tx = tx.clone();
    std::thread::Builder::new().name("glide-tray".into()).spawn(move || match Worker::connect(sink, worker_tx) {
        Ok(worker) => worker.run(&rx),
        Err(err) => tracing::warn!("the system tray is unavailable: {err}"),
    })?;
    Ok(TrayHandle { tx })
}

fn watcher_owner(connection: &zbus::blocking::Connection) -> Option<String> {
    let bus = zbus::blocking::fdo::DBusProxy::new(connection).ok()?;
    let owner = bus.get_name_owner(BusName::try_from(WATCHER_NAME).ok()?).ok()?;
    Some(owner.to_string())
}

fn trusted(sender: Option<&str>, interface: Option<&str>, watcher: Option<&str>, unique: &str) -> bool {
    match interface {
        Some("org.freedesktop.DBus") => sender == Some("org.freedesktop.DBus"),
        Some(WATCHER_INTERFACE) => sender.is_some() && sender == watcher && sender != Some(unique),
        _ => true,
    }
}

fn listen(connection: &zbus::blocking::Connection, tx: &mpsc::Sender<Request>, unique: &str) {
    let mut watcher = watcher_owner(connection);
    for message in zbus::blocking::MessageIterator::from(connection) {
        let Ok(message) = message else {
            continue;
        };
        let header = message.header();
        if header.message_type() != zbus::message::Type::Signal {
            continue;
        }
        let interface = header.interface().map(|i| i.as_str().to_string());
        let member = header.member().map(|m| m.as_str().to_string());
        let sender = header.sender().map(|s| s.as_str().to_string());
        if !trusted(sender.as_deref(), interface.as_deref(), watcher.as_deref(), unique) {
            continue;
        }
        let request = match (interface.as_deref(), member.as_deref()) {
            (Some("org.freedesktop.DBus"), Some("NameOwnerChanged")) => {
                let Ok((name, _old, new)) = message.body().deserialize::<(String, String, String)>() else {
                    continue;
                };
                if name == WATCHER_NAME {
                    watcher = (!new.is_empty()).then(|| new.clone());
                    Request::WatcherOwner(new)
                } else if new.is_empty() {
                    Request::Vanished(name)
                } else {
                    continue;
                }
            }
            (Some(ITEM_INTERFACE), _) => match sender {
                Some(sender) => Request::Changed { sender },
                None => continue,
            },
            (
                Some(WATCHER_INTERFACE),
                Some(member @ ("StatusNotifierItemRegistered" | "StatusNotifierItemUnregistered")),
            ) => {
                let Ok(service) = message.body().deserialize::<String>() else {
                    continue;
                };
                Request::External { service, added: member == "StatusNotifierItemRegistered" }
            }
            _ => continue,
        };
        if tx.send(request).is_err() {
            return;
        }
    }
}

fn collect_batch(
    first: Request, rx: &mpsc::Receiver<Request>, quiet: Duration, limit: Duration,
) -> (BTreeSet<String>, Vec<Request>) {
    let deadline = Instant::now() + limit;
    let mut changed = BTreeSet::new();
    let mut pending = Vec::new();
    let mut next = Some(first);
    while let Some(request) = next.take() {
        match request {
            Request::Changed { sender } => {
                changed.insert(sender);
            }
            other => {
                pending.push(other);
                break;
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        next = rx.recv_timeout(quiet.min(remaining)).ok();
    }
    (changed, pending)
}

fn next_backoff(previous: Option<Duration>) -> Duration {
    previous.map_or(FIRST_BACKOFF, |previous| previous.saturating_mul(2).clamp(FIRST_BACKOFF, MAX_BACKOFF))
}

fn stalls(err: &zbus::Error) -> bool {
    match err {
        zbus::Error::InputOutput(io) => io.kind() == std::io::ErrorKind::TimedOut,
        zbus::Error::MethodError(name, _, _) => matches!(
            name.as_str(),
            "org.freedesktop.DBus.Error.NoReply"
                | "org.freedesktop.DBus.Error.Timeout"
                | "org.freedesktop.DBus.Error.TimedOut"
        ),
        zbus::Error::FDO(fdo) => {
            matches!(**fdo, zbus::fdo::Error::NoReply(_) | zbus::fdo::Error::Timeout(_) | zbus::fdo::Error::TimedOut(_))
        }
        _ => false,
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Stall {
    until: Option<Instant>,
    backoff: Option<Duration>,
}

impl Stall {
    fn active(&self, now: Instant) -> bool {
        self.until.is_some_and(|until| now < until)
    }

    fn record(&mut self, err: &zbus::Error, now: Instant) {
        if stalls(err) {
            let backoff = next_backoff(self.backoff);
            self.backoff = Some(backoff);
            self.until = Some(now + backoff);
        }
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

struct Pending {
    address: Address,
    owner: String,
    stall: Stall,
}

fn remember(pending: &mut Vec<Pending>, entry: Pending) {
    pending.retain(|p| p.address != entry.address);
    if pending.len() >= MAX_PENDING {
        pending.remove(0);
    }
    pending.push(entry);
}

struct Tracked {
    address: Address,
    owner: String,
    props: ItemProps,
    stall: Stall,
}

impl Tracked {
    fn event(&self) -> TrayEvent {
        TrayEvent::Item(Box::new(TrayItem {
            key: self.address.key(),
            address: self.address.clone(),
            props: self.props.clone(),
        }))
    }
}

struct Worker {
    connection: zbus::blocking::Connection,
    sink: Sink,
    registry: Arc<Mutex<Registry>>,
    host: String,
    unique: String,
    watcher: bool,
    items: Vec<Tracked>,
    pending: Vec<Pending>,
}

impl Worker {
    fn connect(sink: Sink, tx: mpsc::Sender<Request>) -> anyhow::Result<Self> {
        let connection = zbus::blocking::connection::Builder::session()?.method_timeout(METHOD_TIMEOUT).build()?;
        let bus = zbus::blocking::fdo::DBusProxy::new(&connection)?;
        for rule in SIGNAL_RULES {
            if let Ok(rule) = zbus::MatchRule::try_from(rule)
                && let Err(err) = bus.add_match_rule(rule)
            {
                tracing::debug!("tray signal match failed: {err}");
            }
        }
        let unique = connection.unique_name().map(ToString::to_string).unwrap_or_default();
        {
            let connection = connection.clone();
            let tx = tx.clone();
            let unique = unique.clone();
            std::thread::Builder::new()
                .name("glide-tray-signals".into())
                .spawn(move || listen(&connection, &tx, &unique))?;
        }
        let registry = Arc::new(Mutex::new(Registry::default()));
        connection.object_server().at(WATCHER_PATH, Watcher { registry: registry.clone(), tx })?;
        let host = format!("{HOST_PREFIX}-{}", std::process::id());
        if let Err(err) = connection.request_name(host.as_str()) {
            tracing::debug!("could not own {host}: {err}");
        }
        let mut worker =
            Self { connection, sink, registry, host, unique, watcher: false, items: Vec::new(), pending: Vec::new() };
        if !worker.claim() {
            worker.join();
        }
        Ok(worker)
    }

    fn run(mut self, rx: &mpsc::Receiver<Request>) {
        while let Ok(first) = rx.recv() {
            let (changed, pending) = collect_batch(first, rx, COALESCE, MAX_BATCH);
            for sender in changed {
                self.refresh_owner(&sender);
            }
            for request in pending {
                self.handle(request);
            }
        }
    }

    fn handle(&mut self, request: Request) {
        match request {
            Request::Register(address, sender) => self.register(address, sender.as_deref()),
            Request::External { service, added } => {
                if self.watcher {
                    return;
                }
                let Some(address) = Address::parse(&service, None) else {
                    return;
                };
                if added {
                    self.register(address, None);
                } else {
                    self.pending.retain(|p| p.address != address);
                    self.forget(|tracked| tracked.address == address);
                }
            }
            Request::Vanished(name) => {
                if self.watcher {
                    let gone = lock(&self.registry).vanish(&name);
                    for address in gone {
                        self.emit("StatusNotifierItemUnregistered", &address.key());
                    }
                }
                self.pending.retain(|p| p.owner != name && p.address.bus != name);
                self.forget(|tracked| tracked.owner == name || tracked.address.bus == name);
            }
            Request::WatcherOwner(owner) => {
                if self.watcher || owner == self.unique {
                    return;
                }
                if owner.is_empty() {
                    self.claim();
                } else {
                    self.join();
                }
            }
            Request::Changed { sender } => self.refresh_owner(&sender),
            Request::Call { key, method } => self.call(&key, method),
            Request::Refresh => {
                for tracked in &self.items {
                    (self.sink)(tracked.event());
                }
            }
        }
    }

    fn emit(&self, member: &str, body: &str) {
        let sent = self.connection.emit_signal(None::<BusName<'_>>, WATCHER_PATH, WATCHER_INTERFACE, member, &(body,));
        if let Err(err) = sent {
            tracing::debug!("tray signal {member} failed: {err}");
        }
    }

    fn claim(&mut self) -> bool {
        let reply = self.connection.request_name_with_flags(WATCHER_NAME, RequestNameFlags::DoNotQueue.into());
        match reply {
            Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {
                self.watcher = true;
                {
                    let mut registry = lock(&self.registry);
                    registry.add_host(&self.host);
                    for tracked in &self.items {
                        registry.add_item(tracked.address.clone());
                    }
                }
                let _ = self.connection.emit_signal(
                    None::<BusName<'_>>,
                    WATCHER_PATH,
                    WATCHER_INTERFACE,
                    "StatusNotifierHostRegistered",
                    &(),
                );
                tracing::info!("serving {WATCHER_NAME}");
                true
            }
            Ok(reply) => {
                tracing::debug!(?reply, "{WATCHER_NAME} is owned elsewhere");
                false
            }
            Err(err) => {
                tracing::info!("{WATCHER_NAME} is owned elsewhere ({err}); acting as a host");
                false
            }
        }
    }

    fn join(&mut self) {
        let registered: zbus::Result<()> = self
            .connection
            .call_method(
                Some(WATCHER_NAME),
                WATCHER_PATH,
                Some(WATCHER_INTERFACE),
                "RegisterStatusNotifierHost",
                &(self.host.as_str(),),
            )
            .map(|_| ());
        if let Err(err) = registered {
            tracing::warn!("could not register as a tray host: {err}");
            return;
        }
        let items = self
            .get(WATCHER_NAME, WATCHER_PATH, WATCHER_INTERFACE, "RegisteredStatusNotifierItems")
            .ok()
            .and_then(|value| Vec::<String>::try_from(value).ok())
            .unwrap_or_default();
        tracing::info!(count = items.len(), "joined the existing tray watcher");
        for service in items {
            if let Some(address) = Address::parse(&service, None) {
                self.register(address, None);
            }
        }
    }

    fn owner_of(&self, bus: &str, sender: Option<&str>) -> String {
        if bus.starts_with(':') {
            return bus.to_string();
        }
        if let Some(sender) = sender.filter(|sender| sender.starts_with(':')) {
            return sender.to_string();
        }
        zbus::blocking::fdo::DBusProxy::new(&self.connection)
            .ok()
            .and_then(|proxy| {
                let name = BusName::try_from(bus).ok()?;
                proxy.get_name_owner(name).ok()
            })
            .map_or_else(|| bus.to_string(), |owner| owner.to_string())
    }

    fn register(&mut self, address: Address, sender: Option<&str>) {
        if let Some(index) = self.items.iter().position(|t| t.address == address) {
            self.refresh(index);
            return;
        }
        let now = Instant::now();
        let (owner, mut stall) = match self.pending.iter().position(|p| p.address == address) {
            Some(index) if self.pending[index].stall.active(now) => return,
            Some(index) => {
                let waiting = self.pending.remove(index);
                (waiting.owner, waiting.stall)
            }
            None => (self.owner_of(&address.bus, sender), Stall::default()),
        };
        match self.fetch(&address) {
            Ok(props) => {
                tracing::info!(key = %address.key(), id = %props.id, "tray item registered");
                let tracked = Tracked { address, owner, props, stall: Stall::default() };
                (self.sink)(tracked.event());
                self.items.push(tracked);
            }
            Err(err) => {
                tracing::debug!(key = %address.key(), "tray item has no readable properties yet: {err}");
                stall.record(&err, Instant::now());
                remember(&mut self.pending, Pending { address, owner, stall });
            }
        }
    }

    fn forget(&mut self, gone: impl Fn(&Tracked) -> bool) {
        let (removed, kept): (Vec<Tracked>, Vec<Tracked>) = std::mem::take(&mut self.items).into_iter().partition(gone);
        self.items = kept;
        for tracked in removed {
            let key = tracked.address.key();
            tracing::info!(%key, "tray item removed");
            if self.watcher && lock(&self.registry).remove_item(&tracked.address) {
                self.emit("StatusNotifierItemUnregistered", &key);
            }
            (self.sink)(TrayEvent::Removed { key });
        }
    }

    fn refresh_owner(&mut self, sender: &str) {
        for index in 0..self.items.len() {
            let tracked = &self.items[index];
            if tracked.owner == sender || tracked.address.bus == sender {
                self.refresh(index);
            }
        }
        let waiting: Vec<Address> = self
            .pending
            .iter()
            .filter(|p| p.owner == sender || p.address.bus == sender)
            .map(|p| p.address.clone())
            .collect();
        for address in waiting {
            self.register(address, None);
        }
    }

    fn refresh(&mut self, index: usize) {
        if self.items[index].stall.active(Instant::now()) {
            return;
        }
        let fetched = self.fetch(&self.items[index].address);
        let tracked = &mut self.items[index];
        match fetched {
            Ok(props) => {
                tracked.stall.clear();
                if props != tracked.props {
                    tracked.props = props;
                    (self.sink)(tracked.event());
                }
            }
            Err(err) => {
                tracked.stall.record(&err, Instant::now());
                let key = tracked.address.key();
                tracing::debug!(%key, "tray item refresh failed: {err}");
            }
        }
    }

    fn get(&self, bus: &str, path: &str, interface: &str, name: &str) -> zbus::Result<OwnedValue> {
        self.connection
            .call_method(Some(bus), path, Some(PROPERTIES_INTERFACE), "Get", &(interface, name))
            .and_then(|reply| reply.body().deserialize())
    }

    fn fetch(&self, address: &Address) -> zbus::Result<ItemProps> {
        let all = self
            .connection
            .call_method(
                Some(address.bus.as_str()),
                address.path.as_str(),
                Some(PROPERTIES_INTERFACE),
                "GetAll",
                &(ITEM_INTERFACE,),
            )
            .and_then(|reply| reply.body().deserialize::<HashMap<String, OwnedValue>>());
        match all {
            Ok(map) => Ok(parse_props(&map)),
            Err(err) if !get_all_unsupported(&err) => Err(err),
            Err(err) => {
                let map = self.fetch_each(address);
                if map.is_empty() { Err(err) } else { Ok(parse_props(&map)) }
            }
        }
    }

    fn fetch_each(&self, address: &Address) -> HashMap<String, OwnedValue> {
        PROPERTY_NAMES
            .into_iter()
            .filter_map(|name| {
                let value = self.get(&address.bus, &address.path, ITEM_INTERFACE, name).ok()?;
                Some((name.to_string(), value))
            })
            .collect()
    }

    fn call(&self, key: &str, method: Method) {
        let Some(tracked) = self.items.iter().find(|t| t.address.key() == key) else {
            return;
        };
        let proxy = zbus::blocking::proxy::Builder::<zbus::blocking::Proxy<'_>>::new(&self.connection)
            .destination(tracked.address.bus.as_str())
            .and_then(|b| b.path(tracked.address.path.as_str()))
            .and_then(|b| b.interface(ITEM_INTERFACE))
            .map(|b| b.cache_properties(zbus::proxy::CacheProperties::No))
            .and_then(zbus::blocking::proxy::Builder::build);
        let proxy = match proxy {
            Ok(proxy) => proxy,
            Err(err) => {
                tracing::debug!(key, "tray proxy failed: {err}");
                return;
            }
        };
        let sent = match method {
            Method::Activate(x, y) => proxy.call_noreply("Activate", &(x, y)),
            Method::SecondaryActivate(x, y) => proxy.call_noreply("SecondaryActivate", &(x, y)),
            Method::ContextMenu(x, y) => proxy.call_noreply("ContextMenu", &(x, y)),
            Method::Scroll(delta, vertical) => {
                let orientation = if vertical { "vertical" } else { "horizontal" };
                proxy.call_noreply("Scroll", &(delta, orientation))
            }
        };
        if let Err(err) = sent {
            tracing::debug!(key, ?method, "tray call failed: {err}");
        }
    }
}

fn get_all_unsupported(err: &zbus::Error) -> bool {
    const UNSUPPORTED: [&str; 5] = [
        "org.freedesktop.DBus.Error.UnknownMethod",
        "org.freedesktop.DBus.Error.NotSupported",
        "org.freedesktop.DBus.Error.UnknownInterface",
        "org.freedesktop.DBus.Error.UnknownProperty",
        "org.freedesktop.DBus.Error.InvalidArgs",
    ];
    match err {
        zbus::Error::MethodError(name, _, _) => UNSUPPORTED.contains(&name.as_str()),
        zbus::Error::FDO(fdo) => matches!(
            **fdo,
            zbus::fdo::Error::UnknownMethod(_)
                | zbus::fdo::Error::NotSupported(_)
                | zbus::fdo::Error::UnknownInterface(_)
                | zbus::fdo::Error::UnknownProperty(_)
                | zbus::fdo::Error::InvalidArgs(_)
        ),
        zbus::Error::Variant(_) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_and_watcher_signals_count_only_from_their_real_senders() {
        let bus = Some("org.freedesktop.DBus");
        let dbus = Some("org.freedesktop.DBus");
        assert!(trusted(bus, dbus, None, ":1.5"));
        assert!(!trusted(Some(":1.66"), dbus, None, ":1.5"), "a forged NameOwnerChanged is ignored");
        let watcher = Some(WATCHER_INTERFACE);
        assert!(trusted(Some(":1.7"), watcher, Some(":1.7"), ":1.5"));
        assert!(!trusted(Some(":1.66"), watcher, Some(":1.7"), ":1.5"), "only the watcher announces items");
        assert!(!trusted(Some(":1.5"), watcher, Some(":1.5"), ":1.5"), "our own announcements are not news");
        assert!(!trusted(Some(":1.7"), watcher, None, ":1.5"));
        assert!(trusted(Some(":1.40"), Some(ITEM_INTERFACE), None, ":1.5"), "items speak for themselves");
    }

    #[test]
    fn only_an_unsupported_get_all_falls_back_to_single_gets() {
        let unsupported = zbus::Error::from(zbus::fdo::Error::UnknownMethod("GetAll".into()));
        assert!(get_all_unsupported(&unsupported));
        let no_reply = zbus::Error::from(zbus::fdo::Error::NoReply("hung".into()));
        assert!(!get_all_unsupported(&no_reply));
        let timeout = zbus::Error::from(std::io::Error::from(std::io::ErrorKind::TimedOut));
        assert!(!get_all_unsupported(&timeout));
        let gone = zbus::Error::from(zbus::fdo::Error::ServiceUnknown("gone".into()));
        assert!(!get_all_unsupported(&gone));
    }

    #[test]
    fn backoff_starts_at_five_seconds_doubles_and_caps_at_two_minutes() {
        assert_eq!(next_backoff(None), Duration::from_secs(5));
        assert_eq!(next_backoff(Some(Duration::from_secs(5))), Duration::from_secs(10));
        assert_eq!(next_backoff(Some(Duration::from_secs(80))), Duration::from_mins(2));
        assert_eq!(next_backoff(Some(Duration::from_mins(2))), Duration::from_mins(2));
        assert_eq!(next_backoff(Some(Duration::MAX)), Duration::from_mins(2));
    }

    #[test]
    fn only_hangs_stall_an_item_and_success_resets_the_backoff() {
        let now = Instant::now();
        let mut stall = Stall::default();
        assert!(!stall.active(now));
        stall.record(&zbus::Error::from(zbus::fdo::Error::ServiceUnknown("gone".into())), now);
        assert!(!stall.active(now));
        let timeout = zbus::Error::from(std::io::Error::from(std::io::ErrorKind::TimedOut));
        stall.record(&timeout, now);
        assert!(stall.active(now + Duration::from_secs(4)));
        assert!(!stall.active(now + Duration::from_secs(5)));
        stall.record(&zbus::Error::from(zbus::fdo::Error::NoReply("hung".into())), now);
        assert!(stall.active(now + Duration::from_secs(9)));
        assert!(!stall.active(now + Duration::from_secs(10)));
        stall.clear();
        assert!(!stall.active(now));
        stall.record(&timeout, now);
        assert_eq!(stall.backoff, Some(Duration::from_secs(5)));
    }

    #[test]
    fn pending_registrations_are_deduplicated_and_capped() {
        let entry = |n: usize| Pending {
            address: Address::parse(&format!("org.kde.StatusNotifierItem-{n}-1"), None).expect("valid address"),
            owner: format!(":1.{n}"),
            stall: Stall::default(),
        };
        let mut pending = Vec::new();
        remember(&mut pending, entry(1));
        remember(&mut pending, entry(1));
        assert_eq!(pending.len(), 1);
        for n in 0..MAX_PENDING * 2 {
            remember(&mut pending, entry(n));
        }
        assert_eq!(pending.len(), MAX_PENDING);
        assert_eq!(pending.last().map(|p| p.owner.as_str()), Some(format!(":1.{}", MAX_PENDING * 2 - 1).as_str()));
    }

    #[test]
    fn a_flood_of_icon_changes_is_coalesced_per_sender() {
        let (tx, rx) = mpsc::channel();
        for _ in 0..5 {
            tx.send(Request::Changed { sender: ":1.4".into() }).expect("receiver alive");
        }
        let (changed, pending) = collect_batch(
            Request::Changed { sender: ":1.3".into() },
            &rx,
            Duration::from_millis(10),
            Duration::from_millis(200),
        );
        assert_eq!(changed.len(), 2);
        assert!(pending.is_empty());
    }

    #[test]
    fn any_other_request_ends_the_batch_so_clicks_are_not_delayed() {
        let (tx, rx) = mpsc::channel();
        tx.send(Request::Refresh).expect("receiver alive");
        tx.send(Request::Changed { sender: ":1.9".into() }).expect("receiver alive");
        let started = Instant::now();
        let (changed, pending) = collect_batch(
            Request::Changed { sender: ":1.3".into() },
            &rx,
            Duration::from_millis(40),
            Duration::from_secs(5),
        );
        assert!(started.elapsed() < Duration::from_millis(500));
        assert_eq!(changed.len(), 1);
        assert!(matches!(pending.as_slice(), [Request::Refresh]));
    }
}

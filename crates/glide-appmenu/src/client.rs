use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, mpsc},
    time::Duration,
};

use zbus::zvariant::{OwnedValue, Signature, Value};

use crate::model::{
    ActionInfo, Activation, DBUSMENU_INTERFACE, DbusMenuNode, GTK_ACTIONS_INTERFACE, GTK_MENUS_INTERFACE, GtkMenus,
    MenuItem, Source, build_dbusmenu, build_gtk_menu, gtk_referenced_groups,
};

#[derive(Debug, Clone, PartialEq)]
pub enum AppMenuEvent {
    Menu { source: Source, items: Vec<MenuItem> },
    Gone { source: Source },
    Registered { window_id: u32, source: Source, pid: Option<u32> },
    Unregistered { window_id: u32 },
}

pub type Sink = Arc<dyn Fn(AppMenuEvent) + Send + Sync>;

#[derive(Debug)]
enum Request {
    Watch(Source),
    Unwatch(Source),
    Activate(Source, Activation),
    Opened(Source, Activation),
    Changed { sender: String },
    OwnerChanged { name: String, new_owner: String },
}

#[derive(Debug, Clone)]
pub struct AppMenuHandle {
    tx: mpsc::Sender<Request>,
}

impl AppMenuHandle {
    fn send(&self, request: Request) {
        if self.tx.send(request).is_err() {
            tracing::debug!("the app menu worker has stopped");
        }
    }

    pub fn watch(&self, source: Source) {
        self.send(Request::Watch(source));
    }

    pub fn unwatch(&self, source: Source) {
        self.send(Request::Unwatch(source));
    }

    pub fn activate(&self, source: Source, activation: Activation) {
        self.send(Request::Activate(source, activation));
    }

    pub fn opened(&self, source: Source, activation: Activation) {
        self.send(Request::Opened(source, activation));
    }
}

const SIGNAL_RULES: [&str; 5] = [
    "type='signal',interface='org.gtk.Menus',member='Changed'",
    "type='signal',interface='org.gtk.Actions',member='Changed'",
    "type='signal',interface='com.canonical.dbusmenu',member='LayoutUpdated'",
    "type='signal',interface='com.canonical.dbusmenu',member='ItemsPropertiesUpdated'",
    "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged'",
];

const DBUS_INTERFACE: &str = "org.freedesktop.DBus";

pub const METHOD_TIMEOUT: Duration = Duration::from_secs(3);
pub const MENU_WAIT: Duration = Duration::from_secs(10);
pub const PARKED_MENU_LIMIT: usize = 32;
const COALESCE: Duration = Duration::from_millis(40);
const MAX_BATCH: Duration = Duration::from_millis(150);

fn collect_batch(
    first: Request, rx: &mpsc::Receiver<Request>, quiet: Duration, limit: Duration,
) -> (BTreeSet<String>, Vec<Request>) {
    let deadline = std::time::Instant::now() + limit;
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
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        next = rx.recv_timeout(quiet.min(remaining)).ok();
    }
    (changed, pending)
}

type GtkMenuReply = (u32, u32, Vec<HashMap<String, OwnedValue>>);

pub fn start(sink: Sink, own_registrar: bool) -> anyhow::Result<AppMenuHandle> {
    let connection = zbus::blocking::connection::Builder::session()?.method_timeout(METHOD_TIMEOUT).build()?;
    start_on(connection, sink, own_registrar)
}

pub fn start_on(
    connection: zbus::blocking::Connection, sink: Sink, own_registrar: bool,
) -> anyhow::Result<AppMenuHandle> {
    let (tx, rx) = mpsc::channel();

    let bus = zbus::blocking::fdo::DBusProxy::new(&connection)?;
    for rule in SIGNAL_RULES {
        if let Ok(rule) = zbus::MatchRule::try_from(rule)
            && let Err(err) = bus.add_match_rule(rule)
        {
            tracing::debug!("app menu signal match failed: {err}");
        }
    }

    {
        let connection = connection.clone();
        let tx = tx.clone();
        std::thread::Builder::new().name("glide-appmenu-signals".into()).spawn(move || listen(&connection, &tx))?;
    }

    if own_registrar && let Err(err) = crate::registrar::serve(&connection, sink.clone()) {
        tracing::warn!("could not own the app menu registrar: {err}");
    }

    std::thread::Builder::new().name("glide-appmenu".into()).spawn(move || Worker::new(connection, sink).run(&rx))?;

    Ok(AppMenuHandle { tx })
}

fn listen(connection: &zbus::blocking::Connection, tx: &mpsc::Sender<Request>) {
    for message in zbus::blocking::MessageIterator::from(connection) {
        let Ok(message) = message else {
            continue;
        };
        let header = message.header();
        if header.message_type() != zbus::message::Type::Signal {
            continue;
        }
        let interface = header.interface().map(|i| i.as_str().to_string());
        if interface.as_deref() == Some(DBUS_INTERFACE) {
            if header.member().is_some_and(|member| member.as_str() == "NameOwnerChanged")
                && let Ok((name, _old, new_owner)) = message.body().deserialize::<(String, String, String)>()
                && tx.send(Request::OwnerChanged { name, new_owner }).is_err()
            {
                return;
            }
            continue;
        }
        let relevant =
            matches!(interface.as_deref(), Some(GTK_MENUS_INTERFACE | GTK_ACTIONS_INTERFACE | DBUSMENU_INTERFACE));
        let Some(sender) = header.sender().map(|s| s.as_str().to_string()) else {
            continue;
        };
        if relevant && tx.send(Request::Changed { sender }).is_err() {
            return;
        }
    }
}

struct Watched {
    source: Source,
    owner: String,
    groups: Vec<u32>,
    primed: bool,
    delivered: bool,
    parked: bool,
}

const fn needs_refresh(delivered: bool, stale: bool) -> bool {
    stale || !delivered
}

struct Worker {
    connection: zbus::blocking::Connection,
    sink: Sink,
    watched: Vec<Watched>,
}

impl Worker {
    fn new(connection: zbus::blocking::Connection, sink: Sink) -> Self {
        Self { connection, sink, watched: Vec::new() }
    }

    fn run(mut self, rx: &mpsc::Receiver<Request>) {
        while let Ok(first) = rx.recv() {
            let (changed, pending) = collect_batch(first, rx, COALESCE, MAX_BATCH);
            for request in pending {
                self.handle(request);
            }
            for sender in changed {
                for index in 0..self.watched.len() {
                    if self.watched[index].owner == sender && !self.watched[index].parked {
                        self.refresh(index);
                    }
                }
            }
        }
    }

    fn handle(&mut self, request: Request) {
        match request {
            Request::Watch(source) => self.watch(source),
            Request::Unwatch(source) => self.unwatch(&source),
            Request::Activate(source, activation) => self.activate(&source, &activation),
            Request::Opened(source, activation) => self.opened(&source, &activation),
            Request::OwnerChanged { name, new_owner } => self.owner_changed(&name, &new_owner),
            Request::Changed { .. } => {}
        }
    }

    fn live(&self, source: &Source) -> Option<usize> {
        self.watched.iter().position(|w| w.source == *source && !w.parked)
    }

    fn watch(&mut self, source: Source) {
        if let Some(index) = self.watched.iter().position(|w| w.source == source) {
            let entry = &mut self.watched[index];
            let revived = entry.parked;
            entry.parked = false;
            if needs_refresh(entry.delivered, revived) {
                self.refresh(index);
            }
            return;
        }
        let owner = self.owner_of(source.bus());
        self.watched.push(Watched {
            source,
            owner,
            groups: Vec::new(),
            primed: false,
            delivered: false,
            parked: false,
        });
        self.refresh(self.watched.len() - 1);
    }

    fn unwatch(&mut self, source: &Source) {
        let Some(index) = self.live(source) else {
            return;
        };
        let mut watched = self.watched.remove(index);
        self.release(&watched.source, &watched.groups);
        if !watched.delivered {
            return;
        }
        watched.groups.clear();
        watched.parked = true;
        self.watched.push(watched);
        while self.watched.iter().filter(|w| w.parked).count() > PARKED_MENU_LIMIT {
            let Some(oldest) = self.watched.iter().position(|w| w.parked) else {
                break;
            };
            let evicted = self.watched.remove(oldest);
            (self.sink)(AppMenuEvent::Gone { source: evicted.source });
        }
    }

    fn opened(&mut self, source: &Source, activation: &Activation) {
        let stale = if let Activation::DbusMenu(id) = activation { self.about_to_show(source, *id) } else { false };
        if let Some(index) = self.live(source)
            && needs_refresh(self.watched[index].delivered, stale)
        {
            self.refresh(index);
        }
    }

    fn owner_changed(&mut self, name: &str, new_owner: &str) {
        let mut index = self.watched.len();
        while index > 0 {
            index -= 1;
            let watched = &self.watched[index];
            let affected = watched.source.bus() == name
                || (name.starts_with(':') && watched.owner == name && new_owner.is_empty());
            if !affected {
                continue;
            }
            if new_owner.is_empty() || (watched.parked && watched.owner != new_owner) {
                let watched = self.watched.remove(index);
                (self.sink)(AppMenuEvent::Gone { source: watched.source });
                continue;
            }
            if watched.owner == new_owner {
                continue;
            }
            let watched = &mut self.watched[index];
            watched.owner = new_owner.to_string();
            watched.groups.clear();
            watched.primed = false;
            self.refresh(index);
        }
    }

    fn owner_of(&self, bus: &str) -> String {
        if bus.starts_with(':') {
            return bus.to_string();
        }
        zbus::blocking::fdo::DBusProxy::new(&self.connection)
            .ok()
            .and_then(|proxy| {
                let name = zbus::names::BusName::try_from(bus).ok()?;
                proxy.get_name_owner(name).ok()
            })
            .map_or_else(|| bus.to_string(), |owner| owner.to_string())
    }

    fn refresh(&mut self, index: usize) {
        let source = self.watched[index].source.clone();
        let result = match &source {
            Source::Gtk { bus, menubar, app, window } => {
                let previous = std::mem::take(&mut self.watched[index].groups);
                self.end_groups(bus, menubar, &previous);
                self.fetch_gtk(bus, menubar, app.as_deref(), window.as_deref()).map(|(items, groups)| {
                    self.watched[index].groups = groups;
                    items
                })
            }
            Source::DbusMenu { bus, path } => {
                let prime = !self.watched[index].primed;
                let fetched = self.fetch_dbusmenu(bus, path, prime);
                if fetched.is_ok() {
                    self.watched[index].primed = true;
                }
                fetched
            }
        };
        self.watched[index].delivered = result.is_ok();
        match result {
            Ok(items) => (self.sink)(AppMenuEvent::Menu { source, items }),
            Err(err) => {
                tracing::warn!("app menu for {source:?} is unavailable: {err}");
                (self.sink)(AppMenuEvent::Gone { source });
            }
        }
    }

    fn release(&self, source: &Source, groups: &[u32]) {
        if let Source::Gtk { bus, menubar, .. } = source {
            self.end_groups(bus, menubar, groups);
        }
    }

    fn proxy(&self, bus: &str, path: &str, interface: &str) -> zbus::Result<zbus::blocking::Proxy<'_>> {
        zbus::blocking::Proxy::new(&self.connection, bus.to_string(), path.to_string(), interface.to_string())
    }

    fn end_groups(&self, bus: &str, menubar: &str, groups: &[u32]) {
        if groups.is_empty() {
            return;
        }
        let sent = self
            .proxy(bus, menubar, GTK_MENUS_INTERFACE)
            .and_then(|proxy| proxy.call_noreply("End", &(groups.to_vec(),)));
        if let Err(err) = sent {
            tracing::debug!("could not end the app menu subscription: {err}");
        }
    }

    const GTK_START_ROUNDS: usize = 16;
    const GTK_GROUP_LIMIT: usize = 256;

    fn fetch_gtk(
        &self, bus: &str, menubar: &str, app: Option<&str>, window: Option<&str>,
    ) -> zbus::Result<(Vec<MenuItem>, Vec<u32>)> {
        let proxy = self.proxy(bus, menubar, GTK_MENUS_INTERFACE)?;
        let mut menus = GtkMenus::new();
        let mut subscribed: BTreeSet<u32> = BTreeSet::new();
        let mut wanted: Vec<u32> = vec![0];
        let mut rounds = 0;
        while !wanted.is_empty() && rounds < Self::GTK_START_ROUNDS && subscribed.len() < Self::GTK_GROUP_LIMIT {
            rounds += 1;
            wanted.truncate(Self::GTK_GROUP_LIMIT - subscribed.len());
            let reply: Vec<GtkMenuReply> = match proxy.call("Start", &(wanted.clone(),)) {
                Ok(reply) => reply,
                Err(err) => {
                    subscribed.extend(wanted);
                    self.end_groups(bus, menubar, &subscribed.into_iter().collect::<Vec<_>>());
                    return Err(err);
                }
            };
            subscribed.extend(wanted.iter().copied());
            for (group, menu, items) in reply {
                menus.insert((group, menu), items);
            }
            wanted = gtk_referenced_groups(&menus).into_iter().filter(|group| !subscribed.contains(group)).collect();
        }

        let mut actions: HashMap<String, ActionInfo> = HashMap::new();
        let mut described = false;
        for (prefix, path) in [("app", app), ("win", window)] {
            let Some(path) = path else { continue };
            if let Some(found) = self.describe_actions(bus, path) {
                described = true;
                actions.extend(found.into_iter().map(|(name, info)| (format!("{prefix}.{name}"), info)));
            }
        }
        let items = build_gtk_menu(&menus, (0, 0), described.then_some(&actions));
        Ok((items, subscribed.into_iter().collect()))
    }

    fn describe_actions(&self, bus: &str, path: &str) -> Option<HashMap<String, ActionInfo>> {
        let proxy = self.proxy(bus, path, GTK_ACTIONS_INTERFACE).ok()?;
        let reply: HashMap<String, (bool, Signature, Vec<OwnedValue>)> = proxy.call("DescribeAll", &()).ok()?;
        let describe =
            |(enabled, _, mut state): (bool, Signature, Vec<OwnedValue>)| ActionInfo { enabled, state: state.pop() };
        Some(reply.into_iter().map(|(name, action)| (name, describe(action))).collect())
    }

    fn fetch_dbusmenu(&self, bus: &str, path: &str, prime: bool) -> zbus::Result<Vec<MenuItem>> {
        let proxy = self.proxy(bus, path, DBUSMENU_INTERFACE)?;
        let layout = || -> zbus::Result<DbusMenuNode> {
            let (_revision, root): (u32, DbusMenuNode) =
                proxy.call("GetLayout", &(0i32, -1i32, Vec::<String>::new()))?;
            Ok(root)
        };
        let root = layout()?;
        let mut stale = false;
        if prime {
            for child in &root.2 {
                if let Ok((id, _, _)) = DbusMenuNode::try_from(child.clone()) {
                    let update: zbus::Result<bool> = proxy.call("AboutToShow", &(id,));
                    stale |= update.unwrap_or(false);
                }
            }
        }
        let root = if stale { layout()? } else { root };
        Ok(build_dbusmenu(&root))
    }

    fn about_to_show(&self, source: &Source, id: i32) -> bool {
        let Source::DbusMenu { bus, path } = source else {
            return false;
        };
        let Ok(proxy) = self.proxy(bus, path, DBUSMENU_INTERFACE) else {
            return false;
        };
        if let Err(err) = proxy.call_noreply("Event", &(id, "opened", Value::from(0i32), 0u32)) {
            tracing::debug!("could not tell the menu it opened: {err}");
        }
        proxy.call::<_, _, bool>("AboutToShow", &(id,)).unwrap_or(false)
    }

    fn activate(&self, source: &Source, activation: &Activation) {
        let result = match (source, activation) {
            (Source::Gtk { bus, app, window, .. }, Activation::Gtk { action, target }) => {
                let (prefix, name) = action.split_once('.').unwrap_or(("app", action));
                let path = match prefix {
                    "win" => window.as_deref(),
                    _ => app.as_deref(),
                };
                match path {
                    Some(path) => self.proxy(bus, path, GTK_ACTIONS_INTERFACE).and_then(|proxy| {
                        let parameter: Vec<OwnedValue> = target.iter().cloned().collect();
                        let platform: HashMap<String, OwnedValue> = HashMap::new();
                        proxy.call_noreply("Activate", &(name, parameter, platform))
                    }),
                    None => Ok(()),
                }
            }
            (Source::DbusMenu { bus, path }, Activation::DbusMenu(id)) => self
                .proxy(bus, path, DBUSMENU_INTERFACE)
                .and_then(|proxy| proxy.call_noreply("Event", &(*id, "clicked", Value::from(0i32), 0u32))),
            _ => Ok(()),
        };
        if let Err(err) = result {
            tracing::debug!("app menu activation failed: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_constant_stream_of_changes_cannot_starve_the_worker() {
        let (tx, rx) = mpsc::channel();
        let feeder = std::thread::spawn(move || {
            for _ in 0..400 {
                if tx.send(Request::Changed { sender: String::from(":1.7") }).is_err() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let started = std::time::Instant::now();
        let first = Request::Changed { sender: String::from(":1.7") };
        let (changed, pending) = collect_batch(first, &rx, Duration::from_millis(40), Duration::from_millis(150));
        assert!(started.elapsed() < Duration::from_millis(600));
        assert_eq!(changed.len(), 1);
        assert!(pending.is_empty());
        drop(rx);
        let _ = feeder.join();
    }

    #[test]
    fn a_menu_wait_outlasts_a_hung_tray_clicks_layout_and_about_to_show_plus_its_own_fetch() {
        assert!(MENU_WAIT > METHOD_TIMEOUT * 3);
    }

    #[test]
    fn a_quiet_queue_returns_after_the_quiet_gap() {
        let (tx, rx) = mpsc::channel::<Request>();
        tx.send(Request::Changed { sender: String::from(":1.9") }).expect("receiver alive");
        let started = std::time::Instant::now();
        let (changed, _) = collect_batch(
            Request::Changed { sender: String::from(":1.8") },
            &rx,
            Duration::from_millis(20),
            Duration::from_secs(5),
        );
        assert!(started.elapsed() < Duration::from_millis(500));
        assert_eq!(changed.len(), 2);
    }
}

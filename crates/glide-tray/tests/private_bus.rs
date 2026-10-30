#![allow(clippy::unused_self)] // zbus properties are methods on the served object.

use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use glide_tray::{Status, TrayEvent, TrayItem};
use zbus::{
    fdo::{RequestNameFlags, RequestNameReply},
    names::BusName,
    zvariant::OwnedValue,
};

type Log = Arc<Mutex<Vec<String>>>;

struct FakeItem {
    status: Arc<Mutex<String>>,
    log: Log,
    pixmap: (i32, i32),
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl FakeItem {
    #[zbus(property)]
    fn id(&self) -> String {
        "fake".into()
    }

    #[zbus(property)]
    fn title(&self) -> String {
        "Fake App".into()
    }

    #[zbus(property)]
    fn status(&self) -> String {
        self.status.lock().expect("status").clone()
    }

    #[zbus(property)]
    fn icon_name(&self) -> String {
        "fake-icon".into()
    }

    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        let (width, height) = self.pixmap;
        let bytes = usize::try_from(width).expect("positive") * usize::try_from(height).expect("positive") * 4;
        vec![(width, height, vec![255; bytes])]
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    fn activate(&self, x: i32, y: i32) {
        self.log.lock().expect("log").push(format!("activate {x} {y}"));
    }
}

struct FakeWatcher {
    hosts: Log,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl FakeWatcher {
    fn register_status_notifier_host(&self, service: &str) {
        self.hosts.lock().expect("hosts").push(service.to_string());
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Daemon {
    child: Child,
    address: String,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn private_bus() -> Option<Daemon> {
    let mut child = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .inspect_err(|err| eprintln!("no dbus-daemon ({err}); skipping"))
        .ok()?;
    let mut address = String::new();
    BufReader::new(child.stdout.take()?).read_line(&mut address).ok()?;
    let address = address.trim().to_string();
    if address.is_empty() {
        let _ = child.kill();
        return None;
    }
    Some(Daemon { child, address })
}

fn connect(daemon: &Daemon) -> zbus::blocking::Connection {
    zbus::blocking::connection::Builder::address(daemon.address.as_str())
        .expect("a bus address")
        .build()
        .expect("private bus")
}

struct Tray {
    handle: glide_tray::TrayHandle,
    rx: mpsc::Receiver<TrayEvent>,
    probe: Arc<()>,
}

fn start_tray(daemon: &Daemon) -> Tray {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let probe = Arc::new(());
    let held = probe.clone();
    let sink: glide_tray::Sink = Arc::new(move |event| {
        std::hint::black_box(&held);
        let _ = tx.lock().expect("sink").send(event);
    });
    let handle = glide_tray::start_on(&daemon.address, sink).expect("the worker starts");
    Tray { handle, rx, probe }
}

fn next_item(rx: &mpsc::Receiver<TrayEvent>) -> TrayItem {
    match rx.recv_timeout(Duration::from_secs(5)).expect("an item arrives") {
        TrayEvent::Item(item) => *item,
        TrayEvent::Removed { key } => panic!("{key} was removed early"),
    }
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

fn owned(dbus: &zbus::blocking::fdo::DBusProxy<'_>, name: &str) -> bool {
    dbus.name_has_owner(BusName::try_from(name).expect("valid name")).unwrap_or(false)
}

fn watcher_property(observer: &zbus::blocking::Connection, name: &str) -> Option<OwnedValue> {
    observer
        .call_method(
            Some(glide_tray::WATCHER_NAME),
            glide_tray::WATCHER_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(glide_tray::WATCHER_INTERFACE, name),
        )
        .and_then(|reply| reply.body().deserialize::<OwnedValue>())
        .ok()
}

fn registered_items(observer: &zbus::blocking::Connection) -> Vec<String> {
    watcher_property(observer, "RegisteredStatusNotifierItems")
        .and_then(|value| Vec::<String>::try_from(value).ok())
        .expect("the watcher lists its items")
}

fn announcements(observer: &zbus::blocking::Connection) -> mpsc::Receiver<String> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(glide_tray::WATCHER_INTERFACE)
        .expect("interface")
        .member("StatusNotifierItemRegistered")
        .expect("member")
        .build();
    let messages =
        zbus::blocking::MessageIterator::for_match_rule(rule, observer, None).expect("the signal subscription");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for message in messages.flatten() {
            if let Ok(key) = message.body().deserialize::<String>()
                && tx.send(key).is_err()
            {
                return;
            }
        }
    });
    rx
}

fn host_registered(observer: &zbus::blocking::Connection) -> bool {
    watcher_property(observer, "IsStatusNotifierHostRegistered")
        .and_then(|value| bool::try_from(value).ok())
        .expect("the watcher reports whether a host exists")
}

fn register(app: &zbus::blocking::Connection, service: &str) {
    app.call_method(
        Some(glide_tray::WATCHER_NAME),
        glide_tray::WATCHER_PATH,
        Some(glide_tray::WATCHER_INTERFACE),
        "RegisterStatusNotifierItem",
        &(service,),
    )
    .expect("registration accepted");
}

fn serve_item(app: &zbus::blocking::Connection, status: &Arc<Mutex<String>>, log: &Log, pixmap: (i32, i32)) {
    app.object_server()
        .at("/StatusNotifierItem", FakeItem { status: status.clone(), log: log.clone(), pixmap })
        .expect("served item");
}

fn host_name() -> String {
    format!("{}-{}", glide_tray::HOST_PREFIX, std::process::id())
}

#[test]
fn an_item_registered_by_path_is_watched_activated_refreshed_and_dropped() {
    let Some(daemon) = private_bus() else { return };
    let tray = start_tray(&daemon);

    let observer = connect(&daemon);
    let dbus = zbus::blocking::fdo::DBusProxy::new(&observer).expect("bus proxy");
    wait_until("the watcher name", || owned(&dbus, glide_tray::WATCHER_NAME));
    assert!(host_registered(&observer), "the tray is the watcher's host");

    let log = Log::default();
    let status = Arc::new(Mutex::new(String::from("Active")));
    let app = connect(&daemon);
    serve_item(&app, &status, &log, (1, 1));
    let unique = app.unique_name().expect("unique").to_string();
    register(&app, "/StatusNotifierItem");

    let item = next_item(&tray.rx);
    let key = format!("{unique}/StatusNotifierItem");
    assert_eq!(item.key, key);
    assert_eq!(item.props.id, "fake");
    assert_eq!(item.props.status, Status::Active);
    assert_eq!(item.props.icon_pixmaps.len(), 1);
    assert_eq!(registered_items(&observer), vec![key.clone()]);

    tray.handle.activate(&key, 11, 22);
    wait_until("the activation", || log.lock().expect("log").iter().any(|line| line == "activate 11 22"));

    "Passive".clone_into(&mut status.lock().expect("status"));
    app.emit_signal(None::<BusName<'_>>, "/StatusNotifierItem", glide_tray::ITEM_INTERFACE, "NewStatus", &("Passive",))
        .expect("signal sent");
    let refreshed = next_item(&tray.rx);
    assert_eq!(refreshed.props.status, Status::Passive);

    drop(app);
    let removed = tray.rx.recv_timeout(Duration::from_secs(5)).expect("a removal arrives");
    assert_eq!(removed, TrayEvent::Removed { key });
    wait_until("the registry to forget the item", || registered_items(&observer).is_empty());
}

#[test]
fn a_registration_naming_a_bus_nobody_owns_leaves_no_ghost_in_the_watcher() {
    let Some(daemon) = private_bus() else { return };
    let tray = start_tray(&daemon);
    let observer = connect(&daemon);
    let dbus = zbus::blocking::fdo::DBusProxy::new(&observer).expect("bus proxy");
    wait_until("the watcher name", || owned(&dbus, glide_tray::WATCHER_NAME));

    let announced = announcements(&observer);
    for ghost in ["org.example.Nothing", "org.example.Nothing/Item", ":1.9999/StatusNotifierItem"] {
        register(&observer, ghost);
    }
    assert!(registered_items(&observer).is_empty(), "nobody owns those names, so nothing is listed");

    let log = Log::default();
    let status = Arc::new(Mutex::new(String::from("Active")));
    let app = connect(&daemon);
    serve_item(&app, &status, &log, (1, 1));
    register(&app, "/StatusNotifierItem");
    let item = next_item(&tray.rx);
    assert_eq!(registered_items(&observer), vec![item.key.clone()]);

    let mut seen = Vec::new();
    while seen.last() != Some(&item.key) {
        seen.push(announced.recv_timeout(Duration::from_secs(5)).expect("the real item is announced"));
    }
    assert_eq!(seen, vec![item.key], "no StatusNotifierItemRegistered was sent for a ghost");
}

#[test]
fn an_item_that_exports_its_object_after_registering_appears_without_another_signal() {
    let Some(daemon) = private_bus() else { return };
    let tray = start_tray(&daemon);
    let observer = connect(&daemon);
    let dbus = zbus::blocking::fdo::DBusProxy::new(&observer).expect("bus proxy");
    wait_until("the watcher name", || owned(&dbus, glide_tray::WATCHER_NAME));

    let app = connect(&daemon);
    let status = Arc::new(Mutex::new(String::from("Active")));
    app.object_server()
        .at("/Elsewhere", FakeItem { status: status.clone(), log: Log::default(), pixmap: (1, 1) })
        .expect("the app answers calls");
    register(&app, "/StatusNotifierItem");
    std::thread::sleep(Duration::from_millis(300));
    assert!(tray.rx.try_recv().is_err(), "nothing is served at the registered path yet");

    serve_item(&app, &status, &Log::default(), (1, 1));
    let item = next_item(&tray.rx);
    assert_eq!(item.props.id, "fake");
}

#[test]
fn oversized_pixmaps_are_dropped_while_the_rest_of_the_item_survives() {
    let Some(daemon) = private_bus() else { return };
    let tray = start_tray(&daemon);
    let observer = connect(&daemon);
    let dbus = zbus::blocking::fdo::DBusProxy::new(&observer).expect("bus proxy");
    wait_until("the watcher name", || owned(&dbus, glide_tray::WATCHER_NAME));

    let over = glide_tray::MAX_PIXMAP_SIDE + 1;
    for pixmap in [(over, 1), (1, over)] {
        let app = connect(&daemon);
        serve_item(&app, &Arc::new(Mutex::new(String::from("Active"))), &Log::default(), pixmap);
        register(&app, "/StatusNotifierItem");
        let item = next_item(&tray.rx);
        assert_eq!(item.props.id, "fake", "{pixmap:?}: the text properties still arrive");
        assert_eq!(item.props.icon_name, "fake-icon");
        assert!(item.props.icon_pixmaps.is_empty(), "{pixmap:?}: the pixmap is over the side limit");
        drop(app);
        let removed = tray.rx.recv_timeout(Duration::from_secs(5)).expect("a removal arrives");
        assert!(matches!(removed, TrayEvent::Removed { .. }));
    }
}

#[test]
fn stopping_the_tray_releases_the_watcher_so_another_tray_can_take_over_and_it_can_start_again() {
    let Some(daemon) = private_bus() else { return };
    let tray = start_tray(&daemon);
    let observer = connect(&daemon);
    let dbus = zbus::blocking::fdo::DBusProxy::new(&observer).expect("bus proxy");
    wait_until("the watcher name", || owned(&dbus, glide_tray::WATCHER_NAME));
    let watcher_owner = dbus
        .get_name_owner(BusName::try_from(glide_tray::WATCHER_NAME).expect("valid name"))
        .expect("the watcher has an owner")
        .to_string();

    let app = connect(&daemon);
    serve_item(&app, &Arc::new(Mutex::new(String::from("Active"))), &Log::default(), (1, 1));
    register(&app, "/StatusNotifierItem");
    let item = next_item(&tray.rx);

    tray.handle.shutdown();
    assert_eq!(
        tray.rx.recv_timeout(Duration::from_secs(5)).expect("the item is removed"),
        TrayEvent::Removed { key: item.key }
    );
    wait_until("the watcher name to be released", || !owned(&dbus, glide_tray::WATCHER_NAME));
    wait_until("the host name to be released", || !owned(&dbus, &host_name()));
    wait_until("the connection to close", || !owned(&dbus, &watcher_owner));
    wait_until("both threads to end", || Arc::strong_count(&tray.probe) == 1);

    let rival = connect(&daemon);
    let reply = rival
        .request_name_with_flags(glide_tray::WATCHER_NAME, RequestNameFlags::DoNotQueue.into())
        .expect("the name request is answered");
    assert!(matches!(reply, RequestNameReply::PrimaryOwner), "another tray takes the watcher name");
    rival.release_name(glide_tray::WATCHER_NAME).expect("released");

    let again = start_tray(&daemon);
    wait_until("the restarted watcher", || owned(&dbus, glide_tray::WATCHER_NAME));
    assert!(host_registered(&observer));
    let second = connect(&daemon);
    serve_item(&second, &Arc::new(Mutex::new(String::from("Active"))), &Log::default(), (1, 1));
    register(&second, "/StatusNotifierItem");
    assert_eq!(next_item(&again.rx).props.id, "fake");

    again.handle.shutdown();
    wait_until("the second stop", || !owned(&dbus, glide_tray::WATCHER_NAME));
    wait_until("the second worker to end", || Arc::strong_count(&again.probe) == 1);
}

#[test]
fn stopping_a_tray_that_only_joined_another_watcher_leaves_that_watcher_alone() {
    let Some(daemon) = private_bus() else { return };
    let hosts = Log::default();
    let rival = connect(&daemon);
    rival.object_server().at(glide_tray::WATCHER_PATH, FakeWatcher { hosts: hosts.clone() }).expect("served watcher");
    rival.request_name(glide_tray::WATCHER_NAME).expect("the rival owns the watcher name");
    let rival_unique = rival.unique_name().expect("unique").to_string();

    let tray = start_tray(&daemon);
    let observer = connect(&daemon);
    let dbus = zbus::blocking::fdo::DBusProxy::new(&observer).expect("bus proxy");
    wait_until("the host registration", || hosts.lock().expect("hosts").contains(&host_name()));
    let owner = dbus.get_name_owner(BusName::try_from(glide_tray::WATCHER_NAME).expect("valid name"));
    assert_eq!(owner.expect("still owned").to_string(), rival_unique);

    tray.handle.shutdown();
    wait_until("the host name to be released", || !owned(&dbus, &host_name()));
    wait_until("the worker to end", || Arc::strong_count(&tray.probe) == 1);
    let owner = dbus.get_name_owner(BusName::try_from(glide_tray::WATCHER_NAME).expect("valid name"));
    assert_eq!(owner.expect("still owned").to_string(), rival_unique, "the other watcher keeps its name");
}

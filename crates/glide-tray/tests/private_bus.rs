#![allow(clippy::unused_self)] // zbus properties are methods on the served object.

use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use glide_tray::{Status, TrayEvent, TrayItem};
use zbus::names::BusName;

type Log = Arc<Mutex<Vec<String>>>;

struct FakeItem {
    status: Arc<Mutex<String>>,
    log: Log,
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
        vec![(1, 1, vec![255, 255, 0, 0])]
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    fn activate(&self, x: i32, y: i32) {
        self.log.lock().expect("log").push(format!("activate {x} {y}"));
    }
}

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
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
    let address = address.trim();
    if address.is_empty() {
        let _ = child.kill();
        return None;
    }
    // SAFETY: this test binary holds a single test, so no other thread reads the environment concurrently.
    unsafe { std::env::set_var("DBUS_SESSION_BUS_ADDRESS", address) };
    Some(Daemon(child))
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

#[test]
fn an_item_registered_by_path_is_watched_activated_refreshed_and_dropped() {
    let Some(_daemon) = private_bus() else { return };

    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let sink: glide_tray::Sink = Arc::new(move |event| {
        let _ = tx.lock().expect("sink").send(event);
    });
    let handle = glide_tray::start(sink).expect("the worker starts");

    let observer = zbus::blocking::Connection::session().expect("private bus");
    let dbus = zbus::blocking::fdo::DBusProxy::new(&observer).expect("bus proxy");
    wait_until("the watcher name", || {
        dbus.name_has_owner(BusName::try_from(glide_tray::WATCHER_NAME).expect("valid name")).unwrap_or(false)
    });

    let log = Log::default();
    let status = Arc::new(Mutex::new(String::from("Active")));
    let app = zbus::blocking::Connection::session().expect("private bus");
    app.object_server()
        .at("/StatusNotifierItem", FakeItem { status: status.clone(), log: log.clone() })
        .expect("served item");
    let unique = app.unique_name().expect("unique").to_string();
    app.call_method(
        Some(glide_tray::WATCHER_NAME),
        glide_tray::WATCHER_PATH,
        Some(glide_tray::WATCHER_INTERFACE),
        "RegisterStatusNotifierItem",
        &("/StatusNotifierItem",),
    )
    .expect("registration accepted");

    let item = next_item(&rx);
    let key = format!("{unique}/StatusNotifierItem");
    assert_eq!(item.key, key);
    assert_eq!(item.props.id, "fake");
    assert_eq!(item.props.status, Status::Active);
    assert_eq!(item.props.icon_pixmaps.len(), 1);

    let listed: Vec<String> = observer
        .call_method(
            Some(glide_tray::WATCHER_NAME),
            glide_tray::WATCHER_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(glide_tray::WATCHER_INTERFACE, "RegisteredStatusNotifierItems"),
        )
        .and_then(|reply| reply.body().deserialize::<zbus::zvariant::OwnedValue>())
        .ok()
        .and_then(|value| Vec::<String>::try_from(value).ok())
        .expect("the watcher lists its items");
    assert_eq!(listed, vec![key.clone()]);

    handle.activate(&key, 11, 22);
    wait_until("the activation", || log.lock().expect("log").iter().any(|line| line == "activate 11 22"));

    "Passive".clone_into(&mut status.lock().expect("status"));
    app.emit_signal(None::<BusName<'_>>, "/StatusNotifierItem", glide_tray::ITEM_INTERFACE, "NewStatus", &("Passive",))
        .expect("signal sent");
    let refreshed = next_item(&rx);
    assert_eq!(refreshed.props.status, Status::Passive);

    drop(app);
    let removed = rx.recv_timeout(Duration::from_secs(5)).expect("a removal arrives");
    assert_eq!(removed, TrayEvent::Removed { key });
}

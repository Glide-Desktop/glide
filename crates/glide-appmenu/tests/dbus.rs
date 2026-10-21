// The fake services must mirror the exact D-Bus method signatures, which
// dictate owned arguments, unused receivers and nested reply tuples; the
// interface macro also reads the ignored `_` arguments when decoding calls.
#![allow(clippy::used_underscore_binding, clippy::unused_self, clippy::needless_pass_by_value, clippy::type_complexity)]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use glide_appmenu::{Activation, AppMenuEvent, MenuItem, Source, Toggle};
use zbus::zvariant::{OwnedValue, Signature, StructureBuilder, Value};

type Log = Arc<Mutex<Vec<String>>>;

fn owned<'a>(value: impl Into<Value<'a>>) -> OwnedValue {
    OwnedValue::try_from(value.into()).expect("plain values own cleanly")
}

fn entry(pairs: &[(&str, OwnedValue)]) -> HashMap<String, OwnedValue> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
}

struct FakeMenus;

#[zbus::interface(name = "org.gtk.Menus")]
impl FakeMenus {
    fn start(&self, groups: Vec<u32>) -> Vec<(u32, u32, Vec<HashMap<String, OwnedValue>>)> {
        let mut out = Vec::new();
        if groups.contains(&0) {
            out.push((0, 0, vec![entry(&[("label", owned("_File")), (":submenu", owned((1u32, 0u32)))])]));
        }
        if groups.contains(&1) {
            out.push((
                1,
                0,
                vec![
                    entry(&[("label", owned("_Quit")), ("action", owned("app.quit")), ("accel", owned("<Primary>q"))]),
                    entry(&[("label", owned("_Wrap")), ("action", owned("win.wrap"))]),
                ],
            ));
        }
        out
    }

    fn end(&self, _groups: Vec<u32>) {}
}

struct FakeActions {
    prefix: &'static str,
    log: Log,
}

#[zbus::interface(name = "org.gtk.Actions")]
impl FakeActions {
    fn describe_all(&self) -> HashMap<String, (bool, Signature, Vec<OwnedValue>)> {
        let mut out = HashMap::new();
        if self.prefix == "app" {
            out.insert("quit".to_string(), (true, Signature::Unit, Vec::new()));
        } else {
            out.insert("wrap".to_string(), (true, Signature::Unit, vec![owned(true)]));
        }
        out
    }

    fn activate(&self, name: String, _parameter: Vec<OwnedValue>, _platform: HashMap<String, OwnedValue>) {
        self.log.lock().expect("log").push(format!("{}.{name}", self.prefix));
    }
}

struct FakeDbusMenu {
    log: Log,
}

fn node(id: i32, props: HashMap<String, OwnedValue>, children: Vec<OwnedValue>) -> OwnedValue {
    owned(
        StructureBuilder::new().add_field(id).add_field(props).add_field(children).build().expect("a well-formed node"),
    )
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl FakeDbusMenu {
    fn get_layout(
        &self, _parent: i32, _depth: i32, _props: Vec<String>,
    ) -> (u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)) {
        let open = node(7, entry(&[("label", owned("_Open"))]), Vec::new());
        let file = node(3, entry(&[("label", owned("_File")), ("children-display", owned("submenu"))]), vec![open]);
        (1, (0, HashMap::new(), vec![file]))
    }

    fn about_to_show(&self, id: i32) -> bool {
        self.log.lock().expect("log").push(format!("{id}:about-to-show"));
        false
    }

    fn event(&self, id: i32, event: String, _data: OwnedValue, _timestamp: u32) {
        self.log.lock().expect("log").push(format!("{id}:{event}"));
    }
}

fn session() -> Option<zbus::blocking::Connection> {
    match zbus::blocking::Connection::session() {
        Ok(connection) => Some(connection),
        Err(err) => {
            eprintln!("no session bus ({err}); skipping");
            None
        }
    }
}

fn client() -> Option<(glide_appmenu::AppMenuHandle, mpsc::Receiver<AppMenuEvent>)> {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let sink: glide_appmenu::Sink = Arc::new(move |event| {
        let _ = tx.lock().expect("sink").send(event);
    });
    let handle = glide_appmenu::start(sink, false).ok()?;
    Some((handle, rx))
}

fn next_menu(rx: &mpsc::Receiver<AppMenuEvent>) -> Vec<MenuItem> {
    loop {
        match rx.recv_timeout(Duration::from_secs(5)).expect("a menu arrives") {
            AppMenuEvent::Menu { items, .. } => return items,
            AppMenuEvent::Gone { source } => panic!("{source:?} was reported gone"),
            _ => {}
        }
    }
}

fn wait_for(log: &Log, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if log.lock().expect("log").iter().any(|line| line == expected) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("never saw {expected:?} in {:?}", log.lock().expect("log"));
}

#[test]
fn a_gtk_menubar_is_read_and_its_actions_fire() {
    let Some(server) = session() else { return };
    let log = Log::default();
    let objects = server.object_server();
    objects.at("/test/menubar", FakeMenus).expect("menus");
    objects.at("/test/app", FakeActions { prefix: "app", log: log.clone() }).expect("app actions");
    objects.at("/test/window", FakeActions { prefix: "win", log: log.clone() }).expect("window actions");
    let bus = server.unique_name().expect("unique name").to_string();

    let Some((handle, rx)) = client() else { return };
    let source = Source::Gtk {
        bus,
        menubar: "/test/menubar".into(),
        app: Some("/test/app".into()),
        window: Some("/test/window".into()),
    };
    handle.watch(source.clone());
    let bar = next_menu(&rx);

    assert_eq!(bar.len(), 1);
    assert_eq!(bar[0].label, "File");
    let quit = &bar[0].children[0];
    assert_eq!(quit.label, "Quit");
    assert_eq!(quit.accel.as_deref(), Some("Ctrl+Q"));
    assert!(quit.enabled);
    assert_eq!(bar[0].children[1].toggle, Toggle::Check(true));

    handle.activate(source, quit.activation.clone());
    wait_for(&log, "app.quit");
}

#[test]
fn a_dbusmenu_layout_is_read_and_clicks_are_delivered() {
    let Some(server) = session() else { return };
    let log = Log::default();
    server.object_server().at("/test/dbusmenu", FakeDbusMenu { log: log.clone() }).expect("dbusmenu");
    let bus = server.unique_name().expect("unique name").to_string();

    let Some((handle, rx)) = client() else { return };
    let source = Source::DbusMenu { bus, path: "/test/dbusmenu".into() };
    handle.watch(source.clone());
    let bar = next_menu(&rx);

    assert_eq!(bar.len(), 1);
    assert_eq!(bar[0].label, "File");
    assert_eq!(bar[0].children[0].activation, Activation::DbusMenu(7));

    handle.activate(source, Activation::DbusMenu(7));
    wait_for(&log, "7:clicked");
}

fn about_to_show_calls(log: &Log) -> usize {
    log.lock().expect("log").iter().filter(|line| line.ends_with(":about-to-show")).count()
}

#[test]
fn a_layout_update_refreshes_without_asking_about_to_show_again() {
    let Some(server) = session() else { return };
    let log = Log::default();
    server.object_server().at("/test/dbusmenu", FakeDbusMenu { log: log.clone() }).expect("dbusmenu");
    let bus = server.unique_name().expect("unique name").to_string();

    let Some((handle, rx)) = client() else { return };
    handle.watch(Source::DbusMenu { bus, path: "/test/dbusmenu".into() });
    next_menu(&rx);
    assert_eq!(about_to_show_calls(&log), 1, "the first fetch primes each top-level menu");

    for revision in 2..5u32 {
        server
            .emit_signal(None::<()>, "/test/dbusmenu", "com.canonical.dbusmenu", "LayoutUpdated", &(revision, 0i32))
            .expect("signal");
        next_menu(&rx);
    }
    assert_eq!(about_to_show_calls(&log), 1, "a refresh must not provoke another layout update");
}

#[test]
fn a_menu_whose_client_leaves_the_bus_is_gone() {
    let Some(server) = session() else { return };
    let log = Log::default();
    server.object_server().at("/test/dbusmenu", FakeDbusMenu { log }).expect("dbusmenu");
    let bus = server.unique_name().expect("unique name").to_string();

    let Some((handle, rx)) = client() else { return };
    let source = Source::DbusMenu { bus, path: "/test/dbusmenu".into() };
    handle.watch(source.clone());
    next_menu(&rx);

    server.close().expect("close");
    loop {
        match rx.recv_timeout(Duration::from_secs(5)).expect("the menu is reported gone") {
            AppMenuEvent::Gone { source: gone } => {
                assert_eq!(gone, source);
                return;
            }
            AppMenuEvent::Menu { .. } => panic!("a vanished client produced a menu"),
            _ => {}
        }
    }
}

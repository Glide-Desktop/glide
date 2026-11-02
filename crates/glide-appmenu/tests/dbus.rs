// The fake services must mirror the exact D-Bus method signatures, which
// dictate owned arguments, unused receivers and nested reply tuples; the
// interface macro also reads the ignored `_` arguments when decoding calls.
#![allow(clippy::used_underscore_binding, clippy::unused_self, clippy::needless_pass_by_value, clippy::type_complexity)]

use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use glide_appmenu::{
    Activation, AppMenuEvent, MenuItem, PARKED_MENU_LIMIT, REGISTRAR_NAME, REGISTRAR_PATH, Source, Toggle,
};
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
        match self.prefix {
            "app" => out.insert("quit".to_string(), (true, Signature::Unit, Vec::new())),
            "unity" => out.insert("copy".to_string(), (true, Signature::Unit, Vec::new())),
            _ => out.insert("wrap".to_string(), (true, Signature::Unit, vec![owned(true)])),
        };
        out
    }

    fn activate(&self, name: String, _parameter: Vec<OwnedValue>, platform: HashMap<String, OwnedValue>) {
        let mut log = self.log.lock().expect("log");
        log.push(format!("{}.{name}", self.prefix));
        let token = |key: &str| platform.get(key).and_then(|value| String::try_from(value.clone()).ok());
        if let (Some(token), Some(startup)) = (token("activation-token"), token("desktop-startup-id")) {
            log.push(format!("{}.{name} token={token} startup={startup}", self.prefix));
        }
    }
}

struct UnityMenus;

#[zbus::interface(name = "org.gtk.Menus")]
impl UnityMenus {
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
                    entry(&[("label", owned("_Quit")), ("action", owned("app.quit"))]),
                    entry(&[("label", owned("_Wrap")), ("action", owned("win.wrap"))]),
                    entry(&[("label", owned("_Copy")), ("action", owned("unity.copy"))]),
                ],
            ));
        }
        out
    }

    fn end(&self, _groups: Vec<u32>) {}
}

struct FlakyMenus {
    failures: AtomicUsize,
}

#[zbus::interface(name = "org.gtk.Menus")]
impl FlakyMenus {
    fn start(&self, groups: Vec<u32>) -> zbus::fdo::Result<Vec<(u32, u32, Vec<HashMap<String, OwnedValue>>)>> {
        if self.failures.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| left.checked_sub(1)).is_ok() {
            return Err(zbus::fdo::Error::Failed("the application is still starting".into()));
        }
        Ok(FakeMenus.start(groups))
    }

    fn end(&self, _groups: Vec<u32>) {}
}

struct FakeDbusMenu {
    log: Log,
}

fn node(id: i32, props: HashMap<String, OwnedValue>, children: Vec<OwnedValue>) -> OwnedValue {
    owned(
        StructureBuilder::new().add_field(id).add_field(props).add_field(children).build().expect("a well-formed node"),
    )
}

fn file_menu_layout() -> (u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)) {
    let open = node(7, entry(&[("label", owned("_Open"))]), Vec::new());
    let file = node(3, entry(&[("label", owned("_File")), ("children-display", owned("submenu"))]), vec![open]);
    (1, (0, HashMap::new(), vec![file]))
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl FakeDbusMenu {
    fn get_layout(
        &self, _parent: i32, _depth: i32, _props: Vec<String>,
    ) -> (u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)) {
        file_menu_layout()
    }

    fn about_to_show(&self, id: i32) -> bool {
        self.log.lock().expect("log").push(format!("{id}:about-to-show"));
        false
    }

    fn event(&self, id: i32, event: String, _data: OwnedValue, _timestamp: u32) {
        self.log.lock().expect("log").push(format!("{id}:{event}"));
    }
}

struct HungDbusMenu {
    log: Log,
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl HungDbusMenu {
    fn get_layout(
        &self, _parent: i32, _depth: i32, _props: Vec<String>,
    ) -> (u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)) {
        file_menu_layout()
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    async fn event(&self, id: i32, event: String, _data: OwnedValue, _timestamp: u32) {
        self.log.lock().expect("log").push(format!("{id}:{event}"));
        std::future::pending::<()>().await;
    }
}

fn wide_layout() -> (u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)) {
    let menus = (1..=4)
        .map(|id| {
            let item = node(id * 10, entry(&[("label", owned("Item"))]), Vec::new());
            node(id, entry(&[("label", owned("_Menu")), ("children-display", owned("submenu"))]), vec![item])
        })
        .collect();
    (1, (0, HashMap::new(), menus))
}

struct SlowPrimeDbusMenu {
    log: Log,
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl SlowPrimeDbusMenu {
    fn get_layout(
        &self, _parent: i32, _depth: i32, _props: Vec<String>,
    ) -> (u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)) {
        wide_layout()
    }

    async fn about_to_show(&self, id: i32) -> bool {
        self.log.lock().expect("log").push(format!("{id}:about-to-show"));
        std::future::pending::<bool>().await
    }

    fn event(&self, _id: i32, _event: String, _data: OwnedValue, _timestamp: u32) {}
}

struct GroupDbusMenu {
    log: Log,
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl GroupDbusMenu {
    fn get_layout(
        &self, _parent: i32, _depth: i32, _props: Vec<String>,
    ) -> (u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)) {
        wide_layout()
    }

    fn about_to_show(&self, id: i32) -> bool {
        self.log.lock().expect("log").push(format!("{id}:about-to-show"));
        false
    }

    fn about_to_show_group(&self, ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        self.log.lock().expect("log").push(format!("group:{ids:?}"));
        (Vec::new(), Vec::new())
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

fn menu_of(rx: &mpsc::Receiver<AppMenuEvent>, wanted: &Source) -> Vec<MenuItem> {
    loop {
        match rx.recv_timeout(Duration::from_secs(5)).expect("a menu arrives") {
            AppMenuEvent::Menu { source, items } if source == *wanted => return items,
            AppMenuEvent::Gone { source } if source == *wanted => panic!("{source:?} was reported gone"),
            _ => {}
        }
    }
}

#[test]
fn a_click_on_a_client_that_never_answers_does_not_hold_up_the_next_menu() {
    let Some(hung_server) = session() else { return };
    let Some(healthy_server) = session() else { return };
    let log = Log::default();
    hung_server.object_server().at("/test/dbusmenu", HungDbusMenu { log: log.clone() }).expect("hung dbusmenu");
    healthy_server.object_server().at("/test/dbusmenu", FakeDbusMenu { log: Log::default() }).expect("dbusmenu");
    let hung = Source::DbusMenu {
        bus: hung_server.unique_name().expect("unique name").to_string(),
        path: "/test/dbusmenu".into(),
    };
    let healthy = Source::DbusMenu {
        bus: healthy_server.unique_name().expect("unique name").to_string(),
        path: "/test/dbusmenu".into(),
    };

    let Some((handle, rx)) = client() else { return };
    handle.watch(hung.clone());
    menu_of(&rx, &hung);

    handle.opened(hung.clone(), Activation::DbusMenu(3));
    handle.activate(hung, Activation::DbusMenu(7));
    let started = Instant::now();
    handle.watch(healthy.clone());
    menu_of(&rx, &healthy);
    assert!(
        started.elapsed() < glide_appmenu::METHOD_TIMEOUT / 2,
        "the worker waited {:?} on replies nobody needs",
        started.elapsed()
    );
    wait_for(&log, "3:opened");
    wait_for(&log, "7:clicked");
}

struct PrivateBus {
    child: Child,
    address: String,
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl PrivateBus {
    fn connect(&self) -> zbus::blocking::Connection {
        zbus::blocking::connection::Builder::address(self.address.as_str())
            .expect("a private bus address")
            .method_timeout(glide_appmenu::METHOD_TIMEOUT)
            .build()
            .expect("a client on the private bus")
    }
}

fn private_bus() -> PrivateBus {
    let mut child = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("dbus-daemon is installed");
    let mut address = String::new();
    BufReader::new(child.stdout.take().expect("piped stdout")).read_line(&mut address).expect("the bus address");
    PrivateBus { child, address: address.trim().to_string() }
}

fn private_client(
    bus: &PrivateBus, own_registrar: bool,
) -> (glide_appmenu::AppMenuHandle, mpsc::Receiver<AppMenuEvent>) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let sink: glide_appmenu::Sink = Arc::new(move |event| {
        let _ = tx.lock().expect("sink").send(event);
    });
    let handle = glide_appmenu::start_on(bus.connect(), sink, own_registrar).expect("the app menu client starts");
    (handle, rx)
}

fn dbusmenu_source(server: &zbus::blocking::Connection, path: &str) -> Source {
    Source::DbusMenu { bus: server.unique_name().expect("unique name").to_string(), path: path.into() }
}

fn gone_of(rx: &mpsc::Receiver<AppMenuEvent>, wanted: &Source) {
    loop {
        match rx.recv_timeout(Duration::from_secs(5)).expect("the menu is reported gone") {
            AppMenuEvent::Gone { source } if source == *wanted => return,
            AppMenuEvent::Menu { source, .. } if source == *wanted => panic!("{source:?} produced a menu"),
            _ => {}
        }
    }
}

struct FlakyDbusMenu {
    failures: AtomicUsize,
    log: Log,
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl FlakyDbusMenu {
    fn get_layout(
        &self, _parent: i32, _depth: i32, _props: Vec<String>,
    ) -> zbus::fdo::Result<(u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>))> {
        if self.failures.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| left.checked_sub(1)).is_ok() {
            return Err(zbus::fdo::Error::Failed("the application is busy".into()));
        }
        Ok(file_menu_layout())
    }

    fn about_to_show(&self, id: i32) -> bool {
        self.log.lock().expect("log").push(format!("{id}:about-to-show"));
        false
    }

    fn event(&self, id: i32, event: String, _data: OwnedValue, _timestamp: u32) {
        self.log.lock().expect("log").push(format!("{id}:{event}"));
    }
}

#[test]
fn a_menu_that_failed_to_load_is_fetched_again_when_it_is_watched_again() {
    let bus = private_bus();
    let server = bus.connect();
    let log = Log::default();
    server
        .object_server()
        .at("/test/dbusmenu", FlakyDbusMenu { failures: AtomicUsize::new(1), log: log.clone() })
        .expect("dbusmenu");
    let source = dbusmenu_source(&server, "/test/dbusmenu");
    let (handle, rx) = private_client(&bus, false);

    handle.watch(source.clone());
    gone_of(&rx, &source);
    handle.watch(source.clone());
    let bar = menu_of(&rx, &source);

    assert_eq!(bar[0].label, "File");
    assert_eq!(about_to_show_calls(&log), 1, "the retry primes lazily filled menus like a first fetch does");
}

#[test]
fn opening_a_menu_that_never_loaded_fetches_it_even_though_about_to_show_says_nothing_changed() {
    let bus = private_bus();
    let server = bus.connect();
    let log = Log::default();
    server
        .object_server()
        .at("/test/dbusmenu", FlakyDbusMenu { failures: AtomicUsize::new(1), log: log.clone() })
        .expect("dbusmenu");
    let source = dbusmenu_source(&server, "/test/dbusmenu");
    let (handle, rx) = private_client(&bus, false);

    handle.watch(source.clone());
    gone_of(&rx, &source);
    handle.opened(source.clone(), Activation::DbusMenu(3));

    assert_eq!(menu_of(&rx, &source)[0].label, "File");
    wait_for(&log, "3:opened");
}

#[test]
fn an_unwatched_menu_is_forgotten_once_its_client_leaves_the_bus() {
    let bus = private_bus();
    let server = bus.connect();
    server.object_server().at("/test/dbusmenu", FakeDbusMenu { log: Log::default() }).expect("dbusmenu");
    let source = dbusmenu_source(&server, "/test/dbusmenu");
    let (handle, rx) = private_client(&bus, false);

    handle.watch(source.clone());
    menu_of(&rx, &source);
    handle.unwatch(source.clone());
    server.close().expect("close");

    gone_of(&rx, &source);
}

#[test]
fn watching_an_unwatched_menu_again_refreshes_it() {
    let bus = private_bus();
    let server = bus.connect();
    server.object_server().at("/test/dbusmenu", FakeDbusMenu { log: Log::default() }).expect("dbusmenu");
    let source = dbusmenu_source(&server, "/test/dbusmenu");
    let (handle, rx) = private_client(&bus, false);

    handle.watch(source.clone());
    menu_of(&rx, &source);
    handle.unwatch(source.clone());
    handle.watch(source.clone());

    assert_eq!(menu_of(&rx, &source)[0].label, "File");
}

#[test]
fn only_a_bounded_number_of_unwatched_menus_are_remembered() {
    let bus = private_bus();
    let server = bus.connect();
    let total = PARKED_MENU_LIMIT + 2;
    let sources: Vec<Source> = (0..total)
        .map(|index| {
            let path = format!("/test/menu/{index}");
            server.object_server().at(path.as_str(), FakeDbusMenu { log: Log::default() }).expect("dbusmenu");
            dbusmenu_source(&server, &path)
        })
        .collect();
    let (handle, rx) = private_client(&bus, false);

    for source in &sources {
        handle.watch(source.clone());
        menu_of(&rx, source);
    }
    for source in &sources {
        handle.unwatch(source.clone());
    }

    gone_of(&rx, &sources[0]);
    gone_of(&rx, &sources[1]);
    while let Ok(event) = rx.recv_timeout(Duration::from_millis(300)) {
        assert!(!matches!(event, AppMenuEvent::Gone { .. }), "only the two oldest are dropped: {event:?}");
    }
}

fn registrations_after(events: &[AppMenuEvent]) -> HashMap<u32, Source> {
    let mut known = HashMap::new();
    for event in events {
        match event {
            AppMenuEvent::Registered { window_id, source, .. } => {
                known.insert(*window_id, source.clone());
            }
            AppMenuEvent::Unregistered { window_id } => {
                known.remove(window_id);
            }
            AppMenuEvent::Menu { .. } | AppMenuEvent::Gone { .. } => {}
        }
    }
    known
}

fn registrar(caller: &zbus::blocking::Connection) -> zbus::blocking::Proxy<'_> {
    zbus::blocking::Proxy::new(caller, REGISTRAR_NAME, REGISTRAR_PATH, "com.canonical.AppMenu.Registrar")
        .expect("the registrar proxy")
}

#[test]
fn a_window_registered_and_unregistered_back_to_back_leaves_nothing_in_the_compositors_view() {
    const WINDOWS: u32 = 20;
    let bus = private_bus();
    let (_handle, rx) = private_client(&bus, true);
    let caller = bus.connect();
    let registrar = registrar(&caller);
    let menu = zbus::zvariant::ObjectPath::try_from("/menu").expect("an object path");

    for window in 1..=WINDOWS {
        registrar.call_noreply("RegisterWindow", &(window, &menu)).expect("register");
        registrar.call_noreply("UnregisterWindow", &(window,)).expect("unregister");
    }

    let mut events = Vec::new();
    let mut unregistered = 0;
    while unregistered < WINDOWS {
        let event = rx.recv_timeout(Duration::from_secs(5)).unwrap_or_else(|_| {
            panic!("only {unregistered} of {WINDOWS} windows were unregistered; saw {events:?}");
        });
        unregistered += u32::from(matches!(event, AppMenuEvent::Unregistered { .. }));
        events.push(event);
    }
    while let Ok(event) = rx.recv_timeout(Duration::from_millis(400)) {
        events.push(event);
    }

    let left = registrations_after(&events);
    assert!(left.is_empty(), "windows nothing will ever unregister: {left:?}");
}

#[test]
fn a_client_that_leaves_the_bus_takes_its_registered_windows_with_it() {
    let bus = private_bus();
    let (_handle, rx) = private_client(&bus, true);
    let caller = bus.connect();
    let menu = zbus::zvariant::ObjectPath::try_from("/menu").expect("an object path");
    registrar(&caller).call::<_, _, ()>("RegisterWindow", &(5u32, &menu)).expect("register");

    let mut events = Vec::new();
    while !events.iter().any(|event| matches!(event, AppMenuEvent::Registered { window_id: 5, .. })) {
        events.push(rx.recv_timeout(Duration::from_secs(5)).expect("the registration is announced"));
    }
    assert_eq!(registrations_after(&events).len(), 1);

    caller.close().expect("close");
    while registrations_after(&events).contains_key(&5) {
        events.push(rx.recv_timeout(Duration::from_secs(5)).expect("the window is unregistered"));
    }
}

#[test]
fn an_undescribed_prefix_greys_nothing_and_unity_actions_run_where_the_menubar_is_exported() {
    let bus = private_bus();
    let server = bus.connect();
    let log = Log::default();
    let objects = server.object_server();
    objects.at("/test/unity", UnityMenus).expect("menus");
    objects.at("/test/unity", FakeActions { prefix: "unity", log: log.clone() }).expect("unity actions");
    objects.at("/test/app", FakeActions { prefix: "app", log: log.clone() }).expect("app actions");
    let source = Source::Gtk {
        bus: server.unique_name().expect("unique name").to_string(),
        menubar: "/test/unity".into(),
        app: Some("/test/app".into()),
        window: Some("/test/nowhere".into()),
    };
    let (handle, rx) = private_client(&bus, false);

    handle.watch(source.clone());
    let bar = menu_of(&rx, &source);
    let items: Vec<(&str, bool)> = bar[0].children.iter().map(|item| (item.label.as_str(), item.enabled)).collect();
    assert_eq!(items, [("Quit", true), ("Wrap", true), ("Copy", true)], "win.* failed to describe, app.* did not");

    handle.activate(source.clone(), bar[0].children[2].activation.clone());
    wait_for(&log, "unity.copy");
    handle.activate_with_token(source, bar[0].children[0].activation.clone(), Some("token-1".into()));
    wait_for(&log, "app.quit token=token-1 startup=token-1");
}

#[test]
fn a_gtk_menubar_whose_first_start_fails_is_fetched_once_more_on_its_own() {
    let bus = private_bus();
    let server = bus.connect();
    let objects = server.object_server();
    objects.at("/test/menubar", FlakyMenus { failures: AtomicUsize::new(1) }).expect("menus");
    objects.at("/test/app", FakeActions { prefix: "app", log: Log::default() }).expect("app actions");
    let source = Source::Gtk {
        bus: server.unique_name().expect("unique name").to_string(),
        menubar: "/test/menubar".into(),
        app: Some("/test/app".into()),
        window: None,
    };
    let (handle, rx) = private_client(&bus, false);

    handle.watch(source.clone());
    gone_of(&rx, &source);
    assert_eq!(menu_of(&rx, &source)[0].label, "File", "nothing watched it again, the worker retried by itself");
}

#[test]
fn a_menu_that_never_answers_about_to_show_costs_one_timeout_not_one_per_top_level_menu() {
    let bus = private_bus();
    let server = bus.connect();
    let log = Log::default();
    server.object_server().at("/test/dbusmenu", SlowPrimeDbusMenu { log: log.clone() }).expect("dbusmenu");
    let source = dbusmenu_source(&server, "/test/dbusmenu");
    let (handle, rx) = private_client(&bus, false);

    let started = Instant::now();
    handle.watch(source.clone());
    let bar = menu_of(&rx, &source);
    assert_eq!(bar.len(), 4);
    assert!(
        started.elapsed() < glide_appmenu::METHOD_TIMEOUT + Duration::from_secs(2),
        "priming took {:?}",
        started.elapsed()
    );
    assert_eq!(about_to_show_calls(&log), 1, "priming stops at the first call that times out");
}

#[test]
fn a_menu_that_supports_about_to_show_group_is_primed_in_one_call() {
    let bus = private_bus();
    let server = bus.connect();
    let log = Log::default();
    server.object_server().at("/test/dbusmenu", GroupDbusMenu { log: log.clone() }).expect("dbusmenu");
    let source = dbusmenu_source(&server, "/test/dbusmenu");
    let (handle, rx) = private_client(&bus, false);

    handle.watch(source.clone());
    menu_of(&rx, &source);
    assert_eq!(*log.lock().expect("log"), ["group:[1, 2, 3, 4]"]);
}

#[test]
fn a_closed_submenu_is_reported_to_the_dbusmenu_client() {
    let bus = private_bus();
    let server = bus.connect();
    let log = Log::default();
    server.object_server().at("/test/dbusmenu", FakeDbusMenu { log: log.clone() }).expect("dbusmenu");
    let source = dbusmenu_source(&server, "/test/dbusmenu");
    let (handle, rx) = private_client(&bus, false);

    handle.watch(source.clone());
    menu_of(&rx, &source);
    handle.closed(source, Activation::DbusMenu(3));
    wait_for(&log, "3:closed");
}

#[test]
fn the_registrar_does_not_wait_in_the_queue_behind_another_owner() {
    let bus = private_bus();
    let other = bus.connect();
    other.request_name(REGISTRAR_NAME).expect("another registrar owns the name");
    let (_handle, _rx) = private_client(&bus, true);
    let caller = bus.connect();
    let dbus = zbus::blocking::fdo::DBusProxy::new(&caller).expect("bus proxy");
    let name = zbus::names::BusName::try_from(REGISTRAR_NAME).expect("a bus name");
    let owner = dbus.get_name_owner(name).expect("the registrar has an owner");
    assert_eq!(owner.as_str(), other.unique_name().expect("unique name").as_str());
    let queued = dbus
        .list_queued_owners(zbus::names::WellKnownName::try_from(REGISTRAR_NAME).expect("a well-known name"))
        .expect("queued owners");
    assert_eq!(queued.len(), 1, "Glide must not sit in the queue unnoticed: {queued:?}");
}

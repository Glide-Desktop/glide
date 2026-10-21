use std::io::BufRead;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_channel::mpsc::UnboundedReceiver;
use glide_app::menubar::{self, Entry, Event, Handles};
use glide_appmenu::{Activation, AppMenuEvent, MenuItem, Source, Toggle};

fn poll<T>(receiver: &mut UnboundedReceiver<T>, deadline: Instant) -> Option<T> {
    while Instant::now() < deadline {
        if let Ok(value) = receiver.try_recv() {
            return Some(value);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

fn wait_for<T>(found: &Mutex<Vec<T>>, test: impl Fn(&T) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if found.lock().expect("lock").iter().any(&test) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn the_shell_reads_the_exported_menu_bar_and_clicks_come_back_as_action_ids() {
    let Ok(mut daemon) = std::process::Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        eprintln!("no dbus-daemon; skipping");
        return;
    };
    let mut address = String::new();
    if let Some(out) = daemon.stdout.take() {
        let _ = std::io::BufReader::new(out).read_line(&mut address);
    }
    // SAFETY: this test binary runs this single test, and no other thread exists before this point.
    unsafe { std::env::set_var("DBUS_SESSION_BUS_ADDRESS", address.trim()) };

    let mut events = menubar::events(&Handles { display: 0, surface: 0 });
    let deadline = Instant::now() + Duration::from_secs(5);
    let publisher = loop {
        match poll(&mut events, deadline) {
            Some(Event::Ready(publisher)) => break publisher,
            Some(_) => {}
            None => panic!("the menu bar never came up"),
        }
    };
    publisher.set(vec![Entry::Submenu {
        label: "Файл".into(),
        enabled: true,
        children: vec![
            Entry::Action {
                id: 7,
                label: "Зберегти".into(),
                enabled: true,
                check: None,
                shortcut: Some("Ctrl+S".into()),
            },
            Entry::Separator,
            Entry::Action {
                id: 9,
                label: "Бічна панель".into(),
                enabled: true,
                check: Some(true),
                shortcut: Some("F9".into()),
            },
        ],
    }]);

    let probe = zbus::blocking::Connection::session().expect("probe connection");
    let owners = zbus::blocking::fdo::DBusProxy::new(&probe).expect("bus proxy").list_names().expect("names");
    let bus = owners
        .into_iter()
        .map(|name| name.to_string())
        .find(|name| {
            name.starts_with(':')
                && probe
                    .call_method(
                        Some(name.as_str()),
                        menubar::PATH,
                        Some("com.canonical.dbusmenu"),
                        "AboutToShow",
                        &(0_i32,),
                    )
                    .is_ok()
        })
        .expect("the menu bar owner");

    let seen: Arc<Mutex<Vec<AppMenuEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let shell =
        glide_appmenu::start(Arc::new(move |event| sink.lock().expect("lock").push(event)), false).expect("shell");
    let source = Source::DbusMenu { bus, path: menubar::PATH.into() };
    shell.watch(source.clone());
    assert!(wait_for(&seen, |event| matches!(event, AppMenuEvent::Menu { .. })), "the shell never read the menu");
    let items: Vec<MenuItem> = seen
        .lock()
        .expect("lock")
        .iter()
        .find_map(|event| match event {
            AppMenuEvent::Menu { items, .. } => Some(items.clone()),
            _ => None,
        })
        .expect("menu");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "Файл");
    let rows = &items[0].children;
    assert_eq!(rows.len(), 3);
    assert_eq!((rows[0].label.as_str(), rows[0].accel.as_deref()), ("Зберегти", Some("Ctrl+S")));
    assert_eq!(rows[2].toggle, Toggle::Check(true));
    let Activation::DbusMenu(id) = rows[0].activation.clone() else { panic!("a dbusmenu row") };

    shell.activate(source, Activation::DbusMenu(id));
    let clicked = loop {
        match poll(&mut events, Instant::now() + Duration::from_secs(5)) {
            Some(Event::Activated(action)) => break action,
            Some(_) => {}
            None => panic!("the click never reached the app"),
        }
    };
    assert_eq!(clicked, 7);
    let _ = daemon.kill();
}

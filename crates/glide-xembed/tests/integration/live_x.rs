use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use glide_tray::{TrayEvent, TrayItem};
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xproto::{
            AtomEnum, ClientMessageEvent, ColormapAlloc, ConnectionExt as _, CreateGCAux, CreateWindowAux, EventMask,
            PropMode, Rectangle, VisualClass, WindowClass,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

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
        .ok()?;
    let mut address = String::new();
    BufReader::new(child.stdout.take()?).read_line(&mut address).ok()?;
    let address = address.trim().to_string();
    (!address.is_empty()).then_some(Daemon { child, address })
}

fn next_event(rx: &mpsc::Receiver<TrayEvent>) -> TrayEvent {
    rx.recv_timeout(Duration::from_secs(5)).expect("the tray reports within 5 s")
}

fn next_item(rx: &mpsc::Receiver<TrayEvent>) -> TrayItem {
    loop {
        if let TrayEvent::Item(item) = next_event(rx) {
            return *item;
        }
    }
}

fn wait_for(conn: &RustConnection, what: &str, mut wanted: impl FnMut(&Event) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        while let Some(event) = conn.poll_for_event().expect("X connection") {
            if wanted(&event) {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {what}");
}

struct IconClient {
    conn: RustConnection,
    window: u32,
}

fn icon_client(display: &str) -> IconClient {
    let (conn, screen_num) = x11rb::connect(Some(display)).expect("the X server");
    let screen = conn.setup().roots[screen_num].clone();
    let visual = screen
        .allowed_depths
        .iter()
        .filter(|depth| depth.depth == 32)
        .flat_map(|depth| depth.visuals.iter())
        .find(|visual| visual.class == VisualClass::TRUE_COLOR)
        .expect("a 32-bit visual")
        .visual_id;
    let colormap = conn.generate_id().expect("id");
    conn.create_colormap(ColormapAlloc::NONE, colormap, screen.root, visual).expect("colormap");
    let window = conn.generate_id().expect("id");
    let events = EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::STRUCTURE_NOTIFY;
    conn.create_window(
        32,
        window,
        screen.root,
        0,
        0,
        16,
        16,
        0,
        WindowClass::INPUT_OUTPUT,
        visual,
        &CreateWindowAux::new().background_pixel(0).border_pixel(0).colormap(colormap).event_mask(events),
    )
    .expect("icon window");
    let info = conn.intern_atom(false, b"_XEMBED_INFO").expect("atom").reply().expect("atom").atom;
    conn.change_property32(PropMode::REPLACE, window, info, info, &[0, 1]).expect("xembed info");
    conn.change_property8(PropMode::REPLACE, window, AtomEnum::WM_NAME, AtomEnum::STRING, b"Steam").expect("name");
    conn.change_property8(PropMode::REPLACE, window, AtomEnum::WM_CLASS, AtomEnum::STRING, b"steam.exe\0steam.exe\0")
        .expect("class");
    let selection = conn
        .intern_atom(false, format!("_NET_SYSTEM_TRAY_S{screen_num}").as_bytes())
        .expect("atom")
        .reply()
        .expect("atom")
        .atom;
    let deadline = Instant::now() + Duration::from_secs(5);
    let owner = loop {
        let owner = conn.get_selection_owner(selection).expect("owner").reply().expect("owner").owner;
        if owner != x11rb::NONE {
            break owner;
        }
        assert!(Instant::now() < deadline, "the bridge never took the tray selection");
        std::thread::sleep(Duration::from_millis(20));
    };
    let opcode = conn.intern_atom(false, b"_NET_SYSTEM_TRAY_OPCODE").expect("atom").reply().expect("atom").atom;
    let dock = ClientMessageEvent::new(32, owner, opcode, [x11rb::CURRENT_TIME, 0, window, 0, 0]);
    conn.send_event(false, owner, EventMask::NO_EVENT, dock).expect("dock request");
    conn.flush().expect("flush");
    IconClient { conn, window }
}

#[test]
#[ignore = "needs an X server without a tray: GLIDE_XEMBED_DISPLAY=:99 next to `Xwayland :99 -rootless`"]
fn an_embedded_icon_reaches_the_tray_and_takes_its_clicks() {
    let Ok(display) = std::env::var("GLIDE_XEMBED_DISPLAY") else {
        eprintln!("GLIDE_XEMBED_DISPLAY is not set; skipping");
        return;
    };
    let Some(daemon) = private_bus() else {
        eprintln!("no dbus-daemon; skipping");
        return;
    };
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let sink: glide_tray::Sink = Arc::new(move |event| {
        let _ = tx.lock().expect("sink").send(event);
    });
    let tray = glide_tray::start_on(&daemon.address, sink).expect("the tray host starts");

    let control = zbus::blocking::connection::Builder::address(daemon.address.as_str())
        .expect("address")
        .build()
        .expect("control connection");
    let registered = Arc::new(Mutex::new(Vec::new()));
    let host = glide_xembed::tray::Host::connect(Some(&display), Some(daemon.address.clone()), control, registered)
        .expect("the bridge owns the tray");
    std::thread::spawn(move || host.run());

    let icon = icon_client(&display);
    wait_for(
        &icon.conn,
        "the icon to be embedded",
        |event| matches!(event, Event::ReparentNotify(moved) if moved.window == icon.window),
    );
    wait_for(&icon.conn, "the icon to be mapped", |event| matches!(event, Event::MapNotify(_)));
    let gc = icon.conn.generate_id().expect("id");
    icon.conn.create_gc(gc, icon.window, &CreateGCAux::new().foreground(0xff_ff_00_00)).expect("gc");
    icon.conn.poly_fill_rectangle(icon.window, gc, &[Rectangle { x: 0, y: 0, width: 64, height: 64 }]).expect("paint");
    icon.conn.flush().expect("flush");

    let item = next_item(&rx);
    assert_eq!(item.props.id, "steam.exe");
    assert_eq!(item.props.title, "Steam");
    let pixmap = item.props.icon_pixmaps.first().expect("a pixmap");
    assert_eq!((pixmap.width, pixmap.height), (32, 32), "resized to the bridge's icon size");
    assert_eq!(&pixmap.argb[..4], &[0xff, 0xff, 0, 0], "opaque red, straight ARGB");

    tray.activate_with_token(&item.key, 10, 10, None);
    wait_for(
        &icon.conn,
        "a left click",
        |event| matches!(event, Event::ButtonRelease(release) if release.detail == 1 && release.event == icon.window),
    );
    tray.context_menu(&item.key, 10, 10);
    wait_for(&icon.conn, "a right click", |event| matches!(event, Event::ButtonPress(press) if press.detail == 3));

    icon.conn.destroy_window(icon.window).expect("destroy");
    icon.conn.flush().expect("flush");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "the item outlived its window");
        if let TrayEvent::Removed { key } = next_event(&rx)
            && key == item.key
        {
            break;
        }
    }
}

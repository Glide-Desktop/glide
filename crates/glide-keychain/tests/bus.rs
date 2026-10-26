use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use glide_ipc::keychain::{AppLink, Draft, Hidden, LOCAL, Request, Response, SecretKind};
use glide_keychain::bus::{self, Keychain};
use glide_keychain::daemon::Daemon;
use glide_keychain::service::Service;
use glide_keychain::vault::Params;

const FAST: Params = Params { memory_kib: 64, passes: 1, lanes: 1 };

struct Bus {
    child: Child,
    address: String,
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn private_bus() -> Option<Bus> {
    let mut child = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .inspect_err(|err| eprintln!("no dbus-daemon ({err}); skipping"))
        .ok()?;
    let mut address = String::new();
    BufReader::new(child.stdout.take()?).read_line(&mut address).ok()?;
    Some(Bus { child, address: address.trim().to_string() })
}

fn builder(bus: &Bus) -> zbus::blocking::connection::Builder<'_> {
    zbus::blocking::connection::Builder::address(bus.address.as_str()).unwrap()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("glide-keychain-bus-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("keychain")
}

fn call(proxy: &zbus::blocking::Proxy<'_>, request: &Request) -> Response {
    let reply: String = proxy.call("Call", &(serde_json::to_string(request).unwrap(),)).unwrap();
    serde_json::from_str(&reply).unwrap()
}

#[test]
fn the_passwords_app_manages_items_over_the_bus() {
    let Some(bus) = private_bus() else {
        return;
    };
    let path = temp("manage");
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&events);
    let keychain = Keychain::new(
        Service::start(Daemon::new(path.clone(), FAST), Arc::new(|_| {})),
        Arc::new(move |event| seen.lock().unwrap().push(event)),
        vec![std::env::current_exe().unwrap().canonicalize().unwrap()],
    );
    let _server = bus::serve_on(builder(&bus), keychain).unwrap();
    let client = builder(&bus).build().unwrap();
    let proxy = zbus::blocking::Proxy::new(&client, bus::BUS_NAME, bus::PATH, bus::INTERFACE).unwrap();

    assert_eq!(call(&proxy, &Request::Status), Response::Status { exists: false, unlocked: false });
    assert_eq!(call(&proxy, &Request::Create { password: Hidden::new("pw".into()) }), Response::Done);
    let draft = Draft {
        vault: LOCAL.into(),
        name: "Telegram".into(),
        username: "me".into(),
        password: Some(Hidden::new("s3cret".into())),
        apps: vec![AppLink { name: "Telegram".into(), app_id: "org.telegram.desktop".into(), program: String::new() }],
        ..Draft::default()
    };
    let Response::Saved { item } = call(&proxy, &Request::Save { draft: Box::new(draft) }) else { panic!() };
    let Response::Items { list } = call(&proxy, &Request::Items) else { panic!() };
    assert_eq!((list.len(), list[0].name.as_str(), list[0].username.as_str()), (1, "Telegram", "me"));
    assert_eq!(
        call(&proxy, &Request::Secret { item, secret: SecretKind::Password }),
        Response::Secret { secret: Hidden::new("s3cret".into()) }
    );
    assert_eq!(call(&proxy, &Request::Lock { vault: None }), Response::Done);
    assert_eq!(call(&proxy, &Request::Items), Response::Items { list: Vec::new() }, "a locked vault lists nothing");
    assert_eq!(call(&proxy, &Request::RequestUnlock { vault: Some(LOCAL.into()) }), Response::Done);
    let events = events.lock().unwrap();
    assert!(
        events.contains(&Response::UnlockRequested { vault: Some(LOCAL.into()), requester: None }),
        "the compositor shows its prompt"
    );
    assert!(events.contains(&Response::Changed), "changes are announced");
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn any_other_program_is_refused() {
    let Some(bus) = private_bus() else {
        return;
    };
    let keychain = Keychain::new(
        Service::start(Daemon::new(temp("refused"), FAST), Arc::new(|_| {})),
        Arc::new(|_| {}),
        vec![PathBuf::from("/usr/bin/glide-passwords")],
    );
    let _server = bus::serve_on(builder(&bus), keychain).unwrap();
    let client = builder(&bus).build().unwrap();
    let proxy = zbus::blocking::Proxy::new(&client, bus::BUS_NAME, bus::PATH, bus::INTERFACE).unwrap();
    let refused = proxy.call::<_, _, String>("Call", &(r#"{"type":"status"}"#,)).unwrap_err();
    assert!(refused.to_string().contains("AccessDenied"), "{refused}");
}

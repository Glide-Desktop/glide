use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use glide_ipc::keychain::Response;
use glide_keychain::bus::{self, Keychain};
use glide_keychain::daemon::Daemon;
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

type Row = (u64, String, String, String, String, u64);

#[test]
fn settings_manages_logins_over_the_bus() {
    let Some(bus) = private_bus() else {
        return;
    };
    let path = temp("manage");
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&events);
    let keychain = Keychain::new(
        Arc::new(Mutex::new(Daemon::new(path.clone(), FAST))),
        Arc::new(move |event| seen.lock().unwrap().push(event)),
        vec![std::env::current_exe().unwrap().canonicalize().unwrap()],
    );
    let _server = bus::serve_on(builder(&bus), keychain).unwrap();
    let client = builder(&bus).build().unwrap();
    let proxy = zbus::blocking::Proxy::new(&client, bus::BUS_NAME, bus::PATH, "org.glide.Keychain1").unwrap();

    let status: (bool, bool) = proxy.call("Status", &()).unwrap();
    assert_eq!(status, (false, false));
    proxy.call::<_, _, ()>("Create", &("pw",)).unwrap();
    let id: u64 = proxy.call("Add", &("Telegram", "me", "s3cret", "org.telegram.desktop", "")).unwrap();
    let rows: Vec<Row> = proxy.call("Items", &()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].0, rows[0].1.as_str(), rows[0].2.as_str(), rows[0].3.as_str()),
        (id, "Telegram", "me", "org.telegram.desktop")
    );
    let password: String = proxy.call("Password", &(id,)).unwrap();
    assert_eq!(password, "s3cret");
    proxy.call::<_, _, ()>("Update", &(id, "Telegram", "you", "", "org.telegram.desktop", "")).unwrap();
    let rows: Vec<Row> = proxy.call("Items", &()).unwrap();
    assert_eq!(rows[0].2, "you");
    proxy.call::<_, _, ()>("Delete", &(id,)).unwrap();
    proxy.call::<_, _, ()>("Lock", &()).unwrap();
    assert!(proxy.call::<_, _, Vec<Row>>("Items", &()).is_err(), "a locked keychain lists nothing");
    proxy.call::<_, _, ()>("RequestUnlock", &()).unwrap();
    assert_eq!(*events.lock().unwrap(), [Response::UnlockRequested], "the compositor is asked to show its prompt");
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn any_other_program_is_refused() {
    let Some(bus) = private_bus() else {
        return;
    };
    let keychain = Keychain::new(
        Arc::new(Mutex::new(Daemon::new(temp("refused"), FAST))),
        Arc::new(|_| {}),
        vec![PathBuf::from("/usr/bin/glide-settings")],
    );
    let _server = bus::serve_on(builder(&bus), keychain).unwrap();
    let client = builder(&bus).build().unwrap();
    let proxy = zbus::blocking::Proxy::new(&client, bus::BUS_NAME, bus::PATH, "org.glide.Keychain1").unwrap();
    let refused = proxy.call::<_, _, (bool, bool)>("Status", &()).unwrap_err();
    assert!(refused.to_string().contains("AccessDenied"), "{refused}");
}

use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};

pub struct Daemon {
    child: Child,
    pub address: String,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Daemon {
    pub fn builder(&self) -> zbus::blocking::connection::Builder<'_> {
        zbus::blocking::connection::Builder::address(self.address.as_str()).expect("a private bus address")
    }

    pub fn client(&self) -> zbus::blocking::Connection {
        self.builder().build().expect("a client on the private bus")
    }
}

#[must_use]
pub fn private_bus() -> Option<Daemon> {
    let mut child = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .inspect_err(|err| eprintln!("no dbus-daemon ({err}); skipping"))
        .ok()?;
    let mut address = String::new();
    BufReader::new(child.stdout.take()?).read_line(&mut address).ok()?;
    let daemon = Daemon { child, address: address.trim().to_string() };
    (!daemon.address.is_empty()).then_some(daemon)
}

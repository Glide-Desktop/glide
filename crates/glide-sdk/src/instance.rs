use std::sync::{Mutex, OnceLock, PoisonError};

use iced::futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use zbus::blocking::Connection;
use zbus::fdo::{RequestNameFlags, RequestNameReply};

pub const INTERFACE: &str = "org.glide.Instance1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    pub args: Vec<String>,
    pub token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    First,
    Forwarded,
    Alone,
}

struct Instance {
    sender: UnboundedSender<Opened>,
}

#[zbus::interface(name = "org.glide.Instance1")]
impl Instance {
    fn open(&self, args: Vec<String>, token: String) {
        let _ = self.sender.unbounded_send(Opened { args, token });
    }
}

struct Held {
    _connection: Connection,
    receiver: Mutex<Option<UnboundedReceiver<Opened>>>,
}

static HELD: OnceLock<Held> = OnceLock::new();

#[must_use]
pub fn object_path(name: &str) -> String {
    format!("/{}", name.replace('.', "/"))
}

#[must_use]
pub fn launch_token() -> String {
    ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|token| !token.is_empty()))
        .unwrap_or_default()
}

#[must_use]
pub fn claim_on(builder: zbus::blocking::connection::Builder<'_>, name: &str, args: &[String], token: &str) -> Claim {
    let path = object_path(name);
    let (sender, receiver) = unbounded();
    let Ok(connection) =
        builder.serve_at(path.as_str(), Instance { sender }).and_then(zbus::blocking::connection::Builder::build)
    else {
        return Claim::Alone;
    };
    match connection.request_name_with_flags(name, RequestNameFlags::DoNotQueue.into()) {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {
            let _ = HELD.set(Held { _connection: connection, receiver: Mutex::new(Some(receiver)) });
            Claim::First
        }
        Ok(_) | Err(zbus::Error::NameTaken) => forward(&connection, name, &path, args, token),
        Err(_) => Claim::Alone,
    }
}

fn forward(connection: &Connection, name: &str, path: &str, args: &[String], token: &str) -> Claim {
    match connection.call_method(Some(name), path, Some(INTERFACE), "Open", &(args.to_vec(), token)) {
        Ok(_) => Claim::Forwarded,
        Err(err) => {
            tracing::warn!("could not hand the launch to the running instance of {name}: {err}");
            Claim::Alone
        }
    }
}

#[must_use]
pub fn claim(name: &str, args: &[String], token: &str) -> Claim {
    match zbus::blocking::connection::Builder::session() {
        Ok(builder) => claim_on(builder, name, args, token),
        Err(_) => Claim::Alone,
    }
}

#[must_use]
pub fn requests() -> UnboundedReceiver<Opened> {
    HELD.get()
        .and_then(|held| held.receiver.lock().unwrap_or_else(PoisonError::into_inner).take())
        .unwrap_or_else(|| unbounded().1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_object_path_follows_the_bus_name() {
        assert_eq!(object_path("org.glide.Settings"), "/org/glide/Settings");
    }

    #[test]
    fn a_second_launch_forwards_its_arguments_and_token_to_the_first() {
        use std::io::BufRead;
        let Ok(mut daemon) = std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            return;
        };
        let mut address = String::new();
        if let Some(out) = daemon.stdout.take() {
            let _ = std::io::BufReader::new(out).read_line(&mut address);
        }
        let address = address.trim().to_string();
        let builder = || zbus::blocking::connection::Builder::address(address.as_str()).expect("address");
        let name = "org.glide.InstanceTest";
        assert_eq!(claim_on(builder(), name, &[], ""), Claim::First);
        let args = vec!["power".to_string()];
        assert_eq!(claim_on(builder(), name, &args, "token-1"), Claim::Forwarded);
        let mut opened = requests();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let got = loop {
            if let Ok(value) = opened.try_recv() {
                break Some(value);
            }
            if std::time::Instant::now() > deadline {
                break None;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let _ = daemon.kill();
        assert_eq!(got, Some(Opened { args, token: "token-1".into() }));
    }
}

use std::io::{BufRead, BufReader, Write};
use std::os::fd::FromRawFd;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use glide_ipc::keychain::{EVENT, Envelope, Request, Response};
use glide_keychain::bus;
use glide_keychain::daemon::{Daemon, default_path, mutates};
use glide_keychain::service::Service;
use glide_keychain::vault::Params;
use zeroize::Zeroizing;

const USAGE: &str = "\
Usage: glide-keychain

The Glide keychain daemon. The compositor starts it with a private socket
as standard input; it is not meant to be run by hand.";

type Writer = Arc<Mutex<UnixStream>>;

fn send(writer: &Writer, reply: &Envelope<Response>) -> bool {
    let Ok(mut line) = serde_json::to_string(reply).map(Zeroizing::new) else {
        return true;
    };
    line.push('\n');
    writer.lock().unwrap_or_else(PoisonError::into_inner).write_all(line.as_bytes()).is_ok()
}

fn main() {
    if std::env::args().nth(1).is_some() {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    let Some(path) = default_path() else {
        eprintln!("glide-keychain: neither XDG_DATA_HOME nor HOME is set");
        std::process::exit(1);
    };
    // SAFETY: the compositor hands over one end of a socketpair as fd 0, and nothing else in this process owns fd 0.
    let channel = unsafe { UnixStream::from_raw_fd(0) };
    if channel.peer_addr().is_err() {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    let writer: Writer = match channel.try_clone() {
        Ok(writer) => Arc::new(Mutex::new(writer)),
        Err(err) => {
            eprintln!("glide-keychain: {err}");
            std::process::exit(1);
        }
    };
    let events_writer = Arc::clone(&writer);
    let connection: Arc<OnceLock<zbus::blocking::Connection>> = Arc::new(OnceLock::new());
    let announcer = Arc::clone(&connection);
    let events: bus::Events = Arc::new(move |body| {
        if body == Response::Changed
            && let Some(connection) = announcer.get()
        {
            bus::announce(connection);
        }
        send(&events_writer, &Envelope { id: EVENT, body });
    });
    let service = Service::start(Daemon::new(path, Params::DEFAULT), Arc::clone(&events));
    match bus::serve(bus::Keychain::new(Arc::clone(&service), Arc::clone(&events), bus::client_program())) {
        Ok(served) => {
            let _ = connection.set(served);
        }
        Err(err) => eprintln!("glide-keychain: {} is not on the session bus ({err})", bus::BUS_NAME),
    }
    let mut reader = BufReader::new(channel);
    let mut line = Zeroizing::new(String::new());
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let reply = match serde_json::from_str::<Envelope<Request>>(&line) {
            Ok(request) => {
                let changes = mutates(&request.body);
                let body = service.handle(request.body);
                if changes && let Some(connection) = connection.get() {
                    bus::announce(connection);
                }
                Envelope { id: request.id, body }
            }
            Err(err) => Envelope { id: EVENT, body: Response::Error { message: format!("unreadable request: {err}") } },
        };
        if !send(&writer, &reply) {
            return;
        }
    }
}

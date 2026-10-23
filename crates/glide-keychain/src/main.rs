use std::io::{BufRead, BufReader, Write};
use std::os::fd::FromRawFd;
use std::os::unix::net::UnixStream;

use glide_ipc::keychain::{Envelope, Request, Response};
use glide_keychain::daemon::{Daemon, default_path};
use glide_keychain::vault::Params;
use zeroize::Zeroizing;

const USAGE: &str = "\
Usage: glide-keychain

The Glide keychain daemon. The compositor starts it with a private socket
as standard input; it is not meant to be run by hand.";

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
    let mut writer = match channel.try_clone() {
        Ok(writer) => writer,
        Err(err) => {
            eprintln!("glide-keychain: {err}");
            std::process::exit(1);
        }
    };
    let mut daemon = Daemon::new(path, Params::DEFAULT);
    let mut reader = BufReader::new(channel);
    let mut line = Zeroizing::new(String::new());
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let reply = match serde_json::from_str::<Envelope<Request>>(&line) {
            Ok(request) => Envelope { id: request.id, body: daemon.handle(request.body) },
            Err(err) => Envelope { id: 0, body: Response::Error { message: format!("unreadable request: {err}") } },
        };
        let Ok(mut out) = serde_json::to_string(&reply).map(Zeroizing::new) else {
            continue;
        };
        out.push('\n');
        if writer.write_all(out.as_bytes()).is_err() {
            return;
        }
    }
}

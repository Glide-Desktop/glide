use std::io::{BufRead, BufReader, Write};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};

use glide_ipc::keychain::{Envelope, Hidden, Request, Response};

struct Daemon {
    child: std::process::Child,
    writer: UnixStream,
    reader: BufReader<UnixStream>,
    next: u64,
}

impl Daemon {
    fn start(data_home: &std::path::Path) -> Self {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_glide-keychain"))
            .env("XDG_DATA_HOME", data_home)
            .stdin(Stdio::from(OwnedFd::from(theirs)))
            .spawn()
            .unwrap();
        let reader = BufReader::new(ours.try_clone().unwrap());
        Self { child, writer: ours, reader, next: 0 }
    }

    fn ask(&mut self, body: Request) -> Response {
        self.next += 1;
        let mut line = serde_json::to_string(&Envelope { id: self.next, body }).unwrap();
        line.push('\n');
        self.writer.write_all(line.as_bytes()).unwrap();
        let mut reply = String::new();
        self.reader.read_line(&mut reply).unwrap();
        let reply: Envelope<Response> = serde_json::from_str(&reply).unwrap();
        assert_eq!(reply.id, self.next, "replies come back in order with their request id");
        reply.body
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn the_daemon_answers_over_the_socket_it_was_given_as_stdin() {
    let home = std::env::temp_dir().join(format!("glide-keychain-channel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let mut daemon = Daemon::start(&home);
    assert_eq!(daemon.ask(Request::Status), Response::Status { exists: false, unlocked: false });
    assert_eq!(daemon.ask(Request::Create { password: Hidden::new("pw".into()) }), Response::Done);
    assert!(home.join("glide/keychain").exists());
    assert_eq!(daemon.ask(Request::Lock), Response::Done);
    assert_eq!(daemon.ask(Request::Unlock { password: Hidden::new("wrong".into()) }), Response::WrongPassword);
    assert_eq!(daemon.ask(Request::Unlock { password: Hidden::new("pw".into()) }), Response::Done);
    assert_eq!(
        daemon.ask(Request::Candidates { program: Some("/usr/bin/x".into()), app_id: None }),
        Response::Candidates { list: vec![] }
    );
    drop(daemon);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn the_daemon_refuses_to_run_without_a_socket() {
    let status = Command::new(env!("CARGO_BIN_EXE_glide-keychain")).stdin(Stdio::null()).status().unwrap();
    assert_eq!(status.code(), Some(2));
}

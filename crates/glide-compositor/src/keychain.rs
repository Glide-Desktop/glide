use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Instant;

use glide_ipc::keychain::{Envelope, Hidden, Request, Response};
use zeroize::Zeroizing;

use crate::agent_prompt::{Answer, Prompt};
use crate::i18n::{Key, t};
use crate::state::GlideState;

const PROGRAM: &str = "glide-keychain";
const OWNER: &str = "keychain";

type Reply = Box<dyn FnOnce(&mut GlideState, Response)>;

#[derive(Default)]
pub struct Keychain {
    sender: Option<mpsc::Sender<Envelope<Request>>>,
    next: u64,
    waiting: HashMap<u64, Reply>,
}

impl Keychain {
    pub fn start(send: impl Fn(Envelope<Response>) + Send + 'static) -> std::io::Result<Self> {
        let (sender, incoming) = mpsc::channel();
        std::thread::Builder::new().name("glide-keychain-link".into()).spawn(move || link(&incoming, &send))?;
        Ok(Self { sender: Some(sender), ..Self::default() })
    }
}

struct Running {
    child: Child,
    writer: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Running {
    fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn program() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(PROGRAM)))
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from(PROGRAM))
}

fn launch() -> std::io::Result<Running> {
    let (ours, theirs) = UnixStream::pair()?;
    let child = Command::new(program()).stdin(Stdio::from(OwnedFd::from(theirs))).stdout(Stdio::null()).spawn()?;
    tracing::info!(pid = child.id(), "started the keychain");
    let reader = BufReader::new(ours.try_clone()?);
    Ok(Running { child, writer: ours, reader })
}

fn link(incoming: &mpsc::Receiver<Envelope<Request>>, send: &impl Fn(Envelope<Response>)) {
    let mut running = None;
    let mut restarts = crate::xwayland::Restarts::default();
    for request in incoming {
        let body =
            exchange(&mut running, &mut restarts, &request).unwrap_or_else(|message| Response::Error { message });
        send(Envelope { id: request.id, body });
    }
    if let Some(running) = running {
        running.stop();
    }
}

fn exchange(
    running: &mut Option<Running>, restarts: &mut crate::xwayland::Restarts, request: &Envelope<Request>,
) -> Result<Response, String> {
    if running.is_none() {
        if !restarts.allow(Instant::now()) {
            return Err("the keychain keeps stopping".into());
        }
        *running = Some(launch().map_err(|err| format!("could not start {PROGRAM}: {err}"))?);
    }
    let Some(daemon) = running.as_mut() else {
        return Err("the keychain is not running".into());
    };
    let mut line = Zeroizing::new(serde_json::to_string(request).map_err(|err| err.to_string())?);
    line.push('\n');
    let mut reply = Zeroizing::new(String::new());
    let exchanged = daemon.writer.write_all(line.as_bytes()).and_then(|()| match daemon.reader.read_line(&mut reply) {
        Ok(0) => Err(std::io::ErrorKind::UnexpectedEof.into()),
        other => other,
    });
    if let Err(err) = exchanged {
        if let Some(dead) = running.take() {
            dead.stop();
        }
        return Err(format!("the keychain stopped: {err}"));
    }
    serde_json::from_str::<Envelope<Response>>(&reply).map(|envelope| envelope.body).map_err(|err| err.to_string())
}

impl GlideState {
    pub fn keychain(&mut self, request: Request, reply: impl FnOnce(&mut Self, Response) + 'static) {
        let keychain = &mut self.session.keychain;
        keychain.next += 1;
        let id = keychain.next;
        let sent = keychain.sender.as_ref().is_some_and(|sender| sender.send(Envelope { id, body: request }).is_ok());
        if sent {
            keychain.waiting.insert(id, Box::new(reply));
        } else {
            reply(self, Response::Error { message: "the keychain link is not running".into() });
        }
    }

    pub fn handle_keychain(&mut self, reply: Envelope<Response>) {
        if let Some(callback) = self.session.keychain.waiting.remove(&reply.id) {
            callback(self, reply.body);
        }
    }

    pub fn lock_keychain(&mut self) {
        self.keychain(Request::Lock, |_, _| {});
    }

    pub fn unlock_keychain(&mut self, wrong: bool, then: impl FnOnce(&mut Self) + 'static) {
        let prompt = Prompt {
            icon: "dialog-password",
            summary: t(Key::KeychainUnlock).to_string(),
            body: t(if wrong { Key::KeychainWrong } else { Key::KeychainUnlockBody }).to_string(),
            accept: None,
            reply: Some((t(Key::KeychainUnlockAction).to_string(), t(Key::KeychainPlaceholder).to_string())),
            secret: true,
        };
        self.ask(OWNER, &prompt, move |state, answer| {
            let Some(Answer::Text(password)) = answer else {
                return;
            };
            state.keychain(
                Request::Unlock { password: Hidden::new(password) },
                move |state, response| match response {
                    Response::Done => then(state),
                    Response::WrongPassword => state.unlock_keychain(true, then),
                    other => tracing::warn!(?other, "the keychain did not unlock"),
                },
            );
        });
    }
}

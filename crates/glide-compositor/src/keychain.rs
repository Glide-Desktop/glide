use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::Instant;

use glide_ipc::keychain::{EVENT, Envelope, Hidden, LOGIN, Request, Response, SyncState, Vault, VaultKind};
use zeroize::Zeroizing;

use crate::state::GlideState;

const PROGRAM: &str = "glide-keychain";

type Reply = Box<dyn FnOnce(&mut GlideState, Response)>;

#[derive(Default)]
pub struct Keychain {
    sender: Option<mpsc::Sender<Envelope<Request>>>,
    next: u64,
    waiting: HashMap<u64, Reply>,
    unlocking: Option<Chain>,
    after_unlock: Vec<Unlocked>,
    pub revision: u64,
    pub prompt: Option<crate::keychain_prompt::Prompt>,
}

#[derive(Debug, Clone, Default)]
pub struct Chain {
    pub vaults: Vec<Vault>,
    pub index: usize,
    pub wrong: bool,
    pub unlocked: Vec<String>,
    pub requester: Option<String>,
    pub login: bool,
}

impl Chain {
    #[must_use]
    pub fn current(&self) -> Option<&Vault> {
        self.vaults.get(self.index)
    }
}

#[must_use]
pub fn locked_chain(vaults: Vec<Vault>, only: Option<&str>) -> Vec<Vault> {
    vaults
        .into_iter()
        .filter(|vault| vault.exists && !vault.unlocked && only.is_none_or(|only| vault.id == only))
        .collect()
}

fn login_vault() -> Vault {
    Vault {
        id: LOGIN.into(),
        name: crate::i18n::t(crate::i18n::Key::KcLoginName).to_string(),
        kind: VaultKind::Local,
        account: String::new(),
        server: String::new(),
        exists: true,
        unlocked: false,
        sync: SyncState::Local,
        count: 0,
    }
}

fn join_login(chain: &mut Chain) {
    if chain.login && !chain.vaults.is_empty() && !chain.vaults.iter().any(|vault| vault.id == LOGIN) {
        chain.vaults.push(login_vault());
    }
}

type Unlocked = Box<dyn FnOnce(&mut GlideState)>;

type Send = Arc<dyn Fn(Envelope<Response>) + std::marker::Send + Sync>;

impl Keychain {
    pub fn start(send: impl Fn(Envelope<Response>) + std::marker::Send + Sync + 'static) -> std::io::Result<Self> {
        let (sender, incoming) = mpsc::channel();
        let send: Send = Arc::new(send);
        std::thread::Builder::new().name("glide-keychain-link".into()).spawn(move || link(&incoming, &send))?;
        Ok(Self { sender: Some(sender), ..Self::default() })
    }
}

struct Running {
    child: Child,
    writer: UnixStream,
}

impl Running {
    fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn exited(&mut self) -> bool {
        !matches!(self.child.try_wait(), Ok(None))
    }
}

fn program() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(PROGRAM)))
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from(PROGRAM))
}

fn launch(send: &Send) -> std::io::Result<Running> {
    let (ours, theirs) = UnixStream::pair()?;
    let child = Command::new(program()).stdin(Stdio::from(OwnedFd::from(theirs))).stdout(Stdio::null()).spawn()?;
    tracing::info!(pid = child.id(), "started the keychain");
    let reader = BufReader::new(ours.try_clone()?);
    let send = Arc::clone(send);
    std::thread::Builder::new().name("glide-keychain-read".into()).spawn(move || read(reader, &send))?;
    Ok(Running { child, writer: ours })
}

fn read(mut reader: BufReader<UnixStream>, send: &Send) {
    let mut line = Zeroizing::new(String::new());
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => match serde_json::from_str::<Envelope<Response>>(&line) {
                Ok(envelope) => send(envelope),
                Err(err) => tracing::warn!(%err, "the keychain sent an unreadable line"),
            },
        }
    }
    send(Envelope { id: EVENT, body: Response::Stopped });
}

fn link(incoming: &mpsc::Receiver<Envelope<Request>>, send: &Send) {
    let mut running: Option<Running> = None;
    let mut restarts = crate::xwayland::Restarts::default();
    for request in incoming {
        if let Err(message) = deliver(&mut running, &mut restarts, send, &request) {
            send(Envelope { id: request.id, body: Response::Error { message } });
        }
    }
    if let Some(running) = running {
        running.stop();
    }
}

fn deliver(
    running: &mut Option<Running>, restarts: &mut crate::xwayland::Restarts, send: &Send, request: &Envelope<Request>,
) -> Result<(), String> {
    if running.as_mut().is_some_and(Running::exited)
        && let Some(dead) = running.take()
    {
        dead.stop();
    }
    if running.is_none() {
        if !restarts.allow(Instant::now()) {
            return Err("the keychain keeps stopping".into());
        }
        *running = Some(launch(send).map_err(|err| format!("could not start {PROGRAM}: {err}"))?);
    }
    let Some(daemon) = running.as_mut() else {
        return Err("the keychain is not running".into());
    };
    let mut line = Zeroizing::new(serde_json::to_string(request).map_err(|err| err.to_string())?);
    line.push('\n');
    if let Err(err) = daemon.writer.write_all(line.as_bytes()) {
        if let Some(dead) = running.take() {
            dead.stop();
        }
        return Err(format!("the keychain stopped: {err}"));
    }
    Ok(())
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
        if reply.id != EVENT {
            if let Some(callback) = self.session.keychain.waiting.remove(&reply.id) {
                callback(self, reply.body);
            }
            return;
        }
        match reply.body {
            Response::Stopped => {
                tracing::warn!("the keychain stopped; starting it again");
                for (_, callback) in std::mem::take(&mut self.session.keychain.waiting) {
                    callback(self, Response::Error { message: "the keychain stopped".into() });
                }
                self.sync_passwords_applet();
            }
            Response::UnlockRequested { vault, requester } => {
                let app = requester.map(|program| self.program_name(&program));
                self.unlock_keychain_for(vault, app, |_| {});
            }
            Response::Changed => {
                self.session.keychain.revision += 1;
                self.sync_passwords_applet();
            }
            other => tracing::debug!(?other, "an unexpected keychain event"),
        }
    }

    pub fn lock_keychain(&mut self) {
        self.keychain(Request::Lock { vault: None }, |state, _| state.sync_passwords_applet());
    }

    pub fn unlock_keychain(&mut self, vault: Option<String>, then: impl FnOnce(&mut Self) + 'static) {
        self.unlock_keychain_for(vault, None, then);
    }

    fn program_name(&self, program: &str) -> String {
        let entry = self.app_directory.index().and_then(|index| index.match_process(Some(program), None));
        entry.map(|app| app.name.clone()).filter(|name| !name.trim().is_empty()).unwrap_or_else(|| {
            std::path::Path::new(program)
                .file_name()
                .map_or_else(|| program.to_string(), |name| name.to_string_lossy().into_owned())
        })
    }

    pub fn unlock_keychain_for(
        &mut self, vault: Option<String>, requester: Option<String>, then: impl FnOnce(&mut Self) + 'static,
    ) {
        self.session.keychain.after_unlock.push(Box::new(then));
        if let Some(chain) = self.session.keychain.unlocking.as_mut() {
            if vault.as_deref() == Some(LOGIN) {
                chain.login = true;
                join_login(chain);
                self.queue_redraw();
            }
            return;
        }
        self.session.keychain.unlocking = Some(Chain::default());
        if vault.as_deref() == Some(LOGIN) {
            let vaults = vec![login_vault()];
            self.session.keychain.unlocking = Some(Chain { vaults, requester, ..Chain::default() });
            self.ask_keychain_password();
            return;
        }
        self.keychain(Request::Vaults, move |state, response| {
            let Response::Vaults { list, .. } = response else {
                state.finish_keychain_unlock(false);
                return;
            };
            let login = state.session.keychain.unlocking.as_ref().is_some_and(|chain| chain.login);
            let mut vaults = locked_chain(list, vault.as_deref());
            if vaults.is_empty() && login {
                vaults.push(login_vault());
            }
            if vaults.is_empty() {
                tracing::info!(
                    vault = vault.as_deref().unwrap_or("-"),
                    "nothing to unlock; the request is answered as done"
                );
                state.finish_keychain_unlock(true);
                return;
            }
            let mut chain = Chain { vaults, requester, login, ..Chain::default() };
            join_login(&mut chain);
            state.session.keychain.unlocking = Some(chain);
            state.ask_keychain_password();
        });
    }

    #[must_use]
    pub fn unlock_chain(&self) -> Option<&Chain> {
        self.session.keychain.unlocking.as_ref().filter(|chain| !chain.vaults.is_empty())
    }

    fn ask_keychain_password(&mut self) {
        let Some(chain) = self.session.keychain.unlocking.as_ref() else {
            return;
        };
        if chain.current().is_none() {
            let any = !chain.unlocked.is_empty();
            self.finish_keychain_unlock(any);
            return;
        }
        if self.screen_locked() {
            self.finish_keychain_unlock(false);
            return;
        }
        self.open_keychain_prompt();
    }

    pub fn abort_keychain_unlock(&mut self) {
        if self.session.keychain.unlocking.is_some() {
            self.finish_keychain_unlock(false);
        }
    }

    pub fn submit_vault_password(&mut self, password: Hidden) {
        let Some(vault) = self.unlock_chain().and_then(Chain::current).map(|vault| vault.id.clone()) else {
            return;
        };
        self.keychain(Request::Unlock { vault: vault.clone(), password }, move |state, response| {
            let Some(chain) = state.session.keychain.unlocking.as_mut() else {
                return;
            };
            match response {
                Response::Done => {
                    chain.unlocked.push(vault);
                    chain.index += 1;
                    chain.wrong = false;
                    state.ask_keychain_password();
                }
                Response::WrongPassword => {
                    chain.wrong = true;
                    state.ask_keychain_password();
                }
                other => {
                    tracing::warn!(?other, "a vault did not unlock");
                    state.skip_vault();
                }
            }
        });
    }

    pub fn skip_vault(&mut self) {
        if let Some(chain) = self.session.keychain.unlocking.as_mut() {
            chain.index += 1;
            chain.wrong = false;
        }
        self.ask_keychain_password();
    }

    pub fn cancel_keychain_unlock(&mut self) {
        let any = self.session.keychain.unlocking.as_ref().is_some_and(|chain| !chain.unlocked.is_empty());
        self.finish_keychain_unlock(any);
    }

    fn finish_keychain_unlock(&mut self, unlocked: bool) {
        self.session.keychain.unlocking = None;
        self.close_keychain_prompt();
        self.keychain(Request::PromptClosed, |_, _| {});
        let waiting = std::mem::take(&mut self.session.keychain.after_unlock);
        if self.topbar.open_popup() != Some(crate::topbar::OpenPopup::Passwords) {
            self.sync_passwords_applet();
        }
        if unlocked {
            for then in waiting {
                then(self);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glide_ipc::keychain::{SyncState, VaultKind};

    #[test]
    fn a_program_asking_during_another_unlock_joins_that_chain_once() {
        let mut chain = Chain { vaults: vec![vault("bw-1", true, false)], login: true, ..Chain::default() };
        join_login(&mut chain);
        join_login(&mut chain);
        let ids: Vec<&str> = chain.vaults.iter().map(|vault| vault.id.as_str()).collect();
        assert_eq!(ids, ["bw-1", LOGIN]);
        let mut waiting = Chain { login: true, ..Chain::default() };
        join_login(&mut waiting);
        assert!(waiting.vaults.is_empty(), "a chain still waiting for its vault list takes the app keys on arrival");
    }

    fn vault(id: &str, exists: bool, unlocked: bool) -> Vault {
        Vault {
            id: id.into(),
            name: id.into(),
            kind: VaultKind::Bitwarden,
            account: String::new(),
            server: String::new(),
            exists,
            unlocked,
            sync: SyncState::Local,
            count: 0,
        }
    }

    #[test]
    fn the_chain_holds_only_existing_locked_vaults() {
        let vaults = vec![
            vault("local", true, true),
            vault("a", true, false),
            vault("b", false, false),
            vault("c", true, false),
        ];
        let ids = |list: Vec<Vault>| list.into_iter().map(|vault| vault.id).collect::<Vec<_>>();
        assert_eq!(ids(locked_chain(vaults.clone(), None)), ["a", "c"]);
        assert_eq!(ids(locked_chain(vaults, Some("c"))), ["c"]);
    }
}

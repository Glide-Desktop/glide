use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use glide_ipc::keychain::{Login, Request, Response};

use crate::bitwarden::Bitwarden;
use crate::daemon::{Daemon, login_failed, mutates};

pub type Events = Arc<dyn Fn(Response) + Send + Sync>;

const TICK: Duration = Duration::from_mins(1);

pub struct Service {
    daemon: Arc<Mutex<Daemon>>,
    wake: mpsc::Sender<bool>,
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service").finish_non_exhaustive()
    }
}

impl Service {
    #[must_use]
    pub fn start(daemon: Daemon, events: Events) -> Arc<Self> {
        let daemon = Arc::new(Mutex::new(daemon));
        let (wake, signals) = mpsc::channel();
        let worker = Arc::clone(&daemon);
        let spawned = std::thread::Builder::new()
            .name("glide-keychain-sync".into())
            .spawn(move || sync_loop(&worker, &signals, &events));
        if let Err(err) = spawned {
            eprintln!("glide-keychain: vaults will not sync in the background ({err})");
        }
        Arc::new(Self { daemon, wake })
    }

    pub fn daemon(&self) -> MutexGuard<'_, Daemon> {
        self.daemon.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[must_use]
    pub fn handle(&self, request: Request) -> Response {
        let force = matches!(request, Request::Sync { .. } | Request::Unlock { .. });
        let wake = force || mutates(&request);
        let response = match request {
            Request::AddBitwarden { login } => self.add_bitwarden(&login),
            Request::SendLoginEmail { login } => {
                let dir = self.daemon().accounts_dir();
                match dir {
                    Some(dir) => {
                        Bitwarden::send_login_email(&dir, &login).map_or_else(login_failed, |()| Response::Done)
                    }
                    None => Response::Unsupported,
                }
            }
            request => self.daemon().handle(request),
        };
        if wake {
            let _ = self.wake.send(force);
        }
        response
    }

    fn add_bitwarden(&self, login: &Login) -> Response {
        let (dir, now) = {
            let daemon = self.daemon();
            (daemon.accounts_dir(), daemon.now())
        };
        let Some(dir) = dir else {
            return Response::Unsupported;
        };
        match Bitwarden::login(&dir, login, now) {
            Ok(account) => self.daemon().adopt_bitwarden(account),
            Err(error) => login_failed(error),
        }
    }
}

fn sync_loop(daemon: &Mutex<Daemon>, signals: &mpsc::Receiver<bool>, events: &Events) {
    let lock = || daemon.lock().unwrap_or_else(PoisonError::into_inner);
    loop {
        let force = match signals.recv_timeout(TICK) {
            Ok(force) => force,
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let force = signals.try_iter().fold(force, |any, next| any || next);
        let (jobs, now) = {
            let mut daemon = lock();
            (daemon.sync_jobs(force), daemon.now())
        };
        if jobs.is_empty() {
            continue;
        }
        events(Response::Changed);
        for (vault, job) in jobs {
            let done = job.run(now);
            lock().finish_sync(&vault, done);
        }
        events(Response::Changed);
    }
}

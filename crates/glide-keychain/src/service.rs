use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use glide_ipc::keychain::{Hidden, Login, Request, Response};

use crate::bitwarden::Bitwarden;
use crate::daemon::{Daemon, export_response, import_failed, login_failed, mutates};

pub type Events = Arc<dyn Fn(Response) + Send + Sync>;

const TICK: Duration = Duration::from_mins(1);

type Toggle = Box<dyn Fn(bool) + Send + Sync>;

pub struct Service {
    daemon: Arc<Mutex<Daemon>>,
    wake: mpsc::Sender<bool>,
    secrets_toggle: std::sync::OnceLock<Toggle>,
    prompts_closed: AtomicU64,
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
        Arc::new(Self { daemon, wake, secrets_toggle: std::sync::OnceLock::new(), prompts_closed: AtomicU64::new(0) })
    }

    pub fn on_secret_service(&self, toggle: Toggle) {
        let _ = self.secrets_toggle.set(toggle);
    }

    #[must_use]
    pub fn prompts_closed(&self) -> u64 {
        self.prompts_closed.load(Ordering::Acquire)
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
            Request::Import { vault, contents, password, binary } => {
                match crate::transfer::parse_file(contents.expose(), binary, password.as_ref().map(Hidden::expose)) {
                    Ok(drafts) => {
                        let total = drafts.len() as u32;
                        let mut count = 0;
                        for draft in drafts {
                            count += u32::from(self.daemon().import_one(&vault, draft));
                        }
                        Response::Imported { count, skipped: total - count }
                    }
                    Err(error) => import_failed(&error),
                }
            }
            Request::Export { vault, password } => {
                let records = self.daemon().export_records(vault.as_deref());
                export_response(&records, &password)
            }
            Request::PromptClosed => {
                self.prompts_closed.fetch_add(1, Ordering::AcqRel);
                Response::Done
            }
            Request::SecretService { enabled } => match self.secrets_toggle.get() {
                Some(toggle) => {
                    toggle(enabled);
                    Response::Done
                }
                None => Response::Unsupported,
            },
            Request::CheckLeaks => {
                let hashes = self.daemon().leak_hashes();
                match crate::leaks::check(&hashes, crate::leaks::fetch_range) {
                    Ok(found) => {
                        self.daemon().store_leaks(found);
                        Response::Done
                    }
                    Err(message) => Response::Error { message },
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
            daemon.take_login_handoff();
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

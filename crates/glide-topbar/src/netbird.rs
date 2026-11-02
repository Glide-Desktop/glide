use std::collections::VecDeque;
use std::sync::{Arc, mpsc};

use glide_netbird::{self as nb, Category, Notice, Reason, Severity, State};

use crate::i18n::{Key, t};

pub const SESSION_ID: u32 = u32::MAX - 3000;
pub const EXPIRED_ID: u32 = u32::MAX - 3001;
pub const LOGIN_ID: u32 = u32::MAX - 3002;
pub const UPDATE_ID: u32 = u32::MAX - 3003;
pub const POLICY_ID: u32 = u32::MAX - 3004;
pub const ERROR_ID: u32 = u32::MAX - 3005;
pub const EXTENDED_ID: u32 = u32::MAX - 3006;
pub const OUTDATED_ID: u32 = u32::MAX - 3007;
const EVENT_FIRST: u32 = u32::MAX - 3100;
const EVENT_SLOTS: u32 = 64;
const SEEN_EVENTS: usize = 128;

pub const EXTEND: &str = "netbird-extend";
pub const DISMISS: &str = "netbird-dismiss";
pub const SIGN_IN: &str = "netbird-sign-in";
pub const OPEN_AGAIN: &str = "netbird-open-again";
pub const INSTALL: &str = "netbird-install";
pub const DOWNLOAD: &str = "netbird-download";
pub const SETTINGS: &str = "netbird-settings";

pub const SETTINGS_SLUG: &str = "netbird";

#[must_use]
pub const fn owns(id: u32) -> bool {
    id >= EVENT_FIRST && id <= SESSION_ID
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Up,
    Down,
    Login,
    SwitchProfile(String),
    ExitNode { id: String, select: bool },
    Extend,
    DismissWarning,
    TriggerUpdate,
    Refresh,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Connect,
    Disconnect,
    Switch(String),
    ExitNode(String),
}

#[derive(Debug)]
pub enum Event {
    Status(Box<nb::Status>),
    Notice { id: String, notice: Notice },
    Networks(Vec<nb::Network>),
    Profiles(Vec<nb::Profile>),
    Config(Box<nb::Config>),
    Features(nb::Features),
    LoginOpened(String),
    LoginDone(Result<(), nb::Error>),
    ExtendOpened(String),
    Extended(Result<nb::Extended, nb::Error>),
    Failed { action: Action, error: nb::Error },
    Updated(Result<nb::UpdateResult, nb::Error>),
    Outdated(String),
}

enum Message {
    Request(Request),
    Feed(nb::Feed),
    LoginFinished,
    ExtendFinished,
}

pub struct Handle {
    sender: mpsc::Sender<Message>,
}

impl Handle {
    pub fn request(&self, request: Request) {
        if self.sender.send(Message::Request(request)).is_err() {
            tracing::warn!("the NetBird worker is gone");
        }
    }
}

type Sink = Arc<dyn Fn(Event) + Send + Sync>;

pub fn start(sink: impl Fn(Event) + Send + Sync + 'static) -> Result<Handle, nb::Error> {
    let daemon = nb::Daemon::connect()?;
    let (sender, receiver) = mpsc::channel();
    let feed = sender.clone();
    let watch = daemon.watch(move |item| {
        let _ = feed.send(Message::Feed(item));
    })?;
    let worker = Worker {
        daemon,
        sink: Arc::new(sink),
        loopback: sender.clone(),
        state: None,
        revision: None,
        connect_login: false,
        login_running: false,
        extend_running: false,
        outdated_announced: None,
    };
    let spawned = std::thread::Builder::new().name("glide-netbird-worker".into()).spawn(move || {
        let _watch = watch;
        worker.run(&receiver);
    });
    if let Err(err) = spawned {
        tracing::warn!("could not start the NetBird worker: {err}");
        return Err(nb::Error::Unavailable);
    }
    Ok(Handle { sender })
}

struct Worker {
    daemon: nb::Daemon,
    sink: Sink,
    loopback: mpsc::Sender<Message>,
    state: Option<State>,
    revision: Option<u64>,
    connect_login: bool,
    login_running: bool,
    extend_running: bool,
    outdated_announced: Option<String>,
}

impl Worker {
    fn run(mut self, receiver: &mpsc::Receiver<Message>) {
        while let Ok(message) = receiver.recv() {
            match message {
                Message::Feed(nb::Feed::Status(status)) => self.on_status(status),
                Message::Feed(nb::Feed::Event(event)) => self.on_event(&event),
                Message::Request(request) => self.on_request(request),
                Message::LoginFinished => self.login_running = false,
                Message::ExtendFinished => self.extend_running = false,
            }
        }
    }

    fn emit(&self, event: Event) {
        (self.sink)(event);
    }

    fn on_status(&mut self, status: Box<nb::Status>) {
        let state = status.state;
        let previous = self.state.replace(state);
        let revision = self.revision.replace(status.networks_revision);
        let version = status.daemon_version.clone();
        self.emit(Event::Status(status));
        if self.connect_login {
            match state {
                State::NeedsLogin => {
                    self.connect_login = false;
                    self.login();
                }
                State::Connected | State::Idle | State::LoginFailed | State::SessionExpired | State::Unavailable => {
                    self.connect_login = false;
                }
                State::Connecting => {}
            }
        }
        if state == State::Unavailable {
            return;
        }
        let reconnected = previous.is_none_or(|previous| previous == State::Unavailable);
        if reconnected && self.fetch_config() != Some(true) {
            self.check_outdated(version);
        }
        if reconnected || previous != Some(state) || revision != self.revision {
            self.fetch_networks();
        }
        if reconnected || previous != Some(state) {
            self.fetch_profiles();
        }
    }

    fn check_outdated(&mut self, version: String) {
        if self.outdated_announced.as_ref() == Some(&version) {
            return;
        }
        if !matches!(self.daemon.manageable(), Ok(false)) {
            return;
        }
        let marker = outdated_marker();
        let recorded = marker.as_deref().and_then(|path| std::fs::read_to_string(path).ok());
        self.outdated_announced = Some(version.clone());
        if recorded.as_deref() == Some(version.as_str()) {
            return;
        }
        if let Some(path) = marker
            && let Err(err) = record_outdated(&path, &version)
        {
            tracing::debug!("could not remember the outdated NetBird service: {err}");
        }
        self.emit(Event::Outdated(version));
    }

    fn on_event(&self, event: &nb::Event) {
        let notice = nb::classify(event);
        match notice {
            Notice::Silent => return,
            Notice::ConfigChanged { .. } => {
                self.fetch_config();
            }
            Notice::ProfileListChanged => self.fetch_profiles(),
            _ => {}
        }
        self.emit(Event::Notice { id: event.id.clone(), notice });
    }

    fn on_request(&mut self, request: Request) {
        match request {
            Request::Up => {
                if self.state.is_some_and(State::needs_login) {
                    self.login();
                    return;
                }
                self.connect_login = true;
                if let Err(error) = self.daemon.up() {
                    self.connect_login = false;
                    self.emit(Event::Failed { action: Action::Connect, error });
                }
            }
            Request::Down => {
                self.connect_login = false;
                if let Err(error) = self.daemon.down() {
                    self.emit(Event::Failed { action: Action::Disconnect, error });
                }
            }
            Request::Login => self.login(),
            Request::SwitchProfile(id) => {
                self.connect_login = true;
                if let Err(error) = self.daemon.switch_profile(&id, true) {
                    self.connect_login = false;
                    self.emit(Event::Failed { action: Action::Switch(id), error });
                }
                self.fetch_profiles();
            }
            Request::ExitNode { id, select } => {
                let result = if select {
                    self.daemon.select_networks(vec![id.clone()], true)
                } else {
                    self.daemon.deselect_networks(vec![id.clone()], true)
                };
                if let Err(error) = result {
                    self.emit(Event::Failed { action: Action::ExitNode(id), error });
                }
                self.fetch_networks();
            }
            Request::Extend => self.extend(),
            Request::DismissWarning => {
                if let Err(err) = self.daemon.dismiss_session_warning() {
                    tracing::debug!("NetBird did not take the warning dismissal: {err}");
                }
            }
            Request::TriggerUpdate => self.emit(Event::Updated(self.daemon.trigger_update())),
            Request::Refresh => {
                self.fetch_config();
                self.fetch_networks();
                self.fetch_profiles();
            }
        }
    }

    fn fetch_networks(&self) {
        if !self.state.is_some_and(State::is_connected) {
            self.emit(Event::Networks(Vec::new()));
            return;
        }
        match self.daemon.networks() {
            Ok(networks) => self.emit(Event::Networks(networks)),
            Err(err) => tracing::debug!("NetBird networks are unavailable, keeping the last list: {err}"),
        }
    }

    fn fetch_profiles(&self) {
        match self.daemon.profiles() {
            Ok(profiles) => self.emit(Event::Profiles(profiles)),
            Err(err) => tracing::debug!("NetBird profiles are unavailable: {err}"),
        }
    }

    fn fetch_config(&self) -> Option<bool> {
        let quiet = match self.daemon.config() {
            Ok(config) => {
                let quiet = config.disable_notifications;
                self.emit(Event::Config(Box::new(config)));
                Some(quiet)
            }
            Err(err) => {
                tracing::debug!("NetBird settings are unavailable: {err}");
                None
            }
        };
        if let Ok(features) = self.daemon.features() {
            self.emit(Event::Features(features));
        }
        quiet
    }

    fn login(&mut self) {
        if self.login_running {
            return;
        }
        self.login_running = true;
        let (daemon, sink, loopback) = (self.daemon.clone(), Arc::clone(&self.sink), self.loopback.clone());
        let spawned = std::thread::Builder::new().name("glide-netbird-login".into()).spawn(move || {
            let result = daemon.login().and_then(|start| {
                if start.needs_sso {
                    sink(Event::LoginOpened(start.uri.clone()));
                }
                daemon.finish_login(&start)
            });
            sink(Event::LoginDone(result));
            let _ = loopback.send(Message::LoginFinished);
        });
        if let Err(err) = spawned {
            self.login_running = false;
            tracing::warn!("could not start the NetBird sign-in: {err}");
        }
    }

    fn extend(&mut self) {
        if self.extend_running {
            return;
        }
        self.extend_running = true;
        let (daemon, sink, loopback) = (self.daemon.clone(), Arc::clone(&self.sink), self.loopback.clone());
        let spawned = std::thread::Builder::new().name("glide-netbird-extend".into()).spawn(move || {
            let result = daemon.request_extend().and_then(|start| {
                if !start.uri.is_empty() {
                    sink(Event::ExtendOpened(start.uri.clone()));
                }
                daemon.wait_extend(&start)
            });
            sink(Event::Extended(result));
            let _ = loopback.send(Message::ExtendFinished);
        });
        if let Err(err) = spawned {
            self.extend_running = false;
            tracing::warn!("could not start the NetBird session extension: {err}");
        }
    }
}

fn outdated_marker() -> Option<std::path::PathBuf> {
    glide_config::state_home().map(|dir| dir.join("glide").join("netbird-outdated"))
}

fn record_outdated(path: &std::path::Path, version: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, version)
}

#[derive(Default)]
pub struct Session {
    pub handle: Option<Handle>,
    pub state: Option<State>,
    pub login_uri: Option<String>,
    pub notifications_disabled: bool,
    pub announced_update: Option<String>,
    seen: VecDeque<String>,
    next_event: u32,
}

impl Session {
    pub fn request(&self, request: Request) {
        if let Some(handle) = &self.handle {
            handle.request(request);
        }
    }

    pub fn first_sighting(&mut self, id: &str) -> bool {
        if id.is_empty() {
            return true;
        }
        if self.seen.iter().any(|seen| seen == id) {
            return false;
        }
        if self.seen.len() == SEEN_EVENTS {
            self.seen.pop_front();
        }
        self.seen.push_back(id.to_string());
        true
    }

    pub fn event_id(&mut self) -> u32 {
        let id = EVENT_FIRST + self.next_event % EVENT_SLOTS;
        self.next_event = self.next_event.wrapping_add(1);
        id
    }
}

#[must_use]
pub fn reason_text(error: &nb::Error) -> String {
    let key = match error.reason() {
        Reason::JwtClockSkew => Key::NbErrClockSkew,
        Reason::JwtExpired => Key::NbErrTokenExpired,
        Reason::JwtSignatureInvalid => Key::NbErrSignature,
        Reason::SessionExpired => Key::NbErrSessionExpired,
        Reason::InvalidSetupKey => Key::NbErrSetup,
        Reason::PermissionDenied => Key::NbErrPermission,
        Reason::DaemonUnreachable => Key::NbErrUnreachable,
        Reason::Unimplemented => Key::NbOutdatedBody,
        Reason::PrivilegeRequired => return error.to_string(),
        Reason::Unknown => Key::NbErrUnknown,
    };
    t(key).to_string()
}

const fn severity_key(severity: Severity) -> Key {
    match severity {
        Severity::Info => Key::NbInfo,
        Severity::Warning => Key::NbWarning,
        Severity::Error => Key::NbError,
        Severity::Critical => Key::NbCritical,
    }
}

#[must_use]
pub fn event_title(severity: Severity, category: Category) -> String {
    let category = match category {
        Category::Network => t(Key::NbNetwork),
        Category::Dns => "DNS",
        Category::Authentication => t(Key::NbAuthentication),
        Category::Connectivity => t(Key::NbConnectivity),
        Category::System => t(Key::NbSystem),
    };
    t(Key::NbEventTitle).replace("{severity}", t(severity_key(severity))).replace("{category}", category)
}

#[must_use]
pub fn action(key: &str, label: Key) -> glide_notify::Action {
    glide_notify::Action { key: key.into(), label: t(label).into() }
}

#[must_use]
pub fn toast(id: u32, summary: String, body: String) -> glide_notify::Notification {
    glide_notify::Notification {
        id,
        app_name: "NetBird".to_string(),
        desktop_entry: "netbird".to_string(),
        app_icon: "netbird".to_string(),
        summary,
        body,
        ..glide_notify::Notification::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netbird_toast_ids_are_local_and_clear_of_every_other_range() {
        for id in [SESSION_ID, EXPIRED_ID, LOGIN_ID, UPDATE_ID, POLICY_ID, ERROR_ID, EXTENDED_ID, OUTDATED_ID] {
            assert!(owns(id) && crate::notifications::is_local(id), "{id}");
        }
        let mut session = Session::default();
        for _ in 0..EVENT_SLOTS * 2 {
            let id = session.event_id();
            assert!(owns(id) && id < OUTDATED_ID, "event slots stay below the fixed notices");
        }
        assert!(!owns(u32::MAX - 2000), "the removable drive toasts start there");
        assert!(!owns(u32::MAX - 1000), "agent prompts start there");
    }

    #[test]
    fn a_daemon_event_is_announced_once_however_often_it_is_replayed() {
        let mut session = Session::default();
        assert!(session.first_sighting("a"));
        assert!(!session.first_sighting("a"));
        assert!(session.first_sighting(""), "events without an id cannot be told apart");
        assert!(session.first_sighting(""));
        for index in 0..SEEN_EVENTS {
            session.first_sighting(&index.to_string());
        }
        assert!(session.first_sighting("a"), "the memory is bounded");
    }

    #[test]
    fn the_outdated_service_is_remembered_by_version() {
        let dir = std::env::temp_dir().join(format!("glide-netbird-outdated-{}", std::process::id()));
        let path = dir.join("glide").join("netbird-outdated");
        record_outdated(&path, "0.30.1").expect("recorded");
        assert_eq!(std::fs::read_to_string(&path).ok().as_deref(), Some("0.30.1"));
        record_outdated(&path, "0.31.0").expect("replaced");
        assert_eq!(std::fs::read_to_string(&path).ok().as_deref(), Some("0.31.0"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn event_titles_name_the_severity_and_the_category() {
        let title = event_title(Severity::Warning, Category::Dns);
        assert!(title.starts_with(t(Key::NbWarning)) && title.ends_with("DNS"), "{title}");
        assert!(!title.contains('{'), "every placeholder is filled: {title}");
        assert!(event_title(Severity::Info, Category::Authentication).ends_with(t(Key::NbAuthentication)));
    }
}

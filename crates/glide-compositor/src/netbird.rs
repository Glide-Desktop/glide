use std::collections::VecDeque;
use std::process::Command;
use std::sync::{Arc, mpsc};

use glide_netbird::{self as nb, Category, Notice, Reason, Remaining, Severity, State};

use crate::i18n::{Key, t};
use crate::notifications::LocalEvent;

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

const EXTEND: &str = "netbird-extend";
const DISMISS: &str = "netbird-dismiss";
const SIGN_IN: &str = "netbird-sign-in";
const OPEN_AGAIN: &str = "netbird-open-again";
const INSTALL: &str = "netbird-install";
const DOWNLOAD: &str = "netbird-download";
const SETTINGS: &str = "netbird-settings";

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
    Outdated,
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
        if reconnected {
            self.fetch_config();
            if matches!(self.daemon.compatible(), Ok(false)) {
                self.emit(Event::Outdated);
            }
        }
        if reconnected || previous != Some(state) || revision != self.revision {
            self.fetch_networks();
        }
        if reconnected || previous != Some(state) {
            self.fetch_profiles();
        }
    }

    fn on_event(&self, event: &nb::Event) {
        let notice = nb::classify(event);
        match notice {
            Notice::Silent => return,
            Notice::ConfigChanged { .. } => self.fetch_config(),
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
        let connected = self.state.is_some_and(State::is_connected);
        let networks = if connected { self.daemon.networks().unwrap_or_default() } else { Vec::new() };
        self.emit(Event::Networks(networks));
    }

    fn fetch_profiles(&self) {
        match self.daemon.profiles() {
            Ok(profiles) => self.emit(Event::Profiles(profiles)),
            Err(err) => tracing::debug!("NetBird profiles are unavailable: {err}"),
        }
    }

    fn fetch_config(&self) {
        match self.daemon.config() {
            Ok(config) => self.emit(Event::Config(Box::new(config))),
            Err(err) => tracing::debug!("NetBird settings are unavailable: {err}"),
        }
        if let Ok(features) = self.daemon.features() {
            self.emit(Event::Features(features));
        }
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

#[derive(Default)]
pub struct Session {
    handle: Option<Handle>,
    state: Option<State>,
    login_uri: Option<String>,
    notifications_disabled: bool,
    announced_update: Option<String>,
    seen: VecDeque<String>,
    next_event: u32,
}

impl Session {
    fn request(&self, request: Request) {
        if let Some(handle) = &self.handle {
            handle.request(request);
        }
    }

    fn first_sighting(&mut self, id: &str) -> bool {
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

    fn event_id(&mut self) -> u32 {
        let id = EVENT_FIRST + self.next_event % EVENT_SLOTS;
        self.next_event = self.next_event.wrapping_add(1);
        id
    }
}

#[must_use]
pub fn remaining_text(remaining: Remaining) -> String {
    let count = |key: Key, n: u64| t(key).replace("{n}", &n.to_string());
    match remaining {
        Remaining::Expired => t(Key::NbSessionExpired).to_string(),
        Remaining::LessThanMinute => t(Key::NbLessThanMinute).to_string(),
        Remaining::Minutes(n) => count(Key::NbMinutes, n),
        Remaining::Hours(n) => count(Key::NbHours, n),
        Remaining::Days(n) => count(Key::NbDays, n),
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

fn action(key: &str, label: Key) -> glide_notify::Action {
    glide_notify::Action { key: key.into(), label: t(label).into() }
}

fn toast(id: u32, summary: String, body: String) -> glide_notify::Notification {
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

pub fn open_url(uri: &str) {
    let mut command = match std::env::var("BROWSER").ok().filter(|browser| !browser.trim().is_empty()) {
        Some(browser) => Command::new(browser),
        None => Command::new("xdg-open"),
    };
    command.arg(uri);
    if let Err(err) = crate::spawn_detached(command) {
        tracing::warn!("could not open the NetBird sign-in page: {err}");
    }
}

pub fn open_settings() {
    open_settings_at(None);
}

pub fn open_settings_at(section: Option<&str>) {
    let mut command = Command::new("glide-settings");
    command.arg(SETTINGS_SLUG).args(section);
    if let Err(err) = crate::spawn_detached(command) {
        tracing::warn!("could not open glide-settings {SETTINGS_SLUG}: {err}");
    }
}

impl crate::GlideState {
    pub fn start_netbird(&mut self, sink: impl Fn(Event) + Send + Sync + 'static) {
        self.topbar.set_netbird_installed(true);
        match start(sink) {
            Ok(handle) => self.session.netbird.handle = Some(handle),
            Err(err) => tracing::warn!("NetBird cannot be managed from the shell: {err}"),
        }
    }

    pub fn netbird_request(&mut self, request: Request) {
        match &request {
            Request::Up => self.topbar.begin_netbird(crate::topbar::NetbirdPending::Connect),
            Request::Down => self.topbar.begin_netbird(crate::topbar::NetbirdPending::Disconnect),
            Request::SwitchProfile(id) => self.topbar.begin_netbird(crate::topbar::NetbirdPending::Profile(id.clone())),
            Request::ExitNode { id, .. } => self.topbar.begin_netbird(crate::topbar::NetbirdPending::Exit(id.clone())),
            Request::Login if self.session.netbird.login_uri.is_some() => {
                if let Some(uri) = self.session.netbird.login_uri.clone() {
                    open_url(&uri);
                }
                return;
            }
            _ => {}
        }
        self.session.netbird.request(request);
        self.queue_redraw();
    }

    pub fn toggle_netbird(&mut self) {
        let request = if self.topbar.netbird_state().is_some_and(State::is_up) { Request::Down } else { Request::Up };
        self.netbird_request(request);
    }

    fn netbird_toast(&mut self, notification: glide_notify::Notification) {
        self.notifications.push(&mut self.dock.icons, notification, true);
        self.refresh_topbar();
        self.queue_redraw();
    }

    fn netbird_error(&mut self, message: String) {
        let mut notice = toast(ERROR_ID, t(Key::NbErrorTitle).to_string(), message);
        notice.urgency = glide_notify::Urgency::Critical;
        self.netbird_toast(notice);
    }

    pub fn handle_netbird(&mut self, event: Event) {
        match event {
            Event::Status(status) => self.netbird_status(*status),
            Event::Notice { id, notice } => {
                if self.session.netbird.first_sighting(&id) {
                    self.netbird_notice(notice);
                }
            }
            Event::Networks(networks) => self.netbird_view(crate::topbar::NetbirdUpdate::Networks(networks)),
            Event::Profiles(profiles) => self.netbird_view(crate::topbar::NetbirdUpdate::Profiles(profiles)),
            Event::Config(config) => {
                self.session.netbird.notifications_disabled = config.disable_notifications;
                self.netbird_view(crate::topbar::NetbirdUpdate::Config(config));
            }
            Event::Features(features) => self.netbird_view(crate::topbar::NetbirdUpdate::Features(features)),
            Event::LoginOpened(uri) => {
                open_url(&uri);
                self.session.netbird.login_uri = Some(uri);
                let mut notice = toast(LOGIN_ID, t(Key::NbLoginTitle).to_string(), t(Key::NbLoginBody).to_string());
                notice.actions = vec![action(OPEN_AGAIN, Key::NbOpenAgain)];
                notice.resident = true;
                self.netbird_toast(notice);
            }
            Event::LoginDone(result) => {
                self.session.netbird.login_uri = None;
                self.notifications.dismiss(LOGIN_ID);
                match result {
                    Ok(()) => {
                        self.notifications.dismiss(EXPIRED_ID);
                    }
                    Err(nb::Error::Cancelled) => {}
                    Err(error) => {
                        self.netbird_error(t(Key::NbLoginFailed).replace("{reason}", &reason_text(&error)));
                    }
                }
                self.queue_redraw();
            }
            Event::ExtendOpened(uri) => open_url(&uri),
            Event::Extended(Ok(nb::Extended::Until(_))) => {
                self.notifications.dismiss(SESSION_ID);
                let notice =
                    toast(EXTENDED_ID, t(Key::NbExtendedTitle).to_string(), t(Key::NbExtendedBody).to_string());
                self.netbird_toast(notice);
            }
            Event::Extended(Ok(nb::Extended::Preempted)) => {}
            Event::Extended(Err(error)) => {
                tracing::info!("the NetBird session was not extended: {error}");
                self.netbird_error(t(Key::NbExtendFailed).to_string());
            }
            Event::Failed { action, error } => self.netbird_failed(&action, &error),
            Event::Updated(Ok(result)) if result.success => {}
            Event::Updated(result) => {
                let reason = match result {
                    Ok(result) => result.error,
                    Err(error) => reason_text(&error),
                };
                let body = if reason.is_empty() {
                    t(Key::NbUpdateFailed).to_string()
                } else {
                    format!("{} {reason}", t(Key::NbUpdateFailed))
                };
                self.netbird_error(body);
            }
            Event::Outdated => {
                let notice =
                    toast(OUTDATED_ID, t(Key::NbOutdatedTitle).to_string(), t(Key::NbOutdatedBody).to_string());
                self.netbird_toast(notice);
            }
        }
    }

    fn netbird_view(&mut self, update: crate::topbar::NetbirdUpdate) {
        if self.topbar.apply_netbird(update) {
            self.queue_redraw();
        }
    }

    fn netbird_status(&mut self, status: nb::Status) {
        let state = status.state;
        let previous = self.session.netbird.state.replace(state);
        if state == State::SessionExpired && previous != Some(State::SessionExpired) {
            let mut notice = toast(EXPIRED_ID, t(Key::NbExpiredTitle).to_string(), t(Key::NbExpiredBody).to_string());
            notice.actions = vec![action(SIGN_IN, Key::NbSignIn)];
            notice.urgency = glide_notify::Urgency::Critical;
            notice.resident = true;
            self.netbird_toast(notice);
        }
        if state.is_connected() && previous != Some(State::Connected) {
            self.notifications.dismiss(EXPIRED_ID);
            self.notifications.dismiss(LOGIN_ID);
        }
        self.netbird_view(crate::topbar::NetbirdUpdate::Status(Box::new(status)));
    }

    fn netbird_failed(&mut self, action: &Action, error: &nb::Error) {
        tracing::info!(?action, "a NetBird action failed: {error}");
        let headline = match action {
            Action::Connect => t(Key::NbConnectFailed).to_string(),
            Action::Disconnect => t(Key::NbDisconnectFailed).to_string(),
            Action::Switch(id) => {
                let name = self.topbar.netbird_profile_name(id).unwrap_or_else(|| id.clone());
                t(Key::NbSwitchFailed).replace("{profile}", &name)
            }
            Action::ExitNode(id) => t(Key::NbExitFailed).replace("{name}", id),
        };
        self.topbar.fail_netbird();
        self.netbird_error(format!("{headline}. {}", reason_text(error)));
    }

    fn netbird_notice(&mut self, notice: Notice) {
        let quiet = self.session.netbird.notifications_disabled && !notice.critical();
        match notice {
            Notice::Silent | Notice::ProfileListChanged | Notice::ConfigChanged { by_policy: false } => {}
            _ if quiet => {}
            Notice::ConfigChanged { by_policy: true } => {
                let notice = toast(POLICY_ID, t(Key::NbPolicyTitle).to_string(), t(Key::NbPolicyBody).to_string());
                self.netbird_toast(notice);
            }
            Notice::SessionWarning { expires_at, final_warning: false } => {
                let body = expires_at.map_or_else(
                    || t(Key::NbWarningGeneric).to_string(),
                    |deadline| {
                        let left = nb::remaining_until(deadline, std::time::SystemTime::now());
                        t(Key::NbWarningBody).replace("{time}", &remaining_text(left))
                    },
                );
                let mut notice = toast(SESSION_ID, t(Key::NbWarningTitle).to_string(), body);
                notice.actions = vec![action(EXTEND, Key::NbExtendNow), action(DISMISS, Key::NbDismiss)];
                notice.resident = true;
                self.netbird_toast(notice);
            }
            Notice::SessionWarning { final_warning: true, .. } => {
                let mut notice = toast(SESSION_ID, t(Key::NbFinalTitle).to_string(), t(Key::NbFinalBody).to_string());
                notice.actions = vec![action(EXTEND, Key::NbRenewSession), action(DISMISS, Key::Close)];
                notice.urgency = glide_notify::Urgency::Critical;
                notice.resident = true;
                self.netbird_toast(notice);
            }
            Notice::SessionDeadlineRejected => {
                let mut notice =
                    toast(EXPIRED_ID, t(Key::NbDeadlineTitle).to_string(), t(Key::NbDeadlineBody).to_string());
                notice.actions = vec![action(SIGN_IN, Key::NbSignIn)];
                self.netbird_toast(notice);
            }
            Notice::UpdateAvailable { version, enforced } => {
                if self.session.netbird.announced_update.as_deref() == Some(version.as_str()) {
                    return;
                }
                self.session.netbird.announced_update = Some(version.clone());
                let mut body = t(Key::NbUpdateBody).replace("{version}", &version);
                if enforced {
                    body.push_str(t(Key::NbUpdateEnforced));
                }
                let mut notice = toast(UPDATE_ID, t(Key::NbUpdateTitle).to_string(), body);
                notice.actions =
                    vec![if enforced { action(INSTALL, Key::NbInstallNow) } else { action(DOWNLOAD, Key::NbDownload) }];
                notice.resident = enforced;
                self.netbird_toast(notice);
            }
            Notice::Installing { version } => {
                let body = t(Key::NbUpdatingBody).replace("{version}", version.as_deref().unwrap_or_default());
                let notice = toast(UPDATE_ID, t(Key::NbUpdatingTitle).to_string(), body);
                self.netbird_toast(notice);
            }
            Notice::Message { severity, category, body } => {
                let id = self.session.netbird.event_id();
                let mut notice = toast(id, event_title(severity, category), body);
                notice.actions = vec![action(SETTINGS, Key::NbSettingsAction)];
                if severity == Severity::Critical {
                    notice.urgency = glide_notify::Urgency::Critical;
                }
                self.netbird_toast(notice);
            }
        }
    }

    pub fn netbird_local_event(&mut self, event: &LocalEvent) -> bool {
        let LocalEvent::Action { id, key } = event else {
            return matches!(event, LocalEvent::Closed { id } | LocalEvent::Reply { id, .. } if owns(*id));
        };
        if !owns(*id) {
            return false;
        }
        match key.as_str() {
            EXTEND => {
                self.notifications.dismiss(*id);
                self.netbird_request(Request::Extend);
            }
            DISMISS => {
                self.notifications.dismiss(*id);
                self.netbird_request(Request::DismissWarning);
            }
            SIGN_IN => {
                self.notifications.dismiss(*id);
                self.netbird_request(Request::Login);
            }
            OPEN_AGAIN => {
                if let Some(uri) = self.session.netbird.login_uri.clone() {
                    open_url(&uri);
                }
            }
            INSTALL => {
                self.notifications.dismiss(*id);
                self.netbird_request(Request::TriggerUpdate);
            }
            DOWNLOAD => {
                self.notifications.dismiss(*id);
                open_url(nb::DOWNLOAD_URL);
            }
            SETTINGS => {
                self.notifications.dismiss(*id);
                open_settings();
            }
            _ => return false,
        }
        self.queue_redraw();
        true
    }

    pub fn tick_netbird(&mut self) -> bool {
        self.topbar.tick_netbird(std::time::SystemTime::now())
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
    fn event_titles_name_the_severity_and_the_category() {
        let title = event_title(Severity::Warning, Category::Dns);
        assert!(title.starts_with(t(Key::NbWarning)) && title.ends_with("DNS"), "{title}");
        assert!(!title.contains('{'), "every placeholder is filled: {title}");
        assert!(event_title(Severity::Info, Category::Authentication).ends_with(t(Key::NbAuthentication)));
    }

    #[test]
    fn remaining_session_time_fills_in_the_count_of_the_largest_unit() {
        for (remaining, count) in
            [(Remaining::Hours(2), "2"), (Remaining::Minutes(15), "15"), (Remaining::Days(3), "3")]
        {
            let text = remaining_text(remaining);
            assert!(text.contains(count) && !text.contains('{'), "{text}");
        }
        assert_eq!(remaining_text(Remaining::LessThanMinute), t(Key::NbLessThanMinute));
    }
}

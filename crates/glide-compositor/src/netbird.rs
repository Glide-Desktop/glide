use std::process::Command;

use glide_netbird::{self as nb, Notice, Severity, State};

use crate::i18n::{Key, t};
use crate::notifications::LocalEvent;
use crate::topbar::remaining_text;
pub use glide_topbar::netbird::{
    Action, DISMISS, DOWNLOAD, ERROR_ID, EXPIRED_ID, EXTEND, EXTENDED_ID, Event, INSTALL, LOGIN_ID, OPEN_AGAIN,
    OUTDATED_ID, POLICY_ID, Request, SESSION_ID, SETTINGS, SETTINGS_SLUG, SIGN_IN, Session, UPDATE_ID, action,
    event_title, owns, reason_text, start, toast,
};

pub fn open_url(uri: &str) {
    let Some(uri) = nb::web_uri(uri) else {
        tracing::warn!("refusing to open a NetBird sign-in address that is not a web address");
        return;
    };
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
            Event::Outdated(version) => {
                if self.session.netbird.notifications_disabled {
                    tracing::info!(%version, "the NetBird service is too old to manage; notifications are off");
                    return;
                }
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

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

use crate::error::Error;
use crate::model::{
    ActiveProfile, Anonymize, BundleOptions, BundleResult, Config, ConfigPatch, Event, ExtendStart, Extended, Features,
    ForwardingRule, LogLevel, LoginStart, Network, Profile, Status, UpdateResult,
};
use crate::pb;
use crate::pb::daemon_service_client::DaemonServiceClient;
use crate::profile_state;
use crate::time::{proto_duration, system_time};

pub const DEFAULT_SOCKET: &str = "/var/run/netbird.sock";
pub const UPLOAD_URL: &str = "https://upload.debug.netbird.io/upload-url";
pub const DOWNLOAD_URL: &str = "https://app.netbird.io/install";

const CALL: Duration = Duration::from_secs(10);
const SLOW: Duration = Duration::from_mins(1);
const INTERACTIVE: Duration = Duration::from_mins(15);
const FIRST_RETRY: Duration = Duration::from_secs(1);
const LAST_RETRY: Duration = Duration::from_secs(10);

fn runtime() -> Result<&'static tokio::runtime::Runtime, Error> {
    static RUNTIME: OnceLock<Option<tokio::runtime::Runtime>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("glide-netbird")
                .enable_all()
                .build()
                .inspect_err(|err| tracing::warn!("could not start the NetBird client runtime: {err}"))
                .ok()
        })
        .as_ref()
        .ok_or(Error::Unavailable)
}

fn block_on<F>(runtime: &tokio::runtime::Runtime, future: F) -> F::Output
where
    F: Future + Send,
    F::Output: Send,
{
    match tokio::runtime::Handle::try_current() {
        Err(_) => runtime.block_on(future),
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(|| runtime.block_on(future))
        }
        Ok(_) => std::thread::scope(|scope| {
            scope.spawn(|| runtime.block_on(future)).join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        }),
    }
}

#[must_use]
pub fn socket_path() -> PathBuf {
    std::env::var("NB_DAEMON_ADDR")
        .ok()
        .and_then(|address| address.strip_prefix("unix://").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET))
}

use glide_config::on_path;

#[must_use]
pub fn installed() -> bool {
    on_path("netbird") || socket_path().exists()
}

#[must_use]
pub fn current_user() -> String {
    ["USER", "LOGNAME"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok().filter(|name| !name.is_empty()))
        .unwrap_or_default()
}

fn non_empty(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}

#[derive(Debug, Clone)]
pub enum Feed {
    Status(Box<Status>),
    Event(Event),
}

pub struct Watch {
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

#[derive(Clone)]
pub struct Daemon {
    client: DaemonServiceClient<Channel>,
}

impl std::fmt::Debug for Daemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Daemon")
    }
}

impl Daemon {
    pub fn connect() -> Result<Self, Error> {
        Self::at(&socket_path())
    }

    pub fn at(path: &Path) -> Result<Self, Error> {
        let runtime = runtime()?;
        let _context = runtime.enter();
        let path = path.to_path_buf();
        let channel = Endpoint::from_static("http://[::]:0")
            .connect_timeout(Duration::from_secs(3))
            .connect_with_connector_lazy(service_fn(move |_: Uri| {
                let path = path.clone();
                async move { Ok::<_, std::io::Error>(TokioIo::new(UnixStream::connect(path).await?)) }
            }));
        Ok(Self { client: DaemonServiceClient::new(channel) })
    }

    fn call<T, F, Fut>(&self, limit: Duration, request: F) -> Result<T, Error>
    where
        T: Send,
        F: FnOnce(DaemonServiceClient<Channel>) -> Fut,
        Fut: Future<Output = Result<tonic::Response<T>, tonic::Status>> + Send,
    {
        let runtime = runtime()?;
        let future = request(self.client.clone());
        block_on(runtime, async move {
            match tokio::time::timeout(limit, future).await {
                Ok(Ok(response)) => Ok(response.into_inner()),
                Ok(Err(status)) => Err(Error::from_status(&status)),
                Err(_) => Err(Error::Timeout),
            }
        })
    }

    pub fn status(&self) -> Result<Status, Error> {
        let request = pb::StatusRequest { get_full_peer_status: true, ..pb::StatusRequest::default() };
        self.call(CALL, |mut client| async move { client.status(request).await }).map(Status::from)
    }

    pub fn compatible(&self) -> Result<bool, Error> {
        match self.call(CALL, |mut client| async move { client.wails_ui_ready(pb::WailsUiReadyRequest {}).await }) {
            Ok(_) => Ok(true),
            Err(Error::Daemon { code: tonic::Code::Unimplemented, .. }) => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub fn active_profile(&self) -> Result<ActiveProfile, Error> {
        let response = self
            .call(CALL, |mut client| async move { client.get_active_profile(pb::GetActiveProfileRequest {}).await })?;
        let username = if response.username.is_empty() { current_user() } else { response.username };
        Ok(ActiveProfile { id: response.id, name: response.profile_name, username })
    }

    pub fn up(&self) -> Result<(), Error> {
        let active = self.active_profile().ok();
        let request = pb::UpRequest {
            profile_name: active.as_ref().and_then(|profile| non_empty(profile.id.clone())),
            username: active.map(|profile| profile.username).and_then(non_empty),
            r#async: true,
        };
        self.call(SLOW, |mut client| async move { client.up(request).await }).map(drop)
    }

    pub fn down(&self) -> Result<(), Error> {
        self.call(SLOW, |mut client| async move { client.down(pb::DownRequest {}).await }).map(drop)
    }

    pub fn login(&self) -> Result<LoginStart, Error> {
        self.login_with(None, None)
    }

    pub(crate) fn login_with(
        &self, management_url: Option<String>, setup_key: Option<String>,
    ) -> Result<LoginStart, Error> {
        let active = self.active_profile().unwrap_or_default();
        let username = if active.username.is_empty() { current_user() } else { active.username.clone() };
        let request = pb::LoginRequest {
            is_unix_desktop_client: true,
            management_url: management_url.unwrap_or_default(),
            setup_key: setup_key.unwrap_or_default(),
            profile_name: non_empty(active.id.clone()),
            username: non_empty(username),
            hint: profile_state::email(&active.id),
            ..pb::LoginRequest::default()
        };
        let response = self.call(SLOW, |mut client| async move { client.login(request).await })?;
        let uri = if response.verification_uri_complete.is_empty() {
            response.verification_uri
        } else {
            response.verification_uri_complete
        };
        Ok(LoginStart {
            needs_sso: response.needs_sso_login,
            user_code: response.user_code,
            uri,
            profile_id: active.id,
        })
    }

    pub fn finish_login(&self, start: &LoginStart) -> Result<(), Error> {
        if start.needs_sso {
            let request = pb::WaitSsoLoginRequest { user_code: start.user_code.clone(), hostname: String::new() };
            let response = self.call(INTERACTIVE, |mut client| async move { client.wait_sso_login(request).await })?;
            if !response.email.is_empty()
                && !start.profile_id.is_empty()
                && let Err(err) = profile_state::store_email(&start.profile_id, &response.email)
            {
                tracing::warn!("could not store the NetBird account email: {err}");
            }
        }
        self.up()
    }

    pub fn config(&self) -> Result<Config, Error> {
        let active = self.active_profile()?;
        let request = pb::GetConfigRequest { profile_name: active.id, username: active.username };
        self.call(CALL, |mut client| async move { client.get_config(request).await }).map(Config::from)
    }

    pub fn set_config(&self, patch: ConfigPatch) -> Result<(), Error> {
        self.set_config_for(&self.active_profile()?, patch)
    }

    pub fn set_config_for(&self, profile: &ActiveProfile, patch: ConfigPatch) -> Result<(), Error> {
        let request = patch.into_request(profile);
        self.call(SLOW, |mut client| async move { client.set_config(request).await }).map(drop)
    }

    pub fn features(&self) -> Result<Features, Error> {
        self.call(CALL, |mut client| async move { client.get_features(pb::GetFeaturesRequest {}).await })
            .map(Features::from)
    }

    pub fn networks(&self) -> Result<Vec<Network>, Error> {
        let response =
            self.call(CALL, |mut client| async move { client.list_networks(pb::ListNetworksRequest {}).await })?;
        Ok(response.routes.into_iter().map(Network::from).collect())
    }

    pub fn select_networks(&self, ids: Vec<String>, append: bool) -> Result<(), Error> {
        let request = pb::SelectNetworksRequest { network_i_ds: ids, append, all: false };
        self.call(CALL, |mut client| async move { client.select_networks(request).await }).map(drop)
    }

    pub fn deselect_networks(&self, ids: Vec<String>, append: bool) -> Result<(), Error> {
        let request = pb::SelectNetworksRequest { network_i_ds: ids, append, all: false };
        self.call(CALL, |mut client| async move { client.deselect_networks(request).await }).map(drop)
    }

    pub fn select_all_networks(&self) -> Result<(), Error> {
        let request = pb::SelectNetworksRequest { all: true, ..pb::SelectNetworksRequest::default() };
        self.call(CALL, |mut client| async move { client.select_networks(request).await }).map(drop)
    }

    pub fn deselect_all_networks(&self) -> Result<(), Error> {
        let request = pb::SelectNetworksRequest { all: true, ..pb::SelectNetworksRequest::default() };
        self.call(CALL, |mut client| async move { client.deselect_networks(request).await }).map(drop)
    }

    pub fn profiles(&self) -> Result<Vec<Profile>, Error> {
        let request = pb::ListProfilesRequest { username: current_user() };
        let response = self.call(CALL, |mut client| async move { client.list_profiles(request).await })?;
        Ok(response
            .profiles
            .into_iter()
            .map(|profile| Profile {
                email: profile_state::email(&profile.id),
                id: profile.id,
                name: profile.name,
                active: profile.is_active,
            })
            .collect())
    }

    pub fn switch_profile(&self, handle: &str, connect: bool) -> Result<String, Error> {
        let previous = self.status().map(|status| status.state).unwrap_or_default();
        let request = pb::SwitchProfileRequest {
            profile_name: non_empty(handle.to_string()),
            username: non_empty(current_user()),
        };
        let id = self.call(CALL, |mut client| async move { client.switch_profile(request).await })?.id;
        if let Err(err) = profile_state::mark_active(&id) {
            tracing::warn!("could not mirror the NetBird profile switch for the CLI: {err}");
        }
        if previous.needs_down_before_switch()
            && let Err(err) = self.down()
        {
            tracing::warn!("NetBird down before the profile switch failed: {err}");
        }
        if connect {
            self.up()?;
        }
        Ok(id)
    }

    pub fn add_profile(&self, name: &str) -> Result<String, Error> {
        let request = pb::AddProfileRequest { username: current_user(), profile_name: name.to_string() };
        self.call(CALL, |mut client| async move { client.add_profile(request).await }).map(|response| response.id)
    }

    pub fn rename_profile(&self, handle: &str, name: &str) -> Result<(), Error> {
        let request = pb::RenameProfileRequest {
            username: current_user(),
            handle: handle.to_string(),
            new_profile_name: name.to_string(),
        };
        self.call(CALL, |mut client| async move { client.rename_profile(request).await }).map(drop)
    }

    pub fn remove_profile(&self, handle: &str) -> Result<(), Error> {
        let request = pb::RemoveProfileRequest { username: current_user(), profile_name: handle.to_string() };
        let id = self.call(CALL, |mut client| async move { client.remove_profile(request).await })?.id;
        if let Err(err) = profile_state::remove(&id) {
            tracing::warn!("could not remove the NetBird profile state: {err}");
        }
        Ok(())
    }

    pub fn deregister(&self, profile_id: &str) -> Result<(), Error> {
        let request =
            pb::LogoutRequest { profile_name: non_empty(profile_id.to_string()), username: non_empty(current_user()) };
        self.call(SLOW, |mut client| async move { client.logout(request).await }).map(drop)
    }

    pub fn configure_profile(&self, profile_id: &str, management_url: &str) -> Result<(), Error> {
        let request = pb::SetConfigRequest {
            username: current_user(),
            profile_name: profile_id.to_string(),
            management_url: management_url.to_string(),
            ..pb::SetConfigRequest::default()
        };
        self.call(SLOW, |mut client| async move { client.set_config(request).await }).map(drop)
    }

    pub fn debug_bundle(&self, options: BundleOptions) -> Result<BundleResult, Error> {
        let request = pb::DebugBundleRequest {
            anonymize: options.anonymize != Anonymize::None,
            anonymize_level: String::from(if options.anonymize == Anonymize::Strict { "strict" } else { "default" }),
            system_info: options.system_info,
            upload_url: if options.upload { UPLOAD_URL.to_string() } else { String::new() },
            log_file_count: options.log_file_count,
            cli_version: format!("glide-{}", env!("CARGO_PKG_VERSION")),
            upload_insecure: false,
        };
        let response = self.call(INTERACTIVE, |mut client| async move { client.debug_bundle(request).await })?;
        Ok(BundleResult {
            path: response.path,
            uploaded_key: response.uploaded_key,
            upload_failure: response.upload_failure_reason,
        })
    }

    pub fn log_level(&self) -> Result<LogLevel, Error> {
        self.call(CALL, |mut client| async move { client.get_log_level(pb::GetLogLevelRequest {}).await })
            .map(|response| LogLevel::from_proto(response.level))
    }

    pub fn set_log_level(&self, level: LogLevel) -> Result<(), Error> {
        let request = pb::SetLogLevelRequest { level: level.to_proto() };
        self.call(CALL, |mut client| async move { client.set_log_level(request).await }).map(drop)
    }

    pub fn start_bundle_capture(&self, limit: Duration) -> Result<(), Error> {
        let request = pb::StartBundleCaptureRequest { timeout: Some(proto_duration(limit)) };
        self.call(CALL, |mut client| async move { client.start_bundle_capture(request).await }).map(drop)
    }

    pub fn stop_bundle_capture(&self) -> Result<(), Error> {
        self.call(CALL, |mut client| async move { client.stop_bundle_capture(pb::StopBundleCaptureRequest {}).await })
            .map(drop)
    }

    pub fn request_extend(&self) -> Result<ExtendStart, Error> {
        let hint = self.active_profile().ok().and_then(|profile| profile_state::email(&profile.id));
        let request = pb::RequestExtendAuthSessionRequest { hint, has_graphical_session: true };
        let response =
            self.call(SLOW, |mut client| async move { client.request_extend_auth_session(request).await })?;
        let uri = if response.verification_uri_complete.is_empty() {
            response.verification_uri
        } else {
            response.verification_uri_complete
        };
        Ok(ExtendStart {
            uri,
            user_code: response.user_code,
            device_code: response.device_code,
            expires_in: Duration::from_secs(u64::try_from(response.expires_in).unwrap_or_default()),
        })
    }

    pub fn wait_extend(&self, start: &ExtendStart) -> Result<Extended, Error> {
        let request = pb::WaitExtendAuthSessionRequest {
            device_code: start.device_code.clone(),
            user_code: start.user_code.clone(),
        };
        match self.call(INTERACTIVE, |mut client| async move { client.wait_extend_auth_session(request).await }) {
            Ok(response) => Ok(Extended::Until(response.session_expires_at.as_ref().and_then(system_time))),
            Err(Error::Cancelled) => Ok(Extended::Preempted),
            Err(err) => Err(err),
        }
    }

    pub fn dismiss_session_warning(&self) -> Result<(), Error> {
        self.call(CALL, |mut client| async move {
            client.dismiss_session_warning(pb::DismissSessionWarningRequest {}).await
        })
        .map(drop)
    }

    pub fn trigger_update(&self) -> Result<UpdateResult, Error> {
        self.call(SLOW, |mut client| async move { client.trigger_update(pb::TriggerUpdateRequest {}).await })
            .map(|response| UpdateResult { success: response.success, error: response.error_msg })
    }

    pub fn forwarding_rules(&self) -> Result<Vec<ForwardingRule>, Error> {
        let response =
            self.call(CALL, |mut client| async move { client.forwarding_rules(pb::EmptyRequest {}).await })?;
        Ok(response.rules.into_iter().map(ForwardingRule::from).collect())
    }

    pub fn events(&self) -> Result<Vec<Event>, Error> {
        match self.call(CALL, |mut client| async move { client.get_events(pb::GetEventsRequest {}).await }) {
            Ok(response) => Ok(response.events.into_iter().map(Event::from).collect()),
            Err(Error::Daemon { code: tonic::Code::Unimplemented, .. }) => self.status().map(|status| status.events),
            Err(err) => Err(err),
        }
    }

    pub fn watch(&self, sink: impl Fn(Feed) + Send + Sync + 'static) -> Result<Watch, Error> {
        let runtime = runtime()?;
        let sink: Arc<dyn Fn(Feed) + Send + Sync> = Arc::new(sink);
        let status = runtime.spawn(watch_status(self.client.clone(), Arc::clone(&sink)));
        let events = runtime.spawn(watch_events(self.client.clone(), sink));
        Ok(Watch { tasks: vec![status, events] })
    }
}

const SUBSCRIBE_WAIT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusFeed {
    Stream,
    Poll,
}

impl StatusFeed {
    fn after_failure(self, error: &Error) -> Self {
        match error {
            Error::Daemon { code: tonic::Code::Unimplemented, .. } => Self::Poll,
            error if error.is_unavailable() => Self::Stream,
            _ => self,
        }
    }

    fn pause(self, backoff: Duration) -> Duration {
        match self {
            Self::Stream => backoff,
            Self::Poll => POLL_INTERVAL,
        }
    }
}

async fn watch_status(mut client: DaemonServiceClient<Channel>, sink: Arc<dyn Fn(Feed) + Send + Sync>) {
    let mut delay = FIRST_RETRY;
    let mut unavailable = false;
    let mut feed = StatusFeed::Stream;
    let mut last_polled: Option<Status> = None;
    let report = |error: &Error, unavailable: &mut bool| {
        if error.is_unavailable() && !*unavailable {
            *unavailable = true;
            sink(Feed::Status(Box::new(Status::unavailable())));
        }
    };
    loop {
        let request = pb::StatusRequest { get_full_peer_status: true, ..pb::StatusRequest::default() };
        let before = feed;
        let failure = match feed {
            StatusFeed::Stream => match tokio::time::timeout(SUBSCRIBE_WAIT, client.subscribe_status(request)).await {
                Err(_) => Some(Error::Timeout),
                Ok(Err(status)) => Some(Error::from_status(&status)),
                Ok(Ok(response)) => {
                    let mut stream = response.into_inner();
                    loop {
                        match stream.message().await {
                            Ok(Some(message)) => {
                                unavailable = false;
                                delay = FIRST_RETRY;
                                sink(Feed::Status(Box::new(Status::from(message))));
                            }
                            Ok(None) => break None,
                            Err(status) => break Some(Error::from_status(&status)),
                        }
                    }
                }
            },
            StatusFeed::Poll => match tokio::time::timeout(CALL, client.status(request)).await {
                Err(_) => Some(Error::Timeout),
                Ok(Err(status)) => Some(Error::from_status(&status)),
                Ok(Ok(response)) => {
                    unavailable = false;
                    delay = FIRST_RETRY;
                    let polled = Status::from(response.into_inner());
                    if last_polled.as_ref() != Some(&polled) {
                        last_polled = Some(polled.clone());
                        sink(Feed::Status(Box::new(polled)));
                    }
                    None
                }
            },
        };
        if let Some(error) = &failure {
            report(error, &mut unavailable);
            feed = feed.after_failure(error);
            last_polled = None;
        }
        if before == StatusFeed::Stream && feed == StatusFeed::Poll {
            continue;
        }
        tokio::time::sleep(feed.pause(delay)).await;
        if feed == StatusFeed::Stream {
            delay = (delay * 2).min(LAST_RETRY);
        }
    }
}

fn missed_since(events: Vec<Event>, since: SystemTime) -> Vec<Event> {
    let mut missed: Vec<Event> =
        events.into_iter().filter(|event| event.timestamp.is_some_and(|stamp| stamp > since)).collect();
    missed.sort_by_key(|event| event.timestamp);
    missed
}

async fn backfill(
    client: &mut DaemonServiceClient<Channel>, sink: &Arc<dyn Fn(Feed) + Send + Sync>, since: SystemTime,
) -> Option<SystemTime> {
    match tokio::time::timeout(CALL, client.get_events(pb::GetEventsRequest {})).await {
        Ok(Ok(response)) => {
            let events = response.into_inner().events.into_iter().map(Event::from).collect();
            for event in missed_since(events, since) {
                sink(Feed::Event(event));
            }
            None
        }
        Ok(Err(status)) if status.code() == tonic::Code::Unimplemented => None,
        _ => Some(since),
    }
}

async fn watch_events(mut client: DaemonServiceClient<Channel>, sink: Arc<dyn Fn(Feed) + Send + Sync>) {
    let mut delay = FIRST_RETRY;
    let mut outage: Option<SystemTime> = None;
    loop {
        if let Some(since) = outage {
            outage = backfill(&mut client, &sink, since).await;
        }
        if let Ok(response) = client.subscribe_events(pb::SubscribeRequest {}).await {
            let mut stream = response.into_inner();
            while let Ok(Some(event)) = stream.message().await {
                delay = FIRST_RETRY;
                sink(Feed::Event(Event::from(event)));
            }
        }
        outage.get_or_insert_with(SystemTime::now);
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(LAST_RETRY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn missing() -> Daemon {
        let path = std::env::temp_dir().join(format!("glide-netbird-missing-{}.sock", std::process::id()));
        Daemon::at(&path).expect("lazy channel")
    }

    #[test]
    fn calls_from_a_plain_thread_report_a_missing_daemon() {
        assert!(missing().status().is_err());
    }

    #[test]
    fn calls_from_inside_a_multi_thread_runtime_do_not_panic() {
        let outer = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().expect("rt");
        let daemon = missing();
        let result = outer.block_on(outer.spawn(async move { daemon.status() })).expect("task");
        assert!(result.is_err());
    }

    #[test]
    fn calls_from_inside_a_current_thread_runtime_do_not_panic() {
        let outer = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("rt");
        let daemon = missing();
        assert!(outer.block_on(async move { daemon.status() }).is_err());
    }

    fn daemon_error(code: tonic::Code) -> Error {
        Error::from_status(&tonic::Status::new(code, "refused"))
    }

    #[test]
    fn a_status_stream_the_daemon_does_not_implement_switches_to_polling_at_once() {
        assert_eq!(StatusFeed::Stream.after_failure(&daemon_error(tonic::Code::Unimplemented)), StatusFeed::Poll);
        assert_eq!(StatusFeed::Poll.after_failure(&daemon_error(tonic::Code::Unimplemented)), StatusFeed::Poll);
    }

    #[test]
    fn polling_hands_back_to_the_stream_when_the_daemon_goes_away_because_it_may_come_back_newer() {
        assert_eq!(StatusFeed::Poll.after_failure(&Error::Unavailable), StatusFeed::Stream);
        assert_eq!(StatusFeed::Stream.after_failure(&Error::Unavailable), StatusFeed::Stream);
    }

    #[test]
    fn other_failures_keep_the_mode_they_happened_in() {
        for error in [Error::Timeout, Error::Cancelled, daemon_error(tonic::Code::Internal)] {
            assert_eq!(StatusFeed::Stream.after_failure(&error), StatusFeed::Stream, "{error:?}");
            assert_eq!(StatusFeed::Poll.after_failure(&error), StatusFeed::Poll, "{error:?}");
        }
    }

    #[test]
    fn polling_waits_a_fixed_interval_while_the_stream_backs_off() {
        assert_eq!(StatusFeed::Poll.pause(LAST_RETRY), POLL_INTERVAL);
        assert_eq!(StatusFeed::Poll.pause(FIRST_RETRY), POLL_INTERVAL);
        assert_eq!(StatusFeed::Stream.pause(Duration::from_secs(4)), Duration::from_secs(4));
    }

    fn stamped(id: &str, stamp: Option<SystemTime>) -> Event {
        Event { id: id.into(), timestamp: stamp, ..Event::default() }
    }

    #[test]
    fn a_back_fill_keeps_only_events_newer_than_the_outage_oldest_first() {
        let since = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let at = |seconds| Some(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds));
        let events = vec![
            stamped("late", at(1500)),
            stamped("before", at(900)),
            stamped("exactly", at(1000)),
            stamped("unstamped", None),
            stamped("soon", at(1001)),
        ];
        let ids: Vec<String> = missed_since(events, since).into_iter().map(|event| event.id).collect();
        assert_eq!(ids, ["soon", "late"]);
    }

    #[test]
    fn calls_from_the_clients_own_worker_do_not_deadlock() {
        let daemon = missing();
        let own = runtime().expect("runtime");
        let task = own.spawn(async move { (daemon.status(), daemon.compatible()) });
        let (status, compatible) = own.block_on(task).expect("task");
        assert!(status.is_err() && compatible.is_err());
    }
}

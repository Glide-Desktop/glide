use std::{collections::HashMap, sync::Arc};

use zbus::{
    blocking::{Connection, Proxy, connection::Builder},
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

use crate::oneshot;

const AGENT_PATH: &str = "/org/glide/PolkitAgent";
const AUTHORITY: &str = "org.freedesktop.PolicyKit1";
const AUTHORITY_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub cookie: String,
    pub action: String,
    pub message: String,
    pub icon: String,
    pub user: String,
    pub caller: Option<i32>,
    pub program: Option<String>,
}

#[derive(Debug)]
pub enum Event {
    Begin(Request, oneshot::Sender<bool>),
    Cancel(String),
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.PolicyKit1.Error")]
enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Failed(String),
    Cancelled(String),
    NotAuthorized(String),
}

type Identity = (String, HashMap<String, OwnedValue>);

fn uid_of(identity: &Identity) -> Option<u32> {
    let (kind, fields) = identity;
    (kind == "unix-user").then(|| fields.get("uid").and_then(|uid| u32::try_from(uid.clone()).ok())).flatten()
}

#[must_use]
pub fn user_name(uid: u32) -> Option<String> {
    let mut buffer = vec![0_u8; 16 * 1024];
    // SAFETY: an all-zero `passwd` is a valid value for `getpwuid_r` to overwrite.
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: every pointer refers to storage owned by this frame and sized as passed; `getpwuid_r` only writes
    // into them, and `found` is either null or points at `entry`, whose strings live in `buffer`.
    let status =
        unsafe { libc::getpwuid_r(uid, &raw mut entry, buffer.as_mut_ptr().cast(), buffer.len(), &raw mut found) };
    if status != 0 || found.is_null() || entry.pw_name.is_null() {
        return None;
    }
    // SAFETY: `pw_name` was checked above and points into `buffer`, which is still alive.
    let name = unsafe { std::ffi::CStr::from_ptr(entry.pw_name) };
    name.to_str().ok().filter(|name| !name.is_empty()).map(str::to_string)
}

fn chosen_user(identities: &[Identity], own: u32, name: impl Fn(u32) -> Option<String>) -> Option<String> {
    let uids: Vec<u32> = identities.iter().filter_map(uid_of).collect();
    let uid = uids.iter().copied().find(|uid| *uid == own).or_else(|| uids.first().copied())?;
    name(uid)
}

fn pid(details: &HashMap<String, String>, key: &str) -> Option<i32> {
    details.get(key).and_then(|value| value.trim().parse().ok()).filter(|pid| *pid > 0)
}

fn request(
    action: String, message: String, icon: String, details: &HashMap<String, String>, cookie: String, user: String,
) -> Request {
    let caller = pid(details, "polkit.subject-pid").or_else(|| pid(details, "polkit.caller-pid"));
    let program = details.get("program").filter(|program| !program.trim().is_empty()).cloned();
    Request { cookie, action, message, icon, user, caller, program }
}

struct Agent {
    sink: Arc<dyn Fn(Event) + Send + Sync>,
}

async fn from_authority(connection: &zbus::Connection, header: &zbus::message::Header<'_>) -> bool {
    let Some(sender) = header.sender() else {
        return false;
    };
    let Ok(proxy) = zbus::fdo::DBusProxy::new(connection).await else {
        return false;
    };
    let Ok(name) = zbus::names::BusName::try_from(AUTHORITY) else {
        return false;
    };
    proxy.get_name_owner(name).await.is_ok_and(|owner| owner.as_str() == sender.as_str())
}

// Signatures are fixed by the org.freedesktop.PolicyKit1.AuthenticationAgent D-Bus interface.
#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
    async fn begin_authentication(
        &self, action_id: String, message: String, icon_name: String, details: HashMap<String, String>, cookie: String,
        identities: Vec<Identity>, #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> Result<(), AgentError> {
        if !from_authority(connection, &header).await {
            return Err(AgentError::NotAuthorized("only the polkit authority may ask this agent".into()));
        }
        // SAFETY: `getuid` has no preconditions and cannot fail.
        let own = unsafe { libc::getuid() };
        let Some(user) = chosen_user(&identities, own, user_name) else {
            return Err(AgentError::Failed("none of the identities is a user this agent can sign in as".into()));
        };
        let (sender, receiver) = oneshot::channel();
        (self.sink)(Event::Begin(request(action_id, message, icon_name, &details, cookie, user), sender));
        match receiver.await {
            Some(true) => Ok(()),
            Some(false) | None => Err(AgentError::Cancelled("the user dismissed the request".into())),
        }
    }

    async fn cancel_authentication(
        &self, cookie: String, #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> Result<(), AgentError> {
        if !from_authority(connection, &header).await {
            return Err(AgentError::NotAuthorized("only the polkit authority may cancel".into()));
        }
        (self.sink)(Event::Cancel(cookie));
        Ok(())
    }
}

fn locale() -> &'static str {
    match crate::i18n::language() {
        crate::i18n::Lang::Uk => "uk_UA.UTF-8",
        crate::i18n::Lang::En => "en_US.UTF-8",
    }
}

fn session_id(connection: &Connection) -> Option<String> {
    if let Some(id) = std::env::var("XDG_SESSION_ID").ok().filter(|id| !id.trim().is_empty()) {
        return Some(id);
    }
    let path: OwnedObjectPath = crate::logind::session_path(connection).ok()?;
    let session = Proxy::new(connection, "org.freedesktop.login1", path, "org.freedesktop.login1.Session").ok()?;
    session.get_property::<String>("Id").ok().filter(|id| !id.is_empty())
}

fn register(connection: &Connection) -> zbus::Result<()> {
    let Some(id) = session_id(connection) else {
        tracing::warn!("no logind session to register the polkit agent for; password prompts will not appear");
        return Err(zbus::Error::Failure("this user has no logind session".into()));
    };
    let subject = ("unix-session", HashMap::from([("session-id", Value::from(id.as_str()))]));
    let authority = Proxy::new(connection, AUTHORITY, AUTHORITY_PATH, "org.freedesktop.PolicyKit1.Authority")?;
    authority
        .call_method("RegisterAuthenticationAgent", &(subject, locale(), AGENT_PATH))
        .inspect_err(|err| tracing::warn!(%err, session = id, "polkit refused the authentication agent"))?;
    tracing::info!(session = id, "registered the polkit authentication agent");
    Ok(())
}

fn serve(builder: Builder<'_>, sink: Arc<dyn Fn(Event) + Send + Sync>) -> zbus::Result<Connection> {
    builder.serve_at(AGENT_PATH, Agent { sink })?.build()
}

pub fn start(sink: impl Fn(Event) + Send + Sync + 'static) -> anyhow::Result<()> {
    let connection = serve(Builder::system()?, Arc::new(sink))?;
    crate::bus::keep_registered(connection, AUTHORITY, "glide-polkit-agent", register)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Mutex, PoisonError, mpsc},
        time::Duration,
    };

    fn identity(kind: &str, uid: u32) -> Identity {
        let key = if kind == "unix-user" { "uid" } else { "gid" };
        (kind.into(), HashMap::from([(key.to_string(), OwnedValue::from(uid))]))
    }

    #[test]
    fn the_signed_in_user_is_preferred_and_groups_are_never_chosen() {
        let names = |uid: u32| Some(format!("user{uid}"));
        let identities = [identity("unix-group", 10), identity("unix-user", 0), identity("unix-user", 1000)];
        assert_eq!(chosen_user(&identities, 1000, names).as_deref(), Some("user1000"));
        assert_eq!(chosen_user(&identities, 1001, names).as_deref(), Some("user0"), "else the first user");
        assert_eq!(chosen_user(&[identity("unix-group", 10)], 1000, names), None);
        assert_eq!(user_name(0).as_deref(), Some("root"));
    }

    #[test]
    fn the_subject_names_the_asking_program_before_the_caller() {
        let details = HashMap::from([
            ("polkit.subject-pid".to_string(), "4182".to_string()),
            ("polkit.caller-pid".to_string(), "377".to_string()),
            ("program".to_string(), "/usr/bin/gparted".to_string()),
        ]);
        let subject = request("a".into(), "m".into(), String::new(), &details, "c".into(), "artem".into());
        assert_eq!((subject.caller, subject.program.as_deref()), (Some(4182), Some("/usr/bin/gparted")));
        let caller = HashMap::from([("polkit.caller-pid".to_string(), "377".to_string())]);
        let fallback = request("a".into(), "m".into(), String::new(), &caller, "c".into(), "artem".into());
        assert_eq!((fallback.caller, fallback.program), (Some(377), None));
    }

    #[test]
    #[ignore = "registers a short-lived agent with the system polkitd"]
    fn the_system_authority_accepts_the_agent_for_this_users_session() {
        let sink: Arc<dyn Fn(Event) + Send + Sync> = Arc::new(|_| {});
        let connection = serve(Builder::system().expect("system bus"), sink).expect("served");
        assert!(session_id(&connection).is_some(), "a logind session for this user");
        register(&connection).expect("polkitd accepts the agent");
    }

    #[test]
    #[ignore = "asks the system polkitd for an interactive check and cancels it"]
    fn the_system_authority_hands_a_request_over_and_takes_a_cancel() {
        let (sender, events) = mpsc::channel();
        let sender = Mutex::new(sender);
        let sink: Arc<dyn Fn(Event) + Send + Sync> = Arc::new(move |event| {
            let _ = sender.lock().unwrap_or_else(PoisonError::into_inner).send(event);
        });
        let connection = serve(Builder::system().expect("system bus"), sink).expect("served");
        register(&connection).expect("polkitd accepts the agent");
        let check = std::process::Command::new("pkcheck")
            .args(["--action-id", "org.freedesktop.timedate1.set-time", "--allow-user-interaction", "--process"])
            .arg(std::process::id().to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("pkcheck");
        let Ok(Event::Begin(request, answer)) = events.recv_timeout(Duration::from_secs(10)) else {
            panic!("polkitd sent no request");
        };
        assert_eq!(request.action, "org.freedesktop.timedate1.set-time");
        assert!(!request.cookie.is_empty() && !request.message.is_empty());
        assert!(!request.user.is_empty());
        answer.send(false);
        let status = check.wait_with_output().expect("pkcheck finished").status;
        assert!(!status.success(), "a cancelled request is not authorised");
    }

    #[test]
    fn only_the_authority_can_begin_and_the_answer_ends_the_call() {
        let Some(daemon) = crate::test_bus::private_bus() else { return };
        let (sender, events) = mpsc::channel();
        let sender = Mutex::new(sender);
        let sink: Arc<dyn Fn(Event) + Send + Sync> = Arc::new(move |event| {
            let _ = sender.lock().unwrap_or_else(PoisonError::into_inner).send(event);
        });
        let agent = serve(daemon.builder(), sink).expect("served");
        let destination = agent.unique_name().expect("a unique name").to_string();
        let authority = daemon.client();
        authority.request_name(AUTHORITY).expect("the test owns the authority name");
        let stranger = daemon.client();
        // SAFETY: `getuid` has no preconditions and cannot fail.
        let own = unsafe { libc::getuid() };
        let call = |client: &Connection| {
            let (client, destination) = (client.clone(), destination.clone());
            std::thread::spawn(move || {
                let details = HashMap::from([("polkit.subject-pid".to_string(), "42".to_string())]);
                let body = (
                    "org.freedesktop.timedate1.set-time",
                    "Authentication is required to set the system time.",
                    "",
                    details,
                    "cookie-1",
                    vec![identity("unix-user", own)],
                );
                client
                    .call_method(
                        Some(destination.as_str()),
                        AGENT_PATH,
                        Some("org.freedesktop.PolicyKit1.AuthenticationAgent"),
                        "BeginAuthentication",
                        &body,
                    )
                    .map(|_| ())
            })
        };

        let err = call(&stranger).join().expect("joined").expect_err("a stranger is refused");
        assert!(err.to_string().contains("PolicyKit1.Error.NotAuthorized"), "{err}");
        assert!(events.try_recv().is_err(), "nothing was shown for the stranger");

        let pending = call(&authority);
        let Ok(Event::Begin(request, answer)) = events.recv_timeout(Duration::from_secs(5)) else {
            panic!("expected a request");
        };
        assert_eq!(request.action, "org.freedesktop.timedate1.set-time");
        assert_eq!((request.cookie.as_str(), request.caller), ("cookie-1", Some(42)));
        assert_eq!(Some(request.user), user_name(own));
        answer.send(true);
        pending.join().expect("joined").expect("authorised");

        let pending = call(&authority);
        let Ok(Event::Begin(_, answer)) = events.recv_timeout(Duration::from_secs(5)) else {
            panic!("expected a request");
        };
        answer.send(false);
        let err = pending.join().expect("joined").expect_err("dismissed");
        assert!(err.to_string().contains("PolicyKit1.Error.Cancelled"), "{err}");

        let cancel = authority.call_method(
            Some(destination.as_str()),
            AGENT_PATH,
            Some("org.freedesktop.PolicyKit1.AuthenticationAgent"),
            "CancelAuthentication",
            &("cookie-1",),
        );
        assert!(cancel.is_ok());
        assert!(
            matches!(events.recv_timeout(Duration::from_secs(5)), Ok(Event::Cancel(cookie)) if cookie == "cookie-1")
        );
    }
}

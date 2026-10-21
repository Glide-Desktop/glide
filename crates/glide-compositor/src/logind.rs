use std::{sync::mpsc, time::Duration};

use zbus::{
    blocking::{Connection, MessageIterator, Proxy, fdo::DBusProxy},
    zvariant::{OwnedFd, OwnedObjectPath},
};

const LOGIN1: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";
const SESSION: &str = "org.freedesktop.login1.Session";
const USER: &str = "org.freedesktop.login1.User";
const LOCK_GRACE: Duration = Duration::from_millis(400);
pub const STARTUP_WAIT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Inhibit {
    what: &'static str,
    why: &'static str,
    mode: &'static str,
}

const SLEEP_DELAY: Inhibit = Inhibit { what: "sleep", why: "Lock the screen before sleeping", mode: "delay" };
const POWER_KEY: Inhibit = Inhibit { what: "handle-power-key", why: "Glide asks before powering off", mode: "block" };

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hint {
    Locked(bool),
    Idle(bool),
}

impl Hint {
    const fn call(self) -> (&'static str, bool) {
        match self {
            Self::Locked(on) => ("SetLockedHint", on),
            Self::Idle(on) => ("SetIdleHint", on),
        }
    }
}

pub struct Handle {
    hints: mpsc::Sender<Hint>,
    locked: bool,
    idle: bool,
}

impl Handle {
    fn new(hints: mpsc::Sender<Hint>) -> Self {
        Self { hints, locked: false, idle: false }
    }

    fn publish(&self, hint: Hint) {
        if self.hints.send(hint).is_err() {
            tracing::debug!(?hint, "logind is unavailable; the session hint was not updated");
        }
    }

    pub fn set_locked(&mut self, locked: bool) {
        if std::mem::replace(&mut self.locked, locked) != locked {
            self.publish(Hint::Locked(locked));
        }
    }

    pub fn set_idle(&mut self, idle: bool) {
        if std::mem::replace(&mut self.idle, idle) != idle {
            self.publish(Hint::Idle(idle));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginEvent {
    Lock,
    Unlock,
    Sleep,
    Wake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Signal {
    Lock,
    Unlock,
    PrepareForSleep,
}

fn classify(interface: &str, member: &str, path: &str, session: &str) -> Option<Signal> {
    match (interface, member) {
        (SESSION, "Lock") if path == session => Some(Signal::Lock),
        (SESSION, "Unlock") if path == session => Some(Signal::Unlock),
        (MANAGER, "PrepareForSleep") if path == MANAGER_PATH => Some(Signal::PrepareForSleep),
        _ => None,
    }
}

fn session_path(connection: &Connection) -> zbus::Result<OwnedObjectPath> {
    let manager = Proxy::new(connection, LOGIN1, MANAGER_PATH, MANAGER)?;
    if let Ok(id) = std::env::var("XDG_SESSION_ID")
        && !id.is_empty()
        && let Ok(reply) = manager.call_method("GetSession", &(id.as_str(),))
        && let Ok(path) = reply.body().deserialize::<OwnedObjectPath>()
    {
        return Ok(path);
    }
    if let Ok(reply) = manager.call_method("GetSessionByPID", &(std::process::id(),))
        && let Ok(path) = reply.body().deserialize::<OwnedObjectPath>()
    {
        return Ok(path);
    }
    let user = Proxy::new(connection, LOGIN1, "/org/freedesktop/login1/user/self", USER)?;
    let (_, path): (String, OwnedObjectPath) = user.get_property("Display")?;
    Ok(path)
}

fn inhibit(connection: &Connection, request: Inhibit) -> Option<OwnedFd> {
    let manager = Proxy::new(connection, LOGIN1, MANAGER_PATH, MANAGER).ok()?;
    manager
        .call_method("Inhibit", &(request.what, "Glide", request.why, request.mode))
        .and_then(|reply| reply.body().deserialize::<OwnedFd>())
        .inspect_err(|err| tracing::debug!(?err, what = request.what, "logind refused an inhibitor"))
        .ok()
}

fn locked_hint(connection: &Connection, session: &OwnedObjectPath) -> bool {
    Proxy::new(connection, LOGIN1, session.as_str(), SESSION)
        .and_then(|proxy| proxy.get_property::<bool>("LockedHint"))
        .inspect_err(|err| tracing::debug!(?err, "cannot read the session's locked hint"))
        .unwrap_or(false)
}

fn publish_hints(connection: &Connection, session: &OwnedObjectPath, hints: mpsc::Receiver<Hint>) {
    let proxy = match Proxy::new(connection, LOGIN1, session.as_str(), SESSION) {
        Ok(proxy) => proxy,
        Err(err) => {
            tracing::warn!(?err, "cannot reach the logind session; a restart while locked will come back unlocked");
            return;
        }
    };
    for hint in hints {
        let (method, on) = hint.call();
        if let Err(err) = proxy.call_method(method, &(on,)) {
            tracing::warn!(?err, ?hint, "logind did not take a session hint");
        }
    }
}

fn watch(
    sink: &dyn Fn(LoginEvent), hints: mpsc::Receiver<Hint>, locked_at_start: &mpsc::Sender<bool>,
) -> zbus::Result<()> {
    let connection = Connection::system()?;
    let session = session_path(&connection)?;
    tracing::info!(session = %session.as_str(), "listening for logind lock requests");
    let locked = locked_hint(&connection, &session);
    if locked {
        tracing::warn!("the session was left locked by a previous compositor; locking again");
        sink(LoginEvent::Lock);
    }
    let _ = locked_at_start.send(locked);
    let publisher = (connection.clone(), session.clone());
    let spawned = std::thread::Builder::new()
        .name("glide-logind-hint".into())
        .spawn(move || publish_hints(&publisher.0, &publisher.1, hints));
    if let Err(err) = spawned {
        tracing::warn!(?err, "cannot publish the locked hint; a restart while locked will come back unlocked");
    }
    let bus = DBusProxy::new(&connection)?;
    for rule in [
        format!("type='signal',sender='{LOGIN1}',interface='{SESSION}',path='{}'", session.as_str()),
        format!("type='signal',sender='{LOGIN1}',interface='{MANAGER}',member='PrepareForSleep'"),
    ] {
        bus.add_match_rule(zbus::MatchRule::try_from(rule.as_str())?)?;
    }
    let power_key = inhibit(&connection, POWER_KEY);
    if power_key.is_none() {
        tracing::warn!("no power key inhibitor; logind will act on the power button without asking");
    }
    let mut inhibitor = inhibit(&connection, SLEEP_DELAY);
    for message in MessageIterator::from(&connection) {
        let Ok(message) = message else {
            continue;
        };
        let header = message.header();
        if header.message_type() != zbus::message::Type::Signal {
            continue;
        }
        let (Some(interface), Some(member), Some(path)) = (header.interface(), header.member(), header.path()) else {
            continue;
        };
        let signal = classify(interface.as_str(), member.as_str(), path.as_str(), session.as_str());
        match signal {
            Some(Signal::Lock) => sink(LoginEvent::Lock),
            Some(Signal::Unlock) => sink(LoginEvent::Unlock),
            Some(Signal::PrepareForSleep) => {
                if message.body().deserialize::<bool>().unwrap_or(false) {
                    sink(LoginEvent::Sleep);
                    std::thread::sleep(LOCK_GRACE);
                    inhibitor = None;
                } else {
                    sink(LoginEvent::Wake);
                    if inhibitor.is_none() {
                        inhibitor = inhibit(&connection, SLEEP_DELAY);
                    }
                }
            }
            None => {}
        }
    }
    drop((inhibitor, power_key));
    Ok(())
}

pub fn start(sink: impl Fn(LoginEvent) + Send + 'static) -> std::io::Result<(Handle, mpsc::Receiver<bool>)> {
    let (hints, hint_requests) = mpsc::channel();
    let (locked_sender, locked_at_start) = mpsc::channel();
    std::thread::Builder::new().name("glide-logind".into()).spawn(move || {
        if let Err(err) = watch(&sink, hint_requests, &locked_sender) {
            tracing::warn!(
                "logind is unavailable ({err}); loginctl lock-session and lock-on-sleep will not lock \
                 the screen, and the power button is left to logind"
            );
        }
    })?;
    Ok((Handle::new(hints), locked_at_start))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: &str = "/org/freedesktop/login1/session/_32";

    #[test]
    fn only_our_session_can_lock_or_unlock_the_screen() {
        assert_eq!(classify(SESSION, "Lock", OURS, OURS), Some(Signal::Lock));
        assert_eq!(classify(SESSION, "Unlock", OURS, OURS), Some(Signal::Unlock));
        assert_eq!(classify(SESSION, "Lock", "/org/freedesktop/login1/session/c1", OURS), None);
        assert_eq!(classify(SESSION, "PauseDevice", OURS, OURS), None);
    }

    #[test]
    fn only_the_power_key_is_taken_away_from_logind() {
        assert_eq!((POWER_KEY.what, POWER_KEY.mode), ("handle-power-key", "block"));
        assert_eq!((SLEEP_DELAY.what, SLEEP_DELAY.mode), ("sleep", "delay"));
    }

    #[test]
    fn session_hints_are_published_once_per_change() {
        let (hints, published) = mpsc::channel();
        let mut handle = Handle::new(hints);
        for locked in [false, true, true, false, false, true] {
            handle.set_locked(locked);
        }
        handle.set_idle(true);
        handle.set_idle(true);
        assert_eq!(
            published.try_iter().collect::<Vec<_>>(),
            [Hint::Locked(true), Hint::Locked(false), Hint::Locked(true), Hint::Idle(true)]
        );
        assert_eq!(Hint::Locked(true).call(), ("SetLockedHint", true));
        assert_eq!(Hint::Idle(false).call(), ("SetIdleHint", false));
    }

    #[test]
    fn a_dead_logind_thread_does_not_break_locking() {
        let (hints, published) = mpsc::channel();
        drop(published);
        Handle::new(hints).set_locked(true);
    }

    #[test]
    fn sleep_is_announced_by_the_manager_object() {
        assert_eq!(classify(MANAGER, "PrepareForSleep", MANAGER_PATH, OURS), Some(Signal::PrepareForSleep));
        assert_eq!(classify(MANAGER, "PrepareForSleep", OURS, OURS), None);
    }
}

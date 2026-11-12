use std::{
    sync::{Arc, Mutex, PoisonError, mpsc},
    time::{Duration, Instant},
};

use zbus::{
    blocking::{Connection, MessageIterator, Proxy, fdo::DBusProxy},
    zvariant::{OwnedFd, OwnedObjectPath},
};

const LOGIN1: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";
const SESSION: &str = "org.freedesktop.login1.Session";
const USER: &str = "org.freedesktop.login1.User";
const SLEEP_GRACE: Duration = Duration::from_secs(2);
const FIRST_RETRY: Duration = Duration::from_secs(1);
const LAST_RETRY: Duration = Duration::from_mins(1);
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Note {
    Hint(Hint),
    Session(OwnedObjectPath),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Hints {
    locked: Option<bool>,
    idle: Option<bool>,
}

impl Hints {
    const fn note(&mut self, hint: Hint) {
        match hint {
            Hint::Locked(on) => self.locked = Some(on),
            Hint::Idle(on) => self.idle = Some(on),
        }
    }

    fn drain(&mut self) -> Vec<Hint> {
        let hints = [self.locked.take().map(Hint::Locked), self.idle.take().map(Hint::Idle)];
        hints.into_iter().flatten().collect()
    }
}

#[derive(Default)]
struct SleepGate {
    fd: Option<OwnedFd>,
    waiting: Option<u64>,
    round: u64,
}

impl SleepGate {
    fn hold(&mut self, fd: Option<OwnedFd>) {
        self.fd = fd;
    }

    const fn begin(&mut self) -> u64 {
        self.round += 1;
        self.waiting = Some(self.round);
        self.round
    }

    fn release(&mut self, round: Option<u64>) -> bool {
        match self.waiting {
            Some(waiting) if round.is_none_or(|round| round == waiting) => {
                self.waiting = None;
                self.fd = None;
                true
            }
            _ => false,
        }
    }

    const fn wake(&mut self) -> bool {
        self.waiting = None;
        self.fd.is_none()
    }
}

type Gate = Arc<Mutex<SleepGate>>;

fn gate(gate: &Gate) -> std::sync::MutexGuard<'_, SleepGate> {
    gate.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Handle {
    notes: mpsc::Sender<Note>,
    locked: bool,
    idle: bool,
    sleep: Gate,
    lock_seen: Option<u64>,
}

impl Handle {
    fn new(notes: mpsc::Sender<Note>, sleep: Gate) -> Self {
        Self { notes, locked: false, idle: false, sleep, lock_seen: None }
    }

    fn publish(&self, hint: Hint) {
        if self.notes.send(Note::Hint(hint)).is_err() {
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

    pub fn sleep_ready(&self) {
        if gate(&self.sleep).release(None) {
            tracing::info!("the lock screen is up; letting the system sleep");
        }
    }

    pub fn settle_sleep(&mut self, locked: bool) {
        let waiting = gate(&self.sleep).waiting;
        match waiting {
            Some(round) if locked && self.lock_seen == Some(round) => {
                self.lock_seen = None;
                self.sleep_ready();
            }
            Some(round) if locked => self.lock_seen = Some(round),
            _ => self.lock_seen = None,
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

const BUS: &str = "org.freedesktop.DBus";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Origin<'a> {
    sender: Option<&'a str>,
    owner: Option<&'a str>,
}

impl Origin<'_> {
    fn is_logind(self) -> bool {
        self.sender.is_some() && self.sender == self.owner
    }
}

fn classify(origin: Origin<'_>, interface: &str, member: &str, path: &str, session: &str) -> Option<Signal> {
    if !origin.is_logind() {
        return None;
    }
    match (interface, member) {
        (SESSION, "Lock") if path == session => Some(Signal::Lock),
        (SESSION, "Unlock") if path == session => Some(Signal::Unlock),
        (MANAGER, "PrepareForSleep") if path == MANAGER_PATH => Some(Signal::PrepareForSleep),
        _ => None,
    }
}

fn session_uid(connection: &Connection, path: &OwnedObjectPath) -> Option<u32> {
    let session = Proxy::new(connection, LOGIN1, path.as_str(), SESSION).ok()?;
    session.get_property::<(u32, OwnedObjectPath)>("User").ok().map(|(uid, _)| uid)
}

fn own_session(connection: &Connection, path: OwnedObjectPath) -> Option<OwnedObjectPath> {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let own = unsafe { libc::getuid() };
    match session_uid(connection, &path) {
        Some(uid) if uid == own => Some(path),
        Some(uid) => {
            tracing::warn!(session = %path.as_str(), uid, "ignoring a logind session that belongs to another user");
            None
        }
        None => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    path: OwnedObjectPath,
    active: bool,
    seated: bool,
}

fn pick_session(candidates: Vec<Candidate>) -> Option<OwnedObjectPath> {
    let best = candidates
        .iter()
        .position(|found| found.active && found.seated)
        .or_else(|| candidates.iter().position(|found| found.seated))
        .unwrap_or(0);
    candidates.into_iter().nth(best).map(|found| found.path)
}

fn displayed_session(connection: &Connection) -> zbus::Result<OwnedObjectPath> {
    let user = Proxy::new(connection, LOGIN1, "/org/freedesktop/login1/user/self", USER)?;
    let (_, display): (String, OwnedObjectPath) = user.get_property("Display")?;
    if display.as_str() != "/" {
        return Ok(display);
    }
    let sessions: Vec<(String, OwnedObjectPath)> = user.get_property("Sessions")?;
    let candidates = sessions
        .into_iter()
        .map(|(_, path)| {
            let (active, seat) =
                Proxy::new(connection, LOGIN1, path.as_str(), SESSION).map_or((None, None), |session| {
                    (
                        session.get_property::<bool>("Active").ok(),
                        session.get_property::<(String, OwnedObjectPath)>("Seat").ok(),
                    )
                });
            Candidate { path, active: active.unwrap_or(false), seated: seat.is_some_and(|(id, _)| !id.is_empty()) }
        })
        .collect();
    pick_session(candidates).ok_or_else(|| zbus::Error::Failure("this user has no logind session".into()))
}

pub fn session_path(connection: &Connection) -> zbus::Result<OwnedObjectPath> {
    let manager = Proxy::new(connection, LOGIN1, MANAGER_PATH, MANAGER)?;
    if let Ok(id) = std::env::var("XDG_SESSION_ID")
        && !id.is_empty()
        && let Ok(reply) = manager.call_method("GetSession", &(id.as_str(),))
        && let Ok(path) = reply.body().deserialize::<OwnedObjectPath>()
        && let Some(path) = own_session(connection, path)
    {
        return Ok(path);
    }
    if let Ok(reply) = manager.call_method("GetSessionByPID", &(std::process::id(),))
        && let Ok(path) = reply.body().deserialize::<OwnedObjectPath>()
    {
        return Ok(path);
    }
    displayed_session(connection)
}

fn login1_owner(bus: &DBusProxy<'_>) -> Option<String> {
    let name = zbus::names::BusName::try_from(LOGIN1).ok()?;
    bus.get_name_owner(name)
        .inspect_err(|err| tracing::debug!(?err, "cannot resolve the owner of logind's bus name"))
        .ok()
        .map(|owner| owner.to_string())
}

fn login1_owner_change(message: &zbus::Message) -> Option<String> {
    let header = message.header();
    let from_bus = header.sender().is_some_and(|sender| sender.as_str() == BUS)
        && header.interface().is_some_and(|name| name.as_str() == BUS)
        && header.member().is_some_and(|name| name.as_str() == "NameOwnerChanged");
    if !from_bus {
        return None;
    }
    let (name, _, owner) = message.body().deserialize::<(String, String, String)>().ok()?;
    (name == LOGIN1).then_some(owner)
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

fn publish_hints(notes: mpsc::Receiver<Note>) {
    let mut connection: Option<Connection> = None;
    let mut session: Option<OwnedObjectPath> = None;
    let mut known = Hints::default();
    let mut pending = Hints::default();
    for note in notes {
        match note {
            Note::Session(path) => {
                session = Some(path);
                pending = known;
            }
            Note::Hint(hint) => {
                known.note(hint);
                pending.note(hint);
            }
        }
        let Some(path) = session.as_ref() else {
            continue;
        };
        if connection.is_none() {
            connection = Connection::system()
                .inspect_err(|err| tracing::warn!(?err, "cannot reach the system bus to publish session hints"))
                .ok();
        }
        let Some(bus) = connection.as_ref() else {
            continue;
        };
        let proxy = match Proxy::new(bus, LOGIN1, path.as_str(), SESSION) {
            Ok(proxy) => proxy,
            Err(err) => {
                tracing::warn!(?err, "cannot reach the logind session; a restart while locked will come back unlocked");
                continue;
            }
        };
        let mut lost = false;
        for hint in pending.drain() {
            let (method, on) = hint.call();
            if let Err(err) = proxy.call_method(method, &(on,)) {
                lost |= connection_lost(&err);
                tracing::warn!(?err, ?hint, "logind did not take a session hint");
            }
        }
        drop(proxy);
        if lost {
            connection = None;
            pending = known;
        }
    }
}

fn connection_lost(err: &zbus::Error) -> bool {
    matches!(err, zbus::Error::InputOutput(_))
}

fn going_to_sleep(sink: &dyn Fn(LoginEvent), sleep: &Gate) {
    sink(LoginEvent::Sleep);
    let round = gate(sleep).begin();
    let timer = Arc::clone(sleep);
    let spawned = std::thread::Builder::new().name("glide-logind-sleep".into()).spawn(move || {
        std::thread::sleep(SLEEP_GRACE);
        if gate(&timer).release(Some(round)) {
            tracing::warn!("the lock screen was not drawn in time; letting the system sleep anyway");
        }
    });
    if let Err(err) = spawned {
        tracing::warn!(?err, "cannot time the sleep delay; releasing it now");
        gate(sleep).release(Some(round));
    }
}

fn watch(
    sink: &dyn Fn(LoginEvent), notes: &mpsc::Sender<Note>, sleep: &Gate, announce: Option<&mpsc::Sender<bool>>,
) -> zbus::Result<()> {
    let connection = Connection::system()?;
    let session = session_path(&connection)?;
    tracing::info!(session = %session.as_str(), "listening for logind lock requests");
    if let Some(announce) = announce {
        let locked = locked_hint(&connection, &session);
        if locked {
            tracing::warn!("the session was left locked by a previous compositor; locking again");
            sink(LoginEvent::Lock);
        }
        let _ = announce.send(locked);
    }
    let _ = notes.send(Note::Session(session.clone()));
    let messages = MessageIterator::from(&connection);
    let bus = DBusProxy::new(&connection)?;
    for rule in [
        format!("type='signal',sender='{LOGIN1}',interface='{SESSION}',path='{}'", session.as_str()),
        format!("type='signal',sender='{LOGIN1}',interface='{MANAGER}',member='PrepareForSleep'"),
        format!("type='signal',sender='{BUS}',interface='{BUS}',member='NameOwnerChanged',arg0='{LOGIN1}'"),
    ] {
        bus.add_match_rule(zbus::MatchRule::try_from(rule.as_str())?)?;
    }
    let mut owner = login1_owner(&bus);
    let power_key = inhibit(&connection, POWER_KEY);
    if power_key.is_none() {
        tracing::warn!("no power key inhibitor; logind will act on the power button without asking");
    }
    let delay = inhibit(&connection, SLEEP_DELAY);
    gate(sleep).hold(delay);
    for message in messages {
        let Ok(message) = message else {
            continue;
        };
        let header = message.header();
        if header.message_type() != zbus::message::Type::Signal {
            continue;
        }
        if let Some(changed) = login1_owner_change(&message) {
            if changed.is_empty() {
                owner = None;
            } else if owner.as_deref() != Some(changed.as_str()) {
                drop(power_key);
                return Ok(());
            }
            continue;
        }
        let (Some(interface), Some(member), Some(path)) = (header.interface(), header.member(), header.path()) else {
            continue;
        };
        let origin = Origin { sender: header.sender().map(zbus::names::UniqueName::as_str), owner: owner.as_deref() };
        let signal = classify(origin, interface.as_str(), member.as_str(), path.as_str(), session.as_str());
        match signal {
            Some(Signal::Lock) => sink(LoginEvent::Lock),
            Some(Signal::Unlock) => sink(LoginEvent::Unlock),
            Some(Signal::PrepareForSleep) => {
                if message.body().deserialize::<bool>().unwrap_or(false) {
                    going_to_sleep(sink, sleep);
                } else {
                    sink(LoginEvent::Wake);
                    if gate(sleep).wake() {
                        let fd = inhibit(&connection, SLEEP_DELAY);
                        gate(sleep).hold(fd);
                    }
                }
            }
            None => {}
        }
    }
    drop(power_key);
    Err(zbus::Error::Failure("the system bus closed the connection".into()))
}

fn keep_watching(
    sink: &dyn Fn(LoginEvent), notes: &mpsc::Sender<Note>, sleep: &Gate, locked_at_start: mpsc::Sender<bool>,
) {
    let mut announce = Some(locked_at_start);
    let mut delay = FIRST_RETRY;
    let mut failures = 0_u32;
    loop {
        let started = Instant::now();
        let first = announce.take();
        match watch(sink, notes, sleep, first.as_ref()) {
            Ok(()) => {
                tracing::info!("logind restarted; resolving the session and taking the inhibitors again");
                delay = FIRST_RETRY;
                continue;
            }
            Err(err) if failures == 0 => tracing::warn!(
                "logind is unavailable ({err}); loginctl lock-session and lock-on-sleep will not lock the screen, \
                 and the power button is left to logind until it comes back"
            ),
            Err(err) => tracing::debug!(%err, failures, "logind is still unavailable"),
        }
        drop(first);
        failures += 1;
        if started.elapsed() >= LAST_RETRY {
            delay = FIRST_RETRY;
        }
        std::thread::sleep(delay);
        delay = (delay * 2).min(LAST_RETRY);
    }
}

pub fn start(sink: impl Fn(LoginEvent) + Send + 'static) -> std::io::Result<(Handle, mpsc::Receiver<bool>)> {
    let (notes, note_requests) = mpsc::channel();
    let (locked_sender, locked_at_start) = mpsc::channel();
    let sleep = Gate::default();
    std::thread::Builder::new().name("glide-logind-hint".into()).spawn(move || publish_hints(note_requests))?;
    let (watch_notes, watch_sleep) = (notes.clone(), Arc::clone(&sleep));
    std::thread::Builder::new()
        .name("glide-logind".into())
        .spawn(move || keep_watching(&sink, &watch_notes, &watch_sleep, locked_sender))?;
    Ok((Handle::new(notes, sleep), locked_at_start))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: &str = "/org/freedesktop/login1/session/_32";
    const LOGIND: Origin<'static> = Origin { sender: Some(":1.3"), owner: Some(":1.3") };

    fn handle() -> (Handle, mpsc::Receiver<Note>) {
        let (notes, published) = mpsc::channel();
        (Handle::new(notes, Gate::default()), published)
    }

    fn fd() -> OwnedFd {
        OwnedFd::from(std::os::fd::OwnedFd::from(std::fs::File::open("/dev/null").expect("/dev/null")))
    }

    fn path(text: &str) -> OwnedObjectPath {
        OwnedObjectPath::try_from(text).expect("an object path")
    }

    #[test]
    fn only_our_session_can_lock_or_unlock_the_screen() {
        assert_eq!(classify(LOGIND, SESSION, "Lock", OURS, OURS), Some(Signal::Lock));
        assert_eq!(classify(LOGIND, SESSION, "Unlock", OURS, OURS), Some(Signal::Unlock));
        assert_eq!(classify(LOGIND, SESSION, "Lock", "/org/freedesktop/login1/session/c1", OURS), None);
        assert_eq!(classify(LOGIND, SESSION, "PauseDevice", OURS, OURS), None);
    }

    #[test]
    fn a_signal_forged_by_another_peer_never_unlocks_the_screen() {
        let forged = Origin { sender: Some(":1.99"), owner: Some(":1.3") };
        assert_eq!(classify(forged, SESSION, "Unlock", OURS, OURS), None);
        assert_eq!(classify(forged, MANAGER, "PrepareForSleep", MANAGER_PATH, OURS), None);
        let unknown_owner = Origin { sender: Some(":1.3"), owner: None };
        assert_eq!(classify(unknown_owner, SESSION, "Unlock", OURS, OURS), None);
        let no_sender = Origin { sender: None, owner: None };
        assert_eq!(classify(no_sender, SESSION, "Lock", OURS, OURS), None);
    }

    #[test]
    fn only_the_power_key_is_taken_away_from_logind() {
        assert_eq!((POWER_KEY.what, POWER_KEY.mode), ("handle-power-key", "block"));
        assert_eq!((SLEEP_DELAY.what, SLEEP_DELAY.mode), ("sleep", "delay"));
    }

    #[test]
    fn session_hints_are_published_once_per_change() {
        let (mut handle, published) = handle();
        for locked in [false, true, true, false, false, true] {
            handle.set_locked(locked);
        }
        handle.set_idle(true);
        handle.set_idle(true);
        assert_eq!(
            published.try_iter().collect::<Vec<_>>(),
            [Hint::Locked(true), Hint::Locked(false), Hint::Locked(true), Hint::Idle(true)].map(Note::Hint)
        );
        assert_eq!(Hint::Locked(true).call(), ("SetLockedHint", true));
        assert_eq!(Hint::Idle(false).call(), ("SetIdleHint", false));
    }

    #[test]
    fn hints_wait_for_a_session_and_the_latest_of_each_kind_is_sent() {
        let mut pending = Hints::default();
        for hint in [Hint::Locked(true), Hint::Idle(true), Hint::Locked(false)] {
            pending.note(hint);
        }
        let known = pending;
        assert_eq!(pending.drain(), [Hint::Locked(false), Hint::Idle(true)]);
        assert!(pending.drain().is_empty(), "a drained hint is not sent twice");
        let mut again = known;
        assert_eq!(again.drain().len(), 2, "a new session is told the known state again");
    }

    #[test]
    fn only_a_broken_bus_connection_is_opened_again() {
        let closed =
            zbus::Error::InputOutput(std::sync::Arc::new(std::io::Error::from(std::io::ErrorKind::BrokenPipe)));
        assert!(connection_lost(&closed));
        assert!(!connection_lost(&zbus::Error::Failure("logind refused".into())), "a refusal keeps the connection");
    }

    #[test]
    fn a_dead_logind_thread_does_not_break_locking() {
        let (mut handle, published) = handle();
        drop(published);
        handle.set_locked(true);
        handle.sleep_ready();
    }

    #[test]
    fn sleep_is_announced_by_the_manager_object() {
        assert_eq!(classify(LOGIND, MANAGER, "PrepareForSleep", MANAGER_PATH, OURS), Some(Signal::PrepareForSleep));
        assert_eq!(classify(LOGIND, MANAGER, "PrepareForSleep", OURS, OURS), None);
    }

    #[test]
    fn the_sleep_delay_is_held_until_the_lock_screen_had_a_pass_to_draw() {
        let (mut handle, _published) = handle();
        gate(&handle.sleep).hold(Some(fd()));
        handle.settle_sleep(true);
        assert!(gate(&handle.sleep).fd.is_some(), "nothing waits, nothing is released");
        let round = gate(&handle.sleep).begin();
        handle.settle_sleep(false);
        handle.settle_sleep(true);
        assert!(gate(&handle.sleep).fd.is_some(), "the pass that locked has not drawn yet");
        handle.settle_sleep(true);
        assert!(gate(&handle.sleep).fd.is_none(), "the next locked pass lets the system sleep");
        assert!(!gate(&handle.sleep).release(Some(round)), "the timeout finds nothing left to release");
    }

    #[test]
    fn a_stale_sleep_timeout_never_drops_the_inhibitor_taken_after_waking() {
        let mut sleep = SleepGate::default();
        sleep.hold(Some(fd()));
        let first = sleep.begin();
        assert!(!sleep.wake(), "woke with the inhibitor still held");
        assert!(!sleep.release(Some(first)), "the first round's timer is spent");
        assert!(sleep.fd.is_some());
        let second = sleep.begin();
        assert!(sleep.release(Some(second)));
        assert!(sleep.wake(), "after a sleep the inhibitor is taken again");
    }

    #[test]
    fn a_session_with_a_seat_is_preferred_and_the_active_one_first() {
        let candidate = |name: &str, active: bool, seated: bool| Candidate {
            path: path(&format!("/org/freedesktop/login1/session/{name}")),
            active,
            seated,
        };
        let picked = pick_session(vec![
            candidate("ssh", true, false),
            candidate("idle", false, true),
            candidate("desk", true, true),
        ]);
        assert_eq!(picked, Some(path("/org/freedesktop/login1/session/desk")));
        let picked = pick_session(vec![candidate("ssh", true, false), candidate("idle", false, true)]);
        assert_eq!(picked, Some(path("/org/freedesktop/login1/session/idle")));
        let picked = pick_session(vec![candidate("ssh", true, false)]);
        assert_eq!(picked, Some(path("/org/freedesktop/login1/session/ssh")));
        assert_eq!(pick_session(Vec::new()), None);
    }
}

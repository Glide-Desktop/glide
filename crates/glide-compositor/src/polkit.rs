use std::sync::Arc;
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use crate::oneshot;
use crate::polkit_agent::{Event, Request};
use crate::polkit_helper::{Cancel, Outcome, Report};

pub const ATTEMPTS: u8 = 3;
pub const DONE_HOLD: Duration = Duration::from_millis(1600);
const MAX_PASSWORD: usize = 512;

pub use glide_topbar::polkit::{Control, Hint, Motion, Stage, View};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Type(char),
    Backspace,
    Clear,
    Submit,
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Nothing,
    Changed,
    Close,
}

#[derive(Debug, Clone, Copy, Default)]
struct Panels {
    reveal: bool,
    details: bool,
    shield: bool,
}

pub struct Prompt {
    pub request: Request,
    pub app: Option<String>,
    pub icon: Option<String>,
    pub exe: Option<String>,
    pub display: Option<String>,
    stage: Stage,
    password: Zeroizing<String>,
    open: Panels,
    failures: u8,
    broken: bool,
    note: Option<String>,
    shaken: Option<Instant>,
    checking: Option<Instant>,
    done: Option<Instant>,
    attempt: u64,
    answer: Option<oneshot::Sender<bool>>,
    cancel: Option<Cancel>,
}

impl std::fmt::Debug for Prompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prompt")
            .field("action", &self.request.action)
            .field("stage", &self.stage)
            .field("failures", &self.failures)
            .finish_non_exhaustive()
    }
}

fn locked_out(report: &Report) -> Option<String> {
    report.notes.iter().chain(&report.errors).map(|text| text.trim()).find(|text| !text.is_empty()).map(str::to_string)
}

impl Prompt {
    #[must_use]
    pub fn new(request: Request, answer: oneshot::Sender<bool>) -> Self {
        Self {
            request,
            app: None,
            icon: None,
            exe: None,
            display: None,
            stage: Stage::Ask,
            password: Zeroizing::new(String::with_capacity(MAX_PASSWORD)),
            open: Panels::default(),
            failures: 0,
            broken: false,
            note: None,
            shaken: None,
            checking: None,
            done: None,
            attempt: 0,
            answer: Some(answer),
            cancel: None,
        }
    }

    #[must_use]
    pub const fn stage(&self) -> Stage {
        self.stage
    }

    const fn editable(&self) -> bool {
        matches!(self.stage, Stage::Ask | Stage::Wrong)
    }

    pub fn type_char(&mut self, ch: char) -> bool {
        if !self.editable() || ch.is_control() || self.password.len() + ch.len_utf8() > MAX_PASSWORD {
            return false;
        }
        self.password.push(ch);
        self.settle_typing();
        true
    }

    pub fn backspace(&mut self) -> bool {
        if !self.editable() || self.password.pop().is_none() {
            return false;
        }
        self.settle_typing();
        true
    }

    pub fn clear(&mut self) -> bool {
        if !self.editable() || self.password.is_empty() {
            return false;
        }
        self.password.clear();
        true
    }

    fn settle_typing(&mut self) {
        if self.stage == Stage::Wrong {
            self.stage = Stage::Ask;
        }
        self.broken = false;
    }

    pub fn submit(&mut self, now: Instant) -> Option<(u64, Zeroizing<String>, Cancel)> {
        if !self.editable() || self.password.is_empty() {
            return None;
        }
        let mut secret = Zeroizing::new(String::with_capacity(MAX_PASSWORD));
        secret.push_str(&self.password);
        self.stage = Stage::Checking;
        self.checking = Some(now);
        self.open.shield = false;
        self.attempt += 1;
        let cancel = Cancel::default();
        self.cancel = Some(cancel.clone());
        Some((self.attempt, secret, cancel))
    }

    pub fn report(&mut self, attempt: u64, report: &Report, now: Instant) -> bool {
        if attempt != self.attempt || self.stage != Stage::Checking {
            return false;
        }
        self.cancel = None;
        self.checking = None;
        self.password.clear();
        self.open.reveal = false;
        match &report.outcome {
            Outcome::Success => {
                self.stage = Stage::Done;
                self.done = Some(now);
                self.resolve(true);
            }
            Outcome::Failure => {
                self.failures = self.failures.saturating_add(1);
                self.shaken = Some(now);
                if let Some(text) = locked_out(report) {
                    self.note = Some(text);
                    self.stage = Stage::Locked;
                } else if self.failures >= ATTEMPTS {
                    self.stage = Stage::Locked;
                } else {
                    self.stage = Stage::Wrong;
                }
            }
            Outcome::Broken(reason) => {
                tracing::warn!(reason, "the polkit helper could not check the password");
                self.broken = true;
                self.shaken = Some(now);
                self.stage = Stage::Wrong;
            }
        }
        true
    }

    fn resolve(&mut self, granted: bool) {
        if let Some(answer) = self.answer.take() {
            answer.send(granted);
        }
    }

    pub fn dismiss(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        self.resolve(false);
    }

    pub fn control(&mut self, control: Control) -> Step {
        match control {
            Control::Close | Control::Cancel | Control::Understood => Step::Close,
            Control::Reveal if self.editable() => {
                self.open.reveal = !self.open.reveal;
                Step::Changed
            }
            Control::Shield => {
                self.open.shield = !self.open.shield;
                Step::Changed
            }
            Control::Details if !matches!(self.stage, Stage::Done | Stage::Locked) => {
                self.open.details = !self.open.details;
                Step::Changed
            }
            Control::Field | Control::Reveal | Control::Submit | Control::Details => Step::Nothing,
        }
    }

    pub fn escape(&mut self) -> Step {
        if self.open.shield {
            self.open.shield = false;
            return Step::Changed;
        }
        Step::Close
    }

    #[must_use]
    pub fn finished(&self, now: Instant) -> bool {
        self.done.is_some_and(|at| now.saturating_duration_since(at) >= DONE_HOLD)
    }

    #[must_use]
    pub fn is_animating(&self, now: Instant) -> bool {
        self.stage == Stage::Checking
            || self.done.is_some()
            || self.shaken.is_some_and(|at| now.saturating_duration_since(at) < glide_theme::motion::t3())
    }

    #[must_use]
    pub const fn motion(&self) -> Motion {
        Motion { shaken: self.shaken, checking: self.checking }
    }

    #[must_use]
    pub fn view(&self, caps_lock: bool) -> View {
        let hint = match self.stage {
            Stage::Checking => Hint::Checking,
            Stage::Wrong if self.broken => Hint::Broken,
            Stage::Wrong => Hint::Wrong { left: ATTEMPTS.saturating_sub(self.failures) },
            _ if caps_lock => Hint::CapsLock,
            _ => Hint::Keys,
        };
        let program = self
            .request
            .program
            .clone()
            .or_else(|| self.exe.clone())
            .map(|path| self.request.caller.map_or_else(|| path.clone(), |pid| format!("{path} · pid {pid}")));
        View {
            stage: self.stage,
            app: self.app.clone(),
            icon: self.icon.clone(),
            message: self.request.message.clone(),
            user: self.request.user.clone(),
            display: self.display.clone().unwrap_or_else(|| self.request.user.clone()),
            action: self.request.action.clone(),
            program,
            bullets: self.password.chars().count(),
            shown: self.open.reveal.then(|| self.password.to_string()),
            details: self.open.details,
            shield: self.open.shield,
            hint,
            note: self.note.clone(),
            ready: self.editable() && !self.password.is_empty(),
        }
    }
}

impl Drop for Prompt {
    fn drop(&mut self) {
        self.dismiss();
    }
}

pub type Attempt = (u64, Report);

#[derive(Default)]
pub struct Polkit {
    pub prompt: Option<Prompt>,
    queue: Vec<(Request, oneshot::Sender<bool>)>,
    reports: Option<Arc<dyn Fn(Attempt) + Send + Sync>>,
}

impl Polkit {
    pub fn attach(&mut self, reports: impl Fn(Attempt) + Send + Sync + 'static) {
        self.reports = Some(Arc::new(reports));
    }
}

fn read_process(pid: i32) -> (Option<String>, Option<String>) {
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok().and_then(|path| path.to_str().map(str::to_string));
    let argv0 = std::fs::read(format!("/proc/{pid}/cmdline"))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes.split(|b| *b == 0).next()?.to_vec()).ok())
        .filter(|arg| !arg.is_empty());
    (exe, argv0)
}

impl crate::GlideState {
    pub fn handle_polkit(&mut self, event: Event) {
        match event {
            Event::Begin(request, answer) => {
                tracing::info!(action = request.action, user = request.user, "polkit asks for authentication");
                if self.screen_locked() {
                    answer.send(false);
                    return;
                }
                if self.session.polkit.prompt.is_some() {
                    self.session.polkit.queue.push((request, answer));
                    return;
                }
                self.open_polkit(request, answer);
            }
            Event::Cancel(cookie) => {
                self.session.polkit.queue.retain(|(request, _)| request.cookie != cookie);
                if self.session.polkit.prompt.as_ref().is_some_and(|prompt| prompt.request.cookie == cookie) {
                    tracing::info!("polkit withdrew the authentication request");
                    self.close_polkit();
                }
            }
        }
    }

    fn open_polkit(&mut self, request: Request, answer: oneshot::Sender<bool>) {
        if self.switcher_open() {
            self.cancel_switcher();
        }
        self.cancel_power_dialog();
        self.power_dialog = None;
        if self.shot_active() {
            self.cancel_shot();
        }
        let mut prompt = Prompt::new(request, answer);
        let (exe, argv0) = prompt.request.caller.map(read_process).unwrap_or_default();
        let app = self.app_directory.index().and_then(|index| index.match_process(exe.as_deref(), argv0.as_deref()));
        prompt.app = app.map(|app| app.name.clone()).filter(|name| !name.trim().is_empty());
        prompt.icon = app.map(|app| app.id.clone());
        prompt.exe = exe;
        let login = crate::greeter::current_user(&self.topbar.system_status().user_name);
        if login.username == prompt.request.user {
            prompt.display = Some(login.display_name).filter(|name| !name.trim().is_empty());
        }
        self.session.polkit.prompt = Some(prompt);
        self.sync_polkit();
    }

    pub fn close_polkit(&mut self) {
        if let Some(mut prompt) = self.session.polkit.prompt.take() {
            prompt.dismiss();
        }
        let locked = self.screen_locked();
        let queue = &mut self.session.polkit.queue;
        let next = (!locked && !queue.is_empty()).then(|| queue.remove(0));
        if let Some((request, answer)) = next {
            self.open_polkit(request, answer);
        } else {
            self.sync_polkit();
        }
    }

    pub fn cancel_all_polkit(&mut self) {
        for (_, answer) in self.session.polkit.queue.drain(..) {
            answer.send(false);
        }
        if self.session.polkit.prompt.is_some() {
            self.close_polkit();
        }
    }

    pub fn sync_polkit(&mut self) {
        let caps = self.seat.get_keyboard().is_some_and(|keyboard| keyboard.modifier_state().caps_lock);
        let prompt = self.session.polkit.prompt.as_ref();
        let view = prompt.map(|prompt| prompt.view(caps));
        let motion = prompt.map(Prompt::motion).unwrap_or_default();
        if self.topbar.set_polkit(view, motion) {
            self.queue_redraw();
        }
    }

    #[must_use]
    pub fn polkit_focused(&self) -> bool {
        self.session.polkit.prompt.is_some() && self.topbar.polkit_shown()
    }

    pub fn polkit_key(&mut self, key: Input) {
        let Some(prompt) = self.session.polkit.prompt.as_mut() else {
            return;
        };
        let changed = match key {
            Input::Type(ch) => prompt.type_char(ch),
            Input::Backspace => prompt.backspace(),
            Input::Clear => prompt.clear(),
            Input::Submit => return self.submit_polkit(),
            Input::Escape => match prompt.escape() {
                Step::Close => return self.close_polkit(),
                Step::Changed => true,
                Step::Nothing => false,
            },
        };
        if changed {
            self.sync_polkit();
        }
    }

    pub fn polkit_control(&mut self, control: Control) {
        if control == Control::Submit {
            return self.submit_polkit();
        }
        let Some(prompt) = self.session.polkit.prompt.as_mut() else {
            return;
        };
        match prompt.control(control) {
            Step::Close => self.close_polkit(),
            Step::Changed => self.sync_polkit(),
            Step::Nothing => {}
        }
    }

    fn submit_polkit(&mut self) {
        let Some(reports) = self.session.polkit.reports.clone() else {
            return;
        };
        let Some(prompt) = self.session.polkit.prompt.as_mut() else {
            return;
        };
        let Some((attempt, secret, cancel)) = prompt.submit(Instant::now()) else {
            return;
        };
        let (user, cookie) = (prompt.request.user.clone(), prompt.request.cookie.clone());
        let spawned = std::thread::Builder::new().name("glide-polkit-helper".into()).spawn(move || {
            let report = crate::polkit_helper::authenticate(&user, &cookie, &secret, &cancel);
            drop(secret);
            reports((attempt, report));
        });
        if let Err(err) = spawned {
            tracing::error!(%err, "could not start the polkit helper thread");
            let broken =
                Report { outcome: Outcome::Broken("no helper thread".into()), errors: Vec::new(), notes: Vec::new() };
            prompt.report(attempt, &broken, Instant::now());
        }
        self.sync_polkit();
    }

    pub fn handle_polkit_report(&mut self, (attempt, report): Attempt) {
        let now = Instant::now();
        let Some(prompt) = self.session.polkit.prompt.as_mut() else {
            return;
        };
        if prompt.report(attempt, &report, now) {
            tracing::info!(outcome = ?report.outcome, "polkit authentication finished");
            self.sync_polkit();
        }
    }

    pub fn tick_polkit(&mut self) -> bool {
        let now = Instant::now();
        let Some(prompt) = self.session.polkit.prompt.as_ref() else {
            return false;
        };
        if prompt.finished(now) {
            self.close_polkit();
            return true;
        }
        prompt.is_animating(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Request {
        Request {
            cookie: "c".into(),
            action: "org.freedesktop.timedate1.set-time".into(),
            message: "Authentication is required to set the system time.".into(),
            icon: String::new(),
            user: "artem".into(),
            caller: Some(42),
            program: None,
        }
    }

    fn answered(receiver: oneshot::Receiver<bool>) -> Option<bool> {
        use std::task::{Context, Poll, Waker};
        let mut receiver = std::pin::pin!(receiver);
        match std::future::Future::poll(receiver.as_mut(), &mut Context::from_waker(Waker::noop())) {
            Poll::Ready(answer) => answer,
            Poll::Pending => panic!("not answered"),
        }
    }

    fn report(outcome: Outcome) -> Report {
        Report { outcome, errors: Vec::new(), notes: Vec::new() }
    }

    fn typed(prompt: &mut Prompt, text: &str) {
        for ch in text.chars() {
            assert!(prompt.type_char(ch));
        }
    }

    #[test]
    fn three_wrong_passwords_lock_the_request_and_each_one_clears_the_field() {
        let (sender, receiver) = oneshot::channel();
        let mut prompt = Prompt::new(request(), sender);
        let now = Instant::now();
        assert!(prompt.submit(now).is_none(), "an empty field submits nothing");
        for left in [2, 1] {
            typed(&mut prompt, "nope");
            let (attempt, secret, _) = prompt.submit(now).expect("submitted");
            assert_eq!(secret.as_str(), "nope");
            assert!(!prompt.type_char('x'), "the field is locked while checking");
            assert!(prompt.report(attempt, &report(Outcome::Failure), now));
            assert_eq!(prompt.stage(), Stage::Wrong);
            let view = prompt.view(false);
            assert_eq!((view.bullets, view.hint), (0, Hint::Wrong { left }));
        }
        typed(&mut prompt, "nope");
        let (attempt, _, _) = prompt.submit(now).expect("submitted");
        assert!(!prompt.report(attempt + 1, &report(Outcome::Success), now), "a stale attempt is ignored");
        prompt.report(attempt, &report(Outcome::Failure), now);
        assert_eq!(prompt.stage(), Stage::Locked);
        assert_eq!(prompt.control(Control::Understood), Step::Close);
        drop(prompt);
        assert_eq!(answered(receiver), Some(false));
    }

    #[test]
    fn a_lockout_message_from_pam_locks_at_once_and_success_answers_yes() {
        let (sender, _receiver) = oneshot::channel();
        let mut prompt = Prompt::new(request(), sender);
        let now = Instant::now();
        typed(&mut prompt, "x");
        let (attempt, _, _) = prompt.submit(now).expect("submitted");
        let locked = Report {
            outcome: Outcome::Failure,
            errors: Vec::new(),
            notes: vec!["The account is locked due to 3 failed logins.".into()],
        };
        prompt.report(attempt, &locked, now);
        assert_eq!(prompt.stage(), Stage::Locked);
        assert!(prompt.view(false).note.is_some_and(|note| note.contains("locked")));

        let (sender, receiver) = oneshot::channel();
        let mut prompt = Prompt::new(request(), sender);
        typed(&mut prompt, "glide");
        let (attempt, _, _) = prompt.submit(now).expect("submitted");
        prompt.report(attempt, &report(Outcome::Success), now);
        assert_eq!(answered(receiver), Some(true));
        assert!(!prompt.finished(now) && prompt.finished(now + DONE_HOLD));
    }

    #[test]
    fn escape_closes_the_shield_first_and_the_view_never_prints_the_password() {
        let (sender, _receiver) = oneshot::channel();
        let mut prompt = Prompt::new(request(), sender);
        typed(&mut prompt, "hunter2");
        assert_eq!(prompt.control(Control::Shield), Step::Changed);
        assert_eq!(prompt.escape(), Step::Changed);
        assert_eq!(prompt.escape(), Step::Close);
        assert_eq!(prompt.view(false).shown, None);
        prompt.control(Control::Reveal);
        let view = prompt.view(true);
        assert_eq!(view.shown.as_deref(), Some("hunter2"));
        assert_eq!(view.hint, Hint::CapsLock);
        assert!(!format!("{view:?}").contains("hunter2"));
        assert!(!format!("{prompt:?}").contains("hunter2"));
        assert_eq!(view.program, None);
    }

    #[test]
    fn a_broken_helper_does_not_count_as_a_wrong_password() {
        let (sender, _receiver) = oneshot::channel();
        let mut prompt = Prompt::new(request(), sender);
        typed(&mut prompt, "x");
        let (attempt, _, _) = prompt.submit(Instant::now()).expect("submitted");
        prompt.report(attempt, &report(Outcome::Broken("gone".into())), Instant::now());
        assert_eq!(prompt.view(false).hint, Hint::Broken);
        typed(&mut prompt, "y");
        assert_eq!(prompt.view(false).hint, Hint::Keys);
        assert_eq!(prompt.failures, 0);
    }
}

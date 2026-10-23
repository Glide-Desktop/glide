use std::time::Duration;

use crate::{notifications::LocalEvent, oneshot};

pub const ACCEPT: &str = "accept";
pub const DECLINE: &str = "decline";
const FIRST_ID: u32 = u32::MAX - 1000;
const SLOTS: u32 = 500;
const TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Accept,
    Text(String),
    Decline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub icon: &'static str,
    pub summary: String,
    pub body: String,
    pub accept: Option<String>,
    pub reply: Option<(String, String)>,
    pub secret: bool,
}

impl Prompt {
    #[must_use]
    pub fn notification(&self, id: u32) -> glide_notify::Notification {
        use crate::i18n::{Key, t};
        let accept =
            self.accept.as_ref().map(|label| glide_notify::Action { key: ACCEPT.into(), label: label.clone() });
        let decline = glide_notify::Action { key: DECLINE.into(), label: t(Key::PowerCancel).into() };
        glide_notify::Notification {
            id,
            app_name: "Glide".to_string(),
            app_icon: self.icon.to_string(),
            summary: self.summary.clone(),
            body: self.body.clone(),
            actions: accept.into_iter().chain([decline]).collect(),
            reply: self.reply.as_ref().map(|(label, placeholder)| glide_notify::Reply {
                key: glide_notify::ACTION_INLINE_REPLY.to_string(),
                label: label.clone(),
                placeholder: placeholder.clone(),
                secret: self.secret,
            }),
            urgency: glide_notify::Urgency::Critical,
            resident: true,
            transient: true,
            timeout: Some(TIMEOUT),
            ..glide_notify::Notification::default()
        }
    }
}

#[must_use]
pub fn fill(template: crate::i18n::Key, name: &str, code: &str) -> String {
    crate::i18n::t(template).replace("{device}", name).replace("{code}", code)
}

fn appeared(message: &zbus::Message, service: &str) -> bool {
    let header = message.header();
    let signal = header.message_type() == zbus::message::Type::Signal
        && header.member().is_some_and(|member| member.as_str() == "NameOwnerChanged");
    signal
        && message
            .body()
            .deserialize::<(String, String, String)>()
            .is_ok_and(|(name, _, owner)| name == service && !owner.is_empty())
}

pub fn keep_registered(
    connection: zbus::blocking::Connection, service: &'static str, thread: &str,
    register: fn(&zbus::blocking::Connection) -> zbus::Result<()>,
) -> anyhow::Result<()> {
    let rule = format!(
        "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',\
         arg0='{service}'"
    );
    zbus::blocking::fdo::DBusProxy::new(&connection)?.add_match_rule(zbus::MatchRule::try_from(rule.as_str())?)?;
    std::thread::Builder::new().name(thread.into()).spawn(move || {
        let messages = zbus::blocking::MessageIterator::from(&connection);
        if let Err(err) = register(&connection) {
            tracing::info!(service, %err, "the service is not reachable yet; the agent waits for it");
        }
        for message in messages {
            if message.as_ref().is_ok_and(|message| appeared(message, service))
                && let Err(err) = register(&connection)
            {
                tracing::warn!(service, %err, "could not register the agent again");
            }
        }
    })?;
    Ok(())
}

#[derive(Debug, Default)]
pub struct Prompts {
    pending: Vec<(u32, &'static str, oneshot::Sender<Answer>)>,
    next: u32,
}

impl Prompts {
    pub fn open(&mut self, owner: &'static str, answer: oneshot::Sender<Answer>) -> u32 {
        let id = FIRST_ID + self.next % SLOTS;
        self.next = self.next.wrapping_add(1);
        if let Some(index) = self.pending.iter().position(|(pending, _, _)| *pending == id) {
            self.pending.swap_remove(index);
        }
        self.pending.push((id, owner, answer));
        id
    }

    pub fn resolve(&mut self, event: &LocalEvent) -> Option<u32> {
        let (id, answer) = match event {
            LocalEvent::Action { id, key } if key == ACCEPT => (*id, Answer::Accept),
            LocalEvent::Action { id, .. } | LocalEvent::Closed { id } => (*id, Answer::Decline),
            LocalEvent::Reply { id, text } => (*id, Answer::Text(text.clone())),
        };
        let index = self.pending.iter().position(|(pending, _, _)| *pending == id)?;
        let (id, _, sender) = self.pending.swap_remove(index);
        sender.send(answer);
        Some(id)
    }

    pub fn cancel(&mut self, owner: &str) -> Vec<u32> {
        let (cancelled, kept): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.pending).into_iter().partition(|(_, pending, _)| *pending == owner);
        self.pending = kept;
        cancelled.into_iter().map(|(id, _, _)| id).collect()
    }
}

type Then = Box<dyn FnOnce(&mut crate::GlideState, Option<Answer>)>;

#[derive(Default)]
pub struct Asks(Vec<(oneshot::Receiver<Answer>, Then)>);

fn answered(receiver: &mut oneshot::Receiver<Answer>) -> std::task::Poll<Option<Answer>> {
    use std::task::{Context, Waker};
    std::future::Future::poll(std::pin::Pin::new(receiver), &mut Context::from_waker(Waker::noop()))
}

impl crate::GlideState {
    pub fn ask(
        &mut self, owner: &'static str, prompt: &Prompt, then: impl FnOnce(&mut Self, Option<Answer>) + 'static,
    ) {
        let (sender, receiver) = oneshot::channel();
        self.show_prompt(owner, prompt, sender);
        self.session.asks.0.push((receiver, Box::new(then)));
    }

    fn settle_asks(&mut self) {
        if self.session.asks.0.is_empty() {
            return;
        }
        for (mut receiver, then) in std::mem::take(&mut self.session.asks.0) {
            match answered(&mut receiver) {
                std::task::Poll::Pending => self.session.asks.0.push((receiver, then)),
                std::task::Poll::Ready(answer) => then(self, answer),
            }
        }
    }

    pub fn show_prompt(&mut self, owner: &'static str, prompt: &Prompt, answer: oneshot::Sender<Answer>) {
        let id = self.session.prompts.open(owner, answer);
        self.notifications.push(&mut self.dock.icons, prompt.notification(id), true);
        self.refresh_topbar();
        self.queue_redraw();
    }

    pub fn cancel_prompts(&mut self, owner: &str) {
        for id in self.session.prompts.cancel(owner) {
            self.notifications.dismiss(id);
        }
        self.queue_redraw();
    }

    pub fn tick_local_notifications(&mut self) {
        self.show_glances();
        for event in self.notifications.take_local_events() {
            if let LocalEvent::Action { id: crate::disk_space::NOTICE_ID, key } = &event
                && key == crate::disk_space::OPEN_STORAGE
            {
                crate::system_info::open_storage_settings();
                continue;
            }
            if let LocalEvent::Action { id: crate::autofill::FILL_ID, key } = &event {
                self.autofill_action(key);
                continue;
            }
            if let LocalEvent::Action { id: crate::otp::OFFER_ID, key } = &event
                && key == crate::otp::INSERT
            {
                self.insert_code();
                continue;
            }
            if let LocalEvent::Action { id: crate::screenshot_ui::NOTICE_ID, key } = &event {
                self.open_screenshot(key == crate::screenshot_ui::SHOW);
                continue;
            }
            if let LocalEvent::Action { id: crate::welcome::NOTICE_ID, key } = &event {
                let page =
                    if key == crate::welcome::OPEN_SHORTCUTS { "glide-settings shortcuts" } else { "glide-settings" };
                if let Err(err) = crate::spawn_shell_command(page) {
                    tracing::warn!(%err, "could not open Settings from the welcome");
                }
                self.notifications.dismiss(crate::welcome::NOTICE_ID);
                self.queue_redraw();
                continue;
            }
            if let LocalEvent::Action { id, key } = &event
                && key == crate::removable::OPEN
                && let Some(path) = self.session.volumes.path(*id)
            {
                crate::removable::open(path.to_string());
                continue;
            }
            if self.netbird_local_event(&event) {
                continue;
            }
            if let Some(id) = self.session.prompts.resolve(&event) {
                self.notifications.dismiss(id);
                self.queue_redraw();
            }
        }
        self.settle_shortcut_requests();
        self.settle_permission_requests();
        self.settle_asks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answered(receiver: oneshot::Receiver<Answer>) -> Option<Answer> {
        use std::task::{Context, Poll, Waker};
        let mut receiver = std::pin::pin!(receiver);
        match std::future::Future::poll(receiver.as_mut(), &mut Context::from_waker(Waker::noop())) {
            Poll::Ready(answer) => answer,
            Poll::Pending => panic!("the prompt was not answered"),
        }
    }

    #[test]
    fn each_kind_of_toast_event_answers_its_own_prompt() {
        let mut prompts = Prompts::default();
        let cases = [
            (LocalEvent::Action { id: 0, key: ACCEPT.into() }, Answer::Accept),
            (LocalEvent::Action { id: 0, key: DECLINE.into() }, Answer::Decline),
            (LocalEvent::Reply { id: 0, text: "0000".into() }, Answer::Text("0000".into())),
            (LocalEvent::Closed { id: 0 }, Answer::Decline),
        ];
        for (event, expected) in cases {
            let (sender, receiver) = oneshot::channel();
            let id = prompts.open("bluetooth", sender);
            let event = match event {
                LocalEvent::Action { key, .. } => LocalEvent::Action { id, key },
                LocalEvent::Reply { text, .. } => LocalEvent::Reply { id, text },
                LocalEvent::Closed { .. } => LocalEvent::Closed { id },
            };
            assert_eq!(prompts.resolve(&event), Some(id));
            assert_eq!(answered(receiver), Some(expected));
            assert_eq!(prompts.resolve(&event), None, "a prompt is answered once");
        }
    }

    #[test]
    fn events_for_other_toasts_are_ignored_and_cancelling_drops_only_that_owner() {
        let mut prompts = Prompts::default();
        let (bluetooth, bluetooth_answer) = oneshot::channel();
        let (network, network_answer) = oneshot::channel();
        let first = prompts.open("bluetooth", bluetooth);
        let second = prompts.open("network", network);
        assert_ne!(first, second);
        assert_eq!(prompts.resolve(&LocalEvent::Closed { id: crate::power::SESSION_NOTICE_ID }), None);
        assert_eq!(prompts.cancel("bluetooth"), [first]);
        assert_eq!(answered(bluetooth_answer), None, "a cancelled prompt resolves empty");
        assert_eq!(prompts.resolve(&LocalEvent::Reply { id: second, text: "secret".into() }), Some(second));
        assert_eq!(answered(network_answer), Some(Answer::Text("secret".into())));
    }

    #[test]
    fn prompt_ids_are_local_and_clear_of_the_fixed_notices() {
        let mut prompts = Prompts::default();
        for _ in 0..SLOTS * 2 {
            let (sender, _receiver) = oneshot::channel();
            let id = prompts.open("bluetooth", sender);
            assert!(crate::notifications::is_local(id));
            assert!(id < u32::MAX - 16, "below the screenshot, session, battery and config notices");
        }
        assert!(prompts.pending.len() <= SLOTS as usize, "a reused slot replaces its old prompt");
    }

    #[test]
    fn a_prompt_toast_stays_until_answered_and_offers_cancel() {
        let prompt = Prompt {
            icon: "bluetooth",
            summary: "Pair?".into(),
            body: "Check the code".into(),
            accept: Some("Pair".into()),
            reply: None,
            secret: false,
        };
        let toast = prompt.notification(FIRST_ID);
        let keys: Vec<&str> = toast.actions.iter().map(|action| action.key.as_str()).collect();
        assert_eq!(keys, [ACCEPT, DECLINE]);
        assert!(toast.resident && toast.transient, "a click on the body must not decline");
        assert_eq!(toast.urgency, glide_notify::Urgency::Critical, "do-not-disturb still shows it");
        let text = Prompt { accept: None, reply: Some(("Enter PIN".into(), "0000".into())), ..prompt.clone() };
        let toast = text.notification(FIRST_ID);
        assert_eq!(toast.actions.len(), 1);
        assert_eq!(toast.reply.map(|reply| (reply.label, reply.secret)), Some(("Enter PIN".to_string(), false)));
        let password = Prompt { reply: Some(("Password".into(), String::new())), secret: true, ..prompt };
        assert!(password.notification(FIRST_ID).reply.is_some_and(|reply| reply.secret), "typed text is masked");
    }
}

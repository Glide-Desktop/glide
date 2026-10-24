use std::collections::HashMap;
use std::task::{Context, Poll, Waker};

use smithay::reexports::wayland_server::Resource;
use smithay::wayland::keyboard_shortcuts_inhibit::{KeyboardShortcutsInhibitor, KeyboardShortcutsInhibitorSeat};

use crate::agent_prompt::{Answer, Prompt};
use crate::i18n::{Key, t};
use crate::{GlideState, oneshot};

const OWNER: &str = "shortcuts";
pub const RESTORED_ID: u32 = u32::MAX - 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

#[must_use]
pub const fn decide(unrestricted: bool, remembered: Option<bool>) -> Decision {
    match (unrestricted, remembered) {
        (true, _) | (false, Some(true)) => Decision::Allow,
        (false, Some(false)) => Decision::Deny,
        (false, None) => Decision::Ask,
    }
}

#[derive(Default)]
pub struct ShortcutGuard {
    pending: Vec<(KeyboardShortcutsInhibitor, String, oneshot::Receiver<Answer>)>,
    remembered: HashMap<String, bool>,
}

fn answered(receiver: &mut oneshot::Receiver<Answer>) -> Poll<Option<Answer>> {
    std::future::Future::poll(std::pin::Pin::new(receiver), &mut Context::from_waker(Waker::noop()))
}

impl GlideState {
    pub fn request_shortcut_inhibit(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        let surface = inhibitor.wl_surface().clone();
        let unrestricted = surface.client().is_none_or(|client| crate::state::client_is_unrestricted(&client));
        let (title, app_id) = crate::state::toplevel_title_and_app_id(&surface);
        let app = app_id.or(title).unwrap_or_default();
        match decide(unrestricted, self.session.shortcuts.remembered.get(&app).copied()) {
            Decision::Allow => {
                inhibitor.activate();
                tracing::info!(app, "a client took over the keyboard shortcuts");
            }
            Decision::Deny => tracing::info!(app, "a sandboxed client was refused the keyboard shortcuts again"),
            Decision::Ask => {
                let name = if app.is_empty() { t(Key::SomeApp).to_string() } else { app.clone() };
                let prompt = Prompt {
                    icon: "input-keyboard".into(),
                    summary: t(Key::ShortcutsAsk).replace("{app}", &name),
                    body: t(Key::ShortcutsAskBody).to_string(),
                    accept: Some(t(Key::ShortcutsAllow).to_string()),
                    reply: None,
                    secret: false,
                };
                let (sender, receiver) = oneshot::channel();
                self.show_prompt(OWNER, &prompt, sender);
                self.session.shortcuts.pending.push((inhibitor, app, receiver));
            }
        }
    }

    pub fn settle_shortcut_requests(&mut self) {
        if self.session.shortcuts.pending.is_empty() {
            return;
        }
        for (inhibitor, app, mut receiver) in std::mem::take(&mut self.session.shortcuts.pending) {
            match answered(&mut receiver) {
                Poll::Pending if inhibitor.wl_surface().is_alive() => {
                    self.session.shortcuts.pending.push((inhibitor, app, receiver));
                }
                Poll::Pending => {}
                Poll::Ready(answer) => {
                    let allowed = answer == Some(Answer::Accept);
                    tracing::info!(app, allowed, "keyboard shortcut request answered");
                    self.session.shortcuts.remembered.insert(app, allowed);
                    if allowed && inhibitor.wl_surface().is_alive() {
                        inhibitor.activate();
                    }
                }
            }
        }
    }

    pub fn restore_shortcuts(&mut self) {
        let Some(surface) = self.seat.get_keyboard().and_then(|keyboard| keyboard.current_focus()) else {
            return;
        };
        let Some(inhibitor) = self.seat.keyboard_shortcuts_inhibitor_for_surface(&surface) else {
            return;
        };
        inhibitor.inactivate();
        tracing::info!("keyboard shortcuts taken back with Super+Esc");
        let notice = glide_notify::Notification {
            id: RESTORED_ID,
            app_name: "Glide".to_string(),
            app_icon: "input-keyboard".to_string(),
            summary: t(Key::ShortcutsRestored).to_string(),
            transient: true,
            ..glide_notify::Notification::default()
        };
        self.notifications.push(&mut self.dock.icons, notice, true);
        self.queue_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_sandboxed_clients_are_asked_and_their_answer_is_remembered() {
        assert_eq!(decide(true, None), Decision::Allow);
        assert_eq!(decide(true, Some(false)), Decision::Allow, "a remembered refusal never limits a host app");
        assert_eq!(decide(false, None), Decision::Ask);
        assert_eq!(decide(false, Some(true)), Decision::Allow);
        assert_eq!(decide(false, Some(false)), Decision::Deny);
    }

    #[test]
    fn an_unanswered_prompt_stays_pending_and_an_answer_is_read_once() {
        let (sender, mut receiver) = oneshot::channel();
        assert!(answered(&mut receiver).is_pending());
        sender.send(Answer::Accept);
        assert_eq!(answered(&mut receiver), Poll::Ready(Some(Answer::Accept)));
    }

    #[test]
    fn the_restored_notice_id_stays_clear_of_the_other_local_ids() {
        assert!(crate::notifications::is_local(RESTORED_ID));
        for taken in [u32::MAX - 1, u32::MAX - 2, u32::MAX - 3, u32::MAX - 4, u32::MAX - 5, u32::MAX - 6] {
            assert_ne!(RESTORED_ID, taken);
        }
    }
}

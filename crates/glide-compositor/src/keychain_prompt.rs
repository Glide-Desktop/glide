use std::time::Instant;

use glide_ipc::keychain::{Hidden, VaultKind};
use zeroize::Zeroizing;

use crate::i18n::{Key, t};
use crate::keychain::Chain;
use crate::state::GlideState;

const MAX_PASSWORD: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Control {
    Close,
    Field,
    Reveal,
    Submit,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Type(char),
    Backspace,
    Clear,
    Submit,
    Skip,
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Chip {
    Done,
    Current,
    Next,
    Later,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Hint {
    Keys { skip: Option<String> },
    Wrong { master: bool, caps: bool },
    CapsLock,
    Checking,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct View {
    pub app: Option<String>,
    pub chips: Vec<(String, Chip)>,
    pub body: String,
    pub user: String,
    pub master: bool,
    pub bullets: usize,
    pub shown: Option<String>,
    pub hint: Hint,
    pub ready: bool,
    pub checking: bool,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View")
            .field("chips", &self.chips)
            .field("hint", &self.hint)
            .field("checking", &self.checking)
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
pub struct Prompt {
    password: Zeroizing<String>,
    reveal: bool,
    checking: Option<Instant>,
    shaken: Option<Instant>,
}

impl std::fmt::Debug for Prompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prompt").field("checking", &self.checking.is_some()).finish_non_exhaustive()
    }
}

impl Prompt {
    const fn editable(&self) -> bool {
        self.checking.is_none()
    }

    pub fn type_char(&mut self, ch: char) -> bool {
        if !self.editable() || ch.is_control() || self.password.len() + ch.len_utf8() > MAX_PASSWORD {
            return false;
        }
        self.password.push(ch);
        true
    }

    pub fn backspace(&mut self) -> bool {
        self.editable() && self.password.pop().is_some()
    }

    pub fn clear(&mut self) -> bool {
        if !self.editable() || self.password.is_empty() {
            return false;
        }
        self.password.clear();
        true
    }

    fn take(&mut self, now: Instant) -> Option<Hidden> {
        if !self.editable() || self.password.is_empty() {
            return None;
        }
        self.checking = Some(now);
        let secret = Hidden::new(self.password.to_string());
        self.password.clear();
        Some(secret)
    }

    fn answered(&mut self, wrong: bool, now: Instant) {
        self.checking = None;
        self.password.clear();
        if wrong {
            self.shaken = Some(now);
        }
    }

    #[must_use]
    pub const fn motion(&self) -> crate::polkit::Motion {
        crate::polkit::Motion { shaken: self.shaken, checking: self.checking }
    }

    #[must_use]
    pub fn is_animating(&self, now: Instant) -> bool {
        self.checking.is_some()
            || self.shaken.is_some_and(|at| now.saturating_duration_since(at) < glide_theme::motion::t3())
    }

    #[must_use]
    pub fn view(&self, chain: &Chain, user: &str, caps_lock: bool) -> Option<View> {
        let current = chain.current()?;
        let master = current.kind == VaultKind::Bitwarden;
        let chips = if chain.vaults.len() + chain.unlocked.len() > 1 { chips(chain) } else { Vec::new() };
        let body = if master {
            t(Key::KcBitwarden).replace("{account}", &current.account)
        } else {
            t(Key::KcLocal).replace("{name}", &current.name)
        };
        let hint = if self.checking.is_some() {
            Hint::Checking
        } else if chain.wrong {
            Hint::Wrong { master, caps: caps_lock }
        } else if caps_lock {
            Hint::CapsLock
        } else {
            let more = chain.index + 1 < chain.vaults.len();
            Hint::Keys { skip: more.then(|| current.name.clone()) }
        };
        Some(View {
            app: chain.requester.clone(),
            chips,
            body,
            user: user.to_string(),
            master,
            bullets: self.password.chars().count(),
            shown: self.reveal.then(|| self.password.to_string()),
            hint,
            ready: self.editable() && !self.password.is_empty(),
            checking: self.checking.is_some(),
        })
    }
}

fn chips(chain: &Chain) -> Vec<(String, Chip)> {
    chain
        .vaults
        .iter()
        .enumerate()
        .map(|(index, vault)| {
            let state = if chain.unlocked.contains(&vault.id) {
                Chip::Done
            } else if index == chain.index {
                Chip::Current
            } else if index == chain.index + 1 {
                Chip::Next
            } else {
                Chip::Later
            };
            let label =
                if state == Chip::Next { t(Key::KcNext).replace("{name}", &vault.name) } else { vault.name.clone() };
            (label, state)
        })
        .collect()
}

impl GlideState {
    pub(crate) fn open_keychain_prompt(&mut self) {
        let now = Instant::now();
        let wrong = self.unlock_chain().is_some_and(|chain| chain.wrong);
        if let Some(prompt) = self.session.keychain.prompt.as_mut() {
            prompt.answered(wrong, now);
        } else {
            if self.switcher_open() {
                self.cancel_switcher();
            }
            self.cancel_power_dialog();
            self.power_dialog = None;
            if self.shot_active() {
                self.cancel_shot();
            }
            self.session.keychain.prompt = Some(Prompt::default());
        }
        self.sync_keychain_prompt();
    }

    pub(crate) fn close_keychain_prompt(&mut self) {
        if self.session.keychain.prompt.take().is_some() {
            self.sync_keychain_prompt();
        }
    }

    pub fn sync_keychain_prompt(&mut self) {
        let caps = self.seat.get_keyboard().is_some_and(|keyboard| keyboard.modifier_state().caps_lock);
        let login = crate::greeter::current_user(&self.topbar.system_status().user_name);
        let user = Some(login.display_name).filter(|name| !name.trim().is_empty()).unwrap_or(login.username);
        let (view, motion) = match (self.session.keychain.prompt.as_ref(), self.unlock_chain()) {
            (Some(prompt), Some(chain)) => (prompt.view(chain, &user, caps), prompt.motion()),
            _ => (None, crate::polkit::Motion::default()),
        };
        if self.topbar.set_keychain(view, motion) {
            self.queue_redraw();
        }
    }

    #[must_use]
    pub fn keychain_focused(&self) -> bool {
        self.session.keychain.prompt.is_some() && self.topbar.keychain_shown()
    }

    pub fn keychain_key(&mut self, key: Input) {
        let Some(prompt) = self.session.keychain.prompt.as_mut() else {
            return;
        };
        let changed = match key {
            Input::Type(ch) => prompt.type_char(ch),
            Input::Backspace => prompt.backspace(),
            Input::Clear => prompt.clear(),
            Input::Submit => return self.submit_keychain_prompt(),
            Input::Skip => {
                if prompt.editable() {
                    self.skip_vault();
                }
                return;
            }
            Input::Escape => return self.cancel_keychain_unlock(),
        };
        if changed {
            self.sync_keychain_prompt();
        }
    }

    pub fn keychain_control(&mut self, control: Control) {
        match control {
            Control::Submit => self.submit_keychain_prompt(),
            Control::Close | Control::Cancel => self.cancel_keychain_unlock(),
            Control::Reveal => {
                if let Some(prompt) = self.session.keychain.prompt.as_mut() {
                    prompt.reveal = !prompt.reveal;
                    self.sync_keychain_prompt();
                }
            }
            Control::Field => {}
        }
    }

    fn submit_keychain_prompt(&mut self) {
        let Some(secret) = self.session.keychain.prompt.as_mut().and_then(|prompt| prompt.take(Instant::now())) else {
            return;
        };
        self.sync_keychain_prompt();
        self.submit_vault_password(secret);
    }

    pub fn tick_keychain_prompt(&mut self) -> bool {
        self.session.keychain.prompt.as_ref().is_some_and(|prompt| prompt.is_animating(Instant::now()))
    }
}

#[cfg(test)]
mod tests {
    use glide_ipc::keychain::{SyncState, Vault};

    use super::*;

    fn vault(id: &str, name: &str, kind: VaultKind) -> Vault {
        Vault {
            id: id.into(),
            name: name.into(),
            kind,
            account: "artem@example.com".into(),
            server: String::new(),
            exists: true,
            unlocked: false,
            sync: SyncState::Local,
            count: 0,
        }
    }

    fn chain() -> Chain {
        Chain {
            vaults: vec![
                vault("local", "Особисте", VaultKind::Local),
                vault("bw-1", "Bitwarden", VaultKind::Bitwarden),
                vault("bw-2", "Bitwarden · дім", VaultKind::Bitwarden),
            ],
            index: 1,
            wrong: false,
            unlocked: vec!["local".into()],
            requester: None,
        }
    }

    #[test]
    fn a_program_that_asked_is_named_in_the_prompt() {
        let asked = Chain { requester: Some("Google Chrome".into()), ..chain() };
        assert_eq!(
            Prompt::default().view(&asked, "artem", false).and_then(|view| view.app).as_deref(),
            Some("Google Chrome")
        );
        assert_eq!(Prompt::default().view(&chain(), "artem", false).and_then(|view| view.app), None);
    }

    #[test]
    fn the_chips_show_the_done_current_and_next_vaults() {
        let view = Prompt::default().view(&chain(), "artem", false).expect("a current vault");
        let states: Vec<Chip> = view.chips.iter().map(|(_, state)| *state).collect();
        assert_eq!(states, [Chip::Done, Chip::Current, Chip::Next]);
        assert!(view.chips[2].0.starts_with("Bitwarden · дім"));
        assert!(view.body.contains("artem@example.com"));
        assert_eq!(view.hint, Hint::Keys { skip: Some("Bitwarden".into()) });
    }

    #[test]
    fn a_single_vault_has_no_chips_and_nothing_to_skip() {
        let single = Chain { vaults: vec![vault("local", "Особисте", VaultKind::Local)], ..Chain::default() };
        let view = Prompt::default().view(&single, "artem", false).expect("a current vault");
        assert!(view.chips.is_empty());
        assert_eq!(view.hint, Hint::Keys { skip: None });
        assert!(view.body.contains("Особисте"));
    }

    #[test]
    fn a_wrong_password_says_so_and_mentions_caps_lock() {
        let wrong = Chain { wrong: true, ..chain() };
        let view = Prompt::default().view(&wrong, "artem", true).expect("a current vault");
        assert_eq!(view.hint, Hint::Wrong { master: true, caps: true });
    }

    #[test]
    fn submitting_hands_over_the_password_and_locks_the_field_until_the_answer() {
        let now = Instant::now();
        let mut prompt = Prompt::default();
        assert!(prompt.take(now).is_none(), "nothing to submit while empty");
        for ch in "hunter2".chars() {
            assert!(prompt.type_char(ch));
        }
        let view = prompt.view(&chain(), "artem", false).expect("view");
        assert!(view.ready && view.shown.is_none() && view.bullets == 7);
        let secret = prompt.take(now).expect("a password");
        assert_eq!(secret.expose(), "hunter2");
        assert!(!prompt.type_char('x'), "the field is closed while checking");
        assert_eq!(prompt.view(&chain(), "artem", false).expect("view").hint, Hint::Checking);
        prompt.answered(true, now);
        assert!(prompt.type_char('x') && prompt.motion().shaken.is_some());
    }

    #[test]
    fn the_password_never_reaches_debug_output() {
        let mut prompt = Prompt { reveal: true, ..Prompt::default() };
        for ch in "hunter2".chars() {
            prompt.type_char(ch);
        }
        let view = prompt.view(&chain(), "artem", false).expect("view");
        assert_eq!(view.shown.as_deref(), Some("hunter2"));
        assert!(!format!("{prompt:?}{view:?}").contains("hunter2"));
    }
}

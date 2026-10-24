use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError, RwLock};
use std::task::{Context, Poll, Waker};

use glide_config::permissions::{Access, Permissions};
use smithay::reexports::wayland_server::Client;

use crate::agent_prompt::{Answer, Prompt};
use crate::i18n::{Key, t};
use crate::identity::Attested;
use crate::{GlideState, oneshot};

const OWNER: &str = "permissions";

const GLIDE_PROGRAMS: [&str; 8] = [
    "glide",
    "glide-portal",
    "glide-files",
    "glide-terminal",
    "glide-text",
    "glide-settings",
    "glide-catalog",
    "glide-keychain",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

#[must_use]
pub fn decide(
    access: Access, attested: Option<&Attested>, permissions: &Permissions, own_dir: Option<&Path>,
) -> Decision {
    let Some(Attested::Program(program)) = attested else {
        return Decision::Deny;
    };
    if trusted(access, program, own_dir) {
        return Decision::Allow;
    }
    if program.to_str().is_none() {
        return Decision::Deny;
    }
    match permissions.remembered(access, &Attested::Program(program.clone()).key()) {
        Some(true) => Decision::Allow,
        Some(false) => Decision::Deny,
        None => Decision::Ask,
    }
}

fn trusted(access: Access, program: &Path, own_dir: Option<&Path>) -> bool {
    let name = program.file_name().and_then(std::ffi::OsStr::to_str).unwrap_or_default();
    if own_dir.is_some_and(|dir| program.parent() == Some(dir)) && GLIDE_PROGRAMS.contains(&name) {
        return true;
    }
    if !program.starts_with("/usr/") {
        return false;
    }
    match access {
        Access::Input => {
            matches!(name, "fcitx5" | "ibus-daemon" | "ibus-ui-gtk3" | "kime" | "maliit-server")
                || name.starts_with("ibus-engine-")
        }
        Access::Screen => name == "xdg-desktop-portal-wlr",
        Access::Clipboard => false,
    }
}

#[derive(Debug, Default)]
pub struct Gate {
    permissions: RwLock<Permissions>,
    own_dir: Option<PathBuf>,
    asked: Mutex<HashSet<(Access, String)>>,
    asks: Mutex<Vec<(Access, String)>>,
}

impl Gate {
    #[must_use]
    pub fn new(permissions: Permissions, own_dir: Option<PathBuf>) -> Self {
        Self { permissions: RwLock::new(permissions), own_dir, ..Self::default() }
    }

    #[must_use]
    pub fn load() -> Self {
        let permissions = match Permissions::path().map(|path| Permissions::load_from(&path)) {
            Some(Ok(permissions)) => permissions,
            Some(Err(err)) => {
                tracing::warn!(%err, "permissions.toml is unreadable; every program is asked again");
                Permissions::default()
            }
            None => Permissions::default(),
        };
        let own_dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
        Self::new(permissions, own_dir)
    }

    pub fn permits(&self, access: Access, client: &Client) -> bool {
        let Some(state) = client.get_data::<crate::state::ClientState>() else {
            return false;
        };
        let attested = match &state.security_context {
            Some(context) => Some(Attested::Sandboxed {
                engine: context.sandbox_engine.clone().unwrap_or_default(),
                app_id: context.app_id.clone().unwrap_or_default(),
            }),
            None => state.identity.attested(),
        };
        self.decide_and_ask(access, attested.as_ref())
    }

    pub fn decide_and_ask(&self, access: Access, attested: Option<&Attested>) -> bool {
        let decision = {
            let permissions = self.permissions.read().unwrap_or_else(PoisonError::into_inner);
            decide(access, attested, &permissions, self.own_dir.as_deref())
        };
        match decision {
            Decision::Allow => true,
            Decision::Deny => false,
            Decision::Ask => {
                if let Some(attested) = attested {
                    self.ask(access, &attested.key());
                }
                false
            }
        }
    }

    fn ask(&self, access: Access, program: &str) {
        let key = (access, program.to_string());
        if self.asked.lock().unwrap_or_else(PoisonError::into_inner).insert(key.clone()) {
            self.asks.lock().unwrap_or_else(PoisonError::into_inner).push(key);
        }
    }

    fn take_asks(&self) -> Vec<(Access, String)> {
        std::mem::take(&mut *self.asks.lock().unwrap_or_else(PoisonError::into_inner))
    }

    fn allow(&self, access: Access, program: &str) -> Permissions {
        let mut permissions = self.permissions.write().unwrap_or_else(PoisonError::into_inner);
        permissions.remember(access, program, true);
        permissions.clone()
    }

    pub fn reload(&self) {
        let fresh = Self::load();
        *self.permissions.write().unwrap_or_else(PoisonError::into_inner) =
            fresh.permissions.into_inner().unwrap_or_else(PoisonError::into_inner);
        self.asked.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }
}

pub struct Request {
    program: String,
    accesses: Vec<Access>,
    id: u32,
    answer: oneshot::Receiver<Answer>,
}

pub type Pending = Vec<Request>;

fn answered(receiver: &mut oneshot::Receiver<Answer>) -> Poll<Option<Answer>> {
    std::future::Future::poll(std::pin::Pin::new(receiver), &mut Context::from_waker(Waker::noop()))
}

fn ask_key(access: Access) -> Key {
    match access {
        Access::Clipboard => Key::PermissionClipboard,
        Access::Input => Key::PermissionInput,
        Access::Screen => Key::PermissionScreen,
    }
}

const fn access_key(access: Access) -> Key {
    match access {
        Access::Clipboard => Key::AccessClipboard,
        Access::Input => Key::AccessInput,
        Access::Screen => Key::AccessScreen,
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

fn prompt_for(program: &str, accesses: &[Access], presented: crate::identity::Presented) -> Prompt {
    let name = presented.name;
    let icon = presented.icon.unwrap_or_else(|| "security-high".to_string());
    let advice = t(Key::PermissionBody).replace("{path}", program);
    let (summary, body) = if let [access] = accesses {
        (t(ask_key(*access)).replace("{app}", &name), advice)
    } else {
        let list: Vec<&str> = accesses.iter().map(|access| t(access_key(*access))).collect();
        (t(Key::PermissionMany).replace("{app}", &name), format!("{}\n{advice}", capitalised(&list.join(" · "))))
    };
    Prompt { icon, summary, body, accept: Some(t(Key::PermissionAllow).to_string()), reply: None, secret: false }
}

fn save(permissions: Permissions) {
    let Some(path) = Permissions::path() else {
        return;
    };
    let spawned = std::thread::Builder::new().name("glide-permissions".into()).spawn(move || {
        if let Err(err) = permissions.save_to(&path) {
            tracing::warn!(%err, "could not save permissions.toml; the answer lasts until logout");
        }
    });
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not start the permissions writer");
    }
}

impl GlideState {
    pub fn settle_permission_requests(&mut self) {
        for (access, program) in self.session.gate.take_asks() {
            tracing::info!(?access, program, "asking whether a program may use a privileged protocol");
            let known = self.session.permission_prompts.iter().position(|request| request.program == program);
            let accesses = match known {
                Some(index) => {
                    let request = &mut self.session.permission_prompts[index];
                    if !request.accesses.contains(&access) {
                        request.accesses.push(access);
                    }
                    request.accesses.clone()
                }
                None => vec![access],
            };
            let attested = crate::identity::Attested::Program(PathBuf::from(&program));
            let presented = crate::identity::present(&attested, self.app_directory.index());
            let prompt = prompt_for(&program, &accesses, presented);
            if let Some(index) = known {
                let id = self.session.permission_prompts[index].id;
                self.update_prompt(id, &prompt);
            } else {
                let (sender, answer) = oneshot::channel();
                let id = self.show_prompt(OWNER, &prompt, sender);
                self.session.permission_prompts.push(Request { program, accesses, id, answer });
            }
        }
        if self.session.permission_prompts.is_empty() {
            return;
        }
        for mut request in std::mem::take(&mut self.session.permission_prompts) {
            let (accesses, program) = (&request.accesses, &request.program);
            match answered(&mut request.answer) {
                Poll::Pending => self.session.permission_prompts.push(request),
                Poll::Ready(Some(Answer::Accept)) => {
                    tracing::info!(?accesses, program, "privileged protocols allowed");
                    let mut saved = None;
                    for access in accesses {
                        saved = Some(self.session.gate.allow(*access, program));
                    }
                    if let Some(permissions) = saved {
                        save(permissions);
                    }
                }
                Poll::Ready(_) => tracing::info!(?accesses, program, "privileged protocols refused for this session"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN: &str = "/opt/glide/bin";

    fn program(path: &str) -> Attested {
        Attested::Program(PathBuf::from(path))
    }

    fn decide_for(access: Access, path: &str, permissions: &Permissions) -> Decision {
        decide(access, Some(&program(path)), permissions, Some(Path::new(OWN)))
    }

    #[test]
    fn a_sandboxed_or_unidentified_client_is_refused_without_asking() {
        let mut permissions = Permissions::default();
        let flatpak = Attested::Sandboxed { engine: "flatpak".into(), app_id: "org.example.Paste".into() };
        permissions.remember(Access::Clipboard, &flatpak.key(), true);
        assert_eq!(decide(Access::Clipboard, Some(&flatpak), &permissions, None), Decision::Deny);
        assert_eq!(decide(Access::Clipboard, None, &permissions, None), Decision::Deny);
    }

    #[test]
    fn glide_programs_are_trusted_only_beside_the_compositor() {
        let permissions = Permissions::default();
        assert_eq!(decide_for(Access::Screen, "/opt/glide/bin/glide-portal", &permissions), Decision::Allow);
        assert_eq!(decide_for(Access::Screen, "/opt/glide/bin/wl-paste", &permissions), Decision::Ask);
        assert_eq!(decide_for(Access::Screen, "/home/me/glide-portal", &permissions), Decision::Ask);
        assert_eq!(
            decide_for(Access::Screen, "/opt/glide/bin/glidectl", &permissions),
            Decision::Ask,
            "any script can run glidectl, so it is no more trusted than its caller"
        );
    }

    #[test]
    fn input_methods_are_trusted_for_input_only_and_only_from_system_folders() {
        let permissions = Permissions::default();
        assert_eq!(decide_for(Access::Input, "/usr/bin/fcitx5", &permissions), Decision::Allow);
        assert_eq!(decide_for(Access::Input, "/usr/lib/ibus/ibus-engine-simple", &permissions), Decision::Allow);
        assert_eq!(decide_for(Access::Clipboard, "/usr/bin/fcitx5", &permissions), Decision::Ask);
        assert_eq!(decide_for(Access::Input, "/home/me/.local/bin/fcitx5", &permissions), Decision::Ask);
    }

    #[test]
    fn a_remembered_answer_decides_and_an_unknown_program_is_asked() {
        let mut permissions = Permissions::default();
        permissions.remember(Access::Clipboard, "/usr/bin/cliphist", true);
        permissions.remember(Access::Input, "/usr/bin/wtype", false);
        assert_eq!(decide_for(Access::Clipboard, "/usr/bin/cliphist", &permissions), Decision::Allow);
        assert_eq!(decide_for(Access::Input, "/usr/bin/wtype", &permissions), Decision::Deny);
        assert_eq!(decide_for(Access::Clipboard, "/usr/bin/wl-paste", &permissions), Decision::Ask);
    }

    #[test]
    fn a_program_is_asked_once_per_session_however_often_it_binds() {
        let gate = Gate::new(Permissions::default(), None);
        let paste = program("/usr/bin/wl-paste");
        for _ in 0..3 {
            assert!(!gate.decide_and_ask(Access::Clipboard, Some(&paste)));
        }
        assert_eq!(gate.take_asks(), vec![(Access::Clipboard, "/usr/bin/wl-paste".to_string())]);
        assert!(!gate.decide_and_ask(Access::Clipboard, Some(&paste)));
        assert!(gate.take_asks().is_empty());
    }

    #[test]
    fn one_prompt_names_every_access_a_program_asked_for() {
        let program = "/usr/lib/signal-desktop/signal-desktop";
        let presented = crate::identity::present(&crate::identity::Attested::Program(program.into()), None);
        let single = prompt_for(program, &[Access::Screen], presented.clone());
        assert_eq!(single.summary, t(Key::PermissionScreen).replace("{app}", "signal-desktop"));
        assert_eq!(single.icon, "security-high", "the shield stands in for a program without an entry");
        assert!(single.body.ends_with(program), "the path stays the last line");

        let all = prompt_for(program, &[Access::Screen, Access::Clipboard, Access::Input], presented);
        assert_eq!(all.summary, t(Key::PermissionMany).replace("{app}", "signal-desktop"));
        let list = all.body.lines().next().unwrap_or_default().to_lowercase();
        for key in [Key::AccessScreen, Key::AccessClipboard, Key::AccessInput] {
            assert!(list.contains(t(key)), "{list:?} names {key:?}");
        }
        assert!(all.body.ends_with(program));
        assert_eq!(all.accept.as_deref(), Some(t(Key::PermissionAllow)));
        assert_eq!(capitalised("буфер"), "Буфер");
    }

    #[test]
    fn allowing_a_program_lets_its_next_connection_in() {
        let gate = Gate::new(Permissions::default(), None);
        let grim = program("/usr/bin/grim");
        assert!(!gate.decide_and_ask(Access::Screen, Some(&grim)));
        let saved = gate.allow(Access::Screen, "/usr/bin/grim");
        assert_eq!(saved.remembered(Access::Screen, "/usr/bin/grim"), Some(true));
        assert!(gate.decide_and_ask(Access::Screen, Some(&grim)));
    }
}

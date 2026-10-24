use std::collections::HashSet;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError, RwLock};
use std::task::{Context, Poll, Waker};

use glide_config::permissions::{Access, Permissions};
use smithay::reexports::wayland_server::Client;

use crate::agent_prompt::{Answer, Prompt};
use crate::i18n::{Key, t};
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    pub program: Option<PathBuf>,
}

impl Identity {
    #[must_use]
    pub fn of_stream(stream: &UnixStream) -> Self {
        Self { program: peer_pid(stream).and_then(program_of) }
    }
}

#[must_use]
pub fn peer_pid(stream: &UnixStream) -> Option<i32> {
    use std::os::fd::AsRawFd;
    let mut credentials = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = libc::socklen_t::try_from(std::mem::size_of::<libc::ucred>()).ok()?;
    // SAFETY: `credentials` and `len` are live locals sized for SO_PEERCRED, and the fd belongs to `stream`.
    let status = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut credentials).cast(),
            &raw mut len,
        )
    };
    (status == 0 && credentials.pid > 0).then_some(credentials.pid)
}

#[must_use]
pub fn program_of(pid: i32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok().filter(|path| path.is_absolute())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

#[must_use]
pub fn decide(
    access: Access, sandboxed: bool, program: Option<&Path>, permissions: &Permissions, own_dir: Option<&Path>,
) -> Decision {
    if sandboxed {
        return Decision::Deny;
    }
    let Some(program) = program else {
        return Decision::Deny;
    };
    if trusted(access, program, own_dir) {
        return Decision::Allow;
    }
    let Some(name) = program.to_str() else {
        return Decision::Deny;
    };
    match permissions.remembered(access, name) {
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
        self.decide_and_ask(access, state.security_context.is_some(), state.identity.program.as_deref())
    }

    pub fn decide_and_ask(&self, access: Access, sandboxed: bool, program: Option<&Path>) -> bool {
        let decision = {
            let permissions = self.permissions.read().unwrap_or_else(PoisonError::into_inner);
            decide(access, sandboxed, program, &permissions, self.own_dir.as_deref())
        };
        match decision {
            Decision::Allow => true,
            Decision::Deny => false,
            Decision::Ask => {
                if let Some(program) = program.and_then(Path::to_str) {
                    self.ask(access, program);
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

fn prompt_for(program: &str, accesses: &[Access], app: Option<&crate::icon::DesktopApp>) -> Prompt {
    let (name, icon) = presented(program, app);
    let advice = t(Key::PermissionBody).replace("{path}", program);
    let (summary, body) = if let [access] = accesses {
        (t(ask_key(*access)).replace("{app}", &name), advice)
    } else {
        let list: Vec<&str> = accesses.iter().map(|access| t(access_key(*access))).collect();
        (t(Key::PermissionMany).replace("{app}", &name), format!("{}\n{advice}", capitalised(&list.join(" · "))))
    };
    Prompt { icon, summary, body, accept: Some(t(Key::PermissionAllow).to_string()), reply: None, secret: false }
}

fn presented(program: &str, app: Option<&crate::icon::DesktopApp>) -> (String, String) {
    let binary = Path::new(program).file_name().and_then(std::ffi::OsStr::to_str).unwrap_or(program);
    let name = app.map(|app| app.name.trim()).filter(|name| !name.is_empty()).unwrap_or(binary);
    let icon = app.map(|app| app.icon.trim()).filter(|icon| !icon.is_empty()).unwrap_or("security-high");
    (name.to_string(), icon.to_string())
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
            let app = self.app_directory.index().and_then(|index| index.match_process(Some(&program), None));
            let prompt = prompt_for(&program, &accesses, app);
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

    fn decide_for(access: Access, program: &str, permissions: &Permissions) -> Decision {
        decide(access, false, Some(Path::new(program)), permissions, Some(Path::new(OWN)))
    }

    #[test]
    fn a_sandboxed_or_unidentified_client_is_refused_without_asking() {
        let permissions = Permissions::default();
        assert_eq!(
            decide(Access::Clipboard, true, Some(Path::new("/usr/bin/wl-paste")), &permissions, None),
            Decision::Deny
        );
        assert_eq!(decide(Access::Clipboard, false, None, &permissions, None), Decision::Deny);
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
        let program = Path::new("/usr/bin/wl-paste");
        for _ in 0..3 {
            assert!(!gate.decide_and_ask(Access::Clipboard, false, Some(program)));
        }
        assert_eq!(gate.take_asks(), vec![(Access::Clipboard, "/usr/bin/wl-paste".to_string())]);
        assert!(!gate.decide_and_ask(Access::Clipboard, false, Some(program)));
        assert!(gate.take_asks().is_empty());
    }

    #[test]
    fn a_prompt_names_the_desktop_entry_and_falls_back_to_the_binary_name() {
        let program = "/usr/lib/signal-desktop/signal-desktop";
        let signal = crate::icon::DesktopApp {
            id: "signal".into(),
            name: "Signal".into(),
            icon: "signal-desktop".into(),
            ..crate::icon::DesktopApp::default()
        };
        assert_eq!(presented(program, Some(&signal)), ("Signal".into(), "signal-desktop".into()));
        assert_eq!(presented(program, None), ("signal-desktop".into(), "security-high".into()));
        let nameless = crate::icon::DesktopApp { name: " ".into(), icon: String::new(), ..signal };
        assert_eq!(presented(program, Some(&nameless)), ("signal-desktop".into(), "security-high".into()));
    }

    #[test]
    fn one_prompt_names_every_access_a_program_asked_for() {
        let program = "/usr/lib/signal-desktop/signal-desktop";
        let single = prompt_for(program, &[Access::Screen], None);
        assert_eq!(single.summary, t(Key::PermissionScreen).replace("{app}", "signal-desktop"));
        assert!(single.body.ends_with(program), "the path stays the last line");

        let all = prompt_for(program, &[Access::Screen, Access::Clipboard, Access::Input], None);
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
        let program = Path::new("/usr/bin/grim");
        assert!(!gate.decide_and_ask(Access::Screen, false, Some(program)));
        let saved = gate.allow(Access::Screen, "/usr/bin/grim");
        assert_eq!(saved.remembered(Access::Screen, "/usr/bin/grim"), Some(true));
        assert!(gate.decide_and_ask(Access::Screen, false, Some(program)));
    }
}

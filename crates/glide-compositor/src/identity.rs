use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use crate::app_index::AppIndex;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Attested {
    Program(PathBuf),
    Sandboxed { engine: String, app_id: String },
}

impl Attested {
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Self::Program(program) => program.display().to_string(),
            Self::Sandboxed { engine, app_id } => format!("{engine}:{app_id}"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    pub program: Option<PathBuf>,
    pub flatpak: Option<String>,
}

impl Identity {
    #[must_use]
    pub fn of_stream(stream: &UnixStream) -> Self {
        peer_pid(stream).map(Self::of_pid).unwrap_or_default()
    }

    #[must_use]
    pub fn of_pid(pid: i32) -> Self {
        let flatpak =
            std::fs::read_to_string(format!("/proc/{pid}/root/.flatpak-info")).ok().and_then(|info| flatpak_app(&info));
        Self { program: program_of(pid), flatpak }
    }

    #[must_use]
    pub fn attested(&self) -> Option<Attested> {
        if let Some(app_id) = &self.flatpak {
            return Some(Attested::Sandboxed { engine: "flatpak".into(), app_id: app_id.clone() });
        }
        self.program.clone().map(Attested::Program)
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

fn flatpak_app(info: &str) -> Option<String> {
    let mut in_application = false;
    for line in info.lines().map(str::trim) {
        if line.starts_with('[') {
            in_application = line == "[Application]";
        } else if in_application && let Some(name) = line.strip_prefix("name=") {
            let name = name.trim();
            return (!name.is_empty()).then(|| name.to_string());
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presented {
    pub name: String,
    pub icon: Option<String>,
    pub desktop_id: Option<String>,
}

#[must_use]
pub fn present(attested: &Attested, index: Option<&AppIndex>) -> Presented {
    let (entry, fallback) = match attested {
        Attested::Program(program) => {
            let path = program.to_str();
            let binary = program.file_name().and_then(std::ffi::OsStr::to_str).unwrap_or_default();
            (index.and_then(|index| index.match_process(path, None)), binary.to_string())
        }
        Attested::Sandboxed { app_id, .. } => (index.and_then(|index| index.get(app_id)), app_id.clone()),
    };
    let name = entry.map(|app| app.name.trim()).filter(|name| !name.is_empty()).map_or(fallback, str::to_string);
    let icon = entry.map(|app| app.icon.trim()).filter(|icon| !icon.is_empty()).map(str::to_string);
    Presented { name, icon, desktop_id: entry.map(|app| app.id.clone()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icon::DesktopApp;

    fn index() -> AppIndex {
        AppIndex::new(vec![
            DesktopApp {
                id: "signal".into(),
                name: "Signal".into(),
                icon: "signal-desktop".into(),
                command: "signal-desktop %U".into(),
                ..DesktopApp::default()
            },
            DesktopApp {
                id: "org.telegram.desktop".into(),
                name: "Telegram".into(),
                icon: "telegram".into(),
                command: "telegram-desktop".into(),
                ..DesktopApp::default()
            },
        ])
    }

    #[test]
    fn a_flatpak_is_attested_by_its_sandbox_and_a_host_program_by_its_path() {
        let info = "[Application]\nname=org.telegram.desktop\nruntime=runtime/org.kde.Platform\n\n[Instance]\nname=x\n";
        assert_eq!(flatpak_app(info).as_deref(), Some("org.telegram.desktop"));
        assert_eq!(flatpak_app("[Instance]\nname=org.example.App\n"), None, "only the Application section names it");
        let sandboxed =
            Identity { program: Some("/app/bin/telegram".into()), flatpak: Some("org.telegram.desktop".into()) };
        assert_eq!(
            sandboxed.attested().map(|attested| attested.key()).as_deref(),
            Some("flatpak:org.telegram.desktop")
        );
        let host = Identity { program: Some("/usr/bin/grim".into()), flatpak: None };
        assert_eq!(host.attested().map(|attested| attested.key()).as_deref(), Some("/usr/bin/grim"));
        assert_eq!(Identity::default().attested(), None);
        let me = Identity::of_pid(i32::try_from(std::process::id()).expect("pid fits")).attested();
        assert!(matches!(me, Some(Attested::Program(path)) if path.is_absolute()));
    }

    #[test]
    fn the_presented_name_comes_from_the_matching_entry_never_the_other_way_round() {
        let index = index();
        let signal = present(&Attested::Program("/usr/lib/signal-desktop/signal-desktop".into()), Some(&index));
        assert_eq!(
            signal,
            Presented { name: "Signal".into(), icon: Some("signal-desktop".into()), desktop_id: Some("signal".into()) }
        );
        let flatpak = Attested::Sandboxed { engine: "flatpak".into(), app_id: "org.telegram.desktop".into() };
        assert_eq!(present(&flatpak, Some(&index)).name, "Telegram");
        let unknown = present(&Attested::Program("/opt/tool/bin/thing".into()), Some(&index));
        assert_eq!((unknown.name.as_str(), unknown.icon, unknown.desktop_id), ("thing", None, None));
        assert_eq!(present(&flatpak, None).name, "org.telegram.desktop", "before the index loads");
    }
}

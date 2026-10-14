use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

const CANDIDATES: &[&str] = &[
    "/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1",
    "/usr/libexec/polkit-gnome-authentication-agent-1",
    "/usr/lib/x86_64-linux-gnu/polkit-gnome/polkit-gnome-authentication-agent-1",
    "/usr/lib/aarch64-linux-gnu/polkit-gnome/polkit-gnome-authentication-agent-1",
    "/usr/libexec/polkit-mate-authentication-agent-1",
    "/usr/lib/mate-polkit/polkit-mate-authentication-agent-1",
    "lxqt-policykit-agent",
    "/usr/lib/polkit-kde-authentication-agent-1",
    "/usr/libexec/kf6/polkit-kde-authentication-agent-1",
    "/usr/lib/xfce-polkit/xfce-polkit",
    "soteria",
];

const AGENT_PREFIXES: &[&str] =
    &["polkit-gnome", "polkit-mate", "polkit-kde", "lxqt-policykit", "xfce-polkit", "soteria", "hyprpolkitagent"];

const SETTLE_DELAY: Duration = Duration::from_secs(2);

#[must_use]
pub fn is_agent_process(comm: &str) -> bool {
    let comm = comm.trim();
    AGENT_PREFIXES.iter().any(|prefix| comm.starts_with(prefix))
        || (comm.contains("polkit") && comm.contains("agent") && !comm.starts_with("polkit-agent-he"))
}

#[must_use]
pub fn first_installed(candidates: &[&str], path_dirs: &[PathBuf], exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    candidates.iter().find_map(|candidate| {
        let candidate = Path::new(candidate);
        if candidate.is_absolute() {
            exists(candidate).then(|| candidate.to_path_buf())
        } else {
            path_dirs.iter().map(|dir| dir.join(candidate)).find(|path| exists(path))
        }
    })
}

fn agent_running() -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    entries.flatten().any(|entry| {
        let path = entry.path();
        entry.file_name().to_str().is_some_and(|name| name.bytes().all(|b| b.is_ascii_digit()))
            && std::fs::metadata(&path).is_ok_and(|meta| meta.uid() == uid)
            && std::fs::read_to_string(path.join("comm")).is_ok_and(|comm| is_agent_process(&comm))
    })
}

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.mode() & 0o111 != 0)
}

pub fn spawn_if_missing() {
    let spawned = std::thread::Builder::new().name("glide-polkit-agent".to_string()).spawn(|| {
        std::thread::sleep(SETTLE_DELAY);
        if agent_running() {
            tracing::debug!("a polkit agent is already running");
            return;
        }
        let path_dirs: Vec<PathBuf> =
            std::env::var_os("PATH").map(|path| std::env::split_paths(&path).collect()).unwrap_or_default();
        let Some(agent) = first_installed(CANDIDATES, &path_dirs, is_executable) else {
            tracing::info!(
                "no polkit agent installed; privileged actions in Settings will fail. \
                     Install polkit-gnome, lxqt-policykit or another agent."
            );
            return;
        };
        match crate::spawn_detached(std::process::Command::new(&agent)) {
            Ok(pid) => tracing::info!(pid, agent = %agent.display(), "spawned polkit agent"),
            Err(err) => {
                tracing::warn!(agent = %agent.display(), "polkit agent did not start: {err}");
            }
        }
    });
    if let Err(err) = spawned {
        tracing::warn!("polkit agent check did not start: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_agents_are_recognised_by_their_truncated_comm() {
        for comm in [
            "polkit-gnome-au\n",
            "polkit-mate-aut",
            "polkit-kde-auth",
            "lxqt-policykit-",
            "xfce-polkit",
            "soteria",
            "hyprpolkitagent",
            "mate-polkit-agent",
        ] {
            assert!(is_agent_process(comm), "{comm}");
        }
    }

    #[test]
    fn the_polkit_daemon_and_its_helper_are_not_agents() {
        for comm in ["polkitd", "polkit-agent-he", "gnome-shell", "sh"] {
            assert!(!is_agent_process(comm), "{comm}");
        }
    }

    #[test]
    fn the_first_installed_candidate_wins_and_bare_names_search_path() {
        let installed =
            [PathBuf::from("/usr/lib/xfce-polkit/xfce-polkit"), PathBuf::from("/opt/bin/lxqt-policykit-agent")];
        let exists = |path: &Path| installed.iter().any(|known| known == path);
        let dirs = [PathBuf::from("/usr/bin"), PathBuf::from("/opt/bin")];
        assert_eq!(first_installed(CANDIDATES, &dirs, exists), Some(PathBuf::from("/opt/bin/lxqt-policykit-agent")));
        assert_eq!(first_installed(CANDIDATES, &[], exists), Some(PathBuf::from("/usr/lib/xfce-polkit/xfce-polkit")));
        assert_eq!(first_installed(CANDIDATES, &dirs, |_| false), None);
    }
}

use crate::{GlideState, window::GlideWindow};

const X11_BRIDGE: &str = "xwayland-sat";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    Kill(i32),
    CloseOnly,
}

#[must_use]
pub fn plan(pid: Option<i32>, own_pid: i32, process_name: Option<&str>) -> Plan {
    let shared = process_name.is_none_or(|name| name.trim().starts_with(X11_BRIDGE));
    match pid {
        Some(pid) if pid > 1 && pid != own_pid && !shared => Plan::Kill(pid),
        _ => Plan::CloseOnly,
    }
}

fn process_name(pid: i32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()
}

impl GlideState {
    pub fn force_quit_window(&mut self, window: &GlideWindow) {
        let pid = self.window_pid(window);
        let name = pid.filter(|&p| p > 0).and_then(process_name);
        let own_pid = i32::try_from(std::process::id()).unwrap_or(0);
        match plan(pid, own_pid, name.as_deref()) {
            Plan::Kill(pid) => {
                tracing::warn!(pid, name = name.as_deref().map(str::trim), "force quitting an app");
                // SAFETY: `kill` takes plain integers; the pid is a live client's,
                // checked above to be neither init, a process group nor Glide itself.
                if unsafe { libc::kill(pid, libc::SIGKILL) } != 0 {
                    tracing::error!(pid, err = %std::io::Error::last_os_error(), "could not kill the app");
                }
            }
            Plan::CloseOnly => {
                tracing::warn!(?pid, "this window shares its process with others; asking it to close instead");
                if let Some(toplevel) = window.toplevel() {
                    toplevel.send_close();
                }
            }
        }
        self.reconcile_space();
    }

    pub fn force_quit_focused(&mut self) {
        let focused = self.layout.focused_window().and_then(|id| self.window_map.get(&id).cloned());
        if let Some(window) = focused {
            self.force_quit_window(&window);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_client_is_killed_by_its_pid() {
        assert_eq!(plan(Some(4242), 100, Some("gedit\n")), Plan::Kill(4242));
    }

    #[test]
    fn glide_itself_init_and_process_groups_are_never_signalled() {
        for pid in [100, 1, 0, -1, -4242] {
            assert_eq!(plan(Some(pid), 100, Some("anything")), Plan::CloseOnly, "{pid}");
        }
        assert_eq!(plan(None, 100, None), Plan::CloseOnly);
    }

    #[test]
    fn the_x11_bridge_is_spared_because_it_hosts_every_x11_app() {
        assert_eq!(plan(Some(4242), 100, Some("xwayland-satell\n")), Plan::CloseOnly);
        assert_eq!(plan(Some(4242), 100, None), Plan::CloseOnly, "a process that cannot be identified is left alone");
    }
}

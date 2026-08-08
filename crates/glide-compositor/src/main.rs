#![allow(irrefutable_let_patterns)]

mod handlers;

mod backend;
mod config;
mod cursor;
mod decoration;
mod focus;
mod grabs;
mod input;
mod ipc;
mod logging;
mod overview;
mod paint;
mod state;
mod window;

use smithay::{
    backend::session::libseat::LibSeatSession,
    reexports::{calloop::EventLoop, wayland_server::Display},
};

use backend::tty::TtyData;
pub use state::GlideState;

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "profile-with-tracy")]
    tracy_client::Client::start();
    profiling::register_thread!("Main");

    let log_path = logging::init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        log = ?log_path,
        "starting glide"
    );

    if let Err(err) = run() {
        tracing::error!("fatal: {err:#}");
        return Err(err);
    }

    Ok(())
}

fn run() -> anyhow::Result<()> {
    let (session, notifier) = LibSeatSession::new()?;

    let mut event_loop = EventLoop::try_new()?;

    let display = Display::new()?;

    let mut state = GlideState::new(
        TtyData::new(session)?,
        event_loop.handle(),
        event_loop.get_signal(),
        display,
    );

    let config = config::Config::load_or_default();
    config.apply_to_state(&mut state);

    crate::backend::tty::init_tty(&mut event_loop, &mut state, notifier)?;

    state.device_added()?;

    crate::backend::tty::init_udev_monitor(&mut event_loop, &mut state)?;

    let ipc_server = ipc::init_ipc(&mut event_loop, &state.socket_name)?;

    unsafe {
        std::env::set_var("WAYLAND_DISPLAY", &state.socket_name);
        if let Some(server) = ipc_server.as_ref() {
            std::env::set_var("GLIDE_IPC_SOCK", server.socket_path());
        }
    }

    spawn_autostart(&config.general.autostart);

    event_loop.run(None, &mut state, move |state| {
        reap_child_processes();
        state.space.refresh();
        state.popups.cleanup();

        if state.tick_overview() | state.any_button_hover_animating() {
            state.queue_redraw();
        }

        let _ = state.display_handle.flush_clients();
    })?;

    tracing::info!("shutting down");

    drop(ipc_server);

    Ok(())
}

fn spawn_autostart(commands: &[String]) {
    for command in commands {
        match spawn_shell_command(command) {
            Ok(child) => tracing::info!(command, pid = child.id(), "spawned autostart command"),
            Err(err) => tracing::error!(command, "failed to spawn autostart command: {err}"),
        }
    }
}

pub fn spawn_shell_command(command: &str) -> std::io::Result<std::process::Child> {
    reap_child_processes();
    let result = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .spawn();
    reap_child_processes();
    result
}

pub fn reap_child_processes() {
    unsafe { while libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) > 0 {} }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reap_child_processes() {
        for _ in 0..3 {
            let _ = std::process::Command::new("true").spawn();
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        reap_child_processes();
        reap_child_processes();
    }

    #[test]
    fn spawn_shell_command_runs_through_a_shell() {
        let mut child = spawn_shell_command("true; true").expect("sh must exist");
        let status = child.wait().expect("child must be waitable");
        assert!(status.success());
    }

    #[test]
    fn spawn_shell_command_reports_a_command_sh_itself_cannot_run() {
        let mut child =
            spawn_shell_command("definitely-not-a-real-command-xyz").expect("sh must exist");
        let status = child.wait().expect("child must be waitable");
        assert!(!status.success());
    }

    #[test]
    fn spawn_autostart_does_not_panic_on_an_empty_list() {
        spawn_autostart(&[]);
    }

    #[test]
    fn spawn_autostart_runs_every_command_in_the_list() {
        let dir = std::env::temp_dir().join(format!("glide-autostart-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let marker_a = dir.join("a");
        let marker_b = dir.join("b");

        spawn_autostart(&[
            format!("touch {}", marker_a.display()),
            format!("touch {}", marker_b.display()),
        ]);
        std::thread::sleep(std::time::Duration::from_millis(200));
        reap_child_processes();

        assert!(marker_a.exists(), "first autostart command did not run");
        assert!(marker_b.exists(), "second autostart command did not run");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

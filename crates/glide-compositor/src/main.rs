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
        // The tty is in graphics mode at this point, so anything printed to
        // the terminal is invisible; the log file is the only place this
        // will actually be readable. anyhow's `{err:#}` prints the full
        // "caused by" chain, not just the outermost message.
        tracing::error!("fatal: {err:#}");
        return Err(err);
    }

    Ok(())
}

fn run() -> anyhow::Result<()> {
    let (session, notifier) = LibSeatSession::new()?;

    // Deliberately not type-annotated: `EventLoop<'l, Data>` carries an
    // explicit lifetime since calloop 0.14, and an explicit annotation here
    // would elide it into a fresh (non-'static) inference variable. Letting
    // inference flow from the `.handle()`/`.get_signal()` calls below
    // resolves it to `'static` directly (see the comment on
    // `GlideState::new` for why those are passed in already-extracted).
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

    // Set up the session, seat and libinput event sources.
    crate::backend::tty::init_tty(&mut event_loop, &mut state, notifier)?;

    // Open the primary GPU, set up rendering and initialize any already
    // connected outputs. Until this returns, the screen still shows a
    // frozen snapshot from before the session took control of the tty (see
    // TtyData's docs) — not a crash, just an expected gap.
    state.device_added()?;

    // Watch udev for monitors being plugged/unplugged after startup.
    crate::backend::tty::init_udev_monitor(&mut event_loop, &mut state)?;

    // Start the control socket. Bound to this variable for its whole lifetime
    // on purpose: dropping it unlinks the socket file, so it must outlive the
    // event loop rather than being cleaned up early.
    let ipc_server = ipc::init_ipc(&mut event_loop, &state.socket_name)?;

    // Point child processes at our sockets.
    //
    // SAFETY: `set_var` is unsound only when another thread may be reading the
    // environment concurrently. Glide is single-threaded at this point (the
    // event loop has not started and nothing has been spawned yet).
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

        // Drive the overview's open/close animation, and any titlebar
        // button's hover fade-in. Both change the scene every frame on
        // their own, so repaints have to be requested explicitly — the
        // damage-driven path only wakes on client activity, and neither of
        // these is that.
        if state.tick_overview() | state.any_button_hover_animating() {
            state.queue_redraw();
        }

        let _ = state.display_handle.flush_clients();
    })?;

    tracing::info!("shutting down");

    // Explicit so it is obvious the socket file is unlinked here rather than
    // at some earlier point the borrow checker happened to allow.
    drop(ipc_server);

    Ok(())
}

/// Spawns every command in `commands`, in order, right after the sockets
/// autostart clients need are ready — see `GeneralConfig::autostart`'s
/// docs. Each is a full shell command line, not just a bare executable
/// name, via [`spawn_shell_command`].
fn spawn_autostart(commands: &[String]) {
    for command in commands {
        match spawn_shell_command(command) {
            Ok(child) => tracing::info!(command, pid = child.id(), "spawned autostart command"),
            Err(err) => tracing::error!(command, "failed to spawn autostart command: {err}"),
        }
    }
}

/// Spawns `command` through `sh -c`, so it can be a full shell command
/// line — arguments, pipes, whatever `sh -c` accepts — not just a bare
/// executable name. The one place that actually runs a process for
/// [`spawn_autostart`], the `exec` keybind action, and the `glidectl exec`
/// IPC command, all of which used to duplicate this same three lines.
///
/// # Errors
///
/// Returns whatever [`std::process::Command::spawn`] does — i.e. `sh`
/// itself failed to launch. `command`'s own success or failure inside that
/// shell doesn't show up here at all, only in the spawned child's
/// eventual exit status.
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
        // Spawn 3 short-lived child processes
        for _ in 0..3 {
            let _ = std::process::Command::new("true").spawn();
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        reap_child_processes();
        // Spawning again to ensure reap is idempotent and non-blocking
        reap_child_processes();
    }

    #[test]
    fn spawn_shell_command_runs_through_a_shell() {
        // "true; true" only succeeds if the command actually goes through
        // `sh -c` (a bare exec of that string as a program name would
        // fail) — the whole point of this helper over a plain
        // `Command::new(command)`.
        let mut child = spawn_shell_command("true; true").expect("sh must exist");
        let status = child.wait().expect("child must be waitable");
        assert!(status.success());
    }

    #[test]
    fn spawn_shell_command_reports_a_command_sh_itself_cannot_run() {
        // `sh` always exists and always spawns; failure shows up in the
        // *exit status*, not as an `Err` from `spawn()` — this just
        // documents that spawn_shell_command's `Result` is about whether
        // `sh` itself launched, not whether the command inside it worked.
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
        // Autostart commands run async (spawn, not wait); give them a
        // moment, then reap so this doesn't leak zombie test processes.
        std::thread::sleep(std::time::Duration::from_millis(200));
        reap_child_processes();

        assert!(marker_a.exists(), "first autostart command did not run");
        assert!(marker_b.exists(), "second autostart command did not run");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

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

    // Spawn a test client, that will run under GlideState
    spawn_client();

    event_loop.run(None, &mut state, move |state| {
        reap_child_processes();
        state.space.refresh();
        state.popups.cleanup();
        let _ = state.display_handle.flush_clients();
    })?;

    tracing::info!("shutting down");

    // Explicit so it is obvious the socket file is unlinked here rather than
    // at some earlier point the borrow checker happened to allow.
    drop(ipc_server);

    Ok(())
}

fn spawn_client() {
    let mut args = std::env::args().skip(1);
    let flag = args.next();
    let arg = args.next();

    let command = match (flag.as_deref(), arg) {
        (Some("-c" | "--command"), Some(command)) => command,
        _ => "alacritty".to_string(),
    };

    match std::process::Command::new(&command).spawn() {
        Ok(child) => tracing::info!(command, pid = child.id(), "spawned client"),
        Err(err) => tracing::error!(command, "failed to spawn client: {err}"),
    }
    reap_child_processes();
}

pub fn reap_child_processes() {
    unsafe {
        while libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) > 0 {}
    }
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
}


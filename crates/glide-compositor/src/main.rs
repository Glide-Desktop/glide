#![allow(irrefutable_let_patterns)]

mod handlers;

mod backend;
mod grabs;
mod input;
mod state;

use backend::{tty::TtyData, winit::WinitData};
use smithay::{
    backend::session::libseat::LibSeatSession,
    reexports::{calloop::EventLoop, wayland_server::Display},
};
pub use state::GlideState;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    // Running under an existing Wayland/X11 session means we're being
    // developed/tested nested inside a host compositor; otherwise we assume
    // we own the tty and should drive the display directly.
    let nested =
        std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some();

    if nested { run_winit() } else { run_tty() }
}

fn run_winit() -> Result<(), Box<dyn std::error::Error>> {
    // Deliberately not type-annotated: `EventLoop<'l, Data>` carries an
    // explicit lifetime since calloop 0.14, and an explicit annotation here
    // would elide it into a fresh (non-'static) inference variable. Letting
    // inference flow from the `.handle()`/`.get_signal()` calls below
    // resolves it to `'static` directly (see the comment on
    // `GlideState::new` for why those are passed in already-extracted).
    let mut event_loop = EventLoop::try_new()?;

    let display = Display::new()?;

    let mut state = GlideState::new(
        WinitData,
        event_loop.handle(),
        event_loop.get_signal(),
        display,
    );

    // Open a Wayland/X11 window for our nested compositor
    crate::backend::winit::init_winit(&mut event_loop, &mut state)?;

    // Set WAYLAND_DISPLAY to our socket name, so child processes connect to GlideState rather
    // than the host compositor
    unsafe { std::env::set_var("WAYLAND_DISPLAY", &state.socket_name) };

    // Spawn a test client, that will run under GlideState
    spawn_client();

    event_loop.run(None, &mut state, move |_| {
        // GlideState is running
    })?;

    Ok(())
}

fn run_tty() -> Result<(), Box<dyn std::error::Error>> {
    let (session, notifier) = LibSeatSession::new()?;

    // See the comment in `run_winit` about why this is not type-annotated.
    let mut event_loop = EventLoop::try_new()?;

    let display = Display::new()?;

    let mut state = GlideState::new(
        TtyData::new(session),
        event_loop.handle(),
        event_loop.get_signal(),
        display,
    );

    // Set up the session, seat and libinput event sources.
    crate::backend::tty::init_tty(&mut event_loop, &mut state, notifier)?;

    // Open the primary GPU, set up rendering and initialize any already
    // connected outputs. Until this returns, the screen still shows a
    // frozen snapshot from before the session took control of the tty (see
    // TtyData's docs) — not a crash, just an expected gap.
    state.device_added()?;

    event_loop.run(None, &mut state, move |_| {
        // GlideState is running
    })?;

    Ok(())
}

fn init_logging() {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }
}

fn spawn_client() {
    let mut args = std::env::args().skip(1);
    let flag = args.next();
    let arg = args.next();

    match (flag.as_deref(), arg) {
        (Some("-c") | Some("--command"), Some(command)) => {
            std::process::Command::new(command).spawn().ok();
        }
        _ => {
            std::process::Command::new("alacritty").spawn().ok();
        }
    }
}

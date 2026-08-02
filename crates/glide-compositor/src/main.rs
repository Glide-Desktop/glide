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
    let mut event_loop: EventLoop<GlideState<WinitData>> = EventLoop::try_new()?;

    let display: Display<GlideState<WinitData>> = Display::new()?;

    let mut state = GlideState::new(WinitData, &mut event_loop, display);

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

    let mut event_loop: EventLoop<GlideState<TtyData>> = EventLoop::try_new()?;

    let display: Display<GlideState<TtyData>> = Display::new()?;

    let mut state = GlideState::new(TtyData::new(session), &mut event_loop, display);

    // Set up the session, seat and libinput event sources. There is no
    // rendering yet (see M1.2 and onward), so the tty's own text console
    // stays visible on screen for now.
    crate::backend::tty::init_tty(&mut event_loop, &mut state, notifier)?;

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

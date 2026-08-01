#![allow(irrefutable_let_patterns)]

mod handlers;

mod backend;
mod grabs;
mod input;
mod state;

use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
pub use state::GlideState;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    let mut event_loop: EventLoop<GlideState> = EventLoop::try_new()?;

    let display: Display<GlideState> = Display::new()?;

    let mut state = GlideState::new(&mut event_loop, display);

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

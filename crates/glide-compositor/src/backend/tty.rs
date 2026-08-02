use smithay::{
    backend::{
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        session::{
            Event as SessionEvent, Session, libseat::LibSeatSession,
            libseat::LibSeatSessionNotifier,
        },
    },
    reexports::{calloop::EventLoop, input::Libinput},
};

use crate::{GlideState, state::Backend};

/// Backend data for running Glide directly on a DRM/KMS output from a tty,
/// without a host compositor.
///
/// At this stage (M1.1) this only carries the session used for seat
/// management and input; DRM/GBM device handling is added in a later
/// milestone.
pub struct TtyData {
    session: LibSeatSession,
}

impl TtyData {
    pub fn new(session: LibSeatSession) -> Self {
        Self { session }
    }
}

impl Backend for TtyData {
    fn seat_name(&self) -> String {
        self.session.seat()
    }

    fn change_vt(&mut self, vt: i32) {
        if let Err(err) = self.session.change_vt(vt) {
            tracing::error!(vt, "failed to switch vt: {}", err);
        }
    }
}

/// Set up the libinput event source and session pause/resume handling for
/// the tty backend.
pub fn init_tty(
    event_loop: &mut EventLoop<GlideState<TtyData>>,
    state: &mut GlideState<TtyData>,
    notifier: LibSeatSessionNotifier,
) -> Result<(), Box<dyn std::error::Error>> {
    let seat_name = state.backend_data.seat_name();
    let session = state.backend_data.session.clone();

    let mut libinput_context =
        Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.into());
    libinput_context
        .udev_assign_seat(&seat_name)
        .map_err(|()| "failed to assign libinput seat")?;
    let libinput_backend = LibinputInputBackend::new(libinput_context.clone());

    event_loop
        .handle()
        .insert_source(libinput_backend, move |event, _, state| {
            state.process_input_event(event);
        })?;

    event_loop
        .handle()
        .insert_source(notifier, move |event, &mut (), _state| match event {
            SessionEvent::PauseSession => {
                tracing::info!("session paused");
                libinput_context.suspend();
            }
            SessionEvent::ActivateSession => {
                tracing::info!("session resumed");
                if let Err(err) = libinput_context.resume() {
                    tracing::error!("failed to resume libinput context: {:?}", err);
                }
            }
        })?;

    Ok(())
}

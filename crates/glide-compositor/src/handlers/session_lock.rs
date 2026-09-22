use smithay::{
    reexports::wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface},
    utils::Size,
    wayland::session_lock::{
        LockSurface, LockSurfaceConfigure, SessionLockHandler, SessionLockManagerState,
        SessionLocker,
    },
};

use crate::state::{GlideState, SessionLock};

impl SessionLockHandler for GlideState {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.protocols.session_lock_state
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        tracing::info!("session locked");
        self.session_lock = Some(SessionLock {
            surfaces: std::collections::HashMap::new(),
        });
        confirmation.lock();
        if let Some(launcher) = self.launcher.as_mut() {
            launcher.begin_close();
        }
        self.begin_close_overview(None);
        self.snapshots.clear();
        self.queue_redraw();
    }

    fn unlock(&mut self) {
        tracing::info!("session unlocked");
        self.session_lock = None;
        self.apply_keyboard_focus_public();
        self.queue_redraw();
    }

    fn new_surface(&mut self, surface: LockSurface, wl_output: WlOutput) {
        let output = smithay::output::Output::from_resource(&wl_output);
        let Some(output) = output else {
            return;
        };
        let size = self
            .space
            .output_geometry(&output)
            .map_or(Size::from((0, 0)), |geometry| geometry.size);
        surface.with_pending_state(|state| {
            state.size = Some(Size::from((
                u32::try_from(size.w).unwrap_or(0),
                u32::try_from(size.h).unwrap_or(0),
            )));
        });
        surface.send_configure();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(
                self,
                Some(surface.wl_surface().clone()),
                smithay::utils::SERIAL_COUNTER.next_serial(),
            );
        }
        if let Some(lock) = self.session_lock.as_mut() {
            lock.surfaces.insert(output, surface);
        }
        self.queue_redraw();
    }

    fn ack_configure(&mut self, _surface: WlSurface, _configure: LockSurfaceConfigure) {
        self.queue_redraw();
    }
}

use smithay::{
    backend::input::InputTime,
    desktop::PopupManager,
    reexports::calloop::timer::{TimeoutAction, Timer},
    reexports::wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface},
    utils::Size,
    wayland::session_lock::{
        LockSurface, LockSurfaceConfigure, SessionLockHandler, SessionLockManagerState, SessionLocker,
    },
};

use crate::state::{GlideState, SessionLock};

const LOCK_CONFIRM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

impl SessionLockHandler for GlideState {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.protocols.session_lock_state
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        tracing::info!("session locked");
        let mut lock = SessionLock::new(confirmation);
        if self.outputs_blanked || !self.backend_data.session_active() || self.space.outputs().next().is_none() {
            lock.confirm();
        }
        self.session_lock = Some(lock);
        self.break_grabs_for_lock();
        let _ = self.loop_handle.insert_source(Timer::from_duration(LOCK_CONFIRM_TIMEOUT), |_, (), state| {
            if let Some(lock) = state.session_lock.as_mut() {
                lock.confirm();
            }
            TimeoutAction::Drop
        });
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
        self.apply_keyboard_focus();
        self.queue_redraw();
    }

    fn new_surface(&mut self, surface: LockSurface, wl_output: WlOutput) {
        let Some(output) = smithay::output::Output::from_resource(&wl_output) else {
            return;
        };
        let size = self.space.output_geometry(&output).map_or(Size::from((0, 0)), |geometry| geometry.size);
        surface.with_pending_state(|state| {
            state.size = Some(Size::from((u32::try_from(size.w).unwrap_or(0), u32::try_from(size.h).unwrap_or(0))));
        });
        surface.send_configure();
        if let Some(lock) = self.session_lock.as_mut() {
            lock.surfaces.insert(output, surface);
        }
        self.focus_lock_surface();
        self.queue_redraw();
    }

    fn ack_configure(&mut self, _surface: WlSurface, _configure: LockSurfaceConfigure) {
        self.queue_redraw();
    }
}

impl GlideState {
    pub(crate) fn break_grabs_for_lock(&mut self) {
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.unset_grab(self);
        }
        if let Some(pointer) = self.seat.get_pointer() {
            pointer.unset_grab(self, serial, InputTime::now());
        }
        if let Some(touch) = self.seat.get_touch() {
            touch.unset_grab(self);
        }
        let roots: Vec<WlSurface> =
            self.space.elements().filter_map(|window| window.toplevel().map(|t| t.wl_surface().clone())).collect();
        for root in roots {
            for (popup, _) in PopupManager::popups_for_surface(&root) {
                let _ = PopupManager::dismiss_popup(&root, &popup);
            }
        }
        self.cancel_touchpad_gesture();
        self.focus_lock_surface();
    }
}

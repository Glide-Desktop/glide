use smithay::{
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
        GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
        GestureSwipeUpdateEvent, GrabStartData as PointerGrabStartData, MotionEvent, PointerGrab, PointerInnerHandle,
        RelativeMotionEvent,
    },
    utils::{Logical, Point},
};

use crate::{GlideState, focus::PointerFocusTarget, window::GlideWindow};

const BTN_LEFT: u32 = 0x110;

const RESTORE_THRESHOLD: f64 = 4.0;

pub struct MoveSurfaceGrab {
    pub start_data: PointerGrabStartData<GlideState>,
    pub window: GlideWindow,
    pub initial_window_location: Point<i32, Logical>,
    pub pending_restore: bool,
}

impl PointerGrab<GlideState> for MoveSurfaceGrab {
    fn motion(
        &mut self, data: &mut GlideState, handle: &mut PointerInnerHandle<'_, GlideState>,
        _focus: Option<(PointerFocusTarget, Point<f64, Logical>)>, event: &MotionEvent,
    ) {
        handle.motion(data, None, event);

        let delta = event.location - self.start_data.location;

        if self.pending_restore {
            if delta.x.abs() < RESTORE_THRESHOLD && delta.y.abs() < RESTORE_THRESHOLD {
                return;
            }
            self.pending_restore = false;
            data.restore_for_move(&self.window, event.location);
            self.start_data.location = event.location;
            let Some(restored) = data.space.element_location(&self.window) else {
                return;
            };
            self.initial_window_location = restored;
            return;
        }

        let new_location = self.initial_window_location.to_f64() + delta;
        let new_location = new_location.to_i32_round();

        data.space.map_element(self.window.clone(), new_location, true);

        data.record_window_move(&self.window, new_location);
        data.update_snap_preview(event.location);
    }

    forward_pointer_events!();

    fn button(&mut self, data: &mut GlideState, handle: &mut PointerInnerHandle<'_, GlideState>, event: &ButtonEvent) {
        handle.button(data, event);

        if !handle.current_pressed().contains(&BTN_LEFT) {
            self.window.set_dragging(false);
            let snapped = data.apply_snap(&self.window);
            if !snapped && !self.pending_restore {
                data.settle_moved_window(&self.window, handle.current_location());
            }
            data.queue_redraw();
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn unset(&mut self, data: &mut GlideState) {
        self.window.set_dragging(false);
        data.snap_dwell.reset();
        if let Some(preview) = data.snap_preview.as_mut() {
            preview.dismiss();
            data.queue_redraw();
        }
    }
}

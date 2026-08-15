use smithay::{
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
        GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
        GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData as PointerGrabStartData,
        MotionEvent, PointerGrab, PointerInnerHandle, RelativeMotionEvent,
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
        &mut self,
        data: &mut GlideState,
        handle: &mut PointerInnerHandle<'_, GlideState>,
        _focus: Option<(PointerFocusTarget, Point<f64, Logical>)>,
        event: &MotionEvent,
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

        data.space
            .map_element(self.window.clone(), new_location, true);

        data.record_window_move(&self.window, new_location);
    }

    fn relative_motion(
        &mut self,
        data: &mut GlideState,
        handle: &mut PointerInnerHandle<'_, GlideState>,
        focus: Option<(PointerFocusTarget, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut GlideState,
        handle: &mut PointerInnerHandle<'_, GlideState>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);

        if !handle.current_pressed().contains(&BTN_LEFT) {
            self.window.set_dragging(false);
            data.queue_redraw();
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn axis(
        &mut self,
        data: &mut GlideState,
        handle: &mut PointerInnerHandle<'_, GlideState>,
        details: AxisFrame,
    ) {
        handle.axis(data, details);
    }

    fn frame(&mut self, data: &mut GlideState, handle: &mut PointerInnerHandle<'_, GlideState>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut GlideState,
        handle: &mut PointerInnerHandle<'_, GlideState>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut GlideState,
        handle: &mut PointerInnerHandle<'_, GlideState>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut GlideState,
        handle: &mut PointerInnerHandle<'_, GlideState>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut GlideState,
        handle: &mut PointerInnerHandle<'_, GlideState>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut GlideState,
        handle: &mut PointerInnerHandle<'_, GlideState>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut GlideState,
        handle: &mut PointerInnerHandle<'_, GlideState>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut GlideState,
        handle: &mut PointerInnerHandle<'_, GlideState>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut GlideState,
        handle: &mut PointerInnerHandle<'_, GlideState>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &PointerGrabStartData<GlideState> {
        &self.start_data
    }

    fn unset(&mut self, _data: &mut GlideState) {}
}

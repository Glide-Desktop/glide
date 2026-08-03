//! Move grab is the state of a composer during which the client window is being dragged around.
//!
//! eg. Usually whenever a user clicks on the app's titlebar and starts dragging, the compositors
//! enters a `MoveSurfaceGrab` state.

use smithay::{
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
        GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
        GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData as PointerGrabStartData,
        MotionEvent, PointerGrab, PointerInnerHandle, RelativeMotionEvent,
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point},
};

use crate::{GlideState, window::GlideWindow};

// The button code as defined in the Linux kernel's
// linux/input-event-codes.h header file, e.g. BTN_LEFT.
const BTN_LEFT: u32 = 0x110;

pub struct MoveSurfaceGrab {
    pub start_data: PointerGrabStartData<GlideState>,
    pub window: GlideWindow,
    pub initial_window_location: Point<i32, Logical>,
}

impl PointerGrab<GlideState> for MoveSurfaceGrab {
    fn motion(
        &mut self,
        data: &mut GlideState,
        handle: &mut PointerInnerHandle<'_, GlideState>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        // While the grab is active, no client has pointer focus
        handle.motion(data, None, event);

        let delta = event.location - self.start_data.location;
        let new_location = self.initial_window_location.to_f64() + delta;
        let new_location = new_location.to_i32_round();

        data.space
            .map_element(self.window.clone(), new_location, true);

        // Mirror the move into the layout engine. Without this the next
        // reconcile would re-apply the stored position and teleport the
        // window back to where the drag started.
        data.record_window_move(&self.window, new_location);
    }

    fn relative_motion(
        &mut self,
        data: &mut GlideState,
        handle: &mut PointerInnerHandle<'_, GlideState>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
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
            // No more buttons are pressed, release the grab.
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

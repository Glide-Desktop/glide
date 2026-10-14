use smithay::{
    input::{
        pointer::{
            AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
            GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
            GestureSwipeUpdateEvent, GrabStartData as PointerGrabStartData, MotionEvent, PointerGrab,
            PointerInnerHandle, RelativeMotionEvent,
        },
        touch::{
            DownEvent, GrabStartData as TouchGrabStartData, MotionEvent as TouchMotionEvent, OrientationEvent,
            ShapeEvent, TouchGrab, TouchInnerHandle, UpEvent,
        },
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point},
};

use crate::{GlideState, focus::PointerFocusTarget, window::GlideWindow};

const RESTORE_THRESHOLD: f64 = 4.0;

pub struct MoveCore {
    window: GlideWindow,
    start: Point<f64, Logical>,
    last: Point<f64, Logical>,
    initial_window_location: Point<i32, Logical>,
    pending_restore: bool,
}

impl MoveCore {
    #[must_use]
    pub const fn new(
        window: GlideWindow, start: Point<f64, Logical>, initial_window_location: Point<i32, Logical>,
        pending_restore: bool,
    ) -> Self {
        Self { window, start, last: start, initial_window_location, pending_restore }
    }

    fn motion(&mut self, data: &mut GlideState, location: Point<f64, Logical>) {
        self.last = location;
        let delta = location - self.start;
        if self.pending_restore {
            if delta.x.abs() < RESTORE_THRESHOLD && delta.y.abs() < RESTORE_THRESHOLD {
                return;
            }
            self.pending_restore = false;
            data.restore_for_move(&self.window, location);
            self.start = location;
            let Some(restored) = data.space.element_location(&self.window) else {
                return;
            };
            self.initial_window_location = restored;
            return;
        }
        let new_location = (self.initial_window_location.to_f64() + delta).to_i32_round();
        data.space.map_element(self.window.clone(), new_location, true);
        data.record_window_move(&self.window, new_location);
        data.update_snap_preview(location);
    }

    fn finish(&self, data: &mut GlideState, location: Point<f64, Logical>) {
        self.window.set_dragging(false);
        let snapped = data.apply_snap(&self.window);
        if !snapped && !self.pending_restore {
            data.settle_moved_window(&self.window, location);
        }
        data.queue_redraw();
    }

    fn unset(&self, data: &mut GlideState) {
        self.window.set_dragging(false);
        data.snap_dwell.reset();
        if let Some(preview) = data.snap_preview.as_mut() {
            preview.dismiss();
            data.queue_redraw();
        }
    }
}

pub struct MoveSurfaceGrab {
    start_data: PointerGrabStartData<GlideState>,
    core: MoveCore,
}

impl MoveSurfaceGrab {
    #[must_use]
    pub const fn new(
        start_data: PointerGrabStartData<GlideState>, window: GlideWindow,
        initial_window_location: Point<i32, Logical>, pending_restore: bool,
    ) -> Self {
        let start = start_data.location;
        Self { start_data, core: MoveCore::new(window, start, initial_window_location, pending_restore) }
    }
}

impl PointerGrab<GlideState> for MoveSurfaceGrab {
    fn motion(
        &mut self, data: &mut GlideState, handle: &mut PointerInnerHandle<'_, GlideState>,
        _focus: Option<(PointerFocusTarget, Point<f64, Logical>)>, event: &MotionEvent,
    ) {
        handle.motion(data, None, event);
        self.core.motion(data, event.location);
    }

    forward_pointer_events!();

    fn button(&mut self, data: &mut GlideState, handle: &mut PointerInnerHandle<'_, GlideState>, event: &ButtonEvent) {
        handle.button(data, event);
        if !handle.current_pressed().contains(&self.start_data.button) {
            self.core.finish(data, handle.current_location());
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn unset(&mut self, data: &mut GlideState) {
        self.core.unset(data);
    }
}

pub struct TouchMoveGrab {
    start_data: TouchGrabStartData<GlideState>,
    core: MoveCore,
}

impl TouchMoveGrab {
    #[must_use]
    pub const fn new(
        start_data: TouchGrabStartData<GlideState>, window: GlideWindow, initial_window_location: Point<i32, Logical>,
        pending_restore: bool,
    ) -> Self {
        let start = start_data.location;
        Self { start_data, core: MoveCore::new(window, start, initial_window_location, pending_restore) }
    }
}

impl TouchGrab<GlideState> for TouchMoveGrab {
    fn down(
        &mut self, data: &mut GlideState, handle: &mut TouchInnerHandle<'_, GlideState>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>, event: &DownEvent,
    ) {
        handle.down(data, None, event);
    }

    fn up(&mut self, data: &mut GlideState, handle: &mut TouchInnerHandle<'_, GlideState>, event: &UpEvent) {
        handle.up(data, event);
        if event.slot == self.start_data.slot {
            self.core.finish(data, self.core.last);
            handle.unset_grab(self, data);
        }
    }

    fn motion(
        &mut self, data: &mut GlideState, handle: &mut TouchInnerHandle<'_, GlideState>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>, event: &TouchMotionEvent,
    ) {
        if event.slot == self.start_data.slot {
            self.core.motion(data, event.location);
        } else {
            handle.motion(data, None, event);
        }
    }

    fn frame(&mut self, data: &mut GlideState, handle: &mut TouchInnerHandle<'_, GlideState>) {
        handle.frame(data);
    }

    fn cancel(&mut self, data: &mut GlideState, handle: &mut TouchInnerHandle<'_, GlideState>) {
        handle.cancel(data);
        handle.unset_grab(self, data);
    }

    fn shape(&mut self, data: &mut GlideState, handle: &mut TouchInnerHandle<'_, GlideState>, event: &ShapeEvent) {
        handle.shape(data, event);
    }

    fn orientation(
        &mut self, data: &mut GlideState, handle: &mut TouchInnerHandle<'_, GlideState>, event: &OrientationEvent,
    ) {
        handle.orientation(data, event);
    }

    fn start_data(&self) -> &TouchGrabStartData<GlideState> {
        &self.start_data
    }

    fn unset(&mut self, data: &mut GlideState) {
        self.core.unset(data);
    }
}

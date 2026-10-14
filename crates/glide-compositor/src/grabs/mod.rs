macro_rules! forward_pointer_events {
    () => {
        forward_pointer_events!(
            gesture_swipe_begin: GestureSwipeBeginEvent,
            gesture_swipe_update: GestureSwipeUpdateEvent,
            gesture_swipe_end: GestureSwipeEndEvent,
            gesture_pinch_begin: GesturePinchBeginEvent,
            gesture_pinch_update: GesturePinchUpdateEvent,
            gesture_pinch_end: GesturePinchEndEvent,
            gesture_hold_begin: GestureHoldBeginEvent,
            gesture_hold_end: GestureHoldEndEvent,
        );
    };
    ($($gesture:ident: $event:ty),+ $(,)?) => {
        fn relative_motion(
            &mut self,
            data: &mut GlideState,
            handle: &mut PointerInnerHandle<'_, GlideState>,
            focus: Option<(PointerFocusTarget, Point<f64, Logical>)>,
            event: &RelativeMotionEvent,
        ) {
            handle.relative_motion(data, focus, event);
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

        $(
            fn $gesture(
                &mut self,
                data: &mut GlideState,
                handle: &mut PointerInnerHandle<'_, GlideState>,
                event: &$event,
            ) {
                handle.$gesture(data, event);
            }
        )+

        fn start_data(&self) -> &PointerGrabStartData<GlideState> {
            &self.start_data
        }
    };
}

pub mod move_grab;
pub use move_grab::MoveSurfaceGrab;

pub mod resize_grab;
pub use resize_grab::ResizeSurfaceGrab;

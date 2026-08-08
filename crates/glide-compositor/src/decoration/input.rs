use smithay::{
    input::{
        Seat,
        pointer::{
            AxisFrame, ButtonEvent, CursorIcon, CursorImageStatus, GestureHoldBeginEvent,
            GestureHoldEndEvent, GesturePinchBeginEvent, GesturePinchEndEvent,
            GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
            GestureSwipeUpdateEvent, GrabStartData, MotionEvent, PointerTarget,
            RelativeMotionEvent,
        },
    },
    utils::{IsAlive, Serial},
};

use crate::{
    GlideState,
    grabs::{MoveSurfaceGrab, ResizeSurfaceGrab, resize_grab::ResizeEdge},
    window::GlideWindow,
};

use super::{header::DecorationAction, menu::run_menu_item};

pub(crate) fn resize_cursor(edges: ResizeEdge) -> CursorIcon {
    match edges {
        e if e == ResizeEdge::TOP => CursorIcon::NResize,
        e if e == ResizeEdge::BOTTOM => CursorIcon::SResize,
        e if e == ResizeEdge::LEFT => CursorIcon::WResize,
        e if e == ResizeEdge::RIGHT => CursorIcon::EResize,
        e if e == ResizeEdge::TOP_LEFT => CursorIcon::NwResize,
        e if e == ResizeEdge::TOP_RIGHT => CursorIcon::NeResize,
        e if e == ResizeEdge::BOTTOM_LEFT => CursorIcon::SwResize,
        e if e == ResizeEdge::BOTTOM_RIGHT => CursorIcon::SeResize,
        _ => CursorIcon::Default,
    }
}

pub(crate) fn start_move(
    seat: &Seat<GlideState>,
    state: &mut GlideState,
    window: &GlideWindow,
    event: &ButtonEvent,
) {
    let seat = seat.clone();
    let window = window.clone();
    let button = event.button;
    let serial = event.serial;

    state.loop_handle.insert_idle(move |state| {
        let Some(pointer) = seat.get_pointer() else {
            return;
        };
        let Some(initial_window_location) = state.space.element_location(&window) else {
            return;
        };

        let start_data = GrabStartData {
            focus: None,
            button,
            location: pointer.current_location(),
        };
        let grab = MoveSurfaceGrab {
            start_data,
            window: window.clone(),
            initial_window_location,
        };
        pointer.set_grab(state, grab, serial, smithay::input::pointer::Focus::Clear);
    });
}

pub(crate) fn start_resize(
    seat: &Seat<GlideState>,
    state: &mut GlideState,
    window: &GlideWindow,
    event: &ButtonEvent,
    edges: ResizeEdge,
) {
    let seat = seat.clone();
    let window = window.clone();
    let button = event.button;
    let serial = event.serial;

    state.loop_handle.insert_idle(move |state| {
        let Some(pointer) = seat.get_pointer() else {
            return;
        };
        let Some(location) = state.space.element_location(&window) else {
            return;
        };

        let start_data = GrabStartData {
            focus: None,
            button,
            location: pointer.current_location(),
        };
        let initial_rect = smithay::utils::Rectangle::new(location, window.geometry().size);

        let grab = ResizeSurfaceGrab::start(start_data, window.clone(), edges, initial_rect);
        pointer.set_grab(state, grab, serial, smithay::input::pointer::Focus::Clear);

        state.cursor_status = CursorImageStatus::Named(resize_cursor(edges));
    });
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decoration(pub GlideWindow);

impl IsAlive for Decoration {
    fn alive(&self) -> bool {
        self.0.alive()
    }
}

impl Decoration {
    fn update_pointer(&self, data: &mut GlideState, event: &MotionEvent) {
        let icon = {
            if let Some(mut deco) = self.0.try_decoration_state_mut() {
                deco.header.pointer_enter(event.location);
                deco.header
                    .resize_edges_at(event.location)
                    .map_or(CursorIcon::Default, resize_cursor)
            } else {
                CursorIcon::Default
            }
        };
        data.cursor_status = CursorImageStatus::Named(icon);
    }
}

impl PointerTarget<GlideState> for Decoration {
    fn enter(&self, _seat: &Seat<GlideState>, data: &mut GlideState, event: &MotionEvent) {
        self.update_pointer(data, event);
    }

    fn motion(&self, _seat: &Seat<GlideState>, data: &mut GlideState, event: &MotionEvent) {
        self.update_pointer(data, event);
    }

    fn relative_motion(
        &self,
        _seat: &Seat<GlideState>,
        _data: &mut GlideState,
        _event: &RelativeMotionEvent,
    ) {
    }

    fn button(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &ButtonEvent) {
        let action = self
            .0
            .try_decoration_state_mut()
            .and_then(|mut deco| deco.header.on_button(event));

        match action {
            Some(DecorationAction::ToggleMaximize) => {
                if let Some(id) = data.window_id(&self.0) {
                    data.layout.toggle_maximized(id);
                    data.reconcile_space();
                }
            }
            Some(DecorationAction::Close) => {
                if let Some(toplevel) = self.0.toplevel() {
                    toplevel.send_close();
                }
            }
            Some(DecorationAction::Run(item)) => run_menu_item(data, &self.0, item),
            Some(DecorationAction::StartMove) => start_move(seat, data, &self.0, event),
            Some(DecorationAction::StartResize(edges)) => {
                start_resize(seat, data, &self.0, event, edges);
            }
            None => {}
        }
    }

    fn axis(&self, _seat: &Seat<GlideState>, _data: &mut GlideState, _frame: AxisFrame) {}

    fn frame(&self, _seat: &Seat<GlideState>, _data: &mut GlideState) {}

    fn leave(&self, _seat: &Seat<GlideState>, _data: &mut GlideState, _serial: Serial, _time: u32) {
        if let Some(mut deco) = self.0.try_decoration_state_mut() {
            deco.header.pointer_leave();
        }
    }

    fn gesture_swipe_begin(
        &self,
        _seat: &Seat<GlideState>,
        _data: &mut GlideState,
        _event: &GestureSwipeBeginEvent,
    ) {
    }
    fn gesture_swipe_update(
        &self,
        _seat: &Seat<GlideState>,
        _data: &mut GlideState,
        _event: &GestureSwipeUpdateEvent,
    ) {
    }
    fn gesture_swipe_end(
        &self,
        _seat: &Seat<GlideState>,
        _data: &mut GlideState,
        _event: &GestureSwipeEndEvent,
    ) {
    }
    fn gesture_pinch_begin(
        &self,
        _seat: &Seat<GlideState>,
        _data: &mut GlideState,
        _event: &GesturePinchBeginEvent,
    ) {
    }
    fn gesture_pinch_update(
        &self,
        _seat: &Seat<GlideState>,
        _data: &mut GlideState,
        _event: &GesturePinchUpdateEvent,
    ) {
    }
    fn gesture_pinch_end(
        &self,
        _seat: &Seat<GlideState>,
        _data: &mut GlideState,
        _event: &GesturePinchEndEvent,
    ) {
    }
    fn gesture_hold_begin(
        &self,
        _seat: &Seat<GlideState>,
        _data: &mut GlideState,
        _event: &GestureHoldBeginEvent,
    ) {
    }
    fn gesture_hold_end(
        &self,
        _seat: &Seat<GlideState>,
        _data: &mut GlideState,
        _event: &GestureHoldEndEvent,
    ) {
    }
}

use std::cell::Cell;

use smithay::{
    backend::input::ButtonState,
    input::{
        Seat,
        pointer::{
            AxisFrame, ButtonEvent, CursorIcon, CursorImageStatus, GestureHoldBeginEvent, GestureHoldEndEvent,
            GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
            GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData, MotionEvent, PointerTarget,
            RelativeMotionEvent,
        },
    },
    utils::{IsAlive, Logical, Point, Serial},
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

#[derive(Debug, Default)]
struct HeldGrabButton(Cell<Option<u32>>);

fn held_grab_button(window: &GlideWindow) -> &Cell<Option<u32>> {
    let data = window.user_data();
    data.insert_if_missing(HeldGrabButton::default);
    &data.get::<HeldGrabButton>().expect("inserted just above").0
}

fn release_held_grab_button(window: &GlideWindow, event: &ButtonEvent) {
    if event.state == ButtonState::Released {
        let held = held_grab_button(window);
        if held.get() == Some(event.button) {
            held.set(None);
        }
    }
}

fn grab_when_idle(
    seat: &Seat<GlideState>, state: &mut GlideState, window: &GlideWindow, event: &ButtonEvent,
    begin: impl FnOnce(&mut GlideState, &GlideWindow, GrabStartData<GlideState>, Point<i32, Logical>) -> Option<Grab>
    + 'static,
) {
    let (seat, window, button, serial) = (seat.clone(), window.clone(), event.button, event.serial);
    held_grab_button(&window).set(Some(button));
    state.loop_handle.insert_idle(move |state| {
        if held_grab_button(&window).take() != Some(button) {
            return;
        }
        let Some(pointer) = seat.get_pointer() else {
            return;
        };
        state.finish_workspace_swipe();
        let Some(location) = state.space.element_location(&window) else {
            return;
        };
        let start_data = GrabStartData { focus: None, button, location: pointer.current_location() };
        let clear = smithay::input::pointer::Focus::Clear;
        match begin(state, &window, start_data, location) {
            Some(Grab::Move(grab)) => pointer.set_grab(state, grab, serial, clear),
            Some(Grab::Resize(grab, edges)) => {
                pointer.set_grab(state, grab, serial, clear);
                state.cursor_status = CursorImageStatus::Named(resize_cursor(edges));
            }
            None => return,
        }
        state.queue_redraw();
    });
}

enum Grab {
    Move(MoveSurfaceGrab),
    Resize(ResizeSurfaceGrab, ResizeEdge),
}

pub(crate) fn start_move(seat: &Seat<GlideState>, state: &mut GlideState, window: &GlideWindow, event: &ButtonEvent) {
    grab_when_idle(seat, state, window, event, |state, window, start_data, location| {
        window.set_dragging(true);
        Some(Grab::Move(MoveSurfaceGrab::new(start_data, window.clone(), location, state.is_size_locked(window))))
    });
}

pub(crate) fn start_resize(
    seat: &Seat<GlideState>, state: &mut GlideState, window: &GlideWindow, event: &ButtonEvent, edges: ResizeEdge,
) {
    if state.is_size_locked(window) {
        return;
    }
    grab_when_idle(seat, state, window, event, move |state, window, start_data, location| {
        if state.is_size_locked(window) {
            return None;
        }
        let initial_rect = smithay::utils::Rectangle::new(location, window.geometry().size);
        Some(Grab::Resize(ResizeSurfaceGrab::start(start_data, window.clone(), edges, initial_rect), edges))
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
        let resizable = !data.is_size_locked(&self.0);
        let icon = self.0.try_decoration_state_mut().map_or(CursorIcon::Default, |mut deco| {
            deco.header.pointer_enter(event.location);
            deco.header.resize_edges_at(event.location).filter(|_| resizable).map_or(CursorIcon::Default, resize_cursor)
        });
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

    fn relative_motion(&self, _seat: &Seat<GlideState>, _data: &mut GlideState, _event: &RelativeMotionEvent) {}

    fn button(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &ButtonEvent) {
        release_held_grab_button(&self.0, event);
        let action = self.0.try_decoration_state_mut().and_then(|mut deco| deco.header.on_button(event));

        match action {
            Some(DecorationAction::ToggleMaximize) => {
                if let Some(id) = data.window_id(&self.0) {
                    data.morph_geometry(id, |state| {
                        state.layout.toggle_maximized(id);
                    });
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

    fn leave(
        &self, _seat: &Seat<GlideState>, data: &mut GlideState, _serial: Serial,
        _time: smithay::backend::input::InputTime,
    ) {
        if let Some(mut deco) = self.0.try_decoration_state_mut() {
            deco.header.pointer_leave();
        }
        data.cursor_status = CursorImageStatus::default_named();
    }

    fn gesture_swipe_begin(&self, _seat: &Seat<GlideState>, _data: &mut GlideState, _event: &GestureSwipeBeginEvent) {}
    fn gesture_swipe_update(&self, _seat: &Seat<GlideState>, _data: &mut GlideState, _event: &GestureSwipeUpdateEvent) {
    }
    fn gesture_swipe_end(&self, _seat: &Seat<GlideState>, _data: &mut GlideState, _event: &GestureSwipeEndEvent) {}
    fn gesture_pinch_begin(&self, _seat: &Seat<GlideState>, _data: &mut GlideState, _event: &GesturePinchBeginEvent) {}
    fn gesture_pinch_update(&self, _seat: &Seat<GlideState>, _data: &mut GlideState, _event: &GesturePinchUpdateEvent) {
    }
    fn gesture_pinch_end(&self, _seat: &Seat<GlideState>, _data: &mut GlideState, _event: &GesturePinchEndEvent) {}
    fn gesture_hold_begin(&self, _seat: &Seat<GlideState>, _data: &mut GlideState, _event: &GestureHoldBeginEvent) {}
    fn gesture_hold_end(&self, _seat: &Seat<GlideState>, _data: &mut GlideState, _event: &GestureHoldEndEvent) {}
}

use std::{borrow::Cow, sync::Arc};

use smithay::{
    input::{
        Seat,
        dnd::{DndFocus, Source},
        pointer::{
            AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
            GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
            GestureSwipeUpdateEvent, MotionEvent, PointerTarget, RelativeMotionEvent,
        },
    },
    reexports::wayland_server::{DisplayHandle, protocol::wl_surface::WlSurface},
    utils::{IsAlive, Logical, Point, Serial},
    wayland::{seat::WaylandFocus, selection::data_device::WlOfferData},
};

use crate::{GlideState, decoration::Decoration, window::GlideWindow};

#[derive(Debug, Clone, PartialEq)]
pub enum PointerFocusTarget {
    Surface(WlSurface),
    Decoration(Decoration),
}

impl PointerFocusTarget {
    #[must_use]
    pub fn as_surface(&self) -> Option<&WlSurface> {
        match self {
            Self::Surface(s) => Some(s),
            Self::Decoration(_) => None,
        }
    }
}

impl From<WlSurface> for PointerFocusTarget {
    fn from(surface: WlSurface) -> Self {
        Self::Surface(surface)
    }
}

impl From<GlideWindow> for PointerFocusTarget {
    fn from(window: GlideWindow) -> Self {
        Self::Decoration(Decoration(window))
    }
}

impl IsAlive for PointerFocusTarget {
    fn alive(&self) -> bool {
        match self {
            Self::Surface(s) => s.alive(),
            Self::Decoration(d) => d.alive(),
        }
    }
}

impl WaylandFocus for PointerFocusTarget {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        match self {
            Self::Surface(s) => Some(Cow::Borrowed(s)),
            Self::Decoration(_) => None,
        }
    }
}

impl PointerTarget<GlideState> for PointerFocusTarget {
    fn enter(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &MotionEvent) {
        match self {
            Self::Surface(s) => PointerTarget::enter(s, seat, data, event),
            Self::Decoration(d) => PointerTarget::enter(d, seat, data, event),
        }
    }

    fn motion(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &MotionEvent) {
        match self {
            Self::Surface(s) => PointerTarget::motion(s, seat, data, event),
            Self::Decoration(d) => PointerTarget::motion(d, seat, data, event),
        }
    }

    fn relative_motion(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &RelativeMotionEvent) {
        match self {
            Self::Surface(s) => PointerTarget::relative_motion(s, seat, data, event),
            Self::Decoration(d) => PointerTarget::relative_motion(d, seat, data, event),
        }
    }

    fn button(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &ButtonEvent) {
        match self {
            Self::Surface(s) => PointerTarget::button(s, seat, data, event),
            Self::Decoration(d) => PointerTarget::button(d, seat, data, event),
        }
    }

    fn axis(&self, seat: &Seat<GlideState>, data: &mut GlideState, frame: AxisFrame) {
        match self {
            Self::Surface(s) => PointerTarget::axis(s, seat, data, frame),
            Self::Decoration(d) => PointerTarget::axis(d, seat, data, frame),
        }
    }

    fn frame(&self, seat: &Seat<GlideState>, data: &mut GlideState) {
        match self {
            Self::Surface(s) => PointerTarget::frame(s, seat, data),
            Self::Decoration(d) => PointerTarget::frame(d, seat, data),
        }
    }

    fn leave(
        &self, seat: &Seat<GlideState>, data: &mut GlideState, serial: Serial, time: smithay::backend::input::InputTime,
    ) {
        match self {
            Self::Surface(s) => PointerTarget::leave(s, seat, data, serial, time),
            Self::Decoration(d) => PointerTarget::leave(d, seat, data, serial, time),
        }
    }

    fn gesture_swipe_begin(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &GestureSwipeBeginEvent) {
        match self {
            Self::Surface(s) => PointerTarget::gesture_swipe_begin(s, seat, data, event),
            Self::Decoration(d) => PointerTarget::gesture_swipe_begin(d, seat, data, event),
        }
    }

    fn gesture_swipe_update(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &GestureSwipeUpdateEvent) {
        match self {
            Self::Surface(s) => PointerTarget::gesture_swipe_update(s, seat, data, event),
            Self::Decoration(d) => PointerTarget::gesture_swipe_update(d, seat, data, event),
        }
    }

    fn gesture_swipe_end(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &GestureSwipeEndEvent) {
        match self {
            Self::Surface(s) => PointerTarget::gesture_swipe_end(s, seat, data, event),
            Self::Decoration(d) => PointerTarget::gesture_swipe_end(d, seat, data, event),
        }
    }

    fn gesture_pinch_begin(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &GesturePinchBeginEvent) {
        match self {
            Self::Surface(s) => PointerTarget::gesture_pinch_begin(s, seat, data, event),
            Self::Decoration(d) => PointerTarget::gesture_pinch_begin(d, seat, data, event),
        }
    }

    fn gesture_pinch_update(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &GesturePinchUpdateEvent) {
        match self {
            Self::Surface(s) => PointerTarget::gesture_pinch_update(s, seat, data, event),
            Self::Decoration(d) => PointerTarget::gesture_pinch_update(d, seat, data, event),
        }
    }

    fn gesture_pinch_end(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &GesturePinchEndEvent) {
        match self {
            Self::Surface(s) => PointerTarget::gesture_pinch_end(s, seat, data, event),
            Self::Decoration(d) => PointerTarget::gesture_pinch_end(d, seat, data, event),
        }
    }

    fn gesture_hold_begin(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &GestureHoldBeginEvent) {
        match self {
            Self::Surface(s) => PointerTarget::gesture_hold_begin(s, seat, data, event),
            Self::Decoration(d) => PointerTarget::gesture_hold_begin(d, seat, data, event),
        }
    }

    fn gesture_hold_end(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &GestureHoldEndEvent) {
        match self {
            Self::Surface(s) => PointerTarget::gesture_hold_end(s, seat, data, event),
            Self::Decoration(d) => PointerTarget::gesture_hold_end(d, seat, data, event),
        }
    }
}

impl DndFocus<GlideState> for PointerFocusTarget {
    type OfferData<S: Source> = WlOfferData<S>;

    fn enter<S: Source>(
        &self, data: &mut GlideState, dh: &DisplayHandle, source: Arc<S>, seat: &Seat<GlideState>,
        location: Point<f64, Logical>, serial: &Serial,
    ) -> Option<Self::OfferData<S>> {
        match self {
            Self::Surface(s) => DndFocus::enter(s, data, dh, source, seat, location, serial),
            Self::Decoration(_) => None,
        }
    }

    fn motion<S: Source>(
        &self, data: &mut GlideState, offer: Option<&mut Self::OfferData<S>>, seat: &Seat<GlideState>,
        location: Point<f64, Logical>, time: smithay::backend::input::InputTime,
    ) {
        if let Self::Surface(s) = self {
            DndFocus::motion(s, data, offer, seat, location, time);
        }
    }

    fn leave<S: Source>(&self, data: &mut GlideState, offer: Option<&mut Self::OfferData<S>>, seat: &Seat<GlideState>) {
        if let Self::Surface(s) = self {
            DndFocus::leave(s, data, offer, seat);
        }
    }

    fn drop<S: Source>(&self, data: &mut GlideState, offer: Option<&mut Self::OfferData<S>>, seat: &Seat<GlideState>) {
        if let Self::Surface(s) = self {
            DndFocus::drop(s, data, offer, seat);
        }
    }
}

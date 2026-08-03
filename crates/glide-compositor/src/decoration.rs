//! Server-side window decorations: a titlebar with a fixed button layout and
//! a chevron-triggered menu for actions that don't get their own button.
//!
//! # Layout (not configurable — see [`crate::config::DecorationTheme`])
//!
//! ```text
//! ┌───┬───┬─────────────────────────────────┬───┐
//! │ M │ v │            (title)              │ X │
//! └───┴───┴─────────────────────────────────┴───┘
//! ```
//! Maximize (M) is always leftmost, close (X) always rightmost, and the
//! chevron (v) — which opens a dropdown for actions that don't warrant their
//! own button, currently just Minimize — sits immediately to its right.
//! Everything between the chevron and the close button is drag territory:
//! pressing there and moving starts an interactive move, exactly as if the
//! client itself had requested one.
//!
//! Every window is decorated for now; respecting a client's own
//! `xdg-decoration` preference is M4.5.

use std::cell::{RefCell, RefMut};

use smithay::{
    backend::input::ButtonState,
    backend::renderer::element::{
        Kind,
        solid::{SolidColorBuffer, SolidColorRenderElement},
    },
    input::{
        Seat,
        pointer::{
            AxisFrame, ButtonEvent, CursorImageStatus, GestureHoldBeginEvent, GestureHoldEndEvent,
            GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent,
            GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData,
            MotionEvent, PointerTarget, RelativeMotionEvent,
        },
    },
    utils::{IsAlive, Logical, Physical, Point, Scale, Serial},
};

use crate::{GlideState, config::DecorationTheme, grabs::MoveSurfaceGrab, window::GlideWindow};

/// Width of the maximize/close/chevron buttons. Height is the titlebar
/// height itself, from [`DecorationTheme::height`].
pub const BUTTON_WIDTH: i32 = 32;

/// An action offered in the chevron dropdown.
///
/// A single variant today (as requested: the menu exists specifically to
/// hold Minimize), but kept as an enum with an `ALL` slice rather than a
/// bare "minimize button" so adding a second entry later is a one-line
/// change here rather than a new parallel mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Minimize,
}

impl MenuItem {
    pub const ALL: &'static [Self] = &[Self::Minimize];
}

/// Per-window decoration state: hover tracking and the pixel buffers redrawn
/// from it. Lives in the window's own [`smithay::utils::user_data::UserDataMap`]
/// (see [`GlideWindow::decoration_state`]) rather than as a field on
/// `GlideWindow` itself, since that map is exactly what lets `SpaceElement`
/// and `AsRenderElements` — both `&self` methods — mutate per-frame redraw
/// state without `GlideWindow` needing `Send`-unfriendly interior mutability
/// of its own.
#[derive(Debug, Default)]
pub struct DecorationState {
    pub header: HeaderBar,
}

#[derive(Debug, Default)]
pub struct HeaderBar {
    /// 0 means undecorated: no titlebar is drawn or reserved. Set once from
    /// [`DecorationTheme::height`] when a window is created.
    height: i32,
    width: i32,
    pointer_loc: Option<Point<f64, Logical>>,
    menu_open: bool,
    menu_hover: Option<usize>,

    /// Colors and focus, snapshotted here (rather than read live from
    /// `GlideState`) because [`smithay::backend::renderer::element::AsRenderElements`],
    /// where `redraw` ultimately gets called from, has no access to
    /// compositor state — only to the renderer. Kept in sync by
    /// [`crate::GlideState::reconcile_space`] via [`GlideWindow::sync_decoration`].
    /// Hover is unaffected: that already lives entirely in this struct,
    /// updated straight from pointer motion.
    theme: DecorationTheme,
    focused: bool,

    background: SolidColorBuffer,
    maximize_button: SolidColorBuffer,
    close_button: SolidColorBuffer,
    chevron_button: SolidColorBuffer,
    menu_background: SolidColorBuffer,
    menu_items: Vec<SolidColorBuffer>,
}

/// What part of the titlebar a point falls in, for both hit-testing and
/// hover-highlight purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HitZone {
    Maximize,
    Chevron,
    Close,
    /// The plain background: dragging here moves the window.
    Drag,
}

/// What a completed titlebar interaction should do.
///
/// Decided by [`HeaderBar::on_button`] *while* the decoration state is
/// borrowed, and executed by [`Decoration::button`] *after* that borrow is
/// released. The split is not stylistic: executing directly under the
/// borrow meant `reconcile_space` → `sync_decoration` → a second
/// `borrow_mut()` of the same `RefCell` — a guaranteed `BorrowMutError`
/// panic the first time anyone clicked the maximize button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecorationAction {
    ToggleMaximize,
    Close,
    Run(MenuItem),
    StartMove,
}

impl HeaderBar {
    #[must_use]
    pub const fn is_decorated(&self) -> bool {
        self.height > 0
    }

    #[must_use]
    pub const fn height(&self) -> i32 {
        self.height
    }

    pub const fn set_height(&mut self, height: i32) {
        self.height = height;
    }

    /// Snapshots the theme and focus state to redraw with. See the fields'
    /// own docs for why this happens ahead of time rather than at render.
    fn sync(&mut self, theme: &DecorationTheme, focused: bool) {
        if &self.theme != theme {
            self.theme = theme.clone();
        }
        self.focused = focused;
    }

    fn zone_at(&self, x: f64) -> HitZone {
        if x < f64::from(BUTTON_WIDTH) {
            HitZone::Maximize
        } else if x < f64::from(BUTTON_WIDTH * 2) {
            HitZone::Chevron
        } else if x >= f64::from(self.width - BUTTON_WIDTH) {
            HitZone::Close
        } else {
            HitZone::Drag
        }
    }

    /// Height of everything hit-testable: the titlebar strip, plus the open
    /// dropdown extending below it.
    fn interactive_height(&self) -> i32 {
        let menu_rows = if self.menu_open {
            i32::try_from(MenuItem::ALL.len()).unwrap_or(0)
        } else {
            0
        };
        self.height * (1 + menu_rows)
    }

    /// Whether `rel` — relative to the decoration's own origin, the
    /// titlebar's top-left — falls inside the decoration.
    ///
    /// This, not just "the titlebar strip", is what hit-testing must use:
    /// the open dropdown extends *below* the strip, over the client's own
    /// content, and treating that area as content would route clicks on the
    /// menu straight to the client underneath it.
    pub fn contains(&self, rel: Point<f64, Logical>) -> bool {
        self.is_decorated()
            && rel.x >= 0.0
            && rel.x < f64::from(self.width)
            && rel.y >= 0.0
            && rel.y < f64::from(self.interactive_height())
    }

    /// Menu row index under `loc` (decoration-origin coordinates), if the
    /// menu is open and `loc` is inside it.
    ///
    /// Row 0 starts *below* the titlebar strip — at `y == height`, not at
    /// `y == 0` — hence the subtraction: without it, hovering the titlebar
    /// itself lit up row 0 and the real row 0 counted as row 1.
    fn menu_item_at(&self, loc: Point<f64, Logical>) -> Option<usize> {
        if !self.menu_open || loc.x < 0.0 || loc.x >= f64::from(self.width) {
            return None;
        }

        let y_in_menu = loc.y - f64::from(self.height);
        if y_in_menu < 0.0 {
            return None;
        }

        // Row indices are always small (bounded by MenuItem::ALL.len(), a
        // handful of entries at most), so this narrowing never truncates in
        // a way that changes which row was clicked.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = (y_in_menu / f64::from(self.height)) as usize;

        (index < MenuItem::ALL.len()).then_some(index)
    }

    pub fn pointer_enter(&mut self, loc: Point<f64, Logical>) {
        self.pointer_loc = Some(loc);
        self.menu_hover = self.menu_item_at(loc);
    }

    pub fn pointer_leave(&mut self) {
        self.pointer_loc = None;
        self.menu_hover = None;
    }

    /// Decides what a button press/release on the decoration should do,
    /// mutating only the header's own state (menu open/closed).
    ///
    /// Deliberately has no access to `GlideState`: it runs under the
    /// decoration `RefCell` borrow, and anything that reaches
    /// `reconcile_space` from here would re-borrow it and panic. The caller
    /// executes the returned action after releasing the borrow.
    ///
    /// Only release triggers an action (matching how the rest of Glide
    /// treats clicks); press alone starts a move when it lands on the plain
    /// background, since a drag by definition begins while the button is
    /// still held.
    fn on_button(&mut self, event: &ButtonEvent) -> Option<DecorationAction> {
        let loc = self.pointer_loc?;

        if event.state == ButtonState::Pressed {
            // A press with the menu open is handled entirely on release,
            // same as everywhere else in the titlebar.
            if self.menu_open {
                return None;
            }
            return (self.zone_at(loc.x) == HitZone::Drag).then_some(DecorationAction::StartMove);
        }

        if self.menu_open {
            let item = self.menu_item_at(loc).and_then(|i| MenuItem::ALL.get(i));
            let action = item.map(|&item| DecorationAction::Run(item));
            // Any release closes the menu, whether it hit an item or not.
            self.menu_open = false;
            return action;
        }

        match self.zone_at(loc.x) {
            HitZone::Maximize => Some(DecorationAction::ToggleMaximize),
            HitZone::Close => Some(DecorationAction::Close),
            HitZone::Chevron => {
                self.menu_open = true;
                None
            }
            HitZone::Drag => None,
        }
    }

    /// Redraws every buffer that needs it, from the theme/focus snapshotted
    /// by [`Self::sync`] and the hover state tracked live. Cheap to call
    /// every frame: `SolidColorBuffer::update` only bumps its damage counter
    /// when a value actually changed.
    fn redraw(&mut self) {
        if self.width == 0 || self.height == 0 {
            return;
        }

        let theme = &self.theme;
        let bg = if self.focused {
            theme.background_focused.0
        } else {
            theme.background.0
        };
        self.background.update((self.width, self.height), bg);

        let hovered = self.pointer_loc.map(|l| self.zone_at(l.x));
        let color_for = |zone: HitZone, normal: [f32; 4], hover: [f32; 4]| {
            if hovered == Some(zone) { hover } else { normal }
        };

        self.maximize_button.update(
            (BUTTON_WIDTH, self.height),
            color_for(
                HitZone::Maximize,
                theme.maximize_button.0,
                theme.maximize_button_hover.0,
            ),
        );
        self.chevron_button.update(
            (BUTTON_WIDTH, self.height),
            color_for(
                HitZone::Chevron,
                theme.chevron_button.0,
                theme.chevron_button_hover.0,
            ),
        );
        self.close_button.update(
            (BUTTON_WIDTH, self.height),
            color_for(
                HitZone::Close,
                theme.close_button.0,
                theme.close_button_hover.0,
            ),
        );

        if self.menu_open {
            self.menu_background.update(
                (
                    self.width,
                    self.height * i32::try_from(MenuItem::ALL.len()).unwrap_or(0),
                ),
                theme.menu_background.0,
            );
            self.menu_items
                .resize_with(MenuItem::ALL.len(), SolidColorBuffer::default);
            let menu_hover = self.menu_hover;
            let menu_item_hover = theme.menu_item_hover.0;
            let menu_background = theme.menu_background.0;
            for (i, buf) in self.menu_items.iter_mut().enumerate() {
                let color = if menu_hover == Some(i) {
                    menu_item_hover
                } else {
                    menu_background
                };
                buf.update((self.width, self.height), color);
            }
        }
    }
}

/// Starts an interactive move exactly as if the client had called
/// `xdg_toplevel.move`, without going through that request at all: a
/// titlebar drag is a compositor-only affordance, there is no client-side
/// `move_request` to route through here.
///
/// Deferred via `insert_idle` rather than calling `pointer.set_grab`
/// synchronously — this runs from inside `PointerTarget::button`, itself
/// called from *within* `DefaultGrab::button()`, partway through:
/// ```text
/// DefaultGrab::button():
///     handle.button(data, event)                       // <- dispatches into us
///     if Pressed { handle.set_grab(.., click_grab) }    // <- runs *after* we return
/// ```
/// A grab set now would be unconditionally clobbered by that second
/// `set_grab` a few lines later in the very same call — `PointerInternal::set_grab`
/// replaces whatever grab is active without checking what it is. Deferring
/// to the next event loop iteration runs after that has already happened,
/// so our grab is the one still installed afterward. (This is also how
/// anvil's own titlebar handles the same request.)
///
/// `GrabStartData` is built by hand rather than via `pointer.grab_start_data()`
/// for the same reason: nothing has installed a grab yet at this point for
/// it to return. `focus: None` is fine — unlike `check_grab`'s use of an
/// existing grab's start data, nothing in `MoveSurfaceGrab` reads the
/// `focus` field, only `location`.
fn start_move(
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

fn run_menu_item(state: &mut GlideState, window: &GlideWindow, item: MenuItem) {
    let Some(id) = state.window_id(window) else {
        return;
    };
    match item {
        MenuItem::Minimize => {
            tracing::info!(?id, "minimizing via titlebar menu");
            state.layout.set_minimized(id, true);
        }
    }
    state.reconcile_space();
}

impl GlideWindow {
    /// Per-window decoration state, created with `height: 0` (undecorated)
    /// on first access.
    pub fn decoration_state(&self) -> RefMut<'_, DecorationState> {
        self.user_data()
            .insert_if_missing(|| RefCell::new(DecorationState::default()));
        self.user_data()
            .get::<RefCell<DecorationState>>()
            .expect("inserted unconditionally on the line above")
            .borrow_mut()
    }

    /// Sets the titlebar height (0 disables the decoration entirely) and
    /// snapshots theme/focus for the next redraw. Called from
    /// [`crate::GlideState::reconcile_space`], once per window per
    /// reconcile — cheap, and it is what keeps decoration rendering in sync
    /// with configuration reloads and focus changes despite
    /// [`smithay::backend::renderer::element::AsRenderElements`] having no
    /// access to compositor state of its own.
    ///
    /// Width is synced here too, not only lazily at render time: `zone_at`
    /// measures the close button from the right edge, so hit-testing a
    /// click that somehow arrives before the first render would otherwise
    /// run against `width == 0` and classify most of the bar as Close.
    pub fn sync_decoration(&self, theme: &DecorationTheme, height: i32, focused: bool) {
        let width = self.geometry().size.w;
        let mut state = self.decoration_state();
        state.header.set_height(height);
        state.header.width = width;
        state.header.sync(theme, focused);
    }
}

/// A pointer focus target that isn't a client surface: the decoration
/// belonging to a window. Routed to via [`crate::focus::PointerFocusTarget`]
/// so titlebar clicks reach `HeaderBar::clicked` through Smithay's own
/// pointer dispatch, the same way a `WlSurface` does.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoration(pub GlideWindow);

impl IsAlive for Decoration {
    fn alive(&self) -> bool {
        self.0.alive()
    }
}

impl PointerTarget<GlideState> for Decoration {
    fn enter(&self, _seat: &Seat<GlideState>, data: &mut GlideState, event: &MotionEvent) {
        // The decoration is compositor territory: whatever cursor the client
        // last set via `wl_pointer.set_cursor` stops applying the moment the
        // pointer crosses onto the titlebar. Without this the client's
        // I-beam (or whatever else) lingered over our own UI.
        data.cursor_status = CursorImageStatus::default_named();

        self.0
            .decoration_state()
            .header
            .pointer_enter(event.location);
    }

    fn motion(&self, _seat: &Seat<GlideState>, _data: &mut GlideState, event: &MotionEvent) {
        self.0
            .decoration_state()
            .header
            .pointer_enter(event.location);
    }

    fn relative_motion(
        &self,
        _seat: &Seat<GlideState>,
        _data: &mut GlideState,
        _event: &RelativeMotionEvent,
    ) {
    }

    fn button(&self, seat: &Seat<GlideState>, data: &mut GlideState, event: &ButtonEvent) {
        // The decision is made under the decoration borrow; the temporary
        // `RefMut` ends with this statement. That ordering is load-bearing:
        // the actions below reach `reconcile_space`, which re-borrows this
        // same `RefCell` via `sync_decoration`, and executing them while
        // still borrowed panicked with `BorrowMutError` on the first
        // maximize-button click.
        let action = self.0.decoration_state().header.on_button(event);

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
            None => {}
        }
    }

    fn axis(&self, _seat: &Seat<GlideState>, _data: &mut GlideState, _frame: AxisFrame) {}

    fn frame(&self, _seat: &Seat<GlideState>, _data: &mut GlideState) {}

    fn leave(&self, _seat: &Seat<GlideState>, _data: &mut GlideState, _serial: Serial, _time: u32) {
        self.0.decoration_state().header.pointer_leave();
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

/// Renders the titlebar (and, if open, its dropdown) for one window, using
/// the theme/focus [`HeaderBar::sync`] last snapshotted and the hover state
/// tracked live from pointer motion.
///
/// No renderer parameter: unlike a texture-backed element, `SolidColorBuffer`
/// needs none.
pub fn render_elements(
    window: &GlideWindow,
    location: Point<i32, Physical>,
    scale: Scale<f64>,
    alpha: f32,
) -> Vec<SolidColorRenderElement> {
    let mut state = window.decoration_state();
    let header = &mut state.header;
    if !header.is_decorated() {
        return Vec::new();
    }

    let content_width = window.geometry().size.w;
    if header.width != content_width {
        header.width = content_width;
    }
    header.redraw();

    let mut elements = Vec::new();

    if header.menu_open {
        let menu_y = header.height;
        elements.push(SolidColorRenderElement::from_buffer(
            &header.menu_background,
            location + Point::from((0, menu_y)).to_physical_precise_round(scale),
            scale,
            alpha,
            Kind::Unspecified,
        ));
        for (i, buf) in header.menu_items.iter().enumerate() {
            let y = menu_y + header.height * i32::try_from(i).unwrap_or(0);
            elements.push(SolidColorRenderElement::from_buffer(
                buf,
                location + Point::from((0, y)).to_physical_precise_round(scale),
                scale,
                alpha,
                Kind::Unspecified,
            ));
        }
    }

    let close_x = header.width - BUTTON_WIDTH;
    elements.push(SolidColorRenderElement::from_buffer(
        &header.close_button,
        location + Point::from((close_x, 0)).to_physical_precise_round(scale),
        scale,
        alpha,
        Kind::Unspecified,
    ));
    elements.push(SolidColorRenderElement::from_buffer(
        &header.chevron_button,
        location + Point::from((BUTTON_WIDTH, 0)).to_physical_precise_round(scale),
        scale,
        alpha,
        Kind::Unspecified,
    ));
    elements.push(SolidColorRenderElement::from_buffer(
        &header.maximize_button,
        location,
        scale,
        alpha,
        Kind::Unspecified,
    ));
    elements.push(SolidColorRenderElement::from_buffer(
        &header.background,
        location,
        scale,
        alpha,
        Kind::Unspecified,
    ));

    elements
}

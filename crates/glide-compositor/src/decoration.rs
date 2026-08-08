use std::cell::{Ref, RefCell, RefMut};

use smithay::{
    backend::{
        allocator::Fourcc,
        input::ButtonState,
        renderer::element::{memory::MemoryRenderBuffer, solid::SolidColorBuffer},
    },
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
    utils::{IsAlive, Logical, Point, Rectangle, Size, Transform},
};

use crate::{
    GlideState,
    config::{Color, DecorationTheme, TitleAlign},
    grabs::{MoveSurfaceGrab, ResizeSurfaceGrab, resize_grab::ResizeEdge},
    paint::{Painter, TextAlign},
    window::GlideWindow,
};

pub const BUTTON_WIDTH: i32 = 32;

const RIGHT_CLUSTER_WIDTH: i32 = BUTTON_WIDTH * 2;

pub const RESIZE_BORDER: i32 = 8;

const TITLE_PADDING: i32 = 12;

const MENU_PADDING: i32 = 12;

const MENU_MIN_WIDTH: i32 = 120;

const MENU_CORNER_RADIUS: f32 = 8.0;

const PRESSED_DARKEN: f32 = 0.8;

const BUTTON_HOVER_ANIM: std::time::Duration = std::time::Duration::from_millis(100);

const SHADOW_BANDS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Minimize,
}

impl MenuItem {
    pub const ALL: &'static [Self] = &[Self::Minimize];

    pub fn label(self) -> &'static str {
        match self {
            Self::Minimize => "Minimize",
        }
    }
}

#[derive(Debug, Default)]
pub struct DecorationState {
    pub header: HeaderBar,
    pub border: BorderState,
    pub shadow: ShadowState,
}

pub struct DecorationBuffer {
    pub buf: MemoryRenderBuffer,
    pub offset: Point<i32, Logical>,
}

#[derive(Debug, Default)]
pub struct BorderState {
    top: SolidColorBuffer,
    bottom: SolidColorBuffer,
    left: SolidColorBuffer,
    right: SolidColorBuffer,
}

pub struct BorderStrip {
    pub buf: SolidColorBuffer,
    pub offset: Point<i32, Logical>,
}

#[derive(Debug, Default)]
pub struct ShadowState {
    rings: Vec<SolidColorBuffer>,
}

#[derive(Debug, Default)]
pub struct HeaderBar {
    height: i32,
    width: i32,
    content_height: i32,
    title: String,
    pointer_loc: Option<Point<f64, Logical>>,
    menu_open: bool,
    menu_hover: Option<usize>,
    menu_x: i32,
    menu_width: i32,
    pressed: Option<HitZone>,
    button_hover: Option<HitZone>,
    button_hover_started: Option<std::time::Instant>,

    theme: DecorationTheme,
    focused: bool,
    maximized: bool,
    scale: i32,

    titlebar_buf: Option<MemoryRenderBuffer>,
    maximize_buf: Option<MemoryRenderBuffer>,
    chevron_buf: Option<MemoryRenderBuffer>,
    close_buf: Option<MemoryRenderBuffer>,
    menu_row_bufs: Vec<MemoryRenderBuffer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HitZone {
    Maximize,
    Chevron,
    Close,
    Drag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecorationAction {
    ToggleMaximize,
    Close,
    Run(MenuItem),
    StartMove,
    StartResize(ResizeEdge),
}

fn resize_cursor(edges: ResizeEdge) -> CursorIcon {
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

fn redraw_button(
    buf: &mut Option<MemoryRenderBuffer>,
    height: i32,
    scale: i32,
    bg: Color,
    fg: Color,
    icon: fn(&mut [u8], i32, i32, i32, Color, Color),
) {
    let w = BUTTON_WIDTH;
    let (pw, ph) = (w * scale, height * scale);
    let buf = buf.get_or_insert_with(|| {
        MemoryRenderBuffer::new(Fourcc::Argb8888, (pw, ph), scale, Transform::Normal, None)
    });
    let mut ctx = buf.render();
    ctx.resize((pw, ph));
    let _ = ctx.draw(|pixels| {
        icon(pixels, w, height, scale, fg, bg);
        Ok::<_, ()>(vec![Rectangle::from_size((pw, ph).into())])
    });
}

impl HeaderBar {
    #[must_use]
    pub const fn is_decorated(&self) -> bool {
        self.height > 0
    }

    #[must_use]
    pub const fn has_frame(&self) -> bool {
        self.width > 0 && self.content_height > 0
    }

    #[must_use]
    pub const fn height(&self) -> i32 {
        self.height
    }

    pub const fn set_height(&mut self, height: i32) {
        self.height = height;
    }

    #[must_use]
    pub fn shadow_margin(&self) -> i32 {
        if self.theme.shadow {
            i32::try_from(self.theme.shadow_size).unwrap_or(i32::MAX)
        } else {
            0
        }
    }

    fn sync(&mut self, theme: &DecorationTheme, focused: bool, maximized: bool, scale: i32) {
        if &self.theme != theme {
            self.theme = theme.clone();
        }
        self.focused = focused;
        self.maximized = maximized;
        if self.scale != scale {
            self.scale = scale;
            self.titlebar_buf = None;
            self.maximize_buf = None;
            self.chevron_buf = None;
            self.close_buf = None;
            self.menu_row_bufs.clear();
        }
    }

    const fn effective_scale(&self) -> i32 {
        if self.scale < 1 { 1 } else { self.scale }
    }

    fn button_hover_progress(&mut self, hovered: Option<HitZone>) -> f64 {
        if self.button_hover != hovered {
            self.button_hover = hovered;
            self.button_hover_started = hovered.map(|_| std::time::Instant::now());
        }
        let Some(started) = self.button_hover_started else {
            return 1.0;
        };
        if BUTTON_HOVER_ANIM.is_zero() {
            return 1.0;
        }
        (started.elapsed().as_secs_f64() / BUTTON_HOVER_ANIM.as_secs_f64()).clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn is_hover_animating(&self) -> bool {
        self.button_hover.is_some()
            && self
                .button_hover_started
                .is_some_and(|started| started.elapsed() < BUTTON_HOVER_ANIM)
    }

    pub fn set_title(&mut self, title: String) {
        if self.title != title {
            self.title = title;
            self.titlebar_buf = None;
        }
    }

    fn zone_at(&self, x: f64) -> HitZone {
        let w = f64::from(self.width);
        let bw = f64::from(BUTTON_WIDTH);
        if x < bw {
            HitZone::Maximize
        } else if x >= w - bw {
            HitZone::Close
        } else if x >= w - bw * 2.0 {
            HitZone::Chevron
        } else {
            HitZone::Drag
        }
    }

    fn interactive_height(&self) -> i32 {
        let menu_rows = if self.menu_open {
            i32::try_from(MenuItem::ALL.len()).unwrap_or(0)
        } else {
            0
        };
        self.height * (1 + menu_rows)
    }

    pub fn contains(&self, rel: Point<f64, Logical>) -> bool {
        if !self.has_frame() {
            return false;
        }

        let width = f64::from(self.width);
        let frame_height = f64::from(self.height + self.content_height);
        let band = f64::from(RESIZE_BORDER);

        let in_expanded_frame =
            rel.x >= -band && rel.x < width + band && rel.y >= -band && rel.y < frame_height + band;
        if !in_expanded_frame {
            return false;
        }

        let in_content = rel.x >= 0.0
            && rel.x < width
            && rel.y >= f64::from(self.height)
            && rel.y < frame_height;
        if in_content {
            if !self.menu_open || rel.y >= f64::from(self.interactive_height()) {
                return false;
            }
            let menu_left = f64::from(self.menu_x);
            let menu_right = f64::from(self.menu_x + self.menu_width);
            return rel.x >= menu_left && rel.x < menu_right;
        }

        true
    }

    fn resize_edges_at(&self, rel: Point<f64, Logical>) -> Option<ResizeEdge> {
        if !self.has_frame() {
            return None;
        }

        let width = f64::from(self.width);
        let frame_height = f64::from(self.height + self.content_height);

        let mut edges = ResizeEdge::empty();
        if rel.x < 0.0 {
            edges |= ResizeEdge::LEFT;
        } else if rel.x >= width {
            edges |= ResizeEdge::RIGHT;
        }
        if rel.y < 0.0 {
            edges |= ResizeEdge::TOP;
        } else if rel.y >= frame_height {
            edges |= ResizeEdge::BOTTOM;
        }

        (!edges.is_empty()).then_some(edges)
    }

    fn menu_item_at(&self, loc: Point<f64, Logical>) -> Option<usize> {
        let menu_left = f64::from(self.menu_x);
        let menu_right = f64::from(self.menu_x + self.menu_width);
        if !self.menu_open || loc.x < menu_left || loc.x >= menu_right {
            return None;
        }

        let y_in_menu = loc.y - f64::from(self.height);
        if y_in_menu < 0.0 {
            return None;
        }

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
        self.pressed = None;
    }

    fn on_button(&mut self, event: &ButtonEvent) -> Option<DecorationAction> {
        let loc = self.pointer_loc?;

        if event.state == ButtonState::Pressed {
            if self.menu_open {
                return None;
            }
            if let Some(edges) = self.resize_edges_at(loc) {
                return Some(DecorationAction::StartResize(edges));
            }
            if !self.in_titlebar_strip(loc) {
                return None;
            }
            let zone = self.zone_at(loc.x);
            if zone == HitZone::Drag {
                return Some(DecorationAction::StartMove);
            }
            self.pressed = Some(zone);
            return None;
        }

        self.pressed = None;

        if self.menu_open {
            let item = self.menu_item_at(loc).and_then(|i| MenuItem::ALL.get(i));
            let action = item.map(|&item| DecorationAction::Run(item));
            self.menu_open = false;
            return action;
        }

        if !self.in_titlebar_strip(loc) {
            return None;
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

    fn in_titlebar_strip(&self, loc: Point<f64, Logical>) -> bool {
        loc.x >= 0.0
            && loc.x < f64::from(self.width)
            && loc.y >= 0.0
            && loc.y < f64::from(self.height)
    }

    #[allow(clippy::too_many_lines)]
    fn redraw(&mut self, painter: &Painter) {
        if self.width == 0 || self.height == 0 {
            return;
        }

        let hovered = self
            .pointer_loc
            .filter(|l| self.in_titlebar_strip(*l))
            .map(|l| self.zone_at(l.x));
        let pressed = self.pressed;
        #[allow(clippy::cast_possible_truncation)]
        let hover_t = self.button_hover_progress(hovered) as f32;

        let theme = &self.theme;
        let height = self.height;
        let scale = self.effective_scale();
        let bg_color = if self.focused {
            theme.background_focused
        } else {
            theme.background
        };
        let bg = bg_color.to_buffer_bytes();

        {
            let w = self.width;
            let h = self.height;
            let (pw, ph) = (w * scale, h * scale);
            let buf = self.titlebar_buf.get_or_insert_with(|| {
                MemoryRenderBuffer::new(Fourcc::Argb8888, (pw, ph), scale, Transform::Normal, None)
            });
            let mut ctx = buf.render();
            ctx.resize((pw, ph));
            let title = self.title.clone();
            let align = match theme.title_align {
                TitleAlign::Left => TextAlign::Left,
                TitleAlign::Center => TextAlign::Center,
            };
            let _ = ctx.draw(|pixels| {
                let fg = [255, 255, 255, bg[3]];
                Painter::fill_panel_background(
                    pixels,
                    w,
                    h,
                    scale,
                    bg_color,
                    theme.corner_radius,
                    [true, true, false, false],
                );
                let x_min = BUTTON_WIDTH + TITLE_PADDING;
                let x_max = w - RIGHT_CLUSTER_WIDTH;
                painter.draw_label_over(pixels, w, h, x_min, x_max, scale, align, &title, fg);
                Ok::<_, ()>(vec![Rectangle::from_size((pw, ph).into())])
            });
        }

        let maximize_icon: fn(&mut [u8], i32, i32, i32, Color, Color) = if self.maximized {
            Painter::draw_icon_restore
        } else {
            Painter::draw_icon_maximize
        };

        for (buf, zone, normal, hover_bg, fg, icon) in [
            (
                &mut self.maximize_buf,
                HitZone::Maximize,
                theme.maximize_button,
                theme.maximize_button_hover,
                theme.icon,
                maximize_icon,
            ),
            (
                &mut self.chevron_buf,
                HitZone::Chevron,
                theme.chevron_button,
                theme.chevron_button_hover,
                theme.icon,
                Painter::draw_icon_chevron as fn(&mut [u8], i32, i32, i32, Color, Color),
            ),
            (
                &mut self.close_buf,
                HitZone::Close,
                theme.close_button,
                theme.close_button_hover,
                theme.close_icon,
                Painter::draw_icon_close,
            ),
        ] {
            let btn_bg = if pressed == Some(zone) {
                hover_bg.darken(PRESSED_DARKEN)
            } else if hovered == Some(zone) {
                normal.lerp(hover_bg, hover_t)
            } else {
                normal
            };
            redraw_button(buf, height, scale, btn_bg, fg, icon);
        }

        if self.menu_open {
            let row_h = self.height;

            #[allow(clippy::cast_possible_truncation)]
            let widest = MenuItem::ALL
                .iter()
                .map(|item| painter.text_width(item.label()) as i32)
                .max()
                .unwrap_or(0);
            let row_w = (widest + MENU_PADDING * 2)
                .max(MENU_MIN_WIDTH)
                .min(self.width);
            self.menu_width = row_w;
            self.menu_x = (self.width - row_w).max(0);

            let (prow_w, prow_h) = (row_w * scale, row_h * scale);
            self.menu_row_bufs.resize_with(MenuItem::ALL.len(), || {
                MemoryRenderBuffer::new(
                    Fourcc::Argb8888,
                    (prow_w, prow_h),
                    scale,
                    Transform::Normal,
                    None,
                )
            });
            let menu_hover = self.menu_hover;
            let menu_bg = theme.menu_background;
            let menu_hover_bg = theme.menu_item_hover;
            let font_fg = [230u8, 230, 230, 255];
            let font_fg_hover = [255u8, 255, 255, 255];
            let row_count = self.menu_row_bufs.len();
            for (i, buf) in self.menu_row_bufs.iter_mut().enumerate() {
                let is_hovered = menu_hover == Some(i);
                let bg = if is_hovered { menu_hover_bg } else { menu_bg };
                let fg = if is_hovered { font_fg_hover } else { font_fg };
                let label = MenuItem::ALL
                    .get(i)
                    .map_or("", |item| item.label())
                    .to_owned();
                let mut ctx = buf.render();
                ctx.resize((prow_w, prow_h));
                let _ = ctx.draw(|pixels| {
                    Painter::fill_panel_background(
                        pixels,
                        row_w,
                        row_h,
                        scale,
                        bg,
                        MENU_CORNER_RADIUS,
                        row_corners(i, row_count),
                    );
                    painter.draw_label_over(
                        pixels,
                        row_w,
                        row_h,
                        MENU_PADDING,
                        row_w - MENU_PADDING,
                        scale,
                        TextAlign::Left,
                        &label,
                        fg,
                    );
                    Ok::<_, ()>(vec![Rectangle::from_size((prow_w, prow_h).into())])
                });
            }
        } else {
            self.menu_row_bufs.clear();
        }
    }
}

const fn row_corners(index: usize, total: usize) -> [bool; 4] {
    let is_first = index == 0;
    let is_last = index == total - 1;
    [is_first, is_first, is_last, is_last]
}

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

fn start_resize(
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
    pub fn decoration_state(&self) -> Ref<'_, DecorationState> {
        self.user_data()
            .insert_if_missing(|| RefCell::new(DecorationState::default()));
        self.user_data()
            .get::<RefCell<DecorationState>>()
            .expect("inserted unconditionally on the line above")
            .borrow()
    }

    pub fn decoration_state_mut(&self) -> RefMut<'_, DecorationState> {
        self.user_data()
            .insert_if_missing(|| RefCell::new(DecorationState::default()));
        self.user_data()
            .get::<RefCell<DecorationState>>()
            .expect("inserted unconditionally on the line above")
            .borrow_mut()
    }

    pub fn try_decoration_state(&self) -> Option<Ref<'_, DecorationState>> {
        self.user_data()
            .insert_if_missing(|| RefCell::new(DecorationState::default()));
        self.user_data()
            .get::<RefCell<DecorationState>>()
            .and_then(|cell| cell.try_borrow().ok())
    }

    pub fn try_decoration_state_mut(&self) -> Option<RefMut<'_, DecorationState>> {
        self.user_data()
            .insert_if_missing(|| RefCell::new(DecorationState::default()));
        self.user_data()
            .get::<RefCell<DecorationState>>()
            .and_then(|cell| cell.try_borrow_mut().ok())
    }

    pub fn sync_decoration(
        &self,
        theme: &DecorationTheme,
        height: i32,
        focused: bool,
        maximized: bool,
        scale: i32,
    ) {
        let content = self.geometry().size;
        if let Some(mut state) = self.try_decoration_state_mut() {
            state.header.set_height(height);
            state.header.width = content.w;
            state.header.content_height = content.h;
            state.header.sync(theme, focused, maximized, scale);
        }
    }

    pub fn sync_title(&self, title: String) {
        if let Some(mut state) = self.try_decoration_state_mut() {
            state.header.set_title(title);
        }
    }
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

pub fn frame_buffers(window: &GlideWindow, painter: &Painter) -> Vec<DecorationBuffer> {
    let Some(mut state) = window.try_decoration_state_mut() else {
        return Vec::new();
    };
    let header = &mut state.header;

    let content = window.geometry().size;
    if header.width != content.w {
        header.width = content.w;
    }
    if header.content_height != content.h {
        header.content_height = content.h;
    }

    if !header.is_decorated() {
        return Vec::new();
    }

    if let Some(wl_surface) = window.toplevel().map(|t| t.wl_surface().clone()) {
        use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;
        let title = smithay::wayland::compositor::with_states(&wl_surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().ok())
                .and_then(|guard| guard.title.clone())
        });
        if let Some(title) = title {
            header.set_title(title);
        }
    }

    header.redraw(painter);

    let mut out: Vec<DecorationBuffer> = Vec::new();

    for (i, buf) in header.menu_row_bufs.iter().enumerate() {
        let y = header.height * (1 + i32::try_from(i).unwrap_or(0));
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((header.menu_x, y)),
        });
    }

    if let Some(buf) = &header.maximize_buf {
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((0, 0)),
        });
    }

    if let Some(buf) = &header.close_buf {
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((header.width - BUTTON_WIDTH, 0)),
        });
    }

    if let Some(buf) = &header.chevron_buf {
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((header.width - BUTTON_WIDTH * 2, 0)),
        });
    }

    if let Some(buf) = &header.titlebar_buf {
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((0, 0)),
        });
    }

    out
}

pub(crate) fn border_rects(
    width: i32,
    frame_height: i32,
    border_width: i32,
) -> [(Point<i32, Logical>, Size<i32, Logical>); 4] {
    [
        (
            Point::from((-border_width, -border_width)),
            Size::from((width + border_width * 2, border_width)),
        ),
        (
            Point::from((-border_width, frame_height)),
            Size::from((width + border_width * 2, border_width)),
        ),
        (
            Point::from((-border_width, 0)),
            Size::from((border_width, frame_height)),
        ),
        (
            Point::from((width, 0)),
            Size::from((border_width, frame_height)),
        ),
    ]
}

pub(crate) type ShadowRect = (Point<i32, Logical>, Size<i32, Logical>, f32);

pub(crate) fn shadow_rects(
    width: i32,
    frame_height: i32,
    shadow_size: i32,
    bands: usize,
) -> Vec<ShadowRect> {
    if shadow_size <= 0 || bands == 0 || width <= 0 || frame_height <= 0 {
        return Vec::new();
    }

    let bands_i32 = i32::try_from(bands).unwrap_or(1).max(1);
    let step = (shadow_size / bands_i32).max(1);

    let mut out = Vec::with_capacity(bands * 4);
    let mut outset = 0;
    for i in 0..bands {
        #[allow(clippy::cast_precision_loss)]
        let t = (i as f64 + 0.5) / bands as f64;
        let falloff = 1.0 - t;
        #[allow(clippy::cast_possible_truncation)]
        let alpha = (falloff * falloff) as f32;

        let cur_w = width + outset * 2;
        let cur_h = frame_height + outset * 2;
        for (offset, size) in border_rects(cur_w, cur_h, step) {
            let adjusted = Point::from((offset.x - outset, offset.y - outset));
            out.push((adjusted, size, alpha));
        }
        outset += step;
    }
    out
}

pub fn border_strips(window: &GlideWindow) -> Vec<BorderStrip> {
    let Some(mut state) = window.try_decoration_state_mut() else {
        return Vec::new();
    };
    if !state.header.is_decorated() {
        return Vec::new();
    }

    let width = state.header.width;
    let frame_height = state.header.height + state.header.content_height;
    let border_width = i32::try_from(state.header.theme.border_width)
        .unwrap_or(RESIZE_BORDER)
        .clamp(0, RESIZE_BORDER);
    if border_width == 0 || width <= 0 || frame_height <= 0 {
        return Vec::new();
    }

    let color = if state.header.focused {
        state.header.theme.border_focused
    } else {
        state.header.theme.border
    }
    .0;

    let [top, bottom, left, right] = border_rects(width, frame_height, border_width);
    let border = &mut state.border;
    let mut out = Vec::with_capacity(4);

    border.top.update(top.1, color);
    out.push(BorderStrip {
        buf: border.top.clone(),
        offset: top.0,
    });

    border.bottom.update(bottom.1, color);
    out.push(BorderStrip {
        buf: border.bottom.clone(),
        offset: bottom.0,
    });

    border.left.update(left.1, color);
    out.push(BorderStrip {
        buf: border.left.clone(),
        offset: left.0,
    });

    border.right.update(right.1, color);
    out.push(BorderStrip {
        buf: border.right.clone(),
        offset: right.0,
    });

    out
}

pub fn shadow_strips(window: &GlideWindow) -> Vec<BorderStrip> {
    let Some(mut state) = window.try_decoration_state_mut() else {
        return Vec::new();
    };
    if !state.header.is_decorated() || !state.header.theme.shadow {
        return Vec::new();
    }

    let width = state.header.width;
    let frame_height = state.header.height + state.header.content_height;
    let shadow_size = i32::try_from(state.header.theme.shadow_size).unwrap_or(0);
    let rects = shadow_rects(width, frame_height, shadow_size, SHADOW_BANDS);
    if rects.is_empty() {
        return Vec::new();
    }

    let base_color = state.header.theme.shadow_color.0;
    state
        .shadow
        .rings
        .resize_with(rects.len(), SolidColorBuffer::default);

    let mut out = Vec::with_capacity(rects.len());
    for ((offset, size, ring_alpha), buf) in rects.into_iter().zip(state.shadow.rings.iter_mut()) {
        let mut color = base_color;
        color[3] *= ring_alpha;
        buf.update(size, color);
        out.push(BorderStrip {
            buf: buf.clone(),
            offset,
        });
    }

    out
}

use smithay::utils::Serial;

#[cfg(test)]
mod tests {
    use smithay::utils::SERIAL_COUNTER;

    use super::*;

    fn header() -> HeaderBar {
        HeaderBar {
            height: 32,
            width: 800,
            content_height: 600,
            menu_x: 700,
            menu_width: 100,
            ..HeaderBar::default()
        }
    }

    fn point(x: f64, y: f64) -> Point<f64, Logical> {
        Point::from((x, y))
    }

    fn press() -> ButtonEvent {
        ButtonEvent {
            serial: SERIAL_COUNTER.next_serial(),
            time: 0,
            button: 0x110,
            state: ButtonState::Pressed,
        }
    }

    fn release() -> ButtonEvent {
        ButtonEvent {
            serial: SERIAL_COUNTER.next_serial(),
            time: 0,
            button: 0x110,
            state: ButtonState::Released,
        }
    }

    mod scale {
        use super::*;

        #[test]
        fn effective_scale_defaults_to_one_when_never_synced() {
            let h = HeaderBar::default();
            assert_eq!(h.effective_scale(), 1);
        }

        #[test]
        fn effective_scale_reflects_a_synced_value() {
            let mut h = header();
            h.sync(&DecorationTheme::default(), false, false, 2);
            assert_eq!(h.effective_scale(), 2);
        }

        #[test]
        fn sync_invalidates_cached_buffers_when_scale_changes() {
            let mut h = header();
            h.titlebar_buf = Some(MemoryRenderBuffer::default());
            h.maximize_buf = Some(MemoryRenderBuffer::default());
            h.chevron_buf = Some(MemoryRenderBuffer::default());
            h.close_buf = Some(MemoryRenderBuffer::default());
            h.menu_row_bufs.push(MemoryRenderBuffer::default());

            h.sync(&DecorationTheme::default(), false, false, 2);

            assert!(h.titlebar_buf.is_none());
            assert!(h.maximize_buf.is_none());
            assert!(h.chevron_buf.is_none());
            assert!(h.close_buf.is_none());
            assert!(h.menu_row_bufs.is_empty());
        }

        #[test]
        fn sync_keeps_cached_buffers_when_scale_is_unchanged() {
            let mut h = header();
            h.sync(&DecorationTheme::default(), false, false, 1);
            h.titlebar_buf = Some(MemoryRenderBuffer::default());

            h.sync(&DecorationTheme::default(), true, false, 1);

            assert!(
                h.titlebar_buf.is_some(),
                "an unrelated sync must not drop cached buffers"
            );
        }
    }

    #[allow(clippy::float_cmp)]
    mod hover_animation {
        use super::*;

        #[test]
        fn nothing_hovered_is_always_settled() {
            let mut h = header();
            assert!(!h.is_hover_animating());
            assert_eq!(h.button_hover_progress(None), 1.0);
            assert!(!h.is_hover_animating());
        }

        #[test]
        fn hovering_a_zone_starts_the_fade_in_below_full_progress() {
            let mut h = header();
            let t = h.button_hover_progress(Some(HitZone::Maximize));
            assert!(
                (0.0..1.0).contains(&t),
                "expected a fresh hover to be mid fade-in, got {t}"
            );
            assert!(h.is_hover_animating());
        }

        #[test]
        fn progress_settles_to_one_after_the_animation_duration() {
            let mut h = header();
            h.button_hover_progress(Some(HitZone::Close));
            std::thread::sleep(BUTTON_HOVER_ANIM + std::time::Duration::from_millis(30));
            assert_eq!(h.button_hover_progress(Some(HitZone::Close)), 1.0);
            assert!(!h.is_hover_animating());
        }

        #[test]
        fn switching_the_hovered_zone_restarts_the_fade_with_no_crossfade() {
            let mut h = header();
            h.button_hover_progress(Some(HitZone::Maximize));
            std::thread::sleep(BUTTON_HOVER_ANIM + std::time::Duration::from_millis(30));
            assert_eq!(h.button_hover_progress(Some(HitZone::Maximize)), 1.0);

            let t = h.button_hover_progress(Some(HitZone::Close));
            assert!(
                (0.0..1.0).contains(&t),
                "a different target must restart the fade, got {t}"
            );
        }

        #[test]
        fn hover_leaving_every_button_stops_the_animation_immediately() {
            let mut h = header();
            h.button_hover_progress(Some(HitZone::Chevron));
            assert!(h.is_hover_animating());

            assert_eq!(h.button_hover_progress(None), 1.0);
            assert!(!h.is_hover_animating());
        }
    }

    mod shadow_margin_tests {
        use super::*;

        #[test]
        fn matches_the_theme_shadow_size_when_enabled() {
            let mut h = header();
            let theme = DecorationTheme {
                shadow: true,
                shadow_size: 20,
                ..DecorationTheme::default()
            };
            h.sync(&theme, false, false, 1);
            assert_eq!(h.shadow_margin(), 20);
        }

        #[test]
        fn is_zero_when_shadows_are_disabled() {
            let mut h = header();
            let theme = DecorationTheme {
                shadow: false,
                shadow_size: 20,
                ..DecorationTheme::default()
            };
            h.sync(&theme, false, false, 1);
            assert_eq!(h.shadow_margin(), 0);
        }
    }

    mod row_corners_tests {
        use super::*;

        #[test]
        fn single_row_rounds_all_four_corners() {
            assert_eq!(row_corners(0, 1), [true, true, true, true]);
        }

        #[test]
        fn first_row_of_many_rounds_only_the_top() {
            assert_eq!(row_corners(0, 3), [true, true, false, false]);
        }

        #[test]
        fn last_row_of_many_rounds_only_the_bottom() {
            assert_eq!(row_corners(2, 3), [false, false, true, true]);
        }

        #[test]
        fn middle_row_rounds_nothing() {
            assert_eq!(row_corners(1, 3), [false, false, false, false]);
        }
    }

    mod contains {
        use super::*;

        #[test]
        fn titlebar_strip_is_decoration() {
            assert!(header().contains(point(400.0, 16.0)));
        }

        #[test]
        fn content_area_is_not_decoration() {
            assert!(!header().contains(point(400.0, 300.0)));
        }

        #[test]
        fn resize_band_around_the_frame_is_decoration() {
            let h = header();
            assert!(h.contains(point(-4.0, 300.0)), "left band");
            assert!(h.contains(point(804.0, 300.0)), "right band");
            assert!(h.contains(point(400.0, -4.0)), "top band");
            assert!(h.contains(point(400.0, 636.0)), "bottom band");
        }

        #[test]
        fn beyond_the_band_is_not_decoration() {
            let h = header();
            assert!(!h.contains(point(-9.0, 300.0)));
            assert!(!h.contains(point(400.0, 641.0)));
        }

        #[test]
        fn menu_rows_count_only_while_open() {
            let mut h = header();
            assert!(!h.contains(point(750.0, 48.0)));
            h.menu_open = true;
            assert!(h.contains(point(750.0, 48.0)));
        }

        #[test]
        fn menu_open_does_not_claim_content_outside_the_panel_column() {
            let mut h = header();
            h.menu_open = true;
            assert!(!h.contains(point(400.0, 48.0)), "left of the panel");
        }

        #[test]
        fn undecorated_still_owns_its_resize_band() {
            let h = HeaderBar {
                height: 0,
                width: 800,
                content_height: 600,
                ..HeaderBar::default()
            };
            assert!(
                !h.contains(point(400.0, 16.0)),
                "content belongs to the client"
            );
            assert!(h.contains(point(-4.0, 300.0)), "left band");
            assert!(h.contains(point(804.0, 300.0)), "right band");
            assert!(h.contains(point(400.0, -4.0)), "top band");
            assert!(h.contains(point(400.0, 604.0)), "bottom band");
            assert!(!h.contains(point(-9.0, 300.0)), "beyond the band");
        }

        #[test]
        fn a_window_with_no_known_frame_contains_nothing() {
            let h = HeaderBar::default();
            assert!(!h.contains(point(0.0, 0.0)));
            assert!(!h.contains(point(-4.0, -4.0)));
        }
    }

    mod resize_edges {
        use super::*;

        #[test]
        fn each_side_maps_to_its_edge() {
            let h = header();
            assert_eq!(
                h.resize_edges_at(point(-4.0, 300.0)),
                Some(ResizeEdge::LEFT)
            );
            assert_eq!(
                h.resize_edges_at(point(804.0, 300.0)),
                Some(ResizeEdge::RIGHT)
            );
            assert_eq!(h.resize_edges_at(point(400.0, -4.0)), Some(ResizeEdge::TOP));
            assert_eq!(
                h.resize_edges_at(point(400.0, 636.0)),
                Some(ResizeEdge::BOTTOM)
            );
        }

        #[test]
        fn corners_combine_edges() {
            let h = header();
            assert_eq!(
                h.resize_edges_at(point(-4.0, -4.0)),
                Some(ResizeEdge::TOP_LEFT)
            );
            assert_eq!(
                h.resize_edges_at(point(804.0, 636.0)),
                Some(ResizeEdge::BOTTOM_RIGHT)
            );
        }

        #[test]
        fn inside_the_frame_is_never_a_resize() {
            let h = header();
            assert_eq!(h.resize_edges_at(point(400.0, 16.0)), None, "titlebar");
            assert_eq!(h.resize_edges_at(point(400.0, 300.0)), None, "content");
        }
    }

    mod border_rects {
        use super::*;

        #[test]
        fn strips_tile_the_frame_without_overlapping_corners() {
            let [top, bottom, left, right] = border_rects(800, 632, 2);

            assert_eq!(top.0, Point::from((-2, -2)));
            assert_eq!(top.1, Size::from((804, 2)));

            assert_eq!(bottom.0, Point::from((-2, 632)));
            assert_eq!(bottom.1, Size::from((804, 2)));

            assert_eq!(left.0, Point::from((-2, 0)));
            assert_eq!(left.1, Size::from((2, 632)));
            assert_eq!(right.0, Point::from((800, 0)));
            assert_eq!(right.1, Size::from((2, 632)));
        }

        #[test]
        fn at_max_clamped_width_stays_within_the_resize_band_margin() {
            let [top, _bottom, left, right] = border_rects(800, 632, RESIZE_BORDER);
            assert_eq!(top.0.y, -RESIZE_BORDER);
            assert_eq!(left.0.x, -RESIZE_BORDER);
            assert_eq!(right.0.x + RESIZE_BORDER, 800 + RESIZE_BORDER);
        }

        #[test]
        fn zero_width_produces_zero_sized_strips_not_negative_ones() {
            let [top, ..] = border_rects(800, 632, 0);
            assert_eq!(top.1, Size::from((800, 0)));
        }
    }

    mod shadow_rects_tests {
        use super::*;

        #[test]
        fn produces_four_strips_per_band() {
            let rects = shadow_rects(800, 632, 16, SHADOW_BANDS);
            assert_eq!(rects.len(), SHADOW_BANDS * 4);
        }

        #[test]
        fn bands_tile_outward_from_the_frame_without_gaps_or_overlap() {
            let rects = shadow_rects(800, 632, 18, 6);
            let mut top_ys: Vec<i32> = rects
                .iter()
                .step_by(4)
                .map(|(offset, _, _)| offset.y)
                .collect();
            top_ys.sort_unstable();
            for pair in top_ys.windows(2) {
                assert_eq!(
                    pair[0] + 3,
                    pair[1],
                    "gap or overlap between adjacent bands: {top_ys:?}"
                );
            }
            assert_eq!(*top_ys.first().unwrap(), -18);
        }

        #[test]
        fn alpha_decreases_monotonically_from_the_frame_outward() {
            let rects = shadow_rects(800, 632, 24, SHADOW_BANDS);
            let alphas: Vec<f32> = rects.iter().step_by(4).map(|(_, _, a)| *a).collect();
            for pair in alphas.windows(2) {
                assert!(
                    pair[0] > pair[1],
                    "alpha must strictly decrease band by band: {alphas:?}"
                );
            }
        }

        #[test]
        fn outermost_band_alpha_is_small_but_nonzero() {
            let rects = shadow_rects(800, 632, 24, SHADOW_BANDS);
            let last_alpha = rects.last().unwrap().2;
            assert!(
                last_alpha > 0.0,
                "outermost band must not be fully invisible"
            );
            assert!(last_alpha < 0.1, "outermost band must be faint");
        }

        #[test]
        fn zero_shadow_size_produces_no_strips() {
            assert!(shadow_rects(800, 632, 0, SHADOW_BANDS).is_empty());
        }

        #[test]
        fn zero_bands_produces_no_strips() {
            assert!(shadow_rects(800, 632, 16, 0).is_empty());
        }

        #[test]
        fn degenerate_frame_size_produces_no_strips() {
            assert!(shadow_rects(0, 632, 16, SHADOW_BANDS).is_empty());
            assert!(shadow_rects(800, 0, 16, SHADOW_BANDS).is_empty());
        }
    }

    mod zone_at {
        use super::*;

        #[test]
        fn maximize_is_alone_on_the_left_chevron_and_close_cluster_on_the_right() {
            let h = header();
            assert_eq!(h.zone_at(16.0), HitZone::Maximize);
            assert_eq!(h.zone_at(400.0), HitZone::Drag);
            assert_eq!(h.zone_at(750.0), HitZone::Chevron);
            assert_eq!(h.zone_at(780.0), HitZone::Close);
        }

        #[test]
        fn zone_boundaries_are_exact() {
            let h = header();
            assert_eq!(h.zone_at(31.999), HitZone::Maximize);
            assert_eq!(h.zone_at(32.0), HitZone::Drag);
            assert_eq!(h.zone_at(735.999), HitZone::Drag);
            assert_eq!(h.zone_at(736.0), HitZone::Chevron);
            assert_eq!(h.zone_at(767.999), HitZone::Chevron);
            assert_eq!(h.zone_at(768.0), HitZone::Close);
        }
    }

    mod menu {
        use super::*;

        #[test]
        fn row_zero_starts_below_the_strip() {
            let mut h = header();
            h.menu_open = true;

            assert_eq!(h.menu_item_at(point(750.0, 16.0)), None, "titlebar strip");
            assert_eq!(h.menu_item_at(point(750.0, 48.0)), Some(0), "first row");
            assert_eq!(h.menu_item_at(point(750.0, 80.0)), None, "past the rows");
        }

        #[test]
        fn closed_menu_has_no_rows() {
            assert_eq!(header().menu_item_at(point(750.0, 48.0)), None);
        }

        #[test]
        fn x_outside_the_panel_column_is_never_a_row() {
            let mut h = header();
            h.menu_open = true;
            assert_eq!(
                h.menu_item_at(point(400.0, 48.0)),
                None,
                "left of the compact panel"
            );
        }
    }

    mod on_button {
        use super::*;

        #[test]
        fn press_on_the_plain_strip_starts_a_move() {
            let mut h = header();
            h.pointer_enter(point(400.0, 16.0));

            assert_eq!(h.on_button(&press()), Some(DecorationAction::StartMove));
        }

        #[test]
        fn press_on_a_button_zone_does_not_move() {
            let mut h = header();
            h.pointer_enter(point(16.0, 16.0));

            assert_eq!(h.on_button(&press()), None);
        }

        #[test]
        fn press_on_a_button_zone_marks_it_pressed() {
            let mut h = header();
            h.pointer_enter(point(16.0, 16.0));

            h.on_button(&press());
            assert_eq!(h.pressed, Some(HitZone::Maximize));
        }

        #[test]
        fn release_clears_the_pressed_zone_regardless_of_where_it_lands() {
            let mut h = header();
            h.pointer_enter(point(16.0, 16.0));
            h.on_button(&press());
            assert_eq!(h.pressed, Some(HitZone::Maximize));

            h.pointer_enter(point(400.0, 16.0));
            h.on_button(&release());
            assert_eq!(h.pressed, None);
        }

        #[test]
        fn pointer_leave_clears_the_pressed_zone() {
            let mut h = header();
            h.pointer_enter(point(790.0, 16.0));
            h.on_button(&press());
            assert_eq!(h.pressed, Some(HitZone::Close));

            h.pointer_leave();
            assert_eq!(h.pressed, None);
        }

        #[test]
        fn press_in_the_band_starts_a_resize() {
            let mut h = header();
            h.pointer_enter(point(-4.0, -4.0));

            assert_eq!(
                h.on_button(&press()),
                Some(DecorationAction::StartResize(ResizeEdge::TOP_LEFT))
            );
        }

        #[test]
        fn release_on_maximize_toggles() {
            let mut h = header();
            h.pointer_enter(point(16.0, 16.0));

            assert_eq!(
                h.on_button(&release()),
                Some(DecorationAction::ToggleMaximize)
            );
        }

        #[test]
        fn release_on_close_closes() {
            let mut h = header();
            h.pointer_enter(point(790.0, 16.0));

            assert_eq!(h.on_button(&release()), Some(DecorationAction::Close));
        }

        #[test]
        fn chevron_opens_the_menu_then_an_item_click_runs_it() {
            let mut h = header();

            h.pointer_enter(point(750.0, 16.0));
            assert_eq!(h.on_button(&release()), None);
            assert!(h.menu_open);

            h.pointer_enter(point(750.0, 48.0));
            assert_eq!(
                h.on_button(&release()),
                Some(DecorationAction::Run(MenuItem::Minimize))
            );
            assert!(!h.menu_open, "any release closes the menu");
        }

        #[test]
        fn release_outside_the_open_menu_just_closes_it() {
            let mut h = header();
            h.menu_open = true;
            h.pointer_enter(point(400.0, 16.0));

            assert_eq!(h.on_button(&release()), None);
            assert!(!h.menu_open);
        }

        #[test]
        fn release_in_the_band_after_a_resize_is_not_a_button_click() {
            let mut h = header();
            h.pointer_enter(point(-4.0, -4.0));

            assert_eq!(h.on_button(&release()), None);
        }
    }

    mod cursors {
        use super::*;

        #[test]
        fn every_edge_gets_a_matching_arrow() {
            assert_eq!(resize_cursor(ResizeEdge::TOP), CursorIcon::NResize);
            assert_eq!(resize_cursor(ResizeEdge::BOTTOM), CursorIcon::SResize);
            assert_eq!(resize_cursor(ResizeEdge::LEFT), CursorIcon::WResize);
            assert_eq!(resize_cursor(ResizeEdge::RIGHT), CursorIcon::EResize);
            assert_eq!(resize_cursor(ResizeEdge::TOP_LEFT), CursorIcon::NwResize);
            assert_eq!(resize_cursor(ResizeEdge::TOP_RIGHT), CursorIcon::NeResize);
            assert_eq!(resize_cursor(ResizeEdge::BOTTOM_LEFT), CursorIcon::SwResize);
            assert_eq!(
                resize_cursor(ResizeEdge::BOTTOM_RIGHT),
                CursorIcon::SeResize
            );
        }

        #[test]
        fn test_refcell_try_borrow_safety() {
            let cell = RefCell::new(DecorationState::default());
            {
                let mut mut_borrow = cell.borrow_mut();
                mut_borrow.header.height = 40;
                assert!(
                    cell.try_borrow().is_err(),
                    "try_borrow while mutably borrowed returns Err, avoiding panic"
                );
                assert!(
                    cell.try_borrow_mut().is_err(),
                    "try_borrow_mut while mutably borrowed returns Err, avoiding panic"
                );
            }
            assert!(cell.try_borrow().is_ok());
            assert_eq!(cell.borrow().header.height, 40);
        }
    }
}

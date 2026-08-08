//! Server-side window decorations: a titlebar with a fixed button layout
//! and a chevron-triggered menu for actions that don't get their own
//! button.
//!
//! # Layout (fixed by design — see [`crate::config::DecorationTheme`])
//!
//! ```text
//! ┌───┬────────────────────────────┬───┬───┐
//! │ M │ (title)                    │ v │ X │
//! └───┴────────────────────────────┴───┴───┘
//! ```
//! Maximize (M) is always leftmost, on its own — deliberately separated
//! from close, so a slip while reaching for one can never land on the
//! other. Close (X) is always rightmost; the chevron (v) — which opens a
//! dropdown for actions that don't warrant their own button, currently
//! just Minimize — sits immediately to close's left, so the two read as
//! one cluster. Everything between maximize and the chevron is drag
//! territory, including the title itself (aligned per
//! [`crate::config::TitleAlign`], left by default): pressing there and
//! moving starts an interactive move, exactly as if the client itself had
//! requested one.
//!
//! The chevron's dropdown is a compact panel sized to its widest item, not
//! a full-width strip — it hangs from the right edge of the titlebar,
//! under the chevron/close cluster it belongs to, rather than spanning
//! content that has nothing to do with it.
//!
//! Every window is decorated for now; respecting a client's own
//! `xdg-decoration` preference is M4.5.
//!
//! # Border
//!
//! Every window also gets a thin outline drawn just outside its frame —
//! see [`border_strips`] — the only focus indicator that isn't just the
//! titlebar's own colour, which matters once an undecorated window (no
//! titlebar at all) exists.

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

/// Width of the maximize/close/chevron buttons. Height is the titlebar
/// height itself, from [`DecorationTheme::height`].
pub const BUTTON_WIDTH: i32 = 32;

/// Width of the right-hand cluster (chevron + close).
const RIGHT_CLUSTER_WIDTH: i32 = BUTTON_WIDTH * 2;

/// Width of the invisible resize band around a decorated window's frame.
///
/// Sized for touchpads: the band is the only way to start a resize with the
/// pointer (there is no visible border to aim for), so it errs on the
/// generous side rather than demanding pixel-accurate aim at a window edge.
pub const RESIZE_BORDER: i32 = 8;

/// Left inset for the title label, so it doesn't start flush against the
/// frame's left edge.
const TITLE_PADDING: i32 = 12;

/// Horizontal padding inside each compact menu row, both sides.
const MENU_PADDING: i32 = 12;

/// Minimum menu width, so a single short label doesn't render a comically
/// narrow dropdown.
const MENU_MIN_WIDTH: i32 = 120;

/// How much darker a button's hover colour gets while actually pressed —
/// derived from the hover colour rather than a dedicated theme key per
/// button per interaction state.
const PRESSED_DARKEN: f32 = 0.8;

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

    pub fn label(self) -> &'static str {
        match self {
            Self::Minimize => "Minimize",
        }
    }
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
    pub border: BorderState,
}

/// A painted element: a pixel buffer plus its position relative to the
/// decoration's own origin (titlebar top-left).  Returned by
/// [`frame_buffers`] and converted to GPU-uploaded render elements in
/// `window.rs`.
pub struct DecorationBuffer {
    pub buf: MemoryRenderBuffer,
    /// Logical offset from the titlebar top-left.
    pub offset: Point<i32, Logical>,
}

/// The four border strips (top, bottom, left, right) framing a window's
/// whole frame — see the module docs. Persisted per-window (like
/// [`HeaderBar`]'s buffers) so their [`SolidColorBuffer`] `Id`s stay
/// stable across frames for damage tracking, rather than being
/// reallocated — and treated as new by the damage tracker — every redraw.
#[derive(Debug, Default)]
pub struct BorderState {
    top: SolidColorBuffer,
    bottom: SolidColorBuffer,
    left: SolidColorBuffer,
    right: SolidColorBuffer,
}

/// One border strip: a solid-colour buffer plus its position relative to
/// the decoration's own origin (titlebar top-left) — the same convention
/// [`DecorationBuffer::offset`] uses. Returned by [`border_strips`].
pub struct BorderStrip {
    pub buf: SolidColorBuffer,
    pub offset: Point<i32, Logical>,
}

#[derive(Debug, Default)]
pub struct HeaderBar {
    /// 0 means undecorated: no titlebar is drawn or reserved. Set once from
    /// [`DecorationTheme::height`] when a window is created.
    height: i32,
    width: i32,
    /// Height of the client's own content area below the titlebar. Needed
    /// to know where the window's bottom edge — and with it the bottom
    /// resize band — actually is.
    content_height: i32,
    /// Window title, used for the label in the drag zone.
    title: String,
    pointer_loc: Option<Point<f64, Logical>>,
    menu_open: bool,
    menu_hover: Option<usize>,
    /// Left edge and width of the compact dropdown panel, in decoration-
    /// origin coordinates. Computed and cached during [`Self::redraw`]
    /// (which has a [`Painter`] to measure label widths with) and read
    /// back by hit-testing (`menu_item_at`, `contains`), which don't.
    menu_x: i32,
    menu_width: i32,
    /// The button zone currently held down, if any — separate from
    /// `menu_hover`/hover tracking, which reflect where the pointer *is*,
    /// not whether the button is being pressed. Cleared on release
    /// (wherever it lands, matching how the rest of the titlebar treats
    /// clicks) and on the pointer leaving the decoration entirely.
    pressed: Option<HitZone>,

    /// Colors and focus, snapshotted here (rather than read live from
    /// `GlideState`) because [`smithay::backend::renderer::element::AsRenderElements`],
    /// where `redraw` ultimately gets called from, has no access to
    /// compositor state — only to the renderer. Kept in sync by
    /// [`crate::GlideState::reconcile_space`] via [`GlideWindow::sync_decoration`].
    /// Hover is unaffected: that already lives entirely in this struct,
    /// updated straight from pointer motion.
    theme: DecorationTheme,
    focused: bool,
    /// Whether the window is currently maximized — swaps the maximize
    /// button's glyph for the restore one, same idea as `theme`/`focused`:
    /// snapshotted ahead of render for the same reason.
    maximized: bool,

    /// Cached pixel buffers for each titlebar element.  Rebuilt whenever
    /// their appearance changes (hover, focus, open menu, resize).
    titlebar_buf: Option<MemoryRenderBuffer>,
    maximize_buf: Option<MemoryRenderBuffer>,
    chevron_buf: Option<MemoryRenderBuffer>,
    close_buf: Option<MemoryRenderBuffer>,
    /// One buffer per menu row, rebuilt when the menu opens.
    menu_row_bufs: Vec<MemoryRenderBuffer>,
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
    StartResize(ResizeEdge),
}

/// The pointer shape to show over a resize band, so the user can see what
/// dragging there will do before they press.
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

/// Repaints one titlebar button into its cached buffer.
///
/// The buttons differ only in their colours and glyph, so they share this
/// instead of a near-identical block per button. `bg`/`fg` are straight
/// colour — see [`Painter::draw_icon_maximize`]'s docs on why no
/// premultiplication happens at this boundary.
fn redraw_button(
    buf: &mut Option<MemoryRenderBuffer>,
    height: i32,
    bg: Color,
    fg: Color,
    icon: fn(&mut [u8], i32, i32, Color, Color),
) {
    let w = BUTTON_WIDTH;
    let buf = buf.get_or_insert_with(|| {
        MemoryRenderBuffer::new(Fourcc::Argb8888, (w, height), 1, Transform::Normal, None)
    });
    let mut ctx = buf.render();
    ctx.resize((w, height));
    let _ = ctx.draw(|pixels| {
        icon(pixels, w, height, fg, bg);
        Ok::<_, ()>(vec![Rectangle::from_size((w, height).into())])
    });
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

    /// Snapshots the theme, focus, and maximized state to redraw with. See
    /// the fields' own docs for why this happens ahead of time rather than
    /// at render.
    fn sync(&mut self, theme: &DecorationTheme, focused: bool, maximized: bool) {
        if &self.theme != theme {
            self.theme = theme.clone();
        }
        self.focused = focused;
        self.maximized = maximized;
    }

    /// Update the window title stored for rendering in the drag zone.
    pub fn set_title(&mut self, title: String) {
        if self.title != title {
            self.title = title;
            // Invalidate the titlebar buffer so it gets repainted.
            self.titlebar_buf = None;
        }
    }

    /// Which zone `x` falls in. Maximize is on its own at the frame's left
    /// edge — deliberately separated from close, so a slip while reaching
    /// for one can never land on the other; chevron and close cluster at
    /// the right edge. Everything between them is drag territory.
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
    /// The decoration is everything of the frame that is not the client's
    /// content: the titlebar strip, the invisible resize band around the
    /// whole frame, and — when open — the chevron dropdown, which extends
    /// *below* the strip over the content. Each part matters: a strip-only
    /// test routed clicks on the menu straight to the client underneath,
    /// and without the band a resize could only start from a client-side
    /// `resize_request`, which nothing without CSD ever sends.
    ///
    /// The dropdown is compact, not full-width (see the module docs), so
    /// only its own `[menu_x, menu_x + menu_width)` column counts as ours
    /// while it's open — everywhere else in the content area still belongs
    /// to the client, exactly as when the menu is closed.
    pub fn contains(&self, rel: Point<f64, Logical>) -> bool {
        if !self.is_decorated() {
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

    /// Which resize edges `rel` falls on, if it is in the resize band.
    ///
    /// Strict quadrants: a corner is exactly the overlap of two bands, with
    /// no extra corner-widening. `RESIZE_BORDER` is generous enough that
    /// the overlap square is comfortably hittable.
    fn resize_edges_at(&self, rel: Point<f64, Logical>) -> Option<ResizeEdge> {
        if !self.is_decorated() {
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

    /// Menu row index under `loc` (decoration-origin coordinates), if the
    /// menu is open and `loc` is inside it.
    ///
    /// Row 0 starts *below* the titlebar strip — at `y == height`, not at
    /// `y == 0` — hence the subtraction: without it, hovering the titlebar
    /// itself lit up row 0 and the real row 0 counted as row 1.
    ///
    /// `x` is bounded by the compact panel's own `[menu_x, menu_x +
    /// menu_width)`, not the full titlebar width — see `contains`'s docs.
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
        // Dropping the pressed-visual here (rather than only on release)
        // matches common toolkit behaviour: holding a button and dragging
        // off it un-highlights it, even though the eventual release —
        // wherever it lands — is still what decides the actual action, per
        // `on_button`'s own docs. This is a deliberate simplification: a
        // real "drag off cancels, drag back re-arms" state machine is more
        // than this titlebar warrants right now.
        self.pressed = None;
    }

    /// Decides what a button press/release on the decoration should do,
    /// mutating only the header's own state (menu open/closed, pressed
    /// visual).
    ///
    /// Deliberately has no access to `GlideState`: it runs under the
    /// decoration `RefCell` borrow, and anything that reaches
    /// `reconcile_space` from here would re-borrow it and panic. The caller
    /// executes the returned action after releasing the borrow.
    ///
    /// Only release triggers an action (matching how the rest of Glide
    /// treats clicks); press alone starts a move when it lands on the plain
    /// background, since a drag by definition begins while the button is
    /// still held, and marks whichever button zone it landed on as
    /// pressed, for `redraw` to shade darker.
    fn on_button(&mut self, event: &ButtonEvent) -> Option<DecorationAction> {
        let loc = self.pointer_loc?;

        if event.state == ButtonState::Pressed {
            // A press with the menu open is handled entirely on release,
            // same as everywhere else in the titlebar.
            if self.menu_open {
                return None;
            }
            // The resize band wins over the titlebar zones: it extends
            // *outside* the frame where no zone exists anyway, so the only
            // overlap to resolve is precedence, and a press in the band
            // unambiguously means resize.
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

        // Any release — wherever it lands — ends whatever was pressed.
        self.pressed = None;

        if self.menu_open {
            let item = self.menu_item_at(loc).and_then(|i| MenuItem::ALL.get(i));
            let action = item.map(|&item| DecorationAction::Run(item));
            // Any release closes the menu, whether it hit an item or not.
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

    /// Whether `loc` is within the titlebar strip itself, as opposed to the
    /// resize band or the open menu. The button zones (`zone_at`) only make
    /// sense inside the strip: without this check, releasing a resize drag
    /// that started at the top-left corner counted as a maximize click.
    fn in_titlebar_strip(&self, loc: Point<f64, Logical>) -> bool {
        loc.x >= 0.0
            && loc.x < f64::from(self.width)
            && loc.y >= 0.0
            && loc.y < f64::from(self.height)
    }

    /// Redraws every `MemoryRenderBuffer` that needs updating.
    ///
    /// Buffers are only recreated when their content actually changes (hover
    /// state, focus, open menu, window size, title) — so this is cheap to
    /// call every frame.
    // Over pedantic's line-count threshold: this is one visual routine —
    // titlebar background/title, the three buttons, the dropdown — sharing
    // `painter`/`theme`/`hovered`/`pressed`. Splitting it up would mean
    // passing all of those between the pieces instead of just reading them
    // from the enclosing scope, for no reduction in what a reader has to
    // hold in their head at once.
    #[allow(clippy::too_many_lines)]
    fn redraw(&mut self, painter: &Painter) {
        if self.width == 0 || self.height == 0 {
            return;
        }

        let theme = &self.theme;
        let height = self.height;
        let bg_color = if self.focused {
            theme.background_focused
        } else {
            theme.background
        };
        let bg = bg_color.to_buffer_bytes();

        // Determine which zone is hovered/pressed (strip only — not the
        // resize band). `pressed` always implies `hovered` in practice
        // (the pointer is still down on whatever it was pressed on, or has
        // left and cleared `pressed` via `pointer_leave`), but each is
        // tracked independently and read independently below.
        let hovered = self
            .pointer_loc
            .filter(|l| self.in_titlebar_strip(*l))
            .map(|l| self.zone_at(l.x));
        let pressed = self.pressed;

        // ── Titlebar background + title label ─────────────────────────────
        // Repaint whenever width, height, focus, title, or hover changes. We
        // always repaint here for simplicity; the damage system inside
        // MemoryRenderBuffer will only upload the changed pixels to the GPU.
        {
            let w = self.width;
            let h = self.height;
            let buf = self.titlebar_buf.get_or_insert_with(|| {
                MemoryRenderBuffer::new(Fourcc::Argb8888, (w, h), 1, Transform::Normal, None)
            });
            // Grow if needed.
            let mut ctx = buf.render();
            ctx.resize((w, h));
            let title = self.title.clone();
            let align = match theme.title_align {
                TitleAlign::Left => TextAlign::Left,
                TitleAlign::Center => TextAlign::Center,
            };
            let _ = ctx.draw(|pixels| {
                let fg = [255, 255, 255, bg[3]]; // white text, same alpha as bg
                // Clears maximize on the left, the chevron/close cluster
                // on the right.
                let x_min = BUTTON_WIDTH + TITLE_PADDING;
                let x_max = w - RIGHT_CLUSTER_WIDTH;
                painter.draw_label(pixels, w, h, x_min, x_max, align, &title, fg, bg);
                Ok::<_, ()>(vec![Rectangle::from_size((w, h).into())])
            });
        }

        // ── Titlebar buttons ───────────────────────────────────────────────
        // Foregrounds differ slightly per icon: the close glyph is the one
        // meant to stand out, the chevron the least.
        // Maximize/restore are the same button showing different glyphs —
        // whichever one is current communicates what *pressing* it will do
        // next, not the window's present state.
        let maximize_icon: fn(&mut [u8], i32, i32, Color, Color) = if self.maximized {
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
                Painter::draw_icon_chevron as fn(&mut [u8], i32, i32, Color, Color),
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
                hover_bg
            } else {
                normal
            };
            redraw_button(buf, height, btn_bg, fg, icon);
        }

        // ── Dropdown menu ──────────────────────────────────────────────────
        if self.menu_open {
            let row_h = self.height;

            // Compact panel: sized to the widest item (plus padding), never
            // narrower than MENU_MIN_WIDTH nor wider than the titlebar
            // itself, and right-aligned so its right edge lines up with the
            // button cluster's — "hanging from the chevron", per the module
            // docs, without needing per-row width bookkeeping.
            // Menu labels are a handful of short English words; their
            // measured width is nowhere near i32's range.
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

            self.menu_row_bufs
                .resize_with(MenuItem::ALL.len(), MemoryRenderBuffer::default);
            let menu_hover = self.menu_hover;
            let menu_bg = theme.menu_background.to_buffer_bytes();
            let menu_hover_bg = theme.menu_item_hover.to_buffer_bytes();
            // Off-white / pure-white text, opaque: B, G, R, A. (Previously
            // written as [255, 230, 230, 230] under the mistaken belief
            // that this function's colour bytes were [A, R, G, B] — that
            // put 255 in the *blue* channel and made every non-hovered
            // menu row's text render with a faint blue tint instead of
            // neutral grey.)
            let font_fg = [230u8, 230, 230, 255];
            let font_fg_hover = [255u8, 255, 255, 255];
            for (i, buf) in self.menu_row_bufs.iter_mut().enumerate() {
                let is_hovered = menu_hover == Some(i);
                let bg = if is_hovered { menu_hover_bg } else { menu_bg };
                let fg = if is_hovered { font_fg_hover } else { font_fg };
                let label = MenuItem::ALL
                    .get(i)
                    .map_or("", |item| item.label())
                    .to_owned();
                let mut ctx = buf.render();
                ctx.resize((row_w, row_h));
                let _ = ctx.draw(|pixels| {
                    painter.draw_label(
                        pixels,
                        row_w,
                        row_h,
                        MENU_PADDING,
                        row_w - MENU_PADDING,
                        TextAlign::Left,
                        &label,
                        fg,
                        bg,
                    );
                    Ok::<_, ()>(vec![Rectangle::from_size((row_w, row_h).into())])
                });
            }
        } else {
            self.menu_row_bufs.clear();
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

/// Starts an interactive resize from the resize band, exactly as if the
/// client had called `xdg_toplevel.resize`. Deferred via `insert_idle` for
/// the same grab-clobbering reason as [`start_move`] — see its docs.
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
        // The same content-area rectangle a client-initiated resize uses
        // (see `resize_request` in handlers/xdg_shell.rs): the grab talks
        // to the client in configure sizes, which are content sizes.
        let initial_rect = smithay::utils::Rectangle::new(location, window.geometry().size);

        let grab = ResizeSurfaceGrab::start(start_data, window.clone(), edges, initial_rect);
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

    /// Sets the titlebar height (0 disables the decoration entirely) and
    /// snapshots theme/focus/maximized for the next redraw. Called from
    /// [`crate::GlideState::reconcile_space`], once per window per
    /// reconcile — cheap, and it is what keeps decoration rendering in sync
    /// with configuration reloads, focus changes, and maximize/restore
    /// toggling despite
    /// [`smithay::backend::renderer::element::AsRenderElements`] having no
    /// access to compositor state of its own.
    ///
    /// Width is synced here too, not only lazily at render time: `zone_at`
    /// measures the close button from the right edge, so hit-testing a
    /// click that somehow arrives before the first render would otherwise
    /// run against `width == 0` and classify most of the bar as Close.
    pub fn sync_decoration(
        &self,
        theme: &DecorationTheme,
        height: i32,
        focused: bool,
        maximized: bool,
    ) {
        let content = self.geometry().size;
        if let Some(mut state) = self.try_decoration_state_mut() {
            state.header.set_height(height);
            state.header.width = content.w;
            state.header.content_height = content.h;
            state.header.sync(theme, focused, maximized);
        }
    }

    /// Update the window title stored for rendering in the drag zone.
    pub fn sync_title(&self, title: String) {
        if let Some(mut state) = self.try_decoration_state_mut() {
            state.header.set_title(title);
        }
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

impl Decoration {
    /// Shared enter/motion handling: track hover, and set the pointer shape.
    ///
    /// The decoration is compositor territory: whatever cursor the client
    /// last set via `wl_pointer.set_cursor` stops applying the moment the
    /// pointer crosses onto it. Over the resize band that shape is the
    /// matching resize arrow, so the user can see what a drag would do
    /// before pressing; everywhere else it is the plain default.
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
        // The decision is made under the decoration borrow; the temporary
        // `RefMut` ends with this statement. That ordering is load-bearing:
        // the actions below reach `reconcile_space`, which re-borrows this
        // same `RefCell` via `sync_decoration`, and executing them while
        // still borrowed panicked with `BorrowMutError` on the first
        // maximize-button click.
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

/// Returns the painted pixel buffers for one window's decoration: titlebar
/// background (with title), maximize, chevron, close, and — when open —
/// the dropdown rows.
///
/// Each entry is `(buffer, logical_offset_from_titlebar_origin)`.
/// The caller (see `window.rs`) uploads them to the GPU and assembles them
/// into render elements.
///
/// This function takes a renderer reference so that `MemoryRenderBuffer`
/// can upload changed pixels on-demand.  The buffers themselves live in
/// `HeaderBar` so they persist across frames for damage-tracking.
pub fn frame_buffers(window: &GlideWindow, painter: &Painter) -> Vec<DecorationBuffer> {
    let Some(mut state) = window.try_decoration_state_mut() else {
        return Vec::new();
    };
    let header = &mut state.header;
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

    let content_width = window.geometry().size.w;
    if header.width != content_width {
        header.width = content_width;
    }
    header.redraw(painter);

    let mut out: Vec<DecorationBuffer> = Vec::new();

    // ── Titlebar background (behind buttons, so listed last — later in the
    // front-to-back list means further back in z-order).
    // We push buttons first, then the bar, mirroring the previous ordering.
    // ──────────────────────────────────────────────────────────────────────

    // Menu rows (topmost when open) — under the compact panel's own
    // left edge (`menu_x`), not the titlebar's, per the module docs.
    for (i, buf) in header.menu_row_bufs.iter().enumerate() {
        let y = header.height * (1 + i32::try_from(i).unwrap_or(0));
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((header.menu_x, y)),
        });
    }

    // Maximize, alone at the left edge.
    if let Some(buf) = &header.maximize_buf {
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((0, 0)),
        });
    }

    // Chevron/close cluster, right to left, at the right edge.
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

    // Titlebar background (furthest back — last in front-to-back list).
    if let Some(buf) = &header.titlebar_buf {
        out.push(DecorationBuffer {
            buf: buf.clone(),
            offset: Point::from((0, 0)),
        });
    }

    out
}

/// Offset and size of each of the four border strips (top, bottom, left,
/// right) tiling a frame of `width × frame_height`, outset by
/// `border_width` — relative to that frame's own `(0, 0)` origin, so a
/// caller adds its own rectangle's `loc` to place them.
///
/// Pure geometry, no `SolidColorBuffer`/window plumbing — kept separate
/// from [`border_strips`] specifically so it can be unit-tested without a
/// real [`GlideWindow`]. Top and bottom span the full outer width,
/// including corners; left and right fill in only the height between them,
/// so the four strips tile into a frame without overlapping at the
/// corners.
///
/// `pub(crate)`: also used by `overview.rs` to draw the hovered preview's
/// highlight as a real ring instead of a solid slab the size of the whole
/// preview — the exact same tiling problem, just around a different
/// rectangle.
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

/// Returns the four border strips framing one window's whole frame
/// (titlebar + content), or nothing if it isn't decorated or the theme's
/// `border-width` is `0`.
///
/// A separate function from [`frame_buffers`], not folded into it, because
/// border strips are [`SolidColorBuffer`]s — a native Smithay solid-colour
/// element — rather than [`MemoryRenderBuffer`]s; there is no pixel
/// plotting here to route through a [`Painter`] for a flat rectangle, and
/// unlike the titlebar/buttons/menu this doesn't need one.
pub fn border_strips(window: &GlideWindow) -> Vec<BorderStrip> {
    let Some(mut state) = window.try_decoration_state_mut() else {
        return Vec::new();
    };
    if !state.header.is_decorated() {
        return Vec::new();
    }

    let width = state.header.width;
    let frame_height = state.header.height + state.header.content_height;
    // Clamped to RESIZE_BORDER: see `DecorationTheme::border_width`'s docs
    // on why a wider value would draw outside the margin damage tracking
    // is told to cover.
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

// ── Missing import ──────────────────────────────────────────────────────────
use smithay::utils::Serial;

#[cfg(test)]
mod tests {
    use smithay::utils::SERIAL_COUNTER;

    use super::*;

    /// A decorated 800x600-content window with a 32px titlebar.
    ///
    /// `menu_x`/`menu_width` are seeded as `redraw` (which needs a real
    /// [`Painter`] to measure label widths and isn't exercised by these
    /// pure hit-testing tests) would leave them after opening the menu on
    /// an 800-wide bar: a compact panel hanging from the right edge.
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

    mod contains {
        use super::*;

        #[test]
        fn titlebar_strip_is_decoration() {
            assert!(header().contains(point(400.0, 16.0)));
        }

        #[test]
        fn content_area_is_not_decoration() {
            // Clicks there belong to the client, not to us.
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
            // Row 0 lives at y in [height, 2*height) — over the content —
            // and x within the compact panel's own column, not anywhere
            // across the full titlebar width.
            assert!(!h.contains(point(750.0, 48.0)));
            h.menu_open = true;
            assert!(h.contains(point(750.0, 48.0)));
        }

        #[test]
        fn menu_open_does_not_claim_content_outside_the_panel_column() {
            // The whole point of a compact (not full-width) dropdown: the
            // rest of the content area still belongs to the client while
            // it's open.
            let mut h = header();
            h.menu_open = true;
            assert!(!h.contains(point(400.0, 48.0)), "left of the panel");
        }

        #[test]
        fn undecorated_contains_nothing() {
            let h = HeaderBar {
                height: 0,
                width: 800,
                content_height: 600,
                ..HeaderBar::default()
            };
            assert!(!h.contains(point(400.0, 16.0)));
            assert!(!h.contains(point(-4.0, 300.0)));
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

            // Top spans the full outer width, including corners.
            assert_eq!(top.0, Point::from((-2, -2)));
            assert_eq!(top.1, Size::from((804, 2)));

            // Bottom mirrors it at the far edge.
            assert_eq!(bottom.0, Point::from((-2, 632)));
            assert_eq!(bottom.1, Size::from((804, 2)));

            // Left/right fill only the height between top and bottom —
            // not the corners, which the top/bottom strips already cover.
            assert_eq!(left.0, Point::from((-2, 0)));
            assert_eq!(left.1, Size::from((2, 632)));
            assert_eq!(right.0, Point::from((800, 0)));
            assert_eq!(right.1, Size::from((2, 632)));
        }

        #[test]
        fn at_max_clamped_width_stays_within_the_resize_band_margin() {
            // border_strips clamps border_width to RESIZE_BORDER precisely
            // so this holds — see its docs on why that margin matters.
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

    mod zone_at {
        use super::*;

        #[test]
        fn maximize_is_alone_on_the_left_chevron_and_close_cluster_on_the_right() {
            let h = header(); // width 800
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
            // Regression test for the review finding: the divisor version
            // without the height subtraction made the *strip* count as row
            // 0 and the real row 0 count as row 1.
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
            h.pointer_enter(point(16.0, 16.0)); // over maximize

            assert_eq!(h.on_button(&press()), None);
        }

        #[test]
        fn press_on_a_button_zone_marks_it_pressed() {
            let mut h = header();
            h.pointer_enter(point(16.0, 16.0)); // over maximize

            h.on_button(&press());
            assert_eq!(h.pressed, Some(HitZone::Maximize));
        }

        #[test]
        fn release_clears_the_pressed_zone_regardless_of_where_it_lands() {
            let mut h = header();
            h.pointer_enter(point(16.0, 16.0)); // over maximize
            h.on_button(&press());
            assert_eq!(h.pressed, Some(HitZone::Maximize));

            h.pointer_enter(point(400.0, 16.0)); // released elsewhere
            h.on_button(&release());
            assert_eq!(h.pressed, None);
        }

        #[test]
        fn pointer_leave_clears_the_pressed_zone() {
            let mut h = header();
            h.pointer_enter(point(790.0, 16.0)); // over close
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

            h.pointer_enter(point(750.0, 16.0)); // over the chevron
            assert_eq!(h.on_button(&release()), None);
            assert!(h.menu_open);

            h.pointer_enter(point(750.0, 48.0)); // over the Minimize row
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
            h.pointer_enter(point(400.0, 16.0)); // back on the strip

            assert_eq!(h.on_button(&release()), None);
            assert!(!h.menu_open);
        }

        #[test]
        fn release_in_the_band_after_a_resize_is_not_a_button_click() {
            // Regression guard: `zone_at` only means anything inside the
            // strip (`in_titlebar_strip`). A release at the frame's
            // top-left corner is in the resize band — outside the strip on
            // both axes — and must not be classified as landing on the
            // background drag zone or any button.
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

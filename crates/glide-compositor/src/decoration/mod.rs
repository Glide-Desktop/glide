mod header;
mod input;
mod menu;
mod render;

use std::cell::{Ref, RefCell, RefMut};

use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point},
};

use crate::{config::DecorationTheme, window::GlideWindow};

pub use header::HeaderBar;
pub use input::Decoration;
pub use render::{frame_buffers, shadow_and_border_buffer};

#[cfg(test)]
use crate::grabs::resize_grab::ResizeEdge;
#[cfg(test)]
use header::{DecorationAction, HitZone, button_colors};
#[cfg(test)]
use input::resize_cursor;
#[cfg(test)]
use menu::MenuItem;

pub const BUTTON_SIZE: i32 = 26;
pub const BUTTON_PAD_X: i32 = glide_theme::spacing::SP2;
pub const BUTTON_PAD_Y: i32 = 11;

/// Width of the maximize/close *hit* zones, distinct from `BUTTON_SIZE`
/// (their *painted* size). Spans the whole titlebar height and reaches the
/// strip's own left/right edge, per spec §16's 28x28 minimum pointer
/// target — Fitts's-law-friendly, matches how every mainstream desktop
/// treats its corner window controls.
pub const BUTTON_HIT_WIDTH: i32 = 34;

pub const RESIZE_BORDER: i32 = 8;

const TITLE_PADDING: i32 = glide_theme::spacing::SP3;

/// Fixed popover width — spec §12.1's mockup (`.win-context-menu { width:
/// 224px }`). Computed once at open time, not from content, so the menu
/// never has to guess its own size from text it hasn't shaped yet (the
/// bug that used to leave the very first right-click landing flush right
/// regardless of cursor position — see `open_menu_at` in header.rs).
const MENU_WIDTH: i32 = 224;

/// Padding between the popover's own edge and its row stack — spec
/// §11.6/12.1: `.menu { padding: 6px }`.
const MENU_CONTAINER_PADDING: i32 = 6;

/// Row height — spec's mockup renders `.win-context-menu .menu-item` at
/// the compositor's titlebar height today only by coincidence; the design
/// system's own menu rows (`.menu-item`) are 34px, not 48.
const MENU_ROW_HEIGHT: i32 = 34;

const MENU_PADDING: i32 = glide_theme::spacing::SP3;

/// Row radius — spec §7: `--r-2`, the "control" step (the same as a
/// button or input), distinct from the popover's own `--r-3` container
/// radius.
const MENU_CORNER_RADIUS: f32 = glide_theme::shape::R2;

/// Popover container radius — spec §7: `--r-3`, the "container" step
/// (card, menu, window).
const MENU_CONTAINER_RADIUS: f32 = glide_theme::shape::R3;

/// Idle (unfocused) window controls read as this fraction of their normal
/// opacity — spec §12.2/12.3: `.win.idle .wctl button { opacity: .45 }`.
const IDLE_BUTTON_OPACITY: f32 = 0.45;

const BUTTON_HOVER_ANIM: std::time::Duration = glide_theme::motion::T1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BoxDecoKey {
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) scale: i32,
    pub(crate) focused: bool,
    pub(crate) maximized: bool,
    pub(crate) theme_rev: u32,
}

#[derive(Debug)]
pub(crate) struct BoxDecoCache {
    pub(crate) key: BoxDecoKey,
    pub(crate) buf: MemoryRenderBuffer,
}

#[derive(Debug, Default)]
pub struct DecorationState {
    pub header: HeaderBar,
    pub(crate) box_deco: Option<BoxDecoCache>,
}

pub struct DecorationBuffer {
    pub buf: MemoryRenderBuffer,
    pub offset: Point<i32, Logical>,
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
}

#[cfg(test)]
mod tests {
    use smithay::{
        backend::input::ButtonState,
        input::pointer::{ButtonEvent, CursorIcon},
        utils::SERIAL_COUNTER,
    };

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

    fn right_press() -> ButtonEvent {
        ButtonEvent {
            serial: SERIAL_COUNTER.next_serial(),
            time: 0,
            button: 0x111,
            state: ButtonState::Pressed,
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
            h.close_buf = Some(MemoryRenderBuffer::default());
            h.menu_buf = Some(MemoryRenderBuffer::default());

            h.sync(&DecorationTheme::default(), false, false, 2);

            assert!(h.titlebar_buf.is_none());
            assert!(h.maximize_buf.is_none());
            assert!(h.close_buf.is_none());
            assert!(h.menu_buf.is_none());
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
            h.button_hover_progress(Some(HitZone::Maximize));
            assert!(h.is_hover_animating());

            assert_eq!(h.button_hover_progress(None), 1.0);
            assert!(!h.is_hover_animating());
        }
    }

    mod shadow_margin_tests {
        use super::*;

        /// `shadow_margin` must cover the *actual* overhang of the
        /// shadow+border buffer — `shadow_fade_margin(size) + border` —
        /// not just the raw configured size, or `SpaceElement::bbox`
        /// under-reports the decorated footprint and moving a window
        /// leaves stale shadow pixels behind.
        #[test]
        fn covers_the_fade_margin_of_the_larger_shadow_size_plus_the_border() {
            let mut h = header();
            let theme = DecorationTheme {
                shadow: true,
                shadow_size: 20,
                shadow_size_focused: 50,
                border_width: 1,
                ..DecorationTheme::default()
            };
            h.sync(&theme, false, false, 1);
            assert_eq!(h.shadow_margin(), crate::paint::shadow_fade_margin(50) + 1);
            assert!(
                h.shadow_margin() > 50,
                "must exceed the raw configured size — the blur needs room to fade"
            );
        }

        #[test]
        fn only_the_border_remains_when_shadows_are_disabled() {
            let mut h = header();
            let theme = DecorationTheme {
                shadow: false,
                shadow_size: 20,
                shadow_size_focused: 50,
                border_width: 2,
                ..DecorationTheme::default()
            };
            h.sync(&theme, false, false, 1);
            assert_eq!(h.shadow_margin(), 2);
        }
    }

    #[allow(clippy::float_cmp)]
    mod button_colors_tests {
        use super::*;
        use crate::config::Color;

        fn normal() -> Color {
            Color([0.1, 0.1, 0.1, 0.07])
        }
        fn hover_bg() -> Color {
            Color([0.2, 0.4, 0.8, 1.0])
        }
        fn fg() -> Color {
            Color([0.9, 0.9, 0.9, 0.68])
        }
        fn fg_hover() -> Color {
            Color([1.0, 1.0, 1.0, 1.0])
        }

        #[test]
        fn rest_focused_returns_the_colors_unchanged() {
            let (bg, f) = button_colors(
                normal(),
                hover_bg(),
                fg(),
                fg_hover(),
                0.0,
                false,
                false,
                true,
            );
            assert_eq!(bg, normal());
            assert_eq!(f, fg());
        }

        #[test]
        fn full_hover_reaches_exactly_the_hover_color_when_focused() {
            let (bg, f) = button_colors(
                normal(),
                hover_bg(),
                fg(),
                fg_hover(),
                1.0,
                true,
                false,
                true,
            );
            assert_eq!(bg, hover_bg());
            assert_eq!(f, fg_hover());
        }

        #[test]
        fn partial_hover_lies_strictly_between_normal_and_hover() {
            let (bg, _) = button_colors(
                normal(),
                hover_bg(),
                fg(),
                fg_hover(),
                0.5,
                true,
                false,
                true,
            );
            assert!(bg.0[0] > normal().0[0] && bg.0[0] < hover_bg().0[0]);
        }

        #[test]
        fn press_lightens_toward_white_on_top_of_the_resting_color_not_a_separate_darken() {
            let (rest_bg, _) = button_colors(
                normal(),
                hover_bg(),
                fg(),
                fg_hover(),
                0.0,
                false,
                false,
                true,
            );
            let (pressed_bg, _) = button_colors(
                normal(),
                hover_bg(),
                fg(),
                fg_hover(),
                0.0,
                false,
                true,
                true,
            );
            assert!(
                pressed_bg.0[0] > rest_bg.0[0],
                "pressing must lighten toward white (a state layer), not darken"
            );
            let expected = rest_bg.lerp(Color([1.0, 1.0, 1.0, 1.0]), glide_theme::motion::ST_PRESS);
            assert_eq!(pressed_bg, expected);
        }

        #[test]
        fn press_during_an_active_hover_layers_on_top_of_the_hover_color() {
            let (hover_only, _) = button_colors(
                normal(),
                hover_bg(),
                fg(),
                fg_hover(),
                0.7,
                true,
                false,
                true,
            );
            let (hover_and_press, _) = button_colors(
                normal(),
                hover_bg(),
                fg(),
                fg_hover(),
                0.7,
                true,
                true,
                true,
            );
            assert_ne!(
                hover_only, hover_and_press,
                "a press mid-hover must still show the hover color underneath the state layer"
            );
        }

        #[test]
        fn idle_dims_both_background_and_foreground_alpha_by_forty_five_percent() {
            let (bg, f) = button_colors(
                normal(),
                hover_bg(),
                fg(),
                fg_hover(),
                0.0,
                false,
                false,
                false,
            );
            assert!((bg.0[3] - normal().0[3] * 0.45).abs() < 0.001);
            assert!((f.0[3] - fg().0[3] * 0.45).abs() < 0.001);
            assert_eq!(
                [bg.0[0], bg.0[1], bg.0[2]],
                [normal().0[0], normal().0[1], normal().0[2]]
            );
        }

        #[test]
        fn idle_dimming_applies_after_hover_and_press_not_instead_of_them() {
            let (focused_hover, _) = button_colors(
                normal(),
                hover_bg(),
                fg(),
                fg_hover(),
                1.0,
                true,
                false,
                true,
            );
            let (idle_hover, _) = button_colors(
                normal(),
                hover_bg(),
                fg(),
                fg_hover(),
                1.0,
                true,
                false,
                false,
            );
            assert!(
                idle_hover.0[3] < focused_hover.0[3],
                "an idle hovered button must still be dimmer than a focused hovered one"
            );
            assert_eq!(
                [idle_hover.0[0], idle_hover.0[1], idle_hover.0[2]],
                [focused_hover.0[0], focused_hover.0[1], focused_hover.0[2]],
                "dimming must not shift hue, only alpha"
            );
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

    mod zone_at {
        use super::*;

        #[test]
        fn maximize_is_far_left_and_close_is_far_right() {
            let h = header();
            assert_eq!(h.zone_at(16.0, 16.0), HitZone::Maximize);
            assert_eq!(h.zone_at(400.0, 16.0), HitZone::Drag);
            assert_eq!(h.zone_at(780.0, 16.0), HitZone::Close);
        }

        #[test]
        fn zone_boundaries_are_exact() {
            // The hit zone (BUTTON_HIT_WIDTH = 34) is wider than the painted
            // button (BUTTON_SIZE = 26) and reaches the strip's own edge —
            // see `BUTTON_HIT_WIDTH`'s doc comment in mod.rs.
            let h = header();
            assert_eq!(h.zone_at(0.0, 16.0), HitZone::Maximize);
            assert_eq!(h.zone_at(33.999, 16.0), HitZone::Maximize);
            assert_eq!(h.zone_at(34.0, 16.0), HitZone::Drag);
            assert_eq!(h.zone_at(765.999, 16.0), HitZone::Drag);
            assert_eq!(h.zone_at(766.0, 16.0), HitZone::Close);
            assert_eq!(h.zone_at(799.999, 16.0), HitZone::Close);
        }

        #[test]
        fn button_hit_zones_span_the_full_titlebar_height() {
            // Unlike the painted button (vertically centred via
            // BUTTON_PAD_Y), the hit zone covers the whole strip height —
            // there is no dead vertical band above or below it to fall
            // through to Drag.
            let h = header();
            assert_eq!(h.zone_at(16.0, 0.0), HitZone::Maximize, "top of the strip");
            assert_eq!(
                h.zone_at(16.0, f64::from(h.height) - 0.001),
                HitZone::Maximize,
                "bottom of the strip"
            );
        }

        #[test]
        fn outside_the_titlebar_height_is_not_a_button_zone() {
            let h = header();
            assert_eq!(
                h.zone_at(16.0, f64::from(h.height)),
                HitZone::Drag,
                "nothing matches at or past the strip's own bottom edge; zone_at falls back to \
                 Drag by default rather than a button — see the `contains` tests for what \
                 actually happens outside the frame"
            );
        }
    }

    mod menu {
        use super::*;

        #[test]
        fn row_zero_starts_below_the_strip() {
            // header() fixture: height=32, menu_x=700, menu_width=100.
            // Rows start at height + MENU_CONTAINER_PADDING (32+6=38) and
            // are MENU_ROW_HEIGHT (34) tall each: row0 [38,72), row1
            // [72,106), row2 [106,140), then nothing.
            let mut h = header();
            h.menu_open = true;

            assert_eq!(h.menu_item_at(point(750.0, 16.0)), None, "titlebar strip");
            assert_eq!(h.menu_item_at(point(750.0, 40.0)), Some(0), "first row");
            assert_eq!(h.menu_item_at(point(750.0, 80.0)), Some(1), "second row");
            assert_eq!(h.menu_item_at(point(750.0, 120.0)), Some(2), "third row");
            assert_eq!(h.menu_item_at(point(750.0, 145.0)), None, "past the rows");
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
        fn right_click_on_the_strip_opens_the_menu_then_a_row_release_runs_it() {
            let mut h = header();

            h.pointer_enter(point(400.0, 16.0));
            assert_eq!(h.on_button(&right_press()), None);
            assert!(h.menu_open);

            h.pointer_enter(point(400.0, 48.0));
            assert_eq!(
                h.on_button(&release()),
                Some(DecorationAction::Run(MenuItem::Minimize))
            );
            assert!(!h.menu_open, "any release closes the menu");
        }

        #[test]
        fn a_second_press_while_the_menu_is_open_is_ignored() {
            let mut h = header();
            h.menu_open = true;
            h.pointer_enter(point(400.0, 16.0));

            assert_eq!(h.on_button(&press()), None);
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

    mod redraw_dirty_tracking {
        use super::*;
        use crate::paint::Painter;

        fn painter() -> Painter {
            Painter::new(14.0)
        }

        #[test]
        fn first_redraw_is_dirty_and_a_repeat_with_no_changes_is_not() {
            let mut h = header();
            let p = painter();
            assert!(h.redraw(&p), "the first draw must always happen");
            assert!(
                !h.redraw(&p),
                "nothing changed since the last draw, must skip re-rasterizing"
            );
        }

        #[test]
        fn a_zero_sized_header_never_reports_dirty() {
            let mut h = HeaderBar {
                width: 0,
                height: 0,
                ..HeaderBar::default()
            };
            let p = painter();
            assert!(!h.redraw(&p));
            assert!(!h.redraw(&p));
        }

        #[test]
        fn changing_the_title_marks_it_dirty_again() {
            let mut h = header();
            let p = painter();
            assert!(h.redraw(&p));
            assert!(!h.redraw(&p));

            h.set_title("A different title".to_owned());
            assert!(h.redraw(&p), "a changed title must trigger a redraw");
            assert!(!h.redraw(&p));
        }

        #[test]
        fn setting_the_same_title_again_does_not_dirty_it() {
            let mut h = header();
            let p = painter();
            assert!(h.redraw(&p));
            h.set_title(String::new());
            assert!(
                !h.redraw(&p),
                "the title was already empty; re-setting it to the same value is a no-op"
            );
        }

        #[test]
        fn focus_change_marks_it_dirty_again() {
            let mut h = header();
            let p = painter();
            assert!(h.redraw(&p));
            assert!(!h.redraw(&p));

            h.sync(&DecorationTheme::default(), true, false, h.scale.max(1));
            assert!(h.redraw(&p), "focus flipped, must redraw");
        }

        #[test]
        fn maximized_change_marks_it_dirty_again() {
            let mut h = header();
            let p = painter();
            assert!(h.redraw(&p));

            h.sync(&DecorationTheme::default(), h.focused, true, h.scale.max(1));
            assert!(h.redraw(&p), "maximize toggled, must redraw");
        }

        #[test]
        fn a_changed_theme_marks_it_dirty_again() {
            let mut h = header();
            let p = painter();
            assert!(h.redraw(&p));
            assert!(!h.redraw(&p));

            let theme = DecorationTheme {
                background: crate::config::Color([1.0, 0.0, 0.0, 1.0]),
                ..DecorationTheme::default()
            };
            h.sync(&theme, h.focused, h.maximized, h.scale.max(1));
            assert!(h.redraw(&p), "a theme change must trigger a redraw");
        }

        #[test]
        fn hover_animation_stays_dirty_until_it_settles_then_stops() {
            let mut h = header();
            let p = painter();
            h.pointer_enter(point(16.0, 16.0));

            assert!(h.redraw(&p), "first draw with a fresh hover fade-in");

            std::thread::sleep(BUTTON_HOVER_ANIM + std::time::Duration::from_millis(30));
            assert!(
                h.redraw(&p),
                "the fade progressed since the last draw, must redraw once more"
            );
            assert!(
                !h.redraw(&p),
                "the hover animation has settled at 1.0, nothing left to draw"
            );
        }

        #[test]
        fn opening_the_menu_marks_it_dirty_and_so_does_closing_it() {
            let mut h = header();
            let p = painter();
            assert!(h.redraw(&p));
            assert!(!h.redraw(&p));

            h.menu_open = true;
            assert!(h.redraw(&p), "opening the menu must redraw");
            assert!(!h.redraw(&p));

            h.menu_open = false;
            assert!(h.redraw(&p), "closing the menu must redraw");
        }
    }
}

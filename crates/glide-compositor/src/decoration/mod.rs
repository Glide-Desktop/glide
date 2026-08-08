mod header;
mod input;
mod menu;
mod render;

use std::cell::{Ref, RefCell, RefMut};

use smithay::{
    backend::renderer::element::{memory::MemoryRenderBuffer, solid::SolidColorBuffer},
    utils::{Logical, Point},
};

use crate::{config::DecorationTheme, window::GlideWindow};

pub use header::HeaderBar;
pub use input::Decoration;
pub use render::{border_strips, frame_buffers, shadow_strips};

pub(crate) use render::{border_rects, shadow_rects};

#[cfg(test)]
use crate::grabs::resize_grab::ResizeEdge;
#[cfg(test)]
use header::{DecorationAction, HitZone, row_corners};
#[cfg(test)]
use input::resize_cursor;
#[cfg(test)]
use menu::MenuItem;

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
    pub(crate) top: SolidColorBuffer,
    pub(crate) bottom: SolidColorBuffer,
    pub(crate) left: SolidColorBuffer,
    pub(crate) right: SolidColorBuffer,
}

pub struct BorderStrip {
    pub buf: SolidColorBuffer,
    pub offset: Point<i32, Logical>,
}

#[derive(Debug, Default)]
pub struct ShadowState {
    pub(crate) rings: Vec<SolidColorBuffer>,
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

#[cfg(test)]
mod tests {
    use smithay::{
        backend::input::ButtonState,
        input::pointer::{ButtonEvent, CursorIcon},
        utils::{SERIAL_COUNTER, Size},
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

    mod border_rects_tests {
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

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
pub(crate) use input::resize_cursor;
pub use render::{frame_buffers, shadow_and_border_buffer, shadow_and_border_fade};

#[cfg(test)]
use crate::grabs::resize_grab::ResizeEdge;
#[cfg(test)]
use header::{DecorationAction, HitZone, button_colors};
#[cfg(test)]
use menu::MenuItem;

pub const BUTTON_SIZE: i32 = 26;
pub const BUTTON_PAD_X: i32 = glide_theme::spacing::SP2;
pub const BUTTON_PAD_Y: i32 = 11;

pub const BUTTON_HIT_WIDTH: i32 = 34;

pub const RESIZE_BORDER: i32 = 8;

const TITLE_PADDING: i32 = glide_theme::spacing::SP3;

const MENU_WIDTH: i32 = 224;

const MENU_CONTAINER_PADDING: i32 = 6;

const MENU_ROW_HEIGHT: i32 = 34;

const MENU_PADDING: i32 = glide_theme::spacing::SP3;

const MENU_CORNER_RADIUS: f32 = glide_theme::shape::R2;

const MENU_CONTAINER_RADIUS: f32 = glide_theme::shape::R3;

const IDLE_BUTTON_OPACITY: f32 = 0.45;

pub(crate) const DOUBLE_CLICK_MS: u32 = 400;
pub(crate) const DOUBLE_CLICK_SLOP: f64 = 6.0;

pub(crate) fn state_layer_anim() -> std::time::Duration {
    glide_theme::motion::t1()
}

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

#[derive(Debug)]
pub(crate) struct BoxDecoFade {
    pub(crate) bufs: std::collections::HashMap<i32, MemoryRenderBuffer>,
    pub(crate) focused: bool,
    pub(crate) level: glide_ui::Tween,
}

pub(crate) fn focus_fade() -> std::time::Duration {
    glide_theme::motion::t3()
}

fn focus_fade_ease(t: f64) -> f64 {
    glide_theme::motion::E_IO.ease_f64(t)
}

impl BoxDecoFade {
    pub(crate) fn start(from: f64, focused: bool, scale: i32, buf: MemoryRenderBuffer) -> Self {
        let mut level = glide_ui::Tween::at(from.clamp(0.0, 1.0));
        level.retarget(0.0, focus_fade(), focus_fade_ease);
        Self { bufs: std::collections::HashMap::from([(scale, buf)]), focused, level }
    }

    pub(crate) fn alpha(&self) -> Option<f32> {
        let value = self.level.value(focus_fade(), focus_fade_ease);
        (value > 0.0).then_some(value as f32)
    }
}

#[derive(Debug, Default)]
pub struct DecorationState {
    pub header: HeaderBar,
    pub(crate) box_deco: std::collections::HashMap<i32, BoxDecoCache>,
    pub(crate) box_deco_fade: Option<BoxDecoFade>,
}

pub struct DecorationBuffer {
    pub buf: MemoryRenderBuffer,
    pub offset: Point<i32, Logical>,
    pub alpha: f32,
    pub scale: Option<f64>,
    pub overlay: bool,
}

impl DecorationBuffer {
    fn plain(buf: MemoryRenderBuffer, offset: Point<i32, Logical>) -> Self {
        Self { buf, offset, alpha: 1.0, scale: None, overlay: false }
    }
}

pub(crate) fn menu_open_anim() -> std::time::Duration {
    glide_theme::motion::t2()
}

pub(crate) const MENU_OPEN_SCALE: f64 = 0.94;
pub(crate) const MENU_OPEN_RISE: f64 = -6.0;

impl GlideWindow {
    fn decoration_cell(&self) -> &RefCell<DecorationState> {
        self.user_data().insert_if_missing(|| RefCell::new(DecorationState::default()));
        self.user_data().get::<RefCell<DecorationState>>().expect("inserted unconditionally on the line above")
    }

    pub fn try_decoration_state(&self) -> Option<Ref<'_, DecorationState>> {
        self.decoration_cell().try_borrow().ok()
    }

    pub fn try_decoration_state_mut(&self) -> Option<RefMut<'_, DecorationState>> {
        self.decoration_cell().try_borrow_mut().ok()
    }

    pub fn set_dragging(&self, dragging: bool) {
        if let Some(mut state) = self.try_decoration_state_mut() {
            state.header.dragging = dragging;
        }
    }

    pub fn sync_decoration(&self, theme: &DecorationTheme, height: i32, focused: bool, maximized: bool) {
        let content = self.geometry().size;
        if let Some(mut state) = self.try_decoration_state_mut() {
            state.header.set_height(height);
            state.header.width = content.w;
            state.header.content_height = content.h;
            state.header.sync(theme, focused, maximized);
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
    use DecorationAction::{Close, StartMove, StartResize, ToggleMaximize};

    const ON_MAXIMIZE: (f64, f64) = (16.0, 16.0);
    const ON_CLOSE: (f64, f64) = (790.0, 16.0);
    const ON_DRAG: (f64, f64) = (400.0, 16.0);
    const ON_MENU_ROW: (f64, f64) = (400.0, 48.0);

    const _: () = assert!(MENU_OPEN_SCALE < 1.0 && MENU_OPEN_RISE < 0.0);

    fn header() -> HeaderBar {
        HeaderBar { height: 32, width: 800, content_height: 600, menu_x: 700, menu_width: 100, ..HeaderBar::default() }
    }

    fn open_header() -> HeaderBar {
        HeaderBar { menu: glide_ui::Reveal::open_now(), ..header() }
    }

    fn point(x: f64, y: f64) -> Point<f64, Logical> {
        Point::from((x, y))
    }

    fn button(button: u32, state: ButtonState, time: u32) -> ButtonEvent {
        ButtonEvent {
            serial: SERIAL_COUNTER.next_serial(),
            time: smithay::backend::input::InputTime::from_millis(time),
            button,
            state,
        }
    }

    fn press() -> ButtonEvent {
        button(0x110, ButtonState::Pressed, 0)
    }

    fn release() -> ButtonEvent {
        button(0x110, ButtonState::Released, 0)
    }

    fn right_press() -> ButtonEvent {
        button(0x111, ButtonState::Pressed, 0)
    }

    fn right_release() -> ButtonEvent {
        button(0x111, ButtonState::Released, 0)
    }

    fn at(h: &mut HeaderBar, (x, y): (f64, f64), event: &ButtonEvent) -> Option<DecorationAction> {
        h.pointer_enter(point(x, y));
        h.on_button(event)
    }

    fn painter() -> crate::paint::Painter {
        crate::paint::Painter::new(14.0, None)
    }

    fn settle_layer() {
        std::thread::sleep(state_layer_anim() + std::time::Duration::from_millis(30));
    }

    fn settle_menu() {
        std::thread::sleep(menu_open_anim() + std::time::Duration::from_millis(30));
    }

    fn mid(value: f64) -> bool {
        (0.0..1.0).contains(&value)
    }

    #[test]
    fn reversing_focus_mid_fade_continues_from_where_it_was() {
        let fade = focus_fade();
        if fade.is_zero() {
            return;
        }
        let mut h = header();
        for focused in [false, true, false] {
            h.sync(&DecorationTheme::default(), focused, false);
        }
        let started = h.focus_fade_started.expect("a fade is running");
        assert!(started.elapsed() + fade / 10 >= fade);
    }

    #[test]
    fn each_output_scale_keeps_its_own_raster_through_a_sync() {
        let (mut h, p) = (header(), painter());
        assert!(h.redraw(&p, 1));
        assert!(h.redraw(&p, 2), "a new scale gets its own raster");
        assert!(!h.redraw(&p, 1) && !h.redraw(&p, 2));
        for scale in [1, 2] {
            let b = h.buffers_at(scale).expect("rasterised");
            assert!(b.titlebar.is_some() && b.maximize.is_some() && b.close.is_some());
        }
        h.sync(&DecorationTheme::default(), true, false);
        assert!(h.buffers_at(2).is_some_and(|b| b.titlebar.is_some()));

        let mut zero = header();
        assert!(zero.redraw(&p, 0));
        assert!(!zero.redraw(&p, 1), "a non-positive scale is drawn at one");
        assert!(zero.buffers_at(0).is_some());
    }

    mod box_deco_fade {
        use super::*;

        fn cache(focused: bool, scale: i32) -> BoxDecoCache {
            BoxDecoCache { key: key(focused, scale), buf: MemoryRenderBuffer::default() }
        }

        fn key(focused: bool, scale: i32) -> BoxDecoKey {
            BoxDecoKey { width: 800, height: 632, scale, focused, maximized: false, theme_rev: 0 }
        }

        fn alpha(fade: Option<&BoxDecoFade>) -> f32 {
            fade.and_then(BoxDecoFade::alpha).unwrap_or(0.0)
        }

        #[test]
        fn fades_start_full_join_new_scales_and_reverse_from_the_visible_mix() {
            if focus_fade().is_zero() {
                return;
            }
            let fresh = render::next_fade(None, Some(&cache(false, 1)), key(true, 1));
            assert!(alpha(fresh.as_ref()) > 0.95);

            let first = render::next_fade(None, Some(&cache(false, 1)), key(true, 1));
            std::thread::sleep(focus_fade() / 2);
            let before = alpha(first.as_ref());
            assert!((0.05..0.95).contains(&before), "mid-fade: {before}");
            let joined = render::next_fade(first, Some(&cache(false, 2)), key(true, 2)).expect("still fading");
            assert!(joined.bufs.contains_key(&1) && joined.bufs.contains_key(&2));
            assert!((alpha(Some(&joined)) - before).abs() < 0.05);

            let reversed = render::next_fade(Some(joined), Some(&cache(true, 1)), key(false, 1));
            assert!((alpha(reversed.as_ref()) - (1.0 - before)).abs() < 0.05);
        }

        #[test]
        fn a_first_draw_at_a_new_scale_keeps_the_fade_but_a_resize_drops_it() {
            let first = render::next_fade(None, Some(&cache(false, 1)), key(true, 1));
            let kept = render::next_fade(first, None, key(true, 2));
            assert!(kept.as_ref().is_some_and(|fade| fade.bufs.contains_key(&1)));
            let resized = BoxDecoKey { width: 900, ..key(false, 1) };
            assert!(render::next_fade(kept, Some(&cache(true, 1)), resized).is_none());
        }
    }

    #[test]
    fn only_input_on_a_button_arms_the_t1_hover_fade() {
        assert_eq!(state_layer_anim(), glide_theme::motion::t1());
        assert!(state_layer_anim() < menu_open_anim());
        let (mut h, p) = (header(), painter());
        assert_eq!(h.button_hover_progress(), 1.0);
        h.redraw(&p, 1);
        assert!(!h.is_hover_animating(), "drawing never starts an animation");
        h.pointer_enter(point(ON_DRAG.0, ON_DRAG.1));
        assert_eq!(h.hovered_zone(), None);
        assert!(!h.is_hover_animating(), "the drag strip is not a hover target");

        h.pointer_enter(point(ON_MAXIMIZE.0, ON_MAXIMIZE.1));
        assert!(mid(h.button_hover_progress()) && h.is_hover_animating());
        settle_layer();
        assert_eq!(h.button_hover_progress(), 1.0);
        assert!(!h.is_hover_animating());

        h.pointer_enter(point(ON_CLOSE.0, ON_CLOSE.1));
        assert!(mid(h.button_hover_progress()), "a new target restarts the fade");
        h.pointer_leave();
        assert_eq!(h.button_hover_progress(), 1.0);
        assert!(!h.is_hover_animating());
    }

    mod menu_open_animation {
        use super::*;

        fn open(h: &mut HeaderBar) {
            at(h, ON_DRAG, &right_press());
        }

        fn close(h: &mut HeaderBar) {
            at(h, ON_DRAG, &release());
        }

        #[test]
        fn the_menu_fades_in_and_out_over_t2_and_drops_its_raster_afterwards() {
            assert_eq!(menu_open_anim(), glide_theme::motion::t2());
            let (mut h, p) = (header(), painter());
            let menu_drawn = |h: &HeaderBar| h.buffers_at(1).is_some_and(|b| b.menu.is_some());
            assert!(!h.menu.is_open() && !h.is_menu_visible() && !h.is_menu_animating());
            assert!(h.menu_open_progress().abs() < f64::EPSILON);

            open(&mut h);
            assert!(h.menu.is_open() && h.is_menu_visible());
            assert!(mid(h.menu_open_progress()));
            assert!(h.is_menu_animating() && h.is_hover_animating());
            settle_menu();
            assert!((h.menu_open_progress() - 1.0).abs() < f64::EPSILON);
            assert!(!h.is_menu_animating() && h.menu.is_open());
            h.redraw(&p, 1);
            assert!(menu_drawn(&h));

            close(&mut h);
            assert!(!h.menu.is_open() && h.is_menu_visible());
            assert!(mid(h.menu_open_progress()));
            assert!(h.is_menu_animating() && h.is_hover_animating());
            h.redraw(&p, 1);
            assert!(menu_drawn(&h), "still drawn while fading out");
            settle_menu();
            assert!(!h.is_menu_visible() && !h.is_menu_animating());
            assert!(h.menu_open_progress().abs() < f64::EPSILON);
            h.redraw(&p, 1);
            assert!(!menu_drawn(&h));

            open(&mut h);
            assert!(h.menu_open_progress() < 1.0, "a second open animates too");
        }

        #[test]
        fn reversing_mid_transition_continues_from_the_current_progress() {
            let mut h = header();
            open(&mut h);
            std::thread::sleep(std::time::Duration::from_millis(60));
            let before = h.menu_open_progress();
            assert!(mid(before), "still opening: {before}");
            close(&mut h);
            assert!((before - h.menu_open_progress()).abs() < 0.05);
            assert!(!h.menu.is_open() && h.is_menu_visible() && h.is_menu_animating());

            settle_menu();
            open(&mut h);
            settle_menu();
            close(&mut h);
            std::thread::sleep(std::time::Duration::from_millis(60));
            let before = h.menu_open_progress();
            assert!(mid(before), "still closing: {before}");
            open(&mut h);
            assert!((before - h.menu_open_progress()).abs() < 0.05);
            assert!(h.menu.is_open());
        }

        #[test]
        fn a_closed_menu_stops_taking_clicks_immediately_even_while_fading() {
            let mut h = header();
            open(&mut h);
            settle_menu();
            let row = point(ON_MENU_ROW.0, ON_MENU_ROW.1);
            h.pointer_enter(row);
            assert_eq!(h.menu_item_at(row), Some(0));
            assert_eq!(h.on_button(&release()), Some(DecorationAction::Run(MenuItem::Minimize)));
            assert!(h.is_menu_visible(), "the panel is still fading out");
            assert_eq!(h.menu_item_at(row), None);
            assert!(!h.contains(row), "a fading menu must not claim the content");
            h.pointer_enter(row);
            assert_eq!(h.menu_hover, None);
            assert_eq!(h.on_button(&press()), None);
            assert_eq!(h.on_button(&release()), None);
        }

        #[test]
        fn on_a_narrow_window_the_panel_raster_and_hit_area_are_one_rect() {
            let narrow = || HeaderBar { height: 32, width: 120, content_height: 600, ..HeaderBar::default() };
            let mut h = narrow();
            at(&mut h, (60.0, 16.0), &right_press());
            let panel = h.menu_panel_rect();
            assert_eq!((panel.size.width, panel.loc.x), (120, 0));
            assert!(h.contains(point(119.0, 48.0)));
            assert_eq!(h.menu_item_at(point(119.0, 48.0)), None, "container padding");
            assert_eq!(h.menu_item_at(point(100.0, 48.0)), Some(0));
            assert!(!h.contains(point(60.0, 32.0 + f64::from(panel.size.height))));

            let mut h = HeaderBar { menu: glide_ui::Reveal::open_now(), menu_width: 120, ..narrow() };
            assert!(h.redraw(&painter(), 2));
            let mut menu = h.buffers_at(2).and_then(|b| b.menu.clone()).expect("drawn");
            let mut bytes = 0;
            let mut ctx = menu.render();
            let _ = ctx.draw(|pixels| {
                bytes = pixels.len();
                Ok::<_, ()>(Vec::new())
            });
            drop(ctx);
            let panel = h.menu_panel_rect();
            assert_eq!(bytes, usize::try_from(panel.size.width * 2 * panel.size.height * 2 * 4).unwrap());
        }
    }

    #[test]
    fn menu_row_highlights_crossfade_over_t1_and_are_forgotten_once_faded() {
        let (row0, row1) = (point(750.0, 48.0), point(750.0, 80.0));
        let mut h = open_header();
        h.pointer_enter(row0);
        assert_eq!(h.menu_hover, Some(0));
        assert!(mid(h.menu_row_alpha(0)));
        assert!(h.is_menu_row_fading() && h.is_hover_animating());
        settle_layer();
        assert!((h.menu_row_alpha(0) - 1.0).abs() < f64::EPSILON);
        assert!(!h.is_menu_row_fading());

        h.pointer_enter(row1);
        let (outgoing, incoming) = (h.menu_row_alpha(0), h.menu_row_alpha(1));
        assert!(mid(outgoing) && outgoing > 0.5, "old row still lit: {outgoing}");
        assert!(mid(incoming) && incoming < 0.5, "new row fading in: {incoming}");
        settle_layer();
        assert!(h.menu_row_alpha(0).abs() < f64::EPSILON);
        assert!((h.menu_row_alpha(1) - 1.0).abs() < f64::EPSILON);

        h.pointer_leave();
        assert_eq!(h.menu_hover, None);
        assert!(mid(h.menu_row_alpha(1)) && h.is_menu_row_fading());
        settle_layer();
        h.pointer_enter(row0);
        assert!(h.menu_row_alpha(1).abs() < f64::EPSILON);
        assert!(h.menu_row_fade.iter().all(|(row, _)| *row != 1));

        let (mut h, p) = (open_header(), painter());
        assert!(h.redraw(&p, 1) && !h.redraw(&p, 1));
        h.pointer_enter(row0);
        assert!(h.redraw(&p, 1), "a fresh hover redraws");
        settle_layer();
        assert!(h.redraw(&p, 1), "the fade progressed, redraw once more");
        assert!(!h.redraw(&p, 1));
    }

    #[test]
    fn a_press_lights_instantly_and_fades_out_on_release_or_leave() {
        let mut h = header();
        at(&mut h, ON_MAXIMIZE, &press());
        assert!((h.press_layer_progress() - 1.0).abs() < f64::EPSILON);
        assert!(!h.is_press_fading(), "on is instant");
        h.on_button(&release());
        assert_eq!(h.pressed, None);
        assert!(mid(h.press_layer_progress()));
        assert!(h.is_press_fading() && h.is_hover_animating());
        settle_layer();
        assert!(h.press_layer_progress().abs() < f64::EPSILON && !h.is_press_fading());

        at(&mut h, ON_CLOSE, &press());
        h.pointer_leave();
        assert_eq!(h.pressed, None);
        assert!(h.is_press_fading());

        let (mut h, p) = (header(), painter());
        at(&mut h, ON_CLOSE, &press());
        at(&mut h, ON_DRAG, &release());
        assert!(h.redraw(&p, 1));
        assert_eq!(h.press_zone, Some(HitZone::Close), "the fade stays on its zone");
        settle_layer();
        assert!(h.redraw(&p, 1) && !h.redraw(&p, 1));
    }

    #[test]
    fn only_a_quick_close_second_press_on_the_strip_is_a_double_click() {
        let presses = |h: &mut HeaderBar, clicks: &[(f64, u32)]| -> Vec<Option<DecorationAction>> {
            clicks.iter().map(|&(x, time)| at(h, (x, 16.0), &button(0x110, ButtonState::Pressed, time))).collect()
        };
        let far = 400.0 + DOUBLE_CLICK_SLOP + 2.0;
        for (clicks, want) in [
            (vec![(400.0, 0), (400.0, 180)], vec![Some(StartMove), Some(ToggleMaximize)]),
            (vec![(400.0, 0), (400.0, DOUBLE_CLICK_MS + 1)], vec![Some(StartMove); 2]),
            (vec![(400.0, 0), (far, 100)], vec![Some(StartMove); 2]),
            (vec![(400.0, 1000), (400.0, 10)], vec![Some(StartMove); 2]),
            (
                vec![(400.0, 0), (400.0, 100), (400.0, 200)],
                vec![Some(StartMove), Some(ToggleMaximize), Some(StartMove)],
            ),
            (vec![(400.0, 0), (16.0, 50), (400.0, 100)], vec![Some(StartMove), None, Some(StartMove)]),
        ] {
            assert_eq!(presses(&mut header(), &clicks), want, "{clicks:?}");
        }

        for (action, want) in [
            (glide_theme::TitlebarAction::Minimize, DecorationAction::Run(MenuItem::Minimize)),
            (glide_theme::TitlebarAction::None, StartMove),
        ] {
            let mut h = header();
            let theme = DecorationTheme { double_click: action, ..DecorationTheme::default() };
            h.sync(&theme, false, false);
            assert_eq!(presses(&mut h, &[(400.0, 0), (400.0, 120)])[1], Some(want));
        }
    }

    #[test]
    fn hit_zones_follow_the_button_layout_and_span_the_strip() {
        let mut h = header();
        for (x, y, zone) in [
            (0.0, 16.0, HitZone::Maximize),
            (16.0, 0.0, HitZone::Maximize),
            (16.0, 31.999, HitZone::Maximize),
            (33.999, 16.0, HitZone::Maximize),
            (34.0, 16.0, HitZone::Drag),
            (400.0, 16.0, HitZone::Drag),
            (765.999, 16.0, HitZone::Drag),
            (766.0, 16.0, HitZone::Close),
            (799.999, 16.0, HitZone::Close),
            (16.0, 32.0, HitZone::Drag),
        ] {
            assert_eq!(h.zone_at(x, y), zone, "({x}, {y})");
        }
        let swapped =
            DecorationTheme { button_layout: glide_theme::ButtonLayout::CloseMaximize, ..DecorationTheme::default() };
        h.sync(&swapped, false, false);
        assert_eq!(h.zone_at(16.0, 16.0), HitZone::Close);
        assert_eq!(h.zone_at(790.0, 16.0), HitZone::Maximize);
    }

    #[test]
    fn a_drag_changes_the_titlebar_raster_and_releasing_restores_it() {
        let (mut h, p) = (header(), painter());
        assert!(h.redraw(&p, 1) && !h.redraw(&p, 1));
        h.dragging = true;
        assert!(h.redraw(&p, 1));
        h.dragging = false;
        assert!(h.redraw(&p, 1));
    }

    #[test]
    fn the_shadow_margin_covers_the_larger_fade_plus_the_border() {
        let mut h = header();
        for (shadow, border, want) in [(true, 1, crate::paint::shadow_fade_margin(50) + 1), (false, 2, 2)] {
            let theme = DecorationTheme {
                shadow,
                shadow_size: 20,
                shadow_size_focused: 50,
                border_width: border,
                ..DecorationTheme::default()
            };
            h.sync(&theme, false, false);
            assert_eq!(h.shadow_margin(), want);
        }
    }

    #[test]
    fn button_colours_layer_hover_press_and_idle_dimming() {
        use crate::config::Color;
        let (normal, hover_bg) = (Color([0.1, 0.1, 0.1, 0.07]), Color([0.2, 0.4, 0.8, 1.0]));
        let (fg, fg_hover) = (Color([0.9, 0.9, 0.9, 0.68]), Color([1.0; 4]));
        let colors = |hover_t, hovered, press_t, focused| {
            button_colors(normal, hover_bg, fg, fg_hover, hover_t, hovered, press_t, focused)
        };
        assert_eq!(colors(0.0, false, 0.0, true), (normal, fg));
        assert_eq!(colors(1.0, true, 0.0, true), (hover_bg, fg_hover));
        let half = colors(0.5, true, 0.0, true).0;
        assert!(half.0[0] > normal.0[0] && half.0[0] < hover_bg.0[0]);

        let pressed = colors(0.0, false, 1.0, true).0;
        assert_eq!(pressed, normal.state_layer(fg, glide_theme::motion::ST_PRESS));
        assert!(pressed.0[3] > normal.0[3]);
        let half_pressed = colors(0.0, false, 0.5, true).0;
        assert!(half_pressed.0[0] > normal.0[0] && half_pressed.0[0] < pressed.0[0]);
        assert_ne!(colors(0.7, true, 0.0, true), colors(0.7, true, 1.0, true));

        let (dim_back, dim_front) = colors(0.0, false, 0.0, false);
        assert!((dim_back.0[3] - normal.0[3] * 0.45).abs() < 0.001);
        assert!((dim_front.0[3] - fg.0[3] * 0.45).abs() < 0.001);
        assert_eq!(dim_back.0[..3], normal.0[..3]);
        let (focused_hover, idle_hover) = (colors(1.0, true, 0.0, true).0, colors(1.0, true, 0.0, false).0);
        assert!(idle_hover.0[3] < focused_hover.0[3]);
        assert_eq!(idle_hover.0[..3], focused_hover.0[..3]);
    }

    #[test]
    fn the_decoration_owns_the_strip_the_band_and_only_open_menu_rows() {
        let mut h = header();
        for (x, y, inside) in [
            (400.0, 16.0, true),
            (400.0, 300.0, false),
            (-4.0, 300.0, true),
            (804.0, 300.0, true),
            (400.0, -4.0, true),
            (400.0, 636.0, true),
            (-9.0, 300.0, false),
            (400.0, 641.0, false),
            (750.0, 48.0, false),
        ] {
            assert_eq!(h.contains(point(x, y)), inside, "({x}, {y})");
        }
        h.menu = glide_ui::Reveal::open_now();
        assert!(h.contains(point(750.0, 48.0)));
        assert!(!h.contains(point(400.0, 48.0)), "left of the panel");

        let undecorated = HeaderBar { height: 0, width: 800, content_height: 600, ..HeaderBar::default() };
        for (x, y, inside) in [
            (400.0, 16.0, false),
            (-4.0, 300.0, true),
            (804.0, 300.0, true),
            (400.0, -4.0, true),
            (400.0, 604.0, true),
            (-9.0, 300.0, false),
        ] {
            assert_eq!(undecorated.contains(point(x, y)), inside, "({x}, {y})");
        }
        assert!(!HeaderBar::default().contains(point(0.0, 0.0)));
        assert!(!HeaderBar::default().contains(point(-4.0, -4.0)));
    }

    #[test]
    fn the_band_maps_to_resize_edges_with_matching_cursors() {
        let h = header();
        for (x, y, edge) in [
            (-4.0, 300.0, Some(ResizeEdge::LEFT)),
            (804.0, 300.0, Some(ResizeEdge::RIGHT)),
            (400.0, -4.0, Some(ResizeEdge::TOP)),
            (400.0, 636.0, Some(ResizeEdge::BOTTOM)),
            (-4.0, -4.0, Some(ResizeEdge::TOP_LEFT)),
            (804.0, 636.0, Some(ResizeEdge::BOTTOM_RIGHT)),
            (400.0, 16.0, None),
            (400.0, 300.0, None),
        ] {
            assert_eq!(h.resize_edges_at(point(x, y)), edge, "({x}, {y})");
        }
        for (edge, cursor) in [
            (ResizeEdge::TOP, CursorIcon::NResize),
            (ResizeEdge::BOTTOM, CursorIcon::SResize),
            (ResizeEdge::LEFT, CursorIcon::WResize),
            (ResizeEdge::RIGHT, CursorIcon::EResize),
            (ResizeEdge::TOP_LEFT, CursorIcon::NwResize),
            (ResizeEdge::TOP_RIGHT, CursorIcon::NeResize),
            (ResizeEdge::BOTTOM_LEFT, CursorIcon::SwResize),
            (ResizeEdge::BOTTOM_RIGHT, CursorIcon::SeResize),
        ] {
            assert_eq!(resize_cursor(edge), cursor);
        }
    }

    #[test]
    fn menu_rows_start_below_the_strip_inside_the_panel_column() {
        assert_eq!(header().menu_item_at(point(750.0, 48.0)), None, "closed");
        let h = open_header();
        for (x, y, row) in [
            (750.0, 16.0, None),
            (750.0, 40.0, Some(0)),
            (750.0, 80.0, Some(1)),
            (750.0, 120.0, Some(2)),
            (750.0, 145.0, Some(3)),
            (750.0, 200.0, None),
            (400.0, 48.0, None),
        ] {
            assert_eq!(h.menu_item_at(point(x, y)), row, "({x}, {y})");
        }
    }

    #[test]
    fn buttons_act_on_release_the_strip_moves_and_the_band_resizes() {
        let mut h = header();
        assert_eq!(at(&mut h, ON_DRAG, &press()), Some(StartMove));
        assert_eq!(at(&mut h, ON_MAXIMIZE, &press()), None);
        assert_eq!(h.pressed, Some(HitZone::Maximize));
        assert_eq!(at(&mut h, ON_DRAG, &release()), None);
        assert_eq!(h.pressed, None, "release clears the press wherever it lands");
        assert_eq!(at(&mut h, (-4.0, -4.0), &press()), Some(StartResize(ResizeEdge::TOP_LEFT)));
        assert_eq!(at(&mut h, (-4.0, -4.0), &release()), None);

        for (target, action) in [(ON_MAXIMIZE, ToggleMaximize), (ON_CLOSE, Close)] {
            assert_eq!(at(&mut header(), target, &release()), Some(action));
        }
    }

    #[test]
    fn a_right_click_opens_the_menu_and_the_next_click_closes_it() {
        let mut h = header();
        assert_eq!(at(&mut h, ON_DRAG, &right_press()), None);
        assert!(h.menu.is_open());
        assert_eq!(at(&mut h, ON_DRAG, &right_release()), None);
        assert!(h.menu.is_open(), "letting go of the button that opened it keeps the menu up");
        assert_eq!(at(&mut h, ON_MENU_ROW, &press()), None);
        assert_eq!(at(&mut h, ON_MENU_ROW, &release()), Some(DecorationAction::Run(MenuItem::Minimize)));
        assert!(!h.menu.is_open());

        let mut h = header();
        at(&mut h, ON_DRAG, &right_press());
        assert_eq!(
            at(&mut h, ON_MENU_ROW, &right_release()),
            Some(DecorationAction::Run(MenuItem::Minimize)),
            "press, drag onto a row and release picks it"
        );
        assert!(!h.menu.is_open());

        let mut h = header();
        at(&mut h, ON_DRAG, &right_press());
        at(&mut h, ON_DRAG, &right_release());
        assert_eq!(at(&mut h, ON_DRAG, &press()), None, "presses are ignored");
        assert_eq!(h.on_button(&release()), None);
        assert!(!h.menu.is_open(), "a click beside the rows closes it");
    }

    #[test]
    fn a_click_elsewhere_dismisses_the_menu() {
        let mut h = header();
        assert!(!h.dismiss_menu(), "nothing to dismiss");
        at(&mut h, ON_DRAG, &right_press());
        at(&mut h, ON_DRAG, &right_release());
        assert!(h.is_menu_open());
        assert!(h.dismiss_menu());
        assert!(!h.is_menu_open() && h.menu_opener.is_none());
        assert_eq!(h.menu_item_at(point(ON_MENU_ROW.0, ON_MENU_ROW.1)), None);
    }

    #[test]
    fn redraws_happen_exactly_when_something_visible_changed() {
        let p = painter();
        let mut empty = HeaderBar { width: 0, height: 0, ..HeaderBar::default() };
        assert!(!empty.redraw(&p, 1) && !empty.redraw(&p, 1));

        let mut h = header();
        assert!(h.redraw(&p, 1) && !h.redraw(&p, 1));
        h.set_title(String::new());
        assert!(!h.redraw(&p, 1), "the same title is a no-op");
        h.set_title("A different title".to_owned());
        assert!(h.redraw(&p, 1) && !h.redraw(&p, 1));
        h.menu = glide_ui::Reveal::open_now();
        assert!(h.redraw(&p, 1) && !h.redraw(&p, 1));
        h.menu = glide_ui::Reveal::closed();
        assert!(h.redraw(&p, 1));
        h.sync(&DecorationTheme::default(), true, false);
        assert!(h.redraw(&p, 1), "focus flipped");
        h.sync(&DecorationTheme::default(), h.focused, true);
        assert!(h.redraw(&p, 1), "maximize toggled");
        let red =
            DecorationTheme { background: crate::config::Color([1.0, 0.0, 0.0, 1.0]), ..DecorationTheme::default() };
        h.sync(&red, h.focused, h.maximized);
        assert!(h.redraw(&p, 1), "theme changed");

        let mut h = header();
        h.pointer_enter(point(ON_MAXIMIZE.0, ON_MAXIMIZE.1));
        assert!(h.redraw(&p, 1));
        settle_layer();
        assert!(h.redraw(&p, 1), "the hover fade progressed");
        assert!(!h.redraw(&p, 1), "settled");
    }
}

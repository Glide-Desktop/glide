use smithay::{
    backend::{allocator::Fourcc, input::ButtonState, renderer::element::memory::MemoryRenderBuffer},
    input::pointer::ButtonEvent,
    utils::{Logical, Point, Rectangle, Transform},
};

use crate::{
    config::{Color, DecorationTheme},
    grabs::resize_grab::ResizeEdge,
    paint::{Painter, TextAlign},
    raster::{Canvas, rect},
};

use super::{
    BUTTON_HIT_WIDTH, DOUBLE_CLICK_MS, DOUBLE_CLICK_SLOP, IDLE_BUTTON_OPACITY, MENU_CONTAINER_PADDING,
    MENU_CONTAINER_RADIUS, MENU_CORNER_RADIUS, MENU_PADDING, MENU_ROW_HEIGHT, MENU_WIDTH, RESIZE_BORDER,
    menu::MenuItem, menu_open_anim, render::redraw_button, state_layer_anim,
};

use glide_theme::motion::{ease_io, ease_out};

const ROW_ALPHA_STEPS: f64 = 15.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Fade(glide_ui::Tween);

impl Default for Fade {
    fn default() -> Self {
        Self(glide_ui::Tween::at(0.0))
    }
}

impl std::ops::Deref for Fade {
    type Target = glide_ui::Tween;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Fade {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Outline {
    #[default]
    None,
    Drawn,
}

#[derive(Debug, Default)]
pub struct HeaderBar {
    pub(crate) height: i32,
    pub(crate) width: i32,
    pub(crate) content_height: i32,
    pub(crate) pointer_loc: Option<Point<f64, Logical>>,
    pub(crate) menu: glide_ui::Reveal,
    pub(crate) menu_t: Fade,
    pub(crate) menu_hover: Option<usize>,
    pub(crate) menu_row_fade: Vec<(usize, Fade)>,
    pub(crate) menu_x: i32,
    pub(crate) menu_width: i32,
    pub(crate) menu_opener: Option<u32>,
    pub(crate) pressed: Option<HitZone>,
    pub(crate) press_zone: Option<HitZone>,
    pub(crate) press_t: Fade,
    pub(crate) dragging: bool,
    pub(crate) last_drag_press: Option<(u32, Point<f64, Logical>)>,
    pub(crate) button_hover: glide_ui::HoverAnimation<HitZone>,

    pub(crate) theme: DecorationTheme,
    pub(crate) theme_rev: u32,
    pub(crate) focused: bool,
    pub(crate) focus_fade_started: Option<std::time::Instant>,
    pub(crate) maximized: bool,
    pub(crate) outline: Outline,

    pub(crate) buffers: std::collections::HashMap<i32, HeaderBuffers>,
}

type ButtonKey = (i32, [u32; 4], [u32; 4], bool, bool);
type MenuKey = (i32, i32, Vec<u8>, bool, u32, u64);

#[derive(Debug, Default)]
pub(crate) struct HeaderBuffers {
    pub(crate) titlebar: Option<MemoryRenderBuffer>,
    pub(crate) maximize: Option<MemoryRenderBuffer>,
    pub(crate) close: Option<MemoryRenderBuffer>,
    pub(crate) menu: Option<MemoryRenderBuffer>,
    titlebar_key: Option<(i32, i32, [u32; 4], u32)>,
    button_keys: [Option<ButtonKey>; 2],
    menu_key: Option<MenuKey>,
}

fn bits(colour: Color) -> [u32; 4] {
    colour.0.map(f32::to_bits)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HitZone {
    Maximize,
    Close,
    Drag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DecorationAction {
    ToggleMaximize,
    Close,
    Run(MenuItem),
    StartMove,
    StartResize(ResizeEdge),
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn button_colors(
    normal: Color, hover_bg: Color, fg: Color, fg_hover: Color, hover_t: f32, is_hovered: bool, press_t: f32,
    focus_factor: f32,
) -> (Color, Color) {
    let (mut bg, mut fg) =
        if is_hovered { (normal.lerp(hover_bg, hover_t), fg.lerp(fg_hover, hover_t)) } else { (normal, fg) };

    if press_t > 0.0 {
        bg = bg.state_layer(fg, glide_theme::motion::ST_PRESS * press_t);
    }

    let opacity = IDLE_BUTTON_OPACITY + (1.0 - IDLE_BUTTON_OPACITY) * focus_factor.clamp(0.0, 1.0);
    bg = bg.with_alpha(bg.0[3] * opacity);
    fg = fg.with_alpha(fg.0[3] * opacity);

    (bg, fg)
}

impl HeaderBar {
    #[must_use]
    pub const fn is_decorated(&self) -> bool {
        self.height > 0
    }

    #[must_use]
    pub const fn is_framed(&self) -> bool {
        self.height > 0 || matches!(self.outline, Outline::Drawn)
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
    pub fn button_size(&self) -> i32 {
        glide_theme::component::titlebar_button(self.height)
    }

    pub(crate) fn border_width(&self) -> i32 {
        i32::try_from(self.theme.border_width).unwrap_or(RESIZE_BORDER).clamp(0, RESIZE_BORDER)
    }

    #[must_use]
    pub fn shadow_margin(&self) -> i32 {
        let border = self.border_width();
        if self.theme.shadow {
            let s1 = i32::try_from(self.theme.shadow_size).unwrap_or(i32::MAX);
            let s2 = i32::try_from(self.theme.shadow_size_focused).unwrap_or(i32::MAX);
            crate::paint::shadow_fade_margin(s1.max(s2)).saturating_add(border)
        } else {
            border
        }
    }

    pub(crate) fn sync(&mut self, theme: &DecorationTheme, focused: bool, maximized: bool) {
        if &self.theme != theme {
            self.theme = theme.clone();
            self.theme_rev = self.theme_rev.wrapping_add(1);
        }
        if self.focused != focused {
            let now = std::time::Instant::now();
            let duration = super::focus_fade();
            let remaining =
                self.focus_fade_started.map(|started| duration.saturating_sub(started.elapsed())).unwrap_or_default();
            self.focus_fade_started = Some(now.checked_sub(remaining).unwrap_or(now));
            for buffers in self.buffers.values_mut() {
                buffers.titlebar_key = None;
                buffers.button_keys = [None; 2];
            }
        }
        self.focused = focused;
        self.maximized = maximized;
    }

    #[must_use]
    pub(crate) fn buffers_at(&self, scale: i32) -> Option<&HeaderBuffers> {
        self.buffers.get(&scale.max(1))
    }

    pub(crate) fn button_hover_progress(&mut self) -> f64 {
        ease_io(self.button_hover.progress(state_layer_anim()))
    }

    #[must_use]
    pub(crate) fn press_layer_progress(&self) -> f64 {
        self.press_t.value(state_layer_anim(), ease_io)
    }

    #[must_use]
    pub(crate) fn is_press_fading(&self) -> bool {
        self.press_t.is_animating(state_layer_anim())
    }

    fn press_zone(&mut self, zone: HitZone) {
        self.pressed = Some(zone);
        self.press_zone = Some(zone);
        self.press_t.jump(1.0);
    }

    fn release_press(&mut self) {
        self.pressed = None;
        self.press_t.retarget(0.0, state_layer_anim(), ease_io);
    }

    #[must_use]
    pub(crate) fn hovered_zone(&self) -> Option<HitZone> {
        self.pointer_loc
            .filter(|l| self.in_titlebar_strip(*l))
            .map(|l| self.zone_at(l.x, l.y))
            .filter(|zone| !matches!(zone, HitZone::Drag))
    }

    pub(crate) fn sync_button_hover(&mut self) -> bool {
        let zone = self.hovered_zone();
        self.button_hover.set(zone)
    }

    #[must_use]
    pub fn is_hover_animating(&self) -> bool {
        self.button_hover.is_animating(state_layer_anim())
            || self.is_focus_fading()
            || self.is_menu_animating()
            || self.is_menu_row_fading()
            || self.is_press_fading()
    }

    #[must_use]
    pub fn is_focus_fading(&self) -> bool {
        let duration = super::focus_fade();
        !duration.is_zero() && self.focus_fade_started.is_some_and(|started| started.elapsed() < duration)
    }

    fn focus_fade_t(&mut self) -> f32 {
        let Some(started) = self.focus_fade_started else {
            return 1.0;
        };
        let duration = super::focus_fade();
        let elapsed = started.elapsed();
        if duration.is_zero() || elapsed >= duration {
            self.focus_fade_started = None;
            return 1.0;
        }
        let t = elapsed.as_secs_f64() / duration.as_secs_f64();
        ease_io(t) as f32
    }

    #[must_use]
    pub(crate) const fn close_on_left(&self) -> bool {
        matches!(self.theme.button_layout, glide_theme::ButtonLayout::CloseMaximize)
    }

    pub(crate) fn zone_at(&self, x: f64, y: f64) -> HitZone {
        let (width, button) = (f64::from(self.width), f64::from(BUTTON_HIT_WIDTH));
        if !(0.0..f64::from(self.height)).contains(&y) {
            return HitZone::Drag;
        }
        let left = (0.0..button).contains(&x);
        let right = (width - button..width).contains(&x);
        match (left, right, self.close_on_left()) {
            (true, _, true) | (_, true, false) => HitZone::Close,
            (true, _, false) | (_, true, true) => HitZone::Maximize,
            _ => HitZone::Drag,
        }
    }

    #[allow(clippy::unused_self)]
    fn menu_container_height(&self) -> i32 {
        let row_count = i32::try_from(MenuItem::ALL.len()).unwrap_or(0);
        MENU_CONTAINER_PADDING * 2 + MENU_ROW_HEIGHT * row_count
    }

    pub(crate) fn menu_panel_rect(&self) -> glide_geom::Rectangle {
        glide_geom::Rectangle::from_loc_and_size(
            self.menu_x,
            self.height,
            self.menu_width.max(1),
            self.menu_container_height(),
        )
    }

    pub fn contains(&self, rel: Point<f64, Logical>) -> bool {
        if !self.has_frame() {
            return false;
        }

        let width = f64::from(self.width);
        let frame_height = f64::from(self.height + self.content_height);
        let band = f64::from(RESIZE_BORDER);

        let in_expanded_frame = rel.x >= -band && rel.x < width + band && rel.y >= -band && rel.y < frame_height + band;
        if !in_expanded_frame {
            return false;
        }

        let in_content = rel.x >= 0.0 && rel.x < width && rel.y >= f64::from(self.height) && rel.y < frame_height;
        if in_content {
            return self.menu.is_open() && self.in_menu_panel(rel);
        }

        true
    }

    fn in_menu_panel(&self, rel: Point<f64, Logical>) -> bool {
        let panel = self.menu_panel_rect();
        let (left, top) = (f64::from(panel.loc.x), f64::from(panel.loc.y));
        rel.x >= left
            && rel.x < left + f64::from(panel.size.width)
            && rel.y >= top
            && rel.y < top + f64::from(panel.size.height)
    }

    pub(crate) fn resize_edges_at(&self, rel: Point<f64, Logical>) -> Option<ResizeEdge> {
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

    pub(crate) fn menu_item_at(&self, loc: Point<f64, Logical>) -> Option<usize> {
        if !self.menu.is_open() {
            return None;
        }
        let panel = self.menu_panel_rect();
        let x = loc.x - f64::from(panel.loc.x + MENU_CONTAINER_PADDING);
        let y = loc.y - f64::from(panel.loc.y + MENU_CONTAINER_PADDING);
        let width = f64::from(panel.size.width - MENU_CONTAINER_PADDING * 2);
        if !(0.0..width).contains(&x) || y < 0.0 {
            return None;
        }
        let row = (y / f64::from(MENU_ROW_HEIGHT)) as usize;
        (row < MenuItem::ALL.len()).then_some(row)
    }

    fn set_menu_hover(&mut self, hover: Option<usize>) {
        self.menu_hover = hover;
        let duration = state_layer_anim();
        if let Some(row) = hover
            && !self.menu_row_fade.iter().any(|(i, _)| *i == row)
        {
            self.menu_row_fade.push((row, Fade::default()));
        }
        for (row, fade) in &mut self.menu_row_fade {
            let to = if hover == Some(*row) { 1.0 } else { 0.0 };
            fade.retarget(to, duration, ease_io);
        }
        self.menu_row_fade.retain(|(_, fade)| fade.target() > 0.0 || fade.is_animating(duration));
    }

    #[must_use]
    pub(crate) fn menu_row_alpha(&self, row: usize) -> f64 {
        self.menu_row_fade
            .iter()
            .find(|(i, _)| *i == row)
            .map_or(0.0, |(_, fade)| fade.value(state_layer_anim(), ease_io))
    }

    #[must_use]
    pub(crate) fn is_menu_row_fading(&self) -> bool {
        let duration = state_layer_anim();
        self.menu_row_fade.iter().any(|(_, fade)| fade.is_animating(duration))
    }

    pub fn pointer_enter(&mut self, loc: Point<f64, Logical>) {
        self.pointer_loc = Some(loc);
        self.set_menu_hover(self.menu_item_at(loc));
        self.sync_button_hover();
    }

    pub fn pointer_leave(&mut self) {
        self.pointer_loc = None;
        self.set_menu_hover(None);
        self.release_press();
        self.sync_button_hover();
    }

    fn menu_ease(&self) -> fn(f64) -> f64 {
        if self.menu.is_open() { ease_out } else { ease_io }
    }

    fn open_menu_at(&mut self, at_x: i32) {
        let duration = menu_open_anim();
        let ease = self.menu_ease();
        if self.menu.open(duration) {
            self.menu_t.retarget(1.0, duration, ease);
        }
        self.menu_width = MENU_WIDTH.min(self.width);
        self.menu_x = (at_x - self.menu_width / 2).clamp(0, (self.width - self.menu_width).max(0));
        let hover = self.pointer_loc.and_then(|loc| self.menu_item_at(loc));
        self.set_menu_hover(hover);
    }

    fn close_menu(&mut self) {
        self.menu_opener = None;
        let duration = menu_open_anim();
        let ease = self.menu_ease();
        if self.menu.close(duration) {
            self.menu_t.retarget(0.0, duration, ease);
        }
    }

    #[must_use]
    pub const fn is_menu_open(&self) -> bool {
        self.menu.is_open()
    }

    pub fn dismiss_menu(&mut self) -> bool {
        if !self.menu.is_open() {
            return false;
        }
        self.close_menu();
        self.set_menu_hover(None);
        true
    }

    #[must_use]
    pub(crate) fn menu_open_progress(&self) -> f64 {
        self.menu_t.value(menu_open_anim(), self.menu_ease())
    }

    #[must_use]
    pub(crate) fn is_menu_animating(&self) -> bool {
        self.menu.is_animating(menu_open_anim())
    }

    #[must_use]
    pub(crate) fn is_menu_visible(&self) -> bool {
        self.menu.is_visible(menu_open_anim())
    }

    pub fn open_menu_at_default_position(&mut self) {
        self.open_menu_at_x(BUTTON_HIT_WIDTH);
    }

    pub fn open_menu_at_x(&mut self, x: i32) {
        if !self.is_decorated() {
            return;
        }
        self.open_menu_at(x);
    }

    fn is_double_click(&self, time: u32, loc: Point<f64, Logical>) -> bool {
        let Some((previous, at)) = self.last_drag_press else {
            return false;
        };
        let Some(elapsed) = time.checked_sub(previous) else {
            return false;
        };
        elapsed <= DOUBLE_CLICK_MS
            && (loc.x - at.x).abs() <= DOUBLE_CLICK_SLOP
            && (loc.y - at.y).abs() <= DOUBLE_CLICK_SLOP
    }

    pub(crate) fn on_button(&mut self, event: &ButtonEvent) -> Option<DecorationAction> {
        let opener = event.state == ButtonState::Released && self.menu_opener.take() == Some(event.button);
        let loc = self.pointer_loc?;

        if event.state == ButtonState::Pressed {
            if self.menu.is_open() {
                return None;
            }
            if let Some(edges) = self.resize_edges_at(loc) {
                return Some(DecorationAction::StartResize(edges));
            }
            if !self.in_titlebar_strip(loc) {
                return None;
            }
            if event.button == 0x111 {
                self.open_menu_at(loc.x as i32);
                self.menu_opener = Some(event.button);
                return None;
            }

            let zone = self.zone_at(loc.x, loc.y);
            if zone == HitZone::Drag {
                let time = event.time.millis();
                if self.is_double_click(time, loc) {
                    self.last_drag_press = None;
                    match self.theme.double_click {
                        glide_theme::TitlebarAction::Maximize => {
                            return Some(DecorationAction::ToggleMaximize);
                        }
                        glide_theme::TitlebarAction::Minimize => {
                            return Some(DecorationAction::Run(MenuItem::Minimize));
                        }
                        glide_theme::TitlebarAction::None => {}
                    }
                }
                self.last_drag_press = Some((time, loc));
                return Some(DecorationAction::StartMove);
            }
            self.last_drag_press = None;
            self.press_zone(zone);
            return None;
        }

        self.release_press();

        if self.menu.is_open() {
            let item = self.menu_item_at(loc).and_then(|i| MenuItem::ALL.get(i));
            if opener && item.is_none() {
                return None;
            }
            let action = item.map(|&item| DecorationAction::Run(item));
            self.close_menu();
            return action;
        }

        if !self.in_titlebar_strip(loc) {
            return None;
        }

        match self.zone_at(loc.x, loc.y) {
            HitZone::Maximize => Some(DecorationAction::ToggleMaximize),
            HitZone::Close => Some(DecorationAction::Close),
            HitZone::Drag => None,
        }
    }

    fn in_titlebar_strip(&self, loc: Point<f64, Logical>) -> bool {
        loc.x >= 0.0 && loc.x < f64::from(self.width) && loc.y >= 0.0 && loc.y < f64::from(self.height)
    }

    #[allow(clippy::too_many_lines)]
    #[profiling::function]
    pub(crate) fn redraw(&mut self, painter: &Painter, scale: i32) -> bool {
        if self.width == 0 || self.height == 0 {
            return false;
        }
        let scale = scale.max(1);

        self.menu.settle(menu_open_anim());

        let hovered = self.hovered_zone();
        let pressed = self.pressed;
        let hover_t = self.button_hover_progress() as f32;
        let press_t = self.press_layer_progress() as f32;
        let press_zone = if press_t > 0.0 { self.press_zone } else { None };
        let focus_fade_t = self.focus_fade_t();

        let menu_visible = self.is_menu_visible();
        let menu_row_alpha: Vec<u8> = if menu_visible {
            (0..MenuItem::ALL.len())
                .map(|row| (self.menu_row_alpha(row).clamp(0.0, 1.0) * ROW_ALPHA_STEPS).round() as u8)
                .collect()
        } else {
            Vec::new()
        };

        let container_h = self.menu_container_height();
        let menu_w = self.menu_panel_rect().size.width;
        let button_size = self.button_size();
        let buffers = self.buffers.entry(scale).or_default();
        let mut changed = false;

        let theme = &self.theme;
        let mix = |from: crate::config::Color, to: crate::config::Color| {
            let mut out = from;
            for (channel, target) in out.0.iter_mut().zip(to.0) {
                *channel += (target - *channel) * focus_fade_t;
            }
            out
        };
        let (bg_color, title_color) = if self.focused {
            (mix(theme.background, theme.background_focused), mix(theme.title, theme.title_focused))
        } else {
            (mix(theme.background_focused, theme.background), mix(theme.title_focused, theme.title))
        };

        let bg_color =
            if self.dragging { bg_color.state_layer(title_color, glide_theme::motion::ST_DRAG) } else { bg_color };

        let corner_radius = if self.maximized { 0.0 } else { theme.corner_radius };

        let titlebar_key = (self.width, self.height, bits(bg_color), corner_radius.to_bits());
        if buffers.titlebar_key != Some(titlebar_key) {
            buffers.titlebar_key = Some(titlebar_key);
            changed = true;
            let w = self.width;
            let h = self.height;
            let (pw, ph) = (w * scale, h * scale);
            let buf = buffers.titlebar.get_or_insert_with(|| {
                MemoryRenderBuffer::new(Fourcc::Argb8888, (pw, ph), scale, Transform::Normal, None)
            });
            let mut ctx = buf.render();
            ctx.resize((pw, ph));
            let _ = ctx.draw(|pixels| {
                Painter::fill_panel_background(
                    pixels,
                    w,
                    h,
                    scale,
                    bg_color,
                    corner_radius,
                    [true, true, false, false],
                );
                Ok::<_, ()>(vec![Rectangle::from_size((pw, ph).into())])
            });
        }

        let focus_factor = if self.focused { focus_fade_t } else { 1.0 - focus_fade_t };

        let maximize_icon: fn(&mut [u8], i32, i32, i32, Color, Color) =
            if self.maximized { Painter::draw_icon_restore } else { Painter::draw_icon_maximize };

        let fg_hover = Color([1.0, 1.0, 1.0, 1.0]);
        let [maximize_key, close_key] = &mut buffers.button_keys;
        for (key, buf, zone, normal, hover_bg, fg, icon) in [
            (
                maximize_key,
                &mut buffers.maximize,
                HitZone::Maximize,
                theme.maximize_button,
                theme.maximize_button_hover,
                theme.icon,
                maximize_icon,
            ),
            (
                close_key,
                &mut buffers.close,
                HitZone::Close,
                theme.close_button,
                theme.close_button_hover,
                theme.close_icon,
                Painter::draw_icon_close,
            ),
        ] {
            let is_hovered = hovered == Some(zone);
            let is_pressed = pressed == Some(zone);
            let zone_press_t = if press_zone == Some(zone) { press_t } else { 0.0 };
            let (btn_bg, btn_fg) =
                button_colors(normal, hover_bg, fg, fg_hover, hover_t, is_hovered, zone_press_t, focus_factor);
            let restore = self.maximized && zone == HitZone::Maximize;
            let button_key = (button_size, bits(btn_bg), bits(btn_fg), is_pressed, restore);
            if *key != Some(button_key) {
                *key = Some(button_key);
                changed = true;
                redraw_button(buf, button_size, scale, btn_bg, btn_fg, is_pressed, icon);
            }
        }

        let menu_key = (
            menu_w,
            container_h,
            menu_row_alpha.clone(),
            self.maximized,
            self.theme_rev,
            crate::paint::painter_generation(),
        );
        if menu_visible && buffers.menu_key.as_ref() != Some(&menu_key) {
            buffers.menu_key = Some(menu_key);
            changed = true;
            let (pw, ph) = (menu_w * scale, container_h * scale);
            let buf = buffers.menu.get_or_insert_with(|| {
                MemoryRenderBuffer::new(Fourcc::Argb8888, (pw, ph), scale, Transform::Normal, None)
            });
            let mut ctx = buf.render();
            ctx.resize((pw, ph));
            let maximized = self.maximized;
            let _ = ctx.draw(|pixels| {
                let mut c = Canvas::over(pixels, menu_w, container_h, scale);
                c.panel(theme.menu_background, MENU_CONTAINER_RADIUS);
                let s = scale as f32;
                crate::paint::draw_inset_border(c.buf, pw, ph, s, MENU_CONTAINER_RADIUS * s, theme.menu_border);

                let (row_left, row_w) = (MENU_CONTAINER_PADDING, menu_w - MENU_CONTAINER_PADDING * 2);
                for (i, item) in MenuItem::ALL.iter().enumerate() {
                    let row_top = MENU_CONTAINER_PADDING + i32::try_from(i).unwrap_or(0) * MENU_ROW_HEIGHT;
                    let row_alpha = menu_row_alpha.get(i).copied().unwrap_or(0);
                    if row_alpha > 0 {
                        let row_t = (f64::from(row_alpha) / ROW_ALPHA_STEPS) as f32;
                        let hover = theme.menu_item_hover.with_alpha(theme.menu_item_hover.0[3] * row_t);
                        c.fill(rect(row_left, row_top, row_w, MENU_ROW_HEIGHT), hover, MENU_CORNER_RADIUS);
                    }
                    let label_color = if item.is_danger() { theme.menu_text_danger } else { theme.menu_text };
                    let row_bottom = row_top + MENU_ROW_HEIGHT;
                    let accel = item.accel().unwrap_or("");
                    let accel_w = if accel.is_empty() {
                        0
                    } else {
                        painter.text_width_styled(accel, &glide_theme::TextStyle::CAP).ceil() as i32 + MENU_PADDING
                    };
                    let label_x_min = row_left + MENU_PADDING;
                    let label_x_max = (row_left + row_w - MENU_PADDING - accel_w).max(label_x_min);
                    let band = (label_x_min, label_x_max, row_top, row_bottom);
                    let label = item.label(maximized);
                    c.text(painter, band, TextAlign::Left, label, label_color, &glide_theme::TextStyle::MENU_ITEM);
                    if !accel.is_empty() {
                        let accel_x_max = row_left + row_w - MENU_PADDING;
                        let band =
                            ((accel_x_max - accel_w + MENU_PADDING).max(label_x_max), accel_x_max, row_top, row_bottom);
                        c.text(
                            painter,
                            band,
                            TextAlign::Left,
                            accel,
                            theme.menu_accel_text,
                            &glide_theme::TextStyle::CAP,
                        );
                    }
                }
                Ok::<_, ()>(vec![Rectangle::from_size((pw, ph).into())])
            });
        } else if !menu_visible && buffers.menu.take().is_some() {
            buffers.menu_key = None;
            changed = true;
        }

        changed
    }
}

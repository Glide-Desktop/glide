use smithay::{
    backend::{
        allocator::Fourcc, input::ButtonState, renderer::element::memory::MemoryRenderBuffer,
    },
    input::pointer::ButtonEvent,
    utils::{Logical, Point, Rectangle, Transform},
};

use crate::{
    config::{Color, DecorationTheme, TitleAlign},
    grabs::resize_grab::ResizeEdge,
    paint::{Painter, TextAlign},
};

use super::{
    BUTTON_HIT_WIDTH, BUTTON_PAD_X, BUTTON_SIZE, DOUBLE_CLICK_MS, DOUBLE_CLICK_SLOP,
    IDLE_BUTTON_OPACITY, MENU_CONTAINER_PADDING, MENU_CONTAINER_RADIUS, MENU_CORNER_RADIUS,
    MENU_PADDING, MENU_ROW_HEIGHT, MENU_WIDTH, RESIZE_BORDER, TITLE_PADDING, button_hover_anim,
    menu::MenuItem, render::redraw_button,
};

// `focused`, `maximized`, `menu_open` and `dragging` are four genuinely
// orthogonal UI states that can hold in any combination — a focused, maximized
// window can be dragged with its menu open. The lint exists to catch flags that
// should have been one enum; collapsing these would lose information rather
// than express it better.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Default)]
pub struct HeaderBar {
    pub(crate) height: i32,
    pub(crate) width: i32,
    pub(crate) content_height: i32,
    pub(crate) title: String,
    pub(crate) pointer_loc: Option<Point<f64, Logical>>,
    pub(crate) menu_open: bool,
    pub(crate) menu_opened_at: Option<std::time::Instant>,
    pub(crate) menu_hover: Option<usize>,
    pub(crate) menu_x: i32,
    pub(crate) menu_width: i32,
    pub(crate) pressed: Option<HitZone>,
    pub(crate) dragging: bool,
    pub(crate) last_drag_press: Option<(u32, Point<f64, Logical>)>,
    pub(crate) button_hover: glide_ui::HoverAnimation<HitZone>,

    pub(crate) theme: DecorationTheme,
    pub(crate) theme_rev: u32,
    pub(crate) focused: bool,
    pub(crate) focus_fade_started: Option<std::time::Instant>,
    pub(crate) maximized: bool,
    pub(crate) scale: i32,

    pub(crate) titlebar_buf: Option<MemoryRenderBuffer>,
    pub(crate) maximize_buf: Option<MemoryRenderBuffer>,
    pub(crate) close_buf: Option<MemoryRenderBuffer>,
    pub(crate) menu_buf: Option<MemoryRenderBuffer>,

    pub(crate) last_redraw: Option<RedrawSnapshot>,
}

// Mirrors `HeaderBar`'s orthogonal flags; see the note there.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RedrawSnapshot {
    width: i32,
    height: i32,
    scale: i32,
    focused: bool,
    maximized: bool,
    theme_rev: u32,
    painter_gen: u64,
    title: String,
    hovered: Option<HitZone>,
    hover_t_bits: u32,
    focus_fade_bits: u32,
    pressed: Option<HitZone>,
    dragging: bool,
    menu_open: bool,
    menu_width: i32,
    menu_hover: Option<usize>,
    menu_open_bits: u32,
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
    normal: Color,
    hover_bg: Color,
    fg: Color,
    fg_hover: Color,
    hover_t: f32,
    is_hovered: bool,
    is_pressed: bool,
    focused: bool,
) -> (Color, Color) {
    let (mut bg, mut fg) = if is_hovered {
        (normal.lerp(hover_bg, hover_t), fg.lerp(fg_hover, hover_t))
    } else {
        (normal, fg)
    };

    if is_pressed {
        let press_overlay = Color([1.0, 1.0, 1.0, 1.0]);
        bg = bg.lerp(press_overlay, glide_theme::motion::ST_PRESS);
    }

    if !focused {
        bg = bg.with_alpha(bg.0[3] * IDLE_BUTTON_OPACITY);
        fg = fg.with_alpha(fg.0[3] * IDLE_BUTTON_OPACITY);
    }

    (bg, fg)
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
        let border = i32::try_from(self.theme.border_width)
            .unwrap_or(super::RESIZE_BORDER)
            .clamp(0, super::RESIZE_BORDER);
        if self.theme.shadow {
            let s1 = i32::try_from(self.theme.shadow_size).unwrap_or(i32::MAX);
            let s2 = i32::try_from(self.theme.shadow_size_focused).unwrap_or(i32::MAX);
            crate::paint::shadow_fade_margin(s1.max(s2)).saturating_add(border)
        } else {
            border
        }
    }

    pub(crate) fn sync(
        &mut self,
        theme: &DecorationTheme,
        focused: bool,
        maximized: bool,
        scale: i32,
    ) {
        if &self.theme != theme {
            self.theme = theme.clone();
            self.theme_rev = self.theme_rev.wrapping_add(1);
        }
        if self.focused != focused {
            self.focus_fade_started = Some(std::time::Instant::now());
        }
        self.focused = focused;
        self.maximized = maximized;
        if self.scale != scale {
            self.scale = scale;
            self.titlebar_buf = None;
            self.maximize_buf = None;
            self.close_buf = None;
            self.menu_buf = None;
        }
    }

    pub(crate) const fn effective_scale(&self) -> i32 {
        if self.scale < 1 { 1 } else { self.scale }
    }

    pub(crate) fn button_hover_progress(&mut self) -> f64 {
        glide_theme::motion::E_OUT.ease_f64(self.button_hover.progress(button_hover_anim()))
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
        self.button_hover.is_animating(button_hover_anim())
            || self.is_focus_fading()
            || self.is_menu_animating()
    }

    #[must_use]
    pub fn is_focus_fading(&self) -> bool {
        let duration = super::focus_fade();
        !duration.is_zero()
            && self
                .focus_fade_started
                .is_some_and(|started| started.elapsed() < duration)
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
        #[allow(clippy::cast_possible_truncation)]
        {
            glide_theme::motion::E_IO.ease_f64(t) as f32
        }
    }

    pub fn set_title(&mut self, title: String) {
        if self.title != title {
            self.title = title;
            self.titlebar_buf = None;
        }
    }

    fn titlebar_row(&self) -> [(HitZone, glide_geom::Rectangle); 3] {
        let max_rect =
            glide_geom::Rectangle::from_loc_and_size(0, 0, BUTTON_HIT_WIDTH, self.height);
        let close_rect = glide_geom::Rectangle::from_loc_and_size(
            self.width - BUTTON_HIT_WIDTH,
            0,
            BUTTON_HIT_WIDTH,
            self.height,
        );
        let drag_rect = glide_geom::Rectangle::from_loc_and_size(0, 0, self.width, self.height);

        [
            (HitZone::Drag, drag_rect),
            (HitZone::Maximize, max_rect),
            (HitZone::Close, close_rect),
        ]
    }

    pub(crate) fn zone_at(&self, x: f64, y: f64) -> HitZone {
        glide_ui::hit_test(&self.titlebar_row(), x, y).unwrap_or(HitZone::Drag)
    }

    #[allow(clippy::unused_self)]
    fn menu_container_height(&self) -> i32 {
        let row_count = i32::try_from(MenuItem::ALL.len()).unwrap_or(0);
        MENU_CONTAINER_PADDING * 2 + MENU_ROW_HEIGHT * row_count
    }

    fn interactive_height(&self) -> i32 {
        if self.menu_open {
            self.height + self.menu_container_height()
        } else {
            self.height
        }
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

    fn menu_rows(&self) -> Vec<(usize, glide_geom::Rectangle)> {
        let row_count = MenuItem::ALL.len();
        let bounds = glide_geom::Rectangle::from_loc_and_size(
            self.menu_x + MENU_CONTAINER_PADDING,
            self.height + MENU_CONTAINER_PADDING,
            self.menu_width - MENU_CONTAINER_PADDING * 2,
            MENU_ROW_HEIGHT * i32::try_from(row_count).unwrap_or(0),
        );
        let tracks = vec![glide_ui::Track::Fixed(MENU_ROW_HEIGHT); row_count];
        (0..row_count)
            .zip(glide_ui::layout_column(bounds, &tracks))
            .collect()
    }

    pub(crate) fn menu_item_at(&self, loc: Point<f64, Logical>) -> Option<usize> {
        if !self.menu_open {
            return None;
        }
        glide_ui::hit_test(&self.menu_rows(), loc.x, loc.y)
    }

    pub fn pointer_enter(&mut self, loc: Point<f64, Logical>) {
        self.pointer_loc = Some(loc);
        self.menu_hover = self.menu_item_at(loc);
        self.sync_button_hover();
    }

    pub fn pointer_leave(&mut self) {
        self.pointer_loc = None;
        self.menu_hover = None;
        self.pressed = None;
        self.sync_button_hover();
    }

    fn open_menu_at(&mut self, at_x: i32) {
        if !self.menu_open {
            self.menu_opened_at = Some(std::time::Instant::now());
        }
        self.menu_open = true;
        self.menu_width = MENU_WIDTH.min(self.width);
        self.menu_x = (at_x - self.menu_width / 2).clamp(0, (self.width - self.menu_width).max(0));
    }

    #[must_use]
    pub(crate) fn menu_open_progress(&self) -> f64 {
        let Some(started) = self.menu_opened_at else {
            return 1.0;
        };
        let duration = super::menu_open_anim();
        let elapsed = started.elapsed();
        if duration.is_zero() || elapsed >= duration {
            return 1.0;
        }
        glide_theme::motion::E_OUT.ease_f64(elapsed.as_secs_f64() / duration.as_secs_f64())
    }

    #[must_use]
    pub(crate) fn is_menu_animating(&self) -> bool {
        self.menu_open && self.menu_open_progress() < 1.0
    }

    pub fn open_menu_at_default_position(&mut self) {
        if !self.is_decorated() {
            return;
        }
        self.open_menu_at(BUTTON_HIT_WIDTH);
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
            if event.button == 0x111 {
                #[allow(clippy::cast_possible_truncation)]
                self.open_menu_at(loc.x as i32);
                return None;
            }

            let zone = self.zone_at(loc.x, loc.y);
            if zone == HitZone::Drag {
                if self.is_double_click(event.time, loc) {
                    self.last_drag_press = None;
                    return Some(DecorationAction::ToggleMaximize);
                }
                self.last_drag_press = Some((event.time, loc));
                return Some(DecorationAction::StartMove);
            }
            self.last_drag_press = None;
            self.pressed = Some(zone);
            return None;
        }

        self.pressed = None;

        if self.menu_open {
            let item = self.menu_item_at(loc).and_then(|i| MenuItem::ALL.get(i));
            let action = item.map(|&item| DecorationAction::Run(item));
            self.menu_open = false;
            self.menu_opened_at = None;
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
        loc.x >= 0.0
            && loc.x < f64::from(self.width)
            && loc.y >= 0.0
            && loc.y < f64::from(self.height)
    }

    #[allow(clippy::too_many_lines)]
    #[profiling::function]
    pub(crate) fn redraw(&mut self, painter: &Painter) -> bool {
        if self.width == 0 || self.height == 0 {
            return false;
        }

        let hovered = self.hovered_zone();
        let pressed = self.pressed;
        #[allow(clippy::cast_possible_truncation)]
        let hover_t = self.button_hover_progress() as f32;
        let focus_fade_t = self.focus_fade_t();

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let menu_open_bits = (self.menu_open_progress().clamp(0.0, 1.0) * 8.0).round() as u32;

        let snapshot = RedrawSnapshot {
            width: self.width,
            height: self.height,
            scale: self.effective_scale(),
            focused: self.focused,
            maximized: self.maximized,
            theme_rev: self.theme_rev,
            painter_gen: crate::paint::painter_generation(),
            title: self.title.clone(),
            hovered,
            hover_t_bits: hover_t.to_bits(),
            focus_fade_bits: focus_fade_t.to_bits(),
            pressed,
            dragging: self.dragging,
            menu_open: self.menu_open,
            menu_width: self.menu_width,
            menu_hover: self.menu_hover,
            menu_open_bits,
        };
        if self.last_redraw.as_ref() == Some(&snapshot) {
            return false;
        }
        self.last_redraw = Some(snapshot);

        let theme = &self.theme;
        let scale = self.effective_scale();
        let mix = |from: crate::config::Color, to: crate::config::Color| {
            let mut out = from;
            for (channel, target) in out.0.iter_mut().zip(to.0) {
                *channel += (target - *channel) * focus_fade_t;
            }
            out
        };
        let (bg_color, title_color) = if self.focused {
            (
                mix(theme.background, theme.background_focused),
                mix(theme.title, theme.title_focused),
            )
        } else {
            (
                mix(theme.background_focused, theme.background),
                mix(theme.title_focused, theme.title),
            )
        };

        let bg_color = if self.dragging {
            bg_color.lerp(
                crate::config::Color([1.0, 1.0, 1.0, bg_color.0[3]]),
                glide_theme::motion::ST_DRAG,
            )
        } else {
            bg_color
        };

        let corner_radius = if self.maximized {
            0.0
        } else {
            theme.corner_radius
        };

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
                let fg = title_color.to_buffer_bytes();
                Painter::fill_panel_background(
                    pixels,
                    w,
                    h,
                    scale,
                    bg_color,
                    corner_radius,
                    [true, true, false, false],
                );
                let x_min = BUTTON_PAD_X + BUTTON_SIZE + TITLE_PADDING;
                let x_max = w - BUTTON_PAD_X - BUTTON_SIZE - TITLE_PADDING;
                painter.draw_label_over_styled(
                    pixels,
                    w,
                    h,
                    x_min,
                    x_max,
                    scale,
                    align,
                    &title,
                    fg,
                    &glide_theme::TextStyle::WIN_TITLE,
                );
                Ok::<_, ()>(vec![Rectangle::from_size((pw, ph).into())])
            });
        }

        let maximize_icon: fn(&mut [u8], i32, i32, i32, Color, Color) = if self.maximized {
            Painter::draw_icon_restore
        } else {
            Painter::draw_icon_maximize
        };

        for (buf, zone, normal, hover_bg, fg, fg_hover, icon) in &mut [
            (
                &mut self.maximize_buf,
                HitZone::Maximize,
                theme.maximize_button,
                theme.maximize_button_hover,
                theme.icon,
                crate::config::Color([1.0, 1.0, 1.0, 1.0]),
                maximize_icon,
            ),
            (
                &mut self.close_buf,
                HitZone::Close,
                theme.close_button,
                theme.close_button_hover,
                theme.close_icon,
                crate::config::Color([1.0, 1.0, 1.0, 1.0]),
                Painter::draw_icon_close,
            ),
        ] {
            let is_hovered = hovered == Some(*zone);
            let is_pressed = pressed == Some(*zone);
            let (btn_bg, btn_fg) = button_colors(
                *normal,
                *hover_bg,
                *fg,
                *fg_hover,
                hover_t,
                is_hovered,
                is_pressed,
                self.focused,
            );
            redraw_button(buf, scale, btn_bg, btn_fg, is_pressed, *icon);
        }

        if self.menu_open {
            let container_h = self.menu_container_height();
            let (pw, ph) = (MENU_WIDTH * scale, container_h * scale);
            let buf = self.menu_buf.get_or_insert_with(|| {
                MemoryRenderBuffer::new(Fourcc::Argb8888, (pw, ph), scale, Transform::Normal, None)
            });
            let mut ctx = buf.render();
            ctx.resize((pw, ph));

            let menu_hover = self.menu_hover;
            let menu_bg = theme.menu_background;
            let menu_border = theme.menu_border;
            let menu_row_hover_bg = theme.menu_item_hover;
            let menu_text = theme.menu_text;
            let menu_text_danger = theme.menu_text_danger;
            let menu_accel_text = theme.menu_accel_text.to_buffer_bytes();
            let maximized = self.maximized;

            let _ = ctx.draw(|pixels| {
                Painter::fill_panel_background(
                    pixels,
                    MENU_WIDTH,
                    container_h,
                    scale,
                    menu_bg,
                    MENU_CONTAINER_RADIUS,
                    [true; 4],
                );
                #[allow(clippy::cast_precision_loss)]
                crate::paint::draw_inset_border(
                    pixels,
                    pw,
                    ph,
                    scale as f32,
                    MENU_CONTAINER_RADIUS * scale as f32,
                    menu_border,
                );

                for (i, item) in MenuItem::ALL.iter().enumerate() {
                    let row_top =
                        MENU_CONTAINER_PADDING + i32::try_from(i).unwrap_or(0) * MENU_ROW_HEIGHT;
                    let row_left = MENU_CONTAINER_PADDING;
                    let row_w = MENU_WIDTH - MENU_CONTAINER_PADDING * 2;

                    if menu_hover == Some(i) {
                        Painter::fill_rounded_rect_over(
                            pixels,
                            MENU_WIDTH,
                            container_h,
                            row_left,
                            row_top,
                            row_w,
                            MENU_ROW_HEIGHT,
                            scale,
                            menu_row_hover_bg,
                            MENU_CORNER_RADIUS,
                        );
                    }

                    let label_color = if item.is_danger() {
                        menu_text_danger
                    } else {
                        menu_text
                    }
                    .to_buffer_bytes();

                    let row_bottom = row_top + MENU_ROW_HEIGHT;

                    let accel = item.accel().unwrap_or("");
                    #[allow(clippy::cast_possible_truncation)]
                    let accel_w = if accel.is_empty() {
                        0
                    } else {
                        painter.text_width_styled(accel, &glide_theme::TextStyle::CAP) as i32
                            + MENU_PADDING
                    };

                    let label_x_min = row_left + MENU_PADDING;
                    let label_x_max = (row_left + row_w - MENU_PADDING - accel_w).max(label_x_min);
                    painter.draw_label_over_styled_in_band(
                        pixels,
                        MENU_WIDTH,
                        container_h,
                        label_x_min,
                        label_x_max,
                        row_top,
                        row_bottom,
                        scale,
                        TextAlign::Left,
                        item.label(maximized),
                        label_color,
                        &glide_theme::TextStyle::MENU_ITEM,
                    );

                    if !accel.is_empty() {
                        let accel_x_max = row_left + row_w - MENU_PADDING;
                        let accel_x_min = (accel_x_max - accel_w + MENU_PADDING).max(label_x_max);
                        painter.draw_label_over_styled_in_band(
                            pixels,
                            MENU_WIDTH,
                            container_h,
                            accel_x_min,
                            accel_x_max,
                            row_top,
                            row_bottom,
                            scale,
                            TextAlign::Left,
                            accel,
                            menu_accel_text,
                            &glide_theme::TextStyle::CAP,
                        );
                    }
                }

                Ok::<_, ()>(vec![Rectangle::from_size((pw, ph).into())])
            });
        } else {
            self.menu_buf = None;
        }

        true
    }
}

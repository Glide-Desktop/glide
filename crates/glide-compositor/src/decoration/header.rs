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
    BUTTON_HOVER_ANIM, BUTTON_PAD_X, BUTTON_PAD_Y, BUTTON_SIZE, MENU_CORNER_RADIUS, MENU_MIN_WIDTH,
    MENU_PADDING, PRESSED_DARKEN, RESIZE_BORDER, TITLE_PADDING, menu::MenuItem,
    render::redraw_button,
};

#[derive(Debug, Default)]
pub struct HeaderBar {
    pub(crate) height: i32,
    pub(crate) width: i32,
    pub(crate) content_height: i32,
    pub(crate) title: String,
    pub(crate) pointer_loc: Option<Point<f64, Logical>>,
    pub(crate) menu_open: bool,
    pub(crate) menu_hover: Option<usize>,
    pub(crate) menu_x: i32,
    pub(crate) menu_width: i32,
    pub(crate) pressed: Option<HitZone>,
    pub(crate) button_hover: glide_ui::HoverAnimation<HitZone>,

    pub(crate) theme: DecorationTheme,
    pub(crate) focused: bool,
    pub(crate) maximized: bool,
    pub(crate) scale: i32,

    pub(crate) titlebar_buf: Option<MemoryRenderBuffer>,
    pub(crate) maximize_buf: Option<MemoryRenderBuffer>,
    pub(crate) close_buf: Option<MemoryRenderBuffer>,
    pub(crate) menu_row_bufs: Vec<MemoryRenderBuffer>,
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

pub(crate) const fn row_corners(index: usize, total: usize) -> [bool; 4] {
    let is_first = index == 0;
    let is_last = index + 1 == total;
    [is_first, is_first, is_last, is_last]
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
            let s1 = i32::try_from(self.theme.shadow_size).unwrap_or(i32::MAX);
            let s2 = i32::try_from(self.theme.shadow_size_focused).unwrap_or(i32::MAX);
            s1.max(s2)
        } else {
            0
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
        }
        self.focused = focused;
        self.maximized = maximized;
        if self.scale != scale {
            self.scale = scale;
            self.titlebar_buf = None;
            self.maximize_buf = None;
            self.close_buf = None;
            self.menu_row_bufs.clear();
        }
    }

    pub(crate) const fn effective_scale(&self) -> i32 {
        if self.scale < 1 { 1 } else { self.scale }
    }

    pub(crate) fn button_hover_progress(&mut self, hovered: Option<HitZone>) -> f64 {
        self.button_hover.set(hovered);
        self.button_hover.progress(BUTTON_HOVER_ANIM)
    }

    #[must_use]
    pub fn is_hover_animating(&self) -> bool {
        self.button_hover.is_animating(BUTTON_HOVER_ANIM)
    }

    pub fn set_title(&mut self, title: String) {
        if self.title != title {
            self.title = title;
            self.titlebar_buf = None;
        }
    }

    fn titlebar_row(&self) -> [(HitZone, glide_geom::Rectangle); 3] {
        let max_rect = glide_geom::Rectangle::from_loc_and_size(
            BUTTON_PAD_X,
            BUTTON_PAD_Y,
            BUTTON_SIZE,
            BUTTON_SIZE,
        );
        let close_rect = glide_geom::Rectangle::from_loc_and_size(
            self.width - BUTTON_PAD_X - BUTTON_SIZE,
            BUTTON_PAD_Y,
            BUTTON_SIZE,
            BUTTON_SIZE,
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
            self.menu_x,
            self.height,
            self.menu_width,
            self.height * i32::try_from(row_count).unwrap_or(0),
        );
        let tracks = vec![glide_ui::Track::Fixed(self.height); row_count];
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
    }

    pub fn pointer_leave(&mut self) {
        self.pointer_loc = None;
        self.menu_hover = None;
        self.pressed = None;
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
                self.menu_open = true;
                self.menu_x =
                    (loc.x as i32 - self.menu_width / 2).clamp(0, self.width - self.menu_width);
                return None;
            }

            let zone = self.zone_at(loc.x, loc.y);
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
    pub(crate) fn redraw(&mut self, painter: &Painter) {
        if self.width == 0 || self.height == 0 {
            return;
        }

        let hovered = self
            .pointer_loc
            .filter(|l| self.in_titlebar_strip(*l))
            .map(|l| self.zone_at(l.x, l.y));
        let pressed = self.pressed;
        #[allow(clippy::cast_possible_truncation)]
        let hover_t = self.button_hover_progress(hovered) as f32;

        let theme = &self.theme;
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
                let x_min = BUTTON_PAD_X + BUTTON_SIZE + TITLE_PADDING;
                let x_max = w - BUTTON_PAD_X - BUTTON_SIZE - TITLE_PADDING;
                painter.draw_label_over(pixels, w, h, x_min, x_max, scale, align, &title, fg);
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
                crate::config::Color([1.0, 1.0, 1.0, 1.0]), // --acc-fg (#ffffff)
                maximize_icon,
            ),
            (
                &mut self.close_buf,
                HitZone::Close,
                theme.close_button,
                theme.close_button_hover,
                theme.close_icon,
                crate::config::Color([1.0, 1.0, 1.0, 1.0]), // #fff
                Painter::draw_icon_close,
            ),
        ] {
            let (btn_bg, btn_fg) = if pressed == Some(*zone) {
                (hover_bg.darken(PRESSED_DARKEN), *fg_hover)
            } else if hovered == Some(*zone) {
                (normal.lerp(*hover_bg, hover_t), fg.lerp(*fg_hover, hover_t))
            } else {
                (*normal, *fg)
            };
            redraw_button(buf, scale, btn_bg, btn_fg, *icon);
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

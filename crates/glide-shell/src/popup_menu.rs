use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point, Rectangle, Size},
};

use glide_paint::TextAlign;
use glide_theme::Color;

use crate::raster::{Canvas, rect};

pub const WIDTH: i32 = 224;
pub const PADDING: i32 = glide_theme::spacing::SP1;
pub const ROW_HEIGHT: i32 = 34;
pub const HEADER_HEIGHT: i32 = 35;
pub const ROW_PAD_X: i32 = glide_theme::spacing::SP3;
pub const GAP: i32 = glide_theme::spacing::SP2;
pub const EDGE_MARGIN: i32 = glide_theme::spacing::SP2;
pub const RADIUS: f32 = glide_theme::shape::R3;
pub const ROW_RADIUS: f32 = glide_theme::shape::R2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuColors {
    pub background: Color,
    pub border: Color,
    pub text: Color,
    pub text_muted: Color,
    pub text_danger: Color,
    pub item_hover: Color,
    pub appearance: glide_theme::Appearance,
}

impl MenuColors {
    #[must_use]
    pub const fn from_dock(theme: &glide_theme::DockTheme) -> Self {
        Self {
            background: theme.menu_background,
            border: theme.menu_border,
            text: theme.menu_text,
            text_muted: theme.menu_text_muted,
            text_danger: theme.menu_text_danger,
            item_hover: theme.menu_item_hover,
            appearance: theme.appearance,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MenuRow {
    pub label: std::borrow::Cow<'static, str>,
    pub danger: bool,
    pub muted: bool,
}

#[must_use]
pub fn text_px(painter: &glide_paint::Painter, text: &str, style: &glide_theme::TextStyle) -> i32 {
    painter.text_width_styled(text, style).ceil() as i32
}

#[must_use]
pub fn height(rows: usize) -> i32 {
    PADDING * 2 + HEADER_HEIGHT + i32::try_from(rows).unwrap_or(0) * ROW_HEIGHT
}

#[must_use]
pub fn rows_fitting_above(anchor: Rectangle<i32, Logical>, area: Rectangle<i32, Logical>) -> usize {
    let space = anchor.loc.y - GAP - (area.loc.y + EDGE_MARGIN) - PADDING * 2 - HEADER_HEIGHT;
    usize::try_from(space / ROW_HEIGHT).unwrap_or(0)
}

#[must_use]
pub fn place_above(
    anchor: Rectangle<i32, Logical>, area: Rectangle<i32, Logical>, rows: usize,
) -> Rectangle<i32, Logical> {
    let height = height(rows);
    let min_x = area.loc.x + EDGE_MARGIN;
    let max_x = (area.loc.x + area.size.w - EDGE_MARGIN - WIDTH).max(min_x);
    let x = (anchor.loc.x + (anchor.size.w - WIDTH) / 2).clamp(min_x, max_x);
    let y = (anchor.loc.y - GAP - height).max(area.loc.y + EDGE_MARGIN);
    Rectangle::new(Point::from((x, y)), Size::from((WIDTH, height)))
}

#[must_use]
pub fn row_rect(menu: Rectangle<i32, Logical>, row: usize) -> Rectangle<i32, Logical> {
    let row = i32::try_from(row).unwrap_or(0);
    Rectangle::new(
        Point::from((menu.loc.x + PADDING, menu.loc.y + PADDING + HEADER_HEIGHT + row * ROW_HEIGHT)),
        Size::from((WIDTH - PADDING * 2, ROW_HEIGHT)),
    )
}

#[must_use]
pub fn row_at(menu: Rectangle<i32, Logical>, rows: usize, pos: Point<f64, Logical>) -> Option<usize> {
    (0..rows).find(|row| row_rect(menu, *row).to_f64().contains(pos))
}

#[must_use]
pub fn draw(
    painter: &glide_paint::Painter, colors: &MenuColors, title: &str, rows: &[MenuRow], hovered: Option<usize>,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let (w, h) = (WIDTH, height(rows.len()));
    let mut c = Canvas::popover((w, h), scale.max(1), RADIUS, colors.background, colors.border)?;
    let (left, right) = (PADDING + ROW_PAD_X, w - PADDING - ROW_PAD_X);
    let head = (left, right, PADDING, PADDING + HEADER_HEIGHT);
    c.text(painter, head, TextAlign::Left, title, colors.text_muted, &glide_theme::TextStyle::CAP);
    let sp1 = glide_theme::spacing::SP1;
    c.fill(rect(PADDING + sp1, PADDING + HEADER_HEIGHT - 1, w - PADDING * 2 - sp1 * 2, 1), colors.border, 0.0);

    for (row, item) in rows.iter().enumerate() {
        let top = PADDING + HEADER_HEIGHT + i32::try_from(row).unwrap_or(0) * ROW_HEIGHT;
        if hovered == Some(row) {
            c.fill(rect(PADDING, top, w - PADDING * 2, ROW_HEIGHT), colors.item_hover, ROW_RADIUS);
        }
        let color = if item.danger {
            colors.text_danger
        } else if item.muted {
            colors.text_muted
        } else {
            colors.text
        };
        let band = (left, right, top, top + ROW_HEIGHT);
        c.text(painter, band, TextAlign::Left, &item.label, color, &glide_theme::TextStyle::MENU_ITEM);
    }
    Some(c.finish())
}

#[must_use]
pub fn shadow_margin() -> i32 {
    let widest = [glide_theme::Appearance::Light, glide_theme::Appearance::Dark]
        .into_iter()
        .flat_map(|appearance| glide_theme::elevation::layers(glide_theme::elevation::Level::El3, appearance))
        .map(|layer| layer.blur)
        .fold(0.0_f32, f32::max)
        .ceil() as i32;
    glide_paint::shadow_fade_margin(widest)
}

#[must_use]
pub fn draw_shadow(
    size: Size<i32, Logical>, radius: f32, output_scale: i32, appearance: glide_theme::Appearance,
) -> Option<MemoryRenderBuffer> {
    draw_elevation(size, radius, output_scale, glide_theme::elevation::Level::El3, appearance)
}

#[must_use]
pub fn draw_elevation(
    size: Size<i32, Logical>, radius: f32, output_scale: i32, level: glide_theme::elevation::Level,
    appearance: glide_theme::Appearance,
) -> Option<MemoryRenderBuffer> {
    let layers = glide_theme::elevation::layers(level, appearance);
    Some(crate::raster::shadow((size.w, size.h), shadow_margin(), radius, &layers, output_scale)?.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 0)), Size::from((1920, 1080)))
    }

    #[test]
    fn the_menu_sits_above_its_anchor_and_stays_inside_the_area() {
        let anchor = Rectangle::new(Point::from((1900, 1000)), Size::from((42, 42)));
        let menu = place_above(anchor, area(), 3);
        assert_eq!(menu.size.h, height(3));
        assert!(menu.loc.y + menu.size.h < anchor.loc.y);
        assert!(menu.loc.x + menu.size.w <= area().size.w - EDGE_MARGIN);

        let left = Rectangle::new(Point::from((0, 1000)), Size::from((42, 42)));
        assert_eq!(place_above(left, area(), 1).loc.x, EDGE_MARGIN);
    }

    #[test]
    fn the_fitting_row_count_keeps_the_menu_above_its_anchor() {
        let area = Rectangle::new(Point::from((0, 32)), Size::from((1366, 736)));
        let anchor = Rectangle::new(Point::from((600, 700)), Size::from((56, 56)));
        let rows = rows_fitting_above(anchor, area);
        assert!(rows > 0);
        let fits = place_above(anchor, area, rows);
        assert!(fits.loc.y >= area.loc.y + EDGE_MARGIN);
        assert!(fits.loc.y + fits.size.h <= anchor.loc.y - GAP);
        let over = place_above(anchor, area, rows + 1);
        assert!(over.loc.y + over.size.h > anchor.loc.y - GAP);

        let squeezed = Rectangle::new(Point::from((600, 40)), Size::from((56, 56)));
        assert_eq!(rows_fitting_above(squeezed, area), 0);
    }

    #[test]
    fn rows_are_hit_in_order_and_the_header_hits_nothing() {
        let menu = place_above(Rectangle::new(Point::from((500, 900)), Size::from((42, 42))), area(), 2);
        let header = Point::from((f64::from(menu.loc.x) + 20.0, f64::from(menu.loc.y + PADDING) + 5.0));
        assert_eq!(row_at(menu, 2, header), None);
        for row in 0..2 {
            let rect = row_rect(menu, row);
            let centre =
                Point::from((f64::from(rect.loc.x + rect.size.w / 2), f64::from(rect.loc.y + rect.size.h / 2)));
            assert_eq!(row_at(menu, 2, centre), Some(row));
        }
    }
}

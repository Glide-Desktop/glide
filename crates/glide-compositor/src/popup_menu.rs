use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point, Rectangle, Size, Transform},
};

use crate::config::Color;

pub const WIDTH: i32 = 224;
pub const PADDING: i32 = glide_theme::spacing::SP1;
pub const ROW_HEIGHT: i32 = 34;
pub const HEADER_HEIGHT: i32 = 30;
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MenuRow {
    pub label: &'static str,
    pub danger: bool,
}

#[must_use]
pub fn height(rows: usize) -> i32 {
    PADDING * 2 + HEADER_HEIGHT + i32::try_from(rows).unwrap_or(0) * ROW_HEIGHT
}

#[must_use]
pub fn place_above(
    anchor: Rectangle<i32, Logical>,
    area: Rectangle<i32, Logical>,
    rows: usize,
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
        Point::from((
            menu.loc.x + PADDING,
            menu.loc.y + PADDING + HEADER_HEIGHT + row * ROW_HEIGHT,
        )),
        Size::from((WIDTH - PADDING * 2, ROW_HEIGHT)),
    )
}

#[must_use]
pub fn row_at(
    menu: Rectangle<i32, Logical>,
    rows: usize,
    pos: Point<f64, Logical>,
) -> Option<usize> {
    (0..rows).find(|row| row_rect(menu, *row).to_f64().contains(pos))
}

#[must_use]
pub fn draw(
    painter: &crate::paint::Painter,
    colors: &MenuColors,
    title: &str,
    rows: &[MenuRow],
    hovered: Option<usize>,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let (w, h) = (WIDTH, height(rows.len()));
    let scale = scale.max(1);
    let pixels = usize::try_from(w * scale * h * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    crate::paint::Painter::fill_panel_background(
        &mut buf,
        w,
        h,
        scale,
        colors.background,
        RADIUS,
        [true; 4],
    );
    crate::paint::stroke_rounded_rect_over(
        &mut buf,
        w,
        h,
        0,
        0,
        w,
        h,
        scale,
        1.0,
        RADIUS,
        colors.border,
    );

    let row_left = PADDING;
    let row_w = w - PADDING * 2;
    painter.draw_label_over_styled_in_band(
        &mut buf,
        w,
        h,
        row_left + ROW_PAD_X,
        row_left + row_w - ROW_PAD_X,
        PADDING,
        PADDING + HEADER_HEIGHT,
        scale,
        crate::paint::TextAlign::Left,
        title,
        colors.text_muted.to_buffer_bytes(),
        &glide_theme::TextStyle::CAP,
    );
    crate::paint::Painter::fill_rounded_rect_over(
        &mut buf,
        w,
        h,
        row_left + glide_theme::spacing::SP1,
        PADDING + HEADER_HEIGHT - 1,
        row_w - glide_theme::spacing::SP1 * 2,
        1,
        scale,
        colors.border,
        0.0,
    );

    for (row, item) in rows.iter().enumerate() {
        let top = PADDING + HEADER_HEIGHT + i32::try_from(row).unwrap_or(0) * ROW_HEIGHT;
        if hovered == Some(row) {
            crate::paint::Painter::fill_rounded_rect_over(
                &mut buf,
                w,
                h,
                row_left,
                top,
                row_w,
                ROW_HEIGHT,
                scale,
                colors.item_hover,
                ROW_RADIUS,
            );
        }
        let color = if item.danger {
            colors.text_danger
        } else {
            colors.text
        };
        painter.draw_label_over_styled_in_band(
            &mut buf,
            w,
            h,
            row_left + ROW_PAD_X,
            row_left + row_w - ROW_PAD_X,
            top,
            top + ROW_HEIGHT,
            scale,
            crate::paint::TextAlign::Left,
            item.label,
            color.to_buffer_bytes(),
            &glide_theme::TextStyle::MENU_ITEM,
        );
    }

    Some(MemoryRenderBuffer::from_slice(
        &buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (w * scale, h * scale),
        scale,
        Transform::Normal,
        None,
    ))
}

fn shadow_layers() -> Vec<glide_theme::elevation::ShadowLayer> {
    glide_theme::elevation::layers(
        glide_theme::elevation::Level::El3,
        glide_theme::Appearance::Dark,
    )
}

#[must_use]
pub fn shadow_margin() -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let widest = shadow_layers()
        .iter()
        .map(|layer| layer.blur)
        .fold(0.0_f32, f32::max)
        .ceil() as i32;
    crate::paint::shadow_fade_margin(widest)
}

#[must_use]
pub fn draw_shadow(
    size: Size<i32, Logical>,
    radius: f32,
    output_scale: i32,
) -> Option<MemoryRenderBuffer> {
    let margin = shadow_margin();
    let (w, h) = (size.w + margin * 2, size.h + margin * 2);
    let (pw, ph) = (w * output_scale, h * output_scale);
    let mut buf = vec![0u8; usize::try_from(pw * ph * 4).ok()?];

    #[allow(clippy::cast_precision_loss)]
    let factor = output_scale as f32;
    let layers: Vec<_> = shadow_layers()
        .into_iter()
        .map(|layer| crate::paint::ShadowLayerSpec {
            dy: layer.dy * factor,
            blur: layer.blur * factor,
            color: layer.color,
        })
        .collect();
    #[allow(clippy::cast_precision_loss)]
    crate::paint::draw_outset_shadow_and_border(
        &mut buf,
        pw,
        ph,
        factor * size.w as f32,
        factor * size.h as f32,
        &layers,
        0.0,
        Color([0.0, 0.0, 0.0, 0.0]),
        radius * factor,
        Some(radius * factor),
    );

    Some(MemoryRenderBuffer::from_slice(
        &buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (pw, ph),
        output_scale,
        Transform::Normal,
        None,
    ))
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
    fn rows_are_hit_in_order_and_the_header_hits_nothing() {
        let menu = place_above(
            Rectangle::new(Point::from((500, 900)), Size::from((42, 42))),
            area(),
            2,
        );
        let header = Point::from((
            f64::from(menu.loc.x) + 20.0,
            f64::from(menu.loc.y + PADDING) + 5.0,
        ));
        assert_eq!(row_at(menu, 2, header), None);
        for row in 0..2 {
            let rect = row_rect(menu, row);
            let centre = Point::from((
                f64::from(rect.loc.x + rect.size.w / 2),
                f64::from(rect.loc.y + rect.size.h / 2),
            ));
            assert_eq!(row_at(menu, 2, centre), Some(row));
        }
    }
}

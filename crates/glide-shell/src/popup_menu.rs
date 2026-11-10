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

pub const GLYPH: i32 = 16;
pub const GLYPH_GAP: i32 = glide_theme::spacing::SP2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuGlyph {
    Window,
    Windows,
    Open,
    NewWindow,
    Action,
    Pin,
    Unpin,
    Hide,
    Quit,
}

impl MenuGlyph {
    const fn body(self) -> &'static str {
        match self {
            Self::Window => "<rect x='3.5' y='5' width='17' height='14' rx='2.5'/><path d='M3.5 9.5h17'/>",
            Self::Windows => {
                "<rect x='3' y='8' width='13' height='11' rx='2'/><path d='M8 8V7a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-3'/>"
            }
            Self::Open => {
                "<path d='M14 4.5h5.5V10'/><path d='M19.5 4.5 11 13'/><path d='M18 14v3.5a2 2 0 0 1-2 2H6.5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2H10'/>"
            }
            Self::NewWindow => "<rect x='3.5' y='5' width='17' height='14' rx='2.5'/><path d='M12 9v6M9 12h6'/>",
            Self::Action => "<path d='M13 3.5 5.5 13.5H12l-1 7 7.5-10H12z'/>",
            Self::Pin => "<path d='M9 4.5h6l-.8 5.2 3 3.3H6.8l3-3.3z'/><path d='M12 13v6.5'/>",
            Self::Unpin => {
                "<path d='M9 4.5h6l-.8 5.2 3 3.3H6.8l3-3.3z'/><path d='M12 13v6.5'/><path d='M4.5 4.5l15 15'/>"
            }
            Self::Hide => "<path d='M5 12h14'/>",
            Self::Quit => "<path d='M6.5 6.5l11 11M17.5 6.5l-11 11'/>",
        }
    }

    fn svg(self) -> String {
        format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='1.75' stroke-linecap='round' stroke-linejoin='round'>{}</svg>",
            self.body()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MenuRow {
    pub label: std::borrow::Cow<'static, str>,
    pub danger: bool,
    pub muted: bool,
    pub glyph: Option<MenuGlyph>,
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
    Some(paint(painter, colors, title, rows, hovered, scale)?.finish())
}

fn paint(
    painter: &glide_paint::Painter, colors: &MenuColors, title: &str, rows: &[MenuRow], hovered: Option<usize>,
    scale: i32,
) -> Option<Canvas<'static>> {
    let (w, h) = (WIDTH, height(rows.len()));
    let mut c = Canvas::popover((w, h), scale.max(1), RADIUS, colors.background, colors.border)?;
    let (left, right) = (PADDING + ROW_PAD_X, w - PADDING - ROW_PAD_X);
    let text_left = if rows.iter().any(|row| row.glyph.is_some()) { left + GLYPH + GLYPH_GAP } else { left };
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
        if let Some(glyph) = item.glyph
            && let Some(pixels) = glide_paint::render_svg_tinted(glyph.svg().as_bytes(), GLYPH * c.scale, color)
        {
            c.blit_square(&pixels, GLYPH, (left, top + (ROW_HEIGHT - GLYPH) / 2));
        }
        let band = (text_left, right, top, top + ROW_HEIGHT);
        c.text(painter, band, TextAlign::Left, &item.label, color, &glide_theme::TextStyle::MENU_ITEM);
    }
    Some(c)
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

    fn row(label: &'static str, glyph: Option<MenuGlyph>) -> MenuRow {
        MenuRow { label: label.into(), danger: glyph == Some(MenuGlyph::Quit), muted: false, glyph }
    }

    #[test]
    fn every_glyph_renders_and_a_row_with_one_paints_ink_in_its_leading_slot() {
        let painter = glide_paint::Painter::new(14.0, None);
        let theme = glide_theme::DockTheme::default();
        let colors = MenuColors::from_dock(&theme);
        for glyph in [
            MenuGlyph::Window,
            MenuGlyph::Windows,
            MenuGlyph::Open,
            MenuGlyph::NewWindow,
            MenuGlyph::Action,
            MenuGlyph::Pin,
            MenuGlyph::Unpin,
            MenuGlyph::Hide,
            MenuGlyph::Quit,
        ] {
            let plain = paint(&painter, &colors, "App", &[row("", None)], None, 1).expect("canvas");
            let drawn = paint(&painter, &colors, "App", &[row("", Some(glyph))], None, 1).expect("canvas");
            assert_ne!(plain.buf, drawn.buf, "{glyph:?} leaves a mark");
        }
    }

    #[test]
    #[ignore = "writes preview PNGs to $GLIDE_MENU_PREVIEW"]
    fn preview() {
        let Some(dir) = std::env::var_os("GLIDE_MENU_PREVIEW") else {
            return;
        };
        let painter = glide_paint::Painter::new(14.0, None);
        let entries = [
            ("Inbox — Mail", MenuGlyph::Window),
            ("Чернетка листа до команди", MenuGlyph::Window),
            ("Показати всі вікна", MenuGlyph::Windows),
            ("Нове вікно", MenuGlyph::NewWindow),
            ("Нове приватне вікно", MenuGlyph::Action),
            ("Відкріпити від дока", MenuGlyph::Unpin),
            ("Сховати", MenuGlyph::Hide),
            ("Завершити", MenuGlyph::Quit),
        ];
        for appearance in [glide_theme::Appearance::Dark, glide_theme::Appearance::Light] {
            let tokens = glide_theme::Tokens::new(appearance, glide_theme::Accent::Blue);
            let colors = MenuColors::from_dock(&glide_theme::DockTheme::from_tokens(&tokens));
            let scale = 2;
            let (gap, pad) = (24, 24);
            let menu_h = height(entries.len());
            let (w, h) = (pad * 2 + WIDTH * 2 + gap, pad * 2 + menu_h);
            let mut sheet = Canvas::new(w, h, scale).expect("canvas");
            let full = sheet.full();
            sheet.fill(full, tokens.win_bg, 0.0);
            for (column, with_glyphs) in [false, true].into_iter().enumerate() {
                let rows: Vec<MenuRow> =
                    entries.iter().map(|(label, glyph)| row(label, with_glyphs.then_some(*glyph))).collect();
                let menu = paint(&painter, &colors, "Firefox", &rows, Some(3), scale).expect("menu");
                let x = pad + i32::try_from(column).unwrap_or(0) * (WIDTH + gap);
                sheet.blit(&menu.buf, rect(x, pad, WIDTH, menu_h));
            }
            let (pw, ph) = (u32::try_from(w * scale).unwrap_or(0), u32::try_from(h * scale).unwrap_or(0));
            let png = crate::image::Image::from_bgra(pw, ph, sheet.buf).expect("image").png().expect("png");
            let name = if appearance == glide_theme::Appearance::Dark { "menu-dark.png" } else { "menu-light.png" };
            std::fs::write(std::path::Path::new(&dir).join(name), png).expect("write");
        }
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

use iced::advanced::renderer::{Quad, Renderer as _};
use iced::advanced::text::{self, Paragraph as _, Renderer as _};
use iced::alignment::Vertical;
use iced::border::Radius;
use iced::{Border, Color, Font, Pixels, Point, Rectangle, Renderer, Size};

pub mod boxes;

const SAMPLE: &str = "MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Grid {
    pub columns: usize,
    pub lines: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub font: Font,
    pub size: f32,
    pub cell: Size,
    pub baseline: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub row: usize,
    pub column: usize,
    pub right_half: bool,
}

impl Metrics {
    #[must_use]
    pub fn measure(font: Font, size: f32, line_height: f32) -> Self {
        let height = (size * line_height).round().max(1.0);
        let paragraph = <Renderer as text::Renderer>::Paragraph::with_text(text::Text {
            content: SAMPLE,
            bounds: Size::INFINITE,
            size: Pixels(size),
            line_height: text::LineHeight::Absolute(Pixels(height)),
            font,
            align_x: text::Alignment::Left,
            align_y: Vertical::Top,
            shaping: text::Shaping::Basic,
            wrapping: text::Wrapping::None,
        });
        let width = paragraph.min_bounds().width / SAMPLE.len() as f32;
        let baseline = ((height - 1.32 * size) / 2.0 + 1.02 * size).round();
        Self { font, size, cell: Size::new(width.max(1.0), height), baseline }
    }

    #[must_use]
    pub fn grid(&self, bounds: Rectangle) -> Grid {
        let fit = |space: f32, cell: f32| (space / cell).floor().max(1.0) as usize;
        Grid { columns: fit(bounds.width, self.cell.width).max(2), lines: fit(bounds.height, self.cell.height) }
    }

    #[must_use]
    pub fn locate(&self, local: Point, grid: Grid) -> Hit {
        let (x, y) = (local.x.max(0.0), local.y.max(0.0));
        let column = ((x / self.cell.width) as usize).min(grid.columns.saturating_sub(1));
        let row = ((y / self.cell.height) as usize).min(grid.lines.saturating_sub(1));
        Hit { row, column, right_half: x - column as f32 * self.cell.width > self.cell.width / 2.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub row: usize,
    pub column: usize,
    pub width: usize,
}

impl Span {
    #[must_use]
    pub const fn new(row: usize, column: usize, width: usize) -> Self {
        Self { row, column, width }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Underline {
    Single,
    Double,
    Curl,
    Dotted,
    Dashed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caret {
    Beam,
    Underline,
    Block,
}

#[must_use]
pub fn thumb(bounds: Rectangle, lines: usize, history: usize, offset: usize) -> Rectangle {
    let track = bounds.height - 8.0;
    let height = (track * lines as f32 / (lines + history) as f32).clamp(24.0_f32.min(track), track);
    let progress = 1.0 - offset as f32 / history.max(1) as f32;
    Rectangle { x: bounds.x + bounds.width - 7.0, y: bounds.y + 4.0 + (track - height) * progress, width: 4.0, height }
}

pub struct Painter<'r> {
    pub renderer: &'r mut Renderer,
    origin: Point,
    metrics: Metrics,
    clip: Rectangle,
    grid: Grid,
    corners: Radius,
    run: String,
    run_at: Span,
    run_style: (Color, Font),
    background: Option<(Span, Color, bool)>,
}

impl<'r> Painter<'r> {
    #[must_use]
    pub fn new(renderer: &'r mut Renderer, bounds: Rectangle, metrics: Metrics, grid: Grid, corners: Radius) -> Self {
        Self {
            renderer,
            origin: bounds.position(),
            metrics,
            clip: bounds,
            grid,
            corners,
            run: String::new(),
            run_at: Span::new(0, 0, 0),
            run_style: (Color::TRANSPARENT, metrics.font),
            background: None,
        }
    }

    #[must_use]
    pub const fn clip(&self) -> Rectangle {
        self.clip
    }

    #[must_use]
    pub const fn metrics(&self) -> Metrics {
        self.metrics
    }

    #[must_use]
    pub fn rect(&self, span: Span) -> Rectangle {
        let cell = self.metrics.cell;
        let left = (self.origin.x + span.column as f32 * cell.width).round();
        let right = (self.origin.x + (span.column + span.width) as f32 * cell.width).round();
        Rectangle {
            x: left,
            y: self.origin.y + span.row as f32 * cell.height,
            width: right - left,
            height: cell.height,
        }
    }

    pub fn quad(&mut self, bounds: Rectangle, color: Color) {
        self.renderer.fill_quad(Quad { bounds, ..Quad::default() }, color);
    }

    pub fn rounded(&mut self, bounds: Rectangle, radius: impl Into<Radius>, color: Color) {
        let border = Border { radius: radius.into(), ..Border::default() };
        self.renderer.fill_quad(Quad { bounds, border, ..Quad::default() }, color);
    }

    pub fn outline(&mut self, bounds: Rectangle, border: Border, fill: Color) {
        self.renderer.fill_quad(Quad { bounds, border, ..Quad::default() }, fill);
    }

    pub fn text(&mut self, content: String, span: Span, color: Color, font: Font, basic: bool) {
        let rect = self.rect(span);
        let at = Point::new(self.origin.x + span.column as f32 * self.metrics.cell.width, rect.y);
        let bounds = Size::new(rect.width + self.metrics.cell.width * 2.0, rect.height);
        self.renderer.fill_text(self.paragraph(content, bounds, font, basic), at, color, self.clip);
    }

    fn paragraph(&self, content: String, bounds: Size, font: Font, basic: bool) -> text::Text<String, Font> {
        text::Text {
            content,
            bounds,
            size: Pixels(self.metrics.size),
            line_height: text::LineHeight::Absolute(Pixels(self.metrics.cell.height)),
            font,
            align_x: text::Alignment::Left,
            align_y: Vertical::Top,
            shaping: if basic { text::Shaping::Basic } else { text::Shaping::Advanced },
            wrapping: text::Wrapping::None,
        }
    }

    pub fn flush(&mut self) {
        let trimmed = self.run.trim_end().len();
        if trimmed > 0 {
            self.run.truncate(trimmed);
            let content = std::mem::take(&mut self.run);
            let span = Span { width: trimmed, ..self.run_at };
            let (color, font) = self.run_style;
            self.text(content, span, color, font, true);
        }
        self.run.clear();
    }

    pub fn glyph(&mut self, at: Span, glyph: char, color: Color, font: Font) {
        let style = (color, font);
        let continues = !self.run.is_empty()
            && self.run_at.row == at.row
            && self.run_at.column + self.run.len() == at.column
            && self.run_style == style;
        if !continues {
            self.flush();
            if glyph == ' ' {
                return;
            }
            self.run_at = at;
            self.run_style = style;
        }
        self.run.push(glyph);
    }

    pub fn symbol(&mut self, at: Span, glyph: char, color: Color) -> bool {
        if !boxes::handles(glyph) {
            return false;
        }
        self.flush();
        let rect = self.rect(at);
        boxes::draw(self.renderer, glyph, rect, color)
    }

    fn bleed(&self, span: Span) -> (Rectangle, Radius) {
        let rect = self.rect(span);
        let clip = self.clip;
        let (left, top) = (span.column == 0, span.row == 0);
        let right = span.column + span.width >= self.grid.columns;
        let bottom = span.row + 1 >= self.grid.lines;
        let x = if left { clip.x } else { rect.x };
        let y = if top { clip.y } else { rect.y };
        let end_x = if right { clip.x + clip.width } else { rect.x + rect.width };
        let end_y = if bottom { clip.y + clip.height } else { rect.y + rect.height };
        let corner = |a: bool, b: bool, radius: f32| if a && b { radius } else { 0.0 };
        let radius = Radius {
            top_left: corner(top, left, self.corners.top_left),
            top_right: corner(top, right, self.corners.top_right),
            bottom_right: corner(bottom, right, self.corners.bottom_right),
            bottom_left: corner(bottom, left, self.corners.bottom_left),
        };
        (Rectangle { x, y, width: end_x - x, height: end_y - y }, radius)
    }

    pub fn fill(&mut self, next: Option<(Span, Color, bool)>) {
        match (self.background, next) {
            (Some((span, bg, bleeds)), Some((more, next_bg, next_bleeds)))
                if span.row == more.row
                    && span.column + span.width == more.column
                    && bg == next_bg
                    && bleeds == next_bleeds =>
            {
                self.background = Some((Span { width: span.width + more.width, ..span }, bg, bleeds));
            }
            (previous, next) => {
                if let Some((span, bg, bleeds)) = previous {
                    let (bounds, radius) = if bleeds { self.bleed(span) } else { (self.rect(span), Radius::default()) };
                    self.rounded(bounds, radius, bg);
                }
                self.background = next;
            }
        }
    }

    pub fn underline(&mut self, span: Span, style: Underline, color: Color) {
        let rect = self.rect(span);
        let y = rect.y + (self.metrics.baseline + 2.0).min(rect.height - 1.0);
        let line = |x: f32, y: f32, width: f32| Rectangle { x, y, width, height: 1.0 };
        match style {
            Underline::Single => self.quad(line(rect.x, y, rect.width), color),
            Underline::Double => {
                self.quad(line(rect.x, y - 1.0, rect.width), color);
                self.quad(line(rect.x, y + 1.0, rect.width), color);
            }
            Underline::Curl | Underline::Dotted | Underline::Dashed => {
                let (dash, gap): (f32, f32) = if style == Underline::Dashed { (4.0, 2.0) } else { (2.0, 2.0) };
                let curl = style == Underline::Curl;
                let mut x = rect.x;
                let mut up = false;
                while x < rect.x + rect.width {
                    let lift = if curl && up { -1.0 } else { 0.0 };
                    self.quad(line(x, y + lift, dash.min(rect.x + rect.width - x)), color);
                    x += dash + if curl { 0.0 } else { gap };
                    up = !up;
                }
            }
        }
    }

    pub fn strike(&mut self, span: Span, color: Color) {
        let rect = self.rect(span);
        self.quad(Rectangle { y: (rect.y + rect.height / 2.0).round(), height: 1.0, ..rect }, color);
    }

    pub fn gutter(&mut self, row: usize, color: Color) {
        let rect = self.rect(Span::new(row, 0, 1));
        self.quad(Rectangle { x: self.clip.x, y: rect.y + 2.0, width: 2.0, height: rect.height - 4.0 }, color);
    }

    pub fn band(&mut self, row: usize, color: Color) {
        let rect = self.rect(Span::new(row, 0, self.grid.columns));
        self.quad(Rectangle { x: self.clip.x, width: self.clip.width, ..rect }, color);
    }

    pub fn caret(&mut self, at: Span, caret: Caret, color: Color, focused: bool) {
        let rect = self.rect(at);
        let stroke = 2.0_f32.min(rect.width);
        match (focused, caret) {
            (true, Caret::Beam) => self.quad(Rectangle { width: stroke, ..rect }, color),
            (true, Caret::Underline) => {
                self.quad(Rectangle { y: rect.y + rect.height - stroke, height: stroke, ..rect }, color);
            }
            (true, Caret::Block) => self.quad(rect, color),
            (false, _) => {
                self.outline(rect, Border { color, width: 1.0, radius: 0.0.into() }, Color::TRANSPARENT);
            }
        }
    }

    pub fn scrollbar(&mut self, lines: usize, history: usize, offset: usize, color: Color) {
        let bar = thumb(self.clip, lines, history, offset);
        self.rounded(bar, crate::theme::R1, color);
    }

    pub fn badges(&mut self, badges: Vec<Badge>) {
        let clip = self.clip;
        let height = self.metrics.cell.height;
        let paragraphs: Vec<_> = badges
            .into_iter()
            .map(|badge| {
                let bounds = Size::new(badge.pill.width + self.metrics.cell.width, height);
                (badge.pill, badge.radius, badge.fill, badge.at, badge.ink, {
                    self.paragraph(badge.label, bounds, badge.font, badge.basic)
                })
            })
            .collect();
        self.renderer.with_layer(clip, |renderer| {
            for (pill, radius, fill, at, ink, paragraph) in paragraphs {
                let border = Border { radius: radius.into(), ..Border::default() };
                renderer.fill_quad(Quad { bounds: pill, border, ..Quad::default() }, fill);
                renderer.fill_text(paragraph, at, ink, clip);
            }
        });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Badge {
    pub pill: Rectangle,
    pub radius: f32,
    pub fill: Color,
    pub label: String,
    pub at: Point,
    pub font: Font,
    pub ink: Color,
    pub basic: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics() -> Metrics {
        Metrics { font: Font::MONOSPACE, size: 13.0, cell: Size::new(8.0, 16.0), baseline: 13.0 }
    }

    #[test]
    fn a_grid_fits_whole_cells_and_keeps_two_columns() {
        let m = metrics();
        assert_eq!(m.grid(Rectangle::with_size(Size::new(81.0, 33.0))), Grid { columns: 10, lines: 2 });
        assert_eq!(m.grid(Rectangle::with_size(Size::new(3.0, 3.0))), Grid { columns: 2, lines: 1 });
    }

    #[test]
    fn a_point_lands_in_its_cell_and_half() {
        let m = metrics();
        let grid = Grid { columns: 10, lines: 5 };
        assert_eq!(m.locate(Point::new(13.0, 17.0), grid), Hit { row: 1, column: 1, right_half: true });
        assert_eq!(m.locate(Point::new(9.0, 0.0), grid), Hit { row: 0, column: 1, right_half: false });
        assert_eq!(m.locate(Point::new(500.0, 500.0), grid), Hit { row: 4, column: 9, right_half: true });
    }

    #[test]
    fn the_scroll_thumb_sits_at_the_bottom_when_not_scrolled() {
        let bounds = Rectangle::with_size(Size::new(100.0, 208.0));
        let live = thumb(bounds, 10, 90, 0);
        assert!((live.y + live.height - 204.0).abs() < 0.01);
        assert_eq!(thumb(bounds, 10, 90, 90).y, 4.0);
        assert!(live.height >= 24.0);
    }
}

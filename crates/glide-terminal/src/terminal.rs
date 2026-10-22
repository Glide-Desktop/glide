use std::cell::RefCell;
use std::time::{Duration, Instant};

use alacritty_terminal::grid::{Dimensions, Indexed, Scroll};
use alacritty_terminal::index::{Column, Direction, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{CursorShape, Rgb};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::text::{self, Paragraph as _, Renderer as _};
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell};
use iced::alignment::Vertical;
use iced::border::Radius;
use iced::{
    Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Renderer, Size, Theme, keyboard, mouse,
};

use crate::boxes;
use crate::hints::Hint;
use crate::input::{self, Button};
use crate::marks::{self, Mark};
use crate::profile::{Palette, color, mix};
use crate::session::{Cells, Listener, Pane, PaneId};

pub const PAD: f32 = glide_theme::spacing::SP2 as f32;
const SAMPLE: &str = "MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM";
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const BAR_ZONE: f32 = 12.0;

#[derive(Debug, Clone)]
pub enum Action {
    Resize(PaneId, Cells, Size),
    Focus(PaneId),
    Selected(PaneId),
    PastePrimary(PaneId),
    Open(String),
    Context(PaneId, Point, Option<String>),
}

fn url_search() -> RegexSearch {
    RegexSearch::new(crate::hints::URLS).expect("the URL pattern compiles")
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fonts {
    pub regular: Font,
    pub size: f32,
    pub line_height: f32,
}

impl Fonts {
    fn styled(self, flags: Flags) -> Font {
        Font {
            weight: if flags.contains(Flags::BOLD) { iced::font::Weight::Bold } else { self.regular.weight },
            style: if flags.contains(Flags::ITALIC) { iced::font::Style::Italic } else { self.regular.style },
            ..self.regular
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub palette: Palette,
    pub fonts: Fonts,
    pub bold_bright: bool,
    pub accent: Color,
    pub bookmark: Color,
}

pub struct Terminal<'a> {
    pane: &'a Pane,
    look: Look,
    focused: bool,
    blink: bool,
    find: Option<&'a str>,
    hints: Option<(&'a [Hint], &'a str)>,
    flash: Option<(Flash, f32)>,
    corners: Radius,
    drop_target: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flash {
    Bell,
    Mark,
}

impl<'a> Terminal<'a> {
    #[must_use]
    pub fn new(pane: &'a Pane, look: Look, focused: bool) -> Self {
        Self {
            pane,
            look,
            focused,
            blink: true,
            find: None,
            hints: None,
            flash: None,
            corners: Radius::default(),
            drop_target: false,
        }
    }

    #[must_use]
    pub fn drop_target(mut self, target: bool) -> Self {
        self.drop_target = target;
        self
    }

    #[must_use]
    pub fn hints(mut self, hints: Option<(&'a [Hint], &'a str)>) -> Self {
        self.hints = hints;
        self
    }

    #[must_use]
    pub fn corners(mut self, corners: Radius) -> Self {
        self.corners = corners;
        self
    }

    #[must_use]
    pub fn blink(mut self, visible: bool) -> Self {
        self.blink = visible;
        self
    }

    #[must_use]
    pub fn find(mut self, pattern: Option<&'a str>) -> Self {
        self.find = pattern;
        self
    }

    #[must_use]
    pub fn flash(mut self, flash: Option<(Flash, f32)>) -> Self {
        self.flash = flash;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Metrics {
    fonts: Fonts,
    cell: Size,
    baseline: f32,
}

impl Metrics {
    fn measure(fonts: Fonts) -> Self {
        let height = (fonts.size * fonts.line_height).round().max(1.0);
        let paragraph = <Renderer as text::Renderer>::Paragraph::with_text(text::Text {
            content: SAMPLE,
            bounds: Size::INFINITE,
            size: Pixels(fonts.size),
            line_height: text::LineHeight::Absolute(Pixels(height)),
            font: fonts.regular,
            align_x: text::Alignment::Left,
            align_y: Vertical::Top,
            shaping: text::Shaping::Basic,
            wrapping: text::Wrapping::None,
        });
        let width = paragraph.min_bounds().width / SAMPLE.len() as f32;
        let baseline = ((height - 1.32 * fonts.size) / 2.0 + 1.02 * fonts.size).round();
        Self { fonts, cell: Size::new(width.max(1.0), height), baseline }
    }

    fn cells(&self, bounds: Rectangle) -> Cells {
        let fit = |space: f32, cell: f32| ((space - 2.0 * PAD) / cell).floor().max(1.0) as usize;
        Cells { columns: fit(bounds.width, self.cell.width).max(2), lines: fit(bounds.height, self.cell.height) }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Link {
    Osc(Hyperlink),
    Text(Match, String),
}

impl Link {
    fn uri(&self) -> &str {
        match self {
            Self::Osc(link) => link.uri(),
            Self::Text(_, uri) => uri,
        }
    }

    fn covers(&self, point: GridPoint, cell: &Cell) -> bool {
        match self {
            Self::Osc(link) => cell.hyperlink().as_ref() == Some(link),
            Self::Text(range, _) => range.contains(&point),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum Drag {
    #[default]
    None,
    Select {
        moved: bool,
        option: bool,
    },
    Scrollbar,
}

#[derive(Default)]
struct State {
    metrics: Option<Metrics>,
    drag: Drag,
    clicks: Option<(Instant, GridPoint, u8)>,
    modifiers: keyboard::Modifiers,
    held: Option<u8>,
    reported: Option<(usize, usize)>,
    wheel: f32,
    hover: Option<Link>,
    near_bar: bool,
    urls: Option<RegexSearch>,
    find: RefCell<Option<(String, Option<RegexSearch>)>>,
}

fn releases_grab(state: &State, button: mouse::Button) -> bool {
    match state.held {
        Some(held) => [mouse::Button::Left, mouse::Button::Middle, mouse::Button::Right][usize::from(held)] == button,
        None => state.drag == Drag::None || button == mouse::Button::Left,
    }
}

struct Hit {
    row: usize,
    column: usize,
    side: Side,
    point: GridPoint,
}

fn locate(local: Point, metrics: &Metrics, cells: Cells, offset: usize) -> Hit {
    let x = (local.x - PAD).max(0.0);
    let y = (local.y - PAD).max(0.0);
    let column = ((x / metrics.cell.width) as usize).min(cells.columns.saturating_sub(1));
    let row = ((y / metrics.cell.height) as usize).min(cells.lines.saturating_sub(1));
    let side = if x - column as f32 * metrics.cell.width > metrics.cell.width / 2.0 { Side::Right } else { Side::Left };
    Hit { row, column, side, point: GridPoint::new(Line(row as i32 - offset as i32), Column(column)) }
}

fn thumb(bounds: Rectangle, lines: usize, history: usize, offset: usize) -> Rectangle {
    let track = bounds.height - 8.0;
    let height = (track * lines as f32 / (lines + history) as f32).clamp(24.0_f32.min(track), track);
    let progress = 1.0 - offset as f32 / history.max(1) as f32;
    Rectangle { x: bounds.x + bounds.width - 7.0, y: bounds.y + 4.0 + (track - height) * progress, width: 4.0, height }
}

fn link_at(term: &Term<Listener>, point: GridPoint, urls: &mut RegexSearch) -> Option<Link> {
    if let Some(link) = term.grid()[point].hyperlink().filter(|link| !marks::is_mark(link.uri())) {
        return Some(Link::Osc(link));
    }
    let start = term.line_search_left(point);
    let end = term.line_search_right(point);
    RegexIter::new(start, end, Direction::Right, term, urls).find(|found| found.contains(&point)).map(|found| {
        let uri = term.bounds_to_string(*found.start(), *found.end());
        Link::Text(found, crate::hints::clean_url(&uri).to_string())
    })
}

fn arrows(term: &Term<Listener>, target: GridPoint) -> Vec<u8> {
    let cursor = term.grid().cursor.point;
    let columns = term.columns() as i64;
    let distance = (i64::from(target.line.0) - i64::from(cursor.line.0)) * columns + target.column.0 as i64
        - cursor.column.0 as i64;
    let step: &[u8] = if distance < 0 { b"\x1b[D" } else { b"\x1b[C" };
    step.repeat(distance.unsigned_abs().min(2000) as usize)
}

impl Terminal<'_> {
    fn report(&self, mode: TermMode, button: Button, pressed: bool, hit: &Hit, modifiers: keyboard::Modifiers) {
        if let Some(bytes) = input::mouse_report(mode, button, pressed, hit.column, hit.row, modifiers) {
            self.pane.write(bytes);
        }
    }

    fn press(
        &self, state: &mut State, button: mouse::Button, local: Point, absolute: Point, bounds: Rectangle,
    ) -> Option<Action> {
        let metrics = state.metrics?;
        let mut term = self.pane.term.lock();
        let mode = *term.mode();
        let hit = locate(local, &metrics, self.pane.cells, term.grid().display_offset());
        let reporting = mode.intersects(TermMode::MOUSE_MODE) && !state.modifiers.shift();
        let focus = (!self.focused).then_some(Action::Focus(self.pane.id));
        match button {
            mouse::Button::Left if local.x > bounds.width - BAR_ZONE && term.history_size() > 0 => {
                state.drag = Drag::Scrollbar;
                drop(term);
                self.drag_scrollbar(local, bounds);
                return focus;
            }
            mouse::Button::Left if reporting => {
                state.held = Some(0);
                self.report(mode, Button::Left, true, &hit, state.modifiers);
                return focus;
            }
            mouse::Button::Left => {}
            mouse::Button::Middle if reporting => {
                state.held = Some(1);
                self.report(mode, Button::Middle, true, &hit, state.modifiers);
                return None;
            }
            mouse::Button::Middle => return Some(Action::PastePrimary(self.pane.id)),
            mouse::Button::Right if reporting => {
                state.held = Some(2);
                self.report(mode, Button::Right, true, &hit, state.modifiers);
                return None;
            }
            mouse::Button::Right => {
                let link = link_at(&term, hit.point, state.urls.get_or_insert_with(url_search));
                return Some(Action::Context(self.pane.id, absolute, link.map(|link| link.uri().to_string())));
            }
            _ => return None,
        }
        if state.modifiers.control()
            && let Some(link) = link_at(&term, hit.point, state.urls.get_or_insert_with(url_search))
        {
            state.hover = None;
            return Some(Action::Open(link.uri().to_string()));
        }
        let now = Instant::now();
        let count = match state.clicks {
            Some((at, point, count)) if now - at < DOUBLE_CLICK && point == hit.point => count % 3 + 1,
            _ => 1,
        };
        state.clicks = Some((now, hit.point, count));
        let option = state.modifiers.alt() && count == 1;
        let kind = match count {
            2 => SelectionType::Semantic,
            3 => SelectionType::Lines,
            _ if option => SelectionType::Block,
            _ => SelectionType::Simple,
        };
        let extend = state.modifiers.shift() && count == 1;
        if let (true, Some(selection)) = (extend, term.selection.as_mut()) {
            selection.update(hit.point, hit.side);
        } else {
            term.selection = Some(Selection::new(kind, hit.point, hit.side));
        }
        state.drag = Drag::Select { moved: count > 1, option };
        focus
    }

    fn drag_scrollbar(&self, local: Point, bounds: Rectangle) {
        let mut term = self.pane.term.lock();
        let history = term.history_size();
        let lines = term.screen_lines();
        let bar = thumb(Rectangle::with_size(bounds.size()), lines, history, 0);
        let track = bounds.height - 8.0 - bar.height;
        let progress = ((local.y - 4.0 - bar.height / 2.0) / track.max(1.0)).clamp(0.0, 1.0);
        let target = ((1.0 - progress) * history as f32).round() as i32;
        let current = term.grid().display_offset() as i32;
        term.scroll_display(Scroll::Delta(target - current));
    }

    fn moved(&self, state: &mut State, local: Point, bounds: Rectangle, over: bool) -> bool {
        let Some(metrics) = state.metrics else { return false };
        let near = over && local.x > bounds.width - BAR_ZONE;
        let mut redraw = near != state.near_bar;
        state.near_bar = near;
        if state.drag == Drag::Scrollbar {
            self.drag_scrollbar(local, bounds);
            return true;
        }
        let mut term = self.pane.term.lock();
        let mode = *term.mode();
        if let Drag::Select { moved, .. } = &mut state.drag {
            if local.y < 0.0 {
                term.scroll_display(Scroll::Delta(1));
            } else if local.y > bounds.height {
                term.scroll_display(Scroll::Delta(-1));
            }
            let hit = locate(local, &metrics, self.pane.cells, term.grid().display_offset());
            if let Some(selection) = term.selection.as_mut() {
                selection.update(hit.point, hit.side);
            }
            *moved = true;
            return true;
        }
        let hit = locate(local, &metrics, self.pane.cells, term.grid().display_offset());
        let motion =
            mode.contains(TermMode::MOUSE_MOTION) || (mode.contains(TermMode::MOUSE_DRAG) && state.held.is_some());
        if motion && !state.modifiers.shift() && state.reported != Some((hit.row, hit.column)) {
            state.reported = Some((hit.row, hit.column));
            self.report(mode, Button::Moved(state.held), true, &hit, state.modifiers);
        }
        let hover = if state.modifiers.control() && over {
            link_at(&term, hit.point, state.urls.get_or_insert_with(url_search))
        } else {
            None
        };
        redraw |= hover != state.hover;
        state.hover = hover;
        redraw
    }

    fn released(&self, state: &mut State, button: mouse::Button, local: Point) -> Option<Action> {
        let metrics = state.metrics?;
        if !releases_grab(state, button) {
            return None;
        }
        let drag = std::mem::take(&mut state.drag);
        let mut term = self.pane.term.lock();
        let mode = *term.mode();
        let hit = locate(local, &metrics, self.pane.cells, term.grid().display_offset());
        if let Some(held) = state.held.take() {
            let button = [Button::Left, Button::Middle, Button::Right][usize::from(held)];
            self.report(mode, button, false, &hit, state.modifiers);
            return None;
        }
        if button != mouse::Button::Left {
            return None;
        }
        match drag {
            Drag::Select { moved: false, option: true } if !mode.contains(TermMode::ALT_SCREEN) => {
                term.selection = None;
                self.pane.write(arrows(&term, hit.point));
                None
            }
            Drag::Select { .. } if term.selection.as_ref().is_some_and(|selection| !selection.is_empty()) => {
                Some(Action::Selected(self.pane.id))
            }
            Drag::Select { .. } => {
                term.selection = None;
                None
            }
            _ => None,
        }
    }

    fn wheel(&self, state: &mut State, delta: mouse::ScrollDelta, local: Point) {
        let Some(metrics) = state.metrics else { return };
        state.wheel += match delta {
            mouse::ScrollDelta::Lines { y, .. } => y * 3.0,
            mouse::ScrollDelta::Pixels { y, .. } => y / metrics.cell.height,
        };
        let whole = state.wheel.trunc();
        state.wheel -= whole;
        if whole == 0.0 {
            return;
        }
        let up = whole > 0.0;
        let count = whole.abs() as usize;
        let mut term = self.pane.term.lock();
        let mode = *term.mode();
        let hit = locate(local, &metrics, self.pane.cells, term.grid().display_offset());
        if mode.intersects(TermMode::MOUSE_MODE) && !state.modifiers.shift() {
            let button = if up { Button::WheelUp } else { Button::WheelDown };
            for _ in 0..count.min(10) {
                self.report(mode, button, true, &hit, state.modifiers);
            }
        } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) && !state.modifiers.shift() {
            let key = match (up, mode.contains(TermMode::APP_CURSOR)) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1b[A",
                (false, true) => b"\x1bOB",
                (false, false) => b"\x1b[B",
            };
            self.pane.write(key.repeat(count));
        } else {
            term.scroll_display(Scroll::Delta(if up { count as i32 } else { -(count as i32) }));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    row: usize,
    column: usize,
    width: usize,
}

impl Span {
    const fn new(row: usize, column: usize, width: usize) -> Self {
        Self { row, column, width }
    }
}

struct Painter<'r> {
    renderer: &'r mut Renderer,
    origin: Point,
    metrics: Metrics,
    clip: Rectangle,
    grid: Cells,
    corners: Radius,
    run: String,
    run_at: Span,
    run_style: (Rgb, Font),
    background: Option<(Span, Rgb, bool)>,
}

impl Painter<'_> {
    fn rect(&self, span: Span) -> Rectangle {
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

    fn quad(&mut self, bounds: Rectangle, color: Color) {
        self.renderer.fill_quad(Quad { bounds, ..Quad::default() }, color);
    }

    fn text(&mut self, content: String, span: Span, (fg, font): (Rgb, Font), basic: bool) {
        let rect = self.rect(span);
        self.renderer.fill_text(
            text::Text {
                content,
                bounds: Size::new(rect.width + self.metrics.cell.width * 2.0, rect.height),
                size: Pixels(self.metrics.fonts.size),
                line_height: text::LineHeight::Absolute(Pixels(rect.height)),
                font,
                align_x: text::Alignment::Left,
                align_y: Vertical::Top,
                shaping: if basic { text::Shaping::Basic } else { text::Shaping::Advanced },
                wrapping: text::Wrapping::None,
            },
            Point::new(self.origin.x + span.column as f32 * self.metrics.cell.width, rect.y),
            color(fg, 1.0),
            self.clip,
        );
    }

    fn flush(&mut self) {
        let trimmed = self.run.trim_end().len();
        if trimmed > 0 {
            self.run.truncate(trimmed);
            let content = std::mem::take(&mut self.run);
            let span = Span { width: trimmed, ..self.run_at };
            self.text(content, span, self.run_style, true);
        }
        self.run.clear();
    }

    fn glyph(&mut self, at: Span, glyph: char, style: (Rgb, Font)) {
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

    fn fill(&mut self, next: Option<(Span, Rgb, bool)>) {
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
                    let border = Border { radius, ..Border::default() };
                    self.renderer.fill_quad(Quad { bounds, border, ..Quad::default() }, color(bg, 1.0));
                }
                self.background = next;
            }
        }
    }

    fn underline(&mut self, span: Span, flags: Flags, color: Color) {
        let rect = self.rect(span);
        let y = rect.y + (self.metrics.baseline + 2.0).min(rect.height - 1.0);
        let line = |x: f32, y: f32, width: f32| Rectangle { x, y, width, height: 1.0 };
        if flags.contains(Flags::DOUBLE_UNDERLINE) {
            self.quad(line(rect.x, y - 1.0, rect.width), color);
            self.quad(line(rect.x, y + 1.0, rect.width), color);
        } else if flags.intersects(Flags::UNDERCURL | Flags::DOTTED_UNDERLINE | Flags::DASHED_UNDERLINE) {
            let (dash, gap): (f32, f32) = if flags.contains(Flags::DASHED_UNDERLINE) { (4.0, 2.0) } else { (2.0, 2.0) };
            let mut x = rect.x;
            let mut up = false;
            while x < rect.x + rect.width {
                let lift = if flags.contains(Flags::UNDERCURL) && up { -1.0 } else { 0.0 };
                self.quad(line(x, y + lift, dash.min(rect.x + rect.width - x)), color);
                x += dash + if flags.contains(Flags::UNDERCURL) { 0.0 } else { gap };
                up = !up;
            }
        } else {
            self.quad(line(rect.x, y, rect.width), color);
        }
    }
}

impl Widget<Action, Theme, Renderer> for Terminal<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        if state.metrics.is_none_or(|metrics| metrics.fonts != self.look.fonts) {
            state.metrics = Some(Metrics::measure(self.look.fonts));
        }
        layout::Node::new(limits.max())
    }

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, Action>, _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let Some(metrics) = state.metrics else { return };
        let bounds = layout.bounds();
        let cells = metrics.cells(bounds);
        if cells != self.pane.cells || metrics.cell != self.pane.cell_size {
            shell.publish(Action::Resize(self.pane.id, cells, metrics.cell));
        }
        let local = cursor.position().map(|position| Point::ORIGIN + (position - bounds.position()));
        let over = cursor.is_over(bounds);
        let action = match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
                let changed = match local {
                    Some(local) if over && state.drag == Drag::None => self.moved(state, local, bounds, over),
                    _ => state.hover.take().is_some(),
                };
                if changed {
                    shell.request_redraw();
                }
                None
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) if over => {
                let (Some(local), Some(absolute)) = (local, cursor.position()) else { return };
                shell.capture_event();
                shell.request_redraw();
                self.press(state, *button, local, absolute, bounds)
            }
            Event::Mouse(mouse::Event::ButtonReleased(button)) if state.drag != Drag::None || state.held.is_some() => {
                shell.request_redraw();
                self.released(state, *button, local.unwrap_or(Point::ORIGIN))
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let tracked = over || state.drag != Drag::None || state.held.is_some() || state.near_bar;
                if let Some(local) = local
                    && tracked
                    && self.moved(state, local, bounds, over)
                {
                    shell.request_redraw();
                }
                None
            }
            Event::Mouse(mouse::Event::CursorLeft) => {
                state.near_bar = false;
                state.hover = None;
                None
            }
            Event::Window(iced::window::Event::Unfocused) => {
                if let Some(held) = state.held.take() {
                    let hit = locate(Point::ORIGIN, &metrics, self.pane.cells, 0);
                    self.report(
                        *self.pane.term.lock().mode(),
                        [Button::Left, Button::Middle, Button::Right][usize::from(held)],
                        false,
                        &hit,
                        state.modifiers,
                    );
                }
                state.drag = Drag::None;
                state.modifiers = keyboard::Modifiers::empty();
                None
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if over => {
                if let Some(local) = local {
                    self.wheel(state, *delta, local);
                }
                shell.capture_event();
                shell.request_redraw();
                None
            }
            _ => None,
        };
        if let Some(action) = action {
            shell.publish(action);
        }
    }

    fn mouse_interaction(
        &self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, _viewport: &Rectangle, _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if !cursor.is_over(layout.bounds()) && state.drag == Drag::None {
            return mouse::Interaction::None;
        }
        if state.hover.is_some() {
            mouse::Interaction::Pointer
        } else if state.near_bar || state.drag == Drag::Scrollbar {
            mouse::Interaction::Idle
        } else {
            mouse::Interaction::Text
        }
    }

    fn draw(
        &self, tree: &Tree, renderer: &mut Renderer, _theme: &Theme, _style: &renderer::Style, layout: Layout<'_>,
        _cursor: mouse::Cursor, _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let Some(metrics) = state.metrics else { return };
        let bounds = layout.bounds();
        renderer.with_layer(bounds, |renderer| {
            let term = self.pane.term.lock();
            let palette = &self.look.palette;
            let backdrop =
                Quad { bounds, border: Border { radius: self.corners, ..Border::default() }, ..Quad::default() };
            renderer.fill_quad(backdrop, color(palette.query(term.colors(), 257), palette.opacity));
            let mut painter = Painter {
                renderer,
                origin: Point::new(bounds.x + PAD, bounds.y + PAD),
                metrics,
                clip: bounds,
                grid: Cells { columns: term.columns(), lines: term.screen_lines() },
                corners: self.corners,
                run: String::new(),
                run_at: Span::new(0, 0, 0),
                run_style: (palette.foreground, self.look.fonts.regular),
                background: None,
            };
            let cursor = self.paint_cells(&mut painter, state, &term, bounds);
            if let Some((span, shape)) = cursor {
                self.paint_cursor(&mut painter, span, shape, palette.query(term.colors(), 258));
            }
            let offset = term.grid().display_offset();
            let (lines, history) = (term.screen_lines(), term.history_size());
            if history > 0 && (offset > 0 || state.near_bar || state.drag == Drag::Scrollbar) {
                let bar = thumb(bounds, lines, history, offset);
                let alpha = if state.near_bar || state.drag == Drag::Scrollbar { 0.55 } else { 0.32 };
                painter.renderer.fill_quad(
                    Quad {
                        bounds: bar,
                        border: Border { radius: glide_theme::shape::R1.into(), ..Border::default() },
                        ..Quad::default()
                    },
                    color(palette.foreground, alpha),
                );
            }
            match self.flash {
                Some((Flash::Bell, amount)) => {
                    painter.renderer.fill_quad(backdrop, color(palette.foreground, 0.14 * amount));
                }
                Some((Flash::Mark, amount)) => {
                    let row = painter.rect(Span::new(0, 0, metrics.cells(bounds).columns));
                    let band = Rectangle { x: bounds.x, width: bounds.width, ..row };
                    painter.quad(band, Color { a: 0.22 * amount, ..self.look.accent });
                }
                None => {}
            }
            if let Some((hints, typed)) = self.hints {
                self.paint_hints(&mut painter, hints, typed, offset as i32, lines);
            }
            if self.drop_target {
                let border = Border { color: self.look.accent, width: 2.0, radius: self.corners };
                painter
                    .renderer
                    .fill_quad(Quad { bounds, border, ..Quad::default() }, Color { a: 0.08, ..self.look.accent });
            }
            if let Some(Link::Osc(link)) = &state.hover {
                let ink = color(palette.query(term.colors(), 257), 1.0);
                self.paint_link_target(&mut painter, link.uri(), bounds, color(palette.foreground, 0.92), ink);
            }
        });
    }
}

impl Terminal<'_> {
    fn paint_link_target(&self, painter: &mut Painter<'_>, uri: &str, bounds: Rectangle, fill: Color, ink: Color) {
        let (fonts, cell) = (self.look.fonts, painter.metrics.cell);
        let pad = glide_theme::spacing::SP2 as f32;
        let room = ((bounds.width - PAD * 2.0 - pad * 2.0) / cell.width.max(1.0)).floor().max(1.0) as usize;
        let shown: String = if uri.chars().count() > room {
            uri.chars().take(room.saturating_sub(1)).chain(std::iter::once('…')).collect()
        } else {
            uri.to_string()
        };
        let width = shown.chars().count() as f32 * cell.width + pad * 2.0;
        let height = cell.height + pad;
        let pill = Rectangle {
            x: bounds.x + PAD,
            y: bounds.y + bounds.height - PAD - height,
            width: width.min(bounds.width - PAD * 2.0),
            height,
        };
        let clip = painter.clip;
        painter.renderer.with_layer(clip, |renderer| {
            renderer.fill_quad(
                Quad {
                    bounds: pill,
                    border: Border { radius: glide_theme::shape::R2.into(), ..Border::default() },
                    ..Quad::default()
                },
                fill,
            );
            renderer.fill_text(
                text::Text {
                    content: shown,
                    bounds: Size::new(pill.width, cell.height),
                    size: Pixels(fonts.size),
                    line_height: text::LineHeight::Absolute(Pixels(cell.height)),
                    font: fonts.regular,
                    align_x: text::Alignment::Left,
                    align_y: Vertical::Top,
                    shaping: text::Shaping::Advanced,
                    wrapping: text::Wrapping::None,
                },
                Point::new(pill.x + pad, pill.y + pad / 2.0),
                ink,
                clip,
            );
        });
    }

    fn paint_hints(&self, painter: &mut Painter<'_>, hints: &[Hint], typed: &str, offset: i32, lines: usize) {
        let visible = |line: Line| {
            let row = line.0 + offset;
            (row >= 0 && (row as usize) < lines).then_some(row as usize)
        };
        let columns = self.pane.cells.columns;
        let accent = self.look.accent;
        let mut labels = Vec::new();
        for hint in hints.iter().filter(|hint| hint.label.starts_with(typed)) {
            let (start, end) = (*hint.range.start(), *hint.range.end());
            for line in (start.line.0..=end.line.0).map(Line) {
                let Some(row) = visible(line) else { continue };
                let from = if line == start.line { start.column.0 } else { 0 };
                let to = if line == end.line { end.column.0 + 1 } else { columns };
                painter.underline(Span::new(row, from, to.saturating_sub(from)), Flags::UNDERLINE, accent);
            }
            if let Some(row) = visible(start.line) {
                labels.push((painter.rect(Span::new(row, start.column.0, hint.label.len())), hint.label.clone()));
            }
        }
        let (fonts, cell) = (self.look.fonts, painter.metrics.cell);
        let clip = painter.clip;
        painter.renderer.with_layer(clip, |renderer| {
            for (rect, label) in labels {
                let pill = Rectangle { x: rect.x - 2.0, width: rect.width + 4.0, ..rect };
                renderer.fill_quad(
                    Quad {
                        bounds: pill,
                        border: Border { radius: glide_theme::shape::R1.into(), ..Border::default() },
                        ..Quad::default()
                    },
                    accent,
                );
                renderer.fill_text(
                    text::Text {
                        content: label,
                        bounds: Size::new(pill.width + cell.width, pill.height),
                        size: Pixels(fonts.size),
                        line_height: text::LineHeight::Absolute(Pixels(cell.height)),
                        font: Font { weight: iced::font::Weight::Bold, ..fonts.regular },
                        align_x: text::Alignment::Left,
                        align_y: Vertical::Top,
                        shaping: text::Shaping::Basic,
                        wrapping: text::Wrapping::None,
                    },
                    Point::new(rect.x, rect.y),
                    Color::WHITE,
                    clip,
                );
            }
        });
    }

    fn paint_cells(
        &self, painter: &mut Painter<'_>, state: &State, term: &Term<Listener>, bounds: Rectangle,
    ) -> Option<(Span, CursorShape)> {
        let palette = &self.look.palette;
        let found = self.found(state, term);
        let content = term.renderable_content();
        let offset = content.display_offset as i32;
        let cursor = content.cursor;
        let steady = !term.cursor_style().blinking || !self.focused;
        let cursor_visible = cursor.shape != CursorShape::Hidden && (self.blink || steady);
        let block = self.focused && cursor_visible && cursor.shape == CursorShape::Block;
        let warning = self.look.bookmark;
        let found_bg = mix(
            palette.background,
            Rgb {
                r: (warning.r * 255.0).round() as u8,
                g: (warning.g * 255.0).round() as u8,
                b: (warning.b * 255.0).round() as u8,
            },
            0.55,
        );
        let mut cursor_span = None;
        let cells: Vec<_> = content.display_iter.collect();
        let tint = |indexed: &Indexed<&Cell>| {
            let point = indexed.point;
            let (mut fg, mut bg) = palette.cell(content.colors, indexed.cell, self.look.bold_bright);
            let mut bleeds = true;
            if content.selection.is_some_and(|range| range.contains_cell(indexed, cursor.point, cursor.shape)) {
                (bg, bleeds) = (palette.selection, false);
            } else if found.iter().any(|range| range.contains(&point)) {
                (bg, bleeds) = (found_bg, false);
            }
            if block && point == cursor.point {
                (fg, bg, bleeds) = (palette.cursor_text, palette.query(content.colors, 258), false);
            }
            (fg, bg, bleeds)
        };
        for indexed in &cells {
            let cell = indexed.cell;
            if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                continue;
            }
            let at = Span::new(
                (indexed.point.line.0 + offset) as usize,
                indexed.point.column.0,
                if cell.flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 },
            );
            let (_, bg, bleeds) = tint(indexed);
            painter.fill((bg != palette.query(content.colors, 257)).then_some((at, bg, bleeds)));
        }
        painter.fill(None);
        for indexed in cells {
            let Indexed { point, cell } = indexed;
            let at = Span::new((point.line.0 + offset) as usize, point.column.0, 1);
            if at.column == 0
                && let Some(mark) = marks::at(term, point.line)
            {
                let rect = painter.rect(at);
                let tint =
                    if mark == Mark::Bookmark { self.look.bookmark } else { Color { a: 0.55, ..self.look.accent } };
                painter.quad(
                    Rectangle { x: bounds.x + 2.0, y: rect.y + 2.0, width: 3.0, height: rect.height - 4.0 },
                    tint,
                );
            }
            if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                continue;
            }
            let at = Span { width: if cell.flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 }, ..at };
            if cursor_visible && !block && point == cursor.point {
                cursor_span = Some((at, cursor.shape));
            }
            let (fg, ..) = tint(&indexed);
            if cell.flags.contains(Flags::HIDDEN) {
                painter.flush();
                continue;
            }
            self.paint_glyph(painter, cell, at, fg);
            let hovered = state.hover.as_ref().is_some_and(|link| link.covers(point, cell));
            if cell.flags.intersects(Flags::ALL_UNDERLINES) || hovered {
                let underline = cell.underline_color().map_or(fg, |tint| palette.resolve(content.colors, tint));
                painter.underline(at, if hovered { Flags::UNDERLINE } else { cell.flags }, color(underline, 1.0));
            }
            if cell.flags.contains(Flags::STRIKEOUT) {
                let rect = painter.rect(at);
                painter
                    .quad(Rectangle { y: (rect.y + rect.height / 2.0).round(), height: 1.0, ..rect }, color(fg, 1.0));
            }
        }
        painter.fill(None);
        painter.flush();
        cursor_span
    }

    fn paint_glyph(&self, painter: &mut Painter<'_>, cell: &Cell, at: Span, fg: Rgb) {
        let style = (fg, self.look.fonts.styled(cell.flags));
        let zerowidth = cell.zerowidth().filter(|marks| !marks.is_empty());
        if boxes::handles(cell.c) {
            painter.flush();
            let rect = painter.rect(at);
            boxes::draw(painter.renderer, cell.c, rect, color(fg, 1.0));
        } else if cell.c.is_ascii() && !cell.c.is_ascii_control() && zerowidth.is_none() && at.width == 1 {
            painter.glyph(at, cell.c, style);
        } else if cell.c != ' ' {
            painter.flush();
            let mut content = cell.c.to_string();
            content.extend(zerowidth.into_iter().flatten());
            painter.text(content, at, style, false);
        }
    }

    fn paint_cursor(&self, painter: &mut Painter<'_>, at: Span, shape: CursorShape, cursor: Rgb) {
        let rect = painter.rect(at);
        let tint = color(cursor, 1.0);
        let stroke = 2.0_f32.min(rect.width);
        match (self.focused, shape) {
            (true, CursorShape::Beam) => painter.quad(Rectangle { width: stroke, ..rect }, tint),
            (true, CursorShape::Underline) => {
                painter.quad(Rectangle { y: rect.y + rect.height - stroke, height: stroke, ..rect }, tint);
            }
            _ => painter.renderer.fill_quad(
                Quad {
                    bounds: rect,
                    border: Border { color: tint, width: 1.0, radius: 0.0.into() },
                    ..Quad::default()
                },
                Color::TRANSPARENT,
            ),
        }
    }
}

impl Terminal<'_> {
    fn found(&self, state: &State, term: &Term<Listener>) -> Vec<Match> {
        let Some(pattern) = self.find.filter(|pattern| !pattern.is_empty()) else { return Vec::new() };
        let mut cache = state.find.borrow_mut();
        if cache.as_ref().is_none_or(|(cached, _)| cached != pattern) {
            *cache = Some((pattern.to_string(), RegexSearch::new(pattern).ok()));
        }
        let Some((_, Some(regex))) = cache.as_mut() else { return Vec::new() };
        let offset = term.grid().display_offset() as i32;
        let start = GridPoint::new(Line(-offset), Column(0));
        let end = GridPoint::new(Line(term.screen_lines() as i32 - 1 - offset), term.last_column());
        RegexIter::new(start, end, Direction::Right, term, regex).take(500).collect()
    }
}

impl<'a> From<Terminal<'a>> for Element<'a, Action> {
    fn from(terminal: Terminal<'a>) -> Self {
        Element::new(terminal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Bus, Launch};
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;
    use glide_theme::{Accent, Appearance, Tokens};

    fn rendered(output: &str, bounds: Rectangle) -> Vec<iced_tiny_skia::Layer> {
        let (sender, _receiver) = iced::futures::channel::mpsc::unbounded();
        let pane = Pane::spawn(
            PaneId(1),
            &Bus(sender),
            &Launch { command: Some(("cat".into(), Vec::new())), directory: None },
            Config::default(),
        )
        .unwrap();
        let mut processor: Processor = Processor::new();
        processor.advance(&mut *pane.term.lock(), output.as_bytes());
        let palette = crate::profile::Profile::Meridian.palette(&Tokens::new(Appearance::Dark, Accent::Blue));
        let fonts = Fonts { regular: crate::MONO, size: 13.0, line_height: 1.25 };
        let look = Look { palette, fonts, bold_bright: true, accent: Color::WHITE, bookmark: Color::WHITE };
        let terminal = Terminal::new(&pane, look, true);
        let mut tree = Tree::new(&terminal as &dyn Widget<Action, Theme, Renderer>);
        tree.state.downcast_mut::<State>().metrics =
            Some(Metrics { fonts, cell: Size::new(8.0, 16.0), baseline: 13.0 });
        let mut renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(crate::MONO, Pixels(13.0)));
        renderer.reset(bounds);
        let node = layout::Node::new(bounds.size());
        terminal.draw(
            &tree,
            &mut renderer,
            &Theme::Dark,
            &renderer::Style { text_color: Color::WHITE },
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &bounds,
        );
        let Renderer::Secondary(mut renderer) = renderer else { unreachable!() };
        renderer.layers().to_vec()
    }

    #[test]
    fn colored_backgrounds_precede_block_glyphs_and_underlines() {
        let bounds = Rectangle::with_size(Size::new(656.0, 400.0));
        let palette = crate::profile::Profile::Meridian.palette(&Tokens::new(Appearance::Dark, Accent::Blue));
        for (output, point) in [
            ("\x1b[?25l\x1b[41m█\x1b[0m", Point::new(12.0, 16.0)),
            ("\x1b[?25l\x1b[41;4mA\x1b[0m", Point::new(12.0, 23.5)),
        ] {
            let layers = rendered(output, bounds);
            let colors: Vec<_> = layers
                .iter()
                .flat_map(|layer| &layer.quads)
                .filter(|(quad, _)| quad.bounds.contains(point))
                .map(|(_, background)| *background)
                .collect();
            assert!(colors.contains(&iced::Background::Color(color(palette.ansi[1], 1.0))));
            assert_eq!(colors.last(), Some(&iced::Background::Color(color(palette.foreground, 1.0))));
        }
    }

    #[test]
    fn edge_cell_backgrounds_fill_the_padding_but_inner_ones_and_the_cursor_do_not() {
        let bounds = Rectangle::with_size(Size::new(656.0, 400.0));
        let palette = crate::profile::Profile::Meridian.palette(&Tokens::new(Appearance::Dark, Accent::Blue));
        let colors_at = |output: &str, point: Point| -> Vec<_> {
            rendered(output, bounds)
                .iter()
                .flat_map(|layer| &layer.quads)
                .filter(|(quad, _)| quad.bounds.contains(point))
                .map(|(_, background)| *background)
                .collect()
        };
        let black = iced::Background::Color(color(palette.ansi[0], 1.0));
        let red = iced::Background::Color(color(palette.ansi[1], 1.0));
        let cursor = iced::Background::Color(color(palette.cursor, 1.0));
        for point in [Point::new(1.0, 1.0), Point::new(654.0, 200.0), Point::new(328.0, 399.0)] {
            assert!(colors_at("\x1b[?25l\x1b[40m\x1b[2J", point).contains(&black), "{point:?}");
        }
        assert!(!colors_at("\x1b[?25l\x1b[3G\x1b[41mx", Point::new(1.0, 12.0)).contains(&red));
        assert!(colors_at("", Point::new(12.0, 12.0)).contains(&cursor));
        assert!(!colors_at("", Point::new(1.0, 12.0)).contains(&cursor));
    }

    #[test]
    fn osc_background_and_cursor_colors_reach_the_renderer_and_every_layer_is_clipped() {
        let bounds = Rectangle::with_size(Size::new(40.0, 40.0));
        let layers = rendered("\x1b]11;#112233\x07\x1b]12;#445566\x07", bounds);
        let colors: Vec<_> = layers.iter().flat_map(|layer| &layer.quads).map(|(_, background)| *background).collect();
        assert!(colors.contains(&iced::Background::Color(Color::from_rgb8(0x11, 0x22, 0x33))));
        assert!(colors.contains(&iced::Background::Color(Color::from_rgb8(0x44, 0x55, 0x66))));
        assert!(layers.iter().all(|layer| layer.bounds.intersection(&bounds) == Some(layer.bounds)));
    }

    #[test]
    fn only_the_button_that_started_a_grab_can_release_it() {
        let mut state = State { drag: Drag::Select { moved: true, option: false }, ..State::default() };
        assert!(!releases_grab(&state, mouse::Button::Right));
        assert!(releases_grab(&state, mouse::Button::Left));
        state.held = Some(2);
        assert!(!releases_grab(&state, mouse::Button::Left));
        assert!(releases_grab(&state, mouse::Button::Right));
    }
}

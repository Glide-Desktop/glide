use std::cell::RefCell;
use std::time::{Duration, Instant};

use alacritty_terminal::grid::{Dimensions, Indexed, Scroll};
use alacritty_terminal::index::{Column, Direction, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{CursorShape, Rgb};
use glide_sdk::widget::cells::{Badge, Caret, Metrics, Painter, Span, Underline, thumb};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell};
use iced::border::Radius;
use iced::{Border, Color, Element, Event, Font, Length, Point, Rectangle, Renderer, Size, Theme, keyboard, mouse};

use crate::hints::Hint;
use crate::input::{self, Button};
use crate::marks::{self, Mark};
use crate::profile::{Palette, color, mix};
use crate::session::{Cells, Listener, Pane, PaneId};

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
    measured: Option<Fonts>,
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
    let hit = metrics.locate(local, cells.into());
    let (row, column) = (hit.row, hit.column);
    let side = if hit.right_half { Side::Right } else { Side::Left };
    Hit { row, column, side, point: GridPoint::new(Line(row as i32 - offset as i32), Column(column)) }
}

const fn underline_style(flags: Flags) -> Underline {
    if flags.contains(Flags::DOUBLE_UNDERLINE) {
        Underline::Double
    } else if flags.contains(Flags::UNDERCURL) {
        Underline::Curl
    } else if flags.contains(Flags::DOTTED_UNDERLINE) {
        Underline::Dotted
    } else if flags.contains(Flags::DASHED_UNDERLINE) {
        Underline::Dashed
    } else {
        Underline::Single
    }
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
        let fonts = self.look.fonts;
        if state.metrics.is_none() || state.measured != Some(fonts) {
            state.metrics = Some(Metrics::measure(fonts.regular, fonts.size, fonts.line_height));
            state.measured = Some(fonts);
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
        let cells = Cells::from(metrics.grid(bounds));
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
            let grid = Cells { columns: term.columns(), lines: term.screen_lines() };
            let mut painter = Painter::new(renderer, bounds, metrics, grid.into(), self.corners);
            let cursor = self.paint_cells(&mut painter, state, &term);
            if let Some((span, shape)) = cursor {
                self.paint_cursor(&mut painter, span, shape, palette.query(term.colors(), 258));
            }
            let offset = term.grid().display_offset();
            let (lines, history) = (term.screen_lines(), term.history_size());
            if history > 0 && (offset > 0 || state.near_bar || state.drag == Drag::Scrollbar) {
                let alpha = if state.near_bar || state.drag == Drag::Scrollbar { 0.55 } else { 0.32 };
                painter.scrollbar(lines, history, offset, color(palette.foreground, alpha));
            }
            match self.flash {
                Some((Flash::Bell, amount)) => {
                    painter.renderer.fill_quad(backdrop, color(palette.foreground, 0.14 * amount));
                }
                Some((Flash::Mark, amount)) => painter.band(0, Color { a: 0.22 * amount, ..self.look.accent }),
                None => {}
            }
            if let Some((hints, typed)) = self.hints {
                self.paint_hints(&mut painter, hints, typed, offset as i32, lines);
            }
            if self.drop_target {
                let border = Border { color: self.look.accent, width: 2.0, radius: self.corners };
                painter.outline(bounds, border, Color { a: 0.08, ..self.look.accent });
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
        let (fonts, cell) = (self.look.fonts, painter.metrics().cell);
        let pad = glide_sdk::theme::SP2;
        let room = ((bounds.width - pad * 4.0) / cell.width.max(1.0)).floor().max(1.0) as usize;
        let shown: String = if uri.chars().count() > room {
            uri.chars().take(room.saturating_sub(1)).chain(std::iter::once('…')).collect()
        } else {
            uri.to_string()
        };
        let width = shown.chars().count() as f32 * cell.width + pad * 2.0;
        let height = cell.height + pad;
        let pill = Rectangle {
            x: bounds.x + pad,
            y: bounds.y + bounds.height - pad - height,
            width: width.min(bounds.width - pad * 2.0),
            height,
        };
        painter.badges(vec![Badge {
            pill,
            radius: glide_sdk::theme::R2,
            fill,
            label: shown,
            at: Point::new(pill.x + pad, pill.y + pad / 2.0),
            font: fonts.regular,
            ink,
            basic: false,
        }]);
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
                painter.underline(Span::new(row, from, to.saturating_sub(from)), Underline::Single, accent);
            }
            if let Some(row) = visible(start.line) {
                labels.push((painter.rect(Span::new(row, start.column.0, hint.label.len())), hint.label.clone()));
            }
        }
        let bold = Font { weight: iced::font::Weight::Bold, ..self.look.fonts.regular };
        let badges = labels.into_iter().map(|(rect, label)| Badge {
            pill: Rectangle { x: rect.x - 2.0, width: rect.width + 4.0, ..rect },
            radius: glide_sdk::theme::R1,
            fill: accent,
            label,
            at: rect.position(),
            font: bold,
            ink: Color::WHITE,
            basic: true,
        });
        painter.badges(badges.collect());
    }

    fn paint_cells(
        &self, painter: &mut Painter<'_>, state: &State, term: &Term<Listener>,
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
            painter.fill((bg != palette.query(content.colors, 257)).then_some((at, color(bg, 1.0), bleeds)));
        }
        painter.fill(None);
        for indexed in cells {
            let Indexed { point, cell } = indexed;
            let at = Span::new((point.line.0 + offset) as usize, point.column.0, 1);
            if at.column == 0
                && let Some(mark) = marks::at(term, point.line)
            {
                let tint =
                    if mark == Mark::Bookmark { self.look.bookmark } else { Color { a: 0.55, ..self.look.accent } };
                painter.gutter(at.row, tint);
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
                let style = if hovered { Underline::Single } else { underline_style(cell.flags) };
                painter.underline(at, style, color(underline, 1.0));
            }
            if cell.flags.contains(Flags::STRIKEOUT) {
                painter.strike(at, color(fg, 1.0));
            }
        }
        painter.fill(None);
        painter.flush();
        cursor_span
    }

    fn paint_glyph(&self, painter: &mut Painter<'_>, cell: &Cell, at: Span, fg: Rgb) {
        let (ink, font) = (color(fg, 1.0), self.look.fonts.styled(cell.flags));
        let zerowidth = cell.zerowidth().filter(|marks| !marks.is_empty());
        if painter.symbol(at, cell.c, ink) {
            return;
        }
        if cell.c.is_ascii() && !cell.c.is_ascii_control() && zerowidth.is_none() && at.width == 1 {
            painter.glyph(at, cell.c, ink, font);
        } else if cell.c != ' ' {
            painter.flush();
            let mut content = cell.c.to_string();
            content.extend(zerowidth.into_iter().flatten());
            painter.text(content, at, ink, font, false);
        }
    }

    fn paint_cursor(&self, painter: &mut Painter<'_>, at: Span, shape: CursorShape, cursor: Rgb) {
        let caret = match shape {
            CursorShape::Beam => Caret::Beam,
            CursorShape::Underline => Caret::Underline,
            _ => Caret::Block,
        };
        painter.caret(at, caret, color(cursor, 1.0), self.focused && caret != Caret::Block);
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
    use glide_sdk::theme::{Accent, Appearance, Tokens};

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
            Some(Metrics { font: fonts.regular, size: fonts.size, cell: Size::new(8.0, 16.0), baseline: 13.0 });
        let mut renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(crate::MONO, iced::Pixels(13.0)));
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
            ("\x1b[?25l\x1b[41m█\x1b[0m", Point::new(4.0, 8.0)),
            ("\x1b[?25l\x1b[41;4mA\x1b[0m", Point::new(4.0, 15.5)),
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
    fn the_grid_starts_at_the_pane_edge_and_edge_backgrounds_fill_the_remainder_but_the_cursor_does_not() {
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
        assert!(colors_at("\x1b[?25l\x1b[41mx", Point::new(0.5, 0.5)).contains(&red));
        assert!(!colors_at("\x1b[?25l\x1b[3G\x1b[41mx", Point::new(4.0, 8.0)).contains(&red));
        assert!(colors_at("\x1b[24;1H", Point::new(4.0, 380.0)).contains(&cursor));
        assert!(!colors_at("\x1b[24;1H", Point::new(4.0, 395.0)).contains(&cursor));
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

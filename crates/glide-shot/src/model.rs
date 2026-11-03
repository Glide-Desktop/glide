use std::sync::Arc;

use smithay::utils::{Logical, Point, Rectangle, Size};

use super::compose::Sprite;

pub type Pos = Point<f64, Logical>;
pub type Area = Rectangle<f64, Logical>;

pub const BTN_LEFT: u32 = 0x110;
pub const MIN_SIDE: f64 = 4.0;
pub const HANDLE_REACH: f64 = 9.0;
const MIN_MARK: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Area,
    Window,
    Screen,
}

pub const MODES: [Mode; 3] = [Mode::Area, Mode::Window, Mode::Screen];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Choose,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Move,
    Pen,
    Arrow,
    Rect,
    Highlight,
    Blur,
    Text,
    Step,
}

pub const TOOLS: [Tool; 8] =
    [Tool::Move, Tool::Pen, Tool::Arrow, Tool::Rect, Tool::Highlight, Tool::Blur, Tool::Text, Tool::Step];

pub const SWATCHES: [[u8; 3]; 6] =
    [[255, 59, 48], [255, 204, 0], [52, 199, 89], [10, 132, 255], [28, 28, 30], [255, 255, 255]];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

pub const HANDLES: [Handle; 8] =
    [Handle::NW, Handle::N, Handle::NE, Handle::E, Handle::SE, Handle::S, Handle::SW, Handle::W];

impl Handle {
    #[must_use]
    pub fn at(self, rect: Area) -> Pos {
        let (x0, y0) = (rect.loc.x, rect.loc.y);
        let (x1, y1) = (x0 + rect.size.w, y0 + rect.size.h);
        let (cx, cy) = (f64::midpoint(x0, x1), f64::midpoint(y0, y1));
        Point::from(match self {
            Self::N => (cx, y0),
            Self::S => (cx, y1),
            Self::E => (x1, cy),
            Self::W => (x0, cy),
            Self::NE => (x1, y0),
            Self::NW => (x0, y0),
            Self::SE => (x1, y1),
            Self::SW => (x0, y1),
        })
    }

    const fn moves(self) -> (i8, i8) {
        match self {
            Self::N => (0, -1),
            Self::S => (0, 1),
            Self::E => (1, 0),
            Self::W => (-1, 0),
            Self::NE => (1, -1),
            Self::NW => (-1, -1),
            Self::SE => (1, 1),
            Self::SW => (-1, 1),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Pen(Vec<Pos>),
    Arrow(Pos, Pos),
    Rect(Pos, Pos),
    Highlight(Pos, Pos),
    Blur(Pos, Pos),
    Text { at: Pos, text: String },
    Step { at: Pos, number: u32 },
}

impl Shape {
    #[must_use]
    pub const fn needs_sprite(&self) -> bool {
        matches!(self, Self::Text { .. } | Self::Step { .. })
    }

    #[must_use]
    pub fn bounds(&self) -> Option<Area> {
        let points: Vec<Pos> = match self {
            Self::Pen(points) => points.clone(),
            Self::Arrow(a, b) | Self::Rect(a, b) | Self::Highlight(a, b) | Self::Blur(a, b) => vec![*a, *b],
            Self::Text { at, .. } | Self::Step { at, .. } => vec![*at],
        };
        span(&points)
    }

    fn is_meaningful(&self) -> bool {
        match self {
            Self::Pen(points) => points.len() >= 2,
            Self::Arrow(a, b) => (a.x - b.x).hypot(a.y - b.y) >= MIN_MARK * 2.0,
            Self::Rect(a, b) | Self::Highlight(a, b) | Self::Blur(a, b) => {
                (a.x - b.x).abs() >= MIN_MARK && (a.y - b.y).abs() >= MIN_MARK
            }
            Self::Text { text, .. } => !text.trim().is_empty(),
            Self::Step { .. } => true,
        }
    }
}

#[must_use]
pub fn span(points: &[Pos]) -> Option<Area> {
    let first = points.first()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
    for point in points {
        x0 = x0.min(point.x);
        y0 = y0.min(point.y);
        x1 = x1.max(point.x);
        y1 = y1.max(point.y);
    }
    Some(Rectangle::new(Point::from((x0, y0)), Size::from((x1 - x0, y1 - y0))))
}

#[must_use]
pub fn rect_between(a: Pos, b: Pos) -> Area {
    span(&[a, b]).unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct Mark {
    pub shape: Shape,
    pub swatch: usize,
    pub sprite: Option<Arc<Sprite>>,
}

impl Mark {
    #[must_use]
    pub const fn new(shape: Shape, swatch: usize) -> Self {
        Self { shape, swatch, sprite: None }
    }

    #[must_use]
    pub fn color(&self) -> [u8; 3] {
        SWATCHES.get(self.swatch).copied().unwrap_or(SWATCHES[0])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Mode(Mode),
    Shadow,
    Capture,
    Close,
    Tool(Tool),
    Swatch(usize),
    Undo,
    Redo,
    Copy,
    Save,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Gesture {
    Select { origin: Pos, before: Option<Area> },
    Move { grab: Pos, from: Area },
    Resize { handle: Handle, from: Area },
    Draw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    Redraw,
    Close,
    Copy,
    Save,
    Window(usize),
}

pub struct Shot {
    pub mode: Mode,
    pub stage: Stage,
    pub bounds: Size<f64, Logical>,
    pub selection: Option<Area>,
    pub windows: Vec<Area>,
    pub hovered: Option<usize>,
    pub window: Option<usize>,
    pub tool: Tool,
    pub swatch: usize,
    pub shadow: bool,
    pub marks: Vec<Mark>,
    redo: Vec<Mark>,
    pub draft: Option<Mark>,
    pub typing: bool,
    gesture: Option<Gesture>,
    pub pointer: Pos,
    pub hover: Option<Control>,
    pub revision: u64,
}

impl Shot {
    #[must_use]
    pub fn new(bounds: Size<f64, Logical>, windows: Vec<Area>, memory: &Memory) -> Self {
        let selection = memory.selection.filter(|area| fits(*area, bounds));
        Self {
            mode: memory.mode,
            stage: Stage::Choose,
            bounds,
            selection,
            windows,
            hovered: None,
            window: None,
            tool: memory.tool,
            swatch: memory.swatch,
            shadow: memory.shadow,
            marks: Vec::new(),
            redo: Vec::new(),
            draft: None,
            typing: false,
            gesture: None,
            pointer: Point::from((0.0, 0.0)),
            hover: None,
            revision: 0,
        }
    }

    #[must_use]
    pub fn memory(&self) -> Memory {
        Memory { mode: self.mode, selection: self.selection, tool: self.tool, swatch: self.swatch, shadow: self.shadow }
    }

    #[must_use]
    pub fn full(&self) -> Area {
        Rectangle::from_size(self.bounds)
    }

    #[must_use]
    pub fn target(&self) -> Option<Area> {
        match self.mode {
            Mode::Area => self.selection,
            Mode::Window => self.window.or(self.hovered).and_then(|index| self.windows.get(index).copied()),
            Mode::Screen => Some(self.full()),
        }
    }

    #[must_use]
    pub fn editing(&self) -> bool {
        self.stage == Stage::Edit
    }

    #[must_use]
    pub fn shadow_applies(&self) -> bool {
        self.shadow && self.mode != Mode::Screen
    }

    #[must_use]
    pub fn selecting(&self) -> bool {
        matches!(self.gesture, Some(Gesture::Select { .. }))
    }

    #[must_use]
    pub fn resizable(&self) -> bool {
        self.editing() && self.mode == Mode::Area && !self.selecting()
    }

    #[must_use]
    pub fn can_capture(&self) -> bool {
        self.target().is_some_and(|area| area.size.w >= MIN_SIDE && area.size.h >= MIN_SIDE)
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.typing || !self.marks.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty() && !self.typing
    }

    pub fn set_mode(&mut self, mode: Mode) -> bool {
        if self.editing() || self.selecting() || self.mode == mode {
            return false;
        }
        self.mode = mode;
        self.gesture = None;
        self.hovered = if self.over_bar(self.pointer) { None } else { self.window_at(self.pointer) };
        true
    }

    pub fn set_tool(&mut self, tool: Tool) -> bool {
        self.commit_text();
        let changed = self.tool != tool;
        self.tool = tool;
        changed
    }

    pub fn set_swatch(&mut self, swatch: usize) -> bool {
        if swatch >= SWATCHES.len() || self.swatch == swatch {
            return false;
        }
        self.swatch = swatch;
        if let Some(draft) = self.draft.as_mut() {
            draft.swatch = swatch;
            draft.sprite = None;
        }
        true
    }

    pub fn toggle_shadow(&mut self) {
        self.shadow = !self.shadow;
    }

    pub fn capture(&mut self) -> Outcome {
        if self.editing() || !self.can_capture() {
            return Outcome::Nothing;
        }
        self.enter_edit()
    }

    fn enter_edit(&mut self) -> Outcome {
        self.stage = Stage::Edit;
        self.gesture = None;
        if self.mode != Mode::Area && self.tool == Tool::Move {
            self.tool = Tool::Pen;
        }
        match self.mode {
            Mode::Window => {
                self.window = self.window.or(self.hovered);
                self.window.map_or(Outcome::Redraw, Outcome::Window)
            }
            _ => Outcome::Redraw,
        }
    }

    #[must_use]
    pub fn window_at(&self, pos: Pos) -> Option<usize> {
        self.windows.iter().position(|area| area.contains(pos))
    }

    #[must_use]
    pub fn handle_at(&self, pos: Pos) -> Option<Handle> {
        if !self.resizable() {
            return None;
        }
        let target = self.target()?;
        HANDLES.into_iter().find(|handle| {
            let at = handle.at(target);
            (at.x - pos.x).abs() <= HANDLE_REACH && (at.y - pos.y).abs() <= HANDLE_REACH
        })
    }

    fn clamp(&self, pos: Pos) -> Pos {
        Point::from((pos.x.clamp(0.0, self.bounds.w), pos.y.clamp(0.0, self.bounds.h)))
    }

    fn clamp_into_target(&self, pos: Pos) -> Pos {
        let Some(target) = self.target() else {
            return self.clamp(pos);
        };
        Point::from((
            pos.x.clamp(target.loc.x, target.loc.x + target.size.w),
            pos.y.clamp(target.loc.y, target.loc.y + target.size.h),
        ))
    }

    pub fn motion(&mut self, pos: Pos, control: Option<Control>) -> bool {
        let pos = self.clamp(pos);
        self.pointer = pos;
        let hover_changed = self.hover != control;
        self.hover = control;
        match self.gesture {
            Some(Gesture::Select { origin, .. }) => {
                self.selection = Some(rect_between(origin, pos));
                true
            }
            Some(Gesture::Move { grab, from }) => {
                let dx = (pos.x - grab.x).clamp(-from.loc.x, self.bounds.w - from.loc.x - from.size.w);
                let dy = (pos.y - grab.y).clamp(-from.loc.y, self.bounds.h - from.loc.y - from.size.h);
                self.selection = Some(Rectangle::new(Point::from((from.loc.x + dx, from.loc.y + dy)), from.size));
                true
            }
            Some(Gesture::Resize { handle, from }) => {
                self.selection = Some(resized(from, handle, pos, self.bounds));
                true
            }
            Some(Gesture::Draw) => {
                let pos = self.clamp_into_target(pos);
                self.extend_draft(pos);
                true
            }
            None => {
                let hovered =
                    if self.mode == Mode::Window && !self.editing() && control.is_none() && !self.over_bar(pos) {
                        self.window_at(pos)
                    } else {
                        self.hovered
                    };
                let window_changed = hovered != self.hovered;
                self.hovered = hovered;
                hover_changed || window_changed
            }
        }
    }

    fn extend_draft(&mut self, pos: Pos) {
        let Some(draft) = self.draft.as_mut() else {
            return;
        };
        match &mut draft.shape {
            Shape::Pen(points) => {
                if points.last().is_none_or(|last| (last.x - pos.x).hypot(last.y - pos.y) >= 1.0) {
                    points.push(pos);
                }
            }
            Shape::Arrow(_, end) | Shape::Rect(_, end) | Shape::Highlight(_, end) | Shape::Blur(_, end) => *end = pos,
            Shape::Text { .. } | Shape::Step { .. } => {}
        }
    }

    pub fn press(&mut self, pos: Pos, control: Option<Control>) -> Outcome {
        let pos = self.clamp(pos);
        self.pointer = pos;
        if let Some(control) = control {
            return self.activate(control);
        }
        if self.over_bar(pos) {
            return Outcome::Nothing;
        }
        if self.typing {
            self.commit_text();
            if self.tool == Tool::Text {
                return Outcome::Redraw;
            }
        }
        if !self.editing() {
            return match self.mode {
                Mode::Area => {
                    self.begin_select(pos);
                    Outcome::Redraw
                }
                Mode::Window => match self.window_at(pos) {
                    Some(index) => {
                        self.window = Some(index);
                        self.enter_edit()
                    }
                    None => Outcome::Nothing,
                },
                Mode::Screen => self.enter_edit(),
            };
        }
        if let Some(handle) = self.handle_at(pos)
            && let Some(from) = self.target()
        {
            self.gesture = Some(Gesture::Resize { handle, from });
            return Outcome::Redraw;
        }
        let Some(target) = self.target() else {
            return Outcome::Nothing;
        };
        if !target.contains(pos) {
            if self.mode == Mode::Area && self.tool == Tool::Move {
                self.begin_select(pos);
                return Outcome::Redraw;
            }
            return Outcome::Nothing;
        }
        let swatch = self.swatch;
        let shape = match self.tool {
            Tool::Move => {
                if self.mode == Mode::Area {
                    self.gesture = Some(Gesture::Move { grab: pos, from: target });
                    return Outcome::Redraw;
                }
                return Outcome::Nothing;
            }
            Tool::Pen => Shape::Pen(vec![pos]),
            Tool::Arrow => Shape::Arrow(pos, pos),
            Tool::Rect => Shape::Rect(pos, pos),
            Tool::Highlight => Shape::Highlight(pos, pos),
            Tool::Blur => Shape::Blur(pos, pos),
            Tool::Text => {
                self.draft = Some(Mark::new(Shape::Text { at: pos, text: String::new() }, swatch));
                self.typing = true;
                return Outcome::Redraw;
            }
            Tool::Step => {
                let number = self.next_step();
                self.push_mark(Mark::new(Shape::Step { at: pos, number }, swatch));
                return Outcome::Redraw;
            }
        };
        self.draft = Some(Mark::new(shape, swatch));
        self.gesture = Some(Gesture::Draw);
        Outcome::Redraw
    }

    fn begin_select(&mut self, pos: Pos) {
        self.gesture = Some(Gesture::Select { origin: pos, before: self.selection });
        self.selection = Some(Rectangle::new(pos, Size::from((0.0, 0.0))));
    }

    #[must_use]
    pub fn over_bar(&self, pos: Pos) -> bool {
        self.bar_visible() && self.bar().rect.contains(pos)
    }

    pub fn release_button(&mut self, pos: Pos, button: u32) -> Outcome {
        if button != BTN_LEFT {
            return Outcome::Nothing;
        }
        self.release(pos)
    }

    pub fn release(&mut self, pos: Pos) -> Outcome {
        let pos = self.clamp(pos);
        let Some(gesture) = self.gesture.take() else {
            return Outcome::Nothing;
        };
        match gesture {
            Gesture::Select { origin, before } => {
                let area = rect_between(origin, pos);
                if area.size.w >= MIN_SIDE && area.size.h >= MIN_SIDE {
                    self.selection = Some(area);
                    if !self.editing() {
                        return self.enter_edit();
                    }
                } else {
                    self.selection = before;
                }
                Outcome::Redraw
            }
            Gesture::Move { .. } | Gesture::Resize { .. } => Outcome::Redraw,
            Gesture::Draw => {
                let pos = self.clamp_into_target(pos);
                self.extend_draft(pos);
                if let Some(draft) = self.draft.take().filter(|draft| draft.shape.is_meaningful()) {
                    self.push_mark(draft);
                }
                Outcome::Redraw
            }
        }
    }

    pub fn activate(&mut self, control: Control) -> Outcome {
        match control {
            Control::Mode(mode) => {
                self.set_mode(mode);
                Outcome::Redraw
            }
            Control::Shadow => {
                self.toggle_shadow();
                Outcome::Redraw
            }
            Control::Capture => self.capture(),
            Control::Close => Outcome::Close,
            Control::Tool(tool) => {
                self.set_tool(tool);
                Outcome::Redraw
            }
            Control::Swatch(swatch) => {
                self.set_swatch(swatch);
                Outcome::Redraw
            }
            Control::Undo => {
                self.undo();
                Outcome::Redraw
            }
            Control::Redo => {
                self.redo();
                Outcome::Redraw
            }
            Control::Copy => self.finish(Outcome::Copy),
            Control::Save => self.finish(Outcome::Save),
        }
    }

    pub fn finish(&mut self, outcome: Outcome) -> Outcome {
        if !self.editing() {
            return self.capture();
        }
        self.commit_text();
        self.gesture = None;
        self.draft = None;
        if self.can_capture() { outcome } else { Outcome::Nothing }
    }

    fn next_step(&self) -> u32 {
        let used = self.marks.iter().filter_map(|mark| match mark.shape {
            Shape::Step { number, .. } => Some(number),
            _ => None,
        });
        used.max().unwrap_or(0) + 1
    }

    fn push_mark(&mut self, mark: Mark) {
        self.marks.push(mark);
        self.redo.clear();
        self.revision += 1;
    }

    pub fn type_text(&mut self, text: &str) -> bool {
        if !self.typing {
            return false;
        }
        let Some(Mark { shape: Shape::Text { text: typed, .. }, sprite, .. }) = self.draft.as_mut() else {
            return false;
        };
        let printable: String = text.chars().filter(|c| !c.is_control()).collect();
        if printable.is_empty() {
            return false;
        }
        typed.push_str(&printable);
        *sprite = None;
        true
    }

    pub fn backspace(&mut self) -> bool {
        if !self.typing {
            return false;
        }
        let Some(Mark { shape: Shape::Text { text, .. }, sprite, .. }) = self.draft.as_mut() else {
            return false;
        };
        let removed = text.pop().is_some();
        *sprite = None;
        removed
    }

    pub fn commit_text(&mut self) -> bool {
        if !self.typing {
            return false;
        }
        self.typing = false;
        match self.draft.take() {
            Some(draft) if draft.shape.is_meaningful() => {
                self.push_mark(draft);
                true
            }
            _ => false,
        }
    }

    pub fn undo(&mut self) -> bool {
        if self.typing {
            self.typing = false;
            self.draft = None;
            return true;
        }
        let Some(mark) = self.marks.pop() else {
            return false;
        };
        self.redo.push(mark);
        self.revision += 1;
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.typing {
            return false;
        }
        let Some(mark) = self.redo.pop() else {
            return false;
        };
        self.marks.push(mark);
        self.revision += 1;
        true
    }

    pub fn nudge(&mut self, dx: f64, dy: f64, grow: bool) -> bool {
        if !self.resizable() {
            return false;
        }
        let Some(area) = self.selection else {
            return false;
        };
        let moved = if grow {
            let w = (area.size.w + dx).clamp(MIN_SIDE, (self.bounds.w - area.loc.x).max(MIN_SIDE));
            let h = (area.size.h + dy).clamp(MIN_SIDE, (self.bounds.h - area.loc.y).max(MIN_SIDE));
            Rectangle::new(area.loc, Size::from((w, h)))
        } else {
            Rectangle::new(Point::from((area.loc.x + dx, area.loc.y + dy)), area.size)
        };
        let moved = contained(moved, self.bounds);
        let changed = moved != area;
        self.selection = Some(moved);
        changed
    }

    pub fn sprites_wanted(&mut self) -> impl Iterator<Item = &mut Mark> {
        self.marks
            .iter_mut()
            .chain(self.draft.as_mut())
            .filter(|mark| mark.shape.needs_sprite() && mark.sprite.is_none())
    }
}

fn fits(area: Area, bounds: Size<f64, Logical>) -> bool {
    area.size.w >= MIN_SIDE
        && area.size.h >= MIN_SIDE
        && area.loc.x >= 0.0
        && area.loc.y >= 0.0
        && area.loc.x + area.size.w <= bounds.w
        && area.loc.y + area.size.h <= bounds.h
}

fn contained(area: Area, bounds: Size<f64, Logical>) -> Area {
    let w = area.size.w.clamp(MIN_SIDE.min(bounds.w), bounds.w);
    let h = area.size.h.clamp(MIN_SIDE.min(bounds.h), bounds.h);
    let x = area.loc.x.clamp(0.0, bounds.w - w);
    let y = area.loc.y.clamp(0.0, bounds.h - h);
    Rectangle::new(Point::from((x, y)), Size::from((w, h)))
}

#[must_use]
pub fn resized(from: Area, handle: Handle, pos: Pos, bounds: Size<f64, Logical>) -> Area {
    let (mx, my) = handle.moves();
    let (mut x0, mut y0) = (from.loc.x, from.loc.y);
    let (mut x1, mut y1) = (x0 + from.size.w, y0 + from.size.h);
    match mx {
        -1 => x0 = pos.x,
        1 => x1 = pos.x,
        _ => {}
    }
    match my {
        -1 => y0 = pos.y,
        1 => y1 = pos.y,
        _ => {}
    }
    contained(rect_between(Point::from((x0, y0)), Point::from((x1, y1))), bounds)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Memory {
    pub mode: Mode,
    pub selection: Option<Area>,
    pub tool: Tool,
    pub swatch: usize,
    pub shadow: bool,
}

impl Default for Memory {
    fn default() -> Self {
        Self { mode: Mode::Area, selection: None, tool: Tool::Move, swatch: 0, shadow: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Slot {
    Control(Control, f64),
    Gap(f64),
    Rule,
}

pub const CHOOSE_H: f64 = 64.0;
pub const CHOOSE_PAD: f64 = 8.0;
pub const MODE_W: f64 = 84.0;
pub const TOGGLE_W: f64 = 76.0;
pub const CONTROL_H: f64 = 48.0;
pub const DISC: f64 = 48.0;
pub const CLOSE: f64 = 32.0;
pub const CHOOSE_BOTTOM: f64 = 40.0;

pub const EDIT_H: f64 = 44.0;
pub const EDIT_PAD: f64 = 6.0;
pub const BUTTON: f64 = 32.0;
pub const SWATCH: f64 = 24.0;
pub const RULE: f64 = 13.0;
pub const EDIT_GAP: f64 = 10.0;
pub const EDGE: f64 = 12.0;

#[must_use]
pub fn choose_slots() -> Vec<Slot> {
    let mut slots: Vec<Slot> = Vec::new();
    for (index, mode) in MODES.into_iter().enumerate() {
        if index > 0 {
            slots.push(Slot::Gap(2.0));
        }
        slots.push(Slot::Control(Control::Mode(mode), MODE_W));
    }
    slots.extend([
        Slot::Rule,
        Slot::Control(Control::Shadow, TOGGLE_W),
        Slot::Rule,
        Slot::Control(Control::Capture, DISC),
        Slot::Gap(12.0),
        Slot::Control(Control::Close, CLOSE),
    ]);
    slots
}

#[must_use]
pub fn edit_slots() -> Vec<Slot> {
    let mut slots: Vec<Slot> = Vec::new();
    for (index, tool) in TOOLS.into_iter().enumerate() {
        if index > 0 {
            slots.push(Slot::Gap(2.0));
        }
        slots.push(Slot::Control(Control::Tool(tool), BUTTON));
    }
    slots.push(Slot::Rule);
    for index in 0..SWATCHES.len() {
        slots.push(Slot::Control(Control::Swatch(index), SWATCH));
    }
    slots.extend([
        Slot::Rule,
        Slot::Control(Control::Undo, BUTTON),
        Slot::Gap(2.0),
        Slot::Control(Control::Redo, BUTTON),
        Slot::Rule,
        Slot::Control(Control::Close, BUTTON),
        Slot::Gap(2.0),
        Slot::Control(Control::Copy, BUTTON),
        Slot::Gap(4.0),
        Slot::Control(Control::Save, BUTTON),
    ]);
    slots
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    pub rect: Area,
    pub items: Vec<(Control, Area)>,
    pub rules: Vec<Pos>,
}

impl Bar {
    #[must_use]
    pub fn control_at(&self, pos: Pos) -> Option<Control> {
        if !self.rect.contains(pos) {
            return None;
        }
        let local = pos - self.rect.loc;
        self.items.iter().find(|(_, area)| area.contains(local)).map(|(control, _)| *control)
    }

    #[must_use]
    pub fn item(&self, control: Control) -> Option<Area> {
        self.items.iter().find(|(item, _)| *item == control).map(|(_, area)| *area)
    }
}

fn lay_out(slots: &[Slot], height: f64, pad: f64, control_h: f64) -> (f64, Vec<(Control, Area)>, Vec<Pos>) {
    let mut x = pad;
    let mut items = Vec::new();
    let mut rules = Vec::new();
    let middle = (height / 2.0).round();
    for slot in slots {
        match *slot {
            Slot::Control(control, width) => {
                let h = match control {
                    Control::Capture => DISC,
                    Control::Close if height > EDIT_H => CLOSE,
                    Control::Swatch(_) => SWATCH,
                    _ => control_h,
                }
                .min(height);
                let y = ((height - h) / 2.0).round();
                items.push((control, Rectangle::new(Point::from((x, y)), Size::from((width, h)))));
                x += width;
            }
            Slot::Gap(width) => x += width,
            Slot::Rule => {
                rules.push(Point::from((x + (RULE / 2.0).floor(), middle)));
                x += RULE;
            }
        }
    }
    (x + pad, items, rules)
}

#[must_use]
pub fn choose_bar(bounds: Size<f64, Logical>) -> Bar {
    let (width, items, rules) = lay_out(&choose_slots(), CHOOSE_H, CHOOSE_PAD, CONTROL_H);
    let x = ((bounds.w - width) / 2.0).round().max(0.0);
    let y = (bounds.h - CHOOSE_BOTTOM - CHOOSE_H).max(0.0);
    Bar { rect: Rectangle::new(Point::from((x, y)), Size::from((width, CHOOSE_H))), items, rules }
}

type Layout = (f64, f64, Vec<(Control, Area)>, Vec<Pos>);

fn edit_layout(available: f64) -> Layout {
    let slots = edit_slots();
    let (width, items, rules) = lay_out(&slots, EDIT_H, EDIT_PAD, BUTTON);
    if width + EDGE * 2.0 <= available {
        return (width, EDIT_H, items, rules);
    }
    let split = slots.iter().position(|slot| *slot == Slot::Rule).unwrap_or(slots.len());
    let rows = [&slots[..split], slots.get(split + 1..).unwrap_or_default()];
    let laid: Vec<_> = rows.iter().map(|row| lay_out(row, EDIT_H, EDIT_PAD, BUTTON)).collect();
    let width = laid.iter().map(|(width, ..)| *width).fold(0.0, f64::max);
    let mut items = Vec::new();
    let mut rules = Vec::new();
    for (row, (row_width, row_items, row_rules)) in laid.into_iter().enumerate() {
        let shift = Point::from((((width - row_width) / 2.0).round(), row as f64 * (EDIT_H - EDIT_PAD)));
        items.extend(
            row_items.into_iter().map(|(control, area)| (control, Rectangle::new(area.loc + shift, area.size))),
        );
        rules.extend(row_rules.into_iter().map(|rule| rule + shift));
    }
    (width, EDIT_H * 2.0 - EDIT_PAD, items, rules)
}

#[must_use]
pub fn edit_bar(bounds: Size<f64, Logical>, target: Area) -> Bar {
    let (width, height, items, rules) = edit_layout(bounds.w);
    let bottom = target.loc.y + target.size.h;
    let y = if bottom + EDIT_GAP + height <= bounds.h - EDGE {
        bottom + EDIT_GAP
    } else if target.loc.y - EDIT_GAP - height >= EDGE {
        target.loc.y - EDIT_GAP - height
    } else {
        (bottom - EDIT_GAP - height).clamp(EDGE.min(bounds.h - height), (bounds.h - EDGE - height).max(0.0))
    };
    let centre = target.loc.x + target.size.w / 2.0;
    let max_x = (bounds.w - EDGE - width).max(0.0);
    let x = (centre - width / 2.0).round().clamp(EDGE.min(max_x), max_x);
    Bar { rect: Rectangle::new(Point::from((x, y.round())), Size::from((width, height))), items, rules }
}

impl Shot {
    #[must_use]
    pub fn bar(&self) -> Bar {
        match self.target().filter(|_| self.editing()) {
            Some(target) => edit_bar(self.bounds, target),
            None => choose_bar(self.bounds),
        }
    }

    #[must_use]
    pub fn bar_visible(&self) -> bool {
        !self.selecting()
            && !matches!(self.gesture, Some(Gesture::Draw | Gesture::Move { .. } | Gesture::Resize { .. }))
    }

    #[must_use]
    pub fn control_at(&self, pos: Pos) -> Option<Control> {
        self.bar_visible().then(|| self.bar().control_at(pos)).flatten().filter(|control| self.enabled(*control))
    }

    #[must_use]
    pub fn enabled(&self, control: Control) -> bool {
        match control {
            Control::Capture => self.can_capture(),
            Control::Undo => self.can_undo(),
            Control::Redo => self.can_redo(),
            _ => true,
        }
    }

    #[must_use]
    pub fn active(&self, control: Control) -> bool {
        match control {
            Control::Mode(mode) => self.mode == mode,
            Control::Shadow => self.shadow,
            Control::Tool(tool) => self.tool == tool,
            Control::Swatch(swatch) => self.swatch == swatch,
            _ => false,
        }
    }

    #[must_use]
    pub fn drawing(&self) -> bool {
        matches!(self.gesture, Some(Gesture::Draw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Pos {
        Point::from((x, y))
    }

    fn shot() -> Shot {
        let windows = vec![
            Rectangle::new(p(100.0, 100.0), Size::from((400.0, 300.0))),
            Rectangle::new(p(50.0, 50.0), Size::from((900.0, 600.0))),
        ];
        Shot::new(Size::from((1920.0, 1080.0)), windows, &Memory::default())
    }

    #[test]
    fn dragging_an_area_opens_the_editor_on_that_area() {
        let mut shot = shot();
        assert_eq!(shot.press(p(200.0, 150.0), None), Outcome::Redraw);
        shot.motion(p(500.0, 450.0), None);
        assert!(shot.selecting() && !shot.bar_visible(), "the bar steps aside while selecting");
        assert_eq!(shot.release(p(600.0, 400.0)), Outcome::Redraw);
        assert!(shot.editing());
        assert_eq!(shot.target(), Some(Rectangle::new(p(200.0, 150.0), Size::from((400.0, 250.0)))));
    }

    #[test]
    fn clicks_on_the_bar_outside_its_controls_never_reach_the_capture() {
        let mut shot = shot();
        let kept = Rectangle::new(p(200.0, 200.0), Size::from((300.0, 200.0)));
        shot.selection = Some(kept);
        let bar = shot.bar();
        let padding = bar.rect.loc + Point::from((CHOOSE_PAD / 2.0, bar.rect.size.h / 2.0));
        let rule = bar.rect.loc + bar.rules[0];
        for pos in [padding, rule] {
            assert_eq!(shot.press(pos, shot.control_at(pos)), Outcome::Nothing);
            shot.release(pos);
            assert_eq!(shot.selection, Some(kept));
        }
        shot.capture();
        shot.set_tool(Tool::Arrow);
        shot.press(p(250.0, 250.0), None);
        shot.release(p(300.0, 300.0));
        let bar = shot.bar();
        let padding = bar.rect.loc + Point::from((EDIT_PAD / 2.0, bar.rect.size.h / 2.0));
        shot.press(padding, shot.control_at(padding));
        shot.release(padding);
        assert!(shot.editing() && shot.marks.len() == 1 && shot.selection == Some(kept));
    }

    #[test]
    fn a_stray_click_outside_the_capture_keeps_it() {
        let mut shot = shot();
        let kept = Rectangle::new(p(200.0, 200.0), Size::from((300.0, 200.0)));
        shot.selection = Some(kept);
        shot.capture();
        shot.press(p(50.0, 50.0), None);
        shot.motion(p(52.0, 51.0), None);
        shot.release(p(52.0, 51.0));
        assert!(shot.editing(), "still editing");
        assert_eq!(shot.selection, Some(kept));
        shot.press(p(50.0, 50.0), None);
        shot.release(p(150.0, 120.0));
        assert_eq!(shot.selection, Some(Rectangle::new(p(50.0, 50.0), Size::from((100.0, 70.0)))));
    }

    #[test]
    fn a_click_without_a_drag_leaves_nothing_selected() {
        let mut shot = shot();
        shot.press(p(10.0, 10.0), None);
        shot.release(p(11.0, 11.0));
        assert!(!shot.editing());
        assert_eq!(shot.selection, None);
        assert!(!shot.can_capture());
    }

    #[test]
    fn the_window_mode_picks_the_topmost_window_under_the_pointer() {
        let mut shot = shot();
        shot.set_mode(Mode::Window);
        shot.motion(p(200.0, 200.0), None);
        assert_eq!(shot.hovered, Some(0));
        shot.motion(p(700.0, 500.0), None);
        assert_eq!(shot.hovered, Some(1));
        assert_eq!(shot.press(p(700.0, 500.0), None), Outcome::Window(1));
        assert!(shot.editing());
        assert_eq!(shot.tool, Tool::Pen, "nothing to move in a window capture");
        assert!(!shot.set_mode(Mode::Screen), "the mode is fixed once editing");
    }

    #[test]
    fn the_screen_mode_captures_everything_on_a_click() {
        let mut shot = shot();
        shot.set_mode(Mode::Screen);
        shot.press(p(5.0, 5.0), None);
        assert_eq!(shot.target(), Some(shot.full()));
        assert!(!shot.shadow_applies(), "a full screen never gets a shadow");
    }

    #[test]
    fn handles_resize_and_dragging_inside_moves_the_selection() {
        let mut shot = shot();
        shot.selection = Some(Rectangle::new(p(100.0, 100.0), Size::from((200.0, 100.0))));
        shot.capture();
        assert_eq!(shot.handle_at(p(301.0, 199.0)), Some(Handle::SE));
        shot.press(p(300.0, 200.0), None);
        shot.motion(p(400.0, 260.0), None);
        shot.release(p(400.0, 260.0));
        assert_eq!(shot.target(), Some(Rectangle::new(p(100.0, 100.0), Size::from((300.0, 160.0)))));
        shot.press(p(150.0, 150.0), None);
        shot.motion(p(-500.0, 160.0), None);
        shot.release(p(-500.0, 160.0));
        assert_eq!(shot.target().map(|area| area.loc), Some(p(0.0, 110.0)), "kept on the screen");
    }

    #[test]
    fn a_handle_dragged_past_the_far_edge_flips_instead_of_inverting() {
        let from = Rectangle::new(p(100.0, 100.0), Size::from((100.0, 100.0)));
        let flipped = resized(from, Handle::W, p(300.0, 150.0), Size::from((1920.0, 1080.0)));
        assert_eq!(flipped, Rectangle::new(p(200.0, 100.0), Size::from((100.0, 100.0))));
    }

    fn inside(area: Area, bounds: Size<f64, Logical>) -> bool {
        area.size.w >= MIN_SIDE
            && area.size.h >= MIN_SIDE
            && area.loc.x >= 0.0
            && area.loc.y >= 0.0
            && area.loc.x + area.size.w <= bounds.w
            && area.loc.y + area.size.h <= bounds.h
    }

    #[test]
    fn a_west_handle_dragged_to_the_right_edge_keeps_a_minimum_capture_on_the_screen() {
        let bounds = Size::from((1920.0, 1080.0));
        let from = Rectangle::new(p(1000.0, 100.0), Size::from((920.0, 200.0)));
        let squeezed = resized(from, Handle::W, p(1920.0, 150.0), bounds);
        assert!(inside(squeezed, bounds), "{squeezed:?} must stay on the screen");
        assert_eq!(squeezed, Rectangle::new(p(1916.0, 100.0), Size::from((MIN_SIDE, 200.0))));
        let squeezed = resized(from, Handle::S, p(1500.0, 0.0), bounds);
        assert!(inside(squeezed, bounds));
    }

    #[test]
    fn dragging_a_handle_past_the_edge_never_leaves_a_selection_that_cannot_be_saved() {
        let mut shot = shot();
        shot.selection = Some(Rectangle::new(p(1000.0, 100.0), Size::from((920.0, 200.0))));
        shot.capture();
        assert_eq!(shot.handle_at(p(1000.0, 200.0)), Some(Handle::W));
        shot.press(p(1000.0, 200.0), None);
        shot.motion(p(5000.0, 200.0), None);
        shot.release(p(5000.0, 200.0));
        let target = shot.target().expect("selection");
        assert!(inside(target, shot.bounds), "{target:?} must stay on the screen");
        assert!(shot.can_capture());
        assert_eq!(shot.finish(Outcome::Save), Outcome::Save);
    }

    #[test]
    fn growing_a_selection_that_is_partly_off_screen_neither_panics_nor_stays_off_screen() {
        let mut shot = shot();
        shot.selection = Some(Rectangle::new(p(1918.0, 100.0), Size::from((100.0, 100.0))));
        shot.capture();
        assert!(shot.nudge(1.0, 1.0, true));
        let target = shot.target().expect("selection");
        assert!(inside(target, shot.bounds), "{target:?} must be pulled back onto the screen");
        shot.selection = Some(Rectangle::new(p(1918.0, 1070.0), Size::from((100.0, 100.0))));
        shot.nudge(-10.0, -10.0, false);
        let target = shot.target().expect("selection");
        assert!(inside(target, shot.bounds), "{target:?} must be pulled back onto the screen");
    }

    #[test]
    fn nudging_keeps_the_selection_in_bounds_at_every_edge() {
        let mut shot = shot();
        shot.selection = Some(Rectangle::new(p(1800.0, 960.0), Size::from((120.0, 120.0))));
        shot.capture();
        assert!(!shot.nudge(10.0, 10.0, false), "already in the corner");
        assert!(!shot.nudge(10.0, 10.0, true), "cannot grow past the corner");
        assert!(shot.nudge(-10.0, 0.0, false));
        assert_eq!(shot.selection.map(|area| area.loc), Some(p(1790.0, 960.0)));
        assert!(shot.nudge(-200.0, -200.0, true));
        assert_eq!(shot.selection.map(|area| area.size), Some(Size::from((MIN_SIDE, MIN_SIDE))));
    }

    #[test]
    fn the_capture_disc_takes_the_highlighted_window_not_the_one_under_the_bar() {
        let mut shot = shot();
        shot.windows.push(Rectangle::new(p(0.0, 900.0), Size::from((1920.0, 180.0))));
        shot.set_mode(Mode::Window);
        shot.motion(p(200.0, 200.0), None);
        assert_eq!(shot.hovered, Some(0));
        let bar = shot.bar();
        let padding = bar.rect.loc + Point::from((CHOOSE_PAD / 2.0, bar.rect.size.h / 2.0));
        assert_eq!(shot.window_at(padding), Some(2), "a window lies under the bar");
        shot.motion(padding, shot.control_at(padding));
        assert_eq!(shot.hovered, Some(0), "the bar's padding keeps the highlight");
        let capture = bar.item(Control::Capture).expect("capture disc");
        let centre = bar.rect.loc + capture.loc + Point::from((capture.size.w / 2.0, capture.size.h / 2.0));
        shot.motion(centre, shot.control_at(centre));
        assert_eq!(shot.activate(Control::Capture), Outcome::Window(0));
    }

    #[test]
    fn choosing_the_window_mode_from_the_bar_does_not_pick_the_window_under_it() {
        let mut shot = shot();
        shot.windows.push(Rectangle::new(p(0.0, 900.0), Size::from((1920.0, 180.0))));
        shot.set_mode(Mode::Window);
        shot.motion(p(700.0, 500.0), None);
        assert_eq!(shot.hovered, Some(1));
        shot.set_mode(Mode::Area);
        let bar = shot.bar();
        let button = bar.item(Control::Mode(Mode::Window)).expect("window mode button");
        let over = bar.rect.loc + button.loc + Point::from((button.size.w / 2.0, button.size.h / 2.0));
        shot.motion(over, shot.control_at(over));
        shot.press(over, shot.control_at(over));
        assert_eq!(shot.mode, Mode::Window);
        assert_eq!(shot.hovered, None, "nothing was highlighted before the pointer reached the bar");
        assert!(!shot.can_capture());
        shot.set_mode(Mode::Screen);
        shot.motion(p(200.0, 200.0), None);
        shot.set_mode(Mode::Window);
        assert_eq!(shot.hovered, Some(0), "away from the bar the window under the pointer is picked");
    }

    #[test]
    fn copying_before_the_capture_exists_still_hands_the_window_over_for_a_window_alone_render() {
        let mut shot = shot();
        shot.set_mode(Mode::Window);
        shot.motion(p(700.0, 500.0), None);
        assert_eq!(shot.finish(Outcome::Copy), Outcome::Window(1));
        assert!(shot.editing());
        assert_eq!(shot.finish(Outcome::Copy), Outcome::Copy);
    }

    #[test]
    fn copying_before_the_capture_exists_only_opens_the_editor_in_area_and_screen_modes() {
        let mut area = shot();
        assert_eq!(area.finish(Outcome::Copy), Outcome::Nothing, "nothing selected yet");
        area.selection = Some(Rectangle::new(p(200.0, 200.0), Size::from((300.0, 200.0))));
        assert_eq!(area.finish(Outcome::Copy), Outcome::Redraw);
        assert!(area.editing());
        assert_eq!(area.finish(Outcome::Copy), Outcome::Copy);
        let mut screen = shot();
        screen.set_mode(Mode::Screen);
        assert_eq!(screen.finish(Outcome::Save), Outcome::Redraw);
        assert_eq!(screen.finish(Outcome::Save), Outcome::Save);
    }

    #[test]
    fn copying_in_the_window_mode_without_a_window_under_the_pointer_does_nothing() {
        let mut shot = shot();
        shot.set_mode(Mode::Window);
        shot.motion(p(5.0, 5.0), None);
        assert_eq!(shot.finish(Outcome::Copy), Outcome::Nothing);
        assert!(!shot.editing());
    }

    #[test]
    fn releasing_another_button_does_not_end_the_selection_in_progress() {
        let mut shot = shot();
        shot.press(p(200.0, 150.0), None);
        shot.motion(p(500.0, 450.0), None);
        assert_eq!(shot.release_button(p(500.0, 450.0), 0x111), Outcome::Nothing);
        assert_eq!(shot.release_button(p(500.0, 450.0), 0x112), Outcome::Nothing);
        assert!(shot.selecting() && !shot.editing(), "the drag goes on");
        shot.motion(p(600.0, 400.0), None);
        assert_eq!(shot.release_button(p(600.0, 400.0), BTN_LEFT), Outcome::Redraw);
        assert!(shot.editing());
        assert_eq!(shot.target(), Some(Rectangle::new(p(200.0, 150.0), Size::from((400.0, 250.0)))));
    }

    #[test]
    fn releasing_another_button_does_not_commit_a_half_drawn_stroke() {
        let mut shot = shot();
        shot.set_mode(Mode::Screen);
        shot.capture();
        shot.set_tool(Tool::Arrow);
        shot.press(p(100.0, 100.0), None);
        shot.motion(p(200.0, 200.0), None);
        assert_eq!(shot.release_button(p(200.0, 200.0), 0x111), Outcome::Nothing);
        assert!(shot.drawing() && shot.marks.is_empty());
        shot.motion(p(300.0, 300.0), None);
        shot.release_button(p(300.0, 300.0), BTN_LEFT);
        assert_eq!(shot.marks.len(), 1);
        assert_eq!(shot.marks[0].shape, Shape::Arrow(p(100.0, 100.0), p(300.0, 300.0)));
    }

    #[test]
    fn strokes_are_kept_inside_the_capture_and_undo_redo_walk_them() {
        let mut shot = shot();
        shot.selection = Some(Rectangle::new(p(100.0, 100.0), Size::from((200.0, 200.0))));
        shot.capture();
        shot.set_tool(Tool::Arrow);
        shot.press(p(150.0, 150.0), None);
        shot.motion(p(900.0, 900.0), None);
        shot.release(p(900.0, 900.0));
        assert_eq!(shot.marks.len(), 1);
        assert_eq!(shot.marks[0].shape, Shape::Arrow(p(150.0, 150.0), p(300.0, 300.0)));
        assert!(shot.undo() && shot.marks.is_empty() && shot.can_redo());
        assert!(shot.redo() && shot.marks.len() == 1);
        shot.press(p(120.0, 120.0), None);
        shot.release(p(121.0, 121.0));
        assert_eq!(shot.marks.len(), 1, "a tap leaves no arrow behind");
        assert!(!shot.can_redo() || shot.marks.len() == 1);
    }

    #[test]
    fn new_marks_clear_the_redo_history() {
        let mut shot = shot();
        shot.set_mode(Mode::Screen);
        shot.capture();
        shot.set_tool(Tool::Step);
        shot.press(p(10.0, 10.0), None);
        shot.undo();
        assert!(shot.can_redo());
        shot.press(p(20.0, 20.0), None);
        assert!(!shot.can_redo());
    }

    #[test]
    fn steps_count_up_from_the_highest_one_left() {
        let mut shot = shot();
        shot.set_mode(Mode::Screen);
        shot.capture();
        shot.set_tool(Tool::Step);
        for x in [10.0, 20.0, 30.0] {
            shot.press(p(x, 10.0), None);
        }
        shot.undo();
        shot.press(p(40.0, 10.0), None);
        let numbers: Vec<u32> = shot
            .marks
            .iter()
            .filter_map(|mark| match mark.shape {
                Shape::Step { number, .. } => Some(number),
                _ => None,
            })
            .collect();
        assert_eq!(numbers, [1, 2, 3]);
    }

    #[test]
    fn text_is_typed_into_a_draft_and_committed_when_the_pointer_moves_on() {
        let mut shot = shot();
        shot.set_mode(Mode::Screen);
        shot.capture();
        shot.set_tool(Tool::Text);
        shot.press(p(10.0, 10.0), None);
        assert!(shot.typing);
        assert!(shot.type_text("Привіт\u{8}"));
        assert!(shot.backspace());
        assert!(matches!(&shot.draft, Some(Mark { shape: Shape::Text { text, .. }, .. }) if text == "Приві"));
        shot.press(p(400.0, 400.0), None);
        assert!(!shot.typing && shot.draft.is_none());
        assert_eq!(shot.marks.len(), 1);
        shot.press(p(500.0, 500.0), None);
        assert!(shot.typing);
        assert!(!shot.commit_text() && shot.marks.len() == 1, "empty text is dropped");
    }

    #[test]
    fn copy_and_save_need_an_edited_capture() {
        let mut shot = shot();
        assert_eq!(shot.activate(Control::Save), Outcome::Nothing);
        shot.set_mode(Mode::Screen);
        assert_eq!(shot.activate(Control::Save), Outcome::Redraw, "the first press only opens the editor");
        assert_eq!(shot.activate(Control::Copy), Outcome::Copy);
    }

    #[test]
    fn a_remembered_selection_comes_back_only_while_it_fits() {
        let memory = Memory {
            selection: Some(Rectangle::new(p(1500.0, 900.0), Size::from((300.0, 100.0)))),
            ..Memory::default()
        };
        assert!(Shot::new(Size::from((1920.0, 1080.0)), Vec::new(), &memory).can_capture());
        assert!(!Shot::new(Size::from((1280.0, 720.0)), Vec::new(), &memory).can_capture());
    }

    #[test]
    fn the_edit_bar_goes_below_then_above_then_inside_the_capture() {
        let bounds = Size::from((1920.0, 1080.0));
        let below = edit_bar(bounds, Rectangle::new(p(400.0, 100.0), Size::from((800.0, 300.0))));
        assert!((below.rect.loc.y - 410.0).abs() < 1e-9);
        let above = edit_bar(bounds, Rectangle::new(p(400.0, 300.0), Size::from((800.0, 760.0))));
        assert!((above.rect.loc.y - (300.0 - EDIT_GAP - EDIT_H)).abs() < 1e-9);
        let inside = edit_bar(bounds, Rectangle::from_size(bounds));
        assert!(inside.rect.loc.y + EDIT_H <= 1080.0 - EDIT_GAP + 1e-9 && inside.rect.loc.y > 900.0);
        let left = edit_bar(bounds, Rectangle::new(p(0.0, 0.0), Size::from((40.0, 40.0))));
        assert!((left.rect.loc.x - EDGE).abs() < 1e-9, "kept off the screen edge");
    }

    #[test]
    fn every_bar_control_is_hit_where_it_is_laid_out_and_rules_stay_between() {
        let bounds = Size::from((1920.0, 1080.0));
        let narrow = Size::from((540.0, 960.0));
        let capture = Rectangle::new(p(100.0, 100.0), Size::from((300.0, 300.0)));
        for bar in [
            choose_bar(bounds),
            edit_bar(bounds, Rectangle::new(p(400.0, 100.0), Size::from((600.0, 300.0)))),
            edit_bar(narrow, capture),
        ] {
            for (control, area) in &bar.items {
                let centre = bar.rect.loc + area.loc + Point::from((area.size.w / 2.0, area.size.h / 2.0));
                assert_eq!(bar.control_at(centre), Some(*control));
                assert!(area.loc.x >= 0.0 && area.loc.x + area.size.w <= bar.rect.size.w);
                assert!(area.loc.y >= 0.0 && area.loc.y + area.size.h <= bar.rect.size.h);
            }
            assert!(bar.rules.iter().all(|rule| bar.items.iter().all(|(_, area)| !area.contains(*rule))));
        }
    }

    #[test]
    fn the_edit_bar_wraps_onto_two_rows_on_a_narrow_output() {
        let narrow = Size::from((540.0, 960.0));
        let bar = edit_bar(narrow, Rectangle::new(p(100.0, 100.0), Size::from((300.0, 300.0))));
        assert!(bar.rect.loc.x >= EDGE && bar.rect.loc.x + bar.rect.size.w <= narrow.w - EDGE, "{:?}", bar.rect);
        assert!(bar.rect.size.h > EDIT_H, "two rows");
        let tools = bar.item(Control::Tool(Tool::Move)).expect("tools");
        let save = bar.item(Control::Save).expect("save");
        assert!(save.loc.y > tools.loc.y + tools.size.h / 2.0, "the swatches and actions move to the second row");
        assert_eq!(bar.items.len(), edit_bar(Size::from((1920.0, 1080.0)), bar.rect).items.len());
        let wide = edit_bar(Size::from((1920.0, 1080.0)), Rectangle::new(p(100.0, 100.0), Size::from((300.0, 300.0))));
        assert!((wide.rect.size.h - EDIT_H).abs() < 1e-9, "one row wherever it fits");
    }

    #[test]
    fn the_mode_stays_put_while_an_area_is_being_dragged() {
        let mut shot = shot();
        shot.press(p(200.0, 150.0), None);
        shot.motion(p(500.0, 450.0), None);
        assert!(!shot.set_mode(Mode::Window));
        assert_eq!(shot.mode, Mode::Area);
        assert!(shot.selecting());
    }

    #[test]
    fn disabled_controls_are_not_hit() {
        let mut shot = shot();
        let bar = shot.bar();
        let capture = bar.item(Control::Capture).expect("capture disc");
        let centre = bar.rect.loc + capture.loc + Point::from((capture.size.w / 2.0, capture.size.h / 2.0));
        assert_eq!(shot.control_at(centre), None, "nothing selected yet");
        shot.set_mode(Mode::Screen);
        assert_eq!(shot.control_at(centre), Some(Control::Capture));
    }
}

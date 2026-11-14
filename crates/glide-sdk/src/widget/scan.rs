use std::f32::consts::{FRAC_PI_2, TAU};
use std::time::{Duration, Instant};

use glide_iced::Paint;
use glide_iced::style::{self, SP1, SP2};
use glide_theme::TextStyle;
use glide_theme::motion;
use glide_theme::shape::R4;
use iced::widget::canvas::{self, Action, Frame, Geometry, LineCap, LineJoin, Path, Stroke};
use iced::widget::{Space, canvas as surface, column, container, image, row, stack};
use iced::{Alignment, Color, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector, mouse, window};

use super::text;
use crate::anim::phase;

pub const TICKS: usize = 72;
pub const SCAN_SIDE: f32 = 380.0;
pub const LENS_SIDE: f32 = 292.0;
const GLOW: Duration = Duration::from_millis(1400);
const NUDGE: Duration = Duration::from_millis(1200);
const BREATHE: Duration = Duration::from_millis(1200);
const SHAKE: Duration = Duration::from_millis(420);
const CUE_SIDE: f32 = 44.0;
const CUE_GAP: f32 = 18.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Idle,
    Done,
    Active,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    Left,
    Right,
    Tilt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Idle,
    Ready,
    Scanning,
    Recognised,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMark {
    Front,
    Left,
    Right,
    Tilt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Current,
    Done,
    Failed,
}

#[derive(Debug, Clone)]
pub struct Step {
    pub mark: StepMark,
    pub label: String,
    pub state: StepState,
}

pub struct Scan {
    pub ticks: Vec<Tick>,
    pub frame: Option<image::Handle>,
    pub dimmed: bool,
    pub cue: Option<Cue>,
    pub done: bool,
}

#[derive(Debug)]
pub struct Clock {
    started: Instant,
    mood: Option<Mood>,
}

impl Default for Clock {
    fn default() -> Self {
        Self { started: Instant::now(), mood: None }
    }
}

fn redraw<M>(event: &Event, animating: bool) -> Option<Action<M>> {
    let frame = matches!(event, Event::Window(window::Event::RedrawRequested(_)));
    (frame && animating && !motion::is_disabled()).then(Action::request_redraw)
}

fn pulse(started: Instant, period: Duration) -> f32 {
    0.5 + 0.5 * (TAU * phase(started, period)).cos()
}

struct Ring {
    paint: Paint,
    ticks: Vec<Tick>,
}

impl<M> canvas::Program<M> for Ring {
    type State = Clock;

    fn update(&self, _: &mut Clock, event: &Event, _: Rectangle, _: mouse::Cursor) -> Option<Action<M>> {
        redraw(event, self.ticks.contains(&Tick::Active))
    }

    fn draw(
        &self, state: &Clock, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let outer = bounds.width.min(bounds.height) / 2.0 - 2.5;
        let glow = 0.35 + 0.65 * pulse(state.started, GLOW);
        let count = self.ticks.len().max(1) as f32;
        for (index, tick) in self.ticks.iter().enumerate() {
            let color = match tick {
                Tick::Idle => self.paint.fill_3(),
                Tick::Done => self.paint.ok(),
                Tick::Active => self.paint.acc_bg().scale_alpha(glow),
                Tick::Failed => self.paint.danger(),
            };
            let angle = index as f32 / count * TAU - FRAC_PI_2;
            let (sin, cos) = angle.sin_cos();
            let line = Path::line(
                Point::new(center.x + cos * (outer - 18.0), center.y + sin * (outer - 18.0)),
                Point::new(center.x + cos * outer, center.y + sin * outer),
            );
            frame.stroke(&line, Stroke::default().with_color(color).with_width(5.0).with_line_cap(LineCap::Round));
        }
        vec![frame.into_geometry()]
    }
}

struct CueMark {
    paint: Paint,
    cue: Cue,
}

impl<M> canvas::Program<M> for CueMark {
    type State = Clock;

    fn update(&self, _: &mut Clock, event: &Event, _: Rectangle, _: mouse::Cursor) -> Option<Action<M>> {
        redraw(event, true)
    }

    fn draw(
        &self, state: &Clock, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let swing = (TAU * phase(state.started, NUDGE)).sin().max(0.0) * 8.0;
        let offset = match self.cue {
            Cue::Left => Vector::new(-swing, 0.0),
            Cue::Right => Vector::new(swing, 0.0),
            Cue::Tilt => Vector::new(0.0, (TAU * phase(state.started, NUDGE)).sin() * 8.0),
        };
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0) + offset;
        frame.fill(&Path::circle(center, CUE_SIDE / 2.0), self.paint.acc_bg());
        let stroke = Stroke::default()
            .with_color(self.paint.acc_fg())
            .with_width(2.6)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round);
        let chevron = |frame: &mut Frame, points: [(f32, f32); 3]| {
            let path = Path::new(|p| {
                p.move_to(center + Vector::new(points[0].0, points[0].1));
                p.line_to(center + Vector::new(points[1].0, points[1].1));
                p.line_to(center + Vector::new(points[2].0, points[2].1));
            });
            frame.stroke(&path, stroke);
        };
        match self.cue {
            Cue::Left => chevron(&mut frame, [(3.0, -7.0), (-4.0, 0.0), (3.0, 7.0)]),
            Cue::Right => chevron(&mut frame, [(-3.0, -7.0), (4.0, 0.0), (-3.0, 7.0)]),
            Cue::Tilt => {
                chevron(&mut frame, [(-5.0, -3.0), (0.0, -8.0), (5.0, -3.0)]);
                chevron(&mut frame, [(-5.0, 3.0), (0.0, 8.0), (5.0, 3.0)]);
            }
        }
        vec![frame.into_geometry()]
    }
}

fn face_path(side: f32, inner: Mood) -> (Path, Path) {
    let unit = side / 24.0;
    let at = |x: f32, y: f32| Point::new(x * unit, y * unit);
    let corners = Path::new(|p| {
        for (start, corner, end) in [
            ((4.0, 9.0), (4.0, 4.0), (9.0, 4.0)),
            ((15.0, 4.0), (20.0, 4.0), (20.0, 9.0)),
            ((20.0, 15.0), (20.0, 20.0), (15.0, 20.0)),
            ((9.0, 20.0), (4.0, 20.0), (4.0, 15.0)),
        ] {
            p.move_to(at(start.0, start.1));
            p.arc_to(at(corner.0, corner.1), at(end.0, end.1), 2.0 * unit);
            p.line_to(at(end.0, end.1));
        }
    });
    let features = Path::new(|p| match inner {
        Mood::Recognised => {
            p.move_to(at(8.0, 12.5));
            p.line_to(at(11.0, 15.5));
            p.line_to(at(16.0, 9.5));
        }
        mood => {
            p.move_to(at(9.5, 9.5));
            p.line_to(at(9.5, 11.0));
            p.move_to(at(14.5, 9.5));
            p.line_to(at(14.5, 11.0));
            if mood == Mood::Rejected {
                p.move_to(at(9.0, 16.0));
                p.quadratic_curve_to(at(12.0, 13.0), at(15.0, 16.0));
            } else {
                p.move_to(at(9.0, 14.5));
                p.quadratic_curve_to(at(12.0, 17.5), at(15.0, 14.5));
            }
        }
    });
    (corners, features)
}

struct FaceMark {
    color: Color,
    mood: Mood,
    weight: f32,
}

impl<M> canvas::Program<M> for FaceMark {
    type State = Clock;

    fn update(&self, state: &mut Clock, event: &Event, _: Rectangle, _: mouse::Cursor) -> Option<Action<M>> {
        if state.mood != Some(self.mood) {
            state.mood = Some(self.mood);
            state.started = Instant::now();
        }
        let shaking = self.mood == Mood::Rejected && state.started.elapsed() < SHAKE;
        redraw(event, self.mood == Mood::Scanning || shaking)
    }

    fn draw(
        &self, state: &Clock, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let side = bounds.width.min(bounds.height);
        let stroke = Stroke::default()
            .with_color(self.color)
            .with_width(self.weight * side / 24.0)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round);
        let (corners, features) = face_path(side, self.mood);
        let shake = if self.mood == Mood::Rejected && !motion::is_disabled() {
            let t = (state.started.elapsed().as_secs_f32() / SHAKE.as_secs_f32()).min(1.0);
            (t * TAU * 2.5).sin() * (1.0 - t) * side * 0.08
        } else {
            0.0
        };
        let breathe =
            if self.mood == Mood::Scanning { 1.0 - 0.14 * (1.0 - pulse(state.started, BREATHE)) } else { 1.0 };
        frame.with_save(|frame| {
            frame.translate(Vector::new(shake, 0.0));
            frame.stroke(&features, stroke);
            frame.translate(Vector::new(side / 2.0, side / 2.0));
            frame.scale(breathe);
            frame.translate(Vector::new(-side / 2.0, -side / 2.0));
            frame.stroke(&corners, stroke);
        });
        vec![frame.into_geometry()]
    }
}

struct LensMask {
    ground: Color,
}

impl<M> canvas::Program<M> for LensMask {
    type State = ();

    fn draw(&self, (): &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let side = bounds.width.min(bounds.height);
        let corners = Path::new(|p| {
            p.rectangle(Point::ORIGIN, bounds.size());
            p.circle(Point::new(bounds.width / 2.0, bounds.height / 2.0), side / 2.0);
        });
        frame.fill(
            &corners,
            canvas::Fill { style: canvas::Style::Solid(self.ground), rule: canvas::fill::Rule::EvenOdd },
        );
        vec![frame.into_geometry()]
    }
}

#[must_use]
pub fn sheet_card<'a, M: 'a>(paint: &Paint, width: f32, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let paint = *paint;
    container(content).padding([SP2 * 2.5, SP2 * 5.0]).width(width).style(move |_| style::popover(&paint)).into()
}

#[must_use]
pub fn face_mark<'a, M: 'a>(color: Color, side: f32, mood: Mood) -> Element<'a, M> {
    surface(FaceMark { color, mood, weight: 1.6 }).width(side).height(side).into()
}

#[must_use]
pub fn face_tile<'a, M: 'a>(paint: &Paint, side: f32, mood: Mood) -> Element<'a, M> {
    let paint = *paint;
    let (fill, ink) = match mood {
        Mood::Idle => (paint.fill_1(), paint.fg_2()),
        Mood::Ready | Mood::Recognised => (paint.ok_soft(), paint.ok()),
        Mood::Scanning => (paint.acc_soft(), paint.acc_bg()),
        Mood::Rejected => (paint.danger_soft(), paint.danger()),
    };
    let mark = surface(FaceMark { color: ink, mood, weight: 1.4 }).width(side * 0.62).height(side * 0.62);
    container(mark).center(side).style(move |_| style::rounded(fill, R4)).into()
}

#[must_use]
pub fn ring_ticks(tone: impl Fn(f32) -> Tick) -> Vec<Tick> {
    (0..TICKS).map(|index| tone(index as f32 * 360.0 / TICKS as f32)).collect()
}

#[must_use]
pub fn scan<'a, M: 'a>(paint: &Paint, view: Scan) -> Element<'a, M> {
    let paint = *paint;
    let lens: Element<'a, M> = match view.frame {
        Some(handle) => stack![
            container(
                image(handle)
                    .width(LENS_SIDE - 2.0)
                    .height(LENS_SIDE - 2.0)
                    .content_fit(iced::ContentFit::Cover)
                    .border_radius(LENS_SIDE / 2.0)
                    .opacity(if view.dimmed { 0.45_f32 } else { 1.0_f32 })
            )
            .center(LENS_SIDE),
            surface(LensMask { ground: paint.popover_bg() }).width(LENS_SIDE).height(LENS_SIDE),
        ]
        .into(),
        None => container(Space::new())
            .width(LENS_SIDE)
            .height(LENS_SIDE)
            .style(move |_| style::rounded(paint.sunken_bg(), LENS_SIDE / 2.0))
            .into(),
    };
    let mut layers = stack![container(lens).center(SCAN_SIDE)];
    if view.done {
        let disc = container(face_mark(paint.ok(), LENS_SIDE * 0.42, Mood::Recognised))
            .center(LENS_SIDE)
            .style(move |_| style::rounded(paint.ok_soft(), LENS_SIDE / 2.0));
        layers = layers.push(container(disc).center(SCAN_SIDE));
    }
    layers = layers.push(surface(Ring { paint, ticks: view.ticks }).width(SCAN_SIDE).height(SCAN_SIDE));
    let side = SCAN_SIDE + 2.0 * (CUE_SIDE + CUE_GAP);
    let cue = |cue: Cue| -> Element<'a, M> {
        surface(CueMark { paint, cue }).width(CUE_SIDE + 16.0).height(CUE_SIDE + 16.0).into()
    };
    let blank = || -> Element<'a, M> { Space::new().width(CUE_SIDE + 16.0).height(CUE_SIDE + 16.0).into() };
    let (start, end) = match view.cue {
        Some(Cue::Left) => (cue(Cue::Left), blank()),
        Some(Cue::Right) => (blank(), cue(Cue::Right)),
        Some(Cue::Tilt) => (cue(Cue::Tilt), cue(Cue::Tilt)),
        None => (blank(), blank()),
    };
    container(row![start, layers, end].spacing(CUE_GAP - 8.0).align_y(Alignment::Center))
        .width(Length::Fixed(side))
        .center_x(Length::Fixed(side))
        .into()
}

struct StepGlyph {
    color: Color,
    mark: Option<StepMark>,
}

impl<M> canvas::Program<M> for StepGlyph {
    type State = ();

    fn draw(&self, (): &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let side = bounds.width.min(bounds.height);
        let unit = side / 24.0;
        let at = |x: f32, y: f32| Point::new(x * unit, y * unit);
        let stroke = Stroke::default()
            .with_color(self.color)
            .with_width(2.4 * unit)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round);
        let path = match self.mark {
            None => Path::new(|p| {
                p.move_to(at(5.0, 12.0));
                p.line_to(at(10.0, 17.0));
                p.line_to(at(19.0, 7.0));
            }),
            Some(StepMark::Front) => {
                let (corners, features) = face_path(side, Mood::Idle);
                frame.stroke(&corners, stroke);
                features
            }
            Some(StepMark::Left) => Path::new(|p| {
                p.move_to(at(19.0, 12.0));
                p.line_to(at(5.0, 12.0));
                p.move_to(at(11.0, 6.0));
                p.line_to(at(5.0, 12.0));
                p.line_to(at(11.0, 18.0));
            }),
            Some(StepMark::Right) => Path::new(|p| {
                p.move_to(at(5.0, 12.0));
                p.line_to(at(19.0, 12.0));
                p.move_to(at(13.0, 6.0));
                p.line_to(at(19.0, 12.0));
                p.line_to(at(13.0, 18.0));
            }),
            Some(StepMark::Tilt) => Path::new(|p| {
                p.move_to(at(12.0, 3.0));
                p.line_to(at(12.0, 21.0));
                p.move_to(at(7.0, 8.0));
                p.line_to(at(12.0, 3.0));
                p.line_to(at(17.0, 8.0));
                p.move_to(at(7.0, 16.0));
                p.line_to(at(12.0, 21.0));
                p.line_to(at(17.0, 16.0));
            }),
        };
        frame.stroke(&path, stroke);
        vec![frame.into_geometry()]
    }
}

#[must_use]
pub fn step_trail<'a, M: 'a>(paint: &Paint, steps: Vec<Step>) -> Element<'a, M> {
    let paint = *paint;
    let mut line = row![].align_y(Alignment::Start);
    let total = steps.len();
    for (index, step) in steps.into_iter().enumerate() {
        let (fill, ink, label) = match step.state {
            StepState::Pending => (paint.fill_1(), paint.fg_3(), paint.fg_3()),
            StepState::Current => (paint.acc_bg(), paint.acc_fg(), paint.acc_text()),
            StepState::Done => (paint.ok(), paint.acc_fg(), paint.fg_2()),
            StepState::Failed => (paint.danger(), paint.acc_fg(), paint.danger()),
        };
        let mark = if step.state == StepState::Done { None } else { Some(step.mark) };
        let ring = step.state == StepState::Current;
        let disc =
            container(surface(StepGlyph { color: ink, mark }).width(18).height(18)).center(40).style(move |_| {
                let mut style = style::rounded(fill, 20.0);
                if ring {
                    style.shadow = iced::Shadow { color: paint.acc_ring(), offset: Vector::ZERO, blur_radius: 6.0 };
                }
                style
            });
        let caption =
            text(step.label, TextStyle::CAP).color(label).align_x(Alignment::Center).width(Length::Fixed(76.0));
        line = line.push(column![disc, caption].spacing(SP1 + 2.0).align_x(Alignment::Center).width(76));
        if index + 1 < total {
            let done = step.state == StepState::Done;
            let joint = container(Space::new().width(14).height(2))
                .style(move |_| style::rounded(if done { paint.ok() } else { paint.fill_2() }, 1.0));
            line = line.push(container(joint).padding(iced::Padding::ZERO.top(19)));
        }
    }
    container(line).center_x(Length::Shrink).padding(iced::Padding::ZERO.top(SP2)).into()
}

#[must_use]
pub fn hold_bar<'a, M: 'a>(paint: &Paint, fraction: f32) -> Element<'a, M> {
    let paint = *paint;
    let filled = (180.0 * fraction.clamp(0.0, 1.0)).max(0.0);
    container(container(Space::new().width(filled).height(4)).style(move |_| style::rounded(paint.acc_bg(), 2.0)))
        .width(180)
        .height(4)
        .style(move |_| style::rounded(paint.fill_2(), 2.0))
        .into()
}

#[must_use]
pub fn scan_size() -> Size {
    Size::new(SCAN_SIDE + 2.0 * (CUE_SIDE + CUE_GAP), SCAN_SIDE)
}

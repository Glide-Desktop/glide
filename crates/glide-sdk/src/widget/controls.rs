use std::ops::RangeInclusive;
use std::rc::Rc;
use std::time::{Duration, Instant};

use glide_iced::Paint;
use glide_iced::style::to_iced;
use glide_theme::motion;
use glide_theme::shape::{R1, R2};
use iced::keyboard::key::Named;
use iced::widget::canvas::{self, Action, Frame, Geometry, LineCap, Path, Stroke};
use iced::widget::{canvas as surface, mouse_area};
use iced::{Color, Element, Event, Point, Radians, Rectangle, Renderer, Size, Theme, Vector, mouse, window};

use super::focus::{activatable, focusable};
use super::mix;
use crate::anim::{Motion, Speed, onscreen, phase};

#[derive(Debug, Clone, Copy)]
pub struct Pointer {
    pub hover: Motion,
    pub press: Motion,
    over: bool,
    pressed: bool,
}

impl Default for Pointer {
    fn default() -> Self {
        Self { hover: Motion::at(0.0), press: Motion::at(0.0), over: false, pressed: false }
    }
}

impl Pointer {
    pub fn track(&mut self, event: &Event, bounds: Rectangle, cursor: mouse::Cursor, enabled: bool) -> bool {
        let over = enabled && cursor.is_over(bounds);
        let mut changed = over != self.over;
        self.over = over;
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                self.pressed = true;
                self.press.jump(1.0);
                changed = true;
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Window(window::Event::Unfocused) => {
                self.pressed = false;
            }
            _ => {}
        }
        changed |= self.hover.to(if over { 1.0 } else { 0.0 }, Speed::STATE);
        changed |= self.press.to(if self.pressed { 1.0 } else { 0.0 }, Speed::PRESS);
        changed
    }

    #[must_use]
    pub fn animating(&self) -> bool {
        self.hover.animating(Speed::STATE) || self.press.animating(Speed::PRESS)
    }

    #[must_use]
    pub fn hover(&self) -> f32 {
        self.hover.value(Speed::STATE)
    }

    #[must_use]
    pub fn press(&self) -> f32 {
        self.press.value(Speed::PRESS)
    }

    #[must_use]
    pub fn layer(&self) -> f32 {
        motion::ST_HOVER * self.hover() + (motion::ST_PRESS - motion::ST_HOVER) * self.press()
    }
}

#[derive(Debug, Default)]
pub struct Sketch {
    cache: canvas::Cache,
    key: std::cell::Cell<Option<u64>>,
}

impl Sketch {
    fn draw(&self, renderer: &Renderer, size: Size, key: &[f32], draw: impl FnOnce(&mut Frame)) -> Vec<Geometry> {
        let key = fingerprint(key, size);
        if self.key.get() != Some(key) {
            self.cache.clear();
            self.key.set(Some(key));
        }
        vec![self.cache.draw(renderer, size, draw)]
    }
}

fn fingerprint(values: &[f32], size: Size) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    for value in values.iter().chain([size.width, size.height].iter()) {
        value.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

fn colour_bits(color: Color) -> [f32; 4] {
    [color.r, color.g, color.b, color.a]
}

#[derive(Debug)]
pub struct Toggle {
    pointer: Pointer,
    value: Motion,
    started: bool,
    sketch: Sketch,
}

impl Default for Toggle {
    fn default() -> Self {
        Self { pointer: Pointer::default(), value: Motion::at(0.0), started: false, sketch: Sketch::default() }
    }
}

impl Toggle {
    fn update<M>(
        &mut self, event: &Event, bounds: Rectangle, cursor: mouse::Cursor, target: f32, enabled: bool, speed: Speed,
    ) -> Option<Action<M>> {
        if !self.started {
            self.value.jump(target);
            self.started = true;
        }
        let mut changed = self.value.to(target, speed);
        changed |= self.pointer.track(event, bounds, cursor, enabled);
        let redraw = matches!(event, Event::Window(window::Event::RedrawRequested(_)))
            && (self.value.animating(speed) || self.pointer.animating());
        (changed || redraw).then(Action::request_redraw)
    }
}

fn shadowed_circle(frame: &mut Frame, center: Point, radius: f32, color: Color, shadow: f32) {
    if shadow > 0.0 {
        frame.fill(&Path::circle(center + Vector::new(0.0, 1.0), radius + 0.5), Color { a: shadow, ..Color::BLACK });
    }
    frame.fill(&Path::circle(center, radius), color);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchSize {
    Panel,
    Bar,
}

impl SwitchSize {
    const fn track(self) -> Size {
        match self {
            Self::Panel => Size::new(44.0, 26.0),
            Self::Bar => Size::new(28.0, 16.0),
        }
    }
}

struct Switch {
    paint: Paint,
    on: bool,
    enabled: bool,
    size: SwitchSize,
}

impl<M> canvas::Program<M> for Switch {
    type State = Toggle;

    fn update(&self, state: &mut Toggle, event: &Event, bounds: Rectangle, cursor: mouse::Cursor) -> Option<Action<M>> {
        state.update(event, bounds, cursor, if self.on { 1.0 } else { 0.0 }, self.enabled, Speed::OVERSHOOT)
    }

    fn draw(
        &self, state: &Toggle, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let paint = &self.paint;
        let thumb_on = state.value.value(Speed::OVERSHOOT);
        let color_on = state.value.value(Speed::COLOR);
        let size = self.size.track();
        let origin = Point::new((bounds.width - size.width) / 2.0, (bounds.height - size.height) / 2.0);
        let off = if self.enabled { paint.fill_3() } else { paint.fill_2() };
        let on = if self.enabled { paint.acc_bg() } else { paint.acc_bg().scale_alpha(0.42) };
        let track = paint.layer(mix(off, on, color_on), state.pointer.layer());
        let inset = (size.height - size.height * 0.77).max(2.0) / 2.0;
        let radius = size.height / 2.0 - inset;
        let travel = size.width - 2.0 * (inset + radius);
        let stretch = 2.0 * state.pointer.press();
        let x = origin.x + inset + radius + travel * thumb_on;
        let center = Point::new(x, origin.y + size.height / 2.0);
        let thumb = mix(to_iced(paint.tokens.switch_thumb()), Color::WHITE, color_on).scale_alpha(paint.alpha);
        let [tr, tg, tb, ta] = colour_bits(track);
        let [hr, hg, hb, ha] = colour_bits(thumb);
        let key = [thumb_on, stretch, size.width, size.height, tr, tg, tb, ta, hr, hg, hb, ha, paint.alpha];
        state.sketch.draw(renderer, bounds.size(), &key, |frame| {
            frame.fill(&Path::rounded_rectangle(origin, size, (size.height / 2.0).into()), track);
            if stretch > 0.01 {
                let width = 2.0 * radius + stretch;
                let left = (x - radius - stretch * thumb_on).max(origin.x + inset);
                frame.fill(
                    &Path::rounded_rectangle(
                        Point::new(left, center.y - radius),
                        Size::new(width, 2.0 * radius),
                        radius.into(),
                    ),
                    thumb,
                );
            } else {
                shadowed_circle(frame, center, radius, thumb, 0.18 * paint.alpha);
            }
        })
    }

    fn mouse_interaction(&self, _: &Toggle, bounds: Rectangle, cursor: mouse::Cursor) -> mouse::Interaction {
        if self.enabled && cursor.is_over(bounds) { mouse::Interaction::Pointer } else { mouse::Interaction::default() }
    }
}

fn clickable<'a, M: Clone + 'a>(element: Element<'a, M>, message: Option<M>) -> Element<'a, M> {
    match message {
        Some(message) => mouse_area(element).on_release(message).interaction(mouse::Interaction::Pointer).into(),
        None => element,
    }
}

#[must_use]
pub fn switch<'a, M: Clone + 'a>(
    paint: &Paint, on: bool, on_toggle: Option<impl Fn(bool) -> M + 'a>,
) -> Element<'a, M> {
    sized_switch(paint, on, SwitchSize::Panel, on_toggle)
}

#[must_use]
pub fn sized_switch<'a, M: Clone + 'a>(
    paint: &Paint, on: bool, size: SwitchSize, on_toggle: Option<impl Fn(bool) -> M + 'a>,
) -> Element<'a, M> {
    let track = size.track();
    let message = on_toggle.map(|toggle| toggle(!on));
    let canvas =
        surface(Switch { paint: *paint, on, enabled: message.is_some(), size }).width(track.width).height(track.height);
    activatable(paint, clickable(canvas.into(), message.clone()), track.height / 2.0, message)
}

pub(crate) fn switch_face<'a, M: 'a>(paint: &Paint, on: bool, enabled: bool) -> Element<'a, M> {
    let size = SwitchSize::Panel;
    let track = size.track();
    surface(Switch { paint: *paint, on, enabled, size }).width(track.width).height(track.height).into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    Off,
    On,
    Mixed,
}

struct CheckBox {
    paint: Paint,
    check: Check,
    enabled: bool,
    round: bool,
}

pub const CHECK: f32 = 20.0;

impl<M> canvas::Program<M> for CheckBox {
    type State = Toggle;

    fn update(&self, state: &mut Toggle, event: &Event, bounds: Rectangle, cursor: mouse::Cursor) -> Option<Action<M>> {
        let target = if self.check == Check::Off { 0.0 } else { 1.0 };
        state.update(event, bounds, cursor, target, self.enabled, Speed::OVERSHOOT)
    }

    fn draw(
        &self, state: &Toggle, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let paint = &self.paint;
        let pop = state.value.value(Speed::OVERSHOOT);
        let fill = state.value.value(Speed::COLOR);
        let layer = state.pointer.layer();
        let check = match self.check {
            Check::Off => 0.0,
            Check::Mixed => 2.0,
            Check::On => 1.0,
        };
        let flags = [f32::from(u8::from(self.round)), f32::from(u8::from(self.enabled)), check];
        let colours =
            [paint.acc_bg(), paint.view_bg(), paint.fill_3(), paint.line_2(), paint.acc_fg()].map(colour_bits);
        let mut key = vec![pop, fill, layer, paint.alpha];
        key.extend(flags);
        key.extend(colours.iter().flatten());
        state.sketch.draw(renderer, bounds.size(), &key, |frame| self.paint_check(frame, bounds, (pop, fill, layer)))
    }

    fn mouse_interaction(&self, _: &Toggle, bounds: Rectangle, cursor: mouse::Cursor) -> mouse::Interaction {
        if self.enabled && cursor.is_over(bounds) { mouse::Interaction::Pointer } else { mouse::Interaction::default() }
    }
}

impl CheckBox {
    fn paint_check(&self, frame: &mut Frame, bounds: Rectangle, (pop, fill, layer): (f32, f32, f32)) {
        let paint = &self.paint;
        let side = CHECK.min(bounds.width).min(bounds.height);
        let origin = Point::new((bounds.width - side) / 2.0, (bounds.height - side) / 2.0);
        let radius = if self.round { side / 2.0 } else { R2 / 2.0 + 1.0 };
        let off = paint.layer(paint.view_bg(), layer);
        let on = if self.enabled { paint.acc_bg() } else { paint.fill_3() };
        let on = paint.layer(on, layer);
        let outer = Path::rounded_rectangle(origin, Size::new(side, side), radius.into());
        if self.round {
            frame.fill(&outer, mix(off, on, fill));
            let inner = Path::rounded_rectangle(
                origin + Vector::new(1.0, 1.0),
                Size::new(side - 2.0, side - 2.0),
                (radius - 1.0).into(),
            );
            frame.fill(&inner, mix(off, paint.view_bg(), fill));
            let dot = 4.5 * pop;
            if dot > 0.1 {
                frame.fill(&Path::circle(Point::new(origin.x + side / 2.0, origin.y + side / 2.0), dot), on);
            }
            let ring = Stroke::default().with_color(paint.line_2().scale_alpha(1.0 - fill)).with_width(1.0);
            frame.stroke(
                &Path::rounded_rectangle(
                    origin + Vector::new(0.5, 0.5),
                    Size::new(side - 1.0, side - 1.0),
                    (radius - 0.5).into(),
                ),
                ring,
            );
            return;
        }
        frame.fill(&outer, mix(off, on, fill));
        let ring = Stroke::default().with_color(paint.line_2().scale_alpha(1.0 - fill)).with_width(1.0);
        frame.stroke(
            &Path::rounded_rectangle(
                origin + Vector::new(0.5, 0.5),
                Size::new(side - 1.0, side - 1.0),
                (radius - 0.5).into(),
            ),
            ring,
        );
        if pop > 0.01 {
            let center = Point::new(origin.x + side / 2.0, origin.y + side / 2.0);
            frame.with_save(|frame| {
                frame.translate(Vector::new(center.x, center.y));
                frame.scale(pop.max(0.0));
                let mark = match self.check {
                    Check::Mixed => Path::line(Point::new(-4.5, 0.0), Point::new(4.5, 0.0)),
                    _ => Path::new(|path| {
                        path.move_to(Point::new(-4.5, 0.2));
                        path.line_to(Point::new(-1.4, 3.3));
                        path.line_to(Point::new(4.6, -3.4));
                    }),
                };
                let stroke = Stroke::default()
                    .with_color(paint.acc_fg())
                    .with_width(2.0)
                    .with_line_cap(LineCap::Round)
                    .with_line_join(canvas::LineJoin::Round);
                frame.stroke(&mark, stroke);
            });
        }
    }
}

#[must_use]
pub fn check_box<'a, M: Clone + 'a>(paint: &Paint, check: Check, message: Option<M>) -> Element<'a, M> {
    let enabled = message.is_some();
    activatable(paint, clickable(mark(paint, check, false, enabled), message.clone()), R2 / 2.0 + 1.0, message)
}

#[must_use]
pub fn radio_mark<'a, M: Clone + 'a>(paint: &Paint, selected: bool, message: Option<M>) -> Element<'a, M> {
    let enabled = message.is_some();
    let check = if selected { Check::On } else { Check::Off };
    activatable(paint, clickable(mark(paint, check, true, enabled), message.clone()), CHECK / 2.0, message)
}

#[must_use]
pub fn radio_indicator<'a, M: 'a>(paint: &Paint, selected: bool) -> Element<'a, M> {
    mark(paint, if selected { Check::On } else { Check::Off }, true, true)
}

pub(crate) fn mark<'a, M: 'a>(paint: &Paint, check: Check, round: bool, enabled: bool) -> Element<'a, M> {
    surface(CheckBox { paint: *paint, check, enabled, round }).width(CHECK).height(CHECK).into()
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Drag {
    pointer: Pointer,
    value: Option<Motion>,
    dragging: bool,
}

struct Slider<'a, M> {
    paint: Paint,
    range: RangeInclusive<f32>,
    value: f32,
    step: f32,
    on_change: Option<Rc<dyn Fn(f32) -> M + 'a>>,
    on_release: Option<M>,
}

fn nudge(range: &RangeInclusive<f32>, value: f32, step: f32, key: Named) -> Option<f32> {
    let (start, end) = (*range.start(), *range.end());
    let unit = if step > 0.0 { step } else { (end - start) / 20.0 };
    let next = match key {
        Named::ArrowLeft | Named::ArrowDown => value - unit,
        Named::ArrowRight | Named::ArrowUp => value + unit,
        Named::PageDown => value - 5.0 * unit,
        Named::PageUp => value + 5.0 * unit,
        Named::Home => start,
        Named::End => end,
        _ => return None,
    }
    .clamp(start, end);
    (next != value).then_some(next)
}

const THUMB: f32 = 18.0;
const SLIDER_HEIGHT: f32 = THUMB + 12.0;

impl<M> Slider<'_, M> {
    fn fraction(&self, value: f32) -> f32 {
        let (start, end) = (*self.range.start(), *self.range.end());
        if end <= start { 0.0 } else { ((value - start) / (end - start)).clamp(0.0, 1.0) }
    }

    fn value_at(&self, bounds: Rectangle, x: f32) -> f32 {
        let (start, end) = (*self.range.start(), *self.range.end());
        let fraction = ((x - bounds.x - THUMB / 2.0) / (bounds.width - THUMB).max(1.0)).clamp(0.0, 1.0);
        let value = start + fraction * (end - start);
        if self.step > 0.0 {
            ((value - start) / self.step).round().mul_add(self.step, start).clamp(start, end)
        } else {
            value
        }
    }
}

impl<M: Clone> canvas::Program<M> for Slider<'_, M> {
    type State = Drag;

    fn update(&self, state: &mut Drag, event: &Event, bounds: Rectangle, cursor: mouse::Cursor) -> Option<Action<M>> {
        let enabled = self.on_change.is_some();
        let target = self.fraction(self.value);
        let motion = state.value.get_or_insert_with(|| Motion::at(target));
        let mut changed = if state.dragging {
            motion.jump(target);
            false
        } else {
            motion.to(target, Speed::SLIDE)
        };
        changed |= state.pointer.track(event, bounds, cursor, enabled);
        let publish = |x: f32| self.on_change.as_ref().map(|change| change(self.value_at(bounds, x)));
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if enabled && cursor.is_over(bounds) => {
                state.dragging = true;
                let message = cursor.position().and_then(|point| publish(point.x));
                return Some(message.map_or_else(Action::request_redraw, Action::publish).and_capture());
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.dragging => {
                return publish(position.x).map(|message| Action::publish(message).and_capture());
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging => {
                state.dragging = false;
                return Some(self.on_release.clone().map_or_else(Action::request_redraw, Action::publish));
            }
            _ => {}
        }
        let animating = state.pointer.animating() || state.value.is_some_and(|motion| motion.animating(Speed::SLIDE));
        let redraw = matches!(event, Event::Window(window::Event::RedrawRequested(_))) && animating;
        (changed || redraw).then(Action::request_redraw)
    }

    fn draw(&self, state: &Drag, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let paint = &self.paint;
        let fraction = state.value.map_or_else(|| self.fraction(self.value), |motion| motion.value(Speed::SLIDE));
        let mut frame = Frame::new(renderer, bounds.size());
        let middle = bounds.height / 2.0;
        let rail = 6.0;
        let span = bounds.width - THUMB;
        let x = THUMB / 2.0 + span * fraction;
        frame.fill(
            &Path::rounded_rectangle(Point::new(0.0, middle - rail / 2.0), Size::new(bounds.width, rail), R1.into()),
            paint.fill_2(),
        );
        let fill = if self.on_change.is_some() { paint.acc_bg() } else { paint.fg_4() };
        frame.fill(&Path::rounded_rectangle(Point::new(0.0, middle - rail / 2.0), Size::new(x, rail), R1.into()), fill);
        let ring = 4.0 * state.pointer.hover() + 2.0 * if state.dragging { 1.0 } else { state.pointer.press() };
        let center = Point::new(x, middle);
        if ring > 0.05 {
            frame.fill(&Path::circle(center, THUMB / 2.0 + ring), paint.acc_ring());
        }
        shadowed_circle(&mut frame, center, THUMB / 2.0, fill, 0.18 * paint.alpha);
        frame.fill(&Path::circle(center, THUMB / 2.0 - 4.0), Color::WHITE.scale_alpha(paint.alpha));
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, state: &Drag, bounds: Rectangle, cursor: mouse::Cursor) -> mouse::Interaction {
        if state.dragging {
            mouse::Interaction::Grabbing
        } else if self.on_change.is_some() && cursor.is_over(bounds) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }
}

#[must_use]
pub fn slider<'a, M: Clone + 'a>(
    paint: &Paint, range: RangeInclusive<f32>, value: f32, on_change: impl Fn(f32) -> M + 'a,
) -> Element<'a, M> {
    stepped_slider(paint, range, value, 0.0, Some(on_change), None)
}

#[must_use]
pub fn stepped_slider<'a, M: Clone + 'a>(
    paint: &Paint, range: RangeInclusive<f32>, value: f32, step: f32, on_change: Option<impl Fn(f32) -> M + 'a>,
    on_release: Option<M>,
) -> Element<'a, M> {
    let on_change = on_change.map(|change| Rc::new(change) as Rc<dyn Fn(f32) -> M + 'a>);
    let keys = on_change.clone().map(|change| {
        let (range, release) = (range.clone(), on_release.clone());
        move |key| {
            nudge(&range, value, step, key)
                .map(|next| std::iter::once(change(next)).chain(release.clone()).collect())
                .unwrap_or_default()
        }
    });
    let canvas = surface(Slider { paint: *paint, range, value, step, on_change, on_release })
        .width(iced::Fill)
        .height(SLIDER_HEIGHT);
    let focus = focusable(paint, canvas, SLIDER_HEIGHT / 2.0);
    match keys {
        Some(keys) => focus.on_key(keys).into(),
        None => focus.into(),
    }
}

pub const ZOOM_TRACK: f32 = 72.0;

#[must_use]
pub fn zoom_control<'a, M: Clone + 'a>(
    paint: &Paint, level: usize, levels: usize, on_change: impl Fn(usize) -> M + 'a, labels: (&'a str, &'a str),
) -> Element<'a, M> {
    use iced::widget::{container, row};
    let last = levels.saturating_sub(1);
    let level = level.min(last);
    let out = (level > 0).then(|| on_change(level - 1));
    let into = (level < last).then(|| on_change(level + 1));
    let pick = move |value: f32| on_change(value.round().clamp(0.0, last as f32) as usize);
    let track = stepped_slider(paint, 0.0..=last as f32, level as f32, 1.0, Some(pick), None);
    row![
        super::icon_button(paint, super::Icon::Minus, labels.0, out, false),
        container(track).width(ZOOM_TRACK),
        super::icon_button(paint, super::Icon::Plus, labels.1, into, false),
    ]
    .spacing(2)
    .align_y(iced::Alignment::Center)
    .into()
}

#[derive(Debug, Clone, Copy)]
pub struct Bar {
    value: Option<Motion>,
    started: Instant,
}

impl Default for Bar {
    fn default() -> Self {
        Self { value: None, started: Instant::now() }
    }
}

struct Progress {
    paint: Paint,
    fraction: Option<f32>,
    height: f32,
    paused: bool,
}

const SWEEP: Duration = Duration::from_millis(1400);

impl<M> canvas::Program<M> for Progress {
    type State = Bar;

    fn update(&self, state: &mut Bar, event: &Event, _: Rectangle, _: mouse::Cursor) -> Option<Action<M>> {
        let Some(fraction) = self.fraction else {
            let redraw = matches!(event, Event::Window(window::Event::RedrawRequested(_)))
                && !motion::is_disabled()
                && !self.paused;
            return redraw.then(Action::request_redraw);
        };
        let motion = state.value.get_or_insert_with(|| Motion::at(fraction));
        let changed = motion.to(fraction, Speed::SLIDE);
        let redraw =
            matches!(event, Event::Window(window::Event::RedrawRequested(_))) && motion.animating(Speed::SLIDE);
        (changed || redraw).then(Action::request_redraw)
    }

    fn draw(&self, state: &Bar, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let paint = &self.paint;
        let mut frame = Frame::new(renderer, bounds.size());
        let top = (bounds.height - self.height) / 2.0;
        let radius = (self.height / 2.0).min(R1);
        frame.fill(
            &Path::rounded_rectangle(Point::new(0.0, top), Size::new(bounds.width, self.height), radius.into()),
            paint.fill_2(),
        );
        let (start, width) = if let Some(fraction) = self.fraction {
            (0.0, bounds.width * state.value.map_or(fraction, |motion| motion.value(Speed::SLIDE)))
        } else if motion::is_disabled() || self.paused {
            (bounds.width * 0.33, bounds.width * 0.34)
        } else {
            let width = bounds.width * 0.34;
            let t = motion::ease_io(f64::from(phase(state.started, SWEEP))) as f32;
            (-width + (bounds.width + width) * t, width)
        };
        let left = start.max(0.0);
        let right = (start + width).min(bounds.width);
        if right > left {
            frame.fill(
                &Path::rounded_rectangle(Point::new(left, top), Size::new(right - left, self.height), radius.into()),
                if self.paused { paint.fg_3() } else { paint.acc_bg() },
            );
        }
        vec![frame.into_geometry()]
    }
}

#[must_use]
pub fn progress<'a, M: 'a>(paint: &Paint, fraction: Option<f32>) -> Element<'a, M> {
    bar(paint, fraction, false)
}

#[must_use]
pub fn paused_progress<'a, M: 'a>(paint: &Paint, fraction: Option<f32>, paused: bool) -> Element<'a, M> {
    bar(paint, fraction, paused)
}

fn bar<'a, M: 'a>(paint: &Paint, fraction: Option<f32>, paused: bool) -> Element<'a, M> {
    let fraction = fraction.map(|value| value.clamp(0.0, 1.0));
    onscreen(surface(Progress { paint: *paint, fraction, height: 6.0, paused }).width(iced::Fill).height(6))
}

struct Chevron {
    paint: Paint,
    open: bool,
    fold: bool,
}

impl<M> canvas::Program<M> for Chevron {
    type State = Toggle;

    fn update(&self, state: &mut Toggle, event: &Event, bounds: Rectangle, cursor: mouse::Cursor) -> Option<Action<M>> {
        state.update(event, bounds, cursor, if self.open { 1.0 } else { 0.0 }, false, Speed::SLIDE)
    }

    fn draw(
        &self, state: &Toggle, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let turn = state.value.value(Speed::SLIDE);
        let ink = self.paint.fg_3();
        let [r, g, b, a] = colour_bits(ink);
        state.sketch.draw(renderer, bounds.size(), &[turn, r, g, b, a], |frame| {
            frame.with_save(|frame| {
                frame.translate(Vector::new(bounds.width / 2.0, bounds.height / 2.0));
                let angle = if self.fold {
                    std::f32::consts::FRAC_PI_2 - std::f32::consts::PI * turn
                } else {
                    std::f32::consts::FRAC_PI_2 * turn
                };
                frame.rotate(Radians(angle));
                let mark = Path::new(|path| {
                    path.move_to(Point::new(-1.8, -4.0));
                    path.line_to(Point::new(2.2, 0.0));
                    path.line_to(Point::new(-1.8, 4.0));
                });
                let stroke = Stroke::default()
                    .with_color(ink)
                    .with_width(1.6)
                    .with_line_cap(LineCap::Round)
                    .with_line_join(canvas::LineJoin::Round);
                frame.stroke(&mark, stroke);
            });
        })
    }
}

#[must_use]
pub fn chevron<'a, M: 'a>(paint: &Paint, open: bool) -> Element<'a, M> {
    surface(Chevron { paint: *paint, open, fold: false }).width(16).height(16).into()
}

#[must_use]
pub fn fold_chevron<'a, M: 'a>(paint: &Paint, open: bool) -> Element<'a, M> {
    surface(Chevron { paint: *paint, open, fold: true }).width(16).height(16).into()
}

struct Spinner {
    paint: Paint,
}

const TURN: Duration = Duration::from_millis(900);

impl<M> canvas::Program<M> for Spinner {
    type State = Bar;

    fn update(&self, _: &mut Bar, event: &Event, _: Rectangle, _: mouse::Cursor) -> Option<Action<M>> {
        let redraw = matches!(event, Event::Window(window::Event::RedrawRequested(_))) && !motion::is_disabled();
        redraw.then(Action::request_redraw)
    }

    fn draw(&self, state: &Bar, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let paint = &self.paint;
        let mut frame = Frame::new(renderer, bounds.size());
        let side = bounds.width.min(bounds.height);
        let width = if side > 28.0 { 3.0 } else { 2.5 };
        let radius = side / 2.0 - width / 2.0;
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        frame.stroke(&Path::circle(center, radius), Stroke::default().with_color(paint.fill_2()).with_width(width));
        let start = std::f32::consts::TAU * phase(state.started, TURN) - std::f32::consts::FRAC_PI_2;
        let arc = Path::new(|path| {
            path.arc(canvas::path::Arc {
                center,
                radius,
                start_angle: Radians(start),
                end_angle: Radians(start + std::f32::consts::FRAC_PI_2 * 1.2),
            });
        });
        frame
            .stroke(&arc, Stroke::default().with_color(paint.acc_bg()).with_width(width).with_line_cap(LineCap::Round));
        vec![frame.into_geometry()]
    }
}

#[must_use]
pub fn spinner<'a, M: 'a>(paint: &Paint, large: bool) -> Element<'a, M> {
    sized_spinner(paint, if large { 34.0 } else { 22.0 })
}

#[must_use]
pub fn sized_spinner<'a, M: 'a>(paint: &Paint, side: f32) -> Element<'a, M> {
    onscreen(surface(Spinner { paint: *paint }).width(side).height(side))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sketch_tessellates_again_only_when_what_it_draws_changes() {
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0)));
        let sketch = Sketch::default();
        let drawn = std::cell::Cell::new(0);
        let size = Size::new(44.0, 26.0);
        let draw = |key: &[f32]| {
            let _ = sketch.draw(&renderer, size, key, |frame| {
                drawn.set(drawn.get() + 1);
                frame.fill(&Path::circle(Point::new(10.0, 10.0), 4.0), Color::WHITE);
            });
        };
        draw(&[0.0, 1.0]);
        draw(&[0.0, 1.0]);
        assert_eq!(drawn.get(), 1, "a window redraw for something else reuses the geometry");
        draw(&[0.5, 1.0]);
        assert_eq!(drawn.get(), 2);
    }

    fn slider(step: f32) -> Slider<'static, f32> {
        Slider {
            paint: Paint::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue),
            range: 0.0..=100.0,
            value: 50.0,
            step,
            on_change: Some(Rc::new(|value| value)),
            on_release: None,
        }
    }

    #[test]
    fn arrows_page_keys_and_home_end_move_a_slider_within_its_range() {
        let range = 0.0..=100.0;
        assert_eq!(nudge(&range, 50.0, 0.0, Named::ArrowRight), Some(55.0), "a free slider moves a twentieth");
        assert_eq!(nudge(&range, 50.0, 10.0, Named::ArrowDown), Some(40.0));
        assert_eq!(nudge(&range, 50.0, 10.0, Named::PageUp), Some(100.0));
        assert_eq!(nudge(&range, 95.0, 10.0, Named::ArrowUp), Some(100.0), "the end clamps");
        assert_eq!(nudge(&range, 100.0, 10.0, Named::End), None, "nothing changes at the end");
        assert_eq!(nudge(&range, 30.0, 10.0, Named::Home), Some(0.0));
        assert_eq!(nudge(&range, 30.0, 10.0, Named::Tab), None);
    }

    #[test]
    fn a_slider_maps_the_pointer_onto_its_range_and_steps() {
        let bounds = Rectangle::new(Point::new(10.0, 0.0), Size::new(218.0, 30.0));
        let free = slider(0.0);
        assert_eq!(free.value_at(bounds, 0.0), 0.0, "left of the track clamps");
        assert_eq!(free.value_at(bounds, 1000.0), 100.0, "right of the track clamps");
        assert!((free.value_at(bounds, 10.0 + THUMB / 2.0 + 100.0) - 50.0).abs() < 0.01);
        assert_eq!(slider(25.0).value_at(bounds, 10.0 + THUMB / 2.0 + 70.0), 25.0);
        assert_eq!(free.fraction(150.0), 1.0);
    }

    #[test]
    fn a_switch_thumb_travels_with_overshoot_and_settles() {
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(44.0, 26.0));
        let redraw = Event::Window(window::Event::RedrawRequested(Instant::now()));
        let mut toggle = Toggle::default();
        let first: Option<Action<()>> =
            toggle.update(&redraw, bounds, mouse::Cursor::Unavailable, 0.0, true, Speed::OVERSHOOT);
        assert!(first.is_none(), "the first frame starts at rest");
        let flipped: Option<Action<()>> =
            toggle.update(&redraw, bounds, mouse::Cursor::Unavailable, 1.0, true, Speed::OVERSHOOT);
        assert!(flipped.is_some(), "flipping asks for frames");
        std::thread::sleep((Speed::OVERSHOOT.duration)() / 2);
        let middle = toggle.value.value(Speed::OVERSHOOT);
        assert!(middle > 0.3 && middle < 1.35, "mid-flight thumb position {middle}");
        std::thread::sleep((Speed::OVERSHOOT.duration)());
        assert_eq!(toggle.value.value(Speed::OVERSHOOT), 1.0);
        let settled: Option<Action<()>> =
            toggle.update(&redraw, bounds, mouse::Cursor::Unavailable, 1.0, true, Speed::OVERSHOOT);
        assert!(settled.is_none(), "a settled switch stops asking for frames");
    }

    #[test]
    fn a_pointer_reports_pressed_only_after_a_press_over_it() {
        let mut pointer = Pointer::default();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(20.0, 20.0));
        let inside = mouse::Cursor::Available(Point::new(5.0, 5.0));
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        assert!(
            pointer.track(&press, bounds, mouse::Cursor::Available(Point::new(50.0, 5.0)), true) || !pointer.pressed
        );
        assert!(!pointer.pressed);
        pointer.track(&press, bounds, inside, true);
        assert!(pointer.pressed);
        pointer.track(&Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)), bounds, inside, true);
        assert!(!pointer.pressed);
        let mut disabled = Pointer::default();
        disabled.track(&press, bounds, inside, false);
        assert!(!disabled.pressed, "a disabled control never presses");
    }
}

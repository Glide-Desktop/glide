use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use glide_theme::motion;
pub use glide_ui::Reveal;
use glide_ui::Tween;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{Operation, Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell};
use iced::{Color, Element, Event, Length, Rectangle, Renderer, Size, Theme, Transformation, Vector, mouse, window};

#[derive(Debug, Clone, Copy)]
pub struct Speed {
    pub duration: fn() -> Duration,
    pub ease: fn(f64) -> f64,
}

impl Speed {
    pub const STATE: Self = Self { duration: motion::t1, ease: motion::ease_io };
    pub const PRESS: Self = Self { duration: motion::t1, ease: motion::ease_out };
    pub const COLOR: Self = Self { duration: motion::t2, ease: motion::ease_out };
    pub const SLIDE: Self = Self { duration: motion::t2, ease: motion::ease_out };
    pub const OVERSHOOT: Self = Self { duration: motion::t2, ease: motion::ease_over };
    pub const EXPAND: Self = Self { duration: motion::t3, ease: motion::ease_out };
    pub const GROW: Self = Self { duration: motion::t3, ease: motion::ease_over };
    pub const FADE: Self = Self { duration: motion::t2, ease: motion::ease_io };
    pub const ARRIVE: Self = Self { duration: motion::t4, ease: motion::ease_out };
    pub const SHAKE: Self = Self { duration: motion::t3, ease: linear };
}

const fn linear(progress: f64) -> f64 {
    progress
}

#[must_use]
pub fn shake_offset(progress: f32) -> f32 {
    if progress <= 0.0 || progress >= 1.0 {
        return 0.0;
    }
    let settle = motion::ease_out(f64::from(progress)) as f32;
    -8.0 * (progress * std::f32::consts::TAU * 2.0).sin() * (1.0 - settle)
}

#[must_use]
pub fn shake<'a, M: 'a>(count: u32, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let content = content.into();
    animate(count as f32, Speed::SHAKE, move |_| content)
        .offset(|value| Vector::new(shake_offset(value.fract()), 0.0))
        .into()
}

#[derive(Debug, Clone, Copy)]
pub struct Fade {
    target: bool,
    from: f32,
    start: Option<Instant>,
    enter: Speed,
    leave: Speed,
}

impl Fade {
    #[must_use]
    pub fn new(on: bool, speed: Speed) -> Self {
        Self::asymmetric(on, speed, speed)
    }

    #[must_use]
    pub fn asymmetric(on: bool, enter: Speed, leave: Speed) -> Self {
        Self { target: on, from: if on { 1.0 } else { 0.0 }, start: None, enter, leave }
    }

    pub fn set(&mut self, on: bool, now: Instant) {
        if self.target == on {
            return;
        }
        self.from = self.progress(now);
        self.target = on;
        self.start = Some(now);
    }

    #[must_use]
    pub fn target(&self) -> bool {
        self.target
    }

    fn speed(&self) -> Speed {
        if self.target { self.enter } else { self.leave }
    }

    fn elapsed(&self, now: Instant) -> Option<f64> {
        let start = self.start?;
        let duration = (self.speed().duration)();
        if motion::is_disabled() || duration.is_zero() {
            return None;
        }
        let elapsed = now.saturating_duration_since(start).as_secs_f64() / duration.as_secs_f64();
        (elapsed < 1.0).then_some(elapsed)
    }

    #[must_use]
    pub fn progress(&self, now: Instant) -> f32 {
        let to = if self.target { 1.0 } else { 0.0 };
        match self.elapsed(now) {
            Some(elapsed) => self.from + (to - self.from) * (self.speed().ease)(elapsed) as f32,
            None => to,
        }
    }

    #[must_use]
    pub fn is_animating(&self, now: Instant) -> bool {
        self.elapsed(now).is_some()
    }

    #[must_use]
    pub fn is_settled_off(&self, now: Instant) -> bool {
        !self.target && !self.is_animating(now)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Motion(Tween);

impl Motion {
    #[must_use]
    pub fn at(value: f32) -> Self {
        Self(Tween::at(f64::from(value)))
    }

    pub fn to(&mut self, target: f32, speed: Speed) -> bool {
        self.0.retarget(f64::from(target), (speed.duration)(), speed.ease)
    }

    pub fn jump(&mut self, value: f32) {
        self.0.jump(f64::from(value));
    }

    #[must_use]
    pub fn value(&self, speed: Speed) -> f32 {
        self.0.value((speed.duration)(), speed.ease) as f32
    }

    #[must_use]
    pub fn target(&self) -> f32 {
        self.0.target() as f32
    }

    #[must_use]
    pub fn animating(&self, speed: Speed) -> bool {
        self.0.is_animating((speed.duration)())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Value(Rc<Cell<f32>>);

impl Value {
    #[must_use]
    pub fn get(&self) -> f32 {
        self.0.get()
    }

    fn set(&self, value: f32) {
        self.0.set(value);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub scale: f32,
    pub offset: Vector,
}

impl Default for Transform {
    fn default() -> Self {
        Self { scale: 1.0, offset: Vector::ZERO }
    }
}

type Shape<'a> = Box<dyn Fn(f32) -> Transform + 'a>;

pub struct Animate<'a, M> {
    content: Element<'a, M>,
    target: f32,
    hover: bool,
    from: Option<f32>,
    speed: Speed,
    value: Value,
    shape: Option<Shape<'a>>,
    key: Option<u64>,
    veil: Option<Color>,
}

#[derive(Debug, Clone, Copy)]
struct Beat {
    motion: Motion,
    key: Option<u64>,
}

#[must_use]
pub fn animate<'a, M: 'a, E: Into<Element<'a, M>>>(
    target: f32, speed: Speed, content: impl FnOnce(&Value) -> E,
) -> Animate<'a, M> {
    let value = Value::default();
    value.set(target);
    Animate {
        content: content(&value).into(),
        target,
        hover: false,
        from: None,
        speed,
        value,
        shape: None,
        key: None,
        veil: None,
    }
}

#[must_use]
pub fn hover<'a, M: 'a, E: Into<Element<'a, M>>>(speed: Speed, content: impl FnOnce(&Value) -> E) -> Animate<'a, M> {
    let mut animate = animate(0.0, speed, content);
    animate.hover = true;
    animate
}

impl<'a, M: 'a> Animate<'a, M> {
    #[must_use]
    pub fn from(mut self, start: f32) -> Self {
        self.from = Some(start);
        self.value.set(start);
        self
    }

    #[must_use]
    pub fn keyed(mut self, key: u64) -> Self {
        self.key = Some(key);
        self
    }

    #[must_use]
    pub fn veil(mut self, backdrop: Color) -> Self {
        self.veil = Some(backdrop);
        self
    }

    #[must_use]
    pub fn transform(mut self, shape: impl Fn(f32) -> Transform + 'a) -> Self {
        self.shape = Some(Box::new(shape));
        self
    }

    #[must_use]
    pub fn scale(self, shape: impl Fn(f32) -> f32 + 'a) -> Self {
        self.transform(move |value| Transform { scale: shape(value), offset: Vector::ZERO })
    }

    #[must_use]
    pub fn offset(self, shape: impl Fn(f32) -> Vector + 'a) -> Self {
        self.transform(move |value| Transform { scale: 1.0, offset: shape(value) })
    }
}

impl<M> Widget<M, Theme, Renderer> for Animate<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Beat>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Beat { motion: Motion::at(self.from.unwrap_or(self.target)), key: self.key })
    }

    forward!(content: children, diff, size, size_hint, layout, operate, mouse_interaction, overlay<M>);

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, viewport: &Rectangle,
    ) {
        if self.hover {
            self.target = if cursor.is_over(layout.bounds()) { 1.0 } else { 0.0 };
        }
        let beat = tree.state.downcast_mut::<Beat>();
        if beat.key != self.key {
            beat.key = self.key;
            beat.motion.jump(self.from.unwrap_or(self.target));
        }
        let motion = &mut beat.motion;
        let changed = motion.to(self.target, self.speed);
        self.value.set(motion.value(self.speed));
        if changed
            || (matches!(event, Event::Window(window::Event::RedrawRequested(_))) && motion.animating(self.speed))
        {
            shell.request_redraw();
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>,
        cursor: mouse::Cursor, viewport: &Rectangle,
    ) {
        let value = tree.state.downcast_ref::<Beat>().motion.value(self.speed);
        self.value.set(value);
        let child = &tree.children[0];
        match &self.shape {
            None => self.content.as_widget().draw(child, renderer, theme, style, layout, cursor, viewport),
            Some(shape) => {
                let Transform { scale, offset } = shape(value);
                let center = layout.bounds().center();
                let transform = Transformation::translate(center.x + offset.x, center.y + offset.y)
                    * Transformation::scale(scale)
                    * Transformation::translate(-center.x, -center.y);
                renderer.with_transformation(transform, |renderer| {
                    self.content.as_widget().draw(child, renderer, theme, style, layout, cursor, viewport);
                });
            }
        }
        if let Some(backdrop) = self.veil.filter(|_| value < 1.0) {
            renderer.fill_quad(
                renderer::Quad { bounds: layout.bounds(), ..renderer::Quad::default() },
                backdrop.scale_alpha((1.0 - value).clamp(0.0, 1.0)),
            );
        }
    }
}

impl<'a, M: 'a> From<Animate<'a, M>> for Element<'a, M> {
    fn from(animate: Animate<'a, M>) -> Self {
        Element::new(animate)
    }
}

#[derive(Debug)]
pub struct Lingering<T> {
    held: RefCell<Option<(T, Option<Instant>)>>,
}

impl<T> Default for Lingering<T> {
    fn default() -> Self {
        Self { held: RefCell::new(None) }
    }
}

impl<T: Clone> Lingering<T> {
    pub fn show(&self, current: Option<&T>) -> Option<(T, bool)> {
        let mut held = self.held.borrow_mut();
        if let Some(value) = current {
            *held = Some((value.clone(), None));
            return Some((value.clone(), true));
        }
        let (value, since) = held.as_mut()?;
        let since = *since.get_or_insert_with(Instant::now);
        if since.elapsed() >= motion::t3() {
            *held = None;
            return None;
        }
        Some((value.clone(), false))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Bottom,
    Right,
    Left,
}

pub struct Collapse<'a, M> {
    content: Element<'a, M>,
    open: bool,
    edge: Edge,
    from_closed: bool,
}

#[must_use]
pub fn collapse<'a, M: 'a>(open: bool, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    Element::new(Collapse { content: content.into(), open, edge: Edge::Bottom, from_closed: false })
}

#[must_use]
pub fn grow<'a, M: 'a>(open: bool, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    Element::new(Collapse { content: content.into(), open, edge: Edge::Bottom, from_closed: true })
}

#[must_use]
pub fn grow_x<'a, M: 'a>(open: bool, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    Element::new(Collapse { content: content.into(), open, edge: Edge::Right, from_closed: true })
}

#[must_use]
pub fn slide_in_end<'a, M: 'a>(open: bool, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    Element::new(Collapse { content: content.into(), open, edge: Edge::Left, from_closed: false })
}

impl<M> Widget<M, Theme, Renderer> for Collapse<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Motion>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Motion::at(if self.open && !self.from_closed { 1.0 } else { 0.0 }))
    }

    forward!(content: children, diff);

    fn size(&self) -> Size<Length> {
        let size = self.content.as_widget().size();
        if self.edge == Edge::Bottom {
            Size::new(size.width, Length::Shrink)
        } else {
            Size::new(Length::Shrink, size.height)
        }
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let value = tree.state.downcast_ref::<Motion>().value(Speed::EXPAND);
        let sideways = self.edge != Edge::Bottom;
        let room = if sideways {
            layout::Limits::new(Size::new(0.0, limits.min().height), Size::new(f32::INFINITY, limits.max().height))
        } else {
            layout::Limits::new(Size::new(limits.min().width, 0.0), Size::new(limits.max().width, f32::INFINITY))
        };
        let child = self.content.as_widget_mut().layout(&mut tree.children[0], renderer, &room);
        let size = child.size();
        let shown = if sideways {
            Size::new((size.width * value).ceil(), size.height)
        } else {
            Size::new(size.width, (size.height * value).ceil())
        };
        let child = if self.edge == Edge::Left {
            child.move_to(iced::Point::new(shown.width - size.width, 0.0))
        } else {
            child
        };
        layout::Node::with_children(shown, vec![child])
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        if self.open || (layout.bounds().height > 0.0 && layout.bounds().width > 0.0) {
            let Some(child) = layout.children().next() else { return };
            self.content.as_widget_mut().operate(&mut tree.children[0], child, renderer, operation);
        }
    }

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, viewport: &Rectangle,
    ) {
        let motion = tree.state.downcast_mut::<Motion>();
        let changed = motion.to(if self.open { 1.0 } else { 0.0 }, Speed::EXPAND);
        let animating = motion.animating(Speed::EXPAND);
        if changed || (animating && matches!(event, Event::Window(window::Event::RedrawRequested(_)))) {
            shell.invalidate_layout();
            shell.request_redraw();
        }
        if !self.open && !animating {
            return;
        }
        let Some(child) = layout.children().next() else { return };
        let cursor = if cursor.is_over(layout.bounds()) { cursor } else { mouse::Cursor::Unavailable };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>,
        cursor: mouse::Cursor, viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if bounds.height < 0.5 || bounds.width < 0.5 {
            return;
        }
        let Some(child) = layout.children().next() else { return };
        renderer.with_layer(bounds, |renderer| {
            self.content.as_widget().draw(&tree.children[0], renderer, theme, style, child, cursor, viewport);
        });
    }

    fn mouse_interaction(
        &self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &Renderer,
    ) -> mouse::Interaction {
        let Some(child) = layout.children().next().filter(|_| cursor.is_over(layout.bounds())) else {
            return mouse::Interaction::None;
        };
        self.content.as_widget().mouse_interaction(&tree.children[0], child, cursor, viewport, renderer)
    }

    fn overlay<'b>(
        &'b mut self, tree: &'b mut Tree, layout: Layout<'b>, renderer: &Renderer, viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, M, Theme, Renderer>> {
        let child = layout.children().next().filter(|_| self.open)?;
        self.content.as_widget_mut().overlay(&mut tree.children[0], child, renderer, viewport, translation)
    }
}

#[derive(Debug, Clone, Copy)]
struct Clock(Instant);

impl Default for Clock {
    fn default() -> Self {
        Self(Instant::now())
    }
}

pub struct Spin<'a, M> {
    content: Element<'a, M>,
    period: Duration,
    value: Value,
}

#[must_use]
pub fn spin<'a, M: 'a, E: Into<Element<'a, M>>>(period: Duration, content: impl FnOnce(&Value) -> E) -> Element<'a, M> {
    let value = Value::default();
    Element::new(Spin { content: content(&value).into(), period, value })
}

#[must_use]
pub fn phase(started: Instant, period: Duration) -> f32 {
    if motion::is_disabled() || period.is_zero() {
        return 0.0;
    }
    (started.elapsed().as_secs_f64() / period.as_secs_f64()).fract() as f32
}

impl<M> Widget<M, Theme, Renderer> for Spin<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Clock>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Clock::default())
    }

    forward!(content: children, diff, size, layout, operate, mouse_interaction, overlay<M>);

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, viewport: &Rectangle,
    ) {
        if matches!(event, Event::Window(window::Event::RedrawRequested(_))) && !motion::is_disabled() {
            shell.request_redraw();
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>,
        cursor: mouse::Cursor, viewport: &Rectangle,
    ) {
        self.value.set(phase(tree.state.downcast_ref::<Clock>().0, self.period));
        self.content.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lingering_value_outlives_its_source_until_the_exit_is_done() {
        let hold = Lingering::<String>::default();
        assert_eq!(hold.show(None), None);
        let text = String::from("find");
        assert_eq!(hold.show(Some(&text)), Some((text.clone(), true)));
        if motion::is_disabled() {
            assert_eq!(hold.show(None), None);
            return;
        }
        assert_eq!(hold.show(None), Some((text.clone(), false)));
        assert_eq!(hold.show(Some(&text)), Some((text, true)), "coming back reopens it");
    }

    #[test]
    fn a_fade_reverses_from_where_it_is_and_settles() {
        let start = Instant::now();
        let mut fade = Fade::asymmetric(false, Speed::ARRIVE, Speed::FADE);
        assert!(fade.progress(start) < f32::EPSILON && fade.is_settled_off(start));
        fade.set(true, start);
        let middle = start + (Speed::ARRIVE.duration)() / 2;
        let halfway = fade.progress(middle);
        if !motion::is_disabled() {
            assert!(halfway > 0.0 && halfway < 1.0 && fade.is_animating(middle));
        }
        fade.set(false, middle);
        assert!((fade.progress(middle) - halfway).abs() < 1e-4, "reversing keeps the current value");
        assert!(fade.is_settled_off(middle + Duration::from_secs(2)));
    }

    #[test]
    fn a_shake_swings_both_ways_and_comes_to_rest() {
        let samples: Vec<f32> = (0..=20).map(|step| shake_offset(step as f32 / 20.0)).collect();
        assert_eq!((samples[0], samples[20]), (0.0, 0.0));
        assert!(samples.iter().any(|offset| *offset > 0.5) && samples.iter().any(|offset| *offset < -2.0));
        assert!(samples.iter().all(|offset| offset.abs() <= 8.0));
        assert!(samples[17].abs() < samples[2].abs(), "the swing dies down");
    }

    #[test]
    fn a_collapse_unfolds_through_intermediate_heights() {
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0)));
        let content = || iced::widget::Space::new().width(100).height(120);
        let limits = layout::Limits::new(Size::ZERO, Size::new(400.0, 400.0));
        let mut closed: Element<'_, ()> = collapse(false, content());
        let mut tree = Tree::new(&closed);
        assert_eq!(closed.as_widget_mut().layout(&mut tree, &renderer, &limits).size().height, 0.0);
        let mut open: Element<'_, ()> = collapse(true, content());
        tree.diff(&open);
        let node = open.as_widget_mut().layout(&mut tree, &renderer, &limits);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        open.as_widget_mut().update(
            &mut tree,
            &Event::Window(window::Event::RedrawRequested(Instant::now())),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &Rectangle::with_size(Size::new(400.0, 400.0)),
        );
        assert!(shell.is_layout_invalid(), "every frame of the unfold asks for a new layout");
        std::thread::sleep((Speed::EXPAND.duration)() / 3);
        let middle = open.as_widget_mut().layout(&mut tree, &renderer, &limits).size().height;
        assert!(middle > 10.0 && middle < 120.0, "a third of the way through t3 it is part open: {middle}");
        std::thread::sleep((Speed::EXPAND.duration)());
        assert_eq!(open.as_widget_mut().layout(&mut tree, &renderer, &limits).size().height, 120.0);
    }

    #[test]
    fn a_zero_duration_settles_a_motion_at_once() {
        let still = Speed { duration: || Duration::ZERO, ease: motion::ease_over };
        let mut value = Motion::at(0.0);
        assert!(value.to(1.0, still));
        assert_eq!(value.value(still), 1.0);
        assert!(!value.animating(still));
        assert_eq!(phase(Instant::now(), Duration::ZERO), 0.0);
    }

    #[test]
    fn a_retarget_continues_from_the_drawn_value() {
        let slow = Speed { duration: || Duration::from_millis(400), ease: |t| t };
        let mut value = Motion::at(0.0);
        value.to(1.0, slow);
        std::thread::sleep(Duration::from_millis(100));
        let mid = value.value(slow);
        value.to(0.0, slow);
        assert!((value.value(slow) - mid).abs() < 0.05);
    }

    #[test]
    fn speeds_follow_the_spec_table() {
        assert_eq!((Speed::STATE.duration)(), motion::t1());
        assert_eq!((Speed::SLIDE.duration)(), motion::t2());
        assert_eq!((Speed::EXPAND.duration)(), motion::t3());
        assert!(motion::ease_over(0.6) > 1.0, "E_OVER overshoots before settling");
    }
}

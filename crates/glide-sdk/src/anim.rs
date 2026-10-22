use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use glide_theme::motion;
use glide_ui::Tween;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{Operation, Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, overlay};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Transformation, Vector, mouse, window};

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
    pub const SPRING: Self = Self { duration: motion::t2, ease: motion::ease_over };
    pub const EXPAND: Self = Self { duration: motion::t3, ease: motion::ease_out };
    pub const GROW: Self = Self { duration: motion::t3, ease: motion::ease_over };
    pub const FADE: Self = Self { duration: motion::t2, ease: motion::ease_io };
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
}

#[must_use]
pub fn animate<'a, M: 'a, E: Into<Element<'a, M>>>(
    target: f32, speed: Speed, content: impl FnOnce(&Value) -> E,
) -> Animate<'a, M> {
    let value = Value::default();
    value.set(target);
    Animate { content: content(&value).into(), target, hover: false, from: None, speed, value, shape: None }
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
        tree::Tag::of::<Motion>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Motion::at(self.from.unwrap_or(self.target)))
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.content]);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        self.content.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.content.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, viewport: &Rectangle,
    ) {
        if self.hover {
            self.target = if cursor.is_over(layout.bounds()) { 1.0 } else { 0.0 };
        }
        let motion = tree.state.downcast_mut::<Motion>();
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
        let value = tree.state.downcast_ref::<Motion>().value(self.speed);
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
    }

    fn mouse_interaction(
        &self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }

    fn overlay<'b>(
        &'b mut self, tree: &'b mut Tree, layout: Layout<'b>, renderer: &Renderer, viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(&mut tree.children[0], layout, renderer, viewport, translation)
    }
}

impl<'a, M: 'a> From<Animate<'a, M>> for Element<'a, M> {
    fn from(animate: Animate<'a, M>) -> Self {
        Element::new(animate)
    }
}

pub struct Collapse<'a, M> {
    content: Element<'a, M>,
    open: bool,
}

#[must_use]
pub fn collapse<'a, M: 'a>(open: bool, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    Element::new(Collapse { content: content.into(), open })
}

impl<M> Widget<M, Theme, Renderer> for Collapse<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Motion>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Motion::at(if self.open { 1.0 } else { 0.0 }))
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.content]);
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.content.as_widget().size().width, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let value = tree.state.downcast_ref::<Motion>().value(Speed::EXPAND);
        let child = self.content.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(Size::new(limits.min().width, 0.0), Size::new(limits.max().width, f32::INFINITY)),
        );
        let size = child.size();
        layout::Node::with_children(Size::new(size.width, (size.height * value).ceil()), vec![child])
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        if layout.bounds().height > 0.0 {
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
        if bounds.height < 0.5 {
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

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.content]);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        self.content.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
    }

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

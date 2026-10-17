use std::time::{Duration, Instant};

use glide_theme::motion::{self, E_OUT};
use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, mouse, overlay, renderer};
use iced::{Element, Event, Length, Rectangle, Size, Transformation, Vector, window};

pub const BUTTON: f32 = 0.975;

pub struct Squish<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    pressed: f32,
}

#[derive(Debug, Clone, Copy)]
struct State {
    held: bool,
    from: f32,
    since: Option<Instant>,
}

#[must_use]
pub fn eased(from: f32, to: f32, elapsed: Duration, span: Duration) -> f32 {
    let progress = if span.is_zero() { 1.0 } else { (elapsed.as_secs_f32() / span.as_secs_f32()).min(1.0) };
    from + (to - from) * E_OUT.ease(progress)
}

impl State {
    fn scale(&self, pressed: f32, now: Instant) -> f32 {
        let target = if self.held { pressed } else { 1.0 };
        match self.since {
            Some(since) => eased(self.from, target, now.duration_since(since), motion::t1()),
            None => target,
        }
    }

    fn settled(&self, now: Instant) -> bool {
        self.since.is_none_or(|since| now.duration_since(since) >= motion::t1())
    }

    fn hold(&mut self, held: bool, pressed: f32, now: Instant) {
        if self.held != held {
            self.from = self.scale(pressed, now);
            self.held = held;
            self.since = Some(now);
        }
    }
}

impl<'a, Message, Theme, Renderer> Squish<'a, Message, Theme, Renderer> {
    pub fn new(content: impl Into<Element<'a, Message, Theme, Renderer>>, pressed: f32) -> Self {
        Self { content: content.into(), pressed }
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Squish<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State { held: false, from: 1.0, since: None })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
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

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, viewport: &Rectangle,
    ) {
        let now = Instant::now();
        let live = cursor.is_over(layout.bounds())
            && self.content.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
                == mouse::Interaction::Pointer;
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if live => {
                state.hold(true, self.pressed, now);
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left) | mouse::Event::CursorLeft) => {
                if state.held {
                    state.hold(false, self.pressed, now);
                    shell.request_redraw();
                }
            }
            Event::Window(window::Event::RedrawRequested(at)) if !state.settled(*at) => shell.request_redraw(),
            _ => {}
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
        let scale = tree.state.downcast_ref::<State>().scale(self.pressed, Instant::now());
        let content = &tree.children[0];
        if (scale - 1.0).abs() < f32::EPSILON {
            self.content.as_widget().draw(content, renderer, theme, style, layout, cursor, viewport);
            return;
        }
        let center = layout.bounds().center();
        let transformation = Transformation::translate(center.x, center.y)
            * Transformation::scale(scale)
            * Transformation::translate(-center.x, -center.y);
        renderer.with_transformation(transformation, |renderer| {
            self.content.as_widget().draw(content, renderer, theme, style, layout, cursor, viewport);
        });
    }

    fn mouse_interaction(
        &self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }

    fn operate(
        &mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn widget::Operation,
    ) {
        self.content.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self, tree: &'b mut Tree, layout: Layout<'b>, renderer: &Renderer, viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(&mut tree.children[0], layout, renderer, viewport, translation)
    }
}

impl<'a, Message: 'a, Theme: 'a, Renderer> From<Squish<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(squish: Squish<'a, Message, Theme, Renderer>) -> Self {
        Self::new(squish)
    }
}

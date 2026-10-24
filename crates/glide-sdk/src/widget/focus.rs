use glide_iced::Paint;
use iced::advanced::layout::Layout;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{Id, Operation, Tree, Widget, operation, tree};
use iced::advanced::{Clipboard, Shell};
use iced::keyboard::{self, Key, key::Named};
use iced::{Border, Color, Element, Event, Rectangle, Renderer, Theme, mouse, window};

use crate::anim::{Motion, Speed};

pub const RING: f32 = 3.0;

type Keys<'a, M> = Box<dyn Fn(Named) -> Vec<M> + 'a>;

pub struct Focus<'a, M> {
    content: Element<'a, M>,
    paint: Paint,
    radius: f32,
    activate: Option<M>,
    keys: Option<Keys<'a, M>>,
    id: Option<Id>,
    inset: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Ring {
    pub(crate) focused: bool,
    shown: Motion,
}

impl Default for Ring {
    fn default() -> Self {
        Self { focused: false, shown: Motion::at(0.0) }
    }
}

impl Ring {
    fn value(&self) -> f32 {
        self.shown.value(Speed::STATE)
    }

    pub(crate) fn settle<M>(&mut self, event: &Event, shell: &mut Shell<'_, M>) {
        if let Event::Mouse(mouse::Event::ButtonPressed(_)) | Event::Window(window::Event::Unfocused) = event {
            self.focused = false;
        }
        let changed = self.shown.to(if self.focused { 1.0 } else { 0.0 }, Speed::STATE);
        if changed
            || (self.shown.animating(Speed::STATE) && matches!(event, Event::Window(window::Event::RedrawRequested(_))))
        {
            shell.request_redraw();
        }
    }

    pub(crate) fn draw(&self, renderer: &mut Renderer, paint: &Paint, bounds: Rectangle, radius: f32, inset: bool) {
        let shown = self.value();
        if shown <= 0.01 {
            return;
        }
        let (bounds, radius) = if inset { (bounds, radius) } else { (bounds.expand(RING), radius + RING) };
        renderer.fill_quad(
            Quad {
                bounds,
                border: Border { color: paint.acc_ring().scale_alpha(shown), width: RING, radius: radius.into() },
                ..Quad::default()
            },
            Color::TRANSPARENT,
        );
    }
}

impl operation::Focusable for Ring {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

pub(crate) fn plain(event: &Event) -> Option<Named> {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed { key: Key::Named(named), modifiers, .. })
            if !modifiers.control() && !modifiers.alt() && !modifiers.logo() =>
        {
            Some(*named)
        }
        _ => None,
    }
}

#[must_use]
pub fn focusable<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>, radius: f32) -> Focus<'a, M> {
    Focus { content: content.into(), paint: *paint, radius, activate: None, keys: None, id: None, inset: false }
}

pub(crate) fn activatable<'a, M: Clone + 'a>(
    paint: &Paint, content: impl Into<Element<'a, M>>, radius: f32, message: Option<M>,
) -> Element<'a, M> {
    focusable(paint, content, radius).on_activate(message).into()
}

impl<'a, M: 'a> Focus<'a, M> {
    #[must_use]
    pub fn inset(mut self) -> Self {
        self.inset = true;
        self
    }

    #[must_use]
    pub fn on_activate(mut self, message: Option<M>) -> Self {
        self.activate = message;
        self
    }

    #[must_use]
    pub fn on_key(mut self, keys: impl Fn(Named) -> Vec<M> + 'a) -> Self {
        self.keys = Some(Box::new(keys));
        self
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    fn enabled(&self) -> bool {
        self.activate.is_some() || self.keys.is_some()
    }
}

impl<'a, M: 'a> From<Focus<'a, M>> for Element<'a, M>
where
    M: Clone,
{
    fn from(focus: Focus<'a, M>) -> Self {
        Element::new(focus)
    }
}

impl<M: Clone> Widget<M, Theme, Renderer> for Focus<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Ring>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Ring::default())
    }

    forward!(content: children, diff, size, size_hint, layout, mouse_interaction, overlay<M>);

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        let ring = tree.state.downcast_mut::<Ring>();
        if self.enabled() {
            operation.focusable(self.id.as_ref(), layout.bounds(), ring);
        } else {
            ring.focused = false;
        }
        operation.traverse(&mut |operation| {
            self.content.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
        });
    }

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, viewport: &Rectangle,
    ) {
        let ring = tree.state.downcast_mut::<Ring>();
        if !self.enabled() {
            ring.focused = false;
        }
        if ring.focused
            && let Some(named) = plain(event)
        {
            let messages = match named {
                Named::Enter | Named::Space if self.activate.is_some() => self.activate.clone().into_iter().collect(),
                named => self.keys.as_ref().map(|keys| keys(named)).unwrap_or_default(),
            };
            if !messages.is_empty() {
                for message in messages {
                    shell.publish(message);
                }
                shell.capture_event();
                return;
            }
        }
        ring.settle(event, shell);
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
        self.content.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
        tree.state.downcast_ref::<Ring>().draw(renderer, &self.paint, layout.bounds(), self.radius, self.inset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Size;
    use iced::advanced::layout;
    use iced::advanced::widget::operation::focusable::focus_next;
    use iced::keyboard::{Location, Modifiers};
    use iced::widget::{Column, Space};

    fn renderer() -> Renderer {
        Renderer::Secondary(iced_tiny_skia::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0)))
    }

    fn key(named: Named) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(named),
            modified_key: Key::Named(named),
            physical_key: keyboard::key::Physical::Unidentified(keyboard::key::NativeCode::Unidentified),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
            text: None,
            repeat: false,
        })
    }

    fn send(element: &mut Element<'_, u8>, tree: &mut Tree, event: &Event) -> Vec<u8> {
        let renderer = renderer();
        let node =
            element.as_widget_mut().layout(tree, &renderer, &layout::Limits::new(Size::ZERO, Size::new(200.0, 200.0)));
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            tree,
            event,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &Rectangle::with_size(Size::new(200.0, 200.0)),
        );
        messages
    }

    fn tab(element: &mut Element<'_, u8>, tree: &mut Tree) {
        let renderer = renderer();
        let node =
            element.as_widget_mut().layout(tree, &renderer, &layout::Limits::new(Size::ZERO, Size::new(200.0, 200.0)));
        let mut operation: Box<dyn Operation> = Box::new(focus_next::<()>());
        loop {
            element.as_widget_mut().operate(tree, Layout::new(&node), &renderer, operation.as_mut());
            match operation.finish() {
                operation::Outcome::Chain(next) => operation = next,
                _ => break,
            }
        }
    }

    fn controls(paint: &Paint) -> Element<'static, u8> {
        let first = focusable(paint, Space::new().width(20).height(20), 4.0).on_activate(Some(1));
        let disabled = focusable::<u8>(paint, Space::new().width(20).height(20), 4.0).on_activate(None);
        let second = focusable(paint, Space::new().width(20).height(20), 4.0).on_key(|named| match named {
            Named::ArrowRight => vec![7, 8],
            _ => Vec::new(),
        });
        Column::with_children([first.into(), disabled.into(), second.into()]).into()
    }

    #[test]
    fn tab_walks_the_enabled_controls_and_keys_reach_only_the_focused_one() {
        let paint = Paint::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue);
        let mut element = controls(&paint);
        let mut tree = Tree::new(&element);
        assert!(send(&mut element, &mut tree, &key(Named::Enter)).is_empty(), "nothing is focused yet");
        tab(&mut element, &mut tree);
        assert_eq!(send(&mut element, &mut tree, &key(Named::Space)), vec![1]);
        assert!(send(&mut element, &mut tree, &key(Named::ArrowRight)).is_empty(), "the first control has no keys");
        tab(&mut element, &mut tree);
        assert_eq!(send(&mut element, &mut tree, &key(Named::ArrowRight)), vec![7, 8], "the disabled one is skipped");
        assert!(send(&mut element, &mut tree, &key(Named::Enter)).is_empty());
        send(&mut element, &mut tree, &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)));
        assert!(send(&mut element, &mut tree, &key(Named::ArrowRight)).is_empty(), "a click takes keyboard focus away");
    }

    #[test]
    fn tab_reaches_a_control_wrapped_in_a_tooltip_or_a_spinner() {
        let paint = Paint::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue);
        let control = || focusable(&paint, Space::new().width(20).height(20), 4.0).on_activate(Some(3));
        for mut element in [
            crate::widget::popover::tooltip(&paint, control(), "Hint"),
            crate::anim::spin(std::time::Duration::from_secs(1), |_| control()),
        ] {
            let mut tree = Tree::new(&element);
            tab(&mut element, &mut tree);
            assert_eq!(send(&mut element, &mut tree, &key(Named::Enter)), vec![3]);
        }
    }
}

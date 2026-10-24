use iced::advanced::widget::{Id, Operation, Tree, operation::Scrollable};
use iced::advanced::{Clipboard, Layout, Shell, Widget, mouse};
use iced::{Element, Event, Point, Rectangle, Task, Vector};

const PIXELS_PER_STEP: f32 = 60.0;

pub struct ContextArea<'a, Message> {
    content: Element<'a, Message>,
    on_right_press: Box<dyn Fn(Point) -> Message + 'a>,
    on_zoom: Option<Box<dyn Fn(i8) -> Message + 'a>>,
}

#[derive(Default)]
struct ZoomState {
    modifiers: iced::keyboard::Modifiers,
    pixels: f32,
}

pub fn context_area<'a, Message>(
    content: impl Into<Element<'a, Message>>, on_right_press: impl Fn(Point) -> Message + 'a,
) -> ContextArea<'a, Message> {
    ContextArea { content: content.into(), on_right_press: Box::new(on_right_press), on_zoom: None }
}

impl<'a, Message> ContextArea<'a, Message> {
    #[must_use]
    pub fn on_zoom(mut self, on_zoom: impl Fn(i8) -> Message + 'a) -> Self {
        self.on_zoom = Some(Box::new(on_zoom));
        self
    }
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for ContextArea<'_, Message> {
    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        iced::advanced::widget::tree::Tag::of::<ZoomState>()
    }

    fn state(&self) -> iced::advanced::widget::tree::State {
        iced::advanced::widget::tree::State::new(ZoomState::default())
    }

    forward!(content: children, diff, size, layout, operate, mouse_interaction, draw, overlay<Message>);

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor,
        renderer: &iced::Renderer, clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, viewport: &Rectangle,
    ) {
        let zoom = tree.state.downcast_mut::<ZoomState>();
        if let Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) = event {
            zoom.modifiers = *modifiers;
        }
        if let (Some(on_zoom), Event::Mouse(mouse::Event::WheelScrolled { delta })) = (&self.on_zoom, event)
            && zoom.modifiers.command()
            && cursor.is_over(layout.bounds())
        {
            let step = match delta {
                mouse::ScrollDelta::Lines { y, .. } => y.signum(),
                mouse::ScrollDelta::Pixels { y, .. } => {
                    zoom.pixels += y;
                    let whole = (zoom.pixels / PIXELS_PER_STEP).trunc();
                    zoom.pixels -= whole * PIXELS_PER_STEP;
                    whole.signum()
                }
            };
            if step != 0.0 {
                shell.publish(on_zoom(step as i8));
            }
            shell.capture_event();
            return;
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
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) = event
            && let Some(position) = cursor.position_over(layout.bounds())
        {
            shell.publish((self.on_right_press)(position));
        }
    }
}

impl<'a, Message: 'a> From<ContextArea<'a, Message>> for Element<'a, Message> {
    fn from(area: ContextArea<'a, Message>) -> Self {
        Element::new(area)
    }
}

struct Reveal {
    target: Id,
    top: f32,
    bottom: f32,
}

impl<T> Operation<T> for Reveal {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }

    fn scrollable(
        &mut self, id: Option<&Id>, bounds: Rectangle, _content_bounds: Rectangle, translation: Vector,
        state: &mut dyn Scrollable,
    ) {
        if id != Some(&self.target) {
            return;
        }
        let offset = translation.y;
        let target = if self.top < offset {
            self.top
        } else if self.bottom > offset + bounds.height {
            self.bottom - bounds.height
        } else {
            return;
        };
        state.scroll_to(iced::widget::scrollable::AbsoluteOffset { x: None, y: Some(target.max(0.0)) });
    }
}

pub fn reveal<T: Send + 'static>(target: Id, top: f32, bottom: f32) -> Task<T> {
    iced::advanced::widget::operate(Reveal { target, top, bottom })
}

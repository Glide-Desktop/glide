use std::time::{Duration, Instant};

use glide_theme::motion;
use iced::advanced::layout::Layout;
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell};
use iced::{Element, Event, Rectangle, Renderer, Theme, Vector, keyboard, mouse, window};

const LINE: f32 = 60.0;

pub struct SmoothWheel<'a, M> {
    content: Element<'a, M>,
}

#[must_use]
pub fn smooth_wheel<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    Element::new(SmoothWheel { content: content.into() })
}

#[derive(Debug, Clone, Copy)]
struct Run {
    started: Instant,
    total: Vector,
    sent: Vector,
}

#[derive(Debug, Default)]
struct State {
    modifiers: keyboard::Modifiers,
    run: Option<Run>,
}

fn span() -> Duration {
    motion::t2()
}

fn progress(started: Instant) -> f32 {
    let span = span();
    if span.is_zero() {
        return 1.0;
    }
    motion::ease_out((started.elapsed().as_secs_f64() / span.as_secs_f64()).clamp(0.0, 1.0)) as f32
}

fn notch(delta: mouse::ScrollDelta) -> Option<Vector> {
    match delta {
        mouse::ScrollDelta::Lines { x, y } if x.fract() == 0.0 && y.fract() == 0.0 => {
            Some(Vector::new(x * LINE, y * LINE))
        }
        _ => None,
    }
}

impl State {
    fn push(&mut self, step: Vector) {
        let rest = self.run.map_or(Vector::ZERO, |run| run.total - run.sent);
        let same = |old: f32, new: f32| if old * new > 0.0 { old } else { 0.0 };
        let carried = Vector::new(same(rest.x, step.x), same(rest.y, step.y));
        self.run = Some(Run { started: Instant::now(), total: carried + step, sent: Vector::ZERO });
    }

    fn advance(&mut self) -> Option<Vector> {
        let run = self.run.as_mut()?;
        let fraction = progress(run.started);
        let due = run.total * fraction;
        let delta = due - run.sent;
        run.sent = due;
        if fraction >= 1.0 {
            self.run = None;
        }
        (delta.x != 0.0 || delta.y != 0.0).then_some(delta)
    }
}

impl<M> Widget<M, Theme, Renderer> for SmoothWheel<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    forward!(content: children, diff, size, size_hint, layout, operate, draw, mouse_interaction, overlay<M>);

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
        }
        let smoothing = !span().is_zero() && !motion::is_disabled() && state.modifiers.is_empty();
        if smoothing
            && let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event
            && let Some(step) = notch(*delta)
        {
            state.push(step);
            shell.capture_event();
            shell.request_redraw();
            return;
        }
        if matches!(event, Event::Window(window::Event::RedrawRequested(_)))
            && let Some(delta) = state.advance()
        {
            let wheel = Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x: delta.x, y: delta.y },
            });
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                &wheel,
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
        if tree.state.downcast_ref::<State>().run.is_some() {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_notch_becomes_pixels_and_a_fine_wheel_passes_through() {
        assert_eq!(notch(mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 }), Some(Vector::new(0.0, -LINE)));
        assert_eq!(notch(mouse::ScrollDelta::Lines { x: 0.0, y: 0.4 }), None);
        assert_eq!(notch(mouse::ScrollDelta::Pixels { x: 0.0, y: 5.0 }), None);
    }

    #[test]
    fn a_run_delivers_exactly_the_notches_it_was_given() {
        let mut state = State::default();
        state.push(Vector::new(0.0, -LINE));
        let mut total = Vector::ZERO;
        let begin = Instant::now();
        while state.run.is_some() && begin.elapsed() < Duration::from_secs(2) {
            if let Some(delta) = state.advance() {
                total += delta;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(state.run.is_none());
        assert!((total.y + LINE).abs() < 0.01, "the animation lands on the full notch: {total:?}");
    }

    #[test]
    fn a_notch_in_the_other_direction_drops_the_unsent_rest() {
        let mut state = State::default();
        state.push(Vector::new(0.0, -LINE));
        state.push(Vector::new(0.0, LINE));
        let run = state.run.expect("a run is active");
        assert!((run.total.y - LINE).abs() < 0.01);
        state.push(Vector::new(0.0, LINE));
        let run = state.run.expect("a run is active");
        assert!((run.total.y - 2.0 * LINE).abs() < 0.01, "the same direction adds up");
    }
}

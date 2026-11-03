use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use iced::advanced::layout::Layout;
use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::{Element, Event, Point, Rectangle, Renderer, Theme, mouse};

#[derive(Debug, Clone, Default)]
pub struct PointerCell(Arc<AtomicU64>);

impl PointerCell {
    #[must_use]
    pub fn get(&self) -> Point {
        let bits = self.0.load(Ordering::Relaxed);
        let x = f32::from_bits((bits >> 32) as u32);
        let y = f32::from_bits((bits & 0xffff_ffff) as u32);
        Point::new(x, y)
    }

    pub fn set(&self, at: Point) {
        let bits = (u64::from(at.x.to_bits()) << 32) | u64::from(at.y.to_bits());
        self.0.store(bits, Ordering::Relaxed);
    }
}

pub(crate) struct Tracked<'a, M> {
    content: Element<'a, M>,
    cell: PointerCell,
}

pub(crate) fn tracked<'a, M: 'a>(cell: PointerCell, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    Element::new(Tracked { content: content.into(), cell })
}

impl<M> Widget<M, Theme, Renderer> for Tracked<'_, M> {
    forward!(content: children, diff, size, layout, operate, mouse_interaction, draw, overlay<M>);

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, viewport: &Rectangle,
    ) {
        if matches!(event, Event::Mouse(mouse::Event::CursorMoved { .. }))
            && let Some(at) = cursor.position_in(layout.bounds())
        {
            self.cell.set(at);
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

type Zone<'a> = Box<dyn Fn(Point) -> bool + 'a>;

pub struct Watched<'a, M> {
    content: Element<'a, M>,
    cell: PointerCell,
    zone: Zone<'a>,
    on_change: Box<dyn Fn(Point) -> M + 'a>,
}

#[must_use]
pub fn watched<'a, M: 'a>(
    cell: PointerCell, content: impl Into<Element<'a, M>>, zone: impl Fn(Point) -> bool + 'a,
    on_change: impl Fn(Point) -> M + 'a,
) -> Element<'a, M> {
    Element::new(Watched { content: content.into(), cell, zone: Box::new(zone), on_change: Box::new(on_change) })
}

#[derive(Debug, Default)]
struct Inside(Option<bool>);

impl<M> Widget<M, Theme, Renderer> for Watched<'_, M> {
    forward!(content: size, layout, operate, mouse_interaction, draw, overlay<M>);

    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        iced::advanced::widget::tree::Tag::of::<Inside>()
    }

    fn state(&self) -> iced::advanced::widget::tree::State {
        iced::advanced::widget::tree::State::new(Inside::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, viewport: &Rectangle,
    ) {
        if matches!(event, Event::Mouse(mouse::Event::CursorMoved { .. }))
            && let Some(at) = cursor.position_in(layout.bounds())
        {
            self.cell.set(at);
            let inside = (self.zone)(at);
            let seen = &mut tree.state.downcast_mut::<Inside>().0;
            if *seen != Some(inside) {
                *seen = Some(inside);
                shell.publish((self.on_change)(at));
            }
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
    fn a_pointer_cell_round_trips_fractional_positions() {
        let cell = PointerCell::default();
        assert_eq!(cell.get(), Point::ORIGIN);
        let at = Point::new(812.25, -3.5);
        let shared = cell.clone();
        shared.set(at);
        assert_eq!(cell.get(), at, "every clone shares the position");
    }
}

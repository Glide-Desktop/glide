use std::time::Instant;

use glide_iced::Paint;
use glide_iced::style::{self, R2, R3, SP1, SP2, SP3};
use glide_iced::widgets;
use glide_theme::elevation::Level;
use glide_theme::{TextStyle, component};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, overlay};
use iced::keyboard::{self, Key, key::Named};
use iced::widget::{Column, Space, container, row};
use iced::{
    Alignment, Element, Event, Fill, Length, Padding, Point, Rectangle, Renderer, Size, Theme, Transformation, Vector,
    mouse, window,
};

use super::{Icon, Wrapping, icon, text};
use crate::anim::{Motion, Speed};

const GAP: f32 = 4.0;
const MARGIN: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Below,
    Above,
}

fn place(anchor: Rectangle, content: Size, bounds: Size, gap: f32, centred: bool) -> (Point, Side) {
    let below = anchor.y + anchor.height + gap;
    let (y, placed) = if below + content.height <= bounds.height - MARGIN || anchor.y - gap - content.height < MARGIN {
        (below, Side::Below)
    } else {
        (anchor.y - gap - content.height, Side::Above)
    };
    let x = if centred { anchor.center_x() - content.width / 2.0 } else { anchor.x };
    let x = x.min(bounds.width - content.width - MARGIN).max(MARGIN);
    (Point::new(x, y.max(MARGIN)), placed)
}

fn rise(progress: f32, side: Side, bounds: Rectangle, closed_scale: f32) -> Transformation {
    let scale = closed_scale + (1.0 - closed_scale) * progress;
    let lift = (1.0 - progress) * component::POPOVER_RISE * if side == Side::Below { -1.0 } else { 1.0 };
    let pivot = Point::new(bounds.center_x(), if side == Side::Below { bounds.y } else { bounds.y + bounds.height });
    Transformation::translate(pivot.x, pivot.y + lift)
        * Transformation::scale(scale)
        * Transformation::translate(-pivot.x, -pivot.y)
}

struct Hint {
    shown_at: Option<Instant>,
    over: bool,
    suppressed: bool,
    reveal: Motion,
    tree: Tree,
}

const STEPS: f32 = 30.0;

fn step(progress: f32) -> u32 {
    (progress.clamp(0.0, 1.0) * STEPS).round() as u32
}

struct Cache<'a, M> {
    key: (u32, Option<usize>),
    element: Element<'a, M>,
}

fn cached<'s, 'a, M>(
    slot: &'s mut Option<Cache<'a, M>>, tree: &mut Tree, key: (u32, Option<usize>),
    build: impl FnOnce() -> Element<'a, M>,
) -> &'s mut Element<'a, M> {
    if slot.as_ref().is_none_or(|cache| cache.key != key) {
        let element = build();
        tree.diff(&element);
        *slot = Some(Cache { key, element });
    }
    &mut slot.as_mut().expect("the cache was just filled").element
}

pub struct Tooltip<'a, M> {
    content: Element<'a, M>,
    label: &'a str,
    paint: Paint,
    tip: Option<Cache<'a, M>>,
}

#[must_use]
pub fn tooltip<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>, label: &'a str) -> Element<'a, M> {
    if label.is_empty() {
        return content.into();
    }
    Element::new(Tooltip { content: content.into(), label, paint: *paint, tip: None })
}

fn tip<'a, M: 'a>(paint: &Paint, label: &'a str) -> Element<'a, M> {
    let paint = *paint;
    widgets::surface(
        container(text(label, TextStyle::CAP).wrapping(Wrapping::None)).padding([SP1, SP2]).style(move |_| {
            let mut style = style::popover(&paint);
            style.border.radius = R2.into();
            style
        }),
        &paint,
        Level::El2,
        R2,
    )
}

impl<'a, M: 'a> Widget<M, Theme, Renderer> for Tooltip<'a, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Hint>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Hint {
            shown_at: None,
            over: false,
            suppressed: false,
            reveal: Motion::at(0.0),
            tree: Tree::new(tip::<M>(&self.paint, self.label)),
        })
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
        let hint = tree.state.downcast_mut::<Hint>();
        let over = cursor.is_over(layout.bounds());
        if matches!(event, Event::Mouse(mouse::Event::ButtonPressed(_)) | Event::Keyboard(_)) && over {
            hint.suppressed = true;
        }
        if !over {
            hint.suppressed = false;
            hint.shown_at = None;
        } else if !hint.over {
            hint.shown_at = Some(Instant::now());
            shell.request_redraw_at(Instant::now() + component::TOOLTIP_DELAY);
        }
        hint.over = over;
        let due = hint.shown_at.is_some_and(|at| at.elapsed() >= component::TOOLTIP_DELAY);
        if let Some(at) = hint.shown_at.filter(|_| !due) {
            shell.request_redraw_at(at + component::TOOLTIP_DELAY);
        }
        let changed = hint.reveal.to(if due && !hint.suppressed { 1.0 } else { 0.0 }, Speed::STATE);
        if changed
            || (hint.reveal.animating(Speed::STATE)
                && matches!(event, Event::Window(window::Event::RedrawRequested(_))))
        {
            shell.request_redraw();
        }
    }

    fn draw(
        &self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>,
        cursor: mouse::Cursor, viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
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
        let (state, children) = (&mut tree.state, &mut tree.children);
        let inner = self.content.as_widget_mut().overlay(&mut children[0], layout, renderer, viewport, translation);
        let hint = state.downcast_mut::<Hint>();
        let progress = step(hint.reveal.value(Speed::STATE)) as f32 / STEPS;
        if inner.is_some() || progress <= 0.0 {
            return inner;
        }
        let (paint, label) = (self.paint, self.label);
        let element =
            cached(&mut self.tip, &mut hint.tree, (step(progress), None), || tip(&paint.faded(progress), label));
        Some(overlay::Element::new(Box::new(Floating {
            element: element.as_widget_mut(),
            tree: &mut hint.tree,
            anchor: layout.bounds() + translation,
            progress,
            side: Side::Below,
            width: None,
            centred: true,
            gap: SP2,
        })))
    }
}

struct Floating<'b, M> {
    element: &'b mut (dyn Widget<M, Theme, Renderer> + 'b),
    tree: &'b mut Tree,
    anchor: Rectangle,
    progress: f32,
    side: Side,
    width: Option<f32>,
    centred: bool,
    gap: f32,
}

impl<M> Floating<'_, M> {
    fn lay(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let max_width = self.width.unwrap_or(bounds.width - 2.0 * MARGIN);
        let below = bounds.height - (self.anchor.y + self.anchor.height + self.gap) - MARGIN;
        let above = self.anchor.y - self.gap - MARGIN;
        let limits = layout::Limits::new(
            Size::new(self.width.unwrap_or(0.0), 0.0),
            Size::new(max_width, below.max(above).max(48.0)),
        );
        let node = self.element.layout(self.tree, renderer, &limits);
        let (at, side) = place(self.anchor, node.size(), bounds, self.gap, self.centred);
        self.side = side;
        node.move_to(at)
    }
}

impl<M> overlay::Overlay<M, Theme, Renderer> for Floating<'_, M> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        self.lay(renderer, bounds)
    }

    fn draw(
        &self, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        use iced::advanced::Renderer as _;
        let bounds = layout.bounds();
        renderer.with_transformation(
            rise(self.progress, self.side, bounds, component::POPOVER_CLOSED_SCALE),
            |renderer| {
                self.element.draw(self.tree, renderer, theme, style, layout, cursor, &bounds);
            },
        );
    }
}

struct Shown {
    open: bool,
    reveal: Motion,
    highlight: Option<usize>,
    tree: Tree,
}

type Build<'a, M> = Box<dyn Fn(&Paint, Option<usize>) -> Element<'a, M> + 'a>;

pub struct Popup<'a, M> {
    anchor: Element<'a, M>,
    build: Build<'a, M>,
    paint: Paint,
    width: Option<f32>,
    rows: usize,
    on_pick: Option<Box<dyn Fn(usize) -> M + 'a>>,
    enabled: bool,
    content: Option<Cache<'a, M>>,
}

impl<'a, M: 'a> Popup<'a, M> {
    #[must_use]
    pub fn new(
        paint: &Paint, anchor: impl Into<Element<'a, M>>, build: impl Fn(&Paint, Option<usize>) -> Element<'a, M> + 'a,
    ) -> Self {
        Self {
            anchor: anchor.into(),
            build: Box::new(build),
            paint: *paint,
            width: None,
            rows: 0,
            on_pick: None,
            enabled: true,
            content: None,
        }
    }

    #[must_use]
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    #[must_use]
    pub fn keyboard(mut self, rows: usize, on_pick: impl Fn(usize) -> M + 'a) -> Self {
        self.rows = rows;
        self.on_pick = Some(Box::new(on_pick));
        self
    }

    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl<'a, M: 'a> From<Popup<'a, M>> for Element<'a, M> {
    fn from(popup: Popup<'a, M>) -> Self {
        Element::new(popup)
    }
}

impl<M> Widget<M, Theme, Renderer> for Popup<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Shown>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Shown {
            open: false,
            reveal: Motion::at(0.0),
            highlight: None,
            tree: Tree::new((self.build)(&self.paint, None)),
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.anchor)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.anchor]);
    }

    fn size(&self) -> Size<Length> {
        self.anchor.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        self.anchor.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, viewport: &Rectangle,
    ) {
        self.anchor.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        let state = tree.state.downcast_mut::<Shown>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if self.enabled && cursor.is_over(layout.bounds()) && !shell.is_event_captured() =>
            {
                state.open = !state.open;
                state.highlight = None;
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) if state.open => {
                match key {
                    Key::Named(Named::Escape) => state.open = false,
                    Key::Named(Named::ArrowDown | Named::Tab) if self.rows > 0 => {
                        state.highlight = Some(state.highlight.map_or(0, |at| (at + 1) % self.rows));
                    }
                    Key::Named(Named::ArrowUp) if self.rows > 0 => {
                        state.highlight =
                            Some(state.highlight.map_or(self.rows - 1, |at| (at + self.rows - 1) % self.rows));
                    }
                    Key::Named(Named::Enter | Named::Space) => {
                        if let (Some(at), Some(pick)) = (state.highlight, &self.on_pick) {
                            shell.publish(pick(at));
                            state.open = false;
                        }
                    }
                    _ => return,
                }
                shell.capture_event();
            }
            _ => {}
        }
        let changed = state.reveal.to(if state.open { 1.0 } else { 0.0 }, Speed::FADE);
        if changed
            || (state.reveal.animating(Speed::FADE)
                && matches!(event, Event::Window(window::Event::RedrawRequested(_))))
        {
            shell.request_redraw();
        }
    }

    fn draw(
        &self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>,
        cursor: mouse::Cursor, viewport: &Rectangle,
    ) {
        self.anchor.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.enabled && cursor.is_over(layout.bounds()) {
            return mouse::Interaction::Pointer;
        }
        self.anchor.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }

    fn overlay<'b>(
        &'b mut self, tree: &'b mut Tree, layout: Layout<'b>, _: &Renderer, _: &Rectangle, translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        let state = tree.state.downcast_mut::<Shown>();
        let progress = step(state.reveal.value(Speed::FADE)) as f32 / STEPS;
        if progress <= 0.0 && !state.open {
            return None;
        }
        let (paint, highlight, build) = (self.paint, state.highlight, &self.build);
        let element = cached(&mut self.content, &mut state.tree, (step(progress), highlight), || {
            build(&paint.faded(progress.max(0.01)), highlight)
        });
        let anchor = layout.bounds() + translation;
        Some(overlay::Element::new(Box::new(MenuOverlay {
            floating: Floating {
                element: element.as_widget_mut(),
                tree: &mut state.tree,
                anchor,
                progress,
                side: Side::Below,
                width: Some(self.width.unwrap_or(anchor.width).max(160.0)),
                centred: false,
                gap: GAP,
            },
            open: &mut state.open,
            highlight: &mut state.highlight,
        })))
    }
}

struct MenuOverlay<'b, M> {
    floating: Floating<'b, M>,
    open: &'b mut bool,
    highlight: &'b mut Option<usize>,
}

impl<M> overlay::Overlay<M, Theme, Renderer> for MenuOverlay<'_, M> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        self.floating.lay(renderer, bounds)
    }

    fn draw(
        &self, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        overlay::Overlay::<M, Theme, Renderer>::draw(&self.floating, renderer, theme, style, layout, cursor);
    }

    fn update(
        &mut self, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>,
    ) {
        if !*self.open {
            return;
        }
        let bounds = layout.bounds();
        if let Event::Mouse(mouse::Event::ButtonPressed(_)) = event
            && !cursor.is_over(bounds)
            && !cursor.is_over(self.floating.anchor)
        {
            *self.open = false;
            shell.capture_event();
            shell.request_redraw();
            return;
        }
        if matches!(event, Event::Mouse(mouse::Event::CursorMoved { .. })) && cursor.is_over(bounds) {
            *self.highlight = None;
        }
        let mut picked = Vec::new();
        let (captured, invalid, redraw) = {
            let mut inner = Shell::new(&mut picked);
            self.floating.element.update(
                self.floating.tree,
                event,
                layout,
                cursor,
                renderer,
                clipboard,
                &mut inner,
                &bounds,
            );
            (inner.is_event_captured(), inner.is_layout_invalid(), inner.redraw_request())
        };
        if captured {
            shell.capture_event();
        }
        if invalid {
            shell.invalidate_layout();
        }
        shell.request_redraw_at(redraw);
        if !picked.is_empty() {
            *self.open = false;
            shell.request_redraw();
        }
        for message in picked {
            shell.publish(message);
        }
    }

    fn mouse_interaction(&self, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer) -> mouse::Interaction {
        let bounds = layout.bounds();
        self.floating.element.mouse_interaction(self.floating.tree, layout, cursor, &bounds, renderer)
    }
}

fn menu_card<'a, M: 'a>(paint: &Paint, rows: Vec<Element<'a, M>>) -> Element<'a, M> {
    let paint = *paint;
    widgets::surface(
        container(super::scrollable(&paint, Column::with_children(rows).spacing(1)))
            .padding(SP1)
            .width(Fill)
            .style(move |_| style::popover(&paint)),
        &paint,
        Level::El3,
        R3,
    )
}

fn option_row<'a, M: Clone + 'a>(
    paint: &Paint, label: String, checked: bool, highlighted: bool, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let mark: Element<'a, M> =
        if checked { icon(Icon::Check, 14.0, paint.acc_bg()) } else { Space::new().width(14).height(14).into() };
    let content = container(
        row![mark, text(label, TextStyle::MENU_ITEM).wrapping(Wrapping::None)].spacing(SP2).align_y(Alignment::Center),
    )
    .center_y(component::MENU_ITEM)
    .width(Fill)
    .padding(Padding::from([0.0, SP2]))
    .clip(true);
    let row = widgets::container(content, move |hover, press| {
        let lit = if highlighted { 1.0 } else { hover.max(press) };
        container::Style {
            text_color: Some(paint.fg_1()),
            ..style::rounded(super::mix(iced::Color::TRANSPARENT, paint.fill_2(), lit), R2)
        }
    });
    iced::widget::mouse_area(row).on_press(message).interaction(mouse::Interaction::Pointer).into()
}

fn opener<'a, M: 'a>(paint: &Paint, label: String, color: iced::Color, width: Length) -> Element<'a, M> {
    let paint = *paint;
    let mut content = row![text(label, TextStyle::CALL).color(color).wrapping(Wrapping::None)]
        .spacing(SP2)
        .align_y(Alignment::Center);
    if width == Length::Fill {
        content = content.push(Space::new().width(Fill));
    }
    widgets::container(
        container(content.push(icon(Icon::Down, 12.0, paint.fg_2())))
            .padding(Padding::from([0.0, SP3]))
            .height(component::CONTROL)
            .center_y(component::CONTROL)
            .width(width),
        move |hover, press| {
            let background = paint.layer(paint.fill_1(), glide_theme::motion::ST_HOVER * hover + 0.05 * press);
            container::Style {
                border: iced::Border { color: paint.line(), width: 1.0, radius: R2.into() },
                ..style::rounded(background, R2)
            }
        },
    )
}

#[must_use]
pub fn dropdown<'a, T: ToString + PartialEq + Clone + 'a, M: Clone + 'a>(
    paint: &Paint, options: Vec<T>, selected: Option<T>, placeholder: &str, on_select: impl Fn(T) -> M + 'a,
) -> Element<'a, M> {
    let paint = *paint;
    let label = selected.as_ref().map_or_else(|| placeholder.to_string(), ToString::to_string);
    let color = if selected.is_some() { paint.fg_1() } else { paint.fg_3() };
    let anchor = opener(&paint, label, color, Length::Fill);
    let count = options.len();
    let options = std::rc::Rc::new(options);
    let on_select = std::rc::Rc::new(on_select);
    let (pick_options, pick) = (std::rc::Rc::clone(&options), std::rc::Rc::clone(&on_select));
    Popup::new(&paint, anchor, move |paint, highlight| {
        let rows = options
            .iter()
            .enumerate()
            .map(|(index, option)| {
                option_row(
                    paint,
                    option.to_string(),
                    selected.as_ref() == Some(option),
                    highlight == Some(index),
                    on_select(option.clone()),
                )
            })
            .collect();
        menu_card(paint, rows)
    })
    .keyboard(count, move |index| pick(pick_options[index].clone()))
    .into()
}

#[must_use]
pub fn menu_button<'a, M: Clone + 'a>(
    paint: &Paint, label: &'a str, items: Vec<(String, M)>, width: f32,
) -> Element<'a, M> {
    let count = items.len();
    let picks: Vec<M> = items.iter().map(|(_, message)| message.clone()).collect();
    let anchor = opener(paint, label.to_string(), paint.fg_1(), Length::Shrink);
    Popup::new(paint, anchor, move |paint, highlight| {
        let rows = items
            .iter()
            .enumerate()
            .map(|(index, (label, message))| {
                option_row(paint, label.clone(), false, highlight == Some(index), message.clone())
            })
            .collect();
        menu_card(paint, rows)
    })
    .width(width)
    .keyboard(count, move |index| picks[index].clone())
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_popup_opens_below_and_flips_above_near_the_bottom() {
        let bounds = Size::new(800.0, 600.0);
        let anchor = Rectangle::new(Point::new(100.0, 100.0), Size::new(120.0, 34.0));
        let (at, side) = place(anchor, Size::new(200.0, 300.0), bounds, GAP, false);
        assert_eq!(side, Side::Below);
        assert_eq!(at, Point::new(100.0, 138.0));
        let low = Rectangle::new(Point::new(100.0, 520.0), Size::new(120.0, 34.0));
        let (at, side) = place(low, Size::new(200.0, 300.0), bounds, GAP, false);
        assert_eq!(side, Side::Above);
        assert!(at.y + 300.0 <= 520.0);
    }

    #[test]
    fn a_popup_never_leaves_the_window_sideways() {
        let bounds = Size::new(400.0, 600.0);
        let anchor = Rectangle::new(Point::new(380.0, 10.0), Size::new(20.0, 20.0));
        let (at, _) = place(anchor, Size::new(200.0, 100.0), bounds, GAP, true);
        assert!(at.x + 200.0 <= bounds.width - MARGIN + 0.01);
        assert!(at.x >= MARGIN);
    }
}

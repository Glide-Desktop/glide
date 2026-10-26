use std::borrow::Cow;
use std::collections::HashMap;

use glide_iced::Paint;
use glide_iced::style::{self, R2, SP1, SP2, SP3};
use glide_iced::widgets;
use glide_theme::elevation::{self, Level};
use glide_theme::motion::{ST_HOVER, ST_PRESS};
use glide_theme::{TextStyle, shape::R_ROUND};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{Operation, Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, overlay};
use iced::widget::{Space, button, container, mouse_area, row, stack};
use iced::{
    Alignment, Border, Color, Element, Event, Fill, Length, Padding, Point, Rectangle, Renderer, Size, Theme, Vector,
    mouse, window,
};

use super::buttons::{Tone, button_style};
use super::feedback::Severity;
use super::{Icon, Wrapping, icon, mix, text, tooltip};
use crate::anim::{Motion, Speed};

const PAD: f32 = SP2;
const GAP: f32 = SP1;
const CLOSE: f32 = 22.0;
const SLOP: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mark {
    #[default]
    None,
    Dot,
    Alert,
}

#[derive(Debug, Clone)]
pub struct Tab<'a> {
    pub id: u64,
    pub label: Cow<'a, str>,
    pub mark: Mark,
    pub lead: Option<(Icon, Severity)>,
    pub hint: Option<&'a str>,
}

impl<'a> Tab<'a> {
    #[must_use]
    pub fn new(id: u64, label: impl Into<Cow<'a, str>>) -> Self {
        Self { id, label: label.into(), mark: Mark::None, lead: None, hint: None }
    }

    #[must_use]
    pub fn mark(mut self, mark: Mark) -> Self {
        self.mark = mark;
        self
    }

    #[must_use]
    pub fn lead(mut self, glyph: Icon, severity: Severity) -> Self {
        self.lead = Some((glyph, severity));
        self
    }

    #[must_use]
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = Some(hint);
        self
    }
}

type Pick<'a, M> = Box<dyn Fn(usize) -> M + 'a>;
type Reorder<'a, M> = Box<dyn Fn(usize, usize) -> M + 'a>;

pub struct TabStrip<'a, M> {
    paint: Paint,
    tabs: Vec<Tab<'a>>,
    active: usize,
    hovered: Option<usize>,
    height: f32,
    widest: f32,
    bare: bool,
    select: Option<Pick<'a, M>>,
    close: Option<(Pick<'a, M>, &'a str)>,
    hover: Option<Box<dyn Fn(Option<usize>) -> M + 'a>>,
    reorder: Option<Reorder<'a, M>>,
}

#[must_use]
pub fn tab_strip<'a, M>(paint: &Paint, tabs: Vec<Tab<'a>>, active: usize) -> TabStrip<'a, M> {
    TabStrip {
        paint: *paint,
        tabs,
        active,
        hovered: None,
        height: 36.0,
        widest: f32::INFINITY,
        bare: false,
        select: None,
        close: None,
        hover: None,
        reorder: None,
    }
}

impl<'a, M> TabStrip<'a, M> {
    #[must_use]
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    #[must_use]
    pub fn widest(mut self, width: f32) -> Self {
        self.widest = width;
        self
    }

    #[must_use]
    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    #[must_use]
    pub fn hovered(mut self, hovered: Option<usize>, on_hover: impl Fn(Option<usize>) -> M + 'a) -> Self {
        self.hovered = hovered;
        self.hover = Some(Box::new(on_hover));
        self
    }

    #[must_use]
    pub fn on_reorder(mut self, reorder: impl Fn(usize, usize) -> M + 'a) -> Self {
        self.reorder = Some(Box::new(reorder));
        self
    }

    #[must_use]
    pub fn on_select(mut self, select: impl Fn(usize) -> M + 'a) -> Self {
        self.select = Some(Box::new(select));
        self
    }

    #[must_use]
    pub fn on_close(mut self, close: impl Fn(usize) -> M + 'a, label: &'a str) -> Self {
        self.close = Some((Box::new(close), label));
        self
    }
}

fn over(top: Color, bottom: Color) -> Color {
    mix(bottom, Color { a: 1.0, ..top }, top.a)
}

fn tab_face<'a, M: Clone + 'a>(strip: &TabStrip<'a, M>, index: usize, tab: Tab<'a>) -> Element<'a, M> {
    let paint = strip.paint;
    let active = index == strip.active;
    let hovered = strip.hovered == Some(index);
    let tint = tab.lead.map(|(_, severity)| severity.colors(&paint));
    let mut label = row![].spacing(SP2).align_y(Alignment::Center);
    if let (Some((glyph, _)), Some((_, strong))) = (tab.lead, tint) {
        label = label.push(icon(glyph, 14.0, strong));
    }
    label = label.push(text(tab.label, TextStyle::CAP).wrapping(Wrapping::None));
    match tab.mark {
        Mark::Dot if !hovered => {
            label = label
                .push(container(Space::new().width(6).height(6)).style(move |_| style::rounded(paint.fg_3(), R_ROUND)));
        }
        Mark::Alert => label = label.push(icon(Icon::Bell, 12.0, paint.warn())),
        _ => {}
    }
    let close: Element<'a, M> = match &strip.close {
        Some((close, hint)) if hovered || active => {
            let control = button(container(icon(Icon::Close, 10.0, paint.fg_2())).center(Fill))
                .on_press(close(index))
                .width(CLOSE)
                .height(CLOSE)
                .padding(0);
            let control = widgets::button(control, move |status| button_style(&paint, Tone::Ghost, status));
            tooltip(&paint, control, hint)
        }
        _ => Space::new().width(CLOSE).height(CLOSE).into(),
    };
    let mut inner = stack![
        container(label).center_x(Fill).center_y(Fill).padding(Padding::from([0.0, CLOSE + SP1 + 2.0])).clip(true),
        container(close).padding(Padding::from([0.0, SP1])).height(Fill).align_y(Alignment::Center),
    ];
    if let Some((_, strong)) = tint {
        let stripe = container(Space::new().width(Fill).height(2)).style(move |_| style::rounded(strong, 1.0));
        inner = inner.push(container(stripe).height(Fill).align_y(Alignment::End).padding(Padding {
            left: SP3,
            right: SP3,
            ..Padding::ZERO
        }));
    }
    let face = widgets::container(container(inner).width(Fill).height(Fill), move |hover, press| {
        let (background, color) = match (active, tint) {
            (true, Some((soft, strong))) => (over(soft, paint.view_bg()), strong),
            (true, None) => (paint.view_bg(), paint.fg_1()),
            (false, _) => (
                paint.layer(Color::TRANSPARENT, ST_HOVER * hover + (ST_PRESS - ST_HOVER) * press),
                if hover > 0.5 { paint.fg_1() } else { paint.fg_2() },
            ),
        };
        container::Style { text_color: Some(color), ..style::rounded(background, R2) }
    });
    let face = if active { widgets::surface(face, &paint, Level::El1, R2) } else { face };
    let face = match tab.hint {
        Some(hint) => tooltip(&paint, face, hint),
        None => face,
    };
    let mut area = mouse_area(face);
    if let Some(hover) = &strip.hover {
        area = area.on_enter(hover(Some(index))).on_exit(hover(None));
    }
    if let Some((close, _)) = &strip.close {
        area = area.on_middle_press(close(index));
    }
    area.into()
}

impl<'a, M: Clone + 'a> From<TabStrip<'a, M>> for Element<'a, M> {
    fn from(mut strip: TabStrip<'a, M>) -> Self {
        let paint = strip.paint;
        let tabs = std::mem::take(&mut strip.tabs);
        let ids = tabs.iter().map(|tab| tab.id).collect();
        let faces = tabs.into_iter().enumerate().map(|(index, tab)| tab_face(&strip, index, tab)).collect();
        let bar = Element::new(Strip {
            paint,
            faces,
            ids,
            height: strip.height,
            widest: strip.widest,
            select: strip.select,
            reorder: strip.reorder,
        });
        if strip.bare {
            bar
        } else {
            container(bar).width(Fill).style(move |_| style::rounded(paint.bar_bg(), 0.0)).into()
        }
    }
}

#[must_use]
fn tab_width(width: f32, count: usize, widest: f32) -> f32 {
    if count == 0 {
        return 0.0;
    }
    let count = count as f32;
    ((width - 2.0 * PAD - GAP * (count - 1.0)) / count).min(widest).max(0.0)
}

#[must_use]
fn slot_x(index: usize, tab: f32) -> f32 {
    PAD + index as f32 * (tab + GAP)
}

#[must_use]
fn slot_at(left: f32, tab: f32, count: usize) -> usize {
    (((left - PAD) / (tab + GAP).max(1.0)).round().max(0.0) as usize).min(count.saturating_sub(1))
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Press {
    id: u64,
    at: usize,
    origin: f32,
    grab: f32,
    cursor: f32,
    moved: bool,
}

impl Press {
    fn left(&self, tab: f32, count: usize) -> f32 {
        (self.cursor - self.grab).clamp(PAD, slot_x(count.saturating_sub(1), tab).max(PAD))
    }
}

#[derive(Debug, Default)]
struct State {
    ids: Vec<u64>,
    slides: HashMap<u64, Motion>,
    width: f32,
    press: Option<Press>,
}

impl State {
    fn lifted(&self) -> Option<Press> {
        self.press.filter(|press| press.moved)
    }

    fn settle(&mut self, ids: &[u64], slots: &[f32], width: f32, tab: f32) -> bool {
        let resized = (self.width - width).abs() > 0.5;
        self.width = width;
        let lifted = self.lifted();
        let mut started = false;
        for (id, x) in ids.iter().zip(slots) {
            let slide = self.slides.entry(*id).or_insert_with(|| Motion::at(*x));
            match lifted {
                Some(press) if press.id == *id => slide.jump(press.left(tab, ids.len())),
                _ if resized => slide.jump(*x),
                _ => started |= slide.to(*x, Speed::SLIDE),
            }
        }
        self.slides.retain(|id, _| ids.contains(id));
        started
    }

    fn animating(&self) -> bool {
        self.slides.values().any(|slide| slide.animating(Speed::SLIDE))
    }

    fn offset(&self, id: u64, x: f32) -> f32 {
        self.slides.get(&id).map_or(0.0, |slide| slide.value(Speed::SLIDE) - x)
    }
}

struct Strip<'a, M> {
    paint: Paint,
    faces: Vec<Element<'a, M>>,
    ids: Vec<u64>,
    height: f32,
    widest: f32,
    select: Option<Pick<'a, M>>,
    reorder: Option<Reorder<'a, M>>,
}

impl<M> Strip<'_, M> {
    fn tab(layout: Layout<'_>) -> f32 {
        layout.children().next().map_or(0.0, |child| child.bounds().width)
    }

    fn slots(layout: Layout<'_>) -> Vec<f32> {
        let origin = layout.bounds().x;
        layout.children().map(|child| child.bounds().x - origin).collect()
    }
}

impl<M> Widget<M, Theme, Renderer> for Strip<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State { ids: self.ids.clone(), ..State::default() })
    }

    fn children(&self) -> Vec<Tree> {
        self.faces.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<State>();
        if state.ids != self.ids {
            let mut old: Vec<Option<Tree>> = std::mem::take(&mut tree.children).into_iter().map(Some).collect();
            tree.children = self
                .ids
                .iter()
                .zip(&self.faces)
                .map(|(id, face)| {
                    state
                        .ids
                        .iter()
                        .position(|known| known == id)
                        .and_then(|at| old.get_mut(at).and_then(Option::take))
                        .unwrap_or_else(|| Tree::new(face))
                })
                .collect();
            state.ids.clone_from(&self.ids);
            state.press = state
                .press
                .and_then(|press| self.ids.iter().position(|id| *id == press.id).map(|at| Press { at, ..press }));
        }
        tree.diff_children(&self.faces);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(self.height))
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let count = self.faces.len();
        let width = limits.max().width;
        let width = if width.is_finite() { width } else { 2.0 * PAD + count as f32 * self.widest.min(200.0) };
        let tab = tab_width(width, count, self.widest);
        let inner = (self.height - 2.0 * SP1).max(0.0);
        let fixed = layout::Limits::new(Size::new(tab, inner), Size::new(tab, inner));
        let children = self
            .faces
            .iter_mut()
            .zip(&mut tree.children)
            .enumerate()
            .map(|(index, (face, tree))| {
                face.as_widget_mut().layout(tree, renderer, &fixed).move_to(Point::new(slot_x(index, tab), SP1))
            })
            .collect();
        layout::Node::with_children(Size::new(width, self.height), children)
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        for ((face, tree), layout) in self.faces.iter_mut().zip(&mut tree.children).zip(layout.children()) {
            face.as_widget_mut().operate(tree, layout, renderer, operation);
        }
    }

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let inner = if state.lifted().is_some() { mouse::Cursor::Unavailable } else { cursor };
        for ((face, tree), layout) in self.faces.iter_mut().zip(&mut tree.children).zip(layout.children()) {
            face.as_widget_mut().update(tree, event, layout, inner, renderer, clipboard, shell, viewport);
        }
        let bounds = layout.bounds();
        let tab = Self::tab(layout);
        let count = self.ids.len();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if !shell.is_event_captured() => {
                let pressed = cursor.position_over(bounds).and_then(|position| {
                    layout.children().position(|child| child.bounds().contains(position)).map(|at| (at, position.x))
                });
                if let Some((at, x)) = pressed {
                    let x = x - bounds.x;
                    state.press = Some(Press {
                        id: self.ids[at],
                        at,
                        origin: x,
                        grab: x - slot_x(at, tab),
                        cursor: x,
                        moved: false,
                    });
                    if let Some(select) = &self.select {
                        shell.publish(select(at));
                    }
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(press) = &mut state.press {
                    press.cursor = position.x - bounds.x;
                    press.moved |= (press.cursor - press.origin).abs() >= SLOP;
                    if press.moved {
                        let target = slot_at(press.left(tab, count), tab, count);
                        if target != press.at
                            && let Some(reorder) = &self.reorder
                        {
                            shell.publish(reorder(press.at, target));
                            press.at = target;
                        }
                        shell.request_redraw();
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left) | mouse::Event::CursorLeft)
                if state.press.take().is_some_and(|press| press.moved) =>
            {
                shell.request_redraw();
            }
            _ => {}
        }
        let started = state.settle(&self.ids, &Self::slots(layout), bounds.width, tab);
        if started || (state.animating() && matches!(event, Event::Window(window::Event::RedrawRequested(_)))) {
            shell.request_redraw();
        }
    }

    fn draw(
        &self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>,
        cursor: mouse::Cursor, viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let origin = layout.bounds().x;
        let lifted = state.lifted().map(|press| press.id);
        let mut top = None;
        for (((face, tree), layout), id) in self.faces.iter().zip(&tree.children).zip(layout.children()).zip(&self.ids)
        {
            let offset = state.offset(*id, layout.bounds().x - origin);
            if lifted == Some(*id) {
                top = Some((face, tree, layout, offset));
                continue;
            }
            renderer.with_translation(Vector::new(offset, 0.0), |renderer| {
                face.as_widget().draw(tree, renderer, theme, style, layout, cursor, viewport);
            });
        }
        let Some((face, tree, layout, offset)) = top else { return };
        renderer.with_layer(*viewport, |renderer| {
            renderer.with_translation(Vector::new(offset, 0.0), |renderer| {
                for layer in elevation::layers(Level::El2, self.paint.tokens.appearance) {
                    renderer.fill_quad(
                        Quad {
                            bounds: layout.bounds(),
                            border: Border { radius: R2.into(), ..Border::default() },
                            shadow: iced::Shadow {
                                color: style::to_iced(layer.color).scale_alpha(self.paint.alpha),
                                offset: Vector::new(layer.dx, layer.dy),
                                blur_radius: layer.blur,
                            },
                            ..Quad::default()
                        },
                        Color::TRANSPARENT,
                    );
                }
                face.as_widget().draw(tree, renderer, theme, style, layout, cursor, viewport);
            });
        });
    }

    fn mouse_interaction(
        &self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &Renderer,
    ) -> mouse::Interaction {
        if tree.state.downcast_ref::<State>().lifted().is_some() {
            return mouse::Interaction::Grabbing;
        }
        self.faces
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((face, tree), layout)| face.as_widget().mouse_interaction(tree, layout, cursor, viewport, renderer))
            .max()
            .unwrap_or_default()
    }

    fn overlay<'b>(
        &'b mut self, tree: &'b mut Tree, layout: Layout<'b>, renderer: &Renderer, viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        overlay::from_children(&mut self.faces, tree, layout, renderer, viewport, translation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum Sent {
        Select(usize),
        Reorder(usize, usize),
        Hover,
    }

    struct Bench {
        paint: Paint,
        order: Vec<u64>,
        tree: Tree,
    }

    impl Bench {
        const SIZE: Size = Size::new(600.0, 36.0);

        fn new(order: Vec<u64>) -> Self {
            let paint = Paint::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue);
            let tree = Tree::new(Self::strip(&paint, &order));
            Self { paint, order, tree }
        }

        fn strip(paint: &Paint, order: &[u64]) -> Element<'static, Sent> {
            let tabs = order.iter().map(|id| Tab::new(*id, format!("tab {id}"))).collect();
            tab_strip(paint, tabs, 0)
                .on_select(Sent::Select)
                .on_reorder(Sent::Reorder)
                .hovered(None, |_| Sent::Hover)
                .bare()
                .into()
        }

        fn send(&mut self, event: &Event, x: f32) -> Vec<Sent> {
            let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0)));
            let mut element = Self::strip(&self.paint, &self.order);
            self.tree.diff(&element);
            let limits = layout::Limits::new(Size::ZERO, Self::SIZE);
            let node = element.as_widget_mut().layout(&mut self.tree, &renderer, &limits);
            let mut messages = Vec::new();
            let mut shell = Shell::new(&mut messages);
            element.as_widget_mut().update(
                &mut self.tree,
                event,
                Layout::new(&node),
                mouse::Cursor::Available(Point::new(x, 18.0)),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
                &Rectangle::with_size(Self::SIZE),
            );
            messages.retain(|message| *message != Sent::Hover);
            for message in &messages {
                if let Sent::Reorder(from, to) = message {
                    let id = self.order.remove(*from);
                    self.order.insert(*to, id);
                }
            }
            messages
        }

        fn draw(&mut self, x: f32) {
            let mut renderer =
                Renderer::Secondary(iced_tiny_skia::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0)));
            let mut element = Self::strip(&self.paint, &self.order);
            self.tree.diff(&element);
            let limits = layout::Limits::new(Size::ZERO, Self::SIZE);
            let node = element.as_widget_mut().layout(&mut self.tree, &renderer, &limits);
            element.as_widget().draw(
                &self.tree,
                &mut renderer,
                &Theme::Dark,
                &renderer::Style::default(),
                Layout::new(&node),
                mouse::Cursor::Available(Point::new(x, 18.0)),
                &Rectangle::with_size(Self::SIZE),
            );
        }

        fn moved(&mut self, x: f32) -> Vec<Sent> {
            self.send(&Event::Mouse(mouse::Event::CursorMoved { position: Point::new(x, 18.0) }), x)
        }

        fn state(&self) -> &State {
            self.tree.state.downcast_ref::<State>()
        }
    }

    #[test]
    fn dragging_a_tab_selects_it_follows_the_pointer_and_slides_it_home_on_release() {
        let mut bench = Bench::new(vec![10, 20, 30]);
        let tab = tab_width(Bench::SIZE.width, 3, f32::INFINITY);
        let grab = slot_x(0, tab) + 20.0;
        let pressed = bench.send(&Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), grab);
        assert_eq!(pressed, vec![Sent::Select(0)]);
        assert!(bench.moved(grab + 3.0).is_empty(), "a wobble under the slop is still a click");
        assert!(bench.state().lifted().is_none());
        assert_eq!(bench.moved(grab + 0.6 * (tab + GAP)), vec![Sent::Reorder(0, 1)]);
        assert_eq!(bench.order, vec![20, 10, 30]);
        bench.draw(grab + 0.6 * (tab + GAP));
        assert!(bench.moved(grab + 0.7 * (tab + GAP)).is_empty(), "the same slot is not asked for twice");
        assert_eq!(bench.moved(grab + 900.0), vec![Sent::Reorder(1, 2)]);
        assert_eq!(bench.order, vec![20, 30, 10]);
        let held = bench.state().slides[&10].value(Speed::SLIDE);
        assert!((held - slot_x(2, tab)).abs() < 0.01, "the lifted tab stops at the end of the strip: {held}");
        let back = grab + 1.2 * (tab + GAP);
        assert_eq!(bench.moved(back), vec![Sent::Reorder(2, 1)]);
        assert_eq!(bench.order, vec![20, 10, 30]);
        let released = bench.send(&Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)), back);
        assert!(released.is_empty());
        assert!(bench.state().lifted().is_none());
        assert!((bench.state().slides[&10].target() - slot_x(1, tab)).abs() < 0.01, "it glides back into its slot");
        assert!((bench.state().slides[&20].target() - slot_x(0, tab)).abs() < 0.01);
    }

    #[test]
    fn a_press_beside_the_tabs_neither_selects_nor_drags() {
        let mut bench = Bench::new(vec![1, 2]);
        assert!(bench.send(&Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), 2.0).is_empty());
        assert!(bench.moved(300.0).is_empty());
        assert!(bench.state().press.is_none());
    }

    fn press(at: usize, tab: f32, cursor: f32) -> Press {
        let origin = slot_x(at, tab) + 10.0;
        Press { id: at as u64, at, origin, grab: 10.0, cursor, moved: true }
    }

    #[test]
    fn tabs_share_the_strip_evenly_but_never_grow_past_the_widest() {
        let width = 2.0 * PAD + 4.0 * 197.0 + 3.0 * GAP;
        assert!((tab_width(width, 4, f32::INFINITY) - 197.0).abs() < 0.01);
        assert!((tab_width(width, 2, 220.0) - 220.0).abs() < 0.01);
        assert_eq!(tab_width(width, 0, 220.0), 0.0);
        assert_eq!(tab_width(10.0, 3, 220.0), 0.0);
    }

    #[test]
    fn a_dragged_tab_follows_the_pointer_but_stays_inside_the_strip() {
        let tab = 100.0;
        assert!((press(1, tab, 300.0).left(tab, 4) - 290.0).abs() < 0.01);
        assert_eq!(press(0, tab, -400.0).left(tab, 4), PAD);
        assert_eq!(press(0, tab, 9000.0).left(tab, 4), slot_x(3, tab));
        assert_eq!(press(0, tab, 300.0).left(tab, 1), PAD);
    }

    #[test]
    fn the_slot_changes_once_the_dragged_tab_is_past_half_of_its_neighbour() {
        let tab = 100.0;
        let step = tab + GAP;
        assert_eq!(slot_at(PAD + 0.49 * step, tab, 4), 0);
        assert_eq!(slot_at(PAD + 0.51 * step, tab, 4), 1);
        assert_eq!(slot_at(PAD + 2.6 * step, tab, 4), 3);
        assert_eq!(slot_at(-50.0, tab, 4), 0);
        assert_eq!(slot_at(9000.0, tab, 4), 3);
        assert_eq!(slot_at(100.0, tab, 0), 0);
    }

    #[test]
    fn a_reordered_tab_slides_from_its_old_slot_to_the_new_one() {
        let mut state = State::default();
        let tab = 100.0;
        let slots = [slot_x(0, tab), slot_x(1, tab), slot_x(2, tab)];
        assert!(!state.settle(&[1, 2, 3], &slots, 400.0, tab));
        assert!(state.settle(&[2, 1, 3], &slots, 400.0, tab));
        assert!((state.slides[&2].target() - slots[0]).abs() < 0.01);
        assert!((state.slides[&1].target() - slots[1]).abs() < 0.01);
        assert!((state.slides[&3].target() - slots[2]).abs() < 0.01);
        assert!(!state.settle(&[2, 1, 3], &slots, 400.0, tab));
    }

    #[test]
    fn a_resized_strip_snaps_tabs_into_place_instead_of_sliding() {
        let mut state = State::default();
        state.settle(&[1, 2], &[PAD, PAD + 104.0], 216.0, 100.0);
        assert!(!state.settle(&[1, 2], &[PAD, PAD + 204.0], 416.0, 200.0));
        assert!(!state.animating());
        assert_eq!(state.offset(2, PAD + 204.0), 0.0);
    }

    #[test]
    fn the_lifted_tab_is_drawn_where_the_pointer_holds_it() {
        let mut state = State::default();
        let tab = 100.0;
        let slots = [slot_x(0, tab), slot_x(1, tab)];
        state.settle(&[0, 1], &slots, 216.0, tab);
        state.press = Some(press(0, tab, 60.0));
        state.settle(&[0, 1], &slots, 216.0, tab);
        assert!((state.offset(0, slots[0]) - (50.0 - slots[0])).abs() < 0.01);
        state.press = None;
        assert!(state.settle(&[0, 1], &slots, 216.0, tab));
    }
}

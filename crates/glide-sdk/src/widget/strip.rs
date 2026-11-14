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
use iced::advanced::text::{self as typeset, Renderer as _};
use iced::advanced::widget::{Operation, Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, overlay};
use iced::alignment::Vertical;
use iced::widget::{Space, button, container, mouse_area, row, stack};
use iced::{
    Alignment, Border, Color, Element, Event, Fill, Font, Length, Padding, Pixels, Point, Rectangle, Renderer, Size,
    Theme, Transformation, Vector, mouse, window,
};

use super::buttons::{Tone, button_style};
use super::feedback::Severity;
use super::{Icon, Wrapping, icon, mix, popping, text, tooltip};
use crate::anim::{Motion, Speed, animate};

const PAD: f32 = SP2;
const GAP: f32 = SP1;
const CLOSE: f32 = 22.0;
const SLOP: f32 = 6.0;
const ARRIVE: f32 = 0.8;
const MIN_SCALE: f32 = 0.001;
const NARROWEST: f32 = 128.0;
const FADE: f32 = 24.0;
const EDGE: f32 = 32.0;
const NOTCH: f32 = 64.0;
const CARRY: f32 = 720.0;

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
    narrowest: f32,
    bare: bool,
    ground: Color,
    select: Option<Pick<'a, M>>,
    close: Option<(Pick<'a, M>, &'a str)>,
    menu: Option<Pick<'a, M>>,
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
        narrowest: NARROWEST,
        bare: false,
        ground: paint.bar_bg(),
        select: None,
        close: None,
        menu: None,
        hover: None,
        reorder: None,
    }
}

#[must_use]
pub fn tabs_fit(width: f32, count: usize) -> bool {
    run(count, NARROWEST) <= width
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
    pub fn narrowest(mut self, width: f32) -> Self {
        self.narrowest = width.max(CLOSE + 2.0 * SP1);
        self
    }

    #[must_use]
    pub fn ground(mut self, color: Color) -> Self {
        self.ground = color;
        self
    }

    #[must_use]
    pub fn on_menu(mut self, menu: impl Fn(usize) -> M + 'a) -> Self {
        self.menu = Some(Box::new(menu));
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
            let dot = container(Space::new().width(6).height(6)).style(move |_| style::rounded(paint.fg_3(), R_ROUND));
            label = label.push(popping(dot));
        }
        Mark::Alert => label = label.push(popping(icon(Icon::Bell, 12.0, paint.warn()))),
        _ => {}
    }
    let close: Element<'a, M> = match &strip.close {
        Some((close, hint)) => {
            let shown = hovered || active;
            let mut control = button(container(icon(Icon::Close, 10.0, paint.fg_2())).center(Fill))
                .width(CLOSE)
                .height(CLOSE)
                .padding(0);
            if shown {
                control = control.on_press(close(index));
            }
            let control = widgets::button(control, move |status| button_style(&paint, Tone::Ghost, status));
            animate(f32::from(shown), Speed::OVERSHOOT, |_| tooltip(&paint, control, hint))
                .scale(|value| value.max(MIN_SCALE))
                .into()
        }
        None => Space::new().width(CLOSE).height(CLOSE).into(),
    };
    let room = Padding { left: CLOSE + SP1 + 2.0, right: SP3, ..Padding::ZERO };
    let mut inner = stack![
        container(label).center_x(Fill).center_y(Fill).padding(room).clip(true),
        container(close).padding(Padding::from([0.0, SP1])).height(Fill).align_y(Alignment::Center),
    ];
    if let Some((_, strong)) = tint {
        let stripe = container(Space::new().width(Fill).height(2)).style(move |_| style::rounded(strong, 1.0));
        inner = inner.push(container(popping(stripe)).height(Fill).align_y(Alignment::End).padding(Padding {
            left: SP3,
            right: SP3,
            ..Padding::ZERO
        }));
    }
    let on = tint.map_or_else(|| paint.fg_1(), |(_, strong)| strong);
    let face = animate(f32::from(active), Speed::COLOR, move |selected| {
        let selected = selected.clone();
        widgets::container(container(inner).width(Fill).height(Fill), move |hover, press| {
            let lit = selected.get();
            let layer = ST_HOVER * hover + (ST_PRESS - ST_HOVER) * press;
            let idle = if hover > 0.5 { paint.fg_1() } else { paint.fg_2() };
            container::Style {
                text_color: Some(mix(idle, on, lit)),
                ..style::rounded(paint.layer(Color::TRANSPARENT, layer * (1.0 - lit)), R2)
            }
        })
    });
    let face = match tab.hint {
        Some(hint) => tooltip(&paint, face, hint),
        None => face.into(),
    };
    let mut area = mouse_area(face);
    if let Some(hover) = &strip.hover {
        area = area.on_enter(hover(Some(index))).on_exit(hover(None));
    }
    if let Some((close, _)) = &strip.close {
        area = area.on_middle_press(close(index));
    }
    if let Some(menu) = &strip.menu {
        area = area.on_right_press(menu(index));
    }
    animate(1.0, Speed::SLIDE, |_| area).from(0.0).scale(|value| ARRIVE + (1.0 - ARRIVE) * value).into()
}

impl<'a, M: Clone + 'a> From<TabStrip<'a, M>> for Element<'a, M> {
    fn from(mut strip: TabStrip<'a, M>) -> Self {
        let paint = strip.paint;
        let tabs = std::mem::take(&mut strip.tabs);
        let ids: Vec<u64> = tabs.iter().map(|tab| tab.id).collect();
        let looks = tabs
            .iter()
            .map(|tab| Look {
                label: tab.label.to_string(),
                fill: tab
                    .lead
                    .map_or_else(|| paint.view_bg(), |(_, severity)| over(severity.colors(&paint).0, paint.view_bg())),
            })
            .collect();
        let active = ids.get(strip.active).copied();
        let faces = tabs.into_iter().enumerate().map(|(index, tab)| tab_face(&strip, index, tab)).collect();
        let bar = Element::new(Strip {
            paint,
            faces,
            ids,
            looks,
            active,
            height: strip.height,
            widest: strip.widest,
            narrowest: strip.narrowest,
            ground: strip.ground,
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
fn tab_size(width: f32, count: usize, widest: f32, narrowest: f32) -> f32 {
    tab_width(width, count, widest).max(narrowest.min(widest))
}

#[must_use]
fn run(count: usize, tab: f32) -> f32 {
    if count == 0 {
        return 0.0;
    }
    let count = count as f32;
    2.0 * PAD + count * tab + (count - 1.0) * GAP
}

#[must_use]
fn overflow(width: f32, count: usize, tab: f32) -> f32 {
    (run(count, tab) - width).max(0.0)
}

#[must_use]
fn slot_x(index: usize, tab: f32) -> f32 {
    PAD + index as f32 * (tab + GAP)
}

fn within(cursor: mouse::Cursor, bounds: Rectangle, scroll: f32, loose: bool) -> mouse::Cursor {
    if !loose {
        return cursor;
    }
    if cursor.is_over(bounds) { cursor + Vector::new(scroll, 0.0) } else { mouse::Cursor::Unavailable }
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
    fn left(&self, tab: f32, count: usize, scroll: f32) -> f32 {
        (self.cursor + scroll - self.grab).clamp(PAD, slot_x(count.saturating_sub(1), tab).max(PAD))
    }
}

#[derive(Debug, Clone)]
struct Look {
    label: String,
    fill: Color,
}

#[derive(Debug)]
struct Ghost {
    look: Look,
    x: f32,
    width: f32,
    lit: f32,
    fade: Motion,
}

#[derive(Debug, Default)]
struct State {
    ids: Vec<u64>,
    slides: HashMap<u64, Motion>,
    selected: HashMap<u64, Motion>,
    looks: HashMap<u64, Look>,
    ghosts: Vec<Ghost>,
    tab: Option<Motion>,
    laid: Option<f32>,
    width: f32,
    press: Option<Press>,
    scroll: Option<Motion>,
    shown: Option<(u64, usize)>,
    carried: Option<std::time::Instant>,
}

impl State {
    fn scrolled(&self) -> f32 {
        self.scroll.map_or(0.0, |scroll| scroll.value(Speed::SLIDE))
    }

    fn aim(&self) -> f32 {
        self.scroll.map_or(0.0, |scroll| scroll.target())
    }

    fn scroll_to(&mut self, target: f32, limit: f32, glide: bool) -> bool {
        let target = target.clamp(0.0, limit.max(0.0));
        let Some(scroll) = &mut self.scroll else {
            self.scroll = Some(Motion::at(target));
            return target > 0.0;
        };
        if glide {
            return scroll.to(target, Speed::SLIDE);
        }
        let moved = (scroll.value(Speed::SLIDE) - target).abs() > 0.01;
        scroll.jump(target);
        moved
    }

    fn reveal(&mut self, active: Option<(u64, usize)>, tab: f32, width: f32, limit: f32) -> bool {
        let aim = self.aim();
        let mut wanted = aim;
        if self.shown != active {
            self.shown = active;
            if let Some((_, index)) = active {
                let left = slot_x(index, tab) - PAD;
                let right = slot_x(index, tab) + tab + PAD;
                if left < wanted {
                    wanted = left;
                } else if right > wanted + width {
                    wanted = right - width;
                }
            }
        }
        let wanted = wanted.clamp(0.0, limit.max(0.0));
        if self.scroll.is_some() && (wanted - aim).abs() < 0.01 {
            return false;
        }
        self.scroll_to(wanted, limit, true)
    }

    fn shown_tab(&mut self, target: f32, width: f32) -> f32 {
        let resized = self.laid.is_some_and(|laid| (laid - width).abs() > 0.5);
        self.laid = Some(width);
        let motion = self.tab.get_or_insert_with(|| Motion::at(target));
        if resized {
            motion.jump(target);
        } else {
            motion.to(target, Speed::SLIDE);
        }
        motion.value(Speed::SLIDE)
    }

    fn track(&mut self, active: Option<u64>) -> bool {
        let first = self.selected.is_empty();
        let mut started = false;
        for id in &self.ids {
            let target = f32::from(Some(*id) == active);
            let motion = self.selected.entry(*id).or_insert_with(|| Motion::at(if first { target } else { 0.0 }));
            started |= motion.to(target, Speed::COLOR);
        }
        self.selected.retain(|id, _| self.ids.contains(id));
        self.ghosts.retain(|ghost| ghost.fade.animating(Speed::SLIDE));
        started
    }

    fn retire(&mut self, gone: &[u64]) {
        for id in gone {
            let Some(look) = self.looks.remove(id) else { continue };
            let x = self.slides.get(id).map_or(PAD, |slide| slide.value(Speed::SLIDE));
            let width = self.tab.map_or(0.0, |tab| tab.value(Speed::SLIDE));
            let lit = self.selected.get(id).map_or(0.0, |lit| lit.value(Speed::COLOR));
            let mut fade = Motion::at(1.0);
            fade.to(0.0, Speed::SLIDE);
            self.ghosts.push(Ghost { look, x, width, lit, fade });
        }
    }

    fn moving(&self) -> bool {
        self.animating()
            || !self.ghosts.is_empty()
            || self.scroll.is_some_and(|scroll| scroll.animating(Speed::SLIDE))
            || self.tab.is_some_and(|tab| tab.animating(Speed::SLIDE))
            || self.selected.values().any(|lit| lit.animating(Speed::COLOR))
    }

    fn lifted(&self) -> Option<Press> {
        self.press.filter(|press| press.moved)
    }

    fn settle(&mut self, ids: &[u64], slots: &[f32], width: f32, tab: f32) -> bool {
        let resized = (self.width - width).abs() > 0.5;
        self.width = width;
        let lifted = self.lifted();
        let scroll = self.scrolled();
        let mut started = false;
        for (id, x) in ids.iter().zip(slots) {
            let slide = self.slides.entry(*id).or_insert_with(|| Motion::at(*x));
            match lifted {
                Some(press) if press.id == *id => slide.jump(press.left(tab, ids.len(), scroll)),
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
    looks: Vec<Look>,
    active: Option<u64>,
    height: f32,
    widest: f32,
    narrowest: f32,
    ground: Color,
    select: Option<Pick<'a, M>>,
    reorder: Option<Reorder<'a, M>>,
}

impl<M> Strip<'_, M> {
    fn tab(&self, layout: Layout<'_>) -> f32 {
        tab_size(layout.bounds().width, self.ids.len(), self.widest, self.narrowest)
    }

    fn carry(&self, press: &mut Press, tab: f32, scroll: f32, shell: &mut Shell<'_, M>) {
        let count = self.ids.len();
        let target = slot_at(press.left(tab, count, scroll), tab, count);
        if target != press.at
            && let Some(reorder) = &self.reorder
        {
            shell.publish(reorder(press.at, target));
            press.at = target;
        }
    }

    fn fades(&self, renderer: &mut Renderer, bounds: Rectangle, scroll: f32, limit: f32) {
        let ground = self.ground.scale_alpha(self.paint.alpha);
        let mut edge = |x: f32, hidden: f32, leading: bool| {
            let strength = (hidden / FADE).clamp(0.0, 1.0);
            if strength <= 0.0 {
                return;
            }
            let (from, to) = if leading { (strength, 0.0) } else { (0.0, strength) };
            let gradient = iced::gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2))
                .add_stop(0.0, ground.scale_alpha(from))
                .add_stop(1.0, ground.scale_alpha(to));
            renderer.fill_quad(
                Quad { bounds: Rectangle { x, width: FADE, ..bounds }, ..Quad::default() },
                iced::Background::Gradient(iced::Gradient::Linear(gradient)),
            );
        };
        edge(bounds.x, scroll, true);
        edge(bounds.x + bounds.width - FADE, limit - scroll, false);
    }

    fn pill(&self, renderer: &mut Renderer, bounds: Rectangle, fill: Color, level: f32, elevation: Level) {
        if level <= 0.0 {
            return;
        }
        let alpha = level.min(1.0);
        for layer in elevation::layers(elevation, self.paint.tokens.appearance) {
            renderer.fill_quad(
                Quad {
                    bounds,
                    border: Border { radius: R2.into(), ..Border::default() },
                    shadow: iced::Shadow {
                        color: style::to_iced(layer.color).scale_alpha(self.paint.alpha * alpha),
                        offset: Vector::new(layer.dx, layer.dy),
                        blur_radius: layer.blur,
                    },
                    ..Quad::default()
                },
                Color::TRANSPARENT,
            );
        }
        renderer.fill_quad(
            Quad { bounds, border: Border { radius: R2.into(), ..Border::default() }, ..Quad::default() },
            fill.scale_alpha(alpha),
        );
    }

    fn ghost(&self, renderer: &mut Renderer, ghost: &Ghost, origin: Point) {
        let alpha = ghost.fade.value(Speed::SLIDE).clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        let bounds = Rectangle {
            x: origin.x + ghost.x,
            y: origin.y + SP1,
            width: ghost.width,
            height: (self.height - 2.0 * SP1).max(0.0),
        };
        let centre = bounds.center();
        let shrink = ARRIVE + (1.0 - ARRIVE) * alpha;
        let transform = Transformation::translate(centre.x, centre.y)
            * Transformation::scale(shrink)
            * Transformation::translate(-centre.x, -centre.y);
        renderer.with_transformation(transform, |renderer| {
            self.pill(renderer, bounds, ghost.look.fill, ghost.lit * alpha, Level::El1);
            let style = TextStyle::CAP;
            renderer.fill_text(
                typeset::Text {
                    content: ghost.look.label.clone(),
                    bounds: bounds.size(),
                    size: Pixels(style.size_px),
                    line_height: typeset::LineHeight::Absolute(Pixels(style.line_height_px)),
                    font: Font::DEFAULT,
                    align_x: typeset::Alignment::Center,
                    align_y: Vertical::Center,
                    shaping: typeset::Shaping::Advanced,
                    wrapping: typeset::Wrapping::None,
                },
                bounds.position(),
                self.paint.fg_2().scale_alpha(alpha),
                bounds,
            );
        });
    }

    #[allow(clippy::too_many_arguments)] // Widget::draw's own arguments plus the layer the lifted tab is clipped to
    fn paint_tabs(
        &self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>,
        cursor: mouse::Cursor, viewport: &Rectangle, lifted_clip: Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let origin = layout.bounds().x;
        let lifted = state.lifted().map(|press| press.id);
        let mut top = None;
        for ghost in &state.ghosts {
            self.ghost(renderer, ghost, layout.bounds().position());
        }
        for ((((face, tree), layout), id), look) in
            self.faces.iter().zip(&tree.children).zip(layout.children()).zip(&self.ids).zip(&self.looks)
        {
            let offset = state.offset(*id, layout.bounds().x - origin);
            if lifted == Some(*id) {
                top = Some((face, tree, layout, offset, look.fill));
                continue;
            }
            let lit = state.selected.get(id).map_or(0.0, |lit| lit.value(Speed::COLOR));
            renderer.with_translation(Vector::new(offset, 0.0), |renderer| {
                self.pill(renderer, layout.bounds(), look.fill, lit, Level::El1);
                face.as_widget().draw(tree, renderer, theme, style, layout, cursor, viewport);
            });
        }
        let Some((face, tree, layout, offset, fill)) = top else { return };
        renderer.with_layer(lifted_clip, |renderer| {
            renderer.with_translation(Vector::new(offset, 0.0), |renderer| {
                self.pill(renderer, layout.bounds(), fill, 1.0, Level::El2);
                face.as_widget().draw(tree, renderer, theme, style, layout, cursor, viewport);
            });
        });
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
        for (id, look) in self.ids.iter().zip(&self.looks) {
            state.looks.insert(*id, look.clone());
        }
        if state.ids != self.ids {
            let gone: Vec<u64> = state.ids.iter().copied().filter(|id| !self.ids.contains(id)).collect();
            state.retire(&gone);
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
        let target = tab_size(width, count, self.widest, self.narrowest);
        let tab = tree.state.downcast_mut::<State>().shown_tab(target, width);
        let inner = (self.height - 2.0 * SP1).max(0.0);
        let fixed = layout::Limits::new(Size::new(tab, inner), Size::new(tab, inner));
        let children = self
            .faces
            .iter_mut()
            .zip(&mut tree.children)
            .enumerate()
            .map(|(index, (face, tree))| {
                face.as_widget_mut().layout(tree, renderer, &fixed).move_to(Point::new(slot_x(index, target), SP1))
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
        let bounds = layout.bounds();
        let tab = self.tab(layout);
        let count = self.ids.len();
        let limit = overflow(bounds.width, count, tab);
        let scroll = state.scrolled();
        let loose = limit > 0.0 || scroll > 0.0;
        let inner =
            if state.lifted().is_some() { mouse::Cursor::Unavailable } else { within(cursor, bounds, scroll, loose) };
        let view = if loose { Rectangle { x: bounds.x + scroll, ..bounds } } else { *viewport };
        for ((face, tree), layout) in self.faces.iter_mut().zip(&mut tree.children).zip(layout.children()) {
            face.as_widget_mut().update(tree, event, layout, inner, renderer, clipboard, shell, &view);
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if !shell.is_event_captured() => {
                let pressed = cursor.position_over(bounds).and_then(|position| {
                    let inside = Point::new(position.x + scroll, position.y);
                    layout.children().position(|child| child.bounds().contains(inside)).map(|at| (at, position.x))
                });
                if let Some((at, x)) = pressed {
                    let x = x - bounds.x;
                    state.press = Some(Press {
                        id: self.ids[at],
                        at,
                        origin: x + scroll,
                        grab: x + scroll - slot_x(at, tab),
                        cursor: x,
                        moved: false,
                    });
                    state.shown = None;
                    if let Some(select) = &self.select {
                        shell.publish(select(at));
                    }
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(mut press) = state.press {
                    press.cursor = position.x - bounds.x;
                    press.moved |= (press.cursor + scroll - press.origin).abs() >= SLOP;
                    if press.moved {
                        self.carry(&mut press, tab, scroll, shell);
                        shell.request_redraw();
                    }
                    state.press = Some(press);
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left) | mouse::Event::CursorLeft)
                if state.press.take().is_some_and(|press| press.moved) =>
            {
                state.carried = None;
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if limit > 0.0 && !shell.is_event_captured() && cursor.is_over(bounds) =>
            {
                let (by, glide) = match *delta {
                    mouse::ScrollDelta::Lines { x, y } => (-(if x.abs() > y.abs() { x } else { y }) * NOTCH, true),
                    mouse::ScrollDelta::Pixels { x, y } => (-(if x.abs() > y.abs() { x } else { y }), false),
                };
                if state.scroll_to(state.aim() + by, limit, glide) {
                    shell.request_redraw();
                }
                shell.capture_event();
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                let pull = state.lifted().filter(|_| limit > 0.0).map_or(0.0, |press| {
                    if press.cursor < EDGE {
                        press.cursor - EDGE
                    } else if press.cursor > bounds.width - EDGE {
                        press.cursor - (bounds.width - EDGE)
                    } else {
                        0.0
                    }
                });
                if pull == 0.0 {
                    state.carried = None;
                } else if let Some(mut press) = state.lifted() {
                    let elapsed = state.carried.map_or(0.0, |last| now.duration_since(last).as_secs_f32().min(0.05));
                    state.carried = Some(*now);
                    state.scroll_to(state.aim() + (pull / EDGE).clamp(-1.0, 1.0) * CARRY * elapsed, limit, false);
                    self.carry(&mut press, tab, state.scrolled(), shell);
                    state.press = Some(press);
                    shell.request_redraw();
                }
            }
            _ => {}
        }
        let started = state.settle(&self.ids, &Self::slots(layout), bounds.width, tab);
        let selecting = state.track(self.active);
        let active = self.active.and_then(|id| self.ids.iter().position(|known| *known == id).map(|at| (id, at)));
        let revealing = state.lifted().is_none() && state.reveal(active, tab, bounds.width, limit);
        let redraw = matches!(event, Event::Window(window::Event::RedrawRequested(_)));
        if state.tab.is_some_and(|tab| tab.animating(Speed::SLIDE)) && redraw {
            shell.invalidate_layout();
        }
        if started || selecting || revealing || (state.moving() && redraw) {
            shell.request_redraw();
        }
    }

    fn draw(
        &self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>,
        cursor: mouse::Cursor, viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let scroll = state.scrolled();
        let limit = overflow(bounds.width, self.ids.len(), self.tab(layout));
        if limit <= 0.0 && scroll <= 0.0 {
            self.paint_tabs(tree, renderer, theme, style, layout, cursor, viewport, *viewport);
            return;
        }
        let inner = within(cursor, bounds, scroll, true);
        let clip = Rectangle { x: bounds.x + scroll, ..bounds };
        renderer.with_layer(bounds, |renderer| {
            renderer.with_translation(Vector::new(-scroll, 0.0), |renderer| {
                self.paint_tabs(tree, renderer, theme, style, layout, inner, &clip, clip);
            });
        });
        renderer.with_layer(bounds, |renderer| self.fades(renderer, bounds, scroll, limit));
    }

    fn mouse_interaction(
        &self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.lifted().is_some() {
            return mouse::Interaction::Grabbing;
        }
        let bounds = layout.bounds();
        let scroll = state.scrolled();
        let loose = scroll > 0.0 || overflow(bounds.width, self.ids.len(), self.tab(layout)) > 0.0;
        let cursor = within(cursor, bounds, scroll, loose);
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
        let scroll = tree.state.downcast_ref::<State>().scrolled();
        let translation = translation - Vector::new(scroll, 0.0);
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
        Close(usize),
        Menu(usize),
        Hover,
    }

    struct Bench {
        paint: Paint,
        order: Vec<u64>,
        active: usize,
        closable: bool,
        tree: Tree,
    }

    impl Bench {
        const SIZE: Size = Size::new(600.0, 36.0);

        fn new(order: Vec<u64>) -> Self {
            let paint = Paint::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue);
            let mut bench = Self { paint, order, active: 0, closable: false, tree: Tree::empty() };
            bench.tree = Tree::new(bench.strip());
            bench
        }

        fn strip(&self) -> Element<'static, Sent> {
            let tabs = self.order.iter().map(|id| Tab::new(*id, format!("tab {id}"))).collect();
            let mut strip = tab_strip(&self.paint, tabs, self.active)
                .on_select(Sent::Select)
                .on_reorder(Sent::Reorder)
                .hovered(None, |_| Sent::Hover)
                .bare();
            if self.closable {
                strip = strip.on_close(Sent::Close, "close").on_menu(Sent::Menu);
            }
            strip.into()
        }

        fn send(&mut self, event: &Event, x: f32) -> Vec<Sent> {
            let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0)));
            let mut element = self.strip();
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
            let mut element = self.strip();
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
        assert!((press(1, tab, 300.0).left(tab, 4, 0.0) - 290.0).abs() < 0.01);
        assert_eq!(press(0, tab, -400.0).left(tab, 4, 0.0), PAD);
        assert_eq!(press(0, tab, 9000.0).left(tab, 4, 0.0), slot_x(3, tab));
        assert_eq!(press(0, tab, 300.0).left(tab, 1, 0.0), PAD);
        assert!((press(1, tab, 300.0).left(tab, 9, 250.0) - 540.0).abs() < 0.01, "a scrolled strip adds its offset");
    }

    #[test]
    fn tabs_stop_shrinking_at_the_narrowest_and_the_strip_scrolls_instead() {
        assert!((tab_size(600.0, 3, f32::INFINITY, NARROWEST) - tab_width(600.0, 3, f32::INFINITY)).abs() < 0.01);
        assert_eq!(tab_size(600.0, 40, f32::INFINITY, NARROWEST), NARROWEST);
        assert_eq!(tab_size(600.0, 40, 80.0, NARROWEST), 80.0, "the widest still wins over the narrowest");
        assert_eq!(overflow(600.0, 3, 190.0), 0.0);
        assert!((overflow(600.0, 40, NARROWEST) - (run(40, NARROWEST) - 600.0)).abs() < 0.01);
        assert!(tabs_fit(600.0, 4) && !tabs_fit(600.0, 5));
        assert_eq!(run(0, NARROWEST), 0.0);
    }

    fn redraw() -> Event {
        Event::Window(window::Event::RedrawRequested(std::time::Instant::now()))
    }

    #[test]
    fn a_crowded_strip_opens_on_the_active_tab_and_the_wheel_scrolls_it_within_its_run() {
        let mut bench = Bench::new((1..=40).collect());
        bench.active = 30;
        bench.send(&redraw(), 300.0);
        let limit = overflow(Bench::SIZE.width, 40, NARROWEST);
        let scroll = bench.state().scrolled();
        let left = slot_x(30, NARROWEST);
        assert!(scroll <= left && left + NARROWEST <= scroll + Bench::SIZE.width, "tab 30 is in view at {scroll}");
        let wheel =
            |y: f32| Event::Mouse(mouse::Event::WheelScrolled { delta: mouse::ScrollDelta::Pixels { x: 0.0, y } });
        bench.send(&wheel(-100_000.0), 300.0);
        assert!((bench.state().scrolled() - limit).abs() < 0.01, "the wheel stops at the last tab");
        bench.send(&wheel(100_000.0), 300.0);
        assert_eq!(bench.state().scrolled(), 0.0, "and at the first");
        bench.send(&wheel(-150.0), 300.0);
        assert!((bench.state().scrolled() - 150.0).abs() < 0.01);
        let pressed = bench.send(&Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), 20.0);
        assert_eq!(pressed, vec![Sent::Select(1)], "a press lands on the tab that is drawn under it");
        bench.send(&Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)), 20.0);
        bench.draw(20.0);
        bench.active = 0;
        bench.send(&redraw(), 300.0);
        assert_eq!(bench.state().aim(), 0.0, "selecting a hidden tab brings it into view");
        bench.active = 30;
        bench.send(&redraw(), 300.0);
        bench.order.drain(..10);
        bench.active = 20;
        bench.send(&redraw(), 300.0);
        let (aim, left) = (bench.state().aim(), slot_x(20, NARROWEST));
        assert!(aim <= left && left + NARROWEST <= aim + Bench::SIZE.width, "tabs closed before it: {aim}");
    }

    #[test]
    fn a_strip_that_fits_ignores_the_wheel() {
        let mut bench = Bench::new(vec![1, 2, 3]);
        let wheel = Event::Mouse(mouse::Event::WheelScrolled { delta: mouse::ScrollDelta::Lines { x: 0.0, y: -3.0 } });
        bench.send(&wheel, 300.0);
        assert_eq!(bench.state().scrolled(), 0.0);
    }

    #[test]
    fn the_close_button_of_a_crowded_strip_closes_its_tab_without_selecting_it() {
        let mut bench = Bench::new((1..=40).collect());
        bench.closable = true;
        bench.active = 2;
        bench.send(&redraw(), 300.0);
        let on_close = slot_x(2, NARROWEST) - bench.state().scrolled() + SP1 + CLOSE / 2.0;
        bench.moved(on_close);
        let pressed = bench.send(&Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), on_close);
        assert!(pressed.is_empty(), "the button takes the press: {pressed:?}");
        let released = bench.send(&Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)), on_close);
        assert_eq!(released, vec![Sent::Close(2)]);
        let menu = bench.send(&Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)), on_close + 40.0);
        assert_eq!(menu, vec![Sent::Menu(2)]);
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
    fn a_closed_tab_leaves_a_ghost_that_fades_out() {
        let mut state = State { ids: vec![1, 2], ..State::default() };
        state.looks.insert(2, Look { label: String::from("two"), fill: Color::WHITE });
        state.settle(&[1, 2], &[PAD, PAD + 104.0], 216.0, 100.0);
        state.retire(&[2]);
        assert_eq!(state.ghosts.len(), 1);
        assert!(state.ghosts[0].fade.animating(Speed::SLIDE));
        assert!(state.moving());
        state.retire(&[9]);
        assert_eq!(state.ghosts.len(), 1, "an unknown tab leaves nothing behind");
    }

    #[test]
    fn a_new_tab_fades_its_selection_in_instead_of_snapping() {
        let mut state = State { ids: vec![1], ..State::default() };
        assert!(!state.track(Some(1)), "the first population starts settled");
        state.ids = vec![1, 2];
        assert!(state.track(Some(2)));
        assert_eq!(state.selected[&2].target(), 1.0);
        assert_eq!(state.selected[&1].target(), 0.0);
        assert!(state.selected[&2].value(Speed::COLOR) < 0.5);
    }

    #[test]
    fn the_tab_width_glides_when_a_tab_is_added_but_snaps_on_resize() {
        let mut state = State::default();
        assert_eq!(state.shown_tab(200.0, 600.0), 200.0);
        assert!(state.shown_tab(100.0, 600.0) > 100.0);
        assert!(state.tab.is_some_and(|tab| tab.animating(Speed::SLIDE)));
        assert_eq!(state.shown_tab(150.0, 500.0), 150.0);
        assert!(!state.tab.is_some_and(|tab| tab.animating(Speed::SLIDE)));
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

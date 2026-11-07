use std::time::{Duration, Instant};

use glide_iced::style::{R2, R3, SP1, SP2};
use glide_theme::TextStyle;
use glide_theme::shape::R1;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, overlay};
use iced::widget::{Space, column, container, image, mouse_area, row, slider};
use iced::{
    Alignment, Background, Border, Color, Element, Event, Fill, Length, Point, Rectangle, Renderer, Shadow, Size,
    Theme, Vector, mouse,
};

use super::gallery::{GLASS_INK, glass_style};
use super::{Wrapping, text};

pub const SCRUB_HEIGHT: f32 = 20.0;
pub const TRIM_HEIGHT: f32 = 60.0;
pub const GRIP: f32 = 16.0;
pub const PREVIEW: Size = Size { width: 176.0, height: 99.0 };
const RAIL: f32 = 4.0;
const RAIL_HOVER: f32 = 6.0;
const KNOB: f32 = 12.0;
const SLOP: f32 = 6.0;
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const TICK: Color = Color { r: 20.0 / 255.0, g: 19.0 / 255.0, b: 22.0 / 255.0, a: 0.85 };
const OUTSIDE: Color = Color { r: 10.0 / 255.0, g: 10.0 / 255.0, b: 12.0 / 255.0, a: 0.66 };
const HALO: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.55 };

fn white(alpha: f32) -> Color {
    Color { a: alpha, ..Color::WHITE }
}

#[must_use]
pub fn clock(at: Duration, hours: bool) -> String {
    let total = at.as_secs();
    let (h, m, s) = (total / 3600, total % 3600 / 60, total % 60);
    if hours || h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

fn quad(renderer: &mut Renderer, bounds: Rectangle, radius: f32, color: Color) {
    renderer.fill_quad(
        Quad { bounds, border: Border { radius: radius.into(), ..Border::default() }, ..Quad::default() },
        color,
    );
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub at: f32,
    pub title: String,
}

#[must_use]
pub fn mark_at(marks: &[Mark], at: f32) -> Option<&Mark> {
    marks.iter().rfind(|mark| mark.at <= at)
}

type Hover<'a, M> = Box<dyn Fn(Option<u32>) -> M + 'a>;
type Seek<'a, M> = Box<dyn Fn(f32) -> M + 'a>;

pub struct Scrubber<'a, M> {
    position: f32,
    buffered: f32,
    marks: Vec<Mark>,
    duration: Duration,
    buckets: u32,
    preview: Option<image::Handle>,
    on_hover: Hover<'a, M>,
    on_seek: Seek<'a, M>,
    on_release: Option<M>,
}

#[derive(Debug, Default)]
struct Scrub {
    hover: Option<f32>,
    bucket: Option<u32>,
    dragging: bool,
    sent: Option<f32>,
    bubble: Option<Tree>,
}

#[must_use]
pub fn scrubber<'a, M: 'a>(
    position: f32, duration: Duration, on_hover: impl Fn(Option<u32>) -> M + 'a, on_seek: impl Fn(f32) -> M + 'a,
) -> Scrubber<'a, M> {
    Scrubber {
        position: position.clamp(0.0, 1.0),
        buffered: 0.0,
        marks: Vec::new(),
        duration,
        buckets: 200,
        preview: None,
        on_hover: Box::new(on_hover),
        on_seek: Box::new(on_seek),
        on_release: None,
    }
}

impl<'a, M: 'a> Scrubber<'a, M> {
    #[must_use]
    pub fn buffered(mut self, buffered: f32) -> Self {
        self.buffered = buffered.clamp(0.0, 1.0);
        self
    }

    #[must_use]
    pub fn marks(mut self, marks: Vec<Mark>) -> Self {
        self.marks = marks;
        self
    }

    #[must_use]
    pub fn buckets(mut self, buckets: u32) -> Self {
        self.buckets = buckets.max(1);
        self
    }

    #[must_use]
    pub fn preview(mut self, frame: Option<image::Handle>) -> Self {
        self.preview = frame;
        self
    }

    #[must_use]
    pub fn on_release(mut self, message: M) -> Self {
        self.on_release = Some(message);
        self
    }

    fn fraction(bounds: Rectangle, x: f32) -> f32 {
        ((x - bounds.x) / bounds.width.max(1.0)).clamp(0.0, 1.0)
    }

    fn bubble(&self, at: f32) -> Element<'a, M> {
        let picture: Element<'a, M> = match &self.preview {
            Some(handle) => image(handle.clone())
                .width(PREVIEW.width)
                .height(PREVIEW.height)
                .content_fit(iced::ContentFit::Cover)
                .border_radius(6.0)
                .into(),
            None => container(Space::new().width(PREVIEW.width).height(PREVIEW.height))
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::from_rgb8(18, 17, 20))),
                    border: Border { radius: 6.0.into(), ..Border::default() },
                    ..container::Style::default()
                })
                .into(),
        };
        let title = mark_at(&self.marks, at).map_or_else(String::new, |mark| mark.title.clone());
        let time = clock(self.duration.mul_f32(at), self.duration >= Duration::from_hours(1));
        let meta = row![
            text(title, TextStyle { size_px: 11.5, line_height_px: 14.0, ..TextStyle::CAP })
                .color(Color { a: 0.8, ..GLASS_INK })
                .wrapping(Wrapping::None),
            Space::new().width(Fill),
            text(time, TextStyle { size_px: 11.5, line_height_px: 14.0, ..TextStyle::NUM }).color(GLASS_INK),
        ]
        .spacing(SP2)
        .padding([0.0, SP1]);
        container(column![picture, meta].spacing(SP1).width(PREVIEW.width))
            .padding(SP1)
            .style(|_| container::Style {
                border: Border { radius: 10.0.into(), ..glass_style().border },
                ..glass_style()
            })
            .into()
    }
}

impl<'a, M: Clone + 'a> Widget<M, Theme, Renderer> for Scrubber<'a, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Scrub>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Scrub::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Fill, Length::Fixed(SCRUB_HEIGHT))
    }

    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(Fill, SCRUB_HEIGHT, Size::ZERO))
    }

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, _: &Renderer,
        _: &mut dyn Clipboard, shell: &mut Shell<'_, M>, _: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<Scrub>();
        let bounds = layout.bounds();
        let zone = bounds.expand(iced::Padding::from([SP1, 0.0]));
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(at) = cursor.position() else { return };
                let fraction = Self::fraction(bounds, at.x);
                if state.dragging {
                    if state.sent.is_none_or(|sent| (sent - fraction).abs() > 0.0005) {
                        state.sent = Some(fraction);
                        shell.publish((self.on_seek)(fraction));
                    }
                    state.hover = Some(fraction);
                    shell.request_redraw();
                    return;
                }
                if zone.contains(at) {
                    state.hover = Some(fraction);
                    let bucket = ((fraction * self.buckets as f32) as u32).min(self.buckets - 1);
                    if state.bucket != Some(bucket) {
                        state.bucket = Some(bucket);
                        shell.publish((self.on_hover)(Some(bucket)));
                    }
                    shell.request_redraw();
                } else if state.hover.take().is_some() {
                    state.bucket = None;
                    shell.publish((self.on_hover)(None));
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::CursorLeft) if state.hover.is_some() && !state.dragging => {
                state.hover = None;
                state.bucket = None;
                shell.publish((self.on_hover)(None));
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(at) = cursor.position().filter(|at| zone.contains(*at)) {
                    let fraction = Self::fraction(bounds, at.x);
                    state.dragging = true;
                    state.sent = Some(fraction);
                    shell.publish((self.on_seek)(fraction));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging => {
                state.dragging = false;
                state.sent = None;
                if let Some(message) = self.on_release.clone() {
                    shell.publish(message);
                }
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn draw(
        &self, tree: &Tree, renderer: &mut Renderer, _: &Theme, _: &renderer::Style, layout: Layout<'_>,
        _: mouse::Cursor, _: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<Scrub>();
        let bounds = layout.bounds();
        let thick = if state.hover.is_some() || state.dragging { RAIL_HOVER } else { RAIL };
        let rail = Rectangle { x: bounds.x, y: bounds.center_y() - thick / 2.0, width: bounds.width, height: thick };
        let part = |fraction: f32| Rectangle { width: rail.width * fraction, ..rail };
        quad(renderer, rail, 2.0, white(0.2));
        quad(renderer, part(self.buffered.max(self.position)), 2.0, white(0.3));
        quad(renderer, part(self.position), 2.0, GLASS_INK);
        for mark in self.marks.iter().filter(|mark| mark.at > 0.0 && mark.at < 1.0) {
            quad(renderer, Rectangle { x: rail.x + rail.width * mark.at - 1.0, width: 2.0, ..rail }, 0.0, TICK);
        }
        if let Some(hover) = state.hover.filter(|_| !state.dragging) {
            let x = rail.x + rail.width * hover;
            quad(
                renderer,
                Rectangle { x: x - 1.0, y: bounds.y + 4.0, width: 2.0, height: bounds.height - 8.0 },
                1.0,
                white(0.8),
            );
        }
        let x = rail.x + rail.width * self.position;
        renderer.fill_quad(
            Quad {
                bounds: Rectangle { x: x - KNOB / 2.0, y: bounds.center_y() - KNOB / 2.0, width: KNOB, height: KNOB },
                border: Border { radius: (KNOB / 2.0).into(), ..Border::default() },
                shadow: Shadow {
                    color: Color { a: 0.5, ..Color::BLACK },
                    offset: Vector::new(0.0, 1.0),
                    blur_radius: 3.0,
                },
                snap: true,
            },
            Color::WHITE,
        );
    }

    fn mouse_interaction(
        &self, _: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, _: &Rectangle, _: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds().expand(iced::Padding::from([SP1, 0.0]))) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }

    fn overlay<'b>(
        &'b mut self, tree: &'b mut Tree, layout: Layout<'b>, _: &Renderer, _: &Rectangle, translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        let state = tree.state.downcast_mut::<Scrub>();
        let hover = state.hover?;
        let element = self.bubble(hover);
        let tree = state.bubble.get_or_insert_with(|| Tree::new(&element));
        tree.diff(&element);
        let bounds = layout.bounds() + translation;
        let anchor = Point::new(bounds.x + bounds.width * hover, bounds.y - SP2);
        Some(overlay::Element::new(Box::new(Bubble { element, tree, anchor })))
    }
}

impl<'a, M: Clone + 'a> From<Scrubber<'a, M>> for Element<'a, M> {
    fn from(scrubber: Scrubber<'a, M>) -> Self {
        Element::new(scrubber)
    }
}

struct Bubble<'b, M> {
    element: Element<'b, M>,
    tree: &'b mut Tree,
    anchor: Point,
}

impl<M> overlay::Overlay<M, Theme, Renderer> for Bubble<'_, M> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let limits = layout::Limits::new(Size::ZERO, bounds);
        let node = self.element.as_widget_mut().layout(self.tree, renderer, &limits);
        let size = node.size();
        let x = (self.anchor.x - size.width / 2.0).clamp(SP2, (bounds.width - size.width - SP2).max(SP2));
        let y = (self.anchor.y - size.height).max(SP2);
        node.move_to(Point::new(x, y))
    }

    fn draw(
        &self, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let bounds = layout.bounds();
        self.element.as_widget().draw(self.tree, renderer, theme, style, layout, cursor, &bounds);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trim {
    Start(f32),
    End(f32),
    Playhead(f32),
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grip {
    Start,
    End,
}

#[derive(Debug, Default)]
struct Trimming {
    grip: Option<Grip>,
    badges: Vec<Tree>,
}

pub struct TrimBar<'a, M> {
    frames: Vec<Option<image::Handle>>,
    start: f32,
    end: f32,
    playhead: f32,
    least: f32,
    labels: (String, String),
    accent: Color,
    on_change: Box<dyn Fn(Trim) -> M + 'a>,
}

#[must_use]
pub fn trim_bar<'a, M: 'a>(
    frames: Vec<Option<image::Handle>>, (start, end): (f32, f32), playhead: f32, labels: (String, String),
    accent: Color, on_change: impl Fn(Trim) -> M + 'a,
) -> TrimBar<'a, M> {
    TrimBar { frames, start, end, playhead, least: 0.01, labels, accent, on_change: Box::new(on_change) }
}

impl<'a, M: 'a> TrimBar<'a, M> {
    #[must_use]
    pub fn least(mut self, least: f32) -> Self {
        self.least = least.clamp(0.0, 0.5);
        self
    }

    fn strip(bounds: Rectangle) -> Rectangle {
        Rectangle { x: bounds.x + GRIP, width: (bounds.width - 2.0 * GRIP).max(1.0), ..bounds }
    }

    fn at(strip: Rectangle, x: f32) -> f32 {
        ((x - strip.x) / strip.width).clamp(0.0, 1.0)
    }

    fn badge(label: String) -> Element<'a, M> {
        container(text(label, TextStyle { size_px: 11.5, line_height_px: 16.0, ..TextStyle::NUM }).color(GLASS_INK))
            .padding([2.0, 6.0])
            .style(|_| container::Style {
                border: Border { radius: 6.0.into(), ..glass_style().border },
                ..glass_style()
            })
            .into()
    }
}

impl<'a, M: 'a> Widget<M, Theme, Renderer> for TrimBar<'a, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Trimming>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Trimming::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Fill, Length::Fixed(TRIM_HEIGHT))
    }

    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(Fill, TRIM_HEIGHT, Size::ZERO))
    }

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, _: &Renderer,
        _: &mut dyn Clipboard, shell: &mut Shell<'_, M>, _: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<Trimming>();
        let bounds = layout.bounds();
        let strip = Self::strip(bounds);
        let start_x = strip.x + strip.width * self.start;
        let end_x = strip.x + strip.width * self.end;
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(at) = cursor.position().filter(|at| bounds.expand(4.0).contains(*at)) else { return };
                if (start_x - GRIP..=start_x).contains(&at.x) {
                    state.grip = Some(Grip::Start);
                } else if (end_x..=end_x + GRIP).contains(&at.x) {
                    state.grip = Some(Grip::End);
                } else {
                    let fraction = Self::at(strip, at.x).clamp(self.start, self.end);
                    shell.publish((self.on_change)(Trim::Playhead(fraction)));
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let (Some(grip), Some(at)) = (state.grip, cursor.position()) else { return };
                let fraction = Self::at(strip, at.x);
                let change = match grip {
                    Grip::Start => Trim::Start(fraction.min(self.end - self.least).max(0.0)),
                    Grip::End => Trim::End(fraction.max(self.start + self.least).min(1.0)),
                };
                let current = match grip {
                    Grip::Start => self.start,
                    Grip::End => self.end,
                };
                let moved = match change {
                    Trim::Start(value) | Trim::End(value) => (value - current).abs() > 0.0005,
                    _ => false,
                };
                if moved {
                    shell.publish((self.on_change)(change));
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.grip.is_some() => {
                state.grip = None;
                shell.publish((self.on_change)(Trim::Released));
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn draw(
        &self, _: &Tree, renderer: &mut Renderer, _: &Theme, _: &renderer::Style, layout: Layout<'_>, _: mouse::Cursor,
        _: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let strip = Self::strip(bounds);
        let count = self.frames.len().max(1);
        let slice = strip.width / count as f32;
        renderer.with_layer(strip, |renderer| {
            quad(renderer, strip, 6.0, Color::from_rgb8(18, 17, 20));
            for (index, frame) in self.frames.iter().enumerate() {
                let cell = Rectangle { x: strip.x + slice * index as f32, width: slice - 1.0, ..strip };
                if let Some(handle) = frame {
                    use iced::advanced::image::Renderer as _;
                    let picture = iced::advanced::image::Image {
                        handle: handle.clone(),
                        filter_method: iced::advanced::image::FilterMethod::Linear,
                        rotation: iced::Radians(0.0),
                        border_radius: iced::border::Radius::default(),
                        opacity: 1.0,
                        snap: true,
                    };
                    renderer.draw_image(picture, cell, cell);
                }
            }
        });
        let start_x = strip.x + strip.width * self.start;
        let end_x = strip.x + strip.width * self.end;
        quad(renderer, Rectangle { width: start_x - strip.x, ..strip }, 6.0, OUTSIDE);
        quad(renderer, Rectangle { x: end_x, width: strip.x + strip.width - end_x, ..strip }, 6.0, OUTSIDE);
        let edge = |y: f32| Rectangle { x: start_x, y, width: (end_x - start_x).max(0.0), height: 3.0 };
        for y in [bounds.y - 3.0, bounds.y + bounds.height] {
            quad(renderer, Rectangle { y: y - 2.0, height: 7.0, ..edge(y) }, 0.0, HALO);
            quad(renderer, edge(y), 0.0, self.accent);
        }
        let grips = [
            (Rectangle { x: start_x - GRIP, y: bounds.y - 3.0, width: GRIP, height: bounds.height + 6.0 }, true),
            (Rectangle { x: end_x, y: bounds.y - 3.0, width: GRIP, height: bounds.height + 6.0 }, false),
        ];
        for (grip, left) in grips {
            let radius = if left {
                iced::border::Radius::default().top_left(6.0).bottom_left(6.0)
            } else {
                iced::border::Radius::default().top_right(6.0).bottom_right(6.0)
            };
            renderer.fill_quad(
                Quad { bounds: grip.expand(2.0), border: Border { radius, ..Border::default() }, ..Quad::default() },
                HALO,
            );
            renderer.fill_quad(
                Quad { bounds: grip, border: Border { radius, ..Border::default() }, ..Quad::default() },
                self.accent,
            );
            for offset in [-3.0, 1.0] {
                quad(
                    renderer,
                    Rectangle { x: grip.center_x() + offset, y: grip.center_y() - 9.0, width: 2.0, height: 18.0 },
                    1.0,
                    white(0.92),
                );
            }
        }
        let head = strip.x + strip.width * self.playhead;
        quad(
            renderer,
            Rectangle { x: head - 2.0, y: bounds.y - 10.0, width: 4.0, height: bounds.height + 20.0 },
            2.0,
            HALO,
        );
        quad(
            renderer,
            Rectangle { x: head - 1.0, y: bounds.y - 9.0, width: 2.0, height: bounds.height + 18.0 },
            1.0,
            Color::WHITE,
        );
        quad(renderer, Rectangle { x: head - 5.0, y: bounds.y - 9.0, width: 10.0, height: 4.0 }, 2.0, Color::WHITE);
    }

    fn mouse_interaction(
        &self, _: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, _: &Rectangle, _: &Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();
        let strip = Self::strip(bounds);
        let Some(at) = cursor.position_over(bounds.expand(4.0)) else { return mouse::Interaction::None };
        let start_x = strip.x + strip.width * self.start;
        let end_x = strip.x + strip.width * self.end;
        if (start_x - GRIP..=start_x).contains(&at.x) || (end_x..=end_x + GRIP).contains(&at.x) {
            mouse::Interaction::ResizingHorizontally
        } else {
            mouse::Interaction::Pointer
        }
    }

    fn overlay<'b>(
        &'b mut self, tree: &'b mut Tree, layout: Layout<'b>, _: &Renderer, _: &Rectangle, translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        let state = tree.state.downcast_mut::<Trimming>();
        let bounds = layout.bounds() + translation;
        let strip = Self::strip(bounds);
        let badges = [(self.labels.0.clone(), self.start), (self.labels.1.clone(), self.end)];
        if state.badges.len() != 2 {
            state.badges = badges.iter().map(|(label, _)| Tree::new(Self::badge(label.clone()))).collect();
        }
        let mut children = Vec::with_capacity(2);
        for ((label, at), tree) in badges.into_iter().zip(state.badges.iter_mut()) {
            let element = Self::badge(label);
            tree.diff(&element);
            let anchor = Point::new(strip.x + strip.width * at, bounds.y - SP2 - 6.0);
            children.push(overlay::Element::new(Box::new(Bubble { element, tree, anchor })));
        }
        Some(overlay::Group::with_children(children).overlay())
    }
}

impl<'a, M: 'a> From<TrimBar<'a, M>> for Element<'a, M> {
    fn from(bar: TrimBar<'a, M>) -> Self {
        Element::new(bar)
    }
}

#[must_use]
pub fn subtitle<'a, M: 'a>(content: &str) -> Element<'a, M> {
    let style = TextStyle { size_px: 26.0, line_height_px: 36.0, weight: 560, ..TextStyle::BODY };
    let lines = content.lines().filter(|line| !line.trim().is_empty()).map(|line| {
        container(text(line.trim().to_string(), style).color(Color::WHITE).wrapping(Wrapping::None))
            .padding([2.0, 12.0])
            .style(|_| container::Style {
                background: Some(Background::Color(Color { a: 0.55, ..Color::BLACK })),
                border: Border { radius: R1.into(), ..Border::default() },
                ..container::Style::default()
            })
            .into()
    });
    iced::widget::Column::with_children(lines).align_x(Alignment::Center).into()
}

#[must_use]
pub fn glass_slider<'a, M: Clone + 'a>(value: f32, width: f32, on_change: impl Fn(f32) -> M + 'a) -> Element<'a, M> {
    slider(0.0..=1.0, value.clamp(0.0, 1.0), on_change)
        .step(0.01_f32)
        .width(width)
        .height(KNOB)
        .style(|_, _| slider::Style {
            rail: slider::Rail {
                backgrounds: (Background::Color(GLASS_INK), Background::Color(white(0.22))),
                width: RAIL,
                border: Border { radius: 2.0.into(), ..Border::default() },
            },
            handle: slider::Handle {
                shape: slider::HandleShape::Circle { radius: KNOB / 2.0 },
                background: Background::Color(Color::WHITE),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
            },
        })
        .into()
}

#[must_use]
pub fn hud<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> container::Container<'a, M> {
    container(content)
        .padding([6.0, 14.0])
        .style(|_| container::Style { border: Border { radius: R3.into(), ..glass_style().border }, ..glass_style() })
}

#[must_use]
pub fn glass_tag<'a, M: 'a>(label: &str) -> Element<'a, M> {
    container(
        text(label.to_uppercase(), TextStyle { size_px: 10.0, line_height_px: 14.0, ..TextStyle::MICRO })
            .color(GLASS_INK),
    )
    .padding([1.0, 5.0])
    .style(|_| container::Style {
        background: Some(Background::Color(white(0.16))),
        border: Border { radius: R1.into(), ..Border::default() },
        ..container::Style::default()
    })
    .into()
}

#[must_use]
pub fn glass_disc<'a, M: Clone + 'a>(
    glyph: impl super::Glyph + 'a, side: f32, danger: bool, message: Option<M>,
) -> Element<'a, M> {
    let face = iced::widget::button(container(glyph.draw(14.0, GLASS_INK)).center(side))
        .padding(0)
        .width(side)
        .height(side)
        .on_press_maybe(message)
        .style(move |_, status| {
            let hovered =
                matches!(status, iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed);
            let background = match (hovered, danger) {
                (true, true) => Color::from_rgb8(0xbf, 0x37, 0x2c),
                (true, false) => white(0.26),
                (false, _) => white(0.14),
            };
            iced::widget::button::Style {
                background: Some(Background::Color(background)),
                text_color: GLASS_INK,
                border: Border { radius: (side / 2.0).into(), ..Border::default() },
                shadow: Shadow::default(),
                snap: true,
            }
        });
    face.into()
}

#[derive(Debug, Default)]
struct Pressing {
    at: Option<(Point, Instant)>,
    last_click: Option<Instant>,
    dragged: bool,
}

pub struct Stage<'a, M> {
    content: Element<'a, M>,
    on_drag: Option<M>,
    on_double: Option<M>,
    on_click: Option<M>,
    on_right: Option<M>,
    on_move: Option<M>,
    on_leave: Option<M>,
}

#[must_use]
pub fn stage<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Stage<'a, M> {
    Stage {
        content: content.into(),
        on_drag: None,
        on_double: None,
        on_click: None,
        on_right: None,
        on_move: None,
        on_leave: None,
    }
}

impl<'a, M: Clone + 'a> Stage<'a, M> {
    #[must_use]
    pub fn on_move(mut self, message: Option<M>) -> Self {
        self.on_move = message;
        self
    }

    #[must_use]
    pub fn on_leave(mut self, message: M) -> Self {
        self.on_leave = Some(message);
        self
    }

    #[must_use]
    pub fn on_drag(mut self, message: M) -> Self {
        self.on_drag = Some(message);
        self
    }

    #[must_use]
    pub fn on_double_click(mut self, message: M) -> Self {
        self.on_double = Some(message);
        self
    }

    #[must_use]
    pub fn on_click(mut self, message: M) -> Self {
        self.on_click = Some(message);
        self
    }

    #[must_use]
    pub fn on_right_click(mut self, message: M) -> Self {
        self.on_right = Some(message);
        self
    }
}

impl<'a, M: Clone + 'a> Widget<M, Theme, Renderer> for Stage<'a, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Pressing>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Pressing::default())
    }

    forward!(content: children, diff, size, layout, operate, draw, mouse_interaction, overlay<M>);

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, viewport: &Rectangle,
    ) {
        if matches!(event, Event::Mouse(mouse::Event::CursorMoved { .. }))
            && cursor.is_over(layout.bounds())
            && let Some(message) = self.on_move.take()
        {
            shell.publish(message);
        }
        if matches!(event, Event::Mouse(mouse::Event::CursorLeft))
            && let Some(message) = self.on_leave.clone()
        {
            shell.publish(message);
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
        if shell.is_event_captured() {
            return;
        }
        let state = tree.state.downcast_mut::<Pressing>();
        let bounds = layout.bounds();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(at) = cursor.position_over(bounds) {
                    state.at = Some((at, Instant::now()));
                    state.dragged = false;
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if cursor.is_over(bounds)
                    && let Some(message) = self.on_right.clone()
                {
                    shell.publish(message);
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some((start, _)) = state.at
                    && !state.dragged
                    && start.distance(*position) > SLOP
                {
                    state.dragged = true;
                    state.at = None;
                    state.last_click = None;
                    if let Some(message) = self.on_drag.clone() {
                        shell.publish(message);
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.at.take().is_some() && !state.dragged =>
            {
                let now = Instant::now();
                let double = state.last_click.is_some_and(|last| now.duration_since(last) < DOUBLE_CLICK);
                state.last_click = if double { None } else { Some(now) };
                let message = if double { self.on_double.clone() } else { self.on_click.clone() };
                if let Some(message) = message {
                    shell.publish(message);
                }
            }
            _ => {}
        }
    }
}

impl<'a, M: Clone + 'a> From<Stage<'a, M>> for Element<'a, M> {
    fn from(stage: Stage<'a, M>) -> Self {
        Element::new(stage)
    }
}

#[must_use]
pub fn hidden_cursor<'a, M: Clone + 'a>(hidden: bool, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let area = mouse_area(content);
    if hidden { area.interaction(mouse::Interaction::Hidden).into() } else { area.into() }
}

#[must_use]
pub fn fit(frame: Size, video: Size) -> Rectangle {
    if video.width <= 0.0 || video.height <= 0.0 {
        return Rectangle::new(Point::ORIGIN, frame);
    }
    let scale = (frame.width / video.width).min(frame.height / video.height);
    let size = Size::new(video.width * scale, video.height * scale);
    Rectangle::new(Point::new((frame.width - size.width) / 2.0, (frame.height - size.height) / 2.0), size)
}

#[must_use]
pub fn radius_for(corner: f32) -> f32 {
    if corner > 0.0 { R2.max(corner) } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_switches_to_hours_only_when_asked_or_needed() {
        assert_eq!(clock(Duration::from_secs(2534), true), "0:42:14");
        assert_eq!(clock(Duration::from_secs(760), false), "12:40");
        assert_eq!(clock(Duration::from_secs(760), true), "0:12:40");
        assert_eq!(clock(Duration::from_secs(3725), false), "1:02:05");
        assert_eq!(clock(Duration::ZERO, false), "0:00");
    }

    #[test]
    fn marks_resolve_to_the_last_started() {
        let marks = vec![Mark { at: 0.0, title: "a".into() }, Mark { at: 0.4, title: "b".into() }];
        assert_eq!(mark_at(&marks, 0.39).map(|mark| mark.title.as_str()), Some("a"));
        assert_eq!(mark_at(&marks, 0.4).map(|mark| mark.title.as_str()), Some("b"));
        assert!(mark_at(&[], 0.4).is_none());
    }

    #[test]
    fn fit_letterboxes_inside_the_frame() {
        let rect = fit(Size::new(1280.0, 800.0), Size::new(1920.0, 1080.0));
        assert_eq!(rect.width, 1280.0);
        assert_eq!(rect.height, 720.0);
        assert_eq!(rect.y, 40.0);
        let tall = fit(Size::new(1280.0, 720.0), Size::new(1080.0, 1920.0));
        assert_eq!(tall.height, 720.0);
        assert!((tall.x - (1280.0 - 405.0) / 2.0).abs() < 0.01);
    }
}

use std::rc::Rc;

use glide_iced::Paint;
use glide_iced::style::{self, R2, R3, SP1, SP2, SP3};
use glide_theme::TextStyle;
use glide_theme::elevation::Level;
use glide_theme::shape::{R_ROUND, R1};
use iced::widget::canvas::{self, Action, Frame, Geometry, LineCap, LineJoin, Path, Stroke};
use iced::widget::{Space, button, canvas as surface, column, container, image, row, stack};
use iced::{
    Alignment, Background, Border, Color, ContentFit, Element, Event, Fill, Length, Padding, Point, Rectangle,
    Renderer, Shadow, Size, Theme, mouse,
};

use super::{Icon, Wrapping, icon, text, tooltip};
use crate::anim::{Speed, animate};

pub const PAGE_GAP: f32 = 16.0;
pub const THUMB_WIDTH: f32 = 96.0;
pub const TOOL: f32 = 30.0;
pub const PIN: f32 = 22.0;

const BOOKMARK: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='currentColor' stroke='currentColor' stroke-width='2' stroke-linejoin='round'><path d='M6 3h12v18l-6-4-6 4z'/></svg>";
const GLOBE: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round'><circle cx='12' cy='12' r='9'/><path d='M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18'/></svg>";

fn inline<'a, M: 'a>(bytes: &'static [u8], side: f32, tint: Color) -> Element<'a, M> {
    super::svg_glyph(iced::widget::svg::Handle::from_memory(bytes), side, tint)
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Area {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Area {
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    fn scaled(&self, scale: f32) -> Rectangle {
        Rectangle { x: self.x * scale, y: self.y * scale, width: self.w * scale, height: self.h * scale }
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageTool {
    #[default]
    Select,
    Mark,
    Ink,
    Pin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint2 {
    Fill,
    Under,
    Strike,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Marked {
    pub areas: Vec<Area>,
    pub color: Color,
    pub style: Paint2,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PageEvent {
    Click(f32, f32),
    Select(Vec<usize>),
    Stroke(Vec<(f32, f32)>),
    Hover(Option<usize>),
    Clear,
}

#[derive(Debug, Clone, Default)]
pub struct Layer {
    pub scale: f32,
    pub words: Vec<Area>,
    pub marks: Vec<Marked>,
    pub hits: Vec<(Area, bool)>,
    pub selection: Vec<Area>,
    pub links: Vec<Area>,
    pub pins: Vec<((f32, f32), Color)>,
    pub strokes: Vec<(Vec<(f32, f32)>, Color)>,
    pub tool: PageTool,
    pub ink: Color,
    pub ring: Color,
}

#[derive(Debug, Default)]
pub struct LayerState {
    press: Option<(f32, f32)>,
    drag: Option<(usize, usize)>,
    stroke: Vec<(f32, f32)>,
    hover: Option<usize>,
}

struct Overlay<'a, M> {
    layer: Layer,
    on_event: Rc<dyn Fn(PageEvent) -> M + 'a>,
}

fn nearest(words: &[Area], x: f32, y: f32) -> Option<usize> {
    let distance = |area: &Area| {
        let dx = (area.x - x).max(0.0).max(x - area.x - area.w);
        let dy = (area.y - y).max(0.0).max(y - area.y - area.h);
        dx * dx + dy * dy * 4.0
    };
    words.iter().enumerate().min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b))).map(|(index, _)| index)
}

fn zone(layer: &Layer, x: f32, y: f32) -> Option<usize> {
    let pin = PIN / layer.scale.max(0.01);
    layer
        .pins
        .iter()
        .position(|((px, py), _)| x >= *px && x <= px + pin && y >= *py && y <= py + pin)
        .map(|index| index + layer.links.len())
        .or_else(|| layer.links.iter().position(|area| area.contains(x, y)))
}

impl<M> canvas::Program<M> for Overlay<'_, M> {
    type State = LayerState;

    fn update(
        &self, state: &mut LayerState, event: &Event, bounds: Rectangle, cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        let scale = self.layer.scale.max(0.01);
        let at = |cursor: mouse::Cursor| cursor.position_in(bounds).map(|p| (p.x / scale, p.y / scale));
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let (x, y) = at(cursor)?;
                state.press = Some((x, y));
                state.drag = None;
                state.stroke.clear();
                if self.layer.tool == PageTool::Ink {
                    state.stroke.push((x, y));
                }
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let point = cursor.position_from(bounds.position()).map(|p| (p.x / scale, p.y / scale));
                if let (Some(start), Some((x, y))) = (state.press, point) {
                    if self.layer.tool == PageTool::Ink {
                        let far = state.stroke.last().is_none_or(|(lx, ly)| (lx - x).hypot(ly - y) * scale > 2.0);
                        if far {
                            state.stroke.push((x, y));
                        }
                        return Some(Action::request_redraw().and_capture());
                    }
                    if matches!(self.layer.tool, PageTool::Select | PageTool::Mark)
                        && (start.0 - x).hypot(start.1 - y) * scale > 4.0
                    {
                        let from = nearest(&self.layer.words, start.0, start.1);
                        let to = nearest(&self.layer.words, x, y);
                        let drag = from.zip(to).map(|(a, b)| (a.min(b), a.max(b)));
                        if drag != state.drag {
                            state.drag = drag;
                            return Some(Action::request_redraw().and_capture());
                        }
                        return Some(Action::capture());
                    }
                    return None;
                }
                let hover = at(cursor).and_then(|(x, y)| zone(&self.layer, x, y));
                if hover != state.hover {
                    state.hover = hover;
                    return Some(Action::publish((self.on_event)(PageEvent::Hover(hover))));
                }
                None
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let start = state.press.take()?;
                if self.layer.tool == PageTool::Ink {
                    let stroke = std::mem::take(&mut state.stroke);
                    return (stroke.len() > 1).then(|| Action::publish((self.on_event)(PageEvent::Stroke(stroke))));
                }
                if let Some((a, b)) = state.drag.take() {
                    return Some(Action::publish((self.on_event)(PageEvent::Select((a..=b).collect()))));
                }
                Some(Action::publish((self.on_event)(PageEvent::Click(start.0, start.1))))
            }
            _ => None,
        }
    }

    fn draw(
        &self, state: &LayerState, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let scale = self.layer.scale;
        let rect = |frame: &mut Frame, area: &Area, color: Color| {
            let r = area.scaled(scale);
            frame.fill_rectangle(Point::new(r.x, r.y), Size::new(r.width, r.height), color);
        };
        for mark in &self.layer.marks {
            for area in &mark.areas {
                let (band, color) = match mark.style {
                    Paint2::Fill => (*area, Color { a: 0.42, ..mark.color }),
                    Paint2::Under => (Area::new(area.x, area.y + area.h - 1.2, area.w, 1.6), mark.color),
                    Paint2::Strike => (Area::new(area.x, area.y + area.h * 0.52, area.w, 1.4), mark.color),
                };
                rect(&mut frame, &band, color);
                if mark.focused {
                    let r = area.scaled(scale);
                    frame.stroke(
                        &Path::rectangle(Point::new(r.x - 1.0, r.y - 1.0), Size::new(r.width + 2.0, r.height + 2.0)),
                        Stroke::default().with_color(self.layer.ring).with_width(1.5),
                    );
                }
            }
        }
        for (area, current) in &self.layer.hits {
            if *current {
                let r = area.scaled(scale);
                rect(&mut frame, area, Color { a: 0.28, ..self.layer.ring });
                frame.stroke(
                    &Path::rounded_rectangle(
                        Point::new(r.x - 2.0, r.y - 2.0),
                        Size::new(r.width + 4.0, r.height + 4.0),
                        3.0.into(),
                    ),
                    Stroke::default().with_color(self.layer.ring).with_width(2.0),
                );
            } else {
                rect(&mut frame, area, Color::from_rgba8(240, 196, 64, 0.48));
            }
        }
        let live: Vec<Area> = state.drag.map_or_else(
            || self.layer.selection.clone(),
            |(a, b)| self.layer.words.get(a..=b).map(<[Area]>::to_vec).unwrap_or_default(),
        );
        let tint = if self.layer.tool == PageTool::Mark {
            Color { a: 0.42, ..self.layer.ink }
        } else {
            Color::from_rgba8(47, 111, 208, 0.24)
        };
        for area in &live {
            rect(&mut frame, area, tint);
        }
        let draw_stroke = |frame: &mut Frame, points: &[(f32, f32)], color: Color| {
            if points.len() < 2 {
                return;
            }
            let path = Path::new(|builder| {
                builder.move_to(Point::new(points[0].0 * scale, points[0].1 * scale));
                for (x, y) in &points[1..] {
                    builder.line_to(Point::new(x * scale, y * scale));
                }
            });
            frame.stroke(
                &path,
                Stroke::default()
                    .with_color(color)
                    .with_width(2.2)
                    .with_line_cap(LineCap::Round)
                    .with_line_join(LineJoin::Round),
            );
        };
        for (points, color) in &self.layer.strokes {
            draw_stroke(&mut frame, points, *color);
        }
        draw_stroke(&mut frame, &state.stroke, self.layer.ink);
        for ((x, y), color) in &self.layer.pins {
            let origin = Point::new(x * scale, y * scale);
            frame.fill(&Path::rounded_rectangle(origin, Size::new(PIN, PIN), 6.0.into()), *color);
            let body = Path::new(|b| {
                b.move_to(Point::new(origin.x + 6.0, origin.y + 7.0));
                b.line_to(Point::new(origin.x + 16.0, origin.y + 7.0));
                b.move_to(Point::new(origin.x + 6.0, origin.y + 11.0));
                b.line_to(Point::new(origin.x + 13.0, origin.y + 11.0));
            });
            frame.stroke(
                &body,
                Stroke::default().with_color(Color::from_rgb8(59, 42, 5)).with_width(1.6).with_line_cap(LineCap::Round),
            );
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, state: &LayerState, bounds: Rectangle, cursor: mouse::Cursor) -> mouse::Interaction {
        let Some(p) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };
        let (x, y) = (p.x / self.layer.scale.max(0.01), p.y / self.layer.scale.max(0.01));
        if state.hover.is_some() || zone(&self.layer, x, y).is_some() {
            return mouse::Interaction::Pointer;
        }
        match self.layer.tool {
            PageTool::Ink | PageTool::Pin => mouse::Interaction::Crosshair,
            _ if self.layer.words.iter().any(|w| w.contains(x, y)) => mouse::Interaction::Text,
            _ => mouse::Interaction::default(),
        }
    }
}

#[must_use]
pub fn page_layer<'a, M: 'a>(
    layer: Layer, width: f32, height: f32, on_event: impl Fn(PageEvent) -> M + 'a,
) -> Element<'a, M> {
    surface(Overlay { layer, on_event: Rc::new(on_event) }).width(width).height(height).into()
}

#[must_use]
pub fn page_sheet<'a, M: 'a>(
    paint: &Paint, picture: Option<image::Handle>, paper: Color, width: f32, height: f32, layer: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let face: Element<'a, M> = match picture {
        Some(handle) => image(handle).width(width).height(height).content_fit(ContentFit::Fill).into(),
        None => Space::new().width(width).height(height).into(),
    };
    let mut layers = vec![face];
    if let Some(layer) = layer {
        layers.push(layer);
    }
    let paint = *paint;
    glide_iced::widgets::surface(
        container(stack(layers)).width(width).height(height).clip(true).style(move |_| container::Style {
            background: Some(Background::Color(paper)),
            border: Border { color: paint.line(), width: 1.0, radius: 2.0.into() },
            ..container::Style::default()
        }),
        &paint,
        Level::El1,
        2.0,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThumbMarks {
    pub bookmark: bool,
    pub note: bool,
    pub broken: bool,
}

#[must_use]
pub fn page_thumb<'a, M: Clone + 'a>(
    paint: &Paint, picture: Option<image::Handle>, aspect: f32, label: impl Into<String>, current: bool,
    marks: ThumbMarks, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let height = (THUMB_WIDTH * aspect).round();
    let face: Element<'a, M> = match picture {
        Some(handle) => image(handle).width(THUMB_WIDTH).height(height).content_fit(ContentFit::Fill).into(),
        None => Space::new().width(THUMB_WIDTH).height(height).into(),
    };
    let mut layers = vec![face];
    if marks.bookmark {
        layers.push(
            container(inline(BOOKMARK, 12.0, paint.acc_bg()))
                .align_right(Fill)
                .padding(Padding::from([0.0, 4.0]))
                .into(),
        );
    }
    if marks.note {
        layers.push(
            container(
                container(Space::new())
                    .width(8)
                    .height(8)
                    .style(|_| style::rounded(Color::from_rgb8(0xe0, 0xa8, 0x3f), R_ROUND)),
            )
            .align_bottom(Fill)
            .padding(6)
            .into(),
        );
    }
    if marks.broken {
        layers.push(container(icon(Icon::Warning, 18.0, paint.warn())).center(Fill).into());
    }
    let sheet =
        container(stack(layers)).width(THUMB_WIDTH).height(height).clip(true).style(move |_| container::Style {
            background: Some(Background::Color(Color::WHITE)),
            border: Border {
                color: if current { paint.acc_bg() } else { paint.line_2() },
                width: if current { 2.0 } else { 1.0 },
                radius: 2.0.into(),
            },
            shadow: if current {
                Shadow { color: paint.acc_ring(), offset: iced::Vector::ZERO, blur_radius: 6.0 }
            } else {
                Shadow::default()
            },
            ..container::Style::default()
        });
    let caption = text(
        label.into(),
        TextStyle { size_px: 11.5, line_height_px: 14.0, weight: if current { 600 } else { 450 }, ..TextStyle::CAP },
    )
    .color(if current { paint.acc_text() } else { paint.fg_2() });
    button(column![sheet, caption].spacing(6).align_x(Alignment::Center))
        .on_press(message)
        .padding(Padding::from([2.0, 4.0]))
        .style(move |_, status| style::flat(paint.layered(Color::TRANSPARENT, status), paint.fg_2(), R2))
        .into()
}

#[must_use]
pub fn floating<'a, M: 'a>(
    paint: &Paint, content: impl Into<Element<'a, M>>, padding: impl Into<Padding>,
) -> Element<'a, M> {
    let paint = *paint;
    iced::widget::opaque(glide_iced::widgets::surface(
        container(content).padding(padding).style(move |_| style::popover(&paint)),
        &paint,
        Level::El3,
        R3,
    ))
}

#[must_use]
pub fn tool_button<'a, M: Clone + 'a>(
    paint: &Paint, glyph: impl super::Glyph, label: &'a str, on: bool, message: Option<M>,
) -> Element<'a, M> {
    let paint = *paint;
    let tint = if on {
        paint.acc_soft_fg()
    } else if message.is_some() {
        paint.fg_2()
    } else {
        paint.fg_4()
    };
    let control = button(container(icon(glyph, 16.0, tint)).center(Fill))
        .on_press_maybe(message)
        .width(TOOL)
        .height(TOOL)
        .padding(0)
        .style(move |_, status| {
            let base = if on { paint.acc_soft() } else { Color::TRANSPARENT };
            style::flat(paint.layered(base, status), tint, R2)
        });
    tooltip(&paint, control, label)
}

#[must_use]
pub fn tool_label<'a, M: Clone + 'a>(
    paint: &Paint, lead: Option<Element<'a, M>>, label: impl Into<String>, on: bool, message: Option<M>,
) -> Element<'a, M> {
    let paint = *paint;
    let ink = if on {
        paint.acc_soft_fg()
    } else if message.is_some() {
        paint.fg_1()
    } else {
        paint.fg_4()
    };
    let mut content = row![].spacing(6).align_y(Alignment::Center);
    if let Some(lead) = lead {
        content = content.push(lead);
    }
    content = content.push(
        text(label.into(), TextStyle { size_px: 12.5, weight: 500, ..TextStyle::MENU_ITEM })
            .color(ink)
            .wrapping(Wrapping::None),
    );
    button(container(content).center_y(Fill))
        .on_press_maybe(message)
        .height(TOOL)
        .padding(Padding::from([0.0, 8.0]))
        .style(move |_, status| {
            let base = if on { paint.acc_soft() } else { Color::TRANSPARENT };
            style::flat(paint.layered(base, status), ink, R2)
        })
        .into()
}

#[must_use]
pub fn tool_divider<'a, M: 'a>(paint: &Paint) -> Element<'a, M> {
    let paint = *paint;
    container(container(Space::new().width(1).height(18)).style(move |_| style::rounded(paint.line_2(), 0.0)))
        .padding(Padding::from([0.0, SP1]))
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paper {
    pub face: Color,
    pub ink: Color,
}

#[must_use]
pub fn paper_picker<'a, T: Copy + PartialEq + 'a, M: Clone + 'a>(
    paint: &Paint, options: &[(T, &'a str, Paper)], selected: T, on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    let paint = *paint;
    let tiles =
        options.iter().map(|&(value, label, paper)| {
            let on = value == selected;
            let lines = column![
                container(Space::new())
                    .width(Fill)
                    .height(3)
                    .style(move |_| style::rounded(Color { a: 0.45, ..paper.ink }, 1.0)),
                container(Space::new())
                    .width(Length::FillPortion(4))
                    .height(3)
                    .style(move |_| style::rounded(Color { a: 0.45, ..paper.ink }, 1.0)),
                container(Space::new())
                    .width(Length::FillPortion(3))
                    .height(3)
                    .style(move |_| style::rounded(Color { a: 0.45, ..paper.ink }, 1.0)),
            ]
            .spacing(5);
            let tile = animate(if on { 1.0 } else { 0.0 }, Speed::COLOR, move |shown| {
                let shown = shown.clone();
                container(lines).padding(Padding::from([0.0, 14.0])).center_y(56).width(Fill).style(move |_| {
                    let amount = shown.get().clamp(0.0, 1.0);
                    container::Style {
                        background: Some(Background::Color(paper.face)),
                        border: Border {
                            color: if amount > 0.5 { paint.acc_bg() } else { paint.line_2() },
                            width: 1.0 + amount,
                            radius: R2.into(),
                        },
                        shadow: Shadow {
                            color: paint.acc_ring().scale_alpha(amount),
                            offset: iced::Vector::ZERO,
                            blur_radius: 6.0 * amount,
                        },
                        ..container::Style::default()
                    }
                })
            });
            let caption = text(label, TextStyle { weight: if on { 600 } else { 450 }, ..TextStyle::CAP })
                .color(if on { paint.fg_1() } else { paint.fg_2() });
            button(column![tile, caption].spacing(6).align_x(Alignment::Center).width(Fill))
                .on_press(on_select(value))
                .padding(0)
                .width(Fill)
                .style(|_, _| button::Style::default())
                .into()
        });
    iced::widget::Row::with_children(tiles).spacing(SP2).into()
}

#[must_use]
pub fn reading_progress<'a, M: 'a>(paint: &Paint, fraction: f32, label: impl Into<String>) -> Element<'a, M> {
    let paint = *paint;
    let fraction = fraction.clamp(0.0, 1.0);
    let filled = (120.0 * fraction).round();
    let track =
        container(container(Space::new()).width(filled).height(3).style(move |_| style::rounded(paint.acc_bg(), 2.0)))
            .width(120)
            .height(3)
            .style(move |_| style::rounded(paint.fill_2(), 2.0));
    row![track, text(label.into(), TextStyle::CAP).color(paint.fg_2())].spacing(SP3).align_y(Alignment::Center).into()
}

#[must_use]
pub fn progress_line<'a, M: 'a>(paint: &Paint, fraction: f32) -> Element<'a, M> {
    let paint = *paint;
    let fraction = fraction.clamp(0.0, 1.0);
    container(row![
        container(Space::new())
            .width(Length::FillPortion((fraction * 1000.0) as u16 + 1))
            .height(3)
            .style(move |_| style::rounded(paint.acc_bg(), 2.0)),
        Space::new().width(Length::FillPortion(((1.0 - fraction) * 1000.0) as u16 + 1)),
    ])
    .width(Fill)
    .height(3)
    .style(move |_| style::rounded(paint.fill_2(), 2.0))
    .into()
}

#[must_use]
pub fn note_card<'a, M: 'a>(
    paint: &Paint, author: &str, when: &str, body: Element<'a, M>, actions: Vec<Element<'a, M>>,
) -> Element<'a, M> {
    let paint = *paint;
    let initial = author.chars().next().map_or_else(String::new, |c| c.to_uppercase().collect());
    let avatar = container(
        text(initial, TextStyle { size_px: 10.0, line_height_px: 20.0, weight: 600, ..TextStyle::CAP })
            .color(Color::WHITE),
    )
    .center(20)
    .style(move |_| style::rounded(paint.acc_bg(), R_ROUND));
    let head = row![
        avatar,
        text(author.to_string(), TextStyle { weight: 600, ..TextStyle::CAP }).color(paint.fg_1()).width(Fill),
        text(when.to_string(), TextStyle::CAP).color(paint.fg_2()),
    ]
    .spacing(SP2)
    .align_y(Alignment::Center);
    let mut content = column![head, body].spacing(SP2).width(248);
    if !actions.is_empty() {
        content = content.push(iced::widget::Row::with_children(actions).spacing(6));
    }
    floating(&paint, content, SP3)
}

#[must_use]
pub fn link_preview<'a, M: 'a>(
    paint: &Paint, picture: Option<image::Handle>, aspect: f32, title: impl Into<String>, place: impl Into<String>,
    hint: impl Into<String>,
) -> Element<'a, M> {
    let paint = *paint;
    let face: Element<'a, M> = match picture {
        Some(handle) => image(handle).width(84).height(84.0 * aspect).into(),
        None => Space::new().width(84).height(84.0 * aspect).into(),
    };
    let sheet = container(face).style(move |_| container::Style {
        background: Some(Background::Color(Color::WHITE)),
        border: Border { color: paint.line_2(), width: 1.0, radius: 2.0.into() },
        ..container::Style::default()
    });
    let words = column![
        text(title.into(), TextStyle::HEAD).color(paint.fg_1()),
        text(place.into(), TextStyle::CAP).color(paint.fg_2()),
        Space::new().height(Fill),
        text(hint.into(), TextStyle::CAP).color(paint.fg_2()),
    ]
    .spacing(4)
    .width(Fill)
    .height(84.0 * aspect);
    floating(&paint, row![sheet, words].spacing(SP3).width(300), Padding::from(10))
}

#[must_use]
pub fn external_link<'a, M: Clone + 'a>(
    paint: &Paint, address: &str, note: &str, open: (&'a str, M), copy: (&'a str, M),
) -> Element<'a, M> {
    let paint = *paint;
    let head = row![
        inline(GLOBE, 16.0, paint.fg_2()),
        text(address.to_string(), TextStyle { size_px: 12.0, ..TextStyle::NUM })
            .color(paint.fg_1())
            .wrapping(Wrapping::None),
    ]
    .spacing(SP2)
    .align_y(Alignment::Center);
    let actions = row![
        super::sized_button(&paint, open.0, super::Tone::Accent, super::Size::Small, Some(open.1)),
        super::sized_button(&paint, copy.0, super::Tone::Neutral, super::Size::Small, Some(copy.1)),
    ]
    .spacing(6);
    floating(
        &paint,
        column![head, text(note.to_string(), TextStyle::CAP).color(paint.fg_2()), actions].spacing(10).width(320),
        SP3,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Intact,
    Changed,
    Waiting,
}

#[must_use]
pub fn signature_status<'a, M: Clone + 'a>(
    paint: &Paint, verdict: Verdict, who: &str, lines: Vec<String>, details: Option<(&'a str, M)>,
) -> Element<'a, M> {
    let paint = *paint;
    let (lead, raised): (Element<'a, M>, bool) = match verdict {
        Verdict::Intact => (icon(Icon::Shield, 16.0, paint.ok()), true),
        Verdict::Changed => (icon(Icon::Warning, 16.0, paint.warn()), true),
        Verdict::Waiting => (
            container(Space::new())
                .width(14)
                .height(14)
                .style(move |_| container::Style {
                    border: Border { color: paint.warn(), width: 1.5, radius: R_ROUND.into() },
                    ..container::Style::default()
                })
                .into(),
            false,
        ),
    };
    let mut content = column![
        row![lead, text(who.to_string(), TextStyle { size_px: 13.0, ..TextStyle::HEAD }).color(paint.fg_1())]
            .spacing(SP2)
            .align_y(Alignment::Center)
    ]
    .spacing(6);
    for line in lines {
        content = content.push(text(line, TextStyle::CAP).color(paint.fg_2()));
    }
    if let Some((label, message)) = details {
        content = content.push(super::link(&paint, label, Some(message)));
    }
    let card = container(content).padding(SP3).width(Fill).style(move |_| container::Style {
        background: Some(Background::Color(paint.card_bg())),
        border: Border { color: paint.line(), width: 1.0, radius: R3.into() },
        ..container::Style::default()
    });
    if raised { glide_iced::widgets::surface(card, &paint, Level::El1, R3) } else { card.into() }
}

#[derive(Debug, Default)]
pub struct PadState {
    stroke: Vec<(f32, f32)>,
}

type Points = Vec<(f32, f32)>;

struct Pad<'a, M> {
    strokes: Vec<Points>,
    ink: Color,
    base: Color,
    on_stroke: Rc<dyn Fn(Points) -> M + 'a>,
}

impl<M> canvas::Program<M> for Pad<'_, M> {
    type State = PadState;

    fn update(
        &self, state: &mut PadState, event: &Event, bounds: Rectangle, cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let p = cursor.position_in(bounds)?;
                state.stroke = vec![(p.x, p.y)];
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if !state.stroke.is_empty() => {
                let p = cursor.position_from(bounds.position())?;
                state.stroke.push((p.x.clamp(0.0, bounds.width), p.y.clamp(0.0, bounds.height)));
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if !state.stroke.is_empty() => {
                let stroke = std::mem::take(&mut state.stroke);
                Some(Action::publish((self.on_stroke)(stroke)))
            }
            _ => None,
        }
    }

    fn draw(
        &self, state: &PadState, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::new(16.0, bounds.height - 26.0), Size::new(bounds.width - 32.0, 1.0), self.base);
        let all = self.strokes.iter().chain(std::iter::once(&state.stroke));
        for points in all.filter(|points| points.len() > 1) {
            let path = Path::new(|b| {
                b.move_to(Point::new(points[0].0, points[0].1));
                for (x, y) in &points[1..] {
                    b.line_to(Point::new(*x, *y));
                }
            });
            frame.stroke(
                &path,
                Stroke::default()
                    .with_color(self.ink)
                    .with_width(2.2)
                    .with_line_cap(LineCap::Round)
                    .with_line_join(LineJoin::Round),
            );
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, _: &PadState, bounds: Rectangle, cursor: mouse::Cursor) -> mouse::Interaction {
        if cursor.is_over(bounds) { mouse::Interaction::Crosshair } else { mouse::Interaction::default() }
    }
}

#[must_use]
pub fn signature_pad<'a, M: 'a>(
    paint: &Paint, strokes: Vec<Vec<(f32, f32)>>, height: f32, on_stroke: impl Fn(Vec<(f32, f32)>) -> M + 'a,
) -> Element<'a, M> {
    let paint = *paint;
    let pad = surface(Pad { strokes, ink: paint.fg_1(), base: paint.line_2(), on_stroke: Rc::new(on_stroke) })
        .width(Fill)
        .height(height);
    container(pad)
        .width(Fill)
        .height(height)
        .style(move |_| container::Style {
            background: Some(Background::Color(paint.view_bg())),
            border: Border { color: paint.line_2(), width: 1.0, radius: R2.into() },
            ..container::Style::default()
        })
        .into()
}

#[must_use]
pub fn hit_row<'a, M: Clone + 'a>(
    paint: &Paint, page: &str, before: &str, matched: &str, after: &str, current: bool, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let serif = TextStyle { size_px: 12.5, line_height_px: 18.0, ..TextStyle::CALL };
    let mark = Color::from_rgba8(240, 196, 64, 0.48);
    let snippet: iced::widget::text::Rich<'a, (), M> = iced::widget::rich_text![
        iced::widget::span(before.to_string()).color(paint.fg_1()),
        iced::widget::span(matched.to_string()).color(paint.fg_1()).background(mark).font(style::SEMIBOLD),
        iced::widget::span(after.to_string()).color(paint.fg_1()),
    ]
    .size(serif.size_px)
    .line_height(iced::widget::text::LineHeight::Absolute(serif.line_height_px.into()))
    .width(Fill);
    let number = text(page.to_string(), TextStyle { size_px: 11.5, line_height_px: 18.0, ..TextStyle::NUM })
        .color(if current { paint.acc_soft_fg() } else { paint.fg_2() })
        .width(24)
        .align_x(Alignment::End);
    button(row![number, container(snippet).max_height(36.0).clip(true)].spacing(10).align_y(Alignment::Start))
        .on_press(message)
        .width(Fill)
        .padding(Padding { top: 7.0, right: 10.0, bottom: 8.0, left: 12.0 })
        .style(move |_, status| {
            let base = if current { paint.acc_soft() } else { Color::TRANSPARENT };
            style::flat(paint.layered(base, status), paint.fg_1(), R2)
        })
        .into()
}

#[must_use]
pub fn quote_row<'a, M: Clone + 'a>(
    paint: &Paint, color: Color, quote: &str, comment: Option<&str>, meta: &[String], current: bool, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let bar = container(Space::new()).width(3).height(Fill).style(move |_| style::rounded(color, 1.5));
    let quote = text(quote.to_string(), TextStyle { size_px: 12.5, line_height_px: 18.0, ..TextStyle::CALL })
        .color(paint.fg_1());
    let mut body =
        column![row![bar, container(quote).max_height(36.0).clip(true)].spacing(SP2).height(Length::Shrink)].spacing(4);
    if let Some(comment) = comment.filter(|c| !c.is_empty()) {
        body = body.push(
            text(comment.to_string(), TextStyle { size_px: 12.5, line_height_px: 18.0, ..TextStyle::CALL })
                .color(paint.fg_1()),
        );
    }
    body = body.push(
        text(meta.join(" · "), TextStyle { size_px: 11.5, line_height_px: 14.0, ..TextStyle::CAP }).color(paint.fg_2()),
    );
    button(body)
        .on_press(message)
        .width(Fill)
        .padding(Padding { top: 8.0, right: 10.0, bottom: 10.0, left: 12.0 })
        .style(move |_, status| {
            let base = if current { paint.acc_soft() } else { Color::TRANSPARENT };
            style::flat(paint.layered(base, status), paint.fg_1(), R2)
        })
        .into()
}

#[derive(Debug, Clone)]
pub struct Cover<'a> {
    pub picture: Option<image::Handle>,
    pub title: String,
    pub face: Color,
    pub ink: Color,
    pub badge: &'a str,
    pub name: String,
    pub meta: String,
    pub progress: Option<f32>,
}

#[must_use]
pub fn doc_cover<'a, M: Clone + 'a>(paint: &Paint, cover: Cover<'a>, width: f32, message: M) -> Element<'a, M> {
    let paint = *paint;
    let height = (width * 4.0 / 3.0).round();
    let face = cover.face;
    let art: Element<'a, M> = match cover.picture {
        Some(handle) => image(handle).width(width).height(height).content_fit(ContentFit::Cover).into(),
        None => container(
            text(
                cover.title.clone(),
                TextStyle { size_px: 13.0, line_height_px: 16.0, weight: 650, ..TextStyle::HEAD },
            )
            .color(cover.ink),
        )
        .padding(Padding::from([14.0, 10.0]))
        .width(width)
        .height(height)
        .into(),
    };
    let tag = container(super::format_badge(cover.badge)).align_bottom(height).padding(6);
    let sheet = container(stack![art, tag]).width(width).height(height).clip(true).style(move |_| container::Style {
        background: Some(Background::Color(face)),
        border: Border { color: paint.line(), width: 1.0, radius: R1.into() },
        ..container::Style::default()
    });
    let sheet = glide_iced::widgets::surface(sheet, &paint, Level::El1, R1);
    let mut info = column![
        text(cover.name.clone(), TextStyle { size_px: 13.0, line_height_px: 17.0, weight: 500, ..TextStyle::CALL })
            .color(paint.fg_1())
            .wrapping(Wrapping::WordOrGlyph)
    ]
    .spacing(6)
    .width(width);
    if let Some(fraction) = cover.progress.filter(|f| *f > 0.0) {
        info = info.push(progress_line(&paint, fraction));
    }
    info = info.push(text(cover.meta.clone(), TextStyle::CAP).color(paint.fg_2()));
    let body = column![
        crate::anim::hover(Speed::OVERSHOOT, move |_| sheet).transform(|value| crate::anim::Transform {
            scale: 1.0 + 0.03 * value,
            offset: iced::Vector::new(0.0, -4.0 * value),
        }),
        info
    ]
    .spacing(SP2)
    .width(width);
    button(body).on_press(message).padding(0).style(|_, _| button::Style::default()).into()
}

#[must_use]
pub fn form_input_style(
    paint: &Paint, status: iced::widget::text_input::Status, needed: bool,
) -> iced::widget::text_input::Style {
    use iced::widget::text_input::{Status, Style};
    let blue = Color::from_rgb8(0x2f, 0x6f, 0xd0);
    let amber = Color::from_rgb8(0xb5, 0x7e, 0x10);
    let focused = matches!(status, Status::Focused { .. });
    let edge = if focused {
        blue
    } else if needed {
        amber
    } else {
        Color { a: 0.45, ..blue }
    };
    Style {
        background: Background::Color(if focused {
            Color::WHITE
        } else if needed {
            Color { a: 0.14, ..Color::from_rgb8(0xe5, 0xb3, 0x55) }
        } else {
            Color { a: 0.07, ..blue }
        }),
        border: Border { color: edge, width: if focused { 2.0 } else { 1.0 }, radius: 2.0.into() },
        icon: paint.fg_2(),
        placeholder: Color { a: 0.45, ..Color::from_rgb8(0x1b, 0x1a, 0x17) },
        value: Color::from_rgb8(0x1b, 0x1a, 0x17),
        selection: Color { a: 0.3, ..blue },
    }
}

#[must_use]
pub fn sign_here<'a, M: Clone + 'a>(
    paint: &Paint, label: &'a str, width: f32, height: f32, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let blue = Color::from_rgb8(0x2f, 0x6f, 0xd0);
    let face = container(
        text(label, TextStyle { size_px: 12.0, weight: 500, ..TextStyle::CAP })
            .color(Color::from_rgb8(0x1f, 0x56, 0xa8)),
    )
    .center(Fill)
    .width(width)
    .height(height);
    button(face)
        .on_press(message)
        .padding(0)
        .style(move |_, status| button::Style {
            background: Some(Background::Color(Color {
                a: if matches!(status, button::Status::Hovered) { 0.10 } else { 0.05 },
                ..blue
            })),
            border: Border { color: blue, width: 1.5, radius: R1.into() },
            text_color: paint.fg_1(),
            ..button::Style::default()
        })
        .into()
}

#[must_use]
pub fn continue_card<'a, M: Clone + 'a>(
    paint: &Paint, cover: Cover<'a>, status: Option<Element<'a, M>>, when: String, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let face = cover.face;
    let mini: Element<'a, M> = match cover.picture {
        Some(handle) => image(handle).width(60).height(80).content_fit(ContentFit::Cover).into(),
        None => container(
            text(cover.title.clone(), TextStyle { size_px: 9.0, line_height_px: 11.0, weight: 650, ..TextStyle::CAP })
                .color(cover.ink),
        )
        .padding(6)
        .width(60)
        .height(80)
        .into(),
    };
    let mini = container(mini).width(60).height(80).clip(true).style(move |_| container::Style {
        background: Some(Background::Color(face)),
        border: Border { color: paint.line(), width: 1.0, radius: 3.0.into() },
        ..container::Style::default()
    });
    let mut words = column![
        text(cover.name.clone(), TextStyle { size_px: 13.5, line_height_px: 19.0, weight: 600, ..TextStyle::CALL })
            .color(paint.fg_1())
            .wrapping(Wrapping::None),
        text(cover.meta.clone(), TextStyle::CAP).color(paint.fg_2()).wrapping(Wrapping::None),
        Space::new().height(SP3),
    ]
    .spacing(4)
    .width(Fill)
    .clip(true);
    if let Some(fraction) = cover.progress {
        words = words.push(progress_line(&paint, fraction));
    }
    if let Some(status) = status {
        words = words.push(status);
    }
    words = words.push(text(when, TextStyle::CAP).color(paint.fg_2()).wrapping(Wrapping::None));
    let card = container(row![mini, words].spacing(14)).padding(12).width(Fill).style(move |_| container::Style {
        background: Some(Background::Color(paint.card_bg())),
        border: Border { color: paint.line(), width: 1.0, radius: R3.into() },
        ..container::Style::default()
    });
    let card = crate::anim::hover(Speed::COLOR, move |_| glide_iced::widgets::surface(card, &paint, Level::El1, R3));
    button(card).on_press(message).padding(0).width(Fill).style(|_, _| button::Style::default()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_zones_cover_links_then_pins() {
        let layer = Layer {
            scale: 2.0,
            links: vec![Area::new(10.0, 10.0, 40.0, 10.0)],
            pins: vec![((100.0, 100.0), Color::WHITE)],
            ..Layer::default()
        };
        assert_eq!(zone(&layer, 20.0, 15.0), Some(0));
        assert_eq!(zone(&layer, 105.0, 105.0), Some(1));
        assert_eq!(zone(&layer, 300.0, 300.0), None);
    }

    #[test]
    fn the_nearest_word_wins_a_drag_end() {
        let words =
            vec![Area::new(0.0, 0.0, 10.0, 10.0), Area::new(20.0, 0.0, 10.0, 10.0), Area::new(0.0, 20.0, 10.0, 10.0)];
        assert_eq!(nearest(&words, 25.0, 4.0), Some(1));
        assert_eq!(nearest(&words, 2.0, 26.0), Some(2));
    }
}

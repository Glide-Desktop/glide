use std::borrow::Cow;

use glide_iced::Paint;
use glide_iced::style::{self, R2, SP1, SP2, SP3, to_iced};
use glide_iced::widgets;
use glide_theme::elevation::{self, Level};
use glide_theme::shape::{R_ROUND, R1};
use glide_theme::{TextStyle, component};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{Operation, Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, overlay};
use iced::keyboard::key::Named;
use iced::widget::{Column, Row, Space, button, container, mouse_area, row};
use iced::{
    Alignment, Border, Color, Element, Event, Fill, Length, Padding, Rectangle, Renderer, Shadow, Theme, Vector, mouse,
    window,
};

use super::controls::{Check, mark};
use super::focus::{activatable, focusable};
use super::{Icon, Wrapping, icon, mix, text};
use crate::anim::{Motion, Speed, animate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indicator {
    Thumb,
    Underline,
    Selection,
}

struct Track<'a, M> {
    content: Element<'a, M>,
    selected: Option<usize>,
    paint: Paint,
    indicator: Indicator,
}

#[derive(Debug, Clone, Copy)]
struct Slide {
    position: Motion,
    presence: Motion,
    bar: Motion,
    selected: Option<usize>,
}

impl<M> Track<'_, M> {
    fn items(layout: Layout<'_>) -> Vec<Rectangle> {
        layout.children().next().map(|inner| inner.children().map(|child| child.bounds()).collect()).unwrap_or_default()
    }

    fn rect(items: &[Rectangle], position: f32) -> Option<Rectangle> {
        let last = items.len().checked_sub(1)?;
        let position = position.clamp(0.0, last as f32);
        let low = position.floor() as usize;
        let high = (low + 1).min(last);
        let t = position - low as f32;
        let (a, b) = (items[low], items[high]);
        let lerp = |from: f32, to: f32| from + (to - from) * t;
        Some(Rectangle {
            x: lerp(a.x, b.x),
            y: lerp(a.y, b.y),
            width: lerp(a.width, b.width),
            height: lerp(a.height, b.height),
        })
    }

    fn shadow(&self) -> Shadow {
        elevation::layers(Level::El1, self.paint.tokens.appearance).first().map_or_else(Shadow::default, |layer| {
            Shadow {
                color: to_iced(layer.color).scale_alpha(self.paint.alpha),
                offset: Vector::new(layer.dx, layer.dy),
                blur_radius: layer.blur,
            }
        })
    }
}

impl<M> Widget<M, Theme, Renderer> for Track<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Slide>()
    }

    fn state(&self) -> tree::State {
        let at = self.selected.map_or(0.0, |index| index as f32);
        let present = if self.selected.is_some() { 1.0 } else { 0.0 };
        tree::State::new(Slide {
            position: Motion::at(at),
            presence: Motion::at(present),
            bar: Motion::at(present),
            selected: self.selected,
        })
    }

    forward!(content: children, diff, size);

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let child = self.content.as_widget_mut().layout(&mut tree.children[0], renderer, limits);
        layout::Node::with_children(child.size(), vec![child])
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        let Some(child) = layout.children().next() else { return };
        self.content.as_widget_mut().operate(&mut tree.children[0], child, renderer, operation);
    }

    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, viewport: &Rectangle,
    ) {
        let slide = tree.state.downcast_mut::<Slide>();
        let grows = self.indicator == Indicator::Selection;
        let mut changed = false;
        if slide.selected != self.selected {
            if let Some(index) = self.selected {
                if slide.selected.is_none() {
                    slide.position.jump(index as f32);
                }
                if grows {
                    slide.bar.jump(0.0);
                }
            }
            slide.selected = self.selected;
            changed = true;
        }
        if let Some(index) = self.selected {
            changed |= slide.position.to(index as f32, Speed::SLIDE);
        }
        let present = if self.selected.is_some() { 1.0 } else { 0.0 };
        changed |= slide.presence.to(present, Speed::FADE);
        changed |= slide.bar.to(present, Speed::GROW);
        let animating = slide.position.animating(Speed::SLIDE)
            || slide.presence.animating(Speed::FADE)
            || (grows && slide.bar.animating(Speed::GROW));
        if changed || (animating && matches!(event, Event::Window(window::Event::RedrawRequested(_)))) {
            shell.request_redraw();
        }
        let Some(child) = layout.children().next() else { return };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child,
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
        let slide = tree.state.downcast_ref::<Slide>();
        let presence = slide.presence.value(Speed::FADE);
        let items = Self::items(layout);
        if presence > 0.0
            && let Some(rect) = Self::rect(&items, slide.position.value(Speed::SLIDE))
        {
            let paint = self.paint.faded(presence);
            match self.indicator {
                Indicator::Thumb => renderer.fill_quad(
                    Quad {
                        bounds: rect,
                        border: Border { radius: (R1 + 2.0).into(), ..Border::default() },
                        shadow: self.shadow(),
                        snap: true,
                    },
                    to_iced(paint.tokens.segment_thumb()).scale_alpha(paint.alpha),
                ),
                Indicator::Underline => renderer.fill_quad(
                    Quad {
                        bounds: Rectangle {
                            x: rect.x + SP2,
                            y: rect.y + rect.height - 3.0,
                            width: (rect.width - 2.0 * SP2).max(0.0),
                            height: 3.0,
                        },
                        border: Border { radius: 2.0.into(), ..Border::default() },
                        ..Quad::default()
                    },
                    paint.acc_bg(),
                ),
                Indicator::Selection => {
                    renderer.fill_quad(
                        Quad {
                            bounds: rect,
                            border: Border { radius: R2.into(), ..Border::default() },
                            ..Quad::default()
                        },
                        paint.acc_soft(),
                    );
                    let grow = slide.bar.value(Speed::GROW).max(0.0);
                    let height = 18.0 * grow;
                    if height > 0.5 {
                        renderer.fill_quad(
                            Quad {
                                bounds: Rectangle { x: rect.x, y: rect.center_y() - height / 2.0, width: 3.0, height },
                                border: Border { radius: 2.0.into(), ..Border::default() },
                                ..Quad::default()
                            },
                            paint.acc_bg(),
                        );
                    }
                }
            }
        }
        let Some(child) = layout.children().next() else { return };
        self.content.as_widget().draw(&tree.children[0], renderer, theme, style, child, cursor, viewport);
    }

    fn mouse_interaction(
        &self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &Renderer,
    ) -> mouse::Interaction {
        let Some(child) = layout.children().next() else { return mouse::Interaction::None };
        self.content.as_widget().mouse_interaction(&tree.children[0], child, cursor, viewport, renderer)
    }

    fn overlay<'b>(
        &'b mut self, tree: &'b mut Tree, layout: Layout<'b>, renderer: &Renderer, viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        let child = layout.children().next()?;
        self.content.as_widget_mut().overlay(&mut tree.children[0], child, renderer, viewport, translation)
    }
}

#[must_use]
pub fn track<'a, M: 'a>(
    paint: &Paint, indicator: Indicator, selected: Option<usize>, content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    Element::new(Track { content: content.into(), selected, paint: *paint, indicator })
}

fn segment_button<'a, M: Clone + 'a>(
    paint: &Paint, label: Cow<'a, str>, active: bool, message: M, height: f32, width: Length,
) -> Element<'a, M> {
    let paint = *paint;
    let style = if active { TextStyle { weight: 560, ..TextStyle::CALL } } else { TextStyle::CALL };
    animate(if active { 1.0 } else { 0.0 }, Speed::COLOR, move |value| {
        let value = value.clone();
        let label = text(label, style).wrapping(Wrapping::None);
        let control = button(container(label).center_x(Fill).center_y(Fill))
            .on_press(message)
            .height(height)
            .width(width)
            .padding(Padding::from([0.0, SP3 + 2.0]));
        widgets::button(control, move |status| {
            let amount = value.get().clamp(0.0, 1.0);
            let background = paint.layered(Color::TRANSPARENT, status).scale_alpha(1.0 - amount);
            style::flat(background, super::mix(paint.fg_2(), paint.fg_1(), amount), R1 + 2.0)
        })
    })
    .into()
}

#[must_use]
pub fn segmented<'a, T: Copy + PartialEq + 'a, L: Clone + Into<Cow<'a, str>>, M: Clone + 'a>(
    paint: &Paint, options: &[(T, L)], selected: T, on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    segmented_sized(paint, options, selected, on_select, Length::Shrink)
}

#[must_use]
pub fn segmented_fill<'a, T: Copy + PartialEq + 'a, L: Clone + Into<Cow<'a, str>>, M: Clone + 'a>(
    paint: &Paint, options: &[(T, L)], selected: T, on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    segmented_sized(paint, options, selected, on_select, Length::Fill)
}

fn segmented_sized<'a, T: Copy + PartialEq + 'a, L: Clone + Into<Cow<'a, str>>, M: Clone + 'a>(
    paint: &Paint, options: &[(T, L)], selected: T, on_select: impl Fn(T) -> M, width: Length,
) -> Element<'a, M> {
    let paint = *paint;
    let index = options.iter().position(|(value, _)| *value == selected);
    let segments = options.iter().map(|(value, label)| {
        let active = *value == selected;
        segment_button(&paint, label.clone().into(), active, on_select(*value), component::SMALL_CONTROL, width)
    });
    let content = Row::with_children(segments).spacing(2).width(width);
    let control = container(track(&paint, Indicator::Thumb, index, content))
        .width(width)
        .padding((component::CONTROL - component::SMALL_CONTROL) / 2.0)
        .style(move |_| style::rounded(paint.fill_1(), R2));
    arrows(&paint, control, R2, options, index, on_select)
}

pub const ICON_SEGMENT: f32 = 30.0;
const SEGMENT_GAP: f32 = 2.0;
const SEGMENT_INSET: f32 = (component::CONTROL - component::SMALL_CONTROL) / 2.0;

#[must_use]
pub fn icon_segmented_width(segments: usize) -> f32 {
    let count = segments as f32;
    count * ICON_SEGMENT + (count - 1.0).max(0.0) * SEGMENT_GAP + 2.0 * SEGMENT_INSET
}

#[must_use]
pub fn icon_segmented<'a, T: Copy + PartialEq + 'a, G: super::Glyph, M: Clone + 'a>(
    paint: &Paint, options: &[(T, G, &'a str)], selected: T, on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    let paint = *paint;
    let index = options.iter().position(|(value, ..)| *value == selected);
    let segments = options.iter().map(|&(value, glyph, label)| {
        let active = value == selected;
        let message = on_select(value);
        let tint = if active { paint.fg_1() } else { paint.fg_2() };
        let control = button(container(icon(glyph, 15.0, tint)).center(Fill))
            .on_press(message)
            .width(ICON_SEGMENT)
            .height(component::SMALL_CONTROL)
            .padding(0);
        let control = widgets::button(control, move |status| {
            let background = if active { Color::TRANSPARENT } else { paint.layered(Color::TRANSPARENT, status) };
            style::flat(background, tint, R1 + 2.0)
        });
        super::tooltip(&paint, control, label)
    });
    let content = Row::with_children(segments).spacing(SEGMENT_GAP);
    let control = container(track(&paint, Indicator::Thumb, index, content))
        .padding(SEGMENT_INSET)
        .style(move |_| style::rounded(paint.fill_1(), R2));
    let keyed: Vec<(T, ())> = options.iter().map(|&(value, ..)| (value, ())).collect();
    arrows(&paint, control, R2, &keyed, index, on_select)
}

fn arrows<'a, T: Copy + 'a, L, M: Clone + 'a>(
    paint: &Paint, control: impl Into<Element<'a, M>>, radius: f32, options: &[(T, L)], index: Option<usize>,
    on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    let pick = |at: Option<usize>| at.filter(|&at| Some(at) != index).map(|at| on_select(options[at].0));
    let last = options.len().checked_sub(1);
    let previous = pick(index.map_or(last, |at| at.checked_sub(1)));
    let next = pick(index.map_or(Some(0), |at| (at + 1 < options.len()).then_some(at + 1)));
    let (first, end) = (pick(last.map(|_| 0)), pick(last));
    focusable(paint, control, radius)
        .on_key(move |key| {
            match key {
                Named::ArrowLeft | Named::ArrowUp => previous.clone(),
                Named::ArrowRight | Named::ArrowDown => next.clone(),
                Named::Home => first.clone(),
                Named::End => end.clone(),
                _ => None,
            }
            .into_iter()
            .collect()
        })
        .into()
}

#[must_use]
pub fn tabs<'a, T: Copy + PartialEq + 'a, L: Clone + Into<Cow<'a, str>>, M: Clone + 'a>(
    paint: &Paint, options: &[(T, L)], selected: T, on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    let paint = *paint;
    let index = options.iter().position(|(value, _)| *value == selected);
    let items = options.iter().map(|(value, label)| {
        let active = *value == selected;
        let style = if active { TextStyle { weight: 600, ..TextStyle::CALL } } else { TextStyle::CALL };
        let label: Cow<'a, str> = label.clone().into();
        let message = on_select(*value);
        Element::from(animate(if active { 1.0 } else { 0.0 }, Speed::COLOR, move |shown| {
            let shown = shown.clone();
            let control = button(container(text(label, style).wrapping(Wrapping::None)).center_y(Fill))
                .on_press(message)
                .height(component::CONTROL + SP1)
                .padding(Padding::from([0.0, SP3]));
            widgets::button(control, move |status| {
                let idle = if status == button::Status::Hovered { paint.fg_1() } else { paint.fg_2() };
                let color = super::mix(idle, paint.acc_text(), shown.get().clamp(0.0, 1.0));
                style::flat(paint.layered(Color::TRANSPARENT, status), color, R2)
            })
        }))
    });
    let bar = track(&paint, Indicator::Underline, index, Row::with_children(items).spacing(SP1));
    iced::widget::column![arrows(&paint, bar, R2, options, index, on_select), super::separator(&paint)].into()
}

#[must_use]
pub fn sidebar_list<'a, M: Clone + 'a>(
    paint: &Paint, items: &[(Option<impl super::Glyph>, &'a str)], selected: Option<usize>,
    on_select: impl Fn(usize) -> M,
) -> Element<'a, M> {
    let paint = *paint;
    let rows = items.iter().enumerate().map(|(index, &(glyph, label))| {
        let active = selected == Some(index);
        let tint = if active { paint.acc_soft_fg() } else { paint.fg_3() };
        let mut content = row![].spacing(SP2 + 2.0).align_y(Alignment::Center);
        if let Some(glyph) = glyph {
            content = content.push(icon(glyph, 16.0, tint));
        }
        let style = if active { TextStyle { weight: 600, ..TextStyle::CALL } } else { TextStyle::CALL };
        content = content.push(text(label, style).wrapping(Wrapping::None));
        let message = on_select(index);
        let control = button(container(content).center_y(Fill).clip(true))
            .on_press(message.clone())
            .width(Fill)
            .height(component::CONTROL)
            .padding(Padding::from([0.0, SP3]));
        let control = widgets::button(control, move |status| {
            let color = if active { paint.acc_soft_fg() } else { paint.fg_2() };
            let background = if active { Color::TRANSPARENT } else { paint.layered(Color::TRANSPARENT, status) };
            style::flat(background, color, R2)
        });
        focusable(&paint, control, R2).inset().on_activate(Some(message)).into()
    });
    track(&paint, Indicator::Selection, selected, Column::with_children(rows).spacing(2))
}

#[must_use]
pub fn chip<'a, M: Clone + 'a>(
    paint: &Paint, label: impl Into<Cow<'a, str>>, on: bool, message: Option<M>,
) -> Element<'a, M> {
    let paint = *paint;
    let label = label.into();
    animate(if on { 1.0 } else { 0.0 }, Speed::COLOR, move |value| {
        let value = value.clone();
        let mark =
            if on { icon(Icon::Check, 12.0, paint.acc_soft_fg()) } else { iced::widget::Space::new().width(0).into() };
        let control = button(
            container(
                row![mark, text(label, TextStyle::CAP).wrapping(Wrapping::None)]
                    .spacing(SP1)
                    .align_y(Alignment::Center),
            )
            .center_y(Fill),
        )
        .on_press_maybe(message.clone())
        .height(component::SMALL_CONTROL)
        .padding(Padding::from([0.0, SP3]));
        let control = widgets::button(control, move |status| {
            let amount = value.get();
            let status = if status == button::Status::Disabled { button::Status::Active } else { status };
            let base = super::mix(paint.fill_1(), paint.acc_soft(), amount);
            let mut style = style::flat(
                paint.layered(base, status),
                super::mix(paint.fg_2(), paint.acc_soft_fg(), amount),
                R_ROUND,
            );
            style.border = Border { color: paint.line().scale_alpha(1.0 - amount), width: 1.0, radius: R_ROUND.into() };
            style
        });
        activatable(&paint, control, component::SMALL_CONTROL / 2.0, message)
    })
    .into()
}

#[must_use]
pub fn breadcrumbs<'a, M: Clone + 'a>(
    paint: &Paint, crumbs: &[&'a str], on_select: impl Fn(usize) -> M,
) -> Element<'a, M> {
    let last = crumbs.len().saturating_sub(1);
    let items = crumbs.iter().enumerate().map(|(index, &label)| {
        let current = index == last;
        Crumb::new(paint, label).current(current).on_press((!current).then(|| on_select(index))).into()
    });
    crumb_trail(paint, items.collect())
}

#[must_use]
pub fn crumb_trail<'a, M: 'a>(paint: &Paint, crumbs: Vec<Element<'a, M>>) -> Element<'a, M> {
    let mut trail = Row::new().spacing(2).align_y(Alignment::Center);
    for (index, crumb) in crumbs.into_iter().enumerate() {
        if index > 0 {
            trail = trail.push(icon(Icon::Right, 10.0, paint.fg_4()));
        }
        trail = trail.push(crumb);
    }
    trail.into()
}

type Painted<'a, M> = Box<dyn FnOnce() -> Element<'a, M> + 'a>;

pub struct Crumb<'a, M> {
    paint: Paint,
    label: String,
    glyph: Option<Painted<'a, M>>,
    current: bool,
    hovered: bool,
    message: Option<M>,
}

impl<'a, M: Clone + 'a> Crumb<'a, M> {
    #[must_use]
    pub fn new(paint: &Paint, label: impl Into<String>) -> Self {
        Self { paint: *paint, label: label.into(), glyph: None, current: false, hovered: false, message: None }
    }

    #[must_use]
    pub fn glyph(mut self, glyph: impl super::Glyph + 'a, tint: Color) -> Self {
        self.glyph = Some(Box::new(move || icon(glyph, 13.0, tint)));
        self
    }

    #[must_use]
    pub fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    #[must_use]
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    #[must_use]
    pub fn on_press(mut self, message: Option<M>) -> Self {
        self.message = message;
        self
    }
}

impl<'a, M: Clone + 'a> From<Crumb<'a, M>> for Element<'a, M> {
    fn from(crumb: Crumb<'a, M>) -> Self {
        let Crumb { paint, label, glyph, current, hovered, message } = crumb;
        let mut face = Row::new().spacing(SP1).align_y(Alignment::Center);
        if let Some(glyph) = glyph {
            face = face.push(glyph());
        }
        if !label.is_empty() {
            let style = if current { TextStyle { weight: 600, ..TextStyle::CALL } } else { TextStyle::CALL };
            face = face.push(text(label, style).wrapping(Wrapping::None));
        }
        let control = button(container(face).center_y(Fill))
            .on_press_maybe(message.clone())
            .height(component::SMALL_CONTROL)
            .padding(Padding::from([0.0, SP2]));
        let control = widgets::button(control, move |status| {
            let status = match status {
                button::Status::Active | button::Status::Disabled if hovered => button::Status::Hovered,
                button::Status::Disabled => button::Status::Active,
                other => other,
            };
            let color = if current || status != button::Status::Active { paint.fg_1() } else { paint.fg_2() };
            style::flat(paint.layered(Color::TRANSPARENT, status), color, R1)
        });
        activatable(&paint, control, R1, message)
    }
}

pub const DOT: f32 = 8.0;
pub const DOT_ACTIVE: f32 = 22.0;

#[must_use]
pub fn page_dots<'a, M: Clone + 'a>(
    paint: &Paint, count: usize, current: usize, on_select: impl Fn(usize) -> M,
) -> Element<'a, M> {
    let paint = *paint;
    let dots = (0..count).map(|index| {
        let active = index == current;
        let dot = animate(if active { 1.0 } else { 0.0 }, Speed::SLIDE, move |value| {
            let amount = value.get().clamp(0.0, 1.0);
            let color = mix(paint.fill_3(), paint.acc_bg(), amount);
            container(Space::new().width(DOT + (DOT_ACTIVE - DOT) * amount).height(DOT))
                .style(move |_| style::rounded(color, R_ROUND))
        });
        let target = container(dot).padding(Padding::from([SP1, 3.0]));
        mouse_area(target).on_release(on_select(index)).interaction(mouse::Interaction::Pointer).into()
    });
    let options: Vec<(usize, ())> = (0..count).map(|index| (index, ())).collect();
    let row = Row::with_children(dots).align_y(Alignment::Center);
    arrows(&paint, row, R_ROUND, &options, (current < count).then_some(current), on_select)
}

#[must_use]
pub fn swatches<'a, T: Copy + PartialEq + 'a, M: Clone + 'a>(
    paint: &Paint, colors: &[(T, Color)], selected: Option<T>, on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    let items = colors
        .iter()
        .map(|&(value, color)| swatch(paint, color, selected == Some(value), Some(on_select(value)), SWATCH));
    Row::with_children(items).spacing(SP2).align_y(Alignment::Center).into()
}

#[must_use]
pub fn swatch<'a, M: Clone + 'a>(
    paint: &Paint, color: Color, selected: bool, message: Option<M>, side: f32,
) -> Element<'a, M> {
    let paint = *paint;
    let swatch = animate(if selected { 1.0 } else { 0.0 }, Speed::OVERSHOOT, move |shown| {
        let shown = shown.clone();
        let ink = if 0.299 * color.r + 0.587 * color.g + 0.114 * color.b > 0.62 { Color::BLACK } else { Color::WHITE };
        let mark: Element<'a, M> =
            if selected { icon(Icon::Check, (side * 0.5).round(), ink) } else { Space::new().into() };
        let dot = container(mark).center(side).style(move |_| style::rounded(color, R_ROUND));
        container(dot).padding(3).style(move |_| {
            let amount = shown.get().clamp(0.0, 1.2);
            container::Style {
                border: Border {
                    color: paint.acc_bg().scale_alpha(amount.min(1.0)),
                    width: 2.0,
                    radius: R_ROUND.into(),
                },
                ..container::Style::default()
            }
        })
    })
    .scale(|value| 1.0 + 0.32 * value * (1.0 - value));
    let Some(message) = message else { return swatch.into() };
    let control = mouse_area(swatch).on_release(message.clone()).interaction(mouse::Interaction::Pointer);
    activatable(&paint, control, side / 2.0 + 3.0, Some(message))
}

pub const SWATCH: f32 = 24.0;

#[must_use]
pub fn checkbox<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, check: Check, message: Option<M>) -> Element<'a, M> {
    labelled(paint, mark(paint, check, false, message.is_some()), label, message)
}

fn labelled<'a, M: Clone + 'a>(
    paint: &Paint, mark: Element<'a, M>, label: &'a str, message: Option<M>,
) -> Element<'a, M> {
    let enabled = message.is_some();
    let color = if enabled { paint.fg_1() } else { paint.fg_4() };
    let content = row![mark, text(label, TextStyle::CALL).color(color)].spacing(SP2).align_y(Alignment::Center);
    match message {
        Some(message) => activatable(
            paint,
            mouse_area(content).on_release(message.clone()).interaction(mouse::Interaction::Pointer),
            R1,
            Some(message),
        ),
        None => content.into(),
    }
}

#[must_use]
pub fn radio_group<'a, T: Copy + PartialEq + 'a, M: Clone + 'a>(
    paint: &Paint, options: &[(T, &'a str)], selected: Option<T>, on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    let rows = options.iter().map(|&(value, label)| {
        let check = if selected == Some(value) { Check::On } else { Check::Off };
        labelled(paint, mark(paint, check, true, true), label, Some(on_select(value)))
    });
    Column::with_children(rows).spacing(SP2 + 2.0).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::Space;
    use iced::{Point, Size};

    fn renderer() -> Renderer {
        Renderer::Secondary(iced_tiny_skia::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0)))
    }

    fn pump(element: &mut Element<'_, ()>, tree: &mut Tree, renderer: &Renderer) -> bool {
        let limits = layout::Limits::new(Size::ZERO, Size::new(400.0, 100.0));
        let node = element.as_widget_mut().layout(tree, renderer, &limits);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            tree,
            &Event::Window(window::Event::RedrawRequested(std::time::Instant::now())),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &Rectangle::with_size(Size::new(400.0, 100.0)),
        );
        shell.redraw_request() == window::RedrawRequest::NextFrame
    }

    #[test]
    fn the_indicator_travels_between_items_instead_of_jumping() {
        let paint = Paint::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue);
        let make = |selected| {
            track(
                &paint,
                Indicator::Thumb,
                Some(selected),
                Row::with_children([0, 1].map(|_| Space::new().width(40).height(20).into())).spacing(10),
            )
        };
        let renderer = renderer();
        let mut first: Element<'_, ()> = make(0);
        let mut tree = Tree::new(&first);
        pump(&mut first, &mut tree, &renderer);
        let mut second: Element<'_, ()> = make(1);
        tree.diff(&second);
        assert!(pump(&mut second, &mut tree, &renderer), "a new selection asks for frames");
        std::thread::sleep((Speed::SLIDE.duration)() / 2);
        let middle = tree.state.downcast_ref::<Slide>().position.value(Speed::SLIDE);
        assert!(middle > 0.2 && middle < 1.0, "half-way through t2 the thumb is between the items: {middle}");
        std::thread::sleep((Speed::SLIDE.duration)());
        assert_eq!(tree.state.downcast_ref::<Slide>().position.value(Speed::SLIDE), 1.0);
        assert!(!pump(&mut second, &mut tree, &renderer), "a settled thumb stops asking for frames");
    }

    #[test]
    fn the_indicator_slides_between_neighbouring_items() {
        let items = [
            Rectangle::new(Point::new(0.0, 0.0), Size::new(40.0, 28.0)),
            Rectangle::new(Point::new(42.0, 0.0), Size::new(80.0, 28.0)),
        ];
        let rect = Track::<()>::rect(&items, 0.5).expect("two items");
        assert_eq!(rect.x, 21.0);
        assert_eq!(rect.width, 60.0);
        assert_eq!(Track::<()>::rect(&items, 7.0).expect("clamped").x, 42.0);
        assert!(Track::<()>::rect(&[], 0.0).is_none());
    }
}

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
use iced::widget::{Column, Row, button, container, mouse_area, row};
use iced::{
    Alignment, Border, Color, Element, Event, Fill, Length, Padding, Rectangle, Renderer, Shadow, Size, Theme, Vector,
    mouse, window,
};

use super::controls::{Check, mark};
use super::{Icon, Wrapping, icon, text};
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
        let mut changed = false;
        if slide.selected != self.selected {
            if let Some(index) = self.selected {
                if slide.selected.is_none() {
                    slide.position.jump(index as f32);
                }
                slide.bar.jump(0.0);
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
            || slide.bar.animating(Speed::GROW);
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
    paint: &Paint, label: &'a str, active: bool, message: M, height: f32, width: Length,
) -> Element<'a, M> {
    let paint = *paint;
    let style = if active { TextStyle { weight: 560, ..TextStyle::CALL } } else { TextStyle::CALL };
    let label = text(label, style).wrapping(Wrapping::None);
    let control = button(container(label).center_x(Fill).center_y(Fill))
        .on_press(message)
        .height(height)
        .width(width)
        .padding(Padding::from([0.0, SP3 + 2.0]));
    widgets::button(control, move |status| {
        let color = if active { paint.fg_1() } else { paint.fg_2() };
        let background = if active { Color::TRANSPARENT } else { paint.layered(Color::TRANSPARENT, status) };
        style::flat(background, color, R1 + 2.0)
    })
}

#[must_use]
pub fn segmented<'a, T: Copy + PartialEq + 'a, M: Clone + 'a>(
    paint: &Paint, options: &[(T, &'a str)], selected: T, on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    let paint = *paint;
    let index = options.iter().position(|(value, _)| *value == selected);
    let segments = options.iter().map(|&(value, label)| {
        segment_button(&paint, label, value == selected, on_select(value), component::SMALL_CONTROL, Length::Shrink)
    });
    let content = Row::with_children(segments).spacing(2);
    container(track(&paint, Indicator::Thumb, index, content))
        .padding((component::CONTROL - component::SMALL_CONTROL) / 2.0)
        .style(move |_| style::rounded(paint.fill_1(), R2))
        .into()
}

#[must_use]
pub fn tabs<'a, T: Copy + PartialEq + 'a, M: Clone + 'a>(
    paint: &Paint, options: &[(T, &'a str)], selected: T, on_select: impl Fn(T) -> M,
) -> Element<'a, M> {
    let paint = *paint;
    let index = options.iter().position(|(value, _)| *value == selected);
    let items = options.iter().map(|&(value, label)| {
        let active = value == selected;
        let style = if active { TextStyle { weight: 600, ..TextStyle::CALL } } else { TextStyle::CALL };
        let control = button(container(text(label, style).wrapping(Wrapping::None)).center_y(Fill))
            .on_press(on_select(value))
            .height(component::CONTROL + SP1)
            .padding(Padding::from([0.0, SP3]));
        widgets::button(control, move |status| {
            let color = if active {
                paint.acc_text()
            } else if status == button::Status::Hovered {
                paint.fg_1()
            } else {
                paint.fg_2()
            };
            style::flat(paint.layered(Color::TRANSPARENT, status), color, R2)
        })
    });
    let bar = track(&paint, Indicator::Underline, index, Row::with_children(items).spacing(SP1));
    iced::widget::column![bar, super::separator(&paint)].into()
}

#[must_use]
pub fn sidebar_list<'a, M: Clone + 'a>(
    paint: &Paint, items: &[(Option<Icon>, &'a str)], selected: Option<usize>, on_select: impl Fn(usize) -> M,
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
        let control = button(container(content).center_y(Fill).clip(true))
            .on_press(on_select(index))
            .width(Fill)
            .height(component::CONTROL)
            .padding(Padding::from([0.0, SP3]));
        widgets::button(control, move |status| {
            let color = if active { paint.acc_soft_fg() } else { paint.fg_2() };
            let background = if active { Color::TRANSPARENT } else { paint.layered(Color::TRANSPARENT, status) };
            style::flat(background, color, R2)
        })
    });
    track(&paint, Indicator::Selection, selected, Column::with_children(rows).spacing(2))
}

#[must_use]
pub fn chip<'a, M: Clone + 'a>(paint: &Paint, label: &'a str, on: bool, message: M) -> Element<'a, M> {
    let paint = *paint;
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
        .on_press(message)
        .height(component::SMALL_CONTROL)
        .padding(Padding::from([0.0, SP3]));
        widgets::button(control, move |status| {
            let amount = value.get();
            let base = super::mix(paint.fill_1(), paint.acc_soft(), amount);
            let mut style = style::flat(
                paint.layered(base, status),
                super::mix(paint.fg_2(), paint.acc_soft_fg(), amount),
                R_ROUND,
            );
            style.border = Border { color: paint.line().scale_alpha(1.0 - amount), width: 1.0, radius: R_ROUND.into() };
            style
        })
    })
    .into()
}

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
        Some(message) => mouse_area(content).on_release(message).interaction(mouse::Interaction::Pointer).into(),
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
    use iced::Point;

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

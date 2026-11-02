use std::cell::Cell;
use std::rc::Rc;

use glide_theme::motion;
use glide_ui::Tween;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{Operation, Tree, Widget, operation, tree};
use iced::advanced::{Clipboard, Shell, overlay};
use iced::widget::button as control;
use iced::{
    Background, Border, Color, Element, Event, Length, Rectangle, Renderer, Size, Theme, Transformation, Vector, mouse,
};

use crate::style::{Paint, R2};

enum Decoration {
    Button(Rc<Cell<(f32, f32)>>),
    Layer(Rc<Cell<(f32, f32)>>),
    Field(FieldColors),
    Scale(f32),
    Lift { scale: f32, dy: f32 },
    Shadow(Vec<iced::Shadow>, iced::border::Radius),
}

struct FieldColors {
    background: Color,
    border: Color,
    hover: Color,
    accent: Color,
    ring: Color,
    tinted: bool,
}

struct Decorated<'a, Message> {
    child: Element<'a, Message>,
    decoration: Decoration,
}

struct State {
    hover: Tween,
    press: Tween,
    focus: Tween,
}

impl Default for State {
    fn default() -> Self {
        Self { hover: Tween::at(0.0), press: Tween::at(0.0), focus: Tween::at(0.0) }
    }
}

pub fn button<'a, Message: Clone + 'a>(
    control: control::Button<'a, Message>, style: impl Fn(control::Status) -> control::Style + 'a,
) -> Element<'a, Message> {
    let amounts = Rc::new(Cell::new((0.0, 0.0)));
    let values = Rc::clone(&amounts);
    let child = control.style(move |_, status| {
        if status == control::Status::Disabled {
            return style(status);
        }
        let (hover, press) = values.get();
        blend(
            &blend(&style(control::Status::Active), &style(control::Status::Hovered), hover),
            &style(control::Status::Pressed),
            press,
        )
    });
    Element::new(Decorated { child: child.into(), decoration: Decoration::Button(amounts) })
}

pub fn field<'a, Message: 'a>(child: impl Into<Element<'a, Message>>, paint: &Paint) -> Element<'a, Message> {
    Element::new(Decorated {
        child: child.into(),
        decoration: Decoration::Field(FieldColors {
            background: paint.view_bg(),
            border: paint.line_2(),
            hover: paint.fill_3(),
            accent: paint.acc_bg(),
            ring: paint.acc_ring(),
            tinted: false,
        }),
    })
}

pub fn search_field<'a, Message: 'a>(child: impl Into<Element<'a, Message>>, paint: &Paint) -> Element<'a, Message> {
    Element::new(Decorated {
        child: child.into(),
        decoration: Decoration::Field(FieldColors {
            background: paint.fill_1(),
            border: Color::TRANSPARENT,
            hover: paint.fill_3(),
            accent: paint.acc_bg(),
            ring: paint.acc_ring(),
            tinted: false,
        }),
    })
}

pub fn tinted_field<'a, Message: 'a>(
    child: impl Into<Element<'a, Message>>, paint: &Paint, accent: Color, ring: Color,
) -> Element<'a, Message> {
    Element::new(Decorated {
        child: child.into(),
        decoration: Decoration::Field(FieldColors {
            background: paint.view_bg(),
            border: accent,
            hover: accent,
            accent,
            ring,
            tinted: true,
        }),
    })
}

pub fn container<'a, Message: 'a>(
    panel: iced::widget::Container<'a, Message>, style: impl Fn(f32, f32) -> iced::widget::container::Style + 'a,
) -> Element<'a, Message> {
    let amounts = Rc::new(Cell::new((0.0, 0.0)));
    let values = Rc::clone(&amounts);
    let child = panel.style(move |_| {
        let (hover, press) = values.get();
        style(hover, press)
    });
    Element::new(Decorated { child: child.into(), decoration: Decoration::Layer(amounts) })
}

pub fn scale<'a, Message: 'a>(child: impl Into<Element<'a, Message>>, scale: f32) -> Element<'a, Message> {
    Element::new(Decorated { child: child.into(), decoration: Decoration::Scale(scale) })
}

pub fn lift<'a, Message: 'a>(child: impl Into<Element<'a, Message>>, scale: f32, dy: f32) -> Element<'a, Message> {
    Element::new(Decorated { child: child.into(), decoration: Decoration::Lift { scale, dy } })
}

pub fn surface<'a, Message: 'a>(
    child: impl Into<Element<'a, Message>>, paint: &Paint, level: glide_theme::elevation::Level,
    radius: impl Into<iced::border::Radius>,
) -> Element<'a, Message> {
    let layers = glide_theme::elevation::layers(level, paint.tokens.appearance)
        .into_iter()
        .map(|layer| iced::Shadow {
            color: crate::style::to_iced(layer.color).scale_alpha(paint.alpha),
            offset: Vector::new(layer.dx, layer.dy),
            blur_radius: layer.blur,
        })
        .collect();
    Element::new(Decorated { child: child.into(), decoration: Decoration::Shadow(layers, radius.into()) })
}

fn color(from: Color, to: Color, amount: f32) -> Color {
    crate::style::to_iced(crate::style::from_iced(from).lerp_premultiplied(crate::style::from_iced(to), amount))
}

fn blend(from: &control::Style, to: &control::Style, amount: f32) -> control::Style {
    let mix = |from: f32, to: f32| from + (to - from) * amount;
    let nearest = if amount < 0.5 { from } else { to };
    let background = match (from.background, to.background) {
        (None, None) => None,
        (Some(Background::Gradient(_)), _) | (_, Some(Background::Gradient(_))) => nearest.background,
        (start, end) => {
            let solid = |background: Option<Background>| match background {
                Some(Background::Color(color)) => color,
                _ => Color::TRANSPARENT,
            };
            Some(Background::Color(color(solid(start), solid(end), amount)))
        }
    };
    control::Style {
        background,
        text_color: color(from.text_color, to.text_color, amount),
        border: Border {
            color: color(from.border.color, to.border.color, amount),
            width: mix(from.border.width, to.border.width),
            radius: nearest.border.radius,
        },
        shadow: iced::Shadow {
            color: color(from.shadow.color, to.shadow.color, amount),
            offset: Vector::new(
                mix(from.shadow.offset.x, to.shadow.offset.x),
                mix(from.shadow.offset.y, to.shadow.offset.y),
            ),
            blur_radius: mix(from.shadow.blur_radius, to.shadow.blur_radius),
        },
        snap: nearest.snap,
    }
}

#[derive(Default)]
struct Focus(bool);

impl Operation for Focus {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(
        &mut self, _: Option<&iced::advanced::widget::Id>, _: Rectangle, state: &mut dyn operation::Focusable,
    ) {
        self.0 |= state.is_focused();
    }
}

impl<Message> Widget<Message, Theme, Renderer> for Decorated<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.child]);
    }
    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }
    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        self.child.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.child.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(
        &mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer,
        clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, viewport: &Rectangle,
    ) {
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        let field = match self.decoration {
            Decoration::Field(_) => true,
            Decoration::Button(_) | Decoration::Layer(_) => false,
            Decoration::Scale(_) | Decoration::Lift { .. } | Decoration::Shadow(..) => return,
        };
        let mut focused = Focus::default();
        if field {
            self.child.as_widget_mut().operate(&mut tree.children[0], layout, renderer, &mut focused);
        }
        let state = tree.state.downcast_mut::<State>();
        let hover = cursor.is_over(layout.bounds());
        let mut changed = state.hover.retarget(f64::from(hover), motion::t1(), motion::ease_io);
        changed |= state.focus.retarget(f64::from(focused.0), motion::t1(), motion::ease_io);
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if hover
                    && !field
                    && (!matches!(self.decoration, Decoration::Button(_))
                        || self.child.as_widget().mouse_interaction(
                            &tree.children[0],
                            layout,
                            cursor,
                            viewport,
                            renderer,
                        ) == mouse::Interaction::Pointer) =>
            {
                state.press.jump(1.0);
                changed = true;
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Window(iced::window::Event::Unfocused) => {
                changed |= state.press.retarget(0.0, motion::t1(), motion::ease_out);
            }
            _ => {}
        }
        if changed
            || state.hover.is_animating(motion::t1())
            || state.press.is_animating(motion::t1())
            || state.focus.is_animating(motion::t1())
        {
            shell.request_redraw();
        }
    }
    fn draw(
        &self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>,
        cursor: mouse::Cursor, viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let press = state.press.value(motion::t1(), motion::ease_out) as f32;
        let scale = match &self.decoration {
            Decoration::Button(values) => {
                values.set((state.hover.value(motion::t1(), motion::ease_io) as f32, press));
                1.0 - (1.0 - glide_theme::component::BUTTON_PRESSED_SCALE) * press
            }
            Decoration::Layer(values) => {
                values.set((state.hover.value(motion::t1(), motion::ease_io) as f32, press));
                1.0
            }
            Decoration::Field(colors) => {
                let bounds = layout.bounds();
                let focus = state.focus.value(motion::t1(), motion::ease_io) as f32;
                let hover = state.hover.value(motion::t1(), motion::ease_io) as f32;
                renderer.fill_quad(
                    Quad {
                        bounds: bounds.expand(glide_theme::component::FIELD_HALO),
                        border: Border {
                            color: colors.ring.scale_alpha(if colors.tinted { 1.0 } else { focus }),
                            width: glide_theme::component::FIELD_HALO,
                            radius: (R2 + glide_theme::component::FIELD_HALO).into(),
                        },
                        ..Quad::default()
                    },
                    Color::TRANSPARENT,
                );
                renderer.fill_quad(
                    Quad {
                        bounds,
                        border: Border {
                            color: color(color(colors.border, colors.hover, hover), colors.accent, focus),
                            width: 1.0,
                            radius: R2.into(),
                        },
                        ..Quad::default()
                    },
                    colors.background,
                );
                1.0
            }
            Decoration::Scale(scale) => *scale,
            Decoration::Lift { scale, dy } => {
                if (*scale - 1.0).abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
                    self.child.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
                    return;
                }
                let center = layout.bounds().center();
                let transform = Transformation::translate(center.x, center.y + dy)
                    * Transformation::scale(*scale)
                    * Transformation::translate(-center.x, -center.y);
                renderer.with_transformation(transform, |renderer| {
                    self.child.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
                });
                return;
            }
            Decoration::Shadow(layers, radius) => {
                for shadow in layers {
                    renderer.fill_quad(
                        Quad {
                            bounds: layout.bounds(),
                            border: Border { radius: *radius, ..Border::default() },
                            shadow: *shadow,
                            ..Quad::default()
                        },
                        Color::TRANSPARENT,
                    );
                }
                1.0
            }
        };
        let center = layout.bounds().center();
        let transform = Transformation::translate(center.x, center.y)
            * Transformation::scale(scale)
            * Transformation::translate(-center.x, -center.y);
        renderer.with_transformation(transform, |renderer| {
            self.child.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
        });
    }
    fn mouse_interaction(
        &self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &Renderer,
    ) -> mouse::Interaction {
        self.child.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }
    fn overlay<'a>(
        &'a mut self, tree: &'a mut Tree, layout: Layout<'a>, renderer: &Renderer, viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        self.child.as_widget_mut().overlay(&mut tree.children[0], layout, renderer, viewport, translation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glide_theme::{
        Accent, Appearance,
        elevation::{self, Level},
    };

    fn pressed_after_click(control: control::Button<'_, ()>) -> f64 {
        let mut view = button(control.width(80).height(40), |_| control::Style::default());
        let mut tree = Tree::new(&view);
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0)));
        let node = view.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(500.0, 500.0)),
        );
        let mut messages = Vec::new();
        view.as_widget_mut().update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Layout::new(&node),
            mouse::Cursor::Available(iced::Point::new(40.0, 20.0)),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(Size::new(500.0, 500.0)),
        );
        tree.state.downcast_ref::<State>().press.target()
    }

    #[test]
    fn a_resting_button_keeps_its_own_shadow_and_rises_to_the_pressed_one() {
        let shadow = |blur: f32, dy: f32| iced::Shadow {
            color: Color { a: 0.2, ..Color::BLACK },
            offset: Vector::new(0.0, dy),
            blur_radius: blur,
        };
        let rest = control::Style { shadow: shadow(2.0, 1.0), ..control::Style::default() };
        let pressed = control::Style {
            shadow: shadow(8.0, 4.0),
            border: Border { width: 2.0, ..Border::default() },
            ..control::Style::default()
        };
        assert_eq!(blend(&rest, &pressed, 0.0).shadow, rest.shadow);
        assert_eq!(blend(&rest, &pressed, 0.0).border.width, 0.0);
        assert_eq!(blend(&rest, &pressed, 1.0).shadow, pressed.shadow);
        let halfway = blend(&rest, &pressed, 0.5).shadow;
        assert_eq!(halfway.blur_radius, 5.0);
        assert_eq!(halfway.offset, Vector::new(0.0, 2.5));
    }

    #[test]
    fn gradient_backgrounds_survive_the_state_blend() {
        let gradient = Background::Gradient(iced::Gradient::Linear(iced::gradient::Linear::new(0.0)));
        let rest = control::Style { background: Some(gradient), ..control::Style::default() };
        let hovered = control::Style { background: Some(Background::Color(Color::WHITE)), ..rest };
        assert_eq!(blend(&rest, &hovered, 0.2).background, Some(gradient));
        assert_eq!(blend(&rest, &hovered, 0.8).background, Some(Background::Color(Color::WHITE)));
        let plain = control::Style { background: None, ..rest };
        assert_eq!(blend(&plain, &plain, 0.5).background, None);
    }

    fn field_quads(view: Element<'_, ()>) -> Vec<Quad> {
        let mut view = view;
        let mut tree = Tree::new(&view);
        let mut renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0)));
        renderer.reset(Rectangle::with_size(Size::new(500.0, 500.0)));
        let node = view.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(500.0, 500.0)),
        );
        view.as_widget().draw(
            &tree,
            &mut renderer,
            &Theme::Dark,
            &renderer::Style { text_color: Color::BLACK },
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &Rectangle::with_size(Size::new(500.0, 500.0)),
        );
        let Renderer::Secondary(mut renderer) = renderer else { unreachable!() };
        renderer.layers().iter().flat_map(|layer| &layer.quads).map(|(quad, _)| *quad).collect()
    }

    #[test]
    fn validated_fields_keep_their_ring_while_plain_fields_wait_for_focus() {
        let paint = Paint::default();
        let space = || iced::widget::Space::new().width(80).height(30);
        let ring = |quads: &[Quad]| quads.first().map(|quad| quad.border.color.a).expect("ring quad");
        let tinted = field_quads(tinted_field(space(), &paint, paint.danger(), paint.danger_soft()));
        assert_eq!(ring(&tinted), paint.danger_soft().a);
        let (border, danger) = (tinted[1].border.color, paint.danger());
        assert!([border.r - danger.r, border.g - danger.g, border.b - danger.b].iter().all(|d| d.abs() < 1e-3));
        assert_eq!(ring(&field_quads(field(space(), &paint))), 0.0);
    }

    #[test]
    fn only_enabled_buttons_shrink_when_pressed() {
        let space = || iced::widget::Space::new().width(80).height(40);
        assert_eq!(pressed_after_click(iced::widget::button(space()).on_press(())), 1.0);
        assert_eq!(pressed_after_click(iced::widget::button(space())), 0.0);
    }

    #[test]
    fn every_elevation_layer_reaches_the_renderer_in_both_appearances() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let paint = Paint::new(appearance, Accent::Blue).faded(0.5);
            for level in [Level::El1, Level::El2, Level::El3] {
                let mut view: Element<'_, ()> =
                    surface(iced::widget::Space::new().width(80).height(40), &paint, level, R2);
                let mut tree = Tree::new(&view);
                let mut renderer =
                    Renderer::Secondary(iced_tiny_skia::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0)));
                renderer.reset(Rectangle::with_size(Size::new(500.0, 500.0)));
                let node = view.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(Size::ZERO, Size::new(500.0, 500.0)),
                );
                view.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &Theme::Dark,
                    &renderer::Style { text_color: Color::BLACK },
                    Layout::new(&node),
                    mouse::Cursor::Unavailable,
                    &Rectangle::with_size(Size::new(500.0, 500.0)),
                );
                let Renderer::Secondary(mut renderer) = renderer else { unreachable!() };
                let actual: Vec<_> =
                    renderer.layers().iter().flat_map(|layer| &layer.quads).map(|(quad, _)| quad.shadow).collect();
                let expected = elevation::layers(level, appearance);
                assert_eq!(actual.len(), expected.len());
                for (actual, expected) in actual.iter().zip(expected) {
                    assert_eq!(actual.blur_radius, expected.blur);
                    assert_eq!(actual.offset, Vector::new(expected.dx, expected.dy));
                    assert_eq!(actual.color, crate::style::to_iced(expected.color).scale_alpha(paint.alpha));
                }
            }
        }
    }
}

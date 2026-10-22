use glide_iced::Paint;
use glide_iced::style::{self, R2, R3, SP1, SP2, SP3, SP4};
use glide_iced::widgets;
use glide_theme::elevation::Level;
use glide_theme::{TextStyle, component, motion};
use iced::widget::{Column, Space, button as control, column, container, mouse_area, row, stack};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding, Vector, mouse};

use super::controls::{chevron, switch_face};
use super::focus::{activatable, focusable};
use super::{Icon, Wrapping, icon, mix, separator, text};
use crate::anim::{Speed, animate, collapse, hover};

#[must_use]
pub fn card_style(paint: &Paint) -> container::Style {
    container::Style {
        text_color: Some(paint.fg_1()),
        background: Some(Background::Color(paint.card_bg())),
        border: Border { color: paint.line(), width: 1.0, radius: R3.into() },
        ..container::Style::default()
    }
}

#[must_use]
pub fn card<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let paint = *paint;
    widgets::surface(
        container(content).padding(SP4).width(Fill).style(move |_| card_style(&paint)),
        &paint,
        Level::El1,
        R3,
    )
}

#[must_use]
pub fn action_card<'a, M: Clone + 'a>(paint: &Paint, content: impl Into<Element<'a, M>>, message: M) -> Element<'a, M> {
    let paint = *paint;
    let content = content.into();
    let lifted = hover(Speed::SPRING, move |value| {
        let value = value.clone();
        widgets::surface(
            container(content).padding(SP4).width(Fill).style(move |_| {
                let amount = value.get().clamp(0.0, 1.0);
                container::Style {
                    border: Border { color: mix(paint.line(), paint.line_2(), amount), width: 1.0, radius: R3.into() },
                    ..card_style(&paint)
                }
            }),
            &paint,
            Level::El1,
            R3,
        )
    })
    .offset(|value| Vector::new(0.0, -3.0 * value));
    let control = mouse_area(lifted).on_release(message.clone()).interaction(mouse::Interaction::Pointer);
    activatable(&paint, control, R3, Some(message))
}

#[must_use]
pub fn list<'a, M: 'a>(paint: &Paint, rows: Vec<Element<'a, M>>) -> Element<'a, M> {
    let paint = *paint;
    let mut items = column![];
    let count = rows.len();
    for (index, item) in rows.into_iter().enumerate() {
        items = items.push(item);
        if index + 1 < count {
            items = items.push(container(separator(&paint)).padding(Padding::from([0.0, SP4])));
        }
    }
    widgets::surface(container(items).width(Fill).clip(true).style(move |_| card_style(&paint)), &paint, Level::El1, R3)
}

fn row_label<'a, M: 'a>(paint: &Paint, title: &'a str, subtitle: Option<&'a str>) -> Element<'a, M> {
    let mut label = column![text(title, TextStyle::CALL)].spacing(2);
    if let Some(subtitle) = subtitle {
        label = label.push(text(subtitle, TextStyle::CAP).color(paint.fg_3()));
    }
    container(label).width(Fill).into()
}

#[must_use]
pub fn list_row<'a, M: 'a>(
    paint: &Paint, title: &'a str, subtitle: Option<&'a str>, trailing: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    container(row![row_label(paint, title, subtitle), trailing.into()].spacing(SP3).align_y(Alignment::Center))
        .padding(Padding::from([SP2, SP4]))
        .width(Fill)
        .into()
}

#[must_use]
pub fn well<'a, M: 'a>(paint: &Paint, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let background = paint.fill_1();
    container(content).padding(SP1).width(Fill).style(move |_| style::rounded(background, R2)).into()
}

#[must_use]
pub fn select_row<'a, M: Clone + 'a>(
    paint: &Paint, leading: Option<Element<'a, M>>, label: impl Into<std::borrow::Cow<'a, str>>, selected: bool,
    message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let label = label.into();
    let picked = message.clone();
    let row = animate(if selected { 1.0 } else { 0.0 }, Speed::COLOR, move |value| {
        let value = value.clone();
        let mut content = row![].spacing(SP3).align_y(Alignment::Center);
        if let Some(leading) = leading {
            content = content.push(leading);
        }
        content = content.push(text(label, TextStyle::CALL).wrapping(Wrapping::None)).push(Space::new().width(Fill));
        if selected {
            content = content.push(icon(Icon::Check, 16.0, paint.acc_soft_fg()));
        }
        let control = control(container(content).center_y(Fill).clip(true))
            .on_press(picked)
            .width(Fill)
            .height(component::CONTROL)
            .padding(Padding::from([0.0, SP3]));
        widgets::button(control, move |status| {
            let amount = value.get().clamp(0.0, 1.0);
            let background = mix(paint.layered(Color::TRANSPARENT, status), paint.acc_soft(), amount);
            style::flat(background, mix(paint.fg_1(), paint.acc_soft_fg(), amount), R2)
        })
    });
    focusable(&paint, row, R2).inset().on_activate(Some(message)).into()
}

pub const TREE_ROW: f32 = 28.0;
pub const TREE_INDENT: f32 = 14.0;

#[must_use]
pub fn tree_row<'a, M: Clone + 'a>(
    paint: &Paint, depth: usize, expanded: Option<bool>, glyph: impl super::Glyph, label: &'a str, current: bool,
    message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let chevron: Element<'a, M> = match expanded {
        Some(open) => chevron(&paint, open),
        None => Space::new().width(12).height(12).into(),
    };
    let glyph = icon(glyph, 15.0, if current { paint.acc_bg() } else { paint.fg_3() });
    let style = if current { TextStyle::HEAD } else { TextStyle::CALL };
    let indent = SP2 + depth as f32 * TREE_INDENT;
    let content = row![
        Space::new().width(indent),
        container(chevron).center(12),
        glyph,
        text(label, style).wrapping(Wrapping::None)
    ]
    .spacing(SP1 + 2.0)
    .align_y(Alignment::Center);
    let control = control(container(content).center_y(Fill).clip(true))
        .on_press(message.clone())
        .width(Fill)
        .height(TREE_ROW)
        .padding(0);
    let control = widgets::button(control, move |status| {
        let base = if current { paint.acc_soft() } else { Color::TRANSPARENT };
        style::flat(paint.layered(base, status), if current { paint.fg_1() } else { paint.fg_2() }, R2)
    });
    focusable(&paint, control, R2).inset().on_activate(Some(message)).into()
}

pub const STATUS_BAR: f32 = 26.0;

#[must_use]
pub fn status_bar<'a, M: 'a>(
    paint: &Paint, left: Vec<Element<'a, M>>, right: Vec<Element<'a, M>>, corners: iced::border::Radius,
) -> Element<'a, M> {
    let background = paint.bar_bg();
    let left = iced::widget::Row::with_children(left).spacing(SP3).align_y(Alignment::Center);
    let right = iced::widget::Row::with_children(right).spacing(SP1).align_y(Alignment::Center);
    container(row![left, Space::new().width(Fill), right].align_y(Alignment::Center))
        .padding(Padding::from([0.0, SP3]).right(SP2))
        .width(Fill)
        .height(STATUS_BAR)
        .clip(true)
        .style(move |_| style::rounded(background, corners))
        .into()
}

#[must_use]
pub fn status_button<'a, M: Clone + 'a>(
    paint: &Paint, label: impl Into<std::borrow::Cow<'a, str>>, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let control = control(text(label.into(), TextStyle::CAP).wrapping(Wrapping::None))
        .on_press(message.clone())
        .height(STATUS_BAR - 4.0)
        .padding(Padding::from([2.0, SP2]));
    let control =
        widgets::button(control, move |status| super::buttons::button_style(&paint, super::Tone::Ghost, status));
    activatable(&paint, control, R2, Some(message))
}

#[must_use]
pub fn facts<'a, M: 'a>(paint: &Paint, rows: Vec<(String, String)>, label_width: f32) -> Element<'a, M> {
    let paint = *paint;
    Column::with_children(rows.into_iter().map(|(label, value)| {
        row![
            text(label, TextStyle::CAP).color(paint.fg_2()).align_x(Alignment::End).width(label_width),
            text(value, TextStyle::CAP).width(Fill).wrapping(Wrapping::WordOrGlyph),
        ]
        .spacing(SP3)
        .into()
    }))
    .spacing(SP2)
    .into()
}

#[must_use]
pub fn switch_row<'a, M: Clone + 'a>(
    paint: &Paint, title: &'a str, subtitle: Option<&'a str>, on: bool, on_toggle: Option<impl Fn(bool) -> M + 'a>,
) -> Element<'a, M> {
    let paint = *paint;
    let message = on_toggle.map(|toggle| toggle(!on));
    let content = list_row(&paint, title, subtitle, switch_face(&paint, on, message.is_some()));
    let Some(message) = message else { return content };
    let lit = widgets::container(container(content).width(Fill), move |hover, press| {
        let amount = motion::ST_HOVER * hover + (motion::ST_PRESS - motion::ST_HOVER) * press;
        style::rounded(paint.layer(Color::TRANSPARENT, amount), 0.0)
    });
    let control = mouse_area(lit).on_release(message.clone()).interaction(mouse::Interaction::Pointer);
    focusable(&paint, control, R2).inset().on_activate(Some(message)).into()
}

#[must_use]
pub fn action_row<'a, M: Clone + 'a>(
    paint: &Paint, title: &'a str, subtitle: Option<&'a str>, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let content = row![row_label(&paint, title, subtitle), icon(Icon::Right, 12.0, paint.fg_3())]
        .spacing(SP3)
        .align_y(Alignment::Center);
    let control = control(content).on_press(message.clone()).width(Fill).padding(Padding::from([SP2 + 2.0, SP4]));
    let control = widgets::button(control, move |status| {
        style::flat(paint.layered(Color::TRANSPARENT, status), paint.fg_1(), 0.0)
    });
    focusable(&paint, control, R2).inset().on_activate(Some(message)).into()
}

#[must_use]
pub fn section<'a, M: 'a>(paint: &Paint, title: &'a str, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    column![
        container(text(title, TextStyle::MICRO).color(paint.fg_3())).padding(Padding::from([0.0, SP2])),
        content.into()
    ]
    .spacing(SP2)
    .into()
}

#[must_use]
pub fn expander<'a, M: Clone + 'a>(
    paint: &Paint, title: &'a str, open: bool, on_toggle: M, content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let paint = *paint;
    let header = control(
        row![text(title, TextStyle::HEAD), Space::new().width(Fill), chevron::<M>(&paint, open)]
            .align_y(Alignment::Center),
    )
    .on_press(on_toggle.clone())
    .width(Fill)
    .padding(Padding::from([SP3, SP4]));
    let header = widgets::button(header, move |status| {
        style::flat(paint.layered(Color::TRANSPARENT, status), paint.fg_1(), 0.0)
    });
    let header = focusable(&paint, header, R3).inset().on_activate(Some(on_toggle));
    let body = collapse(open, container(content).padding(Padding::from([0.0, SP4]).bottom(SP4)).width(Fill));
    widgets::surface(
        container(column![header, body]).width(Fill).clip(true).style(move |_| card_style(&paint)),
        &paint,
        Level::El1,
        R3,
    )
}

#[must_use]
pub fn sidebar_item<'a, M: Clone + 'a>(
    paint: &Paint, glyph: Option<impl super::Glyph + 'a>, label: &'a str, selected: bool, message: M,
) -> Element<'a, M> {
    let mut item = SidebarItem::new(paint, label).selected(selected).current(selected).on_press(Some(message));
    if let Some(glyph) = glyph {
        item = item.glyph(glyph);
    }
    item.into()
}

type Drawn<'a, M> = Box<dyn FnOnce(Color) -> Element<'a, M> + 'a>;

#[allow(clippy::struct_excessive_bools)] // Independent visual flags of one row, each set by its own builder call
pub struct SidebarItem<'a, M> {
    paint: Paint,
    glyph: Option<(Drawn<'a, M>, Color)>,
    label: String,
    ink: Option<Color>,
    accessory: Option<Element<'a, M>>,
    message: Option<M>,
    selected: bool,
    current: bool,
    hovered: bool,
    held: bool,
}

impl<'a, M: Clone + 'a> SidebarItem<'a, M> {
    #[must_use]
    pub fn new(paint: &Paint, label: impl Into<String>) -> Self {
        Self {
            paint: *paint,
            glyph: None,
            label: label.into(),
            ink: None,
            accessory: None,
            message: None,
            selected: false,
            current: false,
            hovered: false,
            held: false,
        }
    }

    #[must_use]
    pub fn glyph(self, glyph: impl super::Glyph + 'a) -> Self {
        let tint = self.paint.fg_3();
        self.tinted(glyph, tint)
    }

    #[must_use]
    pub fn tinted(mut self, glyph: impl super::Glyph + 'a, tint: Color) -> Self {
        self.glyph = Some((Box::new(move |tint| icon(glyph, 16.0, tint)), tint));
        self
    }

    #[must_use]
    pub fn ink(mut self, ink: Option<Color>) -> Self {
        self.ink = ink;
        self
    }

    #[must_use]
    pub fn accessory(mut self, accessory: impl Into<Element<'a, M>>) -> Self {
        self.accessory = Some(accessory.into());
        self
    }

    #[must_use]
    pub fn on_press(mut self, message: Option<M>) -> Self {
        self.message = message;
        self
    }

    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
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
    pub fn held(mut self, held: bool) -> Self {
        self.held = held;
        self
    }
}

impl<'a, M: Clone + 'a> From<SidebarItem<'a, M>> for Element<'a, M> {
    fn from(item: SidebarItem<'a, M>) -> Self {
        let SidebarItem { paint, glyph, label, ink, accessory, message, selected, current, hovered, held } = item;
        animate(if selected { 1.0 } else { 0.0 }, Speed::COLOR, move |value| {
            let trailing = if accessory.is_some() { SP1 } else { SP3 };
            let mut content = row![].spacing(SP2 + 2.0).align_y(Alignment::Center);
            if let Some((glyph, tint)) = glyph {
                content = content.push(glyph(if selected { paint.acc_soft_fg() } else { tint }));
            }
            let style = if current { TextStyle { weight: 600, ..TextStyle::CALL } } else { TextStyle::CALL };
            let mut label = text(label, style).wrapping(Wrapping::None).width(Fill);
            if let Some(ink) = ink.filter(|_| !selected) {
                label = label.color(ink);
            }
            content = content.push(label);
            if let Some(accessory) = accessory {
                content = content.push(accessory);
            }
            let control = control(container(content).center_y(Fill).clip(true))
                .on_press_maybe(message.clone())
                .width(Fill)
                .height(component::CONTROL)
                .padding(Padding::from([0.0, SP3]).right(trailing));
            let value = value.clone();
            let item = widgets::button(control, move |status| {
                let status = match status {
                    _ if held => control::Status::Pressed,
                    control::Status::Active | control::Status::Disabled if hovered => control::Status::Hovered,
                    control::Status::Disabled => control::Status::Active,
                    other => other,
                };
                let amount = value.get().clamp(0.0, 1.0);
                let rest = if matches!(status, control::Status::Active) { paint.fg_2() } else { paint.fg_1() };
                let background = mix(paint.layered(Color::TRANSPARENT, status), paint.acc_soft(), amount);
                style::flat(background, mix(rest, paint.acc_soft_fg(), amount), R2)
            });
            let item = focusable(&paint, item, R2).inset().on_activate(message);
            let bar = animate(if current { 1.0 } else { 0.0 }, Speed::GROW, |_| {
                container(Space::new().width(3).height(18)).style(move |_| style::rounded(paint.acc_bg(), 2.0))
            })
            .from(0.0)
            .scale(|value| value.max(0.0));
            stack![item, container(bar).height(component::CONTROL).center_y(component::CONTROL)]
        })
        .into()
    }
}

#[must_use]
pub fn sidebar<'a, M: 'a>(paint: &Paint, width: f32, items: Vec<Element<'a, M>>) -> Element<'a, M> {
    let background = paint.sidebar_bg();
    container(super::scrollable(paint, Column::with_children(items).spacing(2).padding(SP2)))
        .width(width)
        .height(Fill)
        .style(move |_| style::rounded(background, 0.0))
        .into()
}

#[must_use]
pub fn surface<'a, M: 'a>(background: Color, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content).width(Fill).height(Length::Fill).style(move |_| style::rounded(background, 0.0)).into()
}

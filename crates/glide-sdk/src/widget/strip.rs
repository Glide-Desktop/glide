use std::borrow::Cow;

use glide_iced::Paint;
use glide_iced::style::{self, R2, SP1, SP2};
use glide_iced::widgets;
use glide_theme::elevation::Level;
use glide_theme::motion::{ST_HOVER, ST_PRESS};
use glide_theme::{TextStyle, shape::R_ROUND};
use iced::widget::{Space, button, container, mouse_area, row, stack};
use iced::{Alignment, Color, Element, Fill, Padding};

use super::buttons::{Tone, button_style};
use super::{Icon, Wrapping, icon, text, tooltip};

pub const TAB_PAD: f32 = SP2;
pub const TAB_GAP: f32 = SP1;
const CLOSE: f32 = 22.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mark {
    #[default]
    None,
    Dot,
    Alert,
}

#[derive(Debug, Clone)]
pub struct Tab<'a> {
    pub label: Cow<'a, str>,
    pub mark: Mark,
}

impl<'a> Tab<'a> {
    #[must_use]
    pub fn new(label: impl Into<Cow<'a, str>>) -> Self {
        Self { label: label.into(), mark: Mark::None }
    }

    #[must_use]
    pub fn mark(mut self, mark: Mark) -> Self {
        self.mark = mark;
        self
    }
}

type Pick<'a, M> = Box<dyn Fn(usize) -> M + 'a>;

pub struct TabStrip<'a, M> {
    paint: Paint,
    tabs: Vec<Tab<'a>>,
    active: usize,
    hovered: Option<usize>,
    dragged: Option<usize>,
    height: f32,
    select: Option<Pick<'a, M>>,
    close: Option<(Pick<'a, M>, &'a str)>,
    hover: Option<Box<dyn Fn(Option<usize>) -> M + 'a>>,
    drag: Option<Box<dyn Fn(f32) -> M + 'a>>,
}

#[must_use]
pub fn tab_strip<'a, M>(paint: &Paint, tabs: Vec<Tab<'a>>, active: usize) -> TabStrip<'a, M> {
    TabStrip {
        paint: *paint,
        tabs,
        active,
        hovered: None,
        dragged: None,
        height: 36.0,
        select: None,
        close: None,
        hover: None,
        drag: None,
    }
}

#[must_use]
pub fn tab_slot(x: f32, width: f32, count: usize) -> usize {
    let slot = (width - 2.0 * TAB_PAD + TAB_GAP) / count.max(1) as f32;
    (((x - TAB_PAD) / slot.max(1.0)).floor().max(0.0) as usize).min(count.saturating_sub(1))
}

impl<'a, M> TabStrip<'a, M> {
    #[must_use]
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    #[must_use]
    pub fn hovered(mut self, hovered: Option<usize>, on_hover: impl Fn(Option<usize>) -> M + 'a) -> Self {
        self.hovered = hovered;
        self.hover = Some(Box::new(on_hover));
        self
    }

    #[must_use]
    pub fn dragged(mut self, dragged: Option<usize>, on_drag: impl Fn(f32) -> M + 'a) -> Self {
        self.dragged = dragged;
        self.drag = Some(Box::new(on_drag));
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

impl<'a, M: Clone + 'a> From<TabStrip<'a, M>> for Element<'a, M> {
    fn from(strip: TabStrip<'a, M>) -> Self {
        let paint = strip.paint;
        let tabs = strip.tabs.into_iter().enumerate().map(|(index, tab)| {
            let active = index == strip.active;
            let hovered = strip.hovered == Some(index);
            let mut label =
                row![text(tab.label, TextStyle::CAP).wrapping(Wrapping::None)].spacing(SP2).align_y(Alignment::Center);
            match tab.mark {
                Mark::Dot if !hovered => {
                    label = label.push(
                        container(Space::new().width(6).height(6))
                            .style(move |_| style::rounded(paint.fg_3(), R_ROUND)),
                    );
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
            let inner = stack![
                container(label)
                    .center_x(Fill)
                    .center_y(Fill)
                    .padding(Padding::from([0.0, CLOSE + SP1 + 2.0]))
                    .clip(true),
                container(close).padding(Padding::from([0.0, SP1])).height(Fill).align_y(Alignment::Center),
            ];
            let tab = widgets::container(container(inner).width(Fill).height(Fill), move |hover, press| {
                let (background, color) = if active {
                    (paint.view_bg(), paint.fg_1())
                } else {
                    (
                        paint.layer(Color::TRANSPARENT, ST_HOVER * hover + (ST_PRESS - ST_HOVER) * press),
                        if hover > 0.5 { paint.fg_1() } else { paint.fg_2() },
                    )
                };
                container::Style { text_color: Some(color), ..style::rounded(background, R2) }
            });
            let lifted = strip.dragged == Some(index);
            let tab: Element<'a, M> = if active {
                widgets::surface(tab, &paint, if lifted { Level::El2 } else { Level::El1 }, R2)
            } else {
                tab
            };
            let mut area = mouse_area(tab);
            if let Some(select) = &strip.select {
                area = area.on_press(select(index));
            }
            if let Some(hover) = &strip.hover {
                area = area.on_enter(hover(Some(index))).on_exit(hover(None));
            }
            if let Some((close, _)) = &strip.close {
                area = area.on_middle_press(close(index));
            }
            area.into()
        });
        let bar = container(iced::widget::Row::with_children(tabs).spacing(TAB_GAP).height(Fill))
            .padding(Padding::from([SP1, TAB_PAD]))
            .width(Fill)
            .height(strip.height)
            .style(move |_| style::rounded(paint.bar_bg(), 0.0));
        match strip.drag {
            Some(drag) => mouse_area(bar).on_move(move |point| drag(point.x)).into(),
            None => bar.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dragged_tab_lands_in_the_slot_under_the_pointer_and_never_outside_the_strip() {
        let width = 2.0 * TAB_PAD + 4.0 * 197.0 + 3.0 * TAB_GAP;
        assert_eq!(tab_slot(TAB_PAD + 1.0, width, 4), 0);
        assert_eq!(tab_slot(TAB_PAD + 197.0 + TAB_GAP + 1.0, width, 4), 1);
        assert_eq!(tab_slot(width - TAB_PAD - 1.0, width, 4), 3);
        assert_eq!(tab_slot(-40.0, width, 4), 0);
        assert_eq!(tab_slot(width + 400.0, width, 4), 3);
        assert_eq!(tab_slot(100.0, width, 0), 0);
    }
}

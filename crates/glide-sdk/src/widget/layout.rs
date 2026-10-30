use glide_iced::Paint;
use glide_iced::style::{self, R2, R3, SP1, SP2, SP3, SP4, SP5};
use glide_iced::widgets;
use glide_theme::elevation::Level;
use glide_theme::{TextStyle, component};
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
    let lifted = hover(Speed::OVERSHOOT, move |value| {
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

pub const ROW_HEIGHT: f32 = 44.0;
const ROW_PAD_Y: f32 = (ROW_HEIGHT - component::CONTROL) / 2.0;

#[must_use]
pub fn list<'a, M: 'a>(paint: &Paint, rows: Vec<Element<'a, M>>) -> Element<'a, M> {
    let paint = *paint;
    let mut items = column![];
    let count = rows.len();
    for (index, item) in rows.into_iter().enumerate() {
        items = items.push(item);
        if index + 1 < count {
            items = items.push(separator(&paint));
        }
    }
    let boxed = container(items).width(Fill).style(move |_| card_style(&paint));
    widgets::surface(boxed, &paint, Level::El1, R3)
}

pub struct Group<'a, M> {
    paint: Paint,
    title: Option<String>,
    description: Option<String>,
    suffix: Option<Element<'a, M>>,
    rows: Vec<Element<'a, M>>,
}

impl<'a, M: 'a> Group<'a, M> {
    #[must_use]
    pub fn new(paint: impl Into<Paint>) -> Self {
        Self { paint: paint.into(), title: None, description: None, suffix: None, rows: Vec::new() }
    }

    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    #[must_use]
    pub fn suffix(mut self, suffix: impl Into<Element<'a, M>>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    #[must_use]
    pub fn push(mut self, row: impl Into<Element<'a, M>>) -> Self {
        self.rows.push(row.into());
        self
    }

    #[must_use]
    pub fn push_maybe(self, row: Option<impl Into<Element<'a, M>>>) -> Self {
        match row {
            Some(row) => self.push(row),
            None => self,
        }
    }

    #[must_use]
    pub fn extend(mut self, rows: impl IntoIterator<Item = Element<'a, M>>) -> Self {
        self.rows.extend(rows);
        self
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    #[must_use]
    pub fn build(self) -> Element<'a, M> {
        self.into()
    }
}

impl<'a, M: 'a> From<Group<'a, M>> for Element<'a, M> {
    fn from(group: Group<'a, M>) -> Self {
        let Group { paint, title, description, suffix, rows } = group;
        let mut outer = Column::new().spacing(SP2);
        if title.is_some() || description.is_some() || suffix.is_some() {
            let mut heading = Column::new().spacing(2).width(Fill);
            if let Some(title) = title {
                heading = heading.push(text(title, TextStyle::HEAD));
            }
            if let Some(description) = description {
                heading = heading.push(text(description, TextStyle::CALL).color(paint.fg_2()));
            }
            let mut header = row![heading].spacing(SP3).align_y(Alignment::Center);
            if let Some(suffix) = suffix {
                header = header.push(suffix);
            }
            outer = outer.push(container(header).padding(Padding::ZERO.left(SP1)));
        }
        if !rows.is_empty() {
            outer = outer.push(list(&paint, rows));
        }
        outer.into()
    }
}

pub struct ListRow<'a, M> {
    paint: Paint,
    title: String,
    subtitle: Option<String>,
    prefix: Option<Element<'a, M>>,
    suffix: Option<Element<'a, M>>,
    message: Option<M>,
    ink: TitleInk,
    stacked: bool,
    strong: bool,
    beside: Option<Element<'a, M>>,
    changed: bool,
}

pub const CHANGED_MARK: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TitleInk {
    Plain,
    Dimmed,
    Accent,
}

impl<'a, M: Clone + 'a> ListRow<'a, M> {
    #[must_use]
    pub fn new(paint: impl Into<Paint>, title: impl Into<String>) -> Self {
        Self {
            paint: paint.into(),
            title: title.into(),
            subtitle: None,
            prefix: None,
            suffix: None,
            message: None,
            ink: TitleInk::Plain,
            stacked: false,
            strong: false,
            beside: None,
            changed: false,
        }
    }

    #[must_use]
    pub const fn changed(mut self, changed: bool) -> Self {
        self.changed = changed;
        self
    }

    #[must_use]
    pub const fn strong(mut self) -> Self {
        self.strong = true;
        self
    }

    #[must_use]
    pub const fn accent(mut self) -> Self {
        self.ink = TitleInk::Accent;
        self
    }

    #[must_use]
    pub fn beside(mut self, beside: impl Into<Element<'a, M>>) -> Self {
        self.beside = Some(beside.into());
        self
    }

    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        let subtitle = subtitle.into();
        if !subtitle.is_empty() {
            self.subtitle = Some(subtitle);
        }
        self
    }

    #[must_use]
    pub fn prefix(mut self, prefix: impl Into<Element<'a, M>>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    #[must_use]
    pub fn suffix(mut self, suffix: impl Into<Element<'a, M>>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    #[must_use]
    pub fn on_press(mut self, message: M) -> Self {
        self.message = Some(message);
        self
    }

    #[must_use]
    pub fn on_press_maybe(mut self, message: Option<M>) -> Self {
        self.message = message;
        self
    }

    #[must_use]
    pub fn dimmed(mut self, dimmed: bool) -> Self {
        if dimmed {
            self.ink = TitleInk::Dimmed;
        } else if self.ink == TitleInk::Dimmed {
            self.ink = TitleInk::Plain;
        }
        self
    }

    #[must_use]
    pub fn stacked(mut self, stacked: bool) -> Self {
        self.stacked = stacked;
        self
    }

    #[must_use]
    pub fn build(self) -> Element<'a, M> {
        self.into()
    }
}

impl<'a, M: Clone + 'a> From<ListRow<'a, M>> for Element<'a, M> {
    fn from(item: ListRow<'a, M>) -> Self {
        let ListRow { paint, title, subtitle, prefix, suffix, message, ink, stacked, strong, beside, changed } = item;
        let ink = match ink {
            TitleInk::Plain => paint.fg_1(),
            TitleInk::Dimmed => paint.fg_3(),
            TitleInk::Accent => paint.acc_text(),
        };
        let style = if strong { TextStyle::HEAD } else { TextStyle::BODY };
        let mut head = row![].spacing(SP2).align_y(Alignment::Center);
        if changed {
            head = head.push(super::popping(super::feedback::dot(paint.acc_bg(), CHANGED_MARK)));
        }
        head = head.push(text(title, style).color(ink));
        if let Some(beside) = beside {
            head = head.push(beside);
        }
        let mut labels = column![head].spacing(1).width(Fill);
        if let Some(subtitle) = subtitle {
            labels = labels.push(text(subtitle, TextStyle::CAP).color(paint.fg_2()));
        }
        let mut line = row![].spacing(SP3).align_y(Alignment::Center);
        if let Some(prefix) = prefix {
            line = line.push(prefix);
        }
        line = line.push(labels);
        let mut below = None;
        if let Some(suffix) = suffix {
            if stacked { below = Some(suffix) } else { line = line.push(suffix) }
        }
        let line = row![Space::new().height(component::CONTROL), line].align_y(Alignment::Center);
        let mut content = Column::new().spacing(SP2).push(line);
        if let Some(suffix) = below {
            content = content.push(container(suffix).padding(Padding::ZERO.bottom(ROW_PAD_Y)));
        }
        let body = container(content).padding(Padding::from([ROW_PAD_Y, SP3])).width(Fill);
        let Some(message) = message else { return body.into() };
        let control = control(body).on_press(message.clone()).padding(0).width(Fill);
        let control = widgets::button(control, move |status| {
            style::flat(paint.layered(Color::TRANSPARENT, status), paint.fg_1(), R2)
        });
        focusable(&paint, control, R2).inset().on_activate(Some(message)).into()
    }
}

#[must_use]
pub fn list_row<'a, M: Clone + 'a>(
    paint: &Paint, title: &'a str, subtitle: Option<&'a str>, trailing: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    ListRow::new(paint, title).subtitle(subtitle.unwrap_or_default()).suffix(trailing).into()
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
    status_strip(paint.bar_bg(), STATUS_BAR, left, right, corners)
}

#[must_use]
pub fn status_strip<'a, M: 'a>(
    background: Color, height: f32, left: Vec<Element<'a, M>>, right: Vec<Element<'a, M>>,
    corners: iced::border::Radius,
) -> Element<'a, M> {
    let left = iced::widget::Row::with_children(left).spacing(SP3).align_y(Alignment::Center).width(Fill).clip(true);
    let right = iced::widget::Row::with_children(right).spacing(SP1).align_y(Alignment::Center);
    container(row![left, right].spacing(SP3).align_y(Alignment::Center))
        .padding(Padding::from([0.0, SP3]).right(SP2))
        .width(Fill)
        .height(height)
        .clip(true)
        .style(move |_| style::rounded(background, corners))
        .into()
}

pub const ISLAND_GAP: f32 = SP3 + 3.0;
pub const ISLAND_TRAVEL: f32 = SP5;

#[must_use]
pub fn island_motion(shown: f32) -> (f32, f32) {
    let shown = shown.clamp(0.0, 1.0);
    (glide_theme::motion::assist(f64::from(shown)) as f32, (1.0 - shown) * ISLAND_TRAVEL)
}

#[must_use]
pub fn status_islands<'a, M: 'a>(
    paint: &Paint, background: Color, shown: f32, height: f32, left: Vec<Element<'a, M>>, right: Vec<Element<'a, M>>,
) -> Element<'a, M> {
    if shown <= 0.01 {
        return Space::new().into();
    }
    let (alpha, drop) = island_motion(shown);
    let paint = paint.faded(alpha);
    let background = background.scale_alpha(alpha);
    let island = |items: Vec<Element<'a, M>>| -> Element<'a, M> {
        let content = iced::widget::Row::with_children(items).spacing(SP2).align_y(Alignment::Center);
        let card =
            container(content).padding(Padding::from([0.0, SP1])).height(height).align_y(Alignment::Center).style(
                move |_| container::Style { background: Some(Background::Color(background)), ..style::popover(&paint) },
            );
        widgets::surface(card, &paint, Level::El2, R3)
    };
    let islands = row![island(left), Space::new().width(Fill), island(right)].align_y(Alignment::Center);
    let islands = animate(1.0, Speed::EXPAND, move |_| islands).offset(move |_| Vector::new(0.0, drop));
    container(islands)
        .width(Fill)
        .height(Fill)
        .align_y(iced::alignment::Vertical::Bottom)
        .padding(Padding { left: SP3, right: SP3, bottom: ISLAND_GAP, top: 0.0 })
        .clip(true)
        .into()
}

#[must_use]
pub fn status_button<'a, M: Clone + 'a>(
    paint: &Paint, label: impl Into<std::borrow::Cow<'a, str>>, message: M, height: f32,
) -> Element<'a, M> {
    let paint = *paint;
    let control = control(text(label.into(), TextStyle::CAP).wrapping(Wrapping::None))
        .on_press(message.clone())
        .height(height)
        .padding(Padding::from([0.0, SP2]));
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
    let message = on_toggle.map(|toggle| toggle(!on));
    ListRow::new(paint, title)
        .subtitle(subtitle.unwrap_or_default())
        .suffix(switch_face(paint, on, message.is_some()))
        .on_press_maybe(message)
        .into()
}

#[must_use]
pub fn action_row<'a, M: Clone + 'a>(
    paint: &Paint, title: &'a str, subtitle: Option<&'a str>, message: M,
) -> Element<'a, M> {
    ListRow::new(paint, title)
        .subtitle(subtitle.unwrap_or_default())
        .suffix(icon(Icon::Right, 12.0, paint.fg_3()))
        .on_press(message)
        .into()
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
pub fn profile_card<'a, M: Clone + 'a>(
    paint: &Paint, avatar: impl Into<Element<'a, M>>, name: &str, subtitle: &str, selected: bool, message: M,
) -> Element<'a, M> {
    let paint = *paint;
    let (ink, soft, mark) = if selected {
        (paint.acc_soft_fg(), paint.acc_soft_fg(), paint.acc_soft_fg())
    } else {
        (paint.fg_1(), paint.fg_2(), paint.fg_3())
    };
    let labels = column![
        text(name.to_string(), TextStyle::HEAD).color(ink).wrapping(Wrapping::None),
        text(subtitle.to_string(), TextStyle::CAP).color(soft).wrapping(Wrapping::None),
    ]
    .spacing(1)
    .width(Fill)
    .clip(true);
    let content =
        row![avatar.into(), labels, icon(Icon::Right, 12.0, mark)].spacing(SP2 + 2.0).align_y(Alignment::Center);
    let card = control(content).on_press(message.clone()).padding(Padding::from([SP2, SP2 + 2.0])).width(Fill);
    let card = widgets::button(card, move |status| {
        let base = if selected { paint.acc_soft() } else { paint.card_bg() };
        control::Style {
            border: Border { color: paint.line(), width: 1.0, radius: R3.into() },
            ..style::flat(paint.layered(base, status), ink, R3)
        }
    });
    activatable(&paint, card, R3, Some(message))
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
    leading: Option<Element<'a, M>>,
    label: String,
    matched: Option<(usize, usize)>,
    ink: Option<Color>,
    accessory: Option<Element<'a, M>>,
    count: Option<String>,
    meter: Option<f32>,
    message: Option<M>,
    selected: bool,
    current: bool,
    hovered: bool,
    held: bool,
}

pub const SIDEBAR_METER_ROW: f32 = 44.0;
const METER_WARN: f32 = 0.85;

impl<'a, M: Clone + 'a> SidebarItem<'a, M> {
    #[must_use]
    pub fn new(paint: &Paint, label: impl Into<String>) -> Self {
        Self {
            paint: *paint,
            glyph: None,
            leading: None,
            label: label.into(),
            matched: None,
            ink: None,
            accessory: None,
            count: None,
            meter: None,
            message: None,
            selected: false,
            current: false,
            hovered: false,
            held: false,
        }
    }

    #[must_use]
    pub fn count(mut self, count: usize) -> Self {
        self.count = (count > 0).then(|| count.to_string());
        self
    }

    #[must_use]
    pub fn meter(mut self, fraction: Option<f32>) -> Self {
        self.meter = fraction.map(|fraction| fraction.clamp(0.0, 1.0));
        self
    }

    #[must_use]
    pub fn leading(mut self, leading: impl Into<Element<'a, M>>) -> Self {
        self.leading = Some(leading.into());
        self
    }

    #[must_use]
    pub fn matched(mut self, range: Option<(usize, usize)>) -> Self {
        self.matched = range.filter(|&(start, end)| {
            start < end && self.label.is_char_boundary(start) && self.label.is_char_boundary(end)
        });
        self
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
        let SidebarItem {
            paint,
            glyph,
            leading,
            label,
            matched,
            ink,
            accessory,
            count,
            meter,
            message,
            selected,
            current,
            hovered,
            held,
        } = item;
        let height = if meter.is_some() { SIDEBAR_METER_ROW } else { component::CONTROL };
        animate(if selected { 1.0 } else { 0.0 }, Speed::COLOR, move |value| {
            let trailing = if accessory.is_some() { SP1 } else { SP3 };
            let mut content = row![].spacing(SP2 + 2.0).align_y(Alignment::Center);
            if let Some(leading) = leading {
                content = content.push(leading);
            } else if let Some((glyph, tint)) = glyph {
                content = content.push(glyph(if selected { paint.acc_soft_fg() } else { tint }));
            }
            let style = if current { TextStyle { weight: 600, ..TextStyle::CALL } } else { TextStyle::CALL };
            let ink = ink.filter(|_| !selected);
            let piece = |part: String, style: TextStyle| {
                let piece = text(part, style).wrapping(Wrapping::None);
                match ink {
                    Some(ink) => piece.color(ink),
                    None => piece,
                }
            };
            let label: Element<'a, M> = match matched {
                Some((start, end)) => {
                    let hit = if selected { paint.acc_soft_fg() } else { paint.acc_text() };
                    row![
                        piece(label[..start].to_string(), style),
                        text(label[start..end].to_string(), TextStyle { weight: 600, ..style })
                            .color(hit)
                            .wrapping(Wrapping::None),
                        piece(label[end..].to_string(), style),
                    ]
                    .width(Fill)
                    .into()
                }
                None => piece(label, style).width(Fill).into(),
            };
            let label: Element<'a, M> = match meter {
                Some(fraction) => {
                    let fill = if fraction > METER_WARN { paint.warn() } else { paint.fg_3() };
                    column![label, super::usage_bar(&paint, &[(fraction, fill)], 4.0)].spacing(SP1).width(Fill).into()
                }
                None => label,
            };
            content = content.push(label);
            if let Some(count) = count {
                let ink = if selected { paint.acc_soft_fg() } else { paint.fg_2() };
                content = content.push(text(count, TextStyle::CAP).color(ink).wrapping(Wrapping::None));
            }
            if let Some(accessory) = accessory {
                content = content.push(accessory);
            }
            let control = control(container(content).center_y(Fill).clip(true))
                .on_press_maybe(message.clone())
                .width(Fill)
                .height(height)
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
            stack![item, container(bar).height(height).center_y(height)]
        })
        .into()
    }
}

#[must_use]
pub fn disclosure<'a, M: 'a>(paint: &Paint, value: Option<String>) -> Element<'a, M> {
    let mut line = row![].spacing(SP1 + 2.0).align_y(Alignment::Center);
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        line = line.push(text(value, TextStyle::CALL).color(paint.fg_2()));
    }
    line.push(icon(Icon::Right, 12.0, paint.fg_3())).into()
}

#[must_use]
pub fn inert<'a, M: 'a>(paint: &Paint, enabled: bool, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let content = content.into();
    let veil = Color { a: 0.55, ..paint.card_bg() };
    let cover = animate(f32::from(!enabled), Speed::COLOR, move |level| {
        let level = level.clone();
        container(Space::new().width(Fill).height(Fill))
            .style(move |_| style::rounded(Color { a: veil.a * level.get(), ..veil }, glide_theme::shape::R1))
    });
    let cover: Element<'a, M> = if enabled { cover.into() } else { iced::widget::opaque(cover) };
    stack![content, cover].into()
}

pub const TABLE_HEADER: f32 = 26.0;

#[must_use]
pub fn table_header<'a, M: 'a>(paint: &Paint, cells: Vec<Element<'a, M>>) -> Element<'a, M> {
    let background = paint.fill_1();
    let cells = iced::widget::Row::with_children(cells)
        .padding(Padding::from([0.0, SP4]))
        .height(TABLE_HEADER)
        .align_y(Alignment::Center);
    container(cells).width(Fill).style(move |_| style::rounded(background, 0.0)).into()
}

pub struct HeaderCell<'a, M> {
    paint: Paint,
    label: &'a str,
    mark: Option<Drawn<'a, M>>,
    width: Length,
    separated: bool,
    lit: bool,
    target: bool,
    message: Option<M>,
}

impl<'a, M: Clone + 'a> HeaderCell<'a, M> {
    #[must_use]
    pub fn new(paint: &Paint, label: &'a str) -> Self {
        Self {
            paint: *paint,
            label,
            mark: None,
            width: Length::Fill,
            separated: true,
            lit: false,
            target: false,
            message: None,
        }
    }

    #[must_use]
    pub fn sorted(self, ascending: Option<bool>) -> Self {
        match ascending {
            Some(ascending) => self.sort_mark(if ascending { Icon::Up } else { Icon::Down }),
            None => self,
        }
    }

    #[must_use]
    pub fn sort_mark(mut self, glyph: impl super::Glyph + 'a) -> Self {
        self.mark = Some(Box::new(move |tint| icon(glyph, 11.0, tint)));
        self
    }

    #[must_use]
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    #[must_use]
    pub fn separated(mut self, separated: bool) -> Self {
        self.separated = separated;
        self
    }

    #[must_use]
    pub fn lit(mut self, lit: bool) -> Self {
        self.lit = lit;
        self
    }

    #[must_use]
    pub fn target(mut self, target: bool) -> Self {
        self.target = target;
        self
    }

    #[must_use]
    pub fn on_press(mut self, message: Option<M>) -> Self {
        self.message = message;
        self
    }
}

impl<'a, M: Clone + 'a> From<HeaderCell<'a, M>> for Element<'a, M> {
    fn from(cell: HeaderCell<'a, M>) -> Self {
        let HeaderCell { paint, label, mark, width, separated, lit, target, message } = cell;
        let mut content =
            row![text(label, TextStyle::MICRO).wrapping(Wrapping::None)].spacing(SP1).align_y(Alignment::Center);
        if let Some(mark) = mark {
            content = content.push(Space::new().width(Fill)).push(mark(paint.fg_3()));
        }
        let padding = Padding::from([SP1, SP2]).left(if separated { SP2 } else { 0.0 });
        let control = control(container(content).width(Fill).clip(true))
            .on_press_maybe(message.clone())
            .padding(padding)
            .width(Fill);
        let control = widgets::button(control, move |status| {
            let hovered = lit || target || matches!(status, control::Status::Hovered | control::Status::Pressed);
            let ink = if hovered { paint.acc_text() } else { paint.fg_3() };
            let background = if target { paint.fill_2() } else { Color::TRANSPARENT };
            style::flat(background, ink, glide_theme::shape::R1)
        });
        let mut cell = row![].align_y(Alignment::Center).width(width);
        if separated {
            let rule = container(Space::new().width(1).height(14)).style(move |_| style::rounded(paint.line(), 0.0));
            cell = cell.push(rule);
        }
        cell.push(activatable(&paint, control, glide_theme::shape::R1, message)).into()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_fits_one_control_and_its_hover_is_concentric_with_the_card() {
        assert!((ROW_HEIGHT - 44.0).abs() < f32::EPSILON);
        assert!((ROW_HEIGHT - 2.0 * ROW_PAD_Y - component::CONTROL).abs() < f32::EPSILON);
        assert!((R3 - SP1 - R2).abs() < f32::EPSILON, "rows sit SP1 inside the card, so R2 follows R3");
    }

    #[test]
    fn status_islands_travel_as_scene_content_and_opacity_only_assists() {
        let (alpha, lift) = island_motion(0.0);
        assert_eq!(alpha, 0.0);
        assert!(lift >= 24.0, "scene content travels at least 24 px");
        assert!((island_motion(0.4).0 - 1.0).abs() < 1e-6, "opaque once 40 % of the travel is done");
        assert_eq!(island_motion(1.0), (1.0, 0.0));
        let (half_alpha, half_lift) = island_motion(0.2);
        assert!(half_alpha < 1.0 && half_lift > 0.0);
    }
}

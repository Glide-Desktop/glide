use glide_app::menubar::Entry;
use glide_iced::Paint;
use glide_iced::style::{self, R2, SP1, SP2, SP3};
use glide_iced::widgets;
use glide_theme::{TextStyle, component, motion};
use glide_ui::Reveal;
use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::widget::{Space, button, container, mouse_area, pin, row, stack};
use iced::{Alignment, Background, Element, Fill, Length, Padding, Point, Size};

use crate::widget::{Icon, Wrapping, icon, overlay_scrollable, popover, text};

pub const WIDTH: f32 = 280.0;
pub const SCROLL: &str = "glide-menu";
pub const SUB_SCROLL: &str = "glide-submenu";
pub const CASCADE_GAP: f32 = 2.0;
const ITEM: f32 = component::MENU_ITEM;
const STEP: f32 = component::SMALL_CONTROL;
const MARK: f32 = 14.0;
const DOT: f32 = 10.0;
const SWATCH: f32 = 18.0;
const SEPARATOR: f32 = 9.0;
const HEADING: f32 = SP2 + SP1 + TextStyle::CAP.line_height_px;

#[derive(Debug, Clone, PartialEq)]
pub enum Item<M> {
    Action {
        label: String,
        shortcut: Option<String>,
        message: M,
        enabled: bool,
        check: Option<bool>,
        more: bool,
        danger: bool,
        swatch: Option<iced::Color>,
    },
    Submenu {
        label: String,
        items: Vec<Item<M>>,
        enabled: bool,
    },
    Stepper {
        label: String,
        value: String,
        less: Option<M>,
        more: Option<M>,
    },
    Swatches(Vec<Swatch<M>>),
    Heading(String),
    Separator,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Swatch<M> {
    pub label: String,
    pub color: iced::Color,
    pub on: bool,
    pub message: M,
}

impl<M> Item<M> {
    #[must_use]
    pub fn action(label: impl Into<String>, message: M) -> Self {
        Self::Action {
            label: label.into(),
            shortcut: None,
            message,
            enabled: true,
            check: None,
            more: false,
            danger: false,
            swatch: None,
        }
    }

    #[must_use]
    pub fn danger(mut self) -> Self {
        if let Self::Action { danger, .. } = &mut self {
            *danger = true;
        }
        self
    }

    #[must_use]
    pub fn swatch(mut self, color: iced::Color) -> Self {
        if let Self::Action { swatch, .. } = &mut self {
            *swatch = Some(color);
        }
        self
    }

    #[must_use]
    pub fn opens(&self) -> bool {
        matches!(self, Self::Action { enabled: true, more: true, .. } | Self::Submenu { enabled: true, .. })
    }

    fn checks(&self) -> bool {
        matches!(self, Self::Action { check: Some(_), .. })
    }

    #[must_use]
    pub fn stepper(label: impl Into<String>, value: impl Into<String>, less: Option<M>, more: Option<M>) -> Self {
        Self::Stepper { label: label.into(), value: value.into(), less, more }
    }

    #[must_use]
    pub fn swatches(swatches: Vec<Swatch<M>>) -> Self {
        Self::Swatches(swatches)
    }

    #[must_use]
    pub fn cascades(mut self) -> Self {
        if let Self::Action { more, .. } = &mut self {
            *more = true;
        }
        self
    }

    #[must_use]
    pub fn message(&self) -> Option<&M> {
        match self {
            Self::Action { message, enabled: true, .. } => Some(message),
            _ => None,
        }
    }

    #[must_use]
    pub fn submenu(label: impl Into<String>, items: Vec<Self>) -> Self {
        Self::Submenu { label: label.into(), items, enabled: true }
    }

    #[must_use]
    pub fn heading(label: impl Into<String>) -> Self {
        Self::Heading(label.into())
    }

    #[must_use]
    pub const fn separator() -> Self {
        Self::Separator
    }

    #[must_use]
    pub fn shortcut(mut self, chord: impl Into<String>) -> Self {
        if let Self::Action { shortcut, .. } = &mut self {
            *shortcut = Some(chord.into());
        }
        self
    }

    #[must_use]
    pub fn enabled(mut self, on: bool) -> Self {
        match &mut self {
            Self::Action { enabled, .. } | Self::Submenu { enabled, .. } => *enabled = on,
            Self::Stepper { less, more, .. } if !on => {
                *less = None;
                *more = None;
            }
            Self::Stepper { .. } | Self::Swatches(_) | Self::Heading(_) | Self::Separator => {}
        }
        self
    }

    #[must_use]
    pub fn messages(&self) -> Vec<&M> {
        match self {
            Self::Action { message, enabled: true, .. } => vec![message],
            Self::Stepper { less, more, .. } => less.iter().chain(more.iter()).collect(),
            Self::Swatches(swatches) => swatches.iter().map(|swatch| &swatch.message).collect(),
            _ => Vec::new(),
        }
    }

    #[must_use]
    pub fn checked(mut self, on: bool) -> Self {
        if let Self::Action { check, .. } = &mut self {
            *check = Some(on);
        }
        self
    }

    #[must_use]
    pub fn actionable(&self) -> bool {
        match self {
            Self::Action { enabled, .. } | Self::Submenu { enabled, .. } => *enabled,
            Self::Stepper { less, more, .. } => less.is_some() || more.is_some(),
            Self::Swatches(_) | Self::Heading(_) | Self::Separator => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Menu<M> {
    pub label: String,
    pub items: Vec<Item<M>>,
}

impl<M> Menu<M> {
    #[must_use]
    pub fn new(label: impl Into<String>, items: Vec<Item<M>>) -> Self {
        Self { label: label.into(), items }
    }
}

fn export_items<M: Clone>(items: &[Item<M>], actions: &mut Vec<M>) -> Vec<Entry> {
    items
        .iter()
        .map(|item| match item {
            Item::Action { label, shortcut, message, enabled, check, .. } => {
                actions.push(message.clone());
                Entry::Action {
                    id: u32::try_from(actions.len()).unwrap_or(u32::MAX),
                    label: label.clone(),
                    enabled: *enabled,
                    check: *check,
                    shortcut: shortcut.clone(),
                }
            }
            Item::Submenu { label, items, enabled } => {
                Entry::Submenu { label: label.clone(), enabled: *enabled, children: export_items(items, actions) }
            }
            Item::Stepper { label, less, more, .. } => {
                let children = [("−", less), ("+", more)]
                    .into_iter()
                    .map(|(sign, message)| {
                        if let Some(message) = message {
                            actions.push(message.clone());
                        }
                        Entry::Action {
                            id: if message.is_some() { u32::try_from(actions.len()).unwrap_or(u32::MAX) } else { 0 },
                            label: sign.to_string(),
                            enabled: message.is_some(),
                            check: None,
                            shortcut: None,
                        }
                    })
                    .collect();
                Entry::Submenu { label: label.clone(), enabled: less.is_some() || more.is_some(), children }
            }
            Item::Swatches(swatches) => Entry::Submenu {
                label: String::new(),
                enabled: true,
                children: swatches
                    .iter()
                    .map(|swatch| {
                        actions.push(swatch.message.clone());
                        Entry::Action {
                            id: u32::try_from(actions.len()).unwrap_or(u32::MAX),
                            label: swatch.label.clone(),
                            enabled: true,
                            check: Some(swatch.on),
                            shortcut: None,
                        }
                    })
                    .collect(),
            },
            Item::Heading(label) => {
                Entry::Action { id: 0, label: label.clone(), enabled: false, check: None, shortcut: None }
            }
            Item::Separator => Entry::Separator,
        })
        .collect()
}

#[must_use]
pub fn export<M: Clone>(menus: &[Menu<M>]) -> (Vec<Entry>, Vec<M>) {
    let mut actions = Vec::new();
    let entries = menus
        .iter()
        .map(|menu| Entry::Submenu {
            label: menu.label.clone(),
            enabled: true,
            children: export_items(&menu.items, &mut actions),
        })
        .collect();
    (entries, actions)
}

#[must_use]
pub fn exported<M: Clone>(actions: &[M], id: u32) -> Option<M> {
    actions.get(usize::try_from(id).ok()?.checked_sub(1)?).cloned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    pub modifiers: Modifiers,
    pub key: String,
}

impl Chord {
    #[must_use]
    pub fn parse(label: &str) -> Option<Self> {
        let (head, key) = match label.strip_suffix("++") {
            Some(head) => (head, "+"),
            None => match label.rsplit_once('+') {
                Some((head, key)) => (head, key),
                None => ("", label),
            },
        };
        let mut modifiers = Modifiers::empty();
        for part in head.split('+').filter(|part| !part.is_empty()) {
            modifiers |= match part.trim().to_ascii_lowercase().as_str() {
                "ctrl" | "control" => Modifiers::CTRL,
                "shift" => Modifiers::SHIFT,
                "alt" => Modifiers::ALT,
                "super" | "logo" | "meta" => Modifiers::LOGO,
                _ => return None,
            };
        }
        let key = key.trim().to_ascii_lowercase();
        (!key.is_empty()).then_some(Self { modifiers, key })
    }

    #[must_use]
    pub fn matches(&self, key: &Key, physical: keyboard::key::Physical, modifiers: Modifiers) -> bool {
        if modifiers != self.modifiers {
            return false;
        }
        let pressed = match key {
            Key::Named(named) => named_key(*named).map(str::to_string),
            Key::Character(_) => key.to_latin(physical).map(|latin| latin.to_ascii_lowercase().to_string()),
            Key::Unidentified => None,
        };
        pressed.is_some_and(|pressed| pressed == self.key)
    }
}

fn named_key(named: Named) -> Option<&'static str> {
    Some(match named {
        Named::Enter => "enter",
        Named::Tab => "tab",
        Named::Space => "space",
        Named::Backspace => "backspace",
        Named::Delete => "delete",
        Named::Escape => "esc",
        Named::Insert => "insert",
        Named::Home => "home",
        Named::End => "end",
        Named::PageUp => "pageup",
        Named::PageDown => "pagedown",
        Named::ArrowUp => "up",
        Named::ArrowDown => "down",
        Named::ArrowLeft => "left",
        Named::ArrowRight => "right",
        Named::F1 => "f1",
        Named::F2 => "f2",
        Named::F3 => "f3",
        Named::F4 => "f4",
        Named::F5 => "f5",
        Named::F6 => "f6",
        Named::F7 => "f7",
        Named::F8 => "f8",
        Named::F9 => "f9",
        Named::F10 => "f10",
        Named::F11 => "f11",
        Named::F12 => "f12",
        _ => return None,
    })
}

fn find_shortcut<M: Clone>(
    items: &[Item<M>], key: &Key, physical: keyboard::key::Physical, modifiers: Modifiers,
) -> Option<M> {
    items.iter().find_map(|item| match item {
        Item::Action { shortcut: Some(shortcut), message, enabled: true, .. } => {
            Chord::parse(shortcut).filter(|chord| chord.matches(key, physical, modifiers)).map(|_| message.clone())
        }
        Item::Submenu { items, enabled: true, .. } => find_shortcut(items, key, physical, modifiers),
        _ => None,
    })
}

#[must_use]
pub fn shortcut<M: Clone>(
    menus: &[Menu<M>], key: &Key, physical: keyboard::key::Physical, modifiers: Modifiers,
) -> Option<M> {
    menus.iter().find_map(|menu| find_shortcut(&menu.items, key, physical, modifiers))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Bar(usize),
    Context,
}

#[derive(Debug, Clone)]
pub struct Popup<M> {
    pub source: Source,
    pub items: Vec<Item<M>>,
    pub at: Point,
    pub selected: Option<usize>,
    pub reveal: Reveal,
    trail: Vec<Vec<Item<M>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Hovered(usize),
    Picked(usize),
    Stepped(usize, bool),
    Swatched(usize, usize),
    Dismiss,
}

pub enum Outcome<M> {
    None,
    Run(M),
    Stay(M),
    Close,
}

impl<M: Clone> Popup<M> {
    #[must_use]
    pub fn new(source: Source, items: Vec<Item<M>>, at: Point) -> Self {
        let mut reveal = Reveal::closed();
        reveal.open(motion::t2());
        Self { source, items, at, selected: None, reveal, trail: Vec::new() }
    }

    fn actionable(&self) -> Vec<usize> {
        actionable(&self.items)
    }

    pub fn pick(&mut self, index: usize) -> Outcome<M> {
        match self.items.get(index) {
            Some(Item::Action { message, enabled: true, .. }) => Outcome::Run(message.clone()),
            Some(Item::Submenu { items, enabled: true, .. }) => {
                let items = items.clone();
                self.trail.push(std::mem::replace(&mut self.items, items));
                self.selected = self.actionable().first().copied();
                Outcome::None
            }
            _ => Outcome::None,
        }
    }

    #[must_use]
    pub fn depth(&self) -> usize {
        self.trail.len()
    }

    pub fn back(&mut self) -> bool {
        let Some(items) = self.trail.pop() else { return false };
        self.items = items;
        self.selected = self.actionable().first().copied();
        true
    }

    pub fn step(&mut self, forward: bool) {
        if let Some(next) = walk(&self.items, self.selected, if forward { Walk::Next } else { Walk::Previous }) {
            self.selected = Some(next);
        }
    }

    pub fn key(&mut self, key: &Key) -> Option<Outcome<M>> {
        self.key_with(key, false)
    }

    pub fn key_with(&mut self, key: &Key, backwards: bool) -> Option<Outcome<M>> {
        if let Some(Item::Stepper { less, more, .. }) = self.selected.and_then(|index| self.items.get(index)) {
            let step = match key {
                Key::Named(Named::ArrowLeft) => less.clone(),
                Key::Named(Named::ArrowRight) => more.clone(),
                _ => None,
            };
            if let Some(message) = step {
                return Some(Outcome::Stay(message));
            }
        }
        Some(match key {
            Key::Named(Named::Tab) if backwards => {
                self.step(false);
                Outcome::None
            }
            Key::Named(Named::ArrowDown | Named::Tab) => {
                self.step(true);
                Outcome::None
            }
            Key::Named(Named::ArrowUp) => {
                self.step(false);
                Outcome::None
            }
            Key::Named(Named::Home) => {
                self.selected = self.actionable().first().copied();
                Outcome::None
            }
            Key::Named(Named::End) => {
                self.selected = self.actionable().last().copied();
                Outcome::None
            }
            Key::Named(Named::Enter | Named::Space | Named::ArrowRight) => match self.selected {
                Some(index) if matches!(key, Key::Named(Named::ArrowRight)) => {
                    if matches!(self.items.get(index), Some(Item::Submenu { .. })) {
                        self.pick(index)
                    } else {
                        return None;
                    }
                }
                Some(index) => self.pick(index),
                None => Outcome::None,
            },
            Key::Named(Named::ArrowLeft) => {
                if self.back() {
                    Outcome::None
                } else {
                    return None;
                }
            }
            Key::Named(Named::Escape) => {
                if self.back() {
                    Outcome::None
                } else {
                    Outcome::Close
                }
            }
            _ => return None,
        })
    }
}

impl<M> Popup<M> {
    #[must_use]
    pub fn height(&self) -> f32 {
        height(&self.items)
    }

    #[must_use]
    pub fn placement(&self, window: Size) -> (Point, Size) {
        place(self.at, self.height(), width(&self.items, WIDTH), window, self.source == Source::Context)
    }
}

#[must_use]
pub fn actionable<M>(items: &[Item<M>]) -> Vec<usize> {
    items.iter().enumerate().filter(|(_, item)| item.actionable()).map(|(index, _)| index).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Walk {
    First,
    Last,
    Next,
    Previous,
}

#[must_use]
pub fn walk<M>(items: &[Item<M>], current: Option<usize>, walk: Walk) -> Option<usize> {
    let rows = actionable(items);
    let last = rows.len().checked_sub(1)?;
    let at = current.and_then(|current| rows.iter().position(|row| *row == current));
    let pick = match (walk, at) {
        (Walk::First, _) | (Walk::Next, None) => 0,
        (Walk::Last, _) | (Walk::Previous, None) => last,
        (Walk::Next, Some(at)) => (at + 1) % rows.len(),
        (Walk::Previous, Some(at)) => at.checked_sub(1).unwrap_or(last),
    };
    Some(rows[pick])
}

fn row_height<M>(item: &Item<M>) -> f32 {
    match item {
        Item::Separator => SEPARATOR,
        Item::Heading(_) => HEADING,
        _ => ITEM,
    }
}

#[must_use]
pub fn height<M>(items: &[Item<M>]) -> f32 {
    2.0 * SP1 + 2.0 + items.iter().map(row_height).sum::<f32>()
}

#[must_use]
pub fn span<M>(items: &[Item<M>], index: usize) -> (f32, f32) {
    let top: f32 = items.iter().take(index).map(row_height).sum();
    (top, top + items.get(index).map_or(0.0, row_height))
}

pub const MAX_WIDTH: f32 = 480.0;

#[must_use]
pub fn width<M>(items: &[Item<M>], least: f32) -> f32 {
    let measure = crate::widget::measure;
    let widest = items
        .iter()
        .map(|item| match item {
            Item::Action { label, shortcut, more, swatch, .. } => {
                let keys = shortcut.as_deref().map_or(0.0, |keys| measure(keys, TextStyle::CAP) + SP3);
                let dot = if swatch.is_some() { DOT + SP2 } else { 0.0 };
                measure(label, TextStyle::MENU_ITEM) + keys + dot + if *more { 12.0 + SP2 } else { 0.0 }
            }
            Item::Submenu { label, .. } => measure(label, TextStyle::MENU_ITEM) + 12.0 + SP2,
            Item::Stepper { label, value, .. } => {
                measure(label, TextStyle::MENU_ITEM)
                    + SP3
                    + 2.0 * STEP
                    + (measure(value, TextStyle::NUM) + 2.0 * SP1).max(STEP)
            }
            Item::Swatches(swatches) => swatches.len() as f32 * (SWATCH + SP2),
            Item::Heading(label) => measure(label, TextStyle::CAP),
            Item::Separator => 0.0,
        })
        .fold(0.0, f32::max);
    let mark = if items.iter().any(Item::checks) { MARK + SP2 } else { 0.0 };
    let row_padding = 2.0 * SP2;
    let card = 2.0 * SP1 + 2.0;
    let scrollbar = 6.0 + SP1;
    (widest + mark + SP2 + row_padding + card + scrollbar + SP1).ceil().clamp(least, MAX_WIDTH)
}

#[must_use]
pub fn place(at: Point, height: f32, width: f32, window: Size, may_flip: bool) -> (Point, Size) {
    let width = width.min(window.width - 2.0 * SP2);
    let x = if may_flip && at.x + width > window.width - SP2 { at.x - width } else { at.x };
    let x = x.min(window.width - width - SP2).max(SP2);
    let below = window.height - at.y - SP2;
    let above = at.y - SP1 - SP2;
    let room = (window.height - 2.0 * SP2).max(ITEM);
    if !may_flip || height <= below {
        let height = height.min(below.max(ITEM));
        return (Point::new(x, at.y.min(window.height - height - SP2).max(SP2)), Size::new(width, height));
    }
    if at.y > window.height * 0.6 && height <= above {
        return (Point::new(x, at.y - height - SP1), Size::new(width, height));
    }
    let height = height.min(room);
    (Point::new(x, (window.height - height - SP2).max(SP2)), Size::new(width, height))
}

#[must_use]
pub fn cascade(parent: (Point, Size), row_top: f32, height: f32, width: f32, window: Size) -> (Point, Size) {
    let (origin, size) = parent;
    let width = width.min(window.width - 2.0 * SP2);
    let right = origin.x + size.width + CASCADE_GAP;
    let x = if right + width <= window.width - SP2 { right } else { origin.x - CASCADE_GAP - width };
    let x = x.min(window.width - width - SP2).max(SP2);
    let height = height.min(window.height - 2.0 * SP2);
    let y = (origin.y + row_top).min(window.height - height - SP2).max(SP2);
    (Point::new(x, y), Size::new(width, height))
}

#[must_use]
pub fn rows<'a, M>(paint: &Paint, items: &[Item<M>], selected: Option<usize>) -> Vec<Element<'a, Event>> {
    let marks = items.iter().any(Item::checks);
    items.iter().enumerate().map(|(index, item)| item_row(paint, item, index, selected == Some(index), marks)).collect()
}

#[must_use]
pub fn layer<'a, E: Clone + 'a>(
    paint: &Paint, rows: Vec<Element<'a, E>>, at: Point, size: Size, progress: f32, dismiss: Option<E>,
) -> Element<'a, E> {
    card_layer(paint, rows, SCROLL, at, size, progress, dismiss)
}

#[must_use]
pub fn sublayer<'a, E: Clone + 'a>(
    paint: &Paint, rows: Vec<Element<'a, E>>, at: Point, size: Size, progress: f32,
) -> Element<'a, E> {
    card_layer(paint, rows, SUB_SCROLL, at, size, progress, None)
}

fn card_layer<'a, E: Clone + 'a>(
    paint: &Paint, rows: Vec<Element<'a, E>>, id: &'static str, at: Point, size: Size, progress: f32,
    dismiss: Option<E>,
) -> Element<'a, E> {
    let content = overlay_scrollable(paint, iced::widget::column(rows)).id(id).height(size.height - 2.0 * SP1 - 2.0);
    let card = container(popover(paint, content)).width(size.width);
    let scale = component::POPOVER_CLOSED_SCALE + (1.0 - component::POPOVER_CLOSED_SCALE) * progress;
    let card = pin(widgets::scale(card, scale)).x(at.x).y(at.y + (1.0 - progress) * component::POPOVER_RISE);
    match dismiss {
        Some(dismiss) => stack![
            mouse_area(Space::new().width(Fill).height(Fill))
                .on_press(dismiss.clone())
                .on_right_press(dismiss.clone())
                .on_middle_press(dismiss),
            card,
        ]
        .into(),
        None => card.into(),
    }
}

#[must_use]
pub fn item_row<'a, M>(paint: &Paint, item: &Item<M>, index: usize, selected: bool, marks: bool) -> Element<'a, Event> {
    let paint = *paint;
    match item {
        Item::Separator => container(
            container(Space::new().width(Fill).height(1)).style(move |_| style::rounded(paint.hairline(), 0.0)),
        )
        .padding(Padding::from([SP1, SP2]))
        .into(),
        Item::Heading(label) => container(text(label.clone(), TextStyle::CAP).color(paint.fg_3()))
            .padding(Padding::from([SP2, SP3]).bottom(SP1))
            .height(HEADING)
            .into(),
        Item::Action { label, enabled, .. } | Item::Submenu { label, enabled, .. } => {
            let enabled = *enabled;
            let (shortcut, check, more, danger, swatch) = match item {
                Item::Action { shortcut, check, more, danger, swatch, .. } => {
                    (shortcut.clone(), *check, *more, *danger, *swatch)
                }
                _ => (None, None, true, false, None),
            };
            let mut content = row![].spacing(SP2).align_y(Alignment::Center);
            if marks {
                content = content.push(match check {
                    Some(true) => icon(Icon::Check, MARK, if enabled { paint.acc_bg() } else { paint.fg_4() }),
                    _ => Space::new().width(MARK).height(MARK).into(),
                });
            }
            if let Some(color) = swatch {
                content = content.push(
                    container(Space::new().width(DOT).height(DOT))
                        .style(move |_| style::rounded(color, glide_theme::shape::R_ROUND)),
                );
            }
            content = content
                .push(text(label.clone(), TextStyle::MENU_ITEM).wrapping(Wrapping::None))
                .push(Space::new().width(Fill));
            if let Some(shortcut) = shortcut {
                content = content.push(text(shortcut, TextStyle::CAP).color(if enabled {
                    paint.fg_3()
                } else {
                    paint.fg_4()
                }));
            }
            if more {
                content = content.push(icon(Icon::Right, 12.0, paint.fg_3()));
            }
            let control = button(container(content).center_y(Fill).clip(true))
                .on_press_maybe(enabled.then_some(Event::Picked(index)))
                .width(Fill)
                .height(ITEM)
                .padding(Padding::from([0.0, SP2]));
            let control = widgets::button(control, move |status| {
                let mut style = style::menu_item(&paint, status);
                if selected {
                    style.background = Some(Background::Color(paint.fill_2()));
                }
                if danger && enabled {
                    style.text_color = paint.danger();
                }
                style
            });
            mouse_area(control).on_enter(Event::Hovered(index)).into()
        }
        Item::Stepper { label, value, less, more } => {
            let step = |sign: &'static str, active: bool, more: bool| -> Element<'a, Event> {
                let control = button(container(text(sign, TextStyle::BODY)).center(Fill))
                    .on_press_maybe(active.then_some(Event::Stepped(index, more)))
                    .width(STEP)
                    .height(STEP)
                    .padding(0);
                widgets::button(control, move |status| style::action(&paint, glide_iced::Tone::Neutral, status))
            };
            let value = text(value.clone(), TextStyle::NUM).width(Length::Shrink).wrapping(Wrapping::None);
            let content = row![
                Space::new().width(14),
                text(label.clone(), TextStyle::MENU_ITEM).wrapping(Wrapping::None),
                Space::new().width(Fill),
                step("−", less.is_some(), false),
                container(value).padding(Padding::from([0.0, SP1])).center_y(STEP),
                step("+", more.is_some(), true),
            ]
            .spacing(SP2)
            .align_y(Alignment::Center);
            let row = container(content).padding(Padding::from([0.0, SP2])).height(ITEM).center_y(ITEM);
            let row = widgets::container(row, move |hover, _| {
                let layer = if selected { 1.0 } else { hover };
                style::rounded(paint.layer(iced::Color::TRANSPARENT, motion::ST_HOVER * layer), R2)
            });
            mouse_area(row).on_enter(Event::Hovered(index)).into()
        }
        Item::Swatches(swatches) => {
            let dots = swatches.iter().enumerate().map(|(at, swatch)| {
                crate::widget::swatch(&paint, swatch.color, swatch.on, Some(Event::Swatched(index, at)), SWATCH)
            });
            let content = iced::widget::Row::with_children(dots).spacing(SP1).align_y(Alignment::Center);
            container(content).padding(Padding::from([0.0, SP2])).height(ITEM).center_y(ITEM).into()
        }
    }
}

#[must_use]
pub fn view<'a, M>(paint: &Paint, popup: &'a Popup<M>, window: Size) -> Element<'a, Event> {
    let progress = popup.reveal.value(motion::t2()) as f32;
    let paint = paint.faded(progress);
    let (at, size) = popup.placement(window);
    layer(&paint, rows(&paint, &popup.items, popup.selected), at, size, progress, Some(Event::Dismiss))
}

#[must_use]
pub fn bar_width(label: &str) -> f32 {
    crate::widget::measure(label, TextStyle::CALL) + 2.0 * SP3
}

#[must_use]
pub fn bar<'a, M>(paint: &Paint, menus: &'a [Menu<M>], open: Option<usize>) -> Element<'a, BarEvent> {
    let labels: Vec<&str> = menus.iter().map(|menu| menu.label.as_str()).collect();
    bar_of(paint, &labels, open)
}

#[must_use]
pub fn bar_of<'a>(paint: &Paint, labels: &[&str], open: Option<usize>) -> Element<'a, BarEvent> {
    let paint = *paint;
    let buttons = labels.iter().enumerate().map(|(index, label)| {
        let active = open == Some(index);
        let control = button(container(text(label.to_string(), TextStyle::CALL).wrapping(Wrapping::None)).center(Fill))
            .on_press(BarEvent::Pressed(index))
            .width(bar_width(label))
            .height(component::SMALL_CONTROL)
            .padding(0);
        let control = widgets::button(control, move |status| {
            if active {
                style::flat(paint.fill_2(), paint.fg_1(), R2)
            } else {
                style::action(&paint, glide_iced::Tone::Ghost, status)
            }
        });
        mouse_area(control).on_enter(BarEvent::Hovered(index)).into()
    });
    iced::widget::Row::with_children(buttons).spacing(SP1).into()
}

#[must_use]
pub fn bar_total(menus: &[Menu<impl Sized>]) -> f32 {
    menus.iter().map(|menu| bar_width(&menu.label) + SP1).sum()
}

#[must_use]
pub fn bar_x(menus: &[Menu<impl Sized>], start: f32, index: usize) -> f32 {
    start + menus.iter().take(index).map(|menu| bar_width(&menu.label) + SP1).sum::<f32>()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarEvent {
    Pressed(usize),
    Hovered(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Command {
        New,
        Save,
        Wrap,
        Zoom,
    }

    fn menus() -> Vec<Menu<Command>> {
        vec![
            Menu::new(
                "File",
                vec![
                    Item::action("New", Command::New).shortcut("Ctrl+N"),
                    Item::separator(),
                    Item::action("Save", Command::Save).shortcut("Ctrl+S").enabled(false),
                ],
            ),
            Menu::new(
                "View",
                vec![
                    Item::heading("Text"),
                    Item::action("Wrap", Command::Wrap).checked(true),
                    Item::submenu("Zoom", vec![Item::action("Bigger", Command::Zoom).shortcut("Ctrl++")]),
                ],
            ),
        ]
    }

    #[test]
    fn exported_ids_lead_back_to_their_messages() {
        let (entries, actions) = export(&menus());
        assert_eq!(entries.len(), 2);
        assert_eq!(actions, vec![Command::New, Command::Save, Command::Wrap, Command::Zoom]);
        let Entry::Submenu { children, .. } = &entries[1] else { panic!("a menu is a submenu") };
        let Entry::Submenu { children: zoom, .. } = &children[2] else { panic!("zoom is a submenu") };
        let Entry::Action { id, .. } = zoom[0] else { panic!("an action") };
        assert_eq!(exported(&actions, id), Some(Command::Zoom));
        assert_eq!(exported(&actions, 0), None, "headings export with id 0 and run nothing");
    }

    #[test]
    fn shortcuts_parse_with_a_literal_plus() {
        assert_eq!(Chord::parse("Ctrl++"), Some(Chord { modifiers: Modifiers::CTRL, key: "+".into() }));
        assert_eq!(
            Chord::parse("Ctrl+Shift+S"),
            Some(Chord { modifiers: Modifiers::CTRL | Modifiers::SHIFT, key: "s".into() })
        );
        assert_eq!(Chord::parse("F3"), Some(Chord { modifiers: Modifiers::empty(), key: "f3".into() }));
        assert_eq!(Chord::parse("Hyper+X"), None);
    }

    #[test]
    fn only_enabled_shortcuts_fire() {
        let physical = keyboard::key::Physical::Code(keyboard::key::Code::KeyN);
        let n = Key::Character("n".into());
        assert_eq!(shortcut(&menus(), &n, physical, Modifiers::CTRL), Some(Command::New));
        assert_eq!(shortcut(&menus(), &n, physical, Modifiers::CTRL | Modifiers::SHIFT), None);
        let s = Key::Character("s".into());
        let physical = keyboard::key::Physical::Code(keyboard::key::Code::KeyS);
        assert_eq!(shortcut(&menus(), &s, physical, Modifiers::CTRL), None, "Save is disabled");
    }

    #[test]
    fn shortcuts_follow_the_physical_key_on_other_layouts() {
        let physical = keyboard::key::Physical::Code(keyboard::key::Code::KeyN);
        let cyrillic = Key::Character("т".into());
        assert_eq!(shortcut(&menus(), &cyrillic, physical, Modifiers::CTRL), Some(Command::New));
    }

    #[test]
    fn keyboard_walks_only_enabled_rows_and_enters_submenus() {
        let mut popup = Popup::new(Source::Context, menus()[1].items.clone(), Point::ORIGIN);
        assert!(matches!(popup.key(&Key::Named(Named::ArrowDown)), Some(Outcome::None)));
        assert_eq!(popup.selected, Some(1), "the heading is skipped");
        popup.step(true);
        assert_eq!(popup.selected, Some(2));
        assert!(matches!(popup.key(&Key::Named(Named::ArrowRight)), Some(Outcome::None)));
        assert_eq!(popup.items.len(), 1, "the submenu replaced the rows");
        assert!(matches!(popup.key(&Key::Named(Named::Enter)), Some(Outcome::Run(Command::Zoom))));
        assert!(matches!(popup.key(&Key::Named(Named::Escape)), Some(Outcome::None)), "Esc leaves the submenu first");
        assert!(matches!(popup.key(&Key::Named(Named::Escape)), Some(Outcome::Close)));
    }

    #[test]
    fn a_popup_stays_inside_the_window() {
        let popup = Popup::new(Source::Context, menus()[0].items.clone(), Point::new(990.0, 700.0));
        let window = Size::new(1000.0, 720.0);
        let (at, size) = popup.placement(window);
        assert!(at.x + size.width <= window.width - SP2 + 0.5);
        assert!(at.y + size.height <= window.height - SP2 + 0.5);
        assert!(at.x >= SP2 && at.y >= SP2);
    }

    #[test]
    fn a_tall_menu_scrolls_below_its_anchor_instead_of_covering_it() {
        let window = Size::new(960.0, 640.0);
        let (at, size) = place(Point::new(100.0, 36.0), 900.0, 300.0, window, false);
        assert_eq!(at.y, 36.0, "a bar menu opens where the bar ends");
        assert!(size.height <= window.height - 36.0 - SP2 + 0.5);
        let (at, size) = place(Point::new(100.0, 600.0), 500.0, 300.0, window, true);
        assert!(at.y + size.height <= 600.0, "near the bottom a context menu opens upwards");
        let (short, _) = place(Point::new(100.0, 500.0), 80.0, 300.0, window, true);
        assert_eq!(short.y, 500.0, "a short menu stays below even low in the window");
        let (at, size) = place(Point::new(100.0, 300.0), 500.0, 300.0, window, true);
        assert_eq!(size.height, 500.0, "a context menu that fits the window is shown whole");
        assert!(at.y + size.height <= window.height - SP2 + 0.5);
        let (at, size) = place(Point::new(100.0, 300.0), 2000.0, 300.0, window, true);
        assert_eq!((at.y, size.height), (SP2, window.height - 2.0 * SP2), "only taller than the window scrolls");
    }

    #[test]
    fn row_spans_include_headings_and_separators_above_them() {
        let items =
            [Item::heading("h"), Item::action("a", Command::New), Item::Separator, Item::action("b", Command::Save)];
        assert_eq!(span(&items, 0), (0.0, HEADING));
        assert_eq!(span(&items, 1), (HEADING, HEADING + ITEM));
        let top = HEADING + ITEM + SEPARATOR;
        assert_eq!(span(&items, 3), (top, top + ITEM));
    }

    #[test]
    fn walking_skips_disabled_rows_and_wraps() {
        let items =
            [Item::heading("h"), Item::action("a", Command::New), Item::action("b", Command::Save).enabled(false)];
        assert_eq!(walk(&items, None, Walk::Next), Some(1));
        assert_eq!(walk(&items, Some(1), Walk::Next), Some(1), "the only enabled row wraps onto itself");
        assert_eq!(walk(&items, None, Walk::Last), Some(1));
        assert_eq!(walk(&[Item::<Command>::Separator], None, Walk::First), None);
    }

    #[test]
    fn marks_are_reserved_only_when_a_row_can_be_checked() {
        let plain = [Item::action("Open", Command::New)];
        let checked = [Item::action("Open", Command::New), Item::action("Wrap", Command::Wrap).checked(false)];
        assert!(width(&checked, 0.0) > width(&plain, 0.0));
    }

    #[test]
    fn the_width_fits_the_longest_label_and_its_shortcut() {
        let items = vec![
            Item::action("A label long enough to need more than the least width", Command::New)
                .shortcut("Ctrl+Shift+N"),
        ];
        assert!(width(&items, 200.0) > 200.0);
        assert_eq!(width(&[Item::action("x", Command::New)], 200.0), 200.0);
        let huge = vec![Item::action("x".repeat(400), Command::New)];
        assert_eq!(width(&huge, 200.0), MAX_WIDTH);
    }

    #[test]
    fn a_heading_reserves_what_it_draws() {
        assert_eq!(HEADING, SP2 + SP1 + TextStyle::CAP.line_height_px);
        let items = [Item::heading("h"), Item::action("a", Command::New)];
        assert_eq!(span(&items, 1), (HEADING, HEADING + ITEM));
    }

    #[test]
    fn a_cascade_opens_beside_its_row_and_flips_at_the_window_edge() {
        let window = Size::new(1000.0, 600.0);
        let parent = (Point::new(100.0, 50.0), Size::new(250.0, 400.0));
        let (at, size) = cascade(parent, 70.0, 200.0, 220.0, window);
        assert_eq!(at, Point::new(352.0, 120.0), "the first row lines up with the parent row");
        assert_eq!(size, Size::new(220.0, 200.0));
        let near_edge = (Point::new(700.0, 50.0), Size::new(250.0, 400.0));
        assert_eq!(cascade(near_edge, 0.0, 200.0, 220.0, window).0.x, 700.0 - CASCADE_GAP - 220.0);
        let low = cascade(parent, 380.0, 200.0, 220.0, window);
        assert_eq!(low.0.y, 600.0 - SP2 - 200.0, "a submenu low in the window rises to stay on screen");
    }
}

use glide_app::menubar::Entry;
use glide_iced::Paint;
use glide_iced::style::{self, R2, SP1, SP2, SP3};
use glide_iced::widgets;
use glide_theme::{TextStyle, component, motion};
use glide_ui::Reveal;
use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::widget::{Space, button, container, mouse_area, pin, row, stack};
use iced::{Alignment, Background, Element, Fill, Padding, Point, Size};

use crate::widget::{Icon, Wrapping, icon, popover, scrollable, text};

pub const WIDTH: f32 = 280.0;
const ITEM: f32 = component::MENU_ITEM;
const SEPARATOR: f32 = 9.0;
const HEADING: f32 = 26.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Item<M> {
    Action { label: String, shortcut: Option<String>, message: M, enabled: bool, check: Option<bool> },
    Submenu { label: String, items: Vec<Item<M>>, enabled: bool },
    Heading(String),
    Separator,
}

impl<M> Item<M> {
    #[must_use]
    pub fn action(label: impl Into<String>, message: M) -> Self {
        Self::Action { label: label.into(), shortcut: None, message, enabled: true, check: None }
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
            Self::Heading(_) | Self::Separator => {}
        }
        self
    }

    #[must_use]
    pub fn checked(mut self, on: bool) -> Self {
        if let Self::Action { check, .. } = &mut self {
            *check = Some(on);
        }
        self
    }

    fn actionable(&self) -> bool {
        matches!(self, Self::Action { enabled: true, .. } | Self::Submenu { enabled: true, .. })
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
            Item::Action { label, shortcut, message, enabled, check } => {
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
    Dismiss,
}

pub enum Outcome<M> {
    None,
    Run(M),
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
        self.items.iter().enumerate().filter(|(_, item)| item.actionable()).map(|(index, _)| index).collect()
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

    pub fn back(&mut self) -> bool {
        let Some(items) = self.trail.pop() else { return false };
        self.items = items;
        self.selected = self.actionable().first().copied();
        true
    }

    pub fn step(&mut self, forward: bool) {
        let rows = self.actionable();
        if rows.is_empty() {
            return;
        }
        let position = self.selected.and_then(|selected| rows.iter().position(|row| *row == selected));
        let next = match (position, forward) {
            (None, true) => 0,
            (None, false) => rows.len() - 1,
            (Some(at), true) => (at + 1) % rows.len(),
            (Some(at), false) => (at + rows.len() - 1) % rows.len(),
        };
        self.selected = Some(rows[next]);
    }

    pub fn key(&mut self, key: &Key) -> Option<Outcome<M>> {
        Some(match key {
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
        2.0 * SP1
            + 2.0
            + self
                .items
                .iter()
                .map(|item| match item {
                    Item::Separator => SEPARATOR,
                    Item::Heading(_) => HEADING,
                    _ => ITEM,
                })
                .sum::<f32>()
    }

    #[must_use]
    pub fn placement(&self, window: Size) -> (Point, Size) {
        let width = WIDTH.min(window.width - 2.0 * SP2);
        let height = self.height().min((window.height - 2.0 * SP3).max(1.0));
        let upward = self.source == Source::Context && self.at.y > window.height * 0.6;
        let x = self.at.x.min(window.width - width - SP2).max(SP2);
        let y = if upward { self.at.y - height - SP1 } else { self.at.y };
        (Point::new(x, y.min(window.height - height - SP2).max(SP2)), Size::new(width, height))
    }
}

fn row_element<M>(paint: &Paint, item: &Item<M>, index: usize, selected: bool) -> Element<'static, Event> {
    let paint = *paint;
    match item {
        Item::Separator => container(
            container(Space::new().width(Fill).height(1)).style(move |_| style::rounded(paint.hairline(), 0.0)),
        )
        .padding(Padding::from([SP1, SP2]))
        .into(),
        Item::Heading(label) => container(text(label.clone(), TextStyle::MICRO).color(paint.fg_3()))
            .padding(Padding::from([SP2, SP3]).bottom(SP1))
            .into(),
        Item::Action { label, enabled, .. } | Item::Submenu { label, enabled, .. } => {
            let enabled = *enabled;
            let (shortcut, check, more) = match item {
                Item::Action { shortcut, check, .. } => (shortcut.clone(), *check, false),
                _ => (None, None, true),
            };
            let mark: Element<'static, Event> = match check {
                Some(true) => icon(Icon::Check, 14.0, if enabled { paint.acc_bg() } else { paint.fg_4() }),
                _ => Space::new().width(14).height(14).into(),
            };
            let mut content = row![
                mark,
                text(label.clone(), TextStyle::MENU_ITEM).wrapping(Wrapping::None),
                Space::new().width(Fill)
            ]
            .spacing(SP2)
            .align_y(Alignment::Center);
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
                style
            });
            mouse_area(control).on_enter(Event::Hovered(index)).into()
        }
    }
}

#[must_use]
pub fn view<M>(paint: &Paint, popup: &Popup<M>, window: Size) -> Element<'static, Event> {
    let progress = popup.reveal.value(motion::t2()) as f32;
    let paint = paint.faded(progress);
    let rows: Vec<Element<'static, Event>> = popup
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| row_element(&paint, item, index, popup.selected == Some(index)))
        .collect();
    let (at, size) = popup.placement(window);
    let content = scrollable(&paint, iced::widget::column(rows)).height(size.height - 2.0 * SP1 - 2.0);
    let card = container(popover(&paint, content)).width(size.width);
    let scale = component::POPOVER_CLOSED_SCALE + (1.0 - component::POPOVER_CLOSED_SCALE) * progress;
    stack![
        mouse_area(Space::new().width(Fill).height(Fill))
            .on_press(Event::Dismiss)
            .on_right_press(Event::Dismiss)
            .on_middle_press(Event::Dismiss),
        pin(widgets::scale(card, scale)).x(at.x).y(at.y + (1.0 - progress) * component::POPOVER_RISE),
    ]
    .into()
}

#[must_use]
pub fn bar_width(label: &str) -> f32 {
    crate::widget::measure(label, TextStyle::CALL) + 2.0 * SP3
}

#[must_use]
pub fn bar<M>(paint: &Paint, menus: &[Menu<M>], open: Option<usize>) -> Element<'static, BarEvent> {
    let paint = *paint;
    let buttons = menus.iter().enumerate().map(|(index, menu)| {
        let active = open == Some(index);
        let control =
            button(container(text(menu.label.clone(), TextStyle::CALL).wrapping(Wrapping::None)).center(Fill))
                .on_press(BarEvent::Pressed(index))
                .width(bar_width(&menu.label))
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
}

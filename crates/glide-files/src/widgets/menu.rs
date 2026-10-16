use std::borrow::Cow;
use std::path::Path;

use iced::widget::{
    Column, button, container, float, mouse_area, opaque, pin, responsive, row, rule, scrollable, space, stack, text,
};
use iced::{Alignment, Element, Fill, Length, Padding, Point, Size, Vector, mouse};

use super::HEADER_HEIGHT;
use crate::app::{App, Menu, MenuKind, Message, ViewMode};
use crate::fs::{Offer, SortBy, Volume};
use crate::i18n::Key;
use crate::theme::{self, ThemeState};

pub const WIDTH: f32 = 232.0;
pub const ROW: f32 = 26.0;
const SEPARATOR: f32 = 9.0;
const HEADER: f32 = 24.0;
const PADDING: f32 = 5.0;
const MARGIN: f32 = 8.0;
const RISE: f32 = 6.0;
const OPTIONS_RIGHT_INSET: f32 = 12.0 + super::header::WINDOW_CONTROL + 2.0 + 3.0 * 10.0 + super::header::SEARCH_WIDTH;

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub label: Cow<'static, str>,
    pub message: Option<Message>,
    pub shortcut: Option<&'static str>,
    pub danger: bool,
    pub checked: Option<bool>,
    pub swatch: Option<iced::Color>,
}

impl Item {
    fn new(label: impl Into<Cow<'static, str>>, message: Message) -> Self {
        Self { label: label.into(), message: Some(message), shortcut: None, danger: false, checked: None, swatch: None }
    }

    fn swatch(mut self, color: iced::Color) -> Self {
        self.swatch = Some(color);
        self
    }

    fn submenu(self) -> Self {
        self.keys("›")
    }

    fn keys(mut self, shortcut: &'static str) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    fn enabled(mut self, enabled: bool) -> Self {
        if !enabled {
            self.message = None;
        }
        self
    }

    fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    fn checked(mut self, on: bool) -> Self {
        self.checked = Some(on);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Item(Item),
    Separator,
    Header(Cow<'static, str>),
}

impl From<Item> for Entry {
    fn from(item: Item) -> Self {
        Self::Item(item)
    }
}

impl Entry {
    fn height(&self) -> f32 {
        match self {
            Self::Item(_) => ROW,
            Self::Separator => SEPARATOR,
            Self::Header(_) => HEADER,
        }
    }
}

fn bookmarkable(app: &App, path: &Path) -> bool {
    path.is_dir()
        && !app.is_bookmarked(path)
        && !app.places.iter().any(|place| place.path == path)
        && !app.trash.contains(path)
}

fn joined(paths: &[std::path::PathBuf]) -> String {
    paths.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join("\n")
}

fn offered(offers: &[Offer]) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut group = None;
    for (index, offer) in offers.iter().enumerate() {
        if index == 0 || offer.group != group {
            entries.push(Entry::Separator);
            entries.extend(offer.group.clone().map(|name| Entry::Header(name.into())));
            group.clone_from(&offer.group);
        }
        entries.push(Item::new(offer.label.clone(), Message::RunAction(Box::new(offer.clone()))).into());
    }
    entries
}

#[must_use]
#[allow(clippy::too_many_lines)] // One declarative entry list per menu kind
pub fn entries(app: &App, menu: &Menu) -> Vec<Entry> {
    let kind = &menu.kind;
    let item = |key: Key, message: Message| Item::new(key.text(app.lang), message);
    let selected = app.selected();
    let single = selected.len() == 1;
    let single_dir = single && selected[0].is_dir();
    let in_trash = app.in_trash();
    let properties = item(Key::Properties, Message::Properties).keys("Ctrl+I");
    match kind {
        MenuKind::Selection if in_trash => vec![
            item(Key::Restore, Message::Restore)
                .enabled(selected.iter().any(|path| app.trash.is_top_level(path)))
                .into(),
            item(Key::DeletePermanently, Message::Delete).keys("Shift+Del").danger().into(),
            Entry::Separator,
            properties.enabled(single).into(),
        ],
        MenuKind::Selection => {
            let bookmark = selected
                .first()
                .filter(|path| single && bookmarkable(app, path))
                .map(|path| item(Key::AddBookmark, Message::AddBookmark(path.clone())));
            let mut entries: Vec<Entry> = vec![item(Key::Open, Message::OpenSelection).keys("Enter").into()];
            if menu.open_with {
                entries.push(item(Key::OpenWith, Message::SubMenu(MenuKind::OpenWith)).submenu().into());
            }
            entries.push(item(Key::OpenInTerminal, Message::Terminal).enabled(single_dir).into());
            entries.extend(offered(&menu.offers));
            entries.extend([
                Entry::Separator,
                item(Key::Cut, Message::Cut).keys("Ctrl+X").into(),
                item(Key::Copy, Message::Copy).keys("Ctrl+C").into(),
                item(Key::Duplicate, Message::Duplicate).keys("Ctrl+D").into(),
                item(Key::CreateLink, Message::CreateLink).into(),
                item(Key::CopyPath, Message::CopyText(joined(&selected))).into(),
                Entry::Separator,
                item(Key::Tags, Message::SubMenu(MenuKind::Tags)).submenu().into(),
            ]);
            entries.extend(bookmark.map(Entry::from));
            entries.extend([
                item(Key::Rename, Message::Rename).keys("F2").enabled(single).into(),
                item(Key::MoveToTrash, Message::Trash).keys("Del").danger().into(),
                Entry::Separator,
                properties.enabled(single).into(),
            ]);
            entries
        }
        MenuKind::Background if in_trash => vec![
            item(Key::EmptyTrash, Message::EmptyTrash).danger().enabled(app.trash_count > 0).into(),
            Entry::Separator,
            item(Key::SelectAll, Message::SelectAll).keys("Ctrl+A").into(),
        ],
        MenuKind::Background => vec![
            item(Key::NewFolder, Message::NewFolder).keys("Ctrl+Shift+N").into(),
            item(Key::NewFile, Message::NewFile).into(),
            item(Key::NewDocument, Message::SubMenu(MenuKind::Templates)).submenu().into(),
            Entry::Separator,
            item(Key::Paste, Message::Paste).keys("Ctrl+V").into(),
            item(Key::SelectAll, Message::SelectAll).keys("Ctrl+A").into(),
            Entry::Separator,
            Entry::Header(Key::ViewAs.text(app.lang).into()),
            item(Key::ViewGrid, Message::SetView(ViewMode::Grid))
                .keys("Ctrl+1")
                .checked(app.view == ViewMode::Grid)
                .into(),
            item(Key::ViewList, Message::SetView(ViewMode::List))
                .keys("Ctrl+2")
                .checked(app.view == ViewMode::List)
                .into(),
            Entry::Separator,
            item(Key::OpenInTerminal, Message::Terminal).into(),
            item(Key::CopyPath, Message::CopyText(app.current.display().to_string())).into(),
            properties.into(),
        ],
        MenuKind::Location(path) => {
            let mut entries: Vec<Entry> = vec![
                item(Key::Open, Message::Navigate(path.clone())).into(),
                item(Key::OpenInTerminal, Message::TerminalAt(path.clone())).enabled(!app.trash.contains(path)).into(),
                item(Key::CopyPath, Message::CopyText(path.display().to_string())).into(),
                Entry::Separator,
            ];
            if app.is_bookmarked(path) {
                entries.push(item(Key::RemoveBookmark, Message::RemoveBookmark(path.clone())).into());
                entries.push(Entry::Separator);
            } else if bookmarkable(app, path) {
                entries.push(item(Key::AddBookmark, Message::AddBookmark(path.clone())).into());
                entries.push(Entry::Separator);
            }
            if *path == app.trash.files() {
                entries.push(item(Key::EmptyTrash, Message::EmptyTrash).danger().enabled(app.trash_count > 0).into());
                entries.push(Entry::Separator);
            }
            entries.push(item(Key::Properties, Message::PropertiesOf(path.clone())).into());
            entries
        }
        MenuKind::Volume(object) => app.volume(object).map(|volume| volume_entries(app, volume)).unwrap_or_default(),
        MenuKind::Options => {
            let mut entries = vec![Entry::Header(Key::SortBy.text(app.lang).into())];
            entries.extend(
                SortBy::ALL
                    .into_iter()
                    .map(|by| Item::new(by.label(app.lang), Message::SortBy(by)).checked(app.sort.by == by).into()),
            );
            entries.extend([
                item(Key::Reversed, Message::ToggleReverse).checked(!app.sort.ascending).into(),
                Entry::Separator,
                item(Key::ShowHidden, Message::ToggleHidden).keys("Ctrl+H").checked(app.show_hidden).into(),
                Entry::Separator,
                item(Key::ZoomIn, Message::Zoom(1)).keys("Ctrl++").into(),
                item(Key::ZoomOut, Message::Zoom(-1)).keys("Ctrl+-").into(),
                item(Key::ZoomReset, Message::Zoom(0)).keys("Ctrl+0").into(),
                item(Key::Columns, Message::SubMenu(MenuKind::Columns))
                    .submenu()
                    .enabled(app.view == ViewMode::List)
                    .into(),
                Entry::Separator,
                item(Key::QueueOperations, Message::ToggleQueue).checked(app.queue).into(),
                Entry::Separator,
                item(Key::NewFolder, Message::NewFolder).keys("Ctrl+Shift+N").enabled(!in_trash).into(),
                item(Key::NewFile, Message::NewFile).enabled(!in_trash).into(),
                item(Key::OpenInTerminal, Message::Terminal).into(),
                Entry::Separator,
                item(Key::Reload, Message::Reload).keys("F5").into(),
            ]);
            entries
        }
        MenuKind::Templates => {
            let mut entries = vec![Entry::Header(Key::NewDocument.text(app.lang).into())];
            let root = crate::fs::templates_dir().unwrap_or_default();
            entries.extend(app.templates.iter().map(|template| {
                let shown = template.strip_prefix(&root).unwrap_or(template).with_extension("");
                Item::new(shown.display().to_string(), Message::NewFromTemplate(template.clone())).into()
            }));
            if app.templates.is_empty() {
                entries.push(item(Key::NoTemplates, Message::CloseMenu).enabled(false).into());
            }
            entries.extend([Entry::Separator, item(Key::OpenTemplates, Message::OpenTemplates).into()]);
            entries
        }
        MenuKind::Tags => {
            let entries_of = app.selected_entries().collect::<Vec<_>>();
            let mut entries: Vec<Entry> = crate::fs::tags::Color::ALL
                .into_iter()
                .map(|color| {
                    let on = !entries_of.is_empty()
                        && entries_of.iter().all(|entry| crate::fs::tags::colors(&entry.tags).contains(&color));
                    item(color_key(color), Message::ToggleTag(color))
                        .swatch(super::content::tag_color(color))
                        .checked(on)
                        .into()
                })
                .collect();
            entries.extend([Entry::Separator, item(Key::EditTags, Message::EditTags).into()]);
            entries
        }
        MenuKind::OpenWith => offered(&menu.offers).into_iter().skip(1).collect(),
        MenuKind::Columns => {
            let mut entries = vec![Entry::Header(Key::Columns.text(app.lang).into())];
            entries.extend(crate::config::ListColumn::ALL.into_iter().map(|column| {
                Item::new(super::content::column_label(column, app.lang), Message::ToggleColumn(column))
                    .checked(app.columns.contains(&column))
                    .into()
            }));
            entries
        }
    }
}

const fn color_key(color: crate::fs::tags::Color) -> Key {
    use crate::fs::tags::Color;
    match color {
        Color::Red => Key::TagRed,
        Color::Orange => Key::TagOrange,
        Color::Yellow => Key::TagYellow,
        Color::Green => Key::TagGreen,
        Color::Blue => Key::TagBlue,
        Color::Purple => Key::TagPurple,
        Color::Gray => Key::TagGray,
    }
}

fn volume_entries(app: &App, volume: &Volume) -> Vec<Entry> {
    let item = |key: Key, message: Message| Item::new(key.text(app.lang), message);
    let idle = !app.busy.contains(&volume.object);
    let object = volume.object.clone();
    let mut entries: Vec<Entry> = match &volume.mount {
        Some(path) => vec![
            item(Key::Open, Message::Navigate(path.clone())).into(),
            item(Key::OpenInTerminal, Message::TerminalAt(path.clone())).into(),
            item(Key::CopyPath, Message::CopyText(path.display().to_string())).into(),
            Entry::Separator,
            item(Key::Unmount, Message::Unmount(object.clone())).enabled(idle).into(),
        ],
        None => vec![
            item(Key::Open, Message::Mount(object.clone(), false)).enabled(idle).into(),
            item(Key::MountReadOnly, Message::Mount(object.clone(), true)).enabled(idle).into(),
        ],
    };
    if volume.can_remove() {
        entries.push(item(Key::Eject, Message::Eject(object.clone())).enabled(idle).into());
    }
    entries.push(Entry::Separator);
    entries.push(item(Key::Repair, Message::Repair(object)).enabled(idle).into());
    if let Some(path) = &volume.mount {
        entries.push(item(Key::Properties, Message::PropertiesOf(path.clone())).into());
    }
    entries
}

#[must_use]
pub fn height(entries: &[Entry]) -> f32 {
    entries.iter().map(Entry::height).sum::<f32>() + 2.0 * PADDING + 2.0
}

#[must_use]
pub fn place(at: Point, height: f32, window: Size) -> Point {
    let x = if at.x + WIDTH > window.width - MARGIN { (at.x - WIDTH).max(MARGIN) } else { at.x };
    let y = if at.y + height > window.height - MARGIN { (window.height - height - MARGIN).max(MARGIN) } else { at.y };
    Point::new(x, y)
}

#[must_use]
pub fn overlay<'a>(app: &'a App, menu: &'a Menu, progress: f32, live: bool) -> Element<'a, Message> {
    responsive(move |window| {
        let entries = entries(app, menu);
        let full = height(&entries);
        let at = match menu.kind {
            MenuKind::Options => {
                Point::new((window.width - OPTIONS_RIGHT_INSET - WIDTH).max(MARGIN), HEADER_HEIGHT - 6.0)
            }
            _ => place(menu.at, full, window),
        };
        let fit = full.min(window.height - at.y - MARGIN);
        let rise = (1.0 - progress) * -RISE;
        let panel =
            float(panel(&app.theme.faded(progress), entries, live, fit)).translate(move |_, _| Vector::new(0.0, rise));
        let panel: Element<'_, Message> = if live { opaque(panel) } else { panel.into() };
        let positioned = pin(panel).position(at);
        if live {
            let dismiss = mouse_area(space().width(Fill).height(Fill))
                .on_press(Message::CloseMenu)
                .on_right_press(Message::CloseMenu)
                .interaction(mouse::Interaction::Idle);
            stack![dismiss, positioned].into()
        } else {
            positioned.into()
        }
    })
    .into()
}

fn panel<'a>(theme: &ThemeState, entries: Vec<Entry>, live: bool, fit: f32) -> Element<'a, Message> {
    let t = *theme;
    let checks = entries.iter().any(|entry| matches!(entry, Entry::Item(Item { checked: Some(_), .. })));
    let rows = entries.into_iter().map(|entry| -> Element<'a, Message> {
        match entry {
            Entry::Separator => {
                container(rule::horizontal(1).style(move |_| theme::line(&t))).padding(Padding::from([4, 8])).into()
            }
            Entry::Header(label) => container(text(label).size(11).font(theme::SEMIBOLD).color(t.fg_3()))
                .padding(Padding::from([0, 8]))
                .height(Length::Fixed(HEADER))
                .align_y(Alignment::Center)
                .into(),
            Entry::Item(item) => {
                let mut content = row![].spacing(6).align_y(Alignment::Center);
                if checks {
                    let mark = if item.checked == Some(true) { "✓" } else { "" };
                    content = content.push(text(mark).size(13).width(Length::Fixed(14.0)));
                }
                if let Some(color) = item.swatch {
                    content = content
                        .push(container(space().width(10).height(10)).style(move |_| theme::rounded(color, 5.0)));
                }
                content = content.push(text(item.label).size(13)).push(space::horizontal());
                if let Some(keys) = item.shortcut {
                    content = content.push(text(keys).size(12));
                }
                let danger = item.danger;
                let enabled = item.message.is_some();
                let message = item.message.filter(|_| live).map(|message| Message::Menu(Box::new(message)));
                button(container(content).height(Fill).align_y(Alignment::Center))
                    .on_press_maybe(message)
                    .width(Fill)
                    .height(Length::Fixed(ROW))
                    .padding(Padding::from([0, 8]))
                    .style(move |_, status| {
                        let status = match (enabled, live) {
                            (false, _) => button::Status::Disabled,
                            (true, false) => button::Status::Active,
                            (true, true) => status,
                        };
                        theme::menu_item(&t, danger, status)
                    })
                    .into()
            }
        }
    });
    container(scrollable(Column::with_children(rows)).height(Fill).style(move |_, status| theme::scroller(&t, status)))
        .padding(PADDING)
        .width(Length::Fixed(WIDTH))
        .height(Length::Fixed(fit))
        .style(move |_| theme::popover(&t))
        .into()
}

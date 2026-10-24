use std::path::Path;

use glide_sdk::menu::{self as sdk, Event, Walk};
use iced::widget::responsive;
use iced::{Element, Point, Size};

use super::header::{GAP, INSET, SEARCH_WIDTH, WINDOW_CONTROL};
use super::{CONTROL, HEADER_HEIGHT};
use crate::app::{App, Menu, MenuKind, Message, Step, ViewMode};
use crate::fs::{Offer, SortBy, Volume};
use crate::i18n::Key;
use crate::theme::SP2;

pub type Entry = sdk::Item<Message>;

pub const WIDTH: f32 = 248.0;
const OPTIONS_RIGHT_INSET: f32 = INSET + WINDOW_CONTROL + GAP + SEARCH_WIDTH + GAP;
const OPTIONS_TOP: f32 = HEADER_HEIGHT.midpoint(CONTROL) + SP2;

#[must_use]
pub fn next_highlight(entries: &[Entry], current: Option<usize>, step: Step) -> Option<usize> {
    let walk = match step {
        Step::First => Walk::First,
        Step::Last => Walk::Last,
        Step::Down => Walk::Next,
        Step::Up => Walk::Previous,
        Step::Left | Step::Right => {
            return current.filter(|at| entries.get(*at).is_some_and(|e| e.message().is_some()));
        }
    };
    sdk::walk(entries, current, walk)
}

#[must_use]
pub fn span(entries: &[Entry], index: usize) -> (f32, f32) {
    sdk::span(entries, index)
}

#[must_use]
pub(crate) fn options_at(window: Size) -> Point {
    options_for(window, WIDTH)
}

fn options_for(window: Size, width: f32) -> Point {
    Point::new((window.width - OPTIONS_RIGHT_INSET - width).max(SP2), OPTIONS_TOP)
}

fn bookmarkable(app: &App, path: &Path) -> bool {
    path.is_dir()
        && !app.is_bookmarked(path)
        && !app.places.iter().any(|place| place.path == path)
        && !app.is_device(path)
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
            entries.extend(offer.group.clone().map(Entry::heading));
            group.clone_from(&offer.group);
        }
        entries.push(Entry::action(offer.label.clone(), Message::RunAction(Box::new(offer.clone()))));
    }
    entries
}

#[must_use]
#[allow(clippy::too_many_lines)] // One declarative entry list per menu kind
pub fn entries(app: &App, menu: &Menu) -> Vec<Entry> {
    let kind = &menu.kind;
    let item = |key: Key, message: Message| Entry::action(key.text(app.lang), message);
    let selected = app.selected();
    let single = selected.len() == 1;
    let single_dir = single && selected[0].is_dir();
    let in_trash = app.in_trash();
    let properties = item(Key::Properties, Message::Properties).shortcut("Ctrl+I");
    match kind {
        MenuKind::Selection if in_trash => vec![
            item(Key::Restore, Message::Restore).enabled(selected.iter().any(|path| app.trash.is_top_level(path))),
            item(Key::DeletePermanently, Message::Delete).shortcut("Shift+Del").danger(),
            Entry::Separator,
            properties.enabled(single),
        ],
        MenuKind::Selection => {
            let bookmark = selected
                .first()
                .filter(|path| single && bookmarkable(app, path))
                .map(|path| item(Key::AddBookmark, Message::AddBookmark(path.clone())));
            let mut entries = vec![item(Key::Open, Message::OpenSelection).shortcut("Enter")];
            if let (true, [found]) = (app.is_searching(), selected.as_slice()) {
                entries.push(item(Key::ShowInFolder, Message::Reveal(found.clone())));
            }
            if menu.open_with {
                entries.push(item(Key::OpenWith, Message::SubMenu(MenuKind::OpenWith)).cascades());
            }
            if single_dir {
                entries.push(item(Key::OpenInTerminal, Message::Terminal));
            }
            entries.extend(offered(&menu.offers));
            entries.extend([
                Entry::Separator,
                item(Key::Cut, Message::Cut).shortcut("Ctrl+X"),
                item(Key::Copy, Message::Copy).shortcut("Ctrl+C"),
                item(Key::Duplicate, Message::Duplicate).shortcut("Ctrl+D"),
                item(Key::CreateLink, Message::CreateLink),
                item(Key::CopyPath, Message::CopyText(joined(&selected))),
                Entry::Separator,
                item(Key::Tags, Message::SubMenu(MenuKind::Tags)).cascades(),
            ]);
            entries.extend(bookmark);
            entries.extend([
                item(Key::Rename, Message::Rename).shortcut("F2").enabled(single),
                item(Key::MoveToTrash, Message::Trash).shortcut("Del").danger(),
                Entry::Separator,
                properties.enabled(single),
            ]);
            entries
        }
        MenuKind::Background if in_trash => vec![
            item(Key::EmptyTrash, Message::EmptyTrash).danger().enabled(app.trash_count > 0),
            Entry::Separator,
            item(Key::SelectAll, Message::SelectAll).shortcut("Ctrl+A"),
        ],
        MenuKind::Background => vec![
            item(Key::NewFolder, Message::NewFolder).shortcut("Ctrl+Shift+N"),
            item(Key::NewFile, Message::NewFile),
            item(Key::NewDocument, Message::SubMenu(MenuKind::Templates)).cascades(),
            Entry::Separator,
            item(Key::Paste, Message::Paste).shortcut("Ctrl+V"),
            item(Key::SelectAll, Message::SelectAll).shortcut("Ctrl+A"),
            Entry::Separator,
            Entry::heading(Key::ViewAs.text(app.lang)),
            item(Key::ViewGrid, Message::SetView(ViewMode::Grid))
                .shortcut("Ctrl+1")
                .checked(app.view == ViewMode::Grid),
            item(Key::ViewList, Message::SetView(ViewMode::List))
                .shortcut("Ctrl+2")
                .checked(app.view == ViewMode::List),
            Entry::Separator,
            item(Key::OpenInTerminal, Message::Terminal),
            item(Key::CopyPath, Message::CopyText(app.current.display().to_string())),
            properties,
        ],
        MenuKind::Location(path) => {
            let mut entries = vec![
                item(Key::Open, Message::Navigate(path.clone())),
                item(Key::OpenInTerminal, Message::TerminalAt(path.clone())).enabled(!app.trash.contains(path)),
                item(Key::CopyPath, Message::CopyText(path.display().to_string())),
                Entry::Separator,
            ];
            if app.is_bookmarked(path) {
                entries.push(item(Key::RemoveBookmark, Message::RemoveBookmark(path.clone())));
                entries.push(Entry::Separator);
            } else if bookmarkable(app, path) {
                entries.push(item(Key::AddBookmark, Message::AddBookmark(path.clone())));
                entries.push(Entry::Separator);
            }
            if *path == app.trash.files() {
                entries.push(item(Key::EmptyTrash, Message::EmptyTrash).danger().enabled(app.trash_count > 0));
                entries.push(Entry::Separator);
            }
            entries.push(item(Key::Properties, Message::PropertiesOf(path.clone())));
            entries
        }
        MenuKind::Volume(object) => app.volume(object).map(|volume| volume_entries(app, volume)).unwrap_or_default(),
        MenuKind::Options => {
            let mut entries = vec![Entry::heading(Key::SortBy.text(app.lang))];
            entries.extend(
                SortBy::ALL
                    .into_iter()
                    .map(|by| Entry::action(by.label(app.lang), Message::SortKey(by)).checked(app.sort.by == by)),
            );
            entries.extend([
                item(Key::Reversed, Message::ToggleReverse).checked(!app.sort.ascending),
                Entry::Separator,
                item(Key::ShowHidden, Message::ToggleHidden).shortcut("Ctrl+H").checked(app.show_hidden),
                Entry::Separator,
                item(Key::ZoomIn, Message::Zoom(1)).shortcut("Ctrl++"),
                item(Key::ZoomOut, Message::Zoom(-1)).shortcut("Ctrl+-"),
                item(Key::ZoomReset, Message::Zoom(0)).shortcut("Ctrl+0"),
                item(Key::Columns, Message::SubMenu(MenuKind::Columns)).cascades().enabled(app.view == ViewMode::List),
                Entry::Separator,
                item(Key::QueueOperations, Message::ToggleQueue).checked(app.queue),
                Entry::Separator,
                item(Key::NewFolder, Message::NewFolder).shortcut("Ctrl+Shift+N").enabled(!in_trash),
                item(Key::NewFile, Message::NewFile).enabled(!in_trash),
                item(Key::OpenInTerminal, Message::Terminal),
                Entry::Separator,
                item(Key::Reload, Message::Reload).shortcut("F5"),
            ]);
            entries
        }
        MenuKind::Templates => {
            let mut entries = vec![Entry::heading(Key::NewDocument.text(app.lang))];
            let root = crate::fs::templates_dir().unwrap_or_default();
            entries.extend(app.templates.iter().map(|template| {
                let shown = template.strip_prefix(&root).unwrap_or(template).with_extension("");
                Entry::action(shown.display().to_string(), Message::NewFromTemplate(template.clone()))
            }));
            if app.templates.is_empty() {
                entries.push(item(Key::NoTemplates, Message::CloseMenu).enabled(false));
            }
            entries.extend([Entry::Separator, item(Key::OpenTemplates, Message::OpenTemplates)]);
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
                })
                .collect();
            entries.extend([Entry::Separator, item(Key::EditTags, Message::EditTags)]);
            entries
        }
        MenuKind::OpenWith => {
            let mut entries: Vec<Entry> = offered(&menu.offers).into_iter().skip(1).collect();
            if !entries.is_empty() {
                entries.push(Entry::Separator);
            }
            entries.push(item(Key::OtherApp, Message::OpenWithDialog));
            entries
        }
        MenuKind::Columns => {
            let mut entries = vec![Entry::heading(Key::Columns.text(app.lang))];
            entries.extend(crate::config::ListColumn::ALL.into_iter().map(|column| {
                Entry::action(super::content::column_label(column, app.lang), Message::ToggleColumn(column))
                    .checked(app.columns.contains(&column))
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
    let item = |key: Key, message: Message| Entry::action(key.text(app.lang), message);
    let idle = !app.busy.contains(&volume.object);
    let object = volume.object.clone();
    let mut entries = match &volume.mount {
        Some(path) => vec![
            item(Key::Open, Message::Navigate(path.clone())),
            item(Key::OpenInTerminal, Message::TerminalAt(path.clone())),
            item(Key::CopyPath, Message::CopyText(path.display().to_string())),
            Entry::Separator,
            item(Key::Unmount, Message::Unmount(object.clone())).enabled(idle),
        ],
        None => vec![
            item(Key::Open, Message::Mount(object.clone(), false)).enabled(idle),
            item(Key::MountReadOnly, Message::Mount(object.clone(), true)).enabled(idle),
        ],
    };
    if volume.can_remove() {
        entries.push(item(Key::Eject, Message::Eject(object.clone())).enabled(idle));
    }
    entries.push(Entry::Separator);
    entries.push(item(Key::Repair, Message::Repair(object)).enabled(idle));
    if let Some(path) = &volume.mount {
        entries.push(item(Key::Properties, Message::PropertiesOf(path.clone())));
    }
    entries
}

#[must_use]
pub fn overlay<'a>(app: &'a App, menu: &'a Menu, progress: f32, live: bool) -> Element<'a, Message> {
    responsive(move |window| {
        let entries = entries(app, menu);
        let width = sdk::width(&entries, WIDTH);
        let height = sdk::height(&entries);
        let (at, size) = match menu.kind {
            MenuKind::Options => sdk::place(options_for(window, width), height, width, window, false),
            _ => sdk::place(menu.at, height, width, window, true),
        };
        let paint = super::paint(&app.theme.faded(progress));
        let rows = sdk::rows(&paint, &entries, menu.highlight)
            .into_iter()
            .zip(&entries)
            .enumerate()
            .map(|(index, (row, entry))| {
                let message = entry.message().cloned().filter(|_| live);
                row.map(move |event| match (event, &message) {
                    (Event::Picked(_), Some(message)) => Message::Menu(Box::new(message.clone())),
                    (Event::Dismiss, _) => Message::CloseMenu,
                    (_, picked) => Message::MenuHover(index, live && picked.is_some()),
                })
            })
            .collect();
        sdk::layer(&paint, rows, at, size, progress, live.then_some(Message::CloseMenu))
    })
    .into()
}

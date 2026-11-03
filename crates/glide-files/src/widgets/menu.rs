use std::path::Path;

use glide_sdk::menu::{self as sdk, Event, Walk};
use iced::widget::responsive;
use iced::{Element, Point, Size};

use super::header::{GAP, WINDOW_CONTROL};
use super::{CONTROL, HEADER_HEIGHT};
use crate::app::{App, Menu, MenuKind, Message, Step, ViewMode};
use crate::fs::{Offer, SortBy, Volume};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::theme::SP2;

pub type Entry = sdk::Item<Message>;

pub const WIDTH: f32 = 248.0;
const SUB_WIDTH: f32 = 236.0;
const OPTIONS_RIGHT_INSET: f32 = 2.0 * SP2 + WINDOW_CONTROL + 2.0 + GAP;
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

pub(crate) fn bookmarkable(app: &App, path: &Path) -> bool {
    path.is_dir()
        && !app.is_bookmarked(path)
        && !app.places.iter().any(|place| place.path == path)
        && !app.is_device(path)
        && !app.trash.contains(path)
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
    let glyphed = |key: Key, message: Message, icon: Icon| item(key, message).glyph(icon.handle());
    let properties = item(Key::Properties, Message::Properties).shortcut("Alt+Enter");
    match kind {
        MenuKind::Selection if in_trash => vec![
            item(Key::Restore, Message::Restore).enabled(selected.iter().any(|path| app.trash.is_top_level(path))),
            glyphed(Key::DeletePermanently, Message::Delete, Icon::Trash).shortcut("Shift+Delete").danger(),
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
            if single_dir {
                entries.push(item(Key::OpenInNewWindow, Message::NewWindow(Some(selected[0].clone()))));
            }
            if menu.open_with {
                entries.push(item(Key::OpenWith, Message::SubMenu(MenuKind::OpenWith)).cascades());
            }
            entries.extend([
                Entry::Separator,
                glyphed(Key::Cut, Message::Cut, Icon::Cut).shortcut("Ctrl+X"),
                glyphed(Key::Copy, Message::Copy, Icon::Copy).shortcut("Ctrl+C"),
                item(Key::Duplicate, Message::Duplicate).shortcut("Ctrl+D"),
                item(Key::CreateLink, Message::CreateLink),
                item(Key::Rename, Message::Rename).shortcut("F2").enabled(single),
                Entry::Separator,
                Entry::heading(Key::Tags.text(app.lang)),
                tag_swatches(app),
                item(Key::EditTags, Message::EditTags),
                Entry::Separator,
                glyphed(Key::OpenInTerminal, Message::Terminal, Icon::Terminal),
                item(Key::CopyPath, Message::CopyPath).shortcut("Ctrl+Shift+C"),
            ]);
            entries.extend(bookmark);
            entries.extend(offered(&menu.offers).into_iter().skip(1));
            entries.extend([
                Entry::Separator,
                glyphed(Key::MoveToTrash, Message::Trash, Icon::Trash).shortcut("Delete").danger(),
                Entry::Separator,
                properties.enabled(single),
            ]);
            entries
        }
        MenuKind::Background if in_trash => vec![
            glyphed(Key::EmptyTrash, Message::EmptyTrash, Icon::Trash).danger().enabled(app.trash_count > 0),
            Entry::Separator,
            item(Key::SelectAll, Message::SelectAll).shortcut("Ctrl+A"),
        ],
        MenuKind::Background => vec![
            glyphed(Key::NewFolder, Message::NewFolder, Icon::FolderPlus).shortcut("Ctrl+Shift+N"),
            item(Key::NewDocument, Message::SubMenu(MenuKind::Templates)).cascades(),
            Entry::Separator,
            item(Key::Paste, Message::Paste).shortcut("Ctrl+V"),
            item(Key::SelectAll, Message::SelectAll).shortcut("Ctrl+A"),
            Entry::Separator,
            glyphed(Key::OpenInTerminal, Message::Terminal, Icon::Terminal),
            item(Key::AddBookmark, Message::BookmarkHere).shortcut("Ctrl+B").enabled(bookmarkable(app, &app.current)),
            Entry::Separator,
            item(Key::FolderProperties, Message::Properties),
        ],
        MenuKind::Location(path) => {
            let mut entries = vec![
                item(Key::Open, Message::Navigate(path.clone())),
                item(Key::OpenInNewWindow, Message::NewWindow(Some(path.clone()))),
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
            let (level, levels) = app.zoom_level();
            let zoom = Entry::stepper(
                Key::Zoom.text(app.lang),
                format!("{} %", app.metrics().percent(app.view)),
                (level > 0).then_some(Message::Zoom(-1)),
                (level + 1 < levels).then_some(Message::Zoom(1)),
            );
            let mut entries = vec![
                Entry::heading(Key::ViewAs.text(app.lang)),
                zoom,
                item(Key::ShowHidden, Message::ToggleHidden).shortcut("Ctrl+H").checked(app.show_hidden),
                item(Key::InspectorPanel, Message::ToggleInspector)
                    .shortcut("Ctrl+I")
                    .checked(app.inspector_shown())
                    .enabled(app.inspector_fits()),
                item(Key::Columns, Message::SubMenu(MenuKind::Columns)).cascades().enabled(app.view == ViewMode::List),
                Entry::Separator,
                Entry::heading(Key::SortBy.text(app.lang)),
            ];
            entries.extend(
                SortBy::ALL.into_iter().map(|by| item(sort_key(by), Message::SortKey(by)).checked(app.sort.by == by)),
            );
            entries.extend([
                item(Key::Reversed, Message::ToggleReverse).checked(!app.sort.ascending),
                Entry::Separator,
                item(Key::QueueOperations, Message::ToggleQueue).checked(app.queue),
                item(Key::EnterPath, Message::EditPath).shortcut("Ctrl+L"),
                Entry::Separator,
                item(Key::Shortcuts, Message::ShowShortcuts).shortcut("F1"),
            ]);
            entries
        }
        MenuKind::Templates => {
            let mut entries = vec![item(Key::NewFile, Message::NewFile)];
            let root = crate::fs::templates_dir().unwrap_or_default();
            entries.extend(app.templates.iter().map(|template| {
                let shown = template.strip_prefix(&root).unwrap_or(template);
                Entry::action(shown.display().to_string(), Message::NewFromTemplate(template.clone()))
            }));
            entries.extend([Entry::Separator, item(Key::OpenTemplates, Message::OpenTemplates)]);
            entries
        }
        MenuKind::Tags => {
            let picked: Vec<_> = app.selected_entries().collect();
            let mut entries: Vec<Entry> = crate::fs::tags::Color::ALL
                .into_iter()
                .map(|color| {
                    let on = !picked.is_empty()
                        && picked.iter().all(|entry| crate::fs::tags::colors(&entry.tags).contains(&color));
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
        MenuKind::Columns => crate::config::ListColumn::ALL
            .into_iter()
            .map(|column| {
                Entry::action(super::content::column_label(column, app.lang), Message::ToggleColumn(column))
                    .checked(app.columns.contains(&column))
            })
            .collect(),
    }
}

fn tag_swatches(app: &App) -> Entry {
    let picked: Vec<_> = app.selected_entries().collect();
    Entry::swatches(
        crate::fs::tags::Color::ALL
            .into_iter()
            .map(|color| sdk::Swatch {
                label: color_key(color).text(app.lang).to_string(),
                color: super::content::tag_color(color),
                on: !picked.is_empty()
                    && picked.iter().all(|entry| crate::fs::tags::colors(&entry.tags).contains(&color)),
                message: Message::ToggleTag(color),
            })
            .collect(),
    )
}

const fn sort_key(by: SortBy) -> Key {
    match by {
        SortBy::Name => Key::SortByName,
        SortBy::Size => Key::SortBySize,
        SortBy::Modified => Key::SortByModified,
        SortBy::Type => Key::SortByType,
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
    let lang = app.lang;
    let usage = volume.mount.as_ref().and_then(|path| app.usage.get(path));
    let heading = match usage {
        Some(&(free, total)) => format!(
            "{} · {}",
            volume.name(lang),
            crate::i18n::free_of(&crate::fs::format_bytes(free, lang), &crate::fs::format_bytes(total, lang), lang)
        ),
        None => volume.name(lang),
    };
    let mut entries = vec![Entry::heading(heading)];
    match &volume.mount {
        Some(path) => entries.extend([
            item(Key::Open, Message::Navigate(path.clone())),
            item(Key::OpenInNewWindow, Message::NewWindow(Some(path.clone()))),
        ]),
        None => entries.extend([
            item(Key::Open, Message::Mount(object.clone(), false)).enabled(idle),
            item(Key::MountReadOnly, Message::Mount(object.clone(), true)).enabled(idle),
        ]),
    }
    entries.push(Entry::Separator);
    if volume.can_remove() {
        entries.push(item(Key::Eject, Message::Eject(object.clone())).glyph(Icon::Eject.handle()).enabled(idle));
    }
    if volume.mount.is_some() {
        entries.push(item(Key::Unmount, Message::Unmount(object.clone())).enabled(idle));
    }
    entries.push(Entry::Separator);
    entries.push(item(Key::Repair, Message::Repair(object)).enabled(idle));
    if let Some(path) = &volume.mount {
        entries.push(Entry::Separator);
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
        let root_rows = rows(&paint, &entries, menu.highlight, live, Message::MenuHover);
        let root = sdk::layer(&paint, root_rows, at, size, progress, live.then_some(Message::CloseMenu));
        let Some((row, child)) = &menu.child else {
            return root;
        };
        let items = self::entries(app, child);
        let row_top = sdk::span(&entries, *row).0;
        let (sub_at, sub_size) =
            sdk::cascade((at, size), row_top, sdk::height(&items), sdk::width(&items, SUB_WIDTH), window);
        let sub_rows = rows(&paint, &items, child.highlight, live, Message::SubMenuHover);
        iced::widget::stack![root, sdk::sublayer(&paint, sub_rows, sub_at, sub_size, progress)].into()
    })
    .into()
}

fn rows<'a>(
    paint: &glide_sdk::Paint, entries: &[Entry], highlight: Option<usize>, live: bool,
    hover: fn(usize, bool) -> Message,
) -> Vec<Element<'a, Message>> {
    sdk::rows(paint, entries, highlight)
        .into_iter()
        .zip(entries)
        .enumerate()
        .map(|(index, (row, entry))| {
            let message = entry.message().cloned().filter(|_| live);
            let walkable = live && entry.actionable();
            let steps = match entry {
                sdk::Item::Stepper { less, more, .. } => [less.clone(), more.clone()],
                _ => [None, None],
            };
            let swatches: Vec<Message> = match entry {
                sdk::Item::Swatches(swatches) => swatches.iter().map(|swatch| swatch.message.clone()).collect(),
                _ => Vec::new(),
            };
            let stay = move |picked: Option<Message>| {
                picked.filter(|_| live).map_or(hover(index, false), |picked| Message::MenuStay(Box::new(picked)))
            };
            row.map(move |event| match (event, &message) {
                (Event::Picked(_), Some(message)) => Message::Menu(Box::new(message.clone())),
                (Event::Stepped(_, more), _) => stay(steps[usize::from(more)].clone()),
                (Event::Swatched(_, at), _) => stay(swatches.get(at).cloned()),
                (Event::Dismiss, _) => Message::CloseMenu,
                _ => hover(index, walkable),
            })
        })
        .collect()
}

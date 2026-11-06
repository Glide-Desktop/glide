use glide_sdk::theme::{SP2, SP3, SP5, TextStyle};
use glide_sdk::widget::{self as ui, Cover, Size};
use glide_sdk::window::{self, Header, Side, side_corners};
use iced::widget::{Column, Row, Space, column, container, mouse_area, row, scrollable, text_input};
use iced::{Alignment, Element, Fill, Length, Padding};

use super::{SEARCH_ID, SIDEBAR, cover_colors, ink};
use crate::app::{App, Chip, Lib, Message, Place};
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::store::{self, Entry};

const COVER_MIN: f32 = 150.0;
const GRID_GAP: f32 = 20.0;

fn place_item(app: &App, glyph: Icon, key: Key, place: Place, count: usize, warn: bool) -> Element<'_, Message> {
    let on = app.place == place;
    let mut item = ui::SidebarItem::new(&app.paint, app.t(key))
        .glyph(glyph)
        .selected(on)
        .current(on)
        .on_press(Some(Message::Library(Lib::Place(place))));
    if warn && count > 0 {
        let mark = TextStyle { weight: 600, ..TextStyle::CAP };
        item = item.accessory(ui::text(count.to_string(), mark).color(app.paint.warn()));
    } else if count > 0 {
        item = item.count(count);
    }
    item.into()
}

fn sidebar(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let top = mouse_area(
        container(window::maximize_button(
            &paint,
            &app.window,
            app.t(Key::Maximize),
            Message::Window(window::Action::ToggleMaximize),
        ))
        .padding([0.0, SP3])
        .height(window::HEADER)
        .width(Fill)
        .align_y(Alignment::Center),
    )
    .on_press(Message::Window(window::Action::Drag))
    .on_double_click(Message::Window(window::Action::ToggleMaximize));
    let count = |test: &dyn Fn(&store::Memory) -> bool| {
        app.entries.iter().filter(|e| app.memories.get(&store::key(&e.path)).is_some_and(test)).count()
    };
    let reading = count(&|m| m.page > 0 && m.progress() < 0.98);
    let noted = count(&|m| m.notes > 0);
    let liked = count(&|m| m.favourite);
    let unsigned = count(&|m| m.unsigned);
    let mut nav = column![
        ui::section_title(&paint, app.t(Key::Library), false),
        place_item(app, Icon::Recent, Key::Recent, Place::Recent, 0, false),
        place_item(app, Icon::Book, Key::Reading, Place::Reading, reading, false),
        place_item(app, Icon::Pen, Key::WithNotes, Place::Notes, noted, false),
        place_item(app, Icon::Heart, Key::Favourites, Place::Favourites, liked, false),
        place_item(app, Icon::Sign, Key::AwaitingSignature, Place::Unsigned, unsigned, unsigned > 0),
        Space::new().height(SP3),
        ui::section_title(&paint, app.t(Key::Folders), false),
    ]
    .spacing(2);
    for folder in &app.prefs.folders {
        let name = folder
            .file_name()
            .map_or_else(|| folder.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
        let on = app.place == Place::Folder(folder.clone());
        nav = nav.push(
            ui::SidebarItem::new(&paint, name)
                .glyph(Icon::Folder)
                .selected(on)
                .current(on)
                .on_press(Some(Message::Library(Lib::Place(Place::Folder(folder.clone()))))),
        );
    }
    nav = nav.push(
        ui::SidebarItem::new(&paint, app.t(Key::WatchFolder))
            .glyph(Icon::Plus)
            .on_press(Some(Message::Library(Lib::Watch))),
    );
    let note = container(ui::text(app.t(Key::LibraryNote), TextStyle::CAP).color(paint.fg_2())).padding(Padding {
        top: 0.0,
        right: SP5 - 4.0,
        bottom: SP3 + 2.0,
        left: SP5 - 4.0,
    });
    let nav = container(ui::overlay_scrollable(&paint, nav.padding(Padding::from([0.0, SP2])))).height(Fill);
    ui::pane(paint.sidebar_bg(), side_corners(Side::Left, app.window.corner()), column![top, nav, note])
        .width(Length::Fixed(SIDEBAR))
        .into()
}

fn title_of(app: &App) -> String {
    match &app.place {
        Place::Recent => app.t(Key::Recent).into(),
        Place::Reading => app.t(Key::Reading).into(),
        Place::Notes => app.t(Key::WithNotes).into(),
        Place::Favourites => app.t(Key::Favourites).into(),
        Place::Unsigned => app.t(Key::AwaitingSignature).into(),
        Place::Folder(path) => path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
    }
}

fn header(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let input =
        text_input(app.t(Key::SearchLibrary), &app.query).id(SEARCH_ID).on_input(|q| Message::Library(Lib::Query(q)));
    let clear = (!app.query.is_empty()).then_some(Message::Library(Lib::ClearQuery));
    let search = container(ui::search_input(&paint, input, clear, app.t(Key::ClearSearch))).width(200);
    let open = ui::sized_button(
        &paint,
        app.t(Key::OpenFile),
        ui::Tone::Neutral,
        Size::Small,
        Some(Message::Library(Lib::OpenDialog)),
    );
    let bar = Header::new("")
        .labels("", app.t(Key::CloseWindow))
        .without_maximize()
        .corners(side_corners(Side::TopRight, app.window.corner()))
        .leading(
            container(ui::text(title_of(app), TextStyle::WIN_TITLE).color(ink(app))).padding(Padding::from([0.0, SP2])),
        )
        .trailing(search)
        .trailing(open);
    window::header(&paint, &app.window, bar, Message::Window, Message::Close, 0.0)
}

fn meta(app: &App, entry: &Entry) -> (String, Option<f32>) {
    let memory = app.memories.get(&store::key(&entry.path));
    match memory {
        Some(m) if m.unsigned => (app.t(Key::NeedsSignature).into(), None),
        Some(m) if m.pages > 0 && m.progress() >= 0.98 => (app.t(Key::Read).into(), Some(1.0)),
        Some(m) if m.pages > 0 => (
            i18n::fill(app.t(Key::PageOf), &[("n", &(m.page + 1).to_string()), ("total", &m.pages.to_string())]),
            Some(m.progress()),
        ),
        _ => (app.t(Key::NeverOpened).into(), None),
    }
}

fn when(app: &App, opened: u64) -> String {
    let now = crate::app::unix_now();
    if opened == 0 {
        return String::new();
    }
    let days = now.saturating_sub(opened) / 86_400;
    match days {
        0 => app.t(Key::Today).into(),
        1 => app.t(Key::Yesterday).into(),
        _ => i18n::date(app.lang, &crate::app::iso_date(opened), false),
    }
}

fn name_of(entry: &Entry) -> String {
    entry.path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

fn stem_of(entry: &Entry) -> String {
    entry.path.file_stem().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

fn continuing(app: &App) -> Option<Element<'_, Message>> {
    let cards = app.continuing();
    if cards.is_empty() || !matches!(app.place, Place::Recent) || !app.query.is_empty() {
        return None;
    }
    let paint = app.paint;
    let items = cards.into_iter().map(|entry| {
        let memory = app.memory(&entry.path);
        let (face, ink) = cover_colors(entry.format, &name_of(entry));
        let picture = app.covers.get(&entry.path).cloned().flatten();
        let (line, progress) = meta(app, entry);
        let status = memory.unsigned.then(|| ui::badge(&paint, app.t(Key::NeedsSignature), ui::Severity::Warn));
        let cover = Cover {
            picture,
            title: stem_of(entry),
            face,
            ink,
            badge: entry.format.badge(),
            name: name_of(entry),
            meta: line,
            progress: progress.filter(|_| !memory.unsigned),
        };
        container(ui::continue_card(
            &paint,
            cover,
            status,
            when(app, memory.opened),
            Message::OpenPath(entry.path.clone()),
        ))
        .width(Length::FillPortion(1))
        .into()
    });
    let grid = Row::with_children(items).spacing(SP3).width(Fill);
    Some(
        column![
            container(ui::text(app.t(Key::ContinueReading), TextStyle::HEAD)).padding(Padding {
                top: 20.0,
                right: 0.0,
                bottom: 12.0,
                left: 0.0
            }),
            grid
        ]
        .into(),
    )
}

fn chips(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let chips = [
        (Chip::All, Key::All),
        (Chip::Pdf, Key::Pdf),
        (Chip::Books, Key::Books),
        (Chip::Comics, Key::Comics),
        (Chip::Scans, Key::Scans),
    ];
    Row::with_children(
        chips.iter().map(|(chip, key)| {
            ui::chip(&paint, app.t(*key), app.chip == *chip, Some(Message::Library(Lib::Chip(*chip))))
        }),
    )
    .spacing(6)
    .into()
}

fn grid(app: &App, width: f32) -> Element<'_, Message> {
    let paint = app.paint;
    let shown = app.shown_entries();
    if shown.is_empty() {
        let (title, body) = if app.entries.is_empty() {
            (Key::EmptyLibrary, Key::EmptyLibraryBody)
        } else {
            (Key::NothingFound, Key::NothingFoundBody)
        };
        let action = app.entries.is_empty().then(|| {
            ui::sized_button(
                &paint,
                app.t(Key::OpenFile),
                ui::Tone::Accent,
                Size::Medium,
                Some(Message::Library(Lib::OpenDialog)),
            )
        });
        return container(ui::empty_state(&paint, Icon::Book, app.t(title), app.t(body), action)).height(360).into();
    }
    let columns = ((width + GRID_GAP) / (COVER_MIN + GRID_GAP)).floor().max(1.0) as usize;
    let side = ((width - GRID_GAP * (columns as f32 - 1.0)) / columns as f32).floor();
    let mut rows = Column::new().spacing(24);
    for chunk in shown.chunks(columns) {
        let cells = chunk.iter().map(|entry| {
            let (face, ink) = cover_colors(entry.format, &name_of(entry));
            let (line, progress) = meta(app, entry);
            let picture = app.covers.get(&entry.path).cloned().flatten();
            let cover = Cover {
                picture,
                title: stem_of(entry),
                face,
                ink,
                badge: entry.format.badge(),
                name: name_of(entry),
                meta: line,
                progress,
            };
            let card: Element<'_, Message> = ui::doc_cover(&paint, cover, side, Message::OpenPath(entry.path.clone()));
            let path = entry.path.clone();
            mouse_area(card).on_right_press(Message::Library(Lib::Favourite(path))).into()
        });
        let mut line = Row::with_children(cells).spacing(GRID_GAP);
        for _ in chunk.len()..columns {
            line = line.push(Space::new().width(side));
        }
        rows = rows.push(line);
    }
    rows.into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let corner = app.window.corner();
    let width = (app.window.size.width - SIDEBAR - 1.0 - 2.0 * 32.0).max(COVER_MIN);
    let mut content = column![].width(Fill);
    if let Some(cards) = continuing(app) {
        content = content.push(cards);
    }
    content = content.push(
        row![ui::text(app.t(Key::AllDocuments), TextStyle::HEAD).width(Fill), chips(app)]
            .align_y(Alignment::Center)
            .padding(Padding { top: 20.0, right: 0.0, bottom: 12.0, left: 0.0 }),
    );
    content = content.push(grid(app, width));
    let body =
        scrollable(container(content).padding(Padding { top: SP2, right: 32.0, bottom: SP5, left: 32.0 })).height(Fill);
    let shown = app.shown_entries().len();
    let count = i18n::plural(app.lang, shown, Key::DocumentsOne, Key::DocumentsFew, Key::DocumentsMany);
    let status = ui::status_bar(
        &paint,
        vec![ui::text(count, TextStyle::CAP).color(paint.fg_2()).into()],
        vec![ui::text(app.t(Key::DropHint), TextStyle::CAP).color(paint.fg_2()).into()],
        side_corners(Side::BottomRight, corner),
    );
    let main = ui::pane(paint.view_bg(), side_corners(Side::Right, corner), column![header(app), body, status]);
    ui::pane(paint.win_bg(), corner, row![sidebar(app), ui::vertical_separator(&paint), main]).into()
}

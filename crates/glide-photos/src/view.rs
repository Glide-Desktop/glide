use std::path::PathBuf;

use glide_sdk::theme::{R3, SP1, SP2, SP3, SP4, SP5, TextStyle};
use glide_sdk::widget::{self as ui, CoverCard, Size, Tile};
use glide_sdk::window::{self, Header, Side, side_corners};
use iced::widget::{Column, Row, Space, column, container, mouse_area, opaque, row, scrollable, stack, text_input};
use iced::{Alignment, Color, Element, Fill, Length, Padding};

use crate::app::{App, Backdrop, Message, Modal, Place, Scale, ViewerEvent, ZOOM_SIDES};
use crate::formats::{Failure, Family, Note};
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::library::{self, Filter as Chip, Smart};

pub const SIDEBAR: f32 = 208.0;
pub const HEADER: f32 = window::HEADER;
pub const STATUS: f32 = 28.0;
pub const GAP: f32 = 2.0;
pub const GROUP_HEAD: f32 = 46.0;
pub const FILMSTRIP: f32 = 72.0;
pub const STAGE_MARGIN: f32 = 24.0;
pub const GRID_ID: &str = "glide-photos-grid";
pub const SEARCH_ID: &str = "glide-photos-search";
pub const ALBUM_FIELD: &str = "glide-photos-album-name";

#[must_use]
pub fn group_height(items: usize, columns: usize, side: f32) -> f32 {
    let rows = items.div_ceil(columns.max(1));
    GROUP_HEAD + rows as f32 * side + rows.saturating_sub(1) as f32 * GAP + SP2
}

fn ink(app: &App) -> Color {
    if app.window.focused { app.paint.fg_1() } else { app.paint.fg_3() }
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let base: Element<'_, Message> = if app.editor.is_some() {
        crate::editor_view::view(app)
    } else if app.viewer.is_some() {
        crate::viewer_view::view(app)
    } else {
        browser(app)
    };
    let mut layers = vec![base];
    if !app.window.edge_to_edge() {
        layers.push(window::resize_handles(Message::Window));
    }
    if let Some(modal) = &app.modal {
        layers.push(modal_view(app, modal));
    }
    stack(layers).into()
}

fn browser(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let corner = app.window.corner();
    let body = column![header(app), content(app), status(app)];
    ui::pane(
        paint.win_bg(),
        corner,
        row![
            sidebar(app),
            ui::vertical_separator(&paint),
            ui::pane(paint.view_bg(), side_corners(Side::Right, corner), body)
        ],
    )
    .into()
}

fn side_item(
    app: &App, glyph: Icon, label: impl Into<String>, place: Place, count: Option<usize>,
) -> ui::SidebarItem<'_, Message> {
    let paint = app.paint;
    let on = app.place == place;
    let mut item =
        ui::SidebarItem::new(&paint, label).glyph(glyph).selected(on).current(on).on_press(Some(Message::Go(place)));
    if let Some(count) = count.filter(|&count| count > 0) {
        item = item.count(count);
    }
    item
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
        .height(HEADER)
        .width(Fill)
        .align_y(Alignment::Center),
    )
    .on_press(Message::Window(window::Action::Drag))
    .on_double_click(Message::Window(window::Action::ToggleMaximize));
    let mut nav = column![
        ui::section_title(&paint, app.t(Key::Photos), false),
        side_item(app, Icon::Grid, app.t(Key::Library), Place::Library, None),
        side_item(app, Icon::Heart, app.t(Key::Favourites), Place::Favourites, None),
        side_item(app, Icon::Albums, app.t(Key::Albums), Place::Albums, None),
        Space::new().height(SP3),
        ui::section_title(&paint, app.t(Key::Folders), false),
    ]
    .spacing(2);
    for root in app.prefs.roots() {
        let name =
            root.file_name().map_or_else(|| root.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
        let item: Element<'_, Message> = side_item(app, Icon::Folder, name, Place::Folder(root.clone()), None).into();
        nav = nav.push(mouse_area(item).on_right_press(Message::FolderMenu(root)));
    }
    nav = nav
        .push(ui::SidebarItem::new(&paint, app.t(Key::AddFolder)).glyph(Icon::Plus).on_press(Some(Message::AddFolder)));
    if !app.cards.is_empty() {
        nav = nav.push(Space::new().height(SP3)).push(ui::section_title(&paint, app.t(Key::Devices), false));
        for (card, _) in &app.cards {
            nav = nav.push(
                side_item(app, Icon::Card, card.label.clone(), Place::Card(card.dcim.clone()), None).accessory(
                    ui::icon_button(
                        &paint,
                        Icon::Eject,
                        app.t(Key::Eject),
                        Some(Message::Eject(card.mount.clone())),
                        false,
                    ),
                ),
            );
        }
    }
    let nav = container(ui::overlay_scrollable(&paint, nav.padding(Padding::from([0.0, SP2])))).height(Fill);
    let trash = container(side_item(app, Icon::Trash, app.t(Key::Trash), Place::Trash, Some(app.trash.len())))
        .padding(Padding::from([SP2, SP2]));
    ui::pane(paint.sidebar_bg(), side_corners(Side::Left, app.window.corner()), column![top, nav, trash])
        .width(Length::Fixed(SIDEBAR))
        .into()
}

fn search(app: &App, placeholder: Key) -> Element<'_, Message> {
    let input = text_input(app.t(placeholder), &app.query).id(SEARCH_ID).on_input(Message::Query);
    let clear = (!app.query.is_empty()).then_some(Message::ClearQuery);
    container(ui::search_input(&app.paint, input, clear, app.t(Key::ClearSearch))).width(220).into()
}

fn more_button(app: &App, message: Message) -> Element<'_, Message> {
    ui::sized_icon_button(
        &app.paint,
        glide_sdk::Icon::More,
        app.t(Key::MoreActions),
        Some(message),
        false,
        Size::Medium,
    )
}

fn header(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let mut bar = Header::new("")
        .labels("", app.t(Key::CloseWindow))
        .without_maximize()
        .corners(side_corners(Side::TopRight, app.window.corner()));
    if app.selecting {
        let n = app.selected.len();
        let title = app.t(Key::SelectedPhotos).replace("{n}", &n.to_string());
        let bytes: u64 =
            app.photos.iter().filter(|photo| app.selected.contains(&photo.path)).map(|photo| photo.bytes).sum();
        bar = bar
            .leading(
                row![
                    ui::text(title, TextStyle::WIN_TITLE).color(ink(app)),
                    ui::text(i18n::bytes(app.lang, bytes), TextStyle::CAP).color(paint.fg_2()),
                ]
                .spacing(SP2)
                .align_y(Alignment::Center),
            )
            .trailing(ui::sized_button(
                &paint,
                app.t(Key::SelectAll),
                Tone::Ghost,
                Size::Small,
                Some(Message::SelectAll),
            ))
            .trailing(ui::sized_button(&paint, app.t(Key::Done), Tone::Neutral, Size::Small, Some(Message::EndSelect)));
        return window::header(&paint, &app.window, bar, Message::Window, Message::Close, 0.0);
    }
    let title = match &app.place {
        Place::Library => app.t(Key::Library).to_string(),
        Place::Favourites => app.t(Key::Favourites).to_string(),
        Place::Albums => app.t(Key::Albums).to_string(),
        Place::Album(index) => app.album_name(*index).to_string(),
        Place::Smart(smart) => smart_name(*smart).text(app.lang).to_string(),
        Place::Folder(path) | Place::Card(path) => {
            path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        }
        Place::Trash => app.t(Key::Trash).to_string(),
    };
    bar = bar.leading(ui::text(title, TextStyle::WIN_TITLE).color(ink(app)));
    let dated = !matches!(app.place, Place::Albums | Place::Trash);
    if dated && !app.searching() {
        let segmented = ui::segmented(
            &paint,
            &[
                (Scale::Years, app.t(Key::Years)),
                (Scale::Months, app.t(Key::Months)),
                (Scale::All, app.t(Key::AllPhotos)),
            ],
            app.scale,
            Message::Scale,
        );
        bar = bar.center(container(segmented).center_x(Fill));
    }
    if app.place == Place::Albums {
        bar = bar.trailing(search(app, Key::SearchAlbums)).trailing(ui::sized_button(
            &paint,
            app.t(Key::NewAlbum),
            Tone::Accent,
            Size::Small,
            Some(Message::AskNewAlbum(Vec::new())),
        ));
    } else if app.place == Place::Trash {
        if !app.trash.is_empty() {
            bar = bar.trailing(ui::sized_button(
                &paint,
                app.t(Key::EmptyTrashAction),
                Tone::Danger,
                Size::Small,
                Some(Message::AskEmptyTrash),
            ));
        }
    } else {
        bar = bar.trailing(search(app, Key::SearchPhotos)).trailing(ui::sized_button(
            &paint,
            app.t(Key::Select),
            Tone::Neutral,
            Size::Small,
            Some(Message::StartSelect),
        ));
    }
    window::header(&paint, &app.window, bar, Message::Window, Message::Close, 0.0)
}

use glide_sdk::Tone;

fn status(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let lang = app.lang;
    let shown = app.shown();
    let left = if !app.scanned {
        app.t(Key::Indexing).to_string()
    } else if app.selecting {
        app.t(Key::SelectedOf)
            .replace("{n}", &i18n::number(app.selected.len()))
            .replace("{total}", &i18n::number(shown.len()))
    } else if app.place == Place::Albums {
        format!(
            "{} · {} {}",
            i18n::count(lang, app.prefs.albums.len(), [Key::AlbumsOne, Key::AlbumsFew, Key::AlbumsMany]),
            Smart::ALL.len(),
            app.t(Key::Smart)
        )
    } else if app.place == Place::Trash {
        i18n::photos(lang, app.trash.len())
    } else {
        let photos = format!(
            "{} {}",
            i18n::number(shown.len()),
            i18n::photos(lang, shown.len()).split_once(' ').map_or("", |(_, w)| w)
        );
        match app.scale {
            Scale::All => photos,
            Scale::Months => format!(
                "{} · {photos}",
                i18n::count(
                    lang,
                    library::periods(&app.photos, &shown, true).len(),
                    [Key::MonthsOne, Key::MonthsFew, Key::MonthsMany]
                )
            ),
            Scale::Years => format!(
                "{} · {photos}",
                i18n::count(
                    lang,
                    library::periods(&app.photos, &shown, false).len(),
                    [Key::YearsOne, Key::YearsFew, Key::YearsMany]
                )
            ),
        }
    };
    let mut right: Vec<Element<'_, Message>> = Vec::new();
    if app.selecting {
        right.push(ui::text(app.t(Key::SelectionHint), TextStyle::CAP).color(paint.fg_2()).into());
    } else if app.scale == Scale::All && !matches!(app.place, Place::Albums | Place::Trash) {
        right.push(ui::zoom_control(
            &paint,
            app.prefs.zoom,
            ZOOM_SIDES.len(),
            Message::Zoom,
            (app.t(Key::ZoomOut), app.t(Key::ZoomIn)),
        ));
    }
    ui::status_bar(
        &paint,
        vec![ui::text(left, TextStyle::CAP).color(paint.fg_2()).into()],
        right,
        side_corners(Side::BottomRight, app.window.corner()),
    )
}

#[must_use]
pub const fn smart_name(smart: Smart) -> Key {
    match smart {
        Smart::Recent => Key::Recent,
        Smart::Raw => Key::Raw,
        Smart::Animated => Key::Animated,
        Smart::Vector => Key::Vector,
        Smart::Screenshots => Key::Screenshots,
        Smart::Hdr => Key::Hdr,
        Smart::Panoramas => Key::Panoramas,
        Smart::Paged => Key::Paged,
    }
}

const fn smart_hint(smart: Smart) -> Key {
    match smart {
        Smart::Recent => Key::RecentHint,
        Smart::Raw => Key::RawHint,
        Smart::Animated => Key::AnimatedHint,
        Smart::Vector => Key::VectorHint,
        Smart::Screenshots => Key::ScreenshotsHint,
        Smart::Hdr => Key::HdrHint,
        Smart::Panoramas => Key::PanoramasHint,
        Smart::Paged => Key::PagedHint,
    }
}

const fn smart_icon(smart: Smart) -> Icon {
    match smart {
        Smart::Recent => Icon::Clock,
        Smart::Raw => Icon::Aperture,
        Smart::Animated => Icon::Play,
        Smart::Vector => Icon::Pen,
        Smart::Screenshots => Icon::Screen,
        Smart::Hdr => Icon::Sun,
        Smart::Panoramas => Icon::Wide,
        Smart::Paged => Icon::Pages,
    }
}

fn smart_tint(smart: Smart) -> Color {
    let ramp = match smart {
        Smart::Recent => glide_sdk::theme::Accent::Blue,
        Smart::Raw => glide_sdk::theme::Accent::Graphite,
        Smart::Animated => glide_sdk::theme::Accent::Orange,
        Smart::Vector => glide_sdk::theme::Accent::Purple,
        Smart::Screenshots => glide_sdk::theme::Accent::Teal,
        Smart::Hdr => glide_sdk::theme::Accent::Amber,
        Smart::Panoramas => glide_sdk::theme::Accent::Green,
        Smart::Paged => glide_sdk::theme::Accent::Red,
    };
    glide_sdk::theme::to_iced(ramp.ramp()[3])
}

fn content(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    if app.place == Place::Albums {
        return albums(app);
    }
    if app.place == Place::Trash {
        return trash(app);
    }
    let shown = app.shown();
    let mut top: Vec<Element<'_, Message>> = Vec::new();
    if let Some((card, fresh)) = app
        .cards
        .iter()
        .find(|(card, fresh)| *fresh > 0 && !app.prefs.dismissed_cards.contains(&card.mount))
        .filter(|_| app.place == Place::Library && !app.selecting)
    {
        let text = app.t(Key::CardNew).replace("{n}", &fresh.to_string()).replace("{card}", &card.label);
        let copy = (!app.copying).then(|| Message::CopyCard(card.clone()));
        top.push(
            container(ui::noted_banner(
                &paint,
                ui::Severity::Accent,
                Icon::Card,
                "",
                &text,
                vec![
                    ui::sized_button(&paint, app.t(Key::CopyToLibrary), Tone::Accent, Size::Small, copy),
                    ui::icon_button(
                        &paint,
                        Icon::Close,
                        app.t(Key::Hide),
                        Some(Message::DismissCard(card.mount.clone())),
                        false,
                    ),
                ],
            ))
            .padding(Padding::from([SP4, SP5]).bottom(0.0))
            .into(),
        );
    }
    if app.searching() {
        top.push(search_head(app, &shown));
    }
    let body: Element<'_, Message> = if !app.scanned && app.photos.is_empty() {
        container(ui::busy_label(&paint, app.t(Key::Indexing))).center(Fill).into()
    } else if app.photos.is_empty() && app.place == Place::Library {
        ui::empty_state(
            &paint,
            Icon::Image,
            app.t(Key::EmptyTitle),
            app.t(Key::EmptyBody),
            Some(
                row![
                    ui::primary(&paint, app.t(Key::UsePictures), Message::UsePictures),
                    ui::secondary(&paint, app.t(Key::OtherFolder), Message::AddFolder),
                ]
                .spacing(SP2)
                .into(),
            ),
        )
    } else if shown.is_empty() && app.searching() {
        let body = app.t(Key::NothingFoundBody).replace("{q}", app.query.trim());
        ui::empty_state(
            &paint,
            Icon::Search,
            app.t(Key::NothingFound),
            body,
            Some(ui::tonal(&paint, app.t(Key::SearchEverywhere), Message::Go(Place::Library))),
        )
    } else if shown.is_empty() {
        let (title, body) = if matches!(app.place, Place::Album(_)) {
            (Key::EmptyAlbum, Key::EmptyAlbumBody)
        } else {
            (Key::NothingFound, Key::EmptyAlbumBody)
        };
        ui::empty_state(&paint, Icon::Albums, app.t(title), app.t(body), None)
    } else if app.searching() {
        grid(app, &shown, false)
    } else {
        match app.scale {
            Scale::All => grid(app, &shown, true),
            Scale::Months => periods(app, &shown, true),
            Scale::Years => periods(app, &shown, false),
        }
    };
    let mut column = Column::with_children(top);
    column = column.push(container(body).height(Fill));
    column.height(Fill).into()
}

fn search_head<'a>(app: &'a App, shown: &[usize]) -> Element<'a, Message> {
    let paint = app.paint;
    let chips = [
        (Chip::Everything, Key::Everything),
        (Chip::Favourites, Key::Favourites),
        (Chip::Kind(Smart::Raw), Key::Raw),
        (Chip::Kind(Smart::Animated), Key::Animated),
        (Chip::Kind(Smart::Vector), Key::Vector),
        (Chip::Kind(Smart::Screenshots), Key::Screenshots),
        (Chip::ThisYear, Key::ThisYear),
    ];
    let chips = Row::with_children(
        chips
            .into_iter()
            .map(|(chip, key)| ui::chip(&paint, key.text(app.lang), app.chip == chip, Some(Message::Chip(chip)))),
    )
    .spacing(6)
    .wrap();
    let mut col = column![chips].spacing(SP2).padding(Padding::from([SP3, SP5]).bottom(0.0));
    let query = app.query.trim().to_lowercase();
    if !query.is_empty() && !shown.is_empty() {
        let mut hints: Vec<Element<'_, Message>> = Vec::new();
        for (index, album) in app.prefs.albums.iter().enumerate() {
            if album.name.to_lowercase().contains(&query) {
                hints.push(suggestion(
                    app,
                    Icon::Albums,
                    &album.name,
                    app.t(Key::Album),
                    album.photos.len(),
                    Message::Go(Place::Album(index)),
                ));
            }
        }
        for root in app.prefs.roots() {
            let name = root.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            if name.to_lowercase().contains(&query) {
                let count = app.photos.iter().filter(|photo| photo.path.starts_with(&root)).count();
                hints.push(suggestion(
                    app,
                    Icon::Folder,
                    &name,
                    app.t(Key::Place),
                    count,
                    Message::Go(Place::Folder(root)),
                ));
            }
        }
        col = col.extend(hints.into_iter().take(3));
        col = col.push(
            row![
                ui::text(app.t(Key::Photos), TextStyle::HEAD).color(paint.fg_1()),
                ui::text(app.t(Key::Matches).replace("{n}", &i18n::number(shown.len())), TextStyle::CAP)
                    .color(paint.fg_2()),
            ]
            .spacing(SP2)
            .align_y(Alignment::Center),
        );
    }
    col.into()
}

fn suggestion<'a>(
    app: &'a App, glyph: Icon, name: &str, kind: &str, count: usize, message: Message,
) -> Element<'a, Message> {
    let paint = app.paint;
    ui::ListRow::new(paint, format!("{name} · {kind}"))
        .prefix(ui::icon(glyph, 16.0, paint.fg_2()))
        .suffix(ui::text(i18n::number(count), TextStyle { size_px: 12.0, ..TextStyle::NUM }).color(paint.fg_2()))
        .on_press(message)
        .into()
}

fn tile(app: &App, index: usize, side: f32) -> Element<'_, Message> {
    let photo = &app.photos[index];
    let badge = if photo.format.family == Family::Raw {
        Some("RAW".to_string())
    } else if photo.animated() {
        Some(photo.format.name.to_string())
    } else if photo.paged() {
        Some(app.t(Key::Pages).replace("{n}", &photo.traits.pages.to_string()))
    } else if photo.format.family == Family::Vector {
        Some("SVG".into())
    } else if photo.format.family == Family::Hdr {
        Some("HDR".into())
    } else {
        None
    };
    ui::photo_tile(
        &app.paint,
        Tile {
            picture: app.thumb(&photo.path),
            side,
            badge,
            favourite: app.prefs.is_favourite(&photo.path),
            picked: app.selecting.then(|| app.selected.contains(&photo.path)),
            on_press: Some(Message::Tile(index)),
            on_right: Some(Message::TileMenu(index)),
            label: &photo.name,
        },
    )
}

fn grid<'a>(app: &'a App, shown: &[usize], grouped: bool) -> Element<'a, Message> {
    let paint = app.paint;
    let (columns, side) = app.grid();
    let groups: Vec<library::Group> = if grouped {
        library::by_day(&app.photos, shown)
    } else {
        vec![library::Group { date: library::Date::default(), items: shown.to_vec() }]
    };
    let low = app.scroll - app.viewport;
    let high = app.scroll + app.viewport * 2.0;
    let mut y = 0.0;
    let mut column = Column::new();
    let mut skipped = 0.0;
    for (number, group) in groups.iter().enumerate() {
        let height = if grouped {
            group_height(group.items.len(), columns, side)
        } else {
            group_height(group.items.len(), columns, side) - GROUP_HEAD
        };
        if y + height < low || y > high {
            skipped += height;
            y += height;
            continue;
        }
        if skipped > 0.0 {
            column = column.push(Space::new().height(skipped));
            skipped = 0.0;
        }
        let mut section = Column::new().spacing(0);
        if grouped {
            let all = group.items.iter().all(|&i| app.selected.contains(&app.photos[i].path));
            let mut head = row![
                ui::text(i18n::date(app.lang, group.date, app.today), TextStyle::T3).color(paint.fg_1()),
                ui::text(folder_line(app, &group.items), TextStyle::CALL).color(paint.fg_2()),
                Space::new().width(Fill),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            if app.selecting {
                head = head.push(ui::link(
                    &paint,
                    app.t(if all { Key::DeselectDay } else { Key::SelectDay }),
                    Some(Message::SelectDay(number)),
                ));
            }
            section = section.push(container(head).height(GROUP_HEAD).align_y(Alignment::Center).padding([0.0, 2.0]));
        }
        let mut rows = Column::new().spacing(GAP);
        for chunk in group.items.chunks(columns) {
            rows = rows.push(Row::with_children(chunk.iter().map(|&index| tile(app, index, side))).spacing(GAP));
        }
        section = section.push(rows).push(Space::new().height(SP2));
        column = column.push(section);
        y += height;
    }
    if skipped > 0.0 {
        column = column.push(Space::new().height(skipped));
    }
    let list = scrollable(container(column.width(Fill)).padding(Padding::from([0.0, SP5])))
        .id(GRID_ID)
        .on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset().y, viewport.bounds().height))
        .height(Fill);
    list.into()
}

fn folder_line(app: &App, items: &[usize]) -> String {
    let mut names: Vec<String> = Vec::new();
    for &index in items {
        if let Some(name) = app.photos[index].folder.file_name().map(|name| name.to_string_lossy().into_owned())
            && !names.contains(&name)
        {
            names.push(name);
        }
        if names.len() == 2 {
            break;
        }
    }
    names.join(" · ")
}

fn periods<'a>(app: &'a App, shown: &[usize], monthly: bool) -> Element<'a, Message> {
    let width = app.content_width();
    let (columns, ratio) = if monthly { (3usize, 10.0 / 16.0) } else { (2, 8.4 / 16.0) };
    let gap = 20.0;
    let card_w = ((width - gap * (columns as f32 - 1.0)) / columns as f32).floor();
    let card_h = (card_w * ratio).floor();
    let cards: Vec<Element<'_, Message>> = library::periods(&app.photos, shown, monthly)
        .into_iter()
        .map(|period| {
            let title = match period.month {
                Some(month) => format!("{} {}", i18n::month(app.lang, month), period.year),
                None => period.year.to_string(),
            };
            let count = i18n::photos(app.lang, period.count);
            let subtitle = if monthly && !period.folders.is_empty() {
                format!("{} · {count}", period.folders.join(", "))
            } else {
                count
            };
            ui::cover_card(
                &app.paint,
                CoverCard {
                    pictures: vec![app.thumb(&app.photos[period.cover].path)],
                    width: card_w,
                    height: card_h,
                    title,
                    subtitle,
                    overlay: !monthly,
                    on_press: Some(Message::PeriodPressed(period.year, period.month)),
                    on_right: None,
                },
            )
        })
        .collect();
    let mut rows = Column::new().spacing(if monthly { 24 } else { 20 });
    let mut cards = cards.into_iter().peekable();
    while cards.peek().is_some() {
        rows = rows.push(Row::with_children(cards.by_ref().take(columns)).spacing(gap));
    }
    scrollable(container(rows).padding(Padding::from([SP4, SP5]))).height(Fill).into()
}

fn albums(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let width = app.content_width() - 2.0 * SP1;
    let columns = 5usize;
    let side = ((width - 20.0 * (columns as f32 - 1.0)) / columns as f32).floor();
    let query = app.query.trim().to_lowercase();
    let mine: Vec<Element<'_, Message>> = app
        .prefs
        .albums
        .iter()
        .enumerate()
        .filter(|(_, album)| query.is_empty() || album.name.to_lowercase().contains(&query))
        .map(|(index, album)| {
            let present: Vec<&PathBuf> =
                album.photos.iter().filter(|path| app.photos.iter().any(|p| &p.path == *path)).collect();
            let pictures: Vec<_> = if present.len() >= 4 {
                present.iter().take(4).map(|path| app.thumb(path)).collect()
            } else {
                vec![present.first().and_then(|path| app.thumb(path))]
            };
            let element: Element<'_, Message> = ui::cover_card(
                &paint,
                CoverCard {
                    pictures,
                    width: side,
                    height: side,
                    title: album.name.clone(),
                    subtitle: i18n::photos(app.lang, present.len()),
                    overlay: false,
                    on_press: Some(Message::Go(Place::Album(index))),
                    on_right: None,
                },
            );
            mouse_area(element).on_right_press(Message::DeleteAlbumAsk(index)).into()
        })
        .collect();
    let mut mine_rows = Column::new().spacing(20);
    if mine.is_empty() {
        mine_rows =
            mine_rows.push(ui::empty_note(&paint, Icon::Albums, app.t(Key::NoAlbums), app.t(Key::NoAlbumsBody)));
    }
    let mut mine = mine.into_iter().peekable();
    while mine.peek().is_some() {
        mine_rows = mine_rows.push(Row::with_children(mine.by_ref().take(columns)).spacing(20));
    }
    let now = app.now;
    let smart: Vec<Element<'_, Message>> = Smart::ALL
        .into_iter()
        .map(|smart| {
            let n = app.photos.iter().filter(|photo| smart.holds(photo, now)).count();
            let line = row![
                ui::icon_tile(smart_icon(smart), smart_tint(smart), 30.0),
                column![
                    ui::text(smart_name(smart).text(app.lang), TextStyle::HEAD).color(paint.fg_1()),
                    ui::text(smart_hint(smart).text(app.lang), TextStyle::CAP).color(paint.fg_2()),
                ]
                .width(Fill),
                ui::text(i18n::number(n), TextStyle { size_px: 12.0, ..TextStyle::NUM }).color(paint.fg_2()),
            ]
            .spacing(SP3)
            .align_y(Alignment::Center);
            container(ui::action_card(&paint, container(line).padding([8.0, 12.0]), Message::Go(Place::Smart(smart))))
                .width(Fill)
                .into()
        })
        .collect();
    let mut smart_rows = Column::new().spacing(10);
    let mut smart = smart.into_iter().peekable();
    while smart.peek().is_some() {
        smart_rows = smart_rows.push(Row::with_children(smart.by_ref().take(4)).spacing(10));
    }
    let page = column![
        ui::text(app.t(Key::MyAlbums), TextStyle::T2).color(paint.fg_1()),
        mine_rows,
        Space::new().height(SP3),
        row![
            ui::text(app.t(Key::SmartAlbums), TextStyle::T2).color(paint.fg_1()),
            ui::text(app.t(Key::UpdateThemselves), TextStyle::CALL).color(paint.fg_2()),
        ]
        .spacing(10)
        .align_y(Alignment::End),
        smart_rows,
    ]
    .spacing(SP3);
    scrollable(container(page).padding(Padding::from([20.0, SP5 + SP1]))).height(Fill).into()
}

fn trash(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    if app.trash.is_empty() {
        return ui::empty_state(&paint, Icon::Trash, app.t(Key::EmptyTrash), app.t(Key::EmptyTrashBody), None);
    }
    let (columns, side) = app.grid();
    let mut rows = Column::new().spacing(GAP);
    for chunk in app.trash.chunks(columns) {
        rows = rows.push(
            Row::with_children(chunk.iter().map(|item| {
                ui::photo_tile(
                    &paint,
                    Tile {
                        picture: app.thumb(&item.file),
                        side,
                        badge: None,
                        favourite: false,
                        picked: None,
                        on_press: Some(Message::TrashMenu(item.clone())),
                        on_right: Some(Message::TrashMenu(item.clone())),
                        label: "",
                    },
                )
            }))
            .spacing(GAP),
        );
    }
    let note = ui::text(app.t(Key::EmptyTrashBody), TextStyle::CALL).color(paint.fg_2());
    scrollable(column![note, rows].spacing(SP3).padding(Padding::from([SP4, SP5]))).height(Fill).into()
}

fn modal_view<'a>(app: &'a App, modal: &'a Modal) -> Element<'a, Message> {
    let paint = app.paint;
    let Modal::NewAlbum { name, .. } = modal;
    let input = text_input(app.t(Key::AlbumName), name)
        .id(ALBUM_FIELD)
        .on_input(Message::AlbumName)
        .on_submit(Message::CreateAlbum);
    let create = (!name.trim().is_empty()).then_some(Message::CreateAlbum);
    let card = column![
        ui::text(app.t(Key::NewAlbum), TextStyle::T3).color(paint.fg_1()),
        ui::field(&paint, input),
        row![
            Space::new().width(Fill),
            ui::secondary(&paint, app.t(Key::Cancel), Message::CancelModal),
            ui::button(&paint, app.t(Key::Create), Tone::Accent, create),
        ]
        .spacing(SP2),
    ]
    .spacing(SP4)
    .width(380);
    let card = container(card).padding(22).style(move |_| glide_sdk::glide_iced::style::popover(&paint));
    let card = glide_sdk::glide_iced::widgets::surface(card, &paint, glide_sdk::theme::Level::El3, R3);
    let scrim = mouse_area(
        container(Space::new())
            .width(Fill)
            .height(Fill)
            .style(|_| glide_sdk::glide_iced::style::rounded(Color { a: 0.42, ..ui::STAGE }, 0.0)),
    )
    .on_press(Message::CancelModal);
    stack![scrim, container(opaque(card)).center_x(Fill).padding(Padding::default().top(92.0))].into()
}

pub fn failure_text(app: &App, failure: &Failure) -> String {
    match failure {
        Failure::Unsupported(name) => app.t(Key::Unsupported).replace("{f}", name),
        Failure::Unreadable(error) => error.clone(),
    }
}

pub fn note_text(app: &App, note: Option<Note>) -> Option<&'static str> {
    match note? {
        Note::EmbeddedPreview => Some(app.t(Key::Embedded)),
        Note::ToneMapped => Some(app.t(Key::ToneMapped)),
        Note::Converted => Some(app.t(Key::Converted)),
        Note::Partial => None,
    }
}

#[must_use]
pub const fn backdrop_colour(backdrop: Backdrop) -> Option<Color> {
    match backdrop {
        Backdrop::Checks => None,
        Backdrop::White => Some(Color::WHITE),
        Backdrop::Dark => Some(Color::BLACK),
    }
}

pub fn more(app: &App) -> Element<'_, Message> {
    more_button(app, Message::Viewer(ViewerEvent::More))
}

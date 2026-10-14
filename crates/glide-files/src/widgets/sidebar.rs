use iced::widget::{Column, button, column, container, mouse_area, row, scrollable, space, text};

use iced::{Alignment, Element, Fill, Length, Padding};

use super::header::window_control;
use super::{HEADER_HEIGHT, SIDEBAR_WIDTH, ellipsize, icon};
use crate::app::{App, Device, Hover, MenuKind, Message, volume_icon};
use crate::fs::{Place, Volume};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::theme::{self, Tone};

pub const ITEM_HEIGHT: f32 = 28.0;
const EJECT: f32 = 20.0;

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let lang = app.lang;
    let mut sections = column![].spacing(14).padding(Padding::from([4, 10]).bottom(14));
    let devices = app.devices();
    for (title, rows) in [
        (Key::Places, app.places.iter().map(|place| item(app, place, false)).collect()),
        (Key::Bookmarks, app.bookmarks.iter().map(|place| item(app, place, true)).collect()),
        (
            Key::Devices,
            devices
                .into_iter()
                .map(|device| match device {
                    Device::Place(place) => item(app, place, false),
                    Device::Volume(volume) => volume_item(app, volume),
                })
                .chain(app.phones.iter().map(|phone| phone_item(app, phone)))
                .collect::<Vec<_>>(),
        ),
    ] {
        let bookmarks = title == Key::Bookmarks;
        let inviting = bookmarks && app.bookmark_drop_offered();
        if rows.is_empty() && !inviting {
            continue;
        }
        let active = bookmarks && app.bookmark_drop_active();
        let color = if active { t.acc_text() } else { t.fg_3() };
        let header = container(text(title.text(lang)).size(11).font(theme::SEMIBOLD).color(color))
            .padding(Padding::from([2, 8]))
            .width(Fill)
            .style(move |_| if active { theme::rounded(t.fill_2(), theme::R1) } else { container::Style::default() });
        let header: Element<'_, Message> = if bookmarks {
            mouse_area(header)
                .on_enter(Message::Hover(Hover::Bookmarks, true))
                .on_exit(Message::Hover(Hover::Bookmarks, false))
                .into()
        } else {
            header.into()
        };
        sections = sections.push(column![header, Column::with_children(rows)].spacing(4));
    }

    let top = mouse_area(
        container(window_control(t, Icon::Maximize, Message::WindowMaximize))
            .padding(Padding::from([0, 14]))
            .height(HEADER_HEIGHT)
            .width(Fill)
            .align_y(Alignment::Center),
    )
    .on_press(Message::WindowDrag)
    .on_double_click(Message::WindowMaximize);

    let mut panel =
        column![top, scrollable(sections).width(Fill).height(Fill).style(move |_, status| theme::scroller(t, status)),];
    if !app.jobs.is_empty() {
        panel = panel.push(super::divider(t)).push(super::jobs(app));
    }
    container(panel)
        .width(Length::Fixed(SIDEBAR_WIDTH))
        .height(Fill)
        .style(move |_| theme::rounded(t.sidebar_bg(), theme::window_corners(theme::Side::Left)))
        .into()
}

fn item<'a>(app: &'a App, place: &'a Place, bookmark: bool) -> Element<'a, Message> {
    let t = &app.theme;
    let selected = (place.path == app.current && app.search.is_none())
        || app.is_drop_target(&place.path)
        || app.is_bookmark_target(&place.path);
    let key = Hover::Place(place.path.clone());
    let hover = app.hover(&key);
    let row = button(
        container(
            row![
                icon(place.icon, 16.0, t.acc_text()),
                text(ellipsize(&place.name, 24)).size(13).wrapping(text::Wrapping::None),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .height(Fill)
        .align_y(Alignment::Center),
    )
    .on_press_maybe((!bookmark).then(|| Message::Navigate(place.path.clone())))
    .width(Fill)
    .height(Length::Fixed(ITEM_HEIGHT))
    .padding(Padding::from([0, 8]))
    .style(move |_, status| theme::sidebar_row(t, selected, hover, status));
    let area = mouse_area(row);
    let area = if bookmark { area.on_press(Message::BookmarkPressed(place.path.clone())) } else { area };
    area.on_right_press(Message::MenuTarget(MenuKind::Location(place.path.clone())))
        .on_enter(Message::Hover(key.clone(), true))
        .on_exit(Message::Hover(key, false))
        .into()
}

fn phone_item<'a>(app: &'a App, phone: &'a crate::fs::mtp::Phone) -> Element<'a, Message> {
    let t = &app.theme;
    let busy = app.busy.contains(&phone.id);
    let selected = app.search.is_none() && phone.mount.as_ref().is_some_and(|mount| app.current.starts_with(mount));
    let key = Hover::Place(phone.mount.clone().unwrap_or_else(|| std::path::PathBuf::from(&phone.id)));
    let hover = app.hover(&key);
    let fallback = if phone.camera { Key::Camera } else { Key::Phone };
    let name = if phone.name.is_empty() { fallback.text(app.lang).to_string() } else { phone.name.clone() };
    let glyph = if phone.camera { Icon::Camera } else { Icon::Phone };
    let tint = if phone.mount.is_some() { t.acc_text() } else { t.fg_3() };
    let mut content = row![
        icon(glyph, 16.0, if busy { t.fg_4() } else { tint }),
        text(ellipsize(&name, 20)).size(13).wrapping(text::Wrapping::None),
        space::horizontal(),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if phone.mount.is_some() && !busy {
        content = content.push(
            button(container(icon(Icon::Eject, 12.0, t.fg_2())).center(Fill))
                .on_press(Message::ClosePhone(phone.id.clone()))
                .width(Length::Fixed(EJECT))
                .height(Length::Fixed(EJECT))
                .padding(0)
                .style(move |_, status| theme::action(t, Tone::Ghost, status)),
        );
    }
    let row = button(container(content).height(Fill).align_y(Alignment::Center))
        .on_press_maybe((!busy).then(|| Message::OpenPhone(phone.id.clone())))
        .width(Fill)
        .height(Length::Fixed(ITEM_HEIGHT))
        .padding(Padding { left: 8.0, right: 4.0, ..Padding::ZERO })
        .style(move |_, status| theme::sidebar_row(t, selected, hover, status));
    mouse_area(row).on_enter(Message::Hover(key.clone(), true)).on_exit(Message::Hover(key, false)).into()
}

fn volume_item<'a>(app: &'a App, volume: &'a Volume) -> Element<'a, Message> {
    let t = &app.theme;
    let busy = app.busy.contains(&volume.object);
    let selected = app.search.is_none() && volume.mount.as_ref() == Some(&app.current);
    let key = Hover::Place(volume.mount.clone().unwrap_or_else(|| volume.device.clone()));
    let hover = app.hover(&key);
    let (glyph_color, label_color) = if busy {
        (t.fg_4(), t.fg_3())
    } else if volume.mount.is_some() {
        (t.acc_text(), t.fg_1())
    } else {
        (t.fg_3(), t.fg_2())
    };
    let mut content = row![
        icon(volume_icon(volume), 16.0, glyph_color),
        text(ellipsize(&volume.name(app.lang), 20)).size(13).color(label_color).wrapping(text::Wrapping::None),
        space::horizontal(),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if !busy && (volume.mount.is_some() || volume.can_remove()) {
        let message = if volume.can_remove() {
            Message::Eject(volume.object.clone())
        } else {
            Message::Unmount(volume.object.clone())
        };
        content = content.push(
            button(container(icon(Icon::Eject, 12.0, t.fg_2())).center(Fill))
                .on_press(message)
                .width(Length::Fixed(EJECT))
                .height(Length::Fixed(EJECT))
                .padding(0)
                .style(move |_, status| theme::action(t, Tone::Ghost, status)),
        );
    }
    let press = match &volume.mount {
        _ if busy => None,
        Some(path) => Some(Message::Navigate(path.clone())),
        None => Some(Message::Mount(volume.object.clone(), false)),
    };
    let row = button(container(content).height(Fill).align_y(Alignment::Center))
        .on_press_maybe(press)
        .width(Fill)
        .height(Length::Fixed(ITEM_HEIGHT))
        .padding(Padding { left: 8.0, right: 4.0, ..Padding::ZERO })
        .style(move |_, status| {
            let status = if busy { button::Status::Active } else { status };
            theme::sidebar_row(t, selected, hover, status)
        });
    mouse_area(row)
        .on_right_press(Message::MenuTarget(MenuKind::Volume(volume.object.clone())))
        .on_enter(Message::Hover(key.clone(), true))
        .on_exit(Message::Hover(key, false))
        .into()
}

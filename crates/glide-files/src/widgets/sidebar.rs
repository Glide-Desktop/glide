use iced::widget::{Column, button, column, container, mouse_area, row, scrollable, space, text};

use iced::{Alignment, Color, Element, Fill, Length, Padding};

use super::header::window_control;
use super::{HEADER_HEIGHT, SIDEBAR_WIDTH, fit, icon};
use crate::app::{App, Device, Hover, MenuKind, Message, volume_icon};
use crate::fs::{Place, Volume};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::theme::{self, Tone};

pub const ITEM_HEIGHT: f32 = theme::SMALL_CONTROL;
const EJECT: f32 = ITEM_HEIGHT;
const JOBS_HEIGHT: f32 = 320.0;
const GLYPH: f32 = 16.0;

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let lang = app.lang;
    let mut sections =
        column![].spacing(theme::SP4).padding(Padding::from([theme::SP1, theme::SP2]).bottom(theme::SP4));
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
        let header =
            container(text(title.text(lang).to_uppercase()).size(theme::MICRO).font(theme::SEMIBOLD).color(color))
                .padding(Padding::from([2.0, theme::SP2]))
                .width(Fill)
                .style(
                    move |_| if active { theme::rounded(t.fill_2(), theme::R1) } else { container::Style::default() },
                );
        let header: Element<'_, Message> = if bookmarks {
            mouse_area(header)
                .on_enter(Message::Hover(Hover::Bookmarks, true))
                .on_exit(Message::Hover(Hover::Bookmarks, false))
                .into()
        } else {
            header.into()
        };
        sections = sections.push(column![header, Column::with_children(rows)].spacing(theme::SP1));
    }

    let top = mouse_area(
        container(window_control(app, Icon::Maximize, Message::WindowMaximize))
            .padding(Padding::from([0.0, theme::SP3]))
            .height(HEADER_HEIGHT)
            .width(Fill)
            .align_y(Alignment::Center),
    )
    .on_press(Message::WindowDrag)
    .on_double_click(Message::WindowMaximize);

    let mut panel =
        column![top, scrollable(sections).width(Fill).height(Fill).style(move |_, status| theme::scroller(t, status)),];
    if !app.jobs.is_empty() {
        let jobs = scrollable(super::jobs(app)).style(move |_, status| theme::scroller(t, status));
        panel = panel.push(super::divider(t)).push(container(jobs).max_height(JOBS_HEIGHT));
    }
    let corners = theme::window_corners(theme::Side::Left, app.corner());
    container(panel)
        .width(Length::Fixed(SIDEBAR_WIDTH))
        .height(Fill)
        .style(move |_| theme::rounded(t.sidebar_bg(), corners))
        .into()
}

struct Line<'a> {
    glyph: Icon,
    tint: Color,
    label: String,
    dim: Option<Color>,
    eject: Option<Message>,
    press: Option<Message>,
    selected: bool,
    key: Hover,
    menu: Option<MenuKind>,
    drag: Option<Message>,
    app: &'a App,
}

impl<'a> Line<'a> {
    fn view(self) -> Element<'a, Message> {
        let t = &self.app.theme;
        let room = SIDEBAR_WIDTH - 4.0 * theme::SP2 - GLYPH - if self.eject.is_some() { EJECT } else { 0.0 };
        let mut label = text(fit(&self.label, room, theme::CALL)).size(theme::CALL).wrapping(text::Wrapping::None);
        if let Some(color) = self.dim {
            label = label.color(color);
        }
        let mut content = row![icon(self.glyph, GLYPH, self.tint), label, space::horizontal()]
            .spacing(theme::SP2)
            .align_y(Alignment::Center);
        if let Some(message) = self.eject {
            content = content.push(
                button(container(icon(Icon::Eject, 12.0, t.fg_2())).center(Fill))
                    .on_press(message)
                    .width(Length::Fixed(EJECT))
                    .height(Length::Fixed(EJECT))
                    .padding(0)
                    .style(move |_, status| theme::action(t, Tone::Ghost, status)),
            );
        }
        let (selected, hover) = (self.selected, self.app.hover(&self.key));
        let row = button(container(content).height(Fill).align_y(Alignment::Center))
            .on_press_maybe(self.press)
            .width(Fill)
            .height(Length::Fixed(ITEM_HEIGHT))
            .padding(Padding::ZERO.left(theme::SP2))
            .style(move |_, status| theme::sidebar_row(t, selected, hover, status));
        let mut area =
            mouse_area(row).on_enter(Message::Hover(self.key.clone(), true)).on_exit(Message::Hover(self.key, false));
        if let Some(kind) = self.menu {
            area = area.on_right_press(Message::MenuTarget(kind));
        }
        if let Some(drag) = self.drag {
            area = area.on_press(drag);
        }
        area.into()
    }
}

fn item<'a>(app: &'a App, place: &'a Place, bookmark: bool) -> Element<'a, Message> {
    Line {
        glyph: place.icon,
        tint: app.theme.acc_text(),
        label: place.name.clone(),
        dim: None,
        eject: None,
        press: (!bookmark).then(|| Message::Navigate(place.path.clone())),
        selected: (place.path == app.current && app.search.is_none())
            || app.is_drop_target(&place.path)
            || app.is_bookmark_target(&place.path),
        key: Hover::Place(place.path.clone()),
        menu: Some(MenuKind::Location(place.path.clone())),
        drag: bookmark.then(|| Message::BookmarkPressed(place.path.clone())),
        app,
    }
    .view()
}

fn phone_item<'a>(app: &'a App, phone: &'a crate::fs::mtp::Phone) -> Element<'a, Message> {
    let t = &app.theme;
    let busy = app.busy.contains(&phone.id);
    let fallback = if phone.camera { Key::Camera } else { Key::Phone };
    Line {
        glyph: if phone.camera { Icon::Camera } else { Icon::Phone },
        tint: match (busy, phone.mount.is_some()) {
            (true, _) => t.fg_4(),
            (false, true) => t.acc_text(),
            (false, false) => t.fg_3(),
        },
        label: if phone.name.is_empty() { fallback.text(app.lang).to_string() } else { phone.name.clone() },
        dim: None,
        eject: (phone.mount.is_some() && !busy).then(|| Message::ClosePhone(phone.id.clone())),
        press: (!busy).then(|| Message::OpenPhone(phone.id.clone())),
        selected: app.search.is_none() && phone.mount.as_ref().is_some_and(|mount| app.current.starts_with(mount)),
        key: Hover::Place(phone.mount.clone().unwrap_or_else(|| std::path::PathBuf::from(&phone.id))),
        menu: None,
        drag: None,
        app,
    }
    .view()
}

fn volume_item<'a>(app: &'a App, volume: &'a Volume) -> Element<'a, Message> {
    let t = &app.theme;
    let busy = app.busy.contains(&volume.object);
    let (tint, dim) = if busy {
        (t.fg_4(), Some(t.fg_3()))
    } else if volume.mount.is_some() {
        (t.acc_text(), None)
    } else {
        (t.fg_3(), Some(t.fg_2()))
    };
    let eject = if volume.can_remove() {
        Some(Message::Eject(volume.object.clone()))
    } else {
        volume.mount.is_some().then(|| Message::Unmount(volume.object.clone()))
    };
    Line {
        glyph: volume_icon(volume),
        tint,
        label: volume.name(app.lang),
        dim,
        eject: eject.filter(|_| !busy),
        press: match &volume.mount {
            _ if busy => None,
            Some(path) => Some(Message::Navigate(path.clone())),
            None => Some(Message::Mount(volume.object.clone(), false)),
        },
        selected: app.search.is_none() && volume.mount.as_ref() == Some(&app.current),
        key: Hover::Place(volume.mount.clone().unwrap_or_else(|| volume.device.clone())),
        menu: Some(MenuKind::Volume(volume.object.clone())),
        drag: None,
        app,
    }
    .view()
}

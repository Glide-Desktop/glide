use std::path::Path;

use glide_sdk::widget::pane;
use iced::widget::{Column, column, container, mouse_area};
use iced::{Alignment, Color, Element, Fill, Length, Padding};

use super::header::maximize_button;
use super::{HEADER_HEIGHT, SIDEBAR_WIDTH};
use crate::app::{App, Device, Hover, MenuKind, Message, volume_icon};
use crate::fs::{Place, Volume};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::theme;

pub const ITEM_HEIGHT: f32 = theme::CONTROL;
const EJECT: f32 = theme::SMALL_CONTROL;
const JOBS_HEIGHT: f32 = 320.0;
const GLYPH: f32 = 16.0;
const ROW_GAP: f32 = 2.0;

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let lang = app.lang;
    let mut sections =
        column![].spacing(theme::SP4).padding(Padding::from([theme::SP1, theme::SP2]).bottom(theme::SP4));
    let devices = app.devices();
    for (title, rows) in [
        (
            Key::Places,
            app.places
                .iter()
                .filter(|place| app.chooser.is_none() || !app.trash.contains(&place.path))
                .map(|place| item(app, place, false))
                .collect(),
        ),
        (Key::Bookmarks, app.shown_bookmarks().map(|place| item(app, place, true)).collect()),
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
        let header = glide_sdk::widget::section_title(t, title.text(lang), active);
        let header: Element<'_, Message> = if bookmarks {
            mouse_area(header)
                .on_enter(Message::Hover(Hover::Bookmarks, true))
                .on_exit(Message::Hover(Hover::Bookmarks, false))
                .into()
        } else {
            header
        };
        sections = sections.push(column![header, Column::with_children(rows).spacing(ROW_GAP)].spacing(theme::SP1));
    }

    let top = mouse_area(
        container(maximize_button(app))
            .padding(Padding::from([0.0, theme::SP3]))
            .height(HEADER_HEIGHT)
            .width(Fill)
            .align_y(Alignment::Center),
    )
    .on_press(Message::Window(glide_sdk::window::Action::Drag))
    .on_double_click(Message::Window(glide_sdk::window::Action::ToggleMaximize));

    let places = glide_sdk::widget::scrollable(t, sections).width(Fill).height(Fill);
    if app.chooser.is_some() {
        let corners = theme::window_corners(theme::Side::BottomLeft, app.corner());
        return pane(t.sidebar_bg(), corners, places)
            .padding(Padding::from([theme::SP2, 0.0]))
            .width(Length::Fixed(SIDEBAR_WIDTH))
            .into();
    }
    let mut panel = column![top, places];
    if !app.jobs.is_empty() {
        let jobs = glide_sdk::widget::scrollable(t, super::jobs(app));
        panel = panel.push(super::divider(t)).push(container(jobs).max_height(JOBS_HEIGHT));
    }
    let corners = theme::window_corners(theme::Side::Left, app.corner());
    pane(t.sidebar_bg(), corners, panel).width(Length::Fixed(SIDEBAR_WIDTH)).into()
}

struct Line<'a> {
    glyph: Icon,
    tint: Color,
    label: String,
    dim: Option<Color>,
    eject: Option<Message>,
    press: Option<Message>,
    selected: bool,
    active: bool,
    held: bool,
    key: Hover,
    menu: Option<MenuKind>,
    drag: Option<Message>,
    app: &'a App,
}

impl<'a> Line<'a> {
    fn view(self) -> Element<'a, Message> {
        let paint = super::paint(&self.app.theme);
        let active = self.active;
        let mut room = SIDEBAR_WIDTH - 2.0 * theme::SP2 - 2.0 * theme::SP3 - GLYPH - theme::SP2 - 2.0;
        room -= if self.eject.is_some() { EJECT + theme::SP2 } else { 0.0 };
        let style =
            glide_sdk::theme::TextStyle { weight: if active { 600 } else { 400 }, ..glide_sdk::theme::TextStyle::CALL };
        let mut item = glide_sdk::widget::SidebarItem::new(&paint, glide_sdk::widget::elide(&self.label, room, style))
            .tinted(self.glyph, self.tint)
            .ink(self.dim)
            .on_press(self.press)
            .selected(self.selected)
            .current(active)
            .hovered(self.app.hover(&self.key) > 0.5)
            .held(self.held);
        if let Some(message) = self.eject {
            let label = if matches!(message, Message::Unmount(_)) { Key::Unmount } else { Key::Eject };
            item = item.accessory(glide_sdk::widget::icon_button(
                &paint,
                Icon::Eject,
                label.text(self.app.lang),
                Some(message),
                false,
            ));
        }
        let row: Element<'a, Message> = item.into();
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

fn is_active(app: &App, path: Option<&Path>) -> bool {
    path.is_some_and(|path| app.sidebar_active() == Some(path))
}

fn item<'a>(app: &'a App, place: &'a Place, bookmark: bool) -> Element<'a, Message> {
    let active = is_active(app, Some(&place.path));
    Line {
        glyph: place.icon,
        tint: app.theme.fg_3(),
        label: place.name.clone(),
        dim: None,
        eject: None,
        press: (!bookmark).then(|| Message::Navigate(place.path.clone())),
        selected: active || app.is_drop_target(&place.path) || app.is_bookmark_target(&place.path),
        active,
        held: bookmark && app.is_bookmark_pressed(&place.path),
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
    let active = is_active(app, phone.mount.as_deref());
    Line {
        glyph: if phone.camera { Icon::Camera } else { Icon::Phone },
        tint: if busy { t.fg_4() } else { t.fg_3() },
        label: if phone.name.is_empty() { fallback.text(app.lang).to_string() } else { phone.name.clone() },
        dim: match (busy, phone.mount.is_some()) {
            (true, _) => Some(t.fg_4()),
            (false, true) => None,
            (false, false) => Some(t.fg_3()),
        },
        eject: (phone.mount.is_some() && !busy).then(|| Message::ClosePhone(phone.id.clone())),
        press: (!busy).then(|| Message::OpenPhone(phone.id.clone())),
        selected: active,
        active,
        held: false,
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
        (t.fg_4(), Some(t.fg_4()))
    } else if volume.mount.is_some() {
        (t.fg_3(), None)
    } else {
        (t.fg_3(), Some(t.fg_3()))
    };
    let active = is_active(app, volume.mount.as_deref());
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
        selected: active,
        active,
        held: false,
        key: Hover::Place(volume.mount.clone().unwrap_or_else(|| volume.device.clone())),
        menu: Some(MenuKind::Volume(volume.object.clone())),
        drag: None,
        app,
    }
    .view()
}

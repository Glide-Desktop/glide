pub mod chooser;
pub mod content;
pub mod dialogs;
pub mod header;
pub mod inspector;
pub mod menu;
pub mod operations;
pub mod sidebar;
pub mod status;

use std::time::Instant;

use glide_sdk::theme::TextStyle;
use glide_sdk::widget::{art, pane, separator, vertical_separator};
use iced::widget::{Stack, column, container, mouse_area, opaque, row};
use iced::{Alignment, Color, Element, Fill, Length, Padding};

use crate::app::{App, Hover, Message, Notice, NoticeAction};
use crate::fs::FileEntry;
use crate::i18n::Key;
use crate::icons::Icon;
use crate::native_icons::Art;
use crate::theme::{self, Side, ThemeState, window_corners};

pub const SIDEBAR_WIDTH: f32 = 208.0;
pub const HEADER_HEIGHT: f32 = glide_sdk::window::HEADER;
pub(crate) const STATUS_HEIGHT: f32 = 28.0;
pub const INSPECTOR_MIN_WINDOW: f32 = SIDEBAR_WIDTH + glide_sdk::widget::INSPECTOR_WIDTH + 440.0;
pub const TRASH_BANNER: f32 = glide_sdk::widget::INFO_BAR;
pub const CONTROL: f32 = theme::SMALL_CONTROL;

#[must_use]
pub fn window(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let base = chooser::base(app).unwrap_or_else(|| browser(app));
    let base = glide_sdk::widget::context_area(base, Message::ContextMenu).on_zoom(Message::Zoom);

    let now = Instant::now();
    let motion = &app.motion;
    let mut layers: Vec<Element<'_, Message>> = vec![base.into()];
    if !app.is_maximized() {
        layers.push(glide_sdk::window::resize_handles(Message::Window));
    }
    if let Some(rect) = app.band_rect() {
        layers.push(glide_sdk::widget::selection_band(t, rect));
    }
    if let Some(drag) = &app.drag {
        layers.push(drag_ghost(app, drag));
    }
    if let Some(notice) = app.notice.as_ref().or(motion.toast_ghost.as_ref()).filter(|_| !app.operations_shown()) {
        layers.push(toast(app, notice, motion.toast.progress(now), app.notice.is_some()));
    }
    let operations = motion.operations.progress(now);
    if operations > 0.0 {
        layers.push(operations::panel(app, operations));
    }
    if let Some(menu) = app.menu.as_ref().or(motion.menu_ghost.as_ref()) {
        layers.push(menu::overlay(app, menu, motion.menu.progress(now), app.menu.is_some()));
    }
    if let Some(modal) = app.modal.as_ref().or(motion.modal_ghost.as_ref()) {
        layers.push(dialogs::view(app, modal, motion.modal.progress(now)));
    }
    Stack::with_children(layers).into()
}

fn browser(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let banner = glide_sdk::anim::grow(app.in_trash(), column![trash_banner(app), divider(t)]);
    let inspector = glide_sdk::anim::slide_in_end(
        app.inspector_shown(),
        row![vertical_separator(t), inspector::view(app)].height(Fill),
    );
    let middle = row![content::view(app), inspector].height(Fill);
    let body = column![header::view(app), divider(t), banner, middle, divider(t), status::view(app)];
    let corner = app.corner();
    pane(
        t.win_bg(),
        corner,
        row![sidebar::view(app), vertical_separator(t), pane(t.view_bg(), window_corners(Side::Right, corner), body),],
    )
    .into()
}

fn trash_banner(app: &App) -> Element<'_, Message> {
    use glide_sdk::widget::{Size, elide_end, measure};
    let t = &app.theme;
    let lang = app.lang;
    let restorable = app.selected_entries().any(|e| app.trash.is_top_level(&e.path));
    let title = Key::Trash.text(lang);
    let buttons = [Key::Restore, Key::EmptyTrash];
    let taken: f32 = buttons.iter().map(|key| Size::Small.width(key.text(lang)) + theme::SP3).sum();
    let heading = measure(title, TextStyle { weight: 630, ..TextStyle::CALL });
    let aside = if app.inspector_shown() { glide_sdk::widget::INSPECTOR_WIDTH + 1.0 } else { 0.0 };
    let room = app.pane.width + aside - theme::SP4 - theme::SP3 - heading - theme::SP3 - taken;
    let blurb = elide_end(Key::TrashBanner.text(lang), room, TextStyle::CALL);
    let actions = buttons
        .into_iter()
        .map(|key| {
            let (message, tone) = match key {
                Key::Restore => (restorable.then_some(Message::Restore), glide_sdk::Tone::Neutral),
                _ => ((app.trash_count > 0).then_some(Message::EmptyTrash), glide_sdk::Tone::Danger),
            };
            glide_sdk::widget::compact_button(t, key.text(lang), tone, message)
        })
        .collect();
    glide_sdk::widget::info_bar(t, Some(glide_sdk::widget::Severity::Warn), title, &blurb, actions)
}

fn drag_ghost<'a>(app: &'a App, drag: &'a crate::app::Drag) -> Element<'a, Message> {
    let t = &app.theme;
    let lang = app.lang;
    let first = drag.paths.first().and_then(|path| app.entries.iter().find(|entry| &entry.path == path));
    let art = match first {
        Some(entry) => file_icon(app, entry, 40.0),
        None => icon(Icon::File, 32.0, t.fg_2()),
    };
    let count = drag.paths.len();
    let art: Element<'_, Message> = if count > 1 {
        Stack::with_children([
            container(art).padding(theme::SP2).into(),
            container(glide_sdk::widget::count_badge(t, u32::try_from(count).unwrap_or(u32::MAX)))
                .align_right(Length::Fixed(52.0))
                .into(),
        ])
        .into()
    } else {
        art
    };
    let bookmark = drag.action == crate::app::DropAction::Bookmark;
    let hint = (app.drop_target().is_some() || bookmark).then(|| {
        let (key, glyph) = match drag.action {
            crate::app::DropAction::Trash => (Key::MoveToTrash, Icon::Trash),
            crate::app::DropAction::Copy => (Key::DropCopy, Icon::Copy),
            crate::app::DropAction::Move => (Key::DropMove, Icon::Move),
            crate::app::DropAction::Bookmark => (Key::AddBookmark, Icon::Folder),
        };
        glide_sdk::widget::bubble(
            t,
            row![icon(glyph, 12.0, t.fg_1()), glide_sdk::widget::text(key.text(lang), TextStyle::CAP)]
                .spacing(theme::SP1)
                .align_y(Alignment::Center),
        )
    });
    let mut ghost = column![art].spacing(theme::SP1).align_x(Alignment::Start);
    if let Some(hint) = hint {
        ghost = ghost.push(hint);
    }
    let at = drag.at;
    let (x, y) = crate::drag_icon::OFFSET;
    container(ghost)
        .padding(Padding { top: at.y + y as f32, left: at.x + x as f32, ..Padding::ZERO })
        .width(Fill)
        .height(Fill)
        .into()
}

fn toast<'a>(app: &'a App, notice: &'a Notice, progress: f32, live: bool) -> Element<'a, Message> {
    const TOAST_WIDTH: f32 = 470.0;
    let paint = app.theme.faded(progress);
    let dismiss = Message::DismissNotice(notice.id);
    let action = notice.action.as_ref().map(|action| {
        let label = match action {
            NoticeAction::Undo => Key::Undo,
            NoticeAction::Repair(_) => Key::RepairAction,
        };
        (label.text(app.lang), if live { Message::NoticeChosen(action.clone()) } else { dismiss.clone() })
    });
    let close = Some((dismiss, Key::Close.text(app.lang)));
    let card = glide_sdk::overlay::toast_card(&paint, &notice.text, None, action, close, TOAST_WIDTH);
    let card = glide_sdk::overlay::toast_motion(card, progress, live);
    let card: Element<'a, Message> = if live {
        mouse_area(opaque(card))
            .on_enter(Message::Hover(Hover::Toast, true))
            .on_exit(Message::Hover(Hover::Toast, false))
            .into()
    } else {
        card
    };
    container(card)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::End)
        .padding(Padding { bottom: STATUS_HEIGHT + 16.0, left: SIDEBAR_WIDTH, ..Padding::ZERO })
        .into()
}

#[must_use]
pub fn icon<'a>(icon: Icon, size: f32, color: Color) -> Element<'a, Message> {
    glide_sdk::widget::Glyph::draw(icon, size, color)
}

impl glide_sdk::widget::Glyph for Icon {
    fn draw<'a, M: 'a>(self, side: f32, tint: Color) -> Element<'a, M> {
        glide_sdk::widget::svg_glyph(self.handle(), side, tint)
    }
}

#[must_use]
pub(crate) fn file_icon<'a>(app: &App, entry: &FileEntry, size: f32) -> Element<'a, Message> {
    let opacity = if app.is_cut(&entry.path) { 0.45 } else { 1.0 };
    if size >= 32.0
        && let Some(thumb) = app.thumbnail(entry)
    {
        return art(Art::from_path(thumb), size, opacity);
    }
    if let Some(found) = app.art(entry) {
        return art(found, size, opacity);
    }
    let t = &app.theme;
    let mut color = if entry.is_dir { t.acc_bg() } else { t.fg_2() };
    color.a *= opacity;
    icon(entry.category.icon(), size, color)
}

#[must_use]
pub fn divider(t: &ThemeState) -> Element<'_, Message> {
    separator(t)
}

#[must_use]
pub fn paint(t: &ThemeState) -> glide_sdk::Paint {
    *t
}

#[must_use]
pub fn check<'a>(t: &ThemeState, label: &'a str, on: bool, message: Message) -> Element<'a, Message> {
    let state = if on { glide_sdk::widget::Check::On } else { glide_sdk::widget::Check::Off };
    glide_sdk::widget::checkbox(t, label, state, Some(message))
}

pub use glide_sdk::widget::ellipsize;

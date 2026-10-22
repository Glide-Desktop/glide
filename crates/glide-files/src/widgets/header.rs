use iced::widget::{container, row, text_input};
use iced::{Element, Length};

use super::{HEADER_HEIGHT, fit_sentence, text_width};
use crate::app::{App, MenuKind, Message, PATH_ID, SEARCH_ID, ViewMode};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::theme::{self, SP2, SP3};

pub const SEARCH_WIDTH: f32 = 200.0;
pub const WINDOW_CONTROL: f32 = glide_theme::component::SMALL_CONTROL;
pub const GAP: f32 = SP2;
pub const INSET: f32 = SP3;
pub const SEMIBOLD_SPREAD: f32 = 0.94;

#[must_use]
pub fn search_title(label: &str, query: &str, room: f32) -> String {
    let frame = text_width(&format!("{label} «»"), theme::TITLE);
    let shown = fit_sentence(query, room - frame, theme::TITLE);
    if shown.is_empty() { label.to_string() } else { format!("{label} «{shown}»") }
}

#[must_use]
pub fn maximize_button(app: &App) -> Element<'_, Message> {
    let paint = super::paint(&app.theme);
    glide_sdk::window::maximize_button(&paint, &sdk_window(app), Key::Maximize.text(app.lang), Message::WindowMaximize)
}

#[must_use]
pub fn bar_background(app: &App) -> iced::Color {
    if app.is_idle() { app.theme.bar_bg_idle() } else { app.theme.bar_bg() }
}

pub(crate) fn sdk_window(app: &App) -> glide_sdk::window::Window {
    use glide_sdk::window::{Frame, Window};
    Window {
        size: app.window_size(),
        focused: !app.is_idle(),
        frame: if app.is_maximized() { Frame::Maximized } else { Frame::Floating },
    }
}

pub(crate) fn window_message(action: glide_sdk::window::Action) -> Message {
    use glide_sdk::window::Action;
    match action {
        Action::ToggleMaximize => Message::WindowMaximize,
        _ => Message::WindowDrag,
    }
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    use glide_sdk::widget::{Size, icon_segmented, sized_icon_button};
    let paint = super::paint(&app.theme);
    let lang = app.lang;
    let back = app.can_go_back().then_some(Message::Back);
    let forward = app.can_go_forward().then_some(Message::Forward);
    let nav = row![
        sized_icon_button(&paint, Icon::ChevronLeft, Key::Back.text(lang), back, false, Size::Medium),
        sized_icon_button(&paint, Icon::ChevronRight, Key::Forward.text(lang), forward, false, Size::Medium),
    ];
    let views = icon_segmented(
        &paint,
        &[
            (ViewMode::Grid, Icon::Grid, Key::ViewGrid.text(lang)),
            (ViewMode::List, Icon::List, Key::ViewList.text(lang)),
        ],
        app.view,
        Message::SetView,
    );
    let options_open = app.menu.as_ref().is_some_and(|menu| menu.kind == MenuKind::Options);
    let options = Some(Message::ToggleOptions);
    let mut header = glide_sdk::window::Header::new("")
        .labels("", Key::Close.text(lang))
        .height(HEADER_HEIGHT)
        .without_maximize()
        .corners(theme::window_corners(theme::Side::TopRight, app.corner()))
        .leading(nav)
        .trailing(views)
        .trailing(sized_icon_button(
            &paint,
            Icon::More,
            Key::MoreActions.text(lang),
            options,
            options_open,
            Size::Medium,
        ))
        .trailing(search_field(app));
    if let Some(field) = path_field(app) {
        header = header.center(field);
    }
    glide_sdk::window::header(&paint, &sdk_window(app), header, window_message, Message::Quit, 0.0)
}

#[must_use]
pub fn path_field(app: &App) -> Option<Element<'_, Message>> {
    let path = app.path_edit.as_ref()?;
    let input = text_input(Key::PathPlaceholder.text(app.lang), path)
        .id(PATH_ID)
        .on_input(Message::PathChanged)
        .on_submit(Message::PathSubmitted);
    Some(glide_sdk::widget::field(&super::paint(&app.theme), input))
}

pub fn search_field(app: &App) -> Element<'_, Message> {
    let query = app.search.as_ref().map_or("", |search| search.query.as_str());
    let input = text_input(Key::Search.text(app.lang), query).id(SEARCH_ID).on_input(Message::SearchChanged);
    let clear = (!query.is_empty()).then(|| Message::SearchChanged(String::new()));
    let field =
        glide_sdk::widget::search_input(&super::paint(&app.theme), input, clear, Key::ClearSearch.text(app.lang));
    container(field).width(Length::Fixed(SEARCH_WIDTH)).into()
}

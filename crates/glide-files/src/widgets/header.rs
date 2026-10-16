use std::time::Instant;

use iced::widget::{button, container, mouse_area, row, space, stack, text, text_input};
use iced::{Alignment, Element, Fill, Length, Padding};

use super::{CONTROL, HEADER_HEIGHT, icon, icon_button};
use crate::app::{App, MenuKind, Message, PATH_ID, SEARCH_ID, ViewMode};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::theme::{self, R2, SP2, SP3, Tone};

pub const SEARCH_WIDTH: f32 = 200.0;
pub const WINDOW_CONTROL: f32 = 26.0;
pub const GAP: f32 = SP2;
pub const INSET: f32 = SP3;
const IDLE_CONTROLS: f32 = 0.45;
const SEGMENT_WIDTH: f32 = 30.0;
const SEGMENT_GAP: f32 = 2.0;

#[must_use]
pub fn window_control(app: &App, glyph: Icon, message: Message) -> Element<'_, Message> {
    let t = if app.is_idle() { app.theme.faded(IDLE_CONTROLS) } else { app.theme };
    let bar = bar_background(app);
    let danger = glyph == Icon::Close;
    let tint = move |status| {
        let state = match status {
            iced::widget::svg::Status::Hovered => button::Status::Hovered,
            iced::widget::svg::Status::Idle => button::Status::Active,
        };
        let style = theme::window_control(&t, danger, state);
        let fill = match style.background {
            Some(iced::Background::Color(color)) => color,
            _ => iced::Color::TRANSPARENT,
        };
        theme::flatten(style.text_color, theme::flatten(fill, bar))
    };
    button(
        iced::widget::svg(glyph.handle()).width(Fill).height(Fill).style(move |_, status| theme::glyph(tint(status))),
    )
    .on_press(message)
    .width(Length::Fixed(WINDOW_CONTROL))
    .height(Length::Fixed(WINDOW_CONTROL))
    .padding(0)
    .style(move |_, status| theme::window_control(&t, danger, status))
    .into()
}

#[must_use]
pub fn bar_background(app: &App) -> iced::Color {
    if app.is_idle() { app.theme.bar_bg_idle() } else { app.theme.bar_bg() }
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let nav = row![
        icon_button(t, Icon::ChevronLeft, app.can_go_back().then_some(Message::Back), false),
        icon_button(t, Icon::ChevronRight, app.can_go_forward().then_some(Message::Forward), false),
    ];

    let heading: Element<'_, Message> = match &app.path_edit {
        Some(input) => text_input(Key::PathPlaceholder.text(app.lang), input)
            .id(PATH_ID)
            .on_input(Message::PathChanged)
            .on_submit(Message::PathSubmitted)
            .size(theme::CALL)
            .padding(Padding::from([5.0, SP3]))
            .width(Fill)
            .style(move |_, status| theme::field(t, status))
            .into(),
        None => text(app.place_title()).size(theme::TITLE).font(theme::SEMIBOLD).wrapping(text::Wrapping::None).into(),
    };

    let offset = app.motion.thumb.progress(Instant::now()) * (SEGMENT_WIDTH + SEGMENT_GAP);
    let thumb = row![
        space().width(offset),
        container(space()).width(SEGMENT_WIDTH).height(CONTROL - 4.0).style(move |_| theme::segment_thumb(t)),
    ]
    .width(2.0 * SEGMENT_WIDTH + SEGMENT_GAP)
    .height(CONTROL - 4.0);
    let views = container(stack![
        thumb,
        row![segment(app, ViewMode::Grid, Icon::Grid), segment(app, ViewMode::List, Icon::List),].spacing(SEGMENT_GAP),
    ])
    .padding(2)
    .style(move |_| theme::rounded(t.fill_1(), R2));

    let options_open = app.menu.as_ref().is_some_and(|menu| menu.kind == MenuKind::Options);
    let background = bar_background(app);
    let bar = container(
        row![
            nav,
            container(heading).width(Fill).clip(true),
            views,
            icon_button(t, Icon::More, Some(Message::ToggleOptions), options_open),
            search_field(app),
            window_control(app, Icon::Close, Message::Quit),
        ]
        .spacing(GAP)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([0.0, INSET]).left(SP2))
    .height(HEADER_HEIGHT)
    .width(Fill)
    .align_y(Alignment::Center)
    .style(move |_| theme::rounded(background, theme::window_corners(theme::Side::TopRight, app.corner())));

    mouse_area(bar).on_press(Message::WindowDrag).on_double_click(Message::WindowMaximize).into()
}

fn segment(app: &App, mode: ViewMode, glyph: Icon) -> Element<'_, Message> {
    let t = &app.theme;
    let active = app.view == mode;
    button(container(icon(glyph, 15.0, if active { t.fg_1() } else { t.fg_2() })).center_x(Fill).center_y(Fill))
        .on_press(Message::SetView(mode))
        .width(Length::Fixed(SEGMENT_WIDTH))
        .height(Length::Fixed(CONTROL - 4.0))
        .padding(0)
        .style(move |_, status| theme::segment(t, active, status))
        .into()
}

fn search_field(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let query = app.search.as_ref().map_or("", |search| search.query.as_str());
    let input = text_input(Key::Search.text(app.lang), query)
        .id(SEARCH_ID)
        .on_input(Message::SearchChanged)
        .size(theme::CALL)
        .padding(Padding { top: 5.0, bottom: 5.0, left: CONTROL, right: if query.is_empty() { SP2 } else { CONTROL } })
        .width(Fill)
        .style(move |_, status| theme::search_field(t, status));
    let mut overlay =
        row![container(icon(Icon::Search, 14.0, t.fg_3())).center(Length::Fixed(CONTROL)), space::horizontal()];
    if !query.is_empty() {
        overlay = overlay.push(
            button(container(icon(Icon::Close, 12.0, t.fg_2())).center(Fill))
                .on_press(Message::SearchChanged(String::new()))
                .width(Length::Fixed(CONTROL))
                .height(Length::Fixed(CONTROL))
                .padding(0)
                .style(move |_, status| theme::action(t, Tone::Ghost, status)),
        );
    }
    container(stack![input, overlay.height(CONTROL).align_y(Alignment::Center)])
        .width(Length::Fixed(SEARCH_WIDTH))
        .height(CONTROL)
        .into()
}

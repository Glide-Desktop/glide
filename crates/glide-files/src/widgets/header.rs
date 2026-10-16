use std::time::Instant;

use iced::widget::{button, container, mouse_area, row, space, stack, text, text_input};
use iced::{Alignment, Element, Fill, Length, Padding};

use super::{CONTROL, HEADER_HEIGHT, fit, fit_sentence, hint, icon, icon_button, text_width};
use crate::app::{App, MenuKind, Message, PATH_ID, SEARCH_ID, ViewMode};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::theme::{self, R2, SP2, SP3};

pub const SEARCH_WIDTH: f32 = 200.0;
pub const WINDOW_CONTROL: f32 = 26.0;
pub const GAP: f32 = SP2;
pub const INSET: f32 = SP3;
const IDLE_CONTROLS: f32 = 0.45;
const SEGMENT_WIDTH: f32 = 30.0;
const SEGMENT_GAP: f32 = 2.0;
const CHROME: f32 = SP2
    + 2.0 * CONTROL
    + 4.0
    + 2.0 * SEGMENT_WIDTH
    + SEGMENT_GAP
    + CONTROL
    + SEARCH_WIDTH
    + WINDOW_CONTROL
    + 5.0 * GAP
    + INSET;
pub const SEMIBOLD_SPREAD: f32 = 0.94;
const CLEAR: f32 = 24.0;
const CLEAR_INSET: f32 = (CONTROL - CLEAR) / 2.0;

#[must_use]
pub fn search_title(label: &str, query: &str, room: f32) -> String {
    let frame = text_width(&format!("{label} «»"), theme::TITLE);
    let shown = fit_sentence(query, room - frame, theme::TITLE);
    if shown.is_empty() { label.to_string() } else { format!("{label} «{shown}»") }
}

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
    let lang = app.lang;
    let back = icon_button(t, Icon::ChevronLeft, app.can_go_back().then_some(Message::Back), false);
    let forward = icon_button(t, Icon::ChevronRight, app.can_go_forward().then_some(Message::Forward), false);
    let nav = row![hint(t, back, Key::Back.text(lang)), hint(t, forward, Key::Forward.text(lang))];

    let heading: Element<'_, Message> = if let Some(input) = &app.path_edit {
        text_input(Key::PathPlaceholder.text(app.lang), input)
            .id(PATH_ID)
            .on_input(Message::PathChanged)
            .on_submit(Message::PathSubmitted)
            .size(theme::CALL)
            .padding(Padding::from([5.0, SP3]))
            .width(Fill)
            .style(move |_, status| theme::field(t, status))
            .into()
    } else {
        let room = (app.pane.width - CHROME) * SEMIBOLD_SPREAD;
        let query = app.search.as_ref().map(|search| search.query.trim()).filter(|query| !query.is_empty());
        let title = match query {
            Some(query) => search_title(Key::Search.text(lang), query, room),
            None => fit(&app.place_title(), room, theme::TITLE),
        };
        text(title)
            .size(theme::TITLE)
            .font(theme::SEMIBOLD)
            .color(if app.is_idle() { t.fg_3() } else { t.fg_1() })
            .wrapping(text::Wrapping::None)
            .into()
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
            hint(
                t,
                icon_button(t, Icon::More, Some(Message::ToggleOptions), options_open),
                Key::MoreActions.text(lang)
            ),
            search_field(app),
            hint(t, window_control(app, Icon::Close, Message::Quit), Key::Close.text(lang)),
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
    let label = match mode {
        ViewMode::Grid => Key::ViewGrid,
        ViewMode::List => Key::ViewList,
    };
    let control =
        button(container(icon(glyph, 15.0, if active { t.fg_1() } else { t.fg_2() })).center_x(Fill).center_y(Fill))
            .on_press(Message::SetView(mode))
            .width(Length::Fixed(SEGMENT_WIDTH))
            .height(Length::Fixed(CONTROL - 4.0))
            .padding(0)
            .style(move |_, status| theme::segment(t, active, status));
    hint(t, control, label.text(app.lang))
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
        let field = theme::flatten(t.fill_1(), bar_background(app));
        let (rest, lit) =
            (theme::flatten(t.fg_3(), field), theme::flatten(t.fg_1(), theme::flatten(t.fill_2(), field)));
        let glyph = iced::widget::svg(Icon::Close.handle()).width(Fill).height(Fill).style(move |_, status| {
            theme::glyph(if status == iced::widget::svg::Status::Hovered { lit } else { rest })
        });
        let clear = button(glyph)
            .on_press(Message::SearchChanged(String::new()))
            .width(Length::Fixed(CLEAR))
            .height(Length::Fixed(CLEAR))
            .padding(2)
            .style(move |_, status| theme::clear_button(t, status));
        overlay = overlay.push(hint(t, clear, Key::ClearSearch.text(app.lang))).push(space().width(CLEAR_INSET));
    }
    container(stack![input, overlay.height(CONTROL).align_y(Alignment::Center)])
        .width(Length::Fixed(SEARCH_WIDTH))
        .height(CONTROL)
        .into()
}

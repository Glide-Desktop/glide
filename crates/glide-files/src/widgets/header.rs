use std::time::Instant;

use iced::widget::{button, container, mouse_area, row, space, stack, text_input};
use iced::{Alignment, Element, Fill, Length, Padding};

use super::{CONTROL, HEADER_HEIGHT, Ring, fit_sentence, hint, icon, icon_button, ringed, text_width};
use crate::app::{App, Field, Hover, MenuKind, Message, PATH_ID, SEARCH_ID, ViewMode};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::theme::{self, R2, SP2, SP3};

pub const SEARCH_WIDTH: f32 = 200.0;
pub const WINDOW_CONTROL: f32 = glide_theme::component::SMALL_CONTROL;
pub const GAP: f32 = SP2;
pub const INSET: f32 = SP3;
const IDLE_CONTROLS: f32 = glide_theme::component::IDLE_CONTROLS;
const SEGMENT_WIDTH: f32 = 30.0;
const SEGMENT_GAP: f32 = 2.0;
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
    let danger = glyph == Icon::Close;
    let glyph =
        if danger { glide_iced::glyph::window_close() } else { glide_iced::glyph::window_control(app.is_maximized()) };
    button(glyph)
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

    let heading = path_field(app).unwrap_or_else(|| space().width(Fill).into());

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

#[must_use]
pub fn path_field(app: &App) -> Option<Element<'_, Message>> {
    let t = &app.theme;
    let path = app.path_edit.as_ref()?;
    let input = text_input(Key::PathPlaceholder.text(app.lang), path)
        .id(PATH_ID)
        .on_input(Message::PathChanged)
        .on_submit(Message::PathSubmitted)
        .size(theme::CALL)
        .padding(Padding::from([5.0, SP3]))
        .width(Fill)
        .style(move |_, status| theme::field(t, status));
    let ring = if app.focused_field() == Some(Field::Path) { Ring::Focus } else { Ring::Off };
    Some(ringed(t, input, ring))
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

pub fn search_field(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let query = app.search.as_ref().map_or("", |search| search.query.as_str());
    let input = text_input(Key::Search.text(app.lang), query)
        .id(SEARCH_ID)
        .on_input(Message::SearchChanged)
        .size(theme::CALL)
        .padding(Padding { top: 5.0, bottom: 5.0, left: CONTROL, right: if query.is_empty() { SP2 } else { CONTROL } })
        .width(Fill)
        .style(move |_, status| theme::search_field(t, status));
    let lead = if app.is_search_focused() { t.acc_text() } else { t.fg_3() };
    let mut overlay =
        row![container(icon(Icon::Search, 14.0, lead)).center(Length::Fixed(CONTROL)), space::horizontal()];
    if !query.is_empty() {
        let lit = app.hover(&Hover::ClearSearch) > 0.5;
        let glyph = icon(Icon::Close, CLEAR - 4.0, if lit { t.fg_1() } else { t.fg_3() });
        let clear = button(container(glyph).center(Fill))
            .on_press(Message::SearchChanged(String::new()))
            .width(Length::Fixed(CLEAR))
            .height(Length::Fixed(CLEAR))
            .padding(0)
            .style(move |_, status| theme::clear_button(t, status));
        let clear = mouse_area(clear)
            .on_enter(Message::Hover(Hover::ClearSearch, true))
            .on_exit(Message::Hover(Hover::ClearSearch, false));
        overlay = overlay.push(hint(t, clear, Key::ClearSearch.text(app.lang))).push(space().width(CLEAR_INSET));
    }
    let field = container(stack![input, overlay.height(CONTROL).align_y(Alignment::Center)])
        .width(Length::Fixed(SEARCH_WIDTH - 2.0 * theme::RING))
        .height(CONTROL);
    ringed(t, field, if app.is_search_focused() { Ring::Focus } else { Ring::Off })
}

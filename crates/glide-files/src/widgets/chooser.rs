use iced::widget::{Row, checkbox, column, container, mouse_area, pick_list, row, rule, text, text_input};
use iced::{Alignment, Element, Fill, Length, Padding};

use super::header::{GAP, INSET, bar_background, path_field, search_field};
use super::{HEADER_HEIGHT, Ring, divider, hint, icon_button, labelled, ringed};
use crate::app::{App, Message};
use crate::chooser::{Mode, Pick, State};
use crate::i18n::{Key, Lang};
use crate::icons::Icon;
use crate::theme::{self, Side, Tone};

pub const NAME_ID: &str = "chooser-name";
const PREVIEW_ICON: f32 = 128.0;
const PREVIEW_WIDTH: f32 = 216.0;
const NAME_ROW: f32 = 560.0;

#[must_use]
pub fn preview(app: &App) -> Option<Element<'_, Message>> {
    let chooser = app.chooser.as_ref()?;
    if chooser.picks_folders() {
        return None;
    }
    let mut selected = app.selected_entries();
    let entry = selected.next()?;
    if selected.next().is_some() || entry.is_dir {
        return None;
    }
    let t = &app.theme;
    let kind = format!("{} · {}", entry.category.name(app.lang), crate::fs::format_bytes(entry.size, app.lang));
    let modified = entry.modified.map(|time| crate::fs::format_time(time, std::time::SystemTime::now(), app.lang));
    let body = column![
        container(super::file_icon(app, entry, PREVIEW_ICON)).center_x(Fill),
        text(entry.name.clone()).size(13.5).font(theme::SEMIBOLD).color(t.fg_1()).wrapping(text::Wrapping::WordOrGlyph),
        column![text(kind).size(theme::CAP).color(t.fg_2())]
            .push(modified.map(|modified| text(modified).size(theme::CAP).color(t.fg_3())))
            .spacing(theme::SP1),
    ]
    .spacing(theme::SP2)
    .padding(theme::SP3)
    .width(Length::Fixed(PREVIEW_WIDTH));
    Some(body.into())
}

#[must_use]
pub fn accept_label(chooser: &State, lang: Lang) -> &str {
    let request = &chooser.request;
    request.accept_label.as_deref().filter(|label| !label.trim().is_empty()).unwrap_or_else(|| {
        match request.mode {
            Mode::Open if request.directory => Key::ChooserSelect,
            Mode::Open => Key::ChooserOpen,
            Mode::Save | Mode::SaveFiles => Key::ChooserSave,
        }
        .text(lang)
    })
}

fn header<'a>(app: &'a App, chooser: &'a State) -> Element<'a, Message> {
    let (t, lang) = (&app.theme, app.lang);
    let cancel = labelled(t, Key::Cancel.text(lang), Tone::Neutral, Some(Message::ChooserCancel));
    let accept = labelled(t, accept_label(chooser, lang), Tone::Primary, Some(Message::ChooserAccept));
    let title =
        text(app.title()).size(theme::TITLE).font(theme::SEMIBOLD).color(t.fg_1()).wrapping(text::Wrapping::None);
    let background = bar_background(app);
    let bar = container(
        row![
            container(cancel).width(Fill).align_x(Alignment::Start),
            container(title).clip(true),
            container(accept).width(Fill).align_x(Alignment::End),
        ]
        .spacing(GAP)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([0.0, INSET]))
    .height(HEADER_HEIGHT)
    .width(Fill)
    .align_y(Alignment::Center)
    .style(move |_| theme::rounded(background, theme::window_corners(Side::Top, app.corner())));
    mouse_area(bar).on_press(Message::WindowDrag).on_double_click(Message::WindowMaximize).into()
}

fn toolbar<'a>(app: &'a App, chooser: &'a State) -> Element<'a, Message> {
    let (t, lang) = (&app.theme, app.lang);
    let back = icon_button(t, Icon::ChevronLeft, app.can_go_back().then_some(Message::Back), false);
    let forward = icon_button(t, Icon::ChevronRight, app.can_go_forward().then_some(Message::Forward), false);
    let location =
        path_field(app).unwrap_or_else(|| container(super::pathbar::crumbs_row(app)).width(Fill).clip(true).into());
    let mut bar = Row::new()
        .push(hint(t, back, Key::Back.text(lang)))
        .push(hint(t, forward, Key::Forward.text(lang)))
        .push(container(location).width(Fill))
        .push(search_field(app))
        .spacing(GAP)
        .align_y(Alignment::Center);
    if chooser.new_folder_offered() {
        bar = bar.push(hint(
            t,
            icon_button(t, Icon::FolderPlus, Some(Message::NewFolder), false),
            Key::NewFolder.text(lang),
        ));
    }
    container(bar)
        .padding(Padding::from([theme::SP1, INSET]).left(theme::SP2))
        .width(Fill)
        .style(move |_| theme::surface(t.view_bg()))
        .into()
}

fn name_row<'a>(app: &'a App, chooser: &'a State) -> Option<Element<'a, Message>> {
    if chooser.request.mode != Mode::Save {
        return None;
    }
    let (t, lang) = (&app.theme, app.lang);
    let style = *t;
    let input = text_input(Key::ChooserName.text(lang), &chooser.name)
        .id(NAME_ID)
        .on_input(Message::ChooserName)
        .on_submit(Message::ChooserAccept)
        .padding(Padding::from([7.0, theme::SP3]))
        .size(13.5)
        .width(Fill)
        .style(move |_, status| theme::field(&style, status));
    let ring = if chooser.name.trim().is_empty() || crate::chooser::valid_name(&chooser.name) {
        Ring::Off
    } else {
        Ring::Invalid
    };
    let row = row![
        text(Key::ChooserName.text(lang)).size(theme::CALL).font(theme::SEMIBOLD).color(t.fg_2()),
        ringed(t, input, ring)
    ]
    .spacing(theme::SP3)
    .align_y(Alignment::Center);
    Some(
        container(container(row).max_width(NAME_ROW))
            .padding(Padding::from([theme::SP2, theme::SP4]))
            .width(Fill)
            .center_x(Fill)
            .style(move |_| theme::surface(t.view_bg()))
            .into(),
    )
}

fn dropdown<'a>(
    app: &'a App, options: Vec<Pick>, selected: Option<Pick>, on_pick: impl Fn(Pick) -> Message + 'a,
) -> Element<'a, Message> {
    let t = &app.theme;
    pick_list(options, selected, on_pick)
        .text_size(theme::CALL)
        .padding(Padding::from([6.0, theme::SP3]))
        .width(Length::Shrink)
        .style(move |_, status| theme::dropdown(t, status))
        .menu_style(move |_| theme::dropdown_menu(t))
        .into()
}

fn footer<'a>(app: &'a App, chooser: &'a State) -> Option<Element<'a, Message>> {
    let request = &chooser.request;
    if request.filters.is_empty() && request.choices.is_empty() {
        return None;
    }
    let (t, lang) = (&app.theme, app.lang);
    let label = |caption: &'a str| text(caption).size(theme::CALL).color(t.fg_2()).wrapping(text::Wrapping::None);
    let mut bar = Row::new().spacing(theme::SP4).align_y(Alignment::Center);
    if !request.filters.is_empty() {
        let options = chooser.filter_options();
        let selected = chooser.filter.and_then(|index| options.get(index).cloned());
        let picker = dropdown(app, options, selected, |pick| Message::ChooserFilter(pick.index));
        bar = bar
            .push(row![label(Key::ChooserFileType.text(lang)), picker].spacing(theme::SP2).align_y(Alignment::Center));
    }
    for (index, choice) in request.choices.iter().enumerate() {
        if choice.is_toggle() {
            let on = chooser.choice_is_on(index);
            let check = checkbox(on)
                .label(choice.label.as_str())
                .text_size(theme::CALL)
                .on_toggle(move |on| Message::ChooserChoice(index, on.to_string()))
                .style(move |_, status| theme::checkbox(t, status));
            bar = bar.push(check);
        } else {
            let options = chooser.choice_options(index);
            let current = chooser.choices.get(index);
            let selected = options.iter().find(|pick| Some(&pick.id) == current).cloned();
            let picker = dropdown(app, options, selected, |pick| Message::ChooserChoice(pick.index, pick.id));
            bar = bar.push(row![label(&choice.label), picker].spacing(theme::SP2).align_y(Alignment::Center));
        }
    }
    let corners = theme::window_corners(Side::BottomRight, app.corner());
    Some(
        container(bar.wrap().vertical_spacing(theme::SP2).align_x(Alignment::End))
            .padding(Padding::from([theme::SP2, INSET]))
            .width(Fill)
            .align_x(Alignment::End)
            .style(move |_| theme::rounded(bar_background(app), corners))
            .into(),
    )
}

#[must_use]
pub fn base(app: &App) -> Option<Element<'_, Message>> {
    let chooser = app.chooser.as_ref()?;
    let t = &app.theme;
    let content: Element<'_, Message> = match preview(app) {
        Some(preview) => {
            row![super::content::view(app), rule::vertical(1).style(move |_| theme::line(t)), preview].into()
        }
        None => super::content::view(app),
    };
    let mut pane = column![content].width(Fill).height(Fill);
    if let Some(footer) = footer(app, chooser) {
        pane = pane.push(divider(t)).push(footer);
    }
    let corners = theme::window_corners(Side::BottomRight, app.corner());
    let body = row![
        super::sidebar::view(app),
        rule::vertical(1).style(move |_| theme::line(t)),
        container(pane).width(Fill).height(Fill).style(move |_| theme::rounded(t.view_bg(), corners)),
    ]
    .height(Fill);
    let mut window = column![header(app, chooser), divider(t), toolbar(app, chooser), divider(t)];
    if let Some(name) = name_row(app, chooser) {
        window = window.push(name).push(divider(t));
    }
    Some(
        container(window.push(body))
            .width(Fill)
            .height(Fill)
            .style(move |_| theme::rounded(t.win_bg(), app.corner()))
            .into(),
    )
}

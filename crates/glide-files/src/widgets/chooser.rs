use glide_sdk::theme::TextStyle;
use glide_sdk::widget::{Wrapping, pane, text, vertical_separator};
use iced::widget::{Row, column, container, row, text_input};
use iced::{Alignment, Element, Fill, Length, Padding};

use super::header::{GAP, INSET, bar_background, path_field, search_field};
use super::{HEADER_HEIGHT, divider};
use crate::app::{App, Message};
use crate::chooser::{Mode, Pick, State};
use crate::i18n::{Key, Lang};
use crate::icons::Icon;
use crate::theme::{self, Side};

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
        text(entry.name.clone(), TextStyle::HEAD).wrapping(Wrapping::WordOrGlyph),
        column![text(kind, TextStyle::CAP).color(t.fg_2())]
            .push(modified.map(|modified| text(modified, TextStyle::CAP).color(t.fg_3())))
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
    use glide_sdk::widget::{Size, compact_button};
    let lang = app.lang;
    let paint = super::paint(&app.theme);
    let (cancel, accept) = (Key::Cancel.text(lang), accept_label(chooser, lang));
    let taken = Size::Small.width(cancel).max(Size::Small.width(accept));
    let header = glide_sdk::window::Header::new(app.title())
        .height(HEADER_HEIGHT)
        .without_maximize()
        .without_close()
        .corners(theme::window_corners(Side::Top, app.corner()))
        .leading(compact_button(&paint, cancel, glide_sdk::Tone::Neutral, Some(Message::ChooserCancel)))
        .trailing(compact_button(&paint, accept, glide_sdk::Tone::Accent, Some(Message::ChooserAccept)));
    glide_sdk::window::header(&paint, &app.window(), header, Message::Window, Message::ChooserCancel, taken)
}

fn toolbar<'a>(app: &'a App, chooser: &'a State) -> Element<'a, Message> {
    use glide_sdk::widget::icon_button;
    let (t, lang) = (&app.theme, app.lang);
    let paint = super::paint(t);
    let back =
        icon_button(&paint, Icon::ChevronLeft, Key::Back.text(lang), app.can_go_back().then_some(Message::Back), false);
    let forward = app.can_go_forward().then_some(Message::Forward);
    let forward = icon_button(&paint, Icon::ChevronRight, Key::Forward.text(lang), forward, false);
    let location =
        path_field(app).unwrap_or_else(|| container(super::pathbar::crumbs_row(app)).width(Fill).clip(true).into());
    let mut bar = Row::new()
        .push(back)
        .push(forward)
        .push(container(location).width(Fill))
        .push(search_field(app))
        .spacing(GAP)
        .align_y(Alignment::Center);
    if chooser.new_folder_offered() {
        let label = Key::NewFolder.text(lang);
        bar = bar.push(icon_button(&paint, Icon::FolderPlus, label, Some(Message::NewFolder), false));
    }
    pane(t.view_bg(), 0.0, bar)
        .padding(Padding::from([theme::SP1, INSET]).left(theme::SP2))
        .height(Length::Shrink)
        .into()
}

fn name_row<'a>(app: &'a App, chooser: &'a State) -> Option<Element<'a, Message>> {
    use glide_sdk::widget::{Validity, validated_field};
    if chooser.request.mode != Mode::Save {
        return None;
    }
    let (t, lang) = (&app.theme, app.lang);
    let input = text_input(Key::ChooserName.text(lang), &chooser.name)
        .id(NAME_ID)
        .on_input(Message::ChooserName)
        .on_submit(Message::ChooserAccept);
    let validity = if chooser.name.trim().is_empty() || crate::chooser::valid_name(&chooser.name) {
        Validity::Neutral
    } else {
        Validity::Invalid
    };
    let row = row![
        text(Key::ChooserName.text(lang), TextStyle { weight: 600, ..TextStyle::CALL }).color(t.fg_2()),
        validated_field(&super::paint(t), input, validity)
    ]
    .spacing(theme::SP3)
    .align_y(Alignment::Center);
    Some(
        pane(t.view_bg(), 0.0, container(row).max_width(NAME_ROW))
            .padding(Padding::from([theme::SP2, theme::SP4]))
            .center_x(Fill)
            .height(Length::Shrink)
            .into(),
    )
}

fn dropdown<'a>(
    app: &'a App, options: Vec<Pick>, selected: Option<Pick>, on_pick: impl Fn(Pick) -> Message + 'a,
) -> Element<'a, Message> {
    let widest =
        options.iter().map(|pick| glide_sdk::widget::measure(&pick.to_string(), TextStyle::CALL)).fold(0.0, f32::max);
    let width = (widest + 2.0 * theme::SP3 + theme::SP2 + 12.0).clamp(140.0, 360.0);
    let picker = glide_sdk::widget::dropdown(&super::paint(&app.theme), options, selected, "", on_pick);
    container(picker).width(width).into()
}

fn footer<'a>(app: &'a App, chooser: &'a State) -> Option<Element<'a, Message>> {
    let request = &chooser.request;
    if request.filters.is_empty() && request.choices.is_empty() {
        return None;
    }
    let (t, lang) = (&app.theme, app.lang);
    let label = |caption: &'a str| text(caption, TextStyle::CALL).color(t.fg_2()).wrapping(Wrapping::None);
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
            let message = Message::ChooserChoice(index, (!on).to_string());
            bar = bar.push(super::check(t, choice.label.as_str(), on, message));
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
        pane(bar_background(app), corners, bar.wrap().vertical_spacing(theme::SP2).align_x(Alignment::End))
            .padding(Padding::from([theme::SP2, INSET]))
            .align_x(Alignment::End)
            .height(Length::Shrink)
            .into(),
    )
}

#[must_use]
pub fn base(app: &App) -> Option<Element<'_, Message>> {
    let chooser = app.chooser.as_ref()?;
    let t = &app.theme;
    let content: Element<'_, Message> = match preview(app) {
        Some(preview) => row![super::content::view(app), vertical_separator(t), preview].into(),
        None => super::content::view(app),
    };
    let mut listing = column![content].width(Fill).height(Fill);
    if let Some(footer) = footer(app, chooser) {
        listing = listing.push(divider(t)).push(footer);
    }
    let corners = theme::window_corners(Side::BottomRight, app.corner());
    let body = row![super::sidebar::view(app), vertical_separator(t), pane(t.view_bg(), corners, listing)].height(Fill);
    let mut window = column![header(app, chooser), divider(t), toolbar(app, chooser), divider(t)];
    if let Some(name) = name_row(app, chooser) {
        window = window.push(name).push(divider(t));
    }
    Some(pane(t.win_bg(), app.corner(), window.push(body)).into())
}

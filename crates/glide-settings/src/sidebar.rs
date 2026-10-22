use iced::widget::text::{Span, Wrapping};
use iced::widget::{
    Column, Row, button, column, container, rich_text, row, rule, scrollable, space, span, stack, text, text_input,
};
use iced::{Alignment, Element, Fill, Length, Padding};

use crate::app::{App, Message, SEARCH_ID};
use crate::i18n::{Key as K, Lang};
use crate::icons::Icon;
use crate::panels::{Ctx, Panel};
use crate::theme;
use crate::widgets;

pub const ITEM_HEIGHT: f32 = 32.0;
pub const TILE_SIZE: f32 = 24.0;
pub const AVATAR_SIZE: f32 = 40.0;
pub const GROUP_GAP: f32 = 16.0;
pub const ITEM_GAP: f32 = 2.0;
pub const EDGE: f32 = 12.0;

#[must_use]
pub fn highlight(label: &str, query: &str) -> Option<(usize, usize)> {
    let needle: Vec<char> = query.trim().chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return None;
    }
    let folded: Vec<(usize, usize, char)> = label
        .char_indices()
        .flat_map(|(start, ch)| {
            let end = start + ch.len_utf8();
            ch.to_lowercase().map(move |lower| (start, end, lower))
        })
        .collect();
    folded
        .windows(needle.len())
        .find(|window| window.iter().zip(&needle).all(|(folded, wanted)| folded.2 == *wanted))
        .map(|window| (window[0].0, window[window.len() - 1].1))
}

#[must_use]
pub fn results_label(lang: Lang, count: usize) -> String {
    let noun = match lang {
        Lang::En if count == 1 => K::ResultOne,
        Lang::En => K::ResultMany,
        Lang::Uk => {
            let tens = count % 100;
            match count % 10 {
                1 if tens != 11 => K::ResultOne,
                2..=4 if !(12..=14).contains(&tens) => K::ResultFew,
                _ => K::ResultMany,
            }
        }
    };
    format!("{count} {}", noun.text(lang))
}

fn search_field<'a>(c: &Ctx, query: &'a str) -> Element<'a, Message> {
    let t = c.theme;
    let mut search = stack![
        text_input(c.t(K::SearchPlaceholder), query)
            .id(SEARCH_ID)
            .on_input(Message::SearchChanged)
            .on_submit(Message::SearchSubmitted)
            .padding(Padding::from([8, 32]))
            .size(13)
            .style(move |_: &iced::Theme, status| theme::search_field_style(&t, status)),
        container(widgets::icon(Icon::Search, 15.0, t.fg_3()))
            .padding(Padding::ZERO.left(11))
            .center_y(Length::Fixed(theme::CONTROL_HEIGHT - 1.0)),
    ];
    if query.is_empty() {
        search = search.push(
            container(widgets::key_combo(t, "Ctrl+F"))
                .padding(Padding::ZERO.right(7))
                .align_right(Fill)
                .center_y(Length::Fixed(theme::CONTROL_HEIGHT - 1.0)),
        );
    } else {
        search = search.push(
            container(
                button(
                    container(widgets::icon(Icon::Close, 12.0, t.fg_3()))
                        .center_x(Length::Fixed(22.0))
                        .center_y(Length::Fixed(22.0)),
                )
                .padding(0)
                .on_press(Message::ClearSearch)
                .style(move |_: &iced::Theme, status| theme::icon_button_style(&t, status)),
            )
            .padding(Padding::ZERO.right(6))
            .align_right(Fill)
            .center_y(Length::Fixed(theme::CONTROL_HEIGHT - 1.0)),
        );
    }
    search.into()
}

fn account_card<'a>(app: &'a App, c: &Ctx, selected: bool, user: &'a str, login: &'a str) -> Element<'a, Message> {
    let t = c.theme;
    let me = app.current_account();
    let user = me.map_or(user, |me| me.display_name());
    let login = me.map_or(login, |me| me.user_name.as_str());
    let name = [user, login].into_iter().find(|name| !name.trim().is_empty()).unwrap_or(c.t(K::User));
    let subtitle = if login.is_empty() || login == name { c.t(K::LocalAccount) } else { login };
    let (name_color, sub_color, chevron) =
        if selected { (t.acc_soft_fg(), t.acc_soft_fg(), t.acc_soft_fg()) } else { (t.fg_1(), t.fg_2(), t.fg_3()) };
    let avatar = match me {
        Some(me) => crate::panels::users::portrait(t, me, AVATAR_SIZE),
        None => widgets::avatar(t, name, AVATAR_SIZE),
    };
    button(
        row![
            avatar,
            column![
                text(name).size(14).font(theme::SEMIBOLD).color(name_color).wrapping(Wrapping::None),
                text(subtitle).size(12).color(sub_color).wrapping(Wrapping::None),
            ]
            .spacing(1)
            .width(Fill)
            .clip(true),
            widgets::icon(Icon::ChevronRight, 14.0, chevron),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .on_press(Message::PanelSelected(Panel::Users))
    .padding([8, 10])
    .width(Fill)
    .style(move |_: &iced::Theme, status| theme::account_card_style(&t, selected, status))
    .into()
}

fn item_label<'a>(c: &Ctx, label: &'static str, query: &str, selected: bool) -> Element<'a, Message> {
    let t = c.theme;
    let (color, accent) = if selected { (t.acc_soft_fg(), t.acc_soft_fg()) } else { (t.fg_1(), t.acc_text()) };
    let font = if selected { theme::SEMIBOLD } else { iced::Font::DEFAULT };
    match highlight(label, query) {
        Some((start, end)) => {
            let spans: Vec<Span<'a, (), iced::Font>> = vec![
                span(&label[..start]).font(font),
                span(&label[start..end]).font(theme::SEMIBOLD).color(accent).underline(selected),
                span(&label[end..]).font(font),
            ];
            rich_text(spans).size(14).color(color).wrapping(Wrapping::None).into()
        }
        None => text(label).size(14).font(font).color(color).wrapping(Wrapping::None).into(),
    }
}

fn item<'a>(app: &'a App, c: &Ctx, panel: Panel, first_hit: bool) -> Element<'a, Message> {
    let t = c.theme;
    let query = app.search_query();
    let searching = !query.trim().is_empty();
    let selected = app.active_panel() == panel;
    let trailing_color = if selected { t.acc_soft_fg() } else { t.fg_3() };

    let mut line = Row::new()
        .spacing(10)
        .height(Fill)
        .align_y(Alignment::Center)
        .push(widgets::tile(panel.icon(), panel.color(), TILE_SIZE))
        .push(container(item_label(c, panel.label(c.lang), query, selected)).width(Fill).clip(true));
    if searching {
        if let Some(word) = panel.matched_keyword(query) {
            line = line.push(text(word).size(11.5).color(trailing_color).wrapping(Wrapping::None));
        }
        if first_hit {
            line = line.push(widgets::key_cap(t, "↵"));
        }
    } else if let Some(status) = app.panel_status(panel) {
        line = line.push(text(status).size(12).color(trailing_color).wrapping(Wrapping::None));
    }

    let indicator = container(space())
        .width(theme::SIDEBAR_INDICATOR_WIDTH)
        .height(theme::SIDEBAR_INDICATOR_HEIGHT)
        .style(move |_: &iced::Theme| theme::sidebar_indicator_style(&t, selected));

    button(row![indicator, line].spacing(5).height(Fill).align_y(Alignment::Center))
        .on_press(Message::PanelSelected(panel))
        .width(Fill)
        .height(ITEM_HEIGHT)
        .padding(Padding::ZERO.right(8))
        .style(move |_: &iced::Theme, status| theme::sidebar_item_style(&t, selected, status))
        .into()
}

fn group_heading<'a>(c: &Ctx, title: K) -> Element<'a, Message> {
    container(widgets::micro_label(c.theme, c.t(title))).padding(Padding::from([0, 10]).bottom(4)).into()
}

fn footer<'a>(c: &Ctx, online: bool) -> Element<'a, Message> {
    let t = c.theme;
    let (dot, label) = if online { (t.ok(), K::CompositorOnline) } else { (t.fg_4(), K::CompositorOffline) };
    let status = button(
        row![
            widgets::status_dot(dot, 8.0),
            text(c.t(label)).size(12).color(t.fg_2()).wrapping(Wrapping::None).width(Fill),
            text(format!("Glide {}", env!("CARGO_PKG_VERSION"))).size(11.5).color(t.fg_3()).wrapping(Wrapping::None),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .on_press(Message::PanelSelected(Panel::About))
    .padding([7, 10])
    .width(Fill)
    .style(move |_: &iced::Theme, status| theme::row_button_style(&t, status));
    column![
        rule::horizontal(1).style(move |_: &iced::Theme| theme::rule_style(&t)),
        container(status).padding([6.0, EDGE - 4.0]),
    ]
    .into()
}

fn no_results<'a>(c: &Ctx) -> Element<'a, Message> {
    let t = c.theme;
    container(
        column![widgets::icon(Icon::Search, 28.0, t.fg_4()), text(c.t(K::NoResults)).size(13).color(t.fg_2()),]
            .spacing(8)
            .align_x(Alignment::Center),
    )
    .padding([28, 8])
    .center_x(Fill)
    .into()
}

pub fn view<'a>(app: &'a App, c: Ctx, user: &'a str, login: &'a str) -> Element<'a, Message> {
    let t = c.theme;
    let query = app.search_query();
    let searching = !query.trim().is_empty();
    let visible = app.visible_panels();

    let mut groups = Column::new().spacing(GROUP_GAP);
    if searching && !visible.is_empty() {
        groups = groups
            .push(container(text(results_label(c.lang, visible.len())).size(12).color(t.fg_3())).padding([0, 10]));
    }
    for group in Panel::GROUPS {
        let hits: Vec<Panel> = group.panels.iter().copied().filter(|panel| visible.contains(panel)).collect();
        if hits.is_empty() {
            continue;
        }
        let mut items = Column::new().spacing(ITEM_GAP).push(group_heading(&c, group.title));
        for panel in hits {
            items = items.push(item(app, &c, panel, searching && visible[0] == panel));
        }
        groups = groups.push(items);
    }
    if visible.is_empty() {
        groups = groups.push(no_results(&c));
    }

    let top = column![search_field(&c, query), account_card(app, &c, app.active_panel() == Panel::Users, user, login),]
        .spacing(EDGE)
        .padding(Padding::from([EDGE, EDGE]).bottom(0));

    let list = scrollable(container(groups).padding(Padding::from([EDGE + 4.0, EDGE]).bottom(EDGE)))
        .height(Fill)
        .style(move |_: &iced::Theme, status| theme::scrollable_style(&t, status));

    container(column![top, list, footer(&c, app.online())].width(Length::Fixed(theme::SIDEBAR_WIDTH)))
        .height(Fill)
        .style(move |_: &iced::Theme| theme::solid(t.sidebar_bg(), iced::border::Radius::default()))
        .into()
}

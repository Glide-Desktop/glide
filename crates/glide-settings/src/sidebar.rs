use glide_sdk::theme::SP1;
use glide_sdk::theme::TextStyle;
use glide_sdk::widget::{self as sdk, Wrapping, text};
use iced::widget::{Column, Row, column, container, text_input};
use iced::{Alignment, Element, Fill, Length, Padding};

use crate::app::{App, Message, SEARCH_ID};
use crate::i18n::{Key as K, Lang};
use crate::icons::Icon;
use crate::panels::{Ctx, Panel};
use crate::theme;
use crate::widgets;

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
    let forms = [K::ResultOne, K::ResultFew, K::ResultMany].map(|key| key.text(lang));
    glide_sdk::i18n::count(count as u64, lang, forms)
}

fn search_field<'a>(c: &Ctx, query: &'a str) -> Element<'a, Message> {
    let input = text_input(c.t(K::SearchPlaceholder), query)
        .id(SEARCH_ID)
        .on_input(Message::SearchChanged)
        .on_submit(Message::SearchSubmitted);
    let clear = (!query.is_empty()).then_some(Message::ClearSearch);
    let hint = widgets::key_combo(c.theme, "Ctrl+F");
    sdk::hinted_search_input(&widgets::paint(&c.theme), input, clear, "", Some(hint))
}

fn account_card<'a>(app: &'a App, c: &Ctx, selected: bool, user: &'a str, login: &'a str) -> Element<'a, Message> {
    let t = c.theme;
    let me = app.current_account();
    let user = me.map_or(user, |me| me.display_name());
    let login = me.map_or(login, |me| me.user_name.as_str());
    let name = [user, login].into_iter().find(|name| !name.trim().is_empty()).unwrap_or(c.t(K::User));
    let subtitle = if login.is_empty() || login == name { c.t(K::LocalAccount) } else { login };
    let avatar = match me {
        Some(me) => crate::panels::users::portrait(t, me, AVATAR_SIZE),
        None => widgets::avatar(t, name, AVATAR_SIZE),
    };
    sdk::profile_card(&widgets::paint(&t), avatar, name, subtitle, selected, Message::PanelSelected(Panel::Users))
}

fn item<'a>(app: &'a App, c: &Ctx, panel: Panel, first_hit: bool) -> Element<'a, Message> {
    let t = c.theme;
    let paint = widgets::paint(&t);
    let query = app.search_query();
    let searching = !query.trim().is_empty();
    let selected = app.active_panel() == panel;
    let trailing_color = if selected { t.acc_soft_fg() } else { t.fg_3() };
    let label = panel.label(c.lang);
    let mut trailing = Row::new().spacing(SP1 + 2.0).align_y(Alignment::Center);
    if searching {
        if let Some(word) = panel.matched_keyword(query) {
            trailing = trailing.push(text(word, TextStyle::CAP).color(trailing_color).wrapping(Wrapping::None));
        }
        if first_hit {
            trailing = trailing.push(widgets::key_cap(t, "↵"));
        }
    } else if let Some(status) = app.panel_status(panel) {
        trailing = trailing.push(text(status, TextStyle::CAP).color(trailing_color).wrapping(Wrapping::None));
    }
    sdk::SidebarItem::new(&paint, label)
        .leading(widgets::tile(panel.icon(), panel.color(), TILE_SIZE))
        .matched(highlight(label, query))
        .accessory(trailing)
        .selected(selected)
        .current(selected)
        .on_press(Some(Message::PanelSelected(panel)))
        .into()
}

fn group_heading<'a>(c: &Ctx, title: K) -> Element<'a, Message> {
    container(widgets::micro_label(c.theme, c.t(title))).padding(Padding::from([0, 10]).bottom(4)).into()
}

fn footer<'a>(c: &Ctx, online: bool) -> Element<'a, Message> {
    let t = c.theme;
    let paint = widgets::paint(&t);
    let (dot, label) = if online { (t.ok(), K::CompositorOnline) } else { (t.fg_4(), K::CompositorOffline) };
    let version = text(format!("Glide {}", env!("CARGO_PKG_VERSION")), TextStyle::CAP).color(t.fg_3());
    let status = sdk::SidebarItem::new(&paint, c.t(label))
        .leading(widgets::status_dot(dot, 8.0))
        .ink(Some(t.fg_2()))
        .accessory(version.wrapping(Wrapping::None))
        .on_press(Some(Message::PanelSelected(Panel::About)));
    column![sdk::separator(&paint), container(status).padding([6.0, EDGE - 4.0])].into()
}

fn no_results<'a>(c: &Ctx) -> Element<'a, Message> {
    widgets::empty_state(c.theme, Icon::Search, "", c.t(K::NoResults))
}

pub fn view<'a>(app: &'a App, c: Ctx, user: &'a str, login: &'a str) -> Element<'a, Message> {
    let t = c.theme;
    let query = app.search_query();
    let searching = !query.trim().is_empty();
    let visible = app.visible_panels();

    let mut groups = Column::new().spacing(GROUP_GAP);
    if searching && !visible.is_empty() {
        let found = text(results_label(c.lang, visible.len()), TextStyle::CAP).color(t.fg_3());
        groups = groups.push(container(found).padding([0, 10]));
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

    let paint = widgets::paint(&t);
    let list =
        sdk::scrollable(&paint, container(groups).padding(Padding::from([EDGE + 4.0, EDGE]).bottom(EDGE))).height(Fill);
    let body = column![top, list, footer(&c, app.online())].width(Length::Fixed(theme::SIDEBAR_WIDTH));
    sdk::pane(t.sidebar_bg(), 0.0, body).width(Length::Shrink).into()
}

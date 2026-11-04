use glide_sdk::theme::SP1;
use glide_sdk::theme::TextStyle;
use glide_sdk::widget::{self as sdk, Wrapping, text};
use iced::widget::{Column, Row, column, container, text_input};
use iced::{Alignment, Element, Fill, Length, Padding};

use crate::app::{App, Message, SEARCH_ID};
use crate::i18n::{Key as K, Lang};
use crate::icons::Icon;
use crate::panels::{Category, Ctx, Panel};
use crate::theme;
use crate::widgets;

pub const TILE_SIZE: f32 = 22.0;
pub(crate) const AVATAR_SIZE: f32 = 36.0;
pub const GROUP_GAP: f32 = 9.0;
pub const ITEM_GAP: f32 = 1.0;
pub const EDGE: f32 = 12.0;
const STATUS: TextStyle = TextStyle { size_px: 11.5, line_height_px: 16.0, weight: 450, ..TextStyle::CAP };

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
    let paint = c.theme.paint;
    sdk::sized_search_input(&paint, input, clear, "", Some(sdk::keycap(&paint, "Ctrl F")), 32.0, 10.0)
}

fn account_card<'a>(app: &'a App, c: &Ctx, selected: bool, user: &'a str, login: &'a str) -> Element<'a, Message> {
    let t = c.theme;
    let me = app.current_account();
    let user = me.map_or(user, |me| me.display_name());
    let login = me.map_or(login, |me| me.user_name.as_str());
    let name = [user, login].into_iter().find(|name| !name.trim().is_empty()).unwrap_or(c.t(K::User));
    let role = c.t(if me.is_some_and(|me| me.administrator) { K::RoleAdministrator } else { K::RoleStandard });
    let subtitle = if login.is_empty() { role.to_string() } else { format!("{login} · {role}") };
    let avatar = match me {
        Some(me) => crate::panels::users::portrait(t, me, AVATAR_SIZE),
        None => widgets::avatar(t, name, AVATAR_SIZE),
    };
    sdk::profile_card(&t.paint, avatar, name, &subtitle, selected, Message::PanelSelected(Panel::Users))
}

fn hit_label(category: Category, query: &str, lang: Lang) -> Option<String> {
    let panel = category.matched_panel(query)?;
    let label = category.label(lang);
    if label.to_lowercase().contains(&query.trim().to_lowercase()) {
        return None;
    }
    let tab = panel.label(lang);
    Some(if tab == label { panel.matched_keyword(query).unwrap_or_default().to_string() } else { tab.to_string() })
}

fn item<'a>(app: &'a App, c: &Ctx, category: Category, first_hit: bool) -> Element<'a, Message> {
    let t = c.theme;
    let paint = widgets::paint(&t);
    let query = app.search_query();
    let searching = !query.trim().is_empty();
    let selected = app.active_category() == category;
    let trailing_color = if selected { iced::Color { a: 0.8, ..t.acc_soft_fg() } } else { t.fg_3() };
    let label = category.label(c.lang);
    let mut trailing = Row::new().spacing(SP1 + 2.0).align_y(Alignment::Center);
    if searching {
        if let Some(word) = hit_label(category, query, c.lang).filter(|word| !word.is_empty()) {
            trailing = trailing.push(text(word, STATUS).color(trailing_color).wrapping(Wrapping::None));
        }
        if first_hit {
            trailing = trailing.push(widgets::key_cap(t, "↵"));
        }
    } else if let Some(status) = app.category_status(category) {
        let warn = category == Category::Home;
        let (style, color) =
            if warn { (TextStyle { weight: 600, ..STATUS }, t.warn()) } else { (STATUS, trailing_color) };
        trailing = trailing.push(text(status, style).color(color).wrapping(Wrapping::None));
    }
    sdk::SidebarItem::new(&paint, label)
        .dense()
        .leading(sdk::inked_tile(category.icon(), category.color(), category.ink(), TILE_SIZE))
        .matched(highlight(label, query))
        .accessory(trailing)
        .selected(selected)
        .current(selected)
        .on_press(Some(Message::CategorySelected(category)))
        .into()
}

fn group_heading<'a>(c: &Ctx, title: K) -> Element<'a, Message> {
    container(widgets::micro_label(c.theme, c.t(title))).padding(Padding::from([0, 10]).bottom(4)).into()
}

fn no_results<'a>(c: &Ctx) -> Element<'a, Message> {
    widgets::empty_state(c.theme, Icon::Search, "", c.t(K::NoResults))
}

pub fn view<'a>(app: &'a App, c: Ctx, user: &'a str, login: &'a str) -> Element<'a, Message> {
    let t = c.theme;
    let query = app.search_query();
    let searching = !query.trim().is_empty();
    let visible = app.visible_categories();

    let mut groups = Column::new().spacing(GROUP_GAP);
    if searching && !visible.is_empty() {
        let found = text(results_label(c.lang, visible.len()), TextStyle::CAP).color(t.fg_3());
        groups = groups.push(container(found).padding([0, 10]));
    }
    for group in Category::GROUPS {
        let hits: Vec<Category> =
            group.categories.iter().copied().filter(|category| visible.contains(category)).collect();
        if hits.is_empty() {
            continue;
        }
        let mut items = Column::new().spacing(ITEM_GAP);
        if let Some(title) = group.title {
            items = items.push(group_heading(&c, title));
        }
        for category in hits {
            items = items.push(item(app, &c, category, searching && visible[0] == category));
        }
        groups = groups.push(items);
    }
    if visible.is_empty() {
        groups = groups.push(no_results(&c));
    }

    let top = column![search_field(&c, query), account_card(app, &c, app.active_panel() == Panel::Users, user, login),]
        .spacing(10)
        .padding(Padding::from([EDGE, EDGE]).bottom(0));

    let paint = widgets::paint(&t);
    let list =
        sdk::scrollable(&paint, container(groups).padding(Padding::from([10.0, EDGE]).bottom(EDGE))).height(Fill);
    let body = column![top, list].width(Length::Fixed(theme::SIDEBAR_WIDTH));
    sdk::pane(t.sidebar_bg(), 0.0, body).width(Length::Shrink).into()
}

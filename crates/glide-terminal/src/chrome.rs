use alacritty_terminal::index::Direction;
use iced::border::Radius;
use iced::widget::{Space, column, container, pane_grid, row, text_input};
use iced::{Alignment, Color, Element, Fill, Padding, Rectangle, Size};

use crate::app::{App, Command, Confirm, FIND_ID, Menu, MenuKind, Message, shortcut_label};
use crate::i18n::Key;
use crate::marks::Mark;
use crate::profile::{Profile, color};
use crate::session::Context;
use crate::terminal::Terminal;
use glide_sdk::overlay::{ringed, toast_pill};
use glide_sdk::theme::{R2, R3, SP2, SP3, TextStyle, component, motion};
use glide_sdk::widget::{
    Icon, Severity, Wrapping, button, empty_state, grid, icon_button, palette_swatch, pane, preview_card,
    separator as rule, split_view, status_badge, text, tooltip,
};
use glide_sdk::window::{Action as WindowAction, resize_handles};
use glide_sdk::{Paint, Tone};

pub const HEADER: f32 = glide_sdk::window::HEADER;
const FIND_WIDTH: f32 = 520.0;
const MENU_WIDTH: f32 = 264.0;
const PANE_MIN: f32 = 50.0;
const SWATCH: Size = Size::new(96.0, 44.0);

fn header(app: &App) -> Element<'_, Message> {
    let window = app.window;
    let controls = window.controls_paint(&app.paint);
    let options_open =
        app.menu.as_ref().is_some_and(|menu| matches!(menu.kind, MenuKind::Options | MenuKind::Profiles));
    let new_tab = Some(Message::Command(Command::NewTab));
    let options = Some(Message::Command(Command::Options));
    let several = app.tabs.len() > 1;
    let center = glide_sdk::widget::swapped(several, None, if several { tab_bar(app) } else { context_title(app) });
    let header = glide_sdk::window::Header::new("")
        .labels(Key::Maximize.text(app.lang), Key::CloseWindow.text(app.lang))
        .height(HEADER)
        .context(Message::HeaderMenu)
        .center(center)
        .trailing(icon_button(&controls, Icon::Plus, Key::NewTab.text(app.lang), new_tab, false))
        .trailing(icon_button(&controls, Icon::More, Key::Options.text(app.lang), options, options_open));
    glide_sdk::window::header(&app.paint, &window, header, Message::Window, Message::Command(Command::CloseWindow), 0.0)
}

fn lead(context: &Context) -> Option<(Icon, Severity)> {
    let severity = if context.root() { Severity::Danger } else { Severity::Accent };
    match context {
        Context::Local => None,
        Context::Elevated => Some((Icon::Shield, severity)),
        Context::Remote { .. } => Some((Icon::Server, severity)),
    }
}

fn context_title(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let mut title = row![].spacing(SP2).align_y(Alignment::Center);
    if let Some(tab) = app.tab() {
        let context = tab.context();
        let label = match context {
            Context::Local => None,
            Context::Elevated => Some(String::from("root")),
            remote @ Context::Remote { .. } => remote.short_address().map(|address| format!("ssh · {address}")),
        };
        if let (Some(label), Some((_, severity))) = (label, lead(context)) {
            title = title.push(tooltip(&paint, status_badge(&paint, label, severity), &tab.hint));
        }
    }
    let color = if app.window.focused { paint.fg_1() } else { paint.fg_3() };
    title = title.push(text(app.title(), TextStyle::WIN_TITLE).color(color).wrapping(Wrapping::None));
    container(title).center_x(Fill).center_y(HEADER).clip(true).into()
}

fn tab_bar(app: &App) -> Element<'_, Message> {
    use glide_sdk::widget::{Mark, Tab, tab_strip};
    let tabs = app
        .tabs
        .iter()
        .map(|tab| {
            let mark = if tab.bell {
                Mark::Alert
            } else if tab.activity {
                Mark::Dot
            } else {
                Mark::None
            };
            let mut item = Tab::new(tab.id.0, app.tab_title(tab)).mark(mark);
            if let Some((glyph, severity)) = lead(tab.context()) {
                item = item.lead(glyph, severity).hint(&tab.hint);
            }
            item
        })
        .collect();
    tab_strip(&app.paint, tabs, app.active)
        .height(HEADER)
        .bare()
        .on_select(Message::TabPressed)
        .hovered(app.hovered_tab, Message::TabHovered)
        .on_reorder(Message::TabMoved)
        .on_close(|index| Message::Command(Command::CloseTab(index)), Key::CloseTab.text(app.lang))
        .into()
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FindView {
    query: String,
    case: bool,
    counting: bool,
    index: usize,
    total: usize,
}

pub(crate) fn find_view(app: &App) -> Option<FindView> {
    let find = app.find.as_ref()?;
    Some(FindView {
        query: find.query.clone(),
        case: find.case,
        counting: find.counting,
        index: find.index,
        total: find.total,
    })
}

fn find_bar<'a>(app: &'a App, find: &FindView, open: bool) -> Element<'a, Message> {
    let paint = app.paint;
    let input = text_input(Key::FindPlaceholder.text(app.lang), &find.query)
        .id(FIND_ID)
        .on_input(Message::FindInput)
        .on_submit(Message::FindSubmit);
    let count = if find.query.is_empty() {
        String::new()
    } else if find.counting && find.index == 0 {
        String::from("…")
    } else if find.total == 0 {
        Key::NoMatches.text(app.lang).to_string()
    } else {
        format!("{} / {}", find.index, find.total)
    };
    let width = (app.window.size.width - 2.0 * SP3 - 2.0 * SP2).clamp(0.0, FIND_WIDTH);
    let bar = glide_sdk::widget::find_bar(&paint, input)
        .narrow(width < FIND_WIDTH)
        .floating(width)
        .steps(
            (Message::Command(Command::FindUp), Key::FindPrevious.text(app.lang)),
            (Message::Command(Command::FindDown), Key::FindNext.text(app.lang)),
        )
        .toggle("Aa", Key::CaseSensitive.text(app.lang), find.case, Message::FindCase)
        .status(count, false)
        .done(Key::Done.text(app.lang), Message::FindClose);
    container(glide_sdk::anim::grow(open, bar))
        .width(Fill)
        .align_x(Alignment::End)
        .padding(Padding { top: HEADER + 1.0 + SP2, right: SP3, ..Padding::ZERO })
        .into()
}

#[must_use]
pub(crate) fn pane_at(app: &App, (x, y): (f32, f32)) -> Option<pane_grid::Pane> {
    let tab = app.tab()?;
    let top = HEADER + 1.0;
    let size = Size::new(app.window.size.width, (app.window.size.height - top).max(0.0));
    let point = iced::Point::new(x, y - top);
    tab.panes
        .layout()
        .pane_regions(1.0, PANE_MIN, size)
        .into_iter()
        .find(|(_, region)| region.contains(point))
        .map(|(pane, _)| pane)
}

fn corners(region: Rectangle) -> Radius {
    let bottom = region.y + region.height >= 0.999;
    let mut radius = Radius::default();
    if bottom && region.x <= 0.001 {
        radius = radius.bottom_left(R3);
    }
    if bottom && region.x + region.width >= 0.999 {
        radius = radius.bottom_right(R3);
    }
    radius
}

fn content(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let bottom =
        if app.edge_to_edge() { Radius::default() } else { Radius::default().bottom_left(R3).bottom_right(R3) };
    let Some(tab) = app.tab() else {
        let palette = app.prefs.profile.palette(&paint.tokens);
        let notice: Element<'_, Message> = match &app.error {
            Some(error) => empty_state(&paint, Icon::Warning, Key::SpawnFailed.text(app.lang), error, None),
            None => Space::new().into(),
        };
        return pane(color(palette.background, palette.opacity), bottom, notice).into();
    };
    let look = app.look(tab.profile);
    let regions = tab.panes.layout().pane_regions(0.0, 0.0, Size::new(1.0, 1.0));
    let blink = app.blink_visible();
    let pattern = app.find.as_ref().map(|find| find.pattern.as_str());
    let target = app.drop_at.and_then(|at| pane_at(app, at));
    let grid = split_view(&paint, &tab.panes, |handle, pane, _| {
        let focused = handle == tab.focus;
        let terminal = Terminal::new(pane, look, app.pane_focused(pane.id))
            .blink(blink)
            .find(if focused { pattern } else { None })
            .hints(
                app.hint_mode
                    .as_ref()
                    .filter(|mode| mode.pane == pane.id)
                    .map(|mode| (mode.items.as_slice(), mode.typed.as_str())),
            )
            .flash(app.flash(pane.id))
            .drop_target(target == Some(handle))
            .corners(if app.edge_to_edge() {
                Radius::default()
            } else {
                regions.get(&handle).copied().map_or_else(Radius::default, corners)
            });
        pane_grid::Content::new(glide_sdk::widget::arriving(Element::from(terminal).map(Message::Terminal)))
    })
    .min_size(PANE_MIN)
    .on_click(Message::PaneClicked)
    .on_resize(6, Message::SplitResized);
    let backdrop = if tab.panes.len() > 1 { paint.divider() } else { Color::TRANSPARENT };
    pane(backdrop, bottom, grid).into()
}

type MenuItem = glide_sdk::menu::Item<Message>;

fn entry(app: &App, key: Key, command: Command, enabled: bool) -> MenuItem {
    let item = MenuItem::action(key.text(app.lang), Message::Command(command)).enabled(enabled);
    match shortcut_label(command) {
        Some(chord) => item.shortcut(chord),
        None => item,
    }
}

#[must_use]
pub fn menu_items(app: &App, kind: &MenuKind) -> Vec<MenuItem> {
    let separator = MenuItem::separator;
    match kind {
        MenuKind::Window => vec![
            MenuItem::action(Key::Maximize.text(app.lang), Message::Window(WindowAction::ToggleMaximize)),
            MenuItem::action(Key::Minimize.text(app.lang), Message::Window(WindowAction::Minimize)),
            entry(app, Key::CloseWindow, Command::CloseWindow, true),
        ],
        MenuKind::Options => vec![
            entry(app, Key::NewWindow, Command::NewWindow, true),
            entry(app, Key::NewTab, Command::NewTab, true),
            entry(app, Key::SplitDown, Command::SplitDown, true),
            entry(app, Key::SplitRight, Command::SplitRight, true),
            separator(),
            entry(app, Key::Find, Command::Find, true),
            entry(app, Key::OpenHints, Command::Hints, true),
            entry(app, Key::ExportText, Command::Export, true),
            entry(app, Key::ClearScrollback, Command::ClearScrollback, true),
            entry(app, Key::Reset, Command::Reset, true),
            separator(),
            entry(app, Key::MarkLine, Command::Mark(Mark::Line), true),
            entry(app, Key::Bookmark, Command::Mark(Mark::Bookmark), true),
            entry(app, Key::PreviousMark, Command::Jump(Direction::Left, false), true),
            entry(app, Key::NextMark, Command::Jump(Direction::Right, false), true),
            separator(),
            entry(app, Key::Profiles, Command::Profiles, true).cascades(),
            MenuItem::stepper(
                Key::FontSize.text(app.lang),
                format!("{:.0}", app.fonts().size),
                Some(Message::Command(Command::Smaller)),
                Some(Message::Command(Command::Bigger)),
            ),
            entry(app, Key::FullScreen, Command::FullScreen, true),
            separator(),
            entry(app, Key::Shortcuts, Command::Shortcuts, true),
        ],
        MenuKind::Context(link) => {
            let selection = app
                .focused()
                .is_some_and(|pane| pane.term.lock().selection.as_ref().is_some_and(|selection| !selection.is_empty()));
            let mut items = vec![
                entry(app, Key::Copy, Command::Copy, selection),
                entry(app, Key::Paste, Command::Paste, true),
                entry(app, Key::SelectAll, Command::SelectAll, true),
            ];
            if app.man_topic().is_some() {
                items.extend([separator(), entry(app, Key::ManPage, Command::ManPage, true)]);
            }
            if let Some(link) = link {
                items.extend([
                    separator(),
                    MenuItem::action(Key::OpenLink.text(app.lang), Message::Open(link.clone())),
                    MenuItem::action(Key::CopyLink.text(app.lang), Message::CopyText(link.clone())),
                ]);
            }
            items.extend([
                separator(),
                entry(app, Key::SplitDown, Command::SplitDown, true),
                entry(app, Key::SplitRight, Command::SplitRight, true),
                entry(app, Key::ClosePane, Command::ClosePane, true),
                separator(),
                entry(app, Key::ClearScrollback, Command::ClearScrollback, true),
                entry(app, Key::Profiles, Command::Profiles, true).cascades(),
            ]);
            items
        }
        MenuKind::Profiles => Vec::new(),
    }
}

pub fn menu_actions(app: &App, kind: &MenuKind) -> Vec<Message> {
    if *kind == MenuKind::Profiles {
        let mut actions: Vec<_> =
            Profile::ALL.into_iter().map(|profile| Message::Command(Command::SetProfile(profile))).collect();
        if app.tab().is_some_and(|tab| tab.profile != app.prefs.profile) {
            actions.push(Message::Command(Command::MakeDefault));
        }
        return actions;
    }
    menu_items(app, kind).iter().flat_map(|item| item.messages().into_iter().cloned()).collect()
}

fn menu_selected(app: &App, message: &Message) -> bool {
    let Some(menu) = &app.menu else { return false };
    let Some(selected) = menu.selected else { return false };
    let actions = menu_actions(app, &menu.kind);
    match (actions.get(selected), message) {
        (Some(Message::Command(a)), Message::Command(b)) => a == b,
        (Some(Message::Open(a)), Message::Open(b)) | (Some(Message::CopyText(a)), Message::CopyText(b)) => a == b,
        (Some(Message::Window(a)), Message::Window(b)) => a == b,
        _ => false,
    }
}

fn swatch(app: &App, paint: &Paint, profile: Profile, current: bool) -> Element<'static, Message> {
    let palette = profile.palette(&paint.tokens);
    let accents = [1, 2, 3, 4].map(|index| color(palette.ansi[index], paint.alpha));
    let background = color(palette.background, paint.alpha);
    let sample =
        palette_swatch(paint, background, color(palette.foreground, paint.alpha), &accents, crate::MONO, SWATCH);
    let mut name = profile.name().to_string();
    if profile == app.prefs.profile {
        name = format!("{name} · {}", Key::DefaultProfile.text(app.lang));
    }
    let name = glide_sdk::widget::elide_end(&name, SWATCH.width, TextStyle { weight: 600, ..TextStyle::CAP });
    let selected = menu_selected(app, &Message::Command(Command::SetProfile(profile)));
    let card = preview_card(paint, sample, name, current, Some(Message::Command(Command::SetProfile(profile))));
    iced::widget::container(ringed(paint, card, R3, selected)).center_x(Fill).into()
}

fn profiles(app: &App, paint: &Paint) -> (Element<'static, Message>, Size) {
    use glide_sdk::theme::SP1;
    use glide_sdk::widget::RING;
    let current = app.tab().map_or(app.prefs.profile, |tab| tab.profile);
    let cells = Profile::ALL.iter().map(|profile| swatch(app, paint, *profile, *profile == current)).collect();
    let selected = menu_selected(app, &Message::Command(Command::MakeDefault));
    let make_default = (current != app.prefs.profile).then_some(Message::Command(Command::MakeDefault));
    let make_default = button(paint, Key::MakeDefault.text(app.lang), Tone::Neutral, make_default);
    let title = text(Key::Profiles.text(app.lang).trim_end_matches('…').to_string(), TextStyle::HEAD);
    let footer = row![Space::new().width(Fill), ringed(paint, make_default, R2, selected)];
    let body = column![title, grid(cells, 3, SP2), footer].spacing(SP3).padding(SP2);
    let rows = Profile::ALL.len().div_ceil(3) as f32;
    let cell = Size::new(
        SWATCH.width + 2.0 * (SP1 + RING),
        SWATCH.height + 2.0 * (SP1 + RING) + SP1 + 2.0 + TextStyle::CAP.line_height_px,
    );
    let width = 3.0 * cell.width + 2.0 * SP2 + 2.0 * SP2 + 2.0;
    let height = TextStyle::HEAD.line_height_px
        + rows * cell.height
        + (rows - 1.0) * SP2
        + component::CONTROL
        + 2.0 * RING
        + 2.0 * SP3
        + 2.0 * SP2
        + 2.0 * SP1
        + 2.0;
    (body.into(), Size::new(width, height))
}

fn menu_rows(app: &App, paint: &Paint, menu: &Menu) -> (Vec<Element<'static, Message>>, Size) {
    use glide_sdk::menu::{self, Event};
    let items = menu_items(app, &menu.kind);
    let mut first = Vec::with_capacity(items.len());
    let mut owner = Vec::new();
    for (row, item) in items.iter().enumerate() {
        let count = item.messages().len();
        first.push((count > 0).then_some(owner.len()));
        owner.extend(std::iter::repeat_n(row, count));
    }
    let selected = menu.selected.and_then(|at| owner.get(at).copied());
    let size = Size::new(menu::width(&items, MENU_WIDTH), menu::height(&items));
    let rows = menu::rows(paint, &items, selected)
        .into_iter()
        .zip(&items)
        .zip(first)
        .map(|((row, item), first)| {
            let messages: Vec<Message> = item.messages().into_iter().cloned().collect();
            row.map(move |event| match event {
                Event::Hovered(_) => first.map_or(Message::Frame, Message::MenuHovered),
                Event::Picked(_) => messages.first().cloned().unwrap_or(Message::Frame),
                Event::Stepped(_, more) => {
                    messages.get(usize::from(more)).or(messages.first()).cloned().unwrap_or(Message::Frame)
                }
                Event::Dismiss => Message::Dismiss,
            })
        })
        .collect();
    (rows, size)
}

fn menu_layer<'a>(app: &'a App, menu: &'a Menu) -> Element<'a, Message> {
    let progress = menu.reveal.value(motion::t2()) as f32;
    let paint = app.paint.faded(progress);
    let (rows, size) = match &menu.kind {
        MenuKind::Profiles => {
            let (body, size) = profiles(app, &paint);
            (vec![body], size)
        }
        _ => menu_rows(app, &paint, menu),
    };
    let window = app.window.size;
    let (anchor, may_flip) = match menu.kind {
        MenuKind::Context(_) | MenuKind::Window => (menu.at, true),
        _ => (iced::Point::new(window.width - size.width - 8.0, HEADER - 2.0), false),
    };
    let (at, size) = glide_sdk::menu::place(anchor, size.height, size.width, window, may_flip);
    glide_sdk::menu::layer(&paint, rows, at, size, progress, Some(Message::Dismiss))
}

fn dialog<'a>(app: &'a App, confirm: &'a Confirm) -> Element<'a, Message> {
    use glide_sdk::overlay::{dialog_buttons, dialog_card, modal};
    let progress = confirm.reveal.value(motion::t2()) as f32;
    let paint = app.paint.faded(progress);
    let body = format!("{}: {}.", Key::ConfirmBody.text(app.lang), confirm.processes.join(", "));
    let buttons = dialog_buttons(
        &paint,
        vec![
            (Key::Terminate.text(app.lang), glide_sdk::Tone::Danger, Some(Message::Confirm(true))),
            (Key::Cancel.text(app.lang), glide_sdk::Tone::Neutral, Some(Message::Confirm(false))),
        ],
        Some(usize::from(!confirm.accept_focused)),
    );
    let body = text(body, TextStyle::CALL).color(paint.fg_2()).into();
    let card = dialog_card(&paint, Key::ConfirmTitle.text(app.lang), Some(body), buttons, 380.0, app.window.size);
    modal(&paint, progress, card, None, R3)
}

pub fn view(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let body = column![header(app), rule(&paint), content(app)];
    let mut layers: Vec<Element<'_, Message>> = vec![body.into()];
    if !app.edge_to_edge() {
        layers.push(resize_handles(Message::Window));
    }
    if let Some((find, open)) = app.find_hold.show(find_view(app).as_ref()) {
        layers.push(find_bar(app, &find, open));
    }
    if let Some(error) = app.error.as_ref().filter(|_| !app.tabs.is_empty()) {
        layers.push(toast_pill(&paint, 1.0, error, Some(paint.danger()), None, app.window.size));
    }
    if let Some((text, at)) = &app.notice {
        layers.push(toast_pill(&paint, App::reveal(*at), text, None, None, app.window.size));
    }
    if let Some(menu) = &app.menu {
        layers.push(menu_layer(app, menu));
    }
    if let Some(confirm) = app.confirm.as_ref().or(app.closing_confirm.as_ref()) {
        layers.push(dialog(app, confirm));
    }
    iced::widget::stack(layers).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_bottom_corners_of_the_window_are_rounded() {
        let whole = corners(Rectangle { x: 0.0, y: 0.0, width: 1.0, height: 1.0 });
        assert_eq!((whole.bottom_left, whole.bottom_right, whole.top_left), (R3, R3, 0.0));
        let top_right = corners(Rectangle { x: 0.5, y: 0.0, width: 0.5, height: 0.5 });
        assert_eq!(top_right, Radius::default());
    }

    #[test]
    fn compact_find_controls_stay_inside_the_minimum_window_width() {
        use iced::advanced::{layout, widget::Tree};
        fn inside(node: &layout::Node, x: f32, width: f32) {
            let bounds = node.bounds();
            let x = x + bounds.x;
            assert!(x >= -0.01 && x + bounds.width <= width + 0.01, "{x} {bounds:?}");
            for child in node.children() {
                inside(child, x, width);
            }
        }
        let (mut app, _) = App::with_launch(crate::session::Launch::default());
        app.window.size = Size::new(420.0, 260.0);
        app.find = Some(crate::app::Find::default());
        let find = find_view(&app).expect("a find session is open");
        let mut view = find_bar(&app, &find, true);
        let mut tree = Tree::new(&view);
        let renderer = iced::Renderer::Secondary(iced_tiny_skia::Renderer::new(crate::MONO, iced::Pixels(13.0)));
        let node =
            view.as_widget_mut().layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, Size::new(420.0, 84.0)));
        inside(&node, 0.0, 420.0);
    }
}

use alacritty_terminal::index::Direction;
use glide_theme::elevation::Level;
use glide_theme::motion::ST_HOVER;
use iced::border::Radius;
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, pane_grid, pin, row, scrollable, space, stack, text_input,
};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding, Rectangle, Size};

use crate::app::{App, Command, Confirm, FIND_ID, Menu, MenuKind, Message, Tab, shortcut_label};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::marks::Mark;
use crate::profile::{Profile, color};
use crate::style::{self, Paint, R2, R3, SP1, SP2, SP3, SP4, SP5, SP6, Tone};
use crate::terminal::Terminal;
use crate::widgets;
use glide_iced::text::{self, text};
use glide_sdk::widget::{separator as rule, tooltip as tip};
use glide_sdk::window::resize_edges;
use glide_theme::{TextStyle, component};

pub const HEADER: f32 = 35.0;
const HEADER_CONTROL: f32 = CONTROL;
const TABS: f32 = component::LARGE_CONTROL;
const TAB_PAD: f32 = SP2;
const TAB_GAP: f32 = SP1;
const FIND: f32 = component::LARGE_CONTROL;
const CONTROL: f32 = component::SMALL_CONTROL;
const MENU_WIDTH: f32 = 264.0;
const ITEM: f32 = component::MENU_ITEM;
const PANE_MIN: f32 = 50.0;
const SWATCH: Size = Size::new(96.0, 44.0);

fn glyph<'a>(icon: Icon, size: f32, tint: Color) -> Element<'a, Message> {
    crate::icons::glyph(icon, size, tint)
}

fn icon_button<'a>(
    paint: &Paint, icon: Icon, message: Message, on: bool, label: &'static str, side: f32,
) -> Element<'a, Message> {
    let paint = *paint;
    let control =
        button(container(glyph(icon, side.min(16.0), if on { paint.fg_1() } else { paint.fg_2() })).center(Fill))
            .on_press(message)
            .width(side)
            .height(side)
            .padding(0);
    tip(
        &paint,
        widgets::button(control, move |status| {
            if on { style::flat(paint.fill_2(), paint.fg_1(), R2) } else { style::action(&paint, Tone::Ghost, status) }
        }),
        label,
    )
}

fn header(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let control_paint = if app.window_focused() { paint } else { paint.faded(component::IDLE_CONTROLS) };
    let options_open =
        app.menu.as_ref().is_some_and(|menu| matches!(menu.kind, MenuKind::Options | MenuKind::Profiles));
    let close = button(crate::icons::window_close())
        .on_press(Message::Command(Command::CloseWindow))
        .width(HEADER_CONTROL)
        .height(HEADER_CONTROL)
        .padding(0);
    let close = tip(
        &paint,
        widgets::button(close, move |status| style::window_control(&control_paint, status)),
        Key::CloseWindow.text(app.lang),
    );
    let maximize = button(crate::icons::window_control(app.edge_to_edge()))
        .on_press(Message::WindowMaximize)
        .width(HEADER_CONTROL)
        .height(HEADER_CONTROL)
        .padding(0);
    let maximize = tip(
        &paint,
        widgets::button(maximize, move |status| style::maximize_control(&control_paint, status)),
        Key::Maximize.text(app.lang),
    );
    let controls = row![
        icon_button(
            &control_paint,
            Icon::Plus,
            Message::Command(Command::NewTab),
            false,
            Key::NewTab.text(app.lang),
            HEADER_CONTROL
        ),
        icon_button(
            &control_paint,
            Icon::More,
            Message::Command(Command::Options),
            options_open,
            Key::Options.text(app.lang),
            HEADER_CONTROL
        ),
        Space::new().width(2),
        close,
    ]
    .spacing(SP2)
    .align_y(Alignment::Center);
    let bar = container(row![maximize, Space::new().width(Fill), controls].align_y(Alignment::Center))
        .padding(Padding::from([0.0, SP2]))
        .height(Fill)
        .align_y(Alignment::Center);
    let corners = if app.edge_to_edge() { Radius::default() } else { Radius::default().top_left(R3).top_right(R3) };
    let background = if app.window_focused() { paint.bar_bg() } else { paint.bar_bg_idle() };
    let bar = container(bar).width(Fill).height(HEADER).style(move |_| style::rounded(background, corners));
    mouse_area(bar)
        .on_press(Message::WindowDrag)
        .on_double_click(Message::WindowMaximize)
        .on_right_press(Message::HeaderMenu)
        .into()
}

fn tab_button<'a>(app: &'a App, index: usize, tab: &'a Tab) -> Element<'a, Message> {
    let paint = app.paint;
    let active = index == app.active;
    let hovered = app.hovered_tab == Some(index);
    let mut label = row![text(app.tab_title(tab), TextStyle::CAP).wrapping(text::Wrapping::None)]
        .spacing(SP2)
        .align_y(Alignment::Center);
    if tab.bell {
        label = label.push(glyph(Icon::Bell, 12.0, paint.warn()));
    } else if tab.activity {
        label = label.push(
            container(Space::new().width(6).height(6))
                .style(move |_| style::rounded(paint.fg_3(), glide_theme::shape::R_ROUND)),
        );
    }
    let close: Element<'a, Message> = if hovered {
        let control = button(container(glyph(Icon::Close, 11.0, paint.fg_2())).center(Fill))
            .on_press(Message::Command(Command::CloseTab(index)))
            .width(CONTROL)
            .height(CONTROL)
            .padding(0);
        tip(
            &paint,
            widgets::button(control, move |status| style::action(&paint, Tone::Ghost, status)),
            Key::CloseTab.text(app.lang),
        )
    } else {
        Space::new().width(CONTROL).height(CONTROL).into()
    };
    let inner = stack![
        container(label).center_x(Fill).center_y(Fill).padding(Padding::from([0, 26])).clip(true),
        container(close).padding(Padding::from([0.0, SP2])).height(Fill).align_y(Alignment::Center),
    ];
    let lifted = app.dragged_tab() == Some(index);
    let tab = widgets::container(container(inner).width(Fill).height(Fill), move |hover, press| {
        let (background, text) = if active {
            (paint.fill_2(), paint.fg_1())
        } else {
            (
                paint.layer(Color::TRANSPARENT, ST_HOVER * hover + (glide_theme::motion::ST_PRESS - ST_HOVER) * press),
                if hover > 0.5 { paint.fg_1() } else { paint.fg_2() },
            )
        };
        container::Style { text_color: Some(text), ..style::rounded(background, R2) }
    });
    let tab =
        if active { widgets::surface(tab, &paint, if lifted { Level::El2 } else { Level::El1 }, R2) } else { tab };
    mouse_area(tab)
        .on_press(Message::TabPressed(index))
        .on_enter(Message::TabHovered(Some(index)))
        .on_exit(Message::TabHovered(None))
        .on_middle_press(Message::Command(Command::CloseTab(index)))
        .into()
}

#[must_use]
pub fn tab_slot(x: f32, width: f32, count: usize) -> usize {
    let slot = (width - 2.0 * TAB_PAD + TAB_GAP) / count.max(1) as f32;
    (((x - TAB_PAD) / slot.max(1.0)).floor().max(0.0) as usize).min(count.saturating_sub(1))
}

fn tab_bar(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let tabs: Vec<Element<'_, Message>> =
        app.tabs.iter().enumerate().map(|(index, tab)| tab_button(app, index, tab)).collect();
    let bar = container(iced::widget::row(tabs).spacing(TAB_GAP).height(Fill))
        .padding(Padding::from([SP1, TAB_PAD]))
        .width(Fill)
        .height(TABS)
        .style(move |_| style::rounded(paint.bar_bg(), 0.0));
    mouse_area(bar).on_move(|point| Message::TabMoved(point.x)).into()
}

fn find_bar(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let Some(find) = &app.find else { return Space::new().into() };
    let field = widgets::field(
        container(
            row![
                glyph(Icon::Search, 14.0, paint.fg_3()),
                text_input(Key::FindPlaceholder.text(app.lang), &find.query)
                    .id(FIND_ID)
                    .on_input(Message::FindInput)
                    .on_submit(Message::FindSubmit)
                    .size(TextStyle::BODY.size_px)
                    .padding(Padding::from([SP1, 0.0]))
                    .style(move |_, status| text_input::Style {
                        background: Background::Color(Color::TRANSPARENT),
                        border: Border::default(),
                        ..style::field(&paint, status)
                    }),
            ]
            .spacing(SP2)
            .align_y(Alignment::Center),
        )
        .padding(Padding::from([0.0, SP2]))
        .width(if app.window.width < 680.0 { Length::Fill } else { Length::Fixed(300.0) })
        .height(component::CONTROL)
        .align_y(Alignment::Center),
        &paint,
    );
    let count = if find.query.is_empty() {
        String::new()
    } else if find.counting && find.index == 0 {
        String::from("…")
    } else if find.total == 0 {
        Key::NoMatches.text(app.lang).to_string()
    } else {
        format!("{} / {}", find.index, find.total)
    };
    let case = button(text("Aa", TextStyle::CAP))
        .on_press(Message::FindCase)
        .height(CONTROL)
        .padding(Padding::from([SP1, SP2]));
    let case = tip(
        &paint,
        widgets::button(case, move |status| style::toggle(&paint, find.case, status)),
        Key::CaseSensitive.text(app.lang),
    );
    let done = button(text(Key::Done.text(app.lang), TextStyle::CALL))
        .on_press(Message::FindClose)
        .height(CONTROL)
        .padding(Padding::from([SP1, SP3]));
    let done = widgets::button(done, move |status| style::action(&paint, Tone::Neutral, status));
    let controls = row![
        row![
            icon_button(
                &paint,
                Icon::Up,
                Message::Command(Command::FindUp),
                false,
                Key::FindPrevious.text(app.lang),
                CONTROL
            ),
            icon_button(
                &paint,
                Icon::Down,
                Message::Command(Command::FindDown),
                false,
                Key::FindNext.text(app.lang),
                CONTROL
            ),
        ]
        .spacing(SP1),
        case,
        text(count, TextStyle::NUM).color(paint.fg_3()),
        space::horizontal(),
        done,
    ]
    .spacing(SP2)
    .align_y(Alignment::Center)
    .width(Fill);
    let contents: Element<'_, Message> = if app.window.width < 680.0 {
        column![field, controls].spacing(SP1).into()
    } else {
        row![field, controls].spacing(SP2).align_y(Alignment::Center).into()
    };
    container(contents)
        .padding(Padding::from([0.0, SP3]))
        .width(Fill)
        .height(find_height(app))
        .align_y(Alignment::Center)
        .style(move |_| style::rounded(paint.bar_bg(), 0.0))
        .into()
}

fn find_height(app: &App) -> f32 {
    if app.window.width < 680.0 { 84.0 } else { FIND }
}

#[must_use]
pub fn pane_at(app: &App, (x, y): (f32, f32)) -> Option<pane_grid::Pane> {
    let tab = app.tab()?;
    let mut top = HEADER + 1.0;
    if app.tabs.len() > 1 {
        top += TABS + 1.0;
    }
    if app.find.is_some() {
        top += find_height(app) + 1.0;
    }
    let size = Size::new(app.window.width, (app.window.height - top).max(0.0));
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
            Some(error) => column![
                text(Key::SpawnFailed.text(app.lang), TextStyle::HEAD).color(color(palette.foreground, 1.0)),
                text(error.clone(), TextStyle::CALL).color(paint.fg_2()),
            ]
            .spacing(SP2)
            .align_x(Alignment::Center)
            .into(),
            None => Space::new().into(),
        };
        return container(notice)
            .center(Fill)
            .style(move |_| style::rounded(color(palette.background, palette.opacity), bottom))
            .into();
    };
    let look = app.look(tab.profile);
    let regions = tab.panes.layout().pane_regions(0.0, 0.0, Size::new(1.0, 1.0));
    let blink = app.blink_visible();
    let pattern = app.find.as_ref().map(|find| find.pattern.as_str());
    let target = app.drop_at.and_then(|at| pane_at(app, at));
    let grid = pane_grid(&tab.panes, |handle, pane, _| {
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
        pane_grid::Content::new(Element::from(terminal).map(Message::Terminal))
    })
    .spacing(1)
    .min_size(PANE_MIN)
    .on_click(Message::PaneClicked)
    .on_resize(6, Message::SplitResized)
    .style(move |_| {
        let split = pane_grid::Line { color: paint.acc_bg(), width: 2.0 };
        pane_grid::Style {
            hovered_region: pane_grid::Highlight {
                background: Background::Color(paint.acc_soft()),
                border: Border { color: paint.acc_bg(), width: 1.0, radius: R2.into() },
            },
            picked_split: split,
            hovered_split: split,
        }
    });
    let backdrop = if tab.panes.len() > 1 { paint.divider() } else { Color::TRANSPARENT };
    container(grid).width(Fill).height(Fill).style(move |_| style::rounded(backdrop, bottom)).into()
}

fn item<'a>(
    app: &App, paint: &Paint, label: &'a str, hint: Option<&'a str>, message: Option<Message>,
) -> Element<'a, Message> {
    let paint = *paint;
    let selected = message.as_ref().is_some_and(|message| menu_selected(app, message));
    let index = message_index(app, message.as_ref());
    let mut content = row![text(label, TextStyle::MENU_ITEM), Space::new().width(Fill)].align_y(Alignment::Center);
    if let Some(hint) = hint {
        content = content.push(text(hint, TextStyle::CAP).font(crate::MONO));
    }
    let control = button(content).on_press_maybe(message).width(Fill).height(ITEM).padding(Padding::from([0.0, SP3]));
    let control = widgets::button(control, move |status| {
        let mut style = style::menu_item(&paint, status);
        if selected {
            style.border = Border { color: paint.acc_bg(), width: 2.0, radius: R2.into() };
        }
        style
    });
    if let Some(index) = index { mouse_area(control).on_enter(Message::MenuHovered(index)).into() } else { control }
}

fn command<'a>(app: &App, paint: &Paint, key: Key, command: Command, enabled: bool) -> Element<'a, Message> {
    let label = key.text(app.lang);
    item(app, paint, label, shortcut_label(command), enabled.then_some(Message::Command(command)))
}

fn font_size(app: &App, paint: &Paint) -> Element<'static, Message> {
    let paint = *paint;
    let step = |label: &'static str, command: Command| {
        let selected = menu_selected(app, &Message::Command(command));
        let control = button(container(text(label, TextStyle::BODY)).center(Fill))
            .on_press(Message::Command(command))
            .width(CONTROL)
            .height(CONTROL)
            .padding(0);
        widgets::button(control, move |status| {
            let mut style = style::action(&paint, Tone::Neutral, status);
            if selected {
                style.border = Border { color: paint.acc_bg(), width: 2.0, radius: R2.into() };
            }
            style
        })
    };
    let size = text(format!("{:.0}", app.fonts().size), TextStyle::NUM).width(CONTROL).align_x(Alignment::Center);
    row![
        text(Key::FontSize.text(app.lang), TextStyle::CALL),
        Space::new().width(Fill),
        step("−", Command::Smaller),
        size,
        step("+", Command::Bigger),
    ]
    .spacing(SP1)
    .padding(Padding::from([0.0, SP3]).right(SP2))
    .height(ITEM)
    .align_y(Alignment::Center)
    .into()
}

fn separator<'a>(paint: &Paint) -> Element<'a, Message> {
    container(rule(paint)).padding(Padding::from([SP1, SP2])).into()
}

fn card<'a>(paint: &Paint, content: impl Into<Element<'a, Message>>, width: f32) -> Element<'a, Message> {
    let paint = *paint;
    opaque(widgets::surface(
        container(content).padding(SP1).width(width).style(move |_| style::popover(&paint)),
        &paint,
        Level::El3,
        R3,
    ))
}

fn menu_entries(app: &App, paint: &Paint, kind: &MenuKind) -> (Vec<Element<'static, Message>>, f32) {
    let has_selection = app
        .focused()
        .is_some_and(|pane| pane.term.lock().selection.as_ref().is_some_and(|selection| !selection.is_empty()));
    let mut entries: Vec<Element<'static, Message>> = Vec::new();
    let mut height = 10.0;
    let mut push = |entry: Element<'static, Message>, size: f32| {
        entries.push(entry);
        height += size;
    };
    let sep = ITEM / 3.0;
    match kind {
        MenuKind::Window => {
            push(item(app, paint, Key::Maximize.text(app.lang), None, Some(Message::WindowMaximize)), ITEM);
            push(item(app, paint, Key::Minimize.text(app.lang), None, Some(Message::WindowMinimize)), ITEM);
            push(command(app, paint, Key::CloseWindow, Command::CloseWindow, true), ITEM);
        }
        MenuKind::Options => {
            push(command(app, paint, Key::NewWindow, Command::NewWindow, true), ITEM);
            push(command(app, paint, Key::NewTab, Command::NewTab, true), ITEM);
            push(command(app, paint, Key::SplitDown, Command::SplitDown, true), ITEM);
            push(command(app, paint, Key::SplitRight, Command::SplitRight, true), ITEM);
            push(separator(paint), sep);
            push(command(app, paint, Key::Find, Command::Find, true), ITEM);
            push(command(app, paint, Key::OpenHints, Command::Hints, true), ITEM);
            push(command(app, paint, Key::ExportText, Command::Export, true), ITEM);
            push(command(app, paint, Key::ClearScrollback, Command::ClearScrollback, true), ITEM);
            push(command(app, paint, Key::Reset, Command::Reset, true), ITEM);
            push(separator(paint), sep);
            push(command(app, paint, Key::MarkLine, Command::Mark(Mark::Line), true), ITEM);
            push(command(app, paint, Key::Bookmark, Command::Mark(Mark::Bookmark), true), ITEM);
            push(command(app, paint, Key::PreviousMark, Command::Jump(Direction::Left, false), true), ITEM);
            push(command(app, paint, Key::NextMark, Command::Jump(Direction::Right, false), true), ITEM);
            push(separator(paint), sep);
            push(command(app, paint, Key::Profiles, Command::Profiles, true), ITEM);
            push(font_size(app, paint), ITEM);
            push(command(app, paint, Key::FullScreen, Command::FullScreen, true), ITEM);
        }
        MenuKind::Context(link) => {
            push(command(app, paint, Key::Copy, Command::Copy, has_selection), ITEM);
            push(command(app, paint, Key::Paste, Command::Paste, true), ITEM);
            push(command(app, paint, Key::SelectAll, Command::SelectAll, true), ITEM);
            if app.man_topic().is_some() {
                push(separator(paint), sep);
                push(command(app, paint, Key::ManPage, Command::ManPage, true), ITEM);
            }
            if let Some(link) = link {
                push(separator(paint), sep);
                let open = Some(Message::Open(link.clone()));
                push(item(app, paint, Key::OpenLink.text(app.lang), None, open), ITEM);
                let copy = Some(Message::CopyText(link.clone()));
                push(item(app, paint, Key::CopyLink.text(app.lang), None, copy), ITEM);
            }
            push(separator(paint), sep);
            push(command(app, paint, Key::SplitDown, Command::SplitDown, true), ITEM);
            push(command(app, paint, Key::SplitRight, Command::SplitRight, true), ITEM);
            push(command(app, paint, Key::ClosePane, Command::ClosePane, true), ITEM);
            push(separator(paint), sep);
            push(command(app, paint, Key::ClearScrollback, Command::ClearScrollback, true), ITEM);
            push(command(app, paint, Key::Profiles, Command::Profiles, true), ITEM);
        }
        MenuKind::Profiles => {}
    }
    (entries, height)
}

pub fn menu_actions(app: &App, kind: &MenuKind) -> Vec<Message> {
    let commands: Vec<Command> = match kind {
        MenuKind::Options => vec![
            Command::NewWindow,
            Command::NewTab,
            Command::SplitDown,
            Command::SplitRight,
            Command::Find,
            Command::Hints,
            Command::Export,
            Command::ClearScrollback,
            Command::Reset,
            Command::Mark(Mark::Line),
            Command::Mark(Mark::Bookmark),
            Command::Jump(Direction::Left, false),
            Command::Jump(Direction::Right, false),
            Command::Profiles,
            Command::Smaller,
            Command::Bigger,
            Command::FullScreen,
        ],
        MenuKind::Profiles => {
            let mut actions: Vec<_> =
                Profile::ALL.into_iter().map(|profile| Message::Command(Command::SetProfile(profile))).collect();
            if app.tab().is_some_and(|tab| tab.profile != app.prefs.profile) {
                actions.push(Message::Command(Command::MakeDefault));
            }
            return actions;
        }
        MenuKind::Window => {
            return vec![Message::WindowMaximize, Message::WindowMinimize, Message::Command(Command::CloseWindow)];
        }
        MenuKind::Context(link) => {
            let mut actions = Vec::new();
            if app
                .focused()
                .is_some_and(|pane| pane.term.lock().selection.as_ref().is_some_and(|selection| !selection.is_empty()))
            {
                actions.push(Message::Command(Command::Copy));
            }
            actions.extend([Message::Command(Command::Paste), Message::Command(Command::SelectAll)]);
            if app.man_topic().is_some() {
                actions.push(Message::Command(Command::ManPage));
            }
            if let Some(link) = link {
                actions.extend([Message::Open(link.clone()), Message::CopyText(link.clone())]);
            }
            actions.extend(
                [
                    Command::SplitDown,
                    Command::SplitRight,
                    Command::ClosePane,
                    Command::ClearScrollback,
                    Command::Profiles,
                ]
                .map(Message::Command),
            );
            return actions;
        }
    };
    commands.into_iter().map(Message::Command).collect()
}

fn menu_selected(app: &App, message: &Message) -> bool {
    let Some(menu) = &app.menu else { return false };
    let Some(selected) = menu.selected else { return false };
    let actions = menu_actions(app, &menu.kind);
    match (actions.get(selected), message) {
        (Some(Message::Command(a)), Message::Command(b)) => a == b,
        (Some(Message::Open(a)), Message::Open(b)) | (Some(Message::CopyText(a)), Message::CopyText(b)) => a == b,
        (Some(Message::WindowMaximize), Message::WindowMaximize)
        | (Some(Message::WindowMinimize), Message::WindowMinimize) => true,
        _ => false,
    }
}

fn message_index(app: &App, message: Option<&Message>) -> Option<usize> {
    let menu = app.menu.as_ref()?;
    let message = message?;
    menu_actions(app, &menu.kind).iter().position(|action| match (action, message) {
        (Message::Command(a), Message::Command(b)) => a == b,
        (Message::Open(a), Message::Open(b)) | (Message::CopyText(a), Message::CopyText(b)) => a == b,
        (Message::WindowMaximize, Message::WindowMaximize) | (Message::WindowMinimize, Message::WindowMinimize) => true,
        _ => false,
    })
}

fn swatch(app: &App, paint: &Paint, profile: Profile, current: bool) -> Element<'static, Message> {
    let paint = *paint;
    let palette = profile.palette(&paint.tokens);
    let sample = row![
        text("Aa", TextStyle::BODY).font(crate::MONO).color(color(palette.foreground, paint.alpha)),
        Space::new().width(Fill),
        row([1, 2, 3, 4].map(|index| {
            let tint = color(palette.ansi[index], paint.alpha);
            container(Space::new().width(6).height(6))
                .style(move |_| style::rounded(tint, glide_theme::shape::R_ROUND))
                .into()
        }))
        .spacing(SP1),
    ]
    .align_y(Alignment::Center);
    let preview = container(sample)
        .padding(Padding::from([0.0, SP3]))
        .width(Fill)
        .height(SWATCH.height)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(Background::Color(color(palette.background, paint.alpha))),
            border: Border {
                color: if current { paint.acc_bg() } else { paint.line() },
                width: if current { 2.0 } else { 1.0 },
                radius: R2.into(),
            },
            ..container::Style::default()
        });
    let mut name = profile.name().to_string();
    if profile == app.prefs.profile {
        name = format!("{name} · {}", Key::DefaultProfile.text(app.lang));
    }
    let selected = menu_selected(app, &Message::Command(Command::SetProfile(profile)));
    let control = button(column![preview, text(name, TextStyle::CAP).wrapping(text::Wrapping::None)].spacing(SP1))
        .on_press(Message::Command(Command::SetProfile(profile)))
        .width(SWATCH.width)
        .padding(0);
    widgets::button(control, move |status| {
        let lit = current || matches!(status, button::Status::Hovered | button::Status::Pressed);
        let mut style = style::flat(Color::TRANSPARENT, if lit { paint.fg_1() } else { paint.fg_2() }, R2);
        if selected {
            style.border = Border { color: paint.acc_bg(), width: 2.0, radius: R2.into() };
        }
        style
    })
}

fn profiles(app: &App, paint: &Paint) -> (Element<'static, Message>, Size) {
    let paint = *paint;
    let current = app.tab().map_or(app.prefs.profile, |tab| tab.profile);
    let rows = Profile::ALL.chunks(3).map(|chunk| {
        iced::widget::row(chunk.iter().map(|profile| swatch(app, &paint, *profile, *profile == current)))
            .spacing(SP2)
            .into()
    });
    let grid = iced::widget::column(rows).spacing(SP3);
    let selected = menu_selected(app, &Message::Command(Command::MakeDefault));
    let make_default = button(text(Key::MakeDefault.text(app.lang), TextStyle::CALL))
        .on_press_maybe((current != app.prefs.profile).then_some(Message::Command(Command::MakeDefault)))
        .height(component::CONTROL)
        .padding(Padding::from([SP1, SP3]));
    let make_default = widgets::button(make_default, move |status| {
        let mut style = style::action(&paint, Tone::Neutral, status);
        if selected {
            style.border = Border { color: paint.acc_bg(), width: 2.0, radius: R2.into() };
        }
        style
    });
    let title = text(Key::Profiles.text(app.lang).trim_end_matches('…').to_string(), TextStyle::HEAD);
    let body = column![title, grid, row![Space::new().width(Fill), make_default].align_y(Alignment::Center)]
        .spacing(SP3)
        .padding(SP2);
    let width = SWATCH.width * 3.0 + 16.0 + 26.0;
    let height = 4.0 * (SWATCH.height + 20.0) + 3.0 * 10.0 + 12.0 + 30.0 + 26.0;
    let height = height.min((app.window.height - 24.0).max(1.0));
    (card(&paint, scrollable(body).id("terminal-menu").height(height - 8.0), width), Size::new(width, height))
}

fn menu_layer<'a>(app: &'a App, menu: &'a Menu) -> Element<'a, Message> {
    let progress = menu.reveal.value(glide_theme::motion::t2()) as f32;
    let paint = app.paint.faded(progress);
    let (card, size) = match &menu.kind {
        MenuKind::Profiles => profiles(app, &paint),
        kind => {
            let (entries, height) = menu_entries(app, &paint, kind);
            let height = height.min((app.window.height - 24.0).max(1.0));
            (
                card(
                    &paint,
                    scrollable(iced::widget::column(entries)).id("terminal-menu").height(height - 8.0),
                    MENU_WIDTH,
                ),
                Size::new(MENU_WIDTH, height),
            )
        }
    };
    let window = app.window;
    let (x, y) = match menu.kind {
        MenuKind::Context(_) | MenuKind::Window => (menu.at.x, menu.at.y),
        _ => (window.width - size.width - 8.0, HEADER - 2.0),
    };
    let x = x.min(window.width - size.width - 8.0).max(8.0);
    let y = y.min(window.height - size.height - SP2).max(SP2) + (1.0 - progress) * component::POPOVER_RISE;
    stack![
        mouse_area(Space::new().width(Fill).height(Fill))
            .on_press(Message::Dismiss)
            .on_right_press(Message::Dismiss)
            .on_middle_press(Message::Dismiss),
        pin(widgets::scale(card, component::POPOVER_CLOSED_SCALE + (1.0 - component::POPOVER_CLOSED_SCALE) * progress))
            .x(x)
            .y(y),
    ]
    .into()
}

fn dialog<'a>(app: &'a App, confirm: &'a Confirm) -> Element<'a, Message> {
    let progress = confirm.reveal.value(glide_theme::motion::t2()) as f32;
    let paint = app.paint.faded(progress);
    let body = format!("{}: {}.", Key::ConfirmBody.text(app.lang), confirm.processes.join(", "));
    let cancel = button(text(Key::Cancel.text(app.lang), TextStyle::CALL))
        .on_press(Message::Confirm(false))
        .height(component::CONTROL)
        .padding(Padding::from([SP2, SP4]));
    let cancel = widgets::button(cancel, move |status| {
        let mut style = style::action(&paint, Tone::Neutral, status);
        if !confirm.accept_focused {
            style.border = Border { color: paint.acc_bg(), width: 2.0, radius: R2.into() };
        }
        style
    });
    let terminate = button(text(Key::Terminate.text(app.lang), TextStyle::CALL))
        .on_press(Message::Confirm(true))
        .height(component::CONTROL)
        .padding(Padding::from([SP2, SP4]));
    let terminate = widgets::button(terminate, move |status| {
        let mut style = style::action(&paint, Tone::Danger, status);
        if confirm.accept_focused {
            style.border = Border { color: paint.acc_bg(), width: 2.0, radius: R2.into() };
        }
        style
    });
    let card = container(
        column![
            text(Key::ConfirmTitle.text(app.lang), TextStyle::T3),
            text(body, TextStyle::CALL).color(paint.fg_2()),
            row![Space::new().width(Fill), cancel, terminate].spacing(SP2),
        ]
        .spacing(SP3),
    )
    .padding(SP4)
    .width(380)
    .max_width(app.window.width - SP6)
    .style(move |_| style::popover(&paint));
    let scrim = Color { a: 0.32 * progress, ..Color::BLACK };
    stack![
        opaque(container(Space::new().width(Fill).height(Fill)).style(move |_| style::rounded(scrim, R3))),
        container(widgets::scale(
            opaque(widgets::surface(card, &paint, Level::El3, R3)),
            component::DIALOG_CLOSED_SCALE + (1.0 - component::DIALOG_CLOSED_SCALE) * progress
        ))
        .center(Fill),
    ]
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let mut body = column![header(app)];
    if app.tabs.len() > 1 {
        body = body.push(rule(&paint)).push(tab_bar(app));
    }
    if app.find.is_some() {
        body = body.push(rule(&paint)).push(find_bar(app));
    }
    body = body.push(rule(&paint)).push(content(app));
    let mut layers: Vec<Element<'_, Message>> = vec![body.into()];
    if !app.edge_to_edge() {
        layers.push(resize_edges(Message::WindowResize));
    }
    if let Some(error) = app.error.as_ref().filter(|_| !app.tabs.is_empty()) {
        let error = container(text(error.as_str(), TextStyle::CALL).color(paint.danger()))
            .padding(SP3)
            .max_width(app.window.width - 48.0)
            .style(move |_| style::popover(&paint));
        layers.push(
            container(widgets::surface(error, &paint, Level::El3, R3))
                .width(Fill)
                .height(Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::End)
                .padding(SP5)
                .into(),
        );
    }
    if let Some((text, at)) = &app.notice {
        layers.push(notice(app, text, *at));
    }
    if let Some(menu) = &app.menu {
        layers.push(menu_layer(app, menu));
    }
    if let Some(confirm) = app.confirm.as_ref().or(app.closing_confirm.as_ref()) {
        layers.push(dialog(app, confirm));
    }
    iced::widget::stack(layers).into()
}

fn notice<'a>(app: &'a App, message: &'a str, at: std::time::Instant) -> Element<'a, Message> {
    let progress = App::reveal(at);
    let paint = app.paint.faded(progress);
    let pill = container(text(message, TextStyle::CALL).wrapping(text::Wrapping::None))
        .padding(Padding::from([SP2, SP4]))
        .max_width(app.window.width - 48.0)
        .clip(true)
        .style(move |_| style::popover(&paint));
    container(widgets::surface(pill, &paint, Level::El3, R3))
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::End)
        .padding(Padding::from([0.0, SP5]).bottom(SP5 + (1.0 - progress) * component::POPOVER_RISE))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dragged_tab_lands_in_the_slot_under_the_pointer_and_never_outside_the_strip() {
        let width = 2.0 * TAB_PAD + 4.0 * 197.0 + 3.0 * TAB_GAP;
        assert_eq!(tab_slot(TAB_PAD + 1.0, width, 4), 0);
        assert_eq!(tab_slot(TAB_PAD + 197.0 + TAB_GAP + 1.0, width, 4), 1);
        assert_eq!(tab_slot(width - TAB_PAD - 1.0, width, 4), 3);
        assert_eq!(tab_slot(-40.0, width, 4), 0);
        assert_eq!(tab_slot(width + 400.0, width, 4), 3);
        assert_eq!(tab_slot(100.0, width, 0), 0);
    }

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
        app.window = Size::new(420.0, 260.0);
        app.find = Some(crate::app::Find::default());
        let mut view = find_bar(&app);
        let mut tree = Tree::new(&view);
        let renderer = iced::Renderer::Secondary(iced_tiny_skia::Renderer::new(crate::MONO, iced::Pixels(13.0)));
        let node =
            view.as_widget_mut().layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, Size::new(420.0, 84.0)));
        inside(&node, 0.0, 420.0);
    }
}

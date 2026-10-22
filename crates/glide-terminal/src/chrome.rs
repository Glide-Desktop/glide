use alacritty_terminal::index::Direction;
use glide_theme::elevation::Level;
use glide_theme::motion::ST_HOVER;
use iced::border::Radius;
use iced::widget::{Space, button, column, container, mouse_area, opaque, pane_grid, row, space, stack, text_input};
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
use glide_sdk::widget::{icon_button, separator as rule, tooltip as tip};
use glide_sdk::window::resize_edges;
use glide_theme::{TextStyle, component};

pub const HEADER: f32 = 35.0;
const TABS: f32 = component::LARGE_CONTROL;
const TAB_PAD: f32 = SP2;
const TAB_GAP: f32 = SP1;
const FIND: f32 = component::LARGE_CONTROL;
const CONTROL: f32 = component::SMALL_CONTROL;
const MENU_WIDTH: f32 = 264.0;
const PANE_MIN: f32 = 50.0;
const SWATCH: Size = Size::new(96.0, 44.0);

fn glyph<'a>(icon: Icon, size: f32, tint: Color) -> Element<'a, Message> {
    crate::icons::glyph(icon, size, tint)
}

fn sdk_window(app: &App) -> glide_sdk::window::Window {
    use glide_sdk::window::{Frame, Window};
    Window {
        size: app.window,
        focused: app.window_focused(),
        frame: if app.edge_to_edge() { Frame::Maximized } else { Frame::Floating },
    }
}

fn window_message(action: glide_sdk::window::Action) -> Message {
    use glide_sdk::window::Action;
    match action {
        Action::Drag => Message::WindowDrag,
        Action::ToggleMaximize => Message::WindowMaximize,
        Action::Minimize => Message::WindowMinimize,
        Action::Resize(direction) => Message::WindowResize(direction),
        Action::Resized(_) | Action::Focused(_) | Action::Framed(_) => Message::Frame,
    }
}

fn header(app: &App) -> Element<'_, Message> {
    let window = sdk_window(app);
    let controls = window.controls_paint(&app.paint);
    let options_open =
        app.menu.as_ref().is_some_and(|menu| matches!(menu.kind, MenuKind::Options | MenuKind::Profiles));
    let new_tab = Some(Message::Command(Command::NewTab));
    let options = Some(Message::Command(Command::Options));
    let header = glide_sdk::window::Header::new("")
        .labels(Key::Maximize.text(app.lang), Key::CloseWindow.text(app.lang))
        .height(HEADER)
        .context(Message::HeaderMenu)
        .trailing(icon_button(&controls, Icon::Plus, Key::NewTab.text(app.lang), new_tab, false))
        .trailing(icon_button(&controls, Icon::More, Key::Options.text(app.lang), options, options_open));
    glide_sdk::window::header(&app.paint, &window, header, window_message, Message::Command(Command::CloseWindow), 0.0)
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
                Key::FindPrevious.text(app.lang),
                Some(Message::Command(Command::FindUp)),
                false
            ),
            icon_button(
                &paint,
                Icon::Down,
                Key::FindNext.text(app.lang),
                Some(Message::Command(Command::FindDown)),
                false
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
            MenuItem::action(Key::Maximize.text(app.lang), Message::WindowMaximize),
            MenuItem::action(Key::Minimize.text(app.lang), Message::WindowMinimize),
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
        (Some(Message::WindowMaximize), Message::WindowMaximize)
        | (Some(Message::WindowMinimize), Message::WindowMinimize) => true,
        _ => false,
    }
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
    let progress = menu.reveal.value(glide_theme::motion::t2()) as f32;
    let paint = app.paint.faded(progress);
    let (rows, size) = match &menu.kind {
        MenuKind::Profiles => {
            let (body, size) = profiles(app, &paint);
            (vec![body], size)
        }
        _ => menu_rows(app, &paint, menu),
    };
    let window = app.window;
    let (anchor, may_flip) = match menu.kind {
        MenuKind::Context(_) | MenuKind::Window => (menu.at, true),
        _ => (iced::Point::new(window.width - size.width - 8.0, HEADER - 2.0), false),
    };
    let (at, size) = glide_sdk::menu::place(anchor, size.height, size.width, window, may_flip);
    glide_sdk::menu::layer(&paint, rows, at, size, progress, Some(Message::Dismiss))
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

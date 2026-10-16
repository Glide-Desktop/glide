use alacritty_terminal::index::Direction;
use glide_theme::elevation::Level;
use glide_theme::motion::ST_HOVER;
use iced::border::Radius;
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, pane_grid, pin, row, space, stack, svg, text, text_input,
};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding, Rectangle, Size, mouse, window};

use crate::app::{App, Command, Confirm, FIND_ID, Menu, MenuKind, Message, Tab, shortcut_label};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::marks::Mark;
use crate::profile::{Profile, color};
use crate::style::{self, Paint, R2, R3, SEMIBOLD, Tone};
use crate::terminal::Terminal;

pub const HEADER: f32 = 40.0;
const TABS: f32 = 34.0;
const TAB_PAD: f32 = 8.0;
const TAB_GAP: f32 = 4.0;
const FIND: f32 = 42.0;
const CONTROL: f32 = 26.0;
const MENU_WIDTH: f32 = 264.0;
const ITEM: f32 = 26.0;
const SWATCH: Size = Size::new(96.0, 44.0);

fn glyph<'a>(icon: Icon, size: f32, tint: Color) -> Element<'a, Message> {
    svg(icon.handle()).width(size).height(size).style(move |_, _| style::glyph(tint)).into()
}

fn icon_button<'a>(paint: &Paint, icon: Icon, message: Message, on: bool) -> Element<'a, Message> {
    let paint = *paint;
    button(container(glyph(icon, 16.0, if on { paint.fg_1() } else { paint.fg_2() })).center(Fill))
        .on_press(message)
        .width(CONTROL)
        .height(CONTROL)
        .padding(0)
        .style(move |_, status| {
            if on { style::flat(paint.fill_2(), paint.fg_1(), R2) } else { style::action(&paint, Tone::Ghost, status) }
        })
        .into()
}

fn rule<'a>(paint: &Paint) -> Element<'a, Message> {
    let paint = *paint;
    container(Space::new().width(Fill).height(1)).style(move |_| style::rounded(paint.hairline(), 0.0)).into()
}

fn header(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let title = text(app.title()).size(13).font(SEMIBOLD).color(paint.fg_1()).wrapping(text::Wrapping::None);
    let options_open =
        app.menu.as_ref().is_some_and(|menu| matches!(menu.kind, MenuKind::Options | MenuKind::Profiles));
    let close = button(container(glyph(Icon::Close, 14.0, paint.fg_2())).center(Fill))
        .on_press(Message::Command(Command::CloseWindow))
        .width(CONTROL)
        .height(CONTROL)
        .padding(0)
        .style(move |_, status| style::window_control(&paint, status));
    let controls = row![
        icon_button(&paint, Icon::Plus, Message::Command(Command::NewTab), false),
        icon_button(&paint, Icon::More, Message::Command(Command::Options), options_open),
        Space::new().width(2),
        close,
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let bar = stack![
        container(title).center_x(Fill).center_y(Fill).padding(Padding::from([0, 118])).clip(true),
        container(row![Space::new().width(Fill), controls].align_y(Alignment::Center))
            .padding(Padding::from([0, 8]))
            .height(Fill)
            .align_y(Alignment::Center),
    ];
    let corners = Radius::default().top_left(R3).top_right(R3);
    let bar = container(bar).width(Fill).height(HEADER).style(move |_| style::rounded(paint.bar_bg(), corners));
    mouse_area(bar).on_press(Message::WindowDrag).on_double_click(Message::WindowMaximize).into()
}

fn tab_button<'a>(app: &'a App, index: usize, tab: &'a Tab) -> Element<'a, Message> {
    let paint = app.paint;
    let active = index == app.active;
    let hovered = app.hovered_tab == Some(index);
    let mut label =
        row![text(app.tab_title(tab)).size(12.5).wrapping(text::Wrapping::None)].spacing(6).align_y(Alignment::Center);
    if tab.bell {
        label = label.push(glyph(Icon::Bell, 12.0, paint.warn()));
    } else if tab.activity {
        label =
            label.push(container(Space::new().width(6).height(6)).style(move |_| style::rounded(paint.fg_3(), 3.0)));
    }
    let close: Element<'a, Message> = if hovered {
        button(container(glyph(Icon::Close, 11.0, paint.fg_2())).center(Fill))
            .on_press(Message::Command(Command::CloseTab(index)))
            .width(18)
            .height(18)
            .padding(0)
            .style(move |_, status| style::action(&paint, Tone::Ghost, status))
            .into()
    } else {
        Space::new().width(18).height(18).into()
    };
    let inner = stack![
        container(label).center_x(Fill).center_y(Fill).padding(Padding::from([0, 26])).clip(true),
        container(close).padding(Padding::from([0, 6])).height(Fill).align_y(Alignment::Center),
    ];
    let lifted = app.dragged_tab() == Some(index);
    let tab = container(inner).width(Fill).height(Fill).style(move |_| {
        let (background, text, level) = match (active, hovered) {
            (true, _) => (paint.fill_2(), paint.fg_1(), Some(if lifted { Level::El2 } else { Level::El1 })),
            (false, true) => (paint.layer(Color::TRANSPARENT, ST_HOVER), paint.fg_1(), None),
            (false, false) => (Color::TRANSPARENT, paint.fg_2(), None),
        };
        container::Style {
            text_color: Some(text),
            shadow: level.map_or_else(Default::default, |level| paint.shadow(level)),
            ..style::rounded(background, R2)
        }
    });
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
        .padding(Padding::from([4.0, TAB_PAD]))
        .width(Fill)
        .height(TABS)
        .style(move |_| style::rounded(paint.bar_bg(), 0.0));
    mouse_area(bar).on_move(|point| Message::TabMoved(point.x)).into()
}

fn find_bar(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let Some(find) = &app.find else { return Space::new().into() };
    let field = container(
        row![
            glyph(Icon::Search, 14.0, paint.fg_3()),
            text_input(Key::FindPlaceholder.text(app.lang), &find.query)
                .id(FIND_ID)
                .on_input(Message::FindInput)
                .on_submit(Message::FindSubmit)
                .size(13)
                .padding(Padding::from([5, 0]))
                .style(move |_, status| text_input::Style {
                    background: Background::Color(Color::TRANSPARENT),
                    border: Border::default(),
                    ..style::field(&paint, status)
                }),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([0, 8]))
    .width(300)
    .height(28)
    .align_y(Alignment::Center)
    .style(move |_| style::rounded(paint.fill_1(), R2));
    let count = if find.query.is_empty() {
        String::new()
    } else if find.counting && find.index == 0 {
        String::from("…")
    } else if find.total == 0 {
        Key::NoMatches.text(app.lang).to_string()
    } else {
        format!("{} / {}", find.index, find.total)
    };
    let case = button(text("Aa").size(12.5).font(SEMIBOLD))
        .on_press(Message::FindCase)
        .padding(Padding::from([4, 8]))
        .style(move |_, status| style::toggle(&paint, find.case, status));
    let done = button(text(Key::Done.text(app.lang)).size(12.5))
        .on_press(Message::FindClose)
        .padding(Padding::from([5, 12]))
        .style(move |_, status| style::action(&paint, Tone::Neutral, status));
    container(
        row![
            field,
            row![
                icon_button(&paint, Icon::Up, Message::Command(Command::FindUp), false),
                icon_button(&paint, Icon::Down, Message::Command(Command::FindDown), false),
            ]
            .spacing(2),
            case,
            text(count).size(12).color(paint.fg_3()),
            space::horizontal(),
            done,
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([0, 10]))
    .width(Fill)
    .height(FIND)
    .align_y(Alignment::Center)
    .style(move |_| style::rounded(paint.bar_bg(), 0.0))
    .into()
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
    let bottom = Radius::default().bottom_left(R3).bottom_right(R3);
    let Some(tab) = app.tab() else {
        let palette = app.prefs.profile.palette(&paint.tokens);
        let notice: Element<'_, Message> = match &app.error {
            Some(error) => column![
                text(Key::SpawnFailed.text(app.lang)).size(14).font(SEMIBOLD).color(color(palette.foreground, 1.0)),
                text(error.clone()).size(12.5).color(color(palette.foreground, 0.7)),
            ]
            .spacing(6)
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
    let grid = pane_grid(&tab.panes, |handle, pane, _| {
        let focused = handle == tab.focus;
        let terminal = Terminal::new(pane, look, focused && app.window_focused())
            .blink(blink)
            .find(if focused { pattern } else { None })
            .hints(
                app.hint_mode
                    .as_ref()
                    .filter(|mode| mode.pane == pane.id)
                    .map(|mode| (mode.items.as_slice(), mode.typed.as_str())),
            )
            .flash(app.flash(pane.id))
            .corners(regions.get(&handle).copied().map_or_else(Radius::default, corners));
        pane_grid::Content::new(Element::from(terminal).map(Message::Terminal))
    })
    .spacing(1)
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
    container(grid).width(Fill).height(Fill).style(move |_| style::rounded(paint.divider(), bottom)).into()
}

fn item<'a>(paint: &Paint, label: &'a str, hint: Option<&'a str>, message: Option<Message>) -> Element<'a, Message> {
    let paint = *paint;
    let mut content = row![text(label).size(13), Space::new().width(Fill)].align_y(Alignment::Center);
    if let Some(hint) = hint {
        content = content.push(text(hint).size(11.5).font(crate::MONO));
    }
    button(content)
        .on_press_maybe(message)
        .width(Fill)
        .height(ITEM)
        .padding(Padding::from([0, 10]))
        .style(move |_, status| style::menu_item(&paint, status))
        .into()
}

fn command<'a>(app: &App, paint: &Paint, key: Key, command: Command, enabled: bool) -> Element<'a, Message> {
    let label = key.text(app.lang);
    item(paint, label, shortcut_label(command), enabled.then_some(Message::Command(command)))
}

fn font_size(app: &App, paint: &Paint) -> Element<'static, Message> {
    let paint = *paint;
    let step = |label: &'static str, command: Command| {
        button(container(text(label).size(14)).center(Fill))
            .on_press(Message::Command(command))
            .width(24)
            .height(22)
            .padding(0)
            .style(move |_, status| style::action(&paint, Tone::Neutral, status))
    };
    let size = text(format!("{:.0}", app.fonts().size)).size(12).font(crate::MONO).width(28).align_x(Alignment::Center);
    row![
        text(Key::FontSize.text(app.lang)).size(13),
        Space::new().width(Fill),
        step("−", Command::Smaller),
        size,
        step("+", Command::Bigger),
    ]
    .spacing(4)
    .padding(Padding::from([0, 10]).right(6))
    .height(ITEM)
    .align_y(Alignment::Center)
    .into()
}

fn separator<'a>(paint: &Paint) -> Element<'a, Message> {
    container(rule(paint)).padding(Padding::from([4, 6])).into()
}

fn card<'a>(paint: &Paint, content: impl Into<Element<'a, Message>>, width: f32) -> Element<'a, Message> {
    let paint = *paint;
    opaque(container(content).padding(5).width(width).style(move |_| style::popover(&paint)))
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
                push(item(paint, Key::OpenLink.text(app.lang), None, open), ITEM);
                let copy = Some(Message::CopyText(link.clone()));
                push(item(paint, Key::CopyLink.text(app.lang), None, copy), ITEM);
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

fn swatch(app: &App, paint: &Paint, profile: Profile, current: bool) -> Element<'static, Message> {
    let paint = *paint;
    let palette = profile.palette(&paint.tokens);
    let sample = row![
        text("Aa").size(15).font(crate::MONO).color(color(palette.foreground, 1.0)),
        Space::new().width(Fill),
        row([1, 2, 3, 4].map(|index| {
            let tint = color(palette.ansi[index], 1.0);
            container(Space::new().width(6).height(6)).style(move |_| style::rounded(tint, 3.0)).into()
        }))
        .spacing(3),
    ]
    .align_y(Alignment::Center);
    let preview = container(sample)
        .padding(Padding::from([0, 10]))
        .width(Fill)
        .height(SWATCH.height)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(Background::Color(color(palette.background, 1.0))),
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
    button(column![preview, text(name).size(11.5).wrapping(text::Wrapping::None)].spacing(4))
        .on_press(Message::Command(Command::SetProfile(profile)))
        .width(SWATCH.width)
        .padding(0)
        .style(move |_, status| {
            let lit = current || matches!(status, button::Status::Hovered | button::Status::Pressed);
            style::flat(Color::TRANSPARENT, if lit { paint.fg_1() } else { paint.fg_2() }, 0.0)
        })
        .into()
}

fn profiles(app: &App, paint: &Paint) -> (Element<'static, Message>, Size) {
    let paint = *paint;
    let current = app.tab().map_or(app.prefs.profile, |tab| tab.profile);
    let rows = Profile::ALL.chunks(3).map(|chunk| {
        iced::widget::row(chunk.iter().map(|profile| swatch(app, &paint, *profile, *profile == current)))
            .spacing(8)
            .into()
    });
    let grid = iced::widget::column(rows).spacing(10);
    let make_default = button(text(Key::MakeDefault.text(app.lang)).size(12.5))
        .on_press_maybe((current != app.prefs.profile).then_some(Message::Command(Command::MakeDefault)))
        .padding(Padding::from([5, 12]))
        .style(move |_, status| style::action(&paint, Tone::Neutral, status));
    let title = text(Key::Profiles.text(app.lang).trim_end_matches('…').to_string()).size(13).font(SEMIBOLD);
    let body = column![row![title, Space::new().width(Fill), make_default].align_y(Alignment::Center), grid]
        .spacing(12)
        .padding(8);
    let width = SWATCH.width * 3.0 + 16.0 + 26.0;
    let height = 4.0 * (SWATCH.height + 20.0) + 3.0 * 10.0 + 12.0 + 30.0 + 26.0;
    (card(&paint, body, width), Size::new(width, height))
}

fn menu_layer<'a>(app: &'a App, menu: &'a Menu) -> Element<'a, Message> {
    let progress = App::reveal(menu.opened);
    let paint = app.paint.faded(progress);
    let (card, size) = match &menu.kind {
        MenuKind::Profiles => profiles(app, &paint),
        kind => {
            let (entries, height) = menu_entries(app, &paint, kind);
            (card(&paint, iced::widget::column(entries), MENU_WIDTH), Size::new(MENU_WIDTH, height))
        }
    };
    let window = app.window;
    let (x, y) = match menu.kind {
        MenuKind::Context(_) => (menu.at.x, menu.at.y),
        _ => (window.width - size.width - 8.0, HEADER - 2.0),
    };
    let x = x.min(window.width - size.width - 8.0).max(8.0);
    let y = y.min(window.height - size.height - 8.0).max(8.0) + (1.0 - progress) * 4.0;
    stack![
        mouse_area(Space::new().width(Fill).height(Fill))
            .on_press(Message::Dismiss)
            .on_right_press(Message::Dismiss)
            .on_middle_press(Message::Dismiss),
        pin(card).x(x).y(y),
    ]
    .into()
}

fn dialog<'a>(app: &'a App, confirm: &'a Confirm) -> Element<'a, Message> {
    let progress = App::reveal(confirm.opened);
    let paint = app.paint.faded(progress);
    let body = format!("{}: {}.", Key::ConfirmBody.text(app.lang), confirm.processes.join(", "));
    let cancel = button(text(Key::Cancel.text(app.lang)).size(13))
        .on_press(Message::Confirm(false))
        .padding(Padding::from([6, 14]))
        .style(move |_, status| style::action(&paint, Tone::Neutral, status));
    let terminate = button(text(Key::Terminate.text(app.lang)).size(13))
        .on_press(Message::Confirm(true))
        .padding(Padding::from([6, 14]))
        .style(move |_, status| style::action(&paint, Tone::Danger, status));
    let card = container(
        column![
            text(Key::ConfirmTitle.text(app.lang)).size(15).font(SEMIBOLD),
            text(body).size(13).color(paint.fg_2()),
            row![Space::new().width(Fill), cancel, terminate].spacing(8),
        ]
        .spacing(12),
    )
    .padding(18)
    .width(380)
    .style(move |_| style::popover(&paint));
    let scrim = Color { a: 0.32 * progress, ..Color::BLACK };
    stack![
        opaque(container(Space::new().width(Fill).height(Fill)).style(move |_| style::rounded(scrim, R3))),
        container(opaque(card)).center(Fill),
    ]
    .into()
}

fn resize_handles<'a>() -> Element<'a, Message> {
    const EDGE: f32 = 5.0;
    const CORNER: f32 = 10.0;
    let handle = |width: Length, height: Length, direction: window::Direction, cursor: mouse::Interaction| {
        mouse_area(Space::new().width(width).height(height))
            .on_press(Message::WindowResize(direction))
            .interaction(cursor)
    };
    let fixed = Length::Fixed;
    let down = mouse::Interaction::ResizingDiagonallyDown;
    let up = mouse::Interaction::ResizingDiagonallyUp;
    let horizontal = mouse::Interaction::ResizingHorizontally;
    let vertical = mouse::Interaction::ResizingVertically;
    let band = |left, middle, right, left_cursor, right_cursor| {
        row![
            handle(fixed(CORNER), fixed(EDGE), left, left_cursor),
            handle(Fill, fixed(EDGE), middle, vertical),
            handle(fixed(CORNER), fixed(EDGE), right, right_cursor),
        ]
    };
    column![
        band(window::Direction::NorthWest, window::Direction::North, window::Direction::NorthEast, down, up),
        row![
            handle(fixed(EDGE), Fill, window::Direction::West, horizontal),
            Space::new().width(Fill).height(Fill),
            handle(fixed(EDGE), Fill, window::Direction::East, horizontal),
        ]
        .height(Fill),
        band(window::Direction::SouthWest, window::Direction::South, window::Direction::SouthEast, up, down),
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
    let frame = container(Space::new().width(Fill).height(Fill)).style(move |_| style::frame(&paint));
    let mut layers: Vec<Element<'_, Message>> = vec![body.into(), frame.into(), resize_handles()];
    if let Some((text, at)) = &app.notice {
        layers.push(notice(app, text, *at));
    }
    if let Some(menu) = &app.menu {
        layers.push(menu_layer(app, menu));
    }
    if let Some(confirm) = &app.confirm {
        layers.push(dialog(app, confirm));
    }
    iced::widget::stack(layers).into()
}

fn notice<'a>(app: &'a App, message: &'a str, at: std::time::Instant) -> Element<'a, Message> {
    let progress = App::reveal(at);
    let paint = app.paint.faded(progress);
    let pill = container(text(message).size(12.5).wrapping(text::Wrapping::None))
        .padding(Padding::from([8, 14]))
        .max_width(app.window.width - 48.0)
        .clip(true)
        .style(move |_| style::popover(&paint));
    container(pill)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::End)
        .padding(Padding::from([0.0, 24.0]).bottom(18.0 + (1.0 - progress) * 6.0))
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
}
